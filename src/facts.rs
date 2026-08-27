//! Fact agreement: how far the figures and identifiers an answer *asserts* are
//! borne out by the ground truth.
//!
//! Agreement is graded, never a step: a near-miss stays high, a gross miss falls
//! away. A figure the ground truth has no comparable counterpart for is
//! **neutral**, not wrong - we score precision of what the answer asserts, not
//! recall of the truth (ARCHITECTURE A3.8).

#![allow(dead_code)]

use crate::bytes::*;
use crate::profile::Profile;
use crate::tokens::{Toks, K_IDENT, K_NUMBER};
use crate::units::*;

// --------------------------------------------------------------------------
// Agreement
// --------------------------------------------------------------------------

/// Graded agreement between an asserted figure and a candidate one, or `None`
/// when the two are not talking about the same quantity at all.
///
/// Comparability is decided *before* magnitude, which is the whole point: two
/// wind speeds are comparable however far apart they are, so 47 m/s against a
/// ground truth of 5 m/s is a wrong answer rather than an unverifiable one.
/// Only when neither side carries a unit do we fall back on a magnitude band to
/// guess whether the figures are about the same thing.
pub fn value_agreement(av: f32, au: u8, gv: f32, gu: u8, p: &Profile) -> Option<f32> {
    let (ad, gd) = (dimension(au), dimension(gu));
    if ad != D_NONE && gd != D_NONE && ad != gd {
        // A temperature is not a near-miss for a wind speed; it is unrelated.
        return None;
    }
    let both_united = ad != D_NONE && gd != D_NONE;
    let one_united = (ad != D_NONE) != (gd != D_NONE);

    // Agreement under one reading of the pair.
    let rate = |a: f32, g: f32| -> (f32, f32) {
        let diff = fabs(a - g);
        let rel = diff / fmax(fabs(g), 1e-6);
        let score = if diff <= p.num_abs_tol || rel <= p.num_rel_tol {
            1.0
        } else {
            // 1/(1 + k*rel): smooth, bounded, and needs no transcendental.
            1.0 / (1.0 + p.num_rel_k * rel)
        };
        (score, rel)
    };

    if both_united {
        // Same dimension: convert and compare. Always comparable, however far
        // apart — 47 m/s against 5 m/s is a wrong speed, not an unknown one.
        return Some(rate(canonical(av, au), canonical(gv, gu)).0);
    }

    if one_united {
        // Only one side named a unit, so we cannot know whether the bare figure
        // was already stated in the canonical unit (`wind_kmh=128.7` beside
        // `128.7 km/h`) or needs converting (`90%` beside `0.9`). Both readings
        // are legitimate, so take the better of the two: a genuinely wrong
        // figure agrees under neither.
        let raw = rate(av, gv).0;
        let conv = rate(canonical(av, au), canonical(gv, gu)).0;
        return Some(fmax(raw, conv));
    }

    let (score, rel) = rate(av, gv);
    if rel > p.num_band_rel {
        // Unitless and orders of magnitude apart: almost certainly a different
        // quantity (a year beside a CVSS score), so the answer is unverifiable
        // here rather than wrong.
        return None;
    }
    Some(score)
}

/// Best agreement this answer figure achieves against any comparable
/// ground-truth figure. `None` when the ground truth offers nothing comparable.
///
/// When the answer's figure names a dimension and the ground truth states any
/// figure in that same dimension, only those are considered. Without this, a
/// wrong "over the next **48** hours" quietly matches a ground-truth latitude of
/// **47.8864** — a right figure attached to entirely the wrong entity.
pub fn best_agreement(ta: &Toks, i: usize, tg: &Toks, p: &Profile) -> Option<f32> {
    let ad = dimension(ta.unit[i]);
    let mut restrict = false;
    if ad != D_NONE {
        let mut k = 0usize;
        while k < tg.n {
            if tg.kind[k] == K_NUMBER && dimension(tg.unit[k]) == ad {
                restrict = true;
                break;
            }
            k += 1;
        }
    }

    let mut best: Option<f32> = None;
    let mut k = 0usize;
    while k < tg.n {
        if tg.kind[k] == K_NUMBER && (!restrict || dimension(tg.unit[k]) == ad) {
            if let Some(a) = value_agreement(ta.val[i], ta.unit[i], tg.val[k], tg.unit[k], p) {
                best = Some(match best {
                    Some(b) if b >= a => b,
                    _ => a,
                });
            }
        }
        k += 1;
    }
    best
}

