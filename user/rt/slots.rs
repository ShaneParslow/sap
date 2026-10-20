//! The program's free capability slots: a range of its root CNode, handed
//! out one at a time or in contiguous runs (retype fills consecutive slots).
//!
//! Slot numbers are capability addresses: the root CNode is a single level
//! with a guard, so a slot's number names it, as for the root task and every
//! process the loader builds.
//!
//! TODO: grow past one CNode by adding second-level CNodes, once a program
//! needs more than `CAPACITY` slots (M7, the process manager).

use core::ops::Range;

use crate::sync::Mutex;

/// The most slots the allocator manages: the root task's whole CNode.
pub const CAPACITY: usize = 4096;
const WORDS: usize = CAPACITY / 64;

struct Bitmap {
    /// The first slot managed; bit `i` is slot `start + i`.
    start: usize,
    len: usize,
    used: [u64; WORDS],
}

static SLOTS: Mutex<Bitmap> = Mutex::new(Bitmap { start: 0, len: 0, used: [0; WORDS] });

/// Manage `range`: slots the program doesn't otherwise use, all empty.
pub(crate) fn init(range: Range<usize>, notification: usize) {
    let mut slots = SLOTS.lock();
    assert!(slots.len == 0, "slots: already initialised");
    assert!(range.len() <= CAPACITY, "slots: more than {CAPACITY}");
    slots.start = range.start;
    slots.len = range.len();
    drop(slots);
    SLOTS.set_notification(notification);
}

/// One free slot, or None if every slot is in use.
pub fn alloc() -> Option<usize> {
    alloc_range(1)
}

/// `count` consecutive free slots, returning the first.
pub fn alloc_range(count: usize) -> Option<usize> {
    let mut slots = SLOTS.lock();
    let mut run = 0;
    for i in 0..slots.len {
        if slots.is_used(i) {
            run = 0;
            continue;
        }
        run += 1;
        if run == count {
            let first = i + 1 - count;
            for j in first..=i {
                slots.used[j / 64] |= 1 << (j % 64);
            }
            return Some(slots.start + first);
        }
    }
    None
}

/// Return `count` slots from `first` on. They should be empty (the caller
/// deleted or moved their capabilities), or the next user finds them full.
pub fn free_range(first: usize, count: usize) {
    let mut slots = SLOTS.lock();
    for slot in first..first + count {
        let i = slot.checked_sub(slots.start).filter(|&i| i < slots.len).expect("slots: not managed here");
        assert!(slots.is_used(i), "slots: freeing a free slot");
        slots.used[i / 64] &= !(1 << (i % 64));
    }
}

pub fn free(slot: usize) {
    free_range(slot, 1);
}

impl Bitmap {
    fn is_used(&self, i: usize) -> bool {
        self.used[i / 64] & (1 << (i % 64)) != 0
    }
}
