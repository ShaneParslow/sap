---
status: implemented
summary: The one program the kernel starts by itself, as in seL4 - user/init, built on the libsap runtime as a static position-independent ELF and embedded in the kernel; how the kernel loads it (contiguous frames, relative relocations, W^X segments, a stack, IPC buffer and boot-info page) and starts it as a thread; and the debug syscalls it still uses.
updated: 2026-10-13
code: [kernel/build.rs, kernel/root_task.rs, kernel/syscall.rs, abi/lib.rs, user/init/main.rs, user/init/build.rs, user/init/.cargo/config.toml, user/libsap/lib.rs, user/libsap/debug.rs]
verified: a7e19d3
sources: ["[[2026-10-08-22c3947b-wiki-setup]]"]
---

# The root task

The kernel loads exactly one program, the **root task**, as seL4 does. Every other program is loaded by the root task, which will become the process manager ([[prog-loading]], milestone M7). Today the root task only proves that user mode works. Completes roadmap milestone M2 ([[roadmap]]).

## Building it

The root task is `user/init`, its own crate. The kernel's build script (`kernel/build.rs`) runs Cargo on it, always in release mode, with the build directory inside the kernel's build output, and passes the result's path to the kernel, which embeds it with `include_bytes!` (`kernel/root_task.rs:25`). The script runs Cargo from the init crate's directory so that crate's `.cargo/config.toml` applies, and clears the outer build's compiler flags, which would otherwise override it.

`user/init` is linked as a **static position-independent executable** (`-C relocation-model=pie`, `--pie --no-dynamic-linker`), in line with the decision that all code is position-independent ([[prog-loading]]). Each segment starts on its own page (`-z separate-loadable-segments`), so segments can be mapped with different permissions. The precompiled `core` library for this target isn't position-independent, so its vtables need relocations in read-only segments; `-z notext` allows that, and the loader applies relocations before mapping anything (`user/init/build.rs`).

It is built on **`libsap`** ([[prog-libsap]]), the user runtime. The kernel starts the root task by the same convention as every other thread: `tp` at its IPC buffer, and here `a0` the boot-info page's address.

## Loading it

`root_task::start` (`kernel/root_task.rs:47`), called at the end of `kmain`:

1. Checks the ELF is a RISC-V position-independent executable.
2. Allocates one contiguous run of frames covering every loadable segment, zeroes it, and copies each segment's file bytes in at its offset; gaps and `.bss` stay zero.
3. Applies the relocations in `.rela.dyn`. All of them must be `R_RISCV_RELATIVE` (store load base + addend), which is all a static position-independent executable has: pointers in statics and the GOT. `user/init` has five.
4. Creates a user address space ([[kernel-memory-riscv#User address spaces]]) and maps each segment at `0x10000` (`LOAD_BASE`, line 29) plus its address, with the segment's permissions. A segment that is both writable and executable is refused. Page 0 and the pages below the load base stay unmapped, so null-pointer accesses fault.
5. Maps a 16 KiB stack ending one page below the top of user space, leaving a guard page, then below it (each after a guard page) the IPC buffer and the read-only boot-info page, and gives the address space ASID 1.
6. Builds the root task's CNode and fills it with capabilities to everything it has and all remaining free memory as untyped memory, described in the boot-info page ([[kernel-capabilities#The root task's capabilities]]).
7. Creates the root task's thread control block in a frame of its own, with copies of its CNode, address space and IPC buffer capabilities in its slots, registers that start it at the entry point on its stack with the boot-info address as its argument, and the highest priority and maximum controlled priority (255), and makes it runnable. `kmain` then starts the scheduler, which runs it ([[kernel-threads]]).

The loader is deliberately narrow: it handles what `user/init` produces and nothing else. General loading, with other formats and interpreters, belongs to the userspace process manager.

## Debug syscalls

Until capability invocations exist (M3), two temporary calls stand in, like seL4's debug syscalls. Their numbers are in `sap-abi` (`abi/lib.rs`), shared with the kernel ([[kernel-threads#Syscalls]]); both are marked `TODO`:

| Number (`a7`) | Call | Effect |
|---|---|---|
| `0x100` | `DEBUG_PUTCHAR(byte)` | prints one byte on the kernel console |
| `0x101` | `DEBUG_HALT(failure)` | logs and shuts down; nonzero reports failure |

Unknown numbers log a warning and return `usize::MAX`. `user/init` prints through `DEBUG_PUTCHAR` (via `libsap`'s console), runs its checks ([[kernel-capabilities#Tests]]), and ends with `DEBUG_HALT`. Its formatting code reaches vtables through relocated pointers, so the output itself shows the relocations were applied.

Faults in other threads go to their fault handlers ([[kernel-ipc#Fault delivery]]); the root task has none, so a fault in it is logged and ends the boot with a failure. Tested: a null read (`address 0x8`), a read of a kernel address, and 30 million loop iterations under 100 Hz timer interrupts.

> [!todo] Not built
> Device untyped memory for the device manager; a log service replacing the debug calls (M7).
