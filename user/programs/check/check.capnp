@0xaa31abaf4b600755;
# `check` as a component (`interfaces/system/component.capnp`): the program
# that checks the runtime and the services. Besides its imports
# it has a memory account (`process_slot::ACCOUNT`), whose chunks are the
# untyped memory the checks use.
# TODO: capability fields once RPC brings capability tables (M6).

using Component = import "/system/component.capnp";

# Its bootstrap message. A capability is the index of one of its imports.
struct CheckBootstrap $Component.program("check")
    $Component.defaultLimits((memory = (account = (reservation = 33554432, limit = 50331648, processBits = 21)),
                              priority = 255, maxPriority = 255, cnodeBits = 12, stackPages = 16)) {
  # The ASID pool, for address spaces of the processes the checks start.
  asidPool @0 :UInt32 $Component.held("asidPool");
  # The Supervisor interface, and the hash of `peer` in the boot pack, for
  # starting it through the interface.
  supervisor @1 :UInt32 $Component.held("supervisor");
  peerHash @2 :Data $Component.programHash("peer");
  # The log, which the checks' output goes to.
  log @3 :UInt32 $Component.interface("system/log.capnp:Log");
  # A service its supervisor restarts when it fails.
  restartable @4 :UInt32 $Component.interface("test/restartable.capnp:Restartable");
  # The hash of `counter`, for composing with it.
  counterHash @5 :Data $Component.programHash("counter");
}
