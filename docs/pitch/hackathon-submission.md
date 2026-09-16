# NusaHarvest Siaga Tanam: hackathon submission

## One line

A sponsor-funded conditional grant (hibah bersyarat) for Indonesian smallholder farmers: sponsors lock their own funds on Solana before the season, and if season rainfall falls below the district floor, the grant is paid to farmers in rupiah.

## Links

- Live landing page: https://nusaharvest-siaga.vercel.app
- Code: https://github.com/bryankwandou/nusaharvest-siaga
- Pitch deck: `docs/pitch/deck.html`
- Demo video: **[PLACEHOLDER: VIDEO URL]**
- Solana program ID (devnet): **[PLACEHOLDER: PROGRAM ID]**

## Problem

Rice farmers in Central Java and East Nusa Tenggara plant when the October to December rains arrive. When the rain falls short, seed and labour are already spent, and the quickest cash is usually a trader's advance paid back with a discounted harvest. Existing support is slow: the government's subsidised AUTP crop scheme enrolled about 305 thousand hectares in 2024 against a 1 million hectare target ([Jawa Pos](https://www.jawapos.com/bisnis/015807495/asuransi-pertanian-potensial-raup-rp-18-triliun-tapi-masih-terkendala-edukasi-petani)), and its money follows a field loss assessment, after the planting window. Siaga Tanam is meant to sit alongside AUTP as a fast top-up, not replace it.

Aid that passes through intermediaries has its own failure points. Our first version routed money through cooperatives; no real funds ever moved, and its trigger would have fired in 23 of 25 years. We rebuilt the design so no intermediary ever holds the money.

## What it does

1. **Lock.** A sponsor (CSR team, zakat institution, local government) locks its own funds in a campaign escrow on Solana before the season. District, window (Oct 1 to Dec 31) and threshold are fixed at creation.
2. **Seal.** Farmers register by phone. Before the freeze date the roster is hashed into a merkle root and written on-chain. Nobody can be added after a drought becomes visible. A farmer can verify inclusion in the browser against the on-chain root.
3. **Trigger.** After the window closes, the operator posts the observed ERA5 rainfall total and a hash of the source data.
4. **Dispute, then disburse.** A sponsor-appointed auditor has 48 hours to rerun the public script and dispute. If there is no dispute, anyone can call release, and a licensed payment provider pays each farmer an equal grant in rupiah to their e-wallet. If rainfall was above the threshold, funds roll over or return to the sponsor.

Farmers pay nothing, ever, file nothing, and never touch crypto. NusaHarvest never holds funds: Solana is the public ledger and the sponsor's escrow, and the payment provider handles rupiah.

## Evidence: 25-year backtest

Rainfall from the Open-Meteo Historical Weather API (ERA5 / ERA5-Land). Threshold is each district's 20th percentile of Oct-Dec rainfall over 1991-2020. Data: `research/data/backtest-calibrated.json`.

| District | p20 (mm) | Median (mm) | Trigger years 2001-2025 |
|---|---|---|---|
| Grobogan | 559 | 645 | 2002, 2004, 2006, 2009, 2019, 2023 (6) |
| Demak | 581 | 716 | 2002, 2004, 2006, 2009, 2019, 2023 (6) |
| Klaten | 551 | 796 | 2006, 2009, 2018, 2019, 2023 (5) |
| Kupang | 196 | 312 | 2004, 2015, 2019, 2023 (4) |

Example: Grobogan received 402 mm in Oct-Dec 2023, below its 559 mm threshold.

Known limits: the strong 2015 El Nino did not trigger at the three Java sites, and trigger years have not yet been checked against BPS district rice production.

## Tech summary

- **Solana program (Anchor, in progress):** campaign account with vault, `roster_root`, window timestamps, threshold, observed value, data hash, operator and auditor keys. Instructions: create/fund, LockRoster, Settle, Dispute (only within 172,800 seconds of settle), Release (permissionless after the window), PostReceipts, Refund. States: OPEN, SETTLED, DISPUTED, SETTLED_FINAL, RELEASED, RECEIPTED, REFUNDED.
- **No farmer transactions:** farmers never sign transactions, so the program does not verify merkle proofs; verification happens client-side against the on-chain root.
- **Data pipeline:** Node scripts pull ERA5 daily precipitation, compute window totals and percentiles, and hash the raw response. Anyone can rerun them.
- **Roster service:** registrations hashed (sha256) into a merkle tree; all nodes stored so proofs can be served. Receipts from payouts are rolled into a second merkle root.
- **Frontend:** static landing page with a rainfall explorer for the four backtest districts, deployed on Vercel.
- **Cost:** base fee per roster, settle or receipt transaction is 0.000005 SOL.

## Why Solana

The chain is used only where a spreadsheet can't be trusted: funds locked before the season, a roster that can't grow afterwards, and a settle result anyone can replay during the dispute window. Low fees keep the on-chain cost far below a single Rp2,500 payout fee ([Xendit](https://help.xendit.co/hc/en-us/articles/360027727432-Do-I-get-charged-for-a-bank-transfer-fee-for-Disbursement)).

## Business model

Sponsors pay a platform fee on locked campaign funds (planned at 3-5%, not yet validated with a sponsor). No interest, no token, and farmers pay nothing. The product is a conditional grant from a sponsor's own funds, with a public trigger.

## Status

- Done: 25-year, four-district backtest
- Live: landing page
- In progress: Solana program
- Next: devnet campaign with sealed roster and replayable settle; BPS production validation

There are no sponsors, users or funds yet.

## Status and compliance

- The MVP runs on Solana devnet with test tokens. No real money moves.
- A real-money pilot will run as a written conditional-grant agreement with one sponsor, disbursed through a licensed payment partner, and only after a legal opinion. We do not assume that no licence or approval is needed.

## Team

**[PLACEHOLDER: team names and roles]**
