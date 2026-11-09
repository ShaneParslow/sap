//! The resolver: decides, at runtime, what serves what. From the programs
//! it knows (each with its manifest: what its bootstrap struct declares,
//! `interfaces/system/component.capnp`) and a request naming the components wanted, it
//! finds a provider for every import a component declares by interface:
//! an instance already planned that exports it, or else the one program
//! that does, which it plans too, so dependencies pull themselves in. It
//! builds each component's bootstrap (the request's settings, the import
//! indices, program hashes) and adds the whole composition to its
//! supervisor (`Compositions`), which checks every route to its own
//! authority against grants the resolver doesn't control, then enacts it.
//!
//! Its output goes to the debug console: it runs before the log.
//! TODO: run again when programs are installed (from the store), and
//! reconcile; choose among several providers by policy, not refuse.

#![no_std]
#![no_main]
#![feature(strict_provenance_lints)]
// Pointers come from `sap_sys::address::mapped`, never from integer casts.
#![deny(implicit_provenance_casts, integer_to_ptr_transmutes)]

extern crate alloc;

/// Generated from the component schemas of `resolver`
/// (`user/programs/<name>/<name>.capnp`).
mod components {
    #[allow(clippy::all, dead_code)]
    pub mod resolver_capnp {
        include!(concat!(env!("OUT_DIR"), "/resolver/resolver_capnp.rs"));
    }
}

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use capnp::message::{Builder, Reader, ReaderOptions, SegmentArray, SingleSegmentAllocator};
use sap_interfaces::methods::compositions;
use components::resolver_capnp::resolver_bootstrap;
use sap_interfaces::component_capnp::{import, manifest};
use sap_interfaces::resolver_capnp::{catalog_entry, wanted};
use sap_interfaces::supervisor_capnp::compositions::add_params;
use sap_interfaces::supervisor_capnp::{OnEnd, Status};
use sap_interfaces::words;
use sap_process::process_slot;
use sap_rt::abi::MAX_MESSAGE_LENGTH;
use sap_rt::runtime::{self, Memory, Resources};
use sap_rt::{ipc, println, process};

/// Where the heap grows.
const HEAP: core::ops::Range<usize> = 0x08_0000_0000..0x08_4000_0000;

/// What an import is bound to.
enum To {
    /// Another instance's export: its index in the plan, the export's name
    /// and interface.
    Export(usize, String, u64),
    Serve(String),
    Held(String),
    Supervisor,
}

/// A component the resolver will add.
struct Planned<'a> {
    name: String,
    program: catalog_entry::Reader<'a>,
    settings: &'a [u8],
    on_end: OnEnd,
    routes: Vec<To>,
}

fn manifest_of<'a>(program: catalog_entry::Reader<'a>) -> manifest::Reader<'a> {
    program.get_manifest().expect("resolver: a program without a manifest")
}

fn name_of<'a>(program: catalog_entry::Reader<'a>) -> &'a str {
    manifest_of(program).get_name().ok().and_then(|t| t.to_str().ok()).expect("resolver: a program without a name")
}

/// The instance of `planned` or program of `programs` that exports
/// `interface`: an instance first; else the one program that does.
enum Provider {
    Instance(usize, String),
    Program(usize, String),
}

fn provider(planned: &[Planned], programs: &[catalog_entry::Reader], interface: u64, asker: usize) -> Result<Provider, &'static str> {
    let exported = |p: catalog_entry::Reader| -> Option<String> {
        manifest_of(p).get_exports().ok()?.iter().find(|e| e.get_interface() == interface).and_then(|e| e.get_name().ok()?.to_str().ok().map(String::from))
    };
    let instances: Vec<(usize, String)> = planned.iter().enumerate().filter(|&(i, _)| i != asker).filter_map(|(i, p)| exported(p.program).map(|e| (i, e))).collect();
    match instances.len() {
        1 => return Ok(Provider::Instance(instances[0].0, instances[0].1.clone())),
        0 => {}
        _ => return Err("several instances serve it"),
    }
    let candidates: Vec<(usize, String)> = programs.iter().enumerate().filter_map(|(i, &p)| exported(p).map(|e| (i, e))).collect();
    match candidates.len() {
        1 => Ok(Provider::Program(candidates[0].0, candidates[0].1.clone())),
        0 => Err("nothing serves it"),
        _ => Err("several programs serve it"),
    }
}

