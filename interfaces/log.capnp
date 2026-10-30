@0xe5bb3275ca136870;
# The log: where programs' text goes (design/userspace.md in sap-docs).
# The service writes each line to the console after the name of the
# component that wrote it. The name comes from the route the caller's
# capability was made for (`supervisor.capnp`), so a program can't write
# as another.
#
# Calls follow the convention in `process.capnp`. The messages are in
# `log_messages.capnp`, apart from the interface, because capnpc-rust's
# code for an interface needs a heap and programs without one write to the
# log too.
# TODO: levels, a ring of recent lines, readers, keeping it in the store
# (design/userspace.md).

using Messages = import "log_messages.capnp";

interface Log {
  # Write `text`, where a line may span several calls. Answered once
  # written.
  write @0 Messages.Output -> ();
}
