//! Single-producer, single-consumer rings of variable-length records in
//! shared memory, with notifications as doorbells: sap's channel design
//! (ipc-channels in the wiki).
//!
//! One direction of a channel is a run of pages (`Layout`):
//! - a **producer page**, written by the producer: its index and its
//!   "waiting for space" flag;
//! - **data pages**, written by the producer: the records;
//! - a **consumer page**, written by the consumer: its index and its
//!   "waiting for records" flag.
//!
//! Each side maps the other's pages read-only, so neither can move the
//! other's index. A record is a two-word header (body length in bytes, a
//! kind, an id the application uses to match replies to requests) and the
//! body, padded to an even number of words. Records never wrap: one that
//! doesn't fit before the end of the ring is preceded by a padding record
//! that fills it. A record is published by a release store of the producer's
//! index and released by a release store of the consumer's.
//!
//! **Reading.** The producer can keep writing a record after publishing it,
//! and reading memory that another process writes through ordinary
//! references is undefined behavior in Rust. So `Consumer::recv` reads the
//! header once, checks it against the ring, and copies the body out with
//! volatile reads; the caller works on its own copy (the single-fetch rule
//! of ipc-validation in the wiki). `Consumer::recv_in_place` lends the body
//! in place instead, for a producer trusted not to write what it published.
//!
//! **Waiting.** Neither side makes a system call while the other keeps up: a
//! side signals the other's notification only when the other has said it is
//! waiting, and a consumer polls briefly before it sleeps. The waiting flags
//! use sequentially consistent fences on both sides, so a signal can't be
//! lost between "check" and "sleep".
//!
//! Messages larger than half the ring travel as transferred pages instead
//! (`transfer`).

#![no_std]
#![feature(strict_provenance_lints)]
// Pointers come from `sap_sys::address::mapped`, never from integer casts.
#![deny(implicit_provenance_casts, integer_to_ptr_transmutes)]

use core::sync::atomic::{fence, AtomicUsize, Ordering};

use sap_sys::address::mapped;
use sap_sys::notification;

pub mod transfer;

pub const PAGE: usize = 4096;
const WORD: usize = size_of::<u64>();
/// Words in a record header.
const HEADER: usize = 2;
/// The kind of a padding record, which fills the end of the ring.
const PAD: u32 = 0;
/// How many times a consumer checks for a record before sleeping.
const SPINS: usize = 64;

#[repr(C)]
struct ProducerPage {
    produced: AtomicUsize,
    producer_waiting: AtomicUsize,
}

#[repr(C)]
struct ConsumerPage {
    consumed: AtomicUsize,
    consumer_waiting: AtomicUsize,
}

/// Where one direction's pages are: the producer page at `base`, then
/// `data_pages` data pages (a power of two), then the consumer page.
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub base: usize,
    pub data_pages: usize,
}

impl Layout {
    pub const fn new(base: usize, data_pages: usize) -> Self {
        assert!(data_pages.is_power_of_two(), "ring: data pages must be a power of two");
        Self { base, data_pages }
    }

    /// Pages in this direction, all contiguous from `base`.
    pub const fn pages(&self) -> usize {
        self.data_pages + 2
    }

    /// Whether the producer writes page `i` (and the consumer only reads
    /// it); otherwise the consumer writes it and the producer only reads.
    pub const fn producer_writes(&self, i: usize) -> bool {
        i != self.pages() - 1
    }

    /// The longest record body, in bytes: half the ring, less a header, so
    /// a record always fits after at most one padding record.
    pub const fn max_body(&self) -> usize {
        (self.capacity() / 2 - HEADER) * WORD
    }

    /// The ring's size in words.
    const fn capacity(&self) -> usize {
        self.data_pages * PAGE / WORD
    }

    fn producer_page(&self) -> usize {
        self.base
    }

    fn data(&self) -> usize {
        self.base + PAGE
    }

    fn consumer_page(&self) -> usize {
        self.base + (self.data_pages + 1) * PAGE
    }
}

/// A record's header: what the producer said about the body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Record {
    /// Application-defined, never 0.
    pub kind: u32,
    /// Application-defined, typically to match a reply to its request.
    pub id: u64,
    /// The body's length in bytes.
    pub len: usize,
}

/// Why a receive failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RingError {
    /// The producer broke the ring's rules (an index or a record outside
    /// the ring). Nothing more can be trusted from this ring.
    Corrupt,
    /// The record's body was longer than the caller's buffer; it was
    /// skipped.
    TooLong(Record),
}

/// A word buffer's bytes: record bodies are copied into words, which keeps
/// them aligned for Cap'n Proto.
pub fn as_bytes(words: &[u64]) -> &[u8] {
    // SAFETY: the same memory, as bytes, which have no invalid values.
    unsafe { core::slice::from_raw_parts(words.as_ptr().cast::<u8>(), words.len() * WORD) }
}

