//! The instance API (`Supervisor` and `Instance` in
//! `interfaces/system/supervisor.capnp`): components started at runtime, for a
//! caller that pays for them. Calls arrive as `interfaces/README.md`
//! describes.
//!
//! Such an instance is named after its caller ("check/helper"), and its
//! memory is a child of the account the caller passes, which also pays for
//! its exports' endpoints, so the caller can spend only its own memory. Its
//! routes may be capabilities passed in the call (kept, for restarts),
//! exports the caller imports itself (re-badged for it), its own exports
//! and the instance API; nothing the supervisor holds. It ends with its
//! caller. Its handle doesn't own it: dropping the handle leaves it running
//! (the kernel can't tell the supervisor), but it is still bounded by its
//! caller's account and lifetime.

use alloc::format;
use alloc::vec;
use alloc::vec::Vec;

use capnp::message::{Builder, SingleSegmentAllocator};
use sap_interfaces::methods;
use sap_interfaces::supervisor_capnp::instance::wait_results;
use sap_interfaces::supervisor_capnp::supervisor::start_params;
use sap_interfaces::supervisor_capnp::{component, route, OnEnd, Status};
use sap_interfaces::words;
use sap_rt::abi::{init_slot, object, rights, MAX_EXTRA_CAPS};
use sap_rt::cnode::SlotRef;
use sap_rt::ipc::{self, Give as Send};
use sap_rt::{account, untyped};

use super::{
    account_terms, drop_original, drop_slot, new_slot, slot, text, with_component, Answer, End, Export, Instance, Memory, StartError,
    Supervisor, HANDLE_BADGE, MAX_INSTANCES,
};

/// What an instance started through the API has besides an instance.
pub(super) struct Dynamic {
    /// The index of the instance that started it.
    pub caller: usize,
    /// The original of its caller's handle; 0 until it is made.
    pub handle: usize,
    /// The capabilities passed in the call, kept to give each process a
    /// copy.
    pub passed: Vec<usize>,
    /// The caller's account, which pays.
    pub payer: usize,
    /// The chunk of it the exports' endpoints are made from (its id and
    /// slot), the endpoints, and the caller's capabilities to them.
    pub chunk: Option<(u64, usize)>,
    pub endpoints: Vec<usize>,
    pub callers_caps: Vec<usize>,
    /// How it ended, while nobody has waited for it yet.
    pub ended: Option<End>,
    /// The reply object of a caller waiting for it to end.
    pub waiter: Option<usize>,
}

fn status(error: StartError) -> Status {
    match error {
        StartError::BadComponent => Status::BadRequest,
        StartError::NoSuchProgram => Status::NoSuchProgram,
        StartError::OverLimit => Status::OverLimit,
        StartError::Full => Status::Full,
        StartError::LoadFailed => Status::LoadFailed,
    }
}

fn refuse(status: Status) -> Answer {
    Answer::Reply(status as usize, Vec::new(), Vec::new())
}

/// Give back what `dynamic` holds besides its handle: its exports'
/// endpoints (releasing their chunk destroys them, and every capability to
/// them), the passed capabilities and the payer's.
pub(super) fn release(dynamic: &Dynamic) {
    if let Some((id, chunk)) = dynamic.chunk {
        let _ = account::release(dynamic.payer, id);
        drop_slot(chunk);
    }
    for &s in dynamic.endpoints.iter().chain(&dynamic.callers_caps).chain(&dynamic.passed) {
        drop_slot(s);
    }
    drop_slot(dynamic.payer);
}

/// An entry for an instance that has ended and waits to be waited for.
pub(super) fn ended(dynamic: Dynamic) -> Instance {
    Instance {
        name: Default::default(),
        component: Vec::new(),
        memory: Memory::Untyped(0),
        objects: 0,
        launcher: 0,
        api: 0,
        on_end: OnEnd::Forget,
        started: 0,
        rapid: 0,
        dynamic: Some(dynamic),
    }
}

