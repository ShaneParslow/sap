@0xca2805ab6464725e;
# `composer` as a component (`interfaces/system/component.capnp`): a demo
# of composing at runtime. It makes a composition of its own through its
# supervisor, has the resolver fill it from the request in its settings,
# starts it, waits for it to end, and exits as it ended.

using Component = import "/system/component.capnp";
using Resolver = import "/system/resolver.capnp";

# Its bootstrap message. A capability is the index of one of its imports.
# Its reservation covers what it composes: its members' reservations are
# carved from it.
struct ComposerBootstrap $Component.program("composer")
    $Component.defaultLimits((memory = (account = (reservation = 6291456, limit = 8388608, processBits = 20)),
                              priority = 255, maxPriority = 255, cnodeBits = 6, stackPages = 4)) {
  # The Supervisor interface, to compose with.
  supervisor @0 :UInt32 $Component.held("supervisor");
  # The resolver, to fill the composition.
  resolver @1 :UInt32 $Component.interface("system/resolver.capnp:Resolver");
  # Where its output goes.
  log @2 :UInt32 $Component.interface("system/log.capnp:Log");
  # What to run.
  request @3 :Resolver.Request;
}
