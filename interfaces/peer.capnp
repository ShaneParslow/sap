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
  # Exit with `exitCode`.
  exit @5;
  # Read address 0, which faults.
  crash @6;
  # Serve `Restartable` on `endpoint`, its export, for checking restart.
  # Its reply object is made from its memory account.
  restartable @7;
}

using Sap = import "sap.capnp";

# A service to restart: `peer` in its `restartable` mode serves it.
interface Restartable {
  # How many calls this process has answered, counting this one.
  count @0 () -> (calls :UInt64);
  # Fault.
  crash @1 () -> ();
}

# As a component (only in `restartable` mode), it serves `Restartable`.
struct PeerImports $Sap.program("peer") $Sap.exports([(name = "service", interface = "peer.capnp:Restartable")]) $Sap.limits((
    memory = (account = (reservation = 2097152, limit = 2097152, processBits = 20)),
    priority = 255, cnodeBits = 6, stackPages = 4)) {
  mode @0 :Mode;
  # For `call`, the endpoint to call init on; for the channel modes, the
  # endpoint init sends page transfers on; for `restartable`, the endpoint
  # it serves.
  endpoint @1 :UInt32 $Sap.serves("service");
  dataPages @2 :UInt32;
  # The channel modes: this process's notification, init's doorbell, and
  # a reply object to answer page transfers with.
  notification @3 :UInt32;
  doorbell @4 :UInt32;
  reply @5 :UInt32;
  exitCode @6 :UInt32;
}
