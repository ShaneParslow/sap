//! Generate host bindings for the schemas the manifests are written with.

use std::path::Path;

fn main() {
    let interfaces = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../interfaces");
    let mut command = capnpc::CompilerCommand::new();
    command.src_prefix(&interfaces).default_parent_module(vec!["schemas".into()]);
    for schema in ["memory.capnp", "sap.capnp", "supervisor.capnp"] {
        let path = interfaces.join(schema);
        println!("cargo:rerun-if-changed={}", path.display());
        command.file(path);
    }
    command.run().expect("compiling the schemas (is the capnp tool installed?)");
}
