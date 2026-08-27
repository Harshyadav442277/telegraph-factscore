# scorer — a fact-aware Telegraph scoring module

A freestanding `wasm32-unknown-unknown` scoring module, ~13.9 KB, **zero imports**, no allocator,
no clock, no randomness, no transcendental maths. One Rust source tree compiled once per intent
via constant profiles, the same shape the incumbent uses.

**Scoring in one line:** weight each token by how much it decides the answer, measure how much of
what the *answer asserts* the ground truth supports, gate on whether the answer said anything the
question did not already give away, multiply by typed agreement on figures and identifiers, then
calibrate with a smoothstep instead of a step.

---

## Why this is different from the incumbent

The text actually scored is `converted_answer` — a flat third-person summary, 86.9% of which opens
literally `"The data …"`, and a median **2.25× shorter** than the markdown ground truth
(measured over 515 public score records). Any scorer built on symmetric overlap or on
*recall of the truth* is structurally penalised, which is why live medians sit near 0.006.

So this module scores **precision of the answer**: of what the answer asserts, how much does the
ground truth support? Four consequences fall out, and each is a measurable improvement over the
incumbent rather than a stylistic preference:

1. **Typed facts decide.** Figures are compared inside a *dimension* after unit normalisation —
   `18 km/h` and `5 m/s` are the same claim, `55%` and `0.55` are the same claim, and a
   temperature is never a near-miss for a wind speed. Identifiers (IPs, CVE ids, versions, dates,
   coordinates) admit no tolerance at all.
2. **A wrong fact cannot hide.** Channels combine multiplicatively and the numeric channel leans
   on its *worst* figure, so quoting the right CVE id does not rescue a wrong CVSS score.
3. **Unasserted facts are neutral.** A figure the ground truth never discusses is unverifiable,
   not wrong. That is what makes a terse-but-correct answer score like a verbose one.
4. **Answered-ness is first-class.** After the boilerplate opener is struck, an answer that
   asserts nothing beyond the question's own content scores near zero — *when the ground truth
   carries an answer to be found*.

### What the module deliberately does **not** do

- **It does not penalise question-vocabulary overlap.** Measured across 554 real rows, bag-of-words
  overlap with the question correlates *negatively* (−0.258) with the incumbent's score: the parrot
  effect is positional, not an overlap effect. A general echo penalty would buy nothing and would
  wreck the Spearman agreement the gate requires. The echo flag is used only as a boolean inside
  the answered-ness gate.
- **It never relitigates the ground truth.** In real traffic the refusals are usually the *ground
  truths*, not the answers (8 of 15 weather GTs are hedged; 40 of 58 sub-0.02 rows). When the
  ground truth is itself refusal-shaped, a hedged answer is the correct answer and the gate opens
  fully rather than zeroing everything.
- **No embedding in the hot path.** The whole pipeline is integer and `f32` arithmetic —
  `core` has no `powf`/`exp`/`sqrt` and pulling in libm would add host imports. It also means the
  full fixture gate projects to ~10 s of the node's 600 s budget.
- **No miner fingerprints.** No slug, wallet, field name or phrasing is matched, favourably
  or otherwise. The `OUR-STYLE-WRONG` fixture class exists to prove it: a `livecert`-shaped answer
  with wrong facts loses to a competitor-shaped answer with right facts, 1/1 on both intents.

## Pipeline

```
rank_answer(q, gt, ma)
  ├─ ma blank (empty or ASCII whitespace)      -> EXACTLY 0.0     [Stage-1 trap]
  ├─ normalized_equal(gt, ma)                  -> EXACTLY 1.0     [self-match ratchet]
  └─ tokenise -> annotate units -> mark boilerplate / echo / support
       P     precision of assertion   decisive facts, plus prose at prose_w
       ans   answered-ness gate       novel supported mass, conditioned on the GT
       fmul  typed fact agreement     numbers graded, identifiers exact, multiplicative
       raw   = shaped(P) * fmul * ans
       score = smoothstep(ss_lo, ss_hi, raw)
```

Every constant is documented in [tune.md](tune.md) and lives in one block in
[`src/profile.rs`](src/profile.rs), so a reviewer sees the whole decision surface at once.

