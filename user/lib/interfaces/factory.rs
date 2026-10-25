//! The process factory: the calls a process manager answers on its
//! factory and process-handle capabilities (`init` serves it). Plain words
//! for now.
// TODO: a Cap'n Proto interface once RPC exists (M6).

/// On the factory: start a program. Words: the BLAKE3 hash of its image
/// (`HASH_WORDS` words, little-endian), the bootstrap message's length
/// in bytes, then the message packed into words, little-endian.
/// Capabilities: the untyped memory to build the process from, moved,
/// then the process's imports in order. The reply's label is `OK` or an
/// error below; on `OK` it carries the process handle, and on an error
/// the untyped memory comes back. Imports are dropped on an error.
pub const SPAWN: usize = 1;
/// On a process handle: wait for the process to end. The reply's words
/// are how it ended (`EXITED`, `FAULTED`, `KILLED`) and its code (the
/// exit code, or the fault label); it carries the untyped memory the
/// process was built from, and the handle stops working.
pub const WAIT: usize = 2;
/// On a process handle: destroy the process. Reply `OK`; whoever waits
/// hears `KILLED`.
pub const KILL: usize = 3;

pub const HASH_WORDS: usize = 4;
/// The longest bootstrap message a `SPAWN` carries, in bytes.
pub const MAX_BOOTSTRAP: usize = (sap_abi::MAX_MESSAGE_LENGTH - HASH_WORDS - 1) * 8;

/// Reply labels.
pub const OK: usize = 0;
/// No program with that hash.
pub const NO_SUCH_PROGRAM: usize = 1;
/// A malformed request, a capability of the wrong type, or a call on a
/// handle that doesn't allow it (a second `WAIT`).
pub const BAD_REQUEST: usize = 2;
/// Loading failed, usually for lack of untyped memory.
pub const LOAD_FAILED: usize = 3;
/// The process manager has no room for another process.
pub const FULL: usize = 4;

/// How a process ended, in a `WAIT` reply.
pub const EXITED: usize = 0;
pub const FAULTED: usize = 1;
pub const KILLED: usize = 2;
