//! For init's build: manifests, what each program in the boot pack
//! declares, read from the annotations of `interfaces/sap.capnp` on its
//! bootstrap struct; and the resolver's bootstrap message, every program's
//! manifest with the boot request. Declarations and the request are all
//! this reads; which component serves which import is the resolver's, at
//! runtime.

use std::collections::HashMap;
use std::path::Path;

use capnp::message::ReaderSegments;
use capnp::schema_capnp::{code_generator_request, field, node, type_, value};

use crate::schemas::sap_capnp::{export, manifest, request, resolver_imports};
use crate::schemas::supervisor_capnp::limits;

/// The schemas the manifests are written with.
#[allow(dead_code, clippy::all)]
mod schemas {
    pub mod memory_capnp {
        include!(concat!(env!("OUT_DIR"), "/memory_capnp.rs"));
    }
    pub mod sap_capnp {
        include!(concat!(env!("OUT_DIR"), "/sap_capnp.rs"));
    }
    pub mod supervisor_capnp {
        include!(concat!(env!("OUT_DIR"), "/supervisor_capnp.rs"));
    }
}

/// The annotations of `sap.capnp`, by name.
const ANNOTATIONS: [&str; 7] = ["program", "exports", "limits", "interface", "serves", "held", "programHash"];

/// The manifest of every program that declares one in the schemas the
/// code generator request at `request` describes, by crate name, as the
/// words of a single-segment message.
pub fn manifests(request: &Path) -> HashMap<String, Vec<u64>> {
    let file = std::fs::File::open(request).unwrap();
    let mut options = capnp::message::ReaderOptions::new();
    options.traversal_limit_in_words(None);
    let message = capnp::serialize::read_message(std::io::BufReader::new(file), options).unwrap();
    let request = message.get_root::<code_generator_request::Reader>().unwrap();
    let nodes: Vec<node::Reader> = request.get_nodes().unwrap().iter().collect();
    let by_name: HashMap<&str, u64> = nodes.iter().map(|n| (n.get_display_name().unwrap().to_str().unwrap(), n.get_id())).collect();
    let annotation = |name: &str| by_name[format!("sap.capnp:{name}").as_str()];
    let ids: HashMap<u64, &str> = ANNOTATIONS.iter().map(|&name| (annotation(name), name)).collect();
    // An interface's type id, by its name ("log.capnp:Log").
    let interface = |name: &str| -> u64 {
        let id = *by_name.get(name).unwrap_or_else(|| panic!("no interface {name}"));
        let node = nodes.iter().find(|n| n.get_id() == id).unwrap();
        assert!(matches!(node.which(), Ok(node::Interface(_))), "{name} isn't an interface");
        id
    };
    let text = |v: value::Reader| match v.which() {
        Ok(value::Text(t)) => t.unwrap().to_str().unwrap().to_string(),
        _ => panic!("an annotation that should be text"),
    };

    let mut manifests = HashMap::new();
    for node in &nodes {
        let Ok(node::Struct(structure)) = node.which() else { continue };
        let struct_annotations: HashMap<&str, value::Reader> =
            node.get_annotations().unwrap().iter().filter_map(|a| ids.get(&a.get_id()).map(|&n| (n, a.get_value().unwrap()))).collect();
        let Some(&program) = struct_annotations.get("program") else { continue };
        let program = text(program);
        let display = node.get_display_name().unwrap().to_str().unwrap();

        let mut builder = capnp::message::Builder::new(capnp::message::HeapAllocator::new().first_segment_words(4096));
        let mut m = builder.init_root::<manifest::Builder>();
        m.set_program(program.as_str());
        m.set_data_words(structure.get_data_word_count());
        m.set_pointers(structure.get_pointer_count());
        if let Some(&limits) = struct_annotations.get("limits") {
            let Ok(value::Struct(limits)) = limits.which() else { panic!("{display}: limits") };
            m.set_limits(limits.get_as::<limits::Reader>().unwrap()).unwrap();
        }
        if let Some(&exports) = struct_annotations.get("exports") {
            let Ok(value::List(exports)) = exports.which() else { panic!("{display}: exports") };
            let exports = exports.get_as::<capnp::struct_list::Reader<export::Owned>>().unwrap();
            let mut list = m.reborrow().init_exports(exports.len());
            for (i, e) in exports.iter().enumerate() {
                let mut out = list.reborrow().get(i as u32);
                out.set_name(e.get_name().unwrap());
                out.set_interface(interface(e.get_interface().unwrap().to_str().unwrap()));
            }
        }

        // Imports in field order, and hash settings.
        let mut fields: Vec<field::Reader> = structure.get_fields().unwrap().iter().collect();
        fields.sort_by_key(|f| f.get_code_order());
        let mut imports = Vec::new();
        let mut hashes = Vec::new();
        for f in fields {
            let name = f.get_name().unwrap().to_str().unwrap().to_string();
            let Ok(field::Slot(slot)) = f.which() else { continue };
            for a in f.get_annotations().unwrap() {
                let Some(&kind) = ids.get(&a.get_id()) else { continue };
                let value = text(a.get_value().unwrap());
                let ty = slot.get_type().unwrap().which().unwrap();
                match kind {
                    "programHash" => {
                        assert!(matches!(ty, type_::Data(())), "{display}.{name}: a program hash is Data");
                        hashes.push((slot.get_offset(), value));
                    }
                    _ => {
                        assert!(matches!(ty, type_::Uint32(())), "{display}.{name}: an import is a UInt32");
                        imports.push((name.clone(), slot.get_offset(), kind, value));
                    }
                }
            }
        }
        let mut list = m.reborrow().init_imports(imports.len() as u32);
        for (i, (name, offset, kind, value)) in imports.iter().enumerate() {
            let mut out = list.reborrow().get(i as u32);
            out.set_field(name.as_str());
            out.set_offset(*offset);
            match *kind {
                "interface" => out.set_interface(interface(value)),
                "serves" => out.set_serves(value.as_str()),
                "held" => out.set_held(value.as_str()),
                _ => unreachable!(),
            }
        }
        let mut list = m.init_program_hashes(hashes.len() as u32);
        for (i, (pointer, program)) in hashes.iter().enumerate() {
            let mut out = list.reborrow().get(i as u32);
            out.set_pointer(*pointer as u16);
            out.set_program(program.as_str());
        }

        let segments = builder.get_segments_for_output();
        assert_eq!(segments.len(), 1, "{display}: a manifest is one segment");
        let words = segments[0].chunks(8).map(|c| u64::from_le_bytes(c.try_into().unwrap())).collect();
        assert!(manifests.insert(program.clone(), words).is_none(), "{program} declared twice");
    }
    manifests
}

