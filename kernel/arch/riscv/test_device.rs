//! QEMU's `sifive_test` device: writing to it ends the emulator with a
//! status code. SBI shutdown can't report failure under QEMU (OpenSBI
//! powers off the same way for every reason), so this is used when present.

use core::sync::atomic::{AtomicUsize, Ordering};

const FINISHER_FAIL: u32 = 0x3333;
const FINISHER_PASS: u32 = 0x5555;

/// MMIO address, or 0 if there is no test device.
static ADDRESS: AtomicUsize = AtomicUsize::new(0);

pub fn init(address: usize) {
    ADDRESS.store(address, Ordering::Relaxed);
}

/// Exit QEMU with status 0 on success or `code` on failure. Returns if there
/// is no test device. The MMU is off, so `address` is physical; once paging
/// is on, the device must be mapped.
pub fn exit(failure: bool, code: u16) {
    let address = ADDRESS.load(Ordering::Relaxed);
    if address == 0 {
        return;
    }
    let value = if failure { FINISHER_FAIL | (code as u32) << 16 } else { FINISHER_PASS };
    // SAFETY: `address` is the test device's register, from the device tree.
    unsafe { core::ptr::write_volatile(address as *mut u32, value) };
}
