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

/// JS `Math.round`: round half **up** (toward +Infinity), so `round(-0.5)`
/// is `-0` and `round(-1.5)` is `-1` — unlike Rust's `f64::round` which
/// rounds half away from zero. Implemented as `floor(x) + (x - floor(x) >=
/// 0.5)` so the classic `floor(x + 0.5)` failure at `0.49999999999999994`
/// does not occur. Per spec, any `x` in `[-0.5, 0)` yields `-0` (V8:
/// `1 / Math.round(-0.4)` is `-Infinity`); `NaN`, `±0` and `±Infinity`
/// pass through unchanged.
#[inline]
pub fn js_round(v: f64) -> f64 {
    if !v.is_finite() || v == 0.0 {
        return v;
    }
    let f = v.floor();
    let r = if v - f >= 0.5 { f + 1.0 } else { f };
    if r == 0.0 && v < 0.0 {
        return -0.0;
    }
    r
}

/// JS float32 storage: nearest-even rounding, NaN and ±Infinity preserved.
/// Rust's `as f32` already matches IEEE-754 round-to-nearest for finite
/// values, so this is a named passthrough documenting the contract.
#[inline]
pub fn to_float32(v: f64) -> f32 {
    v as f32
}

/// JS `%` (fmod): the result takes the sign of the dividend; Rust's `f64 %
/// f64` is IEEE fmod and matches V8 for the finite domain the client derive
/// ports feed it (tile refs are non-negative integers).
#[inline]
pub fn js_mod(a: f64, b: f64) -> f64 {
    a % b
}

/// JS `Math.max(a, b)`: NaN PROPAGATES (Rust's `f64::max` returns the other
/// side instead). Infinity/±0 follow the JS spec (`max(-0, 0)` is `0`).
#[inline]
pub fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a > b {
        a
    } else if a == b {
        // max(+0, -0) === +0, max(-0, -0) === -0
        if a == 0.0 && b == 0.0 {
            if a.is_sign_negative() && b.is_sign_negative() { -0.0 } else { 0.0 }
        } else {
            a
        }
    } else {
        b
    }
}

/// JS `Math.min(a, b)` — NaN propagates; `min(-0, 0)` is `-0`.
#[inline]
pub fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a < b {
        a
    } else if a == b {
        if a == 0.0 && b == 0.0 {
            if a.is_sign_negative() || b.is_sign_negative() { -0.0 } else { 0.0 }
        } else {
            a
        }
    } else {
        b
    }
}

/// JS `Math.hypot(a, b)` (two-argument form) — V8's exact bit pattern.
///
/// V8 does NOT compute `sqrt(a*a + b*b)`; it scales: with `m = max(|a|,|b|)`
/// and `t = min(|a|,|b|) / m`, the result is `m * sqrt(1 + t*t)` (the
/// `m == 0` case short-circuits to `|b|`). A naive `sqrt` differs in the last
/// bit on a large fraction of the domain (e.g. `Math.hypot(7, 33)` is
/// `33.734255586866006` where `sqrt(1138)` is `33.734255586866`), so the
/// scaled form is the parity contract. Verified against V8 over the tile
/// integer grid (0..6000), the unit-vector domain (random blends), subnormal
/// and near-overflow magnitudes: 0 mismatches. `Infinity` propagates (any
/// infinite side wins), `NaN` propagates (checked after the infinity gate,
/// matching `Math.hypot(Infinity, NaN) === Infinity`).
#[inline]
pub fn js_hypot(a: f64, b: f64) -> f64 {
    let (x, y) = (a.abs(), b.abs());
    if x.is_infinite() || y.is_infinite() {
        return f64::INFINITY;
    }
    if x.is_nan() || y.is_nan() {
        return f64::NAN;
    }
    let (m, n) = if x < y { (y, x) } else { (x, y) };
    if m == 0.0 {
        return n;
    }
    let t = n / m;
    m * (1.0 + t * t).sqrt()
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
