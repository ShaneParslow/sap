//! Generate host bindings for the schemas the manifests are written with:
//! the system's (at the crate root) and the resolver's bootstrap message
//! (`components::resolver_capnp`), which refers to the system's as
//! `sap_interfaces`, as programs' generated code does
//! (`user/build_components.rs`); `lib.rs` names this crate so.

use std::path::Path;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let interfaces = root.join("interfaces");
    let mut system = capnpc::CompilerCommand::new();
    system.src_prefix(&interfaces);
    let mut ids = Vec::new();
    for schema in ["system/component.capnp", "system/memory.capnp", "system/resolver.capnp", "system/supervisor.capnp"] {
        let path = interfaces.join(schema);
        println!("cargo:rerun-if-changed={}", path.display());
        let text = std::fs::read_to_string(&path).unwrap();
        let id = text.lines().next().and_then(|l| l.strip_prefix("@0x")).and_then(|l| l.strip_suffix(';')).expect("a schema's first line is its id");
        ids.push(u64::from_str_radix(id, 16).unwrap());
        system.file(path);
    }
    system.run().expect("compiling the schemas (is the capnp tool installed?)");

    let programs = root.join("user/programs");
    let resolver = programs.join("resolver/resolver.capnp");
    println!("cargo:rerun-if-changed={}", resolver.display());
    capnpc::CompilerCommand::new()
        .src_prefix(&programs)
        .import_path(&interfaces)
        .default_parent_module(vec!["components".into()])
        .crate_provides("sap_interfaces", ids)
        .file(resolver)
        .run()
        .expect("compiling the resolver's schema");
}
