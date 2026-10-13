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
    /// capabilities' addresses are in the IPC buffer. Results come back in
    /// the same registers: `a1` = message info whose label is an `error`
    /// code, `a2`.. = result words.
    pub const CALL: usize = 0;
    /// Give up the rest of the time slice.
    pub const YIELD: usize = 6;

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
    /// (bits 0..6 = guard size, the rest = guard value).
    pub const CNODE_MINT: usize = 4;
    /// MR0..MR3 as COPY. Move the capability; the source slot ends empty.
    pub const CNODE_MOVE: usize = 5;

    // Untyped memory.
    /// MR0 = object type (`object`), MR1 = size bits (for untyped: log2 of
    /// its size; for CNodes: log2 of the slot count; ignored otherwise),
    /// MR2 = node index, MR3 = node depth, MR4 = node offset, MR5 = count;
    /// extra cap 0 = the root CNode the node is addressed from. Create
    /// `count` objects, with capabilities in slots `offset..offset+count` of
    /// the CNode the node index and depth name; depth 0 names the root
    /// CNode itself.
    pub const UNTYPED_RETYPE: usize = 6;
}

/// Capability rights, as in seL4.
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
    pub const CNODE: usize = 4;
    // RISC-V.
    pub const PAGE_TABLE: usize = 5;
    pub const FRAME_4K: usize = 6;
    pub const FRAME_2M: usize = 7;
    pub const FRAME_1G: usize = 8;

    /// Smallest and largest untyped objects, in bits.
    pub const MIN_UNTYPED_BITS: usize = 4;
    pub const MAX_UNTYPED_BITS: usize = 38;
    /// Most objects one retype creates.
    pub const MAX_RETYPE_COUNT: usize = 256;
}

/// The message info word: label (bits 12..), extra capabilities (7..9) and
/// length in words (0..7).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MessageInfo(pub usize);

impl MessageInfo {
    pub const fn new(label: usize, extra_caps: usize, length: usize) -> Self {
        Self(label << 12 | (extra_caps & 0b11) << 7 | (length & 0x7f))
    }
    pub const fn label(self) -> usize {
        self.0 >> 12
    }
    pub const fn extra_caps(self) -> usize {
        (self.0 >> 7) & 0b11
    }
    pub const fn length(self) -> usize {
        self.0 & 0x7f
    }
}

/// Message words carried in registers; the rest are in the IPC buffer.
pub const MESSAGE_REGISTERS: usize = 4;
/// The longest message, in words.
pub const MAX_MESSAGE_LENGTH: usize = 120;
/// The most extra capabilities one message names.
pub const MAX_EXTRA_CAPS: usize = 3;

/// The per-thread IPC buffer, one page-aligned 1 KiB block in the thread's
/// address space. Message words beyond the registers go in `msg` at the
/// same index (`msg[4]` is message word 4).
#[repr(C)]
pub struct IpcBuffer {
    pub tag: usize,
    pub msg: [usize; MAX_MESSAGE_LENGTH],
    pub user_data: usize,
    /// Addresses of the extra capabilities sent with a message.
    pub caps: [usize; MAX_EXTRA_CAPS],
    pub receive_cnode: usize,
    pub receive_index: usize,
    pub receive_depth: usize,
}

const _: () = assert!(core::mem::size_of::<IpcBuffer>() == 1024);

/// Slots of the root task's CNode that the kernel fills at boot.
pub mod init_slot {
    pub const NULL: usize = 0;
    pub const TCB: usize = 1;
    pub const CNODE: usize = 2;
    pub const VSPACE: usize = 3;
    pub const IPC_BUFFER: usize = 4;
    pub const BOOT_INFO: usize = 5;
    /// The first slot of the ranges listed in `BootInfo`.
    pub const FIRST_FREE: usize = 6;
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
