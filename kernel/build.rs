fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    if arch == "riscv64" {
        println!("cargo:rustc-link-arg-bins=-T{dir}/arch/riscv/linker.ld");
        println!("cargo:rerun-if-changed=arch/riscv/linker.ld");
    }
}
