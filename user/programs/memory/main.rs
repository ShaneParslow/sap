//! The memory manager: holds the machine's RAM, as untyped regions from its
//! supervisor (the `ram` group), and serves memory accounts (`interfaces/system/memory.capnp`) from
//! it, as one pool.
//!
//! RAM is handed out in power-of-two chunks, split from the regions as a
//! buddy allocator would: a chunk is a copy of an untyped capability the
//! manager keeps the original of, so releasing it is one revoke of the
//! original, which destroys the copy and everything made from it. Two free
//! halves merge back into their whole.
//!
//! Each account's capabilities are badged copies of capabilities to the
//! manager's endpoint it mints and keeps: badge (index + 1) * 2 for the
//! account's own, + 1 for its control; ending an account revokes them. The
//! root account's (index 0) are minted by the supervisor instead
//! (`ROOT_ACCOUNT`, `ROOT_CONTROL`).
//!
//! Its output goes to the debug console: it starts before the log.
//! TODO: crash recovery: if the manager dies, its originals go, and the
//! chunks out can't be reclaimed (design/memory.md).

#![no_std]
#![no_main]
#![feature(strict_provenance_lints)]
// Pointers come from `sap_sys::address::mapped`, never from integer casts.
#![deny(implicit_provenance_casts, integer_to_ptr_transmutes)]

extern crate alloc;

/// Generated from the component schemas of `memory`
/// (`user/programs/<name>/<name>.capnp`).
mod components {
    #[allow(clippy::all, dead_code)]
    pub mod memory_capnp {
        include!(concat!(env!("OUT_DIR"), "/memory/memory_capnp.rs"));
    }
}

use alloc::vec;
use alloc::vec::Vec;

use capnp::message::{Builder, Reader, ReaderOptions, SegmentArray, SingleSegmentAllocator};
use components::memory_capnp::memory_bootstrap;
use sap_interfaces::memory_capnp::account_control::set_limit_params;
use sap_interfaces::memory_capnp::memory_account::{allocate_params, allocate_results, child_params, release_params, usage_results};
use sap_interfaces::memory_capnp::{Status, ROOT_ACCOUNT, ROOT_CONTROL};

const _: () = assert!(ROOT_ACCOUNT == 2 && ROOT_CONTROL == 3, "the root account is index 0");
use sap_interfaces::{methods, words};
use sap_process::process_slot;
use sap_rt::abi::{identify, object, rights, MAX_MESSAGE_LENGTH};
use sap_rt::cnode::SlotRef;
use sap_rt::ipc::{self, Give};
use sap_rt::runtime::{self, Memory, Resources};
use sap_rt::{println, process, slots, untyped};

/// The smallest and largest chunks.
const MIN_BITS: u8 = 12;
const MAX_BITS: u8 = 40;
/// Where the manager's heap grows, from its own budget.
const HEAP: core::ops::Range<usize> = 0x08_0000_0000..0x08_4000_0000;
const HOLDER_RIGHTS: usize = rights::WRITE | rights::GRANT_REPLY;
const CONTROLLER_RIGHTS: usize = rights::WRITE;

fn slot(i: usize) -> SlotRef {
    SlotRef::new(process_slot::CNODE, i)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Free,
    /// Retyped into two halves.
    Split([usize; 2]),
    /// A chunk an account holds.
    Out { account: usize, id: u64 },
    /// Merged back into its whole; the entry is unused.
    Gone,
}

struct Block {
    /// The manager's capability to it: the original of every chunk's copy.
    slot: usize,
    bits: u8,
    paddr: u64,
    parent: Option<usize>,
    state: State,
}

struct Account {
    parent: Option<usize>,
    reservation: u64,
    limit: u64,
    /// Bytes of the chunks it holds itself, and with its descendants'.
    own: u64,
    used: u64,
    /// What its children's reservations take of its own.
    reserved_by_children: u64,
    /// The manager's badged capabilities to it; 0 for the root's.
    account_cap: usize,
    control_cap: usize,
    alive: bool,
}

struct Manager {
    /// The receive side of the manager's endpoint, with all rights.
    endpoint: usize,
    blocks: Vec<Block>,
    accounts: Vec<Account>,
    next_id: u64,
    total: u64,
    free: u64,
}

type Result<T> = core::result::Result<T, Status>;

impl Manager {
    /// What `a` has reserved and not yet used for its own chunks or given
    /// its children: set aside from the pool for it. The root's is the
    /// pool itself.
    fn unused_reservation(&self, a: usize) -> u64 {
        let account = &self.accounts[a];
        if account.parent.is_none() || !account.alive {
            return 0;
        }
        account.reservation.saturating_sub(account.reserved_by_children + account.own)
    }

