//! Tokenisation and salience weighting.
//!
//! Split on non-word bytes, but keep `.` `,` `-` `:` `/` `_` *inside* a token
//! when both neighbours are alphanumeric and a digit is involved. That single
//! rule is what keeps `192.168.1.10`, `CVE-2021-44228`, `142.250.0.0/15`,
//! `2026-08-27`, `1,000` and `3.14` intact — the tokens that decide Tier-A
//! correctness. Bytes >= 0x80 are opaque word bytes, never decoded, so emoji /
//! CJK / accented input cannot trap (A1 Stage-1).

use crate::bytes::unit_family_hash;
use crate::bytes::*;
use crate::profile::profile;
use crate::units::{
    leading_range, suffix_is_negative_hemisphere, unit_word_code, P_BASE, U_DEG, U_NONE,
};

/// Ample: a live `converted_answer` runs ~50 tokens and the longest ground truth
/// in the corpus is a few hundred. Only the ~54 KB adversarial Stage-1 input
/// reaches this, where the requirement is merely not to trap.
pub const MAX_TOKENS: usize = 2048;

pub const K_WORD: u8 = 0;
pub const K_NUMBER: u8 = 1;
pub const K_IDENT: u8 = 2;

pub struct Toks {
    pub n: usize,
    pub hash: [u32; MAX_TOKENS],
    pub stem: [u32; MAX_TOKENS],
    /// Four-letter family hash, used only for unrecognised unit-words.
    pub family: [u32; MAX_TOKENS],
    pub w: [f32; MAX_TOKENS],
    pub val: [f32; MAX_TOKENS],
    /// Upper bound of a hyphenated range; equal to `val` for a plain figure.
    pub vhi: [f32; MAX_TOKENS],
    pub kind: [u8; MAX_TOKENS],
    pub unit: [u8; MAX_TOKENS],
    /// Unit this token *names* on its own (`km`, `knots`, `°C`), precomputed so
    /// the unit pass never needs the original bytes back.
    pub uword: [u8; MAX_TOKENS],
    /// Source byte immediately after the token, so a trailing `%` is not lost.
    pub nb: [u8; MAX_TOKENS],
    /// Hash of a neighbouring word that sits where a unit would but names no
    /// unit we know (`hPa`, `bananas`). Zero when there is none.
    pub ufword: [u32; MAX_TOKENS],
    /// Token falls under a negation that has not been closed by a clause break.
    pub neg: [bool; MAX_TOKENS],
    /// First letter, lower-cased, so a run of proper nouns can be reduced to the
    /// acronym a miner may legitimately use instead ("United States" -> "us").
    pub first: [u8; MAX_TOKENS],
    /// Capitalised mid-sentence: a proper noun, i.e. a salient entity.
    pub proper: [bool; MAX_TOKENS],
    /// Starts with an ASCII capital. Kept separately because sentence-initial
    /// names are not marked `proper`: ordinary sentence openers would otherwise
    /// poison the entity channel. `score.rs` uses this only for a tightly
    /// constrained, one-token subject substitution.
    pub capitalized: [bool; MAX_TOKENS],
    /// A **two-letter** ALL-CAPS token: a standard code, not a name. ISO 3166
    /// country codes and US/Canadian state codes are exactly two letters ("US",
    /// "UY", "IS", "CA", "NY"), and their expansion cannot be derived from the
    /// spelling — "UY" is not reachable from "Uruguay" by any lexical rule, so
    /// the acronym pass in `score.rs` (which builds initials from a *run* of
    /// proper nouns) can never produce it. Such a token has to abstain rather
    /// than read as a wrong entity.
    ///
    /// The length bound is what makes this safe. It used to cover every ALL-CAPS
    /// token, and a wrong ISP written as an acronym went free: "operated by AWS"
    /// against a truth of "Google LLC" scored 0.9829 while the same swap spelled
    /// "Cloudflare Inc." scored 0.2248. Organisation acronyms are three letters
    /// or more; standard geographic codes are two.
    pub abbrev: [bool; MAX_TOKENS],
    /// Carries an assertion rather than prose: a figure, an identifier, or a
    /// proper noun. These are what a Tier-A answer is right or wrong about.
    pub decisive: [bool; MAX_TOKENS],
    pub boiler: [bool; MAX_TOKENS],
    /// Stems of the two nearest preceding content words, so a figure can be
    /// compared against the ground-truth figure in the SAME ROLE rather than
    /// against every figure in the document. A ground truth that quotes a
    /// current price, a day's range, a 52-week range and a market cap offers
    /// four currency figures; without a role the numeric channel takes the best
    /// match over all of them, and a wrong price that happens to land near the
    /// 52-week high is scored as nearly right. Zero when there is none.
    /// Figure that is a calendar date rather than an answer: a bare year, or a
    /// day-of-month sitting next to a month name. A pure restatement of the
    /// question scored 0.99997 because its only figures were the date, and they
    /// matched the truth's date exactly, so the numeric channel reported perfect
    /// agreement about nothing.
    pub date_like: [bool; MAX_TOKENS],
    pub role1: [u32; MAX_TOKENS],
    pub role2: [u32; MAX_TOKENS],
    pub echo: [bool; MAX_TOKENS],
    /// Part of an explicit model-name claim. Model names are compared through
    /// their separator-insensitive semantic signature, not generic token shape.
    pub model: [bool; MAX_TOKENS],
    /// How well the ground truth supports this token, in [0,1]. Graded, not
    /// boolean: a figure 1% off must not read the same as one that is absent.
    pub supw: [f32; MAX_TOKENS],
    /// Index of the ground-truth token this one matched, for polarity checks.
    pub supi: [u32; MAX_TOKENS],
    pub has_ident: bool,
}

