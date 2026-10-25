//! The boot pack: the programs init can start before there is a store,
//! built and embedded by `build.rs`, each named by the BLAKE3 hash of its
//! image (the boot image of the wiki's prog-loading page).

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
}

include!(concat!(env!("OUT_DIR"), "/pack.rs"));

/// The image of the program named `hash`, if the pack has it and its bytes
/// still hash to that name.
pub fn find(hash: &Hash) -> Option<&'static [u8]> {
    let program = PROGRAMS.iter().find(|p| p.hash == *hash)?;
    (blake3::hash(program.image).as_bytes() == &hash.0).then_some(program.image)
}
