# Reproducible proof — TEXT_AUTHENTICITY_CHECK

## Claim

The candidate distinguishes assertion-level authenticity findings that the incumbent's lexical
similarity often collapses. On a frozen public corpus, it ranks every correct or semantically
equivalent answer above every paired one-fact counterfactual or opposite verdict.

## Bound artifacts

| Role | Bytes | SHA-256 |
|---|---:|---|
| Candidate `dist/text_authenticity.wasm` | 30,897 | `3bb3bb82e0f6e2db9948e8ce96c8f1796835858d4b0a78332ec0b624501628a9` |
| Incumbent registration 850 | 23,987,851 | `432ae4423edd24ea74d8529fef8bf61d50ccc6622da94619482f9213b1f32395` |

The artifact is bound to source commit `638dae46ba31c1bf3a30e9d0e541b7c56f3fe48b`:
<https://raw.githubusercontent.com/Harshyadav442277/telegraph-factscore/638dae46ba31c1bf3a30e9d0e541b7c56f3fe48b/dist/text_authenticity.wasm>.
Fresh-download hashes reproduced the bound identity, and Linux CI run `33236230467` rebuilt the
tracked WASM byte-for-byte and passed every release gate.

The incumbent URL is commit-pinned in its registry entry:
<https://raw.githubusercontent.com/zkasuran/telegraph-salience-scorer/85381b739a9d047f068dc2b3642ceef9a569f48d/dist/xfmr/tn_t70.wasm>.
Its on-chain Keccak-256 is
`14f7076c4b4931efd33573ab3f2c9f3ee0eb6585101f0c238663e9340c004f57`.

## Frozen corpus

Corpus version `3a30a0fff08c62a3` contains 16 fixtures, 124 labelled answers, 256 strict
correct-over-wrong pairs, and 16 near-equality constraints:

- 12 clean-pair fixtures vary verdict, confidence, named model, perplexity, and burstiness while
  holding almost all surrounding wording constant; and
- 4 label-equivalence fixtures verify that `AI`/`machine-generated`, `human`/`person-written`,
  `original`/`authentic`, and `copied`/`plagiarised` are treated as equivalent findings rather
  than accidental token matches.

All fixtures are readable under `fixtures/synth/`. Correct paraphrases and counterfactuals are
rendered from the same fact record, preventing fluency or length from revealing the label.

## Results

| Measurement | Candidate | Incumbent |
|---|---:|---:|
| All pair wins | **256/256** | 33/256 |
| Mean good score | 0.996997 | 0.787247 |
| Mean bad score | 0.023152 | 0.912065 |
| Separation | **+0.973696** | -0.124818 |
| Score standard deviation | 0.485381 | 0.348862 |
| Worst self-match | 1.0 | 1.0 |
| Clean-pair wins | **240/240** | 21/240 |
| Label-equivalence wins | **16/16** | 12/16 |
| Near-equality constraints | **16/16** | 4/16 |

An out-of-corpus negation probe then tested positive and negative forms of AI/human and
original/copied verdicts, probability scaling, attribution, and multi-axis sentences. The prior
candidate inverted 10 of 20 comparisons, with worst margin `-0.999137`; the release passes 20/20
at mean margin `0.945619`. The probe is retained at `release/probe-negation.mjs` and is
deliberately not part of the benchmark corpus used to report the 256/256 result.

After registration 1671 lost 6 of 15 hidden semantic orderings, two more probes were frozen before
their corresponding fixes. Independent authenticity axes improved from 18/20 at margin 0.463029
to 20/20 at 0.974294. Ordinary vocabulary improved from 8/12 with near-zero mean margin to 12/12
at 0.999465. These public-definition-derived probes are retained under `release/`; no hidden
fixture text was available or used.

The candidate passes all applicable public promotion-proxy conditions; the history-based Spearman
condition is skipped because the intent has no recorded multi-miner history. `SKIP` is not counted
as a pass.

## Reproduce

Fast candidate-only check:

```bash
node harness/check-tac.mjs dist/text_authenticity.wasm
node harness/check-tac.mjs dist/text_authenticity.wasm --json
node release/probe-negation.mjs dist/text_authenticity.wasm
```

Candidate-versus-incumbent comparison:

```bash
mkdir -p harness/champions
curl -sL -o harness/champions/tn_t70_reg850.wasm \
  "https://raw.githubusercontent.com/zkasuran/telegraph-salience-scorer/85381b739a9d047f068dc2b3642ceef9a569f48d/dist/xfmr/tn_t70.wasm"
node harness/run-eval.mjs \
  --scorer dist/text_authenticity.wasm \
  --against harness/champions/tn_t70_reg850.wasm \
  --intent TEXT_AUTHENTICITY_CHECK
```

Independent structural and adversarial verification of the superseded v1.0.0 artifact is recorded at commit
[`f537c7c`](https://github.com/neromtoobad/telegraph-wasm-check/commit/f537c7c085e9d3366c5615fe1ad1f98a0abeff7c):
17/17 structural checks, 14/14 robustness checks, 500 seeded fuzz triples, fresh-instance
determinism, bounded memory, and 16/16 submitted custom cases, with no hard or soft failures. Those
results are historical evidence, not verification of the v1.2 bytes.

## Evidence boundary

This corpus is public and inspectable; Telegraph's evaluation fixtures are hidden and rotate. The
numbers above prove behavior on the bound public inputs and exact incumbent bytes. They do not
guarantee a network promotion. Only the returned on-chain evaluation block can supply that result.
