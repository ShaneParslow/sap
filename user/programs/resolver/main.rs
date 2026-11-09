//! The resolver: decides, at runtime, what serves what. From the programs
//! it knows (each with its manifest: what its bootstrap struct declares,
//! `interfaces/system/component.capnp`) and a request naming the components
//! wanted, it finds a provider for every import a component declares by
//! interface: an export the request binds it to by hand, an instance
//! already planned that exports it, or else the one program that does,
//! which it plans too, so dependencies pull themselves in. It builds each
//! component's bootstrap (the request's settings, the import indices,
//! program hashes) and adds the components to a composition
//! (`Compositions`), which checks every route against its own authority:
//! the resolver holds none of it.
//!
//! At boot it resolves the boot request (its bootstrap message) into its
//! supervisor's own composition and enacts it. Then it serves `Resolver`
//! (`interfaces/system/resolver.capnp`): the same for a request and a
//! composition its caller passes, left for the caller to enact.
//!
//! Its output goes to the debug console: it runs before the log.
//! TODO: a catalog to read programs from (the store) rather than the boot
//! pack in its bootstrap; choose among several providers by policy, not
//! refuse; bind to exports within its caller's reach, not only those named
//! by hand (design/processes.md in sap-docs).

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

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use capnp::message::{Builder, Reader, ReaderOptions, SegmentArray, SingleSegmentAllocator};
use components::resolver_capnp::resolver_bootstrap;
use sap_interfaces::component_capnp::{import, manifest};
use sap_interfaces::methods;
use sap_interfaces::resolver_capnp::resolver::resolve_params;
use sap_interfaces::resolver_capnp::{catalog_entry, request, wanted, Status};
use sap_interfaces::supervisor_capnp::compositions::add_params;
use sap_interfaces::supervisor_capnp::{self, OnEnd};
use sap_interfaces::words;
use sap_process::process_slot;
use sap_rt::abi::{object, MAX_MESSAGE_LENGTH};
use sap_rt::cnode::SlotRef;
use sap_rt::runtime::{self, Memory, Resources};
use sap_rt::{account, endpoint, println, slots, startup, untyped};

/// Where the heap grows.
const HEAP: core::ops::Range<usize> = 0x08_0000_0000..0x08_4000_0000;

/// What an import is bound to.
enum To {
    /// Another planned instance's export: its index in the plan, the
    /// export's name and interface.
    Export(usize, String, u64),
    /// An export outside the plan, bound by hand: its component, name and
    /// interface.
    External(String, String, u64),
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
    /// Imports bound by hand: (field, component, export).
    bindings: Vec<(String, String, String)>,
    routes: Vec<To>,
}

/// Why a request can't be met: what to answer, and why, for the console.
struct Refused(Status, String);

fn refused(status: Status, why: impl Into<String>) -> Refused {
    Refused(status, why.into())
}

