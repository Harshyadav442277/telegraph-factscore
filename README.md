# Telegraph fact-aware scorer — TEXT_AUTHENTICITY_CHECK

This repository contains one registration artifact: a 30,897-byte freestanding WebAssembly
scorer for Telegraph's `TEXT_AUTHENTICITY_CHECK` intent.

This is the v1.2 semantic repair. Its predecessor was registered as 1671: Stage 1 passed, but
Stage 2 rejected it at 9/15 orderings and margin 0.3274022. v1.2 separates independent
authenticity axes and expands ordinary paraphrase coverage rather than hiding that result.
The commit-pinned download reproduces both hashes below; Linux CI run `33236230467` rebuilds it
byte-for-byte from source.

The incumbent can score a one-word wrong verdict almost identically to the truth. This scorer
compares assertion meaning instead: verdict polarity, equivalent labels, named-model attribution,
entities, percentages, perplexity, burstiness, and other typed figures can change the score even
when the surrounding prose is unchanged.

## Headline result

| Public reproducible evidence | This scorer | Incumbent reg. 850 |
|---|---:|---:|
| Correct-over-counterfactual pairs | **256/256** | 33/256 |
| Separation, mean good minus mean bad | **+0.973696** | -0.124818 |
| Clean-pair verdict/fact comparisons | **240/240** | 21/240 |
| Equivalent-label comparisons | **16/16** | 12/16 |
| Equivalent-answer constraints | **16/16** | 4/16 |

These are public offline measurements, not a claim about Telegraph's hidden rotating fixtures.
Predeclared out-of-corpus checks also pass: negation 20/20 (0.945619 mean margin), model aliases
10/10 (0.960045), independent authenticity axes 20/20 (0.974294), and ordinary authenticity
vocabulary 12/12 (0.999465).
The exact reproduction and evidence boundary are in [PROOF.md](PROOF.md); the 90-second rubric map
is [JUDGE_BRIEF.md](JUDGE_BRIEF.md).

## Verify in one command

Node 18+ is the only requirement:

```bash
node harness/check-tac.mjs dist/text_authenticity.wasm
node release/probe-negation.mjs dist/text_authenticity.wasm
node release/probe-authenticity-axes.mjs dist/text_authenticity.wasm
node release/probe-authenticity-vocabulary.mjs dist/text_authenticity.wasm
```

The command checks the Telegraph ABI, blank behavior, score range, self-match, all 256 semantic
orderings, and all 16 equivalence constraints. Add `--json` to the first command for
machine-readable benchmark output. The second command independently checks 20 held-out negation
and categorical comparisons that are not counted in the benchmark totals.

Genuine results—including failures—can be submitted through the repository's
[benchmark report form](https://github.com/Harshyadav442277/telegraph-factscore/issues/new?template=benchmark-result.yml).

To verify the broader ABI and adversarial input suite:

```bash
node verify.mjs dist/text_authenticity.wasm
node release/verify-standalone.mjs .
```

## Reproduce the frozen bytes

The repository pins Rust 1.98.0:

```bash
cargo +1.98.0 test --no-default-features --features text-authenticity
cargo +1.98.0 clippy --all-targets --no-default-features --features text-authenticity -- -D warnings
cargo +1.98.0 build --release --target wasm32-unknown-unknown --no-default-features --features text-authenticity
cmp target/wasm32-unknown-unknown/release/scorer.wasm dist/text_authenticity.wasm
```

Frozen identity:

```text
bytes      30897
sha256     3bb3bb82e0f6e2db9948e8ce96c8f1796835858d4b0a78332ec0b624501628a9
keccak256  8cfc5456b08363d281878b59f587ad9c44b7296b211a6a4bab4ec794a3c58a07
```

Commit-pinned registration URL:

```text
https://raw.githubusercontent.com/Harshyadav442277/telegraph-factscore/638dae46ba31c1bf3a30e9d0e541b7c56f3fe48b/dist/text_authenticity.wasm
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

## Disclosure and community reuse

The module author also operates the Track 1 miner `livecert` (registration 225). That relationship
is disclosed for review. The scorer applies the same public ABI and semantic rules to every answer;
it contains no miner slug, wallet, registration, or response-template fingerprint.

As of 2026-08-29, a public external fork has made nine downstream, measured commits using the
shared fact-aware kernel for IP-geolocation work:
[`shreshth006/telegraph-factscore`](https://github.com/shreshth006/telegraph-factscore/commits/main/).
This is concrete source reuse, not a claim that the fork independently validated this TAC artifact
or that upstream endorses every statement in the diverged branch.

## Honest boundary

- The network benchmark is hidden, so only an on-chain registration supplies the final verdict.
- Text alone cannot establish the truth of an unsupported appended claim; it receives a bounded
  penalty rather than a fabricated factual judgment.
- GitHub source can evolve, but different WASM bytes require a new hash and `registerWasm` call.
- Publishing tooling is not community adoption. Only genuine external use or feedback is counted.

The full reusable harness and corpus are included so other script authors can test their own
modules without spending a transaction.
