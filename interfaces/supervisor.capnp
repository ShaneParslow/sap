@0xd5a1c4e2f03b7a96;
# Compositions: what a supervisor runs (design/processes.md in sap-docs).
# A supervisor starts processes and enforces limits on them; a composition
# is the desired state it enacts. Where a composition comes from (the boot
# image, a device-tree enumerator, a resolver over declared imports) is a
# separate component's job; the supervisor only checks it against what it
# holds and carries it out.
#
# A capability is named by what the supervisor holds, not by a slot: the
# supervisor keeps a table of named capabilities ("asidPool", "untyped.3")
# and routes from it, or by the export of a component it runs.
#
# Exports belong to the supervisor: it makes each export's endpoint when it
# enacts the composition, before anything starts, gives the serving
# component the receive side and each client a capability badged for that
# client. The endpoint outlives the server's instance.
# TODO: lazy start and restart, which exports are for (M7).

struct Composition {
  components @0 :List(Component);
}

struct Component {
  # Unique within the composition; used in logs and, later, routes.
  name @0 :Text;
  # The BLAKE3 hash of the program's image.
  program @1 :Data;
  # The program's bootstrap message: a single-segment Cap'n Proto message
  # whose root is its imports struct, naming imports by index in `imports`.
  bootstrap @2 :Data;
  # The capabilities the instance gets as its imports, in order.
  imports @3 :List(Route);
  limits @4 :Limits;
  # What the supervisor does when the instance ends.
  onEnd @5 :OnEnd;
  # The endpoints the component serves, for routes to name.
  exports @6 :List(Export);
}

struct Export {
  # Unique within the component.
  name @0 :Text;
  # TODO: the interface's hash, for checking routes against it (M6).
}

# Where one import comes from.
struct Route {
  union {
    # A capability in the supervisor's table.
    held @0 :Held;
    # A capability to call another component's export, badged for this
    # component: the server can ask the supervisor which component a
    # badge is (`sap_process::launcher::CLIENT`).
    export @1 :ExportRef;
    # The receive side of one of this component's own exports, by name.
    serve @2 :Text;
  }
}

struct Held {
  name @0 :Text;
  # Moved (the supervisor gives its capability up) or copied with all
  # its rights. Untyped memory is copied, never moved: a copy keeps the
  # supervisor's right to revoke it.
  move @1 :Bool;
}

struct ExportRef {
  component @0 :Text;
  name @1 :Text;
}

# The limits the supervisor enforces on an instance.
struct Limits {
  # Every kernel object of the instance (its address space, CNode,
  # threads, pages) is made from an untyped region of 2^memoryBits bytes
  # the supervisor carves from its own memory; ending the instance is one
  # revoke of it.
  # TODO: a memory account with a reservation and a limit, from the
  # memory manager, instead of a fixed region (design/memory.md).
  memoryBits @0 :UInt8;
  # Its thread's priority and the highest it may give its own threads, at
  # most the supervisor's own.
  # TODO: a scheduling context, once the kernel has them.
  priority @1 :UInt8;
  maxPriority @2 :UInt8;
  # log2 of its CNode's slots, and its stack in pages.
  cnodeBits @3 :UInt8;
  stackPages @4 :UInt16;
  # log2 of the untyped memory it gets for objects of its own (its
  # `process_slot::UNTYPED`), carved from its memory; 0 for none.
  # TODO: the memory account replaces this too (design/memory.md).
  budgetBits @5 :UInt8;
}

enum OnEnd {
  # Destroy the instance and forget it.
  forget @0;
  # End the boot: with success if the instance exited with 0, else with
  # failure. For the root supervisor's test program.
  endBoot @1;
}
