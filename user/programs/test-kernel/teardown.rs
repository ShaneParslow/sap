//! Teardown: a worker thread in each state it can wait or run in, and the
//! objects it depends on destroyed in each way: the thread suspended and
//! resumed, its capability deleted, its memory revoked and reused; the
//! object it waits on deleted, its memory revoked and reused; the reply
//! object it uses deleted. Run under the audit (`cargo run --profile audit
//! --features audit`), the kernel must stay consistent through every case,
//! and reusing revoked memory must find nothing still alive in it.
//!
//! The worker's objects and the worker itself come from two separate
//! untyped regions, so either side can be revoked while the other lives.
//! Nothing here blocks the test's own thread: when it receives, it polls.

use sap_rt::abi::{BootInfo, object, recv_source, rights};
use sap_rt::{endpoint, notification, println, sys, tcb};

use crate::support::{Entry, make, make_many, ok, slot, spawn_with, try_retype, untyped_at_least};

/// Each region's size, in bits: room for the objects, or for the worker's
/// thread, page tables, stack and IPC buffer.
const REGION_BITS: usize = 16;

/// Pages in a region, which reusing it fills with frames.
const REGION_PAGES: usize = 1 << (REGION_BITS - 12);

/// Where the worker's stack and IPC buffer go: nothing else here uses this
/// 1 GiB.
const WORKER_VA: usize = 0x38_0000_0000;

/// The states the worker is put in.
#[derive(Clone, Copy, Debug)]
enum State {
    /// Never started.
    Inactive,
    /// Runnable, spinning.
    Running,
    /// Waiting on a notification.
    WaitingOnNotification,
    /// Receiving on an endpoint, lending a reply object.
    Receiving,
    /// Receiving on an endpoint without a reply object.
    ReceivingWithoutReply,
    /// Receiving on an endpoint, with a notification bound.
    ReceivingBound,
    /// Blocked sending on an endpoint nobody receives on.
    Sending,
    /// Blocked calling an endpoint nobody receives on.
    Calling,
    /// Waiting for the answer to a call this thread received.
    WaitingForReply,
    /// Faulted, its fault blocked on its handler's endpoint.
    FaultSending,
    /// Faulted, its fault received by this thread and not yet answered.
    FaultWaitingForReply,
}

const STATES: [State; 11] = [
    State::Inactive,
    State::Running,
    State::WaitingOnNotification,
    State::Receiving,
    State::ReceivingWithoutReply,
    State::ReceivingBound,
    State::Sending,
    State::Calling,
    State::WaitingForReply,
    State::FaultSending,
    State::FaultWaitingForReply,
];

/// What is done to the worker or its objects.
#[derive(Clone, Copy, Debug)]
enum Action {
    SuspendAndResume,
    DeleteThread,
    RevokeThreadMemory,
    DeleteObject,
    RevokeObjectMemory,
    DeleteReply,
}

const ACTIONS: [Action; 6] = [
    Action::SuspendAndResume,
    Action::DeleteThread,
    Action::RevokeThreadMemory,
    Action::DeleteObject,
    Action::RevokeObjectMemory,
    Action::DeleteReply,
];

/// Wait on `ntfn` forever; when waiting fails, suspend itself (`me`).
extern "C" fn waiter(ntfn: usize, me: usize, _: usize) -> ! {
    loop {
        if notification::wait(ntfn).is_err() {
            let _ = tcb::suspend(me);
        }
    }
}

/// Receive on `ep` forever, lending `reply` (0 for none); when receiving
/// fails, suspend itself (`me`).
extern "C" fn receiver(ep: usize, reply: usize, me: usize) -> ! {
    loop {
        if endpoint::recv(ep, reply, &mut []).is_err() {
            let _ = tcb::suspend(me);
        }
    }
}

/// Send on `ep` forever; when sending fails, suspend itself (`me`).
extern "C" fn sender(ep: usize, me: usize, _: usize) -> ! {
    loop {
        if endpoint::send(ep, 1, &[], &[]).is_err() {
            let _ = tcb::suspend(me);
        }
    }
}

/// Call `ep` forever; when calling fails, suspend itself (`me`).
extern "C" fn caller(ep: usize, me: usize, _: usize) -> ! {
    loop {
        if endpoint::call(ep, 1, &[], &[], &mut []).is_err() {
            let _ = tcb::suspend(me);
        }
    }
}

/// Run an illegal instruction, again each time the fault is answered.
extern "C" fn faulter(_: usize, _: usize, _: usize) -> ! {
    loop {
        // SAFETY: deliberately illegal; the handler decides what follows.
        unsafe { core::arch::asm!("unimp") };
    }
}

extern "C" fn spinner(_: usize, _: usize, _: usize) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

/// The slots one case uses, always the same.
struct Slots {
    /// Where the two regions come from.
    parent: usize,
    /// The objects' region, and the objects.
    objects: usize,
    endpoint: usize,
    notification: usize,
    reply: usize,
    /// The reply object this thread receives the worker's calls with.
    own_reply: usize,
    /// The handler capability the worker's faults go to.
    handler: usize,
    /// The worker's region, and its thread's six slots.
    worker_memory: usize,
    worker: usize,
    /// Where reusing a region puts its frames.
    reuse: usize,
}

impl Slots {
    fn new(info: &BootInfo) -> Slots {
        let base = info.empty.start + 440;
        Slots {
            parent: untyped_at_least(info, 18),
            objects: base,
            endpoint: base + 1,
            notification: base + 2,
            reply: base + 3,
            own_reply: base + 4,
            handler: base + 5,
            worker_memory: base + 6,
            worker: base + 7,
            reuse: base + 16,
        }
    }

