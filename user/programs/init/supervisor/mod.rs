//! The supervisor: starts the components of a composition
//! (`interfaces/system/supervisor.capnp`) and enforces their limits. It holds a
//! table of named capabilities that compositions route from. An instance's
//! memory is a memory account (`interfaces/system/memory.capnp`), a child of the
//! supervisor's, which the instance is built from a chunk of; or, for the
//! memory manager only, untyped memory the supervisor carves from its own
//! pool.
//! Either way, ending an instance is one revoke.
//!
//! It owns the components' exports: it makes each export's endpoint when it
//! enacts a composition, from its own memory, so the endpoint outlives the
//! server's instances. The server gets the receive side; each route to an
//! export gets a capability with a badge of its own, and the server asks
//! which component a badge is (`parent::CLIENT`).
//!
//! An instance whose component says `restart` is started again when it
//! fails, in the same memory, emptied, and with the same exports, so its
//! clients' capabilities keep working: a call it died in gets no reply
//! (`NO_REPLY`), and calls after wait for the new process.
//!
//! Components also join the supervisor's own composition at runtime, through
//! `Compositions` (`compose`), which the resolver uses; and compositions
//! start at runtime for a caller that pays for them, through the Supervisor
//! interface (`api`).
//!
//! Every capability the supervisor gives out names init's endpoint, told
//! apart by badge: an instance's parent capability and fault handler
//! (`INSTANCE_BADGE`) and its capability to the Supervisor interface
//! (`API_BADGE`), plus the instance's index; a runtime composition's handle
//! (`HANDLE_BADGE`) and capability to add to it (`COMPOSED_BADGE`), plus
//! its index. The supervisor keeps the original of each and hands out
//! copies, so revoking the original cuts off every holder before the index
//! is used again.
//! TODO: lazy start (design/processes.md in sap-docs).

mod api;
mod compose;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use capnp::message::{Builder, SingleSegmentAllocator};
use sap_interfaces::memory_capnp::{ROOT_ACCOUNT, ROOT_CONTROL};
use sap_interfaces::component_capnp::limits;
use sap_interfaces::supervisor_capnp::{component, composition, route, OnEnd};
use sap_interfaces::words;
use sap_loader::{self as loader, Give, Launch, Parent, Slots};
use sap_process::parent;
use sap_rt::abi::{fault, init_slot, object, rights, MAX_EXTRA_CAPS};
use sap_rt::cnode::SlotRef;
use sap_rt::endpoint::Give as Send;
use sap_rt::sys::time;
use sap_rt::{account, print, println, slots, tcb, untyped};

use crate::pack;

/// Badges on instances' capabilities, each plus the instance's index: its
/// parent capability and its capability to the Supervisor interface.
const INSTANCE_BADGE: usize = 0x300;
const API_BADGE: usize = 0x400;
/// Badges on a runtime composition's capabilities, each plus its index:
/// its caller's handle, and the capability to add to it.
const HANDLE_BADGE: usize = 0x500;
const COMPOSED_BADGE: usize = 0x700;
/// The badge on the capability to the supervisor's own `Compositions`.
const COMPOSITIONS_BADGE: usize = 0x600;
/// How many instances the supervisor runs at once, and how many runtime
/// compositions it keeps.
const MAX_INSTANCES: usize = 16;
const MAX_COMPOSED: usize = 16;
/// Slots in init's CNode for each instance's objects, while the loader
/// builds it.
const OBJECT_SLOTS: usize = 256;
/// How many times in a row an instance may fail within a second of
/// starting before the supervisor stops restarting it.
const MAX_RAPID: u32 = 3;
/// The groups a supervisor may hold: names standing for a set of
/// capabilities whose number depends on the machine. `ram`: the RAM, as
/// untyped memory, for the memory manager.
pub const GROUPS: &[&str] = &["ram"];
/// The rights of a client's capability to an export: call, and pass
/// capabilities both ways.
const CLIENT_RIGHTS: usize = rights::WRITE | rights::GRANT | rights::GRANT_REPLY;

