//! The supervisor: starts the components of a composition
//! (`interfaces/supervisor.capnp`) and enforces their limits. It holds a
//! table of named capabilities that compositions route from, and a region
//! of untyped memory it carves each instance's memory from; ending an
//! instance is one revoke of that memory.
//!
//! It owns the components' exports: it makes each export's endpoint when it
//! enacts a composition, from its own memory, so the endpoint outlives the
//! server's instances. The server gets the receive side; each route to an
//! export gets a capability with a badge of its own, and the server asks
//! which component a badge is (`launcher::CLIENT`).
//!
//! Each instance's launcher and fault handler is a capability to init's
//! endpoint with the badge `INSTANCE_BADGE` + its index.
//! TODO: lazy start and restart (design/processes.md in sap-docs).

use alloc::string::String;
use alloc::vec::Vec;

use alloc::vec;

use sap_interfaces::supervisor_capnp::{component, composition, route, OnEnd};
use sap_loader::{self as loader, Give, Launch, Parent, Slots};
use sap_process::launcher;
use sap_rt::abi::{fault, init_slot, object, rights};
use sap_rt::cnode::SlotRef;
use sap_rt::{debug, println, slots, tcb, untyped};

use crate::factory::Answer;
use crate::pack;

/// Badges on instances' launcher capabilities: this plus the index.
pub const INSTANCE_BADGE: usize = 0x300;
/// How many instances the supervisor runs at once.
const MAX_INSTANCES: usize = 8;
/// Slots in init's CNode for each instance's objects, while the loader
/// builds it.
const OBJECT_SLOTS: usize = 256;
/// The rights of a client's capability to an export: call, and pass
/// capabilities both ways.
const CLIENT_RIGHTS: usize = rights::WRITE | rights::GRANT | rights::GRANT_REPLY;

fn slot(i: usize) -> SlotRef {
    SlotRef::new(init_slot::CNODE, i)
}

/// Why a component couldn't be started.
#[derive(Debug)]
pub enum StartError {
    /// The composition is malformed, or names a capability the supervisor
    /// doesn't hold or an export nobody declares.
    BadComponent,
    NoSuchProgram,
    /// Its limits exceed the supervisor's own.
    OverLimit,
    /// The supervisor runs `MAX_INSTANCES` already.
    Full,
    /// Too little memory, or the loader failed.
    LoadFailed,
}

struct Instance {
    name: String,
    /// The untyped region every object of the instance is made from.
    memory: usize,
    objects: usize,
    /// The badged endpoint capability that is its launcher and fault
    /// handler.
    launcher: usize,
    on_end: OnEnd,
}

/// An endpoint a component serves.
struct Export {
    component: String,
    name: String,
    endpoint: usize,
}

/// A capability made for a route to an export.
struct Client {
    badge: usize,
    /// Its index in `exports`.
    export: usize,
    /// The component it was made for.
    component: String,
}

pub struct Supervisor {
    /// init's endpoint, which the instances' launcher capabilities name.
    endpoint: usize,
    parent: Parent,
    /// The untyped region instances' memory is carved from.
    pool: usize,
    held: Vec<(String, usize)>,
    exports: Vec<Export>,
    clients: Vec<Client>,
    /// The badge the next client gets; never 0, so an unbadged call is
    /// nobody's.
    next_badge: usize,
    instances: [Option<Instance>; MAX_INSTANCES],
}

impl Supervisor {
    pub fn new(endpoint: usize, pool: usize, parent: Parent) -> Self {
        Self {
            endpoint,
            parent,
            pool,
            held: Vec::new(),
            exports: Vec::new(),
            clients: Vec::new(),
            next_badge: 1,
            instances: [const { None }; MAX_INSTANCES],
        }
    }

    /// Hold `cap` under `name`, for compositions to route from.
    pub fn hold(&mut self, name: impl Into<String>, cap: usize) {
        self.held.push((name.into(), cap));
    }

    fn held(&self, name: &str) -> Option<usize> {
        self.held.iter().find(|(n, _)| n == name).map(|&(_, cap)| cap)
    }

    fn export(&self, component: &str, name: &str) -> Option<usize> {
        self.exports.iter().position(|e| e.component == component && e.name == name)
    }

