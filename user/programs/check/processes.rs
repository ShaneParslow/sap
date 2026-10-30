//! Processes: a second program in its own address space, the process
//! factory, and the supervisor's exports as the log shows them.

use sap_interfaces::methods::{log, process, process_factory};
use sap_interfaces::process_capnp::process::wait_results;
use sap_interfaces::process_capnp::process_factory::spawn_params;
use sap_interfaces::process_capnp::{end, Status};
use sap_interfaces::words;
use sap_process::{launcher, process_slot};
use sap_rt::abi::{attr, error, fault, object, rights};
use sap_rt::cnode::SlotRef;
use sap_rt::sys::Error;
use sap_rt::{ipc, println, slots, tcb, untyped, vspace};
use capnp::message::{Builder, SingleSegmentAllocator};
use sap_interfaces::peer_capnp::{peer_imports, Mode};
use sap_loader::{self as loader, Give, Launch, LoadError, Parent, Slots, LAUNCH};
use sap_ring::as_bytes_mut;
use crate::{Env, expect, slot, untyped_at_least};

/// The second program, built by this crate's build script.
pub static PEER: &[u8] = include_bytes!(env!("PEER_ELF"));

/// A peer's bootstrap message and its length, built in a buffer of its
/// own: `mode`, then the indices of its
/// imports in the order `connect_peer` and `check_process` give them.
pub fn peer_bootstrap(mode: Mode, data_pages: usize) -> ([u8; 256], usize) {
    peer_bootstrap_with(mode, data_pages, 0)
}

/// A peer's bootstrap message with an exit code, for `Mode::Exit`.
fn peer_bootstrap_with(mode: Mode, data_pages: usize, exit_code: u32) -> ([u8; 256], usize) {
    let mut scratch = [0u64; 32];
    let mut allocator = SingleSegmentAllocator::new(as_bytes_mut(&mut scratch));
    let mut message = Builder::new(&mut allocator);
    let mut imports = message.init_root::<peer_imports::Builder>();
    imports.set_mode(mode);
    imports.set_endpoint(0);
    imports.set_data_pages(data_pages as u32);
    imports.set_notification(1);
    imports.set_doorbell(2);
    imports.set_reply(3);
    imports.set_exit_code(exit_code);
    let segments = message.get_segments_for_output();
    assert_eq!(segments.len(), 1, "a bootstrap message is one segment");
    // Copied out: the allocator zeroes its buffer when the message goes.
    let mut bytes = [0; 256];
    bytes[..segments[0].len()].copy_from_slice(segments[0]);
    (bytes, segments[0].len())
}

