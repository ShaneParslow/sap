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
//! Each instance's launcher and fault handler is a capability to init's
//! endpoint with the badge `INSTANCE_BADGE` + its index.
//!
//! An instance whose component says `restart` is started again when it
//! fails, in the same memory, emptied, and with the same exports, so its
//! clients' capabilities keep working: a call it died in gets no reply
//! (`NO_REPLY`), and calls after wait for the new process.
//! TODO: lazy start (design/processes.md in sap-docs).

use alloc::string::String;
use alloc::vec::Vec;

use alloc::vec;

use capnp::message::{Builder, SingleSegmentAllocator};

use sap_interfaces::memory_capnp::{ROOT_CONTROLLER, ROOT_HOLDER};
use sap_interfaces::supervisor_capnp::{component, composition, limits, route, OnEnd};
use sap_loader::{self as loader, Give, Launch, Parent, Slots};
use sap_process::launcher;
use sap_rt::abi::{fault, init_slot, object, rights};
use sap_rt::cnode::SlotRef;
use sap_interfaces::words;
use sap_rt::sys::time;
use sap_rt::{account, debug, println, slots, tcb, untyped};

use crate::factory::Answer;
use crate::pack;

/// Badges on instances' launcher capabilities: this plus the index.
pub const INSTANCE_BADGE: usize = 0x300;
/// How many instances the supervisor runs at once.
const MAX_INSTANCES: usize = 8;
/// Slots in init's CNode for each instance's objects, while the loader
/// builds it.
const OBJECT_SLOTS: usize = 256;
/// How many times in a row an instance may fail within a second of
/// starting before the supervisor stops restarting it.
const MAX_RAPID: u32 = 3;
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

/// Where an instance's objects come from.
enum Memory {
    /// An untyped region carved from the pool.
    Region(usize),
    /// A memory account: the capabilities of its holder and controller,
    /// and the chunk the instance is built from.
    Account { holder: usize, controller: usize, chunk: usize },
}

struct Instance {
    name: String,
    /// The component, kept to start it again: a single-segment message.
    component: Vec<usize>,
    memory: Memory,
    objects: usize,
    /// The badged endpoint capability that is its launcher and fault
    /// handler.
    launcher: usize,
    on_end: OnEnd,
    /// When its process started, and how many times in a row it failed
    /// within a second of starting.
    started: u64,
    rapid: u32,
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
    /// The untyped region regions are carved from.
    pool: usize,
    /// The holder's capability to the supervisor's account, once the
    /// memory manager runs, and the controller's.
    account: Option<(usize, usize)>,
    held: Vec<(String, usize)>,
    exports: Vec<Export>,
    clients: Vec<Client>,
    /// The badge the next client gets: from `ROUTE_BADGES` up, so neither
    /// unbadged calls nor capabilities a server mints are anybody's route.
    next_badge: usize,
    instances: [Option<Instance>; MAX_INSTANCES],
}

