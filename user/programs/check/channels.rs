//! Channels between processes: shared-memory rings, Cap'n Proto messages
//! over them, page transfer and its cost, and Cap'n Proto on its own.

use alloc::format;
use alloc::vec::Vec;
use sap_process::process_slot;
use sap_rt::abi::{attr, object, rights};
use sap_rt::address::mapped;
use sap_rt::cnode::SlotRef;
use sap_rt::{ipc, println, slots, sys, untyped, vspace};
use capnp::message::{Builder, Reader, ReaderOptions, SegmentArray, SingleSegmentAllocator};
use sap_interfaces::echo;
use sap_interfaces::echo_capnp::{reply, request};
use sap_interfaces::peer_capnp::Mode;
use sap_loader::{self as loader, Give, Launch, Parent, Slots, LAUNCH};
use sap_ring::transfer::{Frames, Sender, TransferError, MAX_LEN, MAX_PAGES};
use sap_ring::{as_bytes, as_bytes_mut, Consumer, Layout as RingLayout, Producer, RingError, PAGE};
use crate::{Env, expect, slot, untyped_at_least};
use crate::processes::{PEER, peer_bootstrap};

/// Where a peer maps its channel (init maps each at an address of its own).
const CHANNEL_THERE: usize = 0x20_0000_0000;

/// Where a peer maps frames it receives by page transfer.
const TRANSFER_THERE: usize = CHANNEL_THERE + 0x10_0000;
/// Where init maps the frames it sends by page transfer.
const TRANSFER_HERE: usize = 0x2c_0000_0000;

/// One end of a channel: a ring out and a ring back, and the peer's
/// endpoint for page transfers.
struct Channel {
    out: Producer,
    back: Consumer,
    ep: usize,
}

/// Load a peer in `mode` and connect it by a channel of two rings with
/// `data_pages` data pages each, mapped here at `here`. Everything is made
/// from `parent.untyped`, so revoking it removes the peer and the channel.
fn connect_peer(parent: &Parent, slots: &mut Slots, mode: Mode, data_pages: usize, here: usize) -> Channel {
    let in_root = SlotRef { root: process_slot::CNODE, index: 0, depth: 0 };
    let mut take = || {
        slots.next += 1;
        slots.next - 1
    };
    // A notification each, and a badged capability to signal the other.
    let (mine, theirs, my_bell, their_bell) = (take(), take(), take(), take());
    expect("notification", untyped::retype(parent.untyped, object::NOTIFICATION, 0, in_root, mine, 1), Ok(()));
    expect("notification", untyped::retype(parent.untyped, object::NOTIFICATION, 0, in_root, theirs, 1), Ok(()));
    expect("my doorbell", slot(my_bell).mint_from(slot(mine), rights::WRITE, 1), Ok(()));
    expect("their doorbell", slot(their_bell).mint_from(slot(theirs), rights::WRITE, 1), Ok(()));

    // An endpoint the peer receives page transfers on, and a reply object
    // for it to answer them with.
    let (ep, reply) = (take(), take());
    expect("endpoint", untyped::retype(parent.untyped, object::ENDPOINT, 0, in_root, ep, 1), Ok(()));
    expect("reply object", untyped::retype(parent.untyped, object::REPLY, 0, in_root, reply, 1), Ok(()));

    let (bootstrap, len) = peer_bootstrap(mode, data_pages);
    let imports = [(ep, Give::Copy(rights::READ)), (theirs, Give::Copy(rights::READ)), (my_bell, Give::Copy(rights::WRITE)), (reply, Give::Copy(rights::ALL))];
    let launch = Launch { priority: 255, imports: &imports, bootstrap: &bootstrap[..len], ..LAUNCH };
    let peer = loader::load(PEER, parent, slots, &launch).expect("load the peer");
    let out = RingLayout::new(here, data_pages);
    let back = RingLayout::new(here + out.pages() * PAGE, data_pages);
    let offset = CHANNEL_THERE.wrapping_sub(here);
    let (ro, rw) = (rights::READ, rights::READ | rights::WRITE);
    // Each page is writable only by the side that owns its contents: init
    // produces on `out` and consumes on `back`.
    for (layout, init_produces) in [(out, true), (back, false)] {
        for i in 0..layout.pages() {
            let (frame, copy) = (slots.next, slots.next + 1);
            slots.next += 2;
            expect("ring page", untyped::retype(parent.untyped, object::FRAME_4K, 0, in_root, frame, 1), Ok(()));
            expect("copy for the peer", slot(copy).copy_from(slot(frame), rights::ALL), Ok(()));
            let init_writes = layout.producer_writes(i) == init_produces;
            let va = layout.base + i * PAGE;
            let new_table = || {
                let table = slots::alloc().expect("slot");
                untyped::retype(parent.untyped, object::PAGE_TABLE, 0, in_root, table, 1).map(|()| table)
            };
            let mapped = vspace::map_frame_with_tables(frame, process_slot::VSPACE, va, if init_writes { rw } else { ro }, attr::EXECUTE_NEVER, new_table);
            expect("map here", mapped, Ok(()));
            peer.map_frame(parent, slots, copy, va.wrapping_add(offset), if init_writes { ro } else { rw }).expect("map for the peer");
        }
    }
    peer.map_tables(parent, slots, TRANSFER_THERE).expect("page tables for transfers");
    peer.start().expect("start the peer");
    // SAFETY: both rings are mapped here with the rights each end needs,
    // and this thread is the only user of these ends.
    unsafe { Channel { out: Producer::new(out, their_bell, mine), back: Consumer::new(back, their_bell, mine), ep } }
}

