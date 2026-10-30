//! Rust bindings for sap's Cap'n Proto interfaces, generated at build time
//! from the schemas in interfaces/ at the top of the repository.
//!
//! Schemas that declare interfaces (`process_capnp`) need the `alloc`
//! feature: capnpc's code for an interface uses the heap. A program without
//! a heap gets only the plain structs.

#![no_std]
#![feature(strict_provenance_lints)]
// Pointers come from `sap_sys::address::mapped`, never from integer casts.
#![deny(implicit_provenance_casts, integer_to_ptr_transmutes)]

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod words;

/// The method ordinals of every interface in the schemas, generated: the
/// labels of calls over sap's IPC (`process_capnp` describes the layout).
pub mod methods {
    include!(concat!(env!("OUT_DIR"), "/methods.rs"));
}

/// Record kinds on an echo channel (`sap-ring`); the record id matches a
/// reply to its request.
pub mod echo {
    /// Raw words, echoed with each one incremented.
    pub const WORDS: u32 = 1;
    /// An `echo_capnp::Request`.
    pub const REQUEST: u32 = 2;
    /// An `echo_capnp::Reply`.
    pub const REPLY: u32 = 3;
    /// The request couldn't be read; no body.
    pub const ERROR: u32 = 4;
    /// End of the conversation; echoed, then the peer stops.
    pub const STOP: u32 = 5;
    /// An `echo_capnp::Request` too large for the ring follows by page
    /// transfer, as a call on the peer's endpoint with this label (the
    /// `sap_ring::transfer` protocol); no body. Its reply comes as a record.
    pub const TRANSFER: u32 = 6;
    /// Bytes to sum (the measurement peer); a `SUM` record answers.
    pub const BYTES: u32 = 7;
    /// The sum of the bytes received, as one little-endian `u64`.
    pub const SUM: u32 = 8;
}

#[allow(clippy::all)]
pub mod check_capnp {
    include!(concat!(env!("OUT_DIR"), "/check_capnp.rs"));
}

#[allow(clippy::all)]
pub mod echo_capnp {
    include!(concat!(env!("OUT_DIR"), "/echo_capnp.rs"));
}

#[allow(clippy::all)]
pub mod peer_capnp {
    include!(concat!(env!("OUT_DIR"), "/peer_capnp.rs"));
}

#[allow(clippy::all)]
pub mod supervisor_capnp {
    include!(concat!(env!("OUT_DIR"), "/supervisor_capnp.rs"));
}

#[cfg(feature = "alloc")]
#[allow(clippy::all)]
// Generated servers turn a pointer into an integer only as an identity
// (`as_ptr`), never back into a pointer.
#[allow(implicit_provenance_casts)]
pub mod process_capnp {
    include!(concat!(env!("OUT_DIR"), "/process_capnp.rs"));
}
