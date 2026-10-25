---
status: exploring
summary: All user state, down to window positions and the focused text box, as declared, typed values in the store, so a full session comes back after a shutdown without a memory image; the tension between that reach and least authority, a proposed resolution by ownership of state, and crash-only components that make shutdown an ordinary event.
updated: 2026-10-24
sources: ["[[2026-10-22-e547b584#m2|§m2]]"]
---

# Restorable state

**The human's direction** ([[2026-10-22-e547b584#m2|§m2]]): user state, even a window's position, should be fully visible, declared and self-contained, and every component should expose information and interfaces through which its behavior can be viewed and changed. A user should be able to come back from a full shutdown and get the whole session back from disk, *without* suspending memory to disk. The human proposed that last property as the measure of how fine-grained the system's state has to be. Nothing here is designed yet.

## What the test demands

Restoring from declared state rather than from a memory image means every piece of state a user would notice losing is a typed value in the store ([[store-data-model]]), and a running process holds only what it can rebuild from those values. A process is then a cache of its state, not its owner.

That excludes the two usual answers:
- **Orthogonal persistence** (KeyKOS, EROS, Phantom OS) checkpoints all of memory and restores it transparently. It restores everything, but nothing is declared: the state is opaque bytes, readable only by the program that wrote it, and it carries bugs and leaks across restarts along with everything else.
- **Each application saving what it likes** (macOS state restoration, Android's saved instance state) is declared, but each app chooses the format and the granularity, and no other component can read it.

What sap would do instead is make the second approach universal and typed: the state's schema is an interface like any other ([[iface-capnp]]), so a shell, an agent or a generic inspector can read it and change it with the right capability. "Everything exposes its state" then costs nothing extra, because the state is already outside the process.

## Least authority against reach

The tension the human raised: restoring a session needs something that can see all of it (window positions, which text box has focus, scroll offsets), but least authority ([[vision-principles#2. Least authority through object capabilities]]) says no component should see more than its job needs.

> [!question] Proposed resolution (agent, not discussed)
> Divide state by **who owns it**, and put each owner's state under a ref of its own in the user's store:
> - **The session**, owned by the user's shell or session manager: which programs run, their windows' positions and sizes, the stacking and focus order. The window manager, not the application, holds this; an application never knows where its window is.
> - **Each program's own state**: open documents (as sturdy refs, [[store-sturdy-refs]]), the selected tab, the focused field, the cursor. The program gets a capability to its own state ref and nothing else ([[cap-process-authority#How a process gets objects]]).
>
> Restoring is then the session manager walking the session's ref and restarting each program with a capability to its state ref. The session manager holds the user's root ([[kernel-process-model#Root authority]]) and so reaches everything, but every program it starts sees only its own slice. Reach belongs to one component the user trusts; least authority holds for the rest. This is the same shape as a user owning all of their files while each program is confined to what it was given.

## Shutdown as an ordinary event

If every component keeps its state in the store as it changes, shutting down needs no "save now" broadcast: stopping a component and crashing it lose the same nothing. This is **crash-only software** (Candea and Fox, 2003), and it shapes userspace more than the store does:
- writing state is cheap and frequent, so the store needs small, fast ref updates, and probably a class of state that may lose its last second (scroll position, cursor) and can be written lazily;
- restart is the normal path, which is what on-demand start already assumes: idle servers exit and come back through their state ([[kernel-process-model#Starting on demand]]);
- a broad event such as shutdown, suspend or moving a session to another device becomes "stop everything; later, restore from the session ref", with no special protocol per program.

## Open questions

> [!question] Open
> - Which live state can't be restored by value: open network connections, a capability to a remote object, a running computation. Sturdy refs cover persistent objects; the rest must be declared as re-acquirable or lost.
> - How state schemas evolve across upgrades of the program that owns them ([[iface-versioning]]).
> - Write cost for high-frequency state, and whether "may lose the last second" is a schema annotation, like the proposed `$local` ([[iface-capnp#Further annotations (proposed)]]).
> - Privacy: a declared session is a complete record of what the user was doing, so who can read the session ref matters as much as who can write it.
> - Whether this belongs in [[store-use-cases]] as a walkthrough ("resume a desktop session after a power loss") before any design.
