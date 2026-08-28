# tune.md — every tunable constant

All constants live in one block in [`src/profile.rs`](src/profile.rs), by design: they are meant
to be **swept**, not guessed, and a reviewer should see the whole decision surface at once.
`base()` holds the defaults; each per-intent `profile()` restates only what it changes, so the
diff against `base()` *is* that intent's tuning record.

Sweeps run against `track2/harness/run-eval.mjs`, which reproduces the node's Stage-1 and Stage-2
checks offline. The objective is two-sided and the two sides pull against each other:

- **separation** — `margin = mean(good) − mean(bad)`, must strictly exceed the incumbent's and
  clear 0.15 absolute;
- **agreement** — Spearman rank correlation with the incumbent on real traffic, must be ≥ 0.60,
  *skipped* on intents with fewer than 2 miners.

---

## Salience weights

| Constant | Default | Rationale |
|---|---|---|
| `w_number` | 3.0 | Figures carry Tier-A correctness; they should dominate ordinary words. Mirrors the champion's own numeric weight of 3.0. |
| `w_ident` | 3.4 | Identifiers (IP, CVE id, version, date) are exact-match facts with no tolerance at all, so they weigh slightly above a figure. |
| `w_stop` | 0.05 | Near zero, but deliberately **not** zero: a stopword still enters the precision denominator, so padding an answer with function words dilutes it rather than being free. |
| `w_high` | 0.5 | A non-Latin run we cannot segment. Real content, but we cannot say how much of it. |
| `w_word_base` | 1.0 | Base weight of a content word. |
| `w_len_step` | 0.06 | Longer words carry marginally more information. |
| `w_len_cap` | 12.0 | Cap, so a very long token cannot dominate. |
| `w_proper` | 1.0 | Mid-sentence capitals stand in for proper nouns — places, orgs, tickers — which is exactly what a wrong answer gets wrong. |

## Anti-parrot

| Constant | Default | Rationale |
|---|---|---|
| `echo_discount` | 0.25 (unused) | **Reserved, currently unused.** Measured across 554 real rows, bag-of-words question overlap correlates *negatively* (−0.258) with the champion's score: the parrot effect is positional, not an overlap effect. A general echo penalty would therefore buy nothing and would wreck Spearman agreement. The echo flag survives only as a boolean inside the answered-ness gate. Kept as a constant so a future sweep can re-test the hypothesis cheaply. |

## Answered-ness gate

The mechanism that actually catches a parrot: after striking the boilerplate opener, does the
answer assert anything the question did not already contain?

| Constant | Default | Rationale |
|---|---|---|
| `ans_sat` | 3.0 | Novel supported mass at which the gate is fully open — roughly one figure plus a content word. A *gate*, not a recall term: a little genuine content opens it. |
| `ans_gt_frac` | 0.5 | The saturation point scales with the ground truth's own answer-bearing mass, so a thin ground truth cannot demand more than it contains. |
| `ans_sat_min` | 0.9 | Floor on that saturation point, so a one-word ground truth cannot open the gate on noise. |
| `decisive_min` | 0.5 | Minimum weight for a token to count as content at all (excludes stopwords). |
| `novel_prose_w_gt` | 0.0 base / 0.12 storm | **New (review C5).** Prose novelty weight when the ground truth *does* state decisive facts of its own. Near zero, because prose agreement is not assertion: a ground-truth-blind list of the intent's own field names carries no figure, identifier or proper noun, yet 81% of it appeared in a long ground truth and it scored a perfect 1.0 on recorded rows. If the truth states facts, an answer stating none of them has not answered. |
| `novel_prose_w` | 0.35 | How much ordinary prose counts toward *novelty*, versus a hard assertion. Low, because a parrot padded with generic filler otherwise earns real novelty credit whenever the ground truth is long enough to contain the same common words. Measured: a contentless echo reached answered-ness 0.80 before this split, 0.20 after. |
| `ans_floor` | 0.05 | Floor under the gate. A shut gate must not collapse every non-answer onto one identical value — ties are what cost Spearman. |
| `gt_decisive_min` | 0.8 | Below this much answer-bearing mass the **ground truth is itself refusal-shaped**, so nothing can be "unanswered" against it and the gate opens fully. In real traffic the refusals are usually the ground truths, not the answers (8 of 15 weather GTs are hedged; 40 of 58 sub-0.02 rows). We never relitigate the ground truth. |

