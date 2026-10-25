#!/usr/bin/env bash
# Boot sap on the MangoPi MQ-Pro over USB and print its console until the
# kernel halts. Build first with build.sh.
#
#   run.sh          boot target/board/d1/fw_payload.bin
#   run.sh probe    read the C906's vendor CSRs (probe.S) and return
#
# The board must be in the boot ROM's FEL mode: no SD card, powered over
# its USB-OTG port. Before jumping, run.sh arms the SoC watchdog, which
# resets the board back into FEL 16 s later however the run ends, since
# the D1 has no power-off; so runs can follow each other unattended, and a
# run lasts at most 16 s.
#
# Settings, from the environment:
#   XFEL     the FEL loader (default toolchain/out/xfel/bin/xfel, built by
#            toolchain/build.sh xfel, else xfel on the PATH)
#   SERIAL   the USB-serial adapter on the board's UART0 (/dev/ttyUSB0)
#   CACHES   1 (default) to turn the caches on first with caches.S; 0 to
#            run with them off, as the boot ROM leaves them
#   LOG      where to keep the console output (target/board/d1/console.log)
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
out="$root/target/board/d1"
serial=${SERIAL:-/dev/ttyUSB0}
caches=${CACHES:-1}
log=${LOG:-$out/console.log}
fw="$out/fw_payload.bin"
stub_address=0x41f00000

xfel=${XFEL:-$root/toolchain/out/xfel/bin/xfel}
[ -x "$xfel" ] || xfel=$(command -v xfel) || {
  echo "run.sh: no xfel; build it with toolchain/build.sh xfel" >&2
  exit 1
}

# Wait up to 20 s for the boot ROM's FEL device, which comes back a few
# seconds after the previous run's watchdog reset.
wait_for_fel() {
  local i=0
  until timeout 3 "$xfel" version >/dev/null 2>&1; do
    i=$((i + 1))
    [ $i -lt 20 ] || { echo "run.sh: no FEL device (is the board in FEL mode?)" >&2; exit 1; }
    sleep 1
  done
}

if [ "${1:-}" = probe ]; then
  wait_for_fel
  "$xfel" ddr d1
  "$xfel" write $stub_address "$out/probe.bin"
  "$xfel" exec $stub_address
  echo "mxstatus, mhcr, mcor, mhint:"
  "$xfel" hexdump 0x41f01000 32
  exit 0
fi

[ -f "$fw" ] || { echo "run.sh: no $fw; run build.sh first" >&2; exit 1; }
if [ ! -r "$serial" ] || [ ! -w "$serial" ]; then
  echo "run.sh: can't open $serial; it usually belongs to the uucp group" \
    "(Arch) or dialout (Debian), which you need to be in" >&2
  exit 1
fi
# A second reader on the port would take part of the output.
if command -v fuser >/dev/null && fuser -s "$serial" 2>/dev/null; then
  echo "run.sh: $serial is already open (another console reader?)" >&2
  exit 1
fi

wait_for_fel

stty -F "$serial" 115200 raw -echo cs8 -cstopb -parenb
mkdir -p "$(dirname "$log")"
: > "$log"
cat "$serial" > "$log" &
reader=$!
tail -n +1 -f "$log" &
printer=$!
trap 'kill $reader $printer 2>/dev/null || true' EXIT

"$xfel" ddr d1
"$xfel" write 0x40000000 "$fw"
[ "$caches" = 1 ] && "$xfel" write $stub_address "$out/caches.bin"
# The watchdog at 0x020500a0 (registers as in Linux's sunxi_wdt.c, key
# 0x16aa in the top half of each write): reset the whole system, 16 s,
# reload the counter, enable.
"$xfel" write32 0x020500b4 0x16aa0001
"$xfel" write32 0x020500b8 0x16aa00b0
"$xfel" write32 0x020500b0 0x000014af
"$xfel" write32 0x020500b8 0x16aa00b1
if [ "$caches" = 1 ]; then
  "$xfel" exec $stub_address
else
  "$xfel" exec 0x40000000
fi

# Until the kernel halts or panics, or the watchdog's 16 s are up.
for _ in $(seq 160); do
  grep -q "halted by user\|panic" "$log" && break
  sleep 0.1
done
sleep 0.2
grep -q "halted by user (success)" "$log"
