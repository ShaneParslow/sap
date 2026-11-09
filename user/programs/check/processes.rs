//! Processes: a second program in its own address space, built by hand;
//! the Supervisor interface; the supervisor's exports as the log shows them; and
//! restart.

use sap_interfaces::methods::{compositions, instance, log, restartable, supervisor};
use sap_interfaces::restartable_capnp::restartable::count_results;
use sap_interfaces::supervisor_capnp::instance::wait_results;
use sap_interfaces::supervisor_capnp::supervisor::start_params;
use sap_interfaces::supervisor_capnp::compositions::add_params;
use sap_interfaces::supervisor_capnp::{component, end, OnEnd, Status};
use sap_interfaces::words;
use alloc::vec;
use alloc::vec::Vec;
use sap_process::{parent, process_slot, MAX_BOOTSTRAP};
use sap_rt::abi::{attr, error, fault, object, rights};
use sap_rt::cnode::SlotRef;
use sap_rt::sys::Error;
use sap_rt::{account, endpoint, println, slots, tcb, untyped, vspace};
use capnp::message::{Builder, SingleSegmentAllocator};
use crate::components::counter_capnp::counter_bootstrap;
use crate::components::peer_capnp::peer_bootstrap;
use sap_loader::{self as loader, Give, Launch, LoadError, Parent, Slots, LAUNCH};
use sap_ring::as_bytes_mut;
use crate::{Env, expect, slot, untyped_at_least};

/// The second program, built by this crate's build script.
pub static PEER: &[u8] = include_bytes!(env!("PEER_ELF"));

/// What a peer does: `PeerBootstrap.mode`.
#[derive(Clone, Copy)]
pub enum Mode {
    Call,
    EchoWords,
    ServeEcho,
    Misbehave,
    SumBytes,
    Exit(u32),
    Crash,
    Restartable,
}

/// A peer's bootstrap message and its length, built in a buffer of its
/// own: `mode`, then the indices of its imports in the order `connect_peer`
/// and `check_process` give them.
pub fn peer_bootstrap(mode: Mode, data_pages: usize) -> ([u8; 256], usize) {
    let mut scratch = [0u64; 32];
    let mut allocator = SingleSegmentAllocator::new(as_bytes_mut(&mut scratch));
    let mut message = Builder::new(&mut allocator);
    let mut bootstrap = message.init_root::<peer_bootstrap::Builder>();
    bootstrap.set_endpoint(0);
    let mut m = bootstrap.reborrow().init_mode();
    match mode {
        Mode::Call => m.set_call(()),
        Mode::EchoWords => m.set_echo_words(()),
        Mode::ServeEcho => m.set_serve_echo(()),
        Mode::Misbehave => m.set_misbehave(()),
        Mode::SumBytes => m.set_sum_bytes(()),
        Mode::Exit(code) => m.set_exit(code),
        Mode::Crash => m.set_crash(()),
        Mode::Restartable => m.set_restartable(()),
    }
    let mut channel = bootstrap.init_channel();
    channel.set_data_pages(data_pages as u32);
    channel.set_notification(1);
    channel.set_doorbell(2);
    channel.set_reply(3);
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
        flags: info.flags,
    };
    let mut slots = Slots { next: base + 3, end: base + 200 };
    let (bootstrap, len) = peer_bootstrap(Mode::Call, 0);
    let imports = [(badged, Give::Copy(rights::WRITE | rights::GRANT))];
    let launch = Launch { priority: 255, spare_bits: Some(16), imports: &imports, bootstrap: &bootstrap[..len], ..LAUNCH };

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
    // The process ABI's slots: its own objects, its spare untyped, its import.
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
    slots.next += 1;
    expect("map scratch", vspace::map_frame(marker, process_slot::VSPACE, scratch, rights::READ | rights::WRITE, attr::EXECUTE_NEVER), Ok(()));
    peer.start().expect("start the peer");

    let mut words = [0usize; 8];
    let first = endpoint::recv(ep, reply, &mut words).expect("receive the peer's call");
    assert_eq!((first.badge, first.info.label(), first.info.length()), (0x9ee, 1, 6), "the peer's call");
    assert_eq!(words[..6], [1, 2, 3, 4, 5, 6], "the peer's words");
    let doubled = [2, 4, 6, 8, 10, 12];
    let second = endpoint::reply_recv(ep, reply, 0, &doubled, &[], &mut words).expect("receive the peer's report");
    assert_eq!((second.info.label(), words[0]), (2, 42), "the peer should report the sum of the reply");
    let fault_msg = endpoint::reply_recv(ep, reply, 0, &[], &[], &mut words).expect("receive the peer's fault");
    assert_eq!((fault_msg.info.label(), words[fault::ADDRESS]), (fault::VM, scratch), "the peer's read should fault");
    expect("leave it suspended", endpoint::reply(reply, 1, &[], &[]), Ok(()));

    expect("unmap scratch", vspace::unmap_frame(marker), Ok(()));

    // A bootstrap message longer than a page: the struct three pages in,
    // past zeroed words its root pointer skips. The peer exits with the
    // code it reads there.
    let (bootstrap, len) = peer_bootstrap(Mode::Exit(9), 0);
    let mut message: Vec<u64> = bootstrap[..len].chunks(8).map(|c| {
        let mut word = [0u8; 8];
        word[..c.len()].copy_from_slice(c);
        u64::from_le_bytes(word)
    }).collect();
    const SKIP: usize = 3 * 512;
    // A struct pointer's offset, in words, is its bits 2 to 31; the
    // struct has no pointers of its own to adjust.
    message[0] += (SKIP as u64) << 2;
    message.splice(1..1, core::iter::repeat_n(0, SKIP));
    let far: Vec<u8> = message.iter().flat_map(|w| w.to_le_bytes()).collect();
    let long = Launch { priority: 255, parent_endpoint: Some(badged), bootstrap: &far, ..LAUNCH };
    loader::load(PEER, &parent, &mut slots, &long).and_then(|p| Ok(p.start()?)).expect("load with a long bootstrap");
    let exit = endpoint::recv(ep, reply, &mut words).expect("the peer's exit");
    assert_eq!((exit.info.label(), words[0]), (parent::EXIT, 9), "the peer read its bootstrap past its first page");
    let too_long = vec![0u8; MAX_BOOTSTRAP + 1];
    let refused_long = refused(loader::load(PEER, &parent, &mut slots, &Launch { bootstrap: &too_long, ..long }));
    assert_eq!(refused_long, Some(LoadError::BadLaunch("bootstrap message too long")));
    expect("revoke", slot(parent_untyped).revoke(), Ok(()));
    println!("a second process, calls between address spaces, isolation and a bootstrap of several pages behave");
}

