@0xd5941b354016a816;
# `SAP_BOOT=compose`: composing at runtime. The boot request runs
# `composer`, which asks its supervisor for a composition of its own and
# the resolver to fill it from the request in its settings: `counter`,
# which imports a `Restartable` service, so the resolver adds a fresh
# `peer` to serve it. counter's output goes to composer's log, bound by
# hand: the inner composition may use what its caller imports.

using Resolver = import "/system/resolver.capnp";
using Composer = import "/programs/composer/composer.capnp";

const composer :Composer.ComposerBootstrap = (
  request = (
    components = [
      (name = "counter", program = "counter", onEnd = endComposition,
       routes = [(field = "log", component = "log", export = "log")]),
    ],
  ),
);

const request :Resolver.Request = (
  components = [
    (name = "composer", program = "composer", settingsConstant = "composer", onEnd = endComposition,
     routes = [(field = "resolver", component = "resolver", export = "resolver")]),
  ],
  grants = [
    (component = "composer", held = "supervisor"),
    (component = "composer", held = "endComposition"),
  ],
);
