# Proof — headline-quantity scorers

Four scoring modules for the intents whose question is a single number:
`STOCK_PRICE`, `CRYPTO_PRICE`, `TVL_LOOKUP`, `ONCHAIN_TX_LOOKUP`. One profile,
`headline_quantity_profile` in [`src/profile.rs`](src/profile.rs), serves all
four; only `ONCHAIN_TX_LOOKUP` overrides anything (its absolute epsilon).

Everything below is reproducible from this repository. No number is quoted from
theory.

## The defect these modules fix

Take a ground truth, change **only** its headline figure, and leave every other
word identical. That answer is now wrong for exactly one reason.

The general-purpose profile scored it **0.927**:

| channel | value | why |
|---|---|---|
| precision | 0.959 | one token out of many moved |
| fact | 0.765 | the wrong figure was averaged against the dates and times that still agreed |
| raw | 0.771 | |
| **final** | **0.927** | concave shaping lifted 0.771 |

The whole intent is that one figure. So the numeric channel now takes full
authority, reads the **worst** comparable figure instead of the mean, and
decays steeply enough that display rounding survives while a different price
does not. Reproduce with `node verify.mjs dist/stock_price.wasm`.

## Role-scoped figure comparison

These ground truths quote several figures side by side:

```
The current share price for Apple Inc. (AAPL) is **$309.25** as of August 21, 2026.
- **Day's Range**: $307.01 - $312.38
- **52-Week Range**: $224.69 - $344.57
- **Market Cap**: Approximately $4.51 trillion
```

"Best agreement over every figure in the ground truth" is the wrong question.
A 9%-wrong current price of $337.08 lands near the 52-week high of $344.57 and
was scored as nearly right — measured at **0.55** where it should be near zero.

Each figure now carries the stems of its two nearest preceding content words
(`tokens::role_overlap`). A figure is compared against ground-truth figures in
the same role whenever any exist. A current price is judged against the current
price, not against a 52-week range. This is off for every other profile.

## Measured against the live champions

Champion binaries were downloaded from the `wasm_url` on-chain and their
Keccak-256 checked against the registry's `wasm_hash`. **All match**, so the
incumbent measured here is the incumbent the node runs.

| intent | champion | size | hash |
|---|---|---|---|
| STOCK_PRICE | reg 48 | 1,039,661 B | `51ce38b3…652b6d` MATCH |
| CRYPTO_PRICE | reg 222 | 1,071,645 B | verified MATCH |
| ONCHAIN_TX_LOOKUP | reg 642 | 23,989,222 B | verified MATCH |

### STOCK_PRICE — 16 cases, four answer shapes

Pairs are built from recorded traffic: the good side is **verbatim miner prose**,
the bad side is the same prose with only the headline figure rescaled. Exactly
one objective thing differs.

| answer shape | champion wins | champion margin | ours wins | ours margin |
|---|---|---|---|---|
| ground truth vs figure-swapped | 16/16 | 0.9241 | 16/16 | **0.9356** |
| first line vs figure-swapped | 16/16 | 0.8752 | 16/16 | **0.9359** |
| recorded prose vs figure-swapped | 15/16 | 0.0741 | 15/16 | **0.1344** |
| ground truth vs recorded wrong answer | 16/16 | 0.8725 | 16/16 | **0.9906** |

At least the champion's case wins, and a larger margin, on **every** shape.

### CRYPTO_PRICE and ONCHAIN_TX_LOOKUP

| intent | ours | champion |
|---|---|---|
| CRYPTO_PRICE | 8/8 pairs, margin **0.960172** | 7/8, margin 0.000000 |
| ONCHAIN_TX_LOOKUP | 8/8 pairs, margin **0.901790** | 8/8, margin 0.004102 |

**Both corpora hold only two cases.** That is a thin validation and is stated as
such rather than dressed up.

## What the incumbents actually do on live traffic

Recorded `STOCK_PRICE` traffic, ground truth **$319.70**:

| miner answer | live score |
|---|---|
| "$319.64" — wrong | **0.0208** |
| "$319.70" — exactly right | **0.0140** |

The champion ranked the wrong answer above the exactly-correct one. In another
recorded case a correct answer (relative error 0.00004) scored **0.0196** while
one that was 2% wrong scored **0.6684** — a 34× inversion, on live traffic, not
a constructed example.

## Honest limits

- **The margins above are not predictions of node margin.** The STOCK_PRICE
  champion scores 0.074 on this corpus and 0.6147 on the node's own fixtures, so
  this corpus models *ordering* well and *absolute margin* badly. The defensible
  claim is the comparison on identical inputs, not the absolute number.
- **`TVL_LOOKUP` is unmeasured.** Only 82 of its 150 recorded rows carry a
  converted answer and none states its ground truth's quantity, so no clean pair
  exists and its corpus is empty. That module ships the same profile with no
  intent-specific evidence.
- **Recorded traffic contains no clean pairs for STOCK_PRICE.** Every miner is
  slightly stale — ground truth 491.54, miners 491.71 — because the price moves.
  The traffic corpus therefore separates "less stale" from "more stale", which is
  why the counterfactual corpus is the one measurements are quoted from.
- Only two cases each for CRYPTO_PRICE and ONCHAIN_TX_LOOKUP.

## Reproduce

```bash
node harness/build-factswap.mjs --intent STOCK_PRICE
node harness/run-numeric.mjs fixtures/numeric/STOCK_PRICE-factswap.json \
  ours=dist/stock_price.wasm
node verify.mjs dist/stock_price.wasm
cargo test --no-default-features --features stock-price
```

Nine profiles pass tests, clippy `-D warnings`, `cargo fmt --check`, and the
Stage-1 verifier. Every module is zero-import freestanding wasm32 and scores in
microseconds — which matters, because the node enforces a ten-minute evaluation
budget that has killed several large-model candidates outright.
