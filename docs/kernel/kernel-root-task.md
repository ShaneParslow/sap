---
status: implemented
summary: The one program the kernel starts by itself, as in seL4 - user/init, built as a static position-independent ELF and embedded in the kernel; how the kernel loads it (contiguous frames, relative relocations, W^X segments, a stack below the top of user space) and enters user mode; and the two temporary debug syscalls it uses until capability invocations exist.
updated: 2026-10-11
verified: 1be5924
code: [kernel/build.rs, kernel/root_task.rs, kernel/syscall.rs, kernel/arch/riscv/trap.rs, kernel/arch/riscv/mod.rs, user/init/main.rs, user/init/build.rs, user/init/.cargo/config.toml]
sources: ["[[2026-10-08-22c3947b-wiki-setup]]"]
---

# The root task

The kernel loads exactly one program, the **root task**, as seL4 does. Every other program is loaded by the root task, which will become the process manager ([[prog-loading]], milestone M7). Today the root task only proves that user mode works. Completes roadmap milestone M2 ([[roadmap]]).

## Building it

The root task is `user/init`, its own crate. The kernel's build script (`kernel/build.rs`) runs Cargo on it, always in release mode, with the build directory inside the kernel's build output, and passes the result's path to the kernel, which embeds it with `include_bytes!` (`kernel/root_task.rs:16`). The script runs Cargo from the init crate's directory so that crate's `.cargo/config.toml` applies, and clears the outer build's compiler flags, which would otherwise override it.

`user/init` is linked as a **static position-independent executable** (`-C relocation-model=pie`, `--pie --no-dynamic-linker`), in line with the decision that all code is position-independent ([[prog-loading]]). Each segment starts on its own page (`-z separate-loadable-segments`), so segments can be mapped with different permissions (`user/init/build.rs`).

## Loading it

`root_task::start` (`kernel/root_task.rs:25`), called at the end of `kmain`:

1. Checks the ELF is a RISC-V position-independent executable.
2. Allocates one contiguous run of frames covering every loadable segment, zeroes it, and copies each segment's file bytes in at its offset; gaps and `.bss` stay zero.
3. Applies the relocations in `.rela.dyn`. All of them must be `R_RISCV_RELATIVE` (store load base + addend), which is all a static position-independent executable has: pointers in statics and the GOT. `user/init` has five.
4. Creates a user address space ([[kernel-memory-riscv#User address spaces]]) and maps each segment at `0x10000` (`LOAD_BASE`, line 20) plus its address, with the segment's permissions. A segment that is both writable and executable is refused. Page 0 and the pages below the load base stay unmapped, so null-pointer accesses fault.
5. Maps a 16 KiB stack ending one page below the top of user space, leaving a guard page.
6. Switches to the new address space and enters user mode at the entry point.

The loader is deliberately narrow: it handles what `user/init` produces and nothing else. General loading, with other formats and interpreters, belongs to the userspace process manager.

## Entering and leaving user mode

`trap::enter_user` (`kernel/arch/riscv/trap.rs:174`) writes a trap frame at the top of the kernel stack with the user's entry point and stack pointer and `sstatus` set for user mode (previous privilege user, interrupts on after `sret`, floating point usable), then jumps into the trap return path. Traps from user mode come back to the top of that same stack ([[kernel-traps-riscv#Trap entry and return]]).

## Debug syscalls

Until capability invocations exist (M3), two temporary calls stand in, like seL4's debug syscalls (`kernel/syscall.rs`). Both are marked `TODO`:

| Number (`a7`) | Call | Effect |
|---|---|---|
| `0x100` | `DEBUG_PUTCHAR(byte)` | prints one byte on the kernel console |
| `0x101` | `DEBUG_HALT(failure)` | logs and shuts down; nonzero reports failure |

Unknown numbers log a warning and return `usize::MAX`. `user/init` prints its greeting one byte at a time through `DEBUG_PUTCHAR` and ends with `DEBUG_HALT`. The greeting is reached through a pointer in a static, so printing it also checks that the relocations were applied.

A fault in user mode is logged and shuts down with failure, since the root task is the only user thread. Tested: a null read (`address 0x8`), a read of a kernel address, and 30 million loop iterations under 100 Hz timer interrupts.

> [!todo] Not built
> Capability invocations replacing the debug calls; the root task's initial capabilities (untyped memory, its own address space and capability table) and boot information, as in seL4's boot info (M3); fault delivery to a handler instead of shutdown.
