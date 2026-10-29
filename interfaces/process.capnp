@0xb2a2482a958ff829;
# The process factory: how a program asks the process manager to start
# another, and the handle it gets back (design/processes.md in sap-docs).
# `init` serves both interfaces.
#
# Until Cap'n Proto RPC runs over sap's IPC (M6), a call is one IPC
# message on a capability whose badge says which interface it is: the
# label is the method's ordinal, the words are its params struct as a
# single-segment message, and the reply's label is a `Status`, with the
# results struct as its words. Capability fields are left null; the
# capabilities travel beside the message by kernel capability transfer,
# in the order of the fields that name them.
# TODO: capability fields once Cap'n Proto RPC brings capability tables (M6).

interface ProcessFactory {
  # Start the program whose image has the BLAKE3 hash `program` (32 bytes)
  # in the boot pack, with `bootstrap` as its bootstrap message: a
  # single-segment Cap'n Proto message whose root struct is the program's
  # imports. Every object of the new process is made from `memory`, which
  # is moved to the factory and comes back from `Process.wait`; ending the
  # process is one revoke of it. `imports` are moved into the process's
  # capability table in order.
  #
  # Capabilities in: `memory`, then `imports`. Out: `handle` on `ok`;
  # `memory` back on any other status once the factory had it.
  # Status: `noSuchProgram`, `badRequest` (no memory, memory that isn't
  # untyped, a malformed message), `loadFailed` (usually too little
  # memory), `full`. Imports are dropped on a failure.
  spawn @0 (program :Data, bootstrap :Data, memory :Capability, imports :List(Import))
      -> (handle :Process);
}

# One of a process's imports. A struct, since capnpc-rust doesn't support
# `List(Capability)`.
struct Import {
  capability @0 :Capability;
}

interface Process {
  # Wait for the process to end, and get back the memory it was built
  # from. Once answered, the handle stops working. One waiter at a time:
  # a second `wait` while one is pending is `badRequest`.
  # Capabilities out: `memory`.
  wait @0 () -> (end :End, memory :Capability);

  # Destroy the process. A pending `wait` hears `killed`. Killing a process
  # that has ended does nothing.
  kill @1 () -> ();
}

# How a process ended.
struct End {
  union {
    # It called its launcher's exit with this code; 0 is success.
    exited @0 :UInt64;
    # It faulted; the fault's label, as in `sap_abi::fault`.
    faulted @1 :UInt64;
    # Its handle's `kill`.
    killed @2 :Void;
  }
}

# The label of a reply. The ordinals are the labels.
enum Status {
  ok @0;
  noSuchProgram @1;
  badRequest @2;
  loadFailed @3;
  full @4;
}
