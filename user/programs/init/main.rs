//! The root task, and the root supervisor it is becoming (M7). It enacts
//! the boot composition (`supervisor`): the memory manager, which gets the
//! machine's RAM; the log service; and `check`, the kernel checks, which
//! gets the ASID pool, the one other authority only the root task holds at
//! boot. Then it serves the process factory (`factory`), which starts
//! programs from the boot pack for its clients, until `check` exits.

#![no_std]
#![no_main]
#![feature(strict_provenance_lints)]
// Pointers come from `sap_sys::address::mapped`, never from integer casts.
#![deny(implicit_provenance_casts, integer_to_ptr_transmutes)]

extern crate alloc;

mod factory;
mod pack;
mod supervisor;

use alloc::vec;
use alloc::vec::Vec;

use capnp::message::{Builder, SingleSegmentAllocator};
use factory::{Answer, Factory};
use sap_interfaces::check_capnp::check_imports;
use sap_interfaces::log_capnp::log_imports;
use sap_interfaces::memory_capnp::memory_imports;
use sap_interfaces::supervisor_capnp::{composition, OnEnd};
use sap_loader::Parent;
use sap_ring::as_bytes_mut;
use sap_rt::abi::{init_slot, object, rights, BootInfo, UntypedDesc, MAX_EXTRA_CAPS, MAX_MESSAGE_LENGTH};
use sap_rt::address::mapped;
use sap_rt::cnode::SlotRef;
use sap_rt::runtime::{self, Memory, Resources};
use sap_rt::{ipc, println, slots, untyped};
use supervisor::Supervisor;

/// Where init's heap grows.
const HEAP: core::ops::Range<usize> = 0x08_0000_0000..0x08_4000_0000;
/// Where the loader fills pages for the processes init starts.
const SCRATCH: usize = 0x38_0000_0000;
/// log2 of `check`'s CNode size: as large as the root task's, since the
/// checks were written for it.
const CHECK_CNODE_BITS: u8 = 12;
/// `check`'s stack: 64 KiB. Its checks ran on the root task's 16 KiB, but
/// it also keeps its imports' description on the stack.
const CHECK_STACK_PAGES: u16 = 16;
/// The memory manager's region of the pool, and its budget, for its heap
/// and reply object.
const MEMORY_BITS: u8 = 22;
const MEMORY_BUDGET_BITS: u8 = 20;
/// The log service's account: built from 1 MiB, with a page to spare for
/// its reply object.
const LOG_PROCESS_BITS: u8 = 20;
const LOG_ACCOUNT: u64 = 2 << 20;
/// `check`'s account: built from 2 MiB (its 633 KiB image, CNode and
/// stack); the rest for its heap and the untyped memory its checks use.
const CHECK_PROCESS_BITS: u8 = 21;
const CHECK_RESERVATION: u64 = 32 << 20;
const CHECK_LIMIT: u64 = 48 << 20;

#[unsafe(no_mangle)]
fn sap_main(boot_info: usize, _: usize, _: usize) -> ! {
    // SAFETY: the kernel passes the address of the read-only boot-info page.
    let info = unsafe { &*mapped::<BootInfo>(boot_info) };
    report(info);
    let own = own_untyped(info);
    let pool = pool_untyped(info, own);
    runtime::start(Resources { cnode: init_slot::CNODE, slots: info.empty.start..info.empty.end, memory: Memory::Untyped(own), vspace: init_slot::VSPACE, heap: HEAP });

    // One endpoint for everything: the supervisor's instances report their
    // exits and faults here, and the factory's clients and processes call
    // it, each through a capability with its own badge.
    let parent = Parent {
        cnode: init_slot::CNODE,
        vspace: init_slot::VSPACE,
        asid_pool: init_slot::ASID_POOL,
        authority: init_slot::TCB,
        untyped: own,
        scratch: SCRATCH,
        timer_hz: info.timer_hz,
    };
    let (ep, mut reply, factory_cap) = (new_object(own, object::ENDPOINT), new_object(own, object::REPLY), slot());
    let endpoint = SlotRef::new(init_slot::CNODE, ep);
    let factory_rights = rights::WRITE | rights::GRANT | rights::GRANT_REPLY;
    SlotRef::new(init_slot::CNODE, factory_cap).mint_from(endpoint, factory_rights, factory::FACTORY_BADGE).expect("init: badge");
    let mut factory = Factory::new(ep, own, parent);
    let mut supervisor = Supervisor::new(ep, pool, parent);
    supervisor.hold("asidPool", init_slot::ASID_POOL);
    supervisor.hold("factory", factory_cap);
    for (i, (cptr, _)) in spare_untyped(info, own, pool).enumerate() {
        supervisor.hold(alloc::format!("untyped.{i}"), cptr);
    }
    boot(info, own, pool, &mut supervisor);

    let mut words = [0usize; MAX_MESSAGE_LENGTH];
    loop {
        ipc::set_receive_window(init_slot::CNODE, factory.window, MAX_EXTRA_CAPS);
        let message = ipc::recv(ep, reply, &mut words).expect("init: receive");
        let (badge, label) = (message.badge, message.info.label());
        let words = &words[..message.info.length()];
        let answer = match supervisor.handle(badge, label, words) {
            Some(answer) => Some(answer),
            None => factory.handle(badge, label, words, message.info.extra_caps(), reply),
        };
        match answer {
            Some(Answer::Reply(label, words, caps)) => {
                let _ = ipc::reply(reply, label, &words, &caps);
            }
            Some(Answer::Later) => reply = factory.fresh_reply(),
            Some(Answer::Nothing) => {}
            None => {
                println!("init: ignored a message with badge {badge:#x}, label {label}");
                let _ = ipc::reply(reply, sap_interfaces::process_capnp::Status::BadRequest as usize, &[], &[]);
            }
        }
        factory.clear_window();
    }
}

