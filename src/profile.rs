//! Every tunable constant, in one block.
//!
//! Kept together deliberately: these are meant to be *swept* by the harness
//! iterate loop, not guessed, and a reviewer reading this file should be able to
//! see the whole decision surface at once. `tune.md` documents each one's
//! default and rationale. One source is compiled once per intent (A6); the
//! per-intent blocks below are selected by cargo feature.

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
    /// Prose novelty weight when the ground truth *does* state decisive facts.
    /// Much smaller: prose agreement is not assertion. Not zero, because a
    /// genuine prose-only answer must still outrank a question echo, whose
    /// tokens are excluded from novelty altogether.
    pub novel_prose_w_gt: f32,
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
    /// When true, a figure is compared only against ground-truth figures in the
    /// same role (see `tokens::role_overlap`) whenever any same-role candidate
    /// exists. Off everywhere else, because the intents already calibrated
    /// against a single-figure ground truth gain nothing and would have to be
    /// re-tuned. Measured on STOCK_PRICE: a 9%-wrong current price sitting near
    /// the ground truth's own 52-week high scored 0.55 without this and 0.10
    /// with it.
    pub role_scoped_figures: bool,
    pub id_channel_w: f32,
    /// Multiplier on a figure whose unit we could not identify, when the ground
    /// truth named a real one. Calibrated so that asserting a category error
    /// ("47 bananas") scores no better than asserting an honest wrong value
    /// ("47 m/s" where the truth is 47 km/h), which lands near 0.046.
    pub m_foreign_unit: f32,
    /// Multiplier on a bare figure matched against a united one. Weaker
    /// evidence than a properly-united match, but a legitimate shape
    /// (`wind_kmh=128.7`), so only a light discount.
    pub m_bare_unit: f32,
    /// Multiplier applied when the answer asserts the OPPOSITE verdict to the
    /// one the ground truth states. Categorical: a flipped finding is wrong
    /// however much surrounding detail is right.
    pub m_verdict_flip: f32,
    /// How hard a polarity flip on supported content is punished. A sentence and
    /// its negation are different claims, not near-matches.
    pub m_contra: f32,
    /// How much the *worst* entity decides the entity channel, rather than the
    /// average one. Mirrors `num_min_bias`: a single swapped city must not hide
    /// behind five correct entities.
    pub ent_min_bias: f32,
    /// How far a wrong entity may pull the score down. 1.0 lets a wholly-wrong
    /// entity set zero the term; 0.0 disables the channel.
    pub ent_channel_w: f32,
    /// How hard a hyphenated range is discounted for its own width, relative to
    /// the figure it is compared against. A range that contains the truth is
    /// right; a range wide enough to contain any outcome is a hedge.
    pub m_range_width: f32,
    /// Floor of the fact multiplier. Keeps a wholly-wrong-figure answer above a
    /// cliff so near-misses stay distinguishable from garbage.
    pub fact_floor: f32,
    /// What an unsupported assertion costs when it displaced nothing — the
    /// answer already covers every entity and identifier the ground truth names,
    /// and then asserts one more.
    ///
    /// 0.0 makes such an addition free, which is the pure precision-of-answer
    /// reading (A3.8) and how this scorer behaved until it was measured:
    /// appending a false IP, a false ASN, a false country or a false city to an
    /// otherwise perfect answer all scored >= 0.9999. That is a real hole — an
    /// answer can pad itself with invented facts at no cost.
    ///
    /// 1.0 would treat the addition as a substitution, which is the recall
    /// reading and punishes an answer for volunteering *true* detail the ground
    /// truth happens not to restate. Nothing in the text distinguishes the two:
    /// with no slot schema, an extra true city and an extra false city look
    /// identical. So this is deliberately small — enough that padding is not
    /// free, small enough that a correct, generous answer stays at the top of
    /// the range. The asymmetry we cannot resolve is recorded in the README.
    pub add_w: f32,

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
        m_verdict_flip: 0.04,
        w_number: 3.0,
        w_ident: 3.4,
        w_stop: 0.05,
        w_high: 0.5,
        w_word_base: 1.0,
        w_len_step: 0.06,
        w_len_cap: 12.0,
        w_proper: 1.0,

        ans_sat: 3.0,
        ans_gt_frac: 0.5,
        ans_sat_min: 0.9,
        decisive_min: 0.5,
        novel_prose_w: 0.35,
        novel_prose_w_gt: 0.0,
        ans_floor: 0.05,
        gt_decisive_min: 0.8,

        num_rel_k: 8.0,
        num_rel_tol: 0.005,
        num_abs_tol: 0.02,
        num_band_rel: 10.0,
        num_min_bias: 0.5,
        num_channel_w: 0.9,
        role_scoped_figures: false,
        id_channel_w: 0.9,
        m_foreign_unit: 0.05,
        m_bare_unit: 0.85,
        m_contra: 0.85,
        m_range_width: 2.0,
        ent_min_bias: 0.6,
        ent_channel_w: 0.9,
        fact_floor: 0.10,
        add_w: 0.35,

        // Unsupported prose is very nearly free. Prose the ground truth does not
        // restate is neither a decisive fact nor a contradiction, so it is not
        // evidence of a wrong answer, and none of the three anti-gaming channels
        // depends on it: parroting is caught by the answered-ness gate (novel
        // *supported* mass), wrong facts by the multiplicative fact/entity term,
        // contradictions by the polarity term. At the old 0.25 a *correct*
        // answer lost 12 points for wording the truth differently -- verbatim
        // 1.0000, reworded 0.8785 -- which is what cost registration 1377 the
        // ordering on the node's clean fixtures. Not literally zero, so padding
        // an answer with filler still dilutes it slightly. STORM_ALERT overrides
        // this back up; see the block below and tune.md for that trade.
        prose_w: 0.02,

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
    // Single miner means Spearman is skipped (A6), so this build can calibrate
    // for separation rather than for agreement with the champion's ordering.
    //
    // The ceiling is NOT pulled below 1.0 to buy margin. At ss_hi = 0.88 the
    // concave shaping mapped every precision at or above 0.800 to a literal 1.0,
    // so a wrong city, a wrong ISP and a wrong country each scored a perfect
    // 1.0000 while a correctly-reworded answer scored 0.9606 (pre-flight repro).
    p.ss_lo = 0.0;
    p.ss_hi = 1.0;
    // Concave shaping compounded it, lifting 0.80 to 0.96 before the smoothstep
    // saw it. Keep precision closer to linear so the top of the range ranks.
    p.p_concave = 0.15;
    // `prose_w` is the base 0.02 -- the fix that this intent's rejection
    // (registration 1377) paid for. Left in `base()` rather than restated here
    // because the finding is general: only STORM_ALERT, which must agree with a
    // lexical incumbent to clear Spearman, overrides it. Measured on this
    // profile: every correct phrasing >= 0.999 (was 0.8785 reworded) while a
    // wrong city, a wrong ISP and a swapped country all moved DOWN.
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
    // Risk is a bounded [0,1] score, so it wants an absolute epsilon. Held at
    // the base 0.02 rather than the 0.05 used before: on a canonical percentage
    // 0.05 is five whole points, which made a 1-point and a 5-point miss both
    // perfect and then dropped 73% of the score at 5.001 (review M4).
    p.num_abs_tol = 0.02;
    // ~4 miners means Spearman IS enforced. Full-range knots, so nothing is
    // clipped and every distinct composite keeps a distinct score: ties are
    // exactly what costs Spearman.
    p.ss_lo = 0.0;
    p.ss_hi = 1.0;
    // Ground truths for this intent are frequently themselves refusals, so the
    // answered-ness gate scales down with the GT's own thin content.
    p.ans_gt_frac = 0.40;
    // Prose carries most of precision here -- the Spearman tax, since the
    // incumbent ranks real traffic lexically. Not pushed to 1.0: that drops the
    // decisive-fact pool out of precision entirely and misranks correct answers
    // on questions unlike the tuning set.
    p.prose_w = 0.7;
    // The answered-ness gate is left almost closed. An earlier build set this to
    // 0.75, which pinned it open and paid a miner MORE for parroting the
    // question than for answering it: a mechanical echo scored 0.64 on recorded
    // rows against 0.03 for the real miner answers, and beat every recorded
    // answer on all 13 of them (review C3).
    p.ans_floor = 0.05;
    p.ans_sat = 6.0;
    // Prose novelty is not zeroed here as it is on IP_GEOLOCATION. Zeroing it
    // flattens the many prose-only recorded answers onto ~0, which destroys the
    // rank information the Spearman check reads. This value is the measured
    // maximum of that check subject to BOTH anti-gaming constraints still
    // holding (echo 0.0049 and field-name blob 0.0029, against a recorded-answer
    // mean of 0.0152). See tune.md: the check still does not pass.
    p.novel_prose_w_gt = 0.12;
    p
}

