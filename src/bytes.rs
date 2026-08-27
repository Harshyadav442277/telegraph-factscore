//! Byte classification, case folding, hashing, and the small float helpers.
//!
//! `core` has no `powf`/`exp`/`sqrt` (those live in `std`/libm), and importing a
//! math crate would add host imports. Every routine here is plain arithmetic, so
//! the whole scorer is transcendental-free — which is also what keeps it far
//! inside the node's 10-minute gate budget (ARCHITECTURE A2).

#![allow(dead_code)]

// --------------------------------------------------------------------------
// Byte classes. Bytes >= 0x80 are treated as opaque *word* bytes: we never
// decode UTF-8, so emoji/CJK/accented input can never trap (A1 Stage-1 trap).
// --------------------------------------------------------------------------

pub const fn lower(b: u8) -> u8 {
    if b >= b'A' && b <= b'Z' {
        b + 32
    } else {
        b
    }
}

pub const fn is_digit(b: u8) -> bool {
    b >= b'0' && b <= b'9'
}

pub const fn is_alpha(b: u8) -> bool {
    (b >= b'a' && b <= b'z') || (b >= b'A' && b <= b'Z')
}

pub const fn is_alnum(b: u8) -> bool {
    is_digit(b) || is_alpha(b)
}

/// Word byte: ASCII alphanumeric, or any high byte (opaque multi-byte script).
pub const fn is_wordbyte(b: u8) -> bool {
    is_alnum(b) || b >= 0x80
}

/// ASCII whitespace only. The Stage-1 whitespace test is exact equality to 0,
/// so this must match the host's notion of blank (space/tab/CR/LF/VT/FF).
pub const fn is_space(b: u8) -> bool {
    b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' || b == 0x0b || b == 0x0c
}

/// Separators that may sit *inside* a token when both neighbours are alphanumeric
/// and a digit is involved: `192.168.1.10`, `CVE-2021-44228`, `142.250.0.0/15`,
/// `2026-08-27`, `1,000`, `3.14`.
pub const fn is_sep(b: u8) -> bool {
    b == b'.' || b == b',' || b == b'-' || b == b':' || b == b'/' || b == b'_'
}

// --------------------------------------------------------------------------
// FNV-1a over case-folded bytes. `const fn` so the stopword / boilerplate /
// unit tables are compile-time constants rather than runtime initialisation.
// --------------------------------------------------------------------------

const FNV_OFFSET: u32 = 0x811c_9dc5;
const FNV_PRIME: u32 = 0x0100_0193;

pub const fn hash_bytes(s: &[u8]) -> u32 {
    let mut h = FNV_OFFSET;
    let mut i = 0usize;
    while i < s.len() {
        h ^= lower(s[i]) as u32;
        h = h.wrapping_mul(FNV_PRIME);
        i += 1;
    }
    // 0 is reserved as "absent" in a few places; nudge it off.
    if h == 0 {
        1
    } else {
        h
    }
}

pub const fn hash_str(s: &str) -> u32 {
    hash_bytes(s.as_bytes())
}

/// Hash of the token with one common English suffix removed, so `provides` and
/// `provide`, or `ranges` and `range`, land on the same key. Deliberately crude:
/// a real stemmer is not worth the bytes, and over-stemming only ever costs a
/// little precision on a low-weight word.
pub fn stem_hash(tok: &[u8]) -> u32 {
    let n = tok.len();
    if n < 5 {
        return hash_bytes(tok);
    }
    let cut = |k: usize| -> u32 { hash_bytes(&tok[..n - k]) };
    let e = |k: usize, s: &[u8]| -> bool {
        if n <= k {
            return false;
        }
        let mut i = 0usize;
        while i < k {
            if lower(tok[n - k + i]) != s[i] {
                return false;
            }
            i += 1;
        }
        true
    };
    if n > 6 && e(3, b"ing") {
        return cut(3);
    }
    if n > 5 && e(2, b"ed") {
        return cut(2);
    }
    if n > 5 && e(2, b"ly") {
        return cut(2);
    }
    // Plain plural only. Stripping "es" as a unit would send `provides` to
    // `provid` while `provide` stays put, so the two would never meet.
    if e(1, b"s") && !e(2, b"ss") {
        return cut(1);
    }
    hash_bytes(tok)
}

