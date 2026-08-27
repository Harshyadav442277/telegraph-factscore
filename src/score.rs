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

    // Mark echo and support. Support is **graded**, not boolean: collapsing the
    // agreement to `>= 1 - 1e-6` put a hard cliff on top of a smooth curve, and
    // a 1% change in an asserted figure moved the score by 0.999 whenever the
    // figure was the answer's only decisive content (adversarial review M2).
    let mut i = 0usize;
    while i < ta.n {
        ta.echo[i] = sq.contains_tok(ta, i);
        if ta.kind[i] == K_NUMBER {
            // A figure is supported to the degree some ground-truth figure
            // agrees with it — not merely when the same digits appear.
            ta.supw[i] = best_agreement(ta, i, tg, &p).unwrap_or(0.0);
            ta.supi[i] = 0;
        } else {
            match sg.find(ta, i) {
                Some(k) => {
                    ta.supw[i] = 1.0;
                    ta.supi[i] = k as u32 + 1;
                }
                None => {
                    ta.supw[i] = 0.0;
                    ta.supi[i] = 0;
                }
            }
        }
        i += 1;
    }

    let precision = precision_of(ta, &p);
    let answered = answeredness(ta, tg, sq, &p);
    let (fmul, fact_raw) = fact_multiplier(ta, tg, &p);
    let polarity = polarity_of(ta, tg, &p);

    // Concave shaping pulls a mostly-right answer up without flattening the
    // middle; p_concave = 0 leaves precision linear.
    let shaped = (1.0 - p.p_concave) * precision + p.p_concave * (precision * (2.0 - precision));

    let raw = clamp01(shaped * fmul * answered * polarity);
    let final_score = clamp01(smoothstep(p.ss_lo, p.ss_hi, raw));

    Breakdown {
        precision,
        fact: fact_raw * polarity,
        answered,
        raw,
        final_score,
    }
}