impl Supervisor {
    pub fn new(endpoint: usize, pool: usize, parent: Parent) -> Self {
        Self {
            endpoint,
            parent,
            pool,
            account: None,
            held: Vec::new(),
            exports: Vec::new(),
            clients: Vec::new(),
            next_badge: launcher::ROUTE_BADGES,
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
        // The root account, from the memory manager's export: the manager
        // knows its capabilities by these badges.
        if composition.has_accounts() {
            let accounts = composition.get_accounts().map_err(bad)?;
            let export = self.export(text(accounts.get_component())?, text(accounts.get_name())?).ok_or(StartError::BadComponent)?;
            let (holder, controller) = (slots::alloc().expect("init: out of slots"), slots::alloc().expect("init: out of slots"));
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

    /// The memory an instance with `limits` is built from: a region, or an
    /// account (whose chunk `launch` takes).
    fn memory(&self, limits: limits::Reader) -> Result<Memory, StartError> {
        let bad = |_| StartError::BadComponent;
        let root = SlotRef { root: init_slot::CNODE, index: 0, depth: 0 };
        match limits.get_memory().which().map_err(|_| StartError::BadComponent)? {
            limits::memory::Region(region) => {
                let region = region.map_err(bad)?;
                let memory = slots::alloc().expect("init: out of slots");
                if untyped::retype(self.pool, object::UNTYPED, region.get_bits() as usize, root, memory, 1).is_err() {
                    slots::free(memory);
                    return Err(StartError::OverLimit);
                }
                Ok(Memory::Region(memory))
            }
            limits::memory::Account(terms) => {
                let terms = terms.map_err(bad)?;
                let (parent, _) = self.account.ok_or(StartError::BadComponent)?;
                let caps = slots::alloc_range(2).expect("init: out of slots");
                if account::child(parent, terms.get_reservation(), terms.get_limit(), init_slot::CNODE, caps).is_err() {
                    slots::free_range(caps, 2);
                    return Err(StartError::OverLimit);
                }
                Ok(Memory::Account { holder: caps, controller: caps + 1, chunk: slots::alloc().expect("init: out of slots") })
            }
        }
    }

    /// Destroy everything made from `memory`, and give it back.
    fn end_memory(memory: &Memory) {
        match *memory {
            Memory::Region(region) => {
                let _ = slot(region).revoke();
                let _ = slot(region).delete();
                slots::free(region);
            }
            Memory::Account { holder, controller, chunk } => {
                // TODO: keep the account, with its reservation, for a
                // restart (M7).
                if let Err(error) = account::destroy(controller) {
                    println!("init: ending an account failed: {error:?}");
                }
                for cap in [holder, controller, chunk] {
                    let _ = slot(cap).delete();
                    slots::free(cap);
                }
            }
        }
    }

    fn start(&mut self, component: component::Reader) -> Result<(), StartError> {
        let bad = |_| StartError::BadComponent;
        let name: String = text(component.get_name())?.into();
        let on_end = component.get_on_end().map_err(|_| StartError::BadComponent)?;
        if on_end == OnEnd::Restart {
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
        let launcher_cap = slots::alloc().expect("init: out of slots");
        slot(launcher_cap).mint_from(slot(self.endpoint), rights::WRITE | rights::GRANT, INSTANCE_BADGE + index).expect("init: badge");
        let objects = slots::alloc_range(OBJECT_SLOTS).expect("init: slots for an instance");
        let instance = Instance { name, component: keep(component)?, memory, objects, launcher: launcher_cap, on_end, started: time(), rapid: 0 };
        if let Err(error) = self.launch(component, &instance) {
            self.destroy(&instance);
            return Err(error);
        }
        self.instances[index] = Some(instance);
        Ok(())
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
                route::Serve(export) => Source::Serve(self.export(name, text(export)?).ok_or(StartError::BadComponent)?),
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
                Source::Serve(export) => (self.exports[export].endpoint, Give::Copy(rights::ALL)),
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
            let _ = slot(cap).delete();
            slots::free(cap);
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

    /// Destroy everything instance `instance` was built from, keeping its
    /// memory (empty, with the account's reservation and the composition's
    /// limit) and its launcher, for a new process.
    fn reset(&mut self, instance: &Instance) -> Result<(), StartError> {
        match instance.memory {
            Memory::Region(region) => {
                let _ = slot(region).revoke();
            }
            Memory::Account { controller, .. } => {
                account::revoke(controller).map_err(|_| StartError::OverLimit)?;
                let limit = with_component(&instance.component, |component| {
                    let limits = component.get_limits().map_err(|_| StartError::BadComponent)?;
                    Ok(account_terms(limits)?.get_limit())
                })?;
                account::set_limit(controller, limit).map_err(|_| StartError::OverLimit)?;
            }
        }
        let _ = slot(instance.launcher).revoke();
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
    fn restart(&mut self, index: usize, mut instance: Instance) {
        let now = time();
        instance.rapid = if now - instance.started < self.parent.timer_hz { instance.rapid + 1 } else { 0 };
        if instance.rapid > MAX_RAPID {
            println!("init: {} keeps failing; not restarting it", instance.name);
            self.destroy(&instance);
            return;
        }
        println!("init: restarting {}", instance.name);
        let result = self.reset(&instance).and_then(|()| {
            let words = core::mem::take(&mut instance.component);
            let result = with_component(&words, |component| self.launch(component, &instance));
            instance.component = words;
            result
        });
        match result {
            Ok(()) => {
                instance.started = now;
                self.instances[index] = Some(instance);
            }
            Err(error) => {
                println!("init: restarting {} failed: {error:?}", instance.name);
                self.destroy(&instance);
            }
        }
    }

    /// Destroy an instance: revoking its memory deletes every object made
    /// from it, then the memory itself goes back.
    fn destroy(&mut self, instance: &Instance) {
        Self::end_memory(&instance.memory);
        let _ = slot(instance.launcher).revoke();
        let _ = slot(instance.launcher).delete();
        slots::free(instance.launcher);
        slots::free_range(instance.objects, OBJECT_SLOTS);
        self.clients.retain(|c| c.component != instance.name);
    }

    /// Handle a message from an instance's launcher capability, if `badge`
    /// is one: an exit or a fault ends the instance, and restarts it if it
    /// failed and its component says so; `CLIENT` asks about a client of
    /// its exports.
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
        match instance.on_end {
            OnEnd::Restart if !success => self.restart(index, instance),
            OnEnd::Forget | OnEnd::Restart => self.destroy(&instance),
            OnEnd::EndBoot => {
                self.destroy(&instance);
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

/// A copy of `component`, as a single-segment message, to read with `read`.
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
