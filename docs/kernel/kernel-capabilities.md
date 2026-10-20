---
status: implemented
summary: Capabilities as implemented today, following seL4 - the capability representation, CNodes of 64-byte slots, the derivation tree kept as a linked list through the slots, resolving capability addresses through guarded CNodes, delete and revoke with finalisation, the CALL invocation path, CNode operations, retyping untyped memory, building address spaces from page-table and frame capabilities, and the capabilities the root task starts with.
updated: 2026-10-19
code: [kernel/cap.rs, kernel/cspace.rs, kernel/invocation.rs, kernel/untyped.rs, kernel/thread.rs, kernel/root_task.rs, kernel/arch/riscv/caps.rs, kernel/arch/riscv/vspace.rs, abi/lib.rs, user/sys/sys.rs, user/sys/cnode.rs, user/sys/untyped.rs, user/sys/vspace.rs, user/init/main.rs, kernel/arch/riscv/paging.rs]
verified: b3443ec
sources: ["[[2026-10-08-22c3947b-wiki-setup]]"]
---

# Capabilities

The second part of roadmap milestone M3 ([[roadmap]]): every thread has a capability space, the kernel resolves capability addresses and invokes kernel objects through them, and the root task starts with capabilities to everything. The design is seL4's, as decided ([[kernel-choice]]); where sap differs, this page says so. The model behind it is [[cap-model]] and [[kernel-objects-and-syscalls]].

## Representation

A capability is a `Cap` (`kernel/cap.rs:22`): a tag and the per-capability state seL4 keeps in its capability bits.

