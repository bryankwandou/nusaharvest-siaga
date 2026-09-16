//! NusaHarvest Siaga Tanam: rainfall-triggered parametric payout, pure Pinocchio.
//!
//! See `README.md` for the instruction set, account layouts and the deviations
//! from `research/02-FLOW-DAN-SKEMA-FINAL.md`.
#![cfg_attr(any(target_os = "solana", target_arch = "bpf"), no_std)]

pub mod error;
pub mod logic;
pub mod merkle;
pub mod processor;
pub mod state;

#[cfg(any(target_os = "solana", target_arch = "bpf"))]
mod entry {
    use pinocchio::{no_allocator, nostd_panic_handler, program_entrypoint};
    program_entrypoint!(crate::processor::process_instruction);
    no_allocator!();
    nostd_panic_handler!();
}
