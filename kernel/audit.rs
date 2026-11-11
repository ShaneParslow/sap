//! The audit: a check of the kernel's own data structures, run before every
//! return to user mode in kernels built with the `audit` feature
//! (`cargo run --profile audit --features audit`: the `audit` profile is
//! optimized, with debug assertions and overflow checks). A broken
//! invariant panics, naming it, at
//! the first return to user mode after the operation that broke it. Built
//! without the feature, none of this exists.
//!
//! The ledger (`ledger`) records every object, so the audit checks all of
//! them, reachable or not, and checks each pointer it follows against the
//! ledger before following it. Each module checks its own structures, in
//! an `audit` submodule beside them: slots and derivation lists (`cspace`),
//! the ready queues (`sched`), threads (`thread`), endpoint queues and reply
//! objects (`endpoint`), notifications (`notification`), and address spaces
//! (`arch`). This module runs them and checks what they counted:
//! - every live object is named by at least one capability, and its
//!   capabilities are adjacent on their derivation list;
//! - every capability names a live object;
//! - a page table is mapped in at most one place, and every page-table entry
//!   that maps a frame has a frame capability recording it.
//!
//! An audit visits every slot of every CNode, so it costs about as much as
//! the CNodes are big: fine for `test-kernel` (1.3 s against 0.5 s without,
//! QEMU, `audit` profile), too slow for a full boot.
//! `SAP_AUDIT_INTERVAL=N` at build time audits every Nth return to user mode
//! instead, which is faster but reports a broken invariant up to N returns
//! after the operation that broke it.

pub mod ledger;

use core::sync::atomic::{AtomicUsize, Ordering};

use sap_abi::object;

use crate::cap::Cap;
use crate::cspace::{SLOT_BITS, Slot};
use crate::thread::{TCB_SLOTS, Tcb};
use ledger::{Entry, Identity, Ledger};

/// Audit every this many returns to user mode (`SAP_AUDIT_INTERVAL`).
const INTERVAL: usize = parse_interval(env!("SAP_AUDIT_INTERVAL"));

const fn parse_interval(text: &str) -> usize {
    let digits = text.as_bytes();
    let mut value = 0;
    let mut i = 0;
    while i < digits.len() {
        assert!(
            digits[i].is_ascii_digit(),
            "SAP_AUDIT_INTERVAL must be a number"
        );
        value = value * 10 + (digits[i] - b'0') as usize;
        i += 1;
    }
    assert!(value > 0, "SAP_AUDIT_INTERVAL must be at least 1");
    value
}

/// Returns to user mode since the last audit.
static SINCE_LAST: AtomicUsize = AtomicUsize::new(0);

/// Check every object, every `INTERVAL` calls. Called before every return
/// to user mode.
pub fn run() {
    let since_last = SINCE_LAST.load(Ordering::Relaxed) + 1;
    if since_last < INTERVAL {
        SINCE_LAST.store(since_last, Ordering::Relaxed);
        return;
    }
    SINCE_LAST.store(0, Ordering::Relaxed);

    let ledger = ledger::get();
    ledger.clear_counters();

    slots(ledger);
    crate::sched::audit::queues(ledger);
    waiting(ledger);
    threads(ledger);
    crate::arch::audit::address_spaces(ledger);
    counts(ledger);
}

/// Record the root task's objects, which the kernel makes at boot: those
/// its CNode (`cnode`) and thread (`tcb`) hold capabilities to.
pub fn register_root_task(cnode: Cap, tcb: *mut Tcb) {
    let ledger = ledger::get();
    let Cap::CNode { ptr, radix, .. } = cnode else {
        panic!("audit: the root task's CNode isn't a CNode");
    };

    let register = |ledger: &mut Ledger, slot: *mut Slot| {
        // SAFETY: the root task's slots are live.
        let cap = unsafe { (*slot).cap };
        if let Some(identity) = Identity::of(&cap) {
            ledger.register(identity);
        }
    };
    for i in 0..1usize << radix {
        register(ledger, (ptr + (i << SLOT_BITS)) as *mut Slot);
    }
    for i in 0..TCB_SLOTS {
        // SAFETY: the root task's thread is live.
        register(ledger, unsafe { &raw mut (*tcb).slots[i] });
    }
}