/// Shared calibration for the two text-verification intents.
///
/// Both are decided by a polar verdict, a bounded confidence/similarity value,
/// and a named model or source. The numeric sweep is recorded in `tune.md`:
/// worst-figure bias 1.0 and decay 60 gave the separation knee without turning
/// a small confidence miss into the same score as a gross one. Full-range
/// shaping preserves ordering while no historical-traffic correlation applies.
#[cfg(any(feature = "content-verification", feature = "text-authenticity"))]
const fn text_verification_profile() -> Profile {
    let mut p = base();
    p.w_ident = 4.0;
    p.id_channel_w = 1.0;
    p.num_abs_tol = 0.02;
    p.num_min_bias = 1.0;
    p.num_rel_k = 60.0;
    p.num_channel_w = 1.0;
    // With this steep decay, keep an unknown category ("47 bananas") below an
    // honestly but grossly wrong value expressed in the expected unit.
    p.m_foreign_unit = 0.005;
    p.ss_lo = 0.0;
    p.ss_hi = 1.0;
    p.p_concave = 0.15;
    p.ans_sat = 3.5;
    p
}

#[cfg(feature = "content-verification")]
pub const fn profile() -> Profile {
    text_verification_profile()
}

#[cfg(feature = "text-authenticity")]
pub const fn profile() -> Profile {
    text_verification_profile()
}

