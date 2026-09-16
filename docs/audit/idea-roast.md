# Idea Roast: NusaHarvest Siaga Tanam

Date: 16 September 2026
Scope: `research/01-RISET-V2.md`, `research/02-FLOW-DAN-SKEMA-FINAL.md`, `research/data/backtest-calibrated.json`, `web/index.html`.
Stance: blunt on purpose. Where something is good it is stated once and then left alone.

---

## 0. Verdict in one paragraph

The V2 research is far more honest than V1. Moving from "farmers buy on-chain insurance" to "a legal entity commits aid money in advance and pays it out on a rainfall rule" is the right pivot. But the idea as written still has four problems, and any one of them could kill it. (1) The rainfall signal is coarser than the docs say, so basis risk is higher than advertised. (2) The natural sponsors (CSR, zakat, local government) have budget rules that clash with money being "locked, then maybe returned". (3) With no cooperative and no identity check, onboarding invites fake farmers, and the Merkle root just freezes whatever fake list gets in. (4) The on-chain part protects much less than the landing page claims: one operator key sets both the rainfall number and the roster size, and one disburser key receives all the money. The landing page also already breaks the project's own "no simulated data" rule. None of this is fatal if it is fixed before a pilot. All of it is fatal if a judge or a sponsor's lawyer finds it first.

---

## 1. Basis risk: ERA5 grid vs the actual field

**What the docs claim.** The trigger is "computed per 0.1 degree grid cell (about 11 km)" (01-RISET section 2.4), and the web copy says there is "no human judgment" (step 3).

**What is actually true.**
- Open-Meteo serves ERA5 at 0.25 degrees and ERA5-Land at 0.1 degrees. But ERA5-Land precipitation is interpolated from ERA5, not modelled at 0.1 degrees, so the real resolution of the rain data is about 25 to 31 km. The "0.1 degree cell" is a resampled grid, not finer rain. Source: https://open-meteo.com/en/docs/historical-weather-api
- The backtest itself shows this. In `backtest-calibrated.json`, Grobogan and Demak have **identical** trigger years (2002, 2004, 2006, 2009, 2019, 2023). Two districts roughly 40 km apart moving in lockstep is what a coarse, smooth rain field looks like. Klaten shares four of its five years with them.
- Reanalysis is a model, not a rain gauge. For convective tropical rain in Java, reanalysis tends to spread rain over more days in smaller amounts and to miss local extremes. A dry pocket the size of a village will often not show up at all.
- Kupang is a single point near the coast. Dryland maize in Timor is grown inland and at different altitudes. One point labelled "Kupang" does not represent NTT farmers.
- Only the p20 tier was backtested. The on-chain schema has two tiers (full payout at p10, half at p20), but the data has **no p10 backtest**. How often the full tier would pay, and what it would cost, is unknown.
- 25 years gives only 4 to 6 events per site. A p20 threshold estimated from 30 baseline years has a wide confidence band. Moving the threshold by 20 to 30 mm flips 2004 in Demak (574 mm against a 581 mm floor) and 2019 in Grobogan (539 mm against 559 mm). Several "hits" sit right on the edge.
- Known false negative: the very strong 2015 El Nino did not trigger at the three Java sites. The research admits this. It is the season a Grobogan farmer is most likely to remember as bad.
- Nobody has checked that low October to December rain actually means lost harvests. The BPS district production cross-check is marked "not done". Until it is done, the trigger is proven to detect climate anomalies, not farm losses.
- October 1 to December 31 is a fixed calendar window, but planting onset in rain-fed Java moves by weeks from year to year. A late but normal onset can produce a below-p20 total with a fine harvest (payout without loss). A dry January after a wet December produces loss without payout.
- The landing page says money arrives "a few days after the season closes". The flow spec has a 7-day data lag, then a 48-hour dispute window, then batch disbursement, then up to 14 days for failed payments. The window closes on 31 December, weeks after the replanting decisions. The cash arrives after the damage, not "before the harvest fails".