/// Case- and punctuation-insensitive equality. Drives the exact-match shortcut
/// that pins `rank_answer(q, gt, gt)` to exactly 1.0 (A8 self-match ratchet).
pub fn normalized_equal(a: &[u8], b: &[u8]) -> bool {
    let (mut i, mut j) = (0usize, 0usize);
    loop {
        while i < a.len() && !is_wordbyte(a[i]) {
            i += 1;
        }
        while j < b.len() && !is_wordbyte(b[j]) {
            j += 1;
        }
        if i >= a.len() || j >= b.len() {
            return i >= a.len() && j >= b.len();
        }
        if lower(a[i]) != lower(b[j]) {
            return false;
        }
        i += 1;
        j += 1;
    }
}

/// True when every byte is ASCII whitespace (or the slice is empty).
pub fn is_blank(s: &[u8]) -> bool {
    let mut i = 0usize;
    while i < s.len() {
        if !is_space(s[i]) {
            return false;
        }
        i += 1;
    }
    true
}

// --------------------------------------------------------------------------
// Float helpers (core has no f32::abs / max / min without std).
// --------------------------------------------------------------------------

pub fn fabs(x: f32) -> f32 {
    if x < 0.0 {
        -x
    } else {
        x
    }
}

pub fn fmax(a: f32, b: f32) -> f32 {
    if a > b {
        a
    } else {
        b
    }
}

pub fn fmin(a: f32, b: f32) -> f32 {
    if a < b {
        a
    } else {
        b
    }
}

pub fn clamp01(x: f32) -> f32 {
    // Also collapses NaN to 0: every comparison with NaN is false, so the final
    // `else` arm is reached. The host does this too, but doing it here keeps the
    // breakdown export honest.
    if x > 1.0 {
        1.0
    } else if x > 0.0 {
        x
    } else {
        0.0
    }
}

/// Hermite smoothstep on [0,1]. Continuity, not cliffs (ARCHITECTURE A3.7).
pub fn smoothstep01(x: f32) -> f32 {
    let t = clamp01(x);
    t * t * (3.0 - 2.0 * t)
}

/// Smoothstep with knots: rescale [lo,hi] onto [0,1], then smooth. `hi <= lo`
/// degenerates to a threshold, so callers must keep the knots ordered.
pub fn smoothstep(lo: f32, hi: f32, x: f32) -> f32 {
    if hi <= lo {
        return if x >= hi { 1.0 } else { 0.0 };
    }
    smoothstep01((x - lo) / (hi - lo))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashing_is_case_insensitive() {
        assert_eq!(hash_str("Google"), hash_str("google"));
        assert_ne!(hash_str("google"), hash_str("gaggle"));
    }

    #[test]
    fn stemming_folds_common_suffixes() {
        assert_eq!(stem_hash(b"provides"), stem_hash(b"provide"));
        assert_eq!(stem_hash(b"ranges"), stem_hash(b"range"));
    }

    #[test]
    fn normalized_equal_ignores_case_and_punctuation() {
        assert!(normalized_equal(b"The IP is 1.2.3.4.", b"the ip is 1234"));
        assert!(!normalized_equal(b"valid", b"invalid"));
        assert!(normalized_equal(b"", b"   "));
    }

    #[test]
    fn blank_detection_matches_stage1() {
        assert!(is_blank(b""));
        assert!(is_blank(b" \t\r\n "));
        assert!(!is_blank(b"  x  "));
    }

    #[test]
    fn smoothstep_pins_the_endpoints() {
        // Self-match must survive calibration at exactly 1.0.
        assert_eq!(smoothstep01(1.0), 1.0);
        assert_eq!(smoothstep01(0.0), 0.0);
        assert_eq!(smoothstep(0.05, 0.80, 1.0), 1.0);
        assert_eq!(smoothstep(0.05, 0.80, 0.0), 0.0);
        // Monotone in between, and never a step.
        let a = smoothstep(0.05, 0.80, 0.3);
        let b = smoothstep(0.05, 0.80, 0.5);
        assert!(a > 0.0 && b > a && b < 1.0);
    }

    #[test]
    fn clamp01_collapses_nan() {
        assert_eq!(clamp01(f32::NAN), 0.0);
        assert_eq!(clamp01(f32::INFINITY), 1.0);
        assert_eq!(clamp01(-1.0), 0.0);
    }
}