/// The plan: the wanted components and everything they need.
fn plan<'a>(programs: &[catalog_entry::Reader<'a>], wanted: capnp::struct_list::Reader<'a, wanted::Owned>) -> Vec<Planned<'a>> {
    let find = |name: &str| programs.iter().copied().find(|&p| name_of(p) == name).unwrap_or_else(|| panic!("resolver: no program {name}"));
    let mut planned: Vec<Planned> = wanted
        .iter()
        .map(|w| Planned {
            name: w.get_name().ok().and_then(|t| t.to_str().ok()).expect("resolver: a name").to_string(),
            program: find(w.get_program().ok().and_then(|t| t.to_str().ok()).expect("resolver: a program")),
            settings: w.get_settings().unwrap_or(&[]),
            on_end: w.get_on_end().unwrap_or(OnEnd::Forget),
            routes: Vec::new(),
        })
        .collect();
    // Each instance's imports, in its manifest's order; providers pulled in
    // are planned after it, and resolved in turn.
    let mut next = 0;
    while next < planned.len() {
        let mut routes = Vec::new();
        for import in manifest_of(planned[next].program).get_imports().expect("resolver: imports") {
            let field = import.get_field().ok().and_then(|t| t.to_str().ok()).unwrap_or("?");
            routes.push(match import.which().expect("resolver: an import") {
                import::Interface(interface) => match provider(&planned, programs, interface, next) {
                    Ok(Provider::Instance(i, export)) => To::Export(i, export, interface),
                    Ok(Provider::Program(p, export)) => {
                        // Services pulled in restart when they fail.
                        let program = programs[p];
                        planned.push(Planned { name: name_of(program).to_string(), program, settings: &[], on_end: OnEnd::Restart, routes: Vec::new() });
                        To::Export(planned.len() - 1, export, interface)
                    }
                    Err(why) => panic!("resolver: {}.{field}: {why} (interface {interface:#x})", planned[next].name),
                },
                import::Serves(export) => To::Serve(export.ok().and_then(|t| t.to_str().ok()).expect("resolver: an export").to_string()),
                import::Held(held) => match held.ok().and_then(|t| t.to_str().ok()).expect("resolver: a held name") {
                    "supervisor" => To::Supervisor,
                    held => To::Held(held.to_string()),
                },
            });
        }
        planned[next].routes = routes;
        next += 1;
    }
    planned
}

/// Read a flat message's root struct pointer: the struct's data and
/// pointers sections, as word ranges of `message`.
fn root_struct(message: &[u64]) -> Option<(core::ops::Range<usize>, core::ops::Range<usize>)> {
    let pointer = *message.first()?;
    if pointer & 3 != 0 {
        return None;
    }
    let offset = ((pointer as u32 as i32) >> 2) as isize;
    let (data, pointers) = ((pointer >> 32) as u16 as usize, (pointer >> 48) as usize);
    let start = usize::try_from(1 + offset).ok()?;
    let (data_end, end) = (start + data, start + data + pointers);
    (end <= message.len()).then_some((start..data_end, data_end..end))
}

/// The bytes of the byte list the pointer at word `at` of `message` points
/// to, if it is one.
fn byte_list(message: &[u64], at: usize) -> Option<&[u8]> {
    let pointer = message[at];
    if pointer == 0 {
        return None;
    }
    assert!(pointer & 3 == 1 && (pointer >> 32) & 7 == 2, "resolver: a setting that isn't Text or Data");
    let offset = ((pointer as u32 as i32) >> 2) as isize;
    let start = usize::try_from(at as isize + 1 + offset).ok()?;
    let len = (pointer >> 35) as usize;
    let words = message.get(start..start + len.div_ceil(8))?;
    // SAFETY: any bytes are valid `u8`s; the slice covers the words, which
    // it borrows.
    Some(&unsafe { core::slice::from_raw_parts(words.as_ptr().cast::<u8>(), words.len() * 8) }[..len])
}