## Fact agreement

| Constant | Default | Rationale |
|---|---|---|
| `num_rel_k` | 8.0 | Near-miss decay: `agreement = 1/(1 + k·relerr)`. Smooth and bounded, and needs no transcendental. |
| `num_rel_tol` | 0.02 | Inside this relative error two figures are the same claim. |
| `num_abs_tol` | 0.02 | Absolute tolerance for bounded [0,1] quantities (risk scores, fractions), where a relative band is meaningless near zero. |
| `num_band_rel` | 10.0 | For two *unitless* figures, how many multiples apart they may be and still be about the same quantity. Beyond it the answer's figure is **unverifiable rather than wrong** (a year beside a CVSS score) — precision, not recall. Figures carrying units are compared by dimension instead and ignore this. |
| `num_min_bias` | 0.5 | How much the **worst** figure decides the numeric channel versus the average one. An answer with four right figures and one wrong decisive figure is a wrong answer; a plain mean lets the wrong one hide. This is the FACT-SWAP lever. |
| `num_channel_w` | 0.9 | How far a wrong numeric channel may pull the fact term down. 1.0 lets it zero the term; 0.0 disables the channel. |
| `id_channel_w` | 0.9 | Same, for identifiers. **Now gated by the substitution rule** (clean-pair round, change 3): an unsupported identifier only enters the channel to the extent the ground truth names identifiers the answer never mentions. Before that, `tg.has_ident` alone put every unmatched identifier in the wrong column, so a correct answer that added one true `AS15169` scored 0.4876. |
| `fact_floor` | 0.10 | Floor of the fact multiplier, so a wholly-wrong-figure answer degrades rather than falling off a cliff and near-misses stay distinguishable from garbage. |
| `m_foreign_unit` | 0.05 | **New (review C6).** Multiplier on a figure whose unit we could not identify, when the ground truth named a real one. Calibrated so a category error ("47 bananas") scores no better than an honest wrong value ("47 m/s" against a truth of 47 km/h ≈ 0.046). |
| `m_bare_unit` | 0.85 | **New (review C6).** Multiplier on a *bare* figure matched against a united one. Weaker evidence, but a legitimate shape (`wind_kmh=128.7`), so only a light discount. Asymmetric: applied only when the **answer** is the side missing the unit, so `42%` against a bare `0.42` is not punished for being explicit. |
| `ent_min_bias` | 0.6 | **New (pre-flight ENTITY-SWAP defect).** How much the *worst* entity decides the entity channel rather than the average one. Mirrors `num_min_bias`: swapping one city out of six correct entities must not average away. |
| `ent_channel_w` | 0.9 | **New.** How far a wrong entity may pull the score down. Paired with the substitution rule, which only counts an unsupported entity against the answer to the extent the ground truth names entities the answer never mentions — so extra true detail stays neutral. |
| `m_contra` | 0.85 | **New (review C2).** How hard a polarity flip on supported content is punished. Not 1.0, so a partial contradiction degrades rather than zeroing. |
| `m_range_width` | 2.0 | Discount on a hyphenated range for the width it adds **over the ground truth's own** (clean-pair round, change 4; it was absolute width). A range containing the truth is right; a range wide enough to contain any outcome is a hedge, and `5-50 m/s` must not bank the credit of `46-48 m/s`. Charging absolute width also punished an answer for restating a hyphenated figure the truth itself states — a postal code `162-0843` parses as the range 162..843, and quoting it back scored the numeric channel 0.382. |
| `m_foreign_unit` (applicability) | — | A foreign unit is now only recorded when the word **abuts** its figure (clean-pair round, change 5). Punctuation between them means a new clause: every enumerated list used to poison its own markers, so `2. tools` disagreed with a truth reading `2. Using tools` on a dimension neither side has. |

