//! The boot pack: the programs init can start before there is a store,
//! built and embedded by `build.rs`, each named by the BLAKE3 hash of its
//! image and with its manifest (the boot image of design/processes.md in
//! sap-docs).

/// A program's name: the BLAKE3 hash of its image.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Hash(pub [u8; 32]);

impl core::fmt::Display for Hash {
    /// The first eight bytes in hex, enough to tell programs apart in a log.
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        self.0[..8].iter().try_for_each(|b| write!(f, "{b:02x}"))
    }
}

pub struct Program {
    pub name: &'static str,
    pub hash: Hash,
    image: &'static [u8],
    /// Its declarations (`component_capnp::Manifest`): the words of a
    /// single-segment message.
    pub manifest: &'static [u64],
}

include!(concat!(env!("OUT_DIR"), "/pack.rs"));

/// The image of the program named `hash`, if the pack has it and its bytes
/// still hash to that name.
pub fn find(hash: &Hash) -> Option<&'static [u8]> {
    let program = PROGRAMS.iter().find(|p| p.hash == *hash)?;
    (blake3::hash(program.image).as_bytes() == &hash.0).then_some(program.image)
}

impl Program {
    /// The manifest as IPC-style words, for `sap_interfaces::words`.
    pub fn manifest_words(&self) -> &'static [usize] {
        // SAFETY: `usize` is `u64` on every target sap supports (RV64), so
        // the slice covers the same memory with the same layout.
        const _: () = assert!(size_of::<usize>() == size_of::<u64>());
        unsafe { core::slice::from_raw_parts(self.manifest.as_ptr().cast::<usize>(), self.manifest.len()) }
    }
}