/// The bootstrap message of `planned`: its settings, every import field set
/// to the import's index, every program-hash field to the program's hash,
/// as a flat message.
fn bootstrap(planned: &Planned, programs: &[catalog_entry::Reader]) -> Vec<u8> {
    let manifest = manifest_of(planned.program);
    let (data, pointers) = (manifest.get_data_words() as usize, manifest.get_pointers() as usize);
    let mut out = vec![0u64; 1 + data + pointers];
    out[0] = ((data as u64) << 32) | ((pointers as u64) << 48);
    // Byte lists to append: (pointer index, bytes).
    let mut lists: Vec<(usize, Vec<u8>)> = Vec::new();
    if !planned.settings.is_empty() {
        let settings: Vec<u64> = planned.settings.chunks(8).map(|c| {
            let mut word = [0u8; 8];
            word[..c.len()].copy_from_slice(c);
            u64::from_le_bytes(word)
        }).collect();
        let (their_data, their_pointers) = root_struct(&settings).expect("resolver: settings that aren't a struct");
        for (i, word) in settings[their_data].iter().take(data).enumerate() {
            out[1 + i] = *word;
        }
        for (i, at) in their_pointers.take(pointers).enumerate() {
            if let Some(bytes) = byte_list(&settings, at) {
                lists.push((i, bytes.to_vec()));
            }
        }
    }
    for (index, import) in manifest.get_imports().expect("resolver: imports").iter().enumerate() {
        let offset = import.get_offset() as usize;
        assert!(offset / 2 < data, "resolver: an import field outside the struct");
        let shift = 32 * (offset % 2);
        out[1 + offset / 2] = (out[1 + offset / 2] & !(0xffff_ffff << shift)) | ((index as u64) << shift);
    }
    for hash in manifest.get_program_hashes().expect("resolver: hashes") {
        let wanted = hash.get_program().ok().and_then(|t| t.to_str().ok()).expect("resolver: a program name");
        let program = programs.iter().find(|&&p| name_of(p) == wanted).unwrap_or_else(|| panic!("resolver: no program {wanted}"));
        lists.retain(|(i, _)| *i != hash.get_pointer() as usize);
        lists.push((hash.get_pointer() as usize, program.get_hash().expect("resolver: a hash").to_vec()));
    }
    // Each list after the struct; a list pointer's offset counts from the
    // word after the pointer.
    for (i, bytes) in lists {
        let at = 1 + data + i;
        let start = out.len();
        out.resize(start + bytes.len().div_ceil(8), 0);
        for (j, &byte) in bytes.iter().enumerate() {
            out[start + j / 8] |= (byte as u64) << (8 * (j % 8));
        }
        let offset = (start - at - 1) as u64;
        out[at] = 1 | (offset << 2) | (2 << 32) | ((bytes.len() as u64) << 35);
    }
    out.iter().flat_map(|w| w.to_le_bytes()).collect()
}

