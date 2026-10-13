//! Address-space invocations (RISC-V): page tables, frames, ASIDs.

use sap_abi::label;

use crate::sys::{invoke, Error};

/// Install the page table at `page_table` in address space `vspace`, at the
/// first level below the root that has no table covering `va`.
pub fn map_page_table(page_table: usize, vspace: usize, va: usize) -> Result<(), Error> {
    invoke(page_table, label::PAGE_TABLE_MAP, &[va, 0], &[vspace]).map(drop)
}

pub fn unmap_page_table(page_table: usize) -> Result<(), Error> {
    invoke(page_table, label::PAGE_TABLE_UNMAP, &[], &[]).map(drop)
}

/// Map the frame at `frame` at `va` in `vspace` with `rights`
/// (`sap_abi::rights`) and `attributes` (`sap_abi::attr`).
pub fn map_frame(frame: usize, vspace: usize, va: usize, rights: usize, attributes: usize) -> Result<(), Error> {
    invoke(frame, label::FRAME_MAP, &[va, rights, attributes], &[vspace]).map(drop)
}

pub fn unmap_frame(frame: usize) -> Result<(), Error> {
    invoke(frame, label::FRAME_UNMAP, &[], &[]).map(drop)
}

/// The frame's physical address.
pub fn frame_address(frame: usize) -> Result<usize, Error> {
    invoke(frame, label::FRAME_GET_ADDRESS, &[], &[]).map(|r| r[0])
}

/// Make the unmapped page table at `page_table` the root of a new address
/// space, using the ASID pool at `pool`.
pub fn assign_asid(pool: usize, page_table: usize) -> Result<(), Error> {
    invoke(pool, label::ASID_POOL_ASSIGN, &[], &[page_table]).map(drop)
}
