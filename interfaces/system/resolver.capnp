@0xd5e69bfc58943875;
# The resolver: given the programs it may run and a request naming what
# to run, it finds whatever serves what the wanted components import and
# adds the result to a composition (`supervisor.capnp`)
# (design/processes.md in sap-docs).

using Supervisor = import "supervisor.capnp";
using Component = import "component.capnp";

# Calls follow the convention in `README.md`; the reply's label is a
# `Status`.
interface Resolver {
  # Plan what `request` wants and add it all to `compositions`, unstarted:
  # whoever holds `compositions` enacts it. The resolver keeps nothing of
  # the call: its authority is the capability passed, and the request's
  # grants mean nothing to a composition that isn't the supervisor's own.
  # Capabilities in: `compositions`.
  # Status: `badRequest` (malformed, or settings that aren't a canonical
  # message of the program's struct), `noSuchProgram`, `unresolved` (an
  # import nothing, or more than one thing, serves), `refused` (the
  # composition refused a component; nothing after it was added).
  resolve @0 (request :Request, compositions :Supervisor.Compositions) -> ();
}

enum Status {
  ok @0;
  badRequest @1;
  noSuchProgram @2;
  unresolved @3;
  refused @4;
}

# What to run: the components named here, and whatever serves what they
# import, which the resolver finds.
struct Request {
  components @0 :List(Wanted);
  # Authority of the supervisor's that a component may have: a held route
  # it declares, only if granted here. Only the supervisor's own
  # composition reads them, from the boot request.
  grants @1 :List(Grant);
}

struct Wanted {
  # The instance's name, and its program's, a crate name in the boot pack.
  name @0 :Text;
  program @1 :Text;
  # Its settings: a canonical single-segment message of its program's
  # bootstrap struct, without imports (the resolver writes those); empty
  # for none. A request file names a constant of the struct's type in the
  # same file instead (`settingsConstant`), which the build evaluates
  # into this.
  settings @2 :Data;
  onEnd @3 :Supervisor.OnEnd;
  settingsConstant @4 :Text;
  # Imports bound by hand rather than by the resolver: for an export that
  # isn't planned (one the composition's caller has, or a component
  # already running), or to choose among several.
  routes @5 :List(Binding);
}

# Import `field` of a wanted component is export `export` of `component`:
# one of the request's, or else one outside it.
struct Binding {
  field @0 :Text;
  component @1 :Text;
  export @2 :Text;
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
