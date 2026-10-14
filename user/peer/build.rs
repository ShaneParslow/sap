//! Link as a static, position-independent executable: the loader picks the
//! address and applies the relative relocations. Each segment gets its own
//! pages so the loader can map them with different permissions.
//!
//! `-z notext` allows relocations in read-only segments. The precompiled
//! `core` for this target isn't position-independent, so its vtables need
//! them; the loader applies relocations before mapping anything, so it can
//! write there.

fn main() {
    for arg in ["--pie", "--no-dynamic-linker", "-z", "separate-loadable-segments", "-z", "norelro", "-z", "notext"] {
        println!("cargo:rustc-link-arg-bins={arg}");
    }
}
