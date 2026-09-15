//! CU-cheap logging.
//!
//! `msg!` forwards to `pinocchio_log::log!`, which formats into a fixed stack buffer
//! (no `format!`, no allocator). Build the kit with `default-features = false` to compile
//! every `msg!` out of the binary entirely.
//!
//! Callers must depend on `pinocchio-log` directly: its proc-macro expands to
//! `::pinocchio_log::...` paths.

pub use pinocchio_log::{log_cu_usage, logger::Logger};

/// `msg!("amount={} owner={}", amount, "x")` — integers and `&str` only.
#[cfg(feature = "logging")]
#[macro_export]
macro_rules! msg {
    ($($arg:tt)*) => { ::pinocchio_log::log!($($arg)*) };
}

/// Logging disabled at compile time: arguments are not evaluated.
#[cfg(not(feature = "logging"))]
#[macro_export]
macro_rules! msg {
    ($($arg:tt)*) => {};
}

/// Logs remaining compute units with a label. Pair two calls to measure a block.
#[inline(always)]
pub fn log_remaining_cu(label: &str) {
    #[cfg(any(target_os = "solana", target_arch = "bpf"))]
    {
        extern "C" {
            fn sol_remaining_compute_units() -> u64;
        }
        let mut logger = Logger::<64>::default();
        logger.append(label);
        logger.append(" cu_left=");
        logger.append(unsafe { sol_remaining_compute_units() });
        logger.log();
    }
    #[cfg(not(any(target_os = "solana", target_arch = "bpf")))]
    let _ = label;
}
