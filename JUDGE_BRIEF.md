# Judge brief — TEXT_AUTHENTICITY_CHECK

## The result in 90 seconds

The current canonical scorer for AI-text detection can prefer a wrong categorical verdict when
one word changes and the surrounding prose stays the same. On the public native corpus, flipping
`AI-generated` to `human-written` leaves the incumbent near 1.0; its clean-pair margin is
**-0.165231**.

This submission replaces vocabulary proximity with assertion-level agreement. The frozen scorer:

| Evidence | This scorer | Incumbent registration 850 |
|---|---:|---:|
| Native TAC clean pairs | **240/240** | 21/240 |
| Native TAC clean-pair margin | **0.970427** | -0.165231 |
| Label-equivalence pairs | **16/16** | 12/16 |
| Combined separation | **0.973696** | -0.124818 |
| Content-verification holdout | **144/144** | not the comparison target |

These are offline, reproducible measurements against the exact pinned incumbent WASM—not claims
about Telegraph's hidden fixtures. v1.1.0 was registered as 1671: Stage 1 passed, but Stage 2
rejected it at **9/15** and margin **0.3274022** versus champion 850 at **14/15** and
**0.65861213**. The current v1.2 candidate is a semantic repair of that measured failure and has
not yet received a network verdict.

## Why this is an actual evaluator improvement

The property gained is simple: **a fact or verdict can change the score because its meaning
changed, even when almost every word stayed identical.** A lexical evaluator cannot reliably make
that distinction.

The scorer separately handles:

- closed-set verdicts and their opposites (`AI`/`human`, `original`/`copied`);
- equivalent wording (`AI`/`machine generated`, `original`/`authentic`);
- named model substitutions across token shapes (`GPT-4` versus `Claude`);
- typed figures and unit conversion (`5 m/s` equals `18 km/h`);
- identifiers, country aliases, signed/hemisphere coordinates and entity substitutions;
- negation, question parroting, refusals, keyword stuffing and blank answers; and
- fluent one-fact counterfactuals rather than corrupted synthetic negatives.

There are no miner fingerprints, author identities, fixture IDs, hidden-data probes or
question-specific answers in the scorer.

## Live rubric mapping

The Track 2 rules currently allocate 50% to improvement over the canonical script, 30% to
robustness/code quality, 10% to X engagement and 10% to genuine community adoption. Winners are
chosen by focused manual review.

### 50% — improvement over baseline

- `PROOF.md` binds every comparison to candidate and incumbent SHA-256 values.
- The native corpus contains 256 pairwise orderings across correct paraphrases, terse forms,
  verbatim answers, single-fact counterfactuals, opposites and hedges.
- The content-verification holdout stayed 144/144 after TAC-specific changes, reducing the risk
  that the result is a narrow label lookup.
- The proof deliberately reports losing targets and limitations; it is not a winners-only table.

### 30% — robustness and code quality

- 30,897-byte freestanding `wasm32-unknown-unknown` module, zero imports.
- Pinned Rust 1.98.0 build reproduces the frozen SHA byte-for-byte.
- The release profile passes 85 Rust tests plus `clippy -D warnings`; all five profile matrices pass.
- Local verifier covers ABI arity, blank/Unicode/NUL/large inputs, allocation, range, determinism,
  self-match and semantic separation.
- Independent `telegraph-wasm-check` commit `f537c7c` verified the superseded v1.0.0 artifact;
  those results are not claimed for the v1.1.0 bytes.
- CI rebuilds the real registration feature and fails if size or SHA-256 drifts from the release
  manifest.
- `node harness/check-tac.mjs path/to/module.wasm` gives any TAC author a zero-install,
  incumbent-free public benchmark with machine-readable output. The frozen scorer passes 256/256;
  the exact incumbent fails at 33/256, so the tool discriminates rather than rubber-stamping both.
- A separate 20-pair metamorphic set was never loaded by the public checker. It exposed ten
  negation inversions in the prior candidate (`not AI`, `not human`, `not original`, and negated
  answers). v1.2 passes 20/20 at mean margin 0.945619; the rejected broad shortcut that
  hid an invented model is retained in the worklog as evidence of adversarial selection.
- Two probes frozen before their fixes expose the network-relevant weakness directly: independent
  authenticity axes improved from 18/20 at 0.463029 to 20/20 at 0.974294, and ordinary semantic
  vocabulary improved from 8/12 near zero margin to 12/12 at 0.999465.

### 20% — public evidence, never fabricated

X engagement remains unclaimed until the user publishes the prepared posts. Community reuse now
has one concrete external receipt: [`shreshth006/telegraph-factscore`](https://github.com/shreshth006/telegraph-factscore/commits/main/)
contains nine downstream commits using the shared fact-aware kernel for IP-geolocation work. This
is adoption of the code, not independent validation of this TAC artifact and not endorsement of
every claim in the diverged fork. No stars, testimonials, or benchmark issues are claimed.

## Frozen release identity

```text
artifact   dist/text_authenticity.wasm
bytes      30897
sha256     3bb3bb82e0f6e2db9948e8ce96c8f1796835858d4b0a78332ec0b624501628a9
keccak256  8cfc5456b08363d281878b59f587ad9c44b7296b211a6a4bab4ec794a3c58a07
```

The Keccak implementation was checked by reproducing registration 850's public on-chain hash.
`release/text-authenticity.json` is the machine-readable manifest. Hosted-byte and Linux CI
reproduction must pass before the remaining user-signed registration step.

## Fast verification path

```powershell
cargo +1.98.0 test --no-default-features --features text-authenticity
cargo +1.98.0 build --release --target wasm32-unknown-unknown --no-default-features --features text-authenticity
node verify.mjs dist/text_authenticity.wasm
```

Then read `PROOF.md` sections 1, 3, 5 and 9. Section 6 contains the commit-pinned incumbent URLs;
the proof generator refuses to combine a report with an artifact whose SHA no longer matches.

## Honest limits

- The network benchmark is hidden; offline promotion is evidence, not a guarantee.
- This binary is deliberately tuned for `TEXT_AUTHENTICITY_CHECK`, not generic finance or
  sentiment scoring.
- Text alone cannot prove whether an unsupported appended fact is true; such claims receive a
  bounded penalty rather than pretending the scorer has external knowledge.
- A changed WASM requires a new registration even if GitHub source changes freely afterward.

Sources: [live Track 2 rules](https://hackathon.telegraphprotocol.com/rules),
[scoring-module documentation](https://docs.telegraphprotocol.com/docs/scoring/build-a-scoring-module),
[public WASM registry](https://devnode.telegraphprotocol.com/api/wasm?intent=TEXT_AUTHENTICITY_CHECK),
and [independent verifier](https://github.com/neromtoobad/telegraph-wasm-check).
