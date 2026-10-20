//! System calls and kernel-object invocations for sap programs: thin
//! wrappers over the kernel interface that `sap-abi` defines, as libsel4 is
//! to seL4. No allocation and no global state. The one convention it relies
//! on is that `tp` holds the thread's IPC buffer (`sys::ipc_buffer`).
//!
//! TODO: generate the invocation wrappers from the Cap'n Proto schema of the
//! kernel's objects, with `sap-abi` (roadmap D5).

#![no_std]

pub mod cnode;
pub mod ipc;
pub mod notification;
pub mod sys;
pub mod tcb;
pub mod untyped;
pub mod vspace;

pub use sap_abi as abi;
