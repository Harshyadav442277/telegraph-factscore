//! Telegraph scoring module — fact-aware, precision-of-answer.
//!
//! Freestanding `wasm32-unknown-unknown`, **zero imports**. Exports exactly the
//! canonical ABI (ARCHITECTURE A1):
//!
//! ```text
//! alloc(size: i32) -> i32
//! dealloc(ptr: i32, size: i32)
//! rank_answer(q_ptr, q_len, gt_ptr, gt_len, ma_ptr, ma_len) -> f32   // 6 i32 params
//! breakdown_answer(..., out_ptr) -> i32                              // debug only
//! ```
//!
//! Scoring in one line: weight each token by how much it decides the answer,
//! measure how much of what the *answer asserts* the ground truth supports,
//! gate on whether the answer said anything the question did not already give
//! away, multiply by typed agreement on figures and identifiers, then calibrate
//! with a smoothstep instead of a step.
//!
//! `#![no_std]` for the wasm build; on the host the crate links `std` so the
//! `#[cfg(test)]` unit tests can run under a normal `cargo test`. The shipped
//! artefact is only ever the wasm one, and that build is genuinely `no_std` —
//! `wasm-tools print | grep -c '(import'` is the check that proves it.

#![cfg_attr(target_arch = "wasm32", no_std)]

mod aliases;
mod antonyms;
mod bytes;
mod facts;
mod models;
mod profile;
mod score;
mod sets;
mod tokens;
mod units;

use core::ptr::addr_of_mut;

// --------------------------------------------------------------------------
// Allocator
// --------------------------------------------------------------------------

/// One fixed arena, so the module imports nothing. Sized for three 128 KiB
/// texts (the host's `MaxTextBytes` cap) several times over, so the wrap below
/// can never clobber a string that is still live within one call.
const HEAP_SIZE: usize = 1024 * 1024;

static mut HEAP: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];
static mut BUMP: usize = 0;

// Bump-allocate `size` bytes, 8-byte aligned, wrapping when the arena fills.
//
// Wrapping rather than failing is deliberate: the gate makes thousands of calls
// and never calls `dealloc`, so a strict allocator would return 0 partway
// through the run and the host would fail the call. The host also refuses to
// grow memory, so the pointer must stay inside already-committed linear memory
// (gate analysis §5) — which a fixed static arena guarantees.

/// Where the next allocation lands, and where the bump pointer moves to.
/// Split out so the wrap-around arithmetic is unit-testable without fabricating
/// 32-bit pointers on a 64-bit host.
fn bump_next(bump: usize, size: i32) -> (usize, usize) {
    if size <= 0 || size as usize > HEAP_SIZE {
        return (0, bump);
    }
    let size = size as usize;
    let mut start = (bump + 7) & !7usize;
    if start + size > HEAP_SIZE {
        start = 0;
    }
    (start, start + size)
}

#[no_mangle]
pub extern "C" fn alloc(size: i32) -> i32 {
    let base = addr_of_mut!(HEAP) as *mut u8;
    let (start, next) = unsafe { bump_next(BUMP, size) };
    unsafe {
        BUMP = next;
        // Never hand back 0 for a real request: 0 is the host's "empty string",
        // and a 0 here makes the host's mem.Write fail the whole call.
        base.add(start) as i32
    }
}

/// No-op: the arena is reclaimed by wrapping, not by freeing.
#[no_mangle]
pub extern "C" fn dealloc(_ptr: i32, _size: i32) {}

// --------------------------------------------------------------------------
// Reading host memory
// --------------------------------------------------------------------------

/// Borrow a host-written string.
///
/// The host passes **`ptr = 0, len = 0` for an empty string without calling
/// `alloc`** (gate analysis §5), and Stage 1 passes an empty answer on purpose.
/// So `len` is tested before any slice is constructed — building a slice from
/// address 0 first is exactly the trap that fails registration.
unsafe fn read_bytes<'a>(ptr: i32, len: i32) -> &'a [u8] {
    if len <= 0 || ptr <= 0 {
        return &[];
    }
    unsafe { core::slice::from_raw_parts(ptr as *const u8, len as usize) }
}

// --------------------------------------------------------------------------
// The export the node calls
// --------------------------------------------------------------------------

