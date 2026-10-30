//! The log service: writes what components send to the log
//! (`interfaces/log.capnp`) to the kernel's debug console, each line after
//! the name of the component that wrote it. A call's badge says which
//! client it is; the supervisor, which made each client's capability for
//! its route, says which component a badge is (`launcher::CLIENT`). The
//! service asks once per client and remembers.
//!
//! It runs without a heap.
//! TODO: own the console (the UART, from the device supervisor) instead of
//! the debug syscall; levels, a ring of recent lines, readers (I6).

#![no_std]
#![no_main]
#![feature(strict_provenance_lints)]
// Pointers come from `sap_sys::address::mapped`, never from integer casts.
#![deny(implicit_provenance_casts, integer_to_ptr_transmutes)]

use core::fmt::Write;

use capnp::message::{Reader, ReaderOptions, SegmentArray};
use sap_interfaces::log_capnp::log::write_params;
use sap_interfaces::log_capnp::log_imports;
use sap_interfaces::{methods, words};
use sap_process::{launcher, process_slot, Startup};
use sap_rt::abi::{object, MAX_MESSAGE_LENGTH};
use sap_rt::cnode::SlotRef;
use sap_rt::debug::Console;
use sap_rt::{account, ipc, process, untyped};

/// How many clients the service remembers the names of; later ones are
/// asked about on every call.
const MAX_CLIENTS: usize = 16;

/// Reply labels.
const OK: usize = 0;
const BAD_REQUEST: usize = 1;

struct Client {
    badge: usize,
    name: [u8; launcher::MAX_NAME],
    len: usize,
    /// Whether its next character starts a line.
    at_line_start: bool,
}

impl Client {
    /// Ask the supervisor which component `badge` is; `None` if it says
    /// none.
    fn ask(badge: usize) -> Option<Self> {
        let mut out = [0usize; 1 + launcher::MAX_NAME / 8];
        let reply = ipc::call(process_slot::LAUNCHER, launcher::CLIENT, &[badge], &[], &mut out).ok()?;
        if reply.info.label() != 0 {
            return None;
        }
        let len = out[0].min(launcher::MAX_NAME).min((reply.info.length().saturating_sub(1)) * 8);
        let mut name = [0; launcher::MAX_NAME];
        for (i, byte) in name.iter_mut().enumerate().take(len) {
            *byte = (out[1 + i / 8] >> (8 * (i % 8))) as u8;
        }
        Some(Self { badge, name, len, at_line_start: true })
    }

    fn name(&self) -> &str {
        core::str::from_utf8(&self.name[..self.len]).unwrap_or("?")
    }

    /// Write `text` to the console, each line after this client's name.
    fn write(&mut self, text: &str) {
        for line in text.split_inclusive('\n') {
            if self.at_line_start {
                let _ = write!(Console, "{}: ", self.name());
            }
            let _ = Console.write_str(line);
            self.at_line_start = line.ends_with('\n');
        }
    }
}

/// The receive side of the log's endpoint, from the bootstrap message.
fn endpoint(startup: &'static Startup) -> Result<usize, capnp::Error> {
    let segments = [process::bootstrap(startup)];
    let reader = Reader::new(SegmentArray::new(&segments), ReaderOptions::new());
    let imports = reader.get_root::<log_imports::Reader>()?;
    Ok(process::import(startup, imports.get_endpoint()).expect("log: no such import"))
}

#[unsafe(no_mangle)]
fn sap_main(a0: usize, _: usize, _: usize) -> ! {
    // SAFETY: the loader started this process with its startup record.
    let startup = unsafe { process::startup(a0) };
    let endpoint = endpoint(startup).expect("log: bootstrap message");
    // A reply object, from a page of the service's account.
    let (page, reply) = (startup.free.start, startup.free.start + 1);
    account::allocate(process_slot::ACCOUNT, 12, process_slot::CNODE, page).expect("log: a page of its account");
    let root = SlotRef { root: process_slot::CNODE, index: 0, depth: 0 };
    untyped::retype(page, object::REPLY, 0, root, reply, 1).expect("log: reply object");

    let mut clients: [Option<Client>; MAX_CLIENTS] = [const { None }; MAX_CLIENTS];
    let mut words = [0usize; MAX_MESSAGE_LENGTH];
    loop {
        let message = ipc::recv(endpoint, reply, &mut words).expect("log: receive");
        let words = &words[..message.info.length()];
        let label = if message.info.label() == methods::log::WRITE { write(&mut clients, message.badge, words) } else { BAD_REQUEST };
        let _ = ipc::reply(reply, label, &[], &[]);
    }
}

/// Serve a `write` from the client with `badge`; the reply's label.
fn write(clients: &mut [Option<Client>; MAX_CLIENTS], badge: usize, words: &[usize]) -> usize {
    let segment = [words::bytes(words)];
    let reader = words::reader(&segment);
    let Some(text) = reader.get_root::<write_params::Reader>().and_then(|t| t.get_text()).ok().and_then(|t| t.to_str().ok()) else {
        return BAD_REQUEST;
    };
    if let Some(client) = clients.iter_mut().flatten().find(|c| c.badge == badge) {
        client.write(text);
        return OK;
    }
    let Some(mut client) = Client::ask(badge) else {
        return BAD_REQUEST;
    };
    client.write(text);
    if let Some(free) = clients.iter_mut().find(|c| c.is_none()) {
        *free = Some(client);
    }
    OK
}
