//! Cap'n Proto messages carried in the words of one IPC message, as
//! interfaces with methods are until Cap'n Proto RPC runs over sap's IPC
//! (M6): the single-segment encoding without a segment table, as the
//! bootstrap message has it (`sap_process::Startup`), little-endian.

use capnp::message::{Builder, ReaderOptions, SegmentArray, SingleSegmentAllocator};

/// The message in `words` as bytes, for `SegmentArray`.
pub fn bytes(words: &[usize]) -> &[u8] {
    // SAFETY: any bytes are valid `u8`s, and the slice covers exactly the
    // words' memory, which it borrows.
    unsafe { core::slice::from_raw_parts(words.as_ptr().cast::<u8>(), core::mem::size_of_val(words)) }
}

/// A reader of the message in `segment`, which is `[bytes(words)]`.
pub fn reader<'a>(segment: &'a [&'a [u8]; 1]) -> capnp::message::Reader<SegmentArray<'a>> {
    capnp::message::Reader::new(SegmentArray::new(segment), ReaderOptions::new())
}

/// Copy the message `builder` holds into `words`, and return how many
/// words it takes; `None` if it is more than one segment or doesn't fit.
pub fn pack(builder: &Builder<&mut SingleSegmentAllocator>, words: &mut [usize]) -> Option<usize> {
    let segments = builder.get_segments_for_output();
    let [segment] = &segments[..] else { return None };
    let n = segment.len().div_ceil(8);
    if n > words.len() {
        return None;
    }
    for (word, chunk) in words.iter_mut().zip(segment.chunks(8)) {
        let mut bytes = [0u8; 8];
        bytes[..chunk.len()].copy_from_slice(chunk);
        *word = u64::from_le_bytes(bytes) as usize;
    }
    Some(n)
}
