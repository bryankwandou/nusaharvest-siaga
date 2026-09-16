//! Account wiring. Every handler: parse data -> check signers/owners/PDAs ->
//! call a pure guard in `logic` -> perform CPIs -> write state.

use crate::{
    error::NhError,
    logic::{self, CreateParams},
    merkle,
    state::*,
};
use pinocchio::{
    cpi::{Seed, Signer},
    error::ProgramError,
    sysvars::{clock::Clock, rent::Rent, Sysvar},
    AccountView, Address, ProgramResult,
};

pub mod tag {
    pub const CREATE_CAMPAIGN: u8 = 0;
    pub const LOCK_ROSTER: u8 = 1;
    pub const SETTLE: u8 = 2;
    pub const DISPUTE: u8 = 3;
    pub const RELEASE: u8 = 4;
    pub const POST_RECEIPTS: u8 = 5;
    pub const CLAIM: u8 = 6;
    pub const SWEEP: u8 = 7;
}

const TOKEN_ACCOUNT_LEN: usize = 165;
const MINT_LEN: usize = 82;

pub fn process_instruction(
    program_id: &Address,
    accounts: &mut [AccountView],
    data: &[u8],
) -> ProgramResult {
    let (t, rest) = data.split_first().ok_or(NhError::InvalidTag)?;
    match *t {
        tag::CREATE_CAMPAIGN => create_campaign(program_id, accounts, rest),
        tag::LOCK_ROSTER => lock_roster(program_id, accounts, rest),
        tag::SETTLE => settle(program_id, accounts, rest),
        tag::DISPUTE => dispute(program_id, accounts, rest),
        tag::RELEASE => release(program_id, accounts, rest),
        tag::POST_RECEIPTS => post_receipts(program_id, accounts, rest),
        tag::CLAIM => claim(program_id, accounts, rest),
        tag::SWEEP => sweep(program_id, accounts, rest),
        _ => Err(NhError::InvalidTag.into()),
    }
}

// ---------------------------------------------------------------- helpers

fn now() -> Result<i64, ProgramError> {
    Ok(Clock::get()?.unix_timestamp)
}

fn e<T>(r: Result<T, NhError>) -> Result<T, ProgramError> {
    r.map_err(Into::into)
}

fn signer(a: &AccountView) -> ProgramResult {
    if a.is_signer() { Ok(()) } else { Err(NhError::MissingSigner.into()) }
}

fn writable(a: &AccountView) -> ProgramResult {
    if a.is_writable() { Ok(()) } else { Err(NhError::BadAccount.into()) }
}

fn expect_key(a: &AccountView, key: &[u8; 32]) -> ProgramResult {
    if a.address().as_array() == key { Ok(()) } else { Err(NhError::BadAccount.into()) }
}

fn token_program(a: &AccountView) -> ProgramResult {
    if a.address() == &pinocchio_token::ID { Ok(()) } else { Err(NhError::BadAccount.into()) }
}

fn system_program(a: &AccountView) -> ProgramResult {
    if a.address() == &pinocchio_system::ID { Ok(()) } else { Err(NhError::BadAccount.into()) }
}

struct Data<'a>(&'a [u8]);
impl<'a> Data<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], ProgramError> {
        if self.0.len() < n {
            return Err(NhError::InvalidData.into());
        }
        let (h, t) = self.0.split_at(n);
        self.0 = t;
        Ok(h)
    }
    fn b32(&mut self) -> Result<[u8; 32], ProgramError> {
        let mut o = [0u8; 32];
        o.copy_from_slice(self.take(32)?);
        Ok(o)
    }
    fn u8(&mut self) -> Result<u8, ProgramError> { Ok(self.take(1)?[0]) }
    fn u32(&mut self) -> Result<u32, ProgramError> { Ok(rd_u32(self.take(4)?, 0)) }
    fn i32(&mut self) -> Result<i32, ProgramError> { Ok(rd_i32(self.take(4)?, 0)) }
    fn u64(&mut self) -> Result<u64, ProgramError> { Ok(rd_u64(self.take(8)?, 0)) }
    fn i64(&mut self) -> Result<i64, ProgramError> { Ok(rd_i64(self.take(8)?, 0)) }
    fn end(&self) -> ProgramResult {
        if self.0.is_empty() { Ok(()) } else { Err(NhError::InvalidData.into()) }
    }
}

