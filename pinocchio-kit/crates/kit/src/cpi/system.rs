//! System program CPI. Re-exports every `pinocchio-system` instruction and adds
//! one-line helpers for the calls handlers make constantly.

pub use pinocchio_system::{instructions::*, ID};

use pinocchio::{cpi::Signer, AccountView, ProgramResult};

/// `system_program::transfer` signed by the transaction (`from` is a wallet).
#[inline(always)]
pub fn transfer(from: &AccountView, to: &AccountView, lamports: u64) -> ProgramResult {
    Transfer { from, to, lamports }.invoke()
}

/// `system_program::transfer` where `from` is a **system-owned** PDA.
/// (Program-owned PDAs cannot be debited by the system program — use
/// [`crate::lifecycle::withdraw_lamports`] for those.)
#[inline(always)]
pub fn transfer_signed(
    from: &AccountView,
    to: &AccountView,
    lamports: u64,
    signers: &[Signer],
) -> ProgramResult {
    Transfer { from, to, lamports }.invoke_signed(signers)
}
