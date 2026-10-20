//! Rust bindings for sap's Cap'n Proto interfaces, generated at build time
//! from the schemas in interfaces/ at the top of the repository.

#![no_std]
#![feature(strict_provenance_lints)]
// Pointers come from `sap_sys::address::mapped`, never from integer casts.
#![deny(implicit_provenance_casts, integer_to_ptr_transmutes)]

#[allow(clippy::all)]
pub mod echo_capnp {
    include!(concat!(env!("OUT_DIR"), "/echo_capnp.rs"));
}