pub const EMPTY_TOKS: Toks = Toks {
    n: 0,
    hash: [0; MAX_TOKENS],
    stem: [0; MAX_TOKENS],
    family: [0; MAX_TOKENS],
    w: [0.0; MAX_TOKENS],
    val: [0.0; MAX_TOKENS],
    vhi: [0.0; MAX_TOKENS],
    kind: [K_WORD; MAX_TOKENS],
    unit: [U_NONE; MAX_TOKENS],
    uword: [U_NONE; MAX_TOKENS],
    nb: [0; MAX_TOKENS],
    ufword: [0; MAX_TOKENS],
    neg: [false; MAX_TOKENS],
    first: [0; MAX_TOKENS],
    proper: [false; MAX_TOKENS],
    capitalized: [false; MAX_TOKENS],
    abbrev: [false; MAX_TOKENS],
    decisive: [false; MAX_TOKENS],
    boiler: [false; MAX_TOKENS],
    date_like: [false; MAX_TOKENS],
    role1: [0u32; MAX_TOKENS],
    role2: [0u32; MAX_TOKENS],
    echo: [false; MAX_TOKENS],
    model: [false; MAX_TOKENS],
    supw: [0.0; MAX_TOKENS],
    supi: [0; MAX_TOKENS],
    has_ident: false,
};

/// Fill `role1`/`role2` for every figure: the stems of the two nearest preceding
/// content words, skipping boilerplate, other figures and identifiers.
///
/// "the current share price of Apple Inc. (AAPL) is $309.25" and
/// "**Day's Range**: $307.01" both offer a currency figure, but only the first
/// is answering "what is the current share price". Comparing figures that share
/// a role is what stops a 9%-wrong price from being rescued by a 52-week high it
/// happens to sit near.
fn fill_roles(t: &mut Toks) {
    let mut i = 0usize;
    while i < t.n {
        if t.kind[i] != K_NUMBER {
            i += 1;
            continue;
        }
        let mut slot = 0u8;
        let mut j = i;
        while j > 0 && slot < 2 {
            j -= 1;
            if t.boiler[j] || t.kind[j] == K_NUMBER || t.kind[j] == K_IDENT {
                continue;
            }
            if t.stem[j] == 0 {
                continue;
            }
            if slot == 0 {
                t.role1[i] = t.stem[j];
            } else {
                t.role2[i] = t.stem[j];
            }
            slot += 1;
        }
        i += 1;
    }
}