/// The multiplicative fact term. Numbers are graded; identifiers are exact.
///
/// Returns `(multiplier, raw_agreement)` — the second value is exposed only
/// through `breakdown_answer` for debugging.
pub fn fact_multiplier(ta: &Toks, tg: &Toks, p: &Profile) -> (f32, f32) {
    let (mut num_w, mut num_a) = (0.0f32, 0.0f32);
    let (mut id_w, mut id_a) = (0.0f32, 0.0f32);
    let mut num_min = 1.0f32;

    let mut i = 0usize;
    while i < ta.n {
        if ta.boiler[i] {
            i += 1;
            continue;
        }
        match ta.kind[i] {
            K_NUMBER => {
                // `None` means the ground truth says nothing comparable, so the
                // figure is unverifiable rather than wrong and stays neutral.
                if let Some(best) = best_agreement(ta, i, tg, p) {
                    num_w += ta.w[i];
                    num_a += ta.w[i] * best;
                    num_min = fmin(num_min, best);
                }
            }
            K_IDENT => {
                // Identifiers admit no tolerance. They only enter the channel
                // when the ground truth states identifiers to be checked against.
                if tg.has_ident {
                    id_w += ta.w[i];
                    if ta.sup[i] {
                        id_a += ta.w[i];
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }

    // Channels combine **multiplicatively**, not by averaging: quoting the right
    // CVE id must not rescue a wrong CVSS score. Each channel's weight is how
    // far it is allowed to pull the result down, so a weight of 1.0 lets a
    // wholly-wrong channel zero the term and 0.0 disables it.
    let mut f = 1.0f32;
    if num_w > 0.0 {
        // Worst-case-leaning, not a plain mean. An answer that gets four figures
        // right and one decisive figure wrong is a wrong answer; averaging lets
        // the wrong one hide behind the others, which is exactly the FACT-SWAP
        // failure the whole design exists to catch (ARCHITECTURE A3.5).
        let mean = num_a / num_w;
        let agree = (1.0 - p.num_min_bias) * mean + p.num_min_bias * num_min;
        f *= clamp01(1.0 - p.num_channel_w * (1.0 - agree));
    }
    if id_w > 0.0 {
        f *= clamp01(1.0 - p.id_channel_w * (1.0 - id_a / id_w));
    }
    if num_w <= 0.0 && id_w <= 0.0 {
        // No typed facts on either side: the fact channel abstains entirely
        // rather than dragging a prose answer down.
        return (1.0, 1.0);
    }
    let f = clamp01(f);
    (p.fact_floor + (1.0 - p.fact_floor) * f, f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::base;
    use crate::tokens::tokenize;

    #[test]
    fn parses_decimals_and_thousands() {
        assert_eq!(leading_number(b"10").0, 10.0);
        assert_eq!(leading_number(b"0.429").0, 0.429);
        assert_eq!(leading_number(b"1,000").0, 1000.0);
        // A version string is not a decimal: stop at the integer part.
        assert_eq!(leading_number(b"2.14.1").0, 2.0);
    }

    #[test]
    fn units_normalise_across_dimensions() {
        // 18 km/h == 5 m/s
        let p = base();
        assert!(value_agreement(18.0, U_KMH, 5.0, U_MS, &p).unwrap() > 0.99);
        // 55% == 0.55
        assert!(value_agreement(55.0, U_PCT, 0.55, U_NONE, &p).unwrap() > 0.99);
        // A temperature is not a near-miss for a wind speed: not comparable.
        assert_eq!(value_agreement(23.0, U_TEMP_C, 23.0, U_MS, &p), None);
    }

    #[test]
    fn same_dimension_figures_stay_comparable_however_far_apart() {
        // The bug this pins: 47 m/s against a ground truth of 5 m/s must read as
        // a WRONG speed, not an unverifiable one that escapes the penalty.
        let p = base();
        let a = value_agreement(47.0, U_MS, 5.0, U_MS, &p);
        assert!(a.is_some(), "same-dimension figures are always comparable");
        assert!(a.unwrap() < 0.05, "and a gross miss must score near zero");
    }

    #[test]
    fn unitless_figures_far_apart_are_unverifiable_not_wrong() {
        // "hosted since 2009" beside a ground-truth CVSS of 10 is a different
        // quantity, so it must abstain rather than be scored as a wrong answer.
        let p = base();
        assert_eq!(value_agreement(2009.0, U_NONE, 10.0, U_NONE, &p), None);
    }

    #[test]
    fn agreement_degrades_smoothly_not_in_a_cliff() {
        let p = base();
        let exact = value_agreement(10.0, U_NONE, 10.0, U_NONE, &p).unwrap();
        let near = value_agreement(9.8, U_NONE, 10.0, U_NONE, &p).unwrap();
        let off = value_agreement(7.5, U_NONE, 10.0, U_NONE, &p).unwrap();
        let gross = value_agreement(95.0, U_NONE, 10.0, U_NONE, &p).unwrap();
        assert_eq!(exact, 1.0);
        assert!(near > off && off > gross);
        assert!(gross < 0.05);
        // Continuity: a 2% miss must still read as nearly right.
        assert!(near > 0.5);
    }

    #[test]
    fn a_wrong_figure_multiplies_down_but_not_to_a_cliff() {
        let p = base();
        let mut tg = Toks::new();
        tokenize(b"The CVSS score is 10.", &mut tg);
        annotate_units(&mut tg);

        let mut right = Toks::new();
        tokenize(b"a CVSS score of 10", &mut right);
        annotate_units(&mut right);
        let mut wrong = Toks::new();
        tokenize(b"a CVSS score of 7.5", &mut wrong);
        annotate_units(&mut wrong);

        let (mr, _) = fact_multiplier(&right, &tg, &p);
        let (mw, _) = fact_multiplier(&wrong, &tg, &p);
        assert!(mr > mw, "right {} must beat wrong {}", mr, mw);
        assert!(mw > p.fact_floor * 0.9, "wrong must not fall off a cliff");
    }

    #[test]
    fn unasserted_facts_are_neutral() {
        // The answer states a figure the ground truth never mentions in any
        // comparable band; precision-not-recall says that is not an error.
        let p = base();
        let mut tg = Toks::new();
        tokenize(b"Located in the United States.", &mut tg);
        annotate_units(&mut tg);
        let mut ta = Toks::new();
        tokenize(b"Hosted by Google LLC since 2009", &mut ta);
        annotate_units(&mut ta);
        let (m, _) = fact_multiplier(&ta, &tg, &p);
        assert_eq!(m, 1.0);
    }
}
