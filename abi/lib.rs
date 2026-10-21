//! The binary interface between the kernel and user programs: syscall
//! numbers, invocation labels, error codes and the layouts of structures
//! both sides read. The kernel and every user program depend on this crate,
//! so the two can't drift apart. Numbering and layouts follow seL4 where it
//! has an equivalent.
//!
//! TODO: generate this from a Cap'n Proto schema of the kernel's objects
//! (roadmap D5), so other languages get the same definitions.

#![no_std]

/// System call numbers, passed in `a7` on RISC-V.
pub mod syscall {
    /// Invoke a capability and wait for the reply. Arguments: `a0` = the
    /// capability's address (CPtr), `a1` = message info, `a2`..`a5` =
    /// message registers 0..3; the rest of the message and the extra
    /// capabilities' addresses are in the IPC buffer.
    ///
    /// On a kernel object, the results come back in the same registers:
    /// `a1` = message info whose label is an `error` code, `a2`.. = result
    /// words. On an endpoint, the call is delivered to a receiver and the
    /// caller waits for its reply, which arrives as a received message (see
    /// RECV); `a7` = an `error` code for the call itself.
    pub const CALL: usize = 0;

    // Endpoint IPC. Sending: `a0` = endpoint, `a1` = message info, `a2`..
    // `a5` = message registers 0..3, the rest in the IPC buffer. Receiving:
    // `a0` = endpoint, `a6` = a reply object for answering a CALL. A
    // received message arrives as `a0` = the sender's badge, `a1` = message
    // info (the sender's label, the length delivered), `a2`..`a5` = message
    // registers 0..3 (the rest in the IPC buffer), `a6` = what woke the
    // receiver (`recv_source`). Every IPC syscall returns `a7` = an `error`
    // code.
    /// Send, blocking until a receiver takes the message.
    pub const SEND: usize = 1;
    /// Send if a receiver is waiting; otherwise drop the message.
    pub const NB_SEND: usize = 2;
    /// Block until a message (or a signal on the bound notification).
    pub const RECV: usize = 3;
    /// Receive if a message is waiting; otherwise return at once.
    pub const NB_RECV: usize = 4;
    /// `a6` = reply object, `a1`.. = the reply message. Answer the caller
    /// waiting on the reply object; nothing happens if none is.
    pub const REPLY: usize = 5;
    /// Give up the rest of the time slice.
    pub const YIELD: usize = 6;
    /// REPLY through `a6`, then RECV on `a0` with the same reply object:
    /// a server's loop in one system call.
    pub const REPLY_RECV: usize = 7;

    /// Notifications: `a0` = the notification capability's address.
    /// Results: `a0` = badge bits (WAIT, POLL), `a1` = an `error` code.
    /// Hand the capability's badge to a waiter, or add it to the pending
    /// bits. A signal through an unbadged capability (badge 0) still
    /// counts.
    pub const SIGNAL: usize = 8;
    /// Block until a signal is pending, then take the pending bits.
    pub const WAIT: usize = 9;
    /// Take the pending bits without blocking. `a2` = 1 if a signal was
    /// pending, 0 if not: signals through unbadged capabilities leave no
    /// bits, so `a0` alone can't tell.
    pub const POLL: usize = 10;

    /// TODO: debug-only calls, like seL4's; compile them out of release
    /// kernels once a userspace log service exists (M7).
    /// Print one byte on the kernel console.
    pub const DEBUG_PUTCHAR: usize = 0x100;
    /// Stop the machine; a nonzero argument reports failure.
    pub const DEBUG_HALT: usize = 0x101;
}