/// Multiply a figure by the magnitude word or finance suffix that follows it.
///
/// Without this, a ground truth saying "$12.5 billion" parses as the number
/// 12.5 while an answer saying "$12,500,000,000" parses as 12_500_000_000, and
/// the two never compare — measured, a correct answer written in full digits
/// scored 0.0044 and "$12.5B" scored 0.0025 against the same ground truth.
///
/// Only UPPERCASE single letters are read as suffixes. Lowercase `m`, `k` and
/// `t` are metres, kilo- and tonnes elsewhere in the corpus, and mis-reading a
/// wind speed as a magnitude would be far worse than missing a suffix.
///
/// Called only for profiles that set `scale_words`; the intents already
/// calibrated without it are untouched.
pub fn mark_dates(t: &mut Toks) {
    const MONTHS: [&[u8]; 12] = [
        b"january",
        b"february",
        b"march",
        b"april",
        b"may",
        b"june",
        b"july",
        b"august",
        b"september",
        b"october",
        b"november",
        b"december",
    ];
    let mut month = [0u32; 12];
    let mut m = 0usize;
    while m < 12 {
        month[m] = stem_hash(MONTHS[m]);
        m += 1;
    }
    let is_month = |h: u32| {
        let mut m = 0usize;
        while m < 12 {
            if month[m] == h {
                return true;
            }
            m += 1;
        }
        false
    };
    let mut i = 0usize;
    while i < t.n {
        if t.kind[i] == K_NUMBER {
            let v = t.val[i];
            let whole = v == libm_trunc(v);
            if whole && (1900.0..=2100.0).contains(&v) {
                t.date_like[i] = true;
            } else if whole && (1.0..=31.0).contains(&v) {
                // day-of-month only when a month name is adjacent
                let before = i > 0 && t.kind[i - 1] == K_WORD && is_month(t.stem[i - 1]);
                let after = i + 1 < t.n && t.kind[i + 1] == K_WORD && is_month(t.stem[i + 1]);
                if before || after {
                    t.date_like[i] = true;
                }
            }
        }
        i += 1;
    }
}

/// `f32::trunc` is std-only; this crate is `no_std` on wasm.
fn libm_trunc(v: f32) -> f32 {
    (v as i64) as f32
}

pub fn apply_scale_words(t: &mut Toks) {
    let h_tri = stem_hash(b"trillion");
    let h_bil = stem_hash(b"billion");
    let h_mil = stem_hash(b"million");
    let h_tho = stem_hash(b"thousand");
    // Only the spelled-out magnitudes. Single letters are NOT accepted: `m` is
    // metres and `k` is the kilo- prefix elsewhere in the corpus, and reading
    // "5 m/s" as five million broke unit normalisation outright (measured — the
    // Stage-1 equivalence check between 5 m/s and 18 km/h failed).
    let h_bn = stem_hash(b"bn");
    // Currency words are not units of measure. Left alone they sit where a unit
    // would and read as an unknown category, which multiplied a correct answer
    // by the foreign-unit penalty: "12,500,000,000 dollars" scored 0.000017
    // against a truth of "$12.5 billion".
    let h_usd = stem_hash(b"USD");
    let h_dollars = stem_hash(b"dollars");
    let h_dollar = stem_hash(b"dollar");
    let mut i = 0usize;
    while i < t.n {
        if t.kind[i] != K_NUMBER {
            i += 1;
            continue;
        }
        let mut scale = 1.0f32;
        if i + 1 < t.n && t.kind[i + 1] == K_WORD {
            let h = t.stem[i + 1];
            scale = if h == h_tri {
                1e12
            } else if h == h_bil || h == h_bn {
                1e9
            } else if h == h_mil {
                1e6
            } else if h == h_tho {
                1e3
            } else {
                1.0
            };
            // The magnitude word has been folded into the figure; leaving it as
            // ordinary prose would also let it count as answer content.
            if scale != 1.0 {
                t.boiler[i + 1] = true;
            }
        }
        if scale != 1.0 {
            t.val[i] *= scale;
            t.vhi[i] *= scale;
        }
        // Neutralise a currency word following the figure (possibly past the
        // magnitude word) so it is not read as an unknown unit.
        let mut j = i + 1;
        while j < t.n && j <= i + 3 {
            if t.kind[j] == K_WORD
                && (t.stem[j] == h_usd || t.stem[j] == h_dollars || t.stem[j] == h_dollar)
            {
                t.boiler[j] = true;
                break;
            }
            j += 1;
        }
        i += 1;
    }
}

