---
status: implemented
summary: How the kernel takes traps on RISC-V today - one direct-mode vector that saves user registers straight into the current thread's TCB (found through sscratch) and kernel-mode traps on the stack, separate Rust handlers for each, a return path that resumes any thread, and periodic timer interrupts through the SBI TIME extension; on the cheri branch, user registers saved as whole CHERI capabilities.
updated: 2026-10-20
code: [kernel/arch/riscv/trap.rs, kernel/arch/riscv/paging.rs, kernel/Cargo.toml, kernel/arch/riscv/timer.rs, kernel/arch/riscv/sbi.rs, kernel/arch/riscv/mod.rs, kernel/main.rs, kernel/syscall.rs, kernel/sched.rs]
sources: ["[[2026-10-20-685b0203-ipc-cost-and-cheri]]", "[[2026-10-08-22c3947b-wiki-setup]]"]
---

# Traps and timer on RISC-V

Part of roadmap milestone M2 ([[roadmap]]): the kernel takes traps from itself and from user mode, handles them, and returns, and it receives periodic timer interrupts. CSR access and trap causes come from the `riscv` crate.

## Trap entry and return

`trap::init` (`kernel/arch/riscv/trap.rs:351`) sets `sscratch` to 0 (firmware may leave it set) and writes `stvec` in direct mode: every trap enters `trap_entry` (line 205). Vectored mode would only skip one dispatch for interrupts, since exceptions, syscalls included, always use the base address, so direct mode stays.

`sscratch` tells the entry code where a trap came from. While a user thread runs it holds the address of that thread's saved registers, the `UserContext` (`trap.rs:66`) at the start of its thread control block ([[kernel-threads]]); while the kernel runs it is 0. `trap_entry` swaps it with `sp` and branches on the result.

**From user mode** (`.Lfrom_user`, `trap.rs:253`): `sp` now points at the thread's `UserContext`. The code saves x1 and x3–x31 there, the user stack pointer from `sscratch` (which also resets `sscratch` to 0, line 273), then `sepc` and `sstatus`. It switches to the top of the kernel stack (line 284) and calls `user_trap` (line 388), which never returns. Because user registers live in the TCB, the kernel stack holds nothing between entries, and one stack serves every thread, as in seL4.

**From the kernel**: the code pushes a `UserContext` on the current stack, calls `kernel_trap` (line 419), restores, and returns to the interrupted kernel code. This happens in the idle loop and the boot checks, the only places the kernel enables interrupts, and on kernel bugs.

**Returning to user mode** (`return_to_user`, `trap.rs:290`): given any thread's `UserContext`, it sets `sscratch` to it (line 324), so the thread's next trap saves into its own TCB, then restores `sstatus`, `sepc` and all registers, the stack pointer last, and executes `sret`. The scheduler calls it, through `arch::return_to_user`, to resume whichever thread it picks.

`UserContext` is 34 saved registers (`Reg`, line 40), checked by a compile-time assertion against the assembly offsets (line 77), and the assembly is commented line by line. It also gives generic code an architecture-neutral view of syscalls: `syscall_number` (`a7`), `arg(i)` and `set_result(i)` (`a0`–`a5`) (lines 109–119). `UserContext::new_user` (line 100) builds a new thread's registers: user mode, interrupts on after `sret`, and the FPU off. A `TODO` there notes the kernel doesn't save floating-point registers on a switch, so a thread that uses the FPU faults until lazy FP save and restore exists.

On trap entry the hardware clears `sstatus.SIE`, so kernel code always runs with interrupts disabled; it is never preempted. In user mode, supervisor interrupts are always taken.

## With CHERI

> [!note] Branch
> This section describes the `cheri` branch: a kernel built with the `cheri` feature for CHERI-RISC-V (the RVY draft, [[hw-cheri]]). Without the feature, the kernel is the one described above.

Built with the `cheri` feature, the kernel saves user registers as whole capabilities, so a capability a user thread holds in a register survives traps and context switches. The kernel itself stays integer code: its Rust code never handles capabilities.