| File | Purpose |
|---|---|
| `src/lib.rs` | The ABI: `alloc`, `dealloc`, `rank_answer`, `breakdown_answer`, and the Stage-1 traps |
| `src/bytes.rs` | Byte classes, case folding, FNV-1a hashing, smoothstep, float helpers |
| `src/tokens.rs` | Tokenisation, salience weights, boilerplate openers |
| `src/units.rs` | Figure parsing, unit tables, dimensions, normalisation |
| `src/facts.rs` | Graded agreement and the multiplicative fact term |
| `src/sets.rs` | Open-addressed token sets (matching stays linear, not n·m) |
| `src/score.rs` | The pipeline above |
| `src/profile.rs` | Every tunable constant, and the per-intent overrides |

## Build

```bash
export PATH="/c/Users/hyada/.cargo/bin:$PATH"

cargo test                                                     # 44 unit tests, host target
cargo build --release --target wasm32-unknown-unknown          # generic
cargo build --release --target wasm32-unknown-unknown --no-default-features --features ip-geolocation
cargo build --release --target wasm32-unknown-unknown --no-default-features --features storm-alert

wasm-tools print target/wasm32-unknown-unknown/release/scorer.wasm | grep -c '(import'   # must be 0
wasm-tools validate target/wasm32-unknown-unknown/release/scorer.wasm
node verify.mjs dist/ip_geolocation.wasm
```

The crate is `#![no_std]` on wasm and links `std` on the host, so `cargo test` runs normally while
the shipped artefact stays freestanding. `wasm-tools print | grep -c '(import'` is the check that
proves it — a WASI or `wasm-bindgen` build is an instant registration reject.

## Verification

All three builds pass `verify.mjs` in full. Artefacts:

| Build | Size | Imports | `wasm-tools validate` |
|---|---|---|---|
| `dist/generic.wasm` | 13,869 B | **0** | OK |
| `dist/ip_geolocation.wasm` | 13,870 B | **0** | OK |
| `dist/storm_alert.wasm` | 13,852 B | **0** | OK |

Exported signatures, read back off the binary — `rank_answer` is **exactly six `i32` returning
`f32`** (a 3-param build was rejected live):

```
(export "alloc"            (func (param i32) (result i32)))
(export "dealloc"          (func (param i32 i32)))
(export "rank_answer"      (func (param i32 i32 i32 i32 i32 i32) (result f32)))
(export "breakdown_answer" (func (param i32 i32 i32 i32 i32 i32 i32) (result i32)))
```

Stage-1 traps, each reproducing a recorded live rejection:

```
PASS  zero imports (freestanding)  []
PASS  rank_answer takes exactly 6 params  got 6
PASS  empty answer is EXACTLY 0.0  got 0            <- host passes ptr=0,len=0 without calling alloc
PASS  whitespace-only (spaces) is EXACTLY 0.0  got 0   <- a returned 0.0007 failed a live registration
PASS  self-match beats unrelated cross-match  1 > 0
PASS  self-match clears the 0.75 ratchet  1
PASS  ~54 KB repeated text does not trap  0
PASS  emoji/CJK/accents do not trap
PASS  allocator never returns 0 under sustained load
```

Hand cases on the `ip_geolocation` build:

```
  1.000000  self-match (ground truth as answer)
  0.999275  correct + terse
  1.000000  correct as JSON              <- format equivalence: JSON scores like prose
  0.000070  wrong location (fact swap)
  0.002215  question echo (contentless)  <- the incumbent scores this 0.9933
  0.001962  content-filter refusal
  0.018832  keyword stuffing
  0.000000  off-topic
  0.000000  empty

  CVSS 10  (correct)   -> 1.000000
  CVSS 9.8 (near miss) -> 1.000000       <- inside tolerance, still the same claim
  CVSS 3.1 (wrong)     -> 0.226298       <- degrades, does not fall off a cliff

  5 m/s   (same unit)   -> 1.000000
  18 km/h (same speed)  -> 1.000000      <- unit normalised
  47 m/s  (wrong speed) -> 0.002409
```

## Measured against the live champions

`track2/harness/run-eval.mjs` reproduces the node's two-stage gate offline against the incumbent
binaries (`ipgeo_reg630`, `storm_rpen_reg453`). **Both intents clear every check.**