/// Error codes, returned as the label of the reply's message info.
pub mod error {
    pub const OK: usize = 0;
    pub const INVALID_ARGUMENT: usize = 1;
    /// A capability is the wrong type or lacks a right.
    pub const INVALID_CAPABILITY: usize = 2;
    /// The invocation label isn't valid for this object.
    pub const ILLEGAL_OPERATION: usize = 3;
    pub const RANGE_ERROR: usize = 4;
    pub const ALIGNMENT_ERROR: usize = 5;
    /// A capability address didn't resolve to a slot.
    pub const FAILED_LOOKUP: usize = 6;
    /// The message was shorter than the invocation needs.
    pub const TRUNCATED_MESSAGE: usize = 7;
    /// The destination slot isn't empty.
    pub const DELETE_FIRST: usize = 8;
    /// The capability has children that must be revoked first.
    pub const REVOKE_FIRST: usize = 9;
    pub const NOT_ENOUGH_MEMORY: usize = 10;
    /// A CALL was delivered, but the receiver can't reply: it gave no reply
    /// object, or the reply object was deleted before it replied.
    pub const NO_REPLY: usize = 11;
}

/// What woke a receiver, returned in `a6`.
pub mod recv_source {
    /// A message from a sender.
    pub const MESSAGE: usize = 0;
    /// A signal on the receiver's bound notification; `a0` = its bits.
    pub const NOTIFICATION: usize = 1;
    /// Nothing was waiting (NB_RECV only).
    pub const NOTHING: usize = 2;
}

/// What `CNODE_IDENTIFY` reports about a capability besides its type.
pub mod identify {
    /// No other capability names the same object. For a frame this means
    /// that, once its holder maps it, nobody else can map it: page transfer
    /// relies on it. Never set for untyped memory, whose region other
    /// untyped capabilities can cover.
    pub const EXCLUSIVE: usize = 1 << 0;
    /// A frame or page-table capability that is mapped somewhere.
    pub const MAPPED: usize = 1 << 1;
}

/// Invocation labels: the operation requested on the invoked capability.
pub mod label {
    // CNode: address a slot in the invoked CNode by (index, depth).
    /// MR0 = index, MR1 = depth. Delete every capability derived from the
    /// one in the slot.
    pub const CNODE_REVOKE: usize = 1;
    /// MR0 = index, MR1 = depth. Delete the capability in the slot.
    pub const CNODE_DELETE: usize = 2;
    /// MR0 = dest index, MR1 = dest depth, MR2 = src index, MR3 = src
    /// depth, MR4 = rights; extra cap 0 = the source's root CNode. Derive a
    /// copy with at most `rights`.
    pub const CNODE_COPY: usize = 3;
    /// Like COPY, plus MR5 = data: the new guard for a CNode capability
    /// (bits 0..6 = guard size, the rest = guard value), or the badge for
    /// an unbadged notification or endpoint capability.
    pub const CNODE_MINT: usize = 4;
    /// MR0..MR3 as COPY. Move the capability; the source slot ends empty.
    pub const CNODE_MOVE: usize = 5;
    /// MR0 = index, MR1 = depth. Describe the capability in the slot without
    /// invoking it: invoking a capability of unknown type can do anything
    /// its type does (on an endpoint, it is an IPC call). Returns MR0 = its
    /// object type (`object`), MR1 = `identify` flags; FAILED_LOOKUP if the
    /// slot is empty.
    pub const CNODE_IDENTIFY: usize = 23;

    // Untyped memory.
    /// MR0 = object type (`object`), MR1 = size bits (for untyped: log2 of
    /// its size; for CNodes: log2 of the slot count; ignored otherwise),
    /// MR2 = node index, MR3 = node depth, MR4 = node offset, MR5 = count;
    /// extra cap 0 = the root CNode the node is addressed from. Create
    /// `count` objects, with capabilities in slots `offset..offset+count` of
    /// the CNode the node index and depth name; depth 0 names the root
    /// CNode itself.
    pub const UNTYPED_RETYPE: usize = 6;