/// A route of a component started through the Supervisor interface.
#[derive(Clone, Copy)]
enum To<'a> {
    Passed(u32),
    Client(&'a str, &'a str),
    Serve(&'a str),
    Held(&'a str),
    Supervisor,
}

/// A component to start through the Supervisor interface: `program` in a 2 MiB
/// account, built from 1 MiB of it.
struct Component<'a> {
    name: &'a str,
    program: &'a [u8],
    bootstrap: &'a [u8],
    routes: &'a [To<'a>],
    exports: &'a [&'a str],
    on_end: OnEnd,
    reservation: u64,
    /// Untyped memory from the pool instead of an account, which the API
    /// refuses.
    untyped: bool,
}

impl<'a> Component<'a> {
    fn new(name: &'a str, program: &'a [u8], bootstrap: &'a [u8]) -> Self {
        Self { name, program, bootstrap, routes: &[], exports: &[], on_end: OnEnd::Forget, reservation: 2 << 20, untyped: false }
    }
}

/// Write `component` into `c`.
fn fill(mut c: component::Builder, component: &Component) {
    c.set_name(component.name);
    c.set_program(component.program);
    c.set_bootstrap(component.bootstrap);
    c.set_on_end(component.on_end);
    let mut exports = c.reborrow().init_exports(component.exports.len() as u32);
    for (i, name) in component.exports.iter().enumerate() {
        exports.reborrow().get(i as u32).set_name(name);
    }
    let mut limits = c.reborrow().init_limits();
    if component.untyped {
        limits.reborrow().init_memory().init_untyped().set_bits(20);
    } else {
        let mut terms = limits.reborrow().init_memory().init_account();
        terms.set_reservation(component.reservation);
        terms.set_limit(component.reservation.max(2 << 20));
        terms.set_process_bits(20);
    }
    limits.set_priority(255);
    limits.set_cnode_bits(6);
    limits.set_stack_pages(4);
    let mut routes = c.init_imports(component.routes.len() as u32);
    for (i, &to) in component.routes.iter().enumerate() {
        let mut route = routes.reborrow().get(i as u32);
        match to {
            To::Passed(n) => route.set_passed(n),
            To::Client(c, n) => {
                let mut client = route.init_client();
                client.set_component(c);
                client.set_name(n);
            }
            To::Serve(n) => route.set_serve(n),
            To::Held(n) => route.init_held().set_name(n),
            To::Supervisor => route.set_supervisor(()),
        }
    }
}

