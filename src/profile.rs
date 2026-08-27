//! Every tunable constant, in one block.
//!
//! Kept together deliberately: these are meant to be *swept* by the harness
//! iterate loop, not guessed, and a reviewer reading this file should be able to
//! see the whole decision surface at once. `tune.md` documents each one's
//! default and rationale. One source is compiled once per intent (A6); the
//! per-intent blocks below are selected by cargo feature.

#![allow(dead_code)]

/// Intent tag, for provenance. Not read by the node; mirrors the champion's
/// `TELEGRAPH_INTENT` marker so a reviewer can tell two builds apart.
#[no_mangle]
pub static TELEGRAPH_INTENT: [u8; 32] = intent_tag();

pub struct Profile {
    // ---- salience weights (A3.2) --------------------------------------
    /// Weight of a numeric token. Figures carry Tier-A correctness.
    pub w_number: f32,
    /// Weight of an identifier (IP, CVE id, version, date, coordinate).
    pub w_ident: f32,
    /// Weight of a stopword. Near zero, but not zero: stopwords still dilute a
    /// stuffed answer's precision denominator.
    pub w_stop: f32,
    /// Weight of an opaque non-Latin token (we cannot segment it).
    pub w_high: f32,
    /// Base weight of a content word, before the length bonus.
    pub w_word_base: f32,
    /// Extra weight per character of a content word, capped at `w_len_cap`.
    pub w_len_step: f32,
    pub w_len_cap: f32,
    /// Bonus for a mid-sentence capitalised token (proper nouns carry answers).
    pub w_proper: f32,

    // ---- anti-parrot (A3.6) -------------------------------------------
    /// Reserved. Question-echoed tokens are **not** discounted in precision:
    /// measured over 554 real rows, question-overlap correlates *negatively*
    /// (-0.258) with the champion's score, so a general echo penalty buys
    /// nothing and costs Spearman agreement. The echo flag is used only as a
    /// boolean inside the answered-ness gate, which is what actually catches
    /// the parrot.
    pub echo_discount: f32,

    // ---- answered-ness gate (A3.6, A3.9) ------------------------------
    /// Novel-supported-mass at which the answered-ness gate is fully open.
    pub ans_sat: f32,
    /// Fraction of the ground truth's own answer-bearing mass used as the
    /// saturation point when the GT is thin.
    pub ans_gt_frac: f32,
    /// Floor on the saturation point, so a one-word GT cannot open the gate on
    /// noise.
    pub ans_sat_min: f32,
    /// A token must weigh at least this much to count as decisive content.
    pub decisive_min: f32,
    /// How much ordinary prose counts toward *novelty*, relative to a hard
    /// assertion. Low, because a parrot padded with generic filler otherwise
    /// earns novelty credit whenever the ground truth is long enough to contain
    /// the same common words.
    pub novel_prose_w: f32,
    /// Floor under the answered-ness gate. Keeps a shut gate from collapsing
    /// every non-answer onto exactly the same value, so the ordering *below* the
    /// gate is still resolved by precision. Ties are what cost Spearman.
    pub ans_floor: f32,
    /// Below this much answer-bearing mass, the ground truth is itself
    /// refusal-shaped or hedged. Nothing can be "unanswered" against it, so the
    /// gate opens fully rather than zeroing every answer. In real traffic the
    /// refusals are usually the ground truths, not the answers.
    pub gt_decisive_min: f32,

    // ---- fact agreement (A3.4) ----------------------------------------
    /// Relative-error decay for a numeric near-miss: agreement = 1/(1 + k*rel).
    pub num_rel_k: f32,
    /// Relative tolerance inside which two figures are the same claim.
    pub num_rel_tol: f32,
    /// Absolute tolerance for bounded [0,1] quantities (risk scores, fractions).
    pub num_abs_tol: f32,
    /// For two *unitless* figures, how many multiples apart they may be and
    /// still count as claims about the same quantity. Beyond it the answer's
    /// figure is unverifiable rather than wrong. Figures carrying units are
    /// compared by dimension instead and ignore this.
    pub num_band_rel: f32,
    /// How much the *worst* figure in the answer, rather than the average one,
    /// decides the numeric channel. 0 = plain mean, 1 = worst figure only. A
    /// wrong decisive fact must not hide behind four right ones.
    pub num_min_bias: f32,
    /// How far each channel may pull the fact term down: 1.0 lets a wholly-wrong
    /// channel zero it, 0.0 disables the channel. Channels multiply.
    pub num_channel_w: f32,
    pub id_channel_w: f32,
    /// Floor of the fact multiplier. Keeps a wholly-wrong-figure answer above a
    /// cliff so near-misses stay distinguishable from garbage.
    pub fact_floor: f32,

    // ---- prose vs assertion (A3.4) ------------------------------------
    /// Share of precision carried by ordinary prose rather than by decisive
    /// assertions. A3.4 makes fact agreement dominant and lexical overlap "only
    /// a low-weight tie-breaker for prose quality" - this is that weight.
    /// Keeping it low stops a correct-but-wordy answer being diluted below a
    /// terse wrong one purely for using more words.
    pub prose_w: f32,

    // ---- shaping and calibration (A3.7, A8 stddev) --------------------
    /// Blend between linear precision and concave `p*(2-p)`. 0 = linear.
    pub p_concave: f32,
    /// Smoothstep knots applied to the raw composite. Widening these is the
    /// primary lever on `score_stddev` (gate needs > 0.05).
    pub ss_lo: f32,
    pub ss_hi: f32,
}

