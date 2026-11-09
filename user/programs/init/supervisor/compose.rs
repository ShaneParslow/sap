//! `Compositions` (`interfaces/system/supervisor.capnp`): components joining
//! a composition at runtime. Each `add` checks the component, makes its
//! exports and keeps it; `enact` starts what was added, in order.
//!
//! The supervisor's own composition (`Scope::Root`) is the resolver's at
//! boot: a route to what the supervisor holds, the Supervisor interface or
//! the end `endComposition` needs a grant from the supervisor's own
//! configuration (`grant`). A composition made at runtime
//! (`Scope::Composed`, `api.rs`) has its caller's authority instead.

use alloc::string::String;
use alloc::vec::Vec;

use sap_interfaces::methods;
use sap_interfaces::supervisor_capnp::compositions::add_params;
use sap_interfaces::supervisor_capnp::{component, route, OnEnd, Status};
use sap_interfaces::words;
use sap_rt::abi::{init_slot, object, rights, MAX_EXTRA_CAPS};
use sap_rt::cnode::SlotRef;
use sap_rt::{account, untyped};

use super::api::{refuse, status};
use super::{
    account_terms, keep, new_slot, slot, text, with_component, Answer, Export, StartError, Supervisor, CLIENT_RIGHTS, COMPOSITIONS_BADGE, GROUPS,
};

/// Which composition: the supervisor's own, or one made at runtime.
#[derive(Clone, Copy)]
pub(super) enum Scope {
    Root,
    Composed(usize),
}

impl Supervisor {
    /// A capability to the supervisor's own `Compositions`, for init to
    /// hold for the resolver.
    pub fn compositions(&self) -> usize {
        let cap = new_slot();
        slot(cap).mint_from(slot(self.endpoint), CLIENT_RIGHTS & !rights::GRANT, COMPOSITIONS_BADGE).expect("init: badge");
        cap
    }

    pub(super) fn serve_compositions(&mut self, scope: Scope, label: usize, words: &[usize]) -> Answer {
        match label {
            methods::compositions::ADD => {
                let segment = [words::bytes(words)];
                let message = words::reader(&segment);
                let Ok(component) = message.get_root::<add_params::Reader>().and_then(|p| p.get_component()) else {
                    return refuse(Status::BadRequest);
                };
                refuse(self.add(scope, component).err().map_or(Status::Ok, status))
            }
            methods::compositions::ENACT => refuse(self.enact_pending(scope).err().map_or(Status::Ok, status)),
            _ => refuse(Status::BadRequest),
        }
    }

    /// The full name of `name` in `scope`.
    fn full_name(&self, scope: Scope, name: &str) -> String {
        match scope {
            Scope::Root => name.into(),
            Scope::Composed(c) => self.composed[c].as_ref().expect("init: no such composition").member(name),
        }
    }

    /// Whether `name` is taken: by an instance, or by a component added and
    /// not started yet.
    fn name_taken(&self, name: &str) -> bool {
        let named = |kept: &Vec<usize>| with_component(kept, |c| Ok(text(c.get_name())? == name)).unwrap_or(false);
        self.instances.iter().flatten().any(|i| i.name == name)
            || self.pending.iter().any(named)
            || self.composed.iter().flatten().any(|c| c.ended.is_none() && c.members.iter().any(|m| m == name))
    }

    /// Check `component` against what `scope` may have, make its exports
    /// and keep it for `enact`.
    pub(super) fn add(&mut self, scope: Scope, component: component::Reader) -> Result<(), StartError> {
        let bad = |_| StartError::BadComponent;
        let short = text(component.get_name())?;
        let name = self.full_name(scope, short);
        if short.contains('/') || self.name_taken(&name) {
            return Err(StartError::BadComponent);
        }
        let exports = component.get_exports().map_err(bad)?;
        let ends_composition = component.get_on_end().map_err(|_| StartError::BadComponent)? == OnEnd::EndComposition;
        match scope {
            Scope::Root if ends_composition && !self.granted(&name, "endComposition") => return Err(StartError::BadComponent),
            // Memory from the caller's account; the caller gets a capability
            // to each export of a component it starts, with the handle.
            Scope::Composed(_) if exports.len() as usize >= MAX_EXTRA_CAPS => return Err(StartError::BadComponent),
            Scope::Composed(_) => drop(account_terms(component.get_limits().map_err(bad)?)?),
            Scope::Root => {}
        }
        for route in component.get_imports().map_err(bad)? {
            let allowed = match (scope, route.which().map_err(|_| StartError::BadComponent)?) {
                (Scope::Root, route::Held(held)) => {
                    let held = held.map_err(bad)?;
                    let what = text(held.get_name())?;
                    !held.get_move() && (self.held(what).is_some() || GROUPS.contains(&what)) && self.granted(&name, what)
                }
                (Scope::Root, route::Supervisor(())) => self.granted(&name, "supervisor"),
                (Scope::Root, route::Client(_) | route::Serve(_)) => true,
                (Scope::Root, route::Passed(_)) => false,
                (Scope::Composed(_), route::Held(_)) => false,
                // A member's export, maybe one added later, or the caller's own
                // import: checked by `enact` (`clients_resolve`).
                (Scope::Composed(_), route::Client(_)) => true,
                (Scope::Composed(_), route::Serve(export)) => {
                    let export = text(export)?;
                    exports.iter().any(|e| text(e.get_name()) == Ok(export))
                }
                (Scope::Composed(c), route::Passed(i)) => (i as usize) < self.composed[c].as_ref().expect("init: no such composition").passed.len(),
                (Scope::Composed(_), route::Supervisor(())) => true,
            };
            if !allowed {
                return Err(StartError::BadComponent);
            }
        }
        let kept = keep(component)?;
        match scope {
            Scope::Root => {
                self.make_exports(component)?;
                self.pending.push(kept);
            }
            Scope::Composed(c) => {
                self.make_member_exports(c, &name, component)?;
                let composed = self.composed[c].as_mut().expect("init: no such composition");
                composed.members.push(name);
                composed.pending.push(kept);
            }
        }
        Ok(())
    }