/// Load the peer into a process of its own and talk to it: a call across
/// address spaces, and its attempt to read this process's memory, which
/// faults and is reported here.
pub fn check_process(info: &Env) {
    let base = info.empty.start + 300;
    let parent_untyped = untyped_at_least(info, 20);
    let in_root = SlotRef { root: process_slot::CNODE, index: 0, depth: 0 };
    let (ep, reply, badged) = (base, base + 1, base + 2);
    expect("retype endpoint", untyped::retype(parent_untyped, object::ENDPOINT, 0, in_root, ep, 1), Ok(()));
    expect("retype reply", untyped::retype(parent_untyped, object::REPLY, 0, in_root, reply, 1), Ok(()));
    expect("badge", slot(badged).mint_from(slot(ep), rights::WRITE | rights::GRANT, 0x9ee), Ok(()));

    // The scratch region is where this process fills pages; the peer will
    // try to read it.
    let scratch = 0x38_0000_0000;
    let parent = Parent {
        cnode: process_slot::CNODE,
        vspace: process_slot::VSPACE,
        asid_pool: info.asid_pool,
        authority: process_slot::TCB,
        untyped: parent_untyped,
        scratch,
        timer_hz: info.timer_hz,
    };
    let mut slots = Slots { next: base + 3, end: base + 200 };
    let (bootstrap, len) = peer_bootstrap(Mode::Call, 0);
    let imports = [(badged, Give::Copy(rights::WRITE | rights::GRANT))];
    let launch = Launch { priority: 255, budget_bits: Some(16), imports: &imports, bootstrap: &bootstrap[..len], ..LAUNCH };

    // Refused loads: errors, not panics, since a process manager loads on
    // others' behalf.
    let refused = |result: Result<loader::Process, LoadError>| result.err();
    assert_eq!(refused(loader::load(&[0x7f; 64], &parent, &mut slots, &launch)), Some(LoadError::BadImage("not an ELF file")));
    let too_many = [(badged, Give::Copy(rights::READ)); 64];
    let crowded = Launch { imports: &too_many, ..LAUNCH };
    assert_eq!(refused(loader::load(PEER, &parent, &mut slots, &crowded)), Some(LoadError::BadLaunch("too many imports")));
    let small = slots.next;
    slots.next += 1;
    expect("small untyped", untyped::retype(parent_untyped, object::UNTYPED, 14, in_root, small, 1), Ok(()));
    let starved = Parent { untyped: small, ..parent };
    let result = refused(loader::load(PEER, &starved, &mut slots, &launch));
    assert_eq!(result, Some(LoadError::Sys(Error(error::NOT_ENOUGH_MEMORY))), "a load from 16 KiB of untyped memory");
    expect("clean up the partial process", slot(small).revoke(), Ok(()));

    let peer = loader::load(PEER, &parent, &mut slots, &launch).expect("load the peer");
    // The process ABI's slots: its own objects, its budget, its import.
    let theirs = |index| peer.slot(index).identify().map(|id| id.object_type).map_err(|e| e.0);
    for (index, wanted) in [
        (process_slot::TCB, object::TCB),
        (process_slot::CNODE, object::CNODE),
        (process_slot::VSPACE, object::PAGE_TABLE),
        (process_slot::UNTYPED, object::UNTYPED),
        (process_slot::FIRST_IMPORT, object::ENDPOINT),
    ] {
        assert_eq!(theirs(index), Ok(wanted), "the peer's slot {index}");
    }
    assert_eq!(theirs(process_slot::FIRST_IMPORT + 1), Err(error::FAILED_LOOKUP), "the peer has one import");
    expect("fault handler", tcb::set_fault_handler(peer.tcb, Some(badged)), Ok(()));
    // Something mapped at the scratch address, so the peer's read would
    // succeed if it could see this address space.
    expect("scratch page", untyped::retype(parent_untyped, object::FRAME_4K, 0, in_root, slots.next, 1), Ok(()));
    let marker = slots.next;
    expect("map scratch", vspace::map_frame(marker, process_slot::VSPACE, scratch, rights::READ | rights::WRITE, attr::EXECUTE_NEVER), Ok(()));
    peer.start().expect("start the peer");

    let mut words = [0usize; 8];
    let first = ipc::recv(ep, reply, &mut words).expect("receive the peer's call");
    assert_eq!((first.badge, first.info.label(), first.info.length()), (0x9ee, 1, 6), "the peer's call");
    assert_eq!(words[..6], [1, 2, 3, 4, 5, 6], "the peer's words");
    let doubled = [2, 4, 6, 8, 10, 12];
    let second = ipc::reply_recv(ep, reply, 0, &doubled, &[], &mut words).expect("receive the peer's report");
    assert_eq!((second.info.label(), words[0]), (2, 42), "the peer should report the sum of the reply");
    let fault_msg = ipc::reply_recv(ep, reply, 0, &[], &[], &mut words).expect("receive the peer's fault");
    assert_eq!((fault_msg.info.label(), words[fault::ADDRESS]), (fault::VM, scratch), "the peer's read should fault");
    expect("leave it suspended", ipc::reply(reply, 1, &[], &[]), Ok(()));

    expect("unmap scratch", vspace::unmap_frame(marker), Ok(()));
    expect("revoke", slot(parent_untyped).revoke(), Ok(()));
    println!("a second process, calls between address spaces and isolation behave");
}

/// What a `spawn` returned: its status, and the slot the handle (on
/// success) or the memory (on a refusal) came back in.
struct Spawned {
    status: usize,
    back: usize,
}

/// The reply label for `status`.
fn label(status: Status) -> usize {
    status as usize
}