/// `as_bytes`, mutably.
pub fn as_bytes_mut(words: &mut [u64]) -> &mut [u8] {
    // SAFETY: as for `as_bytes`; any bytes are valid words.
    unsafe { core::slice::from_raw_parts_mut(words.as_mut_ptr().cast::<u8>(), words.len() * WORD) }
}

/// Words a record with a `len`-byte body occupies, header included: an
/// even number, so the space before the end of the ring always fits a
/// header.
const fn record_words(len: usize) -> usize {
    (HEADER + len.div_ceil(WORD)).next_multiple_of(2)
}

/// The writing end of a ring.
pub struct Producer {
    producer: *mut ProducerPage,
    data: *mut u64,
    consumer: *const ConsumerPage,
    layout: Layout,
    /// Signals the consumer's notification.
    doorbell: usize,
    /// This side's notification, which the consumer signals when space frees.
    wait: usize,
    /// How often this end slept for lack of space.
    pub sleeps: usize,
}

/// The reading end of a ring.
pub struct Consumer {
    producer: *const ProducerPage,
    data: *const u64,
    consumer: *mut ConsumerPage,
    layout: Layout,
    /// Signals the producer's notification.
    doorbell: usize,
    /// This side's notification, which the producer signals.
    wait: usize,
    /// How often this end slept for lack of records.
    pub sleeps: usize,
}

impl Producer {
    /// # Safety
    /// The pages of `layout` must be mapped, the consumer page at least
    /// read-only and the rest read-write, and used by no other producer;
    /// `doorbell` signals the consumer and `wait` is this side's
    /// notification.
    pub unsafe fn new(layout: Layout, doorbell: usize, wait: usize) -> Self {
        Self {
            producer: mapped(layout.producer_page()),
            data: mapped(layout.data()),
            consumer: mapped(layout.consumer_page()),
            layout,
            doorbell,
            wait,
            sleeps: 0,
        }
    }

    /// The longest body `send` takes, in bytes.
    pub fn max_body(&self) -> usize {
        self.layout.max_body()
    }

    /// Publish a record, waiting while the ring lacks room for it.
    pub fn send(&mut self, kind: u32, id: u64, body: &[u8]) {
        assert!(kind != PAD, "ring: kind 0 is reserved");
        assert!(body.len() <= self.max_body(), "ring: record too long");
        let capacity = self.layout.capacity();
        let words = record_words(body.len());
        // SAFETY: the pages are mapped as `new` requires; this side writes
        // only its own page and records the consumer has released.
        let (mine, theirs) = unsafe { (&*self.producer, &*self.consumer) };
        loop {
            let produced = mine.produced.load(Ordering::Relaxed);
            let consumed = theirs.consumed.load(Ordering::Acquire);
            let at = produced % capacity;
            let to_end = capacity - at;
            let needed = if words <= to_end { words } else { to_end + words };
            // The consumer writes `consumed`, so it can't be trusted: an
            // index ahead of `produced` wraps to a huge count, which reads
            // as full, the same as a consumer that stops reading.
            let used = produced.wrapping_sub(consumed);
            if used <= capacity && capacity - used >= needed {
                let mut position = produced;
                if words > to_end {
                    // SAFETY: `at` is inside the ring and released.
                    unsafe { self.write_header(at, PAD, 0, (to_end - HEADER) * WORD) };
                    position += to_end;
                }
                let at = position % capacity;
                // SAFETY: `at..at + words` is inside the ring (records never
                // wrap) and released by the consumer.
                unsafe {
                    self.write_header(at, kind, id, body.len());
                    let to = self.data.add(at + HEADER).cast::<u8>();
                    core::ptr::copy_nonoverlapping(body.as_ptr(), to, body.len());
                }
                mine.produced.store(position + words, Ordering::Release);
                fence(Ordering::SeqCst);
                if theirs.consumer_waiting.load(Ordering::Relaxed) != 0 {
                    let _ = notification::signal(self.doorbell);
                }
                return;
            }
            // Full: say so, then look again before sleeping, so a release
            // that raced with the flag isn't missed.
            mine.producer_waiting.store(1, Ordering::Relaxed);
            fence(Ordering::SeqCst);
            if theirs.consumed.load(Ordering::Acquire) == consumed {
                self.sleeps += 1;
                let _ = notification::wait(self.wait);
            }
            mine.producer_waiting.store(0, Ordering::Relaxed);
        }
    }

    /// # Safety
    /// `at` and `at + 1` must be words of the ring the consumer released.
    unsafe fn write_header(&mut self, at: usize, kind: u32, id: u64, len: usize) {
        // SAFETY: per the caller's guarantee.
        unsafe {
            self.data.add(at).write((kind as u64) << 32 | len as u64);
            self.data.add(at + 1).write(id);
        }
    }
}