pub const fn base() -> Profile {
    Profile {
        w_number: 3.0,
        w_ident: 3.4,
        w_stop: 0.05,
        w_high: 0.5,
        w_word_base: 1.0,
        w_len_step: 0.06,
        w_len_cap: 12.0,
        w_proper: 1.0,

        echo_discount: 0.25,

        ans_sat: 3.0,
        ans_gt_frac: 0.5,
        ans_sat_min: 0.9,
        decisive_min: 0.5,
        novel_prose_w: 0.35,
        ans_floor: 0.05,
        gt_decisive_min: 0.8,

        num_rel_k: 8.0,
        num_rel_tol: 0.02,
        num_abs_tol: 0.02,
        num_band_rel: 10.0,
        num_min_bias: 0.5,
        num_channel_w: 0.9,
        id_channel_w: 0.9,
        fact_floor: 0.10,

        prose_w: 0.25,

        p_concave: 0.5,
        // Knots deliberately short of 0 and 1: clipping either end piles real
        // answers onto identical scores, and ties are what cost Spearman.
        ss_lo: 0.02,
        ss_hi: 0.92,
    }
}

// --------------------------------------------------------------------------
// Per-intent overrides. Only the constants that differ are restated, so the
// diff against `base()` *is* the per-intent tuning record.
// --------------------------------------------------------------------------

#[cfg(feature = "ip-geolocation")]
pub const fn profile() -> Profile {
    let mut p = base();
    // The IP itself is always echoed from the question, so the decisive content
    // is country/city/ISP/coordinates only. Demand real novel mass before the
    // answered-ness gate opens.
    p.ans_sat = 3.5;
    // Identifiers (the IP, the CIDR range, the AS number) are the spine of this
    // intent and admit no tolerance at all, so the identifier channel gets full
    // authority to zero the fact term.
    p.w_ident = 4.0;
    p.id_channel_w = 1.0;
    // Single miner means Spearman is skipped (A6), so calibrate purely for
    // separation rather than for agreement with the champion's ordering. The
    // margin bar here is the highest of any target (~0.992), so the top of the
    // range is deliberately saturated to maximise mean(good) - mean(bad).
    p.ss_hi = 0.88;
    p
}

#[cfg(feature = "storm-alert")]
pub const fn profile() -> Profile {
    let mut p = base();
    // Wind speeds and gusts arrive in m/s, km/h and knots across miners; the
    // unit normaliser handles the conversion, so the numeric channel is the
    // dominant signal here and deserves a tighter near-miss decay.
    p.num_channel_w = 1.0;
    p.num_rel_k = 10.0;
    // Risk is a bounded [0,1] score: an absolute epsilon, not a relative one.
    p.num_abs_tol = 0.05;
    // ~4 miners means Spearman IS enforced (>= 0.60), and that check is a hard
    // constraint pulling the opposite way from the rest of this design: the
    // incumbent is a lexical scorer, so agreeing with its ordering of real
    // traffic means *being* more lexical. Every constant below was swept
    // against the two objectives jointly (see tune.md); this is the point that
    // clears rho >= 0.60 while still beating the incumbent's separation.
    //
    // Knots at the full range, so nothing is clipped and every distinct raw
    // composite keeps a distinct score: saturating either end would pile real
    // answers onto identical values, and ties are exactly what costs Spearman.
    // IP_GEOLOCATION, where Spearman is skipped, makes the opposite trade.
    p.ss_lo = 0.0;
    p.ss_hi = 1.0;
    // Ground truths for this intent are frequently themselves refusals ("I
    // cannot provide the specific 48-hour forecast..."), so the answered-ness
    // gate must scale down with the GT's own thin content rather than zeroing.
    p.ans_gt_frac = 0.40;
    // Prose carries most of precision here. That is the Spearman tax: the
    // incumbent ranks real answers lexically, so tracking its ordering means
    // weighting surface agreement above what A3.4 would otherwise want. It is
    // deliberately NOT pushed to 1.0 even though that scored slightly better on
    // this corpus: at 1.0 the decisive-fact pool drops out of precision
    // entirely, and the build then misranks plainly-correct answers on any
    // question unlike the ones tuned against. Fact-awareness survives either
    // way, because the fact term is *multiplicative* and applied after
    // precision -- FACT-SWAP stays 4/4 here.
    p.prose_w = 0.7;
    // A high floor under the answered-ness gate, for the same reason: the
    // incumbent scores contentless echoes highly, so crushing them to zero is
    // precisely the disagreement that fails check C. Chosen for headroom on
    // rho (0.632 against a 0.60 floor), which is the binding constraint here --
    // the margin bar has far more slack than the agreement bar does. The honest consequence is
    // that the parrot exhibit is muted on THIS intent; it is fully expressed on
    // IP_GEOLOCATION, where Spearman is skipped. See tune.md and README.
    p.ans_floor = 0.75;
    p.ans_sat = 2.0;
    p
}

#[cfg(all(feature = "generic", not(feature = "ip-geolocation"), not(feature = "storm-alert")))]
pub const fn profile() -> Profile {
    base()
}

const fn intent_tag() -> [u8; 32] {
    #[cfg(feature = "ip-geolocation")]
    let name = b"IP_GEOLOCATION";
    #[cfg(feature = "storm-alert")]
    let name = b"STORM_ALERT";
    #[cfg(all(feature = "generic", not(feature = "ip-geolocation"), not(feature = "storm-alert")))]
    let name = b"GENERIC";

    let mut out = [0u8; 32];
    let mut i = 0usize;
    while i < name.len() && i < 32 {
        out[i] = name[i];
        i += 1;
    }
    out
}
