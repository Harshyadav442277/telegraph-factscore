# FACT_CHECK evaluator adapter: training pairs

This folder holds the training data recipe for a Season II wide adapter on `FACT_CHECK`. The adapter is a
ModernBERT reward model, built with the organiser's kit `telegraph-wasm-baseline`, branch
`feature/v2-integration`. It carries over the Season I thesis: an answer is judged by what it asserts, not by
how it is worded.

## What a pair teaches

Each claim gets two answers that use the same template, so wording carries no signal.

- **chosen** gives FEVER's verdict and quotes the gold Wikipedia sentence.
- **rejected** is wrong in exactly one way:

| kind | rejected answer | what the model has to notice |
|---|---|---|
| `flip` | opposite verdict, same quote | the verdict contradicts its own support |
| `fabricate` | opposite verdict, quote edited to agree with it | a coherent answer whose date, number or entity is false |
| `misquote` | right verdict, edited quote | the quote no longer says what the claim says |
| `offtopic` | right verdict, quote from an unrelated article | the support is not about the claim |

**What is edited:**

- In SUPPORTS claims, a number or date that the claim and the quote share. Days stay valid days, years stay
  plausible years, and the number format is kept.
- In REFUTES claims, the span where the claim substitutes its own value. That span must have an anchor and the
  same shape: a number for a number, a Name for a Name.
- Names are never swapped at random. That produced nonsense like "the Simon Cowell for Best Actress", which
  teaches the model to spot odd text, not wrong facts.

## Build

```bash
# data and outputs live outside OneDrive: C:\Users\hyada\tg-eval
python -I build_pairs.py --fever <fever_gold/train.jsonl> --out <dir> --train 6000 --val 800 --test 800
python -m wide_runtime.train_adapter --intent FACT_CHECK --data <dir>/train.jsonl \
  --val-data <dir>/val.jsonl --test-data <dir>/test.jsonl --check-data      # from the kit root
```

- Splits are by Wikipedia article, so no article appears in more than one of train, val and test.
- Labels are 50/50 in every split.
- The build is deterministic for a given `--seed`.

## Data gate, 2026-10-21

This is the first full build, seed 7. It passed the kit's `--check-data`:

| check | value | limit |
|---|---|---|
| unique negatives | 98.6% | ≥ 80% |
| largest single negative | 0.1% | ≤ 2% |
| length shortcut | 51.5% | ≤ 65% |
| held-out leak | 2.2% | ≤ 10% |
| surface probe | 53.5% | ≤ 65% (50% means no tell) |

**Kinds per split:**

| split | flip | fabricate | misquote | offtopic |
|---|---|---|---|---|
| train | 2394 | 1522 | 1291 | 793 |
| val | 308 | 219 | 176 | 97 |
| test | 338 | 193 | 166 | 103 |

## Source and licence

- **Source:** FEVER (Thorne et al., 2018), gold-evidence export `copenlu/fever_gold_evidence`, licensed
  CC BY-SA 3.0. Text is Wikipedia's.
- **Not used:** VitaminC, although it would add contrastive revisions. It carries a BIG-bench canary that asks
  for it to be kept out of training corpora.

## Integrity

- None of these pairs is uploaded to the Telegraph evaluation pool. Pool rows may be drawn into the hidden
  testing half, and training on them would grade the adapter on answers it has already seen.
- The pairs contain no miner's answers and no miner's format. Our own miner also serves FACT_CHECK, so this is
  disclosed.