Channels combine **multiplicatively**, not by averaging: quoting the right CVE id must not rescue
a wrong CVSS score.

## Prose versus assertion

| Constant | Default | Rationale |
|---|---|---|
| `prose_w` | **0.02** (was 0.25) | Share of precision carried by ordinary prose rather than by decisive assertions. ARCHITECTURE A3.4 makes fact agreement dominant and lexical overlap "only a low-weight tie-breaker for prose quality" — this is that weight. **Lowered in the clean-pair round** (see below); 0.25 charged a *correct* answer 12 points for wording the truth differently. Not literally zero, so filler still dilutes an answer slightly. STORM_ALERT overrides it back to 0.7. |

## Calibration

| Constant | Default | Rationale |
|---|---|---|
| `p_concave` | 0.5 | Blend between linear precision and concave `p·(2−p)`. Pulls a mostly-right answer up without flattening the middle. 0 = linear. |
| `ss_lo` / `ss_hi` | 0.02 / 0.92 | Smoothstep knots on the raw composite. **The primary lever on `score_stddev`** (gate needs > 0.05) and the main trade between margin and Spearman. Knots short of 0 and 1 saturate the ends, which maximises margin; knots at the full range preserve ordering, which protects Spearman. |

---

## The clean-pair round — what the node's rejection of registration 1377 bought

Registration 1377 (IP_GEOLOCATION) was rejected and returned the calibration this corpus could
never produce offline (GAPS G11):

```
candidate_margin 0.87751794     champion_margin 0.99185944
candidate_wins   14 / 15        champion_wins   15 / 15
worst_self_match 1.0  PASS      score_stddev    0.4654  PASS
historical_rows_evaluated 0  →  Spearman skipped
reason: "lost to the current champion on ordering ... Score correct answers above wrong ones
         more consistently."
```

**The node's fixtures are clean good-vs-bad pairs, not adversarial ones.** Every class in this
corpus was adversarial — parrots, entity swaps, refusals — i.e. exactly where a lexical incumbent
fails, which is why the proxy put the champion at margin 0.438 where the node measured 0.992. On a
clean pair the incumbent is lexically generous and gives any recognisably-correct answer ~1.0.

So the loss was never "wrong answers score too high." It was **correct answers scoring too low for
their wording**: verbatim-correct 1.0000, the identical facts reworded 0.8785. Five separate
mechanisms were charging a correct answer for prose or detail the ground truth does not restate.
Four of them were invisible to every existing test, because a *verbatim* answer short-circuits on
exact match and never reaches the code — only a rewording does, and until the CLEAN-PAIR class
existed nothing in the corpus rewrote a correct answer.

None of these changes touches the anti-gaming machinery, which lives in different channels:
parroting is caught by the answered-ness gate (novel **supported** mass), wrong facts by the
multiplicative fact/entity term, contradictions by the polarity term. Measured after: every wrong
answer in the bar set scored **the same or lower**.

| # | Change | Where | Before → after |
|---|---|---|---|
| 1 | `prose_w` 0.25 → **0.02** in `base()` | `profile.rs` | reworded 0.8785 → 0.9992; verbose 0.8505 → 0.9994; JSON 0.8742 → 0.9991 |
| 2 | An unsupported **non-numeric** assertion abstains in precision when the answer already covers every entity the truth names — the substitution-vs-addition rule the entity channel already used (A3.8). A swap always leaves ground-truth mass uncovered, so it is still charged. | `score.rs precision_of`, new shared `gt_uncovered_mass` | extra true detail 0.8643 → 0.9999; a verbose answer whose only unsupported token was "IP" 0.9066 precision → 1.0 |
| 3 | The **identifier channel** gets the same pairing rule. It had only `tg.has_ident`, so any unmatched identifier counted as wrong. | `facts.rs fact_multiplier` | correct answer + one extra true `AS15169` 0.4876 → 1.0000; a *swapped* IP still 0.0 |
| 4 | The hedge discount on a range is charged on **excess** width over the ground truth's, not absolute width. | `facts.rs value_agreement` | quoting the truth's own postal code `162-0843` (which parses as the range 162..843) 0.4632 → 1.0; `5-90 km/h` vs a truth of `47 km/h` still a hedge |
| 5 | A foreign unit must **abut** its figure. Punctuation between them means the word starts a new clause. | `units.rs annotate_units` | `2. tools` against a truth of `2. Using tools` — numeric channel 0.145 → 1.0; `47 bananas` still 0.0005 |
| 6 | `Toks::abbrev` narrowed from *any* ALL-CAPS token to **two letters only**. | `tokens.rs` | closes a pre-existing hole: a wrong ISP written `AWS` scored **0.9829** where the same swap spelled `Cloudflare Inc.` scored 0.2248; now 0.3005. Two-letter geographic codes (`UY` for Uruguay, unreachable by any lexical rule) still abstain |

