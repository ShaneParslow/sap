@0xce825fb9d5bed94e;
# The bootstrap message of `peer`, the test program init starts in a
# process of its own. A capability is the index of one of the process's
# imports.
# TODO: capability fields once Cap'n Proto RPC brings capability tables (M6).

enum Mode {
  # A call to init over `endpoint`, then a read of init's memory.
  call @0;
  # The channel modes, over two rings of `dataPages` data pages each.
  echoWords @1;
  serveEcho @2;
  misbehave @3;
  sumBytes @4;
}

struct PeerImports {
  mode @0 :Mode;
  # For `call`, the endpoint to call init on; for the channel modes, the
  # endpoint init sends page transfers on.
  endpoint @1 :UInt32;
  dataPages @2 :UInt32;
  # The channel modes: this process's notification, init's doorbell, and
  # a reply object to answer page transfers with.
  notification @3 :UInt32;
  doorbell @4 :UInt32;
  reply @5 :UInt32;
}
