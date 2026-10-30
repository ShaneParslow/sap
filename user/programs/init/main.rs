//! The root task, and the root supervisor (M7). It enacts the base
//! composition (`supervisor`): the memory manager, which gets the machine's
//! RAM, and the resolver, which decides at runtime what else runs, from
//! the boot request (`SAP_BOOT`, `boot/` at the top of the repository) and
//! the programs' manifests, and adds it through `Compositions`. Then it
//! serves its instances until one ends the boot.

#![no_std]
#![no_main]
#![feature(strict_provenance_lints)]
// Pointers come from `sap_sys::address::mapped`, never from integer casts.
#![deny(implicit_provenance_casts, integer_to_ptr_transmutes)]

extern crate alloc;

mod pack;
mod supervisor;

use alloc::vec;
use alloc::vec::Vec;

use capnp::message::{Builder, SingleSegmentAllocator};
use sap_interfaces::memory_capnp::memory_imports;
use sap_interfaces::sap_capnp::{manifest, resolver_imports};
use sap_interfaces::words;
use sap_interfaces::supervisor_capnp::{composition, OnEnd, Status};
use sap_loader::Parent;
use sap_process::startup_flags;
use sap_ring::as_bytes_mut;
use sap_rt::abi::{init_slot, object, BootInfo, UntypedDesc, MAX_EXTRA_CAPS, MAX_MESSAGE_LENGTH};
use sap_rt::address::mapped;
use sap_rt::cnode::SlotRef;
use sap_rt::runtime::{self, Memory, Resources};
use sap_rt::{ipc, println, slots, untyped};
use supervisor::{Answer, Supervisor};

/// Where init's heap grows.
const HEAP: core::ops::Range<usize> = 0x08_0000_0000..0x08_4000_0000;
/// Where the loader fills pages for the processes init starts.
const SCRATCH: usize = 0x38_0000_0000;
#[unsafe(no_mangle)]
fn sap_main(boot_info: usize, _: usize, _: usize) -> ! {
    // SAFETY: the kernel passes the address of the read-only boot-info page.
    let info = unsafe { &*mapped::<BootInfo>(boot_info) };
    report(info);
    let own = own_untyped(info);
    let pool = pool_untyped(info, own);
    runtime::start(Resources { cnode: init_slot::CNODE, slots: info.empty.start..info.empty.end, memory: Memory::Untyped(own), vspace: init_slot::VSPACE, heap: HEAP });

    // One endpoint for everything: the supervisor's instances report their
    // exits and faults here, and call the instance API and their handles,
    // each through a capability with its own badge.
    let parent = Parent {
        cnode: init_slot::CNODE,
        vspace: init_slot::VSPACE,
        asid_pool: init_slot::ASID_POOL,
        authority: init_slot::TCB,
        untyped: own,
        scratch: SCRATCH,
        timer_hz: info.timer_hz,
        flags: if info.cheri != 0 { startup_flags::CHERI } else { 0 },
    };
    let (ep, mut reply) = (new_object(own, object::ENDPOINT), new_object(own, object::REPLY));
    let mut supervisor = Supervisor::new(ep, pool, parent);
    supervisor.hold("asidPool", init_slot::ASID_POOL);
    supervisor.hold_group("ram", spare_untyped(info, own, pool).map(|(cptr, _)| cptr).collect());
    boot(&mut supervisor);

    let mut words = [0usize; MAX_MESSAGE_LENGTH];
    loop {
        ipc::set_receive_window(init_slot::CNODE, supervisor.window, MAX_EXTRA_CAPS);
        let message = ipc::recv(ep, reply, &mut words).expect("init: receive");
        let (badge, label) = (message.badge, message.info.label());
        let words = &words[..message.info.length()];
        match supervisor.handle(badge, label, words, message.info.extra_caps(), reply) {
            Some(Answer::Reply(label, words, caps)) => {
                let _ = ipc::reply(reply, label, &words, &caps);
            }
            Some(Answer::Later) => reply = supervisor.fresh_reply(),
            Some(Answer::Nothing) => {}
            None => {
                println!("init: ignored a message with badge {badge:#x}, label {label}");
                let _ = ipc::reply(reply, Status::BadRequest as usize, &[], &[]);
            }
        }
        supervisor.clear_window();
    }
}

fn report(info: &BootInfo) {
    for program in pack::PROGRAMS {
        let segment = [words::bytes(program.manifest_words())];
        let message = words::reader(&segment);
        let manifest = message.get_root::<manifest::Reader>().expect("init: a manifest");
        let (imports, exports) = (manifest.get_imports().map_or(0, |l| l.len()), manifest.get_exports().map_or(0, |l| l.len()));
        println!("init: boot pack: {} {} ({imports} imports, {exports} exports declared)", program.name, program.hash);
    }
    let untyped = &info.untyped_list[..info.untyped.len()];
    let bytes: usize = untyped.iter().map(|u| 1usize << u.size_bits).sum();
    let largest = untyped.iter().map(|u| u.size_bits).max().unwrap_or(0);
    println!(
        "init: {} image frames, {} page tables, {} untyped ({} MiB, largest 2^{}), slots {}..{} free",
        info.image_frames.len(),
        info.image_paging.len(),
        untyped.len(),
        bytes >> 20,
        largest,
        info.empty.start,
        info.empty.end
    );
}

/// The untyped region init keeps for itself, for its heap and its own
/// objects: the smallest of at least 2 MiB, so the large ones go to the
/// memory manager.
fn own_untyped(info: &BootInfo) -> usize {
    (info.untyped.start..info.untyped.end)
        .zip(&info.untyped_list)
        .filter(|(_, u)| u.is_device == 0 && u.size_bits >= 21)
        .min_by_key(|(_, u)| u.size_bits)
        .map(|(cptr, _)| cptr)
        .expect("init: no untyped region of 2 MiB")
}

