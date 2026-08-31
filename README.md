# Telegraph FactScore

Fact-aware, zero-import WebAssembly evaluators for Telegraph intents whose answers carry typed
assertions: prices, balances, identifiers, coordinates, units, and categorical verdicts.

This is the focused source repository for the Track 2 submission. The broader measurement and
promotion-gate audit is in the
[`miner` submission brief](https://github.com/Harshyadav442277/miner/blob/main/track2/SUBMISSION.md).

## Primary network receipt

The primary form entry is **registration 1725** for `CRYPTO_PRICE`, built from the immutable
[`a0318af` artifact](https://github.com/Harshyadav442277/telegraph-factscore/blob/a0318afd0faed3c519fae4dab63b7a238e6e8031/dist/crypto_price_b3.wasm).

The Telegraph node measured:

| Gate signal | FactScore | Incumbent |
|---|---:|---:|
| Correct fixture orderings | **14/15** | **14/15** |
| Separation margin | **0.7219137** | 0.6295639 |
| Worst self-match | **1.0** | — |
| Real-traffic rank agreement | -0.110385686 | required ≥ 0.60 |

The registration was rejected on the last line: it disagreed with the incumbent's ordering of
real miner traffic. It is not described as active. It is useful evidence because the candidate
matched the incumbent's hidden-fixture win count and separated good/bad fixtures more strongly,
yet correcting recorded incumbent inversions makes agreement with that incumbent worse.

## What the evaluator changes

Lexical similarity can reward the wrong answer when most surrounding words still match. FactScore
adds typed, role-scoped comparison so a current price is compared with the ground truth's current
price—not its date, market cap, or 52-week high—and equivalent units remain equivalent.

One recorded `STOCK_PRICE` row had ground truth `$319.70`; the incumbent scored `$319.64` at
**0.0208** and the exact `$319.70` answer at **0.0140**. Another exactly-correct answer scored
0.0196 while one 2% wrong scored 0.6684. These are recorded network rows, not authored examples.

The latest public proof reports:

| Intent | FactScore | Pinned incumbent |
|---|---:|---:|
| `STOCK_PRICE` | 16/16 across each controlled headline-figure shape; larger margin on every shape | 15–16/16 depending on shape |
| `CRYPTO_PRICE` | **8/8**, margin **0.960172** | 7/8, margin 0.000000 |
| `ONCHAIN_TX_LOOKUP` | **8/8**, margin **0.901790** | 8/8, margin 0.004102 |
| `TVL_LOOKUP` | shipped from the shared profile, **unmeasured** | no admissible clean-pair corpus |

Full method, exact champion hashes, corpus limits, and commands:
[`NUMERIC_PROOF.md`](NUMERIC_PROOF.md).

## Robustness

- deterministic Rust `no_std` modules, about 31–33 KB each;
- zero WASM imports and no network, clock, randomness, identities, or hidden-data probes;
- Telegraph ABI checks for blank answers, self-match, range, arity, allocation, Unicode, NULs,
  oversized inputs, and determinism;
- pinned Rust toolchain, `cargo fmt`, `clippy -D warnings`, unit tests, and Linux CI; and
- microsecond scoring rather than embedding-model inference inside the node's ten-minute gate.

## Verify

```bash
node verify.mjs dist/crypto_price_b3.wasm
node harness/run-numeric.mjs fixtures/numeric/CRYPTO_PRICE-factswap.json \
  ours=dist/crypto_price_b3.wasm
cargo test --no-default-features --features crypto-price
```

## Repository map

```text
src/                evaluator implementation and intent profiles
dist/               compiled WASM artifacts
fixtures/numeric/   recorded and controlled typed-fact corpora
harness/            deterministic ABI and comparison runners
NUMERIC_PROOF.md    current measured claim and honest limits
.github/workflows/  reproducible CI checks
```

## Evidence boundary and disclosure

- Registration 1725 is rejected, not active; its node verdict is reported verbatim above.
- Public-corpus margins are not predictions of hidden-fixture margins. Comparisons use identical
  inputs and exact pinned incumbent binaries.
- `TVL_LOOKUP` is not counted as measured evidence.
- The current `main` branch contains newer b4 artifacts. Registration 1725 remains bound to the
  immutable b3 bytes at commit `a0318afd0faed3c519fae4dab63b7a238e6e8031`.
- The author also operates Track 1 miner `livecert` (miner ID 4433, active registration 389). The
  overlap was disclosed to the organizers. FactScore contains no miner slug, wallet, field name,
  response-template fingerprint, or special case for LiveCert.

License: [MIT](LICENSE).
