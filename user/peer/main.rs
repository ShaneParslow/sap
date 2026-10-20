//! A second program, loaded by the root task into its own process, to check
//! IPC between address spaces. Its third argument picks what it does:
//! - 0: a call over an endpoint (slot `a0`) and an isolation check;
//! - 1: echo word records over a channel of two rings;
//! - 2: serve the echo interface (`interfaces/echo.capnp`) over a channel;
//! - 3: answer one request, then break the channel's rules;
//! - 4: sum the bytes of each record or page transfer (for measuring).
//!
//! In the channel modes, `a0` is the number of data pages per ring. It runs
//! without a heap: Cap'n Proto messages are built in a fixed buffer.

#![no_std]
#![no_main]
#![feature(strict_provenance_lints)]
// Pointers come from `sap_sys::address::mapped`, never from integer casts.
#![deny(implicit_provenance_casts, integer_to_ptr_transmutes)]

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering};

use capnp::message::{Builder, Reader, ReaderOptions, SegmentArray, SingleSegmentAllocator};
use sap_interfaces::echo::{BYTES, ERROR, REPLY, REQUEST, STOP, SUM, TRANSFER, WORDS};
use sap_interfaces::echo_capnp::{reply, request};
use sap_ring::transfer::{Frames, Receiver, TransferError};
use sap_ring::{as_bytes, as_bytes_mut, Consumer, Layout, Producer, RingError, PAGE};
use sap_rt::address::mapped;
use sap_rt::{ipc, notification, println, tcb};

/// An address mapped only in the root task's address space.
const PARENT_ONLY: usize = 0x38_0000_0000;

/// Where the parent maps the channel in this process (init's check uses
/// the same address), and the capability slots it fills for channel modes.
const RING_BASE: usize = 0x20_0000_0000;
const MY_NOTIFICATION: usize = 1;
const PARENT_DOORBELL: usize = 3;
/// For page transfers: the endpoint the parent calls, a reply object, this
/// process's address space and CNode, the slots transferred frames arrive
/// in, and where they are mapped (the parent adds the page tables).
const MY_ENDPOINT: usize = 4;
const MY_REPLY: usize = 5;
const MY_VSPACE: usize = 6;
const MY_CNODE: usize = 7;
const TRANSFER_SLOTS: usize = 16;
const TRANSFER_BASE: usize = RING_BASE + 0x10_0000;

#[unsafe(no_mangle)]
fn sap_main(a0: usize, me: usize, mode: usize) -> ! {
    match mode {
        1 => echo_words(channel(a0)),
        2 => serve_echo(channel(a0)),
        3 => misbehave(channel(a0), a0),
        4 => sum_bytes(channel(a0)),
        _ => call_parent(a0),
    }
    loop {
        let _ = tcb::suspend(me);
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
fn channel(data_pages: usize) -> (Consumer, Producer) {
    let inbound = Layout::new(RING_BASE, data_pages);
    let outbound = Layout::new(RING_BASE + inbound.pages() * PAGE, data_pages);
    // SAFETY: the parent mapped both rings this way before starting us.
    unsafe {
        (
            Consumer::new(inbound, PARENT_DOORBELL, MY_NOTIFICATION),
            Producer::new(outbound, PARENT_DOORBELL, MY_NOTIFICATION),
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
fn transfer_receiver() -> Receiver {
    let frames = Frames { cnode: MY_CNODE, first: TRANSFER_SLOTS, vspace: MY_VSPACE, base: TRANSFER_BASE };
    // SAFETY: the parent leaves those slots empty and that range unused,
    // with page tables in place.
    let receiver = unsafe { Receiver::new(frames) };
    receiver.arm();
    receiver
}

/// Take the page transfer that a `TRANSFER` record with `id` announced, lend
/// its message to `read`, and give the frames back.
fn take_transfer<R>(receiver: &Receiver, id: u64, read: impl FnOnce(&[u8]) -> R) -> Result<R, TransferError> {
    let mut words = [0usize; 2];
    let call = ipc::recv(MY_ENDPOINT, MY_REPLY, &mut words).map_err(TransferError::Sys)?;
    let (len, sent_id) = Receiver::header(&words);
    let count = call.info.extra_caps();
    let result = if call.info.length() == 2 && sent_id == id { receiver.read(count, len, read) } else { Err(TransferError::BadHeader) };
    let _ = receiver.give_back(MY_REPLY, count);
    if result.is_err() {
        // Anything the caller couldn't take back.
        receiver.clear();
    }
    result
}

/// Answer each request with a reply that summarises it, or an error record
/// if it can't be read, until a stop record. Requests too large for the
/// ring arrive by page transfer and are read in place.
fn serve_echo((mut inbound, mut outbound): (Consumer, Producer)) {
    let mut request_words = [0u64; 256];
    let mut scratch = [0u64; 256];
    let mut allocator = SingleSegmentAllocator::new(as_bytes_mut(&mut scratch));
    let receiver = transfer_receiver();
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
                match take_transfer(&receiver, record.id, |body| answer(body, &mut message)) {
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
fn sum_bytes((mut inbound, mut outbound): (Consumer, Producer)) {
    // SAFETY: this function runs once, on this process's only thread.
    let buffer = unsafe { &mut *SUM_BUFFER.0.get() };
    let receiver = transfer_receiver();
    loop {
        let record = inbound.recv(buffer).expect("peer: ring");
        let sum = match record.kind {
            STOP => {
                outbound.send(STOP, record.id, &[]);
                break;
            }
            BYTES => checksum(&as_bytes(buffer)[..record.len]),
            TRANSFER => take_transfer(&receiver, record.id, checksum).expect("peer: transfer"),
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
fn misbehave((mut inbound, mut outbound): (Consumer, Producer), data_pages: usize) {
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
    let _ = notification::signal(PARENT_DOORBELL);
}
