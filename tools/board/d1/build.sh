#!/usr/bin/env bash
# Build sap for the MangoPi MQ-Pro (Allwinner D1, one T-Head C906): the
# plain RISC-V kernel in release mode, as the payload of OpenSBI's
# fw_payload with the board's device tree built in, and the stubs run.sh
# loads before it. Everything goes to target/board/d1/. Extra arguments go
# to cargo build.
#
# The kernel needs no change for the board; only the firmware and the
# device tree differ from QEMU's. OpenSBI comes from toolchain/opensbi,
# built here for plain RV64GC.
#
# Needs dtc and an LLVM with RISC-V: toolchain/out/llvm if it is built
# (toolchain/build.sh llvm), else clang, ld.lld and llvm-objcopy on the
# PATH, or the directory in $LLVM.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
out="$root/target/board/d1"
mkdir -p "$out"

llvm=${LLVM:-$root/toolchain/out/llvm/bin}
if [ -x "$llvm/clang" ]; then
  tool() { echo "$llvm/$1"; }
else
  tool() { command -v "$1" || { echo "build.sh: no $1 (set LLVM)" >&2; exit 1; }; }
fi
clang=$(tool clang) lld=$(tool ld.lld) objcopy=$(tool llvm-objcopy) ar=$(tool llvm-ar)

cargo build --manifest-path "$root/kernel/Cargo.toml" --release "$@"
"$objcopy" -O binary "$root/target/riscv64gc-unknown-none-elf/release/sap-kernel" "$out/sap-kernel.bin"

dtc -q -I dts -O dtb -o "$out/mq-pro.dtb" "$here/mq-pro.dts"

for stub in caches probe; do
  "$clang" --target=riscv64 -march=rv64gc -mno-relax -c "$here/$stub.S" -o "$out/$stub.o"
  "$lld" -Ttext=0x41f00000 "$out/$stub.o" -o "$out/$stub.elf"
  "$objcopy" -O binary "$out/$stub.elf" "$out/$stub.bin"
done

# OpenSBI's make doesn't notice a changed payload or device tree; rebuild
# the firmware images each time.
rm -rf "$out/opensbi/platform/generic/firmware"
make -s -C "$root/toolchain/opensbi" -j"$(nproc)" \
  O="$out/opensbi" PLATFORM=generic \
  LLVM=1 CC="$clang" AR="$ar" LD="$lld" OBJCOPY="$objcopy" \
  PLATFORM_RISCV_ISA=rv64imafdc_zicsr_zifencei PLATFORM_RISCV_ABI=lp64d PLATFORM_RISCV_XLEN=64 \
  FW_TEXT_START=0x40000000 FW_PAYLOAD_OFFSET=0x200000 \
  FW_PAYLOAD_PATH="$out/sap-kernel.bin" FW_FDT_PATH="$out/mq-pro.dtb"
cp "$out/opensbi/platform/generic/firmware/fw_payload.bin" "$out/fw_payload.bin"
echo "build.sh: $out/fw_payload.bin"
