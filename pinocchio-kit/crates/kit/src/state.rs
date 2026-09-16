//! Account state: discriminators, zero-copy loading, (de)serialization traits, space.
//!
//! ## Layout
//! `[discriminator bytes][#[repr(C)] struct bytes][optional trailing dynamic bytes]`
//!
//! Structs must have **alignment 1** (enforced at compile time by [`impl_account!`]).
//! Multi-byte integers therefore live in [`PodU64`]-style byte arrays. That buys three
//! things: no padding (so no uninitialised bytes), no alignment UB when casting a
//! byte slice, and freedom to use any discriminator length (1 byte or Anchor's 8).

use {
    crate::errors::KitError,
    core::mem::size_of,
    pinocchio::{
        account::{Ref, RefMut},
        error::ProgramError,
        AccountView, Address,
    },
};

/// Marker for plain-old-data: every bit pattern is valid, no padding, no pointers.
///
/// # Safety
/// Implement only for `#[repr(C)]`/`#[repr(transparent)]` types of alignment 1 whose
/// fields are all `Pod`. `bool`, `char`, enums and references are **not** Pod — use
/// [`PodBool`] or a raw `u8`.
pub unsafe trait Pod: Copy + 'static {}

unsafe impl Pod for u8 {}
unsafe impl Pod for i8 {}
unsafe impl<const N: usize> Pod for [u8; N] {}

macro_rules! pod_int {
    ($name:ident, $int:ty, $n:literal) => {
        #[doc = concat!("Alignment-1 little-endian `", stringify!($int), "`.")]
        #[repr(transparent)]
        #[derive(Clone, Copy, Default, PartialEq, Eq)]
        pub struct $name(pub [u8; $n]);

        unsafe impl Pod for $name {}

        impl $name {
            #[inline(always)]
            pub const fn get(&self) -> $int {
                <$int>::from_le_bytes(self.0)
            }

            #[inline(always)]
            pub fn set(&mut self, v: $int) {
                self.0 = v.to_le_bytes();
            }

            /// `checked_add` that maps overflow to [`KitError::ArithmeticOverflow`].
            #[inline(always)]
            pub fn checked_add(&mut self, v: $int) -> Result<$int, ProgramError> {
                let n = self.get().checked_add(v).ok_or(KitError::ArithmeticOverflow)?;
                self.set(n);
                Ok(n)
            }

            #[inline(always)]
            pub fn checked_sub(&mut self, v: $int) -> Result<$int, ProgramError> {
                let n = self.get().checked_sub(v).ok_or(KitError::ArithmeticOverflow)?;
                self.set(n);
                Ok(n)
            }
        }

        impl From<$int> for $name {
            #[inline(always)]
            fn from(v: $int) -> Self {
                Self(v.to_le_bytes())
            }
        }

        impl core::fmt::Debug for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                self.get().fmt(f)
            }
        }
    };
}

pod_int!(PodU16, u16, 2);
pod_int!(PodU32, u32, 4);
pod_int!(PodU64, u64, 8);
pod_int!(PodI64, i64, 8);
pod_int!(PodU128, u128, 16);

/// Alignment-1 bool that tolerates any byte (non-zero = true) instead of being UB.
#[repr(transparent)]
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct PodBool(pub u8);

unsafe impl Pod for PodBool {}

impl PodBool {
    #[inline(always)]
    pub const fn get(&self) -> bool {
        self.0 != 0
    }

    #[inline(always)]
    pub fn set(&mut self, v: bool) {
        self.0 = v as u8;
    }
}

/// Bytes written at offset 0 of every account of this type.
pub trait Discriminator {
    const DISCRIMINATOR: &'static [u8];
}

/// Program that must own accounts of this type.
pub trait Owner {
    const OWNER: Address;
}

/// Size needed at `init` (Anchor's `InitSpace`).
pub trait Space {
    const INIT_SPACE: usize;
}

/// A fixed-size, zero-copy account type. Implemented via [`impl_account!`].
pub trait AccountState: Pod + Discriminator + Owner {
    /// Discriminator + struct size.
    const LEN: usize = Self::DISCRIMINATOR.len() + size_of::<Self>();
}

/// Implements [`Pod`], [`Discriminator`], [`Owner`] and [`AccountState`], and statically
/// asserts the struct has alignment 1 (hence no padding).
///
/// ```ignore
/// #[repr(C)]
/// #[derive(Clone, Copy, ShankAccount)]
/// pub struct Counter { pub authority: [u8; 32], pub count: PodU64, pub bump: u8 }
///
/// pinocchio_kit::impl_account!(Counter, discriminator = [1], owner = crate::ID);
/// ```
#[macro_export]
macro_rules! impl_account {
    ($ty:ty, discriminator = $disc:expr, owner = $owner:expr) => {
        const _: () = {
            assert!(
                ::core::mem::align_of::<$ty>() == 1,
                "account structs must have alignment 1: use PodU64/PodU32/... and [u8; 32]"
            );
            assert!(!$disc.is_empty(), "discriminator must not be empty");
        };
        // SAFETY: alignment 1 guarantees no padding; the author guarantees Pod fields.
        unsafe impl $crate::state::Pod for $ty {}
        impl $crate::state::Discriminator for $ty {
            const DISCRIMINATOR: &'static [u8] = &$disc;
        }
        impl $crate::state::Owner for $ty {
            const OWNER: $crate::pinocchio::Address = $owner;
        }
        impl $crate::state::AccountState for $ty {}
        impl $crate::state::Space for $ty {
            const INIT_SPACE: usize = <$ty as $crate::state::AccountState>::LEN;
        }
    };
}

