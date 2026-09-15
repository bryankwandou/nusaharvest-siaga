//! `ctx.remaining_accounts` equivalents plus the duplicate-account guard Anchor adds
//! for `mut` accounts.

use {
    crate::{
        accounts::AccountChecks,
        errors::KitError,
        state::{self, AccountState},
    },
    pinocchio::{error::ProgramError, AccountView, Address},
};

/// Iterator over trailing accounts with typed, validated accessors.
pub struct RemainingAccounts<'a> {
    accounts: &'a [AccountView],
}

impl<'a> RemainingAccounts<'a> {
    #[inline(always)]
    pub const fn new(accounts: &'a [AccountView]) -> Self {
        Self { accounts }
    }

    #[inline(always)]
    pub const fn len(&self) -> usize {
        self.accounts.len()
    }

    #[inline(always)]
    pub const fn is_empty(&self) -> bool {
        self.accounts.is_empty()
    }

    #[inline(always)]
    pub fn as_slice(&self) -> &'a [AccountView] {
        self.accounts
    }

    #[inline(always)]
    pub fn next(&mut self) -> Result<&'a AccountView, ProgramError> {
        let (first, rest) = self
            .accounts
            .split_first()
            .ok_or(ProgramError::NotEnoughAccountKeys)?;
        self.accounts = rest;
        Ok(first)
    }

    #[inline(always)]
    pub fn next_signer(&mut self) -> Result<&'a AccountView, ProgramError> {
        let v = self.next()?;
        v.check_signer()?;
        Ok(v)
    }

    /// Next account, writable and holding a valid `T`.
    #[inline(always)]
    pub fn next_state_mut<T: AccountState>(&mut self) -> Result<AccountView, ProgramError> {
        let v = self.next()?;
        v.check_writable()?;
        drop(state::load::<T>(v)?);
        Ok(*v)
    }

    /// Fixed-size groups, e.g. `(source, destination)` pairs.
    #[inline(always)]
    pub fn chunks<const N: usize>(&self) -> Result<core::slice::ChunksExact<'a, AccountView>, ProgramError> {
        if N == 0 || self.accounts.len() % N != 0 {
            return Err(ProgramError::NotEnoughAccountKeys);
        }
        Ok(self.accounts.chunks_exact(N))
    }
}

/// Rejects the same address appearing twice. Mandatory whenever two `mut` accounts of
/// the same type are debited/credited (e.g. `from == to` double-counting attacks).
/// O(n²) — intended for small n.
#[inline]
pub fn ensure_unique(accounts: &[&AccountView]) -> Result<(), ProgramError> {
    for (i, a) in accounts.iter().enumerate() {
        for b in &accounts[i + 1..] {
            if a.address() == b.address() {
                return Err(KitError::ConstraintDuplicateMutableAccount.into());
            }
        }
    }
    Ok(())
}

/// Same as [`ensure_unique`] for a slice of views (remaining accounts).
#[inline]
pub fn ensure_unique_slice(accounts: &[AccountView]) -> Result<(), ProgramError> {
    for (i, a) in accounts.iter().enumerate() {
        let addr: &Address = a.address();
        if accounts[i + 1..].iter().any(|b| b.address() == addr) {
            return Err(KitError::ConstraintDuplicateMutableAccount.into());
        }
    }
    Ok(())
}
