//! ECMAScript `Number::toString` and `Number.prototype.toFixed` — the exact
//! string conversions the client formatters (`renderNumber`,
//! `formatPercentage`, `formatDebugTranslation`) depend on.
//!
//! Both are bit-exact over the full double domain, not approximations:
//!
//! * [`js_to_string`] implements the spec's shortest-digits formatting:
//!   fixed notation when the decimal-point position `k` satisfies
//!   `-6 < k <= 21`, exponential notation (`e+NN` / `e-NN`, no leading
//!   zeros) otherwise. `-0` prints `"0"`, `NaN`/`±Infinity` print their
//!   spec spellings. The shortest digit string comes from Rust's `{:e}`
//!   (Grisu shortest round-trip — identical digits to V8's dtoa).
//! * [`to_fixed`] implements the spec's exact half-up rounding on the
//!   mathematical value of the double: `|x| = M * 2^E` (frexp decomposition,
//!   subnormals included), scaled by `10^d` through a small little-endian
//!   limb big-integer, then rounded half-up. Consequences pinned by V8
//!   goldens: `(0.5).toFixed(0) == "1"`, `(2.5).toFixed(0) == "3"`,
//!   `1.005.toFixed(2) == "1.00"` (the double is below the half),
//!   `(1e21 - 1).toFixed(0)` expands the exact integer
//!   `"999999999999999868928"`, while `1e21.toFixed(1)` falls back to
//!   [`js_to_string`] and yields `"1e+21"`. The sign is attached when
//!   `x < 0` strictly, so `(-0).toFixed(2) == "0.00"` (no minus) but
//!   `(-0.0001).toFixed(2) == "-0.00"` (minus survives a zero rounding).
//!
//! The `d` domain is `0..=100` (spec); `d > 100` is a RangeError in JS and
//! never enters the capture.

/// Minimal little-endian `u32`-limb non-negative big integer — only the
/// operations `toFixed` needs (scale by `10^d`, shift, add a power of two,
/// shift right with floor, decimal rendering). Values stay under ~1100 bits
/// (`M * 10^100` with `M < 2^53`, plus the `2^k` half-up term for `k <= 1074`).
#[derive(Clone, PartialEq, Eq)]
struct Big(Vec<u32>);

impl Big {
    fn from_u64(v: u64) -> Big {
        let mut limbs = vec![v as u32, (v >> 32) as u32];
        trim(&mut limbs);
        Big(limbs)
    }

    fn is_zero(&self) -> bool {
        self.0.is_empty()
    }

    /// Multiply by a small factor (`10`) with carry.
    fn mul_small(&mut self, m: u32) {
        let mut carry: u64 = 0;
        for limb in self.0.iter_mut() {
            let t = (*limb as u64) * (m as u64) + carry;
            *limb = t as u32;
            carry = t >> 32;
        }
        while carry != 0 {
            self.0.push(carry as u32);
            carry >>= 32;
        }
    }

    /// Left shift by `bits` (arbitrary width).
    fn shl(&mut self, bits: usize) {
        if self.is_zero() || bits == 0 {
            return;
        }
        let words = bits / 32;
        let rem = bits % 32;
        let mut out = vec![0u32; words];
        if rem == 0 {
            out.extend_from_slice(&self.0);
        } else {
            let mut carry: u32 = 0;
            for &limb in &self.0 {
                out.push((limb << rem) | carry);
                carry = limb >> (32 - rem);
            }
            if carry != 0 {
                out.push(carry);
            }
        }
        trim(&mut out);
        self.0 = out;
    }

    /// Add `2^bits` with carry propagation (the bit may already be set).
    fn add_pow2(&mut self, bits: usize) {
        let word = bits / 32;
        let rem = bits % 32;
        while self.0.len() <= word {
            self.0.push(0);
        }
        let mut carry: u64 = 1u64 << rem;
        let mut i = word;
        while carry != 0 {
            if i == self.0.len() {
                self.0.push(0);
            }
            let t = (self.0[i] as u64) + carry;
            self.0[i] = t as u32;
            carry = t >> 32;
            i += 1;
        }
    }