fn slot(i: usize) -> SlotRef {
    SlotRef::new(init_slot::CNODE, i)
}

fn new_slot() -> usize {
    slots::alloc().expect("init: out of slots")
}

/// Delete the capability in `s` and free the slot.
fn drop_slot(s: usize) {
    let _ = slot(s).delete();
    slots::free(s);
}

/// Revoke and delete the original in `s`, and free the slot.
fn drop_original(s: usize) {
    let _ = slot(s).revoke();
    drop_slot(s);
}

/// What the dispatcher should do after handling a message.
pub enum Answer {
    /// Reply with this label, words and capabilities.
    Reply(usize, Vec<usize>, Vec<Send>),
    /// The caller is answered later; the reply object was kept, so receive
    /// with a new one.
    Later,
    /// Nobody to answer (a send, or a faulted thread being destroyed).
    Nothing,
}

/// Why a component couldn't be started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartError {
    /// The composition is malformed, or names a capability the supervisor
    /// doesn't hold or an export nobody declares.
    BadComponent,
    NoSuchProgram,
    /// Its limits exceed the supervisor's own, or its account's.
    OverLimit,
    /// The supervisor runs `MAX_INSTANCES` already.
    Full,
    /// Too little memory, or the loader failed.
    LoadFailed,
}

/// Where an instance's objects come from.
enum Memory {
    /// Untyped memory carved from the pool.
    Untyped(usize),
    /// A memory account: its own capability and its control,
    /// and the chunk the instance is built from.
    Account { account: usize, control: usize, chunk: usize },
}

/// How an instance ended for good: the schema's `End`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum End {
    Exited(u64),
    Faulted(u64),
    Stopped,
}

struct Instance {
    name: String,
    /// The component, kept to start it again: a single-segment message.
    component: Vec<usize>,
    memory: Memory,
    objects: usize,
    /// The originals of its parent capability and fault handler, and of its
    /// capability to the Supervisor interface.
    parent_cap: usize,
    api: usize,
    on_end: OnEnd,
    /// When its process started, and how many times in a row it failed
    /// within a second of starting.
    started: u64,
    rapid: u32,
    /// The runtime composition it is a member of, by index.
    composed: Option<usize>,
}

/// An endpoint a component serves; `endpoint` 0 once it is gone.
struct Export {
    component: String,
    name: String,
    endpoint: usize,
    /// The interface's type id; 0 for unchecked.
    interface: u64,
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
    /// init's endpoint, which all the badged capabilities name.
    endpoint: usize,
    /// Where received capabilities arrive: `MAX_EXTRA_CAPS` slots.
    pub window: usize,
    parent: Parent,
    /// The untyped memory instances' `untyped` limits are carved from.
    pool: usize,
    /// The capability to the supervisor's account, once the memory
    /// manager runs, and its control.
    account: Option<(usize, usize)>,
    held: Vec<(String, usize)>,
    /// The groups it holds, each a name of `GROUPS`.
    groups: Vec<(&'static str, Vec<usize>)>,
    exports: Vec<Export>,
    clients: Vec<Client>,
    /// The badge the next client gets: from `ROUTE_BADGES` up, so neither
    /// unbadged calls nor capabilities a server mints are anybody's route.
    next_badge: usize,
    /// `MAX_INSTANCES` entries, on the heap: init's stack is small.
    instances: Vec<Option<Instance>>,
    /// Routes to what it holds that components added through
    /// `Compositions` may have: (component, held name).
    grants: Vec<(String, String)>,
    /// Components added through its own `Compositions`, kept until
    /// `enact`.
    pending: Vec<Vec<usize>>,
    /// Compositions started at runtime, `MAX_COMPOSED` entries.
    composed: Vec<Option<api::Composed>>,
    /// Reply objects not in use, for when one is kept by a waiter.
    spare_replies: Vec<usize>,
}

impl Supervisor {
    pub fn new(endpoint: usize, pool: usize, parent: Parent) -> Self {
        let window = slots::alloc_range(MAX_EXTRA_CAPS).expect("init: slots for the receive window");
        Self {
            endpoint,
            window,
            parent,
            pool,
            account: None,
            held: Vec::new(),
            groups: Vec::new(),
            exports: Vec::new(),
            clients: Vec::new(),
            next_badge: parent::ROUTE_BADGES,
            instances: (0..MAX_INSTANCES).map(|_| None).collect(),
            grants: Vec::new(),
            pending: Vec::new(),
            composed: (0..MAX_COMPOSED).map(|_| None).collect(),
            spare_replies: Vec::new(),
        }
    }