/// The whole decision, over borrowed slices. `rank_answer` is only the pointer
/// shim around this, which is what lets the unit tests exercise the real branch
/// order without fabricating 32-bit pointers on a 64-bit host.
pub fn rank_slices(q: &[u8], gt: &[u8], ma: &[u8]) -> f32 {
    // Stage 1: an empty or whitespace-only answer is EXACTLY 0.0. A returned
    // 0.0007 has failed a live registration, so this branch precedes everything.
    if bytes::is_blank(ma) {
        return 0.0;
    }
    // A normalised exact match is 1.0, which pins the self-match ratchet at its
    // maximum and guarantees it beats any cross-match.
    if bytes::normalized_equal(gt, ma) {
        return 1.0;
    }
    bytes::clamp01(score::score(q, gt, ma))
}

/// Score `ma` as an answer to `q` given ground truth `gt`. Always in [0,1].
#[no_mangle]
pub extern "C" fn rank_answer(
    q_ptr: i32,
    q_len: i32,
    gt_ptr: i32,
    gt_len: i32,
    ma_ptr: i32,
    ma_len: i32,
) -> f32 {
    let (q, gt, ma) = unsafe {
        (
            read_bytes(q_ptr, q_len),
            read_bytes(gt_ptr, gt_len),
            read_bytes(ma_ptr, ma_len),
        )
    };
    rank_slices(q, gt, ma)
}

/// Breakdown over borrowed slices, following the same branch order as
/// `rank_slices` so the debug view can never disagree with the score.
pub fn breakdown_slices(q: &[u8], gt: &[u8], ma: &[u8]) -> score::Breakdown {
    if bytes::is_blank(ma) {
        return score::Breakdown {
            precision: 0.0,
            fact: 0.0,
            answered: 0.0,
            raw: 0.0,
            final_score: 0.0,
        };
    }
    if bytes::normalized_equal(gt, ma) {
        return score::Breakdown {
            precision: 1.0,
            fact: 1.0,
            answered: 1.0,
            raw: 1.0,
            final_score: 1.0,
        };
    }
    score::breakdown(q, gt, ma)
}

/// Debug companion: writes `[precision, fact, answered, raw, score]` as five
/// f32 at `out_ptr` and returns the count written. Never called by either gate
/// (gate analysis §6) — it exists so a reviewer can see *why* a score came out
/// the way it did. Returns 0 if `out_ptr` is unusable.
#[no_mangle]
pub extern "C" fn breakdown_answer(
    q_ptr: i32,
    q_len: i32,
    gt_ptr: i32,
    gt_len: i32,
    ma_ptr: i32,
    ma_len: i32,
    out_ptr: i32,
) -> i32 {
    if out_ptr <= 0 {
        return 0;
    }
    let (q, gt, ma) = unsafe {
        (
            read_bytes(q_ptr, q_len),
            read_bytes(gt_ptr, gt_len),
            read_bytes(ma_ptr, ma_len),
        )
    };

    let b = breakdown_slices(q, gt, ma);
    let out = out_ptr as *mut f32;
    unsafe {
        out.add(0).write_unaligned(b.precision);
        out.add(1).write_unaligned(b.fact);
        out.add(2).write_unaligned(b.answered);
        out.add(3).write_unaligned(b.raw);
        out.add(4).write_unaligned(b.final_score);
    }
    5
}

/// Trap, never spin.
///
/// This was `loop {}`, which turns any reachable bounds check into a **hang**
/// rather than a fault. The node's fixture gate has a 600 s budget across three
/// attempts: a hang burns the whole budget and reports nothing, where a trap is
/// an immediate, diagnosable rejection. Nothing here is expected to panic — the
/// hot paths use checked accessors — but the failure mode has to be the benign
/// one (adversarial review M1, GAPS G6).
#[cfg(all(target_arch = "wasm32", not(test)))]
#[panic_handler]
fn panic_handler(_: &core::panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}

#[cfg(test)]
mod abi_tests {
    use super::*;

    // These exercise `rank_slices`, the body `rank_answer` delegates to. The
    // six-i32 pointer shim itself is only meaningful under 32-bit wasm — casting
    // a 64-bit host pointer to i32 would truncate it — so the wire ABI is
    // verified against the real module by `verify.mjs`.
    fn call(q: &[u8], gt: &[u8], ma: &[u8]) -> f32 {
        rank_slices(q, gt, ma)
    }

    #[test]
    fn empty_answer_is_exactly_zero() {
        // The host passes ptr=0,len=0 without calling alloc; read_bytes must
        // return an empty slice rather than dereferencing address 0.
        let s = rank_answer(0, 0, 0, 0, 0, 0);
        assert_eq!(s, 0.0);
        assert_eq!(call(b"q", b"Paris", b""), 0.0);
    }

