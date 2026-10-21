@0xaa31abaf4b600755;
# The bootstrap message of `check`, the program the root task starts to
# check the kernel's features. It gets the authority the checks need, which
# only the root task holds at boot. A capability is the index of one of
# the process's imports.
# TODO: capability fields once Cap'n Proto RPC brings capability tables (M6).

struct CheckImports {
  # The ASID pool, for address spaces of the processes the checks start.
  asidPool @0 :UInt32;
  # Untyped memory, as the kernel describes it to the root task.
  untyped @1 :List(Untyped);
  # The process factory, and the hash of `peer` in the boot pack, for
  # starting it through the factory.
  factory @2 :UInt32;
  peerHash @3 :Data;
}

struct Untyped {
  import @0 :UInt32;
  sizeBits @1 :UInt8;
  paddr @2 :UInt64;
}
