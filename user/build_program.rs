// Building a user program from a build script, so the program that runs it
// can embed the result with `include_bytes!`. Shared by kernel/build.rs (the
// root task) and user/init/build.rs (the programs init loads) through
// `include!`, since a build-dependency crate in the workspace would be
// checked for the bare-metal target too.

/// Build the program in `user/<name>` in release mode and return its ELF's
/// path. `user` is the path of the `user/` directory.
///
/// Cargo runs from the program's directory, so user/.cargo/config.toml (the
/// sap target, core built from source) applies, and without this build's
/// compiler flags and target, which would override that config. It gets a
/// target directory of its own under this build's output: sharing this
/// build's would deadlock on its lock.
fn build_user_program(user: &std::path::Path, name: &str) -> std::path::PathBuf {
    let target_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join(name);
    let status = std::process::Command::new(std::env::var("CARGO").unwrap())
        .current_dir(user.join(name))
        .args(["build", "--release", "--target-dir"])
        .arg(&target_dir)
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_BUILD_TARGET")
        .status()
        .unwrap_or_else(|e| panic!("failed to run cargo for {name}: {e}"));
    assert!(status.success(), "building {name} failed");
    // Everything under user/ (the program, its libraries, the target and
    // its config), the shared ABI and the interface schemas.
    println!("cargo:rerun-if-changed={}", user.display());
    println!("cargo:rerun-if-changed={}", user.join("../abi").display());
    println!("cargo:rerun-if-changed={}", user.join("../interfaces").display());
    target_dir.join("riscv64gc-unknown-sap/release").join(name)
}
