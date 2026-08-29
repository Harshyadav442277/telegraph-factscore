# Focused TAC verification harness

This directory contains only the modules needed to inspect the
`TEXT_AUTHENTICITY_CHECK` release. Node 18+ is sufficient.

Fast candidate-only semantic gate:

```bash
node harness/check-tac.mjs dist/text_authenticity.wasm
node harness/check-tac.mjs dist/text_authenticity.wasm --json
node release/probe-negation.mjs dist/text_authenticity.wasm
```

The first command checks the six-argument Telegraph ABI, blank behavior, range, self-match, 256
public correct-over-counterfactual orderings, and 16 equivalent-wording constraints. The second
form emits JSON. The separate probe checks 20 held-out negation and multi-axis comparisons; those
cases are not counted in the 256 benchmark pairs.

Candidate-versus-incumbent proof:

```bash
mkdir -p harness/champions
curl -sL -o harness/champions/tn_t70_reg850.wasm \
  "https://raw.githubusercontent.com/zkasuran/telegraph-salience-scorer/85381b739a9d047f068dc2b3642ceef9a569f48d/dist/xfmr/tn_t70.wasm"
node harness/run-eval.mjs \
  --scorer dist/text_authenticity.wasm \
  --against harness/champions/tn_t70_reg850.wasm \
  --intent TEXT_AUTHENTICITY_CHECK
```

The incumbent download is optional and intentionally not committed. Its expected SHA-256 is
`432ae4423edd24ea74d8529fef8bf61d50ccc6622da94619482f9213b1f32395`.

| Module | Purpose |
|---|---|
| `check-tac.mjs` | Incumbent-free public TAC regression gate |
| `wasm-abi.mjs` | Exact allocator/write/six-argument scorer call path |
| `corpus.mjs` | Fixture validation and deterministic statistics |
| `run-eval.mjs` | Full candidate-versus-incumbent promotion proxy |
| `score-pool.mjs` | Deterministic worker pool for the large incumbent |
| `report.mjs` | Plain-text proxy report |

This harness evaluates public fixtures. Telegraph's hidden rotating fixtures remain the only
authoritative promotion test.
