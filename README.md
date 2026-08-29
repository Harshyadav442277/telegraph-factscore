# Telegraph fact-aware scorer — TEXT_AUTHENTICITY_CHECK

This repository contains one registration artifact: a 25,887-byte freestanding WebAssembly
scorer for Telegraph's `TEXT_AUTHENTICITY_CHECK` intent.

Published artifact commit:
[`25ff808`](https://github.com/Harshyadav442277/telegraph-factscore/commit/25ff8089d4d3f1cfcc639115e14464d7d6313cc1).
The commit-pinned raw download was independently re-fetched and reproduced the byte length and
both hashes shown below.

The incumbent can score a one-word wrong verdict almost identically to the truth. This scorer
compares assertion meaning instead: verdict polarity, equivalent labels, named-model attribution,
entities, percentages, perplexity, burstiness, and other typed figures can change the score even
when the surrounding prose is unchanged.

## Headline result

| Public reproducible evidence | This scorer | Incumbent reg. 850 |
|---|---:|---:|
| Correct-over-counterfactual pairs | **256/256** | 33/256 |
| Separation, mean good minus mean bad | **+0.973844** | -0.124818 |
| Clean-pair verdict/fact comparisons | **240/240** | 21/240 |
| Equivalent-label comparisons | **16/16** | 12/16 |
| Equivalent-answer constraints | **16/16** | 4/16 |

These are public offline measurements, not a claim about Telegraph's hidden rotating fixtures.
The exact reproduction and evidence boundary are in [PROOF.md](PROOF.md); the 90-second rubric map
is [JUDGE_BRIEF.md](JUDGE_BRIEF.md).

## Verify in one command

Node 18+ is the only requirement:

```bash
node harness/check-tac.mjs dist/text_authenticity.wasm
node release/probe-negation.mjs dist/text_authenticity.wasm
```

The command checks the Telegraph ABI, blank behavior, score range, self-match, all 256 semantic
orderings, and all 16 equivalence constraints. Add `--json` to the first command for
machine-readable benchmark output. The second command independently checks 20 held-out negation
and categorical comparisons that are not counted in the benchmark totals.

To verify the broader ABI and adversarial input suite:

```bash
node verify.mjs dist/text_authenticity.wasm
node release/verify-standalone.mjs .
```

## Reproduce the registered bytes

The repository pins Rust 1.98.0:

```bash
cargo +1.98.0 test --no-default-features --features text-authenticity
cargo +1.98.0 clippy --all-targets --no-default-features --features text-authenticity -- -D warnings
cargo +1.98.0 build --release --target wasm32-unknown-unknown --no-default-features --features text-authenticity
cmp target/wasm32-unknown-unknown/release/scorer.wasm dist/text_authenticity.wasm
```

Frozen identity:

```text
bytes      25887
sha256     e7bb15f12e55aa5a0cb8fa30f5d2d5a21a3027d026b207d3d8563d2ae2ae52b6
keccak256  bdd3fea5deb7ce2a48663aa7ec63d5a295ade30c4c2bb2d3254031cb04cdca0f
```

Commit-pinned registration URL:

```text
https://raw.githubusercontent.com/Harshyadav442277/telegraph-factscore/25ff8089d4d3f1cfcc639115e14464d7d6313cc1/dist/text_authenticity.wasm
```

## Design

The module is `no_std`, has zero imports, and exports Telegraph's allocation and six-argument
`rank_answer` ABI. Its hot path is deterministic byte parsing plus integer/`f32` arithmetic:

1. blank answers return exactly zero and normalized self-matches return exactly one;
2. tokens are classified as prose, entities, identifiers, or typed numeric facts;
3. closed-set authenticity labels are compared by semantic class, including equivalent wording;
4. supported assertions earn precision while substitutions and contradictions reduce independent
   multiplicative channels; and
5. a smoothstep calibration preserves ranking without a brittle binary threshold.

There are no miner identities, wallets, fixture IDs, network calls, clocks, randomness, or hidden
data probes in the scorer. The source is under `src/`; `src/profile.rs` contains the complete
decision surface and `src/score.rs` the composition logic.

## Honest boundary

- The network benchmark is hidden, so only an on-chain registration supplies the final verdict.
- Text alone cannot establish the truth of an unsupported appended claim; it receives a bounded
  penalty rather than a fabricated factual judgment.
- GitHub source can evolve, but different WASM bytes require a new hash and `registerWasm` call.
- Publishing tooling is not community adoption. Only genuine external use or feedback is counted.

The full reusable harness and corpus are included so other script authors can test their own
modules without spending a transaction.