/// `MAX_PAGES` frames in slots from `first`, for sending by page transfer
/// from `TRANSFER_HERE`; each is mapped once there, so the page tables
/// exist, then unmapped.
fn transfer_sender(parent: &Parent, slots: &mut Slots) -> (Sender, usize) {
    let in_root = SlotRef { root: process_slot::CNODE, index: 0, depth: 0 };
    let first = slots.next;
    slots.next += MAX_PAGES;
    expect("transfer frames", untyped::retype(parent.untyped, object::FRAME_4K, 0, in_root, first, MAX_PAGES), Ok(()));
    for i in 0..MAX_PAGES {
        let new_table = || {
            slots.next += 1;
            untyped::retype(parent.untyped, object::PAGE_TABLE, 0, in_root, slots.next - 1, 1).map(|()| slots.next - 1)
        };
        let va = TRANSFER_HERE + i * PAGE;
        expect("map once", vspace::map_frame_with_tables(first + i, process_slot::VSPACE, va, rights::READ, attr::EXECUTE_NEVER, new_table), Ok(()));
        expect("unmap", vspace::unmap_frame(first + i), Ok(()));
    }
    let frames = Frames { cnode: process_slot::CNODE, first, vspace: process_slot::VSPACE, base: TRANSFER_HERE };
    // SAFETY: fresh, unmapped frames with no other capabilities, and an
    // address range nothing else in init uses, with its tables in place.
    (unsafe { Sender::new(frames) }, first)
}

/// Untyped memory and slots for the channel checks; revoking the memory
/// removes every peer and channel made from it.
fn channel_resources(info: &Env) -> (Parent, Slots) {
    let first = slots::alloc_range(400).expect("slots");
    let parent = Parent {
        cnode: process_slot::CNODE,
        vspace: process_slot::VSPACE,
        asid_pool: info.asid_pool,
        authority: process_slot::TCB,
        untyped: untyped_at_least(info, 20),
        scratch: 0x38_0000_0000,
        timer_hz: info.timer_hz,
    };
    (parent, Slots { next: first, end: first + 400 })
}

/// Two processes over a channel of two rings, with notifications as
/// doorbells: 10,000 word records echoed by a peer, with at most 48 in
/// flight.
pub fn check_ring(info: &Env) {
    let (parent, mut slots) = channel_resources(info);
    let Channel { mut out, mut back, .. } = connect_peer(&parent, &mut slots, Mode::EchoWords, 1, 0x28_0000_0000);
    const MESSAGES: u64 = 10_000;
    const WINDOW: u64 = 48;
    let (mut sent, mut received, mut checksum) = (0u64, 0u64, 0u64);
    let mut words = [0u64; 2];
    while received < MESSAGES {
        while sent < MESSAGES && sent - received < WINDOW {
            out.send(echo::WORDS, sent, as_bytes(&[sent, sent * 3]));
            sent += 1;
        }
        let record = back.recv(&mut words).expect("ring");
        assert_eq!((record.kind, record.id, record.len), (echo::WORDS, received, 16), "echo record");
        assert_eq!(words, [received + 1, received * 3 + 1], "echo out of order or corrupted");
        checksum = checksum.wrapping_add(words[0]);
        received += 1;
    }
    out.send(echo::STOP, 0, &[]);
    assert_eq!(back.recv(&mut words).map(|r| r.kind), Ok(echo::STOP), "the peer should echo the stop record");
    assert_eq!(checksum, MESSAGES * (MESSAGES + 1) / 2, "checksum");
    sys::yield_now(); // let the peer report and stop
    println!(
        "{MESSAGES} records round trip over shared-memory rings; check slept {} time(s) waiting for echoes, {} for space",
        back.sleeps, out.sleeps
    );
    expect("revoke", slot(parent.untyped).revoke(), Ok(()));
    slots::free_range(slots.end - 400, 400);
    println!("shared-memory rings between processes behave");
}

