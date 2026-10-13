//! The scheduler: fixed priorities, round robin within a priority, as in
//! seL4 without MCS. Every runnable thread is in the ready queue of its
//! priority, including the one on the CPU; the thread that runs is the head
//! of the highest non-empty queue. Preemption and yielding move the current
//! thread to the tail of its queue.
//!
//! TODO: scheduling contexts (CPU time as a capability, donation on Call),
//! following seL4's MCS configuration (kernel-scheduling in the wiki).

use crate::global::Global;
use crate::thread::{Tcb, ThreadState, VSPACE_SLOT};

const PRIORITIES: usize = 256;
/// Timer ticks a thread runs before others at its priority get a turn.
const TIME_SLICE: u32 = 5;

#[derive(Clone, Copy)]
struct Queue {
    head: *mut Tcb,
    tail: *mut Tcb,
}

struct Scheduler {
    queues: [Queue; PRIORITIES],
    /// Bit `p` is set when queue `p` is non-empty.
    bitmap: [u64; PRIORITIES / 64],
    /// The thread on the CPU, or null when idle.
    current: *mut Tcb,
    slice_left: u32,
}

static SCHED: Global<Scheduler> = Global::new(Scheduler {
    queues: [Queue { head: core::ptr::null_mut(), tail: core::ptr::null_mut() }; PRIORITIES],
    bitmap: [0; PRIORITIES / 64],
    current: core::ptr::null_mut(),
    slice_left: TIME_SLICE,
});

fn sched() -> &'static mut Scheduler {
    // SAFETY: callers don't hold the reference across calls back into this
    // module (see global.rs).
    unsafe { SCHED.get() }
}

/// Make `tcb` runnable and append it to its ready queue.
///
/// # Safety
/// `tcb` must point at a live TCB.
pub unsafe fn make_runnable(tcb: *mut Tcb) {
    // SAFETY: per the caller's guarantee.
    let t = unsafe { &mut *tcb };
    if t.state != ThreadState::Running {
        t.state = ThreadState::Running;
        enqueue(tcb);
    }
}

/// Take `tcb` off the CPU and out of its queue.
///
/// # Safety
/// `tcb` must point at a live TCB.
pub unsafe fn make_inactive(tcb: *mut Tcb) {
    // SAFETY: per the caller's guarantee.
    let t = unsafe { &mut *tcb };
    if t.state == ThreadState::Running {
        dequeue(tcb);
    }
    t.state = ThreadState::Inactive;
    if sched().current == tcb {
        sched().current = core::ptr::null_mut();
    }
}

/// The thread on the CPU, or null when idle.
pub fn current() -> *mut Tcb {
    sched().current
}

/// Called on every timer tick.
pub fn tick() {
    let s = sched();
    if s.current.is_null() {
        return;
    }
    s.slice_left -= 1;
    if s.slice_left == 0 {
        rotate_current();
    }
}

/// Let other threads of the same priority run first.
pub fn yield_current() {
    rotate_current();
}

fn rotate_current() {
    let s = sched();
    s.slice_left = TIME_SLICE;
    let current = s.current;
    // SAFETY: `current` is a live TCB while it is set.
    if !current.is_null() && unsafe { (*current).state } == ThreadState::Running {
        dequeue(current);
        enqueue(current);
    }
}

/// Run the highest-priority runnable thread, waiting for an interrupt while
/// there is none. Called at the end of every entry from user mode.
pub fn run() -> ! {
    loop {
        if let Some(next) = highest() {
            let s = sched();
            if s.current != next {
                s.current = next;
                s.slice_left = TIME_SLICE;
            }
            // SAFETY: queued TCBs are live.
            let tcb = unsafe { &mut *next };
            if crate::arch::activate_vspace(tcb.slots[VSPACE_SLOT].cap) {
                crate::arch::return_to_user(&mut tcb.context);
            }
            // TODO: a thread without a valid address space should fault to
            // its handler (M4); until then it is suspended.
            crate::warn!("sched: thread {next:p} has no valid address space; suspending it");
            // SAFETY: next is live.
            unsafe { make_inactive(next) };
            continue;
        }
        sched().current = core::ptr::null_mut();
        crate::arch::idle_once();
    }
}

fn highest() -> Option<*mut Tcb> {
    let s = sched();
    let (word, bits) = s.bitmap.iter().enumerate().rev().find(|(_, b)| **b != 0)?;
    let priority = word * 64 + (63 - bits.leading_zeros() as usize);
    Some(s.queues[priority].head)
}

fn enqueue(tcb: *mut Tcb) {
    let s = sched();
    // SAFETY: tcb is live (callers' guarantee) and not in any queue.
    let t = unsafe { &mut *tcb };
    let p = t.priority as usize;
    let q = &mut s.queues[p];
    t.queue_next = core::ptr::null_mut();
    t.queue_prev = q.tail;
    if q.tail.is_null() {
        q.head = tcb;
    } else {
        // SAFETY: queued TCBs are live.
        unsafe { (*q.tail).queue_next = tcb };
    }
    q.tail = tcb;
    s.bitmap[p / 64] |= 1 << (p % 64);
}

fn dequeue(tcb: *mut Tcb) {
    let s = sched();
    // SAFETY: tcb is live and in the queue of its priority.
    let t = unsafe { &mut *tcb };
    let p = t.priority as usize;
    let q = &mut s.queues[p];
    // SAFETY: neighbours in a queue are live TCBs.
    unsafe {
        match t.queue_prev.is_null() {
            true => q.head = t.queue_next,
            false => (*t.queue_prev).queue_next = t.queue_next,
        }
        match t.queue_next.is_null() {
            true => q.tail = t.queue_prev,
            false => (*t.queue_next).queue_prev = t.queue_prev,
        }
    }
    t.queue_next = core::ptr::null_mut();
    t.queue_prev = core::ptr::null_mut();
    if q.head.is_null() {
        s.bitmap[p / 64] &= !(1 << (p % 64));
    }
}
