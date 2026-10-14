//! Link as a static, position-independent executable: the loader picks the
//! address and applies the relative relocations. Each segment gets its own
//! pages so the loader can map them with different permissions.
//!
//! `-z notext` allows relocations in read-only segments. The precompiled
//! `core` for this target isn't position-independent, so its vtables need
//! them; the loader applies relocations before mapping anything, so it can
//! write there.
//!
//! Also builds `user/peer`, the second program init loads, and points
//! `PEER_ELF` at it for `include_bytes!`.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    for arg in ["--pie", "--no-dynamic-linker", "-z", "separate-loadable-segments", "-z", "norelro", "-z", "notext"] {
        println!("cargo:rustc-link-arg-bins={arg}");
    }

    let here = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let peer_dir = PathBuf::from(&here).join("../peer");
    let target_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("peer");
    // Run from the peer's directory so its .cargo/config.toml applies, and
    // drop this build's flags, which would otherwise override it.
    let status = Command::new(std::env::var("CARGO").unwrap())
        .current_dir(&peer_dir)
        .args(["build", "--release", "--target-dir"])
        .arg(&target_dir)
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_BUILD_TARGET")
        .status()
        .expect("failed to run cargo for user/peer");
    assert!(status.success(), "building user/peer failed");
    let elf = target_dir.join("riscv64gc-unknown-none-elf/release/peer");
    println!("cargo:rustc-env=PEER_ELF={}", elf.display());
    println!("cargo:rerun-if-changed={}", peer_dir.display());
    println!("cargo:rerun-if-changed={here}/../libsap");
}
