//! Builds the programs init can start and writes the boot pack: a table of
//! them in `$OUT_DIR/pack.rs`, each named by the BLAKE3 hash of its image,
//! for `pack.rs` to include.

use std::fmt::Write;
use std::path::PathBuf;

include!("../build_program.rs");

/// The programs in the boot pack, by crate name under user/.
const PROGRAMS: &[&str] = &["check"];

fn main() {
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let mut pack = String::new();
    let mut table = String::from("pub static PROGRAMS: &[Program] = &[\n");
    for &name in PROGRAMS {
        let elf = build_user_program(&dir.join(".."), name);
        let image = std::fs::read(&elf).unwrap_or_else(|e| panic!("reading {}: {e}", elf.display()));
        let hash = blake3::hash(&image);
        let constant = name.to_uppercase();
        writeln!(pack, "/// `{name}`: BLAKE3 {}.", hash.to_hex()).unwrap();
        writeln!(pack, "pub const {constant}: Hash = Hash({:?});", hash.as_bytes()).unwrap();
        writeln!(table, "    Program {{ name: {name:?}, hash: {constant}, image: include_bytes!({:?}) }},", elf.display().to_string()).unwrap();
    }
    table.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("pack.rs");
    std::fs::write(out, pack + &table).unwrap();
}
