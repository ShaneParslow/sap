//! Rust bindings for sap's Cap'n Proto interfaces, generated at build time
//! from the schemas in interfaces/ at the top of the repository.

#![no_std]
#![feature(strict_provenance_lints)]
// Pointers come from `sap_sys::address::mapped`, never from integer casts.
#![deny(implicit_provenance_casts, integer_to_ptr_transmutes)]

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
pub mod echo_capnp {
    include!(concat!(env!("OUT_DIR"), "/echo_capnp.rs"));
}
