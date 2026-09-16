//! Campaign account: 368 bytes, little-endian, explicit offsets (spec 3.1).
//! Accessed through byte offsets, never transmuted, so alignment is irrelevant.

pub const CAMPAIGN_LEN: usize = 368;
pub const CAMPAIGN_TAG: u8 = 1;
pub const RULE_VERSION: u8 = 1;
pub const CLAIM_RECEIPT_LEN: usize = 1;

pub const SEED_CAMPAIGN: &[u8] = b"camp";
pub const SEED_VAULT: &[u8] = b"vault";
pub const SEED_CLAIM: &[u8] = b"claim";

pub mod status {
    pub const OPEN: u8 = 0;
    pub const SETTLED: u8 = 1;
    pub const DISPUTED: u8 = 2;
    pub const SETTLED_FINAL: u8 = 3;
    pub const RELEASED: u8 = 4;
    pub const RECEIPTED: u8 = 5;
    pub const REFUNDED: u8 = 6;
    pub const CLOSED: u8 = 7;
}

pub mod outcome {
    pub const NONE: u8 = 0;
    pub const HALF: u8 = 1;
    pub const FULL: u8 = 2;
}

pub const FLAG_PLEDGE: u8 = 0b0000_0001;

pub mod off {
    pub const TAG: usize = 0;
    pub const BUMP: usize = 1;
    pub const STATUS: usize = 2;
    pub const FLAGS: usize = 3;
    pub const VAULT_BUMP: usize = 4;
    pub const RULE_VERSION: usize = 5;
    pub const UNITS: usize = 8;
    pub const CAMPAIGN_ID: usize = 16;
    pub const SPONSOR: usize = 24;
    pub const OPERATOR: usize = 56;
    pub const AUDITOR: usize = 88;
    /// Spec called this `disburser`; repurposed as the independent rainfall oracle key
    /// that must co-sign every Settle (payout is per-farmer Claim, not a lump sum).
    pub const ORACLE: usize = 120;
    pub const MINT: usize = 152;
    pub const FREEZE_TS: usize = 184;
    pub const WINDOW_END_TS: usize = 192;
    pub const SETTLED_TS: usize = 200;
    pub const AMOUNT_FULL: usize = 208;
    pub const AMOUNT_HALF: usize = 216;
    pub const THR_FULL: usize = 224;
    pub const THR_HALF: usize = 228;
    pub const OBSERVED: usize = 232;
    pub const TERMS_HASH: usize = 240;
    pub const ROSTER_ROOT: usize = 272;
    pub const DATA_HASH: usize = 304;
    pub const RECEIPTS_ROOT: usize = 336;
}

#[inline]
pub fn rd32(d: &[u8], o: usize) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(&d[o..o + 32]);
    out
}
#[inline]
pub fn rd_u32(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}
#[inline]
pub fn rd_i32(d: &[u8], o: usize) -> i32 {
    rd_u32(d, o) as i32
}
#[inline]
pub fn rd_u64(d: &[u8], o: usize) -> u64 {
    let mut b = [0u8; 8];
    b.copy_from_slice(&d[o..o + 8]);
    u64::from_le_bytes(b)
}
#[inline]
pub fn rd_i64(d: &[u8], o: usize) -> i64 {
    rd_u64(d, o) as i64
}
#[inline]
pub fn wr(d: &mut [u8], o: usize, src: &[u8]) {
    d[o..o + src.len()].copy_from_slice(src);
}

/// Outcome bits live in flags bit1..2.
#[inline]
pub fn get_outcome(flags: u8) -> u8 {
    (flags >> 1) & 0b11
}
#[inline]
pub fn with_outcome(flags: u8, outcome: u8) -> u8 {
    (flags & !0b110) | ((outcome & 0b11) << 1)
}
