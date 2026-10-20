//! Page transfer: messages too large for a ring travel as whole frames,
//! whose capabilities the kernel moves from sender to receiver (page
//! transfer in ipc-channels in the wiki).
//!
//! The sender writes the message into frames it owns, unmaps them, and
//! calls the receiver's endpoint, moving the frame capabilities with the
//! call; the message words are the length in bytes and an id. For each
//! capability, the receiver asks the kernel what it is (`identify`, through
//! its own CNode, so nothing it was sent gets invoked) and requires an
//! unmapped 4 KiB frame with no other capability to it, then maps it
//! read-only. A frame capability maps at most once, so that mapping is the
//! frame's only one: nobody else can write the message while it is read,
//! and it can be read in place, through ordinary references. The receiver
//! then unmaps the frames and moves them back in its reply; the sender
//! checks what came back the same way and reuses the frames.
//!
//! The check needs doing once: new capabilities are only derived from
//! existing ones, and the receiver holds the only one. It trusts whoever
//! holds a capability to its own CNode (its parent), who could copy a frame
//! capability out of it; that holder can do anything to the receiver anyway.
//! Whoever holds the untyped memory the frames came from can revoke them,
//! which unmaps them; the receiver then faults rather than reading changed
//! memory.
//!
//! TODO: one call carries at most `MAX_PAGES` 4 KiB frames (64 KiB). Larger
//! messages need 2 MiB frames or several calls, and mapping a batch of
//! frames in one invocation would cut the system calls per page (M5
//! measurement, in ipc-page-transfer in the wiki).

use sap_sys::abi::{attr, object, rights, MAX_EXTRA_CAPS};
use sap_sys::address::mapped;
use sap_sys::cnode::SlotRef;
use sap_sys::ipc::{self, Give, Received};
use sap_sys::sys::Error;
use sap_sys::vspace;

use crate::PAGE;

/// The most frames one call carries.
pub const MAX_PAGES: usize = MAX_EXTRA_CAPS;
/// The longest message one call carries.
pub const MAX_LEN: usize = MAX_PAGES * PAGE;

/// Where a side keeps its frames: slots `first..first + MAX_PAGES` of its
/// root CNode, whose capability is at `cnode` (so a slot's index is also its
/// capability address), and virtual addresses `base..base + MAX_LEN` in the
/// address space at `vspace`, where they are mapped while in use.
#[derive(Clone, Copy, Debug)]
pub struct Frames {
    pub cnode: usize,
    pub first: usize,
    pub vspace: usize,
    pub base: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferError {
    /// The message is longer than the frames sent with it, or than one call
    /// carries.
    TooLong,
    /// A capability isn't a 4 KiB frame, or is mapped elsewhere.
    NotAFrame,
    /// Another capability names one of the frames, so its holder could
    /// write the message while it is read.
    Shared,
    /// The call didn't carry a length and the expected id.
    BadHeader,
    /// A system call failed.
    Sys(Error),
    /// The call's reply didn't bring every frame back, or brought back
    /// something else; the frames that did come back are deleted.
    Lost,
}

impl Frames {
    /// Check that slot `i` holds an unmapped 4 KiB frame that no other
    /// capability names.
    fn check(&self, i: usize) -> Result<(), TransferError> {
        let identity = SlotRef::new(self.cnode, self.first + i).identify().map_err(TransferError::Sys)?;
        if identity.object_type != object::FRAME_4K || identity.mapped() {
            return Err(TransferError::NotAFrame);
        }
        if !identity.exclusive() {
            return Err(TransferError::Shared);
        }
        Ok(())
    }

    fn map(&self, i: usize, granted: usize) -> Result<(), Error> {
        vspace::map_frame(self.first + i, self.vspace, self.base + i * PAGE, granted, attr::EXECUTE_NEVER)
    }

    fn unmap(&self, pages: usize) {
        for i in 0..pages {
            let _ = vspace::unmap_frame(self.first + i);
        }
    }
}

/// The sending side.
pub struct Sender {
    frames: Frames,
}

impl Sender {
    /// # Safety
    /// The slots must hold `MAX_PAGES` 4 KiB frame capabilities, unmapped,
    /// with no other capabilities to the frames; the virtual addresses must
    /// be unused, with page tables in place, and reserved to this sender.
    pub unsafe fn new(frames: Frames) -> Self {
        Self { frames }
    }

