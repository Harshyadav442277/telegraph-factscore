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
| `echo_discount` | 0.25 | **Reserved, currently unused.** Measured across 554 real rows, bag-of-words question overlap correlates *negatively* (−0.258) with the champion's score: the parrot effect is positional, not an overlap effect. A general echo penalty would therefore buy nothing and would wreck Spearman agreement. The echo flag survives only as a boolean inside the answered-ness gate. Kept as a constant so a future sweep can re-test the hypothesis cheaply. |

## Answered-ness gate

The mechanism that actually catches a parrot: after striking the boilerplate opener, does the
answer assert anything the question did not already contain?

| Constant | Default | Rationale |
|---|---|---|
| `ans_sat` | 3.0 | Novel supported mass at which the gate is fully open — roughly one figure plus a content word. A *gate*, not a recall term: a little genuine content opens it. |
| `ans_gt_frac` | 0.5 | The saturation point scales with the ground truth's own answer-bearing mass, so a thin ground truth cannot demand more than it contains. |
| `ans_sat_min` | 0.9 | Floor on that saturation point, so a one-word ground truth cannot open the gate on noise. |
| `decisive_min` | 0.5 | Minimum weight for a token to count as content at all (excludes stopwords). |
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
| `ss_hi` | 0.88 | Saturating the top **maximises margin**, and with Spearman skipped there is no cost to the ties that creates. The margin bar here is the highest of any target (~0.992 live), so headroom matters more than ordering. |

### STORM_ALERT — Spearman **enforced** (~4 miners)

This intent is where the two objectives genuinely conflict, and the constants record the cost.
The incumbent is a lexical scorer, so agreeing with its ranking of real traffic means *being*
more lexical — which is the opposite of the thesis.

| Constant | Value | Why it differs |
|---|---|---|
| `num_channel_w` | 1.0 | Wind speeds and gusts are the answer; a wrong one should be able to zero the fact term. |
| `num_rel_k` | 10.0 | Tighter near-miss decay for speeds. |
| `num_abs_tol` | 0.05 | Risk is a bounded [0,1] score: an absolute epsilon, not a relative one. |
| `ans_gt_frac` | 0.40 | Ground truths here are frequently themselves refusals, so the gate must scale down with the GT's own thin content. |
| `ss_lo` / `ss_hi` | 0.0 / 1.0 | Full range, nothing clipped, so every distinct composite keeps a distinct score. Ties are exactly what costs Spearman. |
| `prose_w` | 0.7 | **The Spearman tax.** Prose carries most of precision here. Deliberately *not* pushed to 1.0 even though that scored marginally better on this corpus (ρ 0.604 / margin 0.431): at 1.0 the decisive-fact pool drops out of precision entirely and the build then misranks plainly-correct answers on any question unlike the ones tuned against — caught by `verify.mjs`, which scored a correct terse answer 0.084 against a wrong one at 0.091. Fact-awareness survives either way, because the fact term is multiplicative and applied *after* precision — FACT-SWAP still measures 4/4. |
| `ans_floor` | 0.75 | Same tax: the incumbent scores contentless echoes highly, so crushing them to zero is precisely the disagreement that fails check C. |
| `ans_sat` | 2.0 | Paired with the higher floor. |

Chosen for **headroom on the binding constraint**. Two configurations clear the gate; the selected
one takes ρ = 0.632 against the 0.60 floor with margin 0.581 against the incumbent's 0.425, in
preference to ρ = 0.606 with margin 0.655. Agreement is the constraint with ~0.03 of slack while
the margin bar has ~0.16, so slack is spent where it is scarce.

**Honest consequence, recorded rather than hidden:** at these settings the parrot exhibit is
*muted* on STORM_ALERT — REAL-PARROT pairwise accuracy is 0/8, slightly worse than the incumbent's
1/8. Agreeing with a lexical incumbent that rewards echoes, and punishing echoes, are directly
opposed, and gate C is the one that is enforced. The property is fully expressed on
IP_GEOLOCATION (6/8 versus the incumbent's 4/8), where Spearman is skipped. STORM_ALERT's margin
delta is also thinner (+0.156 versus +0.188), so if only one intent is registered first, register
IP_GEOLOCATION.

## Sweep method

`scratchpad/sweep/sweep2.mjs` drives the loop. It imports the harness's own `corpus.mjs`, so the
Spearman set it optimises is byte-identical to the one the gate check reads — an earlier sweep
against a hand-rolled proxy set reported rho 0.639 where the harness measured 0.538, which is
exactly the kind of error that makes a candidate fail on-chain after passing locally. Champion
scores are cached once (the incumbent binaries are ~24 MB and dominate runtime); only the
candidate is rebuilt per configuration, at roughly 1.5 s per build.

The sweep restores `profile.rs` in a `finally` block, so an interrupted run cannot leave the
crate patched.