/// How much two figures share a role: 2 both stems, 1 one stem, 0 none.
/// Symmetric, and 0 on either side means "no role recorded", which callers treat
/// as "cannot tell" rather than "different".
pub fn role_overlap(ta: &Toks, i: usize, tg: &Toks, k: usize) -> u8 {
    let a = [ta.role1[i], ta.role2[i]];
    let b = [tg.role1[k], tg.role2[k]];
    let mut hits = 0u8;
    let mut x = 0usize;
    while x < 2 {
        if a[x] != 0 {
            let mut y = 0usize;
            while y < 2 {
                if a[x] == b[y] {
                    hits += 1;
                    break;
                }
                y += 1;
            }
        }
        x += 1;
    }
    hits
}

/// True when both figures carry a recorded role at all.
pub fn roles_known(ta: &Toks, i: usize, tg: &Toks, k: usize) -> bool {
    (ta.role1[i] != 0 || ta.role2[i] != 0) && (tg.role1[k] != 0 || tg.role2[k] != 0)
}

#[cfg(test)]
impl Toks {
    pub const fn new() -> Toks {
        EMPTY_TOKS
    }
}

// --------------------------------------------------------------------------
// Salience
// --------------------------------------------------------------------------

/// ~90 function words. A stopword still weighs a little, so that padding an
/// answer with them dilutes its precision denominator instead of being free.
const STOPWORDS: [u32; 92] = [
    hash_str("the"),
    hash_str("a"),
    hash_str("an"),
    hash_str("and"),
    hash_str("or"),
    hash_str("but"),
    hash_str("if"),
    hash_str("of"),
    hash_str("to"),
    hash_str("in"),
    hash_str("on"),
    hash_str("at"),
    hash_str("by"),
    hash_str("for"),
    hash_str("with"),
    hash_str("from"),
    hash_str("as"),
    hash_str("is"),
    hash_str("are"),
    hash_str("was"),
    hash_str("were"),
    hash_str("be"),
    hash_str("been"),
    hash_str("being"),
    hash_str("am"),
    hash_str("has"),
    hash_str("have"),
    hash_str("had"),
    hash_str("do"),
    hash_str("does"),
    hash_str("did"),
    hash_str("will"),
    hash_str("would"),
    hash_str("shall"),
    hash_str("should"),
    hash_str("can"),
    hash_str("could"),
    hash_str("may"),
    hash_str("might"),
    hash_str("must"),
    hash_str("this"),
    hash_str("that"),
    hash_str("these"),
    hash_str("those"),
    hash_str("it"),
    hash_str("its"),
    hash_str("they"),
    hash_str("them"),
    hash_str("their"),
    hash_str("there"),
    hash_str("here"),
    hash_str("what"),
    hash_str("which"),
    hash_str("who"),
    hash_str("whom"),
    hash_str("whose"),
    hash_str("when"),
    hash_str("where"),
    hash_str("why"),
    hash_str("how"),
    hash_str("all"),
    hash_str("any"),
    hash_str("both"),
    hash_str("each"),
    hash_str("few"),
    hash_str("more"),
    hash_str("most"),
    hash_str("some"),
    hash_str("such"),
    hash_str("than"),
    hash_str("too"),
    hash_str("very"),
    hash_str("just"),
    hash_str("also"),
    hash_str("into"),
    hash_str("over"),
    hash_str("under"),
    hash_str("about"),
    hash_str("between"),
    hash_str("during"),
    hash_str("you"),
    hash_str("your"),
    hash_str("i"),
    hash_str("we"),
    hash_str("our"),
    hash_str("he"),
    hash_str("she"),
    hash_str("his"),
    hash_str("her"),
    hash_str("not"),
    hash_str("no"),
    hash_str("s"),
];

