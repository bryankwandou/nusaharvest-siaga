//! Pure state-machine guards and arithmetic. No account access, so every rule is
//! unit-testable off-chain; the processor only wires accounts to these functions.

use crate::{
    error::NhError,
    state::{outcome, status},
};

/// Dispute window after Settle: 48 hours.
pub const DISPUTE_WINDOW_SECS: i64 = 172_800;
/// Operator no-show grace after window_end_ts before sponsor can reclaim: 30 days.
pub const REFUND_GRACE_SECS: i64 = 2_592_000;
/// If a dispute is not resolved (re-settled) within 14 days after the dispute
/// window closes, anyone may Release and the sponsor is refunded in full.
pub const DISPUTE_RESOLUTION_SECS: i64 = 1_209_600;
/// Claim period after the dispute window before leftovers can be swept: 90 days.
pub const CLAIM_PERIOD_SECS: i64 = 7_776_000;

#[inline]
fn add_ts(a: i64, b: i64) -> Result<i64, NhError> {
    a.checked_add(b).ok_or(NhError::Overflow)
}

pub struct CreateParams {
    pub freeze_ts: i64,
    pub window_end_ts: i64,
    pub amount_full: u64,
    pub amount_half: u64,
    pub thr_full: i32,
    pub thr_half: i32,
    pub deposit: u64,
    pub pledge: bool,
}

pub fn validate_create(now: i64, p: &CreateParams) -> Result<(), NhError> {
    if !(now < p.freeze_ts && p.freeze_ts < p.window_end_ts) {
        return Err(NhError::BadParams);
    }
    if p.thr_full > p.thr_half || p.amount_half > p.amount_full || p.amount_full == 0 {
        return Err(NhError::BadParams);
    }
    if p.pledge && p.deposit != 0 {
        return Err(NhError::BadParams);
    }
    Ok(())
}

pub fn guard_lock_roster(
    st: u8,
    now: i64,
    freeze_ts: i64,
    units: u32,
    pledge: bool,
    vault_balance: u64,
    amount_full: u64,
) -> Result<(), NhError> {
    if st != status::OPEN {
        return Err(NhError::WrongStatus);
    }
    if now >= freeze_ts {
        return Err(NhError::TooLate);
    }
    if units == 0 {
        return Err(NhError::BadParams);
    }
    if !pledge {
        let need = (units as u64)
            .checked_mul(amount_full)
            .ok_or(NhError::Overflow)?;
        if vault_balance < need {
            return Err(NhError::Underfunded);
        }
    }
    Ok(())
}

/// Lower rainfall is worse. observed <= thr_full (e.g. p10) pays full,
/// observed <= thr_half (e.g. p20) pays half, otherwise nothing.
pub fn compute_outcome(observed: i32, thr_full: i32, thr_half: i32) -> u8 {
    if observed <= thr_full {
        outcome::FULL
    } else if observed <= thr_half {
        outcome::HALF
    } else {
        outcome::NONE
    }
}

pub fn payout_per_unit(out: u8, amount_full: u64, amount_half: u64) -> u64 {
    match out {
        outcome::FULL => amount_full,
        outcome::HALF => amount_half,
        _ => 0,
    }
}

/// Returns the status to write after a Settle. The operator signature is checked
/// by the caller; every Settle also needs the independent oracle key, and a
/// re-settle of a DISPUTED campaign additionally needs the auditor.
pub fn guard_settle(
    st: u8,
    now: i64,
    window_end_ts: i64,
    units: u32,
    oracle_cosigned: bool,
    auditor_cosigned: bool,
) -> Result<u8, NhError> {
    if !oracle_cosigned {
        return Err(NhError::MissingSigner);
    }
    match st {
        status::OPEN => {
            if now < window_end_ts {
                return Err(NhError::TooEarly);
            }
            if units == 0 {
                return Err(NhError::WrongStatus);
            }
            Ok(status::SETTLED)
        }
        status::DISPUTED => {
            if !auditor_cosigned {
                return Err(NhError::MissingSigner);
            }
            Ok(status::SETTLED_FINAL)
        }
        _ => Err(NhError::WrongStatus),
    }
}