    /// Hold `cap` under `name`, for compositions to route from.
    pub fn hold(&mut self, name: impl Into<String>, cap: usize) {
        self.held.push((name.into(), cap));
    }

    fn held(&self, name: &str) -> Option<usize> {
        self.held.iter().find(|(n, _)| n == name).map(|&(_, cap)| cap)
    }

    /// Hold `caps` as group `name`, for routes to give together.
    pub fn hold_group(&mut self, name: &'static str, caps: Vec<usize>) {
        assert!(GROUPS.contains(&name), "init: no group {name}");
        self.groups.push((name, caps));
    }

    fn group(&self, name: &str) -> Option<&[usize]> {
        self.groups.iter().find(|(n, _)| *n == name).map(|(_, caps)| &caps[..])
    }

    fn export(&self, component: &str, name: &str) -> Option<usize> {
        self.exports.iter().position(|e| e.endpoint != 0 && e.component == component && e.name == name)
    }

    fn running(&self, index: usize) -> bool {
        index < MAX_INSTANCES && self.instances[index].is_some()
    }

    /// A reply object for the next receive, after the last one was kept.
    pub fn fresh_reply(&mut self) -> usize {
        self.spare_replies.pop().unwrap_or_else(|| {
            let at = new_slot();
            let root = SlotRef { root: init_slot::CNODE, index: 0, depth: 0 };
            untyped::retype(self.parent.untyped, object::REPLY, 0, root, at, 1).expect("init: reply object");
            at
        })
    }

    /// Empty the receive window of anything a request left there.
    pub fn clear_window(&self) {
        for i in 0..MAX_EXTRA_CAPS {
            let _ = slot(self.window + i).delete();
        }
    }

    /// Let component `component`, added through the supervisor's own
    /// `Compositions`, have a route to what the supervisor holds as `held`
    /// (or the Supervisor interface, as "supervisor", or the end
    /// "endComposition").
    pub fn grant(&mut self, component: &str, held: &str) {
        self.grants.push((component.into(), held.into()));
    }

    /// Make the endpoints of `component`'s exports, from the supervisor's
    /// own memory, so they outlive its instances.
    fn make_exports(&mut self, component: component::Reader) -> Result<(), StartError> {
        let bad = |_| StartError::BadComponent;
        let owner = text(component.get_name())?;
        let exports = component.get_exports().map_err(bad)?;
        for export in exports {
            let name = text(export.get_name())?;
            if self.export(owner, name).is_some() || exports.iter().filter(|e| text(e.get_name()) == Ok(name)).count() > 1 {
                return Err(StartError::BadComponent);
            }
        }
        for export in exports {
            let endpoint = new_slot();
            let root = SlotRef { root: init_slot::CNODE, index: 0, depth: 0 };
            untyped::retype(self.parent.untyped, object::ENDPOINT, 0, root, endpoint, 1).expect("init: an export's endpoint");
            self.exports.push(Export { component: owner.into(), name: text(export.get_name())?.into(), endpoint, interface: export.get_interface() });
        }
        Ok(())
    }