    /// Whether every client route of `component`, a member of composition
    /// `c`, names a member's export or one the caller imports itself.
    fn clients_resolve(&self, c: usize, component: component::Reader) -> Result<bool, StartError> {
        for route in component.get_imports().map_err(|_| StartError::BadComponent)? {
            if let Ok(route::Client(to)) = route.which() {
                let to = to.map_err(|_| StartError::BadComponent)?;
                let (component, export) = (text(to.get_component())?, text(to.get_name())?);
                if self.member_export(c, component, export).is_none() && !self.caller_imports(c, component, export)? {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    /// Export `export` of member `component` of composition `c`.
    pub(super) fn member_export(&self, c: usize, component: &str, export: &str) -> Option<usize> {
        self.export(&self.composed[c].as_ref()?.member(component), export)
    }

    /// Whether the caller of composition `c` imports export `export` of
    /// `component` itself, so its members may too.
    pub(super) fn caller_imports(&self, c: usize, component: &str, export: &str) -> Result<bool, StartError> {
        let caller = self.composed[c].as_ref().expect("init: no such composition").caller;
        let kept = &self.instances[caller].as_ref().expect("init: no such caller").component;
        with_component(kept, |caller| {
            Ok(caller.get_imports().map_err(|_| StartError::BadComponent)?.iter().any(|r| match r.which() {
                Ok(route::Client(Ok(t))) => text(t.get_component()) == Ok(component) && text(t.get_name()) == Ok(export),
                _ => false,
            }))
        })
    }

    /// Make the endpoints of `component`'s exports for member `name` of
    /// composition `c`, from a chunk of the composition's payer.
    fn make_member_exports(&mut self, c: usize, name: &str, component: component::Reader) -> Result<(), StartError> {
        let exports = component.get_exports().map_err(|_| StartError::BadComponent)?;
        for export in exports {
            let export = text(export.get_name())?;
            if exports.iter().filter(|e| text(e.get_name()) == Ok(export)).count() > 1 {
                return Err(StartError::BadComponent);
            }
        }
        if exports.is_empty() {
            return Ok(());
        }
        // One page of endpoints for the whole composition.
        let composed = self.composed[c].as_mut().expect("init: no such composition");
        let chunk = match composed.chunk {
            Some((_, chunk)) => chunk,
            None => {
                let chunk = new_slot();
                match account::allocate(composed.payer, 12, init_slot::CNODE, chunk) {
                    Ok(allocated) => composed.chunk = Some((allocated.id, chunk)),
                    Err(_) => {
                        sap_rt::slots::free(chunk);
                        return Err(StartError::OverLimit);
                    }
                }
                chunk
            }
        };
        let root = SlotRef { root: init_slot::CNODE, index: 0, depth: 0 };
        for export in exports {
            let endpoint = new_slot();
            if untyped::retype(chunk, object::ENDPOINT, 0, root, endpoint, 1).is_err() {
                sap_rt::slots::free(endpoint);
                return Err(StartError::OverLimit);
            }
            self.composed[c].as_mut().expect("init: no such composition").endpoints.push(endpoint);
            self.exports.push(Export { component: name.into(), name: text(export.get_name())?.into(), endpoint, interface: export.get_interface() });
        }
        Ok(())
    }

    /// Start everything added to `scope` since its last `enact`, in order.
    /// In a runtime composition, every member's routes must resolve first:
    /// otherwise nothing starts.
    pub(super) fn enact_pending(&mut self, scope: Scope) -> Result<(), StartError> {
        if let Scope::Composed(c) = scope {
            let pending = &self.composed[c].as_ref().expect("init: no such composition").pending;
            for kept in pending {
                if !with_component(kept, |component| self.clients_resolve(c, component))? {
                    return Err(StartError::BadComponent);
                }
            }
        }
        let pending = match scope {
            Scope::Root => core::mem::take(&mut self.pending),
            Scope::Composed(c) => core::mem::take(&mut self.composed[c].as_mut().expect("init: no such composition").pending),
        };
        for kept in &pending {
            with_component(kept, |component| match scope {
                Scope::Root => self.start(component),
                Scope::Composed(c) => {
                    let name = self.full_name(scope, text(component.get_name())?);
                    let payer = self.composed[c].as_ref().expect("init: no such composition").payer;
                    let index = self.instances.iter().position(Option::is_none).ok_or(StartError::Full)?;
                    let memory = Self::new_account(payer, component.get_limits().map_err(|_| StartError::BadComponent)?)?;
                    self.create(index, name, component, memory, Some(c))
                }
            })?;
        }
        Ok(())
    }

    fn granted(&self, component: &str, what: &str) -> bool {
        self.grants.iter().any(|(c, w)| c == component && w == what)
    }
}
