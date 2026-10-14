//! A second program, loaded by the root task into its own process, to check
//! IPC between address spaces (M4). It is started with the capability
//! slots of its endpoint and its own thread as arguments.

#![no_std]
#![no_main]

use libsap::{ipc, println, tcb};

/// An address mapped only in the root task's address space.
const PARENT_ONLY: usize = 0x38_0000_0000;

#[unsafe(no_mangle)]
fn sap_main(ep: usize, me: usize, _: usize) -> ! {
    println!("peer: running in its own address space");
    // A call across address spaces, with words through both IPC buffers.
    let mut out = [0usize; 6];
    let reply = ipc::call(ep, 1, &[1, 2, 3, 4, 5, 6], &[], &mut out).expect("peer: call");
    let sum: usize = out[..reply.info.length()].iter().sum();
    ipc::call(ep, 2, &[sum], &[], &mut []).expect("peer: report");
    // The parent's memory isn't mapped here: this read faults, and the
    // fault goes to the parent, which leaves this thread suspended.
    // SAFETY: deliberately touches an address this process can't reach.
    let _ = unsafe { core::ptr::read_volatile(PARENT_ONLY as *const usize) };
    println!("peer: read the parent's memory (isolation broken)");
    loop {
        let _ = tcb::suspend(me);
    }
}