| Check | IP_GEOLOCATION | STORM_ALERT |
|---|---|---|
| A stddev > 0.05 | PASS 0.4332 | PASS 0.3895 |
| B self-match ≥ max(0.75, incumbent) | PASS 1.0 vs bar 1.0 | PASS 1.0 vs bar 0.9933 |
| C Spearman ≥ 0.60 | SKIP (1 miner) | **PASS 0.632** |
| D1 margin > champion (strict) | PASS **0.784** vs 0.596 | PASS **0.581** vs 0.425 |
| D2 margin ≥ 0.15 | PASS | PASS |
| D3 wins ≥ champion | PASS 27/29 vs 22/29 | PASS 21/29 vs 18/29 |
| **Verdict** | **would promote** | **would promote** |

Per-class pairwise ranking accuracy, candidate vs incumbent:

| Class | IP_GEO cand | IP_GEO ref | STORM cand | STORM ref |
|---|---|---|---|---|
| FACT-SWAP | **4/4** | 4/4 | **4/4** | 4/4 |
| UNIT/FORM | **4/4** | 2/4 | **4/4** | 2/4 |
| LENGTH | **2/2** | 1/2 | **2/2** | 1/2 |
| CONTRADICTION | 1/1 | 1/1 | **1/1** | 0/1 |
| REAL-PARROT | **6/8** | 4/8 | 0/8 | 1/8 |
| OUR-STYLE-WRONG | 1/1 | 1/1 | 1/1 | 1/1 |
| REFUSAL / STUFFING / EMPTY / CONTENT-FILTER / TEMPORAL | all 1.0 | all 1.0 | all 1.0 | all 1.0 |

The FACT-SWAP margins are the clearest exhibit: **0.458** (IP_GEO) and **0.505** (STORM) against
the incumbent's **0.004**. The incumbent orders those pairs correctly but by a margin four
thousandths wide — it is very nearly blind to a swapped decisive fact, which is exactly the failure
mode a Tier-A deterministic intent cannot tolerate.

## Honest limitations

- **The corpus is a proxy, not the node's benchmark.** The node's fixtures are closed-source and
  unrecoverable. What transfers is the *comparison* against a pinned incumbent binary,
  not the absolute numbers.
- **STORM_ALERT trades the parrot exhibit for Spearman.** That intent has ~4 miners, so gate C is
  enforced, and the incumbent is a lexical scorer that *rewards* contentless echoes. Agreeing with
  its ranking of real traffic (ρ ≥ 0.60) and fixing its parrot hole are directly opposed. The
  constants resolve that in favour of passing the gate, and the cost is recorded rather than
  hidden: REAL-PARROT is 0/8 there, versus 6/8 on IP_GEOLOCATION where Spearman is skipped.
- **If only one intent is registered first, register IP_GEOLOCATION.** It has no Spearman
  constraint, the larger margin delta (+0.188 vs +0.156), and it is where the thesis is fully
  expressed. Its live margin bar is also the highest of any target (~0.992), so re-poll
  `/api/wasm` for the current bar and register at a local low.
- **Tuning was measured, not guessed**, but only against this corpus. The sweep imports the
  harness's own `corpus.mjs` so the Spearman set optimised is byte-identical to the one the gate
  reads — an earlier sweep against a hand-rolled proxy reported ρ 0.639 where the harness measured
  0.538, which is precisely the error that makes a candidate fail on-chain after passing locally.
- `breakdown_answer` is debug-only and is never called by either gate.

## Prior art and method

The incumbent champion — `zkasuran/telegraph-salience-scorer` (MIT) — was studied openly, both
its published source and its compiled behaviour, and several of its sound ideas (salience
weighting, a normalized exact-match short-circuit, multiplicative penalties for decisive-fact
disagreement) shaped this design. This module is an independent implementation, not a fork; where
the two disagree — precision-of-answer vs recall-of-truth, smoothstep vs step calibration, typed
unit normalisation, the answered-ness gate — the choice was made by measurement against public
score records and the incumbent's own binaries, and the reasoning is recorded in `tune.md`.

## Disclosure

The author of this scoring module also operates the Track 1 miner `livecert` (registration 225),
which serves intents including STORM_ALERT and IP_GEOLOCATION. The module encodes general intent
correctness — its test corpus includes cases where livecert's own answer style is scored **down**
when factually wrong (the `OUR-STYLE-WRONG` class) — and the overlap was proactively disclosed to
the hackathon organizers, who will flag it for transparent review. No slug, wallet, field name or
phrasing is matched by the scoring logic, favourably or otherwise.
