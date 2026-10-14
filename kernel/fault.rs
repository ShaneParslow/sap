//! Delivering user faults to fault handlers over IPC, as seL4 does.
//!
//! A thread that faults makes an implicit CALL to the endpoint in its fault
//! slot. The message is built from the recorded fault, not from the
//! thread's registers, and the handler's reply doesn't write them: label 0
//! resumes the thread at the faulting instruction, anything else leaves it
//! suspended. A thread without a handler is suspended; if it is the root
//! task, which has nobody to report to, the boot ends with a failure.

use core::sync::atomic::{AtomicUsize, Ordering};

use sap_abi::{fault, rights, MessageInfo};

use crate::cap::Cap;
use crate::endpoint::{self, Endpoint, Rights};
use crate::sched;
use crate::thread::{Tcb, FAULT_SLOT};

/// A fault waiting to be delivered or answered. `kind` 0 means none.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Fault {
    pub kind: usize,
    pub pc: usize,
    pub address: usize,
    pub cause: usize,
}

impl Fault {
    pub fn is_some(&self) -> bool {
        self.kind != 0
    }

    /// Clear the fault; returns whether there was one.
    pub fn take(&mut self) -> bool {
        core::mem::take(self).is_some()
    }

    /// The fault as an IPC message: label and words.
    pub fn message(&self) -> (MessageInfo, [usize; fault::LENGTH]) {
        let mut words = [0; fault::LENGTH];
        words[fault::PC] = self.pc;
        words[fault::ADDRESS] = self.address;
        words[fault::CAUSE] = self.cause;
        (MessageInfo::new(self.kind, 0, fault::LENGTH), words)
    }
}

/// The root task's TCB, which has no handler to report to.
static ROOT: AtomicUsize = AtomicUsize::new(0);

pub fn set_root(tcb: *mut Tcb) {
    ROOT.store(tcb as usize, Ordering::Relaxed);
}

/// The running thread `tcb` faulted: deliver it to its handler, or suspend
/// it if it has none.
///
/// # Safety
/// `tcb` must be the live, running thread.
pub unsafe fn deliver(tcb: *mut Tcb, f: Fault) {
    // SAFETY: per the caller's guarantee; the handler capability names a
    // live endpoint.
    unsafe {
        match (*tcb).slots[FAULT_SLOT].cap {
            Cap::Endpoint { ptr, rights: r, badge } if r as usize & rights::WRITE != 0 => {
                (*tcb).fault = f;
                let none = Rights { grant: false, grant_reply: false };
                endpoint::send(ptr as *mut Endpoint, tcb, badge, none, true, true);
            }
            _ => {
                crate::warn!(
                    "fault: thread {tcb:p} has no fault handler (kind {}, pc {:#x}, address {:#x}); suspending it",
                    f.kind,
                    f.pc,
                    f.address
                );
                if tcb as usize == ROOT.load(Ordering::Relaxed) {
                    crate::error!("fault: the root task faulted");
                    crate::arch::shutdown(true);
                }
                sched::make_inactive(tcb);
            }
        }
    }
}

/// The handler answered `caller`'s fault with `label`: resume it at the
/// faulting instruction if 0, otherwise leave it suspended.
///
/// # Safety
/// `caller` must be live, waiting for its fault's reply, and on no queue.
pub unsafe fn answered(caller: *mut Tcb, label: usize) {
    // SAFETY: per the caller's guarantee.
    unsafe {
        (*caller).fault.take();
        (*caller).state = crate::thread::ThreadState::Inactive;
        if label == 0 {
            sched::make_runnable(caller);
        }
    }
}