fn is_stopword(h: u32) -> bool {
    let mut i = 0usize;
    while i < STOPWORDS.len() {
        if STOPWORDS[i] == h {
            return true;
        }
        i += 1;
    }
    false
}

/// Negators. `not` and `no` are also stopwords, so before this table they
/// weighed 0.05 out of a ~15-token pool and a sentence tied its own negation at
/// 1.0000 (adversarial review C2). Polarity is not a weighting question.
const NEGATORS: [u32; 14] = [
    hash_str("not"),
    hash_str("no"),
    hash_str("never"),
    hash_str("none"),
    hash_str("cannot"),
    hash_str("cant"),
    hash_str("wont"),
    hash_str("didnt"),
    hash_str("doesnt"),
    hash_str("isnt"),
    hash_str("arent"),
    hash_str("without"),
    hash_str("nor"),
    hash_str("neither"),
];

fn is_negator(h: u32) -> bool {
    let mut i = 0usize;
    while i < NEGATORS.len() {
        if NEGATORS[i] == h {
            return true;
        }
        i += 1;
    }
    false
}

/// How many following tokens a negator reaches over, before a clause boundary
/// closes it. "no longer" and "n't" both land here as plain negator tokens.
const NEG_WINDOW: i32 = 5;

fn weight(tok: &[u8], hash: u32, kind: u8, proper: bool, high: bool) -> f32 {
    let p = profile();
    if kind == K_NUMBER {
        return p.w_number;
    }
    if kind == K_IDENT {
        return p.w_ident;
    }
    if is_stopword(hash) {
        return p.w_stop;
    }
    if high {
        // A script we cannot segment: real content, but we cannot say how much.
        return p.w_high;
    }
    let len = if tok.len() as f32 > p.w_len_cap {
        p.w_len_cap
    } else {
        tok.len() as f32
    };
    let mut w = p.w_word_base + p.w_len_step * len;
    if proper {
        w += p.w_proper;
    }
    w
}

// --------------------------------------------------------------------------
// Tokenise
// --------------------------------------------------------------------------