/// Snapshot of the campaign after full validation (owner, size, tag, PDA).
struct Camp {
    d: [u8; CAMPAIGN_LEN],
}
impl Camp {
    fn status(&self) -> u8 { self.d[off::STATUS] }
    fn flags(&self) -> u8 { self.d[off::FLAGS] }
    fn pledge(&self) -> bool { self.flags() & FLAG_PLEDGE != 0 }
    fn outcome(&self) -> u8 { get_outcome(self.flags()) }
    fn units(&self) -> u32 { rd_u32(&self.d, off::UNITS) }
    fn k(&self, o: usize) -> [u8; 32] { rd32(&self.d, o) }
    fn i64(&self, o: usize) -> i64 { rd_i64(&self.d, o) }
    fn u64(&self, o: usize) -> u64 { rd_u64(&self.d, o) }
    fn i32(&self, o: usize) -> i32 { rd_i32(&self.d, o) }
    fn id_le(&self) -> [u8; 8] {
        let mut b = [0u8; 8];
        b.copy_from_slice(&self.d[off::CAMPAIGN_ID..off::CAMPAIGN_ID + 8]);
        b
    }
}

fn load_campaign(program_id: &Address, a: &AccountView) -> Result<Camp, ProgramError> {
    if !a.owned_by(program_id) {
        return Err(NhError::BadOwner.into());
    }
    if a.data_len() != CAMPAIGN_LEN {
        return Err(NhError::InvalidData.into());
    }
    let mut d = [0u8; CAMPAIGN_LEN];
    d.copy_from_slice(&a.try_borrow()?);
    if d[off::TAG] != CAMPAIGN_TAG {
        return Err(NhError::InvalidData.into());
    }
    let c = Camp { d };
    let sponsor = c.k(off::SPONSOR);
    let id = c.id_le();
    let expected = Address::derive_address(
        &[SEED_CAMPAIGN, &sponsor, &id],
        Some(c.d[off::BUMP]),
        program_id,
    );
    if a.address() != &expected {
        return Err(NhError::BadPda.into());
    }
    Ok(c)
}

fn save_campaign(a: &mut AccountView, c: &Camp) -> ProgramResult {
    writable(a)?;
    a.try_borrow_mut()?.copy_from_slice(&c.d);
    Ok(())
}

/// Returns (mint, owner, amount) of an initialized SPL token account.
fn read_token_account(a: &AccountView) -> Result<([u8; 32], [u8; 32], u64), ProgramError> {
    if !a.owned_by(&pinocchio_token::ID) {
        return Err(NhError::BadOwner.into());
    }
    if a.data_len() != TOKEN_ACCOUNT_LEN {
        return Err(NhError::InvalidData.into());
    }
    let d = a.try_borrow()?;
    // state byte at 108: 1 = Initialized, 2 = Frozen
    if d[108] != 1 {
        return Err(NhError::InvalidData.into());
    }
    Ok((rd32(&d, 0), rd32(&d, 32), rd_u64(&d, 64)))
}

/// Vault: PDA ["vault", campaign], SPL token account, mint = campaign.mint,
/// owner = campaign PDA. Returns balance.
fn check_vault(program_id: &Address, c: &Camp, campaign: &AccountView, vault: &AccountView) -> Result<u64, ProgramError> {
    let expected = Address::derive_address(
        &[SEED_VAULT, campaign.address().as_array()],
        Some(c.d[off::VAULT_BUMP]),
        program_id,
    );
    if vault.address() != &expected {
        return Err(NhError::BadPda.into());
    }
    let (mint, owner, amount) = read_token_account(vault)?;
    if mint != c.k(off::MINT) {
        return Err(NhError::BadMint.into());
    }
    if &owner != campaign.address().as_array() {
        return Err(NhError::BadOwner.into());
    }
    Ok(amount)
}

