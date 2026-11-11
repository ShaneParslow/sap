//! The program the root supervisor starts at boot to check the runtime and
//! the services; the kernel's own tests are `test-kernel`. Its bootstrap
//! message (`CheckBootstrap` in `check.capnp`) gives it
//! the ASID pool, which only the root task holds at boot; its untyped
//! memory is chunks of its memory account. Its output
//! goes to the log, which puts the component's name before each line. It
//! reports success by exiting with code 0; a failed check panics, which
//! halts the machine.
//!
//! The checks are grouped by area, one module each, and run in the order
//! `sap_main` calls them. Helpers they share (`Env`, `slot`, `expect`, the
//! untyped regions) are here.

#![no_std]
#![no_main]
#![feature(strict_provenance_lints)]
// Pointers come from `sap_sys::address::mapped`, never from integer casts.
#![deny(implicit_provenance_casts, integer_to_ptr_transmutes)]

extern crate alloc;

mod accounts;
mod processes;
mod rt;
mod channels;
mod cheri;

/// Generated from the component schemas of `check`, `peer`
/// (`user/programs/<name>/<name>.capnp`).
mod components {
    #[allow(clippy::all, dead_code)]
    pub mod check_capnp {
        include!(concat!(env!("OUT_DIR"), "/check/check_capnp.rs"));
    }
    #[allow(clippy::all, dead_code)]
    pub mod peer_capnp {
        include!(concat!(env!("OUT_DIR"), "/peer/peer_capnp.rs"));
    }
    #[allow(clippy::all, dead_code)]
    pub mod counter_capnp {
        include!(concat!(env!("OUT_DIR"), "/counter/counter_capnp.rs"));
    }
}

use sap_process::{process_slot, startup_flags, Startup};
use sap_rt::abi::{SlotRange, UntypedDesc, MAX_BOOT_UNTYPED};
use sap_rt::cnode::SlotRef;
use sap_rt::sys::Error;
use sap_rt::runtime::{self, Memory, Resources};
use sap_rt::{account, println, startup};
use capnp::message::{Reader, ReaderOptions, SegmentArray};
use components::check_capnp::check_bootstrap;
use accounts::check_accounts;
use channels::{check_capnp, check_echo_service, check_ring, measure_transfer};
use cheri::check_cheri;
use processes::{check_compose, check_instances, check_log, check_process, check_restart};
use rt::check_heap;

/// Slot `i` of this process's CNode.
fn slot(i: usize) -> SlotRef {
    SlotRef::new(process_slot::CNODE, i)
}

/// What the checks work with: the startup record, the bootstrap message,
/// and chunks of untyped memory from the account, laid out the way the
/// kernel's boot information is, since the checks were written for the
/// root task.
struct Env {
    timer_hz: u64,
    /// `sap_process::startup_flags`, passed on to processes the checks start.
    flags: usize,
    /// Free slots.
    empty: SlotRange,
    asid_pool: usize,
    /// The Supervisor interface, and the hashes of the peer and the counter
    /// in the boot pack.
    supervisor: usize,
    peer_hash: [u8; 32],
    counter_hash: [u8; 32],
    /// Whether user mode may use CHERI capabilities.
    cheri: bool,
    log: usize,
    /// A service restarted when it fails.
    restartable: usize,
    /// The process's memory account.
    account: usize,
    /// Untyped memory, chunks of the account in consecutive slots,
    /// described in order in `untyped_list`.
    untyped: SlotRange,
    untyped_list: [UntypedDesc; MAX_BOOT_UNTYPED],
}

/// The chunks of untyped memory the checks use, by size, from the account:
/// each check takes the first that is large enough. The runtime takes
/// chunks of its own.
const CHUNKS: [u8; 3] = [16, 18, 20];

/// Read the bootstrap message into an `Env`, and take the chunks.
fn env(startup: &'static Startup) -> Result<Env, capnp::Error> {
    let segments = [startup::bootstrap(startup)];
    let reader = Reader::new(SegmentArray::new(&segments), ReaderOptions::new());
    let imports = reader.get_root::<check_bootstrap::Reader>()?;
    let import = |index: u32| startup::import(startup, index).expect("check: no such import");
    let first = startup.free.start;
    let mut untyped_list = [UntypedDesc { paddr: 0, size_bits: 0, is_device: 0, _pad: [0; 6] }; MAX_BOOT_UNTYPED];
    for (i, &bits) in CHUNKS.iter().enumerate() {
        let chunk = account::allocate(process_slot::ACCOUNT, bits, process_slot::CNODE, first + i).expect("check: a chunk of the account");
        untyped_list[i] = UntypedDesc { paddr: chunk.paddr as usize, size_bits: bits, is_device: 0, _pad: [0; 6] };
    }
    let untyped = SlotRange { start: first, end: first + CHUNKS.len() };
    Ok(Env {
        timer_hz: startup.timer_hz,
        flags: startup.flags,
        empty: SlotRange { start: untyped.end, end: startup.free.end },
        asid_pool: import(imports.get_asid_pool()),
        supervisor: import(imports.get_supervisor()),
        peer_hash: imports.get_peer_hash()?.try_into().expect("check: the peer's hash is 32 bytes"),
        counter_hash: imports.get_counter_hash()?.try_into().expect("check: the counter's hash is 32 bytes"),
        cheri: startup.flags & startup_flags::CHERI != 0,
        log: import(imports.get_log()),
        restartable: import(imports.get_restartable()),
        account: process_slot::ACCOUNT,
        untyped,
        untyped_list,
    })
}

/// Where the heap grows: a 1 GiB range nothing else here uses.
const HEAP: core::ops::Range<usize> = 0x08_0000_0000..0x08_4000_0000;
/// The checks hand out slots below this by hand; the runtime's slot
/// allocator has the rest.
const CHECK_SLOTS: usize = 1024;

/// The first untyped region of at least `2^bits` bytes.
fn untyped_at_least(info: &Env, bits: u8) -> usize {
    (info.untyped.start..info.untyped.end)
        .zip(&info.untyped_list)
        .find(|&(_, u)| u.size_bits >= bits && u.is_device == 0)
        .map(|(cptr, _)| cptr)
        .unwrap_or_else(|| panic!("no untyped region of 2^{bits} bytes"))
}

#[unsafe(no_mangle)]
fn sap_main(a0: usize, _: usize, _: usize) -> ! {
    // SAFETY: the loader started this process with its startup record.
    let startup = unsafe { startup::record(a0) };
    let env = env(startup).expect("check: bootstrap message");
    let info = &env;
    sap_rt::print::set_log(info.log);
    runtime::start(Resources {
        cnode: process_slot::CNODE,
        slots: info.empty.start + CHECK_SLOTS..info.empty.end,
        vspace: process_slot::VSPACE,
        memory: Memory::Account(info.account),
        heap: HEAP,
    });
    // Before anything else allocates: it checks that the heap starts empty.
    check_heap(info);
    check_process(info);
    check_ring(info);
    check_echo_service(info);
    measure_transfer(info);
    check_capnp();
    check_instances(info);
    check_compose(info);
    check_log(info);
    check_restart(info);
    check_accounts(info);
    check_cheri(info);
    println!("all checks passed");
    sap_rt::exit(0)
}

fn expect(what: &str, result: Result<(), Error>, wanted: Result<(), usize>) {
    let got = result.map_err(|e| e.0);
    if got != wanted {
        panic!("{what}: got {got:?}, wanted {wanted:?}");
    }
}