Changes 4, 5 and 6 are **defects, not calibration** — 4 and 5 punished an answer for restating the
ground truth's own figure, and 6 let a wrong organisation go free. They were found by the new
CLEAN-PAIR class and are pinned by five new tests in `score.rs`.

One thing was tried and **reverted**: making `polarity_of` scan for any ground-truth occurrence
with matching polarity, rather than trusting the single hash-set index. It is a real weakness
(the index is whichever occurrence the set happened to store), but the scan cannot see
acronym- or stem-matched tokens, so it read `US` against `United States` as a contradiction and
dropped a correct rewording from 0.9992 to 0.9441. Recorded here rather than shipped.

## The audit round — five semantic failure classes, and a repaired benchmark

An independent audit (`track2/codex_audit.md`) held registration and named five
locally-reproducible failures plus a defect in the benchmark itself. All five are closed. The
first three are **defects**: each punished a *correct* answer for something that carries no
information about correctness.

| # | Failure | Mechanism | Before → after |
|---|---|---|---|
| 1 | Curly Unicode punctuation | Bytes ≥ 0x80 are opaque *word* bytes, so `Shimo’ochiai` (U+2019) is one token where the ASCII `Shimo'ochiai` is two. `bytes::fold_punct` now folds quotes, dashes, ellipses and exotic spaces to ASCII before tokenising. | ASCII apostrophe against a curly ground truth **0.2592 → 0.9998** |
| 2 | Hemisphere ≠ signed coordinate | Three of the four hemisphere letters name units — `s` seconds, `n` and `e` SI prefixes — so the unit-word table claimed them first and only `W` ever worked. The hemisphere reading now runs **before** the unit table, guarded on a fractional magnitude within ±180 so `30 s` stays a duration. The letter is also marked as notation, not content. | `34.9011 S, 56.1645 W` **0.2055 → 1.0000**; wrong hemisphere still 0.0458; `30 s`, `12.5 m/s`, `47 bananas` unchanged |
| 3 | Country aliases | The initials rule reaches `US` from `United States` but nothing lexical reaches `UY` from `Uruguay`. [`src/aliases.rs`](src/aliases.rs) adds ISO 3166 alpha-2 in both directions. | correct `UY` **0.9696 → 0.9992**; and a **wrong** code, previously free, **0.97 → 0.2796** |
| 4 | Correct paraphrases far below equivalents (CLEAN-PAIR 01/10/11) | Three separate causes: the range-width and list-marker defects above, then the polarity channel comparing against whichever ground-truth occurrence the hash set happened to store. A contradiction now requires that the token occur in the truth and *never* with the answer's polarity; a token matched through an alias has no polarity to compare and abstains. | correct-form spread **28/31 → 31/31 within 0.05** |
| 5 | Appended false facts were free | `add_w`, new. An unsupported assertion that displaced nothing entered no channel at all. | false extra IP **0.9999 → 0.8100**, false ASN **1.0000 → 0.8154**, false country **0.9999 → 0.9912**, false city **0.9999 → 0.9906** |