/// Answer a waiter on `reply`: the instance ended as `end`.
pub(super) fn answer(reply: usize, end: End) {
    let mut scratch = [0u64; 8];
    let mut allocator = SingleSegmentAllocator::new(sap_ring::as_bytes_mut(&mut scratch));
    let mut message = Builder::new(&mut allocator);
    let mut root = message.init_root::<wait_results::Builder>().init_end();
    match end {
        End::Exited(code) => root.set_exited(code),
        End::Faulted(label) => root.set_faulted(label),
        End::Stopped => root.set_stopped(()),
    }
    let mut out = [0usize; 8];
    let n = words::pack(&message, &mut out).expect("init: a wait reply fits");
    let _ = ipc::reply(reply, Status::Ok as usize, &out[..n], &[]);
}

impl Supervisor {
    /// A call on the instance API from instance `caller`, with `caps`
    /// capabilities in the window.
    pub(super) fn serve_api(&mut self, caller: usize, label: usize, words: &[usize], caps: usize) -> Answer {
        if label != methods::supervisor::START || caps == 0 {
            return refuse(Status::BadRequest);
        }
        let segment = [words::bytes(words)];
        let message = words::reader(&segment);
        let Ok(component) = message.get_root::<start_params::Reader>().and_then(|p| p.get_component()) else {
            return refuse(Status::BadRequest);
        };
        self.start_for(caller, component, caps).unwrap_or_else(|error| refuse(status(error)))
    }

    /// Whether `component` may be started for `caller` with `passed`
    /// capabilities: an account, not ending the boot, and routes it may
    /// have.
    fn check_request(&self, caller: usize, component: component::Reader, passed: usize) -> Result<(), StartError> {
        let bad = |_| StartError::BadComponent;
        account_terms(component.get_limits().map_err(bad)?)?;
        if component.get_on_end().map_err(|_| StartError::BadComponent)? == OnEnd::EndBoot {
            return Err(StartError::BadComponent);
        }
        let exports = component.get_exports().map_err(bad)?;
        if exports.len() as usize >= MAX_EXTRA_CAPS {
            return Err(StartError::BadComponent);
        }
        let caller_component = &self.instances[caller].as_ref().expect("init: no such caller").component;
        for route in component.get_imports().map_err(bad)? {
            let allowed = match route.which().map_err(|_| StartError::BadComponent)? {
                route::Held(_) => false,
                route::Client(to) => {
                    let to = to.map_err(bad)?;
                    let (c, n) = (text(to.get_component())?, text(to.get_name())?);
                    // Only an export the caller imports itself.
                    with_component(caller_component, |caller| {
                        Ok(caller.get_imports().map_err(bad)?.iter().any(|r| match r.which() {
                            Ok(route::Client(Ok(t))) => text(t.get_component()) == Ok(c) && text(t.get_name()) == Ok(n),
                            _ => false,
                        }))
                    })?
                }
                route::Serve(name) => {
                    let name = text(name)?;
                    exports.iter().any(|e| text(e.get_name()) == Ok(name))
                }
                route::Passed(i) => (i as usize) < passed,
                route::Supervisor(()) => true,
            };
            if !allowed {
                return Err(StartError::BadComponent);
            }
        }
        Ok(())
    }

