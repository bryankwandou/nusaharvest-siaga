# Threat Model: NusaHarvest Siaga Tanam On-Chain Design

Date: 16 September 2026
Scope: the design in `research/02-FLOW-DAN-SKEMA-FINAL.md` (program spec RULE_VERSION 1, six instructions, Campaign account, vault PDA) and the off-chain components it depends on (settlement worker, payout worker, WhatsApp bot, proof API). No program code exists yet, so this is a design review. Every finding below should turn into a test once the code exists.

## Severity scale

- **P0**: fund loss, unauthorized access, incorrect deductions, failed withdrawals (funds stuck), or duplicate payouts.
- **P1**: serious integrity or availability failure that does not directly move funds wrongly, or a P0 that needs an unlikely precondition.
- **P2**: weakness that makes an attack or mistake easier, or damages auditability.
- **P3**: hardening, clarity, hygiene.

## Trust assumptions in the current design

| Actor | Key | Powers | What happens if compromised or dishonest |
|---|---|---|---|
| Sponsor | sponsor wallet (or NusaHarvest, in PLEDGE mode) | CreateCampaign, chooses operator, auditor, disburser, mint | Sets all other roles. In PLEDGE mode this is NusaHarvest. |
| Operator | operator key (hot key on server) | LockRoster (sets `units` and root), Settle (sets `observed`), PostReceipts | Can decide the outcome and how much leaves the vault. |
| Auditor | auditor key | Dispute within 48 hours, co-sign re-settle | Can freeze funds forever (see TM-03). |
| Disburser | disburser key | Receives all released tokens | Can take everything released. |
| Anyone | any wallet | Release | Timing only. |

The core problem: **operator plus disburser equals full control over released funds**, and the auditor is the only brake. If NusaHarvest holds operator and disburser (as the flow implies) and picks the auditor (as PLEDGE mode implies), the "trustless" claim does not hold.

---

## P0 findings

### TM-01. Operator can inflate `units` and drain the vault to the disburser
- **Category:** fund loss, incorrect deductions
- **Scenario:** LockRoster accepts any `units` value as long as vault balance >= `units x amount_full`. The program cannot check that `units` matches the number of leaves in `roster_root`, because there is no on-chain Merkle verification. An operator (compromised server or insider) calls LockRoster with `units` equal to vault balance divided by `amount_full`, and a root that contains fake leaves or only a few real ones. After a FULL or HALF settle, Release sends `units x amount` to the disburser. The real roster may have 200 farmers; the disburser gets money for 1,000.
- **Why the design doesn't catch it:** the off-chain receipts root is posted by the same operator and is never checked against the released amount. Undisbursed money is "returned to the sponsor off-chain", which is a promise, not a mechanism.
- **Mitigation:**
  - Commit the leaf count inside the Merkle root (hash the count into the root, for example `root = sha256("NHROOT" || units_le || tree_root)`) and publish the full leaf list (salted hashes only) to content-addressed storage, so anyone can recount before the freeze.
  - Put `units` behind a second signature (sponsor or auditor) at the final LockRoster before the freeze.
  - Release only the amount supported by receipts: pay the disburser in tranches, each tranche requiring a receipts root with a count and total that the auditor co-signs, and refund the rest on-chain automatically after a deadline.

### TM-02. Operator alone decides the outcome; silent auditor makes a false settle final
- **Category:** fund loss (sponsor side), unauthorized access
- **Scenario:** The operator calls Settle with a fabricated `observed` below `thr_full` in a normal year. The program accepts any i32. If the auditor does not dispute within 172,800 seconds (48 hours), anyone can call Release and the full `units x amount_full` goes to the disburser. Combined with TM-01 and a disburser key held by the same party, this is a complete drain path. Reverse case: settle a real drought as NONE, and the whole vault goes back to the sponsor while farmers get nothing.
- **Aggravating factors:** Settle is allowed at `window_end_ts` with no data lag enforced; the auditor may be the same organisation as the operator (PLEDGE mode); a Friday night settle before a public holiday easily beats a 48-hour window.
- **Mitigation:**
  - Require two signatures for Settle from the start (operator and an independent key), not an optimistic settle.
  - Enforce `now >= window_end_ts + DATA_LAG` on-chain (at least 7 days).
  - At CreateCampaign, reject `operator == auditor`, `auditor == disburser`, `operator == disburser`. This cannot prove the keys belong to different organisations, but it blocks the laziest setup.
  - Bound `observed` (for example 0 to 10,000 mm x 10) and store a data content ID that is published before settle.
  - Make the dispute window long enough to cover Indonesian public holidays (for example 7 days) and make it configurable with a minimum.