fn text(field: capnp::Result<capnp::text::Reader<'_>>) -> Option<&str> {
    field.ok().and_then(|t| t.to_str().ok())
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
fn plan<'a>(programs: &[catalog_entry::Reader<'a>], wanted: capnp::struct_list::Reader<'a, wanted::Owned>) -> Result<Vec<Planned<'a>>, Refused> {
    let bad = |what: &str| refused(Status::BadRequest, format!("malformed: {what}"));
    let find = |name: &str| programs.iter().copied().find(|&p| name_of(p) == name).ok_or_else(|| refused(Status::NoSuchProgram, format!("no program {name}")));
    let mut planned = Vec::new();
    for w in wanted.iter() {
        let mut bindings = Vec::new();
        for b in w.get_routes().map_err(|_| bad("routes"))? {
            let (field, component, export) = (text(b.get_field()), text(b.get_component()), text(b.get_export()));
            bindings.push((field.ok_or_else(|| bad("a route"))?.into(), component.ok_or_else(|| bad("a route"))?.into(), export.ok_or_else(|| bad("a route"))?.into()));
        }
        planned.push(Planned {
            name: text(w.get_name()).ok_or_else(|| bad("a name"))?.into(),
            program: find(text(w.get_program()).ok_or_else(|| bad("a program"))?)?,
            settings: w.get_settings().unwrap_or(&[]),
            on_end: w.get_on_end().unwrap_or(OnEnd::Forget),
            bindings,
            routes: Vec::new(),
        });
    }
    // Each instance's imports, in its manifest's order; providers pulled in
    // are planned after it, and resolved in turn.
    let mut next = 0;
    while next < planned.len() {
        let mut routes = Vec::new();
        let imports = manifest_of(planned[next].program).get_imports().map_err(|_| bad("a manifest"))?;
        for (field, ..) in &planned[next].bindings {
            if !imports.iter().any(|i| text(i.get_field()) == Some(field) && matches!(i.which(), Ok(import::Interface(_)))) {
                return Err(refused(Status::BadRequest, format!("{}: no import {field} to bind", planned[next].name)));
            }
        }
        for import in imports {
            let field = text(import.get_field()).unwrap_or("?");
            let bound = planned[next].bindings.iter().find(|(f, ..)| f == field).map(|(_, c, e)| (c.clone(), e.clone()));
            routes.push(match import.which().map_err(|_| bad("an import"))? {
                import::Interface(interface) => match bound {
                    Some((component, export)) => match planned.iter().position(|p| p.name == component) {
                        Some(i) => To::Export(i, export, interface),
                        None => To::External(component, export, interface),
                    },
                    None => match provider(&planned, programs, interface, next) {
                        Ok(Provider::Instance(i, export)) => To::Export(i, export, interface),
                        Ok(Provider::Program(p, export)) => {
                            // Services pulled in restart when they fail.
                            let program = programs[p];
                            let name = name_of(program).to_string();
                            planned.push(Planned { name, program, settings: &[], on_end: OnEnd::Restart, bindings: Vec::new(), routes: Vec::new() });
                            To::Export(planned.len() - 1, export, interface)
                        }
                        Err(why) => {
                            return Err(refused(Status::Unresolved, format!("{}.{field}: {why} (interface {interface:#x})", planned[next].name)));
                        }
                    },
                },
                import::Serves(export) => To::Serve(text(export).ok_or_else(|| bad("an export"))?.to_string()),
                import::Held(held) => match text(held).ok_or_else(|| bad("a held name"))? {
                    "supervisor" => To::Supervisor,
                    held => To::Held(held.to_string()),
                },
            });
        }
        planned[next].routes = routes;
        next += 1;
    }
    Ok(planned)
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

/// A pointer of a struct's pointer section moved `by` words before its
/// target: its offset grows by as much.
fn moved(pointer: u64, by: usize) -> u64 {
    match pointer & 3 {
        // Struct and list pointers count from the word after themselves.
        0 | 1 if pointer != 0 => {
            let offset = ((pointer as u32 as i32) >> 2) + by as i32;
            (pointer & !0xffff_ffff) | (((offset << 2) as u32 as u64) | (pointer & 3))
        }
        // Null, or a capability: nothing to move.
        _ => pointer,
    }
}

/// The bootstrap message of `planned`: its settings, every import field set
/// to the import's index, every program-hash field to the program's hash,
/// as a flat message.
fn bootstrap(planned: &Planned, programs: &[catalog_entry::Reader]) -> Result<Vec<u8>, Refused> {
    let manifest = manifest_of(planned.program);
    let (data, pointers) = (manifest.get_data_words() as usize, manifest.get_pointers() as usize);
    let bad = |why: &str| refused(Status::BadRequest, format!("{}: settings {why}", planned.name));
    let mut out = vec![0u64; 1];
    if planned.settings.is_empty() {
        out.resize(1 + data + pointers, 0);
    } else {
        // Canonical settings put the root struct right after its pointer and
        // everything it points to after it, in order, so the struct can grow
        // to the manifest's size in place: what follows moves by as much as
        // the struct grows.
        let settings: Vec<u64> = planned.settings.chunks(8).map(|c| {
            let mut word = [0u8; 8];
            word[..c.len()].copy_from_slice(c);
            u64::from_le_bytes(word)
        }).collect();
        let segment = [sap_ring::as_bytes(&settings)];
        if !Reader::new(SegmentArray::new(&segment), ReaderOptions::new()).is_canonical().unwrap_or(false) {
            return Err(bad("that aren't a canonical message"));
        }
        let (their_data, their_pointers) = root_struct(&settings).ok_or_else(|| bad("that aren't a struct"))?;
        let (d, p) = (their_data.len(), their_pointers.len());
        // An empty struct's pointer may point anywhere (canonically, at
        // itself).
        if (d + p > 0 && their_data.start != 1) || d > data || p > pointers {
            return Err(bad("of another struct"));
        }
        out.extend_from_slice(&settings[their_data]);
        out.resize(1 + data, 0);
        out.extend(settings[their_pointers.clone()].iter().map(|&pointer| moved(pointer, pointers - p)));
        out.resize(1 + data + pointers, 0);
        if d + p > 0 {
            out.extend_from_slice(&settings[their_pointers.end..]);
        }
    }
    out[0] = ((data as u64) << 32) | ((pointers as u64) << 48);
    for (index, import) in manifest.get_imports().map_err(|_| bad("with a malformed manifest"))?.iter().enumerate() {
        let offset = import.get_offset() as usize;
        if offset / 2 >= data {
            return Err(bad("with an import outside the struct"));
        }
        let shift = 32 * (offset % 2);
        out[1 + offset / 2] = (out[1 + offset / 2] & !(0xffff_ffff << shift)) | ((index as u64) << shift);
    }
    // Program hashes, each a byte list after everything else; a list
    // pointer's offset counts from the word after the pointer.
    for hash in manifest.get_program_hashes().map_err(|_| bad("with a malformed manifest"))? {
        let wanted = text(hash.get_program()).ok_or_else(|| bad("with a malformed manifest"))?;
        let program = programs.iter().find(|&&p| name_of(p) == wanted).ok_or_else(|| refused(Status::NoSuchProgram, format!("no program {wanted}")))?;
        let bytes = program.get_hash().map_err(|_| bad("with a malformed catalog"))?;
        let at = 1 + data + hash.get_pointer() as usize;
        let start = out.len();
        out.resize(start + bytes.len().div_ceil(8), 0);
        for (j, &byte) in bytes.iter().enumerate() {
            out[start + j / 8] |= (byte as u64) << (8 * (j % 8));
        }
        out[at] = 1 | (((start - at - 1) as u64) << 2) | (2 << 32) | ((bytes.len() as u64) << 35);
    }
    Ok(out.iter().flat_map(|w| w.to_le_bytes()).collect())
}

/// Add `planned[index]` to the composition `compositions`.
fn add(compositions: usize, planned: &[Planned], index: usize, bootstrap: &[u8]) -> Result<(), Refused> {
    let p = &planned[index];
    let manifest = manifest_of(p.program);
    let malformed = || refused(Status::BadRequest, format!("{}: a malformed manifest", p.name));
    let mut scratch = vec![0u8; MAX_MESSAGE_LENGTH * 8];
    let mut allocator = SingleSegmentAllocator::new(&mut scratch);
    let mut message = Builder::new(&mut allocator);
    let mut component = message.init_root::<add_params::Builder>().init_component();
    component.set_name(p.name.as_str());
    component.set_program(p.program.get_hash().map_err(|_| malformed())?);
    component.set_bootstrap(bootstrap);
    component.set_on_end(p.on_end);
    component.set_limits(manifest.get_limits().map_err(|_| malformed())?).map_err(|_| malformed())?;
    let exports = manifest.get_exports().map_err(|_| malformed())?;
    let mut list = component.reborrow().init_exports(exports.len());
    for (i, e) in exports.iter().enumerate() {
        let mut export = list.reborrow().get(i as u32);
        export.set_name(e.get_name().map_err(|_| malformed())?);
        export.set_interface(e.get_interface());
    }
    let mut routes = component.init_imports(p.routes.len() as u32);
    for (i, to) in p.routes.iter().enumerate() {
        let mut route = routes.reborrow().get(i as u32);
        match to {
            To::Export(provider, name, interface) => {
                let mut client = route.init_client();
                client.set_component(planned[*provider].name.as_str());
                client.set_name(name.as_str());
                client.set_interface(*interface);
            }
            To::External(component, name, interface) => {
                let mut client = route.init_client();
                client.set_component(component.as_str());
                client.set_name(name.as_str());
                client.set_interface(*interface);
            }
            To::Serve(name) => route.set_serve(name.as_str()),
            To::Held(name) => route.init_held().set_name(name.as_str()),
            To::Supervisor => route.set_supervisor(()),
        }
    }
    let mut out = [0usize; MAX_MESSAGE_LENGTH];
    let n = words::pack(&message, &mut out).ok_or_else(|| refused(Status::BadRequest, format!("{} is too large for one call", p.name)))?;
    let reply = endpoint::call(compositions, methods::compositions::ADD, &out[..n], &[], &mut [])
        .map_err(|e| refused(Status::Refused, format!("adding {}: {e:?}", p.name)))?;
    let label = reply.info.label();
    if label != supervisor_capnp::Status::Ok as usize {
        let status = u16::try_from(label).ok().and_then(|l| supervisor_capnp::Status::try_from(l).ok());
        return Err(refused(Status::Refused, format!("the composition refused {}: {status:?}", p.name)));
    }
    Ok(())
}

/// Plan what `request` wants and add it all to `compositions`.
fn resolve(programs: &[catalog_entry::Reader], request: request::Reader, compositions: usize) -> Result<(), Refused> {
    let components = request.get_components().map_err(|_| refused(Status::BadRequest, "malformed: components"))?;
    let planned = plan(programs, components)?;
    // Every bootstrap first: nothing is added unless all of them can be.
    let bootstraps = planned.iter().map(|p| bootstrap(p, programs)).collect::<Result<Vec<_>, _>>()?;
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
        add(compositions, &planned, i, &bootstraps[i])?;
    }
    Ok(())
}