pub fn tokenize(src: &[u8], t: &mut Toks) {
    t.n = 0;
    t.has_ident = false;
    let n = src.len();
    let mut i = 0usize;
    let mut negwin: i32 = 0;
    // A capital that opens a sentence says nothing about proper-noun-hood.
    let mut sentence_start = true;

    while i < n && t.n < MAX_TOKENS {
        if !is_wordbyte(src[i]) {
            // A clause boundary ends a negation's reach: in "No, the cert
            // expired" the negation applies to the verdict, not to "expired".
            let b = src[i];
            if b == b'.' || b == b',' || b == b';' || b == b'!' || b == b'?' || b == b':' {
                negwin = 0;
            }
            if b == b'.' || b == b'!' || b == b'?' {
                sentence_start = true;
            }
            i += 1;
            continue;
        }
        let start = i;
        let (mut has_alpha, mut has_digit, mut high) = (false, false, false);
        let mut seps = 0u8;

        while i < n {
            let b = src[i];
            if is_wordbyte(b) {
                if is_alpha(b) {
                    has_alpha = true;
                } else if is_digit(b) {
                    has_digit = true;
                } else {
                    high = true;
                }
                i += 1;
            } else if is_sep(b)
                && i + 1 < n
                && is_alnum(src[i - 1])
                && is_alnum(src[i + 1])
                && (has_digit || is_digit(src[i + 1]))
            {
                seps += 1;
                i += 1;
            } else {
                break;
            }
        }

        let tok = &src[start..i];
        if tok.is_empty() {
            continue;
        }

        // Classify. A leading decimal run followed by nothing or by a known unit
        // is a figure; anything else mixing letters and digits, or carrying two
        // or more internal separators, is an identifier (IP, CVE id, version,
        // date) and admits no numeric tolerance.
        let (mut val, mut vhi, used) = leading_range(tok);
        let rest = &tok[used..];
        let suffix_unit = if rest.is_empty() {
            U_NONE
        } else {
            unit_word_code(rest)
        };
        // A bare decimal carrying only a hemisphere letter is a coordinate, not
        // an identifier: `34.9011S` must parse as -34.9011 rather than falling
        // through to K_IDENT where no tolerance applies (adversarial review M5).
        let hemi = used > 0 && rest.len() == 1 && is_hemisphere(rest[0]);
        let kind = if used > 0 && (rest.is_empty() || suffix_unit != U_NONE || hemi) {
            K_NUMBER
        } else if (has_alpha && has_digit) || (has_digit && seps >= 2) {
            K_IDENT
        } else if has_digit {
            K_NUMBER
        } else {
            K_WORD
        };
        if hemi && (lower(rest[0]) == b's' || lower(rest[0]) == b'w') {
            val = -val;
            vhi = val;
        }

        // `104.8669°W` is a western longitude: negative, not positive.
        if kind == K_NUMBER && suffix_is_negative_hemisphere(rest) {
            val = -val;
            vhi = val;
        }

        // A leading `-` that is a sign rather than a hyphen. Longitudes and
        // negative wind components depend on this: -122.4194 is not 122.4194.
        if kind == K_NUMBER
            && start > 0
            && src[start - 1] == b'-'
            && (start < 2 || !is_alnum(src[start - 2]))
        {
            let span = vhi - val;
            val = -val;
            vhi = val + span;
        }

        let h = hash_bytes(tok);
        // Sentence-initial capitals are not entities. Without this, a verbose
        // answer written as several sentences ("Sustained wind ... Peak gusts
        // ... Precipitation totals ...") reads every sentence opener as a proper
        // noun the ground truth never states, and the entity channel scores a
        // wholly correct answer as a pile of contradictions.
        let proper = start > 0 && has_alpha && tok[0].is_ascii_uppercase() && !sentence_start;
        let k = t.n;

        t.hash[k] = h;
        t.stem[k] = if kind == K_WORD { stem_hash(tok) } else { h };
        t.family[k] = if kind == K_WORD {
            unit_family_hash(tok)
        } else {
            h
        };
        t.kind[k] = kind;
        t.val[k] = val;
        t.vhi[k] = if vhi >= val { vhi } else { val };
        // A partial unit (`km` awaiting an `h`) is not a unit on its own.
        t.unit[k] = if kind == K_NUMBER && suffix_unit < P_BASE {
            if hemi {
                U_DEG
            } else {
                suffix_unit
            }
        } else {
            U_NONE
        };
        t.uword[k] = unit_word_code(tok);
        t.nb[k] = if i < n { src[i] } else { 0 };

        // A finance magnitude letter glued straight onto the figure: "$12.5B",
        // "$4.51T". Uppercase only, and only when nothing alphanumeric follows,
        // so "5 m/s" and "47 km" are untouched — reading a lowercase `m` as
        // *million* previously broke unit normalisation outright. Gated to the
        // headline-quantity profiles.
        if kind == K_NUMBER && i < n && (i + 1 >= n || !is_alnum(src[i + 1])) {
            let mag = match src[i] {
                b'T' => 1e12,
                b'B' => 1e9,
                b'M' => 1e6,
                b'K' => 1e3,
                _ => 0.0,
            };
            if mag > 0.0 && profile().scale_words {
                t.val[k] *= mag;
                t.vhi[k] *= mag;
            }
        }
        t.w[k] = weight(tok, h, kind, proper, high);
        // Every per-token field is written on every push: a field left over from
        // a previous call would make the score depend on call order.
        t.decisive[k] = kind != K_WORD || proper;
        t.proper[k] = proper && kind == K_WORD;
        t.capitalized[k] = has_alpha && tok[0].is_ascii_uppercase();
        t.abbrev[k] = kind == K_WORD && all_upper(tok) && tok.len() <= 2;
        t.first[k] = if is_alpha(tok[0]) { lower(tok[0]) } else { 0 };
        t.boiler[k] = false;
        t.echo[k] = false;
        // Seed compound and standalone family names here while the original
        // bytes are available. The post-pass extends the span over versions and
        // variants such as `3.5 Sonnet` or `4o`.
        t.model[k] = crate::models::family_code(tok) != 0;
        t.supw[k] = 0.0;
        t.supi[k] = 0;
        t.ufword[k] = 0;
        t.neg[k] = negwin > 0;
        if kind == K_IDENT {
            t.has_ident = true;
        }
        // A negator opens a window over what follows; anything else inside an
        // open window counts down toward the clause it belongs to.
        if kind == K_WORD && is_negator(h) {
            negwin = NEG_WINDOW;
            t.neg[k] = true;
        } else if negwin > 0 {
            negwin -= 1;
        }
        sentence_start = false;
        t.n = k + 1;
    }
    mark_model_spans(t);
    fill_roles(t);
}