    /// Floor right shift by `bits`.
    fn shr_floor(&mut self, bits: usize) {
        let words = bits / 32;
        let rem = bits % 32;
        if words >= self.0.len() {
            self.0.clear();
            return;
        }
        let mut out = self.0[words..].to_vec();
        if rem != 0 {
            let mut carry: u32 = 0;
            for limb in out.iter_mut().rev() {
                let new_carry = *limb << (32 - rem);
                *limb = (*limb >> rem) | carry;
                carry = new_carry;
            }
        }
        trim(&mut out);
        self.0 = out;
    }

    /// Decimal digit string (no leading zeros; `"0"` when zero).
    fn to_decimal(&self) -> String {
        if self.is_zero() {
            return "0".to_string();
        }
        // Repeated divmod by 10^9, rendering base-1e9 chunks.
        let mut limbs = self.0.clone();
        let mut chunks: Vec<u32> = Vec::new();
        while !limbs.is_empty() {
            let mut rem: u64 = 0;
            for limb in limbs.iter_mut().rev() {
                let cur = (rem << 32) | (*limb as u64);
                *limb = (cur / 1_000_000_000) as u32;
                rem = cur % 1_000_000_000;
            }
            chunks.push(rem as u32);
            trim(&mut limbs);
        }
        let mut out = chunks.pop().unwrap().to_string();
        while let Some(c) = chunks.pop() {
            out.push_str(&format!("{c:09}"));
        }
        out
    }
}

fn trim(limbs: &mut Vec<u32>) {
    while limbs.last() == Some(&0) {
        limbs.pop();
    }
}

/// ECMAScript `Number::toString` (the 1-argument `String(x)` / template
/// literal behaviour) over the full double domain.
pub fn js_to_string(v: f64) -> String {
    if v.is_nan() {
        return "NaN".to_string();
    }
    if v == f64::INFINITY {
        return "Infinity".to_string();
    }
    if v == f64::NEG_INFINITY {
        return "-Infinity".to_string();
    }
    if v == 0.0 {
        // JS prints both +0 and -0 as "0".
        return "0".to_string();
    }
    let sign = if v < 0.0 { "-" } else { "" };
    let av = v.abs();
    // Rust's LowerExp is the shortest round-trip digit string: `d1[.d2..dn]eE`
    // meaning `digits * 10^(E - (n - 1))`. V8's dtoa produces the same digits.
    let exp_repr = format!("{av:e}");
    let (mant, e_str) = exp_repr.split_once('e').unwrap();
    let e10: i32 = e_str.parse().unwrap();
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    let n = digits.len() as i32;
    // k = number of digits before the decimal point in fixed notation.
    let k = e10 + 1;
    let body = if k <= 0 && k > -6 {
        // 0.000<digits...>
        format!("0.{}{digits}", "0".repeat((-k) as usize))
    } else if k > 0 && k <= 21 {
        if k >= n {
            format!("{digits}{}", "0".repeat((k - n) as usize))
        } else {
            let split = k as usize;
            format!("{}.{}", &digits[..split], &digits[split..])
        }
    } else {
        // Exponential: d1[.d2..dn]e±NN (no leading zeros, no +00 padding).
        let exp = k - 1;
        let mant_body = if n == 1 {
            digits.clone()
        } else {
            format!("{}.{}", &digits[..1], &digits[1..])
        };
        if exp >= 0 {
            format!("{mant_body}e+{exp}")
        } else {
            format!("{mant_body}e-{}", -exp)
        }
    };
    format!("{sign}{body}")
}

