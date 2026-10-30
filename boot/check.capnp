@0xbf27122c15ed85a9;
# The default boot request (`SAP_BOOT=check`): the kernel and service
# checks, and a service for them to restart. The resolver adds what they
# import (the log); the memory manager and the resolver are init's own.

using Sap = import "/sap.capnp";
using Peer = import "/peer.capnp";

const restartable :Peer.PeerImports = (mode = restartable);

const request :Sap.Request = (
  components = [
    (name = "check", program = "check", onEnd = endBoot),
    (name = "restartable", program = "peer", settingsConstant = "restartable", onEnd = restart),
  ],
  grants = [
    (component = "check", held = "asidPool"),
    (component = "check", held = "supervisor"),
    (component = "check", held = "endBoot"),
  ],
);
