//! Hostile arguments: every kernel invocation, and the system calls that
//! can't block, with each argument in turn replaced by an extreme value.
//! Whatever the kernel answers, success or an error, it must answer: a
//! kernel panic (an index out of bounds, or an arithmetic overflow in a
//! build with overflow checks) ends the boot with a failure. Run under the
//! audit (`cargo run --profile audit --features audit`), every answer must
//! also leave the kernel consistent.
//!
//! Each case works on scratch objects, remade before it from one untyped
//! region: a CNode for the CNode invocations (so an extreme slot index only
//! reaches scratch slots), a thread, a frame, two page tables, an address
//! space with tables, an endpoint, a notification and a reply object.
//! Revoking the region removes whatever a case made. A case varies one
//! argument and keeps the rest naming scratch objects; varied capability
//! addresses may name this program's own capabilities, but only as
//! arguments, never as the object invoked.
//!
//! Endpoints are never invoked, and blocking system calls never made: both
//! would wait forever.

use sap_rt::abi::{BootInfo, MessageInfo, init_slot, label, object, reg, rights, syscall};
use sap_rt::cnode::SlotRef;
use sap_rt::{println, sys, vspace};

use crate::support::{fails, make, make_many, ok, slot, try_retype, untyped_at_least};

/// Values every word argument takes in turn: small numbers around type
/// codes, bit counts and slot counts, powers of two around page and table
/// sizes, the top of user space, kernel addresses and the largest values.
const EXTREMES: [usize; 29] = [
    0,
    1,
    2,
    3,
    4,
    5,
    6,
    7,
    12,
    13,
    31,
    32,
    63,
    64,
    65,
    127,
    128,
    255,
    256,
    4095,
    4096,
    1 << 20,
    1 << 32,
    (1 << 38) - 0x1000,
    1 << 38,
    0xffff_ffc0_0000_0000,
    1 << 63,
    usize::MAX - 1,
    usize::MAX,
];

/// Capability addresses every capability argument takes in turn, besides
/// each scratch object: this program's fixed slots, an empty slot, and
/// addresses past the end of its CNode.
const CPTRS: [usize; 12] = [
    0,
    init_slot::TCB,
    init_slot::CNODE,
    init_slot::VSPACE,
    init_slot::IPC_BUFFER,
    init_slot::BOOT_INFO,
    init_slot::ASID_POOL,
    4095,
    4096,
    1 << 20,
    1 << 40,
    usize::MAX,
];

/// Where the scratch address space has page tables, and the scratch frame
/// is mapped. Nothing of this program's is mapped in the same 1 GiB.
const VA: usize = 0x7_0000_0000;

/// The scratch CNode's slots take this many bits.
const RADIX: usize = 4;

/// The most words an invocation here takes: WRITE_REGISTERS's.
const MAX_WORDS: usize = 2 + reg::COUNT;

/// The most capabilities an invocation here takes: CONFIGURE's.
const MAX_CAPS: usize = 3;

/// One invocation as a case: whom it invokes, with what.
#[derive(Clone, Copy)]
struct Invocation {
    target: usize,
    label: usize,
    words: [usize; MAX_WORDS],
    word_count: usize,
    caps: [usize; MAX_CAPS],
    cap_count: usize,
}

impl Invocation {
    fn new(target: usize, label: usize, words: &[usize], caps: &[usize]) -> Invocation {
        let mut invocation = Invocation {
            target,
            label,
            words: [0; MAX_WORDS],
            word_count: words.len(),
            caps: [0; MAX_CAPS],
            cap_count: caps.len(),
        };
        invocation.words[..words.len()].copy_from_slice(words);
        invocation.caps[..caps.len()].copy_from_slice(caps);
        invocation
    }
}

/// The scratch objects, each always in the same slot of the root CNode.
struct Scratch {
    /// The untyped region the others are made from, and where it comes from.
    region: usize,
    parent: usize,
    cnode: usize,
    thread: usize,
    endpoint: usize,
    notification: usize,
    reply: usize,
    frame: usize,
    table: usize,
    spare_table: usize,
    space: usize,
    space_tables: usize,
    /// How many cases ran, and how many of them the kernel refused.
    cases: usize,
    refused: usize,
}

