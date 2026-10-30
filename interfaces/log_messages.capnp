@0xfd60c8c8d4d923be;
# The messages of the log (`log.capnp`) and the log service's bootstrap,
# as plain structs, which programs without a heap can use.

struct Output {
  text @0 :Text;
}

# The bootstrap message of `log`, the log service. A capability is the
# index of one of its imports.
struct LogImports {
  # The receive side of the service's export.
  endpoint @0 :UInt32;
}