    #[test]
    fn whitespace_answer_is_exactly_zero() {
        assert_eq!(call(b"q", b"Paris", b"   "), 0.0);
        assert_eq!(call(b"q", b"Paris", b" \t\r\n "), 0.0);
        assert_eq!(call(b"q", b"Paris", b"\t"), 0.0);
    }

    #[test]
    fn self_match_is_one_and_beats_cross_match() {
        let q = b"What is the capital of France?";
        let gt = b"The capital of France is Paris.";
        let self_m = call(q, gt, gt);
        let cross = call(q, gt, b"Bananas grow in tropical climates.");
        assert_eq!(self_m, 1.0);
        assert!(self_m > cross);
    }

    #[test]
    fn output_is_always_in_range() {
        let cases: [&[u8]; 4] = [
            b"Paris",
            b"",
            b"   ",
            b"\xff\xfe\xfd garbage \xf0\x9f\x97\xbc",
        ];
        for c in cases.iter() {
            let s = call(b"q", b"Paris", c);
            assert!((0.0..=1.0).contains(&s), "out of range: {}", s);
        }
    }

    #[test]
    fn large_and_unicode_inputs_do_not_trap() {
        let big = "storm ".repeat(9 * 1024); // ~54 KB, the Stage-1 adversarial case
        let s = call(b"storm?", b"Winds of 12 m/s are expected.", big.as_bytes());
        assert!((0.0..=1.0).contains(&s));

        let uni = "\u{1F5FC}\u{4E2D}\u{6587} caf\u{E9} \u{2603} 23.1\u{B0}C";
        let s2 = call(b"weather?", b"It is 23.1C.", uni.as_bytes());
        assert!((0.0..=1.0).contains(&s2));

        // Invalid UTF-8 must be treated as opaque bytes, never decoded.
        let s3 = call(b"q", b"Paris", b"\xff\xfe\x00\x80\x80");
        assert!((0.0..=1.0).contains(&s3));
    }

    #[test]
    fn allocations_are_aligned_ascending_and_disjoint() {
        let (a, n1) = bump_next(0, 5);
        let (b, n2) = bump_next(n1, 5);
        let (c, _) = bump_next(n2, 32);
        assert_eq!(a % 8, 0);
        assert_eq!(b % 8, 0);
        assert_eq!(c % 8, 0);
        assert!(b >= a + 5 && c >= b + 5, "allocations must not overlap");
    }

    #[test]
    fn the_arena_wraps_instead_of_failing() {
        // The gate makes thousands of calls and never calls dealloc, so a
        // strict allocator would start refusing partway through the run.
        let mut bump = 0usize;
        for _ in 0..500 {
            let (start, next) = bump_next(bump, 64 * 1024);
            assert!(
                start + 64 * 1024 <= HEAP_SIZE,
                "allocation escaped the arena"
            );
            bump = next;
        }
    }

    #[test]
    fn one_call_worth_of_text_never_wraps_onto_itself() {
        // Three texts at the host's 128 KiB MaxTextBytes cap must coexist.
        let mut bump = HEAP_SIZE - 1; // worst case: about to wrap
        let mut seen = [0usize; 3];
        for slot in seen.iter_mut() {
            let (start, next) = bump_next(bump, 128 * 1024);
            *slot = start;
            bump = next;
        }
        assert!(seen[1] >= seen[0] + 128 * 1024);
        assert!(seen[2] >= seen[1] + 128 * 1024);
    }

    #[test]
    fn degenerate_sizes_stay_inside_the_arena() {
        assert_eq!(bump_next(0, 0).0, 0);
        assert_eq!(bump_next(0, -1).0, 0);
        assert_eq!(bump_next(0, i32::MAX).0, 0);
        // alloc itself must still hand back a real address, never 0.
        assert!(alloc(0) != 0);
        assert!(alloc(16) != 0);
    }

    #[test]
    fn breakdown_agrees_with_the_score() {
        let q = b"What is the capital of France?";
        let gt = b"The capital of France is Paris.";
        let b = breakdown_slices(q, gt, gt);
        assert_eq!(b.final_score, 1.0);
        let ma = b"The data shows the capital is Paris.";
        assert_eq!(breakdown_slices(q, gt, ma).final_score, call(q, gt, ma));
        assert_eq!(breakdown_slices(q, gt, b"  ").final_score, 0.0);
    }
}