    /// Make the endpoints of every export of `composition`, then start its
    /// components, in order.
    pub fn enact(&mut self, composition: composition::Reader) -> Result<(), StartError> {
        let bad = |_| StartError::BadComponent;
        let components = composition.get_components().map_err(bad)?;
        for component in components {
            self.make_exports(component)?;
        }
        // The root account, from the memory manager's export: the manager
        // knows its capabilities by these badges.
        if composition.has_accounts() {
            let accounts = composition.get_accounts().map_err(bad)?;
            let export = self.export(text(accounts.get_component())?, text(accounts.get_name())?).ok_or(StartError::BadComponent)?;
            let (account, control) = (new_slot(), new_slot());
            let endpoint = slot(self.exports[export].endpoint);
            slot(account).mint_from(endpoint, CLIENT_RIGHTS, ROOT_ACCOUNT as usize).expect("init: badge");
            slot(control).mint_from(endpoint, rights::WRITE, ROOT_CONTROL as usize).expect("init: badge");
            self.account = Some((account, control));
        }
        for component in components {
            self.start(component)?;
        }
        Ok(())
    }

    /// A new account for an instance with `terms`, a child of `parent`.
    fn new_account(parent: usize, limits: limits::Reader) -> Result<Memory, StartError> {
        let terms = account_terms(limits)?;
        let caps = slots::alloc_range(2).expect("init: out of slots");
        if account::child(parent, terms.get_reservation(), terms.get_limit(), init_slot::CNODE, caps).is_err() {
            slots::free_range(caps, 2);
            return Err(StartError::OverLimit);
        }
        Ok(Memory::Account { account: caps, control: caps + 1, chunk: new_slot() })
    }

    /// The memory an instance with `limits` is built from: untyped, or an
    /// account of the supervisor's (whose chunk `launch` takes).
    fn memory(&self, limits: limits::Reader) -> Result<Memory, StartError> {
        match limits.get_memory().which().map_err(|_| StartError::BadComponent)? {
            limits::memory::Untyped(_) => {
                let terms = untyped_terms(limits)?;
                let memory = new_slot();
                let root = SlotRef { root: init_slot::CNODE, index: 0, depth: 0 };
                if untyped::retype(self.pool, object::UNTYPED, terms.get_bits() as usize, root, memory, 1).is_err() {
                    slots::free(memory);
                    return Err(StartError::OverLimit);
                }
                Ok(Memory::Untyped(memory))
            }
            limits::memory::Account(_) => Self::new_account(self.account.ok_or(StartError::BadComponent)?.0, limits),
        }
    }

    /// Destroy everything made from `memory`, and give it back.
    fn end_memory(memory: &Memory) {
        match *memory {
            Memory::Untyped(memory) => drop_original(memory),
            Memory::Account { account, control, chunk } => {
                if let Err(error) = sap_rt::account::destroy(control) {
                    println!("init: ending an account failed: {error:?}");
                }
                for cap in [account, control, chunk] {
                    drop_slot(cap);
                }
            }
        }
    }

    /// Make an instance at `index` of `component`, named `name`, in
    /// `memory`, a member of runtime composition `composed` if any, and
    /// start it.
    fn create(&mut self, index: usize, name: String, component: component::Reader, memory: Memory, composed: Option<usize>) -> Result<(), StartError> {
        let on_end = component.get_on_end().map_err(|_| StartError::BadComponent)?;
        let (parent_cap, api) = (new_slot(), new_slot());
        let ep = slot(self.endpoint);
        slot(parent_cap).mint_from(ep, rights::WRITE | rights::GRANT, INSTANCE_BADGE + index).expect("init: badge");
        slot(api).mint_from(ep, CLIENT_RIGHTS, API_BADGE + index).expect("init: badge");
        let objects = slots::alloc_range(OBJECT_SLOTS).expect("init: slots for an instance");
        let kept = match keep(component) {
            Ok(kept) => kept,
            Err(error) => {
                for s in [parent_cap, api] {
                    drop_original(s);
                }
                slots::free_range(objects, OBJECT_SLOTS);
                Self::end_memory(&memory);
                return Err(error);
            }
        };
        let instance = Instance { name, component: kept, memory, objects, parent_cap, api, on_end, started: time(), rapid: 0, composed };
        if let Err(error) = self.launch(component, &instance) {
            self.destroy(instance);
            return Err(error);
        }
        self.instances[index] = Some(instance);
        Ok(())
    }

