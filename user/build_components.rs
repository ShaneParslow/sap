// Generating Rust for programs' component schemas
// (`user/programs/<name>/<name>.capnp`) from a build script, for the crate
// to include as `crate::components::<name>_capnp` (`$OUT_DIR/<name>/
// <name>_capnp.rs`). Types from the schemas in interfaces/ come from
// sap-interfaces. Shared through `include!`, like build_program.rs.

/// Generate the component schemas of the programs `names`. `user` is the
/// path of the `user/` directory.
fn generate_components(user: &std::path::Path, names: &[&str]) {
    let interfaces = user.join("../interfaces");
    let mut command = capnpc::CompilerCommand::new();
    command
        .src_prefix(user.join("programs"))
        .import_path(&interfaces)
        .no_rpc()
        .default_parent_module(vec!["components".into()])
        .crate_provides("sap_interfaces", schema_ids(&interfaces));
    for name in names {
        let path = user.join("programs").join(name).join(format!("{name}.capnp"));
        println!("cargo:rerun-if-changed={}", path.display());
        command.file(path);
    }
    println!("cargo:rerun-if-changed={}", interfaces.display());
    command.run().expect("compiling component schemas (is the capnp tool installed?)");
}

/// The file ids of the schemas under `dir`: each file's first line.
fn schema_ids(dir: &std::path::Path) -> Vec<u64> {
    let mut ids = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            ids.extend(schema_ids(&path));
        } else if path.extension().is_some_and(|e| e == "capnp") {
            let text = std::fs::read_to_string(&path).unwrap();
            let id = text.lines().next().and_then(|l| l.strip_prefix("@0x")).and_then(|l| l.strip_suffix(';'));
            let id = id.unwrap_or_else(|| panic!("{}: the first line isn't the file's id", path.display()));
            ids.push(u64::from_str_radix(id, 16).unwrap());
        }
    }
    ids
}
