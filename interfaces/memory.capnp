@0xa4ad2e7ab3727f4e;
# Memory accounts (design/memory.md in sap-docs): who is charged for
# memory, apart from where it comes from. The memory manager holds the
# machine's RAM and backs every account from one pool; an account is
# charged for the chunks it holds, and its children's.
#
# Each account has a reservation, memory it is guaranteed, and a limit,
# what it may hold at most, counting its children's. Memory beyond the
# reservation comes from what nobody has reserved, while any is free;
# nothing is promised beyond the reservation (no overcommit). A child's
# reservation is carved from its parent's.
#
# An account has two capabilities: the holder's, to allocate from it, and
# the controller's, kept by whoever made it, to empty it, end it or set its
# limit. Destroying what a program built is one `revoke` of its account.
#
# Calls follow the convention in `README.md`; the reply's label is a
# `Status`.
# TODO: mappings (the manager mapping frames into an address space it
# holds: lazy, pageable, swap, copy-on-write), pressure and trim
# (design/memory.md).

interface MemoryAccount {
  # A chunk of 2^sizeBits bytes of untyped memory (2^12 to 2^40), charged
  # to this account until released. `id` names it for `release`; `paddr`
  # is where it is, for DMA. Everything made from the chunk is destroyed
  # with it.
  # Capabilities out: `memory`.
  # Status: `badRequest` (size out of range), `overLimit` (this account or
  # one above it would exceed its limit), `outOfMemory` (no free chunk that
  # large outside others' reservations), `full` (the manager's tables).
  allocate @0 (sizeBits :UInt8) -> (id :UInt64, paddr :UInt64, memory :Capability);

  # Give back chunk `id`: it and everything made from it are destroyed.
  # Status: `noSuchChunk` unless this account holds it.
  release @1 (id :UInt64) -> ();

  # A child account with `reservation` carved from this one's unreserved
  # part and `limit` at most this one's (and at least the reservation).
  # Capabilities out: `holder`, `controller`.
  # Status: `overLimit` (reservation or limit too large), `badRequest`
  # (limit below reservation), `full`.
  child @2 (reservation :UInt64, limit :UInt64) -> (holder :MemoryAccount, controller :AccountControl);

  # What the account holds, counting its children's, and its terms.
  usage @3 () -> (used :UInt64, reservation :UInt64, limit :UInt64);
}

interface AccountControl {
  # Release every chunk of the account and of its children, and end its
  # children. The account stays, empty, with its reservation.
  revoke @0 () -> ();

  # Set the limit, at most the parent's and at least the reservation.
  # Below what the account holds, it can allocate nothing until it
  # releases enough; the account's holder isn't forced to.
  # Status: `badRequest` (below the reservation), `overLimit` (above the
  # parent's).
  setLimit @1 (limit :UInt64) -> ();

  # Revoke, then end the account: its reservation goes back to its parent
  # and its capabilities stop working. The root account can't be ended.
  destroy @2 () -> ();
}

enum Status {
  ok @0;
  badRequest @1;
  overLimit @2;
  outOfMemory @3;
  noSuchChunk @4;
  full @5;
}

# The badges of the root account's capabilities, which the memory
# manager's supervisor mints from the manager's export for itself. The
# root account has all the RAM as its reservation and limit.
const rootHolder :UInt64 = 2;
const rootController :UInt64 = 3;

# The bootstrap message of `memory`, the memory manager. A capability is
# the index of one of its imports.
struct MemoryImports {
  # The receive side of its export, with all rights: it mints the
  # accounts' capabilities from it.
  endpoint @0 :UInt32;
  # The first of the untyped regions of RAM it manages: the `ram` group,
  # which runs to the end of its imports. Each one's size and address come
  # from identifying it.
  ram @1 :UInt32;
}
