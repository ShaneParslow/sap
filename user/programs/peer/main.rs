//! A second program, which `check` loads into processes of its own, to check
//! IPC between address spaces. Its bootstrap message (`PeerBootstrap` in
//! `peer.capnp`) picks what it does:
//! - `call`: a call over an endpoint and an isolation check;
//! - `echoWords`: echo word records over a channel of two rings;
//! - `serveEcho`: serve the echo interface (`interfaces/test/echo.capnp`) over a
//!   channel;
//! - `misbehave`: answer one request, then break the channel's rules;
//! - `sumBytes`: sum the bytes of each record or page transfer (for
//!   measuring);
//! - `exit` and `crash`: exit with a code, or fault, for checking how a
//!   process manager hears of either;
//! - `restartable`: serve an export until asked to fault, for checking that
//!   its supervisor restarts it.
//!
//! It runs without a heap: Cap'n Proto messages are built in a fixed buffer.

#![no_std]
#![no_main]
#![feature(strict_provenance_lints)]
// Pointers come from `sap_sys::address::mapped`, never from integer casts.
#![deny(implicit_provenance_casts, integer_to_ptr_transmutes)]

/// Generated from the component schemas of `peer`
/// (`user/programs/<name>/<name>.capnp`).
mod components {
    #[allow(clippy::all, dead_code)]
    pub mod peer_capnp {
        include!(concat!(env!("OUT_DIR"), "/peer/peer_capnp.rs"));
    }
}

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering};

use capnp::message::{Builder, Reader, ReaderOptions, SegmentArray, SingleSegmentAllocator};
use sap_interfaces::echo::{BYTES, ERROR, REPLY, REQUEST, STOP, SUM, TRANSFER, WORDS};
use sap_interfaces::echo_capnp::{echo_reply as reply, echo_request as request};
use sap_interfaces::methods::restartable;
use sap_interfaces::restartable_capnp::restartable::count_results;
use components::peer_capnp::peer_bootstrap;
use components::peer_capnp::peer_bootstrap::mode::Which as Mode;
use sap_interfaces::words;
use sap_ring::transfer::{Frames, Receiver, TransferError};
use sap_ring::{as_bytes, as_bytes_mut, Consumer, Layout, Producer, RingError, PAGE};
use sap_process::{process_slot, Startup};
use sap_rt::address::mapped;
use sap_rt::abi::object;
use sap_rt::cnode::SlotRef;
use sap_rt::{account, ipc, notification, println, process, tcb, untyped};

/// An address mapped only in the root task's address space.
const PARENT_ONLY: usize = 0x38_0000_0000;

/// Where the parent maps the channel in this process (init's check uses
/// the same address), and where frames received by page transfer are
/// mapped (the parent adds the page tables).
const RING_BASE: usize = 0x20_0000_0000;
const TRANSFER_BASE: usize = RING_BASE + 0x10_0000;

/// What the bootstrap message gives: capabilities as slots of this
/// process's CNode.
#[derive(Clone, Copy)]
struct Peer {
    endpoint: usize,
    data_pages: usize,
    notification: usize,
    doorbell: usize,
    reply: usize,
    /// The first of the empty slots, where page transfers arrive.
    free: usize,
}

/// Why the bootstrap message couldn't be used.
#[derive(Debug)]
#[allow(dead_code, reason = "the fields are read by Debug, in the panic message")]
enum BadBootstrap {
    Message(capnp::Error),
    NoImport(u32),
}

impl From<capnp::Error> for BadBootstrap {
    fn from(e: capnp::Error) -> Self {
        Self::Message(e)
    }
}

impl From<capnp::NotInSchema> for BadBootstrap {
    fn from(e: capnp::NotInSchema) -> Self {
        Self::Message(e.into())
    }
}