/// `Supervisor.start` with `component`, paid from check's account, with
/// `passed` (copied); the handle's slot and the first of its exports'
/// capabilities, or the reply's label.
fn start(info: &Env, component: &Component, passed: &[usize]) -> Result<(usize, usize), usize> {
    let mut scratch = [0u64; 128];
    let mut allocator = SingleSegmentAllocator::new(as_bytes_mut(&mut scratch));
    let mut message = Builder::new(&mut allocator);
    fill(message.init_root::<start_params::Builder>().init_component(), component);
    let mut words = [0usize; 120];
    let len = words::pack(&message, &mut words).expect("start: the request fits a message");
    let mut caps = [endpoint::Give::copy(info.account); 16];
    for (i, &cap) in passed.iter().enumerate() {
        caps[1 + i] = endpoint::Give::copy(cap);
    }
    let back = slots::alloc_range(1 + component.exports.len()).expect("slots");
    endpoint::set_receive_window(process_slot::CNODE, back, 1 + component.exports.len());
    let reply = endpoint::call(info.supervisor, supervisor::START, &words[..len], &caps[..1 + passed.len()], &mut []).expect("start: call");
    match reply.info.label() {
        0 => Ok((back, back + 1)),
        label => Err(label),
    }
}

/// `Restartable.count` on `cap`: how many calls the serving process has
/// answered; or the call's error.
fn count(cap: usize) -> Result<u64, usize> {
    let mut out = [0usize; 4];
    let reply = endpoint::call(cap, restartable::COUNT, &[], &[], &mut out).map_err(|e| e.0)?;
    assert_eq!(reply.info.label(), 0, "count");
    let segment = [words::bytes(&out[..reply.info.length()])];
    let message = words::reader(&segment);
    Ok(message.get_root::<count_results::Reader>().expect("count: the results").get_calls())
}

/// `Restartable.crash` on `cap`: the call's error, as it never returns.
fn crash(cap: usize) -> Result<(), usize> {
    endpoint::call(cap, restartable::CRASH, &[], &[], &mut []).map(drop).map_err(|e| e.0)
}

/// How an instance ended, as `wait` reports it.
#[derive(Debug, PartialEq)]
enum Ended {
    Exited(u64),
    Faulted(u64),
    Stopped,
}

/// `wait` on `handle`: how the instance ended, or the reply's label if it
/// isn't `ok`. The kernel answers a call through an empty slot with the
/// label `FAILED_LOOKUP`.
fn wait(handle: usize) -> Result<Ended, usize> {
    let mut out = [0usize; 8];
    let reply = endpoint::call(handle, instance::WAIT, &[], &[], &mut out).expect("wait: call");
    if reply.info.label() != Status::Ok as usize {
        return Err(reply.info.label());
    }
    let segment = [words::bytes(&out[..reply.info.length()])];
    let message = words::reader(&segment);
    let end = message.get_root::<wait_results::Reader>().and_then(|r| r.get_end()).expect("wait: the results");
    Ok(match end.which().expect("wait: how it ended") {
        end::Exited(code) => Ended::Exited(code),
        end::Faulted(fault) => Ended::Faulted(fault),
        end::Stopped(()) => Ended::Stopped,
    })
}

fn stop(handle: usize) {
    let reply = endpoint::call(handle, instance::STOP, &[], &[], &mut []).expect("stop: call");
    assert_eq!(reply.info.label(), Status::Ok as usize, "stop");
}