/// Check every slot of every CNode and thread, and count what the
/// capabilities in them name.
fn slots(ledger: &mut Ledger) {
    for i in 0..ledger.entries().len() {
        let container = ledger.entries()[i];
        let address = crate::arch::phys_to_virt(container.identity.pa);

        match container.identity.object_type {
            object::CNODE => {
                let count = 1usize << (container.identity.bits as usize - SLOT_BITS);
                for index in 0..count {
                    let slot = (address + (index << SLOT_BITS)) as *mut Slot;
                    check_slot(ledger, slot, container);
                }
            }

            object::TCB => {
                let tcb = address as *mut Tcb;
                for index in 0..TCB_SLOTS {
                    // SAFETY: the ledger holds the thread, so it is in memory.
                    let slot = unsafe { &raw mut (*tcb).slots[index] };
                    check_slot(ledger, slot, container);
                }
            }

            _ => {}
        }
    }
}

/// Check one slot of `container`, and count what its capability names.
fn check_slot(ledger: &mut Ledger, slot: *mut Slot, container: Entry) {
    // SAFETY: the slot is inside an object the ledger holds.
    let cap = unsafe { crate::cspace::audit::slot(slot, ledger) };
    if cap.is_null() {
        return;
    }

    let holder = container.identity;
    assert!(
        container.live,
        "audit: {slot:p} holds a capability inside finalised {holder:?}"
    );

    if matches!(cap, Cap::Untyped { .. }) {
        // SAFETY: as above.
        unsafe { crate::cspace::audit::untyped(slot, ledger) };
        return;
    }

    let Some(identity) = Identity::of(&cap) else {
        return;
    };
    let Some(index) = ledger.live(identity) else {
        panic!("audit: {slot:p} names {identity:?}, which isn't live");
    };

    // SAFETY: as above; its neighbours were checked by `slot`.
    let starts_run = unsafe { crate::cspace::audit::starts_run(slot) };
    let present = crate::arch::audit::mapping_in_place(&cap);

    let entry = &mut ledger.entries()[index];
    entry.caps += 1;
    entry.runs += starts_run as u32;
    entry.present += present as u32;
}

/// Check every live endpoint, notification and reply object.
fn waiting(ledger: &mut Ledger) {
    for i in 0..ledger.entries().len() {
        let entry = ledger.entries()[i];
        if !entry.live {
            continue;
        }
        let address = crate::arch::phys_to_virt(entry.identity.pa);

        // SAFETY: live objects in the ledger are in memory.
        unsafe {
            match entry.identity.object_type {
                object::ENDPOINT => crate::endpoint::audit::queue(address as *mut _, ledger),
                object::REPLY => crate::endpoint::audit::reply(address as *mut _, ledger),
                object::NOTIFICATION => {
                    crate::notification::audit::notification(address as *mut _, ledger);
                }
                _ => {}
            }
        }
    }
}

/// Check every live thread. After `waiting` and the ready queues, whose
/// counts it checks.
fn threads(ledger: &mut Ledger) {
    for i in 0..ledger.entries().len() {
        let entry = ledger.entries()[i];
        if entry.live && entry.identity.object_type == object::TCB {
            let tcb = crate::arch::phys_to_virt(entry.identity.pa) as *mut Tcb;
            // SAFETY: as above.
            unsafe { crate::thread::audit::thread(tcb, entry, ledger) };
        }
    }
}

/// Check what was counted about each live object.
fn counts(ledger: &mut Ledger) {
    for entry in ledger.entries().iter() {
        if !entry.live {
            continue;
        }
        let identity = entry.identity;

        assert!(
            entry.caps > 0,
            "audit: {identity:?} is live, but no capability names it"
        );
        let runs = entry.runs;
        assert!(
            runs == 1,
            "audit: {identity:?}'s capabilities are in {runs} separate runs"
        );

        let refs = entry.refs;
        match identity.object_type {
            object::PAGE_TABLE => {
                assert!(refs <= 1, "audit: {identity:?} is mapped in {refs} places");
            }

            object::FRAME_4K | object::FRAME_2M | object::FRAME_1G => {
                let present = entry.present;
                assert!(
                    refs <= present,
                    "audit: {identity:?} is mapped {refs} times, but capabilities record {present}"
                );
            }

            _ => {}
        }
    }
}
