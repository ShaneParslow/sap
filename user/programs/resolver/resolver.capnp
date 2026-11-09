@0xcae031d778fa7e41;
# The resolver as a component (`interfaces/system/component.capnp`): it
# decides at runtime what serves what (`interfaces/system/resolver.capnp`).
# At boot it adds what the boot request wants to its supervisor's own
# composition; then it serves `Resolver` for anyone's composition.

using Component = import "/system/component.capnp";
using Resolver = import "/system/resolver.capnp";

# Its bootstrap message: every program in the boot pack, and the request.
struct ResolverBootstrap $Component.program("resolver")
    $Component.exports([(name = "resolver", interface = "system/resolver.capnp:Resolver")])
    $Component.defaultLimits((memory = (account = (reservation = 4194304, limit = 4194304, processBits = 21)),
                              priority = 255, cnodeBits = 8, stackPages = 16)) {
  programs @0 :List(Resolver.CatalogEntry);
  request @1 :Resolver.Request;
  # Its capability to add components to its supervisor's own composition.
  compositions @2 :UInt32 $Component.held("compositions");
  # The receive side of its export.
  endpoint @3 :UInt32 $Component.serves("resolver");
}
