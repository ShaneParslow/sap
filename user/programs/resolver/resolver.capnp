@0xcae031d778fa7e41;
# The resolver as a component (`interfaces/system/component.capnp`): it
# decides at runtime what serves what (`interfaces/system/resolver.capnp`)
# and adds the result to its supervisor's composition.

using Component = import "/system/component.capnp";
using Resolver = import "/system/resolver.capnp";

# Its bootstrap message: every program in the boot pack, and the request.
struct ResolverBootstrap $Component.program("resolver")
    $Component.defaultLimits((memory = (account = (reservation = 4194304, limit = 4194304, processBits = 21)),
                              priority = 255, cnodeBits = 8, stackPages = 16)) {
  programs @0 :List(Resolver.CatalogEntry);
  request @1 :Resolver.Request;
  # Its capability to add components to its supervisor.
  compositions @2 :UInt32 $Component.held("compositions");
}
