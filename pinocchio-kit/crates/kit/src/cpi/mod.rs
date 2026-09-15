//! Typed CPI.
//!
//! * [`ProgramId`] marker types (`SystemProgram`, `TokenProgram`, `Token2022Program`,
//!   `AssociatedTokenProgram`) power [`crate::accounts::Program`].
//! * [`CpiBuilder`] is the `CpiContext` analogue for any program you have no crate for:
//!   account metas are derived from the views you pass, so the metas and the account
//!   array can never drift out of order.
//! * [`system`], [`token`] and [`ata`] wrap the common instructions.

pub mod ata;
pub mod system;
pub mod token;

use {
    crate::errors::KitError,
    core::mem::MaybeUninit,
    pinocchio::{
        cpi::{invoke_signed_with_bounds, Signer},
        error::ProgramError,
        instruction::{InstructionAccount, InstructionView},
        AccountView, Address, ProgramResult,
    },
};

/// Compile-time program identity.
pub trait ProgramId {
    const ID: Address;

    #[inline(always)]
    fn verify(address: &Address) -> Result<(), ProgramError> {
        if address == &Self::ID {
            Ok(())
        } else {
            Err(KitError::InvalidProgramId.into())
        }
    }
}

pub struct SystemProgram;
impl ProgramId for SystemProgram {
    const ID: Address = pinocchio_system::ID;
}

pub struct TokenProgram;
impl ProgramId for TokenProgram {
    const ID: Address = pinocchio_token::ID;
}

pub struct Token2022Program;
impl ProgramId for Token2022Program {
    const ID: Address = token::TOKEN_2022_ID;
}

/// Anchor `Interface<'info, TokenInterface>`: accepts SPL Token **or** Token-2022.
pub struct TokenInterface;
impl ProgramId for TokenInterface {
    const ID: Address = pinocchio_token::ID;

    #[inline(always)]
    fn verify(address: &Address) -> Result<(), ProgramError> {
        if address == &pinocchio_token::ID || address == &token::TOKEN_2022_ID {
            Ok(())
        } else {
            Err(KitError::InvalidProgramId.into())
        }
    }
}

pub struct AssociatedTokenProgram;
impl ProgramId for AssociatedTokenProgram {
    const ID: Address = ata::ID;
}

/// Max accounts a [`CpiBuilder`] can hold (stack-allocated).
pub const MAX_CPI_ACCOUNTS: usize = 16;

/// Generic CPI builder.
///
/// ```ignore
/// CpiBuilder::new(&memo_program::ID)
///     .account(signer, false, true)?
///     .invoke_signed(b"hello", &[])?;
/// ```
pub struct CpiBuilder<'a> {
    program_id: &'a Address,
    metas: [MaybeUninit<InstructionAccount<'a>>; MAX_CPI_ACCOUNTS],
    views: [MaybeUninit<&'a AccountView>; MAX_CPI_ACCOUNTS],
    len: usize,
}

impl<'a> CpiBuilder<'a> {
    #[inline(always)]
    pub fn new(program_id: &'a Address) -> Self {
        Self {
            program_id,
            metas: [const { MaybeUninit::uninit() }; MAX_CPI_ACCOUNTS],
            views: [const { MaybeUninit::uninit() }; MAX_CPI_ACCOUNTS],
            len: 0,
        }
    }

    /// Appends an account. Requesting `writable`/`signer` that the outer transaction did
    /// not grant fails here with a clear error instead of an opaque runtime privilege
    /// escalation failure (PDA signers are granted by `invoke_signed`, so pass
    /// `signer = true` only together with its seeds).
    #[inline(always)]
    pub fn account(mut self, view: &'a AccountView, writable: bool, signer: bool) -> Result<Self, ProgramError> {
        if self.len == MAX_CPI_ACCOUNTS {
            return Err(ProgramError::InvalidArgument);
        }
        if writable && !view.is_writable() {
            return Err(KitError::AccountNotMutable.into());
        }
        self.metas[self.len].write(InstructionAccount::new(view.address(), writable, signer));
        self.views[self.len].write(view);
        self.len += 1;
        Ok(self)
    }

    #[inline(always)]
    pub fn invoke(&self, data: &[u8]) -> ProgramResult {
        self.invoke_signed(data, &[])
    }

    #[inline(always)]
    pub fn invoke_signed(&self, data: &[u8], signers: &[Signer]) -> ProgramResult {
        // SAFETY: the first `len` entries of both arrays were initialised by `account`.
        let metas = unsafe {
            core::slice::from_raw_parts(self.metas.as_ptr() as *const InstructionAccount, self.len)
        };
        let views = unsafe {
            core::slice::from_raw_parts(self.views.as_ptr() as *const &AccountView, self.len)
        };
        let instruction = InstructionView { program_id: self.program_id, accounts: metas, data };
        invoke_signed_with_bounds::<MAX_CPI_ACCOUNTS, _>(&instruction, views, signers)
    }
}
