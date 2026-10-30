//! `Compositions` (`interfaces/supervisor.capnp`): components joining the
//! supervisor's own composition at runtime, added by the resolver. Each
//! `add` checks the component, makes its exports and keeps it; `enact`
//! starts what was added, in order. A route to what the supervisor holds
//! needs a grant from the supervisor's own configuration (`grant`).

use alloc::vec::Vec;

use sap_interfaces::methods;
use sap_interfaces::supervisor_capnp::compositions::add_params;
use sap_interfaces::supervisor_capnp::{component, route, OnEnd, Status};
use sap_interfaces::words;
use sap_rt::abi::rights;

use super::{keep, new_slot, slot, text, with_component, Answer, StartError, Supervisor, CLIENT_RIGHTS, COMPOSITIONS_BADGE, GROUPS};

fn reply(status: Status) -> Answer {
    Answer::Reply(status as usize, Vec::new(), Vec::new())
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

impl Supervisor {
    /// A capability to `Compositions`, for init to hold for the resolver.
    pub fn compositions(&self) -> usize {
        let cap = new_slot();
        slot(cap).mint_from(slot(self.endpoint), CLIENT_RIGHTS & !rights::GRANT, COMPOSITIONS_BADGE).expect("init: badge");
        cap
    }

    pub(super) fn serve_compositions(&mut self, label: usize, words: &[usize]) -> Answer {
        match label {
            methods::compositions::ADD => {
                let segment = [words::bytes(words)];
                let message = words::reader(&segment);
                let Ok(component) = message.get_root::<add_params::Reader>().and_then(|p| p.get_component()) else {
                    return reply(Status::BadRequest);
                };
                reply(self.add(component).err().map_or(Status::Ok, status))
            }
            methods::compositions::ENACT => {
                let pending = core::mem::take(&mut self.pending);
                for kept in &pending {
                    if let Err(error) = with_component(kept, |component| self.start(component)) {
                        return reply(status(error));
                    }
                }
                reply(Status::Ok)
            }
            _ => reply(Status::BadRequest),
        }
    }

    /// Check `component`, make its exports and keep it for `enact`.
    fn add(&mut self, component: component::Reader) -> Result<(), StartError> {
        let bad = |_| StartError::BadComponent;
        let name = text(component.get_name())?;
        let named = |kept: &Vec<usize>| with_component(kept, |c| Ok(text(c.get_name())? == name)).unwrap_or(false);
        if self.instances.iter().flatten().any(|i| !i.ended() && i.name == name) || self.pending.iter().any(named) {
            return Err(StartError::BadComponent);
        }
        if component.get_on_end().map_err(|_| StartError::BadComponent)? == OnEnd::EndBoot && !self.granted(name, "endBoot") {
            return Err(StartError::BadComponent);
        }
        for route in component.get_imports().map_err(bad)? {
            let allowed = match route.which().map_err(|_| StartError::BadComponent)? {
                route::Held(held) => {
                    let held = held.map_err(bad)?;
                    let what = text(held.get_name())?;
                    !held.get_move() && (self.held(what).is_some() || GROUPS.contains(&what)) && self.granted(name, what)
                }
                route::Supervisor(()) => self.granted(name, "supervisor"),
                route::Export(_) | route::Serve(_) => true,
                route::Passed(_) => false,
            };
            if !allowed {
                return Err(StartError::BadComponent);
            }
        }
        let kept = keep(component)?;
        self.make_exports(component)?;
        self.pending.push(kept);
        Ok(())
    }

    fn granted(&self, component: &str, what: &str) -> bool {
        self.grants.iter().any(|(c, w)| c == component && w == what)
    }
}