#[inline(always)]
fn check_layout<T: AccountState>(view: &AccountView) -> Result<(), ProgramError> {
    if !view.owned_by(&T::OWNER) {
        return Err(KitError::AccountOwnedByWrongProgram.into());
    }
    if view.data_len() < T::LEN {
        return Err(KitError::AccountDidNotDeserialize.into());
    }
    Ok(())
}

#[inline(always)]
fn check_discriminator<T: Discriminator>(data: &[u8]) -> Result<(), ProgramError> {
    let d = T::DISCRIMINATOR;
    let found = &data[..d.len()];
    if found == d {
        Ok(())
    } else if found.iter().all(|b| *b == 0) {
        Err(KitError::AccountDiscriminatorNotFound.into())
    } else {
        Err(KitError::AccountDiscriminatorMismatch.into())
    }
}

/// Owner + length + discriminator checked, read-only zero-copy view.
#[inline(always)]
pub fn load<T: AccountState>(view: &AccountView) -> Result<Ref<'_, T>, ProgramError> {
    check_layout::<T>(view)?;
    let data = view.try_borrow()?;
    check_discriminator::<T>(&data)?;
    let off = T::DISCRIMINATOR.len();
    // SAFETY: length checked, `T: Pod` with alignment 1.
    Ok(Ref::map(data, |d| unsafe { &*(d[off..].as_ptr() as *const T) }))
}

/// Mutable zero-copy view. Also requires the account to be writable.
#[inline(always)]
pub fn load_mut<T: AccountState>(view: &mut AccountView) -> Result<RefMut<'_, T>, ProgramError> {
    if !view.is_writable() {
        return Err(KitError::AccountNotMutable.into());
    }
    check_layout::<T>(view)?;
    let data = view.try_borrow_mut()?;
    check_discriminator::<T>(&data)?;
    let off = T::DISCRIMINATOR.len();
    // SAFETY: as in `load`, plus exclusive borrow via RefMut.
    Ok(RefMut::map(data, |d| unsafe {
        &mut *(d[off..].as_mut_ptr() as *mut T)
    }))
}

/// For freshly created accounts: requires an all-zero discriminator (re-initialisation
/// guard, Anchor's `zero` constraint), writes the discriminator and returns the struct.
#[inline(always)]
pub fn load_init<T: AccountState>(view: &mut AccountView) -> Result<RefMut<'_, T>, ProgramError> {
    if !view.is_writable() {
        return Err(KitError::AccountNotMutable.into());
    }
    check_layout::<T>(view)?;
    let mut data = view.try_borrow_mut()?;
    let d = T::DISCRIMINATOR;
    if data[..d.len()].iter().any(|b| *b != 0) {
        return Err(KitError::AccountDiscriminatorAlreadySet.into());
    }
    data[..d.len()].copy_from_slice(d);
    let off = d.len();
    Ok(RefMut::map(data, |d| unsafe {
        &mut *(d[off..].as_mut_ptr() as *mut T)
    }))
}

/// Bytes after the fixed struct (for realloc-grown dynamic tails).
#[inline(always)]
pub fn trailing_bytes<T: AccountState>(view: &AccountView) -> Result<Ref<'_, [u8]>, ProgramError> {
    check_layout::<T>(view)?;
    let data = view.try_borrow()?;
    check_discriminator::<T>(&data)?;
    Ok(Ref::map(data, |d| &d[T::LEN..]))
}

#[inline(always)]
pub fn trailing_bytes_mut<T: AccountState>(
    view: &mut AccountView,
) -> Result<RefMut<'_, [u8]>, ProgramError> {
    check_layout::<T>(view)?;
    let data = view.try_borrow_mut()?;
    check_discriminator::<T>(&data)?;
    Ok(RefMut::map(data, |d| &mut d[T::LEN..]))
}

/// Anchor's `AccountDeserialize`: owned copy out of raw account bytes (incl. discriminator).
pub trait AccountDeserialize: Sized {
    fn try_deserialize(data: &[u8]) -> Result<Self, ProgramError>;
}

/// Anchor's `AccountSerialize`: write discriminator + body into raw account bytes.
pub trait AccountSerialize {
    fn try_serialize(&self, data: &mut [u8]) -> Result<(), ProgramError>;
}