    fn start(&mut self, component: component::Reader) -> Result<(), StartError> {
        let bad = |_| StartError::BadComponent;
        let name: String = text(component.get_name())?.into();
        if component.get_on_end().map_err(|_| StartError::BadComponent)? == OnEnd::Restart {
            // A moved capability would be gone for the next instance.
            for route in component.get_imports().map_err(bad)? {
                if let Ok(route::Held(Ok(held))) = route.which()
                    && held.get_move()
                {
                    return Err(StartError::BadComponent);
                }
            }
        }
        let index = self.instances.iter().position(Option::is_none).ok_or(StartError::Full)?;
        let memory = self.memory(component.get_limits().map_err(bad)?)?;
        self.create(index, name, component, memory, None)
    }

    /// Build and start a process of `component` for `instance`, from its
    /// memory, with its parent capability.
    fn launch(&mut self, component: component::Reader, instance: &Instance) -> Result<(), StartError> {
        let bad = |_| StartError::BadComponent;
        let name = &instance.name;
        let hash: [u8; 32] = component.get_program().map_err(bad)?.try_into().map_err(|_| StartError::BadComponent)?;
        let image = pack::find(&pack::Hash(hash)).ok_or(StartError::NoSuchProgram)?;
        // The kernel holds priorities to init's own maximum.
        let limits = component.get_limits().map_err(bad)?;
        let bootstrap = component.get_bootstrap().map_err(bad)?;
        // Resolve every route before making anything.
        enum Source {
            Held(usize, bool),
            Client(usize),
            Copy(usize),
            /// A group, copied: it must be the last import.
            Group(Vec<usize>),
        }
        let mut sources = Vec::new();
        let routes = component.get_imports().map_err(bad)?;
        for (i, route) in routes.iter().enumerate() {
            sources.push(match route.which().map_err(|_| StartError::BadComponent)? {
                route::Held(held) => {
                    let held = held.map_err(bad)?;
                    let name = text(held.get_name())?;
                    match self.group(name) {
                        Some(group) if i + 1 == routes.len() as usize && !held.get_move() => Source::Group(group.to_vec()),
                        Some(_) => return Err(StartError::BadComponent),
                        None => Source::Held(self.held(name).ok_or(StartError::BadComponent)?, held.get_move()),
                    }
                }
                route::Client(to) => {
                    let to = to.map_err(bad)?;
                    let (component, export) = (text(to.get_component())?, text(to.get_name())?);
                    // In a runtime composition, another member's, else one the
                    // caller imports itself; else as named.
                    let export = match instance.composed {
                        Some(c) => match self.member_export(c, component, export) {
                            Some(e) => Some(e),
                            None if self.caller_imports(c, component, export)? => self.export(component, export),
                            None => None,
                        },
                        None => self.export(component, export),
                    };
                    let export = export.ok_or(StartError::BadComponent)?;
                    // The interface the importer expects must be the one served.
                    let (expected, served) = (to.get_interface(), self.exports[export].interface);
                    if expected != 0 && served != 0 && expected != served {
                        return Err(StartError::BadComponent);
                    }
                    Source::Client(export)
                }
                route::Serve(export) => Source::Copy(self.exports[self.export(name, text(export)?).ok_or(StartError::BadComponent)?].endpoint),
                route::Passed(i) => {
                    let passed = instance.composed.and_then(|c| self.composed[c].as_ref()).map_or(&[][..], |c| &c.passed[..]);
                    Source::Copy(*passed.get(i as usize).ok_or(StartError::BadComponent)?)
                }
                route::Supervisor(()) => Source::Copy(instance.api),
            });
        }

        // What the process is built from: its untyped, or a chunk of the
        // account.
        let (untyped, spare_bits, account) = match instance.memory {
            Memory::Untyped(memory) => (memory, untyped_terms(limits)?.get_spare_bits(), None),
            Memory::Account { account, chunk, .. } => {
                let bits = account_terms(limits)?.get_process_bits();
                sap_rt::account::allocate(account, bits, init_slot::CNODE, chunk).map_err(|_| StartError::OverLimit)?;
                (chunk, 0, Some(account))
            }
        };

        // Then the imports: nothing fails between here and the load.
        let (mut imports, mut moved, mut clients) = (Vec::new(), Vec::new(), Vec::new());
        for source in sources {
            imports.push(match source {
                Source::Group(caps) => {
                    imports.extend(caps.iter().map(|&cap| (cap, Give::Copy(rights::ALL))));
                    continue;
                }
                Source::Held(cap, true) => {
                    moved.push(cap);
                    (cap, Give::Move)
                }
                Source::Held(cap, false) | Source::Copy(cap) => (cap, Give::Copy(rights::ALL)),
                Source::Client(export) => {
                    let (cap, client) = self.client_cap(export, name);
                    clients.push((cap, client));
                    (cap, Give::Move)
                }
            });
        }

        let launch = Launch {
            priority: limits.get_priority() as usize,
            max_priority: limits.get_max_priority() as usize,
            cnode_radix: limits.get_cnode_bits() as usize,
            stack_pages: limits.get_stack_pages() as usize,
            spare_bits: (spare_bits > 0).then_some(spare_bits as usize),
            parent_endpoint: Some(instance.parent_cap),
            account,
            imports: &imports,
            bootstrap,
        };
        let parent = Parent { untyped, ..self.parent };
        let mut slots = Slots { next: instance.objects, end: instance.objects + OBJECT_SLOTS };
        let started = loader::load(image, &parent, &mut slots, &launch).and_then(|p| {
            tcb::set_fault_handler(p.tcb, Some(instance.parent_cap))?;
            Ok(p.start()?)
        });
        // Moved capabilities are gone from the table either way, and the
        // clients' capabilities from init's slots.
        self.held.retain(|(_, cap)| !moved.contains(cap));
        for &(cap, _) in &clients {
            drop_slot(cap);
        }
        if started.is_err() {
            return Err(StartError::LoadFailed);
        }
        self.clients.extend(clients.into_iter().map(|(_, client)| client));
        match limits.get_memory().which() {
            Ok(limits::memory::Account(Ok(terms))) => println!(
                "init: started {} ({} KiB image; account of {} KiB reserved, {} KiB limit)",
                name,
                image.len() / 1024,
                terms.get_reservation() >> 10,
                terms.get_limit() >> 10
            ),
            _ => println!("init: started {} ({} KiB image, in untyped memory from the pool)", name, image.len() / 1024),
        }
        Ok(())
    }

