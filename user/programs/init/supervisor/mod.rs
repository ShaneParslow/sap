//! The supervisor: starts the components of a composition
//! (`interfaces/supervisor.capnp`) and enforces their limits. It holds a
//! table of named capabilities that compositions route from. An instance's
//! memory is a memory account (`interfaces/memory.capnp`), a child of the
//! supervisor's, which the instance is built from a chunk of; or, for the
//! memory manager only, a region the supervisor carves from its own pool.
//! Either way, ending an instance is one revoke.
//!
//! It owns the components' exports: it makes each export's endpoint when it
//! enacts a composition, from its own memory, so the endpoint outlives the
//! server's instances. The server gets the receive side; each route to an
//! export gets a capability with a badge of its own, and the server asks
//! which component a badge is (`launcher::CLIENT`).
//!
//! An instance whose component says `restart` is started again when it
//! fails, in the same memory, emptied, and with the same exports, so its
//! clients' capabilities keep working: a call it died in gets no reply
//! (`NO_REPLY`), and calls after wait for the new process.
//!
//! Components also start at runtime, through the instance API (`api`).
//!
//! Every capability the supervisor gives an instance names init's endpoint,
//! told apart by badge: its launcher and fault handler (`INSTANCE_BADGE`),
//! its capability to the instance API (`API_BADGE`), and, for one started
//! through the API, its caller's handle to it (`HANDLE_BADGE`), each plus
//! the instance's index. The supervisor keeps the original of each and
//! hands out copies, so revoking the original cuts off every holder before
//! the index is used again.
//! TODO: lazy start (design/processes.md in sap-docs).

mod api;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use capnp::message::{Builder, SingleSegmentAllocator};
use sap_interfaces::memory_capnp::{ROOT_CONTROLLER, ROOT_HOLDER};
use sap_interfaces::supervisor_capnp::{component, composition, limits, route, OnEnd};
use sap_interfaces::words;
use sap_loader::{self as loader, Give, Launch, Parent, Slots};
use sap_process::launcher;
use sap_rt::abi::{fault, init_slot, object, rights, MAX_EXTRA_CAPS};
use sap_rt::cnode::SlotRef;
use sap_rt::ipc::Give as Send;
use sap_rt::sys::time;
use sap_rt::{account, debug, println, slots, tcb, untyped};

use crate::pack;

/// Badges on instances' capabilities, each plus the instance's index: its
/// launcher, its capability to the instance API, and the handle its
/// caller holds.
const INSTANCE_BADGE: usize = 0x300;
const API_BADGE: usize = 0x400;
const HANDLE_BADGE: usize = 0x500;
/// How many instances the supervisor runs at once.
const MAX_INSTANCES: usize = 16;
/// Slots in init's CNode for each instance's objects, while the loader
/// builds it.
const OBJECT_SLOTS: usize = 256;
/// How many times in a row an instance may fail within a second of
/// starting before the supervisor stops restarting it.
const MAX_RAPID: u32 = 3;
/// The groups a supervisor may hold: names standing for a set of
/// capabilities whose size depends on the machine. `ram`: the RAM untyped
/// regions, for the memory manager.
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
    /// An untyped region carved from the pool.
    Region(usize),
    /// A memory account: the capabilities of its holder and controller,
    /// and the chunk the instance is built from.
    Account { holder: usize, controller: usize, chunk: usize },
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
    /// The originals of its launcher and fault handler, and of its
    /// capability to the instance API.
    launcher: usize,
    api: usize,
    on_end: OnEnd,
    /// When its process started, and how many times in a row it failed
    /// within a second of starting.
    started: u64,
    rapid: u32,
    /// For an instance started through the instance API.
    dynamic: Option<api::Dynamic>,
}

impl Instance {
    /// Whether it ended and only waits to be waited for.
    fn ended(&self) -> bool {
        self.dynamic.as_ref().is_some_and(|d| d.ended.is_some())
    }
}

