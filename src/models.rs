//! Semantic handling for model names.
//!
//! Model identifiers are unusually hostile to a generic tokenizer: `GPT-4o`,
//! `GPT 4o`, and `GPT4o` are the same claim but become three different token
//! shapes.  This module extracts a small, separator-insensitive signature from
//! common model-family syntax.  It does not alias versions or variants: a
//! change from `Gemini 1.5 Pro` to `Gemini 2.0 Pro` remains a contradiction.

use crate::bytes::{is_alnum, is_alpha, is_digit, is_space, lower};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelClaim {
    family: u8,
    signature: u32,
    negated: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relation {
    Same,
    Different,
    Unavailable,
}

const FAMILIES: [&[u8]; 13] = [
    b"gpt",
    b"claude",
    b"gemini",
    b"llama",
    b"mistral",
    b"mixtral",
    b"grok",
    b"qwen",
    b"deepseek",
    b"phi",
    b"falcon",
    b"command",
    b"cohere",
];

fn eq_ascii(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0usize;
    while i < a.len() {
        if lower(a[i]) != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// Return the family code and the byte immediately after it when `src[start..]`
/// begins with a model family. A digit may immediately follow (`GPT4o`), but an
/// alphabetic continuation may not (`gptish` is ordinary prose).
fn family_at(src: &[u8], start: usize) -> Option<(u8, usize)> {
    let mut f = 0usize;
    while f < FAMILIES.len() {
        let name = FAMILIES[f];
        if start + name.len() <= src.len()
            && eq_ascii(&src[start..start + name.len()], name)
            && (start + name.len() == src.len() || !is_alpha(src[start + name.len()]))
        {
            return Some((f as u8 + 1, start + name.len()));
        }
        f += 1;
    }
    None
}

/// Used by the tokenizer to seed a model-name span, including a compound token
/// such as `Claude-3.5-Sonnet`.
pub fn family_code(tok: &[u8]) -> u8 {
    match family_at(tok, 0) {
        Some((family, _)) => family,
        None => 0,
    }
}

fn hash_push(mut h: u32, b: u8) -> u32 {
    h ^= lower(b) as u32;
    h.wrapping_mul(16_777_619)
}

fn is_terminal(b: u8) -> bool {
    b == b',' || b == b';' || b == b':' || b == b'!' || b == b'?'
}

fn is_stop_word(word: &[u8]) -> bool {
    const STOP: [&[u8]; 22] = [
        b"and",
        b"as",
        b"at",
        b"because",
        b"but",
        b"by",
        b"confidence",
        b"detected",
        b"from",
        b"generated",
        b"in",
        b"is",
        b"likely",
        b"output",
        b"probability",
        b"score",
        b"text",
        b"the",
        b"was",
        b"were",
        b"with",
        b"without",
    ];
    let mut i = 0usize;
    while i < STOP.len() {
        if eq_ascii(word, STOP[i]) {
            return true;
        }
        i += 1;
    }
    false
}

fn is_ignored_word(word: &[u8]) -> bool {
    eq_ascii(word, b"model") || eq_ascii(word, b"version") || eq_ascii(word, b"family")
}

fn is_negator(word: &[u8]) -> bool {
    eq_ascii(word, b"no")
        || eq_ascii(word, b"not")
        || eq_ascii(word, b"never")
        || eq_ascii(word, b"without")
}

/// Whether the nearby clause denies the attribution. The window is lexical,
/// bounded, and reset by punctuation, so `not Claude; GPT-4` does not leak the
/// first clause's polarity into the second.
fn negated_before(src: &[u8], end: usize) -> bool {
    let floor = end.saturating_sub(64);
    let mut clause = floor;
    let mut i = floor;
    while i < end {
        if is_terminal(src[i]) || src[i] == b'.' {
            clause = i + 1;
        }
        i += 1;
    }
    let mut last = [[0u8; 12]; 5];
    let mut lens = [0usize; 5];
    let mut count = 0usize;
    i = clause;
    while i < end {
        while i < end && !is_alnum(src[i]) {
            i += 1;
        }
        if i >= end {
            break;
        }
        let slot = count % last.len();
        lens[slot] = 0;
        while i < end && is_alnum(src[i]) {
            if lens[slot] < last[slot].len() {
                last[slot][lens[slot]] = lower(src[i]);
                lens[slot] += 1;
            }
            i += 1;
        }
        count += 1;
    }
    let available = if count < last.len() {
        count
    } else {
        last.len()
    };
    let mut back = 0usize;
    while back < available {
        let index = (count - 1 - back) % last.len();
        if is_negator(&last[index][..lens[index]]) {
            // `not only GPT-4` is additive emphasis, not denial.
            if back == 1 {
                let next = (count - back) % last.len();
                if eq_ascii(&last[next][..lens[next]], b"only") {
                    return false;
                }
            }
            return true;
        }
        back += 1;
    }
    false
}

/// Extract the first explicit, recognised model claim.
///
/// The signature keeps every alphanumeric version/variant component while
/// discarding only presentation separators. Thus punctuation-only aliases
/// compare equal, while family, version, size, and variant substitutions do not.
pub fn claim(src: &[u8]) -> Option<ModelClaim> {
    let mut start = 0usize;
    while start < src.len() {
        let boundary = start == 0 || !is_alnum(src[start - 1]);
        if boundary {
            if let Some((family, mut i)) = family_at(src, start) {
                let mut h = 2_166_136_261u32;
                h = hash_push(h, family);
                let mut components = 0usize;
                while i < src.len() && components < 6 {
                    let b = src[i];
                    if is_terminal(b) {
                        break;
                    }
                    if b == b'.' && (i + 1 == src.len() || !is_digit(src[i + 1])) {
                        break;
                    }
                    if is_space(b) || b == b'-' || b == b'_' || b == b'/' || b == b'.' {
                        i += 1;
                        continue;
                    }
                    if !is_alnum(b) {
                        break;
                    }
                    let word_start = i;
                    while i < src.len() && is_alnum(src[i]) {
                        i += 1;
                    }
                    let word = &src[word_start..i];
                    if is_stop_word(word) {
                        break;
                    }
                    if is_ignored_word(word) {
                        continue;
                    }
                    let mut k = 0usize;
                    while k < word.len() {
                        h = hash_push(h, word[k]);
                        k += 1;
                    }
                    components += 1;
                }
                return Some(ModelClaim {
                    family,
                    signature: h,
                    negated: negated_before(src, start),
                });
            }
        }
        start += 1;
    }
    None
}

pub fn relation(answer: Option<ModelClaim>, truth: Option<ModelClaim>) -> Relation {
    match (answer, truth) {
        (Some(a), Some(g))
            if a.family == g.family && a.signature == g.signature && a.negated == g.negated =>
        {
            Relation::Same
        }
        (Some(_), Some(_)) => Relation::Different,
        _ => Relation::Unavailable,
    }
}

/// Alphabetic model-name continuations that should stay inside the semantic
/// span. Numeric and identifier continuations are admitted by the caller.
pub fn is_variant_hash(h: u32) -> bool {
    const VARIANTS: [u32; 16] = [
        crate::bytes::hash_str("chat"),
        crate::bytes::hash_str("coder"),
        crate::bytes::hash_str("flash"),
        crate::bytes::hash_str("haiku"),
        crate::bytes::hash_str("instruct"),
        crate::bytes::hash_str("latest"),
        crate::bytes::hash_str("mini"),
        crate::bytes::hash_str("opus"),
        crate::bytes::hash_str("preview"),
        crate::bytes::hash_str("pro"),
        crate::bytes::hash_str("reasoning"),
        crate::bytes::hash_str("sonnet"),
        crate::bytes::hash_str("thinking"),
        crate::bytes::hash_str("turbo"),
        crate::bytes::hash_str("ultra"),
        crate::bytes::hash_str("vision"),
    ];
    let mut i = 0usize;
    while i < VARIANTS.len() {
        if VARIANTS[i] == h {
            return true;
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same(a: &[u8], b: &[u8]) -> bool {
        relation(claim(a), claim(b)) == Relation::Same
    }

    #[test]
    fn punctuation_and_spacing_do_not_change_model_identity() {
        assert!(same(b"GPT-4", b"GPT4"));
        assert!(same(b"GPT-4o", b"GPT 4o"));
        assert!(same(b"Claude 3.5 Sonnet", b"Claude-3.5-Sonnet"));
        assert!(same(b"Llama 3.1 70B", b"Llama-3.1-70B"));
    }

    #[test]
    fn versions_variants_and_families_remain_distinct() {
        assert_eq!(
            relation(claim(b"Gemini 1.5 Pro"), claim(b"Gemini 2.0 Pro")),
            Relation::Different
        );
        assert_eq!(
            relation(claim(b"Llama 3.1 70B"), claim(b"Llama 3.1 8B")),
            Relation::Different
        );
        assert_eq!(
            relation(claim(b"GPT-4"), claim(b"Claude 4")),
            Relation::Different
        );
    }

    #[test]
    fn attribution_polarity_is_part_of_the_claim() {
        assert_eq!(
            relation(
                claim(b"not attributable to GPT-4"),
                claim(b"attributed to GPT4")
            ),
            Relation::Different
        );
        assert!(same(b"not attributable to GPT-4", b"not generated by GPT4"));
        assert!(same(
            b"not detected; generated by GPT-4",
            b"generated by GPT4"
        ));
    }

    #[test]
    fn prose_prefixes_are_not_model_families() {
        assert_eq!(claim(b"The response was gptish but named no model."), None);
    }
}