**Class 5 is a trade, not a fix, and the price is symmetric.** Nothing in the text distinguishes an
appended *true* fact from an appended *false* one — that needs slot-aware extraction the module
does not have. So the same `add_w = 0.35` that costs a false ASN 0.18 costs a **true** one the
same 0.18: extra-true-`AS15169` went 1.0000 → 0.8154 and extra true detail 0.9999 → 0.9492. The
value was swept over {0, 0.2, 0.35, 0.5, 0.7, 1.0}; the CLEAN-PAIR margin is *flat* across all of
them, so the corpus cannot choose it and it is a judgement call, recorded here rather than
presented as measured. Appended false **country** and **city** still cost under a point, because
the entity channel is worst-case-leaning and a pure addition is not a contradiction.

### The benchmark was repaired first, and the honest number is lower

The generator's wrong answers were built by positional proper-noun substitution over the whole
text, which produced corrupted strings ("The Iceland. It address …"). A scorer can reject those on
fluency alone, so **248/248 was not a real result**. `gen-clean-pairs.mjs` now emits **fluent
one-fact counterfactuals**: exactly one typed slot (ip, asn, postal, coordinate, speed, country,
organisation, city) replaced by another fixture's value for the same slot, prose untouched. Each
corpus carries a `corpus_version` hash over the generator and every source ground truth.

| IP_GEOLOCATION CLEAN-PAIR | corrupted corpus | repaired corpus |
|---|---|---|
| pairs | 248 | **744** |
| wins | 248/248 | **744/744** |
| margin | 0.99855 | **0.69848** |
| correct-form spread ≤ 0.05 | 28/31 | **31/31** |
| mean of the wrong answers | 0.0000 | 0.3010 (wrong-city 0.2469, wrong-org 0.2427, wrong-country 0.2973, wrong-coordinate 0.3885, wrong-ip 0.4559, wrong-asn 0.5387) |

The margin fell by 0.30 and that is the point: a fluent near-miss *should* score above unrelated
text, so a corpus whose wrong answers are all near-zero cannot measure ordering. Ordering itself is
now perfect (744/744) on the harder set.

**And the harder set is what finally separates the two scorers.** On the corrupted corpus the
incumbent also scored 248/248 — the class discriminated nothing. On fluent one-fact
counterfactuals the incumbent (reg 630) drops to **454/744, 61.0%**, against our 744/744. Over the
whole IP_GEOLOCATION corpus the gate proxy now reads **786/791 wins against 485/791**, margin
**0.7221 vs 0.2934**. That gap — a lexical scorer cannot tell "Tokyo" from "Mumbai" in an otherwise
identical sentence — is the improvement claim, and it only became visible once the benchmark stopped
handing both scorers broken strings to reject.

**One defect the repaired benchmark caught in this round's own work.** The first version of the ISO
table looked codes up by hash alone, and alpha-2 codes collide with English words — `is` is
Iceland, `it` Italy, `in` India, `no` Norway. Every ground truth containing the verb "is" therefore
had "iceland" in its key set, and a counterfactual swapping Uruguay for Iceland scored a perfect
**1.0000**. The fix is a case test (a code must be written ALL-CAPS, a name capitalised
mid-sentence). Nothing but the fluent counterfactuals would have found it.

### Measured result — CLEAN-PAIR, IP_GEOLOCATION

| | reg-1377 build | this build |
|---|---|---|
| pairwise accuracy | 248/248 | 248/248 |
| **margin** (mean good − mean bad) | 0.95980 | **0.99855** |
| correct-verbatim | 1.0000 | 1.0000 |
| correct-reworded | 0.9564 | **0.9992** |
| correct-terse | 0.9632 | **0.9960** |
| correct-verbose | 0.9196 | **0.9989** |
| wrong-unrelated / wrong-swapped | 0.0000 | 0.0000 |

The bar the node set is **margin > 0.99186 and 15/15**. Those figures are on the *corrupted*
corpus and are superseded by the repaired one above; they are kept because the incumbent's number
on the same corrupted corpus, **0.99210**, is the one point of contact between this harness and the
node's own measurement (0.99186 — a difference of 0.00024).

## IP_GEOLOCATION is no longer Spearman-free, and that is the blocking finding