impl Scratch {
    fn new(info: &BootInfo) -> Scratch {
        let base = info.empty.start + 400;
        let scratch = Scratch {
            region: base,
            parent: untyped_at_least(info, 21),
            cnode: base + 1,
            thread: base + 2,
            endpoint: base + 3,
            notification: base + 4,
            reply: base + 5,
            frame: base + 6,
            table: base + 7,
            spare_table: base + 8,
            space: base + 9,
            space_tables: base + 10,
            cases: 0,
            refused: 0,
        };

        let carve = try_retype(scratch.parent, object::UNTYPED, 20, scratch.region, 1);
        ok("carve the scratch region", carve);
        scratch
    }

    /// Every scratch object, as capability arguments of the wrong kind.
    fn objects(&self) -> [usize; 11] {
        [
            self.region,
            self.cnode,
            self.thread,
            self.endpoint,
            self.notification,
            self.reply,
            self.frame,
            self.table,
            self.spare_table,
            self.space,
            self.space_tables,
        ]
    }

    /// Slot `index` of the scratch CNode.
    fn inner(&self, index: usize) -> SlotRef {
        SlotRef {
            root: self.cnode,
            index,
            depth: RADIX,
        }
    }

    /// Remove whatever the last case made, and make the scratch objects
    /// again.
    fn reset(&self) {
        ok("revoke the scratch region", slot(self.region).revoke());

        let cnode = try_retype(self.region, object::CNODE, RADIX, self.cnode, 1);
        ok("scratch CNode", cnode);
        make(self.region, object::TCB, self.thread);
        make(self.region, object::ENDPOINT, self.endpoint);
        make(self.region, object::NOTIFICATION, self.notification);
        make(self.region, object::REPLY, self.reply);
        make(self.region, object::FRAME_4K, self.frame);
        make(self.region, object::PAGE_TABLE, self.table);
        make(self.region, object::PAGE_TABLE, self.spare_table);
        make(self.region, object::PAGE_TABLE, self.space);
        make_many(self.region, object::PAGE_TABLE, self.space_tables, 2);

        // An address space with tables at VA, for mapping into.
        ok(
            "scratch ASID",
            vspace::assign_asid(init_slot::ASID_POOL, self.space),
        );
        for table in [self.space_tables, self.space_tables + 1] {
            ok(
                "scratch table",
                vspace::map_page_table(table, self.space, VA),
            );
        }

        // Sources for the CNode invocations: an endpoint in slot 0 and an
        // unbadged notification in slot 1 of the scratch CNode.
        ok(
            "scratch copy",
            self.inner(0).copy_from(slot(self.endpoint), rights::ALL),
        );
        ok(
            "scratch copy",
            self.inner(1)
                .copy_from(slot(self.notification), rights::ALL),
        );
    }

    fn tally(&mut self, refused: bool) {
        self.cases += 1;
        self.refused += refused as usize;
    }

    /// Run `invocation` on fresh scratch objects.
    fn run(&mut self, invocation: Invocation) {
        self.reset();
        let words = &invocation.words[..invocation.word_count];
        let caps = &invocation.caps[..invocation.cap_count];
        let result = sys::invoke(invocation.target, invocation.label, words, caps);
        self.tally(result.is_err());
    }

    /// Run `invocation` on fresh scratch objects, with a message info that
    /// claims `length` words and `extra` capabilities, whatever it has.
    fn run_lying(&mut self, invocation: Invocation, length: usize, extra: usize) {
        self.reset();
        let buffer = sys::ipc_buffer();
        buffer.msg.fill(0);
        buffer.msg[..invocation.word_count]
            .copy_from_slice(&invocation.words[..invocation.word_count]);
        buffer.caps.fill(self.cnode);
        buffer.caps[..invocation.cap_count]
            .copy_from_slice(&invocation.caps[..invocation.cap_count]);

        let info = MessageInfo::new(invocation.label, extra, length);
        let mut regs = [invocation.target, info.0, 0, 0, 0, 0, 0];
        regs[2..6].copy_from_slice(&buffer.msg[..4]);
        raw_syscall(syscall::CALL, &mut regs);
        self.tally(MessageInfo(regs[1]).label() != 0);
    }

