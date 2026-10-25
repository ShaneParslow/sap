//! The process factory (`sap_abi::factory`): starting programs from the
//! boot pack on a client's behalf, with memory the client provides, and
//! reporting how they end.
//!
//! Each process the factory starts has three capabilities to init's
//! endpoint, told apart by badge: the factory capability its client called
//! (`FACTORY_BADGE`), its launcher and fault handler (`LAUNCHER_BADGE` + its
//! index) and the handle its client holds (`HANDLE_BADGE` + its index).
//! Every object of the process is retyped from the client's untyped memory,
//! so revoking that memory destroys the process; the memory goes back to
//! the client with the answer to `WAIT`.

use alloc::vec::Vec;

use sap_loader::{self as loader, Give, Launch, Parent, Slots, LAUNCH};
use sap_rt::abi::{factory, fault, init_slot, launcher, object, rights, MAX_EXTRA_CAPS};
use sap_rt::cnode::SlotRef;
use sap_rt::ipc::{self, Give as Send};
use sap_rt::{slots, tcb, untyped};

use crate::pack;

/// The badge on factory capabilities.
pub const FACTORY_BADGE: usize = 2;
/// Badges on a process's launcher capability and on its handle: these
/// plus its index in the table.
const LAUNCHER_BADGE: usize = 0x100;
const HANDLE_BADGE: usize = 0x200;
/// How many processes the factory runs at once.
const MAX_PROCESSES: usize = 16;
/// Slots for each process's objects in init's CNode.
const OBJECT_SLOTS: usize = 128;

fn slot(i: usize) -> SlotRef {
    SlotRef::new(init_slot::CNODE, i)
}

fn new_slot() -> usize {
    slots::alloc().expect("init: out of slots")
}

#[derive(Clone, Copy)]
enum State {
    Running,
    Ended { how: usize, code: usize },
}

/// A process the factory started. Its slots are in init's CNode.
struct Process {
    /// The client's untyped memory, which every object below came from.
    memory: usize,
    /// `OBJECT_SLOTS` slots for the loader, from this one.
    objects: usize,
    /// The badged endpoint capabilities: the process's launcher, and the
    /// client's handle (copies of these are what they hold).
    launcher: usize,
    handle: usize,
    state: State,
    /// The reply object of a client waiting for the process to end.
    waiter: Option<usize>,
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

pub struct Factory {
    /// init's endpoint, which all the badged capabilities name.
    endpoint: usize,
    /// Where received capabilities arrive: `MAX_EXTRA_CAPS` slots.
    pub window: usize,
    parent: Parent,
    processes: [Option<Process>; MAX_PROCESSES],
    /// Reply objects not in use, for when one is kept by a waiter.
    spare_replies: Vec<usize>,
    /// Untyped memory for init's own objects (reply objects).
    own: usize,
}

impl Factory {
    pub fn new(endpoint: usize, own: usize, parent: Parent) -> Self {
        let window = slots::alloc_range(MAX_EXTRA_CAPS).expect("init: slots for the receive window");
        Self { endpoint, window, parent, processes: [const { None }; MAX_PROCESSES], spare_replies: Vec::new(), own }
    }

    /// A reply object for the next receive, after the last one was kept.
    pub fn fresh_reply(&mut self) -> usize {
        self.spare_replies.pop().unwrap_or_else(|| {
            let at = new_slot();
            let root = SlotRef { root: init_slot::CNODE, index: 0, depth: 0 };
            untyped::retype(self.own, object::REPLY, 0, root, at, 1).expect("init: reply object");
            at
        })
    }

    /// Empty the receive window of anything a request left there.
    pub fn clear_window(&self) {
        for i in 0..MAX_EXTRA_CAPS {
            let _ = slot(self.window + i).delete();
        }
    }

    /// Handle a message whose badge is one of the factory's, if it is.
    pub fn handle(&mut self, badge: usize, label: usize, words: &[usize], caps: usize, reply: usize) -> Option<Answer> {
        let index = badge & 0xff;
        Some(match badge & !0xff {
            0 if badge == FACTORY_BADGE => match label {
                factory::SPAWN => self.spawn(words, caps),
                _ => Answer::Reply(factory::BAD_REQUEST, Vec::new(), Vec::new()),
            },
            LAUNCHER_BADGE if self.running(index) => match label {
                launcher::EXIT => self.end(index, factory::EXITED, words.first().copied().unwrap_or(0)),
                fault::VM | fault::USER_EXCEPTION => self.end(index, factory::FAULTED, label),
                _ => Answer::Reply(factory::BAD_REQUEST, Vec::new(), Vec::new()),
            },
            HANDLE_BADGE if index < MAX_PROCESSES && self.processes[index].is_some() => match label {
                factory::WAIT => self.wait(index, reply),
                factory::KILL => {
                    if self.running(index) {
                        self.end(index, factory::KILLED, 0);
                    }
                    Answer::Reply(factory::OK, Vec::new(), Vec::new())
                }
                _ => Answer::Reply(factory::BAD_REQUEST, Vec::new(), Vec::new()),
            },
            _ => return None,
        })
    }

    fn running(&self, index: usize) -> bool {
        index < MAX_PROCESSES && matches!(self.processes[index], Some(Process { state: State::Running, .. }))
    }