    /// Make the endpoints of every export of `composition`, then start its
    /// components, in order.
    pub fn enact(&mut self, composition: composition::Reader) -> Result<(), StartError> {
        let bad = |_| StartError::BadComponent;
        let components = composition.get_components().map_err(bad)?;
        for component in components {
            let owner = text(component.get_name())?;
            for export in component.get_exports().map_err(bad)? {
                let name = text(export.get_name())?;
                if self.export(owner, name).is_some() {
                    return Err(StartError::BadComponent);
                }
                let endpoint = slots::alloc().expect("init: out of slots");
                let root = SlotRef { root: init_slot::CNODE, index: 0, depth: 0 };
                untyped::retype(self.parent.untyped, object::ENDPOINT, 0, root, endpoint, 1).expect("init: an export's endpoint");
                self.exports.push(Export { component: owner.into(), name: name.into(), endpoint });
            }
        }
        for component in components {
            self.start(component)?;
        }
        Ok(())
    }

    fn start(&mut self, component: component::Reader) -> Result<(), StartError> {
        let bad = |_| StartError::BadComponent;
        let name: String = text(component.get_name())?.into();
        let hash: [u8; 32] = component.get_program().map_err(bad)?.try_into().map_err(|_| StartError::BadComponent)?;
        let image = pack::find(&pack::Hash(hash)).ok_or(StartError::NoSuchProgram)?;
        // The kernel holds priorities to init's own maximum.
        let limits = component.get_limits().map_err(bad)?;
        let on_end = component.get_on_end().map_err(|_| StartError::BadComponent)?;
        let bootstrap = component.get_bootstrap().map_err(bad)?;
        // Resolve every route before making anything.
        enum Source {
            Held(usize, bool),
            Client(usize),
            Serve(usize),
        }
        let mut sources = Vec::new();
        for route in component.get_imports().map_err(bad)? {
            sources.push(match route.which().map_err(|_| StartError::BadComponent)? {
                route::Held(held) => {
                    let held = held.map_err(bad)?;
                    let cap = self.held(text(held.get_name())?).ok_or(StartError::BadComponent)?;
                    Source::Held(cap, held.get_move())
                }
                route::Export(to) => {
                    let to = to.map_err(bad)?;
                    Source::Client(self.export(text(to.get_component())?, text(to.get_name())?).ok_or(StartError::BadComponent)?)
                }
                route::Serve(export) => Source::Serve(self.export(&name, text(export)?).ok_or(StartError::BadComponent)?),
            });
        }
        let index = self.instances.iter().position(Option::is_none).ok_or(StartError::Full)?;

        // The instance's memory, carved from the pool.
        let memory = slots::alloc().expect("init: out of slots");
        let root = SlotRef { root: init_slot::CNODE, index: 0, depth: 0 };
        if untyped::retype(self.pool, object::UNTYPED, limits.get_memory_bits() as usize, root, memory, 1).is_err() {
            slots::free(memory);
            return Err(StartError::OverLimit);
        }

        // Then the imports: nothing fails between here and the load.
        let (mut imports, mut moved, mut clients) = (Vec::new(), Vec::new(), Vec::new());
        for source in sources {
            imports.push(match source {
                Source::Held(cap, true) => {
                    moved.push(cap);
                    (cap, Give::Move)
                }
                Source::Held(cap, false) => (cap, Give::Copy(rights::ALL)),
                Source::Client(export) => {
                    let (cap, badge) = (slots::alloc().expect("init: out of slots"), self.next_badge);
                    self.next_badge += 1;
                    slot(cap).mint_from(slot(self.exports[export].endpoint), CLIENT_RIGHTS, badge).expect("init: badge");
                    clients.push((cap, Client { badge, export, component: name.clone() }));
                    (cap, Give::Move)
                }
                Source::Serve(export) => (self.exports[export].endpoint, Give::Copy(rights::READ)),
            });
        }

        let launcher_cap = slots::alloc().expect("init: out of slots");
        slot(launcher_cap).mint_from(slot(self.endpoint), rights::WRITE | rights::GRANT, INSTANCE_BADGE + index).expect("init: badge");
        let objects = slots::alloc_range(OBJECT_SLOTS).expect("init: slots for an instance");
        let launch = Launch {
            priority: limits.get_priority() as usize,
            max_priority: limits.get_max_priority() as usize,
            cnode_radix: limits.get_cnode_bits() as usize,
            stack_pages: limits.get_stack_pages() as usize,
            budget_bits: (limits.get_budget_bits() > 0).then_some(limits.get_budget_bits() as usize),
            launcher: Some(launcher_cap),
            imports: &imports,
            bootstrap,
        };
        let parent = Parent { untyped: memory, ..self.parent };
        let mut slots = Slots { next: objects, end: objects + OBJECT_SLOTS };
        let started = loader::load(image, &parent, &mut slots, &launch).and_then(|p| {
            tcb::set_fault_handler(p.tcb, Some(launcher_cap))?;
            Ok(p.start()?)
        });
        // Moved capabilities are gone from the table either way, and the
        // clients' capabilities from init's slots.
        self.held.retain(|(_, cap)| !moved.contains(cap));
        for &(cap, _) in &clients {
            let _ = slot(cap).delete();
            slots::free(cap);
        }
        let instance = Instance { name, memory, objects, launcher: launcher_cap, on_end };
        if started.is_err() {
            self.destroy(&instance);
            return Err(StartError::LoadFailed);
        }
        self.clients.extend(clients.into_iter().map(|(_, client)| client));
        println!("init: started {} ({} KiB image, 2^{} bytes of memory)", instance.name, image.len() / 1024, limits.get_memory_bits());
        self.instances[index] = Some(instance);
        Ok(())
    }

