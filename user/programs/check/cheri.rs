//! CHERI capabilities in memory, when the kernel enables them. Capabilities
//! in registers are the kernel's, tested by `test-kernel`.

use alloc::boxed::Box;
use sap_rt::println;
use crate::Env;

/// The loader and the heap map private pages with `attr::CAPABILITIES`, so
/// DDC stored to the stack or the heap with `sy` comes back tagged with
/// `ly`.
pub fn check_cheri(info: &Env) {
    if !info.cheri {
        return;
    }

    #[repr(C, align(16))]
    struct Slot([usize; 2]);

    let mut on_stack = Slot([0; 2]);
    let mut on_heap = Box::new(Slot([0; 2]));
    for (slot, place) in [(&mut on_stack as *mut Slot, "the stack"), (&mut *on_heap as *mut Slot, "the heap")] {
        let tag: usize;
        // SAFETY: `slot` is 16 aligned, writable bytes this function owns.
        unsafe {
            core::arch::asm!(
                "csrr {c}, 0x03f",
                ".insn s 0x7b, 2, {c}, 0({p})",
                ".insn i 0x7b, 1, {c}, 0({p})",
                ".insn r 0x7b, 0, 0x7a, {tag}, {c}, x4",
                c = out(reg) _,
                p = in(reg) slot,
                tag = lateout(reg) tag,
                options(nostack),
            );
        }
        assert_eq!(tag, 1, "a capability stored to {place} keeps its tag");
    }

    println!("CHERI capabilities survive loads and stores on the stack and heap");
}