/// `Number.prototype.toFixed(d)` for `0 <= d <= 100` — exact half-up rounding
/// of the double's mathematical value, spec fallback to [`js_to_string`]
/// once `|x >= 1e21`, and the `x < 0` strict sign gate (`-0` unsigned).
pub fn to_fixed(v: f64, d: u32) -> String {
    if v.is_nan() {
        return "NaN".to_string();
    }
    if v == f64::INFINITY {
        return "Infinity".to_string();
    }
    if v == f64::NEG_INFINITY {
        return "-Infinity".to_string();
    }
    if v.abs() >= 1e21 {
        return js_to_string(v);
    }
    let negative = v < 0.0; // strict: -0.0 < 0.0 is false
    let av = v.abs();
    // frexp decomposition into `M * 2^E` with integer M (subnormals included).
    let (m, e) = if av == 0.0 {
        (0u64, 0i32)
    } else {
        let bits = av.to_bits();
        let biased = ((bits >> 52) & 0x7FF) as i32;
        let frac = bits & 0x000F_FFFF_FFFF_FFFF;
        if biased == 0 {
            (frac, -1074)
        } else {
            (frac | (1u64 << 52), biased - 1075)
        }
    };
    // N = M * 10^d, then exact half-up rounding of N / 2^(-E) (or N * 2^E).
    let mut n_big = Big::from_u64(m);
    for _ in 0..d {
        n_big.mul_small(10);
    }
    if e >= 0 {
        n_big.shl(e as usize);
    } else {
        // floor((2N + 2^k) / 2^(k+1)) with k = -E  (half-up on N / 2^k).
        let k = (-e) as usize;
        n_big.shl(1);
        n_big.add_pow2(k);
        n_big.shr_floor(k + 1);
    }
    let digits = n_big.to_decimal();
    let body = if d == 0 {
        digits
    } else {
        let dd = d as usize;
        let padded = if digits.len() <= dd {
            format!("{}{digits}", "0".repeat(dd - digits.len() + 1))
        } else {
            digits
        };
        let split = padded.len() - dd;
        format!("{}.{}", &padded[..split], &padded[split..])
    };
    if negative {
        format!("-{body}")
    } else {
        body
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_string_matches_v8_goldens() {
        assert_eq!(js_to_string(f64::NAN), "NaN");
        assert_eq!(js_to_string(f64::INFINITY), "Infinity");
        assert_eq!(js_to_string(f64::NEG_INFINITY), "-Infinity");
        assert_eq!(js_to_string(-0.0), "0");
        assert_eq!(js_to_string(1e21), "1e+21");
        assert_eq!(js_to_string(1e20), "100000000000000000000");
        assert_eq!(js_to_string(1e-7), "1e-7");
        assert_eq!(js_to_string(1.5e30), "1.5e+30");
        assert_eq!(js_to_string(0.1 + 0.2), "0.30000000000000004");
        assert_eq!(js_to_string(1.0 / 3.0), "0.3333333333333333");
        assert_eq!(js_to_string(123456789012345678901.5), "123456789012345680000");
        assert_eq!(js_to_string(999999999999999899999.0), "999999999999999900000");
        assert_eq!(js_to_string(1e21 - 1.0), "1e+21");
        assert_eq!(js_to_string(1.000000000000001e22), "1.000000000000001e+22");
        assert_eq!(js_to_string(5e-324), "5e-324");
        assert_eq!(js_to_string(f64::MAX), "1.7976931348623157e+308");
        assert_eq!(js_to_string(1234567890123456.7), "1234567890123456.8");
        assert_eq!(js_to_string(0.000001), "0.000001");
        assert_eq!(js_to_string(9.999999999999999e20), "999999999999999900000");
        assert_eq!(js_to_string(1020000000000000000000.0), "1.02e+21");
        assert_eq!(js_to_string(100.0), "100");
        assert_eq!(js_to_string(-1e21), "-1e+21");
    }

    #[test]
    fn to_fixed_half_up_ties_and_near_halves() {
        assert_eq!(to_fixed(0.5, 0), "1");
        assert_eq!(to_fixed(1.5, 0), "2");
        assert_eq!(to_fixed(2.5, 0), "3");
        assert_eq!(to_fixed(-0.5, 0), "-1");
        assert_eq!(to_fixed(-1.5, 0), "-2");
        assert_eq!(to_fixed(-2.5, 0), "-3");
        assert_eq!(to_fixed(1.005, 2), "1.00");
        assert_eq!(to_fixed(1.25, 1), "1.3");
        assert_eq!(to_fixed(2.675, 2), "2.67");
        assert_eq!(to_fixed(12.55, 1), "12.6");
        assert_eq!(to_fixed(12.5, 1), "12.5");
    }

    #[test]
    fn to_fixed_sign_and_specials() {
        assert_eq!(to_fixed(-0.0, 2), "0.00"); // strict x < 0 gate
        assert_eq!(to_fixed(0.0, 2), "0.00");
        assert_eq!(to_fixed(f64::NAN, 2), "NaN");
        assert_eq!(to_fixed(f64::INFINITY, 1), "Infinity");
        assert_eq!(to_fixed(f64::NEG_INFINITY, 0), "-Infinity");
        assert_eq!(to_fixed(-0.0001, 2), "-0.00"); // minus survives zero rounding
        assert_eq!(to_fixed(-1e-9, 2), "-0.00");
    }

    #[test]
    fn to_fixed_exact_expansion_and_threshold() {
        assert_eq!(to_fixed(123.456, 0), "123");
        assert_eq!(to_fixed(123.456, 2), "123.46");
        assert_eq!(to_fixed(123.456, 20), "123.45600000000000306954");
        assert_eq!(to_fixed(0.0001, 4), "0.0001");
        assert_eq!(to_fixed(0.0001, 3), "0.000");
        assert_eq!(to_fixed(1.5e-7, 8), "0.00000015");
        assert_eq!(to_fixed(1e-6, 6), "0.000001");
        assert_eq!(to_fixed(5e-7, 6), "0.000000");
        assert_eq!(to_fixed(2f64.powi(53), 0), "9007199254740992");
        assert_eq!(to_fixed(2f64.powi(53) + 2.0, 0), "9007199254740994");
        assert_eq!(to_fixed(1e20, 2), "100000000000000000000.00");
        // 1e21 is exactly representable (5^21 * 2^21), so `1e21 - 1` rounds
        // back to 1e21 and takes the ToString fallback (V8 golden: "1e+21").
        assert_eq!(to_fixed(1e21 - 1.0, 0), "1e+21");
        // The double just below 1e21 expands exactly.
        assert_eq!(to_fixed(999999999999999868928.0, 0), "999999999999999868928");
        assert_eq!(to_fixed(1e21, 1), "1e+21"); // >= 1e21 falls back to ToString
        assert_eq!(to_fixed(1.2345678901234567e21, 0), "1.2345678901234568e+21");
        assert_eq!(to_fixed(0.1 + 0.2, 1), "0.3");
        assert_eq!(to_fixed(0.1 + 0.2, 2), "0.30");
        assert_eq!(to_fixed(1.0000000000000002, 17), "1.00000000000000022");
        assert_eq!(to_fixed(1e10, 1), "10000000000.0");
        assert_eq!(to_fixed(5.0, 0), "5");
        assert_eq!(to_fixed(1.234_567_890_123_456_7, 18), "1.234567890123456690");
        assert_eq!(to_fixed(1.0, 18), "1.000000000000000000");
        assert_eq!(to_fixed(1.0, 100).len(), 102); // "1." + 100 zeros
        assert_eq!(to_fixed(1.0, 100), format!("1.{}", "0".repeat(100)));
    }

    #[test]
    fn uint8_matches_js_semantics() {
        assert_eq!(crate::jsnum::to_uint8(256.0), 0);
        assert_eq!(crate::jsnum::to_uint8(-1.0), 255);
        assert_eq!(crate::jsnum::to_uint8(300.7), 44); // trunc 300 mod 256
        assert_eq!(crate::jsnum::to_uint8(f64::NAN), 0);
        assert_eq!(crate::jsnum::to_uint8(f64::INFINITY), 0);
    }
}
