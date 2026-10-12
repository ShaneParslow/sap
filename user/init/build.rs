//! Link as a static, position-independent executable: the loader picks the
//! address and applies the relative relocations. Each segment gets its own
//! pages so the loader can map them with different permissions.

fn main() {
    for arg in ["--pie", "--no-dynamic-linker", "-z", "separate-loadable-segments", "-z", "norelro"] {
        println!("cargo:rustc-link-arg-bins={arg}");
    }
}
