"""Build FACT_CHECK preference pairs from FEVER gold evidence.

Each pair is one claim and two answers that share a template, so wording carries no signal.
The chosen answer gives FEVER's verdict and quotes the gold Wikipedia sentence. The rejected
answer is wrong in exactly one way:

  flip       the opposite verdict over the same quote (the verdict contradicts its own support)
  fabricate  the opposite verdict over a quote edited to agree with it (coherent and false)
  misquote   the right verdict over an edited quote (the support contradicts the verdict)
  offtopic   the right verdict over a quote from an unrelated article

FEVER (Thorne et al., 2018) is CC BY-SA 3.0; the gold-evidence export is copenlu/fever_gold_evidence.
Pages are split between train, val and test, so no article is seen in more than one split.

    python -I build_pairs.py --fever <train.jsonl> --out <dir> --train 6000 --val 800 --test 800
"""
import argparse
import hashlib
import json
import random
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import fever_text as ft  # noqa: E402

MAX_EVIDENCE_CHARS = 320
LOWERABLE = {"The", "A", "An", "There", "In", "On", "At", "It", "This", "These", "Some", "Many", "Most", "All"}
KIND_WEIGHTS = {"flip": 0.30, "fabricate": 0.32, "misquote": 0.28, "offtopic": 0.10}

PROMPTS = [
    "Fact-check this claim: {claim}",
    "Is it true that {claim_lc}?",
    'Verify: "{claim}"',
    "True or false? {claim}",
    "Check whether this statement is accurate: {claim}",
    "Claim: {claim}\nIs this correct?",
    "I read that {claim_lc}. Is that right?",
    "Can you fact-check the following? {claim}",
]

# (true verdict, false verdict, body). Both answers in a pair use the same entry.
ANSWERS = [
    ("Verdict: TRUE.", "Verdict: FALSE.", "{v} {reason}{ev} (Source: Wikipedia, \"{page}\".)"),
    ("True.", "False.", "{v} {reason}According to Wikipedia's article on {page}: {ev}"),
    ("The claim is accurate.", "The claim is inaccurate.", "{v} {reason}{ev} Source: Wikipedia - {page}."),
    ("Supported.", "Refuted.", "{v} {reason}{ev} [Wikipedia: {page}]"),
    ("true", "false", '{{"verdict": "{v}", "explanation": "{reason}{ev_json}", "source": "{url}"}}'),
    ("This is correct.", "This is incorrect.", "{v} {reason}{ev} This comes from the Wikipedia entry for {page}."),
    ("Rating: True", "Rating: False", "{v}\n{reason}Evidence: {ev}\nSource: {url}"),
    ("Yes, that is right.", "No, that is not right.", "{v} {reason}{ev} (per Wikipedia, {page})"),
]


def claim_fact(label, claim, evidence, rng):
    """(true value, claim's value, false value) for one checkable detail, or None.

    SUPPORTS: a number or date the claim and the evidence share; the false value is an edit of
    it. Names are not swapped: a random name in a random slot reads as nonsense, which teaches
    the model to spot odd text rather than wrong facts.
    REFUTES: the evidence's value where the claim substitutes its own, and nothing else,
    because a shared detail in a refuted claim is not the detail that makes it false."""
    if label == "REFUTES":
        subs = ft.substitutions(claim, evidence)
        if not subs:
            return None
        true_v, claim_v = subs[0]
        return true_v, claim_v, claim_v
    for start, end, tok, dec, is_day in ft.number_spans(evidence):
        shown = tok + ("." + dec if dec else "")
        if re.search(r"(?<![\w.,])" + re.escape(shown) + r"(?![\w])", claim):
            fake = ft.perturb_number(tok, dec, rng, day=is_day)
            if fake and fake not in evidence and fake not in claim:
                return shown, shown, fake
    return None


def reason_text(verdict, said, claim_v):
    if verdict:
        return f"This matches the claim's {claim_v}. "
    return f"The source says {said}, not {claim_v}. "


def render_answer(tpl, verdict, ev, page, reason):
    t_word, f_word, body = tpl
    v = t_word if verdict else f_word
    if body.startswith("{{"):
        reason = reason.replace('"', "'")
    return body.format(v=v, reason=reason, ev=ev, ev_json=ev.replace('"', "'"),
                       page=ft.page_title(page), url=ft.page_url(page))


