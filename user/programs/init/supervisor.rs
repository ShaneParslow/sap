//! The supervisor: starts the components of a composition
//! (`interfaces/supervisor.capnp`) and enforces their limits. It holds a
//! table of named capabilities that compositions route from, and a region
//! of untyped memory it carves each instance's memory from; ending an
//! instance is one revoke of that memory.
//!
//! Each instance's launcher and fault handler is a capability to init's
//! endpoint with the badge `INSTANCE_BADGE` + its index.
//! TODO: exports the supervisor owns, routes between components, lazy
//! start and restart (design/processes.md in sap-docs).

use alloc::string::String;
use alloc::vec::Vec;

use sap_interfaces::supervisor_capnp::{component, composition, OnEnd};
use sap_loader::{self as loader, Give, Launch, Parent, Slots, LAUNCH};
use sap_process::launcher;
use sap_rt::abi::{fault, init_slot, object, rights};
use sap_rt::cnode::SlotRef;
use sap_rt::{debug, println, slots, tcb, untyped};

use crate::pack;

/// Badges on instances' launcher capabilities: this plus the index.
pub const INSTANCE_BADGE: usize = 0x300;
/// How many instances the supervisor runs at once.
const MAX_INSTANCES: usize = 8;
/// Slots in init's CNode for each instance's objects, while the loader
/// builds it.
const OBJECT_SLOTS: usize = 256;

fn slot(i: usize) -> SlotRef {
    SlotRef::new(init_slot::CNODE, i)
}

/// Why a component couldn't be started.
#[derive(Debug)]
pub enum StartError {
    /// The composition is malformed, or names a capability the supervisor
    /// doesn't hold.
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

pub struct Supervisor {
    /// init's endpoint, which the instances' launcher capabilities name.
    endpoint: usize,
    parent: Parent,
    /// The untyped region instances' memory is carved from.
    pool: usize,
    held: Vec<(String, usize)>,
    instances: [Option<Instance>; MAX_INSTANCES],
}

impl Supervisor {
    pub fn new(endpoint: usize, pool: usize, parent: Parent) -> Self {
        Self { endpoint, parent, pool, held: Vec::new(), instances: [const { None }; MAX_INSTANCES] }
    }

    /// Hold `cap` under `name`, for compositions to route from.
    pub fn hold(&mut self, name: impl Into<String>, cap: usize) {
        self.held.push((name.into(), cap));
    }

    fn held(&self, name: &str) -> Option<usize> {
        self.held.iter().find(|(n, _)| n == name).map(|&(_, cap)| cap)
    }

    /// Start every component of `composition`, in order.
    pub fn enact(&mut self, composition: composition::Reader) -> Result<(), StartError> {
        for component in composition.get_components().map_err(|_| StartError::BadComponent)? {
            self.start(component)?;
        }
        Ok(())
    }

    fn start(&mut self, component: component::Reader) -> Result<(), StartError> {
        let bad = |_| StartError::BadComponent;
        let name: String = component.get_name().map_err(bad)?.to_str().map_err(|_| StartError::BadComponent)?.into();
        let hash: [u8; 32] = component.get_program().map_err(bad)?.try_into().map_err(|_| StartError::BadComponent)?;
        let image = pack::find(&pack::Hash(hash)).ok_or(StartError::NoSuchProgram)?;
        let limits = component.get_limits().map_err(bad)?;
        if limits.get_max_priority() < limits.get_priority() {
            return Err(StartError::OverLimit);
        }
        let on_end = component.get_on_end().map_err(|_| StartError::BadComponent)?;
        let bootstrap = component.get_bootstrap().map_err(bad)?;
        let (mut imports, mut moved) = (Vec::new(), Vec::new());
        for route in component.get_imports().map_err(bad)? {
            let held = route.get_held().map_err(bad)?.to_str().map_err(|_| StartError::BadComponent)?;
            let cap = self.held(held).ok_or(StartError::BadComponent)?;
            imports.push((cap, if route.get_move() { Give::Move } else { Give::Copy(rights::ALL) }));
            if route.get_move() {
                moved.push(String::from(held));
            }
        }
        let index = self.instances.iter().position(Option::is_none).ok_or(StartError::Full)?;

        // The instance's memory, carved from the pool.
        let memory = slots::alloc().expect("init: out of slots");
        let root = SlotRef { root: init_slot::CNODE, index: 0, depth: 0 };
        if untyped::retype(self.pool, object::UNTYPED, limits.get_memory_bits() as usize, root, memory, 1).is_err() {
            slots::free(memory);
            return Err(StartError::OverLimit);
        }
        let launcher_cap = slots::alloc().expect("init: out of slots");
        slot(launcher_cap).mint_from(slot(self.endpoint), rights::WRITE | rights::GRANT, INSTANCE_BADGE + index).expect("init: badge");
        let objects = slots::alloc_range(OBJECT_SLOTS).expect("init: slots for an instance");
        let launch = Launch {
            priority: limits.get_priority() as usize,
            max_priority: limits.get_max_priority() as usize,
            cnode_radix: limits.get_cnode_bits() as usize,
            stack_pages: limits.get_stack_pages() as usize,
            launcher: Some(launcher_cap),
            imports: &imports,
            bootstrap,
            ..LAUNCH
        };
        let parent = Parent { untyped: memory, ..self.parent };
        let mut slots = Slots { next: objects, end: objects + OBJECT_SLOTS };
        let started = loader::load(image, &parent, &mut slots, &launch).and_then(|p| {
            tcb::set_fault_handler(p.tcb, Some(launcher_cap))?;
            Ok(p.start()?)
        });
        // Moved capabilities are gone from the table either way.
        self.held.retain(|(n, _)| !moved.contains(n));
        let instance = Instance { name, memory, objects, launcher: launcher_cap, on_end };
        if started.is_err() {
            self.destroy(&instance);
            return Err(StartError::LoadFailed);
        }
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
    /// is one: an exit or a fault ends the instance.
    pub fn handle(&mut self, badge: usize, label: usize, words: &[usize]) -> bool {
        let index = badge.wrapping_sub(INSTANCE_BADGE);
        if index >= MAX_INSTANCES || self.instances[index].is_none() {
            return false;
        }
        let success = match label {
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
                return true;
            }
        };
        let instance = self.instances[index].take().unwrap();
        self.destroy(&instance);
        match instance.on_end {
            OnEnd::Forget => {}
            OnEnd::EndBoot => debug::halt(!success),
        }
        true
    }
}