    /// Destroy an instance: revoking its memory deletes every object made
    /// from it, then the memory itself goes back to the pool.
    fn destroy(&self, instance: &Instance) {
        let _ = slot(instance.memory).revoke();
        let _ = slot(instance.memory).delete();
        let _ = slot(instance.launcher).revoke();
        let _ = slot(instance.launcher).delete();
        slots::free(instance.memory);
        slots::free(instance.launcher);
        slots::free_range(instance.objects, OBJECT_SLOTS);
    }

    /// Handle a message from an instance's launcher capability, if `badge`
    /// is one: an exit or a fault ends the instance; `CLIENT` asks about a
    /// client of its exports.
    pub fn handle(&mut self, badge: usize, label: usize, words: &[usize]) -> Option<Answer> {
        let index = badge.wrapping_sub(INSTANCE_BADGE);
        if index >= MAX_INSTANCES || self.instances[index].is_none() {
            return None;
        }
        let success = match label {
            launcher::CLIENT => return Some(self.client(index, words.first().copied().unwrap_or(0))),
            launcher::EXIT => {
                let code = words.first().copied().unwrap_or(0);
                let name = &self.instances[index].as_ref().unwrap().name;
                if code == 0 {
                    println!("init: {name} exited successfully");
                } else {
                    println!("init: {name} exited with code {code}");
                }
                code == 0
            }
            fault::VM | fault::USER_EXCEPTION if words.len() >= fault::LENGTH => {
                let name = &self.instances[index].as_ref().unwrap().name;
                println!("init: {name} faulted at pc {:#x}, address {:#x}, cause {}", words[fault::PC], words[fault::ADDRESS], words[fault::CAUSE]);
                false
            }
            _ => {
                println!("init: ignored a message from an instance with label {label}");
                return Some(Answer::Nothing);
            }
        };
        let instance = self.instances[index].take().unwrap();
        self.destroy(&instance);
        match instance.on_end {
            OnEnd::Forget => {}
            OnEnd::EndBoot => debug::halt(!success),
        }
        Some(Answer::Nothing)
    }

    /// Answer instance `index`'s question which component `badge` is: the
    /// name, if the badge is a client of one of its exports.
    fn client(&self, index: usize, badge: usize) -> Answer {
        let asker = &self.instances[index].as_ref().unwrap().name;
        let client = self.clients.iter().find(|c| c.badge == badge && self.exports[c.export].component == *asker);
        let Some(client) = client else { return Answer::Reply(launcher::NO_SUCH_CLIENT, Vec::new(), Vec::new()) };
        let name = &client.component.as_bytes()[..client.component.len().min(launcher::MAX_NAME)];
        let mut words = vec![0usize; 1 + name.len().div_ceil(8)];
        words[0] = name.len();
        for (i, &byte) in name.iter().enumerate() {
            words[1 + i / 8] |= (byte as usize) << (8 * (i % 8));
        }
        Answer::Reply(0, words, Vec::new())
    }
}

/// The text of a field of a composition.
fn text(field: capnp::Result<capnp::text::Reader<'_>>) -> Result<&str, StartError> {
    field.ok().and_then(|t| t.to_str().ok()).ok_or(StartError::BadComponent)
}
