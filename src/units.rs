//! Units: parsing figures, naming units, and normalising them.
//!
//! Two figures are only comparable inside one dimension. That single rule is
//! what stops a wrong "over the next 48 hours" from matching a ground-truth
//! gust of 47.3 km/h merely because the digits are close.

#![allow(dead_code)]

use crate::bytes::*;
use crate::tokens::{Toks, K_NUMBER};

// --------------------------------------------------------------------------
// Unit classes
// --------------------------------------------------------------------------

pub const U_NONE: u8 = 0;
pub const U_TEMP_C: u8 = 1;
pub const U_TEMP_F: u8 = 2;
pub const U_MS: u8 = 3;
pub const U_KMH: u8 = 4;
pub const U_KT: u8 = 5;
pub const U_MPH: u8 = 6;
pub const U_PCT: u8 = 7;
pub const U_DEG: u8 = 8;
pub const U_MM: u8 = 9;
pub const U_CM: u8 = 10;
pub const U_M: u8 = 11;
pub const U_KM: u8 = 12;
pub const U_IN: u8 = 13;
pub const U_FT: u8 = 14;
pub const U_SEC: u8 = 15;
pub const U_MIN: u8 = 16;
pub const U_HOUR: u8 = 17;
pub const U_DAY: u8 = 18;

/// Dimension of a unit. Two figures are only comparable within one dimension —
/// a temperature and a wind speed are not a near-miss, they are unrelated.
/// This is also what keeps a wrong "next 48 hours" from quietly matching a
/// ground-truth gust of 47.3 km/h just because the digits are close.
pub const D_NONE: u8 = 0;
pub const D_TEMP: u8 = 1;
pub const D_SPEED: u8 = 2;
pub const D_FRAC: u8 = 3;
pub const D_ANGLE: u8 = 4;
pub const D_LEN: u8 = 5;
pub const D_TIME: u8 = 6;

pub fn dimension(u: u8) -> u8 {
    match u {
        U_TEMP_C | U_TEMP_F => D_TEMP,
        U_MS | U_KMH | U_KT | U_MPH => D_SPEED,
        U_PCT => D_FRAC,
        U_DEG => D_ANGLE,
        U_MM | U_CM | U_M | U_KM | U_IN | U_FT => D_LEN,
        U_SEC | U_MIN | U_HOUR | U_DAY => D_TIME,
        _ => D_NONE,
    }
}

/// Convert to the canonical unit of the dimension: °C, m/s, fraction, degrees,
/// metres, seconds.
pub fn canonical(v: f32, u: u8) -> f32 {
    match u {
        U_TEMP_F => (v - 32.0) / 1.8,
        U_KMH => v / 3.6,
        U_KT => v * 0.514_444,
        U_MPH => v * 0.447_04,
        U_PCT => v / 100.0,
        U_MM => v / 1000.0,
        U_CM => v / 100.0,
        U_KM => v * 1000.0,
        U_IN => v * 0.0254,
        U_FT => v * 0.3048,
        U_MIN => v * 60.0,
        U_HOUR => v * 3600.0,
        U_DAY => v * 86400.0,
        _ => v,
    }
}

/// Partial units: a unit word that only names a unit together with the next
/// token, because `km/h` and `m/s` tokenise apart when no digit touches the `/`.
/// Kept above every real unit code so one comparison separates the two kinds.
pub const P_BASE: u8 = 64;
pub const P_KM: u8 = 64;
pub const P_M: u8 = 65;
pub const P_S: u8 = 66;
pub const P_H: u8 = 67;

const UNIT_TABLE: [(u32, u8); 51] = [
    (hash_str("c"), U_TEMP_C),
    (hash_bytes(&[0xC2, 0xB0, b'c']), U_TEMP_C),
    (hash_str("celsius"), U_TEMP_C),
    (hash_str("degc"), U_TEMP_C),
    (hash_str("f"), U_TEMP_F),
    (hash_bytes(&[0xC2, 0xB0, b'f']), U_TEMP_F),
    (hash_str("fahrenheit"), U_TEMP_F),
    (hash_str("ms"), U_MS),
    (hash_str("mps"), U_MS),
    (hash_str("m/s"), U_MS),
    (hash_str("kmh"), U_KMH),
    (hash_str("kph"), U_KMH),
    (hash_str("km/h"), U_KMH),
    (hash_str("kt"), U_KT),
    (hash_str("kts"), U_KT),
    (hash_str("knot"), U_KT),
    (hash_str("knots"), U_KT),
    (hash_str("mph"), U_MPH),
    (hash_str("mi/h"), U_MPH),
    (hash_str("percent"), U_PCT),
    (hash_str("pct"), U_PCT),
    (hash_bytes(&[0xC2, 0xB0]), U_DEG),
    (hash_str("deg"), U_DEG),
    (hash_str("degree"), U_DEG),
    (hash_str("degrees"), U_DEG),
    // Hemisphere-suffixed coordinates: `37.75°N`, `104.8669°W`.
    (hash_bytes(&[0xC2, 0xB0, b'n']), U_DEG),
    (hash_bytes(&[0xC2, 0xB0, b's']), U_DEG),
    (hash_bytes(&[0xC2, 0xB0, b'e']), U_DEG),
    (hash_bytes(&[0xC2, 0xB0, b'w']), U_DEG),
    (hash_str("km"), P_KM),
    (hash_str("h"), P_H),
    (hash_str("mm"), U_MM),
    (hash_str("cm"), U_CM),
    (hash_str("inch"), U_IN),
    (hash_str("inches"), U_IN),
    (hash_str("ft"), U_FT),
    (hash_str("feet"), U_FT),
    (hash_str("sec"), U_SEC),
    (hash_str("secs"), U_SEC),
    (hash_str("second"), U_SEC),
    (hash_str("seconds"), U_SEC),
    (hash_str("min"), U_MIN),
    (hash_str("mins"), U_MIN),
    (hash_str("minute"), U_MIN),
    (hash_str("minutes"), U_MIN),
    (hash_str("hr"), U_HOUR),
    (hash_str("hrs"), U_HOUR),
    (hash_str("hour"), U_HOUR),
    (hash_str("hours"), U_HOUR),
    (hash_str("day"), U_DAY),
    (hash_str("days"), U_DAY),
];

