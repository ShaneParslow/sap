//! Generate Rust from the Cap'n Proto schemas in interfaces/. Needs the
//! `capnp` schema compiler on the PATH.

use std::path::Path;

/// Schemas with interfaces, whose generated code needs `alloc`.
const WITH_INTERFACES: &[&str] = &["process.capnp"];

fn main() {
    let schemas = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../interfaces");
    let mut command = capnpc::CompilerCommand::new();
    command.src_prefix(&schemas);
    for schema in ["check.capnp", "echo.capnp", "peer.capnp"].iter().chain(WITH_INTERFACES) {
        let path = schemas.join(schema);
        println!("cargo:rerun-if-changed={}", path.display());
        command.file(path);
    }
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let request = out.join("request.bin");
    command.raw_code_generator_request_path(&request);
    command.run().expect("compiling the schemas in interfaces/ (is the capnp tool installed?)");
    for schema in WITH_INTERFACES {
        qualify_alloc(&out.join(schema.replace(".capnp", "_capnp.rs")));
    }
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

/// The code capnpc generates for an interface names `Box` and calls
/// `to_string` as if `std`'s prelude were in scope, inside modules of its
/// own, where no `use` around the `include!` reaches. Spell them out so the
/// code compiles in a `no_std` crate with `alloc`.
fn qualify_alloc(path: &Path) {
    let code = std::fs::read_to_string(path).unwrap();
    let code = code.replace("Box<", "::alloc::boxed::Box<").replace("\".to_string()", "\".into()");
    assert!(!code.contains(".to_string()"), "{}: an unexpected to_string", path.display());
    std::fs::write(path, code).unwrap();
}