- **Wider slots.** Each saved register is a capability's 16 bytes (`Reg`, `trap.rs:40`): the address, which is all Rust code reads or writes, then the metadata; the tag lives in tag memory. A store from Rust clears the tag, so a result the kernel writes (`set_result`) reaches user mode as an integer. `UserContext` grows from 272 to 544 bytes, which still fits a TCB.
- **Capability mode in the stubs.** `trap_entry` starts with `ymodeswy` (line 215), which needs no register, so the `sscratch` swap moves whole capabilities: in integer mode, a CSR read keeps only the address, and the user stack pointer would lose its metadata. A trap from the kernel switches back to integer mode at once, since kernel registers hold no capabilities. A trap from user mode saves every register and `sepcc` with `sy`, then switches back to integer mode before calling Rust.
- **Return.** `return_to_user` switches to capability mode, makes a capability to the context from DDC (the kernel's is the root) and puts it in `sscratch` (line 300), and restores registers with `ly`. If kernel code changed the saved pc, which every system call does to step past `ecall`, the tag is gone: the code rebuilds it from the stored metadata with `ybld` and DDC as the authority (line 314). A thread the kernel set up has no metadata, and gets DDC with the pc as its address.
- **Enabling it.** `enable_cheri` (line 370) sets `senvcfg.Y` (bit 9, line 364), which lets user mode use CHERI. The bit only sticks when the firmware enabled CHERI for supervisor mode (`menvcfg.Y`), so the same write checks that the kernel runs on a CHERI hart, and boot stops if it doesn't.
- **Page tables.** Storing a tagged capability needs the page's `pte.y` bit (bit 58), which the kernel's writable mappings carry under the feature ([[kernel-memory-riscv]]).

RVY instructions are written as raw encodings (`.insn`) through assembler macros (line 168), since Rust's assembler doesn't know them; the encodings were checked against the RVY LLVM fork's assembler.

`check_cheri` in `check` holds DDC in `s2` through a system call and in `s3` through timer interrupts, and checks both keep their tags, while the system call's `a0` result comes back untagged. Saving with ordinary stores instead makes it fail. The kernel tells user mode that capabilities are on with `BootInfo::cheri` ([[kernel-root-task]]).

> [!todo] Not built
> Every user thread gets DDC and a program counter capability derived from the root, so CHERI adds no protection yet; bounded capabilities per thread come with compartments ([[ipc-paging-vs-cheri]]). User pages can't hold capabilities (no `pte.y`), and the kernel doesn't save DDC or the thread-identifier CSRs per thread.

## Dispatch

`user_trap` (`trap.rs:388`) handles traps from user mode, then calls the scheduler, which picks the thread to resume ([[kernel-threads#Scheduler]]):

| Cause | Handling |
|---|---|
| Supervisor timer interrupt | `timer::on_interrupt`, which also counts the tick against the current thread's time slice |
| Environment call | syscall: `sepc += 4`, then `syscall::handle` with the thread's registers |
| Anything else | a user fault, delivered to the thread's fault handler over IPC ([[kernel-ipc#Fault delivery]]) |


`kernel_trap` (`trap.rs:419`) handles the timer, breakpoints (logged, and `sepc` advanced past the `ebreak` using the instruction's real length, since `ebreak` may be the 2-byte `c.ebreak`), and panics on anything else.

## Timer

`timer::init` (`kernel/arch/riscv/timer.rs:20`) records the timer frequency from the boot information ([[kernel-boot-riscv]]). `kmain` starts the timer at 100 Hz (`TICK_HZ`, `kernel/main.rs:45`) and it runs from then on: `timer::start(hz)` (`timer.rs:38`) computes the interval in timer ticks, programs the first deadline with the SBI TIME extension's `set_timer` (`kernel/arch/riscv/sbi.rs:50`), and enables the supervisor timer interrupt in `sie`.

Each interrupt increments a tick counter, programs the next deadline, which also clears the pending interrupt, and calls `sched::tick` (`timer.rs:61`). `now` (`timer.rs:16`) reads the `time` CSR, which every application-class core has (Zicntr). `init` also lets user mode read it (`scounteren.TM`, `timer.rs:27`), as Linux does, so programs can time things without a system call; the root task learns its frequency from the boot-info page. A `TODO` notes that a configuration with time protection would hide it, since a clock is what timing channels are measured with ([[sel4#Information-flow proof]]). Deadlines passed to `set_timer` are absolute values of the same counter, so the two always agree.

`set_timer` traps into firmware on every tick. A `TODO` at `sbi.rs` marks using the Sstc extension, which lets the kernel write `stimecmp` directly, when the hardware has it ([[hw-isa#Handling extension variation]]).

## Boot check

`boot_checks` (`kernel/main.rs:48`) triggers an `ebreak` and confirms execution resumes, then waits in the idle loop (`arch::idle_once`, `kernel/arch/riscv/mod.rs:95`) for ten ticks, which takes about 100 ms.

> [!todo] Not built
> Floating-point state save and restore; external interrupts (PLIC); fault delivery over IPC.
