//! Error plumbing.
//!
//! * [`KitError`] — framework errors. Codes deliberately reuse Anchor's numbers
//!   (100/2000/3000 ranges) so existing client-side decoders and muscle memory still work.
//! * [`error_code!`] — declare program errors (start at 6000 like Anchor) with a
//!   machine-readable table that the IDL script injects into the generated clients.

use pinocchio::error::ProgramError;

/// Declares a `#[repr(u32)]` error enum, `From<_> for ProgramError`, and an `ERRORS`
/// table `(code, name, message)` consumed by `scripts/generate-idl.mjs`.
///
/// ```ignore
/// pinocchio_kit::error_code! {
///     pub enum CounterError {
///         Overflow = 6000 => "Counter would overflow u64",
///         Unauthorized = 6001 => "Signer is not the counter authority",
///     }
/// }
/// ```
#[macro_export]
macro_rules! error_code {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $( $variant:ident = $code:literal => $msg:literal ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[repr(u32)]
        $vis enum $name {
            $( #[doc = $msg] $variant = $code ),+
        }

        impl $name {
            /// `(code, name, message)` for every variant, in declaration order.
            pub const ERRORS: &'static [(u32, &'static str, &'static str)] =
                &[$( ($code, stringify!($variant), $msg) ),+];

            #[inline(always)]
            pub const fn code(self) -> u32 {
                self as u32
            }

            pub const fn name(self) -> &'static str {
                match self { $( Self::$variant => stringify!($variant) ),+ }
            }

            pub const fn message(self) -> &'static str {
                match self { $( Self::$variant => $msg ),+ }
            }
        }

        impl ::core::convert::From<$name> for $crate::pinocchio::error::ProgramError {
            #[inline(always)]
            fn from(e: $name) -> Self {
                $crate::pinocchio::error::ProgramError::Custom(e as u32)
            }
        }
    };
}

error_code! {
    /// Errors raised by pinocchio-kit itself.
    pub enum KitError {
        InstructionMissing = 100 => "Instruction data is empty; discriminator missing",
        InstructionFallbackNotFound = 101 => "Unknown instruction discriminator",
        InstructionDidNotDeserialize = 102 => "Instruction arguments could not be decoded",

        ConstraintMut = 2000 => "A mut constraint was violated",
        ConstraintHasOne = 2001 => "A has_one constraint was violated",
        ConstraintSigner = 2002 => "A signer constraint was violated",
        ConstraintRaw = 2003 => "A raw constraint was violated",
        ConstraintOwner = 2004 => "An owner constraint was violated",
        ConstraintRentExempt = 2005 => "A rent exemption constraint was violated",
        ConstraintSeeds = 2006 => "A seeds constraint was violated",
        ConstraintExecutable = 2007 => "An executable constraint was violated",
        ConstraintClose = 2011 => "A close constraint was violated",
        ConstraintAddress = 2012 => "An address constraint was violated",
        ConstraintZero = 2013 => "Expected zero account discriminant (account already initialized)",
        ConstraintTokenMint = 2014 => "A token mint constraint was violated",
        ConstraintTokenOwner = 2015 => "A token owner constraint was violated",
        ConstraintSpace = 2019 => "A space constraint was violated",
        ConstraintTokenTokenProgram = 2021 => "A token account token program constraint was violated",
        ConstraintDuplicateMutableAccount = 2040 => "The same mutable account was passed twice",

        RequireViolated = 2500 => "A require expression was violated",
        RequireEqViolated = 2501 => "A require_eq expression was violated",
        RequireKeysEqViolated = 2502 => "A require_keys_eq expression was violated",
        RequireNeqViolated = 2503 => "A require_neq expression was violated",
        RequireKeysNeqViolated = 2504 => "A require_keys_neq expression was violated",
        RequireGtViolated = 2505 => "A require_gt expression was violated",
        RequireGteViolated = 2506 => "A require_gte expression was violated",

        AccountDiscriminatorAlreadySet = 3000 => "The account discriminator was already set",
        AccountDiscriminatorNotFound = 3001 => "No discriminator was found on the account",
        AccountDiscriminatorMismatch = 3002 => "Account discriminator did not match",
        AccountDidNotDeserialize = 3003 => "Failed to deserialize the account",
        AccountDidNotSerialize = 3004 => "Failed to serialize the account",
        AccountNotEnoughKeys = 3005 => "Not enough account keys given to the instruction",
        AccountNotMutable = 3006 => "The given account is not mutable",
        AccountOwnedByWrongProgram = 3007 => "The given account is owned by a different program",
        InvalidProgramId = 3008 => "Program ID was not as expected",
        InvalidProgramExecutable = 3009 => "Program account is not executable",
        AccountNotSigner = 3010 => "The given account did not sign",
        AccountNotSystemOwned = 3011 => "The given account is not owned by the system program",
        AccountNotInitialized = 3012 => "The program expected this account to be already initialized",
        AccountReallocExceedsLimit = 3016 => "Realloc exceeds the 10KiB per-instruction limit",

        ArithmeticOverflow = 4000 => "Checked arithmetic overflowed",
    }
}

/// Shorthand for handlers: `Err(err(KitError::ConstraintSeeds))`.
#[inline(always)]
pub const fn err(e: KitError) -> ProgramError {
    ProgramError::Custom(e as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_unique() {
        let t = KitError::ERRORS;
        for (i, a) in t.iter().enumerate() {
            for b in &t[i + 1..] {
                assert_ne!(a.0, b.0, "{} and {} share a code", a.1, b.1);
            }
        }
    }

    #[test]
    fn maps_to_custom() {
        assert_eq!(
            ProgramError::from(KitError::ConstraintSeeds),
            ProgramError::Custom(2006)
        );
    }
}
