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
#
# Components also start at runtime, through the instance API
# (`Supervisor.start`), for a caller that pays for them; such an instance
# is named after its caller ("check/helper") and ends with it.
# TODO: lazy start (M7).

using Memory = import "memory.capnp";

struct Composition {
  components @0 :List(Component);
  # The export of the memory manager this composition runs, for a root
  # supervisor, which then holds the root account; instances' accounts are
  # its children.
  # TODO: a held account instead, for a supervisor that isn't the root.
  accounts @1 :ExportRef;
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
  # The interface it serves, by type id (the stand-in for its hash); 0
  # for unchecked.
  interface @1 :UInt64;
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
    # One of this component's own exports, by name: a capability with all
    # rights, to receive on and to mint capabilities to it from, with
    # badges below `sap_process::launcher::ROUTE_BADGES`.
    serve @2 :Text;
    # For a component started through `Supervisor.start`: the capability
    # passed in that call with this index.
    passed @3 :UInt32;
    # This supervisor's instance API, as this component.
    supervisor @4 :Void;
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
  # The interface the importer expects; 0 for unchecked. The supervisor
  # refuses a route whose export serves another.
  interface @2 :UInt64;
}

# The limits the supervisor enforces on an instance.
struct Limits {
  memory :union {
    # Every kernel object of the instance is made from an untyped region
    # of 2^bits bytes the supervisor carves from its own pool, 2^budgetBits
    # of them its own (`process_slot::UNTYPED`); ending it is one revoke.
    # Only for the memory manager, which can't be its own client.
    region @0 :Region;
    # A memory account (`memory.capnp`), the instance's
    # `process_slot::ACCOUNT`; the instance is built from a chunk of it, so
    # ending it is one revoke of the account.
    account @1 :Account;
  }
  # Its thread's priority and the highest it may give its own threads,
  # both held to the supervisor's own by the kernel.
  # TODO: a scheduling context, once the kernel has them.
  priority @2 :UInt8;
  maxPriority @3 :UInt8;
  # log2 of its CNode's slots, and its stack in pages.
  cnodeBits @4 :UInt8;
  stackPages @5 :UInt16;
}

struct Region {
  bits @0 :UInt8;
  budgetBits @1 :UInt8;
}

struct Account {
  # In bytes: the reservation and limit of the instance's account, a
  # child of the supervisor's.
  reservation @0 :UInt64;
  limit @1 :UInt64;
  # log2 of the chunk the instance is built from: its image, CNode,
  # stack, page tables.
  processBits @2 :UInt8;
}

enum OnEnd {
  # Destroy the instance and forget it.
  forget @0;
  # End the boot: with success if the instance exited with 0, else with
  # failure. For the root supervisor's test program.
  endBoot @1;
  # If it failed (faulted, or exited with a code other than 0), start it
  # again in the same memory, emptied, serving the same exports; if it
  # exited with 0, forget it. Not after failing more than a few times in a
  # row within a second of starting. Its routes can't move capabilities.
  restart @2;
}

# The instance API: starting components at runtime. Calls follow the
# convention in `README.md`; the reply's label is a `Status`.
# TODO: capability fields once RPC brings capability tables (M6); a
# component too large for one message (by page transfer).
interface Supervisor {
  # Start `component`, as its caller's: named "caller/name" (unique while
  # it runs), its memory a child account of `account` (the caller pays,
  # for its exports' endpoints too), ended when its caller ends. Its
  # routes may be `passed` capabilities, exports the caller imports itself
  # (re-badged for it), its own exports, and `supervisor`; never what the
  # supervisor holds. Its memory must be an account, its end `forget` or
  # `restart`.
  # Capabilities in: `account`, then the passed capabilities, in order.
  # Out on `ok`: `instance`, then a capability to call each of its exports,
  # in the component's order.
  # Status: `badRequest` (a malformed request or a route it may not have),
  # `noSuchProgram`, `overLimit` (the account's terms), `full`,
  # `loadFailed`.
  start @0 (component :Component, account :Memory.MemoryAccount, passed :List(Passed))
      -> (instance :Instance, exports :List(Passed));
}

# One capability of a list (capnpc-rust has no `List(Capability)`).
struct Passed {
  capability @0 :Capability;
}

interface Instance {
  # Wait for the instance to end for good (a restart isn't an end). Once
  # answered, the handle stops working. One waiter at a time: a second
  # `wait` while one is pending is `badRequest`.
  wait @0 () -> (end :End);

  # Stop it, for good. Stopping one that has ended does nothing.
  stop @1 () -> ();
}

struct End {
  union {
    # It exited with this code.
    exited @0 :UInt64;
    # It faulted with this label (`sap_abi::fault`), and wasn't restarted.
    faulted @1 :UInt64;
    # `Instance.stop`, or its caller ended.
    stopped @2 :Void;
  }
}

# The label of a reply to the instance API.
enum Status {
  ok @0;
  badRequest @1;
  noSuchProgram @2;
  overLimit @3;
  full @4;
  loadFailed @5;
}

# Adding components to a supervisor's own composition at runtime: the
# resolver's capability. Components are added one call each (a call
# carries at most 120 words), each one's exports made as it is added, and
# started together by `enact`, so a client added before its server still
# reaches it. They are the supervisor's own: paid from its account, named
# as given. A route to something the supervisor holds (or `supervisor`)
# needs a grant for that component, which the supervisor's own trusted
# configuration gives, never the caller. Calls follow `README.md`; the
# reply's label is a `Status`.
# TODO: reconcile: stop and replace what a new composition drops or
# changes (design/processes.md).
interface Compositions {
  # Status: `badRequest` (malformed, a name in use, an export declared
  # twice, a route not granted).
  add @0 (component :Component) -> ();
  # Start everything added since the last `enact`, in the order added.
  # Status: the first failure's; what started before it keeps running.
  enact @1 () -> ();
}