/// The blob length of request number `i` on the ring: it grows and shrinks
/// with `i`, so records of many sizes cross the ring.
fn small_blob(i: u64) -> usize {
    (i as usize * 37) % 900
}

/// The request `init` sends as number `i`, with a blob of `blob_len` bytes;
/// its text and numbers vary with `i` too.
fn echo_request<'a, 'b>(i: u64, blob_len: usize, scratch: &'a mut SingleSegmentAllocator<'b>) -> Builder<&'a mut SingleSegmentAllocator<'b>> {
    let mut message = Builder::new(scratch);
    let mut request = message.init_root::<request::Builder>();
    request.set_id(i);
    let text = &"sapabilities: capabilities everywhere, Cap'n Proto across the rings. "[..(i as usize * 7) % 70];
    request.set_text(text);
    let mut numbers = request.reborrow().init_numbers((i % 50) as u32);
    for k in 0..i % 50 {
        numbers.set(k as u32, (k * i) as u32);
    }
    request.init_blob(blob_len as u32).iter_mut().enumerate().for_each(|(k, b)| *b = (k as u64 + i) as u8);
    message
}

/// Check `reply` against request number `i`, whose blob was `blob_len`
/// bytes.
fn check_reply(i: u64, blob_len: usize, body: &[u8]) {
    let segments = [body];
    let reader = Reader::new(SegmentArray::new(&segments), ReaderOptions::new());
    let reply = reader.get_root::<reply::Reader>().expect("reply");
    let text = &"sapabilities: capabilities everywhere, Cap'n Proto across the rings. "[..(i as usize * 7) % 70];
    assert_eq!(reply.get_id(), i);
    assert!(reply.get_text().unwrap().to_str().unwrap().chars().eq(text.chars().rev()), "reversed text");
    assert_eq!(reply.get_sum(), (0..i % 50).map(|k| (k * i) as u32 as u64).sum::<u64>(), "sum");
    let blob_len = blob_len as u64;
    assert_eq!(reply.get_blob_length() as u64, blob_len, "blob length");
    assert_eq!(reply.get_blob_sum(), (0..blob_len).map(|k| (k + i) as u8 as u64).sum::<u64>(), "blob sum");
}

