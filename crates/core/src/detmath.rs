//! Bit-exact port of `src/core/DetMath.ts`.
//!
//! The JS spec only requires `Math.exp/log/pow/atan2` to be implementation
//! approximated, and engines differ in the last bit. Because the simulation
//! runs on every client and compares hashes, a one-bit difference that lands on
//! a truncation boundary is a desync. These implementations use only `+ - * /`
//! and raw bit views, which IEEE 754 requires to be correctly rounded, so every
//! platform produces the same bits.
//!
//! The arithmetic expressions are transcribed from the TypeScript source with
//! the *same* association order and the same division/multiplication choices
//! (for example `w / 17` is a division there, not a multiplication by a
//! reciprocal). Reassociating any of them changes the last bits and would break
//! parity, so do not "simplify" this file.
//!
//! Accuracy is about 1e-8 relative. Inputs are assumed finite; negative bases
//! for [`pow`] are not supported.

// The literals below are transcribed from the TypeScript source character for
// character. They happen to equal Rust's `core::f64::consts` values, but keeping
// the same decimal text as the TS is what makes the two files diffable, so the
// lint that suggests substituting the constants is deliberately silenced here.
#[allow(clippy::approx_constant)]
const LN2: f64 = 0.6931471805599453;
#[allow(clippy::approx_constant)]
const LOG2E: f64 = 1.4426950408889634;
#[allow(clippy::approx_constant)]
const SQRT2: f64 = 1.4142135623730951;
#[allow(clippy::approx_constant)]
const PI: f64 = 3.141592653589793;
#[allow(clippy::approx_constant)]
const PI_2: f64 = 1.5707963267948966;
#[allow(clippy::approx_constant)]
const PI_4: f64 = 0.7853981633974483;
#[allow(clippy::approx_constant)]
const TAN_PI_8: f64 = 0.41421356237309503;

/// Exponent-field bias of IEEE-754 binary64.
const EXP_BIAS: u64 = 1023;
/// Shift of the exponent field within the 64-bit pattern.
const EXP_SHIFT: u32 = 52;
/// Bit pattern of an exponent field of exactly `EXP_BIAS`, i.e. value 1.
const ONE_BITS: u64 = EXP_BIAS << EXP_SHIFT;
/// Mask covering the 52-bit fraction field.
const FRAC_MASK: u64 = (1u64 << EXP_SHIFT) - 1;
/// Smallest normal positive double, `2^-1022`.
const MIN_NORMAL: f64 = 2.2250738585072014e-308;
/// `2^54`, used to scale subnormals into the normal range.
const SCALE_2_54: f64 = 18014398509481984.0;

/// `2^n` for integer `n`; exact, saturating to `Infinity` / `0` like `Math.pow`.
pub fn pow2(n: i32) -> f64 {
    if n > 1023 {
        return f64::INFINITY;
    }
    if n < -1022 {
        return 0.0;
    }
    // The TS source writes `(n + 1023) << 20` into the high word and zeroes the
    // low word. `n + 1023` is in `1..=2045` here, so the JS shift cannot
    // overflow int32 and the result is exactly this exponent field with a zero
    // fraction. The addition stays signed until the value is known non-negative
    // (a negative `n` cast to u64 would wrap and trip the overflow check).
    f64::from_bits(((n + EXP_BIAS as i32) as u64) << EXP_SHIFT)
}

/// `e^x`.
pub fn exp(x: f64) -> f64 {
    if x > 709.0 {
        return f64::INFINITY;
    }
    if x < -708.0 {
        return 0.0;
    }
    // x = n * ln2 + r, with |r| <= ln2 / 2
    let n = (x * LOG2E + 0.5).floor();
    let r = x - n * LN2;
    // Taylor to r^8; the next term is below 1e-9 relative at |r| <= 0.35.
    let p = 1.0
        + r
            * (1.0
                + r
                    * (1.0 / 2.0
                        + r
                            * (1.0 / 6.0
                                + r
                                    * (1.0 / 24.0
                                        + r
                                            * (1.0 / 120.0
                                                + r
                                                    * (1.0 / 720.0
                                                        + r * (1.0 / 5040.0 + r * (1.0 / 40320.0))))))));
    p * pow2(n as i32)
}

/// Natural log of `x > 0`.
pub fn log(x: f64) -> f64 {
    if x <= 0.0 {
        return if x == 0.0 { f64::NEG_INFINITY } else { f64::NAN };
    }
    let mut x = x;
    let mut e = 0i32;
    if x < MIN_NORMAL {
        // Subnormal: scale into the normal range first.
        x *= SCALE_2_54;
        e = -54;
    }
    let bits = x.to_bits();
    e += ((bits >> EXP_SHIFT) & 0x7ff) as i32 - EXP_BIAS as i32;
    // Replace the exponent with the bias so the value lands in [1, 2).
    let m = f64::from_bits((bits & FRAC_MASK) | ONE_BITS);
    let mut m = m;
    if m > SQRT2 {
        m *= 0.5;
        e += 1;
    }
    // log(m) = 2 * atanh(s), s = (m - 1) / (m + 1), |s| <= 0.172
    let s = (m - 1.0) / (m + 1.0);
    let z = s * s;
    let series = 1.0
        + z * (1.0 / 3.0 + z * (1.0 / 5.0 + z * (1.0 / 7.0 + z * (1.0 / 9.0 + z * (1.0 / 11.0 + z * (1.0 / 13.0))))));
    e as f64 * LN2 + 2.0 * s * series
}