### TM-03. Funds stuck forever in DISPUTED
- **Category:** failed withdrawals
- **Scenario:** The auditor calls Dispute. From DISPUTED the only exit is a re-settle signed by both operator and auditor. If either key is lost, or the two parties disagree permanently, or the auditor is malicious and wants to hold the money hostage, nothing moves. The refund path (`OPEN` and `now >= window_end_ts + 30 days`) does not apply to DISPUTED. The vault is locked with no exit.
- **Mitigation:** Add a timeout: if status is DISPUTED and `now >= settled_ts + DISPUTE_TIMEOUT` (for example 60 days), Release refunds everything to the sponsor (or to the rollover reserve). Also consider allowing the sponsor alone to cancel a dispute and trigger a refund.

### TM-04. Release sends funds to a disburser that is not bound safely
- **Category:** fund loss
- **Scenario:** The `disburser` pubkey is fixed at creation, but Release takes a `disburser_token` account from the caller. The spec says owner and mint are checked. If the implementation checks only the token account's mint, or only that the owner "is a signer-less pubkey", a permissionless caller can pass their own token account. Separately, a legitimate disburser key is a single hot key that holds 100% of the released funds until rupiah conversion, with no on-chain limit.
- **Mitigation:**
  - Require `disburser_token` to be the associated token account of `(campaign.disburser, campaign.mint)` and re-derive it in the program, rather than trusting a passed account with an owner field check.
  - Check the token account's owner program (SPL Token), `mint == campaign.mint`, `owner == campaign.disburser`, and that the account is not frozen or delegated in a way that matters.
  - Make the disburser a multisig (Squads) owned by at least two organisations, not a server key.
  - Test explicitly: Release with an attacker token account of the correct mint must fail.

### TM-05. Sponsor refund goes to a token account the attacker controls
- **Category:** fund loss
- **Scenario:** Same class as TM-04 on the other side. Release takes `sponsor_token` and `sponsor` (rent recipient) from the permissionless caller. If `sponsor_token` is validated only by mint, a caller can send the sponsor's remainder, which is the whole vault in NONE outcomes, to their own account. If `sponsor` (rent receiver) is not checked against `campaign.sponsor`, rent is stolen (small), and it is a sign the other checks are weak too.
- **Mitigation:** Re-derive the sponsor ATA from `(campaign.sponsor, campaign.mint)`; require `sponsor.key == campaign.sponsor`. Unit-test each of these with a wrong account.

### TM-06. Duplicate rupiah payouts off-chain
- **Category:** duplicate payouts
- **Scenario:** The idempotency key for disbursement is the leaf. But: (a) the same person registers under multiple phones and relatives' e-wallets, producing different leaves; (b) the same person is in two overlapping campaigns (different leaf, same farmer, same drought); (c) the payout worker retries after a gateway timeout where the first request actually succeeded but the callback was lost, and a retry after "UBAH" changes the account, so a new request is made with a new target while the first one also lands; (d) if the gateway's idempotency window is shorter than the 14-day retry period, the same key can be accepted twice.
- **Mitigation:**
  - Before any retry, query the gateway for the status of the original request; never create a new request while the first is unknown.
  - Use a payout record state machine with a unique constraint on `(campaign, leaf)` and on `(campaign, payout_account_hmac)`, plus a cross-campaign cap per NIK or per verified name hash in the same window.
  - When the payout account changes, cancel the earlier request first and wait for confirmed cancellation.
  - Reconcile gateway statements daily against the receipts tree before posting receipts.

### TM-07. Payout redirection through WhatsApp account takeover ("UBAH")
- **Category:** fund loss (farmer side), unauthorized access
- **Scenario:** After a failed payout, the farmer is told to reply "UBAH" within 14 days to change the account. Anyone who controls the farmer's WhatsApp (SIM swap, borrowed phone, helper who registered them, social engineering of the 6-digit WhatsApp code) replies UBAH and supplies their own e-wallet. Attackers can also force a failure first (for example, if they know the target e-wallet is dormant) to open the window.
- **Mitigation:** After the freeze, only allow account changes where the new account name validates to the same verified name as the original; require NIK re-confirmation; add a 48-hour delay with a notice to the original number; limit to one change per campaign; log and review changes made by assisted helpers.

