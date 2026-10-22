"""FEVER text clean-up and the edits that turn a true statement into a false one.

FEVER sentences are PTB-tokenised Wikipedia text ("Jackie -LRB-2016 film-RRB- is a film ."),
so every string is detokenised before it reaches an answer. The edits work on that clean text.
"""
import difflib
import re

_PTB = [("-LRB-", "("), ("-RRB-", ")"), ("-LSB-", "["), ("-RSB-", "]"),
        ("-LCB-", "{"), ("-RCB-", "}"), ("-COLON-", ":"), ("``", '"'), ("''", '"'), ("--", "-")]
# A bracketed pronunciation, "[ˈɡæbəˌreɪ]", with the "; " that follows it inside "( ... )".
_IPA = re.compile(r"\[[^\]]*[ˈˌəɪɛæʃʒθðŋɔʊʌɑɒː][^\]]*\]\s*;?\s*")
_SPACE_BEFORE = re.compile(r" ([,.;:!?%)\]}]|'s\b|n't\b|'re\b|'ve\b|'ll\b|'d\b|'m\b)")
_SPACE_AFTER = re.compile(r"([(\[{$]) ")
_QUOTED = re.compile(r'" ([^"]*?) "')
_WORD = re.compile(r"[\w'’-]+|[^\w\s]", re.UNICODE)
_NUMBER = re.compile(r"(?<![\w.])(\d{1,3}(?:,\d{3})+|\d+)(?:\.(\d+))?(?![\w])")
_YEAR_LO, _YEAR_HI = 1000, 2026
_MONTH_BEFORE = re.compile(r"\b(?:January|February|March|April|May|June|July|August|September|"
                           r"October|November|December) $")
_STOPWORDS = {"the", "a", "an", "of", "in", "on", "at", "to", "for", "by", "and", "or", "is", "was",
              "are", "were", "be", "been", "as", "with", "from", "that", "it", "its", "his", "her"}


def detok(text: str) -> str:
    text = text.split("\t")[0]
    for a, b in _PTB:
        text = text.replace(a, b)
    text = _SPACE_BEFORE.sub(r"\1", text)
    text = _SPACE_AFTER.sub(r"\1", text)
    text = _QUOTED.sub(r'"\1"', text)
    text = _IPA.sub("", text).replace("()", "").replace("( ", "(")
    return re.sub(r"\s+", " ", text).replace(" ,", ",").strip()


def page_title(page: str) -> str:
    return detok(page.replace("_", " "))


def page_url(page: str) -> str:
    return "https://en.wikipedia.org/wiki/" + page_title(page).replace(" ", "_")


def _fmt_like(value: float, original: str, decimals: str | None) -> str:
    """Render `value` the way `original` was written: same decimals, same thousands commas."""
    if decimals:
        return f"{value:.{len(decimals)}f}"
    n = int(round(value))
    return f"{n:,}" if "," in original else str(n)


def perturb_number(token: str, decimals: str | None, rng, day: bool = False) -> str | None:
    """A different, plausible value written in the same style, or None. `day` keeps a day of
    the month a valid day."""
    raw = float(token.replace(",", "") + ("." + decimals if decimals else ""))
    if not decimals and _YEAR_LO <= raw <= _YEAR_HI and "," not in token:
        cand = [int(raw) + d for d in (-12, -7, -4, -3, -2, -1, 1, 2, 3, 5, 9) if int(raw) + d <= _YEAR_HI]
        return str(rng.choice(cand))
    if day:
        cand = [d for d in range(1, 29) if d != int(raw)]
        return str(rng.choice(cand))
    if raw < 10 and not decimals:
        cand = [v for v in (raw - 3, raw - 2, raw - 1, raw + 1, raw + 2, raw + 4) if v >= 0 and v != raw]
        return str(int(rng.choice(cand)))
    if raw < 100 and not decimals:
        return str(int(raw) + rng.choice((-9, -6, -4, -3, -2, 2, 3, 5, 7, 11)))
    factor = rng.choice((0.55, 0.7, 0.8, 0.88, 1.12, 1.25, 1.4, 1.75))
    out = _fmt_like(raw * factor, token, decimals)
    return None if out == token + ("." + decimals if decimals else "") else out


def number_spans(text: str):
    """(start, end, token, decimals, is_day) for every number in `text`."""
    return [(m.start(), m.end(), m.group(1), m.group(2), bool(_MONTH_BEFORE.search(text[:m.start()])))
            for m in _NUMBER.finditer(text)]


def _shape(tok: str) -> str:
    if tok[:1].isdigit():
        return "num"
    if tok[:1].isupper():
        return "cap"
    return "low"


def substitutions(claim: str, evidence: str):
    """Spans where the claim and the evidence say different things in the same context.

    Returns [(evidence_text, claim_text)] for word substitutions of the same shape (number for
    number, Name for Name). The span must follow a real anchor (two identical words, or one that
    is not a stopword) and be followed by identical words or end the claim.
    "directed by Peter Jackson" vs "directed by Pablo Larrain" gives ("Pablo Larrain",
    "Peter Jackson")."""
    c, e = _WORD.findall(claim), _WORD.findall(evidence)
    sm = difflib.SequenceMatcher(a=[t.lower() for t in c], b=[t.lower() for t in e], autojunk=False)
    out = []
    ops = sm.get_opcodes()
    for i, (tag, a0, a1, b0, b1) in enumerate(ops):
        if tag != "replace" or i == 0 or ops[i - 1][0] != "equal":
            continue
        anchor = c[ops[i - 1][1]:ops[i - 1][2]]
        if len(anchor) < 2 and anchor[0].lower() in _STOPWORDS:
            continue
        tail = c[a1:]
        if not (all(not t[:1].isalnum() for t in tail) or (i + 1 < len(ops) and ops[i + 1][0] == "equal")):
            continue
        k = a1 - a0
        if k > 4 or b1 - b0 < k:
            continue
        cs, es = c[a0:a1], e[b0:b0 + k]
        if [_shape(t) for t in cs] != [_shape(t) for t in es] or "low" in [_shape(t) for t in cs]:
            continue
        ev_text, cl_text = " ".join(es), " ".join(cs)
        if ev_text in evidence and cl_text in claim and ev_text.lower() != cl_text.lower():
            out.append((ev_text, cl_text))
    return out
