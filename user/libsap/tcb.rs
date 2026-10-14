//! Thread (TCB) invocations.

use sap_abi::{label, reg};

use crate::sys::{invoke, Error};

/// Give the thread at `tcb` a capability space, an address space and
/// optionally an IPC buffer frame mapped at `buffer_va`.
pub fn configure(tcb: usize, cspace: usize, vspace: usize, buffer: Option<(usize, usize)>) -> Result<(), Error> {
    match buffer {
        Some((frame, va)) => invoke(tcb, label::TCB_CONFIGURE, &[va], &[cspace, vspace, frame]),
        None => invoke(tcb, label::TCB_CONFIGURE, &[0], &[cspace, vspace]),
    }
    .map(drop)
}

/// Registers in `sap_abi::reg` order.
pub type Registers = [usize; reg::COUNT];

/// Set the thread's registers, and start it if `resume`.
pub fn write_registers(tcb: usize, resume: bool, regs: &Registers) -> Result<(), Error> {
    let mut words = [0; 2 + reg::COUNT];
    words[0] = resume as usize;
    words[1] = reg::COUNT;
    words[2..].copy_from_slice(regs);
    invoke(tcb, label::TCB_WRITE_REGISTERS, &words, &[]).map(drop)
}

pub fn read_registers(tcb: usize) -> Result<Registers, Error> {
    let first = invoke(tcb, label::TCB_READ_REGISTERS, &[reg::COUNT], &[])?;
    let mut regs = [0; reg::COUNT];
    regs[..first.len()].copy_from_slice(&first);
    let buffer = crate::sys::ipc_buffer();
    regs[first.len()..].copy_from_slice(&buffer.msg[first.len()..reg::COUNT]);
    Ok(regs)
}

pub fn resume(tcb: usize) -> Result<(), Error> {
    invoke(tcb, label::TCB_RESUME, &[], &[]).map(drop)
}

pub fn suspend(tcb: usize) -> Result<(), Error> {
    invoke(tcb, label::TCB_SUSPEND, &[], &[]).map(drop)
}

/// Set the thread's priority, with `authority`'s maximum controlled
/// priority as the limit.
pub fn set_priority(tcb: usize, authority: usize, priority: usize) -> Result<(), Error> {
    invoke(tcb, label::TCB_SET_PRIORITY, &[priority], &[authority]).map(drop)
}

pub fn set_mc_priority(tcb: usize, authority: usize, mcp: usize) -> Result<(), Error> {
    invoke(tcb, label::TCB_SET_MC_PRIORITY, &[mcp], &[authority]).map(drop)
}
