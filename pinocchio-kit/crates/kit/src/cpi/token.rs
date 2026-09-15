//! SPL Token + Token-2022.
//!
//! `pinocchio-token`'s builders are generic over the token program. Instantiate them with
//! [`Legacy`] (Tokenkeg), [`Token2022`] (Tokenz) or pick at runtime with
//! [`transfer_checked`] & co, which take the token program account and dispatch on its
//! address. Always prefer the `*Checked` variants: they bind the mint and decimals,
//! so a lookalike mint cannot be substituted.
//!
//! State loaders here accept both programs (Token-2022 accounts with extensions are
//! longer than 165 bytes), unlike `pinocchio_token::state::*::from_account_view` which
//! only accepts legacy-owned accounts of exact length.

pub use pinocchio_token::{
    instructions::*,
    state::{Account as TokenAccount, AccountState as TokenAccountState, Mint},
    TokenInterface as TokenProgramInterface, ID as TOKEN_ID,
};

use {
    crate::{accounts::AccountChecks, errors::KitError},
    pinocchio::{
        account::Ref, cpi::Signer, error::ProgramError, AccountView, Address, ProgramResult,
    },
};

pub const TOKEN_2022_ID: Address =
    Address::from_str_const("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");

/// Legacy SPL Token only.
pub struct Legacy;
impl TokenProgramInterface for Legacy {
    const ID: Address = TOKEN_ID;
}

/// Token-2022 only.
pub struct Token2022;
impl TokenProgramInterface for Token2022 {
    const ID: Address = TOKEN_2022_ID;
}

const ACCOUNT_TYPE_OFFSET: usize = 165;
const ACCOUNT_TYPE_MINT: u8 = 1;
const ACCOUNT_TYPE_ACCOUNT: u8 = 2;
const TOKEN_ACCOUNT_STATE_OFFSET: usize = 108;

#[inline(always)]
pub fn is_token_program(id: &Address) -> bool {
    id == &TOKEN_ID || id == &TOKEN_2022_ID
}

#[inline(always)]
fn check_token_owner(view: &AccountView) -> Result<(), ProgramError> {
    if is_token_program(view.owner()) {
        Ok(())
    } else {
        Err(KitError::ConstraintTokenTokenProgram.into())
    }
}

/// Interface-aware token account loader with Anchor's `token::mint` / `token::authority`
/// constraints. Pass `None` to skip a constraint.
pub fn load_token_account<'a>(
    view: &'a AccountView,
    mint: Option<&Address>,
    authority: Option<&Address>,
) -> Result<Ref<'a, TokenAccount>, ProgramError> {
    check_token_owner(view)?;
    let len = view.data_len();
    let data = view.try_borrow()?;
    let valid_len = len == TokenAccount::LEN
        || (view.owned_by(&TOKEN_2022_ID)
            && len > ACCOUNT_TYPE_OFFSET
            && data[ACCOUNT_TYPE_OFFSET] == ACCOUNT_TYPE_ACCOUNT);
    if !valid_len || data[TOKEN_ACCOUNT_STATE_OFFSET] == 0 {
        return Err(KitError::AccountNotInitialized.into());
    }
    // SAFETY: at least `TokenAccount::LEN` bytes, layout is the SPL base layout.
    let account = Ref::map(data, |d| unsafe { TokenAccount::from_bytes_unchecked(d) });
    if let Some(m) = mint {
        if account.mint() != m {
            return Err(KitError::ConstraintTokenMint.into());
        }
    }
    if let Some(a) = authority {
        if account.owner() != a {
            return Err(KitError::ConstraintTokenOwner.into());
        }
    }
    Ok(account)
}

/// Interface-aware mint loader.
pub fn load_mint(view: &AccountView) -> Result<Ref<'_, Mint>, ProgramError> {
    check_token_owner(view)?;
    let len = view.data_len();
    let data = view.try_borrow()?;
    let valid_len = len == Mint::LEN
        || (view.owned_by(&TOKEN_2022_ID)
            && len > ACCOUNT_TYPE_OFFSET
            && data[ACCOUNT_TYPE_OFFSET] == ACCOUNT_TYPE_MINT);
    if !valid_len {
        return Err(KitError::AccountDidNotDeserialize.into());
    }
    // SAFETY: at least `Mint::LEN` bytes.
    let mint = Ref::map(data, |d| unsafe { Mint::from_bytes_unchecked(d) });
    if !mint.is_initialized() {
        return Err(KitError::AccountNotInitialized.into());
    }
    Ok(mint)
}

/// `transfer_checked` through whichever token program `token_program` is.
pub fn transfer_checked(
    token_program: &AccountView,
    from: &AccountView,
    mint: &AccountView,
    to: &AccountView,
    authority: &AccountView,
    amount: u64,
    decimals: u8,
    signers: &[Signer],
) -> ProgramResult {
    token_program.check_executable()?;
    if !is_token_program(token_program.address()) {
        return Err(KitError::InvalidProgramId.into());
    }
    TransferChecked::<&AccountView, Legacy>::new(from, mint, to, authority, amount, decimals)
        .invoke_signed_with_unverified_program(signers, token_program.address())
}

/// `mint_to_checked` through whichever token program `token_program` is.
pub fn mint_to_checked(
    token_program: &AccountView,
    mint: &AccountView,
    account: &AccountView,
    mint_authority: &AccountView,
    amount: u64,
    decimals: u8,
    signers: &[Signer],
) -> ProgramResult {
    token_program.check_executable()?;
    if !is_token_program(token_program.address()) {
        return Err(KitError::InvalidProgramId.into());
    }
    MintToChecked::<&AccountView, Legacy>::new(mint, account, mint_authority, amount, decimals)
        .invoke_signed_with_unverified_program(signers, token_program.address())
}

/// `burn_checked` through whichever token program `token_program` is.
pub fn burn_checked(
    token_program: &AccountView,
    account: &AccountView,
    mint: &AccountView,
    authority: &AccountView,
    amount: u64,
    decimals: u8,
    signers: &[Signer],
) -> ProgramResult {
    token_program.check_executable()?;
    if !is_token_program(token_program.address()) {
        return Err(KitError::InvalidProgramId.into());
    }
    BurnChecked::<&AccountView, Legacy>::new(account, mint, authority, amount, decimals)
        .invoke_signed_with_unverified_program(signers, token_program.address())
}

/// `close_account` through whichever token program `token_program` is.
pub fn close_token_account(
    token_program: &AccountView,
    account: &AccountView,
    destination: &AccountView,
    authority: &AccountView,
    signers: &[Signer],
) -> ProgramResult {
    token_program.check_executable()?;
    if !is_token_program(token_program.address()) {
        return Err(KitError::InvalidProgramId.into());
    }
    CloseAccount::<&AccountView, Legacy>::new(account, destination, authority)
        .invoke_signed_with_unverified_program(signers, token_program.address())
}