    /// A capability to call export `export` with a fresh badge, made for
    /// `component`, in a slot of its own.
    fn client_cap(&mut self, export: usize, component: &str) -> (usize, Client) {
        let (cap, badge) = (new_slot(), self.next_badge);
        self.next_badge += 1;
        slot(cap).mint_from(slot(self.exports[export].endpoint), CLIENT_RIGHTS, badge).expect("init: badge");
        (cap, Client { badge, export, component: component.into() })
    }

    /// Destroy everything instance `instance` was built from, keeping its
    /// memory (empty, with the account's reservation and the composition's
    /// limit), its parent capability and its exports, for a new process.
    fn reset(&mut self, instance: &Instance) -> Result<(), StartError> {
        match instance.memory {
            Memory::Untyped(memory) => {
                let _ = slot(memory).revoke();
            }
            Memory::Account { control, .. } => {
                account::revoke(control).map_err(|_| StartError::OverLimit)?;
                let limit = with_component(&instance.component, |component| {
                    Ok(account_terms(component.get_limits().map_err(|_| StartError::BadComponent)?)?.get_limit())
                })?;
                account::set_limit(control, limit).map_err(|_| StartError::OverLimit)?;
            }
        }
        for original in [instance.parent_cap, instance.api] {
            let _ = slot(original).revoke();
        }
        for object in instance.objects..instance.objects + OBJECT_SLOTS {
            let _ = slot(object).delete();
        }
        self.clients.retain(|c| c.component != instance.name);
        Ok(())
    }

