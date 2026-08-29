//! Polar verdict terms: word pairs that assert opposite findings.
//!
//! Why this exists. `polarity_of` catches a *negation* — "is located" against
//! "is **not** located" — because the two share a token and differ in `neg`.
//! It cannot catch an **antonym**, where the flip is carried by a different word
//! entirely. On any intent whose answer is a one-word verdict that is the whole
//! finding, and the miss is nearly free: a verdict term is a lowercase common
//! word, so it is neither an entity nor a figure and falls through to prose,
//! which carries `prose_w = 0.02`.
//!
//! Measured on CONTENT_VERIFICATION clean pairs before this table: flipping
//! "plagiarised" to "original" and changing nothing else scored **0.9999**
//! against a verbatim-correct 1.0000 — the exact inversion class this project
//! criticises the incumbent for, in our own module.
//!
//! Scope, deliberately narrow. These are general English polar pairs used as
//! verdicts across many intents (authenticity, validity, safety, liveness), not
//! a fixture list and not a fact about any miner: the table would be the same if
//! every hidden benchmark were replaced tomorrow, which is the legitimacy test
//! in A4. Comparatives, hedges and domain jargon are out of scope — a term that
//! is merely *different* is already handled as an unsupported token.

use crate::bytes::hash_str;

/// Semantic pole shared by equivalent verdict words.
///
/// Genuineness, originality, integrity, and authorship are deliberately
/// separate. The previous table equated `original` with `authentic` and
/// `genuine`; that makes an authentic copy indistinguishable from an original
/// manuscript, even though the canonical intent names those as different
/// questions.
fn class(h: u32) -> u8 {
    const CLASSES: [(&[u32], u8); 10] = [
        (
            &[
                hash_str("ai"),
                hash_str("machine"),
                hash_str("synthetic"),
                hash_str("automated"),
                hash_str("automatic"),
                hash_str("automatically"),
                hash_str("algorithmic"),
                hash_str("algorithmically"),
            ],
            1,
        ),
        (
            &[
                hash_str("human"),
                hash_str("person"),
                hash_str("manual"),
                hash_str("manually"),
            ],
            2,
        ),
        (
            &[
                hash_str("original"),
                hash_str("independent"),
                hash_str("independently"),
                hash_str("unique"),
                hash_str("uniquely"),
            ],
            3,
        ),
        (
            &[
                hash_str("plagiarised"),
                hash_str("plagiarized"),
                hash_str("copied"),
                hash_str("duplicate"),
                hash_str("duplicated"),
                hash_str("reproduced"),
                hash_str("lifted"),
            ],
            4,
        ),
        (
            &[
                hash_str("authentic"),
                hash_str("genuine"),
                hash_str("real"),
                hash_str("legitimate"),
                hash_str("bona"),
                hash_str("fide"),
            ],
            5,
        ),
        (
            &[
                hash_str("fake"),
                hash_str("forged"),
                hash_str("fabricated"),
                hash_str("counterfeit"),
                hash_str("inauthentic"),
                hash_str("falsely"),
                hash_str("falsified"),
                hash_str("fraudulent"),
                hash_str("hoax"),
                hash_str("bogus"),
                hash_str("spurious"),
            ],
            6,
        ),
        (
            &[
                hash_str("unaltered"),
                hash_str("intact"),
                hash_str("unchanged"),
                hash_str("unmodified"),
                hash_str("pristine"),
            ],
            7,
        ),
        (
            &[
                hash_str("altered"),
                hash_str("modified"),
                hash_str("tampered"),
                hash_str("tampering"),
                hash_str("manipulated"),
                hash_str("edited"),
                hash_str("editing"),
                hash_str("doctored"),
                hash_str("redacted"),
                hash_str("revised"),
            ],
            8,
        ),
        (
            &[
                hash_str("verified"),
                hash_str("confirmed"),
                hash_str("authenticated"),
                hash_str("proven"),
            ],
            9,
        ),
        (
            &[
                hash_str("unverified"),
                hash_str("inconclusive"),
                hash_str("unknown"),
                hash_str("uncertain"),
                hash_str("undetermined"),
                hash_str("unconfirmed"),
                hash_str("unproven"),
            ],
            10,
        ),
    ];
    let mut i = 0usize;
    while i < CLASSES.len() {
        let (words, value) = CLASSES[i];
        let mut k = 0usize;
        while k < words.len() {
            if words[k] == h {
                return value;
            }
            k += 1;
        }
        i += 1;
    }
    0
}