/// Shared calibration for the two headline-quantity intents.
///
/// Both ask for ONE number — a share price, a total value locked — and the
/// ground truth wraps it in prose that also carries dates, times, percentage
/// changes and volumes. Registration 1377 on IP_GEOLOCATION and the 2026-08-29
/// measurement both showed the same defect: with `base()`'s soft numeric
/// channel, an answer that copies the ground truth and changes ONLY the headline
/// figure keeps precision 0.959 and scores **0.927**, because the wrong figure is
/// averaged against the dates and times that still agree.
///
/// The whole intent is that one number. So the numeric channel gets full
/// authority (`num_channel_w = 1.0`) and reads the WORST comparable figure
/// rather than the mean (`num_min_bias = 1.0`): four right figures and one wrong
/// decisive figure is a wrong answer. `num_rel_k = 60` makes the decay steep
/// enough that a rounding difference survives while a genuinely different price
/// does not.
#[cfg(any(
    feature = "stock-price",
    feature = "tvl-lookup",
    feature = "crypto-price",
    feature = "onchain-tx-lookup"
))]
const fn headline_quantity_profile() -> Profile {
    let mut p = base();
    // The decisive figure must be able to zero the fact term on its own.
    p.num_channel_w = 1.0;
    p.num_min_bias = 1.0;
    // Swept against both fixture shapes (tune.md): 120 maximises separation on
    // ground-truth-like answers, which is where the node's own fixtures sit --
    // the champion scores 0.074 on recorded prose, 0.926 on ground-truth-like
    // pairs, and 0.6147 on the real fixtures, so they are roughly two thirds of
    // the way toward the latter. A 0.02% display rounding still agrees at 0.976.
    p.num_rel_k = 120.0;
    // These ground truths quote a current price, a day's range, a 52-week range
    // and a market cap side by side, so "best match over every figure" is the
    // wrong question. Compare like with like.
    p.role_scoped_figures = true;
    // With a decay this steep, an unknown category ("47 bananas") must still
    // rank below an honestly wrong value stated in the expected unit; the
    // text-verification profile carries this for the same reason.
    p.m_foreign_unit = 0.005;
    // Prices and TVL are absolute magnitudes, so relative decay carries the
    // judgement and the absolute epsilon only absorbs display rounding.
    p.num_abs_tol = 0.02;
    // Tickers, protocol names and chain names are identifiers with no tolerance:
    // an answer about Aave V2 does not answer a question about Aave V3.
    p.w_ident = 4.0;
    p.id_channel_w = 1.0;
    // Keep precision close to linear and the range full, so ordering survives.
    // Concave shaping is what lifted a wrong-figure answer from 0.771 to 0.927.
    p.p_concave = 0.15;
    p.ss_lo = 0.0;
    p.ss_hi = 1.0;
    // The question already names the ticker or protocol, so the novel content is
    // the figure itself; demand real novel mass before the gate opens.
    p.ans_sat = 3.5;
    p
}

