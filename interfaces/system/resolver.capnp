@0xd5e69bfc58943875;
# What the resolver works from: the programs it may run and a request
# naming what to run (design/processes.md in sap-docs). The resolver finds
# whatever serves what the wanted components import and adds the result
# to a composition (`supervisor.capnp`).

using Supervisor = import "supervisor.capnp";
using Component = import "component.capnp";

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
struct CatalogEntry {
  hash @0 :Data;
  manifest @1 :Component.Manifest;
}
