//! Faults delivered over IPC.

use core::sync::atomic::{AtomicUsize, Ordering};

use sap_rt::abi::{BootInfo, fault, object, reg, rights};
use sap_rt::address::mapped;
use sap_rt::{endpoint, println, sys, tcb};

use crate::support::{make, map_data, ok, slot, spawn, spawn_with, untyped_at_least};

/// What the faulting thread read after its handler fixed the fault.
static FAULT_READ: AtomicUsize = AtomicUsize::new(0);

/// The value the handler leaves for the faulting read to find.
const FIXED: usize = 0xfa017;

/// What `faulter` does.
const READ: usize = 0;
const ILLEGAL: usize = 1;

/// Read `address` (which faults until a handler maps it) and record the
/// value, or run an illegal instruction, by `mode`. Then suspend itself.
extern "C" fn faulter(address: usize, mode: usize, me: usize) -> ! {
    if mode == READ {
        // SAFETY: deliberately touches an address that may be unmapped; the
        // fault handler maps it before this thread resumes.
        let value = unsafe { core::ptr::read_volatile(mapped::<usize>(address)) };
        FAULT_READ.store(value, Ordering::Relaxed);
    } else {
        // SAFETY: deliberately illegal; the thread never resumes past it.
        unsafe { core::arch::asm!("unimp") };
    }

    loop {
        let _ = tcb::suspend(me);
    }
}

/// A page fault fixed by the handler mapping a page and resuming the
/// thread, an illegal instruction left suspended, and a thread without a
/// handler suspended rather than ending the boot.
pub fn delivery(info: &BootInfo) {
    let base = info.empty.start + 240;
    let parent = untyped_at_least(info, 18);
    let ep = base;
    let reply = base + 1;
    let handler = base + 2;
    let page = base + 3;
    let first = base + 10;
    let second = base + 20;
    let third = base + 30;

    make(parent, object::ENDPOINT, ep);
    make(parent, object::REPLY, reply);
    make(parent, object::FRAME_4K, page);
    ok(
        "badge the handler cap",
        slot(handler).mint_from(slot(ep), rights::WRITE, 0x51),
    );

    // A page fault, fixed by the handler. The address is in the faulting
    // thread's own region, whose page tables exist but where nothing is
    // mapped above its stack and IPC buffer.
    let region = 0x30_0000_0000;
    let target = region + 0x8000;
    let entry = (faulter as *const ()).addr();
    let in_faulter = entry..entry + 0x100;

    let t1 = spawn_with(parent, first, region, faulter, [target, READ, first], false);
    ok("set handler", tcb::set_fault_handler(t1, Some(handler)));
    ok("start", tcb::resume(t1));

    let mut words = [0usize; 3];
    let got = ok("receive the fault", endpoint::recv(ep, reply, &mut words));
    assert_eq!(got.info.label(), fault::VM, "fault kind");
    assert_eq!(got.badge, 0x51, "fault badge");
    assert_eq!(words[fault::ADDRESS], target, "fault address");
    assert!(
        in_faulter.contains(&words[fault::PC]),
        "fault pc not in faulter"
    );

    // Fix it: map a page there with a value in it, and resume the thread.
    map_data(page, target);
    // SAFETY: just mapped read-write.
    unsafe { mapped::<usize>(target).write_volatile(FIXED) };
    ok("resume it", endpoint::reply(reply, 0, &[], &[]));
    sys::yield_now();
    let read = FAULT_READ.load(Ordering::Relaxed);
    assert_eq!(
        read, FIXED,
        "the faulting read didn't complete after the fix"
    );

    // An illegal instruction answered with a nonzero label stays stopped,
    // at the instruction.
    let va = region + 0x4000_0000;
    let t2 = spawn_with(parent, second, va, faulter, [0, ILLEGAL, second], false);
    ok("set handler", tcb::set_fault_handler(t2, Some(handler)));
    ok("start", tcb::resume(t2));

    let got = ok("receive the fault", endpoint::recv(ep, reply, &mut words));
    assert_eq!(got.info.label(), fault::USER_EXCEPTION, "fault kind");
    ok("refuse it", endpoint::reply(reply, 1, &[], &[]));
    sys::yield_now();
    let pc = ok("read registers", tcb::read_registers(t2))[reg::PC];
    assert_eq!(
        pc,
        words[fault::PC],
        "a refused fault should leave the thread at the instruction"
    );

    // Without a handler, the faulting thread is suspended; this thread
    // carries on.
    let va = region + 0x8000_0000;
    let t3 = spawn(parent, third, va, faulter, [0, ILLEGAL, third]);
    sys::yield_now();
    let pc = ok("read registers", tcb::read_registers(t3))[reg::PC];
    assert!(
        in_faulter.contains(&pc),
        "the unhandled thread isn't stopped in faulter"
    );

    ok("revoke", slot(parent).revoke());
    println!("faults delivered, fixed, refused and unhandled behave");
}
