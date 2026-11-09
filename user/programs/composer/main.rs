//! A demo of composing at runtime: makes a composition of its own through
//! its supervisor (`Supervisor.compose`), has the resolver fill it from
//! the request in its settings (`Resolver.resolve`), starts it, waits for
//! it to end, and exits as it ended: 0 if it ended with an exit of 0,
//! else 1.

#![no_std]
#![no_main]
#![feature(strict_provenance_lints)]
// Pointers come from `sap_sys::address::mapped`, never from integer casts.
#![deny(implicit_provenance_casts, integer_to_ptr_transmutes)]

/// Generated from the component schema of `composer`
/// (`user/programs/<name>/<name>.capnp`).
mod components {
    #[allow(clippy::all, dead_code)]
    pub mod composer_capnp {
        include!(concat!(env!("OUT_DIR"), "/composer/composer_capnp.rs"));
    }
}

use capnp::message::{Builder, Reader, ReaderOptions, SegmentArray, SingleSegmentAllocator};
use components::composer_capnp::composer_bootstrap;
use sap_interfaces::methods::{compositions, instance, resolver, supervisor};
use sap_interfaces::resolver_capnp::resolver::resolve_params;
use sap_interfaces::supervisor_capnp::instance::wait_results;
use sap_interfaces::supervisor_capnp::{end, Status};
use sap_interfaces::words;
use sap_process::process_slot;
use sap_rt::abi::MAX_MESSAGE_LENGTH;
use sap_rt::endpoint::{self, Give};
use sap_rt::{print, println, startup};

#[unsafe(no_mangle)]
fn sap_main(a0: usize, _: usize, _: usize) -> ! {
    // SAFETY: the loader started this process with its startup record.
    let startup = unsafe { startup::record(a0) };
    let segments = [startup::bootstrap(startup)];
    let reader = Reader::new(SegmentArray::new(&segments), ReaderOptions::new());
    let bootstrap = reader.get_root::<composer_bootstrap::Reader>().expect("composer: bootstrap message");
    let import = |index: u32| startup::import(startup, index).expect("composer: no such import");
    print::set_log(import(bootstrap.get_log()));
    let (supervisor_cap, resolver_cap) = (import(bootstrap.get_supervisor()), import(bootstrap.get_resolver()));

    // A composition of its own, paid from its account: a capability to add
    // to it, and its handle.
    let (compositions_cap, handle) = (startup.free.start, startup.free.start + 1);
    endpoint::set_receive_window(process_slot::CNODE, compositions_cap, 2);
    let reply = endpoint::call(supervisor_cap, supervisor::COMPOSE, &[], &[Give::copy(process_slot::ACCOUNT)], &mut []).expect("composer: compose");
    assert_eq!(reply.info.label(), Status::Ok as usize, "composer: the supervisor refused to compose");
    endpoint::set_receive_window(process_slot::CNODE, 0, 0);

    // The resolver fills it from the request.
    let mut scratch = [0u64; MAX_MESSAGE_LENGTH];
    let mut allocator = SingleSegmentAllocator::new(sap_ring::as_bytes_mut(&mut scratch));
    let mut message = Builder::new(&mut allocator);
    let request = bootstrap.get_request().expect("composer: a request");
    message.init_root::<resolve_params::Builder>().set_request(request).expect("composer: the request fits a call");
    let mut out = [0usize; MAX_MESSAGE_LENGTH];
    let n = words::pack(&message, &mut out).expect("composer: the request fits a call");
    let reply = endpoint::call(resolver_cap, resolver::RESOLVE, &out[..n], &[Give::copy(compositions_cap)], &mut []).expect("composer: resolve");
    assert_eq!(reply.info.label(), 0, "composer: the resolver refused the request");

    let reply = endpoint::call(compositions_cap, compositions::ENACT, &[], &[], &mut []).expect("composer: enact");
    assert_eq!(reply.info.label(), Status::Ok as usize, "composer: starting the composition failed");
    println!("started its composition; waiting for it to end");

    let mut out = [0usize; 8];
    let reply = endpoint::call(handle, instance::WAIT, &[], &[], &mut out).expect("composer: wait");
    assert_eq!(reply.info.label(), Status::Ok as usize, "composer: wait refused");
    let segment = [words::bytes(&out[..reply.info.length()])];
    let message = words::reader(&segment);
    let ended = message.get_root::<wait_results::Reader>().and_then(|r| r.get_end()).expect("composer: wait's results");
    let code = match ended.which() {
        Ok(end::Exited(code)) => {
            println!("its composition ended: exited with {code}");
            code
        }
        Ok(end::Faulted(label)) => {
            println!("its composition ended: faulted ({label})");
            1
        }
        _ => {
            println!("its composition ended: stopped");
            1
        }
    };
    sap_rt::exit(if code == 0 { 0 } else { 1 })
}
