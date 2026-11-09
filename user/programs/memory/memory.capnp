@0xa18b2803561be703;
# The memory manager as a component (`interfaces/system/component.capnp`):
# it serves memory accounts (`interfaces/system/memory.capnp`) from the
# machine's RAM.

using Component = import "/system/component.capnp";

# Its bootstrap message. A capability is the index of one of its imports.
struct MemoryBootstrap $Component.program("memory")
    $Component.exports([(name = "account", interface = "system/memory.capnp:MemoryAccount")])
    $Component.defaultLimits((memory = (untyped = (bits = 22, spareBits = 20)), priority = 255, cnodeBits = 12, stackPages = 4)) {
  # The receive side of its export, with all rights: it mints the
  # accounts' capabilities from it.
  endpoint @0 :UInt32 $Component.serves("account");
  # The first of the untyped regions of RAM it manages: the `ram` group,
  # which runs to the end of its imports. Each one's size and address come
  # from identifying it.
  ram @1 :UInt32 $Component.held("ram");
}
