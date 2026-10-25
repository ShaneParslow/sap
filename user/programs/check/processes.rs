//! Processes: a second program in its own address space, and the process
//! factory.

use sap_interfaces::factory;
use sap_process::process_slot;
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

/// A peer's bootstrap message and its length, built without the heap
/// (which `check_heap` wants untouched): `mode`, then the indices of its
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
    println!("check: a second process, calls between address spaces and isolation behave");
}

/// What a `SPAWN` returned: its label, and the slot the handle (on
/// success) or the memory (on a refusal) came back in.
struct Spawned {
    label: usize,
    back: usize,
}

/// Ask the factory to start the program named `hash` with `bootstrap`,
/// built from the untyped memory in `memory` (moved), with `imports`
/// (copied).
fn factory_spawn(info: &Env, hash: &[u8; 32], bootstrap: &[u8], memory: Option<usize>, imports: &[usize]) -> Spawned {
    let mut words = [0usize; 120];
    for (i, chunk) in hash.chunks(8).enumerate() {
        words[i] = u64::from_le_bytes(chunk.try_into().unwrap()) as usize;
    }
    words[factory::HASH_WORDS] = bootstrap.len();
    let packed = &mut words[factory::HASH_WORDS + 1..];
    for (i, chunk) in bootstrap.chunks(8).enumerate() {
        let mut word = [0u8; 8];
        word[..chunk.len()].copy_from_slice(chunk);
        packed[i] = u64::from_le_bytes(word) as usize;
    }
    let len = factory::HASH_WORDS + 1 + bootstrap.len().div_ceil(8);
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
    let reply = ipc::call(info.factory, factory::SPAWN, &words[..len], &caps[..n], &mut []).expect("spawn: call");
    Spawned { label: reply.info.label(), back }
}

/// `WAIT` on `handle`: how the process ended, its code, and the slot its
/// memory came back in; or the reply's label if it isn't `OK`. The kernel
/// answers a call through an empty slot with the label `FAILED_LOOKUP`.
fn factory_wait(handle: usize) -> Result<(usize, usize, usize), usize> {
    let back = slots::alloc().expect("slot");
    ipc::set_receive_window(process_slot::CNODE, back, 1);
    let mut out = [0usize; 2];
    let reply = ipc::call(handle, factory::WAIT, &[], &[], &mut out).expect("wait: call");
    if reply.info.label() != factory::OK {
        return Err(reply.info.label());
    }
    assert_eq!(reply.info.length(), 2, "wait: the reply");
    Ok((out[0], out[1], back))
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
    assert_eq!((spawned.label, kind(spawned.back)), (factory::OK, Ok(object::ENDPOINT)), "spawn: a handle");
    let (how, code, back) = factory_wait(spawned.back).expect("wait");
    assert_eq!((how, code, kind(back)), (factory::EXITED, 7, Ok(object::UNTYPED)), "an exit, and the memory back");
    assert_eq!(factory_wait(spawned.back).err(), Some(error::FAILED_LOOKUP), "the handle is gone after the wait");
    let again = factory_spawn(info, &peer, &bootstrap[..len], Some(back), &[]);
    let (how, code, back) = factory_wait(again.back).expect("wait");
    assert_eq!((how, code, kind(back)), (factory::EXITED, 7, Ok(object::UNTYPED)), "the returned memory builds a process again");

    // A fault.
    let (bootstrap, len) = peer_bootstrap(Mode::Crash, 0);
    let crashed = factory_spawn(info, &peer, &bootstrap[..len], Some(back), &[]);
    let (how, code, back) = factory_wait(crashed.back).expect("wait");
    assert_eq!((how, code), (factory::FAULTED, fault::VM), "a fault");

    // A kill: the peer calls an endpoint nobody answers, and blocks.
    let ep = slots::alloc().expect("slot");
    expect("endpoint", untyped::retype(region, object::ENDPOINT, 0, in_root, ep, 1), Ok(()));
    let (bootstrap, len) = peer_bootstrap(Mode::Call, 0);
    let blocked = factory_spawn(info, &peer, &bootstrap[..len], Some(back), &[ep]);
    assert_eq!(blocked.label, factory::OK, "spawn with an import");
    let killed = ipc::call(blocked.back, factory::KILL, &[], &[], &mut []).expect("kill");
    assert_eq!(killed.info.label(), factory::OK, "kill");
    let (how, _, back) = factory_wait(blocked.back).expect("wait");
    assert_eq!(how, factory::KILLED, "a kill");

    // Refusals, each giving the memory back.
    let unknown = factory_spawn(info, &[0; 32], &bootstrap[..len], Some(back), &[]);
    assert_eq!((unknown.label, kind(unknown.back)), (factory::NO_SUCH_PROGRAM, Ok(object::UNTYPED)), "an unknown program");
    let small = factory_spawn(info, &peer, &bootstrap[..len], Some(memory(14)), &[]);
    assert_eq!((small.label, kind(small.back)), (factory::LOAD_FAILED, Ok(object::UNTYPED)), "too little memory");
    let not_memory = factory_spawn(info, &peer, &bootstrap[..len], Some(ep), &[]);
    assert_eq!((not_memory.label, kind(not_memory.back)), (factory::BAD_REQUEST, Ok(object::ENDPOINT)), "an endpoint as memory");
    let nothing = factory_spawn(info, &peer, &bootstrap[..len], None, &[]);
    assert_eq!(nothing.label, factory::BAD_REQUEST, "no memory");
    // A length word claiming more bytes than the request carries.
    let mut lying = [0usize; factory::HASH_WORDS + 1];
    lying[factory::HASH_WORDS] = 64;
    let back = slots::alloc().expect("slot");
    ipc::set_receive_window(process_slot::CNODE, back, 1);
    let reply = ipc::call(info.factory, factory::SPAWN, &lying, &[ipc::Give::moved(unknown.back)], &mut []).expect("spawn: call");
    assert_eq!((reply.info.label(), kind(back)), (factory::BAD_REQUEST, Ok(object::UNTYPED)), "a bootstrap longer than the request");

    expect("revoke", slot(region).revoke(), Ok(()));
    println!("check: the process factory starts programs by hash with the caller's memory, reports exits, faults and kills, and gives the memory back");
}
