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
| `id_channel_w` | 0.9 | Same, for identifiers. |
| `fact_floor` | 0.10 | Floor of the fact multiplier, so a wholly-wrong-figure answer degrades rather than falling off a cliff and near-misses stay distinguishable from garbage. |
| `m_foreign_unit` | 0.05 | **New (review C6).** Multiplier on a figure whose unit we could not identify, when the ground truth named a real one. Calibrated so a category error ("47 bananas") scores no better than an honest wrong value ("47 m/s" against a truth of 47 km/h ≈ 0.046). |
| `m_bare_unit` | 0.85 | **New (review C6).** Multiplier on a *bare* figure matched against a united one. Weaker evidence, but a legitimate shape (`wind_kmh=128.7`), so only a light discount. Asymmetric: applied only when the **answer** is the side missing the unit, so `42%` against a bare `0.42` is not punished for being explicit. |
| `m_contra` | 0.85 | **New (review C2).** How hard a polarity flip on supported content is punished. Not 1.0, so a partial contradiction degrades rather than zeroing. |
| `m_range_width` | 2.0 | **New.** Discount on a hyphenated range for its own width. A range containing the truth is right; a range wide enough to contain any outcome is a hedge, and `5-50 m/s` must not bank the credit of `46-48 m/s`. |

Channels combine **multiplicatively**, not by averaging: quoting the right CVE id must not rescue
a wrong CVSS score.

## Prose versus assertion

| Constant | Default | Rationale |
|---|---|---|
| `prose_w` | 0.25 | Share of precision carried by ordinary prose rather than by decisive assertions. ARCHITECTURE A3.4 makes fact agreement dominant and lexical overlap "only a low-weight tie-breaker for prose quality" — this is that weight. Keeping it low is what stops a correct-but-wordy answer being diluted below a terse wrong one purely for using more words. |

## Calibration

| Constant | Default | Rationale |
|---|---|---|
| `p_concave` | 0.5 | Blend between linear precision and concave `p·(2−p)`. Pulls a mostly-right answer up without flattening the middle. 0 = linear. |
| `ss_lo` / `ss_hi` | 0.02 / 0.92 | Smoothstep knots on the raw composite. **The primary lever on `score_stddev`** (gate needs > 0.05) and the main trade between margin and Spearman. Knots short of 0 and 1 saturate the ends, which maximises margin; knots at the full range preserve ordering, which protects Spearman. |

---

## Per-intent tuning, and the trade behind it

### IP_GEOLOCATION — Spearman **skipped** (single miner)

| Constant | Value | Why it differs |
|---|---|---|
| `ans_sat` | 3.5 | The IP is always echoed from the question, so decisive content is country/city/ISP/coordinates only. Demand real novel mass. |
| `w_ident` | 4.0 | Identifiers are the spine of this intent. |
| `id_channel_w` | 1.0 | Full authority to zero the fact term on a wrong identifier. |
| `ss_hi` / `ss_lo` | **1.0 / 0.0** (was 0.88 / 0.02) | Deliberately *not* pulled below 1.0 to buy margin. At 0.88 the concave shaping mapped every precision at or above 0.800 to a literal 1.0: two decisive facts in ten could be wrong for free, a ground-truth-blind field-name blob reached 1.0 on live rows, and 19 of 75 corpus answers tied at the ceiling (review C4/C5). |
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
| `prose_w` | 0.7 | The Spearman tax. Not pushed to 1.0: that drops the decisive-fact pool out of precision entirely and misranks correct answers on unfamiliar questions. |
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