#[cfg(feature = "stock-price")]
pub const fn profile() -> Profile {
    headline_quantity_profile()
}

#[cfg(feature = "tvl-lookup")]
pub const fn profile() -> Profile {
    headline_quantity_profile()
}

#[cfg(feature = "crypto-price")]
pub const fn profile() -> Profile {
    headline_quantity_profile()
}

#[cfg(feature = "onchain-tx-lookup")]
pub const fn profile() -> Profile {
    let mut p = headline_quantity_profile();
    // Gas fees and transfer values are ETH amounts around 0.002, so the shared
    // absolute epsilon of 0.02 is larger than the quantity itself: a swapped
    // fee and the true one both fell inside it and scored identically (measured
    // 0.9236 for both, 0/2 cases). Relative decay must decide here.
    p.num_abs_tol = 1e-9;
    p
}

#[cfg(all(
    feature = "generic",
    not(feature = "ip-geolocation"),
    not(feature = "storm-alert"),
    not(feature = "content-verification"),
    not(feature = "text-authenticity"),
    not(feature = "stock-price"),
    not(feature = "tvl-lookup"),
    not(feature = "crypto-price"),
    not(feature = "onchain-tx-lookup")
))]
pub const fn profile() -> Profile {
    base()
}

const fn intent_tag() -> [u8; 32] {
    #[cfg(feature = "ip-geolocation")]
    let name = b"IP_GEOLOCATION";
    #[cfg(feature = "storm-alert")]
    let name = b"STORM_ALERT";
    #[cfg(feature = "content-verification")]
    let name = b"CONTENT_VERIFICATION";
    #[cfg(feature = "text-authenticity")]
    let name = b"TEXT_AUTHENTICITY_CHECK";
    #[cfg(feature = "stock-price")]
    let name = b"STOCK_PRICE";
    #[cfg(feature = "tvl-lookup")]
    let name = b"TVL_LOOKUP";
    #[cfg(feature = "crypto-price")]
    let name = b"CRYPTO_PRICE";
    #[cfg(feature = "onchain-tx-lookup")]
    let name = b"ONCHAIN_TX_LOOKUP";
    #[cfg(all(
        feature = "generic",
        not(feature = "ip-geolocation"),
        not(feature = "storm-alert"),
        not(feature = "content-verification"),
        not(feature = "text-authenticity"),
        not(feature = "stock-price"),
        not(feature = "tvl-lookup"),
        not(feature = "crypto-price"),
        not(feature = "onchain-tx-lookup")
    ))]
    let name = b"GENERIC";

    let mut out = [0u8; 32];
    let mut i = 0usize;
    while i < name.len() && i < 32 {
        out[i] = name[i];
        i += 1;
    }
    out
}