/// The echo interface between processes: Cap'n Proto requests of many sizes
/// out, replies back, read from a private copy or (trusting the peer) in
/// place; a malformed request, one too long for the peer, and a peer that
/// breaks the ring's rules.
pub fn check_echo_service(info: &Env) {
    let (parent, mut slots) = channel_resources(info);
    let Channel { mut out, mut back, ep } = connect_peer(&parent, &mut slots, Mode::ServeEcho, 2, 0x28_0000_0000);
    let mut scratch_words = [0u64; 512];
    let mut scratch = SingleSegmentAllocator::new(as_bytes_mut(&mut scratch_words));
    let mut reply_words = [0u64; 512];
    const REQUESTS: u64 = 300;
    const WINDOW: u64 = 6;
    let (mut sent, mut received, mut bytes) = (0, 0, 0);
    while received < REQUESTS {
        while sent < REQUESTS && sent - received < WINDOW {
            let message = echo_request(sent, small_blob(sent), &mut scratch);
            let segment = message.get_segments_for_output()[0];
            bytes += segment.len();
            out.send(echo::REQUEST, sent, segment);
            sent += 1;
        }
        if received % 2 == 0 {
            let record = back.recv(&mut reply_words).expect("ring");
            assert_eq!((record.kind, record.id), (echo::REPLY, received), "reply record");
            check_reply(received, small_blob(received), &as_bytes(&reply_words)[..record.len]);
        } else {
            // SAFETY: the peer is this test's own program, which never
            // writes a record after publishing it.
            let record = unsafe { back.recv_in_place(|record, body| { check_reply(received, small_blob(received), body); record }) }.expect("ring");
            assert_eq!((record.kind, record.id), (echo::REPLY, received), "reply record");
        }
        received += 1;
    }
    println!("{REQUESTS} Cap'n Proto requests ({} KiB) answered across processes, read copied and in place", bytes / 1024);

    // Requests too large for the ring (its largest body is under 4 KiB) go
    // by page transfer: built in a private buffer, written into init's
    // frames, moved to the peer, and read there in place.
    let (mut sender, first) = transfer_sender(&parent, &mut slots);
    let mut big_words = alloc::vec![0u64; MAX_LEN / 8];
    let mut big = SingleSegmentAllocator::new(as_bytes_mut(&mut big_words));

    // Senders that break the rules, by hand: each sends one capability for
    // a 100-byte message and gets it back with an error record.
    let spare = slots.next;
    slots.next += 1;
    let mut lie = |id: u64, cap: ipc::Give, window: usize, why: &str| {
        out.send(echo::TRANSFER, id, &[]);
        ipc::set_receive_window(process_slot::CNODE, window, 1);
        let returned = ipc::call(ep, echo::TRANSFER as usize, &[100, id as usize], &[cap], &mut []).expect("call");
        assert_eq!(returned.info.extra_caps(), 1, "the capability comes back: {why}");
        assert_eq!(back.recv(&mut reply_words).map(|r| (r.kind, r.id)), Ok((echo::ERROR, id)), "{why}");
    };
    // Keeping a copy of a frame would let the sender change the message
    // while the peer reads it.
    expect("keep a copy", slot(spare).copy_from(slot(first), rights::ALL), Ok(()));
    lie(3000, ipc::Give::moved(first), first, "a frame the sender shares");
    expect("drop the copy", slot(spare).delete(), Ok(()));

    // A frame still mapped by the sender (moving a capability keeps its
    // mapping), which the sender could write through.
    expect("map a frame here", vspace::map_frame(first, process_slot::VSPACE, TRANSFER_HERE, rights::READ | rights::WRITE, attr::EXECUTE_NEVER), Ok(()));
    lie(3001, ipc::Give::moved(first), first, "a frame the sender maps");
    expect("unmap it", vspace::unmap_frame(first), Ok(()));
    // Not a frame: an endpoint, which the peer would call if it invoked it
    // to map it.
    lie(3002, ipc::Give::copy(ep), spare, "a capability that isn't a frame");
    expect("drop it", slot(spare).delete(), Ok(()));

    // Then requests of up to 60 KiB go through.
    let mut transferred = 0;
    for (k, blob_len) in [5_000, 20_000, 60_000].into_iter().enumerate() {
        let id = 2000 + k as u64;
        let message = echo_request(id, blob_len, &mut big);
        let segment = message.get_segments_for_output()[0];
        transferred += segment.len();
        out.send(echo::TRANSFER, id, &[]);
        sender.call(ep, echo::TRANSFER as usize, id, segment.len(), |frames| frames.copy_from_slice(segment), &mut []).expect("page transfer");
        let record = back.recv(&mut reply_words).expect("ring");
        assert_eq!((record.kind, record.id), (echo::REPLY, id), "a transfer's reply");
        check_reply(id, blob_len, &as_bytes(&reply_words)[..record.len]);
    }
    assert_eq!(sender.call(ep, 0, 0, MAX_LEN + 1, |_| (), &mut []).map(drop), Err(TransferError::TooLong), "longer than one call carries");
    println!("3 requests ({} KiB) by page transfer, read in place; shared, mapped and non-frame transfers refused", transferred / 1024);

    // Not Cap'n Proto: the peer answers with an error and carries on.
    out.send(echo::REQUEST, 1000, &[0xff; 40]);
    assert_eq!(back.recv(&mut reply_words).map(|r| (r.kind, r.id)), Ok((echo::ERROR, 1000)), "a malformed request");
    // Longer than the peer's 2 KiB buffer: skipped, with an error.
    out.send(echo::REQUEST, 1001, &[0; 3000]);
    assert_eq!(back.recv(&mut reply_words).map(|r| (r.kind, r.id)), Ok((echo::ERROR, 1001)), "an oversized request");
    out.send(echo::STOP, 0, &[]);
    assert_eq!(back.recv(&mut reply_words).map(|r| r.kind), Ok(echo::STOP), "the stop record");
    sys::yield_now();

    // A peer that answers with garbage, then publishes a record longer than
    // what it published: the garbage fails to parse, the record is refused.
    let Channel { mut out, mut back, .. } = connect_peer(&parent, &mut slots, Mode::Misbehave, 1, 0x28_0001_0000);
    let message = echo_request(1, small_blob(1), &mut scratch);
    out.send(echo::REQUEST, 1, message.get_segments_for_output()[0]);
    drop(message);
    let record = back.recv(&mut reply_words).expect("ring");
    let segments = [&as_bytes(&reply_words)[..record.len]];
    let reader = Reader::new(SegmentArray::new(&segments), ReaderOptions::new());
    assert!(reader.get_root::<reply::Reader>().and_then(|r| r.get_text().map(drop)).is_err(), "a garbage reply should not parse");
    assert_eq!(back.recv(&mut reply_words), Err(RingError::Corrupt), "a record past what was published");
    expect("revoke", slot(parent.untyped).revoke(), Ok(()));
    slots::free_range(slots.end - 400, 400);
    println!("Cap'n Proto over rings refuses malformed and oversized requests and a lying peer");
}

