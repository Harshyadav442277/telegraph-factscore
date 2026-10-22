"""Build LLM_OUTPUT_EVALUATION preference pairs from FEVER gold evidence.

The question is a model's statement plus the source it cites. The miner's answer is an evaluation:
is the statement supported by that source, and where does it depart from it. The task is reading
two texts against each other, so it needs no world knowledge: everything the evaluator must check
is in the prompt.

A statement is faithful (a FEVER SUPPORTS claim) or contradicted. It is contradicted when it is a
FEVER REFUTES claim, or when a SUPPORTS claim has had its number or date edited away from the
source. The chosen evaluation gets this right. The rejected one, which uses the same template, is
wrong in one way:

  flip       the opposite verdict, with a generic reason
  fabricate  the opposite verdict, with a specific reason that misreads the source
             (an invented discrepancy, or a hallucinated value "matches")
  misquote   the right verdict, with the discrepancy reported back to front or with the wrong value

FEVER (Thorne et al., 2018), CC BY-SA 3.0, via copenlu/fever_gold_evidence.
Pages are split between train, val and test as in ../fact_check.

    python -I build_eval_pairs.py --fever <train.jsonl> --out <dir> --train 6000 --val 800 --test 800
"""
import argparse
import json
import random
import sys
from collections import Counter, defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "fact_check"))
import build_pairs as fc  # noqa: E402
import fever_text as ft  # noqa: E402

KIND_WEIGHTS = {"flip": 0.40, "fabricate": 0.30, "misquote": 0.30}
EDIT_SHARE = 0.25  # share of SUPPORTS rows with a number whose statement is edited into a hallucination

PROMPTS = [
    "Here is a model answer and its cited source. Rate it for factual accuracy, citation support and "
    "hallucination.\n\nModel answer: {a}\nCited source ({page}): {ev}",
    'Evaluate this LLM output for factual fidelity against the reference.\nOutput: "{a}"\nReference: {ev} '
    "[Wikipedia: {page}]",
    "Does the source support the model's statement? Flag any hallucination.\nStatement: {a}\nSource: {ev}",
    "Judge this generated answer. Is every fact in it backed by the citation?\nAnswer: {a}\n"
    "Citation (Wikipedia, {page}): {ev}",
    "Grade the following model response for hallucinations, using the provided evidence.\nResponse: {a}\n"
    "Evidence: {ev}",
    "LLM output to check: {a}\nSource document ({page}): {ev}\nIs the output faithful to the source?",
]

# (supported, unsupported, body). Both evaluations in a pair use the same entry.
EVALS = [
    ("Verdict: SUPPORTED.", "Verdict: NOT SUPPORTED.", "{v} {reason} Score: {score}/10."),
    ("Faithful.", "Hallucination.", "{v} {reason} Factual accuracy: {score}/10."),
    ("true", "false", '{{"supported": {v}, "score": {score}, "reason": "{reason}"}}'),
    ("The output is supported by its citation.", "The output is not supported by its citation.",
     "{v} {reason} Rating: {score}/10."),
    ("No hallucination found.", "Hallucination detected.", "{v} {reason} Score {score} of 10."),
    ("Pass.", "Fail.", "{v} {reason} ({score}/10)"),
]
# Every reason names the statement it judges ({q}): a generic "the source says so" repeats across
# thousands of pairs and the model memorises it instead of reading.
GENERIC_OK = ["The source confirms that {q}.", "The citation states that {q}.",
              "The claim that {q} is backed by the source."]
GENERIC_BAD = ["The source does not say that {q}.", "The citation does not support the claim that {q}.",
               "Nothing in the source backs the claim that {q}."]


def ok_reason(rng, value, q):
    return rng.choice(["The answer's {a} matches the source, which confirms that {q}.",
                       "The source also gives {a}: it confirms that {q}."]).format(a=value, q=q)


def bad_reason(rng, said, source, q):
    return rng.choice(["The answer says {a}, but the source says {s}, so it is wrong that {q}.",
                       "The source gives {s}, not {a}; it is not true that {q}."]).format(a=said, s=source, q=q)


def as_clause(stmt):
    first = stmt.split(" ", 1)[0]
    s = (first.lower() + stmt[len(first):]) if first in fc.LOWERABLE else stmt
    return s.rstrip(". ")


def render_eval(tpl, supported, reason, rng):
    t_word, f_word, body = tpl
    score = rng.choice((8, 9, 10)) if supported else rng.choice((1, 2, 3))
    v = t_word if supported else f_word
    if body.startswith("{{"):
        reason = reason.replace('"', "'")
    return body.format(v=v, reason=reason, score=score)