/// An endpoint a component serves; `endpoint` 0 once it is gone.
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
    /// init's endpoint, which all the badged capabilities name.
    endpoint: usize,
    /// Where received capabilities arrive: `MAX_EXTRA_CAPS` slots.
    pub window: usize,
    parent: Parent,
    /// The untyped region regions are carved from.
    pool: usize,
    /// The holder's capability to the supervisor's account, once the
    /// memory manager runs, and the controller's.
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
            next_badge: launcher::ROUTE_BADGES,
            instances: (0..MAX_INSTANCES).map(|_| None).collect(),
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
        index < MAX_INSTANCES && self.instances[index].as_ref().is_some_and(|i| !i.ended())
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
                let endpoint = new_slot();
                let root = SlotRef { root: init_slot::CNODE, index: 0, depth: 0 };
                untyped::retype(self.parent.untyped, object::ENDPOINT, 0, root, endpoint, 1).expect("init: an export's endpoint");
                self.exports.push(Export { component: owner.into(), name: name.into(), endpoint });
            }
        }
        // The root account, from the memory manager's export: the manager
        // knows its capabilities by these badges.
        if composition.has_accounts() {
            let accounts = composition.get_accounts().map_err(bad)?;
            let export = self.export(text(accounts.get_component())?, text(accounts.get_name())?).ok_or(StartError::BadComponent)?;
            let (holder, controller) = (new_slot(), new_slot());
            let endpoint = slot(self.exports[export].endpoint);
            slot(holder).mint_from(endpoint, CLIENT_RIGHTS, ROOT_HOLDER as usize).expect("init: badge");
            slot(controller).mint_from(endpoint, rights::WRITE, ROOT_CONTROLLER as usize).expect("init: badge");
            self.account = Some((holder, controller));
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
        Ok(Memory::Account { holder: caps, controller: caps + 1, chunk: new_slot() })
    }

    /// The memory an instance with `limits` is built from: a region, or an
    /// account of the supervisor's (whose chunk `launch` takes).
    fn memory(&self, limits: limits::Reader) -> Result<Memory, StartError> {
        match limits.get_memory().which().map_err(|_| StartError::BadComponent)? {
            limits::memory::Region(_) => {
                let region = region_terms(limits)?;
                let memory = new_slot();
                let root = SlotRef { root: init_slot::CNODE, index: 0, depth: 0 };
                if untyped::retype(self.pool, object::UNTYPED, region.get_bits() as usize, root, memory, 1).is_err() {
                    slots::free(memory);
                    return Err(StartError::OverLimit);
                }
                Ok(Memory::Region(memory))
            }
            limits::memory::Account(_) => Self::new_account(self.account.ok_or(StartError::BadComponent)?.0, limits),
        }
    }

    /// Destroy everything made from `memory`, and give it back.
    fn end_memory(memory: &Memory) {
        match *memory {
            Memory::Region(region) => drop_original(region),
            Memory::Account { holder, controller, chunk } => {
                if let Err(error) = account::destroy(controller) {
                    println!("init: ending an account failed: {error:?}");
                }
                for cap in [holder, controller, chunk] {
                    drop_slot(cap);
                }
            }
        }
    }

    /// Make an instance at `index` of `component`, named `name`, in
    /// `memory`, and start it.
    fn create(&mut self, index: usize, name: String, component: component::Reader, memory: Memory, dynamic: Option<api::Dynamic>) -> Result<(), StartError> {
        let on_end = component.get_on_end().map_err(|_| StartError::BadComponent)?;
        let (launcher_cap, api) = (new_slot(), new_slot());
        let ep = slot(self.endpoint);
        slot(launcher_cap).mint_from(ep, rights::WRITE | rights::GRANT, INSTANCE_BADGE + index).expect("init: badge");
        slot(api).mint_from(ep, CLIENT_RIGHTS, API_BADGE + index).expect("init: badge");
        let objects = slots::alloc_range(OBJECT_SLOTS).expect("init: slots for an instance");
        let kept = match keep(component) {
            Ok(kept) => kept,
            Err(error) => {
                for s in [launcher_cap, api] {
                    drop_original(s);
                }
                slots::free_range(objects, OBJECT_SLOTS);
                Self::end_memory(&memory);
                if let Some(dynamic) = &dynamic {
                    for export in self.exports.iter_mut().filter(|e| e.component == name) {
                        export.endpoint = 0;
                    }
                    api::release(dynamic);
                }
                return Err(error);
            }
        };
        let instance = Instance { name, component: kept, memory, objects, launcher: launcher_cap, api, on_end, started: time(), rapid: 0, dynamic };
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
    /// memory, with its launcher.
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
                route::Export(to) => {
                    let to = to.map_err(bad)?;
                    Source::Client(self.export(text(to.get_component())?, text(to.get_name())?).ok_or(StartError::BadComponent)?)
                }
                route::Serve(export) => Source::Copy(self.exports[self.export(name, text(export)?).ok_or(StartError::BadComponent)?].endpoint),
                route::Passed(i) => {
                    let passed = instance.dynamic.as_ref().map_or(&[][..], |d| &d.passed[..]);
                    Source::Copy(*passed.get(i as usize).ok_or(StartError::BadComponent)?)
                }
                route::Supervisor(()) => Source::Copy(instance.api),
            });
        }

        // What the process is built from: the region, or a chunk of the
        // account.
        let (untyped, budget_bits, account) = match instance.memory {
            Memory::Region(region) => (region, region_terms(limits)?.get_budget_bits(), None),
            Memory::Account { holder, chunk, .. } => {
                let bits = account_terms(limits)?.get_process_bits();
                account::allocate(holder, bits, init_slot::CNODE, chunk).map_err(|_| StartError::OverLimit)?;
                (chunk, 0, Some(holder))
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
            budget_bits: (budget_bits > 0).then_some(budget_bits as usize),
            launcher: Some(instance.launcher),
            account,
            imports: &imports,
            bootstrap,
        };
        let parent = Parent { untyped, ..self.parent };
        let mut slots = Slots { next: instance.objects, end: instance.objects + OBJECT_SLOTS };
        let started = loader::load(image, &parent, &mut slots, &launch).and_then(|p| {
            tcb::set_fault_handler(p.tcb, Some(instance.launcher))?;
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
            _ => println!("init: started {} ({} KiB image, in a region of the pool)", name, image.len() / 1024),
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
    /// limit), its launcher and its exports, for a new process.
    fn reset(&mut self, instance: &Instance) -> Result<(), StartError> {
        match instance.memory {
            Memory::Region(region) => {
                let _ = slot(region).revoke();
            }
            Memory::Account { controller, .. } => {
                account::revoke(controller).map_err(|_| StartError::OverLimit)?;
                let limit = with_component(&instance.component, |component| {
                    Ok(account_terms(component.get_limits().map_err(|_| StartError::BadComponent)?)?.get_limit())
                })?;
                account::set_limit(controller, limit).map_err(|_| StartError::OverLimit)?;
            }
        }
        for original in [instance.launcher, instance.api] {
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
            self.finish(index, end, true);
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
            self.finish(index, end, true);
        }
    }

    /// Stop every instance instance `index` started through the API, and
    /// forget those that ended.
    fn stop_started_by(&mut self, index: usize) {
        for child in 0..MAX_INSTANCES {
            let started_by = self.instances[child].as_ref().and_then(|i| i.dynamic.as_ref()).is_some_and(|d| d.caller == index);
            if child != index && started_by {
                self.finish(child, End::Stopped, false);
            }
        }
    }

    /// Instance `index` ended for good, as `end`: stop what it started,
    /// destroy it, and answer its waiter, or keep the end for one if `keep`
    /// (not when its caller is gone).
    fn finish(&mut self, index: usize, end: End, keep: bool) {
        self.stop_started_by(index);
        let Some(instance) = self.instances[index].take() else { return };
        let dynamic = if instance.ended() { instance.dynamic } else { self.destroy(instance) };
        let Some(mut dynamic) = dynamic else { return };
        let end = dynamic.ended.unwrap_or(end);
        if let Some(waiter) = dynamic.waiter.take() {
            api::answer(waiter, end);
            self.spare_replies.push(waiter);
        } else if keep {
            dynamic.ended = Some(end);
            self.instances[index] = Some(api::ended(dynamic));
            return;
        }
        drop_original(dynamic.handle);
    }

    /// Destroy an instance: revoking its memory deletes every object made
    /// from it, then the memory itself goes back; with it go its originals
    /// and, for one started through the API, what that call gave it, whose
    /// handle and waiter are left for the caller.
    fn destroy(&mut self, instance: Instance) -> Option<api::Dynamic> {
        Self::end_memory(&instance.memory);
        for original in [instance.launcher, instance.api] {
            drop_original(original);
        }
        slots::free_range(instance.objects, OBJECT_SLOTS);
        self.clients.retain(|c| c.component != instance.name);
        if let Some(dynamic) = &instance.dynamic {
            // Its exports go: their endpoints were made from the caller's
            // chunk, and their clients' capabilities with them.
            for export in self.exports.iter_mut().filter(|e| e.component == instance.name) {
                export.endpoint = 0;
            }
            api::release(dynamic);
        }
        instance.dynamic
    }

    /// Handle a message whose badge is one of the supervisor's, if it is:
    /// from an instance's launcher capability (an exit or a fault ends the
    /// instance, and restarts it if it failed and its component says so;
    /// `CLIENT` asks about a client of its exports), on the instance API, or
    /// on a handle.
    pub fn handle(&mut self, badge: usize, label: usize, words: &[usize], caps: usize, reply: usize) -> Option<Answer> {
        let index = badge & 0xff;
        if index >= MAX_INSTANCES {
            return None;
        }
        match badge & !0xff {
            INSTANCE_BADGE if self.running(index) => {}
            API_BADGE if self.running(index) => return Some(self.serve_api(index, label, words, caps)),
            HANDLE_BADGE if self.instances[index].as_ref().is_some_and(|i| i.dynamic.is_some()) => return Some(self.serve_handle(index, label, reply)),
            _ => return None,
        }
        let (success, end) = match label {
            launcher::CLIENT => return Some(self.client(index, words.first().copied().unwrap_or(0))),
            launcher::EXIT => {
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
        match self.instances[index].as_ref().unwrap().on_end {
            OnEnd::Restart if !success => self.restart(index, end),
            OnEnd::Forget | OnEnd::Restart => self.finish(index, end, true),
            OnEnd::EndBoot => {
                self.finish(index, end, true);
                debug::halt(!success)
            }
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

fn region_terms(limits: limits::Reader<'_>) -> Result<sap_interfaces::supervisor_capnp::region::Reader<'_>, StartError> {
    match limits.get_memory().which() {
        Ok(limits::memory::Region(Ok(region))) => Ok(region),
        _ => Err(StartError::BadComponent),
    }
}

fn account_terms(limits: limits::Reader<'_>) -> Result<sap_interfaces::supervisor_capnp::account::Reader<'_>, StartError> {
    match limits.get_memory().which() {
        Ok(limits::memory::Account(Ok(terms))) => Ok(terms),
        _ => Err(StartError::BadComponent),
    }
}
