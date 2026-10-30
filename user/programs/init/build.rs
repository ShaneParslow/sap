//! Builds the programs init can start and writes the boot pack: a table of
//! them in `$OUT_DIR/pack.rs`, each named by the BLAKE3 hash of its image,
//! with its manifest (`tools/manifests`), for `pack.rs` to include.

use std::fmt::Write;
use std::path::PathBuf;

include!("../../build_program.rs");


/// The programs in the boot pack, by crate name under user/programs/.
const PROGRAMS: &[&str] = &["check", "log", "memory", "peer", "resolver"];

fn main() {
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    // Every schema, for its declarations; the Rust capnpc writes is unused.
    let interfaces = dir.join("../../../interfaces");
    let request = out.join("request.bin");
    std::fs::create_dir_all(out.join("unused")).unwrap();
    let mut command = capnpc::CompilerCommand::new();
    command.src_prefix(&interfaces).output_path(out.join("unused")).no_rpc().raw_code_generator_request_path(&request);
    for entry in std::fs::read_dir(&interfaces).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "capnp") {
            println!("cargo:rerun-if-changed={}", path.display());
            command.file(path);
        }
    }
    command.run().expect("compiling the schemas in interfaces/");
    let manifests = sap_manifests::manifests(&request);

    let mut pack = String::new();
    let mut available = Vec::new();
    let mut table = String::from("pub static PROGRAMS: &[Program] = &[\n");
    for &name in PROGRAMS {
        let elf = build_user_program(&dir.join("../.."), name);
        let image = std::fs::read(&elf).unwrap_or_else(|e| panic!("reading {}: {e}", elf.display()));
        let hash = blake3::hash(&image);
        let constant = name.to_uppercase();
        writeln!(pack, "/// `{name}`: BLAKE3 {}.", hash.to_hex()).unwrap();
        writeln!(pack, "pub const {constant}: Hash = Hash({:?});", hash.as_bytes()).unwrap();
        let manifest = manifests.get(name).unwrap_or_else(|| panic!("{name} declares no bootstrap struct ($Sap.program)"));
        available.push((name.to_string(), *hash.as_bytes(), manifest.clone()));
        writeln!(pack, "static {constant}_MANIFEST: [u64; {}] = {manifest:?};", manifest.len()).unwrap();
        writeln!(table, "    Program {{ name: {name:?}, hash: {constant}, image: include_bytes!({:?}), manifest: &{constant}_MANIFEST }},", elf.display().to_string()).unwrap();
    }
    table.push_str("];\n");
    // What to run: `SAP_BOOT`, a request in boot/ or a program's name.
    let boot = std::env::var("SAP_BOOT").unwrap_or_else(|_| "check".into());
    let requests = dir.join("../../../boot");
    println!("cargo:rerun-if-changed={}", requests.display());
    let resolver = sap_manifests::resolver_bootstrap(&boot, &requests, &interfaces, &available);
    writeln!(pack, "/// The resolver's bootstrap message, for `SAP_BOOT={boot}`.").unwrap();
    writeln!(pack, "pub static RESOLVER_BOOTSTRAP: [u64; {}] = {resolver:?};", resolver.len()).unwrap();
    std::fs::write(out.join("pack.rs"), pack + &table).unwrap();
}
