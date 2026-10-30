@0xaa31abaf4b600755;
# The bootstrap message of `check`, the program the root supervisor starts
# to check the kernel's features. Besides these imports it has a memory
# account (`process_slot::ACCOUNT`), whose chunks are the untyped memory
# the checks use. A capability is the index of one of the process's
# imports.
# TODO: capability fields once Cap'n Proto RPC brings capability tables (M6).

struct CheckImports {
  # The ASID pool, for address spaces of the processes the checks start.
  asidPool @0 :UInt32;
  # The process factory, and the hash of `peer` in the boot pack, for
  # starting it through the factory.
  factory @1 :UInt32;
  peerHash @2 :Data;
  # Whether user mode may use CHERI capabilities.
  cheri @3 :Bool;
  # The log, which the checks' output goes to.
  log @4 :UInt32;
  # A service its supervisor restarts when it fails (`peer`'s
  # `restartable`).
  restartable @5 :UInt32;
}