    /// Run system call `number` with `regs` on fresh scratch objects.
    fn run_syscall(&mut self, number: usize, mut regs: [usize; 7]) {
        self.reset();
        let error = raw_syscall(number, &mut regs);
        self.tally(error != 0);
    }
}

/// Make system call `number` with arguments and results in `regs`
/// (`a0`..`a6`); returns what comes back in `a7`.
fn raw_syscall(number: usize, regs: &mut [usize; 7]) -> usize {
    let last: usize;
    // SAFETY: the calls made here write only result registers and this
    // thread's IPC buffer, which nothing else here holds a reference to
    // across the call.
    unsafe {
        core::arch::asm!(
            "ecall",
            inlateout("a0") regs[0] => regs[0], inlateout("a1") regs[1] => regs[1],
            inlateout("a2") regs[2] => regs[2], inlateout("a3") regs[3] => regs[3],
            inlateout("a4") regs[4] => regs[4], inlateout("a5") regs[5] => regs[5],
            inlateout("a6") regs[6] => regs[6], inlateout("a7") number => last,
            options(nostack),
        );
    }
    last
}

/// Every way of varying `invocation` one argument at a time: as given, each
/// word through `EXTREMES`, each capability through `CPTRS` and the scratch
/// objects, shorter messages, and message infos claiming more than there is.
fn sweep(scratch: &mut Scratch, invocation: Invocation) {
    scratch.run(invocation);

    for i in 0..invocation.word_count {
        for value in EXTREMES {
            let mut case = invocation;
            case.words[i] = value;
            scratch.run(case);
        }
    }

    for i in 0..invocation.cap_count {
        for cptr in CPTRS.into_iter().chain(scratch.objects()) {
            let mut case = invocation;
            case.caps[i] = cptr;
            scratch.run(case);
        }
    }

    for word_count in 0..invocation.word_count {
        scratch.run(Invocation {
            word_count,
            ..invocation
        });
    }
    for cap_count in 0..invocation.cap_count {
        scratch.run(Invocation {
            cap_count,
            ..invocation
        });
    }

    let words = invocation.word_count;
    for (length, extra) in [(121, 0), (127, 0), (words, 17), (words, 31), (127, 31)] {
        scratch.run_lying(invocation, length, extra);
    }
}

