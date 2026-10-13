---
status: implemented
summary: Capabilities as implemented today, following seL4 - the capability representation, CNodes of 64-byte slots, the derivation tree kept as a linked list through the slots, resolving capability addresses through guarded CNodes, delete and revoke with finalisation, the CALL invocation path, CNode operations, retyping untyped memory, building address spaces from page-table and frame capabilities, and the capabilities the root task starts with.
updated: 2026-10-13
code: [kernel/cap.rs, kernel/cspace.rs, kernel/invocation.rs, kernel/untyped.rs, kernel/thread.rs, kernel/root_task.rs, kernel/arch/riscv/caps.rs, kernel/arch/riscv/vspace.rs, abi/lib.rs, user/libsap/sys.rs, user/libsap/cnode.rs, user/libsap/untyped.rs, user/libsap/vspace.rs, user/init/main.rs, kernel/arch/riscv/paging.rs]
sources: ["[[2026-10-08-22c3947b-wiki-setup]]"]
---

# Capabilities

The second part of roadmap milestone M3 ([[roadmap]]): every thread has a capability space, the kernel resolves capability addresses and invokes kernel objects through them, and the root task starts with capabilities to everything. The design is seL4's, as decided ([[kernel-choice]]); where sap differs, this page says so. The model behind it is [[cap-model]] and [[kernel-objects-and-syscalls]].

## Representation

A capability is a `Cap` (`kernel/cap.rs:20`): a tag and the per-capability state seL4 keeps in its capability bits.

| Variant | Names | State in the capability |
|---|---|---|
| `Null` | nothing | |
| `Untyped` | a naturally aligned power-of-two block of physical memory | base, size, how much has been retyped (`free_index`), whether it is device memory |
| `CNode` | a table of `1 << radix` slots | radix, guard size and guard value |
| `Thread` | a TCB ([[kernel-threads]]) | |
| `Arch(PageTable)` | a RISC-V page table, any level (`kernel/arch/riscv/caps.rs:20`) | unmapped, the root of an address space with an ASID, or mapped at a level and address |
| `Arch(Frame)` | a 4 KiB, 2 MiB or 1 GiB frame | rights, and the one mapping this capability made: ASID and address, ASID 0 meaning unmapped |
| `Arch(AsidPool)` | the kernel's pool of address-space identifiers | |

The enum is `repr(u8)` with small fields first, so a `Cap` is 40 bytes and an all-zero `Cap` is `Null`: zeroed memory is a CNode of empty slots. Endpoints and notifications arrive with IPC (M4).

## Slots and CNodes

A `Slot` (`kernel/cspace.rs:28`) holds a capability and its links in the derivation tree, in 64 bytes (`SLOT_BITS = 6`, line 25; seL4 uses 32 with packed bitfields). A CNode of radix *r* occupies `1 << (r + 6)` bytes. A TCB holds three slots of its own: its capability-space root, its address-space root and its IPC buffer frame (`kernel/thread.rs:24-26`), copies whose deletion takes the thread's reference away, as in seL4.

## The derivation tree

Every slot is on one doubly linked list, seL4's "mapping database", ordered so that a capability's descendants follow it directly. Nothing is allocated; the links live in the slots.
- **Inserting.** An original (a boot capability, or a newly retyped object) starts a tree, or goes right after the untyped memory it came from (`insert_original`, line 132). A copy goes right after its source (`insert_derived`, line 151).
- **Parenthood is computed, not stored.** `b` is a child of `a` if `a` is *revocable* and `b`'s object lies within `a`'s (`is_parent_of`, line 224; `Cap::same_region_as` in `cap.rs`). Originals are revocable, and so are copies of untyped memory (`Cap::is_revocable_copy`), as in seL4. So a copy of a TCB capability is a sibling, while a copy of untyped memory is a child and owns what is later retyped through it.
- **Moving** keeps the capability's place in the list (`move_cap`, line 166).

## Resolving addresses

