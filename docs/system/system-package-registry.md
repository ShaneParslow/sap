---
status: exploring
summary: A community registry of interfaces and the programs that implement them, served over HTTPS and native RPC and fetched into the store by hash; declared imports make every package's authority visible before install, so an agent can choose implementations for the user's goals; declarative system configuration; and a name directory linking friendly names to keys.
updated: 2026-10-11
sources: ["[[2026-10-08-22c3947b#m22|§m22]]"]
---

# Package registry and name directory

The human proposed an online community repository of interfaces and programs that implement them, working the way PyPI and crates.io do for Python and Rust, with an agent working out what a particular system needs to meet the user's goals ([[2026-10-08-22c3947b#m22|§m22]]). They judged it to sit a couple of layers above the store, changing little underneath. This page records the idea and the few things it needs from the layers below.

## The registry

The registry publishes three kinds of object, all ordinary store values identified by hash:
- **Interfaces:** Cap'n Proto schemas with their intent text and conformance suites ([[iface-spec-contents]]).
- **Implementations:** code objects, whose declared imports and exports say what authority they need and which interfaces they serve ([[prog-loading#Code objects and formats]]).
- **Claims about hashes:** signatures, conformance-test results, reproducible-build records linking a binary's hash to its source's hash, reviews. Claims are values that refer to other values, so anyone can publish them without the registry vouching for them ([[vision-attestation-and-verification]]).

**Access.** Because everything is content-addressed, the registry can be static files over HTTPS, trivially mirrored and cached, as Nix binary caches and Go's module proxy are. A native sap node serves the same objects over capability RPC ([[ipc-network-rpc]]). Fetched objects land in the user's store and are checked by hash, so a mirror can withhold objects but can't tamper with them. A transparency log, as in Go's checksum database or Sigstore, makes it detectable when a name is pointed at different hashes for different people.

## Why capabilities change package security

On today's platforms an installed package runs with all of its user's authority, which is why a compromised dependency is so damaging. In sap a program's authority is exactly what its declared imports name and its launcher grants ([[cap-process-authority]]). An image-resizing library whose new version suddenly imports a network capability stands out before it is installed, and can't use the network if the request is refused. Malicious packages still exist, but their damage is bounded by what was granted.

## The agent as installer

The human's picture: the user states a goal; the agent works out which interfaces are needed, picks implementations by policy (who signed them, which conformance results they have, how much authority they ask for), and installs them. Installation is a reviewed change on a branch of refs, exactly the system-agent case of [[store-use-cases#A system agent]]: nothing global is modified until the user, or a policy the user wrote, accepts the diff.

**Declarative configuration** is the natural next step: the system's configuration is itself a value (which interfaces are wanted, which implementations, which grants), and the installed state is derived from it, as in NixOS. The agent then edits the declaration rather than the system. The human flagged this as a later question.

## Name directory

Keys name nodes and people ([[ipc-network-rpc#Decided properties]]), and people want friendly names. The human proposed a central directory, next to the registry, linking friendly names to keys, like PGP key servers. The lessons from PGP key servers:
- the old SKS network accepted anything and could not delete, which led to certificate-flooding attacks in 2019;
- its successor, keys.openpgp.org, publishes a name only after verifying the email address.

Better current models are verification by something the name's owner controls (AT Protocol checks a DNS record or a file on the owner's web domain; Keybase used posts on the owner's accounts), and transparency logs for key directories (CONIKS, Key Transparency).

Proposed: a directory is one **introducer** among several. Its answer is a suggestion that the user accepts into their own petnames, never an authority the system trusts silently. Several directories, a contact's own recommendation and an in-person QR code can all serve as introducers. That keeps keys as the only identity and names as a layer on top, so nothing below changes.

## What this needs from lower layers

Nothing new in the store. It relies on things already decided or proposed: code objects declare imports and exports; interfaces and code are identified by algorithm-tagged hashes; claims are values that reference hashes; installs are multi-ref updates on a branch ([[store-use-cases#Requirements collected]]).

> [!question] Open
> Who runs the first registry and directory; the policy language for choosing implementations; how conformance results are trusted (re-run locally, or signed by whom); and the declarative configuration format.
