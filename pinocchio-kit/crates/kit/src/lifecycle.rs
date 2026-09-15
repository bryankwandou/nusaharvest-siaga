//! `init`, `init_if_needed`, `close`, `realloc`.

use {
    crate::{
        accounts::AccountChecks,
        errors::KitError,
        state::{self, AccountState},
    },
    pinocchio::{
        cpi::Signer,
        error::ProgramError,
        sysvars::{rent::Rent, Sysvar},
        AccountView, Address, ProgramResult, Resize,
    },
    pinocchio_system::instructions::{Allocate, Assign, CreateAccount, Transfer},
};

/// Maximum growth per instruction enforced by the runtime.
pub const MAX_PERMITTED_DATA_INCREASE: usize = 10 * 1024;

/// Creates `new_account` with `space` bytes owned by `owner`, funded by `payer`.
///
/// Handles the **pre-funded account** griefing case: if someone already sent lamports to
/// the (predictable) PDA address, `CreateAccount` would fail forever. We instead top up,
/// `Allocate` and `Assign`, which is what Anchor's `init` does.
///
/// `signers` must contain the PDA seeds of `new_account` when it is a PDA.
pub fn create_account(
    payer: &AccountView,
    new_account: &AccountView,
    space: usize,
    owner: &Address,
    signers: &[Signer],
) -> ProgramResult {
    let required = Rent::get()?.try_minimum_balance(space)?;
    let current = new_account.lamports();

    if current == 0 {
        return CreateAccount {
            from: payer,
            to: new_account,
            lamports: required,
            space: space as u64,
            owner,
        }
        .invoke_signed(signers);
    }

    new_account.check_uninitialized()?;
    if required > current {
        Transfer { from: payer, to: new_account, lamports: required - current }.invoke()?;
    }
    Allocate { account: new_account, space: space as u64 }.invoke_signed(signers)?;
    Assign { account: new_account, owner }.invoke_signed(signers)
}

/// Anchor `#[account(init, payer, space = T::LEN, seeds, bump)]`.
///
/// Creates the account, writes `T`'s discriminator and returns so the caller can
/// `state::load_mut` and fill fields. Fails if the account already exists.
pub fn init<T: AccountState>(
    payer: &AccountView,
    new_account: &mut AccountView,
    signers: &[Signer],
) -> ProgramResult {
    payer.check_signer()?;
    payer.check_writable()?;
    new_account.check_writable()?;
    new_account.check_uninitialized()?;
    create_account(payer, new_account, T::LEN, &T::OWNER, signers)?;
    drop(state::load_init::<T>(new_account)?);
    Ok(())
}

/// Anchor `init_if_needed`. Returns `true` when the account was created in this call.
///
/// Only safe when the address is a PDA the caller has already verified with seeds;
/// otherwise an attacker could pass a pre-existing account of the right type.
pub fn init_if_needed<T: AccountState>(
    payer: &AccountView,
    account: &mut AccountView,
    signers: &[Signer],
) -> Result<bool, ProgramError> {
    if account.owned_by(&T::OWNER) {
        account.check_writable()?;
        drop(state::load::<T>(account)?);
        return Ok(false);
    }
    init::<T>(payer, account, signers)?;
    Ok(true)
}

/// Anchor `#[account(mut, close = destination)]`.
///
/// Zeroes the data (so a same-transaction "revive" by refunding rent cannot resurrect
/// the old state), moves every lamport, then resets owner and length.
pub fn close(
    program_id: &Address,
    account: &mut AccountView,
    destination: &mut AccountView,
) -> ProgramResult {
    account.check_owner(program_id)?;
    account.check_writable()?;
    destination.check_writable()?;
    if account.address() == destination.address() {
        return Err(KitError::ConstraintClose.into());
    }

    account.try_borrow_mut()?.fill(0);

    let lamports = account.lamports();
    let credited = destination
        .lamports()
        .checked_add(lamports)
        .ok_or(KitError::ArithmeticOverflow)?;
    destination.set_lamports(credited);
    account.set_lamports(0);
    account.close()
}

/// Anchor `#[account(mut, realloc = new_len, realloc::payer = payer, realloc::zero = true)]`.
///
/// Grows or shrinks a program-owned account, charging `payer` the rent delta via the
/// system program (payer must sign) or refunding surplus lamports directly to it.
/// Newly added bytes are always zeroed.
pub fn realloc(
    program_id: &Address,
    account: &mut AccountView,
    payer: &mut AccountView,
    new_len: usize,
    signers: &[Signer],
) -> ProgramResult {
    account.check_owner(program_id)?;
    account.check_writable()?;
    payer.check_writable()?;

    let old_len = account.data_len();
    if new_len > old_len && new_len - old_len > MAX_PERMITTED_DATA_INCREASE {
        return Err(KitError::AccountReallocExceedsLimit.into());
    }

    let required = Rent::get()?.try_minimum_balance(new_len)?;
    let current = account.lamports();

    if required > current {
        payer.check_signer()?;
        Transfer { from: payer, to: account, lamports: required - current }
            .invoke_signed(signers)?;
    } else if current > required {
        let refund = current - required;
        let credited = payer
            .lamports()
            .checked_add(refund)
            .ok_or(KitError::ArithmeticOverflow)?;
        account.set_lamports(required);
        payer.set_lamports(credited);
    }

    account.resize(new_len)
}

/// Moves lamports out of a program-owned account without CPI, keeping it rent exempt.
pub fn withdraw_lamports(
    program_id: &Address,
    from: &mut AccountView,
    to: &mut AccountView,
    amount: u64,
) -> ProgramResult {
    from.check_owner(program_id)?;
    from.check_writable()?;
    to.check_writable()?;
    if from.address() == to.address() {
        return Err(KitError::ConstraintDuplicateMutableAccount.into());
    }
    let floor = Rent::get()?.try_minimum_balance(from.data_len())?;
    let remaining = from
        .lamports()
        .checked_sub(amount)
        .ok_or(ProgramError::InsufficientFunds)?;
    if remaining < floor {
        return Err(ProgramError::InsufficientFunds);
    }
    let credited = to.lamports().checked_add(amount).ok_or(KitError::ArithmeticOverflow)?;
    from.set_lamports(remaining);
    to.set_lamports(credited);
    Ok(())
}
