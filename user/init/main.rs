//! The root task, and the process manager it is becoming (M7). It starts
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

use alloc::vec;
use alloc::vec::Vec;

use capnp::message::{Builder, SingleSegmentAllocator};
use sap_interfaces::check_capnp::check_imports;
use factory::{Answer, Factory};
use sap_loader::{self as loader, Give, Launch, Parent, Slots, LAUNCH};
use sap_ring::as_bytes_mut;
use sap_rt::abi::{fault, init_slot, launcher, object, rights, BootInfo, UntypedDesc, MAX_EXTRA_CAPS, MAX_MESSAGE_LENGTH};
use sap_rt::address::mapped;
use sap_rt::cnode::SlotRef;
use sap_rt::runtime::{self, Resources};
use sap_rt::{debug, ipc, println, slots, tcb, untyped};

/// Where init's heap grows.
const HEAP: core::ops::Range<usize> = 0x08_0000_0000..0x08_4000_0000;
/// Where the loader fills pages for the processes init starts.
const SCRATCH: usize = 0x38_0000_0000;
/// The badge on the endpoint capability `check` reports to.
const CHECK_BADGE: usize = 1;
/// log2 of `check`'s CNode size: as large as the root task's, since the
/// checks were written for it.
const CHECK_CNODE_RADIX: usize = 12;
/// `check`'s stack: 64 KiB. Its checks ran on the root task's 16 KiB, but
/// it also keeps its imports' description on the stack.
const CHECK_STACK_PAGES: usize = 16;

