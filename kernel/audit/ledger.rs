//! The ledger: every kernel object, by physical address, from when it is
//! created until its memory is reclaimed, for the audit (`super`).
//!
//! Retyping is the only way objects come into existence after boot, so
//! three hooks keep the ledger: `untyped::retype` records each object it
//! creates (`created`) and, when it resets a region, drops the objects that
//! were in it (`reclaimed`); `cspace::delete` records an object's last
//! capability going (`finalised`). The root task's objects, which the
//! kernel makes at boot, are recorded by `super::register_root_task`. Each
//! hook checks what it can at once: a new object overlaps no recorded one,
//! a reset region holds no live object, and an object is finalised once.
//!
//! The table lives in memory taken from the boot allocator, so it has a
//! fixed capacity; filling it panics.

use core::fmt;
use core::ops::Range;
use core::ptr;

use sap_abi::object;

use crate::cap::Cap;
use crate::cspace::{SLOT_BITS, Slot};
use crate::endpoint::{ENDPOINT_BITS, REPLY_BITS};
use crate::global::Global;
use crate::mm::{BootAllocator, PAGE_SIZE};
use crate::notification::NOTIFICATION_BITS;
use crate::thread::{TCB_BITS, TCB_SLOTS, Tcb};

/// How many objects the ledger holds.
const CAPACITY: usize = 1 << 15;

/// An object, as the ledger knows it: its memory and type.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Identity {
    pub pa: usize,
    pub bits: u8,
    /// A `sap_abi::object` type.
    pub object_type: usize,
}

impl Identity {
    /// The object `cap` names, if the ledger records objects of its type:
    /// every type but untyped memory (a region, not an object) and the ASID
    /// pool (not in memory).
    pub fn of(cap: &Cap) -> Option<Identity> {
        if matches!(cap, Cap::Untyped { .. }) {
            return None;
        }

        let (pa, bits) = cap.extent()?;
        let object_type = cap.object_type()?;
        Some(Identity {
            pa,
            bits,
            object_type,
        })
    }

    /// The object of `object_type` at kernel address `address`, for the
    /// types whose size is fixed.
    pub fn at(address: usize, object_type: usize) -> Identity {
        let bits = match object_type {
            object::TCB => TCB_BITS,
            object::ENDPOINT => ENDPOINT_BITS,
            object::NOTIFICATION => NOTIFICATION_BITS,
            object::REPLY => REPLY_BITS,
            object::PAGE_TABLE => 12,
            other => panic!("audit: objects of type {other} have no fixed size"),
        };
        let pa = crate::arch::kernel_to_phys(address);
        Identity {
            pa,
            bits: bits as u8,
            object_type,
        }
    }

    fn end(&self) -> usize {
        self.pa + (1 << self.bits)
    }
}

impl fmt::Debug for Identity {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = match self.object_type {
            object::TCB => "thread",
            object::ENDPOINT => "endpoint",
            object::NOTIFICATION => "notification",
            object::REPLY => "reply object",
            object::CNODE => "CNode",
            object::PAGE_TABLE => "page table",
            object::FRAME_4K | object::FRAME_2M | object::FRAME_1G => "frame",
            _ => "object",
        };
        write!(f, "{name} at {:#x} (2^{} bytes)", self.pa, self.bits)
    }
}

/// One object, and what the current audit has counted about it.
#[derive(Clone, Copy)]
pub struct Entry {
    pub identity: Identity,
    /// Whether capabilities may name it: false once its last capability is
    /// deleted.
    pub live: bool,
    /// Capabilities naming it.
    pub caps: u32,
    /// Runs of adjacent slots on a derivation list holding them.
    pub runs: u32,
    /// Page-table entries pointing at it, and for a root, the ASID table.
    pub refs: u32,
    /// Frame capabilities to it whose recorded mapping is in place.
    pub present: u32,
    /// Ready queues it is in.
    pub queued: u32,
    /// Endpoint and notification queues it is in.
    pub waits: u32,
}

impl Entry {
    fn new(identity: Identity) -> Entry {
        Entry {
            identity,
            live: true,
            caps: 0,
            runs: 0,
            refs: 0,
            present: 0,
            queued: 0,
            waits: 0,
        }
    }
}

/// The entries, sorted by address. Objects never overlap, so the address
/// is a key.
pub struct Ledger {
    entries: *mut Entry,
    len: usize,
}

static LEDGER: Global<Ledger> = Global::new(Ledger {
    entries: ptr::null_mut(),
    len: 0,
});

/// Take the ledger's memory from the boot allocator. Call once, before the
/// root task starts.
pub fn init(alloc: &mut BootAllocator) {
    let bytes = CAPACITY * size_of::<Entry>();
    let pa = alloc.alloc_contiguous(bytes.div_ceil(PAGE_SIZE));
    get().entries = crate::arch::phys_to_virt(pa) as *mut Entry;
}

/// The ledger.
pub fn get() -> &'static mut Ledger {
    // SAFETY: the hooks and the audit run one at a time, between and
    // inside kernel operations, and none holds the reference past its call.
    unsafe { LEDGER.get() }
}

