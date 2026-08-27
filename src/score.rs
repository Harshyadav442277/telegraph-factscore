//! The scoring pipeline.
//!
//! Precision of the answer, gated by answered-ness, multiplied by typed fact
//! agreement, calibrated with a smoothstep. In order:
//!
//!   1. tokenise question / ground truth / answer;
//!   2. strike the `"The data ..."` boilerplate opener from the answer;
//!   3. mark each answer token as echoed (present in the question) and/or
//!      supported (present in the ground truth, with numeric tolerance);
//!   4. **P** — weighted fraction of what the answer *asserts* that the ground
//!      truth supports, measured over decisive facts and prose separately;
//!   5. **ans** — answered-ness: does the answer carry novel supported content
//!      at all? A question-echo and a content-filter refusal both fail here;
//!   6. **fmul** — typed fact agreement, multiplicative (A3.4);
//!   7. smoothstep calibration for genuine spread, never a cliff (A3.7).

use crate::bytes::*;
use crate::facts::{best_agreement, fact_multiplier};
use crate::units::annotate_units;
use crate::profile::{profile, Profile};
use crate::sets::{Set, EMPTY_SET};
use crate::tokens::{mark_boilerplate, tokenize, Toks, EMPTY_TOKS, K_NUMBER};

// Scratch state. The module is single-threaded and the host gives each call a
// fresh logical invocation, so these are reset at the top of every score().
static mut TQ: Toks = EMPTY_TOKS;
static mut TG: Toks = EMPTY_TOKS;
static mut TA: Toks = EMPTY_TOKS;
static mut SQ: Set = EMPTY_SET;
static mut SG: Set = EMPTY_SET;

/// The shipped module is single-threaded wasm, where these statics are simply
/// scratch. `cargo test` on the host runs tests in parallel threads against the
/// same statics, so host test builds serialise entry. This exists only so the
/// tests are meaningful; it compiles out of every wasm build.
#[cfg(all(test, not(target_arch = "wasm32")))]
static SCRATCH_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The five debug figures behind a score, in the order `breakdown_answer` writes
/// them: precision, fact agreement, answered-ness, raw composite, final score.
#[derive(Clone, Copy)]
pub struct Breakdown {
    pub precision: f32,
    pub fact: f32,
    pub answered: f32,
    pub raw: f32,
    pub final_score: f32,
}

pub fn score(q: &[u8], gt: &[u8], ma: &[u8]) -> f32 {
    breakdown(q, gt, ma).final_score
}

pub fn breakdown(q: &[u8], gt: &[u8], ma: &[u8]) -> Breakdown {
    #[cfg(all(test, not(target_arch = "wasm32")))]
    let _scratch = SCRATCH_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let p = profile();
    let zero = Breakdown {
        precision: 0.0,
        fact: 0.0,
        answered: 0.0,
        raw: 0.0,
        final_score: 0.0,
    };

    // Nothing to support the answer against: abstain at zero rather than invent
    // a score from the question alone.
    if gt.is_empty() || is_blank(ma) {
        return zero;
    }

    // SAFETY: single-threaded wasm; the three token buffers and two sets are
    // distinct statics, and every field is rewritten before it is read.
    let (tq, tg, ta, sq, sg) = unsafe {
        (
            &mut *core::ptr::addr_of_mut!(TQ),
            &mut *core::ptr::addr_of_mut!(TG),
            &mut *core::ptr::addr_of_mut!(TA),
            &mut *core::ptr::addr_of_mut!(SQ),
            &mut *core::ptr::addr_of_mut!(SG),
        )
    };

    tokenize(q, tq);
    tokenize(gt, tg);
    tokenize(ma, ta);
    annotate_units(tq);
    annotate_units(tg);
    annotate_units(ta);
    if ta.n == 0 || tg.n == 0 {
        return zero;
    }

    sq.fill(tq);
    sg.fill(tg);
    mark_boilerplate(ta);

    // Mark echo and support.
    let mut i = 0usize;
    while i < ta.n {
        ta.echo[i] = sq.contains_tok(ta, i);
        ta.sup[i] = if ta.kind[i] == K_NUMBER {
            // A figure is supported when some ground-truth figure agrees with it
            // inside tolerance — not merely when the same digits appear.
            match best_agreement(ta, i, tg, &p) {
                Some(a) => a >= 1.0 - 1e-6,
                None => false,
            }
        } else {
            sg.contains_tok(ta, i)
        };
        i += 1;
    }

    let precision = precision_of(ta, &p);
    let answered = answeredness(ta, tg, sq, &p);
    let (fmul, fact_raw) = fact_multiplier(ta, tg, &p);

    // Concave shaping pulls a mostly-right answer up without flattening the
    // middle; p_concave = 0 leaves precision linear.
    let shaped = (1.0 - p.p_concave) * precision + p.p_concave * (precision * (2.0 - precision));

    let raw = clamp01(shaped * fmul * answered);
    let final_score = clamp01(smoothstep(p.ss_lo, p.ss_hi, raw));

    Breakdown {
        precision,
        fact: fact_raw,
        answered,
        raw,
        final_score,
    }
}