def statement(row, rng):
    """(statement, faithful, detail) where detail is the value the statement and the source share
    (faithful) or (statement's value, source's value) (contradicted), or None when not known."""
    fact = row["fact"]
    if row["label"] == "REFUTES":
        return row["claim"], False, (fact[1], fact[0]) if fact else None
    if fact and rng.random() < EDIT_SHARE:
        shown, _, fake = fact
        return row["claim"].replace(shown, fake, 1), False, (fake, shown)
    return row["claim"], True, fact[0] if fact else None


def make_pair(row, rng):
    stmt, faithful, detail = statement(row, rng)
    tpl = rng.choice(EVALS)
    page = ft.page_title(row["page"])
    prompt = rng.choice(PROMPTS).format(a=stmt, ev=row["ev"], page=page)
    can_edit = detail is not None and (not faithful or row["fact"])
    kind = rng.choices(list(KIND_WEIGHTS), weights=list(KIND_WEIGHTS.values()))[0] if can_edit else "flip"
    q = as_clause(stmt)
    if kind == "flip":
        good = rng.choice(GENERIC_OK if faithful else GENERIC_BAD).format(q=q)
        bad = rng.choice(GENERIC_BAD if faithful else GENERIC_OK).format(q=q)
        chosen = render_eval(tpl, faithful, good, rng)
        rejected = render_eval(tpl, not faithful, bad, rng)
    elif faithful:
        fake = row["fact"][2]
        chosen = render_eval(tpl, True, ok_reason(rng, detail, q), rng)
        if kind == "fabricate":  # invents a discrepancy the source does not have
            rejected = render_eval(tpl, False, bad_reason(rng, detail, fake, q), rng)
        else:  # right verdict, but "matches" a value that appears nowhere in either text
            rejected = render_eval(tpl, True, ok_reason(rng, fake, q), rng)
    else:
        said, source = detail
        chosen = render_eval(tpl, False, bad_reason(rng, said, source, q), rng)
        if kind == "fabricate":  # misses the hallucination and calls the wrong value a match
            rejected = render_eval(tpl, True, ok_reason(rng, said, q), rng)
        else:  # right verdict, discrepancy reported back to front
            rejected = render_eval(tpl, False, bad_reason(rng, source, said, q), rng)
    return {"prompt": prompt, "chosen": chosen, "rejected": rejected, "label_kind": "preference_pair",
            "provenance": {"source_kind": "synthetic", "source_url": "https://fever.ai/dataset/fever.html"},
            "meta": {"kind": kind, "faithful": faithful, "label": row["label"], "page": row["page"],
                     "fever_id": row["id"]}}


def build_split(rows, n, rng, per_page):
    taken, picked = Counter(), {"SUPPORTS": [], "REFUTES": []}
    rng.shuffle(rows)
    rows.sort(key=lambda r: r["fact"] is None)  # stable: rows with a checkable detail first
    for r in rows:
        key = (r["page"], r["label"])
        if taken[key] < per_page:
            taken[key] += 1
            picked[r["label"]].append(r)
    # SUPPORTS rows split into faithful and edited statements, so take more of them than REFUTES
    # to land near half faithful overall (rows with a number come first, so nearly all can be edited).
    n_sup = int(n * 0.5 / (1 - EDIT_SHARE))
    out = [make_pair(r, rng) for r in picked["SUPPORTS"][:n_sup] + picked["REFUTES"][:n - n_sup]]
    rng.shuffle(out)
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--fever", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--train", type=int, default=6000)
    ap.add_argument("--val", type=int, default=800)
    ap.add_argument("--test", type=int, default=800)
    ap.add_argument("--per-page", type=int, default=4, help="claims per article and label")
    ap.add_argument("--seed", type=int, default=11)
    a = ap.parse_args()
    rng = random.Random(a.seed)
    rows = fc.load_rows(a.fever, rng)
    splits = defaultdict(list)
    for r in rows:
        splits[fc.split_of(r["page"])].append(r)
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    stats = {"source_rows": len(rows)}
    for name, n in (("train", a.train), ("val", a.val), ("test", a.test)):
        pairs = build_split(splits[name], n, rng, a.per_page)
        with open(out / f"{name}.jsonl", "w", encoding="utf-8", newline="\n") as f:
            for p in pairs:
                f.write(json.dumps(p, ensure_ascii=False) + "\n")
        stats[name] = {"pairs": len(pairs), "pages": len({p["meta"]["page"] for p in pairs}),
                       "kinds": Counter(p["meta"]["kind"] for p in pairs),
                       "faithful": Counter(p["meta"]["faithful"] for p in pairs)}
    (out / "stats.json").write_text(json.dumps(stats, indent=1, default=str), encoding="utf-8")
    print(json.dumps(stats, indent=1, default=str))


if __name__ == "__main__":
    main()