A capability address is resolved like a page-table walk (`resolve`, `kernel/cspace.rs:75`): each CNode consumes its guard bits, which must match its guard, then `radix` bits that index a slot; if bits remain and the slot holds another CNode, the walk continues there. An invocation names its target with a full-width address (`lookup_cap`, line 118); CNode operations name a slot by (index, depth) relative to a CNode and require the depth to be used exactly (`lookup_target`, line 110), so they can name a CNode slot itself or an empty slot. The root task's CNode has radix 12 and a 52-bit zero guard, so slot *n* is simply address *n*.

## Delete, finalise, revoke

`delete` (`kernel/cspace.rs:259`) removes a capability. If it was the **last** capability to its object (`is_final`, line 231: no neighbour on the list names the same object, since copies are adjacent), the object is finalised (line 280):
- a CNode's contents are deleted, recursively;
- a thread is taken off the CPU and its own slots are deleted (`thread::finalise`);
- a page table that is an address-space root frees its ASID; a mapped one is unmapped.

Deleting a frame capability that maps its frame unmaps it whether or not it was the last, since each frame capability is its own mapping (`kernel/arch/riscv/vspace.rs:162`). The object's memory isn't reclaimed by deletion; that happens when the untyped memory it came from is revoked.

The slot is emptied *before* finalisation, so a CNode that contains the slot being deleted can't reach it again while its contents are deleted. seL4 instead turns the capability into a "zombie" so deletion can be preempted and doesn't recurse. sap recurses and refuses nesting deeper than 32 CNodes (`DeleteError::TooDeep`, line 54), marked `TODO`.

`revoke` (line 310) deletes the children that follow a capability until the next capability isn't one.

## Invocation

`CALL` (`kernel/invocation.rs:74`) takes the invoked capability's address in `a0`, a message info word in `a1` (label, number of extra capabilities, length, laid out as in seL4, `abi/lib.rs:141`), and message words 0–3 in `a2`–`a5`. Words 4 and up, and the addresses of up to three extra capabilities, are in the thread's IPC buffer (`abi/lib.rs:169`), a 1 KiB block the kernel reaches through the frame capability in the TCB. The reply's label is an error code (`abi/lib.rs:33`, numbered as seL4's) and its result words come back in the same registers.

The capability's type picks the object, and the label picks the operation (`abi/lib.rs:54`). CNodes support (`kernel/invocation.rs:127`):

| Operation | Effect |
|---|---|
| `REVOKE` | delete everything derived from the capability in a slot |
| `DELETE` | delete the capability in a slot |
| `COPY` | derive a copy into an empty slot, with rights reduced to a mask |
| `MINT` | `COPY` plus a new guard for a CNode capability |
| `MOVE` | move a capability to an empty slot |

