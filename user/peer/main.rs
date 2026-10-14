//! A second program, loaded by the root task into its own process, to check
//! IPC between address spaces (M4). Its third argument picks what it does:
//! 0, a call over an endpoint (slot `a0`) and an isolation check; 1, echo
//! messages over a pair of shared-memory rings.

#![no_std]
#![no_main]

use libsap::ring::{Consumer, Producer, MAX_WORDS};
use libsap::{ipc, println, tcb};

/// An address mapped only in the root task's address space.
const PARENT_ONLY: usize = 0x38_0000_0000;

/// Where the parent maps the rings in this process, and the capability
/// slots it fills for ring mode (shared with init's check).
pub const RING_BASE: usize = 0x20_0000_0000;
const MY_NOTIFICATION: usize = 1;
const PARENT_DOORBELL: usize = 3;

#[unsafe(no_mangle)]
fn sap_main(ep: usize, me: usize, mode: usize) -> ! {
    if mode == 1 {
        echo_ring(me);
    }
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

/// Receive messages on the inbound ring and send each back with every word
/// incremented, until a message whose first word is `usize::MAX`.
fn echo_ring(me: usize) -> ! {
    // Inbound: data read-only, control read-write. Outbound: the reverse.
    // SAFETY: the parent mapped the four pages this way before starting us.
    let (mut inbound, mut outbound) = unsafe {
        (
            Consumer::new(RING_BASE, RING_BASE + 0x1000, PARENT_DOORBELL, MY_NOTIFICATION),
            Producer::new(RING_BASE + 0x2000, RING_BASE + 0x3000, PARENT_DOORBELL, MY_NOTIFICATION),
        )
    };
    let mut words = [0usize; MAX_WORDS];
    loop {
        let n = inbound.recv(&mut words);
        if n > 0 && words[0] == usize::MAX {
            outbound.send(&words[..n]);
            break;
        }
        for w in &mut words[..n] {
            *w += 1;
        }
        outbound.send(&words[..n]);
    }
    println!("peer: ring done, slept {} time(s) waiting for messages", inbound.sleeps);
    loop {
        let _ = tcb::suspend(me);
    }
}