fn model_component(t: &Toks, i: usize) -> bool {
    t.model[i]
        || t.kind[i] == K_NUMBER
        || t.kind[i] == K_IDENT
        || t.proper[i]
        || crate::models::is_variant_hash(t.hash[i])
}

/// Extend family seeds and explicit `Model:` / `Attribution:` slots over their
/// version and variant components, stopping at punctuation or ordinary prose.
fn mark_model_spans(t: &mut Toks) {
    let model = hash_str("model");
    let attribution = hash_str("attribution");
    let attributed = hash_str("attributed");
    let mut i = 0usize;
    while i < t.n {
        let seeded = t.model[i];
        let marker = t.hash[i] == model || t.hash[i] == attribution || t.hash[i] == attributed;
        if seeded || marker {
            let mut k = i + 1;
            let mut used = 0usize;
            while k < t.n && used < 6 {
                if !model_component(t, k) {
                    break;
                }
                t.model[k] = true;
                used += 1;
                if is_phrase_break(t.nb[k]) {
                    break;
                }
                k += 1;
            }
        }
        i += 1;
    }
}

/// Every alphabetic byte upper-case, and there is at least one.
fn all_upper(tok: &[u8]) -> bool {
    let mut seen = false;
    let mut i = 0usize;
    while i < tok.len() {
        if is_alpha(tok[i]) {
            if !tok[i].is_ascii_uppercase() {
                return false;
            }
            seen = true;
        }
        i += 1;
    }
    seen
}

// --------------------------------------------------------------------------
// Boilerplate openers
// --------------------------------------------------------------------------

/// The measured opening-phrase histogram of `converted_answer`: 86.9% of live
/// answers open literally "The data ..." (gate analysis §4.1). These carry no
/// information about the answer, so they are struck from both sides of the
/// precision ratio rather than being allowed to inflate it.
/// Only genuinely contentless openers appear here. Four weather-specific
/// entries (`the weather forecast`, `the current weather`, `the forecast for`,
/// `the weather in`) were removed: `weather` and `forecast` are *content* words
/// for a weather intent, so striking them at position 0 scored one phrasing
/// differently from another — worth up to +0.02 on the storm build — which is a
/// phrasing match, and the Rule-04 disclosure says no phrasing is matched
/// (adversarial review M9).
const BOILER: [&[u32]; 8] = [
    &[hash_str("the"), hash_str("data"), hash_str("shows")],
    &[hash_str("the"), hash_str("data"), hash_str("provides")],
    &[hash_str("the"), hash_str("data"), hash_str("indicates")],
    &[hash_str("the"), hash_str("data"), hash_str("describes")],
    &[hash_str("this"), hash_str("data"), hash_str("shows")],
    &[hash_str("this"), hash_str("data"), hash_str("describes")],
    &[hash_str("the"), hash_str("data")],
    &[hash_str("this"), hash_str("data")],
];

