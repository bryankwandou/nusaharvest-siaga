# NusaHarvest Siaga Tanam: on-chain program (pure Pinocchio)

This is the Solana program for rainfall-triggered payouts. It uses Pinocchio 0.11.2, `pinocchio-system` and `pinocchio-token`, with no Anchor, no Borsh, no allocator, and `no_std` on-chain. The `.so` is about 41 KB.

**Devnet program ID: `GeCxWiK9HXEuEsueNQawDKZnwE1SkojPw9dtYLBkcGBX`**
([explorer](https://explorer.solana.com/address/GeCxWiK9HXEuEsueNQawDKZnwE1SkojPw9dtYLBkcGBX?cluster=devnet))

It is based on `research/02-FLOW-DAN-SKEMA-FINAL.md`. Where this code differs from that spec, the change is listed under "Deviations from the spec" below.

## Layout

| Path | What |
|---|---|
| `src/state.rs` | Campaign account, 368 bytes, with byte offsets that match spec 3.1. Also PDA seeds, status and outcome codes |
| `src/logic.rs` | Pure state-machine guards and checked arithmetic. Unit-tested without an SVM |
| `src/merkle.rs` | Roster leaf and node hashing, plus proof verification (sha256 syscall on-chain) |
| `src/processor.rs` | Account checks, calls into the guards, CPIs and state writes |
| `tests-svm/` | A separate crate. It loads the built `.so` into LiteSVM 0.16 and runs real SPL Token CPIs |
| `scripts/devnet-e2e.mjs` | End-to-end flow on devnet |

## Instructions

The first byte of instruction data is the tag. All integers are little-endian.

| Tag | Name | Signers | Accounts | Data |
|---|---|---|---|---|
| 0 | CreateCampaign | sponsor | sponsor(w), campaign PDA(w), vault PDA(w), mint, sponsor_token(w), system, token | campaign_id u64, operator 32, auditor 32, oracle 32, freeze_ts i64, window_end_ts i64, amount_full u64, amount_half u64, thr_full i32, thr_half i32, terms_hash 32, deposit u64, pledge u8 |
| 1 | LockRoster | operator | operator, campaign(w), vault | units u32, roster_root 32 |
| 2 | Settle | operator + oracle (+ auditor if DISPUTED) | operator, oracle, campaign(w), [auditor] | observed_mm10 i32, data_hash 32 |
| 3 | Dispute | auditor | auditor, campaign(w) | none |
| 4 | Release | anyone | campaign(w), vault(w), sponsor_token(w), sponsor(w), token | none |
| 5 | PostReceipts | operator | operator, campaign(w) | receipts_root 32 |
| 6 | Claim | farmer | farmer(w), campaign, vault(w), farmer_token(w), claim_receipt PDA(w), system, token | index u32, proof (n x 32 bytes, n <= 24) |
| 7 | Sweep | anyone | campaign(w), vault(w), sponsor_token(w), sponsor(w), token | none |

PDAs:
- campaign: `["camp", sponsor, campaign_id_le8]`
- vault: `["vault", campaign]`. This is an SPL token account whose owner is the campaign PDA.
- claim receipt: `["claim", campaign, index_le4]`

Roster merkle:
- `leaf = sha256("NH1" || campaign || farmer_wallet || index_u32_le)`
- `node = sha256("NHN" || min(a,b) || max(a,b))`

State machine:

```
OPEN --LockRoster (before freeze_ts, repeatable, units x amount_full <= vault)--> OPEN
OPEN --Settle (after window_end_ts, units > 0, operator+oracle)--> SETTLED
OPEN --Release (window_end_ts + 30d, operator no-show)--> REFUNDED (all to sponsor, vault closed)
SETTLED --Dispute (auditor, < settled_ts + 48h)--> DISPUTED
DISPUTED --Settle (operator+oracle+auditor)--> SETTLED_FINAL
DISPUTED --Release (settled_ts + 48h + 14d, nobody resolved)--> REFUNDED
SETTLED --Release (>= settled_ts + 48h)--> RELEASED
SETTLED_FINAL --Release--> RELEASED
RELEASED: outcome NONE -> everything back to sponsor, vault closed
          outcome FULL/HALF -> units x payout reserved in vault, excess refunded; farmers Claim
RELEASED --PostReceipts--> RECEIPTED   (Claim still allowed)
RELEASED|RECEIPTED --Sweep (settled_ts + 48h + 90d)--> CLOSED (leftover to sponsor, vault closed)
```

Outcome rules (rainfall is in mm x 10):
- `observed <= thr_full` (p10) pays FULL.
- `observed <= thr_half` (p20) pays HALF.
- Anything higher pays NONE.

## Security checks

- **Signer and owner checks.** Every handler checks the signer flag of each role account and compares the account against the pubkey stored in the campaign. The campaign must be owned by the program, exactly 368 bytes, and have tag 1. Token accounts must be owned by SPL Token, 165 bytes, initialized, with the correct mint. Destination token accounts must belong to the sponsor or the claimant. The system and token program IDs are pinned.
- **PDA re-derivation.** The campaign address is re-derived from the stored sponsor, id and bump on every call. The vault is re-derived from the campaign and the stored vault bump. The receipt PDA is found canonically. CreateCampaign checks canonical bumps.
- **No double claim.** Each (campaign, index) gets a claim-receipt PDA, and a second claim fails with `AlreadyClaimed` (14). The leaf commits to the claimant wallet and the index, so nobody else can use a proof. If someone sends lamports to the receipt address ahead of time, `create_pda` tops it up and runs allocate + assign, so that grief does not block the claim.
- **Checked arithmetic.** `units x amount`, the reserve and refund math, and every timestamp addition use `checked_*`. The release profile also sets `overflow-checks = true`.
- **Order guards.** Each instruction checks the status and the clock in `logic.rs`. Early, late and out-of-order calls fail with `WrongStatus` (7), `TooEarly` (8) or `TooLate` (9).
- **Error codes.** Spec codes 1 to 12, plus 13 BadProof, 14 AlreadyClaimed and 15 BadAccount.

## Audit P0 items (docs/audit/threat-model.md)

| # | Item | Status |
|---|---|---|
| 1 | LockRoster commits the unit count together with the root; per-farmer claims instead of a lump sum to a disburser | **Done.** `units` and `roster_root` are written atomically. Release keeps `units x payout` in the vault, and each farmer pulls funds with `Claim` and a merkle proof. |
| 2 | Settle must not rest on one operator key | **Done.** Every Settle needs the operator plus an independent `oracle` key. A re-settle after a dispute also needs the auditor. CreateCampaign rejects campaigns where operator, oracle and auditor are not all different. |
| 3 | Dispute timeout so funds cannot lock forever | **Done.** If a DISPUTED campaign is not re-settled within 48h + 14 days of `settled_ts`, anyone can call Release. It moves to REFUNDED and the sponsor gets everything back. |
| 4 | Return path for unclaimed funds after a deadline | **Done.** `Sweep` (tag 7) sends what is left to the sponsor and closes the vault once 90 days have passed after the dispute window. Status becomes CLOSED. |

## Deviations from the spec

1. **On-chain claims.** Spec 3.2 says farmers do not claim on-chain. Per the task brief and audit P0-1, Release no longer pays a `disburser` key. Farmers now claim with a merkle proof. Because of that, the spec's phone-hash leaf is replaced by a leaf that commits to the farmer's wallet.
2. **Offset 120 renamed.** Offset 120 was `disburser` and is now `oracle`, the Settle co-signer. The total size is still 368 bytes.
3. **New states and instructions.** Tags 6 (Claim) and 7 (Sweep) are new. So is status 7 (CLOSED), plus the timeout transition from DISPUTED to REFUNDED.
4. **Vault owner.** The vault's token owner is the campaign PDA, so the campaign PDA signs transfers out of the vault.
5. **PLEDGE mode.** PLEDGE works as in the spec: Release only changes the status. Claim and Sweep are rejected for PLEDGE campaigns because there are no escrowed funds.

## Test output

### Unit tests (`cargo test` in `program/`, rustc 1.89)

```
test logic::tests::create_validation ... ok
test logic::tests::dispute_window ... ok
test logic::tests::outcome_thresholds ... ok
test logic::tests::release_plans ... ok
test logic::tests::claim_and_receipts_guards ... ok
test logic::tests::early_release_rejected ... ok
test logic::tests::settle_ordering ... ok
test logic::tests::lock_roster_guards ... ok
test merkle::tests::proofs_verify_and_tamper_fails ... ok
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### LiteSVM integration (`cargo build-sbf` in `program/`, then `cargo test` in `tests-svm/`)

`tests-svm` pins rustc 1.98.1 through `rust-toolchain.toml`, because LiteSVM 0.16 needs newer std APIs.

```
test griefed_prefunded_claim_receipt_still_claimable ... ok
test happy_path_half_payout_with_claims ... ok
test unclaimed_funds_swept_after_claim_period ... ok
test wrong_signers_rejected ... ok
test double_claim_rejected ... ok
test out_of_order_and_early_release_rejected ... ok
test late_dispute_rejected_and_unresolved_dispute_refunds ... ok
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

What the integration tests cover:
- **Happy path.** Create, lock, Settle HALF, Release (excess refunded), 3 claims, PostReceipts.
- **Double claim.** Replays are rejected, including after the vault is empty.
- **Wrong signers.** A non-operator LockRoster, a fake oracle, a fake operator, a non-auditor Dispute, a stolen proof, and a claim paid into someone else's token account are all rejected.
- **Out-of-order calls.** Settle before the window ends, LockRoster after freeze, Claim before Release, a second Settle, Release inside the 48h window, and Release while DISPUTED are all rejected. A re-settle without the auditor is rejected, and one with the auditor succeeds.
- **Dispute timing.** A late Dispute is rejected. An unresolved dispute refunds the sponsor.
- **Sweep.** Rejected before the deadline, succeeds after it.
- **Pre-funded receipt PDA.** A claim still succeeds when the receipt address was pre-funded.

## Devnet deployment and end-to-end run

- **Deploy.** The program was deployed with the existing `~/.config/solana/id.json` as the fee payer. The CLI's default keypair was not used. The deploy signature was `2WvXPRME3EvFx93Mu1pyB5BpBtewFEnG68LWebbgvwxMrLyqTqVvL9E8qUyHrNzs5jfGGemuo2zm1RACPzbykokY`. After a small fix to the order of the claim checks, the program was extended and upgraded (`5aBFMyvdSJFyfxfkszMHAMW4mxF7o2eH1dkJpvyK7vcwaTfaYLNE5gbcibwNSqDtpzkvAQj9Z9osGMFG86EcSF5s`).
- **E2E payer.** The run used a throwaway key, `GanYsHJ6vpiZHy8EvvUsuYRAGpevKi2S8hASDYtitBkL`, stored at `program/.devnet-deployer.json` (gitignored). The devnet faucet was rate-limited, so this key was funded with 0.15 devnet SOL from `id.json`.
- **Command:** `NODE_PATH=<node_modules with @solana/web3.js> node scripts/devnet-e2e.mjs .devnet-deployer.json GeCxWiK9HXEuEsueNQawDKZnwE1SkojPw9dtYLBkcGBX`

The run used a test mint with 6 decimals. amount_full was 2.0 tokens, the deposit was 4.5 tokens, and there were 2 farmers. `observed` was 55.0 mm, below the p10 floor, so the outcome was FULL. The 48h dispute window cannot be waited out in a test. The script instead takes the path the design already allows: the auditor disputes, then operator + oracle + auditor re-settle to SETTLED_FINAL, and Release works immediately. There is no test-only bypass in the program.

Result:
- Each farmer received 2,000,000 base units.
- The sponsor got the 500,000 excess back, so their balance went from 10,000,000 to 6,000,000.
- The vault ended at 0.
- The double claim was rejected with `custom program error: 0xe` (AlreadyClaimed).

Campaign `3BF7hWtByRPUiPVqk5NkhQTo7Tx6Af1CA6jzcem1nxYh`, vault `3sFhPSbRNAefHpMZuG57xJDy9up9P8opgK8KDGxyNDKJ`.

| Step | Signature |
|---|---|
| CreateCampaign | [32VQGHQu...](https://explorer.solana.com/tx/32VQGHQuKM1FfD5iuneYzHs7fx2H2e78pXoZbEGxBLijHugMEpYqAm84YcgBmMY8Xxg3744W8JT1wbxpF58SupV6?cluster=devnet) |
| LockRoster | [4P4fkBkR...](https://explorer.solana.com/tx/4P4fkBkR1gSEckH8ze5BHeXZgtRY6NepNDa8Q2WqjsBp3MbvfhEHaC9Te35wBXV89bExAc9sVgXTSTdW55i77CLz?cluster=devnet) |
| Settle (55.0 mm, below floor) | [5uGx8k1h...](https://explorer.solana.com/tx/5uGx8k1h97i6mNS3TuxfkT3eWCvdCbRZ7Q1eNpT8Ndy2p9oVEgVyLdbwhgNGF6HMAXLPREzPYyeKhoj2sCJw76xo?cluster=devnet) |
| Dispute | [hwJrqWj4...](https://explorer.solana.com/tx/hwJrqWj4rDwDvyHv3sDjD3ijBJ2rKyUkXzDqbL9zjVsYaCsuGyBNkoELK1gVc9STBwhmFcFoM1bfFD5ZVwzRN11?cluster=devnet) |
| Settle final (3 signers) | [5XZgLUSb...](https://explorer.solana.com/tx/5XZgLUSbagLhg7Jw4bt425PmYwNWVnjxRgWgM42Ev9gbQhAnbhrDXpTVherDVB2r9SgTg71irozCxoKc7UbAiYtg?cluster=devnet) |
| Release | [XkDrdA3p...](https://explorer.solana.com/tx/XkDrdA3pxUbjKukybXa2P8nLWL6ZKAtuqiu98zrKiJsBpTPEZDGMkEojYyyqxUR45xK56iY4wdNtfhsVfMragBM?cluster=devnet) |
| Claim farmer 0 | [5LBGkfwD...](https://explorer.solana.com/tx/5LBGkfwDSGa533Qxk5wfTBjjbvoorTvKRoPEew1p675ye4odnaw7jB1vjhCgSFwVrLGCiWFbCeTbY4k9FaF969aZ?cluster=devnet) |
| Claim farmer 1 | [2tTc6efg...](https://explorer.solana.com/tx/2tTc6efgKdFDET5jQDKn98NWth6qrkBexdM595hn3TdaKd4WZGnxiyubkKpzKVwiuoZEpAC2X1h772qQe22uoR4g?cluster=devnet) |

## Not done / known limits

- **Upgrade authority.** The upgrade authority is the `id.json` key. Before anything real it should move to a multisig, or the program should be made immutable.
- **Oracle trust.** The oracle is a single co-signing key. Nothing on-chain verifies the rainfall data; only `data_hash` is recorded.
- **Token-2022.** Not supported. Only the legacy SPL Token program is accepted.
- **PostReceipts.** It only records a root. Nothing on-chain checks it.
- **Review status.** No external audit, fuzzing or formal compute-unit budgeting has been done. The largest instruction, Claim, has not been profiled for compute units.
- **Frontend and backend.** Nothing in `web/` is wired to this program yet.
- **pinocchio-kit.** It is not used by this program. Two compile errors in it were fixed (the `pinocchio-token` 0.7 type-alias generics and a test `unwrap_err` on a non-`Debug` type), and its `cargo test` now passes 7 tests.