/// Read the bootstrap message: the mode and its capabilities.
fn bootstrap(startup: &'static Startup) -> Result<(Mode, Peer), BadBootstrap> {
    let segments = [process::bootstrap(startup)];
    let reader = Reader::new(SegmentArray::new(&segments), ReaderOptions::new());
    let bootstrap = reader.get_root::<peer_bootstrap::Reader>()?;
    let import = |index: u32| process::import(startup, index).ok_or(BadBootstrap::NoImport(index));
    let mode = bootstrap.get_mode().which()?;
    let nothing = Peer { endpoint: 0, data_pages: 0, notification: 0, doorbell: 0, reply: 0, free: 0 };
    let channel = bootstrap.get_channel();
    let peer = match mode {
        Mode::Exit(_) | Mode::Crash(()) => nothing,
        Mode::Call(()) | Mode::Restartable(()) => Peer { endpoint: import(bootstrap.get_endpoint())?, free: startup.free.start, ..nothing },
        Mode::EchoWords(()) | Mode::ServeEcho(()) | Mode::Misbehave(()) | Mode::SumBytes(()) => Peer {
            endpoint: import(bootstrap.get_endpoint())?,
            data_pages: channel.get_data_pages() as usize,
            notification: import(channel.get_notification())?,
            doorbell: import(channel.get_doorbell())?,
            reply: import(channel.get_reply())?,
            free: startup.free.start,
        },
    };
    Ok((mode, peer))
}

#[unsafe(no_mangle)]
fn sap_main(a0: usize, _: usize, _: usize) -> ! {
    // SAFETY: the loader started this process with its startup record.
    let startup = unsafe { process::startup(a0) };
    let (mode, peer) = bootstrap(startup).expect("peer: bootstrap message");
    match mode {
        Mode::Call(()) => call_parent(peer.endpoint),
        Mode::EchoWords(()) => echo_words(channel(peer)),
        Mode::ServeEcho(()) => serve_echo(peer, channel(peer)),
        Mode::Misbehave(()) => misbehave(peer, channel(peer)),
        Mode::SumBytes(()) => sum_bytes(peer, channel(peer)),
        Mode::Exit(code) => process::exit(code as usize),
        Mode::Crash(()) => crash(),
        Mode::Restartable(()) => restartable(peer),
    }
    loop {
        let _ = tcb::suspend(process_slot::TCB);
    }
}

fn crash() {
    // SAFETY: deliberately reads an address nothing is mapped at.
    let _ = unsafe { core::ptr::read_volatile(mapped::<usize>(0x1000)) };
}

/// Serve `Restartable` on `peer.endpoint`, for checking restart: `count`
/// answers how many calls this process has answered, `crash` faults. Its
/// supervisor starts it again with a count of 0.
fn restartable(peer: Peer) {
    let (page, answer) = (peer.free, peer.free + 1);
    account::allocate(process_slot::ACCOUNT, 12, process_slot::CNODE, page).expect("peer: a page of its account");
    let root = SlotRef { root: process_slot::CNODE, index: 0, depth: 0 };
    untyped::retype(page, object::REPLY, 0, root, answer, 1).expect("peer: reply object");
    let mut answered = 0;
    loop {
        let message = ipc::recv(peer.endpoint, answer, &mut []).expect("peer: receive");
        match message.info.label() {
            restartable::COUNT => {
                answered += 1;
                let mut scratch = [0u64; 4];
                let mut allocator = SingleSegmentAllocator::new(as_bytes_mut(&mut scratch));
                let mut results = Builder::new(&mut allocator);
                results.init_root::<count_results::Builder>().set_calls(answered);
                let mut out = [0usize; 4];
                let n = words::pack(&results, &mut out).expect("peer: count results fit");
                let _ = ipc::reply(answer, 0, &out[..n], &[]);
            }
            restartable::CRASH => crash(),
            _ => {
                let _ = ipc::reply(answer, 1, &[], &[]);
            }
        }
    }
}

/// A call across address spaces, with words through both IPC buffers, then
/// a read of the parent's memory, which faults to the parent.
fn call_parent(ep: usize) {
    println!("peer: running in its own address space");
    let mut out = [0usize; 6];
    let reply = ipc::call(ep, 1, &[1, 2, 3, 4, 5, 6], &[], &mut out).expect("peer: call");
    let sum: usize = out[..reply.info.length()].iter().sum();
    ipc::call(ep, 2, &[sum], &[], &mut []).expect("peer: report");
    // SAFETY: deliberately touches an address this process can't reach.
    let _ = unsafe { core::ptr::read_volatile(mapped::<usize>(PARENT_ONLY)) };
    println!("peer: read the parent's memory (isolation broken)");
}