fn report(info: &BootInfo) {
    for program in pack::PROGRAMS {
        println!("init: boot pack: {} {}", program.name, program.hash);
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

/// Enact the boot composition: the memory manager, with copies of every
/// spare untyped region as its RAM, serving accounts; the log service,
/// serving its export; then `check`, with a copy of the ASID pool, the
/// factory capability, moved, and the log as its imports. The log and
/// `check` get accounts. Any of them ends the boot when it ends.
fn boot(info: &BootInfo, own: usize, pool: usize, supervisor: &mut Supervisor) {
    let count = spare_untyped(info, own, pool).count();

    // The memory manager's bootstrap message. Imports: its export, then
    // the untyped regions in boot order.
    let memory = bootstrap::<memory_imports::Owned>(|mut root| {
        root.set_endpoint(0);
        let mut list = root.init_ram(count as u32);
        for (i, (_, &UntypedDesc { paddr, size_bits, .. })) in spare_untyped(info, own, pool).enumerate() {
            let mut region = list.reborrow().get(i as u32);
            region.set_import(1 + i as u32);
            region.set_size_bits(size_bits);
            region.set_paddr(paddr as u64);
        }
    });
    let log = bootstrap::<log_imports::Owned>(|mut root| root.set_endpoint(0));
    // Imports: the ASID pool, the factory, the log.
    let check = bootstrap::<check_imports::Owned>(|mut root| {
        root.set_asid_pool(0);
        root.set_factory(1);
        root.set_log(2);
        root.set_peer_hash(&pack::PEER.0);
        root.set_cheri(info.cheri != 0);
    });

    // One segment, as every message is without capnp's `alloc` feature:
    // room for the whole composition, on the heap.
    let mut space = vec![0u64; 2048];
    let mut allocator = SingleSegmentAllocator::new(as_bytes_mut(&mut space));
    let mut message = Builder::new(&mut allocator);
    let mut root = message.init_root::<composition::Builder>();
    let mut accounts = root.reborrow().init_accounts();
    accounts.set_component("memory");
    accounts.set_name("account");
    let mut components = root.init_components(3);

    let mut component = components.reborrow().get(0);
    component.set_name("memory");
    component.set_program(&pack::MEMORY.0);
    component.set_bootstrap(&memory);
    component.set_on_end(OnEnd::EndBoot);
    component.reborrow().init_exports(1).get(0).set_name("account");
    let mut limits = component.reborrow().init_limits();
    let mut region = limits.reborrow().init_memory().init_region();
    region.set_bits(MEMORY_BITS);
    region.set_budget_bits(MEMORY_BUDGET_BITS);
    limits.set_priority(255);
    limits.set_cnode_bits(12);
    limits.set_stack_pages(4);
    let mut imports = component.init_imports(1 + count as u32);
    imports.reborrow().get(0).set_serve("account");
    // Copied, never moved: the supervisor keeps the right to revoke them.
    for i in 0..count {
        imports.reborrow().get(1 + i as u32).init_held().set_name(alloc::format!("untyped.{i}").as_str());
    }

    let mut component = components.reborrow().get(1);
    component.set_name("log");
    component.set_program(&pack::LOG.0);
    component.set_bootstrap(&log);
    component.set_on_end(OnEnd::EndBoot);
    component.reborrow().init_exports(1).get(0).set_name("log");
    let mut limits = component.reborrow().init_limits();
    let mut terms = limits.reborrow().init_memory().init_account();
    terms.set_reservation(LOG_ACCOUNT);
    terms.set_limit(LOG_ACCOUNT);
    terms.set_process_bits(LOG_PROCESS_BITS);
    limits.set_priority(255);
    limits.set_cnode_bits(6);
    limits.set_stack_pages(4);
    component.init_imports(1).get(0).set_serve("log");

    let mut component = components.get(2);
    component.set_name("check");
    component.set_program(&pack::CHECK.0);
    component.set_bootstrap(&check);
    component.set_on_end(OnEnd::EndBoot);
    let mut limits = component.reborrow().init_limits();
    let mut terms = limits.reborrow().init_memory().init_account();
    terms.set_reservation(CHECK_RESERVATION);
    terms.set_limit(CHECK_LIMIT);
    terms.set_process_bits(CHECK_PROCESS_BITS);
    limits.set_priority(255);
    limits.set_max_priority(255);
    limits.set_cnode_bits(CHECK_CNODE_BITS);
    limits.set_stack_pages(CHECK_STACK_PAGES);
    let mut imports = component.init_imports(3);
    imports.reborrow().get(0).init_held().set_name("asidPool");
    let mut factory = imports.reborrow().get(1).init_held();
    factory.set_name("factory");
    factory.set_move(true);
    let mut log = imports.get(2).init_export();
    log.set_component("log");
    log.set_name("log");
    let composition = message.get_root_as_reader::<composition::Reader>().expect("init: the boot composition");
    supervisor.enact(composition).expect("init: enact the boot composition");
}
