//! CNode invocations. A slot is named by a CNode capability and an
//! (index, depth) pair within it; for a single-level CNode, the depth is
//! the full word, and the index is the slot number.

use sap_abi::label;

use crate::sys::{invoke, Error};

/// The depth that names a slot of the root task's CNode by its number.
pub const WORD: usize = usize::BITS as usize;

/// A slot: the CNode capability addressed through, the index and depth.
#[derive(Clone, Copy, Debug)]
pub struct SlotRef {
    pub root: usize,
    pub index: usize,
    pub depth: usize,
}

impl SlotRef {
    /// Slot `index` of the CNode capability at `root` in this thread's own
    /// capability space, named at full depth.
    pub const fn new(root: usize, index: usize) -> Self {
        Self { root, index, depth: WORD }
    }

    pub fn copy_from(self, src: SlotRef, rights: usize) -> Result<(), Error> {
        invoke(self.root, label::CNODE_COPY, &[self.index, self.depth, src.index, src.depth, rights], &[src.root]).map(drop)
    }

    pub fn mint_from(self, src: SlotRef, rights: usize, data: usize) -> Result<(), Error> {
        let words = [self.index, self.depth, src.index, src.depth, rights, data];
        invoke(self.root, label::CNODE_MINT, &words, &[src.root]).map(drop)
    }

    pub fn move_from(self, src: SlotRef) -> Result<(), Error> {
        invoke(self.root, label::CNODE_MOVE, &[self.index, self.depth, src.index, src.depth], &[src.root]).map(drop)
    }

    pub fn delete(self) -> Result<(), Error> {
        invoke(self.root, label::CNODE_DELETE, &[self.index, self.depth], &[]).map(drop)
    }

    pub fn revoke(self) -> Result<(), Error> {
        invoke(self.root, label::CNODE_REVOKE, &[self.index, self.depth], &[]).map(drop)
    }
}