### TM-08. Account substitution and missing ownership checks in a hand-rolled (Pinocchio) program
- **Category:** unauthorized access, fund loss
- **Scenario:** Without Anchor, every check is manual. Typical misses: not checking `campaign.owner == program_id` (attacker passes a fake account with the same layout that names themselves operator); not re-deriving the Campaign PDA from `["camp", sponsor, campaign_id]` and the vault PDA from `["vault", campaign]`; not checking `tag == 1`; not checking `is_signer` on operator or auditor; not checking `is_writable`. A forged Campaign account owned by another program with `operator = attacker` passes LockRoster and Settle, and if the vault PDA is not re-derived from the real campaign, a forged account could point at a real vault.
- **Mitigation:** One shared `load_campaign()` that checks owner, length 368, tag, re-derives the PDA with the stored bump, and one `load_vault()` that re-derives from the campaign key and checks the token program owner. Every instruction must use them. Add negative tests for each check.

### TM-09. Arithmetic on payout amounts
- **Category:** incorrect deductions
- **Scenario:** `units x amount_full` and `units x amount_half` are u32 x u64. The spec says "check overflow" only in LockRoster. If Release computes `units as u64 * amount` with wrapping multiplication in release builds, a crafted `amount_full` near u64 max plus a large `units` wraps to a small number, passes the balance check, and then pays a different amount than intended. Also, rounding: HALF uses `amount_half`, which is independent of `amount_full`; nothing checks that the vault covers HALF payouts if a later LockRoster changed `units`.
- **Mitigation:** Use `checked_mul` everywhere, including Release. Recheck `vault_balance >= units x paid_amount` in Release. Put a sane upper bound on `amount_full` at creation.

---

## P1 findings

