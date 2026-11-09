//! A demo client: calls a `Restartable` service a few times
//! (`interfaces/test/restartable.capnp`), writes each answer to the log,
//! and exits. Which service it calls is its composition's business: the
//! resolver finds one, or a request binds one by hand.

#![no_std]
#![no_main]
#![feature(strict_provenance_lints)]
// Pointers come from `sap_sys::address::mapped`, never from integer casts.
#![deny(implicit_provenance_casts, integer_to_ptr_transmutes)]

/// Generated from the component schema of `counter`
/// (`user/programs/<name>/<name>.capnp`).
mod components {
    #[allow(clippy::all, dead_code)]
    pub mod counter_capnp {
        include!(concat!(env!("OUT_DIR"), "/counter/counter_capnp.rs"));
    }
}

use capnp::message::{Reader, ReaderOptions, SegmentArray};
use components::counter_capnp::counter_bootstrap;
use sap_interfaces::methods::restartable;
use sap_interfaces::restartable_capnp::restartable::count_results;
use sap_interfaces::words;
use sap_rt::{endpoint, print, println, startup};

#[unsafe(no_mangle)]
fn sap_main(a0: usize, _: usize, _: usize) -> ! {
    // SAFETY: the loader started this process with its startup record.
    let startup = unsafe { startup::record(a0) };
    let segments = [startup::bootstrap(startup)];
    let reader = Reader::new(SegmentArray::new(&segments), ReaderOptions::new());
    let bootstrap = reader.get_root::<counter_bootstrap::Reader>().expect("counter: bootstrap message");
    let import = |index: u32| startup::import(startup, index).expect("counter: no such import");
    print::set_log(import(bootstrap.get_log()));
    let service = import(bootstrap.get_service());

    for _ in 0..bootstrap.get_calls() {
        let mut out = [0usize; 4];
        let reply = endpoint::call(service, restartable::COUNT, &[], &[], &mut out).expect("counter: call the service");
        assert_eq!(reply.info.label(), 0, "counter: the service refused");
        let segment = [words::bytes(&out[..reply.info.length()])];
        let message = words::reader(&segment);
        let calls = message.get_root::<count_results::Reader>().expect("counter: the results").get_calls();
        println!("the service has answered {calls} calls");
    }
    sap_rt::exit(0)
}
