//! What a process started by a launcher receives: its startup record and
//! bootstrap message (the process ABI, `sap_abi::Startup`).

use sap_sys::abi::{launcher, process_slot, Startup, BOOTSTRAP_OFFSET, MAX_BOOTSTRAP};
use sap_sys::address::mapped;

/// The startup record whose address the launcher passed in `a0`.
///
/// # Safety
///
/// `a0` must be the first argument the launcher started this process's
/// main thread with, and the page must stay mapped, as the loader leaves it.
pub unsafe fn startup(a0: usize) -> &'static Startup {
    // SAFETY: the launcher mapped the record read-only at `a0`, and nobody
    // writes it after the process starts.
    unsafe { &*mapped::<Startup>(a0) }
}

/// The bootstrap message that follows `startup` in its page, in the flat
/// Cap'n Proto encoding.
pub fn bootstrap(startup: &'static Startup) -> &'static [u8] {
    let len = startup.bootstrap_len.min(MAX_BOOTSTRAP);
    let record: *const Startup = startup;
    // SAFETY: the bootstrap message lies in the same read-only page as the
    // record, `BOOTSTRAP_OFFSET` bytes in, and is at most `MAX_BOOTSTRAP`.
    unsafe { core::slice::from_raw_parts(record.cast::<u8>().add(BOOTSTRAP_OFFSET), len) }
}

/// The slot holding import `index`, if the process has that many.
pub fn import(startup: &Startup, index: u32) -> Option<usize> {
    let slot = startup.imports.start.checked_add(index as usize)?;
    (slot < startup.imports.end).then_some(slot)
}

/// Report exit with `code` (0 for success) to the launcher, then stop. The
/// launcher normally destroys the process when it hears; if nobody
/// listens, the thread suspends itself.
pub fn exit(code: usize) -> ! {
    let _ = sap_sys::ipc::send(process_slot::LAUNCHER, launcher::EXIT, &[code], &[]);
    loop {
        let _ = sap_sys::tcb::suspend(process_slot::TCB);
    }
}