/// Ask the factory to start the program named `hash` with `bootstrap`,
/// built from the untyped memory in `memory` (moved), with `imports`
/// (copied).
fn factory_spawn(info: &Env, hash: &[u8], bootstrap: &[u8], memory: Option<usize>, imports: &[usize]) -> Spawned {
    let mut scratch = [0u64; 128];
    let mut allocator = SingleSegmentAllocator::new(as_bytes_mut(&mut scratch));
    let mut message = Builder::new(&mut allocator);
    let mut params = message.init_root::<spawn_params::Builder>();
    params.set_program(hash);
    params.set_bootstrap(bootstrap);
    let mut words = [0usize; 120];
    let len = words::pack(&message, &mut words).expect("spawn: the request fits a message");
    let mut caps = [ipc::Give::copy(0); 16];
    let mut n = 0;
    if let Some(memory) = memory {
        caps[0] = ipc::Give::moved(memory);
        n = 1;
    }
    for &cap in imports {
        caps[n] = ipc::Give::copy(cap);
        n += 1;
    }
    let back = slots::alloc().expect("slot");
    ipc::set_receive_window(process_slot::CNODE, back, 1);
    let reply = ipc::call(info.factory, process_factory::SPAWN, &words[..len], &caps[..n], &mut []).expect("spawn: call");
    Spawned { status: reply.info.label(), back }
}

/// How a process ended, as `wait` reports it.
#[derive(Debug, PartialEq)]
enum Ended {
    Exited(u64),
    Faulted(u64),
    Killed,
}

/// `wait` on `handle`: how the process ended, and the slot its memory came
/// back in; or the reply's label if it isn't `ok`. The kernel answers a
/// call through an empty slot with the label `FAILED_LOOKUP`.
fn factory_wait(handle: usize) -> Result<(Ended, usize), usize> {
    let back = slots::alloc().expect("slot");
    ipc::set_receive_window(process_slot::CNODE, back, 1);
    let mut out = [0usize; 8];
    let reply = ipc::call(handle, process::WAIT, &[], &[], &mut out).expect("wait: call");
    if reply.info.label() != label(Status::Ok) {
        return Err(reply.info.label());
    }
    let segment = [words::bytes(&out[..reply.info.length()])];
    let message = words::reader(&segment);
    let end = message.get_root::<wait_results::Reader>().and_then(|r| r.get_end()).expect("wait: the results");
    let ended = match end.which().expect("wait: how it ended") {
        end::Exited(code) => Ended::Exited(code),
        end::Faulted(fault) => Ended::Faulted(fault),
        end::Killed(()) => Ended::Killed,
    };
    Ok((ended, back))
}

