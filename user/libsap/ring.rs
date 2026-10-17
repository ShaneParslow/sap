//! Single-producer, single-consumer message rings in shared memory, with
//! notifications as doorbells: sap's channel design (ipc-channels in the
//! wiki), first version.
//!
//! One direction of a channel is two pages:
//! - a **data page**, mapped read-write by the producer and read-only by the
//!   consumer: the producer's index, its "waiting for space" flag, and the
//!   slots;
//! - a **control page**, mapped read-write by the consumer and read-only by
//!   the producer: the consumer's index and its "waiting for messages" flag.
//!
//! So each side writes only what it owns, and neither can corrupt the
//! other's index. A message is published by a release store of the
//! producer's index; the consumer copies a slot out before releasing it
//! (one read of each field, so a producer rewriting the slot can't change
//! what was validated). Neither side makes a system call while the other
//! keeps up: a side signals the other's notification only when the other
//! has said it is waiting, and a consumer polls briefly before it sleeps.
//! The waiting flags use sequentially consistent fences on both sides, so a
//! signal can't be lost between "check" and "sleep".
//!
//! TODO: messages are at most seven words; carry Cap'n Proto messages and
//! larger buffers (M5).

use core::sync::atomic::{fence, AtomicUsize, Ordering};

use crate::notification;

const PAGE: usize = 4096;
/// Words per slot: a length word and up to seven message words.
const SLOT_WORDS: usize = 8;
/// The longest message, in words.
pub const MAX_WORDS: usize = SLOT_WORDS - 1;
/// Slots per ring: the data page minus its 64-byte header.
pub const SLOTS: usize = (PAGE - 64) / (SLOT_WORDS * 8);
/// How many times a consumer checks for a message before sleeping.
const SPINS: usize = 64;

#[repr(C)]
struct Data {
    produced: AtomicUsize,
    producer_waiting: AtomicUsize,
    _pad: [usize; 6],
    slots: [[usize; SLOT_WORDS]; SLOTS],
}

#[repr(C)]
struct Control {
    consumed: AtomicUsize,
    consumer_waiting: AtomicUsize,
}

const _: () = assert!(core::mem::size_of::<Data>() <= PAGE);

/// The writing end of a ring.
pub struct Producer {
    data: *mut Data,
    control: *const Control,
    /// Signals the consumer's notification.
    doorbell: usize,
    /// This side's notification, which the consumer signals when space frees.
    wait: usize,
    /// How often this end slept for lack of space.
    pub sleeps: usize,
}

/// The reading end of a ring.
pub struct Consumer {
    data: *const Data,
    control: *mut Control,
    /// Signals the producer's notification.
    doorbell: usize,
    /// This side's notification, which the producer signals.
    wait: usize,
    /// How often this end slept for lack of messages.
    pub sleeps: usize,
}

impl Producer {
    /// # Safety
    /// `data` must be a ring's data page mapped read-write and `control`
    /// its control page mapped (at least) read-only, used by no other
    /// producer; `doorbell` signals the consumer and `wait` is this side's
    /// notification.
    pub unsafe fn new(data: usize, control: usize, doorbell: usize, wait: usize) -> Self {
        Self { data: data as *mut Data, control: control as *const Control, doorbell, wait, sleeps: 0 }
    }

    /// Publish `words`, waiting while the ring is full.
    pub fn send(&mut self, words: &[usize]) {
        assert!(words.len() <= MAX_WORDS, "ring message too long");
        // SAFETY: the pages are mapped as `new` requires; the slot written is
        // one the consumer has released.
        let (data, control) = unsafe { (&mut *self.data, &*self.control) };
        loop {
            let produced = data.produced.load(Ordering::Relaxed);
            let consumed = control.consumed.load(Ordering::Acquire);
            // The consumer writes `consumed`, so it can't be trusted: one
            // ahead of `produced` must not underflow (a panic in debug
            // builds). It wraps to a huge count, which reads as full, the
            // same as a consumer that stops reading.
            if produced.wrapping_sub(consumed) < SLOTS {
                let slot = &mut data.slots[produced % SLOTS];
                slot[0] = words.len();
                slot[1..=words.len()].copy_from_slice(words);
                data.produced.store(produced.wrapping_add(1), Ordering::Release);
                fence(Ordering::SeqCst);
                if control.consumer_waiting.load(Ordering::Relaxed) != 0 {
                    let _ = notification::signal(self.doorbell);
                }
                return;
            }
            // Full: say so, then look again before sleeping, so a release
            // that raced with the flag isn't missed.
            data.producer_waiting.store(1, Ordering::Relaxed);
            fence(Ordering::SeqCst);
            if control.consumed.load(Ordering::Acquire) == consumed {
                self.sleeps += 1;
                let _ = notification::wait(self.wait);
            }
            data.producer_waiting.store(0, Ordering::Relaxed);
        }
    }
}

impl Consumer {
    /// # Safety
    /// `data` must be a ring's data page mapped (at least) read-only and
    /// `control` its control page mapped read-write, used by no other
    /// consumer; `doorbell` signals the producer and `wait` is this side's
    /// notification.
    pub unsafe fn new(data: usize, control: usize, doorbell: usize, wait: usize) -> Self {
        Self { data: data as *const Data, control: control as *mut Control, doorbell, wait, sleeps: 0 }
    }

    /// Take the next message into `out`, waiting while the ring is empty.
    /// Returns its length (at most `MAX_WORDS` and `out.len()`).
    pub fn recv(&mut self, out: &mut [usize]) -> usize {
        // SAFETY: the pages are mapped as `new` requires.
        let (data, control) = unsafe { (&*self.data, &mut *self.control) };
        let mut spins = 0;
        loop {
            let consumed = control.consumed.load(Ordering::Relaxed);
            let produced = data.produced.load(Ordering::Acquire);
            if produced != consumed {
                // Copy out, reading the length once and bounding it, before
                // releasing the slot.
                let slot = &data.slots[consumed % SLOTS];
                // SAFETY: an in-bounds read of shared memory the producer may
                // be writing; volatile so it is read exactly once.
                let len = unsafe { core::ptr::read_volatile(&slot[0]) }.min(MAX_WORDS).min(out.len());
                out[..len].copy_from_slice(&slot[1..=len]);
                control.consumed.store(consumed.wrapping_add(1), Ordering::Release);
                fence(Ordering::SeqCst);
                if data.producer_waiting.load(Ordering::Relaxed) != 0 {
                    let _ = notification::signal(self.doorbell);
                }
                return len;
            }
            if spins < SPINS {
                spins += 1;
                core::hint::spin_loop();
                continue;
            }
            control.consumer_waiting.store(1, Ordering::Relaxed);
            fence(Ordering::SeqCst);
            if data.produced.load(Ordering::Acquire) == produced {
                self.sleeps += 1;
                let _ = notification::wait(self.wait);
            }
            control.consumer_waiting.store(0, Ordering::Relaxed);
            spins = 0;
        }
    }
}
