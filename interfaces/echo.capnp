@0x981d9a02a02b111e;
# The echo service: the test interface for Cap'n Proto messages between
# processes (roadmap M5). The peer answers each request with a reply that
# summarises it, so both sides read and write every kind of field.

struct Request {
  id @0 :UInt64;
  text @1 :Text;
  numbers @2 :List(UInt32);
  blob @3 :Data;
}

struct Reply {
  id @0 :UInt64;
  # The request's text, reversed.
  text @1 :Text;
  # The sum of the request's numbers.
  sum @2 :UInt64;
  # The length of the request's blob, and the sum of its bytes.
  blobLength @3 :UInt32;
  blobSum @4 :UInt64;
}
