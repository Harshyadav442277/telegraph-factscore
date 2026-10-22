# LLM_OUTPUT_EVALUATION evaluator adapter: training pairs

`LLM_OUTPUT_EVALUATION` is a judgment intent: "judge a generative model's output for factual fidelity,
citation support and hallucination". The organiser's kit can package it; it cannot package FACT_CHECK
(its packager refuses FACT_CHECK, see ../README.md). This folder uses the FACT_CHECK method one level up. The
question gives a model's statement and the source it cites. The miner's answer is an evaluation of that
statement.

Everything the evaluator needs is in the prompt, so the task is reading two texts against each other, not
recalling facts. That is a better fit for a 150M-parameter reward model than open-world fact checking.

## What a pair teaches

**Statements:**

- A statement is **faithful** when it is a FEVER SUPPORTS claim.
- It is **contradicted** when it is a FEVER REFUTES claim, or a SUPPORTS claim whose number or date was edited
  away from the source.
- The two classes are balanced 50/50 in every split, so "always say hallucination" is worth nothing.

**Evaluations:** the chosen evaluation gets the verdict and the detail right. The rejected one uses the same
template and is wrong in one way:

| kind | rejected evaluation |
|---|---|
| `flip` | the opposite verdict |
| `fabricate` | the opposite verdict, with a specific misreading. Either it invents a discrepancy the source does not have, or it says a hallucinated value "matches". |
| `misquote` | the right verdict, with the wrong detail. Either the discrepancy is reported back to front, or the evaluation "matches" a value that appears nowhere. |

Every evaluation names the statement it judges. Without that, the first build reused generic reasons: only
55.6% of rejected answers were unique, and 49.6% of held-out negatives had appeared in training. The kit's
gate refused it.

## Build

```bash
python -B -I build_eval_pairs.py --fever <fever_gold/train.jsonl> --out <dir> --train 6000 --val 800 --test 800
# from the kit root, with the kit venv:
python -m wide_runtime.train_adapter --intent LLM_OUTPUT_EVALUATION --data <dir>/train.jsonl \
  --val-data <dir>/val.jsonl --test-data <dir>/test.jsonl --batch-size 8 --grad-accum 2
```

The generator reuses `../fact_check` (FEVER loading, edits, and the split by article), with seed 11.

## Data gate, 2026-10-22

| check | value | limit |
|---|---|---|
| unique negatives | 100.0% | ≥ 80% |
| largest single negative | 0.0% | ≤ 2% |
| length shortcut | 51.0% | ≤ 65% |
| held-out leak | 0.0% | ≤ 10% |
| surface probe | 52.8% | ≤ 65% (50% means no tell) |

**Kinds per split:**

| split | flip | fabricate | misquote | faithful / contradicted |
|---|---|---|---|---|
| train | 2589 | 1691 | 1720 | 2973 / 3027 |
| val | 351 | 219 | 230 | 395 / 405 |
| test | 337 | 212 | 251 | 424 / 376 |

## Training and package, 2026-10-22

> **Withdrawn.** This first package scores 81.0% as the node runs it, not 96.9%. The kit converts every
> layer to global attention with RoPE theta 10000, unlike the ModernBERT it trains. A retrain against the node's
> architecture replaces it. The numbers below are kept as the record of what was measured, in PyTorch.

**Training:** the kit's `train_adapter`, ModernBERT-base with LoRA r=8, batch 8 × grad-accum 2, 3 epochs. It took
26 minutes on an RTX 3050 Laptop.

| | epoch 0 | epoch 1 | epoch 2 |
|---|---|---|---|
| train loss | 0.489 | 0.129 | 0.068 |
| validation accuracy | 93.1% | 97.1% | 97.5% |

**Final gate on the 800 test pairs** (articles never seen in training):

| check | value |
|---|---|
| pairwise accuracy | **96.9%** |
| "pick the longer" baseline | 51.7% |
| negative memorisation | 0.0 |
| verdict | PASS |

**Package:** `convert_adapter` (1.2 GB `.imwq`, 22 layers) and then `adapterbuild`. Its manifest records:

- class judgment;
- 1,205,363,765 bytes;
- keccak256 `0xe54abbfed8c9f20f89c836a1e9a628978537eb7cac06041eac748f45b067ed89`;
- sha256 `d9796362fe741837972f24b01771692bc880d17e02e4cc396634be957be50dcc`;
- two builds byte-identical;
- the node-load check passed;
- 4 GiB budget headroom of 0.94 GB.

These are in-distribution numbers: the test pairs come from the same templates as training. The node judges the
module on the hidden evaluation pool, which other people write. Only that can say whether 96.9% carries over.

## Limits

- The data covers factual fidelity against one cited sentence. The intent also names tone, safety and
  multi-source citation support, which none of these pairs teach.
- All statements and sources come from Wikipedia via FEVER.

Source, licence and integrity rules are the same as [../fact_check/README.md](../fact_check/README.md): FEVER,
CC BY-SA 3.0. None of these pairs goes to the evaluation pool.