    /// Write a message of `len` bytes with `write`, then call `ep` with
    /// `label`, moving the frames to the receiver. Returns the reply, which
    /// must bring every frame back.
    pub fn call(&mut self, ep: usize, label: usize, id: u64, len: usize, write: impl FnOnce(&mut [u8]), out: &mut [usize]) -> Result<Received, TransferError> {
        if len > MAX_LEN {
            return Err(TransferError::TooLong);
        }
        let f = self.frames;
        let pages = len.div_ceil(PAGE);
        for i in 0..pages {
            if let Err(e) = f.map(i, rights::READ | rights::WRITE) {
                f.unmap(i);
                return Err(TransferError::Sys(e));
            }
        }
        // SAFETY: the frames are mapped read-write at `base`, contiguously,
        // and only this sender maps them (the caller's guarantee for `new`).
        let bytes = unsafe { core::slice::from_raw_parts_mut(mapped::<u8>(f.base), pages * PAGE) };
        write(&mut bytes[..len]);
        // The rest of the last page would show the receiver an older message.
        bytes[len..].fill(0);
        f.unmap(pages);

        ipc::set_receive_window(f.cnode, f.first, pages);
        let mut gives = [Give::moved(0); MAX_PAGES];
        for (i, give) in gives.iter_mut().enumerate().take(pages) {
            *give = Give::moved(f.first + i);
        }
        let reply = ipc::call(ep, label, &[len, id as usize], &gives[..pages], out).map_err(TransferError::Sys)?;
        // The receiver could hand back something other than the frames; a
        // capability of another type would do something else when mapped.
        if reply.info.extra_caps() != pages || (0..pages).any(|i| f.check(i).is_err()) {
            for i in 0..pages {
                let _ = SlotRef::new(f.cnode, f.first + i).delete();
            }
            return Err(TransferError::Lost);
        }
        Ok(reply)
    }
}

/// The receiving side.
pub struct Receiver {
    frames: Frames,
}

impl Receiver {
    /// # Safety
    /// The slots must be empty, and the virtual addresses unused, with page
    /// tables in place, and reserved to this receiver.
    pub unsafe fn new(frames: Frames) -> Self {
        Self { frames }
    }

    /// Make this thread's receive window the receiver's slots. Do this
    /// before receiving a transfer.
    pub fn arm(&self) {
        ipc::set_receive_window(self.frames.cnode, self.frames.first, MAX_PAGES);
    }

    /// The length and id of the transfer `message` (its first two words, as
    /// received in `words`).
    pub fn header(words: &[usize]) -> (usize, u64) {
        (words[0], words[1] as u64)
    }

    /// Map the `count` frames a transfer delivered read-only, check that
    /// each is exclusive, and lend the message's `len` bytes to `read`. The
    /// frames are unmapped again afterwards, whatever the outcome; return
    /// them with `give_back`.
    pub fn read<R>(&self, count: usize, len: usize, read: impl FnOnce(&[u8]) -> R) -> Result<R, TransferError> {
        let f = self.frames;
        let pages = len.div_ceil(PAGE);
        if pages > count || count > MAX_PAGES {
            return Err(TransferError::TooLong);
        }
        for i in 0..pages {
            if let Err(e) = f.check(i) {
                f.unmap(i);
                return Err(e);
            }
            if let Err(e) = f.map(i, rights::READ) {
                f.unmap(i);
                return Err(TransferError::Sys(e));
            }
        }
        // SAFETY: the frames are mapped read-only at `base`, contiguously,
        // and this mapping is their only one: each was unmapped and the only
        // capability to its frame, which this receiver holds (and new
        // capabilities only derive from existing ones), and a frame
        // capability maps once. So nothing writes them until they are
        // unmapped below.
        let result = read(unsafe { core::slice::from_raw_parts(mapped::<u8>(f.base), len) });
        f.unmap(pages);
        Ok(result)
    }

    /// Answer the transfer's call through `reply`, moving the `count`
    /// frames back.
    pub fn give_back(&self, reply: usize, count: usize) -> Result<(), Error> {
        let mut gives = [Give::moved(0); MAX_PAGES];
        let count = count.min(MAX_PAGES);
        for (i, give) in gives.iter_mut().enumerate().take(count) {
            *give = Give::moved(self.frames.first + i);
        }
        ipc::reply(reply, 0, &[], &gives[..count])
    }

    /// Delete whatever is left in the receive window: frames a caller
    /// didn't wait to get back, or capabilities that weren't frames and
    /// weren't given back.
    pub fn clear(&self) {
        for i in 0..MAX_PAGES {
            let _ = SlotRef::new(self.frames.cnode, self.frames.first + i).delete();
        }
    }
}
