//! Instruction decoding: discriminators, a bounds-checked little-endian reader and
//! the [`instructions!`] macro that gives you an exhaustive, type-safe dispatch enum.

use {
    crate::errors::KitError,
    pinocchio::{error::ProgramError, Address},
};

const BAD_DATA: ProgramError = ProgramError::Custom(KitError::InstructionDidNotDeserialize as u32);

/// Declares a `#[repr(u8)]` instruction enum with `TryFrom<u8>` and `decode`.
///
/// Routing with an exhaustive `match` on the enum means adding a variant without a
/// handler is a compile error — the Pinocchio equivalent of Anchor's `#[program]`.
///
/// ```ignore
/// pinocchio_kit::instructions! {
///     pub enum Ix {
///         InitializeCounter = 0,
///         Increment = 1,
///     }
/// }
/// ```
#[macro_export]
macro_rules! instructions {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident { $( $variant:ident = $disc:literal ),+ $(,)? }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[repr(u8)]
        $vis enum $name { $( $variant = $disc ),+ }

        impl $name {
            pub const ALL: &'static [$name] = &[$( $name::$variant ),+];

            /// Splits `data` into `(instruction, args)`.
            #[inline(always)]
            pub fn decode(data: &[u8]) -> ::core::result::Result<(Self, &[u8]), $crate::pinocchio::error::ProgramError> {
                let (disc, rest) = data
                    .split_first()
                    .ok_or($crate::errors::KitError::InstructionMissing)?;
                Ok((Self::try_from(*disc)?, rest))
            }
        }

        impl ::core::convert::TryFrom<u8> for $name {
            type Error = $crate::pinocchio::error::ProgramError;
            #[inline(always)]
            fn try_from(v: u8) -> ::core::result::Result<Self, Self::Error> {
                match v {
                    $( $disc => Ok(Self::$variant), )+
                    _ => Err($crate::errors::KitError::InstructionFallbackNotFound.into()),
                }
            }
        }
    };
}

/// Anchor-compatible 8-byte discriminator: `sha256("<namespace>:<name>")[..8]`,
/// computed at compile time. Use `"global"` for instructions and `"account"` for state.
#[cfg(feature = "anchor-discriminator")]
pub const fn anchor_discriminator(namespace: &str, name: &str) -> [u8; 8] {
    let hash = sha2_const_stable::Sha256::new()
        .update(namespace.as_bytes())
        .update(b":")
        .update(name.as_bytes())
        .finalize();
    let mut out = [0u8; 8];
    let mut i = 0;
    while i < 8 {
        out[i] = hash[i];
        i += 1;
    }
    out
}

/// Zero-copy, bounds-checked little-endian cursor over instruction data.
/// The wire format of fixed-size fields is identical to Borsh, so Shank/Codama clients
/// encode arguments correctly without any custom serializer.
pub struct Reader<'a> {
    data: &'a [u8],
}

impl<'a> Reader<'a> {
    #[inline(always)]
    pub const fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    #[inline(always)]
    pub fn take<const N: usize>(&mut self) -> Result<&'a [u8; N], ProgramError> {
        if self.data.len() < N {
            return Err(BAD_DATA);
        }
        let (head, tail) = self.data.split_at(N);
        self.data = tail;
        // SAFETY: `head.len() == N` was checked above.
        Ok(unsafe { &*(head.as_ptr() as *const [u8; N]) })
    }

    #[inline(always)]
    pub fn bytes(&mut self, len: usize) -> Result<&'a [u8], ProgramError> {
        if self.data.len() < len {
            return Err(BAD_DATA);
        }
        let (head, tail) = self.data.split_at(len);
        self.data = tail;
        Ok(head)
    }

    /// Borsh `Vec<u8>` / `String` layout: `u32` length prefix + bytes.
    #[inline(always)]
    pub fn vec_u8(&mut self) -> Result<&'a [u8], ProgramError> {
        let len = self.u32()? as usize;
        self.bytes(len)
    }

    #[inline(always)]
    pub fn u8(&mut self) -> Result<u8, ProgramError> {
        Ok(self.take::<1>()?[0])
    }

    /// Strict: only `0` and `1` are accepted.
    #[inline(always)]
    pub fn bool(&mut self) -> Result<bool, ProgramError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(BAD_DATA),
        }
    }

    #[inline(always)]
    pub fn u16(&mut self) -> Result<u16, ProgramError> {
        Ok(u16::from_le_bytes(*self.take()?))
    }

    #[inline(always)]
    pub fn u32(&mut self) -> Result<u32, ProgramError> {
        Ok(u32::from_le_bytes(*self.take()?))
    }

    #[inline(always)]
    pub fn u64(&mut self) -> Result<u64, ProgramError> {
        Ok(u64::from_le_bytes(*self.take()?))
    }

    #[inline(always)]
    pub fn i64(&mut self) -> Result<i64, ProgramError> {
        Ok(i64::from_le_bytes(*self.take()?))
    }

    #[inline(always)]
    pub fn u128(&mut self) -> Result<u128, ProgramError> {
        Ok(u128::from_le_bytes(*self.take()?))
    }

    #[inline(always)]
    pub fn address(&mut self) -> Result<Address, ProgramError> {
        Ok(Address::new_from_array(*self.take::<32>()?))
    }

    #[inline(always)]
    pub fn remaining(&self) -> &'a [u8] {
        self.data
    }

    /// Rejects trailing bytes — prevents two encodings of the same instruction.
    #[inline(always)]
    pub fn finish(self) -> Result<(), ProgramError> {
        if self.data.is_empty() {
            Ok(())
        } else {
            Err(BAD_DATA)
        }
    }
}

/// Typed instruction arguments. Implement `parse`; `from_bytes` enforces no trailing data.
pub trait InstructionArgs<'a>: Sized {
    fn parse(reader: &mut Reader<'a>) -> Result<Self, ProgramError>;

    #[inline(always)]
    fn from_bytes(data: &'a [u8]) -> Result<Self, ProgramError> {
        let mut reader = Reader::new(data);
        let args = Self::parse(&mut reader)?;
        reader.finish()?;
        Ok(args)
    }
}

impl InstructionArgs<'_> for () {
    fn parse(_: &mut Reader<'_>) -> Result<Self, ProgramError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    crate::instructions! {
        enum Ix { A = 0, B = 7 }
    }

    #[test]
    fn decodes_and_rejects_unknown() {
        assert_eq!(Ix::decode(&[7, 1]).unwrap().0, Ix::B);
        assert!(Ix::decode(&[]).is_err());
        assert_eq!(
            Ix::decode(&[3]).unwrap_err(),
            ProgramError::Custom(KitError::InstructionFallbackNotFound as u32)
        );
    }

    #[test]
    fn reader_bounds_and_trailing() {
        let data = 42u64.to_le_bytes();
        let mut r = Reader::new(&data);
        assert_eq!(r.u64().unwrap(), 42);
        assert!(r.u8().is_err());

        let mut r = Reader::new(&[1, 2]);
        assert!(r.bool().is_ok());
        assert!(r.finish().is_err());

        assert!(Reader::new(&[2]).bool().is_err());
    }

    #[cfg(feature = "anchor-discriminator")]
    #[test]
    fn anchor_sighash_matches_known_value() {
        // sha256("global:initialize")[..8], the discriminator every Anchor tutorial shows.
        assert_eq!(
            anchor_discriminator("global", "initialize"),
            [175, 175, 109, 31, 13, 152, 155, 237]
        );
    }
}