/// The channel's two rings: inbound from the parent, then outbound.
fn channel(peer: Peer) -> (Consumer, Producer) {
    let inbound = Layout::new(RING_BASE, peer.data_pages);
    let outbound = Layout::new(RING_BASE + inbound.pages() * PAGE, peer.data_pages);
    // SAFETY: the parent mapped both rings this way before starting us.
    unsafe {
        (
            Consumer::new(inbound, peer.doorbell, peer.notification),
            Producer::new(outbound, peer.doorbell, peer.notification),
        )
    }
}

/// Echo word records with every word incremented, until a stop record.
fn echo_words((mut inbound, mut outbound): (Consumer, Producer)) {
    let mut words = [0u64; 8];
    loop {
        let record = inbound.recv(&mut words).expect("peer: ring");
        if record.kind == STOP {
            outbound.send(STOP, record.id, &[]);
            break;
        }
        let n = record.len / 8;
        for w in &mut words[..n] {
            *w += 1;
        }
        outbound.send(WORDS, record.id, &as_bytes(&words)[..record.len]);
    }
    println!("peer: word echo done, slept {} time(s) waiting for records", inbound.sleeps);
}

/// The receiving side of page transfers, armed to receive.
fn transfer_receiver(peer: Peer) -> Receiver {
    let frames = Frames { cnode: process_slot::CNODE, first: peer.free, vspace: process_slot::VSPACE, base: TRANSFER_BASE };
    // SAFETY: the startup record says those slots are empty, and the parent
    // leaves that range unused, with page tables in place.
    let receiver = unsafe { Receiver::new(frames) };
    receiver.arm();
    receiver
}

/// Take the page transfer that a `TRANSFER` record with `id` announced, lend
/// its message to `read`, and give the frames back.
fn take_transfer<R>(peer: Peer, receiver: &Receiver, id: u64, read: impl FnOnce(&[u8]) -> R) -> Result<R, TransferError> {
    let mut words = [0usize; 2];
    let call = ipc::recv(peer.endpoint, peer.reply, &mut words).map_err(TransferError::Sys)?;
    let (len, sent_id) = Receiver::header(&words);
    let count = call.info.extra_caps();
    let result = if call.info.length() == 2 && sent_id == id { receiver.read(count, len, read) } else { Err(TransferError::BadHeader) };
    let _ = receiver.give_back(peer.reply, count);
    if result.is_err() {
        // Anything the caller couldn't take back.
        receiver.clear();
    }
    result
}

/// Answer each request with a reply that summarises it, or an error record
/// if it can't be read, until a stop record. Requests too large for the
/// ring arrive by page transfer and are read in place.
fn serve_echo(peer: Peer, (mut inbound, mut outbound): (Consumer, Producer)) {
    let mut request_words = [0u64; 256];
    let mut scratch = [0u64; 256];
    let mut allocator = SingleSegmentAllocator::new(as_bytes_mut(&mut scratch));
    let receiver = transfer_receiver(peer);
    loop {
        let record = match inbound.recv(&mut request_words) {
            Ok(record) => record,
            Err(RingError::TooLong(record)) => {
                outbound.send(ERROR, record.id, &[]);
                continue;
            }
            Err(RingError::Corrupt) => panic!("peer: the parent broke the ring"),
        };
        match record.kind {
            STOP => {
                outbound.send(STOP, record.id, &[]);
                break;
            }
            REQUEST => {
                let mut message = Builder::new(&mut allocator);
                match answer(&as_bytes(&request_words)[..record.len], &mut message) {
                    Ok(()) => outbound.send(REPLY, record.id, message.get_segments_for_output()[0]),
                    Err(()) => outbound.send(ERROR, record.id, &[]),
                }
            }
            TRANSFER => {
                let mut message = Builder::new(&mut allocator);
                match take_transfer(peer, &receiver, record.id, |body| answer(body, &mut message)) {
                    Ok(Ok(())) => outbound.send(REPLY, record.id, message.get_segments_for_output()[0]),
                    _ => outbound.send(ERROR, record.id, &[]),
                }
            }
            _ => outbound.send(ERROR, record.id, &[]),
        }
    }
    println!("peer: echo service done, slept {} time(s) waiting for requests", inbound.sleeps);
}

