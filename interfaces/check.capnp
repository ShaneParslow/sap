@0xaa31abaf4b600755;
# The bootstrap message of `check`, the program the root supervisor starts
# to check the kernel's features. Besides these imports it has a memory
# account (`process_slot::ACCOUNT`), whose chunks are the untyped memory
# the checks use. A capability is the index of one of the process's
# imports.
# TODO: capability fields once Cap'n Proto RPC brings capability tables (M6).

using Sap = import "sap.capnp";

struct CheckImports $Sap.program("check") $Sap.limits((
    memory = (account = (reservation = 33554432, limit = 50331648, processBits = 21)),
    priority = 255, maxPriority = 255, cnodeBits = 12, stackPages = 16)) {
  # The ASID pool, for address spaces of the processes the checks start.
  asidPool @0 :UInt32 $Sap.held("asidPool");
  # The instance API, and the hash of `peer` in the boot pack, for
  # starting it through the API.
  supervisor @1 :UInt32 $Sap.held("supervisor");
  peerHash @2 :Data $Sap.programHash("peer");
  # The log, which the checks' output goes to.
  log @3 :UInt32 $Sap.interface("log.capnp:Log");
  # A service its supervisor restarts when it fails.
  restartable @4 :UInt32 $Sap.interface("peer.capnp:Restartable");
}
