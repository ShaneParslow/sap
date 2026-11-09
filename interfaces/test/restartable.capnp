@0xd1f7a68d31ce42d6;
# A service to restart, for checking that a supervisor restarts what fails
# behind the same export: `peer` serves it in its `restartable` mode.

interface Restartable {
  # How many calls this process has answered, counting this one.
  count @0 () -> (calls :UInt64);
  # Fault.
  crash @1 () -> ();
}