    /// Start instance `index` again after it failed, unless it keeps
    /// failing: more than `MAX_RAPID` times in a row within a second of
    /// starting.
    /// TODO: back off between restarts, once the supervisor can wait with
    /// a timeout (kernel timeouts or a timer service).
    fn restart(&mut self, index: usize, end: End) {
        // What it started goes with it: its account's children do.
        self.stop_started_by(index);
        let mut instance = self.instances[index].take().expect("init: no such instance");
        let now = time();
        instance.rapid = if now - instance.started < self.parent.timer_hz { instance.rapid + 1 } else { 0 };
        if instance.rapid > MAX_RAPID {
            println!("init: {} keeps failing; not restarting it", instance.name);
            self.instances[index] = Some(instance);
            self.finish(index, end);
            return;
        }
        println!("init: restarting {}", instance.name);
        let result = self.reset(&instance).and_then(|()| {
            let words = core::mem::take(&mut instance.component);
            let result = with_component(&words, |component| self.launch(component, &instance));
            instance.component = words;
            result
        });
        instance.started = now;
        self.instances[index] = Some(instance);
        if let Err(error) = result {
            println!("init: restarting instance {index} failed: {error:?}");
            self.finish(index, end);
        }
    }

    /// End every composition instance `index` started, and forget those
    /// that ended: nobody is left to wait for them.
    fn stop_started_by(&mut self, index: usize) {
        for c in 0..MAX_COMPOSED {
            if self.composed[c].as_ref().is_some_and(|c| c.caller == index) {
                self.end_composed(c, End::Stopped, false);
            }
        }
    }

    /// Instance `index` ended for good, as `end`: destroy it, with what it
    /// started. If it belongs to a runtime composition, that ends too when
    /// the instance's end says so or it was the last member running.
    fn finish(&mut self, index: usize, end: End) {
        self.stop_started_by(index);
        let Some(instance) = self.instances[index].take() else { return };
        let (composed, ends_it) = (instance.composed, instance.on_end == OnEnd::EndComposition);
        self.destroy(instance);
        if let Some(c) = composed {
            let running = self.instances.iter().flatten().any(|i| i.composed == Some(c));
            let pending = self.composed[c].as_ref().is_some_and(|c| !c.pending.is_empty());
            if ends_it || (!running && !pending) {
                self.end_composed(c, end, true);
            }
        }
    }

    /// Destroy an instance: revoking its memory deletes every object made
    /// from it, then the memory itself goes back; with it go its originals
    /// and the capabilities made for its routes.
    fn destroy(&mut self, instance: Instance) {
        Self::end_memory(&instance.memory);
        for original in [instance.parent_cap, instance.api] {
            drop_original(original);
        }
        slots::free_range(instance.objects, OBJECT_SLOTS);
        self.clients.retain(|c| c.component != instance.name);
    }

