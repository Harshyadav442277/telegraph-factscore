//! Telegraph comparator module for deterministic intents.
//!
//! Exports `imw_score_deterministic(truth, miner, spec_ptr) -> i64`, `imw_alloc` and `imw_free`.
//! The score is the network's reference rule, in basis points. The answer key is
//! `ReferenceScore` in telegraph-wasm-baseline `pkg/scoring/comparator/reference.go`, and this
//! file follows it line for line, including the parts that look simplifiable:
//!
//! - the code is truncated to i32, and an unknown code is Exact;
//! - subtraction and abs wrap, as Go int64 arithmetic does;
//! - multiplication saturates;
//! - RelTol computes diff * 10000 in 128 bits, and a quotient outside i64 becomes i64::MAX.
#![no_std]

const SCORE_ONE: i64 = 10_000;
const ABS_TOL: i32 = 1;
const REL_TOL: i32 = 2;

/// The node writes the 16-byte spec (code, tolerance) here through `imw_alloc`. It is never
/// read across calls: every call gets its spec written fresh.
static mut SPEC_BUF: [u8; 64] = [0; 64];

#[no_mangle]
pub extern "C" fn imw_alloc(size: i32) -> i32 {
    if size <= 0 || size as usize > 64 {
        return 0; // the node treats a null pointer as an allocation failure
    }
    core::ptr::addr_of_mut!(SPEC_BUF) as i32
}

#[no_mangle]
pub extern "C" fn imw_free(_ptr: i32, _size: i32) {}

#[no_mangle]
pub extern "C" fn imw_score_deterministic(truth: i64, miner: i64, spec_ptr: i32) -> i64 {
    let p = spec_ptr as usize as *const [u8; 8];
    // SAFETY: the node wrote 16 bytes at spec_ptr, which imw_alloc returned.
    let (code, tolerance) = unsafe { (i64::from_le_bytes(*p), i64::from_le_bytes(*p.add(1))) };
    reference_score(code, tolerance, truth, miner)
}

pub fn reference_score(code: i64, tolerance: i64, truth: i64, miner: i64) -> i64 {
    let diff = miner.wrapping_sub(truth).wrapping_abs();
    match code as i32 {
        ABS_TOL => linear_decay(diff, tolerance),
        REL_TOL => {
            let mut denom = truth.wrapping_abs();
            if denom < 1 {
                denom = 1;
            }
            let q = (diff as i128) * (SCORE_ONE as i128) / (denom as i128);
            let err_bps = if q > i64::MAX as i128 || q < i64::MIN as i128 { i64::MAX } else { q as i64 };
            linear_decay(err_bps, tolerance)
        }
        _ => {
            if miner == truth {
                SCORE_ONE
            } else {
                0
            }
        }
    }
}

fn linear_decay(err: i64, tol: i64) -> i64 {
    if tol <= 0 {
        return if err == 0 { SCORE_ONE } else { 0 };
    }
    let window = tol.saturating_mul(2);
    if err >= window {
        return 0;
    }
    SCORE_ONE.saturating_mul(window.wrapping_sub(err)) / window
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}
