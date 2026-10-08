//! Supervisor timer interrupts through the SBI TIME extension.

use core::arch::asm;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use super::sbi;

const SIE_STIE: usize = 1 << 5;

/// Frequency of the `time` CSR, from the device tree.
static TIMEBASE_HZ: AtomicU64 = AtomicU64::new(0);
static INTERVAL: AtomicU64 = AtomicU64::new(0);
static TICKS: AtomicUsize = AtomicUsize::new(0);

pub fn now() -> u64 {
    let t: u64;
    // SAFETY: reading the time CSR has no side effects.
    unsafe { asm!("rdtime {}", out(reg) t) };
    t
}

pub fn init(timebase_hz: u64) {
    assert!(timebase_hz > 0, "timer: timebase frequency is zero");
    TIMEBASE_HZ.store(timebase_hz, Ordering::Relaxed);
}

fn timebase_hz() -> u64 {
    let hz = TIMEBASE_HZ.load(Ordering::Relaxed);
    assert!(hz > 0, "timer: used before init");
    hz
}

/// Start periodic timer interrupts at `hz`. Interrupts must also be enabled
/// globally (`enable_interrupts`) for them to be taken.
pub fn start(hz: u64) {
    let interval = timebase_hz() / hz;
    INTERVAL.store(interval, Ordering::Relaxed);
    sbi::set_timer(now() + interval);
    // SAFETY: enables only the supervisor timer interrupt source.
    unsafe { asm!("csrs sie, {}", in(reg) SIE_STIE) };
}

pub fn to_micros(time: u64) -> u64 {
    time * 1_000_000 / timebase_hz()
}

pub fn ticks() -> usize {
    TICKS.load(Ordering::Relaxed)
}

/// Called from the trap handler. Programming the next deadline also clears
/// the pending interrupt.
pub(super) fn on_interrupt() {
    TICKS.fetch_add(1, Ordering::Relaxed);
    sbi::set_timer(now() + INTERVAL.load(Ordering::Relaxed));
}