    // RISC-V address spaces, as seL4's.
    /// MR0 = virtual address, MR1 = attributes (unused); extra cap 0 = the
    /// address space (a root page table with an ASID). Install the page
    /// table at the first level below the root that has no table for the
    /// address.
    pub const PAGE_TABLE_MAP: usize = 7;
    /// Remove the page table from its address space and clear it.
    pub const PAGE_TABLE_UNMAP: usize = 8;
    /// MR0 = virtual address (aligned to the frame's size), MR1 = rights
    /// (`rights`, masked by the capability's), MR2 = attributes (bit 0 =
    /// not executable); extra cap 0 = the address space. The page tables
    /// above the address must already be mapped (`FAILED_LOOKUP`
    /// otherwise).
    pub const FRAME_MAP: usize = 9;
    pub const FRAME_UNMAP: usize = 10;
    /// Returns the frame's physical address in MR0 (for device drivers).
    pub const FRAME_GET_ADDRESS: usize = 11;
    /// Extra cap 0 = an unmapped page table. Make it the root of a new
    /// address space with a free ASID.
    pub const ASID_POOL_ASSIGN: usize = 12;

    // Threads.
    /// MR0 = IPC buffer address; extra caps 0 = capability-space root
    /// (a CNode), 1 = address-space root, 2 = IPC buffer frame (optional).
    /// The thread keeps copies of the capabilities.
    pub const TCB_CONFIGURE: usize = 13;
    /// MR0 = flags (bit 0: resume afterwards), MR1 = count, MR2.. =
    /// registers in `reg` order. Not allowed on the calling thread.
    pub const TCB_WRITE_REGISTERS: usize = 14;
    /// MR0 = count. Returns registers in `reg` order. Not allowed on the
    /// calling thread.
    pub const TCB_READ_REGISTERS: usize = 15;
    pub const TCB_RESUME: usize = 16;
    pub const TCB_SUSPEND: usize = 17;
    /// MR0 = priority; extra cap 0 = an authority thread whose maximum
    /// controlled priority must be at least the new priority.
    pub const TCB_SET_PRIORITY: usize = 18;
    /// MR0 = maximum controlled priority; extra cap 0 = authority, as above.
    pub const TCB_SET_MC_PRIORITY: usize = 19;
    /// Extra cap 0 = a notification (with the read right). Signals on it
    /// also wake the thread from RECV on any endpoint.
    pub const TCB_BIND_NOTIFICATION: usize = 20;
    pub const TCB_UNBIND_NOTIFICATION: usize = 21;
    /// Extra cap 0 = an endpoint (with the write right), or none to clear.
    /// The thread's faults are delivered there (`fault`).
    pub const TCB_SET_FAULT_HANDLER: usize = 22;
}

/// Fault messages. A thread that faults makes an implicit CALL to its
/// fault-handler endpoint, with the fault kind as the label and message
/// words `PC`, `ADDRESS`, `CAUSE`. A reply labelled 0 resumes the thread at
/// the faulting instruction (after the handler fixed the cause, say by
/// mapping a page); any other label leaves it suspended. The reply's words
/// don't touch the thread's registers.
pub mod fault {
    /// A page fault or access fault on an instruction fetch, load or store.
    pub const VM: usize = 1;
    /// Any other exception: illegal instruction, misaligned access,
    /// breakpoint.
    pub const USER_EXCEPTION: usize = 2;

    /// Message word indices.
    pub const PC: usize = 0;
    pub const ADDRESS: usize = 1;
    /// The architecture's exception code (`scause` on RISC-V).
    pub const CAUSE: usize = 2;
    pub const LENGTH: usize = 3;
}

/// Register order for reading and writing a thread's registers (RISC-V).
pub mod reg {
    pub const PC: usize = 0;
    pub const RA: usize = 1;
    pub const SP: usize = 2;
    pub const GP: usize = 3;
    pub const TP: usize = 4;
    /// a0..a7 follow.
    pub const A0: usize = 5;
    pub const COUNT: usize = 13;
}

/// Frame mapping attributes.
pub mod attr {
    /// Map without execute permission.
    pub const EXECUTE_NEVER: usize = 1 << 0;
}