/// `Supervisor.compose`, paid from check's account: the capability to add
/// to the composition and its handle, or the reply's label.
fn compose(info: &Env) -> Result<(usize, usize), usize> {
    let back = slots::alloc_range(2).expect("slots");
    endpoint::set_receive_window(process_slot::CNODE, back, 2);
    let reply = endpoint::call(info.supervisor, supervisor::COMPOSE, &[], &[endpoint::Give::copy(info.account)], &mut []).expect("compose: call");
    match reply.info.label() {
        0 => Ok((back, back + 1)),
        label => Err(label),
    }
}

/// `Compositions.add` of `component` on `cap`: the reply's label.
fn add(cap: usize, component: &Component) -> usize {
    let mut scratch = [0u64; 128];
    let mut allocator = SingleSegmentAllocator::new(as_bytes_mut(&mut scratch));
    let mut message = Builder::new(&mut allocator);
    fill(message.init_root::<add_params::Builder>().init_component(), component);
    let mut words = [0usize; 120];
    let len = words::pack(&message, &mut words).expect("add: the component fits a message");
    endpoint::call(cap, compositions::ADD, &words[..len], &[], &mut []).expect("add: call").info.label()
}

/// `Compositions.enact` on `cap`: the reply's label.
fn enact(cap: usize) -> usize {
    endpoint::call(cap, compositions::ENACT, &[], &[], &mut []).expect("enact: call").info.label()
}

/// A counter's bootstrap message and its length: imports `service` then
/// `log`.
fn counter(calls: u32) -> ([u8; 128], usize) {
    let mut scratch = [0u64; 16];
    let mut allocator = SingleSegmentAllocator::new(as_bytes_mut(&mut scratch));
    let mut message = Builder::new(&mut allocator);
    let mut bootstrap = message.init_root::<counter_bootstrap::Builder>();
    bootstrap.set_service(0);
    bootstrap.set_log(1);
    bootstrap.set_calls(calls);
    let segments = message.get_segments_for_output();
    // Copied out: the allocator zeroes its buffer when the message goes.
    let mut bytes = [0; 128];
    bytes[..segments[0].len()].copy_from_slice(segments[0]);
    (bytes, segments[0].len())
}

/// Compositions made at runtime (`Supervisor.compose`): members routed to
/// each other by name and to the caller's own imports, ended by the member
/// whose end ends it, with what still runs stopped and the memory back;
/// and the refusals.
pub fn check_compose(info: &Env) {
    let used = || account::usage(info.account).expect("check's account").used;
    let before = used();
    let (cap, handle) = compose(info).expect("compose");
    let (server_bootstrap, server_len) = peer_bootstrap(Mode::Restartable, 0);
    let server = Component {
        routes: &[To::Serve("service")],
        exports: &["service"],
        on_end: OnEnd::Restart,
        ..Component::new("server", &info.peer_hash, &server_bootstrap[..server_len])
    };
    let ok = Status::Ok as usize;
    assert_eq!(add(cap, &server), ok, "a server");
    let (client_bootstrap, client_len) = counter(2);
    let client = Component {
        routes: &[To::Client("server", "service"), To::Client("log", "log")],
        on_end: OnEnd::EndComposition,
        ..Component::new("client", &info.counter_hash, &client_bootstrap[..client_len])
    };
    let bad = Status::BadRequest as usize;
    assert_eq!(add(cap, &Component { routes: &[To::Held("asidPool")], ..client }), bad, "what the supervisor holds");
    assert_eq!(add(cap, &client), ok, "a client of the server and of the caller's log");
    assert_eq!(add(cap, &client), bad, "a name in use");
    assert_eq!(enact(cap), ok, "enact");
    assert_eq!(wait(handle), Ok(Ended::Exited(0)), "the client's end ends the composition");
    assert!(slot(cap).identify().is_err(), "an ended composition takes no more members");
    assert_eq!(used(), before, "the server is stopped and the memory comes back");

    // Routes are checked when it starts, as a client may come before its
    // server: nothing starts if one names an export that isn't a member's
    // or the caller's.
    for (component, export) in [("nobody", "service"), ("memory", "account")] {
        let (cap, handle) = compose(info).expect("compose");
        let orphan = Component { routes: &[To::Client(component, export), To::Client("log", "log")], ..client };
        assert_eq!(add(cap, &orphan), ok, "a client added before its server");
        assert_eq!(enact(cap), bad, "an export that isn't a member's or the caller's");
        stop(handle);
        assert_eq!(wait(handle), Ok(Ended::Stopped));
    }
    assert_eq!(used(), before, "refused compositions cost nothing");

    // Stopped before it ends.
    let (cap, handle) = compose(info).expect("compose");
    assert_eq!(add(cap, &server), ok, "a server");
    assert_eq!(enact(cap), ok, "enact");
    stop(handle);
    assert_eq!(wait(handle), Ok(Ended::Stopped));
    assert_eq!(used(), before, "a stopped composition's memory comes back");
    println!("compositions made at runtime route their members to each other and to the caller's imports, end with the member that ends them, and give the memory back");
}