/// Strike the longest matching opener from the head of the answer.
pub fn mark_boilerplate(t: &mut Toks) {
    let mut best = 0usize;
    let mut p = 0usize;
    while p < BOILER.len() {
        let phrase = BOILER[p];
        if phrase.len() > best && phrase.len() <= t.n {
            let mut j = 0usize;
            let mut ok = true;
            while j < phrase.len() {
                if t.hash[j] != phrase[j] {
                    ok = false;
                    break;
                }
                j += 1;
            }
            if ok {
                best = phrase.len();
            }
        }
        p += 1;
    }
    let mut j = 0usize;
    while j < best {
        t.boiler[j] = true;
        j += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(s: &[u8]) -> Toks {
        let mut t = Toks::new();
        tokenize(s, &mut t);
        t
    }

    #[test]
    fn identifiers_survive_tokenisation() {
        let t = toks(b"IP 192.168.1.10 and CVE-2021-44228 on 2026-08-27");
        let mut idents = 0;
        for k in 0..t.n {
            if t.kind[k] == K_IDENT {
                idents += 1;
            }
        }
        assert_eq!(idents, 3, "IP, CVE id and date must each stay one token");
        assert!(t.has_ident);
    }

    #[test]
    fn decimals_and_units_are_figures() {
        let t = toks(b"temperature 23.1C with risk 0.429 and 55%");
        let mut nums = 0;
        for k in 0..t.n {
            if t.kind[k] == K_NUMBER {
                nums += 1;
            }
        }
        assert_eq!(nums, 3);
    }

    #[test]
    fn negative_longitudes_keep_their_sign() {
        let t = toks(b"latitude 37.7749 and longitude -122.4194");
        let mut saw_neg = false;
        for k in 0..t.n {
            if t.kind[k] == K_NUMBER && t.val[k] < 0.0 {
                saw_neg = true;
            }
        }
        assert!(saw_neg, "a sign must not be dropped: -122 is not 122");
    }

    #[test]
    fn hyphenated_words_are_not_identifiers() {
        let t = toks(b"a well-known service");
        for k in 0..t.n {
            assert_ne!(t.kind[k], K_IDENT);
        }
    }

    #[test]
    fn high_bytes_never_trap_and_stay_opaque() {
        let t = toks("emoji \u{1F5FC} CJK \u{4E2D}\u{6587} accents caf\u{E9}".as_bytes());
        assert!(t.n > 0);
    }

    #[test]
    fn numbers_outweigh_stopwords_by_a_wide_margin() {
        let t = toks(b"the 10");
        assert!(t.w[1] > t.w[0] * 20.0);
    }

    #[test]
    fn boilerplate_openers_are_struck() {
        let mut t = toks(b"The data shows the IP is in Brisbane");
        mark_boilerplate(&mut t);
        assert!(t.boiler[0] && t.boiler[1] && t.boiler[2]);
        assert!(!t.boiler[3]);
    }

    #[test]
    fn truncation_is_bounded_not_a_trap() {
        // ~54 KB of repeated text, the Stage-1 adversarial case.
        let mut big = [0u8; 54 * 1024];
        let word = b"storm ";
        let mut i = 0usize;
        while i < big.len() {
            big[i] = word[i % word.len()];
            i += 1;
        }
        let t = toks(&big);
        assert!(t.n <= MAX_TOKENS);
    }
}
