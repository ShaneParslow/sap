//! Generates this program's component schema (`log.capnp`).

use std::path::PathBuf;

include!("../../build_components.rs");

fn main() {
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    generate_components(&dir.join("../.."), &["log"]);
}