### TM-10. Adverse selection: registration after the season has started
- **Severity:** P1 (systematic overpayment against the sponsor's intent)
- **Scenario:** The Campaign account stores `freeze_ts` and `window_end_ts` but no `window_start_ts`. CreateCampaign only requires `now < freeze_ts < window_end_ts`. A sponsor (or NusaHarvest in PLEDGE mode) can set `freeze_ts` in mid-November. Registrants watch a dry October and pile in; the operator keeps raising `units` with repeated LockRoster calls.
- **Mitigation:** Store `window_start_ts` and require `freeze_ts <= window_start_ts` on-chain. Cap how much `units` can grow in the last N days before the freeze.

### TM-11. Settle using provisional or revised data
- **Severity:** P1
- **Scenario:** The program allows Settle at `window_end_ts`. ERA5 and Open-Meteo values are revised in the days after. A settle based on day-1 data can differ from the "reproducible" script run a week later, and a disputed outcome looks like manipulation even when it is not.
- **Mitigation:** On-chain data lag (TM-02). Publish the exact fetched snapshot to content-addressed storage and hash that, not a live API URL.

### TM-12. Front-running and griefing the refund path
- **Severity:** P1
- **Scenario:** From OPEN, anyone can call Release at `window_end_ts + 30 days` and refund everything to the sponsor. If the operator's Settle is delayed (RPC outage, data REVIEW path taking longer than expected), a watcher or the sponsor refunds the vault one block before Settle, and farmers lose a legitimate payout. The reverse race also exists.
- **Mitigation:** Longer refund deadline (for example 90 days), and allow the auditor to extend it once. Alert the operator at day 20.

### TM-13. PDA pre-funding denial of service on CreateCampaign
- **Severity:** P1 (availability)
- **Scenario:** The Campaign PDA address is predictable from `sponsor` and `campaign_id`. If the program uses `system_program::create_account`, an attacker who sends a few lamports to that address first makes `create_account` fail, blocking the campaign. The same applies to the vault PDA.
- **Mitigation:** Use the transfer plus allocate plus assign pattern when the account already has lamports. Let the sponsor retry with a new `campaign_id` without friction.

### TM-14. Mint and token program risks
- **Severity:** P1
- **Scenario:** `mint` is chosen by the sponsor. A Token-2022 mint with a transfer fee, transfer hook, permanent delegate or default frozen state can break the balance invariant (vault receives less than `deposit`), make Release fail forever (hook reverts, account frozen), or let the mint authority pull the vault. Even USDC has a freeze authority held by Circle.
- **Mitigation:** Allowlist mints in the program (USDC and specific verified rupiah stablecoins) and only the classic SPL Token program. Check vault balance after deposit rather than trusting `deposit`. Document the USDC freeze-authority risk to sponsors.

### TM-15. Vault close fails, blocking Release
- **Severity:** P1 (failed withdrawal)
- **Scenario:** Release pays out, refunds the remainder and closes the vault in one instruction. If someone sends extra tokens to the vault between the calculation and the close (possible in another transaction earlier in the same block) the computed remainder is still correct if the program reads the balance at execution time, but if it computes remainder from stored values (for example `deposit - paid`) the close fails because the balance is not zero. Result: Release reverts every time.
- **Mitigation:** Always compute the remainder from the live vault balance right before transfer. Test with an extra donation to the vault.

### TM-16. PLEDGE mode is indistinguishable from escrow for most viewers
- **Severity:** P1 (misleading proof, reputational and legal)
- **Scenario:** In PLEDGE mode, NusaHarvest signs CreateCampaign for the sponsor, nothing is locked, and Release only changes status. The chain says RELEASED even though no money moved. The proof page label is a UI promise; explorers and third-party dashboards will not show it.
- **Mitigation:** Make PLEDGE a different account tag or a separate instruction set, and name the status differently (for example PLEDGE_CONFIRMED rather than RELEASED). Require a sponsor signature from a key the sponsor holds, even in PLEDGE mode.

### TM-17. Operator hot key and server secrets
- **Severity:** P1
- **Scenario:** The operator key signs LockRoster, Settle and PostReceipts from cron endpoints protected by a "secret header". Leaking that header (logs, CI output, a Vercel env mistake) lets anyone trigger roster locks and settles on demand; leaking the key itself is TM-01 and TM-02 at once. The AES key for all personal data sits in the same environment.
- **Mitigation:** Separate keys per role and per campaign where practical; keep Settle behind a hardware or KMS-backed signer; verify cron calls with signed requests (for example Vercel cron verification or HMAC with timestamp), not a static header; separate the PII encryption key into a KMS.

### TM-18. Sybil registrations frozen into the roster
- **Severity:** P1 (fund loss to fake recipients; not a program bug, but defeats the program's purpose)
- **Scenario:** No identity check, cheap SIMs, draggable location pins and a 20 m duplicate radius. A local fixer registers dozens of fake farmers. The Merkle root then "proves" that these fake farmers were registered before the freeze.
- **Mitigation:** See idea-roast section 3: anchor to NIK or RDKK, one payout per NIK, cap registrations per helper, flag clusters of registrations from the same device or IP.

---

## P2 findings

### TM-19. Proof API enables enumeration of registered phones
- **Scenario:** `/api/proof/roster` accepts `phone_hash + code` and returns a leaf and proof. If the API also accepts a raw phone with no code, or returns different errors for "phone exists, wrong code" and "phone does not exist", an attacker can test phone numbers and learn who is enrolled (personal data leak under UU PDP, and a target list for TM-07). Rate limiting by IP is easy to bypass.
- **Mitigation:** Require both phone and proof code, return one generic error, rate-limit by phone and by code as well as IP, log failures.

### TM-20. Merkle construction ambiguity
- **Scenario:** Sorted pairs without position and no domain separation between leaves and internal nodes allow a 64-byte internal node to be presented as a leaf (second preimage). Leaves are domain-tagged with "NH1", which helps, but nodes have no tag and an odd leaf count handling is not specified.
- **Mitigation:** Tag internal nodes (for example `sha256(0x01 || min || max)`) and leaves (`0x00` prefix), and specify odd-node handling (promote, do not duplicate). Publish test vectors.

### TM-21. Receipts root is unverifiable against money
- **Scenario:** PostReceipts accepts any root from the operator, once. Nothing ties the receipts count or total to `units x amount`. Failed and undisbursed amounts are returned "off-chain". Auditors cannot tell from chain whether 950 of 1,000 farmers were paid or 50.
- **Mitigation:** Store `receipts_count` and `receipts_total` alongside the root, require auditor co-signature, and emit the undisbursed amount. Better, return undisbursed tokens on-chain before RECEIPTED.

### TM-22. Re-settle has no bound
- **Scenario:** After Dispute, the re-settle can set any `observed` and any `data_hash` with both signatures. There is no record of the original values, so the dispute history is lost on-chain.
- **Mitigation:** Store or emit the original `observed` and `data_hash` before overwriting; keep a dispute counter.

### TM-23. `terms_hash` can be anything
- **Scenario:** In PLEDGE mode NusaHarvest computes `terms_hash` for the sponsor. Nothing forces the document to be published, so a hash of an unpublished document proves nothing to farmers or the public.
- **Mitigation:** Publish terms to content-addressed storage and store the content ID, not just a sha256.

### TM-24. Rule version is informational only
- **Scenario:** `rule_version` is written as 1, but the program logic does not branch on it. A program upgrade that changes Settle thresholds changes behaviour for campaigns already running under version 1.
- **Mitigation:** Make the program immutable per version or reject instructions on campaigns whose `rule_version` the current code does not support. Put upgrade authority in a multisig with a public timelock.

### TM-25. Upgrade authority
- **Scenario:** The deploy plan uses the upgradeable loader with a single deployer. Whoever holds upgrade authority can replace the program and move every vault.
- **Mitigation:** Transfer upgrade authority to a multisig before the first real deposit, or make the program immutable after audit. Publish a verifiable build hash.

---

## P3 findings

### TM-26. Minimal binary size is being optimised over safety
- **Scenario:** Removing logs and keeping the binary under 13,500 bytes to fit a 0.1 SOL deploy budget pushes toward skipping checks. The savings (a few hundredths of a SOL) are negligible compared with the funds at risk.
- **Mitigation:** Treat size as a soft target. Never remove a validation to save bytes.

### TM-27. Error codes without context
- **Scenario:** No logs in release builds make on-chain incidents hard to diagnose for sponsors and auditors.
- **Mitigation:** Keep distinct error codes per check (not a shared `BadParams`), and document each.

### TM-28. Clock dependence
- **Scenario:** All windows use `Clock::unix_timestamp`, which can drift from wall time by a small amount. With a 48-hour window this is not material, but tests should not assume exact seconds.
- **Mitigation:** Add small margins in off-chain schedulers; do not schedule actions at the exact boundary.

### TM-29. Rent and account lifecycle
- **Scenario:** Campaign accounts are kept forever as an archive, paid by the sponsor. Fine, but a closed vault's address can be re-created by someone if the program allows CreateCampaign logic to reuse the same seeds.
- **Mitigation:** Campaign seeds include `campaign_id`; reject CreateCampaign if the Campaign account already exists with data, regardless of the vault state.

---

## Summary table

| ID | Title | Severity |
|---|---|---|
| TM-01 | Operator inflates `units`, vault drained to disburser | P0 |
| TM-02 | Operator alone decides outcome; silent auditor finalises false settle | P0 |
| TM-03 | Funds stuck forever in DISPUTED | P0 |
| TM-04 | Disburser token account not safely bound; single hot disburser key | P0 |
| TM-05 | Sponsor refund to attacker token account | P0 |
| TM-06 | Duplicate rupiah payouts off-chain | P0 |
| TM-07 | Payout redirection via WhatsApp takeover (UBAH) | P0 |
| TM-08 | Missing owner/PDA/signer checks in hand-rolled program | P0 |
| TM-09 | Unchecked multiplication on payout amounts | P0 |
| TM-10 | Registration after the season starts | P1 |
| TM-11 | Settle on provisional data | P1 |
| TM-12 | Refund path races legitimate settle | P1 |
| TM-13 | PDA pre-funding blocks CreateCampaign | P1 |
| TM-14 | Hostile or freezable mint | P1 |
| TM-15 | Vault close failure blocks Release | P1 |
| TM-16 | PLEDGE shows RELEASED with no money moved | P1 |
| TM-17 | Operator hot key, static cron secret, co-located PII key | P1 |
| TM-18 | Sybil registrations frozen into roster | P1 |
| TM-19 | Proof API phone enumeration | P2 |
| TM-20 | Merkle node/leaf ambiguity | P2 |
| TM-21 | Receipts root not tied to money | P2 |
| TM-22 | Re-settle erases dispute history | P2 |
| TM-23 | Unpublished `terms_hash` | P2 |
| TM-24 | `rule_version` not enforced | P2 |
| TM-25 | Single-key upgrade authority | P2 |
| TM-26 | Binary size pressure removes checks | P3 |
| TM-27 | Error codes without context | P3 |
| TM-28 | Clock boundary assumptions | P3 |
| TM-29 | Account re-creation edge cases | P3 |

## Minimum changes before any real money is deposited

1. Two-signature Settle, on-chain data lag, bounded `observed` (TM-02, TM-11).
2. Timeout exit from DISPUTED (TM-03).
3. Re-derived ATAs for disburser and sponsor; disburser as a multisig (TM-04, TM-05).
4. `units` committed into the root and co-signed at the final lock; on-chain return of undisbursed funds (TM-01, TM-21).
5. `window_start_ts` stored and `freeze_ts <= window_start_ts` enforced (TM-10).
6. Mint allowlist, classic SPL Token only, live-balance remainder (TM-14, TM-15).
7. Upgrade authority in a multisig (TM-25).
8. Off-chain: identity anchoring, locked payout accounts after freeze, gateway status check before any retry (TM-06, TM-07, TM-18).