/// The peer's checksum (`sum_bytes`): little-endian words, the tail as
/// bytes.
fn checksum(bytes: &[u8]) -> u64 {
    let (words, tail) = bytes.as_chunks::<8>();
    let tail = tail.iter().map(|&b| u64::from(b)).fold(0, u64::wrapping_add);
    words.iter().map(|&w| u64::from_le_bytes(w)).fold(tail, u64::wrapping_add)
}

/// Time round trips that move `size` bytes to a peer and get their
/// checksum back, through a ring (copied in by init, copied out by the
/// peer) and by page transfer (written into frames by init, read in place
/// by the peer), and print both: where copying stops paying (M5). Rings of
/// 32 data pages take bodies up to 64 KiB, like one transfer.
pub fn measure_transfer(info: &Env) {
    const ROUNDS: u64 = 64;
    let (parent, mut slots) = channel_resources(info);
    let Channel { mut out, mut back, ep } = connect_peer(&parent, &mut slots, Mode::SumBytes, 32, 0x28_0000_0000);
    let (mut sender, first) = transfer_sender(&parent, &mut slots);

    // What one page costs: the system calls each side makes per page.
    let micro = |f: &mut dyn FnMut()| {
        let start = sys::time();
        for _ in 0..ROUNDS {
            f();
        }
        (sys::time() - start) * 10_000_000 / info.timer_hz / ROUNDS
    };
    let rw = rights::READ | rights::WRITE;
    let identify = micro(&mut || {
        let _ = slot(first).identify();
    });
    let map_unmap = micro(&mut || {
        let _ = vspace::map_frame(first, process_slot::VSPACE, TRANSFER_HERE, rw, attr::EXECUTE_NEVER);
        let _ = vspace::unmap_frame(first);
    });
    let null = micro(&mut || sys::yield_now());
    let touch = micro(&mut || {
        let _ = vspace::map_frame(first, process_slot::VSPACE, TRANSFER_HERE, rw, attr::EXECUTE_NEVER);
        // SAFETY: just mapped read-write; nothing else uses it.
        unsafe { mapped::<u64>(TRANSFER_HERE).write_volatile(1) };
        let _ = vspace::unmap_frame(first);
    });
    let us = |t: u64| format!("{}.{} us", t / 10, t % 10);
    println!(
        "per call: yield {}, identify {}, map+unmap {}, map+write+unmap {}",
        us(null), us(identify), us(map_unmap), us(touch)
    );
    let data: Vec<u8> = (0..MAX_LEN).map(|k| (k * 7 + k / 251) as u8).collect();
    let mut sum = [0u64; 1];
    let mut round_trip = |n: usize, id: u64, transfer: bool| {
        if transfer {
            out.send(echo::TRANSFER, id, &[]);
            sender.call(ep, echo::TRANSFER as usize, id, n, |frames| frames.copy_from_slice(&data[..n]), &mut []).expect("transfer");
        } else {
            out.send(echo::BYTES, id, &data[..n]);
        }
        let record = back.recv(&mut sum).expect("ring");
        assert_eq!((record.kind, record.id, sum[0]), (echo::SUM, id, checksum(&data[..n])), "the peer's checksum");
    };
    // Tenths of a microsecond per round trip.
    let tenths = |ticks: u64| ticks * 10_000_000 / info.timer_hz / ROUNDS;
    println!("round trips of n bytes, ring copy vs page transfer (QEMU, {} Hz timer):", info.timer_hz);
    for n in [256, 1024, 4096, 8192, 16384, 32768, 61440] {
        let mut ticks = [0u64; 2];
        for (way, transfer) in [false, true].into_iter().enumerate() {
            round_trip(n, 0, transfer); // warm up
            let start = sys::time();
            for id in 0..ROUNDS {
                round_trip(n, id, transfer);
            }
            ticks[way] = sys::time() - start;
        }
        println!("  {n:>6} B: copy {}, transfer {}", us(tenths(ticks[0])), us(tenths(ticks[1])));
    }
    out.send(echo::STOP, 0, &[]);
    assert_eq!(back.recv(&mut sum).map(|r| r.kind), Ok(echo::STOP), "the stop record");
    sys::yield_now();
    expect("revoke", slot(parent.untyped).revoke(), Ok(()));
    slots::free_range(slots.end - 400, 400);
}

