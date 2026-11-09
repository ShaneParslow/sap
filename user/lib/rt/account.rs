//! Calls on memory accounts (`interfaces/system/memory.capnp`), laid out as
//! `interfaces/README.md` describes. Messages are built on the stack: nothing here
//! allocates, since the heap grows through an account.

use capnp::message::{Builder, SingleSegmentAllocator};
use sap_interfaces::memory_capnp::memory_account::{allocate_params, allocate_results, child_params, release_params, usage_results};
use sap_interfaces::memory_capnp::account_control::set_limit_params;
use sap_interfaces::{methods, words};
use sap_sys::ipc;
use sap_sys::sys::ipc_buffer;

pub use sap_interfaces::memory_capnp::Status;

/// Why a call on an account failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The memory manager refused it.
    Refused(Status),
    /// The call didn't reach the memory manager: the kernel answered, for
    /// an empty slot or a capability that isn't an account's.
    /// TODO: the kernel answers a call on a non-endpoint with its error
    /// code as the label, which a server's labels can't be told apart
    /// from; codes 1 to 5 read as a `Status` (design/kernel.md in
    /// sap-docs).
    Call(sap_sys::sys::Error),
    /// The reply wasn't what the interface says.
    BadReply,
}

/// A chunk `allocate` gave.
#[derive(Clone, Copy, Debug)]
pub struct Chunk {
    /// Its name, for `release`.
    pub id: u64,
    pub paddr: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Usage {
    pub used: u64,
    pub reservation: u64,
    pub limit: u64,
}

/// Room for any of these calls' params or results.
const WORDS: usize = 8;

/// The message `build` builds, as IPC words, and their number.
fn message<T: capnp::traits::Owned>(build: impl FnOnce(T::Builder<'_>)) -> ([usize; WORDS], usize) {
    let mut scratch = [0u64; WORDS];
    // SAFETY: any bytes are valid `u8`s, and the slice covers exactly the
    // scratch words, which it borrows.
    let bytes = unsafe { core::slice::from_raw_parts_mut(scratch.as_mut_ptr().cast::<u8>(), WORDS * 8) };
    let mut allocator = SingleSegmentAllocator::new(bytes);
    let mut message = Builder::new(&mut allocator);
    build(message.init_root::<T::Builder<'_>>());
    let mut out = [0; WORDS];
    let n = words::pack(&message, &mut out).expect("account: a message too large for its buffer");
    (out, n)
}

/// Call `cap` with `label` and `words`, receiving `caps` capabilities into
/// slots `first..` of the CNode at `cnode`; the reply's words. The
/// thread's receive window is put back afterwards.
fn call(cap: usize, label: usize, words: &[usize], receive: (usize, usize, usize)) -> Result<([usize; WORDS], usize), Error> {
    let buffer = ipc_buffer();
    let saved = (buffer.receive_cnode, buffer.receive_index, buffer.receive_count);
    ipc::set_receive_window(receive.0, receive.1, receive.2);
    let mut out = [0; WORDS];
    let reply = ipc::call(cap, label, words, &[], &mut out);
    ipc::set_receive_window(saved.0, saved.1, saved.2);
    let reply = reply.map_err(Error::Call)?;
    match reply.info.label() {
        0 if reply.info.extra_caps() == receive.2 => Ok((out, reply.info.length().min(WORDS))),
        0 => Err(Error::BadReply),
        label => match u16::try_from(label).ok().and_then(|l| Status::try_from(l).ok()) {
            Some(status) => Err(Error::Refused(status)),
            None => Err(Error::Call(sap_sys::sys::Error(label))),
        },
    }
}

const NONE: (usize, usize, usize) = (0, 0, 0);

/// A chunk of 2^`size_bits` bytes from `account`, into slot `slot` of the
/// CNode at `cnode`.
pub fn allocate(account: usize, size_bits: u8, cnode: usize, slot: usize) -> Result<Chunk, Error> {
    let (params, n) = message::<allocate_params::Owned>(|mut p| p.set_size_bits(size_bits));
    let (out, len) = call(account, methods::memory_account::ALLOCATE, &params[..n], (cnode, slot, 1))?;
    let segment = [words::bytes(&out[..len])];
    let reader = words::reader(&segment);
    let results = reader.get_root::<allocate_results::Reader>().map_err(|_| Error::BadReply)?;
    Ok(Chunk { id: results.get_id(), paddr: results.get_paddr() })
}

/// Give chunk `id` back to `account`.
pub fn release(account: usize, id: u64) -> Result<(), Error> {
    let (params, n) = message::<release_params::Owned>(|mut p| p.set_id(id));
    call(account, methods::memory_account::RELEASE, &params[..n], NONE).map(drop)
}

/// A child of `account` with `reservation` and `limit`: its own
/// capability into slot `first` of the CNode at `cnode`, its control's
/// into `first + 1`.
pub fn child(account: usize, reservation: u64, limit: u64, cnode: usize, first: usize) -> Result<(), Error> {
    let (params, n) = message::<child_params::Owned>(|mut p| {
        p.set_reservation(reservation);
        p.set_limit(limit);
    });
    call(account, methods::memory_account::CHILD, &params[..n], (cnode, first, 2)).map(drop)
}

pub fn usage(account: usize) -> Result<Usage, Error> {
    let (out, len) = call(account, methods::memory_account::USAGE, &[], NONE)?;
    let segment = [words::bytes(&out[..len])];
    let reader = words::reader(&segment);
    let results = reader.get_root::<usage_results::Reader>().map_err(|_| Error::BadReply)?;
    Ok(Usage { used: results.get_used(), reservation: results.get_reservation(), limit: results.get_limit() })
}

/// Release every chunk of the account `control` controls, and end its
/// children.
pub fn revoke(control: usize) -> Result<(), Error> {
    call(control, methods::account_control::REVOKE, &[], NONE).map(drop)
}

pub fn set_limit(control: usize, limit: u64) -> Result<(), Error> {
    let (params, n) = message::<set_limit_params::Owned>(|mut p| p.set_limit(limit));
    call(control, methods::account_control::SET_LIMIT, &params[..n], NONE).map(drop)
}

/// Revoke the account `control` controls, then end it.
pub fn destroy(control: usize) -> Result<(), Error> {
    call(control, methods::account_control::DESTROY, &[], NONE).map(drop)
}