/// The Supervisor interface: start `peer` from init's boot pack, paid from check's
/// account; hear how it ended (an exit code, a fault, a stop) and see the
/// memory come back; each kind of route; restart behind an export of its
/// own; and the refusals.
pub fn check_instances(info: &Env) {
    let peer = info.peer_hash;
    let used = || account::usage(info.account).expect("check's account").used;
    let before = used();

    // An exit code; the memory comes back; the handle goes.
    let (bootstrap, len) = peer_bootstrap(Mode::Exit(7), 0);
    let exits = Component::new("exits", &peer, &bootstrap[..len]);
    let (handle, _) = start(info, &exits, &[]).expect("start");
    assert_eq!(wait(handle), Ok(Ended::Exited(7)), "an exit");
    assert_eq!(used(), before, "the caller's memory comes back");
    assert_eq!(wait(handle), Err(error::FAILED_LOOKUP), "the handle is gone after the wait");

    // A fault.
    let (bootstrap, len) = peer_bootstrap(Mode::Crash, 0);
    let (handle, _) = start(info, &Component::new("crashes", &peer, &bootstrap[..len]), &[]).expect("start");
    assert_eq!(wait(handle), Ok(Ended::Faulted(fault::VM as u64)), "a fault");

    // A passed capability: an endpoint of check's, which the peer serves;
    // the caller pays while it runs; then a stop.
    let region = untyped_at_least(info, 16);
    let in_root = SlotRef { root: process_slot::CNODE, index: 0, depth: 0 };
    let ep = slots::alloc().expect("slot");
    expect("endpoint", untyped::retype(region, object::ENDPOINT, 0, in_root, ep, 1), Ok(()));
    let (bootstrap, len) = peer_bootstrap(Mode::Restartable, 0);
    let serves = Component { routes: &[To::Passed(0)], ..Component::new("serves", &peer, &bootstrap[..len]) };
    let (handle, _) = start(info, &serves, &[ep]).expect("start with a passed capability");
    assert_eq!(count(ep), Ok(1), "the peer serves the passed endpoint");
    assert!(used() >= before + (1 << 20), "the caller pays for it");
    assert_eq!(start(info, &serves, &[ep]).err(), Some(Status::BadRequest as usize), "a name already running");
    stop(handle);
    assert_eq!(wait(handle), Ok(Ended::Stopped), "a stop");
    assert_eq!(used(), before);

    // An export of its own, restarted behind it.
    let restarts = Component { routes: &[To::Serve("service")], exports: &["service"], on_end: OnEnd::Restart, ..Component::new("restarts", &peer, &bootstrap[..len]) };
    let (handle, service) = start(info, &restarts, &[]).expect("start with an export");
    assert_eq!(count(service), Ok(1), "the caller gets a capability to the export");
    assert_eq!(crash(service), Err(error::NO_REPLY));
    assert_eq!(count(service), Ok(1), "restarted behind its export");
    stop(handle);
    assert_eq!(wait(handle), Ok(Ended::Stopped));
    assert!(slot(service).identify().is_err(), "a stopped instance's exports go");
    assert_eq!(used(), before, "its exports' memory comes back");

    // Exports the caller imports itself, re-badged; the Supervisor interface.
    let (bootstrap, len) = peer_bootstrap(Mode::Exit(0), 0);
    let imports = Component { routes: &[To::Client("log", "log"), To::Client("restartable", "service"), To::Supervisor], ..Component::new("imports", &peer, &bootstrap[..len]) };
    let (handle, _) = start(info, &imports, &[]).expect("start with the caller's exports");
    assert_eq!(wait(handle), Ok(Ended::Exited(0)));

    // Ending its composition, which is its own: not the boot.
    let (bootstrap5, len5) = peer_bootstrap(Mode::Exit(5), 0);
    let ends = Component { on_end: OnEnd::EndComposition, ..Component::new("ends", &peer, &bootstrap5[..len5]) };
    let (handle, _) = start(info, &ends, &[]).expect("start one that ends its composition");
    assert_eq!(wait(handle), Ok(Ended::Exited(5)));

    // Refusals.
    let refused = |component: &Component, passed: &[usize]| start(info, component, passed).err();
    let bad = Some(Status::BadRequest as usize);
    let base = Component::new("refused", &peer, &bootstrap[..len]);
    assert_eq!(refused(&Component { program: &[0; 32], ..base }, &[]), Some(Status::NoSuchProgram as usize), "an unknown program");
    assert_eq!(refused(&Component { program: &peer[..31], ..base }, &[]), bad, "a short hash");
    assert_eq!(refused(&Component { routes: &[To::Client("memory", "account")], ..base }, &[]), bad, "an export the caller doesn't import");
    assert_eq!(refused(&Component { routes: &[To::Held("asidPool")], ..base }, &[]), bad, "what the supervisor holds");
    assert_eq!(refused(&Component { routes: &[To::Passed(0)], ..base }, &[]), bad, "a capability not passed");
    assert_eq!(refused(&Component { routes: &[To::Serve("nothing")], ..base }, &[]), bad, "an export it doesn't declare");
    assert_eq!(refused(&Component { untyped: true, ..base }, &[]), bad, "untyped memory");
    assert_eq!(refused(&Component { reservation: 1 << 30, ..base }, &[]), Some(Status::OverLimit as usize), "more than the caller has");
    let no_account = endpoint::call(info.supervisor, supervisor::START, &[], &[], &mut []).expect("start: call");
    assert_eq!(no_account.info.label(), Status::BadRequest as usize, "no account");
    // Not a message: a root pointer to a struct far past the words.
    let broken = [(1000 << 2) | (1 << 32)];
    let reply = endpoint::call(info.supervisor, supervisor::START, &broken, &[endpoint::Give::copy(info.account)], &mut []).expect("start: call");
    assert_eq!(reply.info.label(), Status::BadRequest as usize, "a malformed request");
    assert_eq!(used(), before, "refusals cost nothing");

    expect("revoke", slot(region).revoke(), Ok(()));
    println!("the Supervisor interface starts components with the caller's memory and capabilities, restarts them behind their exports, reports exits, faults and stops, and gives the memory back");
}

