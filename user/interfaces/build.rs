//! Generate Rust from the Cap'n Proto schemas in interfaces/. Needs the
//! `capnp` schema compiler on the PATH.

fn main() {
    let schemas = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../interfaces");
    let mut command = capnpc::CompilerCommand::new();
    command.src_prefix(&schemas);
    for schema in ["echo.capnp", "peer.capnp"] {
        let path = schemas.join(schema);
        println!("cargo:rerun-if-changed={}", path.display());
        command.file(path);
    }
    command.run().expect("compiling the schemas in interfaces/ (is the capnp tool installed?)");
}