Every earlier note here calls this intent "single miner, Spearman skipped". **That is stale.**
Public history now carries 25 rows, 13 of them scorable, across **2 miners** (`iplocate` and
`livecert`) and 13 epochs, so check C applies. Registration 1377's
`historical_rows_evaluated: 0` does not prove otherwise — the gate stops at the first failure and
that candidate had already lost the wins check.

Replayed against the live champion (reg 630) on the current public history:

| Build | rho, per row (n=13) | rho, per distinct question (n=12) |
|---|---|---|
| reg 1377 (rejected) | 0.5824 | — |
| **this build** | **0.5934** | **0.6503** |
| floor / target | 0.60 / 0.70 | 0.60 / 0.70 |

This round *improved* rho while also improving correctness, but it does not clear the floor on the
per-row reading, and neither reading reaches the 0.70 target.

**It is not reachable by tuning, and the evidence is in the disagreements themselves.** There are
no ties to break — all 13 scores are distinct on both sides — so the gap is genuine disagreement,
and it is concentrated on four rows where the champion scores a **factually wrong** answer at
~0.99:

| row | ground truth says | answer says | champion | ours |
|---|---|---|---|---|
| 0 | OpenDNS / Cisco, Ashburn VA | "San Jose, California" | 0.9920 | 0.0086 |
| 3 | Google LLC, **United States** | "located in **Mumbai, India**" | 0.9960 | 0.0156 |
| 12 | Google LLC, **Tokyo, Japan** | "located in **Mumbai, India**" | 0.9918 | 0.0855 |
| 11 | Google LLC, **Japan** | "hosted by Google Cloud in **Mumbai, India**" | 0.0111 | 0.1735 |

Raising rho means scoring "Mumbai, India" closer to 0.99 against a ground truth of Tokyo. That is
the whole thesis inverted. The same structural conflict was already recorded for STORM_ALERT; the
fresh history shows it now binds on IP_GEOLOCATION too, and the honest statement is that **the
release gate's rho ≥ 0.70 is not achievable by a scorer that ranks these four rows correctly.**

n = 13 is also small enough that a single rank swap moves rho by ~0.05, and the reading depends on
whether the node samples rows or distinct questions — 0.5934 against 0.6503 on the same build.

### Release-gate status (audit go/no-go for IP_GEOLOCATION)

| Gate | Required | Measured | |
|---|---|---|---|
| unit tests / fmt / clippy | green | 66 pass (61 under STORM), `fmt --check` clean, `clippy -D warnings` clean | PASS |
| WASM structural | 0 imports, `verify.mjs` full | 0 imports, validate + verify pass on all three builds | PASS |
| ENTITY-SWAP | 18/18 | 18/18 | PASS |
| UNIT/FORM | 2/2 | 2/2 | PASS |
| CLEAN-PAIR correct-form spread | ≤ 0.05 on 31/31 | 31/31 | PASS |
| fluent one-fact counterfactuals | every one below every correct phrasing | 744/744 | PASS |
| appended false IP / ASN / country / city / coordinate | not free | 0.8100 / 0.8154 / 0.9912 / 0.9906 / 0.9001 vs a 1.0000 baseline | PASS, weakly |
| fresh public-history rho | ≥ 0.70 preferred, never < 0.60 | **0.5934 (per row), 0.6503 (per question)** | **FAIL** |

Everything the audit asked for is closed except the last line, and the last line is closed only by
agreeing with the incumbent about answers it gets wrong. **No-go on rho alone.**

---

## Per-intent tuning, and the trade behind it

### IP_GEOLOCATION — Spearman **skipped** (single miner)