/// Weighted fraction of the answer's own content that the ground truth supports.
///
/// Question-echoed tokens are **not** discounted here. Measured over 554 real
/// rows, bag-of-words overlap with the question correlates *negatively* (-0.258)
/// with the champion's score, so a general echo penalty would buy nothing and
/// would wreck the Spearman agreement the gate requires on multi-miner intents.
/// The parrot effect is positional, and the mechanism that catches it is
/// `answeredness` below — where the echo flag *is* used, and only there.
///
/// Measured over two separate pools. **Decisive** tokens — figures,
/// identifiers, proper nouns — are what the answer is right or wrong *about*.
/// Ordinary prose is style, and enters only at `prose_w`, because a correct but
/// wordy answer must not be diluted below a terse wrong one merely for using
/// more words (ARCHITECTURE A3.4: facts dominant, lexical overlap a low-weight
/// tie-breaker).
fn precision_of(ta: &Toks, p: &Profile) -> f32 {
    let (mut fact_n, mut fact_d) = (0.0f32, 0.0f32);
    let (mut prose_n, mut prose_d) = (0.0f32, 0.0f32);
    let mut i = 0usize;
    while i < ta.n {
        if ta.boiler[i] {
            i += 1;
            continue;
        }
        let w = ta.w[i];
        if ta.decisive[i] {
            fact_d += w;
            if ta.sup[i] {
                fact_n += w;
            }
        } else {
            prose_d += w;
            if ta.sup[i] {
                prose_n += w;
            }
        }
        i += 1;
    }

    match (fact_d > 0.0, prose_d > 0.0) {
        (true, true) => {
            clamp01((1.0 - p.prose_w) * (fact_n / fact_d) + p.prose_w * (prose_n / prose_d))
        }
        // An answer of pure assertions, or of pure prose: score what it has.
        (true, false) => clamp01(fact_n / fact_d),
        (false, true) => clamp01(prose_n / prose_d),
        _ => 0.0,
    }
}

/// Does the answer contribute anything beyond the question and the boilerplate?
///
/// This is a *gate*, not a recall term: a little genuine novel supported content
/// opens it fully. It only stays shut when the answer adds nothing — the empty
/// answer, the content-filter refusal, and the contentless question-echo that
/// the live champion scores 0.9933 (ARCHITECTURE A3.6, the headline exhibit).
/// This is the **only** place the question-echo flag is consulted.
///
/// Crucially the gate is conditioned on the ground truth. In real traffic the
/// refusals are usually the *ground truths*, not the answers (8 of 15 weather
/// GTs are hedged, and 40 of 58 sub-0.02 rows). When the ground truth itself
/// carries no decisive content there is nothing to be found, a hedged answer is
/// the correct answer, and the gate opens fully — scoring falls back to text
/// agreement. We never relitigate the ground truth.
fn answeredness(ta: &Toks, tg: &Toks, sq: &Set, p: &Profile) -> f32 {
    // The ground truth's own answer-bearing mass: what it says that the question
    // did not already give away.
    // Novelty is counted at full weight for an assertion (a figure, identifier
    // or proper noun) and heavily discounted for ordinary prose. Without that
    // split, a parrot padded with generic filler earns real novelty credit
    // whenever the ground truth is long enough to contain the same common
    // words — measured on live traffic, a contentless echo reached 0.80 on this
    // gate purely through words like "terms", "scope" and "order".
    let mass = |t: &Toks, i: usize| -> f32 {
        if t.decisive[i] {
            t.w[i]
        } else {
            t.w[i] * p.novel_prose_w
        }
    };

    let mut gt_ans = 0.0f32;
    let mut k = 0usize;
    while k < tg.n {
        if tg.w[k] >= p.decisive_min && !sq.contains_tok(tg, k) {
            gt_ans += mass(tg, k);
        }
        k += 1;
    }
    if gt_ans < p.gt_decisive_min {
        // Refusal-shaped ground truth: no answer can be "unanswered" against it.
        return 1.0;
    }

    let mut novel = 0.0f32;
    let mut i = 0usize;
    while i < ta.n {
        if !ta.boiler[i] && !ta.echo[i] && ta.sup[i] && ta.w[i] >= p.decisive_min {
            novel += mass(ta, i);
        }
        i += 1;
    }

    let sat = fmax(fmin(p.ans_sat, gt_ans * p.ans_gt_frac), p.ans_sat_min);
    // A floor under the gate: a shut gate must not collapse every non-answer
    // onto the identical value, or the ordering below it is lost to ties.
    p.ans_floor + (1.0 - p.ans_floor) * smoothstep01(novel / sat)
}

#[cfg(test)]
mod tests {
    use super::*;