**Why it matters.** WorldCover in Ghana is the documented case. A community that felt drought got nothing because satellite data showed enough rain, and farmer trust collapsed (https://bfaglobal.com/catalyst-fund/case-study-worldcover/). The research calls WorldCover "dead". I could not confirm a shutdown from public sources, so do not claim that in pitches.

**Fixes.**
- Stop saying "0.1 degree". Say "roughly 25 km reanalysis cell". Plan a higher-resolution source that blends station data (CHIRPS, 0.05 degrees) as the primary for Java, with ERA5 as the cross-check.
- Backtest the p10 tier and publish it. Publish the confidence interval for each threshold.
- Validate against BPS district rice and maize production and against BMKG station records where they exist. If the correlation is weak, say so and redesign.
- Use a window anchored to rainfall onset (for example, 45 days after the first 50 mm in 10 days) instead of a fixed calendar quarter.
- Add an early mid-window trigger (cumulative rain by 30 November below a low percentile) so some money arrives while replanting is still possible.

---

## 2. Who funds the pool, and why would they?

The research names CSR, LAZ/BAZNAS, NGOs, local governments (pemda) and offtakers. Each has a reason to say no.

| Sponsor type | Stated appeal | The real obstacle |
|---|---|---|
| Corporate CSR | Auditable report for TJSL obligations | CSR budgets are usually annual and meant to be spent. "Locked, then refunded in about 80% of years" means the CSR team must re-budget and re-approve. CSR managers want a story with photos, not a conditional transfer that usually does not happen. |
| LAZ / BAZNAS (zakat) | Rp44.7 trillion ZIS in 2025 | Zakat is meant to reach eligible recipients (asnaf) promptly. Holding zakat on a condition and returning it if it rains is hard to reconcile with sharia guidance and LAZ reporting. Infaq and sadaqah are more plausible than zakat, and still need a sharia board opinion. |
| Local government (pemda) | Accountability for disaster funds | Spending from the regional budget (APBD) needs a legal basis such as a regent's regulation, procurement rules and a registered recipient list. A pemda treasury holding USDC in a program vault is close to impossible. Only PLEDGE mode is realistic, and PLEDGE removes the main on-chain guarantee. |
| NGOs | Anticipatory cash is already their practice | They already do it with spreadsheets and bank transfers (GiveDirectly, WFP). They need a reason to add crypto, legal review and a new vendor. |
| Offtakers (rice mills, feed makers) | Supply security | The most commercially rational sponsor, but they will want it tied to supply contracts, which brings an intermediary back. |

The honest summary: the sponsors most likely to pay can only use PLEDGE mode, and in PLEDGE mode the chain proves the recipient list and the rainfall result but not that the money exists. The most distinctive feature, escrow, only suits crypto-native sponsors, and there are very few of those in Indonesian agriculture.

The cheapest fix is to change the refund rule. Unused money should roll over to the next season or into a standing district reserve instead of going back to the sponsor. That turns an insurance-like "maybe" into a multi-year disaster reserve, which CSR and pemda teams can actually approve and report on.

---

## 3. Farmer onboarding without a cooperative

**KYC.** The MVP has no identity check. It only checks that the e-wallet account name matches. That proves someone owns an e-wallet. It does not prove they farm, own or work land, or live in the district.

**Phone.** Indonesian prepaid SIMs are cheap. Duplicate checks are per campaign, on phone hash, account number and a 20 m location radius. Someone with five SIMs, five relatives' e-wallets and five pins 30 m apart passes every check.

**Location.** WhatsApp "share location" lets the user drag a pin anywhere. It is not a GPS attestation. A farmer on irrigated land, or someone in town, can register inside a rain-fed cell.

**Wallet custody.** The farmer never holds crypto, which is correct. But custody of the money moves to (a) the disburser key and (b) the farmer's e-wallet and WhatsApp account. A WhatsApp takeover or SIM swap lets an attacker reply "UBAH" and redirect the payment during the 14-day retry window.

**Elderly and low-literacy farmers.** The research proposes assisted registration through extension workers (penyuluh) or a farmer's child. A helper who controls the phone is the same intermediary the project says it removed. Without controls, this is where leakage comes back.

**Adverse selection.** Registration stays open until `freeze_ts`, and the on-chain account has no `window_start_ts`. If a sponsor sets the freeze after the window starts, people can watch a dry October and then register.

**Fixes.**
- Tie registration to an existing government list: the RDKK fertiliser subsidy group list, the Kartu Tani or SIMLUHTAN registry, or NIK checked through Dukcapil. The research dismisses e-KYC at around Rp4,000 per check; against a Rp300,000 payout that is cheap fraud protection.
- One payout per NIK across all active campaigns, not one per phone per campaign.
- Enforce on-chain that `freeze_ts <= window_start_ts`, with `window_start_ts` stored in the account.
- Block payout account changes after the freeze unless the new account name matches the verified name, with a cool-down and a notice sent to the old number.
- Allow penyuluh assistance, but log the helper and cap how many registrations one helper can make.

---

## 4. Regulatory: is this insurance?

**The research's argument.** The farmer pays no premium and there is no promise of indemnity, so it is conditional aid, not insurance under UU 40/2014. The Kitabisa Saling Jaga precedent is correctly identified.

**Where that argument is weaker than it looks.**
- The web copy compares the product directly with crop insurance: "Crop insurance does not reach small farmers", a table contrasting "Premium each season" with "None", and "The weather record is the claim". The English page uses "claim" while the spec bans the word in Indonesian. A regulator reads both substance and marketing, and right now the marketing says "this replaces insurance".
- If an offtaker sponsors the campaign and registration is bundled with a supply contract, the farmer's harvest delivery can be argued to be consideration. That starts to look like a premium.
- The landing copy says sponsors lock USDC. Since 10 January 2025, crypto assets are supervised by OJK under POJK 27/2024 (https://news.ddtc.co.id/berita/nasional/1807858/pengawasan-aset-kripto-resmi-beralih-ke-ojk-januari-2025). Converting USDC to rupiah for third parties needs a licensed crypto trader, and paying rupiah on behalf of sponsors is a funds transfer activity under Bank Indonesia rules. NusaHarvest cannot convert and pay without a licence. It needs to be a registered PT that uses licensed partners and ideally never touches the money.
- If NusaHarvest ever accepts public contributions, it needs a PUB fundraising permit (Permensos 8/2021 as amended) or must route through a licensed LAZ.
- UU PDP 27/2022: exact farm coordinates, phone number and e-wallet name together are personal data. Encrypting them with one key held in an environment variable is basic hygiene, not compliance. The project needs someone accountable for data protection, recorded consent from the WhatsApp flow, and a breach response plan.

**Fixes.**
- Get a written legal opinion before any real farmer is registered. Budget for it; it is not optional.
- Rewrite the web copy as "pre-committed disaster aid", not as an insurance alternative. Remove the insurance comparison table.
- Put licensed parties explicitly into the money flow: the sponsor pays rupiah to a licensed disbursement partner, and the chain holds commitments and proofs. Treat USDC escrow as an optional mode for crypto-native sponsors only.
- Longer term, partner with a licensed insurer (Askrindo, Jasindo or a microinsurer) for any product where farmers or offtakers pay anything.

---

## 5. Oracle manipulation

The design calls settlement "reproducible". It is reproducible **after the fact**, which is not the same as trustless.

- One operator key submits `observed`. The program accepts any i32 and checks it against nothing.
- The only check is the auditor, who has 48 hours to dispute. The sponsor appoints the auditor. In PLEDGE mode NusaHarvest signs `CreateCampaign` for the sponsor, so NusaHarvest effectively chooses both operator and auditor. If both keys belong to the same organisation there is no check at all.
- If the auditor does nothing within 48 hours (holiday, lost key, busy), any value becomes final. A Settle can be timed for a Friday night before a long weekend.
- `Settle` is allowed from `window_end_ts`, but the pipeline waits 7 more days because data gets revised. The program does not enforce that lag, so a rushed or malicious settle can use provisional data.
- `data_hash` proves that some JSON existed. It does not prove the JSON matches Open-Meteo. Open-Meteo revises data and does not sign responses, so two honest people fetching on different days can get different numbers. "Anyone can reproduce it" needs a pinned snapshot on neutral storage (Arweave, or IPFS with a public pin), not a NusaHarvest URL.
- The rule "if the two sources differ by more than 25%, a human reviews it" contradicts the web claim of "no human judgment". It is a sensible rule, but it is a human decision point and must be disclosed.

**Fixes.** Enforce the data lag on-chain (`now >= window_end_ts + lag`). Require two independent signers for Settle (the operator plus a key held by the sponsor or a third party) instead of an optimistic settle with a dispute window. Bound `observed` to a plausible range. Publish the canonical data to content-addressed storage before settling and include the content ID in the preimage of `data_hash`. Longer term, publish daily cell values through an oracle network so the operator is not the only source.

---

## 6. Unit economics

Using the research's own numbers:

- Payout Rp300,000 per farmer per event. Trigger frequency about 21% (21 hits in 100 site-years). Expected aid cost: about Rp63,000 per farmer per year.
- Disbursement fee about Rp2,500 per transfer (Xendit, as cited in the research).
- WhatsApp Business template messages cost a few hundred rupiah each in Indonesia (not verified in this audit; check Meta's current rate card). At 6 to 8 messages per farmer per season this is small but not zero.
- Identity check through Dukcapil about Rp4,000 per farmer, if adopted.
- Platform fee: 3 to 5% of locked funds.

**Worked example: one district campaign, 1,000 farmers, ESCROW mode.**
- The sponsor locks 1,000 x Rp300,000 = Rp300 million.
- Platform revenue at 4%: Rp12 million per season (about US$730).
- Expected payout: Rp63 million. In about 79% of seasons the sponsor ties up Rp300 million for three months and gets it all back.
- Running costs include WhatsApp, cloud, RPC, reviewer time, a share of legal costs and support. Rp12 million does not cover one staff month plus legal.

**Correlation.** Trigger years cluster in El Nino years across all four sites: 2006, 2009, 2019 and 2023 hit three or four sites at once. If NusaHarvest ever holds a multi-district reserve instead of isolated campaigns, it pays out everywhere in the same year. That is catastrophe risk, which is why reinsurance exists.

**Size of the benefit.** Rp300,000 is a very small fraction of what it costs to grow a hectare of rice in Java (production cost is commonly cited in the range of Rp15 to 20 million per hectare per season; not re-verified here). It might buy replanting seed for a small plot. It will not replace a lost harvest. Call it "seed money to replant", not protection.

**Conclusion.** A percentage of locked capital is a weak revenue base, because most of that capital returns. Better: a flat setup fee per campaign plus a fee per registered farmer, paid by the sponsor. The business only works at tens of thousands of farmers per season, which requires a government or large offtaker channel.

---

## 7. SWOT

| Strengths | Weaknesses |
|---|---|
| Farmers pay nothing and never touch crypto. | Coarse rainfall data; trigger not validated against yields. |
| Honest research, rerunnable backtest scripts, clear analysis of why V1 failed. | One operator key controls both the rainfall value and the roster size. |
| Small, auditable program design (one account per campaign). | No identity link; fake registrations are cheap. |
| Roster lock addresses a real Indonesian aid-leakage problem. | Escrow suits the fewest sponsors; PLEDGE loses the main guarantee. |
| No personal data on-chain by design. | No legal entity, licence path or legal opinion yet. |
| | Landing page shows generated rainfall bars and overclaims. |

| Opportunities | Threats |
|---|---|
| The government disaster Pooling Fund (Perpres 75/2021) needs auditable distribution. | OJK or Kemensos reads it as unlicensed insurance, fundraising or funds transfer. |
| Anticipatory action is a growing donor funding category. | Licensed insurers (Askrindo, Jasindo, Tugu) run parametric pilots with existing distribution. |
| Insurers could license the proof layer (B2B) instead of competing with it. | One WorldCover-style bad season destroys trust in a district. |
| On-chain reinsurance capital on Solana (for example OnRe) could later take tail risk. | El Nino clustering can drain any pooled reserve in one year. |
| Offtakers want supply stability. | Middlemen (tengkulak) stay faster and closer to farmers. |

---

## 8. Business Model Canvas

| Block | Current answer | Critique |
|---|---|---|
| Customer segments | Sponsors (CSR, LAZ, NGO, pemda, offtakers); beneficiaries are rain-fed rice and dryland maize farmers | Too many sponsor types. Pick one for the pilot, ideally an NGO already doing anticipatory cash, or a rice or feed offtaker. |
| Value proposition | Sponsor: money committed before disaster, no ghost recipients, reproducible trigger. Farmer: cash without a claim. | The sponsor gets proof and reporting. The farmer gets a small, late payment. Fix the timing. |
| Channels | WhatsApp bot, QR codes, penyuluh | Penyuluh are government staff. Using them needs the agriculture office's agreement. That is a partnership, not a channel you control. |
| Customer relationships | Automated messages, public proof page | Farmers will phone someone when money does not arrive. There is no support plan. |
| Revenue streams | 3 to 5% of locked funds | Weak; see section 6. |
| Key resources | Trigger engine, backtest data, Solana program, WhatsApp integration | No licence, no described legal entity, no data partnership with BMKG. |
| Key activities | Campaign setup, registration, settlement, disbursement | Fraud review and dispute handling are missing, and they are the expensive part. |
| Key partners | Payment gateway, e-wallets, data sources, sponsors | Missing: legal counsel, licensed crypto trader, BMKG or university climate partner, a licensed insurer. |
| Cost structure | RPC, rent, disbursement fees, WhatsApp | Missing: legal, compliance, audits, identity checks, support staff. These are the biggest costs. |

---

## 9. Competitor comparison

| Player | What it actually is | Scale and status (sourced) | What it means for NusaHarvest |
|---|---|---|---|
| Etherisc + ACRE Africa (Kenya) | Parametric crop insurance built on Etherisc's framework with Chainlink oracles, paid through M-Pesa, with ACRE as the licensed agricultural microinsurance channel | Chainlink grant aimed at up to 250,000 farmers over three years; later reported more than 17,000 farmers covered with payouts made. https://blog.chain.link/chainlink-awards-grant-to-support-the-joint-venture-between-acre-africa-and-etherisc/ and https://cointelegraph.com/news/etherisc-onboards-17k-kenyan-farmers-covered-by-blockchain-based-crop-insurance | The closest proven design. It worked because ACRE brought a licensed channel and years of distribution. NusaHarvest has neither. Reported reach stayed far below the target, which suggests the blockchain did not solve distribution. |
| Arbol (US, global) | B2B parametric weather risk for agriculture, energy and hospitality, using its dClimate data network | US$60 million Series B. https://www.prnewswire.com/news-releases/arbol-raises-60-million-in-series-b-funding-to-scale-parametric-insurance-responding-to-increasing-climate-risk-302131746.html and https://www.arbol.io/post/an-introduction-to-dclimate-a-decentralized-network-for-climate-data-2 | The money is in B2B risk transfer, not retail smallholders. NusaHarvest's proof layer resembles Arbol's data layer more than its insurance business. |
| Jasindo AUTP (Indonesia) | Government-subsidised indemnity rice insurance. Premium Rp180,000 per ha per season, 80% subsidised (farmer pays Rp36,000), maximum cover Rp6 million per ha, requires farmer group membership and NIK. Covers pests, flood and drought. | 2024: Rp50.16 billion premium, 464,764 farmers, 278,694 ha. https://keuangan.kontan.co.id/news/asuransi-usaha-tani-padi-autp-solusi-perlindungan-petani-dari-risiko-gagal-panen and https://keuangan.kontan.co.id/news/jasindo-memproteksi-305000-hektare-lahan-pertanian-lewat-asuransi-usaha-tani-padi | The real substitute. Up to Rp6 million per ha for three perils, against NusaHarvest's Rp300,000 for drought only. AUTP's weakness is slow claims and trust, not price. Position NusaHarvest as a fast top-up alongside AUTP, not a replacement. AUTP's farmer group and NIK requirement is the identity backbone NusaHarvest lacks. |
| Askrindo / Tugu parametric pilots (IFG group) | Licensed state insurers testing index products | Cited in the research (Sindonews); not re-verified in this audit | Licensed incumbents are already moving. Sell them proof and disbursement tooling. |
| GiveDirectly + Google Flood Hub | Anticipatory cash before floods, no blockchain | Cited in the research | Demand for the concept exists without a chain. NusaHarvest has to show what the chain adds to a sponsor's report. |
| WorldCover (Ghana) | Satellite-index insurance sold directly to farmers | Basis-risk conflict documented. https://bfaglobal.com/catalyst-fund/case-study-worldcover/ | The main cautionary tale. Shutdown not verified. |
| Tengkulak and ijon | Informal credit against the future harvest | Dominant | Wins on speed and presence. A payment in January loses to a loan in November. |

---

## 10. Landing page problems (fix before anyone sees it)

1. **Generated data shown as real.** In `web/index.html`, the `seasons()` function fills every non-trigger year with a pseudo-random value ("seeded per site so bars are stable"). Only the gold trigger bars are real. The section is labelled as real backtest data over 25 years. This is exactly the kind of simulated data the spec bans, and exactly what destroyed V1's credibility. Load the real yearly totals from the CSVs.
2. "1 tx on Solana settles a district. Anyone can read who was paid, how much, and why." The design stores no personal data on-chain and pays out off-chain in rupiah. Nobody can read who was paid. Rewrite it.
3. "No human judgment" contradicts the review path, the operator-submitted settle and the auditor dispute.
4. "Cash arrives before the harvest fails" and "a few days after the season closes" are not supported by the documented timeline.
5. "Sponsors lock USDC" ignores PLEDGE mode, which most Indonesian sponsors would use.
6. The insurance comparison table invites the regulatory reading the project is trying to avoid.

---

## 11. Concrete changes that make the idea stronger

1. **Fix the landing page now.** Real yearly data only. Remove "who was paid", "no human judgment" and the insurance comparison.
2. **Validate the trigger against outcomes.** Use BPS district production, add CHIRPS as a second source, backtest p10 and publish threshold confidence intervals. If the link to yield loss is weak, say so and change the index (soil moisture, vegetation index, or area-yield).
3. **Onset-based window plus an early mid-season payment**, so money arrives while replanting is still possible.
4. **Anchor identity to existing registries** (RDKK or NIK through Dukcapil), allow one payout per NIK across campaigns, and lock payout-account changes after the freeze.
5. **Take the single operator out of the money path.** Two-key settle, on-chain data lag, bounded values, content-addressed data snapshots, enforced `freeze_ts <= window_start_ts`, and a timeout exit from DISPUTED. See `threat-model.md`.
6. **Roll over instead of refunding.** Unused funds go into a multi-season district reserve. This fits CSR and pemda budgeting and makes the story "a standing disaster reserve with public rules".
7. **One sponsor type, one district for the pilot.** For example, an NGO with an anticipatory-action budget or a rice or feed offtaker in Grobogan. Get a signed letter before building further.
8. **Legal first.** Set up a PT, get a written opinion on insurance, fundraising, crypto and funds transfer, and never let NusaHarvest itself convert or hold money.
9. **Change the revenue model** to a setup fee plus a per-registered-farmer fee, not a percentage of locked capital.
10. **Position alongside AUTP** ("fast replanting top-up for AUTP-enrolled farmers"), and pitch the proof and disbursement layer to Jasindo, Askrindo and regional disaster agencies (BPBD). B2B infrastructure is more defensible than winning sponsors one campaign at a time.
