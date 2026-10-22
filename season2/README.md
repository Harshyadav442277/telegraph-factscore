# Season II evaluators

These are Telegraph Hackathon Season II Track 2 (Evaluators) modules, in the two kinds the network runs from
2026-10-21. Season I's `rank_answer` scorers in the root of this repo are retired. They stay here as the record of
the Season I entry.

| module | intents | what proves it |
|---|---|---|
| [comparator/](comparator/README.md): an exact-rule comparator, 2.2 KB of `no_std` Rust | 31 deterministic intents, one copy each in `comparator/dist/` | 234,848 cases through the node's own runtime, 0 mismatches against the network's answer key `ReferenceScore` |
| [adapter/llm_output_eval/](adapter/llm_output_eval/README.md): the training recipe for a ModernBERT reward model | `LLM_OUTPUT_EVALUATION` (judgment) | package not yet released: being retrained in the architecture the node actually runs (see below) |

The adapter package is not in this release. Its first build scored 96.9% on held-out pairs in PyTorch, but only 81.0% in the architecture the node runs. The kit's converter treats every ModernBERT layer as global attention with RoPE theta 10000, while ModernBERT slides a 128-token window on two layers in three and uses theta 160000 on the global ones. It is being retrained against the node's architecture, and it will ship in a later release with its numbers measured that way.

## Hashes (keccak256: what `registerWasm` takes)

- The 31 comparators: [comparator/dist/manifest.json](comparator/dist/manifest.json)

To check hosted bytes, with the URL being a raw file or a release asset:

```bash
cd comparator/verify && go run . hash <url-or-file>
```

## Reproduce

```bash
# comparator
cd comparator && cargo build --release --target wasm32-unknown-unknown
cd verify && go run . ../target/wasm32-unknown-unknown/release/telegraph_comparator.wasm

# adapter pairs (FEVER gold evidence, copenlu/fever_gold_evidence, train.jsonl)
python -B -I adapter/llm_output_eval/build_eval_pairs.py --fever <train.jsonl> --out <dir>
```

Train, convert and package with telegraph-wasm-baseline `feature/v2-integration` at `992f2650`, using the
commands in the adapter README.

## Why the FACT_CHECK pairs are here but no FACT_CHECK package

The FACT_CHECK pairs pass the kit's data gate, but the kit's packager refuses the intent. Its class map covers only
the 31 judgment and hybrid intents added on 2026-09-08. The docs call FACT_CHECK a judgment intent, and we have
asked the organisers which one the node follows. The same method was moved to `LLM_OUTPUT_EVALUATION`, which the
kit does accept.

## Disclosure

The author also runs the miner `livecert`, which serves 9 of the 31 comparator intents:

> MACRO_ECONOMIC_INDICATOR, CORPORATE_REGISTRY_LOOKUP, REGULATORY_FILING_MONITOR, CRYPTO_PRICE, FINANCIAL_DATA,
> WEATHER_CHECK, CROSS_CHAIN_STATE_VERIFY, GAS_PRICE, STOCK_PRICE

A comparator has no discretion: it must reproduce the network's rule exactly, or the node rejects it. The adapter's
training data contains no miner's answers and no miner's format. None of the training pairs was uploaded to the
evaluation pool.

## Licences

The code is MIT. The adapter training data is derived from FEVER (Thorne et al., 2018) and Wikipedia, CC BY-SA 3.0.