    fn start_for(&mut self, caller: usize, component: component::Reader, caps: usize) -> Result<Answer, StartError> {
        let bad = |_| StartError::BadComponent;
        let caller_name = self.instances[caller].as_ref().expect("init: no such caller").name.clone();
        let name = format!("{caller_name}/{}", text(component.get_name())?);
        if self.instances.iter().flatten().any(|i| !i.ended() && i.name == name) {
            return Err(StartError::BadComponent);
        }
        self.check_request(caller, component, caps - 1)?;
        let index = self.instances.iter().position(Option::is_none).ok_or(StartError::Full)?;

        // The capabilities move out of the window: the payer's account, then
        // the passed ones.
        let take = |i: usize| {
            let s = new_slot();
            slot(s).move_from(slot(self.window + i)).expect("init: take a capability");
            s
        };
        let payer = take(0);
        let passed = (1..caps).map(take).collect();
        let mut dynamic =
            Dynamic { caller, handle: 0, passed, payer, chunk: None, endpoints: Vec::new(), callers_caps: Vec::new(), ended: None, waiter: None };

        // Its exports' endpoints, from a page of the payer's account.
        let exports = component.get_exports().map_err(bad)?;
        if !exports.is_empty() {
            let chunk = new_slot();
            match account::allocate(payer, 12, init_slot::CNODE, chunk) {
                Ok(c) => dynamic.chunk = Some((c.id, chunk)),
                Err(_) => {
                    sap_rt::slots::free(chunk);
                    release(&dynamic);
                    return Err(StartError::OverLimit);
                }
            }
            let root = SlotRef { root: init_slot::CNODE, index: 0, depth: 0 };
            for export in exports {
                let endpoint = new_slot();
                untyped::retype(chunk, object::ENDPOINT, 0, root, endpoint, 1).expect("init: an export's endpoint");
                dynamic.endpoints.push(endpoint);
                self.exports.push(Export { component: name.clone(), name: text(export.get_name())?.into(), endpoint, interface: export.get_interface() });
            }
        }
        let memory = match Self::new_account(payer, component.get_limits().map_err(bad)?) {
            Ok(memory) => memory,
            Err(error) => {
                for export in self.exports.iter_mut().filter(|e| e.component == name) {
                    export.endpoint = 0;
                }
                release(&dynamic);
                return Err(error);
            }
        };
        self.create(index, name.clone(), component, memory, Some(dynamic))?;

        // The caller's handle, and a capability to call each export, named
        // for the caller.
        let handle = new_slot();
        slot(handle).mint_from(slot(self.endpoint), rights::WRITE | rights::GRANT_REPLY, HANDLE_BADGE + index).expect("init: badge");
        let mut give = vec![Send::copy(handle)];
        let mut callers_caps = Vec::new();
        for export in exports {
            let e = self.export(&name, text(export.get_name())?).ok_or(StartError::BadComponent)?;
            let (cap, client) = self.client_cap(e, &caller_name);
            self.clients.push(client);
            give.push(Send::copy(cap));
            callers_caps.push(cap);
        }
        let dynamic = self.instances[index].as_mut().and_then(|i| i.dynamic.as_mut()).expect("init: just started");
        dynamic.handle = handle;
        dynamic.callers_caps = callers_caps;
        Ok(Answer::Reply(Status::Ok as usize, Vec::new(), give))
    }

    /// A call on the handle to instance `index`, with `reply` to answer it
    /// on, now or when the instance ends.
    pub(super) fn serve_handle(&mut self, index: usize, label: usize, reply: usize) -> Answer {
        match label {
            methods::instance::WAIT => {
                let dynamic = self.instances[index].as_mut().and_then(|i| i.dynamic.as_mut()).expect("init: a handle without an instance");
                if dynamic.waiter.is_some() {
                    return refuse(Status::BadRequest);
                }
                let Some(end) = dynamic.ended else {
                    dynamic.waiter = Some(reply);
                    return Answer::Later;
                };
                answer(reply, end);
                // Answered: the handle stops working.
                let instance = self.instances[index].take().expect("init: no such instance");
                drop_original(instance.dynamic.expect("init: dynamic").handle);
                Answer::Nothing
            }
            methods::instance::STOP => {
                if self.running(index) {
                    self.finish(index, End::Stopped, true);
                }
                refuse(Status::Ok)
            }
            _ => refuse(Status::BadRequest),
        }
    }
}

const _: () = assert!(MAX_INSTANCES <= 0x100, "an index fits in a badge's low byte");