/// Penalty for asserting the opposite of what the ground truth says.
///
/// Support is a set-membership test, so it cannot see polarity: before this, "is
/// located in Germany" and "is **not** located in Germany" differed by one
/// 0.05-weight stopword out of a ~15-token pool and tied at 1.0000 (adversarial
/// review C2). A supported token whose negation state disagrees with the ground
/// truth occurrence it matched is not coverage; it is a contradiction.
fn polarity_of(ta: &Toks, tg: &Toks, p: &Profile) -> f32 {
    let (mut sup_mass, mut contra_mass) = (0.0f32, 0.0f32);
    let mut i = 0usize;
    while i < ta.n {
        // Any supported *content* token can carry a polarity claim — not just a
        // decisive one. The claim in "is not a known proxy and is not flagged
        // for abuse" lives entirely in lowercase common words, so restricting
        // this to proper nouns and figures left that negation tying its own
        // positive at 1.0000.
        //
        // Echoed tokens are excluded: the question's own subject is not part of
        // the claim, and counting it halved the measured contradiction.
        if !ta.boiler[i]
            && !ta.echo[i]
            && ta.w[i] >= p.decisive_min
            && ta.supw[i] > 0.0
            && ta.supi[i] > 0
        {
            let k = (ta.supi[i] - 1) as usize;
            if k < tg.n {
                let w = ta.w[i] * ta.supw[i];
                sup_mass += w;
                if ta.neg[i] != tg.neg[k] {
                    contra_mass += w;
                }
            }
        }
        i += 1;
    }
    if sup_mass <= 0.0 {
        return 1.0;
    }
    clamp01(1.0 - p.m_contra * (contra_mass / sup_mass))
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
            fact_n += w * ta.supw[i];
        } else {
            prose_d += w;
            prose_n += w * ta.supw[i];
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
    let mass = |t: &Toks, i: usize, prose_w: f32| -> f32 {
        if t.decisive[i] {
            t.w[i]
        } else {
            t.w[i] * prose_w
        }
    };

    // Does the ground truth state decisive facts of its own that the question
    // did not already give away?
    let (mut gt_ans, mut gt_decisive) = (0.0f32, 0.0f32);
    let mut k = 0usize;
    while k < tg.n {
        if tg.w[k] >= p.decisive_min && !sq.contains_tok(tg, k) {
            gt_ans += mass(tg, k, p.novel_prose_w);
            if tg.decisive[k] {
                gt_decisive += tg.w[k];
            }
        }
        k += 1;
    }

    // When it does, novelty is counted from decisive content ONLY. Prose
    // agreement is not assertion: a ground-truth-blind list of the intent's own
    // field names ("country, region, city, latitude, longitude, ISP ...")
    // carries no figure, identifier or proper noun, yet 81% of it appeared
    // somewhere in a long ground truth and it scored a perfect 1.0 on recorded
    // rows (adversarial review C5). If the truth states facts, an answer that
    // states none of them has not answered.
    let novel_prose_w = if gt_decisive >= p.gt_decisive_min {
        p.novel_prose_w_gt
    } else {
        p.novel_prose_w
    };
    if gt_ans < p.gt_decisive_min {
        // Refusal-shaped ground truth: no answer can be "unanswered" against it.
        return 1.0;
    }

    let mut novel = 0.0f32;
    let mut i = 0usize;
    while i < ta.n {
        if !ta.boiler[i] && !ta.echo[i] && ta.w[i] >= p.decisive_min {
            novel += mass(ta, i, novel_prose_w) * ta.supw[i];
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
    fn a_regrouped_figure_is_not_an_exact_match() {
        // The exact-match short-circuit used to fold punctuation, so each of
        // these returned a literal 1.0 for a wrong answer (review C1).
        let q = b"What is the CVSS score?";
        let gt = b"The CVSS score is 10.";
        let wrong = score(q, gt, b"The CVSS score is 1.0");
        let right = score(q, gt, b"The CVSS score is 10.");
        assert_eq!(right, 1.0);
        assert!(wrong < 0.9, "CVSS 1.0 against a truth of 10 scored {}", wrong);
    }

    #[test]
    fn a_negated_claim_does_not_tie_the_correct_one() {
        // One word flips the meaning; before the polarity term both scored
        // 1.0000 (review C2).
        let q = b"Where is the IP 8.8.8.8?";
        let gt = b"The IP 8.8.8.8 is located in Germany.";
        let pos = score(q, gt, b"The data shows the IP 8.8.8.8 is located in Germany.");
        let neg = score(q, gt, b"The data shows the IP 8.8.8.8 is not located in Germany.");
        assert!(pos > neg, "positive {} must beat negated {}", pos, neg);
        assert!(neg < 0.75, "a flat contradiction scored {}", neg);
    }

    #[test]
    fn a_ground_truth_blind_field_list_is_not_an_answer() {
        // A keyword blob written from the intent's field names, with no lookup
        // and no knowledge of any ground truth, scored 1.0 on live rows (C5).
        let q = b"Can you look up the geolocation for the IP address 91.146.179.123?";
        let gt = b"The IP address 91.146.179.123 resolves to Reykjavik, Capital Region, Iceland. It is announced by Ljosleidarinn ehf (AS22057).";
        let blob = score(q, gt,
            b"The data shows the country, region, city, latitude, longitude, coordinates, ISP, organisation, autonomous system network, hosting provider, timezone, postal code, continent and address associated with this IP address, including its allocation, registry, abuse contact and reported activity.");
        let real = score(q, gt, b"The data shows the IP resolves to Reykjavik, Capital Region, Iceland, announced by Ljosleidarinn ehf.");
        assert!(real > blob, "a real answer {} must beat the field-name blob {}", real, blob);
        assert!(blob < 0.5, "field-name blob scored {}", blob);
    }

    #[test]
    fn coordinates_without_a_degree_sign_still_count() {
        // The plain-text form was scored 0.0000, identical to the wrong
        // hemisphere (review M5).
        let q = b"What are the coordinates?";
        let gt = b"Approximate coordinates are -34.9011, -56.1645.";
        let plain = score(q, gt, b"The data shows coordinates 34.9011S, 56.1645W.");
        let wrong = score(q, gt, b"The data shows coordinates 34.9011N, 56.1645E.");
        assert!(plain > 0.5, "correct plain-text coordinates scored {}", plain);
        assert!(plain > wrong, "right {} must beat wrong hemisphere {}", plain, wrong);
    }

    #[test]
    fn an_empty_ground_truth_abstains() {
        assert_eq!(score(Q, b"", b"anything at all"), 0.0);
    }
}
