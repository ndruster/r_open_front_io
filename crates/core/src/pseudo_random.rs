//! Bit-exact port of `src/core/PseudoRandom.ts`.
//!
//! The simulation runs on every client and the results are compared by hash, so
//! this generator must reproduce the TypeScript stream exactly. Two properties
//! make that possible and both are load-bearing:
//!
//! * The state is sfc32 and every operation is a 32-bit integer operation, so
//!   there is no platform-dependent rounding in the stream itself.
//! * `next()` is `(t as u32) as f64 / 2^32`, a division by a power of two,
//!   which is exact in IEEE-754. So matching `next_u32()` matches `next()`.
//!
//! The JS operators used by the TS source (`| 0`, `>>>`, `^`, `+`, `<<`) all
//! act on 32-bit bit patterns, and every one of them has the same bit pattern
//! as the corresponding `u32` operation in this module. The state is therefore
//! held as `u32` rather than `i32`; that is a representational choice, not a
//! behavioural change.

/// `36 ** 8`, the exclusive upper bound of the space `next_id` maps into.
const POW36_8: f64 = 2_821_109_907_456.0;

const BASE36: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";

/// Converts a JS number to the value JS `| 0` (ToInt32) would produce.
///
/// Rounds toward zero, then reduces modulo 2^32 and reinterprets the result as
/// the signed int32 JS would produce. Non-finite inputs become zero. Beyond
/// 2^53 the value has no fractional part in binary64, so `trunc` is a no-op and
/// the float modulo still matches the JS remainder step.
fn js_to_int32_bits(v: f64) -> u32 {
    if !v.is_finite() {
        return 0;
    }
    let n = v.trunc();
    if n.abs() < 9_007_199_254_740_992.0 {
        // Exactly representable, so the integer modulo is the JS semantics.
        return (n as i64).rem_euclid(4_294_967_296) as u32;
    }
    n.rem_euclid(4_294_967_296.0) as u32
}

/// Splits one state word off `h`, matching the `split()` closure in the TS
/// constructor (splitmix32 / Murmur3 finalizer).
///
/// The JS version is written in signed `int32` with `>>>` for the logical
/// shifts; `| 0` after each step only reinterprets. Every operation here has
/// the same 32-bit pattern as its JS counterpart, so working in `u32` is
/// equivalent and avoids repeated casts.
fn splitmix32(h: &mut u32) -> u32 {
    *h = h.wrapping_add(0x9e37_79b9);
    let mut t = *h ^ (*h >> 16);
    t = t.wrapping_mul(0x21f0_aaad);
    t ^= t >> 15;
    t = t.wrapping_mul(0x735a_2d97);
    t ^ (t >> 15)
}

/// Deterministic 32-bit PRNG (sfc32) seeded via splitmix32.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PseudoRandom {
    s0: u32,
    s1: u32,
    s2: u32,
    s3: u32,
}

impl PseudoRandom {
    /// Seeds the generator. Seeds congruent mod 2^32 produce identical streams,
    /// and the fractional part is discarded, because the seed goes through
    /// ToInt32 exactly like `seed | 0` in the TS constructor.
    pub fn new(seed: f64) -> Self {
        let mut h = js_to_int32_bits(seed);
        let mut rng = Self {
            s0: splitmix32(&mut h),
            s1: splitmix32(&mut h),
            s2: splitmix32(&mut h),
            s3: splitmix32(&mut h),
        };
        // Diffuse low-entropy seeds (sequential tick numbers, small ints).
        for _ in 0..12 {
            rng.next_u32();
        }
        rng
    }