pub fn guard_dispute(st: u8, now: i64, settled_ts: i64) -> Result<(), NhError> {
    if st != status::SETTLED {
        return Err(NhError::WrongStatus);
    }
    if now >= add_ts(settled_ts, DISPUTE_WINDOW_SECS)? {
        return Err(NhError::TooLate);
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReleasePlan {
    pub new_status: u8,
    /// Tokens sent back to the sponsor now.
    pub refund: u64,
    /// Close the vault (all tokens leave, rent to sponsor).
    pub close_vault: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn plan_release(
    st: u8,
    now: i64,
    settled_ts: i64,
    window_end_ts: i64,
    out: u8,
    pledge: bool,
    units: u32,
    amount_full: u64,
    amount_half: u64,
    vault_balance: u64,
) -> Result<ReleasePlan, NhError> {
    let refund_all = |new_status| ReleasePlan {
        new_status,
        refund: if pledge { 0 } else { vault_balance },
        close_vault: !pledge,
    };
    match st {
        status::OPEN => {
            if now < add_ts(window_end_ts, REFUND_GRACE_SECS)? {
                return Err(NhError::TooEarly);
            }
            Ok(refund_all(status::REFUNDED))
        }
        status::DISPUTED => {
            let deadline = add_ts(add_ts(settled_ts, DISPUTE_WINDOW_SECS)?, DISPUTE_RESOLUTION_SECS)?;
            if now < deadline {
                return Err(NhError::TooEarly);
            }
            Ok(refund_all(status::REFUNDED))
        }
        status::SETTLED | status::SETTLED_FINAL => {
            if st == status::SETTLED && now < add_ts(settled_ts, DISPUTE_WINDOW_SECS)? {
                return Err(NhError::TooEarly);
            }
            if pledge {
                return Ok(ReleasePlan { new_status: status::RELEASED, refund: 0, close_vault: false });
            }
            let per = payout_per_unit(out, amount_full, amount_half);
            if per == 0 {
                return Ok(refund_all(status::RELEASED));
            }
            let reserve = (units as u64).checked_mul(per).ok_or(NhError::Overflow)?;
            let refund = vault_balance.checked_sub(reserve).ok_or(NhError::Underfunded)?;
            Ok(ReleasePlan { new_status: status::RELEASED, refund, close_vault: false })
        }
        _ => Err(NhError::WrongStatus),
    }
}

pub fn guard_post_receipts(st: u8, out: u8) -> Result<(), NhError> {
    if st != status::RELEASED {
        return Err(NhError::WrongStatus);
    }
    if out == outcome::NONE {
        return Err(NhError::WrongStatus);
    }
    Ok(())
}

/// Returns the amount the claimant receives.
pub fn guard_claim(
    st: u8,
    out: u8,
    pledge: bool,
    index: u32,
    units: u32,
    amount_full: u64,
    amount_half: u64,
) -> Result<u64, NhError> {
    if st != status::RELEASED && st != status::RECEIPTED {
        return Err(NhError::WrongStatus);
    }
    if pledge || out == outcome::NONE {
        return Err(NhError::WrongStatus);
    }
    if index >= units {
        return Err(NhError::BadParams);
    }
    Ok(payout_per_unit(out, amount_full, amount_half))
}

pub fn guard_sweep(st: u8, out: u8, pledge: bool, now: i64, settled_ts: i64) -> Result<(), NhError> {
    if (st != status::RELEASED && st != status::RECEIPTED) || pledge || out == outcome::NONE {
        return Err(NhError::WrongStatus);
    }
    let end = add_ts(add_ts(settled_ts, DISPUTE_WINDOW_SECS)?, CLAIM_PERIOD_SECS)?;
    if now < end {
        return Err(NhError::TooEarly);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(now: i64) -> CreateParams {
        CreateParams {
            freeze_ts: now + 100,
            window_end_ts: now + 1000,
            amount_full: 1_000_000,
            amount_half: 500_000,
            thr_full: 800,
            thr_half: 1200,
            deposit: 3_000_000,
            pledge: false,
        }
    }

    #[test]
    fn create_validation() {
        assert!(validate_create(0, &params(0)).is_ok());
        let mut p = params(0);
        p.freeze_ts = 0;
        assert_eq!(validate_create(0, &p), Err(NhError::BadParams));
        let mut p = params(0);
        p.thr_full = 2000;
        assert_eq!(validate_create(0, &p), Err(NhError::BadParams));
        let mut p = params(0);
        p.pledge = true;
        assert_eq!(validate_create(0, &p), Err(NhError::BadParams));
        let mut p = params(0);
        p.amount_full = 0;
        p.amount_half = 0;
        assert_eq!(validate_create(0, &p), Err(NhError::BadParams));
    }

    #[test]
    fn lock_roster_guards() {
        assert!(guard_lock_roster(status::OPEN, 10, 100, 3, false, 3_000_000, 1_000_000).is_ok());
        assert_eq!(guard_lock_roster(status::OPEN, 10, 100, 4, false, 3_000_000, 1_000_000), Err(NhError::Underfunded));
        assert_eq!(guard_lock_roster(status::OPEN, 100, 100, 3, false, 3_000_000, 1_000_000), Err(NhError::TooLate));
        assert_eq!(guard_lock_roster(status::SETTLED, 10, 100, 3, false, 3_000_000, 1_000_000), Err(NhError::WrongStatus));
        assert_eq!(guard_lock_roster(status::OPEN, 10, 100, u32::MAX, false, u64::MAX, u64::MAX), Err(NhError::Overflow));
        assert!(guard_lock_roster(status::OPEN, 10, 100, 50, true, 0, 1_000_000).is_ok());
    }

    #[test]
    fn outcome_thresholds() {
        assert_eq!(compute_outcome(500, 800, 1200), outcome::FULL);
        assert_eq!(compute_outcome(800, 800, 1200), outcome::FULL);
        assert_eq!(compute_outcome(1000, 800, 1200), outcome::HALF);
        assert_eq!(compute_outcome(1201, 800, 1200), outcome::NONE);
    }

    #[test]
    fn settle_ordering() {
        assert_eq!(guard_settle(status::OPEN, 999, 1000, 3, true, false), Err(NhError::TooEarly));
        assert_eq!(guard_settle(status::OPEN, 1000, 1000, 0, true, false), Err(NhError::WrongStatus));
        assert_eq!(guard_settle(status::OPEN, 1000, 1000, 3, false, false), Err(NhError::MissingSigner));
        assert_eq!(guard_settle(status::OPEN, 1000, 1000, 3, true, false), Ok(status::SETTLED));
        assert_eq!(guard_settle(status::SETTLED, 5000, 1000, 3, true, true), Err(NhError::WrongStatus));
        assert_eq!(guard_settle(status::DISPUTED, 5000, 1000, 3, true, false), Err(NhError::MissingSigner));
        assert_eq!(guard_settle(status::DISPUTED, 5000, 1000, 3, true, true), Ok(status::SETTLED_FINAL));
        assert_eq!(guard_settle(status::RELEASED, 5000, 1000, 3, true, true), Err(NhError::WrongStatus));
    }

    #[test]
    fn dispute_window() {
        assert!(guard_dispute(status::SETTLED, 100, 100).is_ok());
        assert_eq!(guard_dispute(status::SETTLED, 100 + DISPUTE_WINDOW_SECS, 100), Err(NhError::TooLate));
        assert_eq!(guard_dispute(status::SETTLED_FINAL, 101, 100), Err(NhError::WrongStatus));
    }

    #[test]
    fn early_release_rejected() {
        let r = plan_release(status::SETTLED, 100 + DISPUTE_WINDOW_SECS - 1, 100, 50, outcome::FULL, false, 3, 10, 5, 30);
        assert_eq!(r, Err(NhError::TooEarly));
        let t = 100 + DISPUTE_WINDOW_SECS + DISPUTE_RESOLUTION_SECS;
        let r = plan_release(status::DISPUTED, t - 1, 100, 50, outcome::FULL, false, 3, 10, 5, 30);
        assert_eq!(r, Err(NhError::TooEarly));
        let r = plan_release(status::DISPUTED, t, 100, 50, outcome::FULL, false, 3, 10, 5, 30);
        assert_eq!(r, Ok(ReleasePlan { new_status: status::REFUNDED, refund: 30, close_vault: true }));
        let r = plan_release(status::RELEASED, t, 100, 50, outcome::FULL, false, 3, 10, 5, 30);
        assert_eq!(r, Err(NhError::WrongStatus));
        let r = plan_release(status::OPEN, 50 + REFUND_GRACE_SECS - 1, 0, 50, 0, false, 3, 10, 5, 30);
        assert_eq!(r, Err(NhError::TooEarly));
    }

    #[test]
    fn release_plans() {
        let t = 100 + DISPUTE_WINDOW_SECS;
        assert_eq!(
            plan_release(status::SETTLED, t, 100, 50, outcome::HALF, false, 3, 10, 5, 40),
            Ok(ReleasePlan { new_status: status::RELEASED, refund: 25, close_vault: false })
        );
        assert_eq!(
            plan_release(status::SETTLED_FINAL, 0, 100, 50, outcome::NONE, false, 3, 10, 5, 40),
            Ok(ReleasePlan { new_status: status::RELEASED, refund: 40, close_vault: true })
        );
        assert_eq!(
            plan_release(status::OPEN, 50 + REFUND_GRACE_SECS, 0, 50, 0, false, 3, 10, 5, 40),
            Ok(ReleasePlan { new_status: status::REFUNDED, refund: 40, close_vault: true })
        );
        assert_eq!(
            plan_release(status::SETTLED, t, 100, 50, outcome::FULL, false, 5, 10, 5, 40),
            Err(NhError::Underfunded)
        );
    }

    #[test]
    fn claim_and_receipts_guards() {
        assert_eq!(guard_claim(status::RELEASED, outcome::FULL, false, 0, 3, 10, 5), Ok(10));
        assert_eq!(guard_claim(status::RECEIPTED, outcome::HALF, false, 2, 3, 10, 5), Ok(5));
        assert_eq!(guard_claim(status::SETTLED, outcome::FULL, false, 0, 3, 10, 5), Err(NhError::WrongStatus));
        assert_eq!(guard_claim(status::RELEASED, outcome::NONE, false, 0, 3, 10, 5), Err(NhError::WrongStatus));
        assert_eq!(guard_claim(status::RELEASED, outcome::FULL, true, 0, 3, 10, 5), Err(NhError::WrongStatus));
        assert_eq!(guard_claim(status::RELEASED, outcome::FULL, false, 3, 3, 10, 5), Err(NhError::BadParams));
        assert!(guard_post_receipts(status::RELEASED, outcome::FULL).is_ok());
        assert_eq!(guard_post_receipts(status::SETTLED, outcome::FULL), Err(NhError::WrongStatus));
        assert_eq!(guard_sweep(status::RELEASED, outcome::FULL, false, 10, 0), Err(NhError::TooEarly));
    }
}
