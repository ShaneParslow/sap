//! The process ABI: what a launcher gives a process it starts, other than
//! the root task, which the kernel starts with its boot information
//! (`sap_abi::BootInfo`). A startup record and the bootstrap message, in a
//! read-only page whose address arrives in `a0`; fixed slots for the
//! process's own objects, then its imports; and the calls a process makes
//! on its launcher. The kernel knows none of it: launchers and the runtime
//! agree on it between themselves, so it is userspace's, like seL4's
//! `sel4runtime` conventions.

#![no_std]

use sap_abi::SlotRange;

/// Slots of every process's CNode that its launcher fills, other than the
/// root task's (`sap_abi::init_slot`). The process's imports follow, in
/// `Startup::imports`.
pub mod process_slot {
    pub const NULL: usize = 0;
    /// The process's main thread.
    pub const TCB: usize = 1;
    /// The process's CNode, guarded so that slot numbers are capability
    /// addresses.
    pub const CNODE: usize = 2;
    pub const VSPACE: usize = 3;
    /// Untyped memory: the process's memory budget. Empty if it has none.
    pub const UNTYPED: usize = 4;
    /// An endpoint to whoever launched the process, which it calls to
    /// report its exit (`launcher`). Empty if nobody listens.
    pub const LAUNCHER: usize = 5;
    /// The frame of the main thread's IPC buffer, as the root task has it.
    pub const IPC_BUFFER: usize = 6;
    /// The holder's capability to the process's memory account
    /// (`interfaces/memory.capnp`), if its launcher gives it one; empty
    /// otherwise.
    pub const ACCOUNT: usize = 7;
    /// The first slot of the process's imports.
    pub const FIRST_IMPORT: usize = 8;
}

/// Labels of the calls a process makes on its `process_slot::LAUNCHER`
/// endpoint. A launcher is normally also the process's fault handler, on
/// the same endpoint, so fault labels (`sap_abi::fault`) arrive there too; these
/// don't collide with them.
pub mod launcher {
    /// The process is done: word 0 is its exit code, 0 for success. No
    /// reply comes.
    pub const EXIT: usize = 0x100;
    /// A call asking which component a client of the process's exports
    /// is: word 0 is the badge its calls arrive with. The reply's label is
    /// 0 and its words are the name's length in bytes, then the name's
    /// bytes packed little-endian; label `NO_SUCH_CLIENT` if the badge
    /// isn't a client of the caller's exports.
    pub const CLIENT: usize = 0x101;
    pub const NO_SUCH_CLIENT: usize = 1;
    /// The longest name a `CLIENT` reply carries.
    pub const MAX_NAME: usize = 64;
    /// Clients' badges on a supervisor's exports start here; a server
    /// minting capabilities to its own export uses badges below.
    pub const ROUTE_BADGES: usize = 1 << 32;
}

/// A range of virtual addresses, `start..end`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AddressRange {
    pub start: usize,
    pub end: usize,
}

/// What a process's launcher tells it at startup, in a read-only page whose
/// address arrives in its first argument register. The bootstrap message
/// follows the record in the same page.
#[repr(C)]
pub struct Startup {
    /// Ticks per second of the time counter.
    pub timer_hz: u64,
    pub ipc_buffer: usize,
    pub stack: AddressRange,
    /// Addresses nothing is mapped at, for the runtime's heap.
    pub heap: AddressRange,
    /// log2 of the number of slots in the process's CNode.
    pub cnode_radix: usize,
    /// The capabilities the bootstrap message refers to, in order.
    pub imports: SlotRange,
    /// Empty slots the process may use.
    pub free: SlotRange,
    /// Length in bytes of the bootstrap message: a Cap'n Proto message in
    /// the flat (single-segment) encoding, starting at `BOOTSTRAP_OFFSET`.
    /// Its root struct is the program's imports.
    pub bootstrap_len: usize,
}

/// Where the bootstrap message starts in the startup page.
pub const BOOTSTRAP_OFFSET: usize = 128;
/// The longest bootstrap message.
// TODO: larger bootstraps, in frames of their own (M7, if a program needs
// more than a page).
pub const MAX_BOOTSTRAP: usize = 4096 - BOOTSTRAP_OFFSET;

const _: () = assert!(core::mem::size_of::<Startup>() <= BOOTSTRAP_OFFSET);