    const Q: &[u8] = b"Can you look up the geolocation details for the IP address 142.251.42.174 and provide the country, city, and ISP information?";
    const GT: &[u8] = b"The IP address 142.251.42.174 is associated with Google LLC and is located in the United States. The ISP is clearly identified as Google LLC.";

    #[test]
    fn self_match_is_maximal() {
        let s = score(Q, GT, GT);
        assert!(s >= 0.75, "self-match {} must clear the 0.75 ratchet", s);
    }

    #[test]
    fn self_match_beats_an_unrelated_answer() {
        let self_m = score(Q, GT, GT);
        let cross = score(Q, GT, b"The recipe calls for two cups of flour and a pinch of salt.");
        assert!(self_m > cross, "{} must beat {}", self_m, cross);
    }

    #[test]
    fn a_correct_answer_beats_a_wrong_location() {
        let right = score(
            Q,
            GT,
            b"The data shows the IP 142.251.42.174 is hosted by Google LLC in the United States.",
        );
        let wrong = score(
            Q,
            GT,
            b"The data shows the IP 142.251.42.174 is hosted by Cloudflare in Mumbai, India.",
        );
        assert!(right > wrong, "right {} must beat wrong {}", right, wrong);
    }

    #[test]
    fn a_question_echo_scores_near_zero() {
        // The champion's known hole: this contentless restatement scores 0.9933
        // live. It asserts nothing the question did not already contain.
        let echo = score(
            Q,
            GT,
            b"The data shows the geolocation details for the IP address 142.251.42.174 including country, city and ISP information.",
        );
        let real = score(
            Q,
            GT,
            b"The data shows the IP is hosted by Google LLC in the United States.",
        );
        assert!(echo < 0.25, "question echo scored {}, must be near zero", echo);
        assert!(real > echo * 2.0, "a real answer {} must clear the echo {}", real, echo);
    }

    #[test]
    fn the_content_filter_refusal_scores_near_zero() {
        let s = score(Q, GT, b"- The generated text has been blocked by our content filters.");
        assert!(s < 0.1, "content-filter refusal scored {}", s);
    }

    #[test]
    fn keyword_stuffing_loses_to_a_real_answer() {
        let stuffed = score(
            Q,
            GT,
            b"IP address geolocation country city ISP information lookup details network region location provider",
        );
        let real = score(Q, GT, b"The data shows the IP is hosted by Google LLC in the United States.");
        assert!(real > stuffed, "real {} must beat stuffed {}", real, stuffed);
    }

    #[test]
    fn a_wrong_figure_scores_below_the_right_one() {
        let q = b"What is the CVSS score for CVE-2021-44228?";
        let gt = b"The CVSS score for CVE-2021-44228 is 10, indicating critical severity. Affected versions include Apache Log4j up to 2.14.1.";
        let right = score(q, gt, b"The data shows CVE-2021-44228 has a CVSS score of 10 and is critical in Apache Log4j.");
        let wrong = score(q, gt, b"The data shows CVE-2021-44228 has a CVSS score of 7.5 and is critical in Apache Log4j.");
        assert!(right > wrong, "right {} must beat wrong {}", right, wrong);
    }

    #[test]
    fn format_equivalence_holds_within_tolerance() {
        // Same facts, three registers. ARCHITECTURE A4: JSON and prose with equal
        // facts must score equally.
        let prose = score(Q, GT, b"The IP is hosted by Google LLC and located in the United States.");
        let json = score(Q, GT, b"{\"isp\":\"Google LLC\",\"country\":\"United States\"}");
        assert!(fabs(prose - json) < 0.35, "prose {} vs json {}", prose, json);
    }

    #[test]
    fn scores_spread_rather_than_collapsing() {
        // The gate rejects a flat scorer (stddev must exceed 0.05).
        let answers: [&[u8]; 5] = [
            GT,
            b"The data shows the IP is hosted by Google LLC in the United States.",
            b"The data shows the IP is hosted by Cloudflare in Mumbai, India.",
            b"- The generated text has been blocked by our content filters.",
            b"The recipe calls for two cups of flour.",
        ];
        let mut lo = 1.0f32;
        let mut hi = 0.0f32;
        for a in answers.iter() {
            let s = score(Q, GT, a);
            lo = fmin(lo, s);
            hi = fmax(hi, s);
        }
        assert!(hi - lo > 0.05, "spread was only {}", hi - lo);
    }

    #[test]
    fn results_do_not_depend_on_call_order() {
        let a = score(Q, GT, b"The data shows the IP is hosted by Google LLC in the United States.");
        let _ = score(b"unrelated question", b"unrelated ground truth", b"unrelated answer");
        let b = score(Q, GT, b"The data shows the IP is hosted by Google LLC in the United States.");
        assert_eq!(a, b, "stale scratch state leaked between calls");
    }

    #[test]
    fn an_empty_ground_truth_abstains() {
        assert_eq!(score(Q, b"", b"anything at all"), 0.0);
    }
}
