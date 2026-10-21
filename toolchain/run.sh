#!/usr/bin/env bash
# Build the kernel with the `cheri` feature and boot it on the RVY QEMU
# with the purecap OpenSBI, both from toolchain/out/ (toolchain/build.sh).
# Extra arguments go to cargo build, for example --release.
#
# The firmware writes above QEMU's default 128 MiB of RAM, so the machine
# gets 1 GiB.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
root=$(dirname "$here")
profile=debug
for arg in "$@"; do
  [ "$arg" = --release ] && profile=release
done
cargo build --manifest-path "$root/kernel/Cargo.toml" --features cheri "$@"
exec "$here/out/qemu/bin/qemu-system-riscv64y" -machine virt -m 1G -nographic \
  -bios "$here/out/opensbi/share/opensbi/l64pc128/generic/firmware/fw_jump.bin" \
  -kernel "$root/target/riscv64gc-unknown-none-elf/$profile/sap-kernel"