    /// Handle a message whose badge is one of the supervisor's, if it is:
    /// from an instance's parent capability (an exit or a fault ends the
    /// instance, and restarts it if it failed and its component says so;
    /// `CLIENT` asks about a client of its exports), on the Supervisor interface, or
    /// on a handle.
    pub fn handle(&mut self, badge: usize, label: usize, words: &[usize], caps: usize, reply: usize) -> Option<Answer> {
        let index = badge & 0xff;
        if index >= MAX_INSTANCES {
            return None;
        }
        match badge & !0xff {
            INSTANCE_BADGE if self.running(index) => {}
            API_BADGE if self.running(index) => return Some(self.serve_api(index, label, words, caps)),
            HANDLE_BADGE if index < MAX_COMPOSED && self.composed[index].is_some() => return Some(self.serve_handle(index, label, reply)),
            COMPOSED_BADGE if index < MAX_COMPOSED && self.composed[index].as_ref().is_some_and(|c| c.ended.is_none()) => {
                return Some(self.serve_compositions(compose::Scope::Composed(index), label, words));
            }
            COMPOSITIONS_BADGE if badge == COMPOSITIONS_BADGE => return Some(self.serve_compositions(compose::Scope::Root, label, words)),
            _ => return None,
        }
        let (success, end) = match label {
            parent::CLIENT => return Some(self.client(index, words.first().copied().unwrap_or(0))),
            parent::EXIT => {
                let code = words.first().copied().unwrap_or(0);
                let name = &self.instances[index].as_ref().unwrap().name;
                if code == 0 {
                    println!("init: {name} exited successfully");
                } else {
                    println!("init: {name} exited with code {code}");
                }
                (code == 0, End::Exited(code as u64))
            }
            fault::VM | fault::USER_EXCEPTION if words.len() >= fault::LENGTH => {
                let name = &self.instances[index].as_ref().unwrap().name;
                println!("init: {name} faulted at pc {:#x}, address {:#x}, cause {}", words[fault::PC], words[fault::ADDRESS], words[fault::CAUSE]);
                (false, End::Faulted(label as u64))
            }
            _ => {
                println!("init: ignored a message from an instance with label {label}");
                return Some(Answer::Nothing);
            }
        };
        let instance = self.instances[index].as_ref().unwrap();
        match instance.on_end {
            OnEnd::Restart if !success => self.restart(index, end),
            // The end of the supervisor's own composition: the boot's.
            OnEnd::EndComposition if instance.composed.is_none() => {
                self.finish(index, end);
                print::halt(!success)
            }
            OnEnd::Forget | OnEnd::Restart | OnEnd::EndComposition => self.finish(index, end),
        }
        Some(Answer::Nothing)
    }

    /// Answer instance `index`'s question which component `badge` is: the
    /// name, if the badge is a client of one of its exports.
    fn client(&self, index: usize, badge: usize) -> Answer {
        let asker = &self.instances[index].as_ref().unwrap().name;
        let client = self.clients.iter().find(|c| c.badge == badge && self.exports[c.export].component == *asker);
        let Some(client) = client else { return Answer::Reply(parent::NO_SUCH_CLIENT, Vec::new(), Vec::new()) };
        let name = &client.component.as_bytes()[..client.component.len().min(parent::MAX_NAME)];
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

/// A copy of `component`, as a single-segment message, for
/// `with_component`.
fn keep(component: component::Reader) -> Result<Vec<usize>, StartError> {
    let size = component.total_size().map_err(|_| StartError::BadComponent)?.word_count as usize + 1;
    let mut space = vec![0u64; size];
    let mut allocator = SingleSegmentAllocator::new(sap_ring::as_bytes_mut(&mut space));
    let mut message = Builder::new(&mut allocator);
    message.set_root(component).map_err(|_| StartError::BadComponent)?;
    let mut kept = vec![0usize; size];
    let n = words::pack(&message, &mut kept).ok_or(StartError::BadComponent)?;
    kept.truncate(n);
    Ok(kept)
}

/// Call `f` with the component `keep` copied into `kept`.
fn with_component<T>(kept: &[usize], f: impl FnOnce(component::Reader) -> Result<T, StartError>) -> Result<T, StartError> {
    let segment = [words::bytes(kept)];
    let reader = words::reader(&segment);
    f(reader.get_root::<component::Reader>().map_err(|_| StartError::BadComponent)?)
}

fn untyped_terms(limits: limits::Reader<'_>) -> Result<sap_interfaces::component_capnp::untyped::Reader<'_>, StartError> {
    match limits.get_memory().which() {
        Ok(limits::memory::Untyped(Ok(terms))) => Ok(terms),
        _ => Err(StartError::BadComponent),
    }
}

fn account_terms(limits: limits::Reader<'_>) -> Result<sap_interfaces::component_capnp::account::Reader<'_>, StartError> {
    match limits.get_memory().which() {
        Ok(limits::memory::Account(Ok(terms))) => Ok(terms),
        _ => Err(StartError::BadComponent),
    }
}
