//! Turning addresses into pointers: the one place user code does it.
//!
//! A program reaches memory it didn't get from Rust (its IPC buffer, the
//! boot-info page, frames it mapped, rings shared with another process) by
//! address. Every such pointer comes from `mapped`, and the user crates deny
//! casting integers to pointers (`implicit_provenance_casts`), so porting to
//! CHERI, where a pointer can't be made from an integer, changes this
//! function and nothing else (hw-cheri in the wiki).

/// A pointer to memory mapped at `address` in this address space.
///
/// The memory wasn't allocated by Rust, so the pointer takes the exposed
/// provenance Rust gives memory from outside the program.
///
/// TODO: under purecap CHERI, derive it from a capability for the address
/// space that the runtime holds, bounded to `size_of::<T>()` (M10).
#[allow(clippy::disallowed_methods)]
pub fn mapped<T>(address: usize) -> *mut T {
    core::ptr::with_exposed_provenance_mut(address)
}