def make_pair(row, kind, rng, other):
    claim, ev, page, label = row["claim"], row["ev"], row["page"], row["label"] == "SUPPORTS"
    fact = row["fact"]
    tpl = rng.choice(ANSWERS)
    first = claim.split(" ", 1)[0]
    claim_lc = (first.lower() + claim[len(first):]) if first in LOWERABLE else claim
    prompt = rng.choice(PROMPTS).format(claim=claim, claim_lc=claim_lc.rstrip(". "))
    with_reason = kind in ("fabricate", "misquote") and rng.random() < 0.6
    if fact:
        true_v, claim_v, false_v = fact
        ev_false = ev.replace(true_v, false_v, 1)
    good_reason = reason_text(label, true_v, claim_v) if with_reason else ""
    chosen = render_answer(tpl, label, ev, page, good_reason)
    if kind == "flip":
        rejected = render_answer(tpl, not label, ev, page, "")
    elif kind == "fabricate":
        said = false_v if label else claim_v
        rejected = render_answer(tpl, not label, ev_false, page,
                                 reason_text(not label, said, claim_v) if with_reason else "")
    elif kind == "misquote":
        rejected = render_answer(tpl, label, ev_false, page, good_reason)
    else:
        rejected = render_answer(tpl, label, other["ev"], other["page"], "")
    return {"prompt": prompt, "chosen": chosen, "rejected": rejected, "label_kind": "preference_pair",
            "provenance": {"source_kind": "synthetic", "source_url": "https://fever.ai/dataset/fever.html"},
            "meta": {"kind": kind, "label": row["label"], "page": page, "fever_id": row["id"]}}


def load_rows(path, rng):
    rows, seen = [], set()
    for line in Path(path).read_text(encoding="utf-8").splitlines():
        r = json.loads(line)
        if r["label"] == "NOT ENOUGH INFO" or len(r["evidence"]) != 1:
            continue
        claim = ft.detok(r["claim"])
        ev = ft.detok(r["evidence"][0][2])
        if claim in seen or not (20 <= len(ev) <= MAX_EVIDENCE_CHARS) or len(claim) > 200:
            continue
        seen.add(claim)
        rows.append({"id": r["id"], "label": r["label"], "claim": claim, "ev": ev, "page": r["evidence"][0][0]})
    for r in rows:
        r["fact"] = claim_fact(r["label"], r["claim"], r["ev"], rng)
    return rows


def split_of(page):
    h = int(hashlib.sha1(page.encode()).hexdigest(), 16) % 100
    return "train" if h < 80 else "val" if h < 90 else "test"


def pick_kind(row, rng):
    kinds = list(KIND_WEIGHTS) if row["fact"] else ["flip", "offtopic"]
    return rng.choices(kinds, weights=[KIND_WEIGHTS[k] for k in kinds])[0]


def build_split(rows, n, rng, per_page):
    by_label = {"SUPPORTS": [], "REFUTES": []}
    taken = Counter()
    rng.shuffle(rows)
    rows.sort(key=lambda r: r["fact"] is None)  # stable: rows with an editable detail first
    for r in rows:
        key = (r["page"], r["label"])
        if taken[key] < per_page:
            taken[key] += 1
            by_label[r["label"]].append(r)
    half = n // 2
    chosen_rows = by_label["SUPPORTS"][:half] + by_label["REFUTES"][:n - half]
    out = []
    for r in chosen_rows:
        kind = pick_kind(r, rng)
        other = None
        if kind == "offtopic":
            target = len(r["ev"])
            pool = [o for o in rng.sample(rows, min(200, len(rows)))
                    if o["page"] != r["page"] and abs(len(o["ev"]) - target) <= 0.15 * target]
            if not pool:
                kind = "flip"
            else:
                other = pool[0]
        out.append(make_pair(r, kind, rng, other))
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
    ap.add_argument("--seed", type=int, default=7)
    a = ap.parse_args()
    rng = random.Random(a.seed)
    rows = load_rows(a.fever, rng)
    splits = defaultdict(list)
    for r in rows:
        splits[split_of(r["page"])].append(r)
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    stats = {"source_rows": len(rows), "with_fact": sum(1 for r in rows if r["fact"])}
    for name, n in (("train", a.train), ("val", a.val), ("test", a.test)):
        pairs = build_split(splits[name], n, rng, a.per_page)
        with open(out / f"{name}.jsonl", "w", encoding="utf-8", newline="\n") as f:
            for p in pairs:
                f.write(json.dumps(p, ensure_ascii=False) + "\n")
        stats[name] = {"pairs": len(pairs), "pages": len({p["meta"]["page"] for p in pairs}),
                       "kinds": Counter(p["meta"]["kind"] for p in pairs),
                       "labels": Counter(p["meta"]["label"] for p in pairs)}
    (out / "stats.json").write_text(json.dumps(stats, indent=1), encoding="utf-8")
    print(json.dumps(stats, indent=1))


if __name__ == "__main__":
    main()
