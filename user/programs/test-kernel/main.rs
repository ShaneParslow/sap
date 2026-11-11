//! The kernel's tests, run as the root task in place of `init`
//! (`SAP_ROOT=test-kernel cargo run`). With no supervisor, memory manager
//! or log underneath, a failure here is the kernel's: the program holds
//! everything the kernel hands out at boot and talks only to it.
//!
//! Tests are grouped by area, one module each, and run in the order
//! `sap_main` calls them. A failed test panics, which halts the machine
//! with a failure; reaching the end halts it with success.

#![no_std]
#![no_main]
#![feature(strict_provenance_lints)]
// Pointers come from `sap_sys::address::mapped`, never from integer casts.
#![deny(implicit_provenance_casts, integer_to_ptr_transmutes)]

mod caps;
mod cheri;
mod endpoints;
mod faults;
mod hostile;
mod memory;
mod notifications;
mod support;
mod teardown;
mod threads;
mod transfer;

use sap_rt::abi::BootInfo;
use sap_rt::address::mapped;
use sap_rt::println;

#[unsafe(no_mangle)]
fn sap_main(boot_info: usize, _: usize, _: usize) -> ! {
    // SAFETY: the kernel passes the address of the read-only boot-info page.
    let info = unsafe { &*mapped::<BootInfo>(boot_info) };

    caps::cnode_operations(info);
    caps::retype(info);
    memory::mapping(info);
    threads::scheduling(info);
    notifications::signalling(info);
    endpoints::calls(info);
    transfer::capabilities(info);
    faults::delivery(info);
    cheri::registers(info);
    hostile::arguments(info);
    teardown::every_state(info);

    println!("all kernel tests passed");
    sap_rt::print::halt(false)
}
