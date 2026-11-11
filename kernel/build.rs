use std::path::PathBuf;

include!("../user/build_program.rs");

fn main() {
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    if arch == "riscv64" {
        println!("cargo:rustc-link-arg-bins=-T{}", dir.join("arch/riscv/linker.ld").display());
        println!("cargo:rerun-if-changed=arch/riscv/linker.ld");
    }
    // How often the audit runs, with the `audit` feature: every Nth return
    // to user mode (audit.rs).
    println!("cargo:rerun-if-env-changed=SAP_AUDIT_INTERVAL");
    let interval = std::env::var("SAP_AUDIT_INTERVAL").unwrap_or_else(|_| "1".into());
    println!("cargo:rustc-env=SAP_AUDIT_INTERVAL={interval}");

    // The root task, embedded with `include_bytes!(env!("ROOT_TASK_ELF"))`:
    // `init`, or the program `SAP_ROOT` names (`test-kernel`). It is always
    // built in release mode: it is small, and its size is part of the kernel
    // image.
    println!("cargo:rerun-if-env-changed=SAP_ROOT");
    let root = std::env::var("SAP_ROOT").unwrap_or_else(|_| "init".into());
    let image = build_user_program(&dir.join("../user"), &root);
    println!("cargo:rustc-env=ROOT_TASK_ELF={}", image.display());
}
