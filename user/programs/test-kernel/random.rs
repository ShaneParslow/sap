//! Random operations: sequences nobody would write by hand, from a seeded
//! generator, on a pool of slots and an untyped region: retyping, copying,
//! minting, moving, deleting and revoking capabilities, capabilities inside
//! CNodes the pool holds (a CNode may hold its own capability), signals and
//! non-blocking IPC, mapping frames and page tables, ASIDs, and threads
//! configured, bound, given fault handlers, resumed and suspended (at
//! priorities below this thread's, so they never run).
//!
//! Each operation's result is checked where it is cheap to know: a slot a
//! copy or retype filled holds the right type, a move empties its source, a
//! delete empties its slot, a revoke keeps its own capability. Run under
//! the audit (`cargo run --profile audit --features audit`), the kernel must
//! also stay consistent after each. Every capability in the pool derives
//! from the region, so after revoking it every pool slot must be empty, and
//! the whole region must be reusable: the leak check.
//!
//! The seeds are fixed, so a failure reproduces. `SAP_SEED=<number>` at
//! build time runs that seed alone, for longer.

use core::sync::atomic::{AtomicUsize, Ordering};

use sap_rt::abi::{BootInfo, init_slot, object, rights};
use sap_rt::cnode::SlotRef;
use sap_rt::{endpoint, notification, println, tcb, vspace};

use crate::support::{RW, TCB, ok, slot, try_retype, untyped_at_least};

/// Slots the operations use.
const POOL: usize = 48;

/// The region's size, in bits, and in pages.
const REGION_BITS: usize = 20;
const REGION_PAGES: usize = 1 << (REGION_BITS - 12);

const SEEDS: [u64; 4] = [1, 2, 0x5a9_cafe, 0xdead_beef];
const STEPS: usize = 2500;

/// What retyping makes: object types, with size bits.
const KINDS: [(usize, usize); 11] = [
    (object::ENDPOINT, 0),
    (object::NOTIFICATION, 0),
    (object::REPLY, 0),
    (object::TCB, 0),
    (object::FRAME_4K, 0),
    (object::PAGE_TABLE, 0),
    (object::PAGE_TABLE, 0),
    (object::CNODE, INNER_RADIX),
    (object::UNTYPED, 12),
    (object::UNTYPED, 14),
    (object::UNTYPED, 16),
];

/// The radix of every CNode made here, so slots inside one can be named
/// without asking its size.
const INNER_RADIX: usize = 2;

const RIGHTS: [usize; 5] = [
    rights::ALL,
    rights::READ,
    rights::WRITE,
    rights::READ | rights::WRITE,
    rights::WRITE | rights::GRANT,
];

const BADGES: [usize; 4] = [1, 2, 3, 0x40];

/// Where frames and page tables are mapped, in address spaces made here.
const VA: usize = 0x7_0000_0000;
const VAS: [usize; 4] = [VA, VA + 0x1000, VA + 0x20_0000, VA + 0x4000_0000];

/// The current seed and step, for failure messages.
static SEED: AtomicUsize = AtomicUsize::new(0);
static STEP: AtomicUsize = AtomicUsize::new(0);

/// Check `condition`, naming the seed and step if it fails.
macro_rules! check {
    ($condition:expr, $($message:tt)+) => {
        assert!(
            $condition,
            "random: seed {:#x}, step {}: {}",
            SEED.load(Ordering::Relaxed),
            STEP.load(Ordering::Relaxed),
            format_args!($($message)+)
        )
    };
}

/// xorshift64*: small, and good enough to pick operations.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn pick<T: Copy, const N: usize>(&mut self, items: [T; N]) -> T {
        items[self.below(N)]
    }
}

/// The slots and memory the operations use.
struct Pool {
    parent: usize,
    region: usize,
    first: usize,
    /// Where reusing the region puts its frames.
    reuse: usize,
}

