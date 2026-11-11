//! What the tests share: naming slots, checking results, making objects,
//! and starting threads.

use core::fmt::Debug;

use sap_rt::abi::{BootInfo, attr, init_slot, object, reg, rights};
use sap_rt::cnode::SlotRef;
use sap_rt::sys::Error;
use sap_rt::{tcb, untyped, vspace};

/// This address space.
pub const VSPACE: usize = init_slot::VSPACE;

/// This thread.
pub const TCB: usize = init_slot::TCB;

/// The root CNode.
pub const CNODE: usize = init_slot::CNODE;

/// Rights for an ordinary writable mapping.
pub const RW: usize = rights::READ | rights::WRITE;

/// A thread's entry point, with its three start arguments.
pub type Entry = extern "C" fn(usize, usize, usize) -> !;

/// Slot `index` of the root CNode.
pub fn slot(index: usize) -> SlotRef {
    SlotRef::new(CNODE, index)
}

/// The value of `result`, which must have succeeded.
#[track_caller]
pub fn ok<T>(what: &str, result: Result<T, Error>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("{what}: failed with error {}", error.0),
    }
}

/// Check that `result` failed with error `code`.
#[track_caller]
pub fn fails<T: Debug>(what: &str, result: Result<T, Error>, code: usize) {
    match result {
        Err(error) if error.0 == code => {}
        Err(error) => panic!("{what}: failed with error {}, wanted {code}", error.0),
        Ok(value) => panic!("{what}: succeeded with {value:?}, wanted error {code}"),
    }
}

/// The first untyped region of at least `2^bits` bytes.
#[track_caller]
pub fn untyped_at_least(info: &BootInfo, bits: u8) -> usize {
    (info.untyped.start..info.untyped.end)
        .zip(&info.untyped_list)
        .find(|&(_, region)| region.size_bits >= bits && region.is_device == 0)
        .map(|(cptr, _)| cptr)
        .unwrap_or_else(|| panic!("no untyped region of 2^{bits} bytes"))
}

/// Retype `count` objects from `parent` into the root CNode from `dest` on.
pub fn try_retype(
    parent: usize,
    object_type: usize,
    size_bits: usize,
    dest: usize,
    count: usize,
) -> Result<(), Error> {
    // Depth 0 names the root CNode itself as the destination.
    let root = SlotRef {
        root: CNODE,
        index: 0,
        depth: 0,
    };
    untyped::retype(parent, object_type, size_bits, root, dest, count)
}

/// Make `count` fixed-size objects from `parent`, from slot `dest` on.
#[track_caller]
pub fn make_many(parent: usize, object_type: usize, dest: usize, count: usize) {
    ok("retype", try_retype(parent, object_type, 0, dest, count));
}

/// Make one fixed-size object from `parent` in slot `dest`.
#[track_caller]
pub fn make(parent: usize, object_type: usize, dest: usize) {
    make_many(parent, object_type, dest, 1);
}

/// Map the page tables `first` and `first + 1` at `va` in this address
/// space: one per level below the root.
#[track_caller]
pub fn map_tables(first: usize, va: usize) {
    ok("map table", vspace::map_page_table(first, VSPACE, va));
    ok("map table", vspace::map_page_table(first + 1, VSPACE, va));
}

/// Map `frame` read-write and not executable at `va` in this address space.
#[track_caller]
pub fn map_data(frame: usize, va: usize) {
    let mapped = vspace::map_frame(frame, VSPACE, va, RW, attr::EXECUTE_NEVER);
    ok("map frame", mapped);
}

/// Make a thread in this address space at `entry(args)`, with a 4 KiB stack
/// and an IPC buffer in a fresh 1 GiB-aligned region at `va`. Uses six slots
/// from `slots` and memory from `parent`. Returns the TCB's slot. The thread
/// starts at once if `start` is true.
#[track_caller]
pub fn spawn_with(
    parent: usize,
    slots: usize,
    va: usize,
    entry: Entry,
    args: [usize; 3],
    start: bool,
) -> usize {
    let thread = slots;
    let tables = slots + 1;
    let stack = slots + 3;
    let buffer = slots + 4;
    let buffer_va = va + 0x2000;

    make(parent, object::TCB, thread);
    make_many(parent, object::PAGE_TABLE, tables, 2);
    make_many(parent, object::FRAME_4K, stack, 2);

    map_tables(tables, va);
    map_data(stack, va);
    map_data(buffer, buffer_va);

    let configured = tcb::configure(thread, CNODE, VSPACE, Some((buffer, buffer_va)));
    ok("configure", configured);
    ok("priority", tcb::set_priority(thread, TCB, 255));

    let mut regs = [0; reg::COUNT];
    regs[reg::PC] = (entry as *const ()).addr();
    regs[reg::SP] = va + 0x1000;
    regs[reg::TP] = buffer_va;
    regs[reg::A0..reg::A0 + 3].copy_from_slice(&args);
    ok("registers", tcb::write_registers(thread, start, &regs));

    thread
}

/// `spawn_with`, starting the thread at once.
#[track_caller]
pub fn spawn(parent: usize, slots: usize, va: usize, entry: Entry, args: [usize; 3]) -> usize {
    spawn_with(parent, slots, va, entry, args, true)
}
