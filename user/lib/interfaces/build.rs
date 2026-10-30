//! Generate Rust from the Cap'n Proto schemas in interfaces/. Needs the
//! `capnp` schema compiler on the PATH.
//!
//! Without capnp-rust's RPC code (`no_rpc`, from a fork of capnpc the root
//! `Cargo.toml` patches in until upstream has it): sap's calls are its own
//! (`process.capnp` describes them), and capnp-rust's RPC code needs
//! capnp's `alloc` feature, under which building any message allocates.

use std::path::Path;

fn main() {
    let schemas = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../interfaces");
    let mut command = capnpc::CompilerCommand::new();
    command.src_prefix(&schemas).no_rpc();
    for schema in ["check.capnp", "echo.capnp", "log.capnp", "peer.capnp", "process.capnp", "supervisor.capnp"] {
        let path = schemas.join(schema);
        println!("cargo:rerun-if-changed={}", path.display());
        command.file(path);
    }
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let request = out.join("request.bin");
    command.raw_code_generator_request_path(&request);
    command.run().expect("compiling the schemas in interfaces/ (is the capnp tool installed?)");
    write_methods(&request, &out.join("methods.rs"));
}

/// Write a module per interface with a constant per method, its ordinal:
/// `process_factory::SPAWN` for `ProcessFactory.spawn`. capnpc's code has
/// the ordinals only inside its RPC machinery, and a call over sap's IPC
/// uses them as message labels.
fn write_methods(request: &Path, out: &Path) {
    use capnp::schema_capnp::{code_generator_request, node};
    let file = std::fs::File::open(request).unwrap();
    let message = capnp::serialize::read_message(std::io::BufReader::new(file), capnp::message::ReaderOptions::new()).unwrap();
    let request = message.get_root::<code_generator_request::Reader>().unwrap();
    let mut code = String::new();
    for node in request.get_nodes().unwrap() {
        let node::Which::Interface(interface) = node.which().unwrap() else { continue };
        let display = node.get_display_name().unwrap().to_str().unwrap();
        let name = display.rsplit([':', '.']).next().unwrap();
        code += &format!("/// The methods of `{display}`.\npub mod {} {{\n", snake(name));
        // Methods are listed by ordinal.
        for (ordinal, method) in interface.get_methods().unwrap().iter().enumerate() {
            let method = method.get_name().unwrap().to_str().unwrap();
            code += &format!("    pub const {}: usize = {ordinal};\n", snake(method).to_uppercase());
        }
        code += "}\n";
    }
    std::fs::write(out, code).unwrap();
}

/// `ProcessFactory` or `processFactory` as `process_factory`.
fn snake(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}