| Constant | Value | Why it differs |
|---|---|---|
| `prose_w` | 0.02 | Inherited from `base()`, not restated in the profile: the clean-pair finding is general, and only STORM_ALERT (which must agree with a lexical incumbent) overrides it. |
| `ans_sat` | 3.5 | The IP is always echoed from the question, so decisive content is country/city/ISP/coordinates only. Demand real novel mass. |
| `w_ident` | 4.0 | Identifiers are the spine of this intent. |
| `id_channel_w` | 1.0 | Full authority to zero the fact term on a wrong identifier. |
| `ss_hi` / `ss_lo` | **1.0 / 0.0** (was 0.88 / 0.02; the first attempt at this edit silently did not apply, and the shipped build still had 0.88 until pre-flight caught it) | Deliberately *not* pulled below 1.0 to buy margin. At 0.88 the concave shaping mapped every precision at or above 0.800 to a literal 1.0: two decisive facts in ten could be wrong for free, a ground-truth-blind field-name blob reached 1.0 on live rows, and 19 of 75 corpus answers tied at the ceiling (review C4/C5). |
| `p_concave` | **0.15** (was 0.5) | Concave shaping compounded the same saturation, lifting 0.80 to 0.96 before the smoothstep saw it. Closer to linear keeps the top of the range ranking. |
| `novel_prose_w_gt` | 0.0 | Strict here, unlike STORM_ALERT: the C5 blob was demonstrated on this intent's *live* rows at a perfect 1.0, and with Spearman skipped there is no rank-correlation cost to paying for it. |

### STORM_ALERT — Spearman **enforced** (~4 miners) — **currently FAILS check C**

| Constant | Value | Why it differs |
|---|---|---|
| `num_channel_w` | 1.0 | Wind speeds and gusts are the answer; a wrong one should be able to zero the fact term. |
| `num_rel_k` | 10.0 | Tighter near-miss decay for speeds. |
| `num_abs_tol` | 0.02 | Held at the base value. It was 0.05, which on a canonical percentage is five whole points: a 1-point and a 5-point miss both scored perfect, then 5.001 lost 73% of the score (review M4). |
| `ans_gt_frac` | 0.40 | Ground truths here are frequently themselves refusals, so the gate scales down with the GT's own thin content. |
| `ss_lo` / `ss_hi` | 0.0 / 1.0 | Full range, nothing clipped, so every distinct composite keeps a distinct score. |
| `prose_w` | 0.7 | The Spearman tax, and now the **only** profile that overrides the base 0.02. Not pushed to 1.0: that drops the decisive-fact pool out of precision entirely and misranks correct answers on unfamiliar questions. **This is what stops STORM_ALERT clearing a clean-pair gate too** — measured on CLEAN-PAIR, a correct-verbose answer scores 0.8568 here against 0.9989 on the IP_GEOLOCATION profile, and the class margin lands at 0.96001 against the incumbent's 0.97853. So STORM_ALERT now fails on *two* independent grounds — Spearman (0.593 < 0.60) and clean-pair margin — and the two pull in opposite directions: the prose weight that buys rank agreement is the same one that costs separation. Registering it would need the Spearman constraint to go away, not more tuning. |
| `ans_floor` | **0.05** (was 0.75) | The single constant behind review C3. At 0.75 the answered-ness gate was pinned open and the module **paid a miner more for parroting the question than for answering it**. |
| `ans_sat` | 6.0 | Raised with the floor, so novelty is harder to fake. |
| `novel_prose_w_gt` | 0.12 | Not zeroed as on IP_GEOLOCATION. Zeroing flattens the many prose-only recorded answers onto ~0, which destroys the rank information check C sees. This is the measured maximum of rho subject to both anti-gaming constraints still holding. |

**C4 note (IP_GEOLOCATION):** raising `ss_hi` to 1.0 and cutting `p_concave` removed the
saturation *mechanism* — imperfect answers no longer map to a literal 1.0 — but the corpus tie mass
at exactly 1.0 is unchanged at 19/75, because those answers have genuinely perfect precision, and
the review's verbose-vs-3-wrong inversion persists. Both are recorded in the README rather than
tuned around; neither is reachable without slot-aware fact alignment.

**Check C fails, and it is reported rather than tuned away.** Measured on the
harness corpus: rho = **0.5926**, against a floor of 0.60. Every other check
passes, and by wide margins (margin 0.775 vs the incumbent's 0.425, wins 26/29
vs 18/29).

