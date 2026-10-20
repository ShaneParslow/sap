//! Untyped memory invocations.

use sap_abi::label;

use crate::cnode::SlotRef;
use crate::sys::{invoke, Error};

/// Create `count` objects of `object_type` (`sap_abi::object`) from the
/// untyped capability at `untyped`, with capabilities in slots
/// `offset..offset + count` of the CNode at `node`. `size_bits` is the
/// object size for untyped memory and the slot count for CNodes, in bits.
pub fn retype(untyped: usize, object_type: usize, size_bits: usize, node: SlotRef, offset: usize, count: usize) -> Result<(), Error> {
    let words = [object_type, size_bits, node.index, node.depth, offset, count];
    invoke(untyped, label::UNTYPED_RETYPE, &words, &[node.root]).map(drop)
}
