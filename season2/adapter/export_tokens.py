"""Tokenise held-out pairs the way the node does and score them with the float model.

Writes the token ids for `verify_wide` (which scores them with the packaged .wasm in the node's
own runtime) next to the float model's scores, so the two can be compared pair by pair.
Run from the kit root with the kit's venv:

    python <this> --pairs <test.jsonl> --adapter <runs/INTENT/adapter> --n 24 --out <tokens.json>
"""
import argparse
import json
import random
import sys

sys.path.insert(0, ".")
from wide_runtime.train_adapter import DEFAULT_BASE, NODE_MAX_LEN, render  # noqa: E402


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--pairs", required=True)
    ap.add_argument("--adapter", required=True)
    ap.add_argument("--n", type=int, default=24)
    ap.add_argument("--seed", type=int, default=3)
    ap.add_argument("--out", required=True)
    a = ap.parse_args()

    import torch
    from peft import PeftModel
    from transformers import AutoModelForSequenceClassification, AutoTokenizer

    rows = [json.loads(line) for line in open(a.pairs, encoding="utf-8")]
    random.Random(a.seed).shuffle(rows)
    rows = rows[:a.n]
    tok = AutoTokenizer.from_pretrained(DEFAULT_BASE)
    base = AutoModelForSequenceClassification.from_pretrained(DEFAULT_BASE, num_labels=1, attn_implementation="eager")
    base.config.pad_token_id = tok.pad_token_id
    model = PeftModel.from_pretrained(base, a.adapter).merge_and_unload().eval()

    def ids_and_score(prompt, answer):
        enc = tok(render(prompt, answer), truncation=True, max_length=NODE_MAX_LEN, return_tensors="pt")
        with torch.no_grad():
            score = model(**enc).logits.squeeze().item()
        return enc["input_ids"][0].tolist(), score

    out = []
    for r in rows:
        c_ids, c_score = ids_and_score(r["prompt"], r["chosen"])
        r_ids, r_score = ids_and_score(r["prompt"], r["rejected"])
        out.append({"kind": r.get("meta", {}).get("kind"), "chosen_ids": c_ids, "rejected_ids": r_ids,
                    "float_chosen": c_score, "float_rejected": r_score})
    with open(a.out, "w", encoding="utf-8") as f:
        json.dump(out, f)
    wins = sum(1 for p in out if p["float_chosen"] > p["float_rejected"])
    print(f"float model: {wins}/{len(out)} pairs ordered correctly; wrote {a.out}")


if __name__ == "__main__":
    main()