Derivation follows seL4's `deriveCap` (line 176): untyped memory can only be copied while nothing has been retyped from it (`REVOKE_FIRST` otherwise), and a copied frame capability starts unmapped. A destination that isn't empty gives `DELETE_FIRST`; an address that doesn't resolve gives `FAILED_LOOKUP`. Kernel object methods are plain constants in `sap-abi` for now; a `TODO` says to generate them from a Cap'n Proto schema (roadmap D5, [[cap-process-authority#Runtime capabilities and declared imports]]).

On the user side, `libsap` (`user/libsap/`) wraps `CALL` (`sys::call`, `user/libsap/sys.rs:42`), the CNode operations (`user/libsap/cnode.rs`) and retype (`user/libsap/untyped.rs`).

## Retyping untyped memory

Retyping is the only way kernel objects come into existence after boot, so every object's memory is charged to whoever held the untyped capability ([[kernel-objects-and-syscalls#Kernel memory]]). `UNTYPED_RETYPE` (`kernel/invocation.rs:101`, `kernel/untyped.rs:61`) takes an object type (`abi/lib.rs:119`), a size where the type has one, a destination CNode named by (index, depth) from a root CNode passed as an extra capability (depth 0 names the root itself, as in seL4), and a range of empty slots in it, at most 256.

| Type | Size of one object |
|---|---|
| Untyped | `1 << size_bits`, 16 bytes to 256 GiB |
| TCB | 1 KiB |
| CNode | `1 << size_bits` slots of 64 bytes, up to 2^26 slots (`MAX_CNODE_RADIX`, line 21) |
| Page table | 4 KiB |
| Frame | 4 KiB, 2 MiB or 1 GiB |

Endpoints and notifications are accepted as types but refused until IPC exists (M4). Device untyped memory can only become frames or smaller untyped regions.

Untyped memory is handed out with a **watermark**: each object goes at the next address aligned to its size, and the capability's `free_index` advances past it. Memory below the watermark is never handed out again while anything retyped from the region still exists. Once everything has been revoked, so the capability has no children (line 92), the next retype starts from zero. That is the only way memory is reclaimed, and why revoking untyped memory is how a parent takes back everything a child built from it. New objects are zeroed (except in device memory), and their capabilities are inserted as originals right after the untyped capability, so they are its children (`create`, line 121). Every check happens before anything changes, so a failed retype leaves no partial result: `NOT_ENOUGH_MEMORY` past the end of the region, `DELETE_FIRST` for an occupied slot, `RANGE_ERROR` for bad sizes or counts, `INVALID_ARGUMENT` for an unknown type.

## Address spaces

Userspace builds address spaces from capabilities, as on seL4 for RISC-V (`kernel/arch/riscv/vspace.rs:73`):

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
| 1–6 | its TCB, its CNode, its address space (a root page table with ASID 1), its IPC buffer frame, its boot-info frame, the ASID pool (`abi/lib.rs:183`) |
| a range | every frame mapped in its address space (image, stack) |
| a range | every page table of its address space |
| a range | all remaining free memory as untyped, split into naturally aligned power-of-two blocks of at most 2^38 bytes |
| the rest | empty |

The boot-info page (`BootInfo`, `abi/lib.rs:227`), mapped read-only, gives the ranges, the IPC buffer address, the CNode's radix, and each untyped block's address and size; its address arrives in the root task's first argument register. The kernel finds the frames and page tables by walking the address space it built (`arch::walk_user`). Creating the untyped capabilities uses up the boot allocator, so from then on all memory is the root task's to retype. On QEMU with 128 MiB (debug build): 11 frames, 4 page tables, 27 untyped blocks totalling about 126 MiB (largest 32 MiB), and 4,047 empty slots. The counts move with the size of `init`.

ASIDs come from a fixed table of 4,096 entries in the kernel (`kernel/arch/riscv/vspace.rs:26`); a `TODO` says to retype ASID pools from untyped memory, as seL4 does.

## Tests

`user/init` checks each operation against its expected error code (`user/init/main.rs`):
- CNodes: copy, copy into an occupied slot, copy of a copy, move, copy from an emptied slot, delete, delete of an empty slot, revoking untyped memory (which deletes a copy and the copy's copy), an out-of-range slot, and a wrong depth.
- Retype: carving a 64 KiB region from a larger one; retyping a CNode, a TCB, a page table and frames from it; an occupied destination; an unknown type; copying untyped memory that has children (`REVOKE_FIRST`); copying a capability into the new CNode through a two-level address; filling the region exactly, then `NOT_ENOUGH_MEMORY`; revoking the region, after which every object (including the copy inside the new CNode) is gone and the full 64 KiB can be retyped again.
- Address spaces: mapping a frame before its tables (`FAILED_LOOKUP`); mapping a level-1 and a level-0 table at 1 GiB; a third table when every level is taken; mapping a frame, mapping it twice, over another mapping, unaligned, at a kernel address, write-only; writing through the mapping, unmapping, remapping read-only elsewhere and reading the value back; a 2 MiB frame in the level-1 table, whose physical address matches its untyped region; a new address space from a retyped table (refused before it has an ASID, refused a second ASID); unmapping a table, after which the frame can't be mapped until a new table is installed, and then reads the same value. Reading the first mapping after revoking the region faults.

> [!todo] Not built
> TCB invocations (next), endpoints and notifications, badges, zombie-style preemptible deletion, retyped ASID pools, remapping a frame in place to change its rights.
