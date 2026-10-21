#!/usr/bin/env bash
# Build the CHERI-RISC-V (RVY draft) toolchain from the submodules here:
#   llvm     clang and lld from the RVY LLVM fork
#   qemu     qemu-system-riscv64y from the RVY QEMU fork
#   opensbi  a purecap OpenSBI for RVY, built with that clang (needs llvm)
# Everything goes to toolchain/out/, which git ignores. With no arguments,
# builds all three in that order.
#
# Needs cmake, ninja, clang, lld, python and dtc from the host.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
out="$here/out"
jobs=$(nproc)

build_llvm() {
  cmake -S "$here/llvm/llvm" -B "$out/llvm-build" -G Ninja \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX="$out/llvm" \
    -DCMAKE_C_COMPILER=clang \
    -DCMAKE_CXX_COMPILER=clang++ \
    -DLLVM_USE_LINKER=lld \
    -DLLVM_ENABLE_PROJECTS="clang;lld" \
    -DLLVM_TARGETS_TO_BUILD=RISCV \
    -DLLVM_DEFAULT_TARGET_TRIPLE=riscv64-unknown-elf \
    -DLLVM_ENABLE_ASSERTIONS=ON \
    -DLLVM_INSTALL_TOOLCHAIN_ONLY=ON \
    -DLLVM_INSTALL_BINUTILS_SYMLINKS=ON \
    -DLLVM_PARALLEL_LINK_JOBS=2 \
    -DLLVM_ENABLE_LIBXML2=OFF \
    -DLLVM_ENABLE_ZSTD=ON \
    -DLLVM_INCLUDE_EXAMPLES=OFF \
    -DLLVM_INCLUDE_DOCS=OFF \
    -DLLVM_INCLUDE_BENCHMARKS=OFF \
    -DLLVM_INCLUDE_TESTS=OFF \
    -DCLANG_ENABLE_STATIC_ANALYZER=OFF \
    -DCLANG_ENABLE_ARCMT=OFF
  ninja -C "$out/llvm-build" install
}

build_qemu() {
  # QEMU 7.1's meson needs distutils, which Python 3.12 removed; setuptools
  # in a private venv provides it.
  if [ ! -x "$out/venv/bin/python3" ]; then
    python3 -m venv "$out/venv"
    "$out/venv/bin/pip" install -q setuptools
  fi
  mkdir -p "$out/qemu-build"
  (cd "$out/qemu-build" && "$here/qemu/configure" \
    --python="$out/venv/bin/python3" \
    --prefix="$out/qemu" \
    --target-list=riscv64y-softmmu \
    --disable-werror --disable-docs)
  make -C "$out/qemu-build" -j"$jobs" install
}

build_opensbi() {
  local bin="$out/llvm/bin"
  [ -x "$bin/clang" ] || { echo "build.sh: build llvm first" >&2; exit 1; }
  # Purecap, soft-float, as cheribuild's cheri-alliance-opensbi target builds it.
  local isa=rv64ymaczyhybrid_zylg abi=l64pc128
  local cc="$bin/clang --target=riscv64-unknown-elf -march=$isa -mabi=$abi -mno-relax -Qunused-arguments"
  make -C "$here/opensbi" -j"$jobs" \
    O="$out/opensbi-build" I="$out/opensbi" \
    PLATFORM=generic \
    CC="$cc" CPP="$cc -E" LD="$bin/ld.lld" AR="$bin/llvm-ar" OBJCOPY="$bin/llvm-objcopy" \
    PLATFORM_RISCV_ISA=$isa PLATFORM_RISCV_ABI=$abi PLATFORM_RISCV_XLEN=64 \
    FW_PIC=n FW_TEXT_START=0x80000000 \
    all install
}

targets=("$@")
[ ${#targets[@]} -gt 0 ] || targets=(llvm qemu opensbi)
for t in "${targets[@]}"; do
  case "$t" in
    llvm | qemu | opensbi) "build_$t" ;;
    *) echo "build.sh: unknown target '$t' (llvm, qemu, opensbi)" >&2; exit 1 ;;
  esac
done
