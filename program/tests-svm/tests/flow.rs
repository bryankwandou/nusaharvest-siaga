//! End-to-end tests: load the compiled `nusaharvest_siaga.so` into LiteSVM and drive
//! the full campaign lifecycle with real SPL Token CPIs.
//! Build first: `cargo build-sbf` in `program/`.

use litesvm::LiteSVM;
use sha2::{Digest, Sha256};
use solana_account::Account;
use solana_address::Address;
use solana_clock::Clock;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_signer::Signer;
use solana_transaction::Transaction;

const TOKEN: Address = Address::from_str_const("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
const SYSTEM: Address = Address::from_str_const("11111111111111111111111111111111");
const DAY: i64 = 86_400;
const AMOUNT_FULL: u64 = 1_000_000;
const AMOUNT_HALF: u64 = 500_000;
const CAMPAIGN_ID: u64 = 42;
const T0: i64 = 1_800_000_000;

// error codes
const TOO_EARLY: u32 = 8;
const TOO_LATE: u32 = 9;
const WRONG_STATUS: u32 = 7;
const BAD_PROOF: u32 = 13;
const ALREADY_CLAIMED: u32 = 14;
const BAD_ACCOUNT: u32 = 15;
const MISSING_SIGNER: u32 = 3;

fn sha(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}
fn leaf(camp: &Address, who: &Address, i: u32) -> [u8; 32] {
    sha(&[b"NH1", camp.as_ref(), who.as_ref(), &i.to_le_bytes()])
}
fn node(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let (l, r) = if a <= b { (a, b) } else { (b, a) };
    sha(&[b"NHN", l, r])
}
fn build_tree(leaves: &[[u8; 32]]) -> ([u8; 32], Vec<Vec<u8>>) {
    let mut proofs = vec![Vec::new(); leaves.len()];
    let mut pos: Vec<usize> = (0..leaves.len()).collect();
    let mut level = leaves.to_vec();
    while level.len() > 1 {
        let next: Vec<_> = level
            .chunks(2)
            .map(|c| if c.len() == 2 { node(&c[0], &c[1]) } else { c[0] })
            .collect();
        for (li, p) in pos.iter_mut().enumerate() {
            if (*p ^ 1) < level.len() {
                proofs[li].extend_from_slice(&level[*p ^ 1]);
            }
            *p /= 2;
        }
        level = next;
    }
    (level[0], proofs)
}

struct Env {
    svm: LiteSVM,
    pid: Address,
    sponsor: Keypair,
    operator: Keypair,
    oracle: Keypair,
    auditor: Keypair,
    farmers: Vec<Keypair>,
    mint: Address,
    campaign: Address,
    vault: Address,
    sponsor_tok: Address,
    farmer_toks: Vec<Address>,
}

fn mint_data() -> Vec<u8> {
    let mut d = vec![0u8; 82];
    d[36..44].copy_from_slice(&u64::MAX.to_le_bytes());
    d[44] = 6;
    d[45] = 1;
    d
}
fn token_data(mint: &Address, owner: &Address, amount: u64) -> Vec<u8> {
    let mut d = vec![0u8; 165];
    d[0..32].copy_from_slice(mint.as_ref());
    d[32..64].copy_from_slice(owner.as_ref());
    d[64..72].copy_from_slice(&amount.to_le_bytes());
    d[108] = 1;
    d
}
fn put(svm: &mut LiteSVM, key: Address, owner: Address, data: Vec<u8>) {
    svm.set_account(key, Account { lamports: 10_000_000, data, owner, executable: false, rent_epoch: 0 })
        .unwrap();
}
fn token_amount(svm: &LiteSVM, k: &Address) -> u64 {
    let a = svm.get_account(k).unwrap();
    u64::from_le_bytes(a.data[64..72].try_into().unwrap())
}

impl Env {
    fn new() -> Self {
        let mut svm = LiteSVM::new();
        let pid = Address::new_unique();
        svm.add_program_from_file(pid, concat!(env!("CARGO_MANIFEST_DIR"), "/../target/deploy/nusaharvest_siaga.so"))
            .expect("run `cargo build-sbf` in program/ first");
        let kp = || Keypair::new();
        let (sponsor, operator, oracle, auditor) = (kp(), kp(), kp(), kp());
        let farmers: Vec<Keypair> = (0..3).map(|_| kp()).collect();
        for k in [&sponsor, &operator, &oracle, &auditor].into_iter().chain(farmers.iter()) {
            svm.airdrop(&k.pubkey(), 10_000_000_000).unwrap();
        }
        let mint = Address::new_unique();
        put(&mut svm, mint, TOKEN, mint_data());
        let sponsor_tok = Address::new_unique();
        put(&mut svm, sponsor_tok, TOKEN, token_data(&mint, &sponsor.pubkey(), 10_000_000));
        let farmer_toks: Vec<Address> = farmers
            .iter()
            .map(|f| {
                let t = Address::new_unique();
                put(&mut svm, t, TOKEN, token_data(&mint, &f.pubkey(), 0));
                t
            })
            .collect();
        let (campaign, _) = Address::find_program_address(
            &[b"camp", sponsor.pubkey().as_ref(), &CAMPAIGN_ID.to_le_bytes()],
            &pid,
        );
        let (vault, _) = Address::find_program_address(&[b"vault", campaign.as_ref()], &pid);
        let mut env = Env { svm, pid, sponsor, operator, oracle, auditor, farmers, mint, campaign, vault, sponsor_tok, farmer_toks };
        env.set_time(T0);
        env
    }

    fn set_time(&mut self, t: i64) {
        let mut c: Clock = self.svm.get_sysvar();
        c.unix_timestamp = t;
        self.svm.set_sysvar(&c);
    }

    fn send(&mut self, ix: Instruction, signers: &[&Keypair]) -> Result<(), String> {
        self.svm.expire_blockhash();
        let payer = signers[0].pubkey();
        let tx = Transaction::new_signed_with_payer(&[ix], Some(&payer), signers, self.svm.latest_blockhash());
        self.svm.send_transaction(tx).map(|_| ()).map_err(|e| format!("{:?}", e.err))
    }

    fn ix(&self, data: Vec<u8>, metas: Vec<AccountMeta>) -> Instruction {
        Instruction { program_id: self.pid, accounts: metas, data }
    }

    fn create(&mut self, deposit: u64) -> Result<(), String> {
        let mut d = vec![0u8];
        d.extend_from_slice(&CAMPAIGN_ID.to_le_bytes());
        d.extend_from_slice(self.operator.pubkey().as_ref());
        d.extend_from_slice(self.auditor.pubkey().as_ref());
        d.extend_from_slice(self.oracle.pubkey().as_ref());
        d.extend_from_slice(&(T0 + 10 * DAY).to_le_bytes()); // freeze
        d.extend_from_slice(&(T0 + 100 * DAY).to_le_bytes()); // window end
        d.extend_from_slice(&AMOUNT_FULL.to_le_bytes());
        d.extend_from_slice(&AMOUNT_HALF.to_le_bytes());
        d.extend_from_slice(&800i32.to_le_bytes()); // thr_full (p10) mm x10
        d.extend_from_slice(&1200i32.to_le_bytes()); // thr_half (p20) mm x10
        d.extend_from_slice(&[9u8; 32]); // terms hash
        d.extend_from_slice(&deposit.to_le_bytes());
        d.push(0);
        let ix = self.ix(
            d,
            vec![
                AccountMeta::new(self.sponsor.pubkey(), true),
                AccountMeta::new(self.campaign, false),
                AccountMeta::new(self.vault, false),
                AccountMeta::new_readonly(self.mint, false),
                AccountMeta::new(self.sponsor_tok, false),
                AccountMeta::new_readonly(SYSTEM, false),
                AccountMeta::new_readonly(TOKEN, false),
            ],
        );
        let s = self.sponsor.insecure_clone();
        self.send(ix, &[&s])
    }

    fn roster(&self) -> ([u8; 32], Vec<Vec<u8>>) {
        let leaves: Vec<_> = self.farmers.iter().enumerate().map(|(i, f)| leaf(&self.campaign, &f.pubkey(), i as u32)).collect();
        build_tree(&leaves)
    }

    fn lock(&mut self, signer: &Keypair, units: u32) -> Result<(), String> {
        let (root, _) = self.roster();
        let mut d = vec![1u8];
        d.extend_from_slice(&units.to_le_bytes());
        d.extend_from_slice(&root);
        let ix = self.ix(d, vec![
            AccountMeta::new(signer.pubkey(), true),
            AccountMeta::new(self.campaign, false),
            AccountMeta::new_readonly(self.vault, false),
        ]);
        self.send(ix, &[signer])
    }

    fn settle(&mut self, op: &Keypair, oracle: &Keypair, auditor: Option<&Keypair>, observed: i32) -> Result<(), String> {
        let mut d = vec![2u8];
        d.extend_from_slice(&observed.to_le_bytes());
        d.extend_from_slice(&[7u8; 32]);
        let mut metas = vec![
            AccountMeta::new(op.pubkey(), true),
            AccountMeta::new_readonly(oracle.pubkey(), true),
            AccountMeta::new(self.campaign, false),
        ];
        let mut signers = vec![op, oracle];
        if let Some(a) = auditor {
            metas.push(AccountMeta::new_readonly(a.pubkey(), true));
            signers.push(a);
        }
        let ix = self.ix(d, metas);
        self.send(ix, &signers)
    }

    fn dispute(&mut self, who: &Keypair) -> Result<(), String> {
        let ix = self.ix(vec![3u8], vec![AccountMeta::new(who.pubkey(), true), AccountMeta::new(self.campaign, false)]);
        self.send(ix, &[who])
    }

    fn release_like(&mut self, tag: u8, payer: &Keypair) -> Result<(), String> {
        let ix = self.ix(vec![tag], vec![
            AccountMeta::new(self.campaign, false),
            AccountMeta::new(self.vault, false),
            AccountMeta::new(self.sponsor_tok, false),
            AccountMeta::new(self.sponsor.pubkey(), false),
            AccountMeta::new_readonly(TOKEN, false),
        ]);
        self.send(ix, &[payer])
    }

    fn claim(&mut self, who: &Keypair, index: u32, proof: &[u8], dest: Address) -> Result<(), String> {
        let mut d = vec![6u8];
        d.extend_from_slice(&index.to_le_bytes());
        d.extend_from_slice(proof);
        let (receipt, _) = Address::find_program_address(&[b"claim", self.campaign.as_ref(), &index.to_le_bytes()], &self.pid);
        let ix = self.ix(d, vec![
            AccountMeta::new(who.pubkey(), true),
            AccountMeta::new_readonly(self.campaign, false),
            AccountMeta::new(self.vault, false),
            AccountMeta::new(dest, false),
            AccountMeta::new(receipt, false),
            AccountMeta::new_readonly(SYSTEM, false),
            AccountMeta::new_readonly(TOKEN, false),
        ]);
        self.send(ix, &[who])
    }

    fn status(&self) -> u8 {
        self.svm.get_account(&self.campaign).unwrap().data[2]
    }
}

fn assert_code(r: Result<(), String>, code: u32) {
    let e = r.expect_err("expected failure");
    assert!(e.contains(&format!("Custom({code})")), "expected Custom({code}), got {e}");
}

fn kc(k: &Keypair) -> Keypair {
    k.insecure_clone()
}

#[test]
fn happy_path_half_payout_with_claims() {
    let mut env = Env::new();
    env.create(3 * AMOUNT_FULL + 250_000).unwrap();
    assert_eq!(token_amount(&env.svm, &env.vault), 3_250_000);
    let op = kc(&env.operator);
    env.lock(&op, 3).unwrap();

    env.set_time(T0 + 100 * DAY);
    let (oracle, auditor) = (kc(&env.oracle), kc(&env.auditor));
    env.settle(&op, &oracle, None, 1000).unwrap(); // 100.0 mm: below p20, above p10 -> HALF
    assert_eq!(env.status(), 1);

    env.set_time(T0 + 102 * DAY);
    let anyone = kc(&env.farmers[2]);
    env.release_like(4, &anyone).unwrap();
    assert_eq!(env.status(), 4);
    // reserve = 3 * HALF stays; rest back to sponsor
    assert_eq!(token_amount(&env.svm, &env.vault), 3 * AMOUNT_HALF);
    assert_eq!(token_amount(&env.svm, &env.sponsor_tok), 10_000_000 - 3 * AMOUNT_HALF);

    let (_, proofs) = env.roster();
    for i in 0..3 {
        let f = kc(&env.farmers[i]);
        let dest = env.farmer_toks[i];
        env.claim(&f, i as u32, &proofs[i], dest).unwrap();
        assert_eq!(token_amount(&env.svm, &dest), AMOUNT_HALF);
    }
    assert_eq!(token_amount(&env.svm, &env.vault), 0);
    // vault is now empty: a replay must still report AlreadyClaimed, not Underfunded
    let f0 = kc(&env.farmers[0]);
    let d0 = env.farmer_toks[0];
    assert_code(env.claim(&f0, 0, &proofs[0], d0), ALREADY_CLAIMED);

    let mut d = vec![5u8];
    d.extend_from_slice(&[3u8; 32]);
    let ix = env.ix(d, vec![AccountMeta::new(op.pubkey(), true), AccountMeta::new(env.campaign, false)]);
    env.send(ix, &[&op]).unwrap();
    assert_eq!(env.status(), 5);
    let _ = auditor;
}

#[test]
fn double_claim_rejected() {
    let mut env = Env::new();
    env.create(3 * AMOUNT_FULL).unwrap();
    let (op, oracle) = (kc(&env.operator), kc(&env.oracle));
    env.lock(&op, 3).unwrap();
    env.set_time(T0 + 100 * DAY);
    env.settle(&op, &oracle, None, 500).unwrap(); // FULL
    env.set_time(T0 + 103 * DAY);
    env.release_like(4, &op).unwrap();
    let (_, proofs) = env.roster();
    let f = kc(&env.farmers[0]);
    let dest = env.farmer_toks[0];
    env.claim(&f, 0, &proofs[0], dest).unwrap();
    assert_code(env.claim(&f, 0, &proofs[0], dest), ALREADY_CLAIMED);
    assert_eq!(token_amount(&env.svm, &dest), AMOUNT_FULL);
}

#[test]
fn wrong_signers_rejected() {
    let mut env = Env::new();
    env.create(3 * AMOUNT_FULL).unwrap();
    let (op, oracle, auditor) = (kc(&env.operator), kc(&env.oracle), kc(&env.auditor));
    let mallory = Keypair::new();
    env.svm.airdrop(&mallory.pubkey(), 1_000_000_000).unwrap();

    assert_code(env.lock(&mallory, 3), BAD_ACCOUNT);
    env.lock(&op, 3).unwrap();
    env.set_time(T0 + 100 * DAY);
    // operator alone with a fake oracle
    assert_code(env.settle(&op, &mallory, None, 500), BAD_ACCOUNT);
    // mallory as operator
    assert_code(env.settle(&mallory, &oracle, None, 500), BAD_ACCOUNT);
    env.settle(&op, &oracle, None, 500).unwrap();
    // only auditor may dispute
    assert_code(env.dispute(&op), BAD_ACCOUNT);

    env.set_time(T0 + 103 * DAY);
    env.release_like(4, &op).unwrap();
    let (_, proofs) = env.roster();
    // mallory replays farmer 0's proof and index into her own token account
    let mal_tok = Address::new_unique();
    let mint = env.mint;
    put(&mut env.svm, mal_tok, TOKEN, token_data(&mint, &mallory.pubkey(), 0));
    assert_code(env.claim(&mallory, 0, &proofs[0], mal_tok), BAD_PROOF);
    // farmer 1 tries to route farmer 0's slot to farmer 1's account using farmer 0's proof
    let f1 = kc(&env.farmers[1]);
    let d1 = env.farmer_toks[1];
    assert_code(env.claim(&f1, 0, &proofs[0], d1), BAD_PROOF);
    // farmer 0 tries to pay into someone else's token account
    let f0 = kc(&env.farmers[0]);
    assert_code(env.claim(&f0, 0, &proofs[0], d1), 5);
    let _ = auditor;
}

#[test]
fn out_of_order_and_early_release_rejected() {
    let mut env = Env::new();
    env.create(3 * AMOUNT_FULL).unwrap();
    let (op, oracle, auditor) = (kc(&env.operator), kc(&env.oracle), kc(&env.auditor));

    // settle before roster / before window end
    env.set_time(T0 + 1);
    assert_code(env.settle(&op, &oracle, None, 500), TOO_EARLY);
    env.lock(&op, 3).unwrap();
    // release while OPEN, before operator no-show grace
    assert_code(env.release_like(4, &op), TOO_EARLY);
    // lock after freeze
    env.set_time(T0 + 11 * DAY);
    assert_code(env.lock(&op, 3), TOO_LATE);
    // claim before settle
    let (_, proofs) = env.roster();
    let f0 = kc(&env.farmers[0]);
    let d0 = env.farmer_toks[0];
    assert_code(env.claim(&f0, 0, &proofs[0], d0), WRONG_STATUS);

    env.set_time(T0 + 100 * DAY);
    env.settle(&op, &oracle, None, 500).unwrap();
    // settle twice
    assert_code(env.settle(&op, &oracle, None, 500), WRONG_STATUS);
    // release inside the 48h dispute window
    env.set_time(T0 + 101 * DAY);
    assert_code(env.release_like(4, &op), TOO_EARLY);
    // claim still not allowed
    assert_code(env.claim(&f0, 0, &proofs[0], d0), WRONG_STATUS);
    // sweep far too early
    assert_code(env.release_like(7, &op), WRONG_STATUS);

    // dispute blocks release; re-settle needs auditor co-sign
    env.dispute(&auditor).unwrap();
    env.set_time(T0 + 105 * DAY);
    assert_code(env.release_like(4, &op), TOO_EARLY);
    assert_code(env.settle(&op, &oracle, None, 1500), MISSING_SIGNER);
    env.settle(&op, &oracle, Some(&auditor), 1500).unwrap(); // corrected: no payout
    assert_eq!(env.status(), 3);
    env.release_like(4, &op).unwrap();
    assert_eq!(env.status(), 4);
    assert!(env.svm.get_account(&env.vault).map(|a| a.lamports == 0).unwrap_or(true), "vault closed");
    assert_eq!(token_amount(&env.svm, &env.sponsor_tok), 10_000_000);
}

#[test]
fn late_dispute_rejected_and_unresolved_dispute_refunds() {
    let mut env = Env::new();
    env.create(3 * AMOUNT_FULL).unwrap();
    let (op, oracle, auditor) = (kc(&env.operator), kc(&env.oracle), kc(&env.auditor));
    env.lock(&op, 3).unwrap();
    env.set_time(T0 + 100 * DAY);
    env.settle(&op, &oracle, None, 500).unwrap();
    env.dispute(&auditor).unwrap();
    env.set_time(T0 + 115 * DAY);
    assert_code(env.release_like(4, &op), TOO_EARLY);
    env.set_time(T0 + 116 * DAY);
    env.release_like(4, &op).unwrap();
    assert_eq!(env.status(), 6);
    assert_eq!(token_amount(&env.svm, &env.sponsor_tok), 10_000_000);

    let mut env = Env::new();
    env.create(3 * AMOUNT_FULL).unwrap();
    let (op, oracle, auditor) = (kc(&env.operator), kc(&env.oracle), kc(&env.auditor));
    env.lock(&op, 3).unwrap();
    env.set_time(T0 + 100 * DAY);
    env.settle(&op, &oracle, None, 500).unwrap();
    env.set_time(T0 + 102 * DAY);
    assert_code(env.dispute(&auditor), TOO_LATE);
}

#[test]
fn unclaimed_funds_swept_after_claim_period() {
    let mut env = Env::new();
    env.create(3 * AMOUNT_FULL).unwrap();
    let (op, oracle) = (kc(&env.operator), kc(&env.oracle));
    env.lock(&op, 3).unwrap();
    env.set_time(T0 + 100 * DAY);
    env.settle(&op, &oracle, None, 500).unwrap();
    env.set_time(T0 + 102 * DAY);
    env.release_like(4, &op).unwrap();
    let (_, proofs) = env.roster();
    let f0 = kc(&env.farmers[0]);
    let d0 = env.farmer_toks[0];
    env.claim(&f0, 0, &proofs[0], d0).unwrap();
    env.set_time(T0 + 180 * DAY);
    assert_code(env.release_like(7, &op), TOO_EARLY);
    env.set_time(T0 + 192 * DAY);
    env.release_like(7, &op).unwrap();
    assert_eq!(env.status(), 7);
    assert_eq!(token_amount(&env.svm, &env.sponsor_tok), 10_000_000 - AMOUNT_FULL);
}

#[test]
fn griefed_prefunded_claim_receipt_still_claimable() {
    let mut env = Env::new();
    env.create(3 * AMOUNT_FULL).unwrap();
    let (op, oracle) = (kc(&env.operator), kc(&env.oracle));
    env.lock(&op, 3).unwrap();
    env.set_time(T0 + 100 * DAY);
    env.settle(&op, &oracle, None, 500).unwrap();
    env.set_time(T0 + 102 * DAY);
    env.release_like(4, &op).unwrap();
    let (receipt, _) = Address::find_program_address(&[b"claim", env.campaign.as_ref(), &0u32.to_le_bytes()], &env.pid);
    env.svm.set_account(receipt, Account { lamports: 1_000, data: vec![], owner: SYSTEM, executable: false, rent_epoch: 0 }).unwrap();
    let (_, proofs) = env.roster();
    let f0 = kc(&env.farmers[0]);
    let d0 = env.farmer_toks[0];
    env.claim(&f0, 0, &proofs[0], d0).unwrap();
    assert_code(env.claim(&f0, 0, &proofs[0], d0), ALREADY_CLAIMED);
}