    /// `SPAWN`: check the request, load the program from the client's
    /// memory and start it.
    fn spawn(&mut self, words: &[usize], caps: usize) -> Answer {
        let refuse = |label| Answer::Reply(label, Vec::new(), Vec::new());
        if caps == 0 {
            return refuse(factory::BAD_REQUEST);
        }
        // From here on, every refusal gives the memory back.
        let memory = self.window;
        let give_back = |label| Answer::Reply(label, Vec::new(), alloc::vec![Send::moved(memory)]);
        if slot(memory).identify().map(|id| id.object_type) != Ok(object::UNTYPED) {
            return give_back(factory::BAD_REQUEST);
        }
        let Some(bootstrap) = bootstrap(words) else {
            return give_back(factory::BAD_REQUEST);
        };
        let mut hash = [0u8; 32];
        for (i, w) in words[..factory::HASH_WORDS].iter().enumerate() {
            hash[i * 8..i * 8 + 8].copy_from_slice(&(*w as u64).to_le_bytes());
        }
        let Some(image) = pack::find(&pack::Hash(hash)) else {
            return give_back(factory::NO_SUCH_PROGRAM);
        };
        let Some(index) = self.processes.iter().position(Option::is_none) else {
            return give_back(factory::FULL);
        };

        // The memory and the imports move out of the window: the memory
        // into a slot of its own, the imports into the new process.
        let kept = new_slot();
        slot(kept).move_from(slot(memory)).expect("init: keep the memory");
        let objects = slots::alloc_range(OBJECT_SLOTS).expect("init: slots for a process");
        let (launcher_cap, handle) = (new_slot(), new_slot());
        let ep = slot(self.endpoint);
        slot(launcher_cap).mint_from(ep, rights::WRITE | rights::GRANT, LAUNCHER_BADGE + index).expect("init: launcher badge");
        slot(handle).mint_from(ep, rights::WRITE | rights::GRANT_REPLY, HANDLE_BADGE + index).expect("init: handle badge");
        let imports: Vec<(usize, Give)> = (1..caps).map(|i| (self.window + i, Give::Move)).collect();
        let parent = Parent { untyped: kept, ..self.parent };
        let mut slots = Slots { next: objects, end: objects + OBJECT_SLOTS };
        // TODO: the client's scheduling authority should decide the
        // priority, once there are scheduling contexts.
        let launch = Launch { priority: 255, launcher: Some(launcher_cap), imports: &imports, bootstrap: &bootstrap, ..LAUNCH };
        let started = loader::load(image, &parent, &mut slots, &launch).and_then(|p| {
            tcb::set_fault_handler(p.tcb, Some(launcher_cap))?;
            Ok(p.start()?)
        });
        match started {
            Ok(()) => {
                self.processes[index] =
                    Some(Process { memory: kept, objects, launcher: launcher_cap, handle, state: State::Running, waiter: None });
                Answer::Reply(factory::OK, Vec::new(), alloc::vec![Send::copy(handle)])
            }
            Err(_) => {
                // Remove what was built, and give the memory back.
                let _ = slot(kept).revoke();
                for cap in [launcher_cap, handle] {
                    let _ = slot(cap).revoke();
                    let _ = slot(cap).delete();
                    slots::free(cap);
                }
                slots::free_range(objects, OBJECT_SLOTS);
                slot(memory).move_from(slot(kept)).expect("init: return the memory");
                slots::free(kept);
                give_back(factory::LOAD_FAILED)
            }
        }
    }

    /// The process at `index` ended: destroy it, and answer a waiter.
    fn end(&mut self, index: usize, how: usize, code: usize) -> Answer {
        let process = self.processes[index].as_mut().expect("init: no such process");
        // Revoking the memory destroys every object made from it: the
        // thread, its address space and pages, and its CNode with the
        // capabilities in it.
        let _ = slot(process.memory).revoke();
        let _ = slot(process.launcher).revoke();
        let _ = slot(process.launcher).delete();
        slots::free_range(process.objects, OBJECT_SLOTS);
        process.state = State::Ended { how, code };
        if let Some(waiter) = process.waiter.take() {
            self.answer_waiter(index, waiter);
            self.spare_replies.push(waiter);
        }
        Answer::Nothing
    }

    /// `WAIT`: answer now if the process has ended, else keep the reply
    /// object until it does.
    fn wait(&mut self, index: usize, reply: usize) -> Answer {
        let process = self.processes[index].as_mut().expect("init: no such process");
        match process.state {
            _ if process.waiter.is_some() => Answer::Reply(factory::BAD_REQUEST, Vec::new(), Vec::new()),
            State::Running => {
                process.waiter = Some(reply);
                Answer::Later
            }
            State::Ended { .. } => {
                self.answer_waiter(index, reply);
                Answer::Nothing
            }
        }
    }

    /// Tell the waiter on `reply` how the process ended, give back its
    /// memory, and forget the process.
    fn answer_waiter(&mut self, index: usize, reply: usize) {
        let process = self.processes[index].take().expect("init: no such process");
        let State::Ended { how, code } = process.state else { unreachable!("init: answering for a running process") };
        let _ = ipc::reply(reply, factory::OK, &[how, code], &[Send::moved(process.memory)]);
        // The handle stops working: revoking init's original deletes the
        // client's copies.
        let _ = slot(process.handle).revoke();
        let _ = slot(process.handle).delete();
        // The memory moved to the waiter; if the reply couldn't carry it,
        // it is dropped.
        let _ = slot(process.memory).delete();
        for s in [process.handle, process.launcher, process.memory] {
            slots::free(s);
        }
    }
}

/// The bootstrap message of a `SPAWN` request, unpacked from its words.
fn bootstrap(words: &[usize]) -> Option<Vec<u8>> {
    let len = *words.get(factory::HASH_WORDS)?;
    let packed = &words[factory::HASH_WORDS + 1..];
    if len > factory::MAX_BOOTSTRAP || len > packed.len() * 8 {
        return None;
    }
    Some(packed.iter().flat_map(|w| (*w as u64).to_le_bytes()).take(len).collect())
}
