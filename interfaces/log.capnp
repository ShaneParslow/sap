@0xe5bb3275ca136870;
# The log: where programs' text goes (design/userspace.md in sap-docs).
# The service writes each line to the console after the name of the
# component that wrote it. The name comes from the route the caller's
# capability was made for (`supervisor.capnp`), so a program can't write
# as another.
#
# Calls follow the convention in `README.md`.
# TODO: levels, a ring of recent lines, readers, keeping it in the store
# (design/userspace.md).

interface Log {
  # Write `text`, where a line may span several calls. Answered once
  # written.
  write @0 (text :Text) -> ();
}

using Sap = import "sap.capnp";

# The bootstrap message of `log`, the log service. A capability is the
# index of one of its imports.
struct LogImports $Sap.program("log") $Sap.exports([(name = "log", interface = "log.capnp:Log")]) $Sap.limits((
    memory = (account = (reservation = 2097152, limit = 2097152, processBits = 20)),
    priority = 255, cnodeBits = 6, stackPages = 4)) {
  # The receive side of the service's export.
  endpoint @0 :UInt32 $Sap.serves("log");
}