/// Serve `Resolver` on `endpoint`, forever.
fn serve(endpoint_cap: usize, programs: &[catalog_entry::Reader]) -> ! {
    let (page, reply, window) = (slots::alloc().expect("resolver: a slot"), slots::alloc().expect("resolver: a slot"), slots::alloc().expect("resolver: a slot"));
    account::allocate(process_slot::ACCOUNT, 12, process_slot::CNODE, page).expect("resolver: a page of its account");
    let root = SlotRef { root: process_slot::CNODE, index: 0, depth: 0 };
    untyped::retype(page, object::REPLY, 0, root, reply, 1).expect("resolver: reply object");
    let mut words = [0usize; MAX_MESSAGE_LENGTH];
    loop {
        endpoint::set_receive_window(process_slot::CNODE, window, 1);
        let message = endpoint::recv(endpoint_cap, reply, &mut words).expect("resolver: receive");
        let words = &words[..message.info.length()];
        let result = if message.info.label() != methods::resolver::RESOLVE || message.info.extra_caps() != 1 {
            Err(refused(Status::BadRequest, "not a resolve call with one capability"))
        } else {
            let segment = [words::bytes(words)];
            let reader = words::reader(&segment);
            match reader.get_root::<resolve_params::Reader>().and_then(|p| p.get_request()) {
                Ok(request) => resolve(programs, request, window),
                Err(_) => Err(refused(Status::BadRequest, "malformed: the request")),
            }
        };
        let status = match result {
            Ok(()) => Status::Ok,
            Err(Refused(status, why)) => {
                println!("resolver: refused a request: {why}");
                status
            }
        };
        let _ = endpoint::reply(reply, status as usize, &[], &[]);
        let _ = SlotRef::new(process_slot::CNODE, window).delete();
    }
}