/// Extra partials that collide with very common words, kept out of the main
/// table so `m` and `s` are only ever read as units directly after a figure.
const PARTIAL_TABLE: [(u32, u8); 2] = [(hash_str("m"), P_M), (hash_str("s"), P_S)];

/// Unit (or partial unit) named by this byte run, if it names one at all.
pub fn unit_of(s: &[u8]) -> Option<u8> {
    if s.is_empty() {
        return None;
    }
    let h = hash_bytes(s);
    let mut i = 0usize;
    while i < UNIT_TABLE.len() {
        if UNIT_TABLE[i].0 == h {
            return Some(UNIT_TABLE[i].1);
        }
        i += 1;
    }
    let mut j = 0usize;
    while j < PARTIAL_TABLE.len() {
        if PARTIAL_TABLE[j].0 == h {
            return Some(PARTIAL_TABLE[j].1);
        }
        j += 1;
    }
    None
}

/// Unit code carried by a whole token, precomputed at tokenise time so the
/// second pass never needs the original bytes back.
pub fn unit_word_code(s: &[u8]) -> u8 {
    match unit_of(s) {
        Some(u) => u,
        None => U_NONE,
    }
}

/// A `°S` / `°W` suffix makes the coordinate negative.
pub fn suffix_is_negative_hemisphere(rest: &[u8]) -> bool {
    if rest.len() < 2 {
        return false;
    }
    let last = lower(rest[rest.len() - 1]);
    (last == b's' || last == b'w') && rest[0] == 0xC2
}

/// Parse the leading decimal run of a token. Returns the value and how many
/// bytes it consumed; `,` is accepted as a thousands separator between digits
/// and at most one `.` is taken as the decimal point.
pub fn leading_number(tok: &[u8]) -> (f32, usize) {
    let n = tok.len();
    let mut i = 0usize;
    let mut int_part: f32 = 0.0;
    let mut seen_digit = false;
    while i < n {
        let b = tok[i];
        if is_digit(b) {
            int_part = int_part * 10.0 + (b - b'0') as f32;
            seen_digit = true;
            i += 1;
        } else if b == b',' && seen_digit && i + 1 < n && is_digit(tok[i + 1]) {
            i += 1;
        } else {
            break;
        }
    }
    if !seen_digit {
        return (0.0, 0);
    }
    // Optional single fractional part.
    if i + 1 < n && tok[i] == b'.' && is_digit(tok[i + 1]) {
        let mut j = i + 1;
        let mut frac: f32 = 0.0;
        let mut scale: f32 = 1.0;
        while j < n && is_digit(tok[j]) {
            frac = frac * 10.0 + (tok[j] - b'0') as f32;
            scale *= 10.0;
            j += 1;
        }
        // Only a genuine terminator makes this a decimal; `2.14.1` is a version.
        if j >= n || !is_sep(tok[j]) {
            return (int_part + frac / scale, j);
        }
    }
    (int_part, i)
}

/// Second pass over the token stream: attach units that live in the *next*
/// token(s) rather than in the figure itself (`5.9 km/h` tokenises as
/// `5.9`,`km`,`h`), and read a trailing `%` off the source byte after the token.
pub fn annotate_units(t: &mut Toks) {
    let mut k = 0usize;
    while k < t.n {
        if t.kind[k] != K_NUMBER {
            k += 1;
            continue;
        }
        if t.unit[k] == U_NONE {
            if t.nb[k] == b'%' {
                t.unit[k] = U_PCT;
            } else if k + 1 < t.n {
                let u1 = t.uword[k + 1];
                if u1 != U_NONE && u1 < P_BASE {
                    t.unit[k] = u1;
                } else if u1 >= P_BASE {
                    // "km" + "h", "m" + "s": a unit split by a `/` the tokeniser
                    // did not absorb because no digit touched it. Unjoined, the
                    // partial still names a unit of its own — `km` is a distance
                    // and `h` is an hour.
                    let u2 = if k + 2 < t.n { t.uword[k + 2] } else { U_NONE };
                    t.unit[k] = match (u1, u2) {
                        (P_KM, P_H) => U_KMH,
                        (P_M, P_S) => U_MS,
                        (P_KM, _) => U_KM,
                        (P_M, _) => U_M,
                        (P_H, _) => U_HOUR,
                        (P_S, _) => U_SEC,
                        _ => U_NONE,
                    };
                }
            }
        }
        // A hemisphere letter written apart from the figure: `22.5609° S`.
        if t.unit[k] == U_DEG && k + 1 < t.n {
            let h = t.hash[k + 1];
            if h == hash_str("s") || h == hash_str("w") {
                if t.val[k] > 0.0 {
                    t.val[k] = -t.val[k];
                }
            }
        }
        t.cval[k] = canonical(t.val[k], t.unit[k]);
        k += 1;
    }
}
