@0xbf27122c15ed85a9;
# The default boot request (`SAP_BOOT=check`): the runtime and service
# checks, and a service for them to restart. The resolver adds what they
# import (the log); the memory manager and the resolver are init's own.

using Resolver = import "/system/resolver.capnp";
using Peer = import "/programs/peer/peer.capnp";

const restartable :Peer.PeerBootstrap = (mode = (restartable = void));

const request :Resolver.Request = (
  components = [
    (name = "check", program = "check", onEnd = endComposition),
    (name = "restartable", program = "peer", settingsConstant = "restartable", onEnd = restart),
  ],
  grants = [
    (component = "check", held = "asidPool"),
    (component = "check", held = "supervisor"),
    (component = "check", held = "endComposition"),
  ],
);