/// The process factory: start the peer from init's boot pack with memory
/// this process provides; hear how it ended (an exit code, a fault, a
/// kill) and get the memory back; and the refusals.
pub fn check_factory(info: &Env) {
    let region = untyped_at_least(info, 22);
    let in_root = SlotRef { root: process_slot::CNODE, index: 0, depth: 0 };
    let memory = |bits| {
        let at = slots::alloc().expect("slot");
        expect("child memory", untyped::retype(region, object::UNTYPED, bits, in_root, at, 1), Ok(()));
        at
    };
    let kind = |s: usize| slot(s).identify().map(|id| id.object_type).map_err(|e| e.0);
    let peer = info.peer_hash;

    // An exit code, and the memory back, usable again.
    let (bootstrap, len) = peer_bootstrap_with(Mode::Exit, 0, 7);
    let spawned = factory_spawn(info, &peer, &bootstrap[..len], Some(memory(20)), &[]);
    assert_eq!((spawned.status, kind(spawned.back)), (label(Status::Ok), Ok(object::ENDPOINT)), "spawn: a handle");
    let (ended, back) = factory_wait(spawned.back).expect("wait");
    assert_eq!((ended, kind(back)), (Ended::Exited(7), Ok(object::UNTYPED)), "an exit, and the memory back");
    assert_eq!(factory_wait(spawned.back).err(), Some(error::FAILED_LOOKUP), "the handle is gone after the wait");
    let again = factory_spawn(info, &peer, &bootstrap[..len], Some(back), &[]);
    let (ended, back) = factory_wait(again.back).expect("wait");
    assert_eq!((ended, kind(back)), (Ended::Exited(7), Ok(object::UNTYPED)), "the returned memory builds a process again");

    // A fault.
    let (bootstrap, len) = peer_bootstrap(Mode::Crash, 0);
    let crashed = factory_spawn(info, &peer, &bootstrap[..len], Some(back), &[]);
    let (ended, back) = factory_wait(crashed.back).expect("wait");
    assert_eq!(ended, Ended::Faulted(fault::VM as u64), "a fault");

    // A kill: the peer calls an endpoint nobody answers, and blocks.
    let ep = slots::alloc().expect("slot");
    expect("endpoint", untyped::retype(region, object::ENDPOINT, 0, in_root, ep, 1), Ok(()));
    let (bootstrap, len) = peer_bootstrap(Mode::Call, 0);
    let blocked = factory_spawn(info, &peer, &bootstrap[..len], Some(back), &[ep]);
    assert_eq!(blocked.status, label(Status::Ok), "spawn with an import");
    let killed = ipc::call(blocked.back, process::KILL, &[], &[], &mut []).expect("kill");
    assert_eq!(killed.info.label(), label(Status::Ok), "kill");
    let (ended, back) = factory_wait(blocked.back).expect("wait");
    assert_eq!(ended, Ended::Killed, "a kill");

    // Refusals, each giving the memory back.
    let unknown = factory_spawn(info, &[0; 32], &bootstrap[..len], Some(back), &[]);
    assert_eq!((unknown.status, kind(unknown.back)), (label(Status::NoSuchProgram), Ok(object::UNTYPED)), "an unknown program");
    let small = factory_spawn(info, &peer, &bootstrap[..len], Some(memory(14)), &[]);
    assert_eq!((small.status, kind(small.back)), (label(Status::LoadFailed), Ok(object::UNTYPED)), "too little memory");
    let not_memory = factory_spawn(info, &peer, &bootstrap[..len], Some(ep), &[]);
    assert_eq!((not_memory.status, kind(not_memory.back)), (label(Status::BadRequest), Ok(object::ENDPOINT)), "an endpoint as memory");
    let nothing = factory_spawn(info, &peer, &bootstrap[..len], None, &[]);
    assert_eq!(nothing.status, label(Status::BadRequest), "no memory");
    // A request that isn't a message: a root pointer to a struct far past
    // the end of the words.
    let broken = [(1000 << 2) | (1 << 32)];
    let back = slots::alloc().expect("slot");
    ipc::set_receive_window(process_slot::CNODE, back, 1);
    let reply = ipc::call(info.factory, process_factory::SPAWN, &broken, &[ipc::Give::moved(unknown.back)], &mut []).expect("spawn: call");
    assert_eq!((reply.info.label(), kind(back)), (label(Status::BadRequest), Ok(object::UNTYPED)), "a malformed request");
    // A hash of the wrong length.
    let short = factory_spawn(info, &peer[..31], &bootstrap[..len], Some(back), &[]);
    assert_eq!((short.status, kind(short.back)), (label(Status::BadRequest), Ok(object::UNTYPED)), "a short hash");

    expect("revoke", slot(region).revoke(), Ok(()));
    println!("the process factory starts programs by hash with the caller's memory, reports exits, faults and kills, and gives the memory back");
}

/// The log, an export of the supervisor's: it refuses an unknown method
/// and malformed text, and the supervisor names a client only to the
/// server it is a client of, so `check`, which serves nothing, learns
/// nobody's name.
pub fn check_log(info: &Env) {
    let mut out = [0usize; 8];
    let label = |r: Result<ipc::Received, Error>| r.expect("a call").info.label();
    assert_eq!(label(ipc::call(info.log, 7, &[], &[], &mut out)), 1, "the log should refuse an unknown method");
    // A struct whose text pointer runs far past the message.
    let malformed = [1 << 48, 1 | (2 << 32) | (1000 << 35)];
    assert_eq!(label(ipc::call(info.log, log::WRITE, &malformed, &[], &mut out)), 1, "the log should refuse malformed text");
    let asked = ipc::call(process_slot::LAUNCHER, launcher::CLIENT, &[1], &[], &mut out);
    assert_eq!(label(asked), launcher::NO_SUCH_CLIENT, "the supervisor should name clients only to their server");
    println!("the log refuses unknown methods and malformed text; the supervisor names a client only to its server");
}