| Variant | Names | State in the capability |
|---|---|---|
| `Null` | nothing | |
| `Untyped` | a naturally aligned power-of-two block of physical memory | base, size, how much has been retyped (`free_index`), whether it is device memory |
| `CNode` | a table of `1 << radix` slots | radix, guard size and guard value |
| `Thread` | a TCB ([[kernel-threads]]) | |
| `Notification` | a notification ([[kernel-ipc]]) | rights (signal, wait) and a badge |
| `Endpoint` | an endpoint ([[kernel-ipc#Endpoints]]) | rights (send, receive, grant) and a badge |
| `Reply` | a reply object ([[kernel-ipc#Reply objects]]) | |
| `Arch(PageTable)` | a RISC-V page table, any level (`kernel/arch/riscv/caps.rs:20`) | unmapped, the root of an address space with an ASID, or mapped at a level and address |
| `Arch(Frame)` | a 4 KiB, 2 MiB or 1 GiB frame | rights, and the one mapping this capability made: ASID and address, ASID 0 meaning unmapped |
| `Arch(AsidPool)` | the kernel's pool of address-space identifiers | |

The enum is `repr(u8)` with small fields first, so a `Cap` is 40 bytes and an all-zero `Cap` is `Null`: zeroed memory is a CNode of empty slots. Endpoints and reply objects are 16 bytes each.

## Slots and CNodes

A `Slot` (`kernel/cspace.rs:28`) holds a capability and its links in the derivation tree, in 64 bytes (`SLOT_BITS = 6`, line 25; seL4 uses 32 with packed bitfields). A CNode of radix *r* occupies `1 << (r + 6)` bytes. A TCB holds four slots of its own: its capability-space root, its address-space root, its IPC buffer frame and its fault-handler endpoint (`kernel/thread.rs:24-28`), copies whose deletion takes the thread's reference away, as in seL4.

## The derivation tree

Every slot is on one doubly linked list, seL4's "mapping database", ordered so that a capability's descendants follow it directly. Nothing is allocated; the links live in the slots.
- **Inserting.** An original (a boot capability, or a newly retyped object) starts a tree, or goes right after the untyped memory it came from (`insert_original`, line 145). A copy goes right after its source (`insert_derived`, line 165).
- **Parenthood is computed, not stored.** `b` is a child of `a` if `a` is *revocable* and `b`'s object lies within `a`'s (`is_parent_of`, line 242), using `Cap::same_region_as` in `cap.rs`, with an extra rule for badges ([[kernel-ipc#Badges]]). Originals are revocable, and so are copies of untyped memory and copies given a new badge (`Cap::is_revocable_copy`), as in seL4. So a copy of a TCB capability is a sibling, while a copy of untyped memory is a child and owns what is later retyped through it.
- **Moving** keeps the capability's place in the list (`move_cap`, line 181).

## Resolving addresses

A capability address is resolved like a page-table walk (`resolve`, `kernel/cspace.rs:88`): each CNode consumes its guard bits, which must match its guard, then `radix` bits that index a slot; if bits remain and the slot holds another CNode, the walk continues there. An invocation names its target with a full-width address (`lookup_cap`, line 131); CNode operations name a slot by (index, depth) relative to a CNode and require the depth to be used exactly (`lookup_target`, line 123), so they can name a CNode slot itself or an empty slot. The root task's CNode has radix 12 and a 52-bit zero guard, so slot *n* is simply address *n*.

## Delete, finalise, revoke

`delete` (`kernel/cspace.rs:286`) removes a capability. If it was the **last** capability to its object, meaning no neighbour on the list names the same object, since copies are adjacent (`is_final`, line 258), the object is finalised (`finalise`, line 307):
- a CNode's contents are deleted, recursively;
- a thread is taken off the CPU and its own slots are deleted (`thread::finalise`);
- a page table that is an address-space root frees its ASID; a mapped one is unmapped.

Deleting a frame capability that maps its frame unmaps it whether or not it was the last, since each frame capability is its own mapping (`kernel/arch/riscv/vspace.rs:167`). The object's memory isn't reclaimed by deletion; that happens when the untyped memory it came from is revoked.

**Nesting and cycles.** The CNode graph is built by userspace, and the kernel can't trust its shape. Anyone holding a CNode capability with the right to copy can put a capability to a CNode inside itself, directly or through others, so cycles and arbitrarily long chains are possible ([[2026-10-08-22c3947b#m30|§m30]]). They are harmless for lookups, which consume at least one address bit per level and so stop after at most 64. Deletion is where they matter, because deleting the last capability to a CNode deletes everything in it:
- **Cycles.** The slot is emptied *before* the object is finalised, so a CNode that contains (directly or not) the slot being deleted can't reach it again: a cycle ends instead of looping.
- **Depth.** A chain in which each CNode holds the only capability to the next makes deletion recurse once per link, on a fixed-size kernel stack. sap refuses to go deeper than 32 (`DeleteError::TooDeep`, `kernel/cspace.rs:58`, marked `TODO`). The capabilities below that depth stay in place and on the derivation tree, so repeating the revoke continues where it stopped; nothing is lost, but one call may not finish.
- **Time.** One CNode can have 2^26 slots, and deleting it visits each slot in a single system call, with interrupts off. A process can make the kernel unresponsive for a long time this way.

seL4 solves all three with "zombie" capabilities: a capability being finalised becomes a zombie in its slot, deletion proceeds iteratively rather than recursively, and long deletions stop at preemption points and resume on the next call. sap should adopt that before untrusted programs run (`TODO` in `cspace.rs`).

`revoke` (line 352) deletes the children that follow a capability until the next capability isn't one.

## Invocation

`CALL` (`kernel/invocation.rs:81`) takes the invoked capability's address in `a0`, a message info word in `a1` (label, number of extra capabilities, length, laid out as in seL4, `abi/lib.rs:260`), and message words 0–3 in `a2`–`a5`. Words 4 and up, and the addresses of up to 16 extra capabilities, are in the thread's IPC buffer (`abi/lib.rs:290`), a 2 KiB block the kernel reaches through the frame capability in the TCB. The reply's label is an error code (`abi/lib.rs:73`, numbered as seL4's) and its result words come back in the same registers.

The capability's type picks the object, and the label picks the operation (`abi/lib.rs:107`). Threads have their own operations, on [[kernel-threads#Thread invocations]]. CNodes support (`kernel/invocation.rs:135`):

| Operation | Effect |
|---|---|
| `REVOKE` | delete everything derived from the capability in a slot |
| `DELETE` | delete the capability in a slot |
| `COPY` | derive a copy into an empty slot, with rights reduced to a mask |
| `MINT` | `COPY` plus a new guard for a CNode capability, or a badge for an unbadged notification or endpoint capability |
| `MOVE` | move a capability to an empty slot |

Derivation follows seL4's `deriveCap` (`derive`, line 339): untyped memory can only be copied while nothing has been retyped from it (`REVOKE_FIRST` otherwise), and a copied frame capability starts unmapped. A destination that isn't empty gives `DELETE_FIRST`; an address that doesn't resolve gives `FAILED_LOOKUP`. Kernel object methods are plain constants in `sap-abi` for now; a `TODO` says to generate them from a Cap'n Proto schema (roadmap D5, [[cap-process-authority#Runtime capabilities and declared imports]]).

On the user side, `sap-sys` (`user/sys/`, [[prog-libsap#Crates]]) wraps `CALL` (`sys::call`, `user/sys/sys.rs:44`), the CNode operations (`user/sys/cnode.rs`) and retype (`user/sys/untyped.rs`).

## Retyping untyped memory

Retyping is the only way kernel objects come into existence after boot, so every object's memory is charged to whoever held the untyped capability ([[kernel-objects-and-syscalls#Kernel memory]]). `UNTYPED_RETYPE` (`kernel/invocation.rs:109`, `kernel/untyped.rs:64`) takes an object type (`abi/lib.rs:237`), a size where the type has one, a destination CNode named by (index, depth) from a root CNode passed as an extra capability (depth 0 names the root itself, as in seL4), and a range of empty slots in it, at most 256.

| Type | Size of one object |
|---|---|
| Untyped | `1 << size_bits`, 16 bytes to 256 GiB |
| TCB | 1 KiB |
| CNode | `1 << size_bits` slots of 64 bytes, up to 2^26 slots (`MAX_CNODE_RADIX`, line 23) |
| Page table | 4 KiB |
| Frame | 4 KiB, 2 MiB or 1 GiB |

Notifications are 32 bytes; endpoints and reply objects, 16. Device untyped memory can only become frames or smaller untyped regions.

**How the kernel takes memory, and why reuse is safe.** The kernel never takes memory from a process. A process retypes untyped memory it holds into, say, a TCB; the TCB then lives in that memory, and the process can no longer touch those bytes as memory, because the only capability it has to them is the TCB capability ([[2026-10-08-22c3947b#m30|§m30]]). Policy (who gets which memory, when to take it back) is entirely userspace's: the root task holds all memory as untyped at boot, hands regions to the processes it starts, and takes them back by revoking. The kernel only enforces that memory is never in two uses at once:
- an object is placed only above its region's watermark, in memory nothing else uses;
- every capability to an object made from a region is in the region's derivation subtree, including copies, copies in threads' own slots, and mappings recorded in frame capabilities;
- so revoking the region finds and deletes all of them (stopping threads, unmapping frames, freeing ASIDs) before the region can be reused, and reuse is only allowed once it has no children at all.

A thread of another process using an object that is revoked simply sees a broken capability or is stopped; it can't observe the memory being reused. Concurrency inside the kernel isn't an issue, because system calls run to completion with interrupts off (and, with more harts, will run under a big lock, as in seL4). seL4 additionally lets long revocations stop at preemption points and resume, which sap doesn't yet ([[#Delete, finalise, revoke]]).

Untyped memory is handed out with a **watermark**: each object goes at the next address aligned to its size, and the capability's `free_index` advances past it. Memory below the watermark is never handed out again while anything retyped from the region still exists. Once everything has been revoked, so the capability has no children (`kernel/untyped.rs:95`), the next retype starts from zero. That is the only way memory is reclaimed, and why revoking untyped memory is how a parent takes back everything a child built from it. New objects are zeroed (except in device memory), and their capabilities are inserted as originals right after the untyped capability, so they are its children (`create`, `kernel/untyped.rs:124`). Every check happens before anything changes, so a failed retype leaves no partial result: `NOT_ENOUGH_MEMORY` past the end of the region, `DELETE_FIRST` for an occupied slot, `RANGE_ERROR` for bad sizes or counts, `INVALID_ARGUMENT` for an unknown type.

## Address spaces

Userspace builds address spaces from capabilities, as on seL4 for RISC-V (`kernel/arch/riscv/vspace.rs:78`):

| Operation | Effect |
|---|---|
| `ASID_POOL_ASSIGN` | make an unmapped page table the root of a new address space: give it a free ASID, empty its user half, and copy in the kernel's upper half |
| `PAGE_TABLE_MAP` | install a page table at the first level below the root that has no table for the address (`install_table`, `kernel/arch/riscv/paging.rs:340`); `DELETE_FIRST` if every level already has one |
| `PAGE_TABLE_UNMAP` | remove it and clear its entries |
| `FRAME_MAP` | map a frame at an address aligned to its size, with rights masked by the capability's and an execute-never attribute; the tables above must exist (`FAILED_LOOKUP` otherwise) |
| `FRAME_UNMAP` | remove the mapping |
| `FRAME_GET_ADDRESS` | return the frame's physical address, for device drivers |

The address space is passed as an extra capability: a root page table whose ASID is still assigned. Each frame capability maps once and records where, as in seL4; mapping the same frame twice means copying its capability first. A mapping needs read permission, since RISC-V has no write-only pages; user addresses must lie below `USER_END`. After each change the kernel flushes just the affected page or ASID ([[kernel-memory-riscv#ASIDs and the TLB]]).

Deleting a capability undoes what it did (see [[#Delete, finalise, revoke]]), so revoking the untyped memory an address space was built from unmaps every frame and table in it and frees its ASID.

## The root task's capabilities

`root_task::start` (`kernel/root_task.rs:47`) builds the root task's CNode (radix 12, 4,096 slots, `CNODE_RADIX`, line 40) and fills it, as seL4 does:

| Slots | Capability |
|---|---|
| 1–6 | its TCB, its CNode, its address space (a root page table with ASID 1), its IPC buffer frame, its boot-info frame, the ASID pool (`abi/lib.rs:312`) |
| a range | every frame mapped in its address space (image, stack) |
| a range | every page table of its address space |
| a range | all remaining free memory as untyped, split into naturally aligned power-of-two blocks of at most 2^38 bytes |
| the rest | empty |

The boot-info page (`BootInfo`, `abi/lib.rs:356`), mapped read-only, gives the ranges, the IPC buffer address, the CNode's radix, and each untyped block's address and size; its address arrives in the root task's first argument register. The kernel finds the frames and page tables by walking the address space it built (`arch::walk_user`). Creating the untyped capabilities uses up the boot allocator, so from then on all memory is the root task's to retype. On QEMU with 128 MiB (debug build): 11 frames, 4 page tables, 27 untyped blocks totalling about 126 MiB (largest 32 MiB), and 4,047 empty slots. The counts move with the size of `init`.

ASIDs come from a fixed table of 4,096 entries in the kernel (`kernel/arch/riscv/vspace.rs:26`); a `TODO` says to retype ASID pools from untyped memory, as seL4 does.

## Tests

`user/init` checks each operation against its expected error code (`user/init/main.rs`):
- CNodes: copy, copy into an occupied slot, copy of a copy, move, copy from an emptied slot, delete, delete of an empty slot, revoking untyped memory (which deletes a copy and the copy's copy), an out-of-range slot, and a wrong depth.
- Retype: carving a 64 KiB region from a larger one; retyping a CNode, a TCB, a page table and frames from it; an occupied destination; an unknown type; copying untyped memory that has children (`REVOKE_FIRST`); copying a capability into the new CNode through a two-level address; filling the region exactly, then `NOT_ENOUGH_MEMORY`; revoking the region, after which every object (including the copy inside the new CNode) is gone and the full 64 KiB can be retyped again.
- Address spaces: mapping a frame before its tables (`FAILED_LOOKUP`); mapping a level-1 and a level-0 table at 1 GiB; a third table when every level is taken; mapping a frame, mapping it twice, over another mapping, unaligned, at a kernel address, write-only; writing through the mapping, unmapping, remapping read-only elsewhere and reading the value back; a 2 MiB frame in the level-1 table, whose physical address matches its untyped region; a new address space from a retyped table (refused before it has an ASID, refused a second ASID); unmapping a table, after which the frame can't be mapped until a new table is installed, and then reads the same value. Reading the first mapping after revoking the region faults.

> [!todo] Not built
> Zombie-style preemptible deletion, retyped ASID pools, remapping a frame in place to change its rights.
