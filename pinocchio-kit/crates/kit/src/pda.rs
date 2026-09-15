//! PDA helpers.
//!
//! Rule of thumb (same as Anchor's `bump = stored.bump`):
//! * **Create** with [`find_program_address`] — returns the canonical bump — and store it.
//! * **Verify** later with [`verify_seeds`] using the stored bump: one sha256 instead of
//!   the up-to-255 iterations of `find` (~1500 CU per attempt).
//!
//! Never accept a bump from instruction data without comparing against the canonical
//! one: a non-canonical bump yields a *different* valid PDA (bump-seed canonicalisation).

use {
    crate::errors::KitError,
    pinocchio::{error::ProgramError, AccountView, Address},
};

/// Canonical PDA and bump. On-chain this is the `sol_try_find_program_address` syscall.
#[inline(always)]
pub fn find_program_address(seeds: &[&[u8]], program_id: &Address) -> (Address, u8) {
    Address::find_program_address(seeds, program_id)
}

/// `create_program_address` equivalent without the on-curve syscall. Only sound when
/// `bump` is known canonical (e.g. stored by your own program at init).
#[inline(always)]
pub fn derive_with_bump<const N: usize>(
    seeds: &[&[u8]; N],
    bump: u8,
    program_id: &Address,
) -> Address {
    Address::derive_address(seeds, Some(bump), program_id)
}

/// `seeds = [...], bump = stored_bump`.
#[inline(always)]
pub fn verify_seeds<const N: usize>(
    view: &AccountView,
    seeds: &[&[u8]; N],
    bump: u8,
    program_id: &Address,
) -> Result<(), ProgramError> {
    if view.address() == &derive_with_bump(seeds, bump, program_id) {
        Ok(())
    } else {
        Err(KitError::ConstraintSeeds.into())
    }
}

/// `seeds = [...], bump` (no stored bump): runs `find` and returns the canonical bump.
#[inline(always)]
pub fn verify_canonical(
    view: &AccountView,
    seeds: &[&[u8]],
    program_id: &Address,
) -> Result<u8, ProgramError> {
    let (expected, bump) = find_program_address(seeds, program_id);
    if view.address() == &expected {
        Ok(bump)
    } else {
        Err(KitError::ConstraintSeeds.into())
    }
}

/// Builds the `[Seed; N]` array for `invoke_signed`. Include the bump as the last seed:
///
/// ```ignore
/// let bump = [vault.bump];
/// let seeds = signer_seeds!(b"vault", owner.address().as_ref(), &bump);
/// let signer = SignerSeeds::from(&seeds);
/// Transfer { .. }.invoke_signed(&[signer])?;
/// ```
#[macro_export]
macro_rules! signer_seeds {
    ($($seed:expr),+ $(,)?) => {
        [$( $crate::pinocchio::cpi::Seed::from($seed) ),+]
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_bump_derivation_matches_find() {
        let program = Address::new_from_array([42; 32]);
        let user = [1u8; 32];
        let (pda, bump) = find_program_address(&[b"counter", &user], &program);
        assert_eq!(derive_with_bump(&[b"counter", &user], bump, &program), pda);
        assert_ne!(derive_with_bump(&[b"counter", &user], bump.wrapping_sub(1), &program), pda);
    }
}
