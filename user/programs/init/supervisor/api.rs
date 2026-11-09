//! The Supervisor interface (`Supervisor` and `Instance` in
//! `interfaces/system/supervisor.capnp`): compositions started at runtime,
//! for a caller that pays for them. Calls arrive as `interfaces/README.md`
//! describes.
//!
//! `compose` makes an empty composition the caller fills through its own
//! `Compositions` capability (`compose.rs`) and starts with `enact`;
//! `start` is a composition of one, started at once. Either way the caller
//! gets an `Instance` handle to the whole composition.
//!
//! A member is named after the caller ("check/helper"), and its memory is
//! a child of the account the caller passes, which also pays for the
//! members' exports' endpoints, so the caller can spend only its own
//! memory. A member's routes may be capabilities passed in the call (kept,
//! for restarts), exports the caller imports itself (re-badged for it), its
//! own exports, other members' exports and the Supervisor interface;
//! nothing the supervisor holds. A composition ends with its caller. Its
//! handle doesn't own it: dropping the handle leaves it running (the
//! kernel can't tell the supervisor), but it is still bounded by its
//! caller's account and lifetime.

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use capnp::message::{Builder, SingleSegmentAllocator};
use sap_interfaces::methods;
use sap_interfaces::supervisor_capnp::instance::wait_results;
use sap_interfaces::supervisor_capnp::supervisor::start_params;
use sap_interfaces::supervisor_capnp::{component, Status};
use sap_interfaces::words;
use sap_rt::abi::rights;
use sap_rt::endpoint::{self, Give as Send};
use sap_rt::account;

use super::compose::Scope;
use super::{drop_original, drop_slot, new_slot, slot, text, Answer, End, StartError, Supervisor, CLIENT_RIGHTS, COMPOSED_BADGE, HANDLE_BADGE, MAX_COMPOSED};

/// A composition started at runtime for a caller.
pub(super) struct Composed {
    /// The index of the instance that made it, and its name, which its
    /// members' names start with.
    pub caller: usize,
    pub caller_name: String,
    /// The originals of the caller's handle to it (`HANDLE_BADGE`) and of
    /// the capability to add to it (`COMPOSED_BADGE`; 0 for `start`).
    pub handle: usize,
    pub compositions: usize,
    /// The capabilities passed in the call, kept to give each process a
    /// copy.
    pub passed: Vec<usize>,
    /// The caller's account, which pays.
    pub payer: usize,
    /// The chunk of it the members' exports' endpoints are made from (its
    /// id and slot), once a member has exports; the endpoints; the caller's
    /// capabilities to them, for `start`.
    pub chunk: Option<(u64, usize)>,
    pub endpoints: Vec<usize>,
    pub callers_caps: Vec<usize>,
    /// Every member added, by its full name, and those not started yet,
    /// kept until `enact`.
    pub members: Vec<String>,
    pub pending: Vec<Vec<usize>>,
    /// How it ended, while nobody has waited for it yet.
    pub ended: Option<End>,
    /// The reply object of a caller waiting for it to end.
    pub waiter: Option<usize>,
}

impl Composed {
    /// The full name of its member `name`.
    pub fn member(&self, name: &str) -> String {
        format!("{}/{name}", self.caller_name)
    }
}

pub(super) fn status(error: StartError) -> Status {
    match error {
        StartError::BadComponent => Status::BadRequest,
        StartError::NoSuchProgram => Status::NoSuchProgram,
        StartError::OverLimit => Status::OverLimit,
        StartError::Full => Status::Full,
        StartError::LoadFailed => Status::LoadFailed,
    }
}

pub(super) fn refuse(status: Status) -> Answer {
    Answer::Reply(status as usize, Vec::new(), Vec::new())
}

/// Answer a waiter on `reply`: the composition ended as `end`.
fn answer(reply: usize, end: End) {
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
    let _ = endpoint::reply(reply, Status::Ok as usize, &out[..n], &[]);
}

impl Supervisor {
    /// A call on the Supervisor interface from instance `caller`, with `caps`
    /// capabilities in the window.
    pub(super) fn serve_api(&mut self, caller: usize, label: usize, words: &[usize], caps: usize) -> Answer {
        if caps == 0 {
            return refuse(Status::BadRequest);
        }
        match label {
            methods::supervisor::START => {
                let segment = [words::bytes(words)];
                let message = words::reader(&segment);
                let Ok(component) = message.get_root::<start_params::Reader>().and_then(|p| p.get_component()) else {
                    return refuse(Status::BadRequest);
                };
                self.start_for(caller, component, caps).unwrap_or_else(|error| refuse(status(error)))
            }
            methods::supervisor::COMPOSE => match self.new_composed(caller, caps, true) {
                Ok(c) => {
                    let composed = self.composed[c].as_ref().expect("init: just made");
                    Answer::Reply(Status::Ok as usize, Vec::new(), vec![Send::copy(composed.compositions), Send::copy(composed.handle)])
                }
                Err(error) => refuse(status(error)),
            },
            _ => refuse(Status::BadRequest),
        }
    }