/// A buffer for `sum_bytes`, too large for the stack.
struct Buffer(UnsafeCell<[u64; 8192]>);
// SAFETY: used by `sum_bytes` only, on this process's only thread.
unsafe impl Sync for Buffer {}
static SUM_BUFFER: Buffer = Buffer(UnsafeCell::new([0; 8192]));

/// The sum of `bytes` taken as little-endian words (the tail as bytes).
fn checksum(bytes: &[u8]) -> u64 {
    let (words, tail) = bytes.as_chunks::<8>();
    let tail = tail.iter().map(|&b| u64::from(b)).fold(0, u64::wrapping_add);
    words.iter().map(|&w| u64::from_le_bytes(w)).fold(tail, u64::wrapping_add)
}

/// Answer each `BYTES` record, copied out of the ring, and each page
/// transfer, read in place, with the checksum of its bytes, until a stop
/// record. The parent times the two.
fn sum_bytes(peer: Peer, (mut inbound, mut outbound): (Consumer, Producer)) {
    // SAFETY: this function runs once, on this process's only thread.
    let buffer = unsafe { &mut *SUM_BUFFER.0.get() };
    let receiver = transfer_receiver(peer);
    loop {
        let record = inbound.recv(buffer).expect("peer: ring");
        let sum = match record.kind {
            STOP => {
                outbound.send(STOP, record.id, &[]);
                break;
            }
            BYTES => checksum(&as_bytes(buffer)[..record.len]),
            TRANSFER => take_transfer(peer, &receiver, record.id, checksum).expect("peer: transfer"),
            _ => panic!("peer: unexpected record"),
        };
        outbound.send(SUM, record.id, &sum.to_le_bytes());
    }
}

/// Read a request from `body` and build its reply in `message`.
fn answer(body: &[u8], message: &mut Builder<&mut SingleSegmentAllocator>) -> Result<(), ()> {
    let segments = [body];
    let reader = Reader::new(SegmentArray::new(&segments), ReaderOptions::new());
    let request = reader.get_root::<request::Reader>().map_err(drop)?;
    let text = request.get_text().map_err(drop)?.to_str().map_err(drop)?;
    let mut reversed = [0u8; 512];
    if text.len() > reversed.len() {
        return Err(());
    }
    let mut n = 0;
    for c in text.chars().rev() {
        n += c.encode_utf8(&mut reversed[n..]).len();
    }
    let sum = request.get_numbers().map_err(drop)?.iter().map(u64::from).sum();
    let blob = request.get_blob().map_err(drop)?;
    let mut reply = message.init_root::<reply::Builder>();
    reply.set_id(request.get_id());
    reply.set_text(core::str::from_utf8(&reversed[..n]).map_err(drop)?);
    reply.set_sum(sum);
    reply.set_blob_length(blob.len() as u32);
    reply.set_blob_sum(blob.iter().map(|&b| u64::from(b)).sum());
    Ok(())
}

/// Answer the first request with a reply that isn't Cap'n Proto, then
/// publish a record that claims to be longer than what was published.
fn misbehave(peer: Peer, (mut inbound, mut outbound): (Consumer, Producer)) {
    let data_pages = peer.data_pages;
    let mut words = [0u64; 256];
    let record = inbound.recv(&mut words).expect("peer: ring");
    outbound.send(REPLY, record.id, &[0xff; 64]);
    // The outbound ring's producer page and data, written by hand.
    let layout = Layout::new(RING_BASE + Layout::new(RING_BASE, data_pages).pages() * PAGE, data_pages);
    // SAFETY: this process's own producer page of the outbound ring.
    let produced = unsafe { &*mapped::<AtomicUsize>(layout.base) };
    let at = produced.load(Ordering::Relaxed);
    let capacity = data_pages * PAGE / 8;
    // A header claiming a 1 KiB body, but only the header is published.
    // SAFETY: inside the outbound ring's data pages, which this side writes.
    unsafe { mapped::<u64>(layout.base + PAGE + (at % capacity) * 8).write_volatile((u64::from(REPLY) << 32) | 1024) };
    produced.store(at + 2, Ordering::Release);
    let _ = notification::signal(peer.doorbell);
}