/// Add `planned[index]` to the supervisor's composition.
fn add(compositions_cap: usize, planned: &[Planned], index: usize, programs: &[catalog_entry::Reader]) -> Status {
    let p = &planned[index];
    let manifest = manifest_of(p.program);
    let bootstrap = bootstrap(p, programs);
    let mut scratch = vec![0u8; MAX_MESSAGE_LENGTH * 8];
    let mut allocator = SingleSegmentAllocator::new(&mut scratch);
    let mut message = Builder::new(&mut allocator);
    let mut component = message.init_root::<add_params::Builder>().init_component();
    component.set_name(p.name.as_str());
    component.set_program(p.program.get_hash().expect("resolver: a hash"));
    component.set_bootstrap(&bootstrap);
    component.set_on_end(p.on_end);
    component.set_limits(manifest.get_limits().expect("resolver: limits")).expect("resolver: limits");
    let exports = manifest.get_exports().expect("resolver: exports");
    let mut list = component.reborrow().init_exports(exports.len());
    for (i, e) in exports.iter().enumerate() {
        let mut export = list.reborrow().get(i as u32);
        export.set_name(e.get_name().expect("resolver: an export's name"));
        export.set_interface(e.get_interface());
    }
    let mut routes = component.init_imports(p.routes.len() as u32);
    for (i, to) in p.routes.iter().enumerate() {
        let mut route = routes.reborrow().get(i as u32);
        match to {
            To::Export(provider, name, interface) => {
                let mut export = route.init_client();
                export.set_component(planned[*provider].name.as_str());
                export.set_name(name.as_str());
                export.set_interface(*interface);
            }
            To::Serve(name) => route.set_serve(name.as_str()),
            To::Held(name) => route.init_held().set_name(name.as_str()),
            To::Supervisor => route.set_supervisor(()),
        }
    }
    let mut out = [0usize; MAX_MESSAGE_LENGTH];
    let n = words::pack(&message, &mut out).unwrap_or_else(|| panic!("resolver: {} is too large for one call", p.name));
    let reply = ipc::call(compositions_cap, compositions::ADD, &out[..n], &[], &mut []).expect("resolver: add");
    u16::try_from(reply.info.label()).ok().and_then(|l| Status::try_from(l).ok()).unwrap_or(Status::BadRequest)
}

#[unsafe(no_mangle)]
fn sap_main(a0: usize, _: usize, _: usize) -> ! {
    // SAFETY: the loader started this process with its startup record.
    let startup = unsafe { process::startup(a0) };
    let free = startup.free;
    runtime::start(Resources { cnode: process_slot::CNODE, slots: free.start..free.end, memory: Memory::Account(process_slot::ACCOUNT), vspace: process_slot::VSPACE, heap: HEAP });
    // The bootstrap message, copied to the heap: aligned words to read.
    let bytes = process::bootstrap(startup);
    let mut copy = vec![0u64; bytes.len().div_ceil(8)];
    for (i, &byte) in bytes.iter().enumerate() {
        copy[i / 8] |= (byte as u64) << (8 * (i % 8));
    }
    // SAFETY: any bytes are valid `u8`s; the slice covers the words.
    let segment = [unsafe { core::slice::from_raw_parts(copy.as_ptr().cast::<u8>(), copy.len() * 8) }];
    let mut options = ReaderOptions::new();
    options.traversal_limit_in_words(Some(1 << 20));
    let reader = Reader::new(SegmentArray::new(&segment), options);
    let imports = reader.get_root::<resolver_bootstrap::Reader>().expect("resolver: bootstrap message");
    let compositions_cap = process::import(startup, imports.get_compositions()).expect("resolver: no compositions import");
    let programs: Vec<catalog_entry::Reader> = imports.get_programs().expect("resolver: programs").iter().collect();
    let request = imports.get_request().expect("resolver: request");

    let planned = plan(&programs, request.get_components().expect("resolver: components"));
    for (i, p) in planned.iter().enumerate() {
        let serving: Vec<String> = planned
            .iter()
            .filter(|q| q.routes.iter().any(|r| matches!(r, To::Export(provider, ..) if *provider == i)))
            .map(|q| q.name.clone())
            .collect();
        if serving.is_empty() {
            println!("resolver: {} ({})", p.name, name_of(p.program));
        } else {
            println!("resolver: {} ({}), serving {}", p.name, name_of(p.program), serving.join(", "));
        }
        let status = add(compositions_cap, &planned, i, &programs);
        assert!(status == Status::Ok, "resolver: the supervisor refused {}: {status:?}", p.name);
    }
    let reply = ipc::call(compositions_cap, compositions::ENACT, &[], &[], &mut []).expect("resolver: enact");
    assert_eq!(reply.info.label(), Status::Ok as usize, "resolver: enacting failed");
    process::exit(0)
}
