//! RISC-V (RV64GC, S-mode under SBI firmware).

mod entry;
mod paging;
mod sbi;
mod test_device;
pub mod timer;
pub mod trap;

pub use paging::phys_to_virt;

use core::sync::atomic::{AtomicU8, Ordering};

use crate::boot::Region;
use crate::devicetree;
use crate::mm::{self, BootAllocator, PAGE_SIZE};

/// Called by the entry code, running at the kernel's linked address on the
/// boot page table, with OpenSBI's arguments and the image's physical
/// address. Turns them into the architecture-neutral `BootInfo`, sets up
/// traps, paging and the timer, and enters the generic kernel.
#[unsafe(no_mangle)]
extern "C" fn riscv_start(hart_id: usize, dtb: usize, image_pa: usize) -> ! {
    paging::set_kernel_pbase(image_pa);
    trap::init();

    // Find QEMU's test device first, so that a panic later in boot still
    // exits the emulator with a failure status.
    let test_device = devicetree::find_device(dtb, &["sifive,test0"]);
    if let Some(address) = test_device {
        test_device::init(address);
    }

    let mut boot = devicetree::parse(dtb, hart_id);
    assert!(boot.reserved.push(paging::kernel_image()), "boot: no room to reserve the kernel image");

    let mut alloc = BootAllocator::new(mm::free_regions(&boot.memory, &boot.reserved));
    let devices = test_device.map(|start| Region { start, size: PAGE_SIZE });
    paging::init(&boot, devices.as_slice(), &mut alloc);
    boot.free = alloc.into_free();

    timer::init(boot.timer_hz);
    crate::kmain(boot)
}

const CONSOLE_UNPROBED: u8 = 0;
const CONSOLE_DBCN: u8 = 1;
const CONSOLE_LEGACY: u8 = 2;

/// Which SBI console to use, found on first write. Relaxed is enough: every
/// hart that probes stores the same answer, and nothing else is published
/// through this value.
static CONSOLE: AtomicU8 = AtomicU8::new(CONSOLE_UNPROBED);

pub fn console_write(bytes: &[u8]) {
    let mut kind = CONSOLE.load(Ordering::Relaxed);
    if kind == CONSOLE_UNPROBED {
        kind = if sbi::has_debug_console() { CONSOLE_DBCN } else { CONSOLE_LEGACY };
        CONSOLE.store(kind, Ordering::Relaxed);
    }
    if kind == CONSOLE_DBCN && sbi::debug_console_write(bytes) {
        return;
    }
    bytes.iter().copied().for_each(sbi::legacy_putchar);
}

/// Power off the machine; under QEMU this exits the emulator.
pub fn shutdown(failure: bool) -> ! {
    test_device::exit(failure, 1);
    sbi::shutdown(failure);
    halt()
}

pub fn enable_interrupts() {
    // SAFETY: the trap vector is installed before this is called.
    unsafe { riscv::register::sstatus::set_sie() };
}

/// Sleep until an interrupt arrives (or return spuriously).
pub fn wait_for_interrupt() {
    riscv::asm::wfi();
}

pub fn halt() -> ! {
    loop {
        riscv::asm::wfi();
    }
}