/// The untyped region the supervisor carves regions from (the memory
/// manager's): the smallest other of at least 4 MiB.
fn pool_untyped(info: &BootInfo, own: usize) -> usize {
    (info.untyped.start..info.untyped.end)
        .zip(&info.untyped_list)
        .filter(|&(cptr, u)| u.is_device == 0 && u.size_bits >= 22 && cptr != own)
        .min_by_key(|(_, u)| u.size_bits)
        .map(|(cptr, _)| cptr)
        .expect("init: no untyped region of 4 MiB for the supervisor")
}

/// The untyped regions init doesn't keep, in boot order: the memory
/// manager's RAM.
fn spare_untyped(info: &BootInfo, own: usize, pool: usize) -> impl Iterator<Item = (usize, &UntypedDesc)> {
    (info.untyped.start..info.untyped.end).zip(&info.untyped_list).filter(move |&(cptr, u)| u.is_device == 0 && cptr != own && cptr != pool)
}

fn slot() -> usize {
    slots::alloc().expect("init: out of slots")
}

fn new_object(from: usize, object_type: usize) -> usize {
    let at = slot();
    untyped::retype(from, object_type, 0, SlotRef { root: init_slot::CNODE, index: 0, depth: 0 }, at, 1).expect("init: retype");
    at
}

/// A bootstrap message: a single-segment message built by `build`. On the
/// heap: the stack is only 16 KiB.
fn bootstrap<T: capnp::traits::Owned>(build: impl FnOnce(T::Builder<'_>)) -> Vec<u8> {
    let mut scratch = vec![0u64; 512];
    let mut allocator = SingleSegmentAllocator::new(as_bytes_mut(&mut scratch));
    let mut message = Builder::new(&mut allocator);
    build(message.init_root::<T::Builder<'_>>());
    let segments = message.get_segments_for_output();
    assert_eq!(segments.len(), 1, "init: a bootstrap message is one segment");
    // Copied out: the allocator zeroes its buffer when the message goes.
    segments[0].to_vec()
}

/// Enact the base composition, which init knows itself: the memory
/// manager, with copies of every spare untyped region as its RAM (the `ram`
/// group), serving accounts; and the resolver, which adds whatever the boot
/// request (`SAP_BOOT`) wants through `Compositions`, with the request's
/// grants, which init gives the supervisor. Each one's limits and exports
/// are its manifest's.
fn boot(supervisor: &mut Supervisor) {
    let memory = bootstrap::<memory_imports::Owned>(|mut root| {
        root.set_endpoint(0);
        root.set_ram(1);
    });
    let resolver = sap_ring::as_bytes(&pack::RESOLVER_BOOTSTRAP);
    {
        let segment = [resolver];
        let message = capnp::message::Reader::new(capnp::message::SegmentArray::new(&segment), capnp::message::ReaderOptions::new());
        let request = message.get_root::<resolver_imports::Reader>().and_then(|r| r.get_request()).expect("init: the boot request");
        for grant in request.get_grants().expect("init: the boot request's grants") {
            let (component, held) = (grant.get_component().ok().and_then(|t| t.to_str().ok()), grant.get_held().ok().and_then(|t| t.to_str().ok()));
            supervisor.grant(component.expect("init: a grant"), held.expect("init: a grant"));
        }
    }
    let compositions = supervisor.compositions();
    supervisor.hold("compositions", compositions);

    // One segment, as every message is without capnp's `alloc` feature:
    // room for the whole composition, on the heap.
    let mut space = vec![0u64; 2048 + resolver.len() / 8];
    let mut allocator = SingleSegmentAllocator::new(as_bytes_mut(&mut space));
    let mut message = Builder::new(&mut allocator);
    let mut root = message.init_root::<composition::Builder>();
    let mut accounts = root.reborrow().init_accounts();
    accounts.set_component("memory");
    accounts.set_name("account");
    let mut components = root.init_components(2);
    for (i, (name, program, bootstrap, on_end)) in
        [("memory", &pack::MEMORY, &memory[..], OnEnd::EndBoot), ("resolver", &pack::RESOLVER, resolver, OnEnd::Forget)].into_iter().enumerate()
    {
        let entry = pack::PROGRAMS.iter().find(|p| p.hash == *program).expect("init: a base program");
        let segment = [words::bytes(entry.manifest_words())];
        let declared = words::reader(&segment);
        let declared = declared.get_root::<manifest::Reader>().expect("init: a manifest");
        let mut component = components.reborrow().get(i as u32);
        component.set_name(name);
        component.set_program(&program.0);
        component.set_bootstrap(bootstrap);
        component.set_on_end(on_end);
        component.set_limits(declared.get_limits().expect("init: limits")).expect("init: limits");
        let exports = declared.get_exports().expect("init: exports");
        let mut list = component.reborrow().init_exports(exports.len());
        for (j, e) in exports.iter().enumerate() {
            let mut export = list.reborrow().get(j as u32);
            export.set_name(e.get_name().expect("init: an export"));
            export.set_interface(e.get_interface());
        }
        let mut imports = component.init_imports(if name == "memory" { 2 } else { 1 });
        if name == "memory" {
            imports.reborrow().get(0).set_serve("account");
            // Copied, never moved: the supervisor keeps the right to revoke
            // them.
            imports.get(1).init_held().set_name("ram");
        } else {
            imports.get(0).init_held().set_name("compositions");
        }
    }
    let composition = message.get_root_as_reader::<composition::Reader>().expect("init: the base composition");
    supervisor.enact(composition).expect("init: enact the base composition");
}