The conflict is structural, not a tuning miss. The incumbent is a lexical scorer
that *rewards* the exact pathologies this fix round removed — it scores a
contentless question echo 0.9933 (ARCHITECTURE:58) and ties a flat contradiction
at 0.9961. Check C asks a candidate to rank real traffic the way that scorer
does. Having removed the parrot, the blob and the negation tie, we necessarily
disagree with it more, and rho fell from 0.632 (pre-fix, parrot-friendly) to
0.593 (post-fix).

A full sweep of the region — 3 x 2 x 3 x 4 = 72 builds over `prose_w`,
`ans_floor`, `ans_sat` and `novel_prose_w_gt` — found **no configuration** that
reaches rho >= 0.60 while keeping both anti-gaming constraints. The instruction
for this case was to minimise the echo score at rho = 0.62 and record the
tradeoff; rho = 0.62 is not reachable at all after the fixes, so the constants
above instead **maximise rho subject to the anti-gaming constraints holding**:

| Metric | Pre-fix (shipped, rejected by review) | Post-fix (this build) |
|---|---|---|
| question echo, synth corpus | 0.7474 | **0.0058** |
| question echo, recorded rows | 0.6414 (beat every recorded answer on 13/13) | **0.0043** (5/13, all at noise level ~0.004) |
| GT-blind field-name blob | — | **0.0029** vs recorded-answer mean 0.0152 |
| incumbent's own echo, synth | 0.0170 | 0.0170 |
| Spearman vs incumbent | 0.632 | 0.593 (**fails 0.60**) |

We were **44x more parrot-friendly than the incumbent**; we are now **2.9x
less**. That is the trade, stated plainly: this build cannot take the
STORM_ALERT slot through the automated gate as it stands. Register
IP_GEOLOCATION, which passes every check, and treat STORM_ALERT as either a
manual-review exhibit or a later attempt if the fixture rotation moves the bar.

## Sweep method

`scratchpad/sweep/sweep2.mjs` drives the loop. It imports the harness's own `corpus.mjs`, so the
Spearman set it optimises is byte-identical to the one the gate check reads — an earlier sweep
against a hand-rolled proxy set reported rho 0.639 where the harness measured 0.538, which is
exactly the kind of error that makes a candidate fail on-chain after passing locally. Champion
scores are cached once (the incumbent binaries are ~24 MB and dominate runtime); only the
candidate is rebuilt per configuration, at roughly 1.5 s per build.

The sweep restores `profile.rs` in a `finally` block, so an interrupted run cannot leave the
crate patched.

### The clean-pair round's method

`prose_w` was swept, not guessed — 0.25 / 0.10 / 0.05 / 0.02 / 0.0 crossed with `p_concave`
0.15 / 0.35, each build re-measured against the full regression-bar set (every wrong-answer bar,
every Stage-1 trap, and the correct-phrasing family) plus the CLEAN-PAIR class. The curve is
monotone in both directions and 0.02 is where the correct phrasings reach ≥0.999 while every wrong
bar is still at or below its previous value; 0.0 buys another 0.0008 on the good answers and makes
filler literally free, which is not worth it.

The other four changes were **not** swept, because they are not calibration. Each was localised by
ablation — mutating one word or one figure of a correct answer and watching the channel that moved
— and each is pinned by a test rather than a constant. The generator for the class lives beside the
fixtures at [`../fixtures/synth/gen-clean-pairs.mjs`](../fixtures/synth/gen-clean-pairs.mjs); it
only applies mechanical transforms to text that already exists, so no answer in it was authored
against a ground truth (FIXTURES.md's honesty rule).

**The class is a good proxy, and there is one number that says so.** Measured on CLEAN-PAIR, the
IP_GEOLOCATION incumbent (reg 630) scores margin **0.99210**. The node, on its own hidden
fixtures, reported that same incumbent at **0.99186** — a difference of 0.00024. Nothing else in
this corpus predicted the incumbent within 2x. That is a single point of agreement, not a proof
the fixtures match, but it is the first time an offline number has lined up with the node's, and
it is why this class is now the one the tuning loop optimises.
