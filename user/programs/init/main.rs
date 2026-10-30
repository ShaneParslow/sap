//! The root task, and the root supervisor it is becoming (M7). It enacts
//! the boot composition (`supervisor`), which today is one component:
//! `check`, the kernel checks, which gets the authority only the root task
//! holds at boot: untyped memory and the ASID pool. Then it serves the
//! process factory (`factory`), which starts programs from the boot pack
//! for its clients, until `check` exits.

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
use sap_interfaces::supervisor_capnp::{composition, OnEnd};
use sap_loader::Parent;
use sap_ring::as_bytes_mut;
use sap_rt::abi::{init_slot, object, rights, BootInfo, UntypedDesc, MAX_EXTRA_CAPS, MAX_MESSAGE_LENGTH};
use sap_rt::address::mapped;
use sap_rt::cnode::SlotRef;
use sap_rt::runtime::{self, Resources};
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
/// `check`'s memory: 2 MiB, for its 633 KiB image, its CNode and stack.
const CHECK_MEMORY_BITS: u8 = 21;

#[unsafe(no_mangle)]
fn sap_main(boot_info: usize, _: usize, _: usize) -> ! {
    // SAFETY: the kernel passes the address of the read-only boot-info page.
    let info = unsafe { &*mapped::<BootInfo>(boot_info) };
    report(info);
    let own = own_untyped(info);
    let pool = pool_untyped(info, own);
    runtime::start(Resources { cnode: init_slot::CNODE, slots: info.empty.start..info.empty.end, untyped: own, vspace: init_slot::VSPACE, heap: HEAP });

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
        if supervisor.handle(badge, label, words) {
            continue;
        }
        match factory.handle(badge, label, words, message.info.extra_caps(), reply) {
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
/// objects: the smallest of at least 2 MiB, so the large ones go to
/// `check`.
fn own_untyped(info: &BootInfo) -> usize {
    (info.untyped.start..info.untyped.end)
        .zip(&info.untyped_list)
        .filter(|(_, u)| u.is_device == 0 && u.size_bits >= 21)
        .min_by_key(|(_, u)| u.size_bits)
        .map(|(cptr, _)| cptr)
        .expect("init: no untyped region of 2 MiB")
}

/// The untyped region the supervisor carves its instances' memory from:
/// the smallest other of at least 4 MiB.
fn pool_untyped(info: &BootInfo, own: usize) -> usize {
    (info.untyped.start..info.untyped.end)
        .zip(&info.untyped_list)
        .filter(|&(cptr, u)| u.is_device == 0 && u.size_bits >= 22 && cptr != own)
        .min_by_key(|(_, u)| u.size_bits)
        .map(|(cptr, _)| cptr)
        .expect("init: no untyped region of 4 MiB for the supervisor")
}

/// The untyped regions init doesn't keep, in boot order: they go to
/// `check`.
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

/// Enact the boot composition: `check`, with a copy of the ASID pool, the
/// factory capability and every spare untyped region, moved, as its
/// imports; it ends the boot when it ends.
fn boot(info: &BootInfo, own: usize, pool: usize, supervisor: &mut Supervisor) {
    assert!(pack::find(&pack::Hash([0; 32])).is_none(), "init: the boot pack has no program of that hash");
    let count = spare_untyped(info, own, pool).count();

    // check's bootstrap message. Imports: the ASID pool, the factory, then
    // the untyped regions in boot order. On the heap: the stack is only
    // 16 KiB.
    let mut scratch = vec![0u64; 512];
    let mut allocator = SingleSegmentAllocator::new(as_bytes_mut(&mut scratch));
    let mut message = Builder::new(&mut allocator);
    let mut root = message.init_root::<check_imports::Builder>();
    root.set_asid_pool(0);
    root.set_factory(1);
    root.set_peer_hash(&pack::PEER.0);
    root.set_cheri(info.cheri != 0);
    let mut list = root.init_untyped(count as u32);
    for (i, (_, &UntypedDesc { paddr, size_bits, .. })) in spare_untyped(info, own, pool).enumerate() {
        let mut region = list.reborrow().get(i as u32);
        region.set_import(2 + i as u32);
        region.set_size_bits(size_bits);
        region.set_paddr(paddr as u64);
    }
    let segments = message.get_segments_for_output();
    assert_eq!(segments.len(), 1, "init: a bootstrap message is one segment");
    // Copied out: the allocator zeroes its buffer when the message goes.
    let bootstrap: Vec<u8> = segments[0].to_vec();
    drop(message);

    let mut message = capnp::message::Builder::new_default();
    let mut component = message.init_root::<composition::Builder>().init_components(1).get(0);
    component.set_name("check");
    component.set_program(&pack::CHECK.0);
    component.set_bootstrap(&bootstrap);
    component.set_on_end(OnEnd::EndBoot);
    let mut limits = component.reborrow().init_limits();
    limits.set_memory_bits(CHECK_MEMORY_BITS);
    limits.set_priority(255);
    limits.set_max_priority(255);
    limits.set_cnode_bits(CHECK_CNODE_BITS);
    limits.set_stack_pages(CHECK_STACK_PAGES);
    let mut imports = component.init_imports(2 + count as u32);
    imports.reborrow().get(0).set_held("asidPool");
    imports.reborrow().get(1).set_held("factory");
    imports.reborrow().get(1).set_move(true);
    for i in 0..count {
        let mut route = imports.reborrow().get(2 + i as u32);
        route.set_held(alloc::format!("untyped.{i}").as_str());
        route.set_move(true);
    }
    let composition = message.get_root_as_reader::<composition::Reader>().expect("init: the boot composition");
    supervisor.enact(composition).expect("init: enact the boot composition");
}
