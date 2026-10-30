@0x9a7949b34c8cbe16;
# What sap knows about a program besides its image: its declarations, as
# annotations on the struct of its bootstrap message, and the manifest the
# build makes of them for the boot pack, which the resolver reads at
# runtime to decide what serves what (design/processes.md in sap-docs).
# Also the request that names what to run.
#
# A program's bootstrap struct is its imports and its settings. A `UInt32`
# field annotated as an import is the index of one of the process's
# imports; any other field is a setting, which the request gives.
# TODO: typed capability fields say what an import is (M6); the
# annotations then stay only for what a field type can't say.

using Supervisor = import "supervisor.capnp";

# This struct is the bootstrap message of the program built from the
# crate with this name.
annotation program(struct) :Text;
# The exports the program serves.
annotation exports(struct) :List(Export);
# The limits it runs with unless the request says otherwise.
annotation limits(struct) :Supervisor.Limits;

# On a `UInt32` field, an import: a capability to the export of another
# component with this interface ("file.capnp:Name");
annotation interface(field) :Text;
# the program's own export with this name, to serve;
annotation serves(field) :Text;
# something its supervisor holds (a name or a group, or "supervisor" for
# the instance API), given only if the request grants it.
annotation held(field) :Text;

# On a `Data` field, a setting: the hash of the program with this name,
# which the resolver fills in from the programs it knows.
annotation programHash(field) :Text;

struct Export {
  name @0 :Text;
  interface @1 :Text;
}

# What the build makes of a program's declarations.
struct Manifest {
  program @0 :Text;
  # The size of its bootstrap struct, for building one: data words and
  # pointers.
  dataWords @1 :UInt16;
  pointers @2 :UInt16;
  imports @3 :List(Import);
  exports @4 :List(ManifestExport);
  limits @5 :Supervisor.Limits;
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

struct ManifestExport {
  name @0 :Text;
  # The interface's type id.
  interface @1 :UInt64;
}

# What to run: the components named here, and whatever serves what they
# import, which the resolver finds.
struct Request {
  components @0 :List(Wanted);
  # Authority of the supervisor's that a component may have: a held route
  # it declares, only if granted here.
  grants @1 :List(Grant);
}

struct Wanted {
  # The instance's name, and its program's, a crate name in the boot pack.
  name @0 :Text;
  program @1 :Text;
  # Its settings: a single-segment bootstrap message of its program's
  # struct, without imports (the resolver writes those); empty for none.
  # A request file names a constant of the struct's type in the same file
  # instead (`settingsConstant`), which the build evaluates into this.
  settings @2 :Data;
  onEnd @3 :Supervisor.OnEnd;
  settingsConstant @4 :Text;
}

struct Grant {
  component @0 :Text;
  held @1 :Text;
}

# A program the resolver may run: its boot pack hash and manifest.
struct Available {
  hash @0 :Data;
  manifest @1 :Manifest;
}

# The bootstrap message of `resolver`: every program in the boot pack, and
# the request.
struct ResolverImports {
  programs @0 :List(Available);
  request @1 :Request;
  # Its capability to add components to its supervisor.
  compositions @2 :UInt32;
}