/// `x^y` for `x >= 0`.
pub fn pow(x: f64, y: f64) -> f64 {
    if y == 0.0 || x == 1.0 {
        return 1.0;
    }
    if x == 0.0 {
        return if y > 0.0 { 0.0 } else { f64::INFINITY };
    }
    if x < 0.0 {
        return f64::NAN;
    }
    exp(y * log(x))
}

/// `atan(z)` for `z` in `[0, 1]`.
fn atan_unit(z: f64) -> f64 {
    // Fold [tan(pi/8), 1] onto [-tan(pi/8), tan(pi/8)] around pi/4.
    let mut z = z;
    let mut base = 0.0;
    if z > TAN_PI_8 {
        base = PI_4;
        z = (z - 1.0) / (z + 1.0);
    }
    // Taylor to z^17; next term is below 2e-8 at |z| <= 0.4142.
    let w = z * z;
    let series = 1.0
        - w * (1.0 / 3.0
            - w * (1.0 / 5.0
                - w * (1.0 / 7.0
                    - w * (1.0 / 9.0 - w * (1.0 / 11.0 - w * (1.0 / 13.0 - w * (1.0 / 15.0 - w / 17.0)))))));
    base + z * series
}

/// Angle of `(x, y)` in `(-pi, pi]`, like `Math.atan2` (ignoring signed zeros).
pub fn atan2(y: f64, x: f64) -> f64 {
    if y == 0.0 {
        return if x >= 0.0 { 0.0 } else { PI };
    }
    if x == 0.0 {
        return if y > 0.0 { PI_2 } else { -PI_2 };
    }
    let ax = if x < 0.0 { -x } else { x };
    let ay = if y < 0.0 { -y } else { y };
    let mut a = if ay <= ax {
        atan_unit(ay / ax)
    } else {
        PI_2 - atan_unit(ax / ay)
    };
    if x < 0.0 {
        a = PI - a;
    }
    if y < 0.0 {
        -a
    } else {
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rel_err(a: f64, b: f64) -> f64 {
        if b == 0.0 {
            a.abs()
        } else {
            (a - b).abs() / b.abs()
        }
    }

    #[test]
    fn pow2_matches_std_for_every_valid_exponent() {
        for n in -1022..=1023 {
            assert_eq!(pow2(n), (2f64).powi(n), "pow2({n})");
        }
        assert_eq!(pow2(1024), f64::INFINITY);
        assert_eq!(pow2(5000), f64::INFINITY);
        assert_eq!(pow2(-1023), 0.0);
    }

    #[test]
    fn exp_tracks_std_exp_to_1e_minus_8() {
        let mut x = -700.0;
        while x <= 700.0 {
            assert!(rel_err(exp(x), x.exp()) < 1e-8, "exp({x})");
            x += 0.37;
        }
        assert_eq!(exp(0.0), 1.0);
        assert_eq!(exp(710.0), f64::INFINITY);
        assert_eq!(exp(-800.0), 0.0);
    }

    #[test]
    fn log_tracks_std_log() {
        let mut p = -300.0;
        while p <= 300.0 {
            let x = 10f64.powf(p);
            assert!(rel_err(log(x), x.ln()) < 1e-8, "log({x})");
            p += 0.61;
        }
        let mut x = 0.5;
        while x <= 4.0 {
            assert!((log(x) - x.ln()).abs() < 1e-9, "log({x})");
            x += 0.013;
        }
        assert_eq!(log(1.0), 0.0);
        assert_eq!(log(0.0), f64::NEG_INFINITY);
        assert!(log(-1.0).is_nan());
        assert!(rel_err(log(f64::MIN_POSITIVE / 2.0), (f64::MIN_POSITIVE / 2.0).ln()) < 1e-8);
    }

    #[test]
    fn pow_tracks_std_pow_on_game_sized_inputs() {
        let bases = [0.5, 1.0, 2.0, 7.3, 100.0, 5000.0, 123_456.0, 1e6, 4e6, 1e9];
        let exps = [0.0, 0.15, 0.35, 0.5, 0.6, 0.73, 1.0, 2.0, 2.5];
        for x in bases {
            for y in exps {
                assert!(rel_err(pow(x, y), x.powf(y)) < 1e-7, "pow({x}, {y})");
            }
        }
        assert_eq!(pow(0.0, 0.6), 0.0);
        assert_eq!(pow(0.0, 0.0), 1.0);
        assert!(pow(-2.0, 0.5).is_nan());
    }

    #[test]
    fn atan2_tracks_std_across_quadrants() {
        for y in -20..=20 {
            for x in -20..=20 {
                assert!(
                    (atan2(y as f64, x as f64) - (y as f64).atan2(x as f64)).abs() < 1e-7,
                    "atan2({y}, {x})"
                );
            }
        }
        assert_eq!(atan2(0.0, 1.0), 0.0);
        assert_eq!(atan2(0.0, -1.0), PI);
        assert_eq!(atan2(1.0, 0.0), PI_2);
        assert_eq!(atan2(-1.0, 0.0), -PI_2);
    }
}
