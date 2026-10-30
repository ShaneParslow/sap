# capnpc, vendored

capnpc 0.27.0 from crates.io (MIT, `LICENSE`), the Cap'n Proto code
generator for Rust, used by `user/lib/interfaces/build.rs`. Vendored
unchanged first; sap's changes are the later commits to this directory
(`git log -- third_party/capnpc`). The root `Cargo.toml` patches crates.io's
`capnpc` with it.

Changes:
- `CompilerCommand::no_rpc`: no RPC code (interfaces' clients, servers and
  dispatch; accessors of interface-typed fields). sap's calls are its own,
  and capnp-rust's RPC code needs capnp's `alloc` feature, under which
  building any message allocates.
