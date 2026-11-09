@0xc72a6feca974c5e3;
# `counter` as a component (`interfaces/system/component.capnp`): a demo
# client. It calls a `Restartable` service a few times, writes what it
# answered to the log, and exits.

using Component = import "/system/component.capnp";

# Its bootstrap message. A capability is the index of one of its imports.
struct CounterBootstrap $Component.program("counter")
    $Component.defaultLimits((memory = (account = (reservation = 1048576, limit = 1048576, processBits = 20)),
                              priority = 255, cnodeBits = 6, stackPages = 4)) {
  # The service to call.
  service @0 :UInt32 $Component.interface("test/restartable.capnp:Restartable");
  # Where its output goes.
  log @1 :UInt32 $Component.interface("system/log.capnp:Log");
  # How many calls to make.
  calls @2 :UInt32 = 3;
}