    fn set_aside(&self) -> u64 {
        (0..self.accounts.len()).map(|a| self.unused_reservation(a)).sum()
    }

    /// `a` and the accounts above it.
    fn chain(&self, a: usize) -> impl Iterator<Item = usize> + '_ {
        core::iter::successors(Some(a), |&x| self.accounts[x].parent)
    }

    fn allocate(&mut self, a: usize, bits: u8) -> Result<usize> {
        if !(MIN_BITS..=MAX_BITS).contains(&bits) {
            return Err(Status::BadRequest);
        }
        let size = 1u64 << bits;
        if self.chain(a).any(|x| self.accounts[x].used + size > self.accounts[x].limit) {
            return Err(Status::OverLimit);
        }
        // Its own unused reservation, then what nobody has reserved.
        let available = self.free - (self.set_aside() - self.unused_reservation(a));
        if size > available {
            return Err(Status::OutOfMemory);
        }
        let block = self.take(bits)?;
        self.blocks[block].state = State::Out { account: a, id: self.next_id };
        self.next_id += 1;
        self.accounts[a].own += size;
        for x in self.chain(a).collect::<Vec<_>>() {
            self.accounts[x].used += size;
        }
        self.free -= size;
        Ok(block)
    }

    /// A free block of exactly `bits`, splitting a larger one if need be.
    fn take(&mut self, bits: u8) -> Result<usize> {
        let fits = |b: &Block| b.state == State::Free && b.bits >= bits;
        let mut block = (0..self.blocks.len()).filter(|&i| fits(&self.blocks[i])).min_by_key(|&i| self.blocks[i].bits).ok_or(Status::OutOfMemory)?;
        while self.blocks[block].bits > bits {
            block = self.split(block)?;
        }
        Ok(block)
    }

    /// Retype `block` into two halves; the first.
    fn split(&mut self, block: usize) -> Result<usize> {
        let (bits, paddr) = (self.blocks[block].bits - 1, self.blocks[block].paddr);
        let first = slots::alloc_range(2).ok_or(Status::Full)?;
        let root = SlotRef { root: process_slot::CNODE, index: 0, depth: 0 };
        if untyped::retype(self.blocks[block].slot, object::UNTYPED, bits as usize, root, first, 2).is_err() {
            slots::free_range(first, 2);
            return Err(Status::Full);
        }
        let halves = [0, 1].map(|i| {
            let half = Block { slot: first + i, bits, paddr: paddr + ((i as u64) << bits), parent: Some(block), state: State::Free };
            match self.blocks.iter().position(|b| b.state == State::Gone) {
                Some(unused) => {
                    self.blocks[unused] = half;
                    unused
                }
                None => {
                    self.blocks.push(half);
                    self.blocks.len() - 1
                }
            }
        });
        self.blocks[block].state = State::Split(halves);
        Ok(halves[0])
    }

    /// Take chunk `block` back: revoke its original, destroying the copy
    /// and everything made from it, and merge it with its free half.
    fn reclaim(&mut self, block: usize) {
        let State::Out { account, .. } = self.blocks[block].state else { return };
        let size = 1u64 << self.blocks[block].bits;
        let _ = slot(self.blocks[block].slot).revoke();
        self.blocks[block].state = State::Free;
        self.accounts[account].own -= size;
        for x in self.chain(account).collect::<Vec<_>>() {
            self.accounts[x].used -= size;
        }
        self.free += size;
        let mut block = block;
        while let Some(parent) = self.blocks[block].parent {
            let State::Split(halves) = self.blocks[parent].state else { break };
            if halves.iter().any(|&h| self.blocks[h].state != State::Free) {
                break;
            }
            for h in halves {
                let _ = slot(self.blocks[h].slot).delete();
                slots::free(self.blocks[h].slot);
                self.blocks[h].state = State::Gone;
            }
            self.blocks[parent].state = State::Free;
            block = parent;
        }
    }

    fn release(&mut self, a: usize, id: u64) -> Result<()> {
        let block = self.blocks.iter().position(|b| b.state == State::Out { account: a, id }).ok_or(Status::NoSuchChunk)?;
        self.reclaim(block);
        Ok(())
    }

    fn child(&mut self, a: usize, reservation: u64, limit: u64) -> Result<usize> {
        if limit < reservation {
            return Err(Status::BadRequest);
        }
        let parent = &self.accounts[a];
        let unreserved = if parent.parent.is_none() {
            // The root's children are reserved from the pool, beyond what
            // others hold.
            (self.free - self.set_aside()).min(parent.reservation - parent.reserved_by_children)
        } else {
            self.unused_reservation(a)
        };
        if reservation > unreserved || limit > parent.limit {
            return Err(Status::OverLimit);
        }
        let index = match self.accounts.iter().position(|x| !x.alive) {
            Some(index) => index,
            None => {
                self.accounts.push(Account { parent: None, reservation: 0, limit: 0, own: 0, used: 0, reserved_by_children: 0, account_cap: 0, control_cap: 0, alive: false });
                self.accounts.len() - 1
            }
        };
        let caps = slots::alloc_range(2).ok_or(Status::Full)?;
        let badge = (index + 1) * 2;
        let endpoint = slot(self.endpoint);
        if slot(caps).mint_from(endpoint, HOLDER_RIGHTS, badge).is_err() || slot(caps + 1).mint_from(endpoint, CONTROLLER_RIGHTS, badge + 1).is_err() {
            let _ = slot(caps).delete();
            slots::free_range(caps, 2);
            return Err(Status::Full);
        }
        self.accounts[a].reserved_by_children += reservation;
        self.accounts[index] =
            Account { parent: Some(a), reservation, limit, own: 0, used: 0, reserved_by_children: 0, account_cap: caps, control_cap: caps + 1, alive: true };
        Ok(index)
    }

    /// Release every chunk of `a` and of its descendants, and end its
    /// children.
    fn revoke(&mut self, a: usize) {
        for block in 0..self.blocks.len() {
            if let State::Out { account, .. } = self.blocks[block].state
                && account == a
            {
                self.reclaim(block);
            }
        }
        for child in 0..self.accounts.len() {
            if self.accounts[child].alive && self.accounts[child].parent == Some(a) {
                self.end(child);
            }
        }
    }

    /// Revoke `a`, then end it: its capabilities stop working.
    fn end(&mut self, a: usize) {
        self.revoke(a);
        let account = &mut self.accounts[a];
        account.alive = false;
        for cap in [account.account_cap, account.control_cap] {
            let _ = slot(cap).revoke();
            let _ = slot(cap).delete();
            slots::free(cap);
        }
        if let Some(parent) = account.parent {
            let reservation = account.reservation;
            self.accounts[parent].reserved_by_children -= reservation;
        }
    }

    fn set_limit(&mut self, a: usize, limit: u64) -> Result<()> {
        let account = &self.accounts[a];
        let ceiling = account.parent.map_or(self.total, |p| self.accounts[p].limit);
        if limit < account.reservation {
            return Err(Status::BadRequest);
        }
        if limit > ceiling {
            return Err(Status::OverLimit);
        }
        self.accounts[a].limit = limit;
        Ok(())
    }

    /// Serve one call on account `a`'s own capability (`control` false) or
    /// its control: the reply's label, words and capability.
    fn serve(&mut self, a: usize, control: bool, label: usize, words: &[usize], out: &mut [usize]) -> (Status, usize, Option<[usize; 2]>) {
        let segment = [words::bytes(words)];
        let reader = words::reader(&segment);
        let refused = |status| (status, 0, None);
        match (control, label) {
            (false, methods::memory_account::ALLOCATE) => {
                let Ok(params) = reader.get_root::<allocate_params::Reader>() else { return refused(Status::BadRequest) };
                match self.allocate(a, params.get_size_bits()) {
                    Ok(block) => {
                        let b = &self.blocks[block];
                        let State::Out { id, .. } = b.state else { unreachable!("an allocated block is out") };
                        let n = encode::<allocate_results::Owned>(out, |mut r| {
                            r.set_id(id);
                            r.set_paddr(b.paddr);
                        });
                        (Status::Ok, n, Some([b.slot, 0]))
                    }
                    Err(status) => refused(status),
                }
            }
            (false, methods::memory_account::RELEASE) => {
                let Ok(params) = reader.get_root::<release_params::Reader>() else { return refused(Status::BadRequest) };
                refused(self.release(a, params.get_id()).err().unwrap_or(Status::Ok))
            }
            (false, methods::memory_account::CHILD) => {
                let Ok(params) = reader.get_root::<child_params::Reader>() else { return refused(Status::BadRequest) };
                match self.child(a, params.get_reservation(), params.get_limit()) {
                    Ok(c) => (Status::Ok, 0, Some([self.accounts[c].account_cap, self.accounts[c].control_cap])),
                    Err(status) => refused(status),
                }
            }
            (false, methods::memory_account::USAGE) => {
                let account = &self.accounts[a];
                let n = encode::<usage_results::Owned>(out, |mut r| {
                    r.set_used(account.used);
                    r.set_reservation(account.reservation);
                    r.set_limit(account.limit);
                });
                (Status::Ok, n, None)
            }
            (true, methods::account_control::REVOKE) => {
                self.revoke(a);
                refused(Status::Ok)
            }
            (true, methods::account_control::SET_LIMIT) => {
                let Ok(params) = reader.get_root::<set_limit_params::Reader>() else { return refused(Status::BadRequest) };
                refused(self.set_limit(a, params.get_limit()).err().unwrap_or(Status::Ok))
            }
            (true, methods::account_control::DESTROY) if a != 0 => {
                self.end(a);
                refused(Status::Ok)
            }
            _ => refused(Status::BadRequest),
        }
    }
}