impl Pool {
    fn slot(&self, rng: &mut Rng) -> usize {
        self.first + rng.below(POOL)
    }

    /// Revoke the region, check every pool slot is empty, and fill the
    /// region with frames, which the ledger refuses if anything made from
    /// it still lives; then revoke the frames.
    fn reuse(&self) {
        ok("revoke the region", slot(self.region).revoke());
        for index in self.first..self.first + POOL {
            let left = kind(index);
            check!(
                left.is_none(),
                "slot {index} holds type {left:?} after revoking the region"
            );
        }

        let frames = try_retype(self.region, object::FRAME_4K, 0, self.reuse, REGION_PAGES);
        ok("reuse the region", frames);
        ok("revoke the reused region", slot(self.region).revoke());
    }
}

/// The type of the capability in slot `index`, if there is one.
fn kind(index: usize) -> Option<usize> {
    slot(index)
        .identify()
        .ok()
        .map(|identity| identity.object_type)
}

/// Whether invoking the capability in slot `index` is a kernel invocation.
/// Invoking an endpoint is an IPC call instead, which would wait forever.
fn invocable(index: usize) -> bool {
    kind(index) != Some(object::ENDPOINT)
}

fn retype(rng: &mut Rng, pool: &Pool, source: usize, dest: usize) {
    let source = if rng.below(4) == 0 {
        pool.region
    } else {
        source
    };
    if !invocable(source) {
        return;
    }

    let (object_type, size_bits) = rng.pick(KINDS);
    let room = pool.first + POOL - dest;
    let count = (1 + rng.below(3)).min(room);
    if try_retype(source, object_type, size_bits, dest, count).is_ok() {
        for index in dest..dest + count {
            let made = kind(index);
            check!(
                made == Some(object_type),
                "retyped type {object_type} into {index}, found {made:?}"
            );
        }
    }
}

/// After a copy or mint from `source` into `dest`: if it succeeded, `dest`
/// holds a capability of the source's type.
fn copied(source: usize, dest: usize, succeeded: bool) {
    if succeeded {
        let (from, to) = (kind(source), kind(dest));
        check!(
            from == to,
            "copied type {from:?} from {source}, found {to:?} in {dest}"
        );
    }
}

fn moved(source: usize, dest: usize) {
    let before = kind(source);
    if slot(dest).move_from(slot(source)).is_ok() {
        let (from, to) = (kind(source), kind(dest));
        check!(
            to == before,
            "moved type {before:?} into {dest}, found {to:?}"
        );
        check!(
            from.is_none(),
            "moved out of {source}, which still holds type {from:?}"
        );
    }
}

fn deleted(index: usize) {
    if slot(index).delete().is_ok() {
        let left = kind(index);
        check!(
            left.is_none(),
            "deleted {index}, which still holds type {left:?}"
        );
    }
}

fn revoked(index: usize) {
    let before = kind(index);
    if slot(index).revoke().is_ok() {
        let after = kind(index);
        check!(
            after == before,
            "revoking {index} changed it from type {before:?} to {after:?}"
        );
    }
}

/// Copy into, or delete from, a slot inside the CNode in `cnode`.
fn inside(rng: &mut Rng, cnode: usize, source: usize) {
    if kind(cnode) != Some(object::CNODE) {
        return;
    }

    let inner = SlotRef {
        root: cnode,
        index: rng.below(1 << INNER_RADIX),
        depth: INNER_RADIX,
    };
    if rng.below(2) == 0 {
        let _ = inner.copy_from(slot(source), rights::ALL);
    } else {
        let _ = inner.delete();
    }
}

fn frames(rng: &mut Rng, frame: usize, space: usize) {
    if !invocable(frame) {
        return;
    }

    let _ = match rng.below(3) {
        0 => vspace::map_frame(frame, space, rng.pick(VAS), RW, 0),
        1 => vspace::unmap_frame(frame),
        _ => vspace::frame_address(frame).map(drop),
    };
}

