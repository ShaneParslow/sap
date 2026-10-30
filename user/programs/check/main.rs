//! The kernel checks: the program the root supervisor starts at boot to
//! check the kernel's features as they arrive, and the services' too. Its
//! bootstrap message (`CheckImports` in `interfaces/check.capnp`) gives it
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
mod caps;
mod memory;
mod threads;
mod kernel_ipc;
mod processes;
mod rt;
mod channels;
mod cheri;

use sap_process::{process_slot, Startup};
use sap_rt::abi::{SlotRange, UntypedDesc, MAX_BOOT_UNTYPED};
use sap_rt::cnode::SlotRef;
use sap_rt::sys::Error;
use sap_rt::runtime::{self, Resources};
use sap_rt::{account, println, process};
use capnp::message::{Reader, ReaderOptions, SegmentArray};
use sap_interfaces::check_capnp::check_imports;
use accounts::check_accounts;
use caps::{check_cnode_operations, check_retype};
use channels::{check_capnp, check_echo_service, check_ring, measure_transfer};
use cheri::check_cheri;
use kernel_ipc::{check_cap_transfer, check_faults, check_ipc, check_notifications};
use memory::check_mapping;
use processes::{check_factory, check_log, check_process};
use rt::check_heap;
use threads::check_threads;

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
    cnode_radix: usize,
    /// Free slots.
    empty: SlotRange,
    asid_pool: usize,
    /// The process factory, and the hash of the peer in its boot pack.
    factory: usize,
    peer_hash: [u8; 32],
    /// Whether user mode may use CHERI capabilities.
    cheri: bool,
    log: usize,
    /// The process's memory account.
    account: usize,
    /// Untyped memory, chunks of the account in consecutive slots,
    /// described in order in `untyped_list`.
    untyped: SlotRange,
    untyped_list: [UntypedDesc; MAX_BOOT_UNTYPED],
}

/// The chunks of untyped memory the checks use, by size, from the account:
/// the first is revoked by `check_cnode_operations`, the largest is the
/// runtime's, and each check takes the first that is large enough.
const CHUNKS: [u8; 5] = [16, 18, 20, 22, 23];

/// Read the bootstrap message into an `Env`, and take the chunks.
fn env(startup: &'static Startup) -> Result<Env, capnp::Error> {
    let segments = [process::bootstrap(startup)];
    let reader = Reader::new(SegmentArray::new(&segments), ReaderOptions::new());
    let imports = reader.get_root::<check_imports::Reader>()?;
    let import = |index: u32| process::import(startup, index).expect("check: no such import");
    let first = startup.free.start;
    let mut untyped_list = [UntypedDesc { paddr: 0, size_bits: 0, is_device: 0, _pad: [0; 6] }; MAX_BOOT_UNTYPED];
    for (i, &bits) in CHUNKS.iter().enumerate() {
        let chunk = account::allocate(process_slot::ACCOUNT, bits, process_slot::CNODE, first + i).expect("check: a chunk of the account");
        untyped_list[i] = UntypedDesc { paddr: chunk.paddr as usize, size_bits: bits, is_device: 0, _pad: [0; 6] };
    }
    let untyped = SlotRange { start: first, end: first + CHUNKS.len() };
    Ok(Env {
        timer_hz: startup.timer_hz,
        cnode_radix: startup.cnode_radix,
        empty: SlotRange { start: untyped.end, end: startup.free.end },
        asid_pool: import(imports.get_asid_pool()),
        factory: import(imports.get_factory()),
        peer_hash: imports.get_peer_hash()?.try_into().expect("check: the peer's hash is 32 bytes"),
        cheri: imports.get_cheri(),
        log: import(imports.get_log()),
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

/// The untyped region the runtime gets for the heap: the largest, but never
/// the first, which `check_cnode_operations` revokes. No check uses it.
fn runtime_untyped(info: &Env) -> usize {
    let list = &info.untyped_list[1..info.untyped.len()];
    let (i, _) = list.iter().enumerate().filter(|(_, u)| u.is_device == 0).max_by_key(|(_, u)| u.size_bits).expect("no untyped memory for the heap");
    info.untyped.start + 1 + i
}

/// The first untyped region of at least `2^bits` bytes, other than the
/// runtime's.
fn untyped_at_least(info: &Env, bits: u8) -> usize {
    let runtime = runtime_untyped(info);
    (info.untyped.start..info.untyped.end)
        .zip(&info.untyped_list)
        .find(|&(cptr, u)| u.size_bits >= bits && u.is_device == 0 && cptr != runtime)
        .map(|(cptr, _)| cptr)
        .unwrap_or_else(|| panic!("no untyped region of 2^{bits} bytes"))
}

#[unsafe(no_mangle)]
fn sap_main(a0: usize, _: usize, _: usize) -> ! {
    // SAFETY: the loader started this process with its startup record.
    let startup = unsafe { process::startup(a0) };
    let env = env(startup).expect("check: bootstrap message");
    let info = &env;
    sap_rt::debug::set_log(info.log);
    runtime::start(Resources {
        cnode: process_slot::CNODE,
        slots: info.empty.start + CHECK_SLOTS..info.empty.end,
        vspace: process_slot::VSPACE,
        untyped: runtime_untyped(info),
        heap: HEAP,
    });
    check_cnode_operations(info);
    check_retype(info);
    check_mapping(info);
    check_threads(info);
    check_notifications(info);
    check_ipc(info);
    check_cap_transfer(info);
    check_faults(info);
    // Before anything else allocates: it checks that the heap starts empty.
    check_heap(info);
    check_process(info);
    check_ring(info);
    check_echo_service(info);
    measure_transfer(info);
    check_capnp();
    check_factory(info);
    check_log(info);
    check_accounts(info);
    check_cheri(info);
    println!("all checks passed");
    process::exit(0)
}

fn expect(what: &str, result: Result<(), Error>, wanted: Result<(), usize>) {
    let got = result.map_err(|e| e.0);
    if got != wanted {
        panic!("{what}: got {got:?}, wanted {wanted:?}");
    }
}
