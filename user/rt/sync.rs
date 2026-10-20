//! A mutex whose waiters sleep on a notification, the way futex-based
//! mutexes sleep in the kernel elsewhere (Drepper's "mutex2"). An
//! uncontended lock and unlock is one atomic operation each; only a thread
//! that finds the lock taken makes a system call, and an unlock signals only
//! if someone may be waiting.
//!
//! Each mutex needs a notification of its own: with one shared between two
//! mutexes, a waiter on one could take the signal meant for a waiter on the
//! other, which would then sleep with its mutex free. Until it has one, a
//! contended lock yields in a loop, which is enough while a program has a
//! single thread.

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicUsize, Ordering};

use sap_sys::{notification, sys};

/// How many times a thread slept waiting for a mutex, over all mutexes.
static SLEEPS: AtomicUsize = AtomicUsize::new(0);

/// How many times a thread slept waiting for a mutex, over all mutexes.
pub fn sleeps() -> usize {
    SLEEPS.load(Ordering::Relaxed)
}

const UNLOCKED: usize = 0;
const LOCKED: usize = 1;
/// Locked, and a thread may be waiting: unlocking must signal.
const CONTENDED: usize = 2;

pub struct Mutex<T> {
    state: AtomicUsize,
    /// The notification waiters sleep on (a capability address), or 0.
    notification: AtomicUsize,
    value: UnsafeCell<T>,
}

// SAFETY: the lock gives one thread at a time access to the value.
unsafe impl<T: Send> Sync for Mutex<T> {}

impl<T> Mutex<T> {
    pub const fn new(value: T) -> Self {
        Self { state: AtomicUsize::new(UNLOCKED), notification: AtomicUsize::new(0), value: UnsafeCell::new(value) }
    }

    /// Give the mutex a notification of its own to sleep on, with the
    /// right to signal and wait.
    pub fn set_notification(&self, cptr: usize) {
        self.notification.store(cptr, Ordering::Relaxed);
    }

    pub fn lock(&self) -> MutexGuard<'_, T> {
        if self.state.compare_exchange(UNLOCKED, LOCKED, Ordering::Acquire, Ordering::Relaxed).is_err() {
            // Mark it contended so the holder signals; whoever unlocked it
            // in the meantime leaves it UNLOCKED, and the swap takes it.
            while self.state.swap(CONTENDED, Ordering::Acquire) != UNLOCKED {
                match self.notification.load(Ordering::Relaxed) {
                    0 => sys::yield_now(),
                    // A stale signal only means one more look at the state.
                    n => {
                        SLEEPS.fetch_add(1, Ordering::Relaxed);
                        let _ = notification::wait(n);
                    }
                }
            }
        }
        MutexGuard { mutex: self }
    }
}

pub struct MutexGuard<'a, T> {
    mutex: &'a Mutex<T>,
}

impl<T> Deref for MutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: the guard holds the lock.
        unsafe { &*self.mutex.value.get() }
    }
}

impl<T> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: the guard holds the lock.
        unsafe { &mut *self.mutex.value.get() }
    }
}

impl<T> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        if self.mutex.state.swap(UNLOCKED, Ordering::Release) == CONTENDED {
            match self.mutex.notification.load(Ordering::Relaxed) {
                0 => {}
                n => {
                    let _ = notification::signal(n);
                }
            }
        }
    }
}
