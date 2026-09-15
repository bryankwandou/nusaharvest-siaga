//! # pinocchio-kit
//!
//! The pieces Anchor generates for you, written out as small, auditable, `no_std`
//! helpers on top of [Pinocchio](https://github.com/anza-xyz/pinocchio) 0.11.
//!
//! | Anchor                          | pinocchio-kit                                      |
//! |---------------------------------|----------------------------------------------------|
//! | `#[program]` dispatch           | [`instructions!`] + exhaustive `match`             |
//! | `#[derive(Accounts)]`           | `TryFrom<&[AccountView]>` + [`check!`]             |
//! | `Signer`, `Program`, `Account`  | [`accounts::Signer`], [`accounts::Program`], [`accounts::Account`] |
//! | `has_one`, `seeds`, `bump`      | [`has_one!`], [`pda::verify_seeds`]                |
//! | `init`, `init_if_needed`        | [`lifecycle::init`], [`lifecycle::init_if_needed`] |
//! | `close`, `realloc`              | [`lifecycle::close`], [`lifecycle::realloc`]       |
//! | `#[account(zero_copy)]`         | [`impl_account!`] + [`state::load`]                |
//! | `CpiContext`                    | [`cpi::CpiBuilder`] + typed system/token/ATA calls |
//! | `#[error_code]`                 | [`error_code!`]                                    |
//! | `msg!`                          | [`msg!`] (pinocchio-log, no `format!`)             |
#![no_std]

#[cfg(feature = "borsh")]
extern crate alloc;
#[cfg(test)]
extern crate std;

pub mod accounts;
pub mod constraints;
pub mod cpi;
pub mod errors;
pub mod ix;
pub mod lifecycle;
pub mod log;
pub mod pda;
pub mod remaining;
pub mod state;

pub use {pinocchio, pinocchio_log, pinocchio_system, pinocchio_token};

/// Everything a handler usually needs, in one import.
pub mod prelude {
    pub use crate::{
        accounts::{Account, AccountChecks, Program, Signer, SystemAccount, UncheckedAccount},
        check,
        cpi::{AssociatedTokenProgram, SystemProgram, Token2022Program, TokenProgram},
        errors::KitError,
        has_one,
        ix::Reader,
        msg, require, require_eq, require_gt, require_gte, require_keys_eq, require_keys_neq,
        signer_seeds,
        state::{AccountState, Discriminator, PodBool, PodI64, PodU16, PodU32, PodU64},
    };
    pub use pinocchio::{
        cpi::{Seed, Signer as SignerSeeds},
        error::ProgramError,
        sysvars::{clock::Clock, rent::Rent, Sysvar},
        AccountView, Address, ProgramResult,
    };
}