/// The log, an export of the supervisor's: it refuses an unknown method
/// and malformed text, and the supervisor names a client only to the
/// server it is a client of, so `check`, which serves nothing, learns
/// nobody's name.
pub fn check_log(info: &Env) {
    let mut out = [0usize; 8];
    let label = |r: Result<endpoint::Received, Error>| r.expect("a call").info.label();
    assert_eq!(label(endpoint::call(info.log, 7, &[], &[], &mut out)), 1, "the log should refuse an unknown method");
    // A struct whose text pointer runs far past the message.
    let malformed = [1 << 48, 1 | (2 << 32) | (1000 << 35)];
    assert_eq!(label(endpoint::call(info.log, log::WRITE, &malformed, &[], &mut out)), 1, "the log should refuse malformed text");
    let asked = endpoint::call(process_slot::PARENT, parent::CLIENT, &[1], &[], &mut out);
    assert_eq!(label(asked), parent::NO_SUCH_CLIENT, "the supervisor should name clients only to their server");
    println!("the log refuses unknown methods and malformed text; the supervisor names a client only to its server");
}

/// Restart: a service the supervisor restarts when it fails keeps its
/// export, so the same capability reaches the new process; a call the old
/// one died in gets no reply, and the new one starts afresh.
pub fn check_restart(info: &Env) {
    let service = info.restartable;
    assert_eq!((count(service), count(service)), (Ok(1), Ok(2)), "the service counts its calls");
    assert_eq!(crash(service), Err(error::NO_REPLY), "a call the service died in gets no reply");
    assert_eq!(count(service), Ok(1), "the restarted service starts afresh, on the same capability");
    assert_eq!(crash(service), Err(error::NO_REPLY));
    assert_eq!((count(service), count(service)), (Ok(1), Ok(2)), "and again");
    println!("a failed service restarts behind the same capability; the call it died in gets no reply");
}