/// Every invocation, with arguments naming scratch objects.
fn invocations(s: &Scratch) -> [Invocation; 24] {
    let all = rights::ALL;
    let depth = RADIX;
    let registers = [0; reg::COUNT];
    let mut write_registers = [0; MAX_WORDS];
    write_registers[1] = reg::COUNT;
    write_registers[2..].copy_from_slice(&registers);
    let rw = rights::READ | rights::WRITE;

    [
        // CNode, on slots of the scratch CNode.
        Invocation::new(s.cnode, label::CNODE_REVOKE, &[0, depth], &[]),
        Invocation::new(s.cnode, label::CNODE_DELETE, &[0, depth], &[]),
        Invocation::new(s.cnode, label::CNODE_IDENTIFY, &[0, depth], &[]),
        Invocation::new(
            s.cnode,
            label::CNODE_COPY,
            &[2, depth, 0, depth, all],
            &[s.cnode],
        ),
        Invocation::new(
            s.cnode,
            label::CNODE_MINT,
            &[2, depth, 1, depth, all, 5],
            &[s.cnode],
        ),
        Invocation::new(
            s.cnode,
            label::CNODE_MOVE,
            &[2, depth, 0, depth],
            &[s.cnode],
        ),
        // Untyped: an endpoint into slot 2 of the scratch CNode.
        Invocation::new(
            s.region,
            label::UNTYPED_RETYPE,
            &[object::ENDPOINT, 0, 0, 0, 2, 1],
            &[s.cnode],
        ),
        // Thread.
        Invocation::new(
            s.thread,
            label::TCB_CONFIGURE,
            &[VA],
            &[s.cnode, s.space, s.frame],
        ),
        Invocation::new(s.thread, label::TCB_WRITE_REGISTERS, &write_registers, &[]),
        Invocation::new(s.thread, label::TCB_READ_REGISTERS, &[reg::COUNT], &[]),
        Invocation::new(s.thread, label::TCB_RESUME, &[], &[]),
        Invocation::new(s.thread, label::TCB_SUSPEND, &[], &[]),
        Invocation::new(
            s.thread,
            label::TCB_BIND_NOTIFICATION,
            &[],
            &[s.notification],
        ),
        Invocation::new(s.thread, label::TCB_UNBIND_NOTIFICATION, &[], &[]),
        Invocation::new(s.thread, label::TCB_SET_FAULT_HANDLER, &[], &[s.endpoint]),
        Invocation::new(s.thread, label::TCB_SET_PRIORITY, &[0], &[init_slot::TCB]),
        Invocation::new(
            s.thread,
            label::TCB_SET_MC_PRIORITY,
            &[0],
            &[init_slot::TCB],
        ),
        // Address spaces.
        Invocation::new(s.table, label::PAGE_TABLE_MAP, &[VA, 0], &[s.space]),
        Invocation::new(s.table, label::PAGE_TABLE_UNMAP, &[], &[]),
        Invocation::new(s.frame, label::FRAME_MAP, &[VA, rw, 0], &[s.space]),
        Invocation::new(s.frame, label::FRAME_UNMAP, &[], &[]),
        Invocation::new(s.frame, label::FRAME_GET_ADDRESS, &[], &[]),
        Invocation::new(
            init_slot::ASID_POOL,
            label::ASID_POOL_ASSIGN,
            &[],
            &[s.spare_table],
        ),
        // A label no object knows.
        Invocation::new(s.thread, 999, &[], &[]),
    ]
}

/// The system calls that can't block, each argument register in turn
/// through the scratch objects and `CPTRS`, and numbers no call has.
fn system_calls(s: &mut Scratch) {
    let calls = [
        syscall::SIGNAL,
        syscall::POLL,
        syscall::NB_SEND,
        syscall::NB_RECV,
        syscall::REPLY,
    ];
    let info = MessageInfo::new(1, 0, 0).0;
    let cptrs: [usize; 23] = {
        let mut all = [0; 23];
        all[..12].copy_from_slice(&CPTRS);
        all[12..].copy_from_slice(&s.objects());
        all
    };

    for number in calls {
        for a0 in cptrs {
            s.run_syscall(number, [a0, info, 0, 0, 0, 0, s.reply]);
        }
        for a6 in cptrs {
            s.run_syscall(number, [s.endpoint, info, 0, 0, 0, 0, a6]);
        }
        for a1 in EXTREMES {
            s.run_syscall(number, [s.endpoint, a1, 0, 0, 0, 0, s.reply]);
        }
    }

    // Numbers no call has (0x100 and 0x101 are the debug calls).
    for number in [13, 50, 0xff, 0x102, 0x1000, usize::MAX] {
        s.run_syscall(number, [0; 7]);
    }
}

/// Run every case, then check the scratch region can still be used.
pub fn arguments(info: &BootInfo) {
    let mut scratch = Scratch::new(info);

    for invocation in invocations(&scratch) {
        sweep(&mut scratch, invocation);
    }
    system_calls(&mut scratch);

    // Afterwards, the scratch objects come and go as they should.
    scratch.reset();
    let empty = scratch.inner(2).identify();
    fails(
        "the scratch CNode's slot 2 is empty",
        empty,
        sap_rt::abi::error::FAILED_LOOKUP,
    );
    ok("revoke the scratch region", slot(scratch.region).revoke());
    ok("revoke the carve", slot(scratch.parent).revoke());

    let (cases, refused) = (scratch.cases, scratch.refused);
    println!("hostile arguments: {cases} cases answered, {refused} of them refused");
}
