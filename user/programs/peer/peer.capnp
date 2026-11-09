@0xce825fb9d5bed94e;
# `peer` as a component (`interfaces/system/component.capnp`): the test
# program `check` starts in processes of its own, and, in its `restartable`
# mode, a service for checking restart (`interfaces/test/restartable.capnp`).
# TODO: capability fields once RPC brings capability tables (M6).

using Component = import "/system/component.capnp";

# Its bootstrap message. A capability is the index of one of its imports.
struct PeerBootstrap $Component.program("peer")
    $Component.exports([(name = "service", interface = "test/restartable.capnp:Restartable")])
    $Component.defaultLimits((memory = (account = (reservation = 2097152, limit = 2097152, processBits = 20)),
                              priority = 255, cnodeBits = 6, stackPages = 4)) {
  # For `call`, the endpoint to call its parent on; for the channel modes,
  # the endpoint page transfers arrive on; for `restartable`, the endpoint
  # it serves.
  endpoint @0 :UInt32 $Component.serves("service");

  # What it does; by default, as a component, it serves `Restartable`.
  mode :union {
    # Serve `Restartable` on `endpoint`, its export. Its reply object is
    # made from its memory account.
    restartable @1 :Void;
    # A call to its parent over `endpoint`, then a read of the parent's
    # memory.
    call @2 :Void;
    # The channel modes, over `channel`.
    echoWords @3 :Void;
    serveEcho @4 :Void;
    misbehave @5 :Void;
    sumBytes @6 :Void;
    # Exit with this code.
    exit @7 :UInt32;
    # Read address 0, which faults.
    crash @8 :Void;
  }

  # For the channel modes: two rings of `dataPages` data pages each, this
  # process's notification, its parent's doorbell, and a reply object to
  # answer page transfers with.
  channel :group {
    dataPages @9 :UInt32;
    notification @10 :UInt32;
    doorbell @11 :UInt32;
    reply @12 :UInt32;
  }
}
