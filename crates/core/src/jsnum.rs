//! JavaScript numeric coercion primitives (`ToInt32`, `ToUint32`, `ToUint16`),
//! as used by typed-array element writes and `| 0` / `>>> 0` operators.
//!
//! Every ported structure stores values through a typed array, so the exact
//! modulo/wrap behaviour of these operations is part of the parity contract.
//! Rust's `as` casts saturate and must never be used directly for this.
//!
//! Verified against V8 (see `rust/tools/probe_jsnum.mjs`): NaN, ±0 and
//! ±Infinity all coerce to `+0` (this is *not* Rust's `as` behaviour). Finite
//! values are **truncated toward zero** (`-1.5 | 0` is `-1`, unlike
//! `Math.floor` which would give `-2`) and then reduced modulo 2^n into
//! `[0, 2^n)`.

/// `ToUint32`: typed-array write into a `Uint32Array`, and `>>> 0`.
pub fn to_uint32(v: f64) -> u32 {
    if !v.is_finite() {
        return 0; // NaN, ±Infinity and ±0 all coerce to +0
    }
    let n = v.trunc();
    if n.abs() < 9_007_199_254_740_992.0 {
        return (n as i64).rem_euclid(4_294_967_296) as u32;
    }
    n.rem_euclid(4_294_967_296.0) as u32
}

/// `ToInt32`: typed-array write into an `Int32Array`, and `| 0` / `<< 0`.
/// Same bit pattern as [`to_uint32`], reinterpreted as signed.
pub fn to_int32(v: f64) -> i32 {
    to_uint32(v) as i32
}

/// `ToUint16`: typed-array write into a `Uint16Array`.
pub fn to_uint16(v: f64) -> u16 {
    if !v.is_finite() {
        return 0;
    }
    let n = v.trunc();
    if n.abs() < 9_007_199_254_740_992.0 {
        return (n as i64).rem_euclid(65_536) as u16;
    }
    n.rem_euclid(65_536.0) as u16
}

/// JS float32 storage: nearest-even rounding, NaN and ±Infinity preserved.
/// Rust's `as f32` already matches IEEE-754 round-to-nearest for finite
/// values, so this is a named passthrough documenting the contract.
#[inline]
pub fn to_float32(v: f64) -> f32 {
    v as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uint32_matches_js_semantics() {
        assert_eq!(to_uint32(0.0), 0);
        assert_eq!(to_uint32(-0.0), 0);
        assert_eq!(to_uint32(f64::NAN), 0);
        assert_eq!(to_uint32(f64::INFINITY), 0); // JS: 0, Rust `as` would differ
        assert_eq!(to_uint32(f64::NEG_INFINITY), 0);
        assert_eq!(to_uint32(-1.0), 4_294_967_295); // JS: -1 >>> 0
        assert_eq!(to_uint32(-1.5), 4_294_967_295); // trunc: -1, mod 2^32
        assert_eq!(to_uint32(4_294_967_296.0), 0);
        assert_eq!(to_uint32(1.9), 1);
        // 2^53+1 is not representable as f64: the literal *is* 2^53, whose
        // mod-2^32 is 0. The real >2^32 path is 2^32+1 -> 1.
        assert_eq!(to_uint32(9_007_199_254_740_993.0), 0);
        assert_eq!(to_uint32(4_294_967_297.0), 1);
    }

    #[test]
    fn int32_matches_js_semantics() {
        assert_eq!(to_int32(2_147_483_648.0), -2_147_483_648);
        assert_eq!(to_int32(4_294_967_295.0), -1);
        assert_eq!(to_int32(f64::NAN), 0);
        assert_eq!(to_int32(-1.5), -1); // trunc toward zero, then mod
        assert_eq!(to_int32(-2.5), -2);
        assert_eq!(to_int32(1.9), 1);
        assert_eq!(to_int32(4_294_967_296.0), 0);
        assert_eq!(to_int32(-2_147_483_649.0), 2_147_483_647);
        assert_eq!(to_int32(f64::INFINITY), 0);
        assert_eq!(to_int32(-0.5), 0);
    }

    #[test]
    fn uint16_matches_js_semantics() {
        assert_eq!(to_uint16(65_536.0), 0);
        assert_eq!(to_uint16(-1.0), 65_535);
        assert_eq!(to_uint16(70_000.5), 4464); // 70000 mod 65536 = 4464
        assert_eq!(to_uint16(f64::INFINITY), 0);
    }

    #[test]
    fn float32_storage_rounds_to_nearest_even() {
        assert_eq!(to_float32(1.0 + 2f64.powi(-24)), 1.0); // below half-ulp
        assert_eq!(to_float32(2f64.powi(127) * 2.0), f32::INFINITY);
        assert!(to_float32(f64::NAN).is_nan());
    }
}