/// Build a message with `build` into `out`; the number of words.
fn encode<T: capnp::traits::Owned>(out: &mut [usize], build: impl FnOnce(T::Builder<'_>)) -> usize {
    let mut scratch = vec![0u8; 256];
    let mut allocator = SingleSegmentAllocator::new(&mut scratch);
    let mut message = Builder::new(&mut allocator);
    build(message.init_root::<T::Builder<'_>>());
    words::pack(&message, out).expect("memory: a reply too large")
}

/// The manager's endpoint from its bootstrap message, and its RAM: the
/// imports from `ram` to the end, each described by identifying it.
fn manager(startup: &'static sap_process::Startup) -> core::result::Result<Manager, capnp::Error> {
    let segments = [process::bootstrap(startup)];
    let reader = Reader::new(SegmentArray::new(&segments), ReaderOptions::new());
    let imports = reader.get_root::<memory_bootstrap::Reader>()?;
    let import = |index: u32| process::import(startup, index).expect("memory: no such import");
    let mut blocks = Vec::new();
    for cap in import(imports.get_ram())..startup.imports.end {
        let region = slot(cap).identify().expect("memory: a RAM import");
        assert!(region.object_type == object::UNTYPED && region.flags & identify::DEVICE == 0, "memory: RAM that isn't RAM");
        blocks.push(Block { slot: cap, bits: region.size_bits as u8, paddr: region.paddr as u64, parent: None, state: State::Free });
    }
    let total = blocks.iter().map(|b| 1u64 << b.bits).sum();
    let root = Account { parent: None, reservation: total, limit: total, own: 0, used: 0, reserved_by_children: 0, account_cap: 0, control_cap: 0, alive: true };
    Ok(Manager { endpoint: import(imports.get_endpoint()), blocks, accounts: vec![root], next_id: 1, total, free: total })
}

#[unsafe(no_mangle)]
fn sap_main(a0: usize, _: usize, _: usize) -> ! {
    // SAFETY: the loader started this process with its startup record.
    let startup = unsafe { process::startup(a0) };
    let free = startup.free;
    let reply = free.start;
    let root = SlotRef { root: process_slot::CNODE, index: 0, depth: 0 };
    untyped::retype(process_slot::UNTYPED, object::REPLY, 0, root, reply, 1).expect("memory: reply object");
    runtime::start(Resources { cnode: process_slot::CNODE, slots: free.start + 1..free.end, memory: Memory::Untyped(process_slot::UNTYPED), vspace: process_slot::VSPACE, heap: HEAP });
    let mut manager = manager(startup).expect("memory: bootstrap message");
    println!("memory: {} MiB of RAM in {} regions", manager.total >> 20, manager.blocks.len());

    let mut words = [0usize; MAX_MESSAGE_LENGTH];
    let mut out = [0usize; 16];
    // Nothing sends the manager capabilities.
    ipc::set_receive_window(process_slot::CNODE, 0, 0);
    loop {
        let message = ipc::recv(manager.endpoint, reply, &mut words).expect("memory: receive");
        let words = &words[..message.info.length()];
        // Badge (index + 1) * 2, + 1 for the control: the root's are
        // `ROOT_ACCOUNT` and `ROOT_CONTROL`.
        let index = (message.badge / 2).wrapping_sub(1);
        let control = message.badge % 2 == 1;
        let (status, n, caps) = if index < manager.accounts.len() && manager.accounts[index].alive {
            manager.serve(index, control, message.info.label(), words, &mut out)
        } else {
            (Status::BadRequest, 0, None)
        };
        let mut give = [Give::copy(0); 2];
        let caps: &[Give] = match caps {
            Some([a, 0]) => {
                give[0] = Give::copy(a);
                &give[..1]
            }
            Some([a, b]) => {
                give = [Give::copy(a), Give::copy(b)];
                &give
            }
            None => &[],
        };
        let _ = ipc::reply(reply, status as usize, &out[..n], caps);
    }
}
