@0xc284eaf32ad8ef59;
# The log service as a component (`interfaces/system/component.capnp`): it
# serves the log (`interfaces/system/log.capnp`).

using Component = import "/system/component.capnp";

# Its bootstrap message. A capability is the index of one of its imports.
struct LogBootstrap $Component.program("log")
    $Component.exports([(name = "log", interface = "system/log.capnp:Log")])
    $Component.defaultLimits((memory = (account = (reservation = 2097152, limit = 2097152, processBits = 20)),
                              priority = 255, cnodeBits = 6, stackPages = 4)) {
  # The receive side of the service's export.
  endpoint @0 :UInt32 $Component.serves("log");
}