/// Case-folded FNV-1a hashes of polar pairs. Order within a pair is irrelevant;
/// the lookup tests both directions.
static AXES: [(u32, u32); 19] = [
    (hash_str("copied"), hash_str("original")),
    (hash_str("fake"), hash_str("genuine")),
    (hash_str("altered"), hash_str("intact")),
    (hash_str("ai"), hash_str("human")),
    (hash_str("verified"), hash_str("unverified")),
    (hash_str("valid"), hash_str("invalid")),
    (hash_str("trusted"), hash_str("untrusted")),
    (hash_str("safe"), hash_str("malicious")),
    (hash_str("safe"), hash_str("unsafe")),
    (hash_str("clean"), hash_str("infected")),
    (hash_str("expired"), hash_str("current")),
    (hash_str("revoked"), hash_str("active")),
    (hash_str("reachable"), hash_str("unreachable")),
    (hash_str("online"), hash_str("offline")),
    (hash_str("true"), hash_str("false")),
    (hash_str("accurate"), hash_str("inaccurate")),
    (hash_str("correct"), hash_str("incorrect")),
    (hash_str("consistent"), hash_str("inconsistent")),
    // Hyphenated verdicts tokenise apart, so the axis must exist at the
    // component level too: "AI-generated" against "human-written" shares no
    // whole-token pair and scored 0.9999 until these were added.
    (hash_str("generated"), hash_str("written")),
];

/// True when `a` and `b` are opposite verdicts on the same axis.
pub fn opposes(a: u32, b: u32) -> bool {
    let mut i = 0usize;
    while i < AXES.len() {
        let (x, y) = AXES[i];
        if (strongly_equivalent(a, x) && strongly_equivalent(b, y))
            || (strongly_equivalent(a, y) && strongly_equivalent(b, x))
        {
            return true;
        }
        i += 1;
    }
    false
}

/// Opposite after allowing the narrow originality/genuineness shorthand used
/// by single-label answers. This is for support only; contradiction detection
/// uses `opposes` so independent axes cannot erase one another.
pub fn broadly_opposes(a: u32, b: u32) -> bool {
    let mut i = 0usize;
    while i < AXES.len() {
        let (x, y) = AXES[i];
        if (equivalent(a, x) && equivalent(b, y)) || (equivalent(a, y) && equivalent(b, x)) {
            return true;
        }
        i += 1;
    }
    false
}

/// True when two closed-set verdict words state the same finding.
pub fn equivalent(a: u32, b: u32) -> bool {
    if strongly_equivalent(a, b) {
        return true;
    }
    // In a single closed-set verdict, users commonly use "genuine" as broad
    // shorthand for "original". Keep that useful equivalence for terse labels,
    // but not inside `opposes`: the two remain distinct semantic axes when a
    // ground truth states both (for example, an authentic copy).
    matches!((class(a), class(b)), (3, 5) | (5, 3))
}

/// Same semantic pole without cross-axis shorthand.
pub fn strongly_equivalent(a: u32, b: u32) -> bool {
    if a == b {
        return true;
    }
    let ca = class(a);
    ca != 0 && ca == class(b)
}

/// True when `h` names a verdict on any axis — used to decide whether an
/// unsupported token is a *claim* worth checking against the ground truth.
pub fn is_verdict(h: u32) -> bool {
    let mut i = 0usize;
    while i < AXES.len() {
        if AXES[i].0 == h || AXES[i].1 == h {
            return true;
        }
        i += 1;
    }
    class(h) != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytes::hash_str;

    #[test]
    fn opposites_are_symmetric() {
        assert!(opposes(hash_str("plagiarised"), hash_str("original")));
        assert!(opposes(hash_str("original"), hash_str("plagiarised")));
        assert!(opposes(hash_str("valid"), hash_str("invalid")));
        assert!(opposes(hash_str("machine"), hash_str("person")));
        assert!(!opposes(hash_str("fake"), hash_str("original")));
        assert!(opposes(hash_str("fake"), hash_str("authentic")));
        assert!(opposes(hash_str("tampered"), hash_str("unaltered")));
        assert!(opposes(hash_str("doctored"), hash_str("pristine")));
        assert!(opposes(hash_str("automated"), hash_str("manual")));
        assert!(opposes(hash_str("unconfirmed"), hash_str("proven")));
    }

    #[test]
    fn authenticity_equivalents_are_symmetric() {
        assert!(equivalent(hash_str("ai"), hash_str("machine")));
        assert!(equivalent(hash_str("machine"), hash_str("ai")));
        assert!(equivalent(hash_str("original"), hash_str("genuine")));
        assert!(!strongly_equivalent(
            hash_str("original"),
            hash_str("genuine")
        ));
        assert!(equivalent(hash_str("authentic"), hash_str("real")));
        assert!(!equivalent(hash_str("ai"), hash_str("human")));
    }

    #[test]
    fn unrelated_words_do_not_oppose() {
        assert!(!opposes(hash_str("plagiarised"), hash_str("tokyo")));
        assert!(!opposes(hash_str("original"), hash_str("original")));
        assert!(!opposes(hash_str("google"), hash_str("cloudflare")));
    }

    #[test]
    fn verdict_membership() {
        assert!(is_verdict(hash_str("authentic")));
        assert!(!is_verdict(hash_str("mumbai")));
    }
}
