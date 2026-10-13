use std::path::PathBuf;
use std::process::Command;

fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    if arch == "riscv64" {
        println!("cargo:rustc-link-arg-bins=-T{dir}/arch/riscv/linker.ld");
        println!("cargo:rerun-if-changed=arch/riscv/linker.ld");
    }
    build_init(&dir);
}

/// Build the root task (`user/init`) and point `INIT_ELF` at it, so the
/// kernel can embed it with `include_bytes!`. It is always built in release
/// mode: it is small, and its size is part of the kernel image.
fn build_init(kernel_dir: &str) {
    let init_dir = PathBuf::from(kernel_dir).join("../user/init");
    let target_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("user");
    let cargo = std::env::var("CARGO").unwrap();

    // Run from the init crate's directory so its .cargo/config.toml (target,
    // relocation model) applies, and drop the outer build's flags, which
    // would otherwise override that config.
    let status = Command::new(cargo)
        .current_dir(&init_dir)
        .args(["build", "--release", "--target-dir"])
        .arg(&target_dir)
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_BUILD_TARGET")
        .status()
        .expect("failed to run cargo for user/init");
    assert!(status.success(), "building user/init failed");

    let elf = target_dir.join("riscv64gc-unknown-none-elf/release/init");
    println!("cargo:rustc-env=INIT_ELF={}", elf.display());
    println!("cargo:rerun-if-changed={}", init_dir.display());
    println!("cargo:rerun-if-changed={kernel_dir}/../abi");
}