/// The words of the flat message in `segments`, which must be one.
fn flat(segments: &[&[u8]], what: &str) -> Vec<u64> {
    assert_eq!(segments.len(), 1, "{what}: one segment");
    segments[0].chunks(8).map(|c| u64::from_le_bytes(c.try_into().unwrap())).collect()
}

/// Evaluate `constant` in the schema file `file` with the schema compiler,
/// as a single-segment message.
fn evaluate(file: &Path, constant: &str, interfaces: &Path) -> Vec<u8> {
    let output = std::process::Command::new("capnp")
        .args(["eval", "-b", "-I"])
        .arg(interfaces)
        .arg(file)
        .arg(constant)
        .output()
        .expect("running capnp eval");
    assert!(output.status.success(), "capnp eval {} {constant}: {}", file.display(), String::from_utf8_lossy(&output.stderr));
    let message = capnp::serialize::read_message(&mut &output.stdout[..], capnp::message::ReaderOptions::new()).unwrap();
    let segments = message.into_segments();
    let words = flat(&[segments.get_segment(0).unwrap()], constant);
    assert_eq!(segments.len(), 1, "{constant}: one segment");
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}

/// The resolver's bootstrap message: `programs` (name, hash, manifest) and
/// the boot request `boot`: a request file in `requests` (`<boot>.capnp`,
/// whose constant `request` it is), or else a program's name, which then
/// runs alone and ends the boot.
pub fn resolver_bootstrap(boot: &str, requests: &Path, interfaces: &Path, programs: &[(String, [u8; 32], Vec<u64>)]) -> Vec<u64> {
    let mut builder = capnp::message::Builder::new(capnp::message::HeapAllocator::new().first_segment_words(1 << 16));
    let mut root = builder.init_root::<resolver_imports::Builder>();
    root.set_compositions(0);
    let mut list = root.reborrow().init_programs(programs.len() as u32);
    for (i, (_, hash, manifest)) in programs.iter().enumerate() {
        let mut out = list.reborrow().get(i as u32);
        out.set_hash(hash);
        let bytes: Vec<u8> = manifest.iter().flat_map(|w| w.to_le_bytes()).collect();
        let segments: [&[u8]; 1] = [&bytes];
        let message = capnp::message::Reader::new(capnp::message::SegmentArray::new(&segments), capnp::message::ReaderOptions::new());
        out.set_manifest(message.get_root::<manifest::Reader>().unwrap()).unwrap();
    }
    let file = requests.join(format!("{boot}.capnp"));
    let mut out = root.init_request();
    if file.exists() {
        let bytes = evaluate(&file, "request", interfaces);
        let segments: [&[u8]; 1] = [&bytes];
        let message = capnp::message::Reader::new(capnp::message::SegmentArray::new(&segments), capnp::message::ReaderOptions::new());
        let request = message.get_root::<request::Reader>().unwrap();
        out.set_grants(request.get_grants().unwrap()).unwrap();
        let wanted = request.get_components().unwrap();
        let mut list = out.init_components(wanted.len());
        for (i, w) in wanted.iter().enumerate() {
            let mut component = list.reborrow().get(i as u32);
            component.set_name(w.get_name().unwrap());
            component.set_program(w.get_program().unwrap());
            component.set_on_end(w.get_on_end().unwrap());
            let constant = w.get_settings_constant().unwrap().to_str().unwrap();
            if !constant.is_empty() {
                component.set_settings(&evaluate(&file, constant, interfaces));
            }
        }
    } else {
        assert!(programs.iter().any(|(name, ..)| name == boot), "SAP_BOOT={boot}: no request {} and no such program", file.display());
        let mut component = out.reborrow().init_components(1).get(0);
        component.set_name(boot);
        component.set_program(boot);
        component.set_on_end(crate::schemas::supervisor_capnp::OnEnd::EndBoot);
        let mut grant = out.init_grants(1).get(0);
        grant.set_component(boot);
        grant.set_held("endBoot");
    }
    flat(&builder.get_segments_for_output(), "the resolver's bootstrap")
}
