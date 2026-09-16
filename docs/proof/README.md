# What is actually running, and how to check it yourself

Nothing here asks you to trust a screenshot. Every claim below has a command or a URL you can run.

## 1. The program is on devnet

```
solana program show GeCxWiK9HXEuEsueNQawDKZnwE1SkojPw9dtYLBkcGBX --url devnet
```

```
Program Id: GeCxWiK9HXEuEsueNQawDKZnwE1SkojPw9dtYLBkcGBX
Owner: BPFLoaderUpgradeab1e11111111111111111111111
ProgramData Address: CwxcNYoX9F1yogCCUe3bfbrTVCNWhyr4wpvA2wX6Cod1
Authority: C3otspAauyPNbAx9NA4wkH7P8hxhxhb1dyfqzhSmzaj9
```

Explorer: https://explorer.solana.com/address/GeCxWiK9HXEuEsueNQawDKZnwE1SkojPw9dtYLBkcGBX?cluster=devnet

## 2. It holds real campaign accounts

Open https://nusaharvest-siaga.vercel.app/app.html with no wallet. The page calls `getProgramAccounts`
against `api.devnet.solana.com` and decodes each 368-byte account using the offsets in
`program/src/state.rs`. On 16 September 2026 it returned three campaigns, all `RELEASED`
with outcome `FULL`:

| Campaign | Units | Observed | Full floor | Half floor | Vault |
|---|---|---|---|---|---|
| `3BF7hWtByRPUiPVqk5NkhQTo7Tx6Af1CA6jzcem1nxYh` | 2 | 55.0 mm | 80.0 mm | 120.0 mm | 0 (emptied by claims) |
| `6cuktc…EEJFiT` | 2 | 55.0 mm | 80.0 mm | 120.0 mm | 0 |
| `7qXMBU…XaUsz1` | 2 | 55.0 mm | 80.0 mm | 120.0 mm | 0 |

Observed rainfall sits below the full floor, so the outcome is `FULL`. That is the rule the
program applies, read back out of the account the program wrote.

Or from a shell:

```
curl -s https://api.devnet.solana.com -X POST -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"getProgramAccounts","params":["GeCxWiK9HXEuEsueNQawDKZnwE1SkojPw9dtYLBkcGBX",{"encoding":"base64","filters":[{"dataSize":368}]}]}'
```

## 3. The end-to-end run really landed

The ten signatures in `program/README.md` are checked live in the browser with
`getSignatureStatuses`. All ten came back `finalized`:

| Step | Slot |
|---|---|
| Deploy | 499,225,379 |
| Upgrade | 499,229,548 |
| CreateCampaign | 499,229,990 |
| LockRoster | 499,229,993 |
| Settle (55.0 mm) | 499,230,422 |
| Dispute | 499,230,425 |
| Settle final | 499,230,435 |
| Release | 499,230,437 |
| Claim farmer 0 | 499,230,440 |
| Claim farmer 1 | 499,230,443 |

The double claim in that run was rejected with `custom program error: 0xe` (`AlreadyClaimed`).

## 4. The tests pass

```
cd program && cargo test              # 9 unit tests
cd program && cargo build-sbf
cd program/tests-svm && cargo test    # 7 LiteSVM tests with real SPL Token CPIs
```

Output is reproduced in `program/README.md`.

## 5. The claim path is client-verifiable

`web/app.html` rebuilds the roster leaf (`sha256("NH1" || campaign || wallet || index_le`)) and
the merkle path (`sha256("NHN" || min || max)`) in the browser, exactly as `program/src/merkle.rs`
does on chain, and compares the result with `roster_root` from the fetched account. If your wallet
is not on that roster, the page says so and never sends a transaction. If the claim receipt PDA
already exists, it says the index is already claimed.

## What is not proven here

- No external audit, no fuzzing, no compute-unit budgeting.
- The oracle is one co-signing key. Nothing on chain checks the rainfall data itself.
- Devnet, test tokens. No real money has moved and no sponsor has signed anything.
- Token-2022 is not supported, and the upgrade authority is still a single key.
