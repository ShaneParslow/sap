//! Builds `user/peer`, the second program init loads, and points `PEER_ELF`
//! at it for `include_bytes!`.

use std::path::PathBuf;

include!("../build_program.rs");

fn main() {
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let peer = build_user_program(&dir.join(".."), "peer");
    println!("cargo:rustc-env=PEER_ELF={}", peer.display());
}