impl Ledger {
    pub fn entries(&mut self) -> &mut [Entry] {
        // SAFETY: the first `len` entries are initialised; the memory is
        // the ledger's own.
        unsafe { core::slice::from_raw_parts_mut(self.entries, self.len) }
    }

    fn sorted(&self) -> &[Entry] {
        // SAFETY: as above.
        unsafe { core::slice::from_raw_parts(self.entries, self.len) }
    }

    /// The index of the first entry at or above `pa`.
    fn first_from(&self, pa: usize) -> usize {
        self.sorted().partition_point(|e| e.identity.pa < pa)
    }

    /// The indices of the entries starting in `start..end`.
    pub fn within(&self, start: usize, end: usize) -> Range<usize> {
        self.first_from(start)..self.first_from(end)
    }

    /// How many live objects start in `start..end`.
    pub fn live_count(&self, start: usize, end: usize) -> usize {
        let range = self.within(start, end);
        self.sorted()[range].iter().filter(|e| e.live).count()
    }

    /// The index of the entry for exactly `identity`, live or not.
    pub fn find(&self, identity: Identity) -> Option<usize> {
        let index = self.first_from(identity.pa);
        let entry = self.sorted().get(index)?;
        (entry.identity == identity).then_some(index)
    }

    /// The index of the entry for exactly `identity`, if it is live.
    pub fn live(&self, identity: Identity) -> Option<usize> {
        self.find(identity).filter(|&i| self.sorted()[i].live)
    }

    /// Whether `slot` is a slot of a live CNode or thread.
    pub fn is_slot(&self, slot: *const Slot) -> bool {
        let pa = crate::arch::kernel_to_phys(slot as usize);
        let after = self.first_from(pa + 1);
        let Some(entry) = after.checked_sub(1).map(|i| self.sorted()[i]) else {
            return false;
        };
        if !entry.live || pa >= entry.identity.end() {
            return false;
        }

        let offset = pa - entry.identity.pa;
        match entry.identity.object_type {
            object::CNODE => offset % (1 << SLOT_BITS) == 0,
            object::TCB => {
                let first = core::mem::offset_of!(Tcb, slots);
                let size = size_of::<Slot>();
                let index = offset.wrapping_sub(first) / size;
                offset >= first && (offset - first) % size == 0 && index < TCB_SLOTS
            }
            _ => false,
        }
    }

    pub fn clear_counters(&mut self) {
        for entry in self.entries() {
            *entry = Entry {
                live: entry.live,
                ..Entry::new(entry.identity)
            };
        }
    }

    /// Record a new object, which may overlap no recorded one.
    pub fn created(&mut self, identity: Identity) {
        let at = self.first_from(identity.pa);
        if let Some(before) = at.checked_sub(1).map(|i| self.sorted()[i].identity) {
            assert!(
                before.end() <= identity.pa,
                "audit: new {identity:?} overlaps {before:?}"
            );
        }
        if let Some(after) = self.sorted().get(at).map(|e| e.identity) {
            assert!(
                identity.end() <= after.pa,
                "audit: new {identity:?} overlaps {after:?}"
            );
        }
        assert!(
            self.len < CAPACITY,
            "audit: the ledger is full ({CAPACITY} objects)"
        );

        // SAFETY: `at <= len < CAPACITY`, inside the ledger's memory.
        unsafe {
            ptr::copy(
                self.entries.add(at),
                self.entries.add(at + 1),
                self.len - at,
            );
            self.entries.add(at).write(Entry::new(identity));
        }
        self.len += 1;
    }

    /// Record an object made at boot, unless it is recorded already.
    pub fn register(&mut self, identity: Identity) {
        if self.find(identity).is_none() {
            self.created(identity);
        }
    }

    /// Record that the last capability to an object is gone.
    pub fn finalised(&mut self, identity: Identity) {
        let Some(index) = self.find(identity) else {
            panic!("audit: finalising {identity:?}, which the ledger doesn't hold");
        };
        let entry = &mut self.entries()[index];
        assert!(entry.live, "audit: finalising {identity:?} twice");
        entry.live = false;
    }

    /// Drop the objects in the region at `base` of `2^bits` bytes, whose
    /// memory is about to be reused; none may be live.
    pub fn reclaimed(&mut self, base: usize, bits: usize) {
        let range = self.within(base, base + (1 << bits));
        for entry in &self.sorted()[range.clone()] {
            let identity = entry.identity;
            assert!(
                !entry.live,
                "audit: memory at {base:#x} is reused while {identity:?} lives"
            );
        }

        let removed = range.len();
        // SAFETY: both ranges are inside the first `len` entries.
        unsafe {
            let tail = self.len - range.end;
            ptr::copy(
                self.entries.add(range.end),
                self.entries.add(range.start),
                tail,
            );
        }
        self.len -= removed;
    }
}

/// Record the objects `cap` makes.
pub fn created(cap: &Cap) {
    if let Some(identity) = Identity::of(cap) {
        get().created(identity);
    }
}

/// Record that `cap` was the last capability to its object.
pub fn finalised(cap: &Cap) {
    if let Some(identity) = Identity::of(cap) {
        get().finalised(identity);
    }
}

/// Record that the untyped region at `base` of `2^bits` bytes is reset.
pub fn reclaimed(base: usize, bits: usize) {
    get().reclaimed(base, bits);
}
