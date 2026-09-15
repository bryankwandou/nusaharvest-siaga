//! Anchor-style constraint syntax as declarative macros.
//!
//! ```ignore
//! check!(counter => mut, owner = crate::ID, seeds = [b"counter", auth.address().as_ref()], bump = bump, program = &crate::ID);
//! check!(payer => signer, mut);
//! check!(system_program => program = &pinocchio_system::ID);
//! has_one!(counter_state.authority, authority);
//! require!(amount > 0, MyError::ZeroAmount);
//! ```
//!
//! Each clause maps 1:1 to a function in [`crate::accounts::AccountChecks`] or
//! [`crate::pda`], so what you read is exactly what executes.

/// Applies comma-separated constraints to an `&AccountView`, returning early with the
/// matching [`KitError`](crate::errors::KitError) on the first violation.
///
/// Supported clauses: `signer`, `mut`, `executable`, `system`, `uninitialized`,
/// `rent_exempt`, `owner = <Address>`, `address = <Address>`, `program = <&Address>`
/// (address + executable), `space = <usize>`,
/// `seeds = [..], bump = <u8>, program = <&Address>` (stored bump, cheap sha256 derive),
/// `seeds = [..], canonical_bump, program = <&Address>` (find, costs more CU).
#[macro_export]
macro_rules! check {
    ($view:expr => $($rest:tt)+) => {{
        let __view: &$crate::pinocchio::AccountView = $view;
        $crate::check!(@c __view; $($rest)+);
    }};

    (@c $v:ident; ) => {};

    (@c $v:ident; signer $(, $($rest:tt)*)?) => {
        $crate::accounts::AccountChecks::check_signer($v)?;
        $crate::check!(@c $v; $($($rest)*)?);
    };
    (@c $v:ident; mut $(, $($rest:tt)*)?) => {
        $crate::accounts::AccountChecks::check_writable($v)?;
        $crate::check!(@c $v; $($($rest)*)?);
    };
    (@c $v:ident; executable $(, $($rest:tt)*)?) => {
        $crate::accounts::AccountChecks::check_executable($v)?;
        $crate::check!(@c $v; $($($rest)*)?);
    };
    (@c $v:ident; system $(, $($rest:tt)*)?) => {
        $crate::accounts::AccountChecks::check_system_owned($v)?;
        $crate::check!(@c $v; $($($rest)*)?);
    };
    (@c $v:ident; uninitialized $(, $($rest:tt)*)?) => {
        $crate::accounts::AccountChecks::check_uninitialized($v)?;
        $crate::check!(@c $v; $($($rest)*)?);
    };
    (@c $v:ident; rent_exempt $(, $($rest:tt)*)?) => {
        $crate::accounts::AccountChecks::check_rent_exempt($v)?;
        $crate::check!(@c $v; $($($rest)*)?);
    };
    (@c $v:ident; seeds = [$($seed:expr),* $(,)?], bump = $bump:expr, program = $pid:expr $(, $($rest:tt)*)?) => {
        $crate::pda::verify_seeds($v, &[$($seed),*], $bump, $pid)?;
        $crate::check!(@c $v; $($($rest)*)?);
    };
    (@c $v:ident; seeds = [$($seed:expr),* $(,)?], canonical_bump, program = $pid:expr $(, $($rest:tt)*)?) => {
        $crate::pda::verify_canonical($v, &[$($seed),*], $pid)?;
        $crate::check!(@c $v; $($($rest)*)?);
    };
    (@c $v:ident; owner = $owner:expr $(, $($rest:tt)*)?) => {
        $crate::accounts::AccountChecks::check_owner($v, &$owner)?;
        $crate::check!(@c $v; $($($rest)*)?);
    };
    (@c $v:ident; address = $addr:expr $(, $($rest:tt)*)?) => {
        $crate::accounts::AccountChecks::check_address($v, &$addr)?;
        $crate::check!(@c $v; $($($rest)*)?);
    };
    (@c $v:ident; program = $pid:expr $(, $($rest:tt)*)?) => {
        $crate::accounts::AccountChecks::check_program($v, $pid)?;
        $crate::check!(@c $v; $($($rest)*)?);
    };
    (@c $v:ident; space = $len:expr $(, $($rest:tt)*)?) => {
        $crate::accounts::AccountChecks::check_min_data_len($v, $len)?;
        $crate::check!(@c $v; $($($rest)*)?);
    };
}

/// `has_one = authority`: a `[u8; 32]` or `Address` field must equal the account's address.
#[macro_export]
macro_rules! has_one {
    ($field:expr, $account:expr) => {
        if $field.as_ref() != $account.address().as_ref() {
            return Err($crate::errors::KitError::ConstraintHasOne.into());
        }
    };
    ($field:expr, $account:expr, $err:expr) => {
        if $field.as_ref() != $account.address().as_ref() {
            return Err($err.into());
        }
    };
}

/// `require!(cond, err)`; with one argument uses `KitError::RequireViolated`.
#[macro_export]
macro_rules! require {
    ($cond:expr) => {
        $crate::require!($cond, $crate::errors::KitError::RequireViolated)
    };
    ($cond:expr, $err:expr $(,)?) => {
        if !($cond) {
            return Err($err.into());
        }
    };
}

#[macro_export]
macro_rules! require_eq {
    ($a:expr, $b:expr) => { $crate::require_eq!($a, $b, $crate::errors::KitError::RequireEqViolated) };
    ($a:expr, $b:expr, $err:expr $(,)?) => { if $a != $b { return Err($err.into()); } };
}

#[macro_export]
macro_rules! require_gt {
    ($a:expr, $b:expr) => { $crate::require_gt!($a, $b, $crate::errors::KitError::RequireGtViolated) };
    ($a:expr, $b:expr, $err:expr $(,)?) => { if $a <= $b { return Err($err.into()); } };
}

#[macro_export]
macro_rules! require_gte {
    ($a:expr, $b:expr) => { $crate::require_gte!($a, $b, $crate::errors::KitError::RequireGteViolated) };
    ($a:expr, $b:expr, $err:expr $(,)?) => { if $a < $b { return Err($err.into()); } };
}

/// Compares anything that is `AsRef<[u8]>` (`Address`, `[u8; 32]`).
#[macro_export]
macro_rules! require_keys_eq {
    ($a:expr, $b:expr) => { $crate::require_keys_eq!($a, $b, $crate::errors::KitError::RequireKeysEqViolated) };
    ($a:expr, $b:expr, $err:expr $(,)?) => {
        if AsRef::<[u8]>::as_ref(&$a) != AsRef::<[u8]>::as_ref(&$b) { return Err($err.into()); }
    };
}

#[macro_export]
macro_rules! require_keys_neq {
    ($a:expr, $b:expr) => { $crate::require_keys_neq!($a, $b, $crate::errors::KitError::RequireKeysNeqViolated) };
    ($a:expr, $b:expr, $err:expr $(,)?) => {
        if AsRef::<[u8]>::as_ref(&$a) == AsRef::<[u8]>::as_ref(&$b) { return Err($err.into()); }
    };
}