/// Capability rights, as in seL4.
///
/// For notifications: WRITE = may signal, READ = may wait or poll.
/// For endpoints: WRITE = may send or call, READ = may receive, GRANT = may
/// send capabilities, GRANT_REPLY = replies to this caller's calls may carry
/// capabilities.
pub mod rights {
    pub const WRITE: usize = 1 << 0;
    pub const READ: usize = 1 << 1;
    pub const GRANT: usize = 1 << 2;
    pub const GRANT_REPLY: usize = 1 << 3;
    pub const ALL: usize = WRITE | READ | GRANT | GRANT_REPLY;
}

/// Kernel object types, for retyping untyped memory.
pub mod object {
    pub const UNTYPED: usize = 0;
    pub const TCB: usize = 1;
    pub const ENDPOINT: usize = 2;
    pub const NOTIFICATION: usize = 3;
    pub const REPLY: usize = 9;
    pub const CNODE: usize = 4;
    // RISC-V.
    pub const PAGE_TABLE: usize = 5;
    pub const FRAME_4K: usize = 6;
    pub const FRAME_2M: usize = 7;
    pub const FRAME_1G: usize = 8;
    /// The ASID pool: never made by retype, only reported by
    /// `CNODE_IDENTIFY`.
    pub const ASID_POOL: usize = 10;

    /// Smallest and largest untyped objects, in bits.
    pub const MIN_UNTYPED_BITS: usize = 4;
    pub const MAX_UNTYPED_BITS: usize = 38;
    /// Most objects one retype creates.
    pub const MAX_RETYPE_COUNT: usize = 256;
}

/// The message info word: label (bits 12..), extra capabilities (7..12)
/// and length in words (0..7).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MessageInfo(pub usize);

impl MessageInfo {
    pub const fn new(label: usize, extra_caps: usize, length: usize) -> Self {
        Self(label << 12 | (extra_caps & 0x1f) << 7 | (length & 0x7f))
    }
    pub const fn label(self) -> usize {
        self.0 >> 12
    }
    pub const fn extra_caps(self) -> usize {
        (self.0 >> 7) & 0x1f
    }
    pub const fn length(self) -> usize {
        self.0 & 0x7f
    }
}

/// Message words carried in registers; the rest are in the IPC buffer.
pub const MESSAGE_REGISTERS: usize = 4;
/// The longest message, in words.
pub const MAX_MESSAGE_LENGTH: usize = 120;
/// The most capabilities one message carries. The receiver can't be flooded
/// (capabilities land only in the window it set); the bound keeps the
/// kernel's work per message, which can't be preempted, small.
pub const MAX_EXTRA_CAPS: usize = 16;

/// The per-thread IPC buffer: a 2 KiB block, aligned to its size, in the
/// thread's address space. Message words beyond the registers go in `msg`
/// at the same index (`msg[4]` is message word 4).
#[repr(C)]
pub struct IpcBuffer {
    pub tag: usize,
    pub msg: [usize; MAX_MESSAGE_LENGTH],
    pub user_data: usize,
    /// Addresses of the capabilities sent with a message (or, for a kernel
    /// invocation, its extra capability arguments).
    pub caps: [usize; MAX_EXTRA_CAPS],
    /// Bit `i` set: capability `i` is moved (the sender's slot ends empty);
    /// clear: a copy is derived.
    pub caps_move: usize,
    /// Where received capabilities go: slots `receive_index..receive_index +
    /// receive_count` of the CNode whose capability is at `receive_cnode` in
    /// this thread's space. Transfer stops at the first full slot.
    pub receive_cnode: usize,
    pub receive_index: usize,
    pub receive_count: usize,
    pub _reserved: [usize; 114],
}

const _: () = assert!(core::mem::size_of::<IpcBuffer>() == 2048);

/// Slots of the root task's CNode that the kernel fills at boot.
pub mod init_slot {
    pub const NULL: usize = 0;
    pub const TCB: usize = 1;
    pub const CNODE: usize = 2;
    pub const VSPACE: usize = 3;
    pub const IPC_BUFFER: usize = 4;
    pub const BOOT_INFO: usize = 5;
    /// The ASID pool: assigns identifiers to new address spaces.
    pub const ASID_POOL: usize = 6;
    /// The first slot of the ranges listed in `BootInfo`.
    pub const FIRST_FREE: usize = 7;
}