impl Consumer {
    /// # Safety
    /// The pages of `layout` must be mapped, the consumer page read-write
    /// and the rest at least read-only, and used by no other consumer;
    /// `doorbell` signals the producer and `wait` is this side's
    /// notification.
    pub unsafe fn new(layout: Layout, doorbell: usize, wait: usize) -> Self {
        Self {
            producer: mapped(layout.producer_page()),
            data: mapped(layout.data()),
            consumer: mapped(layout.consumer_page()),
            layout,
            doorbell,
            wait,
            sleeps: 0,
        }
    }

    /// Take the next record, copying its body into `out` (whose first
    /// `record.len` bytes are then the body), and waiting while the ring is
    /// empty.
    pub fn recv(&mut self, out: &mut [u64]) -> Result<Record, RingError> {
        let (record, at, end) = self.next()?;
        if record.len > out.len() * WORD {
            self.release(end);
            return Err(RingError::TooLong(record));
        }
        for (i, word) in out.iter_mut().take(record.len.div_ceil(WORD)).enumerate() {
            // SAFETY: inside the record, which `next` checked lies inside
            // the ring; volatile, since the producer may still write it.
            *word = unsafe { self.data.add(at + HEADER + i).read_volatile() };
        }
        self.release(end);
        Ok(record)
    }

    /// Take the next record and lend `f` its body in place, without a copy,
    /// waiting while the ring is empty. The record is released when `f`
    /// returns.
    ///
    /// # Safety
    /// The producer must not write a record after publishing it: reading
    /// memory another process writes is undefined behavior in Rust. Only
    /// for a producer in the same trust domain (ipc-transports in the wiki).
    pub unsafe fn recv_in_place<R>(&mut self, f: impl FnOnce(Record, &[u8]) -> R) -> Result<R, RingError> {
        let (record, at, end) = self.next()?;
        // SAFETY: inside the ring (checked by `next`), not written while lent
        // (the caller's guarantee), and released only after `f` returns.
        let body = unsafe { core::slice::from_raw_parts(self.data.add(at + HEADER).cast::<u8>(), record.len) };
        let result = f(record, body);
        self.release(end);
        Ok(result)
    }

    /// Wait for the next record that isn't padding and check its header
    /// against the ring. Returns the record, its offset in words and the
    /// consumer index just past it.
    fn next(&mut self) -> Result<(Record, usize, usize), RingError> {
        let capacity = self.layout.capacity();
        // SAFETY: the pages are mapped as `new` requires; the producer's
        // index is atomic.
        let (theirs, mine) = unsafe { (&*self.producer, &*self.consumer) };
        let mut spins = 0;
        loop {
            let consumed = mine.consumed.load(Ordering::Relaxed);
            let produced = theirs.produced.load(Ordering::Acquire);
            let available = produced.wrapping_sub(consumed);
            if available > capacity {
                return Err(RingError::Corrupt);
            }
            if available != 0 {
                let at = consumed % capacity;
                // Each header word is read once, then checked.
                // SAFETY: `at` is inside the ring and the producer published
                // at least one word there; volatile, as in `recv`.
                let (h0, id) = unsafe { (self.data.add(at).read_volatile(), self.data.add(at + 1).read_volatile()) };
                let (kind, len) = ((h0 >> 32) as u32, h0 as u32 as usize);
                if len > self.layout.max_body() {
                    return Err(RingError::Corrupt);
                }
                let words = record_words(len);
                if words > available || at + words > capacity {
                    return Err(RingError::Corrupt);
                }
                if kind == PAD {
                    self.release(consumed + words);
                    continue;
                }
                return Ok((Record { kind, id, len }, at, consumed + words));
            }
            if spins < SPINS {
                spins += 1;
                core::hint::spin_loop();
                continue;
            }
            mine.consumer_waiting.store(1, Ordering::Relaxed);
            fence(Ordering::SeqCst);
            if theirs.produced.load(Ordering::Acquire) == produced {
                self.sleeps += 1;
                let _ = notification::wait(self.wait);
            }
            mine.consumer_waiting.store(0, Ordering::Relaxed);
            spins = 0;
        }
    }

    /// Give records up to `end` back to the producer.
    fn release(&mut self, end: usize) {
        // SAFETY: the pages are mapped as `new` requires.
        let (theirs, mine) = unsafe { (&*self.producer, &*self.consumer) };
        mine.consumed.store(end, Ordering::Release);
        fence(Ordering::SeqCst);
        if theirs.producer_waiting.load(Ordering::Relaxed) != 0 {
            let _ = notification::signal(self.doorbell);
        }
    }
}