    /// Fresh regions and objects.
    fn reset(&self) {
        let objects = try_retype(self.parent, object::UNTYPED, REGION_BITS, self.objects, 1);
        ok("objects' region", objects);
        let worker = try_retype(
            self.parent,
            object::UNTYPED,
            REGION_BITS,
            self.worker_memory,
            1,
        );
        ok("worker's region", worker);

        make(self.objects, object::ENDPOINT, self.endpoint);
        make(self.objects, object::NOTIFICATION, self.notification);
        make_many(self.objects, object::REPLY, self.reply, 2);
        let handler = slot(self.handler).mint_from(slot(self.endpoint), rights::WRITE, 7);
        ok("handler capability", handler);
    }

    /// Start the worker at `entry(args)`, or only make it if `start` is
    /// false.
    fn worker(&self, entry: Entry, args: [usize; 3], start: bool) -> usize {
        spawn_with(
            self.worker_memory,
            self.worker,
            WORKER_VA,
            entry,
            args,
            start,
        )
    }

    /// Revoke `region` and fill it with frames, which the ledger refuses if
    /// anything made from it still lives; then revoke the frames.
    fn reuse(&self, region: usize) {
        ok("revoke region", slot(region).revoke());
        let frames = try_retype(region, object::FRAME_4K, 0, self.reuse, REGION_PAGES);
        ok("reuse region", frames);
        ok("revoke reused region", slot(region).revoke());
    }

    /// Remove both regions and everything made from them.
    fn clean_up(&self) {
        self.reuse(self.objects);
        self.reuse(self.worker_memory);
        ok("delete objects' region", slot(self.objects).delete());
        ok("delete worker's region", slot(self.worker_memory).delete());
        ok("revoke the parent", slot(self.parent).revoke());
    }
}

fn settle() {
    for _ in 0..3 {
        sys::yield_now();
    }
}

/// Receive, without blocking, the worker's call or fault on the endpoint,
/// with this thread's reply object, so the worker waits for the answer.
fn take_call(s: &Slots) {
    let received = ok(
        "receive the worker's call",
        endpoint::nb_recv(s.endpoint, s.own_reply, &mut []),
    );
    assert_eq!(
        received.source,
        recv_source::MESSAGE,
        "the worker should be waiting to call"
    );
}

/// Put a new worker in `state`. Returns its thread, the object it waits
/// on, and the reply object it uses, if any.
fn enter(s: &Slots, state: State) -> (usize, usize, Option<usize>) {
    let me = s.worker;
    let (ep, ntfn, reply) = (s.endpoint, s.notification, s.reply);

    let (thread, object, reply) = match state {
        State::Inactive => (s.worker(spinner, [0; 3], false), ep, None),
        State::Running => (s.worker(spinner, [0; 3], true), ep, None),
        State::WaitingOnNotification => (s.worker(waiter, [ntfn, me, 0], true), ntfn, None),
        State::Receiving => (s.worker(receiver, [ep, reply, me], true), ep, Some(reply)),
        State::ReceivingWithoutReply => (s.worker(receiver, [ep, 0, me], true), ep, None),

        State::ReceivingBound => {
            let thread = s.worker(receiver, [ep, reply, me], false);
            ok("bind", tcb::bind_notification(thread, ntfn));
            ok("start", tcb::resume(thread));
            (thread, ntfn, Some(reply))
        }

        State::Sending => (s.worker(sender, [ep, me, 0], true), ep, None),
        State::Calling => (s.worker(caller, [ep, me, 0], true), ep, None),

        State::WaitingForReply => {
            let thread = s.worker(caller, [ep, me, 0], true);
            settle();
            take_call(s);
            (thread, s.own_reply, Some(s.own_reply))
        }

        State::FaultSending | State::FaultWaitingForReply => {
            let thread = s.worker(faulter, [0; 3], false);
            ok(
                "fault handler",
                tcb::set_fault_handler(thread, Some(s.handler)),
            );
            ok("start", tcb::resume(thread));
            if matches!(state, State::FaultWaitingForReply) {
                settle();
                take_call(s);
                (thread, s.own_reply, Some(s.own_reply))
            } else {
                (thread, ep, None)
            }
        }
    };

    settle();
    (thread, object, reply)
}

/// Do `action` to the worker `thread`, which waits on `object` and uses
/// `reply`.
fn act(s: &Slots, action: Action, thread: usize, object: usize, reply: Option<usize>) {
    match action {
        Action::SuspendAndResume => {
            ok("suspend", tcb::suspend(thread));
            settle();
            ok("resume", tcb::resume(thread));
        }

        Action::DeleteThread => ok("delete the thread", slot(thread).delete()),
        Action::RevokeThreadMemory => s.reuse(s.worker_memory),

        Action::DeleteObject => {
            ok("revoke the object", slot(object).revoke());
            ok("delete the object", slot(object).delete());
        }

        Action::RevokeObjectMemory => s.reuse(s.objects),

        Action::DeleteReply => {
            if let Some(reply) = reply {
                ok("delete the reply object", slot(reply).delete());
            }
        }
    }
    settle();
}

/// Every state, with every action.
pub fn every_state(info: &BootInfo) {
    let s = Slots::new(info);
    let mut cases = 0;

    for state in STATES {
        for action in ACTIONS {
            s.reset();
            let (thread, object, reply) = enter(&s, state);
            act(&s, action, thread, object, reply);
            s.clean_up();
            cases += 1;
        }
    }

    println!("teardown: {cases} cases, every waiting state with every way of destroying");
}