#[unsafe(no_mangle)]
fn sap_main(boot_info: usize, _: usize, _: usize) -> ! {
    // SAFETY: the kernel passes the address of the read-only boot-info page.
    let info = unsafe { &*mapped::<BootInfo>(boot_info) };
    report(info);
    let own = own_untyped(info);
    runtime::start(Resources { cnode: init_slot::CNODE, slots: info.empty.start..info.empty.end, untyped: own, vspace: init_slot::VSPACE, heap: HEAP });

    // One endpoint for everything: `check` reports its exit and faults
    // here, and the factory's clients and processes call it, each through
    // a capability with its own badge.
    let parent = Parent {
        cnode: init_slot::CNODE,
        vspace: init_slot::VSPACE,
        asid_pool: init_slot::ASID_POOL,
        authority: init_slot::TCB,
        untyped: own,
        scratch: SCRATCH,
        timer_hz: info.timer_hz,
    };
    let (ep, mut reply, check_launcher, factory_cap) = (new_object(own, object::ENDPOINT), new_object(own, object::REPLY), slot(), slot());
    let endpoint = SlotRef::new(init_slot::CNODE, ep);
    SlotRef::new(init_slot::CNODE, check_launcher).mint_from(endpoint, rights::WRITE | rights::GRANT, CHECK_BADGE).expect("init: badge");
    let factory_rights = rights::WRITE | rights::GRANT | rights::GRANT_REPLY;
    SlotRef::new(init_slot::CNODE, factory_cap).mint_from(endpoint, factory_rights, factory::FACTORY_BADGE).expect("init: badge");
    let mut factory = Factory::new(ep, own, parent);
    start_check(info, &parent, check_launcher, factory_cap);

    let mut words = [0usize; MAX_MESSAGE_LENGTH];
    loop {
        ipc::set_receive_window(init_slot::CNODE, factory.window, MAX_EXTRA_CAPS);
        let message = ipc::recv(ep, reply, &mut words).expect("init: receive");
        let (badge, label) = (message.badge, message.info.label());
        let words = &words[..message.info.length()];
        if badge != CHECK_BADGE {
            match factory.handle(badge, label, words, message.info.extra_caps(), reply) {
                Some(Answer::Reply(label, words, caps)) => {
                    let _ = ipc::reply(reply, label, &words, &caps);
                }
                Some(Answer::Later) => reply = factory.fresh_reply(),
                Some(Answer::Nothing) => {}
                None => {
                    println!("init: ignored a message with badge {badge:#x}, label {label}");
                    let _ = ipc::reply(reply, sap_rt::abi::factory::BAD_REQUEST, &[], &[]);
                }
            }
            factory.clear_window();
            continue;
        }
        match label {
            launcher::EXIT if words.first() == Some(&0) => {
                println!("init: check exited successfully");
                debug::halt(false)
            }
            launcher::EXIT => {
                println!("init: check exited with code {:?}", words.first());
                debug::halt(true)
            }
            fault::VM | fault::USER_EXCEPTION if words.len() >= fault::LENGTH => {
                println!("init: check faulted at pc {:#x}, address {:#x}, cause {}", words[fault::PC], words[fault::ADDRESS], words[fault::CAUSE]);
                debug::halt(true)
            }
            _ => println!("init: ignored a message from check with label {label}"),
        }
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

/// The untyped region init keeps for itself, for its heap and the
/// processes it builds: the smallest of at least 2 MiB, so the large ones
/// go to `check`.
fn own_untyped(info: &BootInfo) -> usize {
    (info.untyped.start..info.untyped.end)
        .zip(&info.untyped_list)
        .filter(|(_, u)| u.is_device == 0 && u.size_bits >= 21)
        .min_by_key(|(_, u)| u.size_bits)
        .map(|(cptr, _)| cptr)
        .expect("init: no untyped region of 2 MiB")
}

fn slot() -> usize {
    slots::alloc().expect("init: out of slots")
}

fn new_object(from: usize, object_type: usize) -> usize {
    let at = slot();
    untyped::retype(from, object_type, 0, SlotRef { root: init_slot::CNODE, index: 0, depth: 0 }, at, 1).expect("init: retype");
    at
}

/// Start `check` from the boot pack, built from init's own untyped memory,
/// with every other untyped region moved to it, a copy of the ASID pool and
/// `factory`, and `launcher` as its launcher and fault handler.
fn start_check(info: &BootInfo, parent: &Parent, launcher: usize, factory: usize) {
    let own = parent.untyped;
    let image = pack::find(&pack::CHECK).expect("init: the boot pack's check doesn't match its hash");
    assert!(pack::find(&pack::Hash([0; 32])).is_none(), "init: the boot pack has no program of that hash");

    // Imports: the ASID pool, the factory, then the untyped regions in boot
    // order.
    let regions = || (info.untyped.start..info.untyped.end).zip(&info.untyped_list).filter(|&(cptr, u)| u.is_device == 0 && cptr != own);
    let mut imports = vec![(init_slot::ASID_POOL, Give::Copy(rights::ALL)), (factory, Give::Move)];
    imports.extend(regions().map(|(cptr, _)| (cptr, Give::Move)));
    let count = imports.len() - 2;

    // On the heap: the stack is only 16 KiB.
    let mut scratch = vec![0u64; 512];
    let mut allocator = SingleSegmentAllocator::new(as_bytes_mut(&mut scratch));
    let mut message = Builder::new(&mut allocator);
    let mut root = message.init_root::<check_imports::Builder>();
    root.set_asid_pool(0);
    root.set_factory(1);
    root.set_peer_hash(&pack::PEER.0);
    let mut list = root.init_untyped(count as u32);
    for (i, (_, &UntypedDesc { paddr, size_bits, .. })) in regions().enumerate() {
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

    let first = slots::alloc_range(256).expect("init: slots for check");
    let mut slots = Slots { next: first, end: first + 256 };
    let launch = Launch {
        priority: 255,
        max_priority: 255,
        cnode_radix: CHECK_CNODE_RADIX,
        stack_pages: CHECK_STACK_PAGES,
        launcher: Some(launcher),
        imports: &imports,
        bootstrap: &bootstrap,
        ..LAUNCH
    };
    let check = loader::load(image, parent, &mut slots, &launch).expect("init: load check");
    tcb::set_fault_handler(check.tcb, Some(launcher)).expect("init: fault handler");
    println!("init: starting check ({} KiB image, {} untyped regions)", image.len() / 1024, count);
    check.start().expect("init: start check");
}
