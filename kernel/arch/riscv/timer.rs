//! Supervisor timer interrupts through the SBI TIME extension.

use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use riscv::register::{sie, time};

use super::sbi;

/// Frequency of the `time` CSR, from the device tree.
static TIMEBASE_HZ: AtomicU64 = AtomicU64::new(0);
static INTERVAL: AtomicU64 = AtomicU64::new(0);
static TICKS: AtomicUsize = AtomicUsize::new(0);

/// The `time` CSR (Zicntr). Every application-class RISC-V core has it, and
/// the RVA profiles require it.
pub fn now() -> u64 {
    time::read() as u64
}

pub fn init(timebase_hz: u64) {
    assert!(timebase_hz > 0, "timer: timebase frequency is zero");
    TIMEBASE_HZ.store(timebase_hz, Ordering::Relaxed);
    // User mode may read `time` (`rdtime`), as on Linux, for measuring.
    // TODO: a configuration with time protection would hide it, as part of
    // closing timing channels between partitions (sel4 in the wiki).
    // SAFETY: only grants U-mode read access to the time counter.
    unsafe { riscv::register::scounteren::set_tm() };
}

pub fn timebase_hz() -> u64 {
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
    // SAFETY: enables only the supervisor timer interrupt source, which the
    // trap handler handles.
    unsafe { sie::set_stimer() };
}

pub fn to_micros(time: u64) -> u64 {
    time * 1_000_000 / timebase_hz()
}

pub fn ticks() -> usize {
    TICKS.load(Ordering::Relaxed)
}

/// Called from the trap handler. Programming the next deadline also clears
/// the pending interrupt. The deadline is in `time` units, as `set_timer`
/// requires.
pub(super) fn on_interrupt() {
    TICKS.fetch_add(1, Ordering::Relaxed);
    sbi::set_timer(now() + INTERVAL.load(Ordering::Relaxed));
    crate::sched::tick();
}
