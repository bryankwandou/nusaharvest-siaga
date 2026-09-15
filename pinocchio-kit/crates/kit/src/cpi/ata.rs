//! Associated Token Account program, hand-rolled on [`CpiBuilder`] — which doubles as
//! the reference for writing a typed CPI wrapper for any program without a Pinocchio crate.

use {
    super::{token::is_token_program, CpiBuilder},
    crate::{accounts::AccountChecks, errors::KitError},
    pinocchio::{cpi::Signer, error::ProgramError, AccountView, Address, ProgramResult},
};

pub const ID: Address = Address::from_str_const("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");

const CREATE: u8 = 0;
const CREATE_IDEMPOTENT: u8 = 1;

/// Canonical ATA address for `(wallet, token_program, mint)`.
#[inline(always)]
pub fn get_associated_token_address(
    wallet: &Address,
    mint: &Address,
    token_program: &Address,
) -> (Address, u8) {
    crate::pda::find_program_address(&[wallet.as_ref(), token_program.as_ref(), mint.as_ref()], &ID)
}

/// Anchor `associated_token::mint = mint, associated_token::authority = wallet`.
pub fn check_associated_token_address(
    ata: &AccountView,
    wallet: &Address,
    mint: &Address,
    token_program: &Address,
) -> Result<(), ProgramError> {
    if ata.address() == &get_associated_token_address(wallet, mint, token_program).0 {
        Ok(())
    } else {
        Err(KitError::ConstraintAddress.into())
    }
}

/// `CreateIdempotent` / `Create` accounts, in program order.
pub struct CreateAssociatedTokenAccount<'a> {
    pub payer: &'a AccountView,
    pub associated_token: &'a AccountView,
    pub wallet: &'a AccountView,
    pub mint: &'a AccountView,
    pub system_program: &'a AccountView,
    pub token_program: &'a AccountView,
    /// `true` = no-op if it already exists (Anchor `init_if_needed`).
    pub idempotent: bool,
}

impl CreateAssociatedTokenAccount<'_> {
    pub fn invoke(&self) -> ProgramResult {
        self.invoke_signed(&[])
    }

    /// `signers` is only needed when `payer` is a PDA.
    pub fn invoke_signed(&self, signers: &[Signer]) -> ProgramResult {
        if !is_token_program(self.token_program.address()) {
            return Err(KitError::InvalidProgramId.into());
        }
        self.system_program.check_address(&pinocchio_system::ID)?;
        let data = [if self.idempotent { CREATE_IDEMPOTENT } else { CREATE }];
        CpiBuilder::new(&ID)
            .account(self.payer, true, true)?
            .account(self.associated_token, true, false)?
            .account(self.wallet, false, false)?
            .account(self.mint, false, false)?
            .account(self.system_program, false, false)?
            .account(self.token_program, false, false)?
            .invoke_signed(&data, signers)
    }
}