/// Destination token account: token-program owned, right mint, and (if given) right owner.
fn check_dest_token(c: &Camp, a: &AccountView, owner: Option<&[u8; 32]>) -> ProgramResult {
    writable(a)?;
    let (mint, tok_owner, _) = read_token_account(a)?;
    if mint != c.k(off::MINT) {
        return Err(NhError::BadMint.into());
    }
    if let Some(o) = owner {
        if &tok_owner != o {
            return Err(NhError::BadOwner.into());
        }
    }
    Ok(())
}

/// Create a PDA account, tolerating lamports pre-sent to the address (griefing).
fn create_pda(
    payer: &AccountView,
    target: &AccountView,
    space: usize,
    owner: &Address,
    signer_seeds: &[Seed],
) -> ProgramResult {
    let signers = [Signer::from(signer_seeds)];
    let rent = Rent::get()?.try_minimum_balance(space)?;
    let current = target.lamports();
    if current == 0 {
        return pinocchio_system::instructions::CreateAccount {
            from: payer,
            to: target,
            lamports: rent,
            space: space as u64,
            owner,
        }
        .invoke_signed(&signers);
    }
    if !target.owned_by(&pinocchio_system::ID) || target.data_len() != 0 {
        return Err(NhError::BadAccount.into());
    }
    let top_up = rent.saturating_sub(current);
    if top_up > 0 {
        pinocchio_system::instructions::Transfer { from: payer, to: target, lamports: top_up }.invoke()?;
    }
    pinocchio_system::instructions::Allocate { account: target, space: space as u64 }.invoke_signed(&signers)?;
    pinocchio_system::instructions::Assign { account: target, owner }.invoke_signed(&signers)
}

/// Move `amount` out of the vault (campaign PDA signs) and optionally close it.
fn pay_out_of_vault(
    c: &Camp,
    vault: &AccountView,
    dest: &AccountView,
    campaign: &AccountView,
    amount: u64,
    close_to: Option<&AccountView>,
) -> ProgramResult {
    let sponsor = c.k(off::SPONSOR);
    let id = c.id_le();
    let bump = [c.d[off::BUMP]];
    let seeds = [
        Seed::from(SEED_CAMPAIGN),
        Seed::from(&sponsor),
        Seed::from(&id),
        Seed::from(&bump),
    ];
    let signers = [Signer::from(&seeds)];
    if amount > 0 {
        pinocchio_token::instructions::Transfer::new(vault, dest, campaign, amount).invoke_signed(&signers)?;
    }
    if let Some(to) = close_to {
        pinocchio_token::instructions::CloseAccount::new(vault, to, campaign).invoke_signed(&signers)?;
    }
    Ok(())
}

// ---------------------------------------------------------------- handlers