/// A word-aligned buffer, as Cap'n Proto segments must be.
#[repr(C, align(8))]
struct Words<const N: usize>([u8; N]);

/// Cap'n Proto within one process: a request built in a fixed buffer with no
/// heap, read back through the checked reader, and a corrupted pointer, bad
/// UTF-8 and an over-long traversal rejected rather than trusted.
pub fn check_capnp() {
    let mut space = Words([0; 1024]);
    let mut message = Builder::new(SingleSegmentAllocator::new(&mut space.0));
    {
        let mut req = message.init_root::<request::Builder>();
        req.set_id(7);
        req.set_text("capabilities");
        let mut numbers = req.reborrow().init_numbers(4);
        for (i, n) in [3u32, 1, 4, 1].into_iter().enumerate() {
            numbers.set(i as u32, n);
        }
        req.set_blob(&[0xff; 5]);
    }
    let segment = message.get_segments_for_output()[0];
    let mut copy = Words([0; 1024]);
    copy.0[..segment.len()].copy_from_slice(segment);
    let words = &copy.0[..segment.len()];

    let segments = [words];
    let reader = Reader::new(SegmentArray::new(&segments), ReaderOptions::new());
    let req = reader.get_root::<request::Reader>().expect("root");
    assert_eq!(req.get_id(), 7);
    assert_eq!(req.get_text().unwrap().to_str(), Ok("capabilities"));
    assert_eq!(req.get_numbers().unwrap().iter().map(u64::from).sum::<u64>(), 9);
    assert_eq!(req.get_blob().unwrap(), &[0xff; 5]);

    // Each corruption is in a private copy, as a receiver's would be.
    let mut bad = Words([0; 1024]);
    let n = segment.len();
    // The root pointer (word 0) pointing far outside the segment.
    bad.0[..n].copy_from_slice(segment);
    bad.0[..4].copy_from_slice(&0x7fff_fff0u32.to_le_bytes());
    let segments = [&bad.0[..n]];
    let reader = Reader::new(SegmentArray::new(&segments), ReaderOptions::new());
    assert!(reader.get_root::<request::Reader>().is_err(), "an out-of-bounds root should be refused");
    // Text that isn't UTF-8: found and overwritten in place.
    bad.0[..n].copy_from_slice(segment);
    let at = bad.0[..n].windows(12).position(|w| w == b"capabilities").expect("the text");
    bad.0[at] = 0xc3;
    bad.0[at + 1] = 0x28;
    let segments = [&bad.0[..n]];
    let reader = Reader::new(SegmentArray::new(&segments), ReaderOptions::new());
    let req = reader.get_root::<request::Reader>().expect("root");
    assert!(req.get_text().unwrap().to_str().is_err(), "invalid UTF-8 should be refused");
    // A traversal limit smaller than the message.
    let segments = [words];
    let mut options = ReaderOptions::new();
    options.traversal_limit_in_words(Some(4));
    let reader = Reader::new(SegmentArray::new(&segments), options);
    let over = reader.get_root::<request::Reader>().and_then(|r| r.get_numbers().and_then(|_| r.get_blob()).map(drop));
    assert!(over.is_err(), "a traversal past the limit should be refused");
    println!("Cap'n Proto messages build without a heap, read back, and refuse bad pointers, UTF-8 and traversal");
}