    /// A new composition for instance `caller`, paid from the account in
    /// the window's first slot, with the capabilities after it passed:
    /// they move out of the window. With `compositions`, a capability to
    /// add to it too.
    fn new_composed(&mut self, caller: usize, caps: usize, compositions: bool) -> Result<usize, StartError> {
        let c = self.composed.iter().position(Option::is_none).ok_or(StartError::Full)?;
        let take = |i: usize| {
            let s = new_slot();
            slot(s).move_from(slot(self.window + i)).expect("init: take a capability");
            s
        };
        let payer = take(0);
        let passed = (1..caps).map(take).collect();
        let mint = |badge: usize, rights: usize| {
            let s = new_slot();
            slot(s).mint_from(slot(self.endpoint), rights, badge).expect("init: badge");
            s
        };
        let handle = mint(HANDLE_BADGE + c, rights::WRITE | rights::GRANT_REPLY);
        let compositions = if compositions { mint(COMPOSED_BADGE + c, CLIENT_RIGHTS & !rights::GRANT) } else { 0 };
        let caller_name = self.instances[caller].as_ref().expect("init: no such caller").name.clone();
        self.composed[c] = Some(Composed {
            caller,
            caller_name,
            handle,
            compositions,
            passed,
            payer,
            chunk: None,
            endpoints: Vec::new(),
            callers_caps: Vec::new(),
            members: Vec::new(),
            pending: Vec::new(),
            ended: None,
            waiter: None,
        });
        Ok(c)
    }

    /// `Supervisor.start`: a composition of `component` alone, started at
    /// once; the caller gets the handle and a capability to call each of
    /// its exports, named for the caller.
    fn start_for(&mut self, caller: usize, component: component::Reader, caps: usize) -> Result<Answer, StartError> {
        let c = self.new_composed(caller, caps, false)?;
        let started = self.add(Scope::Composed(c), component).and_then(|()| self.enact_pending(Scope::Composed(c)));
        if let Err(error) = started {
            self.end_composed(c, End::Stopped, false);
            return Err(error);
        }
        let composed = self.composed[c].as_ref().expect("init: just started");
        let (name, caller_name, handle) = (composed.member(text(component.get_name())?), composed.caller_name.clone(), composed.handle);
        let mut give = vec![Send::copy(handle)];
        let mut callers_caps = Vec::new();
        for export in component.get_exports().map_err(|_| StartError::BadComponent)? {
            let e = self.export(&name, text(export.get_name())?).ok_or(StartError::BadComponent)?;
            let (cap, client) = self.client_cap(e, &caller_name);
            self.clients.push(client);
            give.push(Send::copy(cap));
            callers_caps.push(cap);
        }
        self.composed[c].as_mut().expect("init: just started").callers_caps = callers_caps;
        Ok(Answer::Reply(Status::Ok as usize, Vec::new(), give))
    }

    /// Composition `c` ended as `end`: stop its members that still run,
    /// give back what it holds, and answer its waiter, or keep the end for
    /// one if `keep` (not when its caller is gone); once answered, or if
    /// not kept, its handle stops working.
    pub(super) fn end_composed(&mut self, c: usize, end: End, keep: bool) {
        let Some(mut composed) = self.composed[c].take() else { return };
        if composed.ended.is_some() {
            // It ended before: only its handle is left.
            if keep {
                self.composed[c] = Some(composed);
            } else {
                drop_original(composed.handle);
            }
            return;
        }
        for i in 0..self.instances.len() {
            if self.instances[i].as_ref().is_some_and(|instance| instance.composed == Some(c)) {
                self.stop_started_by(i);
                let instance = self.instances[i].take().expect("init: a member");
                self.destroy(instance);
            }
        }
        // Its members' exports go: their endpoints were made from the
        // payer's chunk, and every capability to them with them.
        let gone: Vec<usize> = (0..self.exports.len()).filter(|&e| composed.members.contains(&self.exports[e].component)).collect();
        for &e in &gone {
            self.exports[e].endpoint = 0;
        }
        self.clients.retain(|client| !gone.contains(&client.export));
        if let Some((id, chunk)) = composed.chunk {
            let _ = account::release(composed.payer, id);
            drop_slot(chunk);
        }
        for &s in composed.endpoints.iter().chain(&composed.callers_caps).chain(&composed.passed) {
            drop_slot(s);
        }
        drop_slot(composed.payer);
        if composed.compositions != 0 {
            drop_original(composed.compositions);
        }
        if let Some(waiter) = composed.waiter.take() {
            answer(waiter, end);
            self.spare_replies.push(waiter);
            drop_original(composed.handle);
        } else if keep {
            composed.ended = Some(end);
            composed.pending.clear();
            self.composed[c] = Some(composed);
        } else {
            drop_original(composed.handle);
        }
    }

    /// A call on the handle to composition `c`, with `reply` to answer it
    /// on, now or when the composition ends.
    pub(super) fn serve_handle(&mut self, c: usize, label: usize, reply: usize) -> Answer {
        match label {
            methods::instance::WAIT => {
                let composed = self.composed[c].as_mut().expect("init: a handle without a composition");
                if composed.waiter.is_some() {
                    return refuse(Status::BadRequest);
                }
                let Some(end) = composed.ended else {
                    composed.waiter = Some(reply);
                    return Answer::Later;
                };
                answer(reply, end);
                // Answered: the handle stops working.
                let composed = self.composed[c].take().expect("init: no such composition");
                drop_original(composed.handle);
                Answer::Nothing
            }
            methods::instance::STOP => {
                self.end_composed(c, End::Stopped, true);
                refuse(Status::Ok)
            }
            _ => refuse(Status::BadRequest),
        }
    }
}

const _: () = assert!(MAX_COMPOSED <= 0x100, "an index fits in a badge's low byte");