/// Zero-copy types get the traits for free via this wrapper-free helper module.
pub mod pod {
    use super::*;

    #[inline(always)]
    pub fn deserialize<T: AccountState>(data: &[u8]) -> Result<T, ProgramError> {
        if data.len() < T::LEN {
            return Err(KitError::AccountDidNotDeserialize.into());
        }
        check_discriminator::<T>(data)?;
        // SAFETY: length checked; read_unaligned copies, Pod accepts any bytes.
        Ok(unsafe {
            core::ptr::read_unaligned(data[T::DISCRIMINATOR.len()..].as_ptr() as *const T)
        })
    }

    #[inline(always)]
    pub fn serialize<T: AccountState>(value: &T, data: &mut [u8]) -> Result<(), ProgramError> {
        if data.len() < T::LEN {
            return Err(KitError::AccountDidNotSerialize.into());
        }
        let d = T::DISCRIMINATOR;
        data[..d.len()].copy_from_slice(d);
        // SAFETY: T is Pod, so viewing it as bytes is sound.
        let bytes = unsafe {
            core::slice::from_raw_parts(value as *const T as *const u8, size_of::<T>())
        };
        data[d.len()..T::LEN].copy_from_slice(bytes);
        Ok(())
    }
}

/// Borsh hybrid: for accounts with `Vec`/`String` fields. Shares the discriminator and
/// owner rules with zero-copy accounts; needs the `borsh` feature and a heap allocator.
#[cfg(feature = "borsh")]
pub mod borsh_state {
    use {super::*, borsh::{BorshDeserialize, BorshSerialize}};

    pub trait BorshAccount: BorshSerialize + BorshDeserialize + Discriminator + Owner {}
    impl<T: BorshSerialize + BorshDeserialize + Discriminator + Owner> BorshAccount for T {}

    impl<T: BorshAccount> AccountDeserialize for T {
        fn try_deserialize(data: &[u8]) -> Result<Self, ProgramError> {
            let d = T::DISCRIMINATOR;
            if data.len() < d.len() {
                return Err(KitError::AccountDiscriminatorNotFound.into());
            }
            check_discriminator::<T>(data)?;
            let mut body = &data[d.len()..];
            T::deserialize(&mut body).map_err(|_| KitError::AccountDidNotDeserialize.into())
        }
    }

    impl<T: BorshAccount> AccountSerialize for T {
        fn try_serialize(&self, data: &mut [u8]) -> Result<(), ProgramError> {
            let d = T::DISCRIMINATOR;
            if data.len() < d.len() {
                return Err(KitError::AccountDidNotSerialize.into());
            }
            data[..d.len()].copy_from_slice(d);
            let mut body = &mut data[d.len()..];
            self.serialize(&mut body)
                .map_err(|_| KitError::AccountDidNotSerialize.into())
        }
    }

    /// Owner-checked borsh load.
    pub fn load<T: BorshAccount>(view: &AccountView) -> Result<T, ProgramError> {
        if !view.owned_by(&T::OWNER) {
            return Err(KitError::AccountOwnedByWrongProgram.into());
        }
        T::try_deserialize(&view.try_borrow()?)
    }

    /// Serialized size including discriminator — feed into `lifecycle::realloc`.
    pub fn serialized_len<T: BorshAccount>(value: &T) -> Result<usize, ProgramError> {
        let body = borsh::to_vec(value).map_err(|_| KitError::AccountDidNotSerialize)?;
        Ok(T::DISCRIMINATOR.len() + body.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Demo {
        authority: [u8; 32],
        count: PodU64,
        flag: PodBool,
    }

    crate::impl_account!(Demo, discriminator = [9], owner = Address::new_from_array([7; 32]));

    #[test]
    fn layout_and_roundtrip() {
        assert_eq!(Demo::LEN, 1 + 32 + 8 + 1);
        let mut buf = [0u8; 42];
        let v = Demo { authority: [3; 32], count: 5u64.into(), flag: PodBool(1) };
        pod::serialize(&v, &mut buf).unwrap();
        assert_eq!(buf[0], 9);
        let back: Demo = pod::deserialize(&buf).unwrap();
        assert_eq!(back.count.get(), 5);
        assert!(back.flag.get());

        buf[0] = 0;
        assert_eq!(
            pod::deserialize::<Demo>(&buf).err().unwrap(),
            KitError::AccountDiscriminatorNotFound.into()
        );
        buf[0] = 8;
        assert_eq!(
            pod::deserialize::<Demo>(&buf).err().unwrap(),
            KitError::AccountDiscriminatorMismatch.into()
        );
    }

    #[test]
    fn pod_overflow_is_an_error() {
        let mut n = PodU64::from(u64::MAX);
        assert!(n.checked_add(1).is_err());
        assert_eq!(n.get(), u64::MAX);
    }
}
