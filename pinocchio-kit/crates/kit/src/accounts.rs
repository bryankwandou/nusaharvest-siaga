//! Account validation: an extension trait with every primitive check, plus typed
//! wrappers equivalent to Anchor's `Signer`, `SystemAccount`, `Program<T>`,
//! `Account<T>` and `UncheckedAccount`.
//!
//! Wrappers hold an [`AccountView`] **by value** (it is a `Copy` pointer into the
//! runtime input buffer). Borrow state lives in the runtime account header, so two
//! wrappers over the same account still cannot hand out overlapping mutable borrows.

use {
    crate::{
        cpi::ProgramId,
        errors::KitError,
        state::{self, AccountState},
    },
    core::{marker::PhantomData, ops::Deref},
    pinocchio::{
        account::{Ref, RefMut},
        error::ProgramError,
        sysvars::{rent::Rent, Sysvar},
        AccountView, Address,
    },
};

type R = Result<(), ProgramError>;

/// Every single-account check Anchor's constraint codegen emits.
pub trait AccountChecks {
    fn check_signer(&self) -> R;
    fn check_writable(&self) -> R;
    fn check_owner(&self, owner: &Address) -> R;
    fn check_address(&self, address: &Address) -> R;
    fn check_executable(&self) -> R;
    /// Address equals `program_id` **and** the account is executable.
    fn check_program(&self, program_id: &Address) -> R;
    fn check_system_owned(&self) -> R;
    /// Uninitialised: system-owned with no data (safe target for `init`).
    fn check_uninitialized(&self) -> R;
    fn check_min_data_len(&self, len: usize) -> R;
    /// Lamports cover rent for the current data length (reads the Rent sysvar).
    fn check_rent_exempt(&self) -> R;
    fn check_rent_exempt_with(&self, rent: &Rent) -> R;
}

impl AccountChecks for AccountView {
    #[inline(always)]
    fn check_signer(&self) -> R {
        if self.is_signer() { Ok(()) } else { Err(KitError::AccountNotSigner.into()) }
    }

    #[inline(always)]
    fn check_writable(&self) -> R {
        if self.is_writable() { Ok(()) } else { Err(KitError::AccountNotMutable.into()) }
    }

    #[inline(always)]
    fn check_owner(&self, owner: &Address) -> R {
        if self.owned_by(owner) { Ok(()) } else { Err(KitError::AccountOwnedByWrongProgram.into()) }
    }

    #[inline(always)]
    fn check_address(&self, address: &Address) -> R {
        if self.address() == address { Ok(()) } else { Err(KitError::ConstraintAddress.into()) }
    }

    #[inline(always)]
    fn check_executable(&self) -> R {
        if self.executable() { Ok(()) } else { Err(KitError::InvalidProgramExecutable.into()) }
    }

    #[inline(always)]
    fn check_program(&self, program_id: &Address) -> R {
        if self.address() != program_id {
            return Err(KitError::InvalidProgramId.into());
        }
        self.check_executable()
    }

    #[inline(always)]
    fn check_system_owned(&self) -> R {
        if self.owned_by(&pinocchio_system::ID) {
            Ok(())
        } else {
            Err(KitError::AccountNotSystemOwned.into())
        }
    }

    #[inline(always)]
    fn check_uninitialized(&self) -> R {
        self.check_system_owned()?;
        if self.is_data_empty() { Ok(()) } else { Err(KitError::ConstraintZero.into()) }
    }

    #[inline(always)]
    fn check_min_data_len(&self, len: usize) -> R {
        if self.data_len() >= len { Ok(()) } else { Err(KitError::ConstraintSpace.into()) }
    }

    #[inline(always)]
    fn check_rent_exempt(&self) -> R {
        self.check_rent_exempt_with(&Rent::get()?)
    }

    #[inline(always)]
    fn check_rent_exempt_with(&self, rent: &Rent) -> R {
        if rent.is_exempt(self.lamports(), self.data_len()) {
            Ok(())
        } else {
            Err(KitError::ConstraintRentExempt.into())
        }
    }
}

macro_rules! view_wrapper {
    ($name:ident $(<$p:ident>)?) => {
        impl$(<$p>)? Deref for $name$(<$p>)? {
            type Target = AccountView;
            #[inline(always)]
            fn deref(&self) -> &AccountView {
                &self.view
            }
        }

        impl$(<$p>)? AsRef<AccountView> for $name$(<$p>)? {
            #[inline(always)]
            fn as_ref(&self) -> &AccountView {
                &self.view
            }
        }

        impl$(<$p>)? $name$(<$p>)? {
            /// Mutable handle for lamport moves / borrows.
            #[inline(always)]
            pub fn view_mut(&mut self) -> &mut AccountView {
                &mut self.view
            }

            #[inline(always)]
            pub fn into_view(self) -> AccountView {
                self.view
            }
        }
    };
}

