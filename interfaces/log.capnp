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

# The bootstrap message of `log`, the log service. A capability is the
# index of one of its imports.
struct LogImports {
  # The receive side of the service's export.
  endpoint @0 :UInt32;
}
