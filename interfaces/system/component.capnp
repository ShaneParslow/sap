@0x9a7949b34c8cbe16;
# How a program defines itself as a component: annotations on the struct of
# its bootstrap message, in the program's own schema
# (`user/programs/<name>/<name>.capnp`), and the manifest the build makes
# of them for the boot pack, which the resolver reads at runtime to decide
# what serves what (design/processes.md in sap-docs).
#
# A program's bootstrap struct holds its imports and its settings. A
# `UInt32` field annotated as an import is the index of one of the
# process's imports; any other field is a setting, which the request
# gives.
# TODO: typed capability fields say what an import is (M6); the
# annotations then stay only for what a field type can't say, and the
# resolver reads compiled schemas instead of manifests.

# This struct is the bootstrap message of the program built from the
# crate with this name.
annotation program(struct) :Text;
# The exports the program serves.
annotation exports(struct) :List(DeclaredExport);
# The limits it runs with unless the request says otherwise.
annotation defaultLimits(struct) :Limits;

# On a `UInt32` field, an import: a capability to the export of another
# component with this interface ("system/log.capnp:Log");
annotation interface(field) :Text;
# the program's own export with this name, to serve;
annotation serves(field) :Text;
# something its supervisor holds (a name or a group, or "supervisor" for
# the Supervisor interface), given only if the request grants it.
annotation held(field) :Text;

# On a `Data` field, a setting: the hash of the program with this name,
# which the resolver fills in from the programs it knows.
annotation programHash(field) :Text;

# An endpoint a component serves.
struct Export {
  # Unique within the component.
  name @0 :Text;
  # The interface it serves, by type id (the stand-in for its hash); 0
  # for unchecked.
  interface @1 :UInt64;
}

# The limits a component's instances run with, which its supervisor
# enforces.
struct Limits {
  memory :union {
    # Every kernel object of the instance is made from untyped memory of
    # 2^bits bytes the supervisor carves from its own pool, 2^spareBits of
    # them given to it as its own (`process_slot::UNTYPED`); ending it is
    # one revoke.
    # Only for the memory manager, which can't be its own client.
    untyped @0 :Untyped;
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

struct Untyped {
  bits @0 :UInt8;
  spareBits @1 :UInt8;
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

# An export, as a program declares it: its interface by name.
struct DeclaredExport {
  name @0 :Text;
  interface @1 :Text;
}

# What the build makes of a program's declarations.
struct Manifest {
  # The program, by crate name.
  name @0 :Text;
  # The size of its bootstrap struct, for building one: data words and
  # pointers.
  dataWords @1 :UInt16;
  pointers @2 :UInt16;
  imports @3 :List(Import);
  # Its exports, each interface by type id.
  exports @4 :List(Export);
  limits @5 :Limits;
  # `Data` fields to fill with a program's hash.
  programHashes @6 :List(HashField);
}

struct HashField {
  # The field's pointer index in the struct.
  pointer @0 :UInt16;
  program @1 :Text;
}

struct Import {
  # The field, by name, and where its `UInt32` is: the offset in the
  # struct's data section, in `UInt32`s.
  field @0 :Text;
  offset @1 :UInt32;
  union {
    # An interface, by type id (the stand-in for its hash).
    interface @2 :UInt64;
    serves @3 :Text;
    held @4 :Text;
  }
}
