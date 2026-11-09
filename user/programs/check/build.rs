//! Builds `user/programs/peer`, the program the checks start in processes of their
//! own, and points `PEER_ELF` at it for `include_bytes!`; generates the
//! component schemas of `check`, and of `peer` and `counter`, whose
//! bootstrap messages the checks write.

use std::path::PathBuf;

include!("../../build_program.rs");
include!("../../build_components.rs");

fn main() {
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let peer = build_user_program(&dir.join("../.."), "peer");
    println!("cargo:rustc-env=PEER_ELF={}", peer.display());
    generate_components(&dir.join("../.."), &["check", "counter", "peer"]);
}
