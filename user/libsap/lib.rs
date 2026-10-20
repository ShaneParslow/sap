//! The runtime for sap user programs: the entry point, a debug console, the
//! panic handler, wrappers for system calls and kernel-object invocations, a
//! slot allocator, locks, a heap, and a loader for starting other programs.
//! A program provides `sap_main`, and calls `init` with the resources the
//! runtime may use before it allocates.
//!
//! Every thread starts with `sp` at the top of its stack, `tp` pointing at
//! its IPC buffer, and up to three arguments in `a0`..`a2`. For the root
//! task, `a0` is the address of the kernel's boot-info page.

#![no_std]

extern crate alloc;

pub mod cnode;
pub mod debug;
pub mod heap;
pub mod ipc;
pub mod notification;
pub mod process;
pub mod ring;
pub mod slots;
pub mod sync;
pub mod sys;
pub mod tcb;
pub mod untyped;
pub mod vspace;

use core::ops::Range;
use core::panic::PanicInfo;

pub use sap_abi as abi;

use crate::cnode::SlotRef;

/// What the runtime may use on the program's behalf.
pub struct Resources {
    /// The root CNode (single-level and guarded, so slot numbers are
    /// capability addresses).
    pub cnode: usize,
    /// Free slots of it for the slot allocator: nothing else may use them.
    pub slots: Range<usize>,
    /// The program's address space.
    pub vspace: usize,
    /// Untyped memory for the heap and the runtime's own objects. Revoking
    /// it destroys them.
    pub untyped: usize,
    /// An unused, page-aligned address range for the heap to grow into.
    pub heap: Range<usize>,
}

/// Start the slot allocator and the heap. Call once, before allocating.
///
/// TODO: a program started by another receives its resources in a typed
/// bootstrap message (M7); today only the root task calls this.
pub fn init(resources: Resources) {
    let Resources { cnode, slots: free, vspace, untyped, heap: region } = resources;
    // A notification for each lock, from the first two free slots.
    let locks = free.start;
    let root = SlotRef { root: cnode, index: 0, depth: 0 };
    untyped::retype(untyped, abi::object::NOTIFICATION, 0, root, locks, 2).expect("runtime: notifications");
    slots::init(free.start + 2..free.end, locks);
    heap::init(cnode, vspace, untyped, region, locks + 1);
}

core::arch::global_asm!(
    r#"
    .section .text.entry
    .globl _start
_start:
    call    sap_start
1:  j       1b
"#
);

unsafe extern "Rust" {
    /// The program's entry point, with the thread's three start arguments.
    fn sap_main(a0: usize, a1: usize, a2: usize) -> !;
}

#[unsafe(no_mangle)]
extern "C" fn sap_start(a0: usize, a1: usize, a2: usize) -> ! {
    // SAFETY: the program defines sap_main.
    unsafe { sap_main(a0, a1, a2) }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("panic: {info}");
    debug::halt(true)
}