/// A range of slots in the root task's CNode, `start..end`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SlotRange {
    pub start: usize,
    pub end: usize,
}

impl SlotRange {
    pub const fn len(&self) -> usize {
        self.end - self.start
    }
    pub const fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct UntypedDesc {
    pub paddr: usize,
    pub size_bits: u8,
    pub is_device: u8,
    pub _pad: [u8; 6],
}

pub const MAX_BOOT_UNTYPED: usize = 230;

/// What the kernel tells the root task at startup, in a read-only page
/// whose address arrives in the root task's first argument register.
#[repr(C)]
pub struct BootInfo {
    pub node_id: usize,
    pub num_nodes: usize,
    /// Ticks per second of the time counter, which user mode reads with
    /// `rdtime` on RISC-V.
    pub timer_hz: u64,
    /// Address of the root task's IPC buffer.
    pub ipc_buffer: usize,
    /// log2 of the number of slots in the root task's CNode.
    pub cnode_radix: usize,
    /// Slots the root task may use.
    pub empty: SlotRange,
    /// Frames holding the root task's image, stack and boot pages.
    pub image_frames: SlotRange,
    /// Page tables of the root task's address space.
    pub image_paging: SlotRange,
    /// Untyped memory, described in order in `untyped_list`.
    pub untyped: SlotRange,
    pub untyped_list: [UntypedDesc; MAX_BOOT_UNTYPED],
}

const _: () = assert!(core::mem::size_of::<BootInfo>() <= 4096);

/// Slots of every process's CNode that its launcher fills, other than the
/// root task's (`init_slot`). The process's imports follow, in
/// `Startup::imports`.
pub mod process_slot {
    pub const NULL: usize = 0;
    /// The process's main thread.
    pub const TCB: usize = 1;
    /// The process's CNode, guarded so that slot numbers are capability
    /// addresses.
    pub const CNODE: usize = 2;
    pub const VSPACE: usize = 3;
    /// Untyped memory: the process's memory budget. Empty if it has none.
    pub const UNTYPED: usize = 4;
    /// The first slot of the process's imports.
    // TODO: a slot for the connection to the launcher, over which exit is
    // reported (M7, process factory).
    pub const FIRST_IMPORT: usize = 8;
}

/// A range of virtual addresses, `start..end`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AddressRange {
    pub start: usize,
    pub end: usize,
}

/// What a process's launcher tells it at startup, in a read-only page whose
/// address arrives in its first argument register. The bootstrap message
/// follows the record in the same page.
#[repr(C)]
pub struct Startup {
    /// Ticks per second of the time counter.
    pub timer_hz: u64,
    pub ipc_buffer: usize,
    pub stack: AddressRange,
    /// Addresses nothing is mapped at, for the runtime's heap.
    pub heap: AddressRange,
    /// log2 of the number of slots in the process's CNode.
    pub cnode_radix: usize,
    /// The capabilities the bootstrap message refers to, in order.
    pub imports: SlotRange,
    /// Empty slots the process may use.
    pub free: SlotRange,
    /// Length in bytes of the bootstrap message: a Cap'n Proto message in
    /// the flat (single-segment) encoding, starting at `BOOTSTRAP_OFFSET`.
    /// Its root struct is the program's imports.
    pub bootstrap_len: usize,
}

/// Where the bootstrap message starts in the startup page.
pub const BOOTSTRAP_OFFSET: usize = 128;
/// The longest bootstrap message.
// TODO: larger bootstraps, in frames of their own (M7, if a program needs
// more than a page).
pub const MAX_BOOTSTRAP: usize = 4096 - BOOTSTRAP_OFFSET;

const _: () = assert!(core::mem::size_of::<Startup>() <= BOOTSTRAP_OFFSET);
