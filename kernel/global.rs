//! Kernel-global mutable state.
//!
//! The kernel runs on one hart, and with interrupts disabled everywhere
//! except the idle loop, which touches no globals. So kernel code is never
//! re-entered while it holds a reference to a global, and `Global` can hand
//! out `&mut` without locking.
//!
//! TODO: with more harts (SMP), take a big kernel lock on every kernel
//! entry, as seL4 does, and keep these unlocked inside it.

use core::cell::UnsafeCell;

pub struct Global<T>(UnsafeCell<T>);

// SAFETY: see the module comment; there is one hart and no re-entry.
unsafe impl<T> Sync for Global<T> {}

impl<T> Global<T> {
    pub const fn new(value: T) -> Self {
        Self(UnsafeCell::new(value))
    }

    /// # Safety
    /// No other reference obtained from this `Global` may be live.
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn get(&self) -> &mut T {
        // SAFETY: per the caller's guarantee.
        unsafe { &mut *self.0.get() }
    }
}