    /// The raw 32-bit output behind [`PseudoRandom::next`].
    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let t = self.s0.wrapping_add(self.s1).wrapping_add(self.s3);
        self.s3 = self.s3.wrapping_add(1);
        self.s0 = self.s1 ^ (self.s1 >> 9);
        self.s1 = self.s2.wrapping_add(self.s2 << 3);
        self.s2 = self.s2.rotate_left(21);
        self.s2 = self.s2.wrapping_add(t);
        t
    }

    /// Next value in `[0, 1)`.
    ///
    /// Deliberately named after the TS method it ports 1:1; a PRNG is not a
    /// bounded iterator, so `std::iter::Iterator` is intentionally not used.
    #[allow(clippy::should_implement_trait)]
    #[inline]
    pub fn next(&mut self) -> f64 {
        self.next_u32() as f64 / 4_294_967_296.0
    }

    /// Next value in `[min, max)`, with both bounds floored to integers first.
    pub fn next_int(&mut self, min: f64, max: f64) -> i64 {
        let lo = min.floor();
        let hi = max.floor();
        (self.next() * (hi - lo)).floor() as i64 + lo as i64
    }

    /// Next value in `[min, max)`.
    pub fn next_float(&mut self, min: f64, max: f64) -> f64 {
        self.next() * (max - min) + min
    }

    /// The numeric value behind [`PseudoRandom::next_id`]: `floor(next() *
    /// 36^8)`, rendered as base 36 zero-padded to 8 by `next_id`. Exact in
    /// f64 since `36^8 < 2^53`; exposed for the no-pointer parity probe.
    pub fn next_id_value(&mut self) -> f64 {
        (self.next() * POW36_8).floor()
    }

    /// Eight base-36 characters, zero padded, like `nextID()` in the TS source.
    pub fn next_id(&mut self) -> String {
        let value = self.next_id_value() as u64;
        let mut buf = *b"00000000";
        let mut i = 8usize;
        let mut v = value;
        while v > 0 && i > 0 {
            i -= 1;
            buf[i] = BASE36[(v % 36) as usize];
            v /= 36;
        }
        // `value < 36^8`, so it always fits in the 8 characters. BASE36 is
        // ASCII, so the buffer is valid UTF-8 by construction.
        String::from_utf8(buf.to_vec()).expect("BASE36 is ASCII")
    }

    /// `true` with probability `1 / odds`, i.e. `nextInt(0, odds) == 0`.
    pub fn chance(&mut self, odds: f64) -> bool {
        self.next_int(0.0, odds) == 0
    }

    /// Random element, or `None` for an empty slice. The TS version throws.
    pub fn rand_element<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        let index = self.rand_index(items.len())?;
        items.get(index)
    }

    /// The index `randFromSet` would land on: `nextInt(0, len)` as a `usize`,
    /// or `None` when `len` is zero. JS sets iterate in insertion order, so
    /// callers using an insertion-ordered container (e.g. `IndexSet`) can pick
    /// with this directly.
    pub fn rand_index(&mut self, len: usize) -> Option<usize> {
        if len == 0 {
            return None;
        }
        Some(self.next_int(0.0, len as f64) as usize)
    }

    /// Fisher-Yates shuffle of a copy, identical to the TS implementation:
    /// walks from the last element down and swaps with `nextInt(0, i + 1)`.
    pub fn shuffle_array<T: Clone>(&mut self, array: &[T]) -> Vec<T> {
        let mut result = array.to_vec();
        let len = result.len();
        let mut i = len;
        while i > 1 {
            i -= 1;
            let j = self.next_int(0.0, i as f64 + 1.0) as usize;
            result.swap(i, j);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_stream() {
        let a = PseudoRandom::new(42.0);
        let b = PseudoRandom::new(42.0);
        let (a, b) = (a.clone(), b.clone());
        let mut a = a;
        let mut b = b;
        for _ in 0..1000 {
            assert_eq!(a.next_u32(), b.next_u32());
        }
    }

    #[test]
    fn next_stays_in_unit_interval() {
        let mut r = PseudoRandom::new(7.0);
        for _ in 0..10_000 {
            let v = r.next();
            assert!((0.0..1.0).contains(&v), "{v} out of [0, 1)");
        }
    }

    #[test]
    fn next_int_bounds_are_respected() {
        let mut r = PseudoRandom::new(99.0);
        for _ in 0..10_000 {
            let v = r.next_int(3.0, 8.0);
            assert!((3..8).contains(&v), "{v} out of [3, 8)");
        }
        // Bounds are floored, so [1.9, 4.7) behaves like [1, 4).
        let mut r = PseudoRandom::new(5.0);
        for _ in 0..100 {
            let v = r.next_int(1.9, 4.7);
            assert!((1..4).contains(&v), "{v} out of [1, 4)");
        }
    }

    #[test]
    fn next_id_is_eight_base36_chars() {
        let mut r = PseudoRandom::new(123.0);
        for _ in 0..100 {
            let id = r.next_id();
            assert_eq!(id.len(), 8);
            assert!(id.bytes().all(|b| b.is_ascii_digit() || b.is_ascii_lowercase()));
        }
    }

    #[test]
    fn seed_truncation_matches_js_to_int32() {
        // 1234.9 and 1234 seed identically; so do 0 and 4294967296.
        assert_eq!(
            PseudoRandom::new(1234.9).next_u32(),
            PseudoRandom::new(1234.0).next_u32()
        );
        assert_eq!(
            PseudoRandom::new(4_294_967_296.0).next_u32(),
            PseudoRandom::new(0.0).next_u32()
        );
        assert_eq!(
            PseudoRandom::new(-1.5).next_u32(),
            PseudoRandom::new(-1.0).next_u32()
        );
    }

    #[test]
    fn shuffle_is_a_permutation_and_leaves_input_alone() {
        let input: Vec<i32> = (0..10).collect();
        let mut r = PseudoRandom::new(55.0);
        let shuffled = r.shuffle_array(&input);
        assert_eq!(input, (0..10).collect::<Vec<i32>>());
        let mut sorted = shuffled.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, input);
    }

    #[test]
    fn consecutive_seeds_are_not_correlated() {
        let mut distinct = std::collections::HashSet::new();
        for seed in 1000..1100 {
            distinct.insert(PseudoRandom::new(seed as f64).next_int(0.0, 100.0));
        }
        assert!(distinct.len() > 50, "only {} distinct values", distinct.len());
    }

    #[test]
    fn stream_is_roughly_uniform() {
        let mut r = PseudoRandom::new(1234.0);
        const N: usize = 20_000;
        let mut buckets = [0usize; 10];
        for _ in 0..N {
            buckets[(r.next() * 10.0).floor() as usize] += 1;
        }
        for count in buckets {
            assert!((1700..2300).contains(&count), "{count} outside expected band");
        }
    }
}