fn create_campaign(program_id: &Address, accounts: &mut [AccountView], data: &[u8]) -> ProgramResult {
    let [sponsor, campaign, vault, mint, sponsor_token, sys, tok, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    let mut r = Data(data);
    let campaign_id = r.u64()?;
    let operator = r.b32()?;
    let auditor = r.b32()?;
    let oracle = r.b32()?;
    let p = CreateParams {
        freeze_ts: r.i64()?,
        window_end_ts: r.i64()?,
        amount_full: r.u64()?,
        amount_half: r.u64()?,
        thr_full: r.i32()?,
        thr_half: r.i32()?,
        deposit: 0,
        pledge: false,
    };
    let terms_hash = r.b32()?;
    let deposit = r.u64()?;
    let pledge = match r.u8()? {
        0 => false,
        1 => true,
        _ => return Err(NhError::InvalidData.into()),
    };
    r.end()?;
    let p = CreateParams { deposit, pledge, ..p };

    signer(sponsor)?;
    writable(sponsor)?;
    writable(campaign)?;
    writable(vault)?;
    system_program(sys)?;
    token_program(tok)?;
    if !mint.owned_by(&pinocchio_token::ID) || mint.data_len() != MINT_LEN {
        return Err(NhError::BadMint.into());
    }
    e(logic::validate_create(now()?, &p))?;
    // Distinct roles: the oracle must be independent from operator (audit P0-2).
    if oracle == operator || auditor == operator || auditor == oracle {
        return Err(NhError::BadParams.into());
    }

    let id_le = campaign_id.to_le_bytes();
    let sponsor_key = *sponsor.address().as_array();
    let (camp_addr, bump) = Address::find_program_address(&[SEED_CAMPAIGN, &sponsor_key, &id_le], program_id);
    if campaign.address() != &camp_addr {
        return Err(NhError::BadPda.into());
    }
    let camp_key = *camp_addr.as_array();
    let (vault_addr, vault_bump) = Address::find_program_address(&[SEED_VAULT, &camp_key], program_id);
    if vault.address() != &vault_addr {
        return Err(NhError::BadPda.into());
    }

    let bump_b = [bump];
    create_pda(
        sponsor,
        campaign,
        CAMPAIGN_LEN,
        program_id,
        &[Seed::from(SEED_CAMPAIGN), Seed::from(&sponsor_key), Seed::from(&id_le), Seed::from(&bump_b)],
    )?;
    let vbump_b = [vault_bump];
    create_pda(
        sponsor,
        vault,
        TOKEN_ACCOUNT_LEN,
        &pinocchio_token::ID,
        &[Seed::from(SEED_VAULT), Seed::from(&camp_key), Seed::from(&vbump_b)],
    )?;
    pinocchio_token::instructions::InitializeAccount3::new(vault, mint, &camp_addr).invoke()?;

    if deposit > 0 {
        let (smint, _, _) = read_token_account(sponsor_token)?;
        if &smint != mint.address().as_array() {
            return Err(NhError::BadMint.into());
        }
        pinocchio_token::instructions::Transfer::new(sponsor_token, vault, sponsor, deposit).invoke()?;
    }

    let mut d = [0u8; CAMPAIGN_LEN];
    d[off::TAG] = CAMPAIGN_TAG;
    d[off::BUMP] = bump;
    d[off::STATUS] = status::OPEN;
    d[off::FLAGS] = if pledge { FLAG_PLEDGE } else { 0 };
    d[off::VAULT_BUMP] = vault_bump;
    d[off::RULE_VERSION] = RULE_VERSION;
    wr(&mut d, off::CAMPAIGN_ID, &id_le);
    wr(&mut d, off::SPONSOR, &sponsor_key);
    wr(&mut d, off::OPERATOR, &operator);
    wr(&mut d, off::AUDITOR, &auditor);
    wr(&mut d, off::ORACLE, &oracle);
    wr(&mut d, off::MINT, mint.address().as_array());
    wr(&mut d, off::FREEZE_TS, &p.freeze_ts.to_le_bytes());
    wr(&mut d, off::WINDOW_END_TS, &p.window_end_ts.to_le_bytes());
    wr(&mut d, off::AMOUNT_FULL, &p.amount_full.to_le_bytes());
    wr(&mut d, off::AMOUNT_HALF, &p.amount_half.to_le_bytes());
    wr(&mut d, off::THR_FULL, &p.thr_full.to_le_bytes());
    wr(&mut d, off::THR_HALF, &p.thr_half.to_le_bytes());
    wr(&mut d, off::TERMS_HASH, &terms_hash);
    save_campaign(campaign, &Camp { d })
}

fn lock_roster(program_id: &Address, accounts: &mut [AccountView], data: &[u8]) -> ProgramResult {
    let [operator, campaign, vault, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    let mut r = Data(data);
    let units = r.u32()?;
    let root = r.b32()?;
    r.end()?;

    signer(operator)?;
    let mut c = load_campaign(program_id, campaign)?;
    expect_key(operator, &c.k(off::OPERATOR))?;
    let bal = check_vault(program_id, &c, campaign, vault)?;
    e(logic::guard_lock_roster(c.status(), now()?, c.i64(off::FREEZE_TS), units, c.pledge(), bal, c.u64(off::AMOUNT_FULL)))?;
    if root == [0u8; 32] {
        return Err(NhError::BadParams.into());
    }
    // Units and root are committed together, atomically (audit P0-1).
    wr(&mut c.d, off::UNITS, &units.to_le_bytes());
    wr(&mut c.d, off::ROSTER_ROOT, &root);
    save_campaign(campaign, &c)
}

fn settle(program_id: &Address, accounts: &mut [AccountView], data: &[u8]) -> ProgramResult {
    let [operator, oracle, campaign, rest @ ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    let mut r = Data(data);
    let observed = r.i32()?;
    let data_hash = r.b32()?;
    r.end()?;

    signer(operator)?;
    signer(oracle)?;
    let mut c = load_campaign(program_id, campaign)?;
    expect_key(operator, &c.k(off::OPERATOR))?;
    expect_key(oracle, &c.k(off::ORACLE))?;
    let auditor_cosigned = match rest.first() {
        Some(a) => {
            expect_key(a, &c.k(off::AUDITOR))?;
            signer(a)?;
            true
        }
        None => false,
    };
    let t = now()?;
    let new_status = e(logic::guard_settle(c.status(), t, c.i64(off::WINDOW_END_TS), c.units(), true, auditor_cosigned))?;
    let out = logic::compute_outcome(observed, c.i32(off::THR_FULL), c.i32(off::THR_HALF));
    c.d[off::STATUS] = new_status;
    c.d[off::FLAGS] = with_outcome(c.flags(), out);
    wr(&mut c.d, off::OBSERVED, &observed.to_le_bytes());
    wr(&mut c.d, off::DATA_HASH, &data_hash);
    wr(&mut c.d, off::SETTLED_TS, &t.to_le_bytes());
    save_campaign(campaign, &c)
}

fn dispute(program_id: &Address, accounts: &mut [AccountView], data: &[u8]) -> ProgramResult {
    let [auditor, campaign, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    Data(data).end()?;
    signer(auditor)?;
    let mut c = load_campaign(program_id, campaign)?;
    expect_key(auditor, &c.k(off::AUDITOR))?;
    e(logic::guard_dispute(c.status(), now()?, c.i64(off::SETTLED_TS)))?;
    c.d[off::STATUS] = status::DISPUTED;
    save_campaign(campaign, &c)
}

fn release(program_id: &Address, accounts: &mut [AccountView], data: &[u8]) -> ProgramResult {
    let [campaign, vault, sponsor_token, sponsor, tok, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    Data(data).end()?;
    let mut c = load_campaign(program_id, campaign)?;
    token_program(tok)?;
    let bal = check_vault(program_id, &c, campaign, vault)?;
    let plan = e(logic::plan_release(
        c.status(),
        now()?,
        c.i64(off::SETTLED_TS),
        c.i64(off::WINDOW_END_TS),
        c.outcome(),
        c.pledge(),
        c.units(),
        c.u64(off::AMOUNT_FULL),
        c.u64(off::AMOUNT_HALF),
        bal,
    ))?;
    if plan.refund > 0 || plan.close_vault {
        let sponsor_key = c.k(off::SPONSOR);
        expect_key(sponsor, &sponsor_key)?;
        writable(sponsor)?;
        check_dest_token(&c, sponsor_token, Some(&sponsor_key))?;
        pay_out_of_vault(&c, vault, sponsor_token, campaign, plan.refund, plan.close_vault.then_some(&*sponsor))?;
    }
    c.d[off::STATUS] = plan.new_status;
    save_campaign(campaign, &c)
}

fn post_receipts(program_id: &Address, accounts: &mut [AccountView], data: &[u8]) -> ProgramResult {
    let [operator, campaign, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    let mut r = Data(data);
    let root = r.b32()?;
    r.end()?;
    signer(operator)?;
    let mut c = load_campaign(program_id, campaign)?;
    expect_key(operator, &c.k(off::OPERATOR))?;
    e(logic::guard_post_receipts(c.status(), c.outcome()))?;
    wr(&mut c.d, off::RECEIPTS_ROOT, &root);
    c.d[off::STATUS] = status::RECEIPTED;
    save_campaign(campaign, &c)
}

fn claim(program_id: &Address, accounts: &mut [AccountView], data: &[u8]) -> ProgramResult {
    let [claimant, campaign, vault, claimant_token, receipt, sys, tok, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    let mut r = Data(data);
    let index = r.u32()?;
    let proof = r.0;

    signer(claimant)?;
    writable(claimant)?;
    writable(receipt)?;
    system_program(sys)?;
    token_program(tok)?;
    let c = load_campaign(program_id, campaign)?;
    let bal = check_vault(program_id, &c, campaign, vault)?;
    let amount = e(logic::guard_claim(
        c.status(),
        c.outcome(),
        c.pledge(),
        index,
        c.units(),
        c.u64(off::AMOUNT_FULL),
        c.u64(off::AMOUNT_HALF),
    ))?;
    let camp_key = *campaign.address().as_array();
    let claimant_key = *claimant.address().as_array();
    let leaf = merkle::leaf(&camp_key, &claimant_key, index);
    if !merkle::verify(&c.k(off::ROSTER_ROOT), leaf, proof) {
        return Err(NhError::BadProof.into());
    }
    check_dest_token(&c, claimant_token, Some(&claimant_key))?;

    // Double-claim guard: one receipt PDA per (campaign, roster index).
    let idx_le = index.to_le_bytes();
    let (rcpt_addr, rbump) = Address::find_program_address(&[SEED_CLAIM, &camp_key, &idx_le], program_id);
    if receipt.address() != &rcpt_addr {
        return Err(NhError::BadPda.into());
    }
    if receipt.owned_by(program_id) {
        return Err(NhError::AlreadyClaimed.into());
    }
    if bal < amount {
        return Err(NhError::Underfunded.into());
    }
    let rb = [rbump];
    create_pda(
        claimant,
        receipt,
        CLAIM_RECEIPT_LEN,
        program_id,
        &[Seed::from(SEED_CLAIM), Seed::from(&camp_key), Seed::from(&idx_le), Seed::from(&rb)],
    )?;
    receipt.try_borrow_mut()?[0] = 1;

    pay_out_of_vault(&c, vault, claimant_token, campaign, amount, None)
}

fn sweep(program_id: &Address, accounts: &mut [AccountView], data: &[u8]) -> ProgramResult {
    let [campaign, vault, sponsor_token, sponsor, tok, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    Data(data).end()?;
    let mut c = load_campaign(program_id, campaign)?;
    token_program(tok)?;
    let bal = check_vault(program_id, &c, campaign, vault)?;
    e(logic::guard_sweep(c.status(), c.outcome(), c.pledge(), now()?, c.i64(off::SETTLED_TS)))?;
    let sponsor_key = c.k(off::SPONSOR);
    expect_key(sponsor, &sponsor_key)?;
    writable(sponsor)?;
    check_dest_token(&c, sponsor_token, Some(&sponsor_key))?;
    pay_out_of_vault(&c, vault, sponsor_token, campaign, bal, Some(sponsor))?;
    c.d[off::STATUS] = status::CLOSED;
    save_campaign(campaign, &c)
}