/// Anchor `Signer<'info>`.
pub struct Signer {
    view: AccountView,
}
view_wrapper!(Signer);

impl Signer {
    #[inline(always)]
    pub fn try_from_view(view: &AccountView) -> Result<Self, ProgramError> {
        view.check_signer()?;
        Ok(Self { view: *view })
    }

    /// `#[account(mut)] signer` — typically the payer.
    #[inline(always)]
    pub fn try_from_view_mut(view: &AccountView) -> Result<Self, ProgramError> {
        view.check_signer()?;
        view.check_writable()?;
        Ok(Self { view: *view })
    }
}

/// Anchor `SystemAccount<'info>`: owned by the system program.
pub struct SystemAccount {
    view: AccountView,
}
view_wrapper!(SystemAccount);

impl SystemAccount {
    #[inline(always)]
    pub fn try_from_view(view: &AccountView) -> Result<Self, ProgramError> {
        view.check_system_owned()?;
        Ok(Self { view: *view })
    }
}

/// Anchor `UncheckedAccount<'info>`. Constructing one is a visible, greppable
/// statement that validation happens elsewhere — add a `/// CHECK:` comment.
pub struct UncheckedAccount {
    view: AccountView,
}
view_wrapper!(UncheckedAccount);

impl UncheckedAccount {
    #[inline(always)]
    pub fn new(view: &AccountView) -> Self {
        Self { view: *view }
    }
}

/// Anchor `Program<'info, P>`: address matches `P::ID` and account is executable.
pub struct Program<P: ProgramId> {
    view: AccountView,
    _p: PhantomData<P>,
}

impl<P: ProgramId> Deref for Program<P> {
    type Target = AccountView;
    #[inline(always)]
    fn deref(&self) -> &AccountView {
        &self.view
    }
}

impl<P: ProgramId> Program<P> {
    #[inline(always)]
    pub fn try_from_view(view: &AccountView) -> Result<Self, ProgramError> {
        P::verify(view.address())?;
        view.check_executable()?;
        Ok(Self { view: *view, _p: PhantomData })
    }
}

/// Anchor `Account<'info, T>` / `AccountLoader<'info, T>` (zero-copy).
///
/// Construction validates owner, size and discriminator once; `load`/`load_mut`
/// re-check the discriminator cheaply and hand out runtime-tracked borrows.
pub struct Account<T: AccountState> {
    view: AccountView,
    _t: PhantomData<T>,
}

impl<T: AccountState> Deref for Account<T> {
    type Target = AccountView;
    #[inline(always)]
    fn deref(&self) -> &AccountView {
        &self.view
    }
}

impl<T: AccountState> AsRef<AccountView> for Account<T> {
    #[inline(always)]
    fn as_ref(&self) -> &AccountView {
        &self.view
    }
}

impl<T: AccountState> Account<T> {
    #[inline(always)]
    pub fn try_from_view(view: &AccountView) -> Result<Self, ProgramError> {
        drop(state::load::<T>(view)?);
        Ok(Self { view: *view, _t: PhantomData })
    }

    /// `#[account(mut)]`.
    #[inline(always)]
    pub fn try_from_view_mut(view: &AccountView) -> Result<Self, ProgramError> {
        view.check_writable()?;
        Self::try_from_view(view)
    }

    #[inline(always)]
    pub fn load(&self) -> Result<Ref<'_, T>, ProgramError> {
        state::load::<T>(&self.view)
    }

    #[inline(always)]
    pub fn load_mut(&mut self) -> Result<RefMut<'_, T>, ProgramError> {
        state::load_mut::<T>(&mut self.view)
    }

    #[inline(always)]
    pub fn view_mut(&mut self) -> &mut AccountView {
        &mut self.view
    }
}

/// Destructures the next `N` accounts, failing with `NotEnoughAccountKeys` otherwise.
/// Returns the fixed accounts and the remaining ones.
#[inline(always)]
pub fn split_accounts<const N: usize>(
    accounts: &[AccountView],
) -> Result<(&[AccountView; N], &[AccountView]), ProgramError> {
    if accounts.len() < N {
        return Err(ProgramError::NotEnoughAccountKeys);
    }
    let (head, tail) = accounts.split_at(N);
    // SAFETY: `head.len() == N`.
    Ok((unsafe { &*(head.as_ptr() as *const [AccountView; N]) }, tail))
}