#[unsafe(no_mangle)]
fn sap_main(a0: usize, _: usize, _: usize) -> ! {
    // SAFETY: the loader started this process with its startup record.
    let startup = unsafe { startup::record(a0) };
    let free = startup.free;
    runtime::start(Resources { cnode: process_slot::CNODE, slots: free.start..free.end, memory: Memory::Account(process_slot::ACCOUNT), vspace: process_slot::VSPACE, heap: HEAP });
    // The bootstrap message, copied to the heap: aligned words to read.
    let bytes = startup::bootstrap(startup);
    let mut copy = vec![0u64; bytes.len().div_ceil(8)];
    for (i, &byte) in bytes.iter().enumerate() {
        copy[i / 8] |= (byte as u64) << (8 * (i % 8));
    }
    let segment = [sap_ring::as_bytes(&copy)];
    let mut options = ReaderOptions::new();
    options.traversal_limit_in_words(Some(1 << 20));
    let reader = Reader::new(SegmentArray::new(&segment), options);
    let imports = reader.get_root::<resolver_bootstrap::Reader>().expect("resolver: bootstrap message");
    let compositions = startup::import(startup, imports.get_compositions()).expect("resolver: no compositions import");
    let endpoint_cap = startup::import(startup, imports.get_endpoint()).expect("resolver: no endpoint import");
    let programs: Vec<catalog_entry::Reader> = imports.get_programs().expect("resolver: programs").iter().collect();

    // The boot request, into the supervisor's own composition, started.
    let request = imports.get_request().expect("resolver: request");
    if let Err(Refused(_, why)) = resolve(&programs, request, compositions) {
        panic!("resolver: the boot request: {why}");
    }
    let reply = endpoint::call(compositions, methods::compositions::ENACT, &[], &[], &mut []).expect("resolver: enact");
    assert_eq!(reply.info.label(), supervisor_capnp::Status::Ok as usize, "resolver: enacting failed");
    serve(endpoint_cap, &programs)
}