fn tables(rng: &mut Rng, table: usize, space: usize) {
    if !invocable(table) {
        return;
    }

    let _ = match rng.below(3) {
        0 => vspace::map_page_table(table, space, rng.pick(VAS)),
        1 => vspace::unmap_page_table(table),
        _ => vspace::assign_asid(init_slot::ASID_POOL, table),
    };
}

/// Thread operations, at priorities below this thread's, so the threads
/// made here never run.
fn threads(rng: &mut Rng, thread: usize, b: usize, c: usize) {
    if !invocable(thread) {
        return;
    }

    let _ = match rng.below(7) {
        0 => tcb::configure(thread, b, c, None),
        1 => tcb::bind_notification(thread, b),
        2 => tcb::unbind_notification(thread),
        3 => tcb::set_fault_handler(thread, Some(b)),
        4 => tcb::set_priority(thread, TCB, rng.pick([0, 1, 254])),
        5 => tcb::resume(thread),
        _ => tcb::suspend(thread),
    };
}

/// One random operation.
fn step(rng: &mut Rng, pool: &Pool) {
    let a = pool.slot(rng);
    let b = pool.slot(rng);
    let c = pool.slot(rng);

    match rng.below(16) {
        0 | 1 => retype(rng, pool, a, b),

        2 | 3 => {
            let copy = slot(b).copy_from(slot(a), rng.pick(RIGHTS));
            copied(a, b, copy.is_ok());
        }

        4 => {
            let mint = slot(b).mint_from(slot(a), rights::ALL, rng.pick(BADGES));
            copied(a, b, mint.is_ok());
        }

        5 => moved(a, b),
        6 | 7 => deleted(a),
        8 => revoked(a),
        9 => inside(rng, a, b),

        10 => {
            let _ = notification::signal(a);
            let _ = notification::poll(b);
        }

        11 => {
            let _ = endpoint::nb_send(a, 1, &[], &[]);
            let _ = endpoint::nb_recv(a, b, &mut []);
        }

        12 => frames(rng, a, b),
        13 => tables(rng, a, b),
        14 => threads(rng, a, b, c),

        _ => {
            if rng.below(40) == 0 {
                pool.reuse();
            }
        }
    }
}

/// `SAP_SEED`, if it was set at build time.
const fn chosen_seed() -> Option<u64> {
    let Some(text) = option_env!("SAP_SEED") else {
        return None;
    };
    let digits = text.as_bytes();
    let mut value: u64 = 0;
    let mut i = 0;
    while i < digits.len() {
        assert!(
            digits[i].is_ascii_digit(),
            "SAP_SEED must be a decimal number"
        );
        value = value * 10 + (digits[i] - b'0') as u64;
        i += 1;
    }
    Some(value)
}

/// Run every seed's operations, each from a fresh region.
pub fn operations(info: &BootInfo) {
    let base = info.empty.start + 500;
    let pool = Pool {
        parent: untyped_at_least(info, (REGION_BITS + 1) as u8),
        region: base,
        first: base + 1,
        reuse: base + 1 + POOL,
    };

    let (seeds, steps): (&[u64], usize) = match chosen_seed() {
        Some(seed) => (&[seed], 10 * STEPS),
        None => (&SEEDS, STEPS),
    };

    for &seed in seeds {
        let carve = try_retype(pool.parent, object::UNTYPED, REGION_BITS, pool.region, 1);
        ok("carve the region", carve);
        SEED.store(seed as usize, Ordering::Relaxed);

        let mut rng = Rng(seed.max(1));
        for index in 0..steps {
            STEP.store(index, Ordering::Relaxed);
            step(&mut rng, &pool);
        }

        pool.reuse();
        ok("delete the region", slot(pool.region).delete());
        ok("revoke the parent", slot(pool.parent).revoke());
        println!("random: seed {seed:#x}, {steps} operations, then nothing left");
    }
}
