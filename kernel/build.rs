use std::path::PathBuf;

include!("../user/build_program.rs");

fn main() {
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    if arch == "riscv64" {
        println!("cargo:rustc-link-arg-bins=-T{}", dir.join("arch/riscv/linker.ld").display());
        println!("cargo:rerun-if-changed=arch/riscv/linker.ld");
    }
    // The root task, embedded with `include_bytes!(env!("INIT_ELF"))`. It is
    // always built in release mode: it is small, and its size is part of the
    // kernel image.
    let init = build_user_program(&dir.join("../user"), "init");
    println!("cargo:rustc-env=INIT_ELF={}", init.display());
}
