//! `src/core/game/TileTraversalScratch.ts` — shared per-game traversal scratch.
//!
//! A generation-stamped `visited` array (one `Uint32Array` slot per tile) plus
//! a reusable `stack` and an `Int32Array` cluster-index map, cached in a
//! `WeakMap` keyed by the `Game` object so repeated queries on one game reuse
//! the same buffers. The port models the `Game` collaborator by its two
//! observable reads (`width()` / `height()`) and keys the cache by a capture-
//! assigned `refid` — two handles to one game share a refid, exactly as the
//! `WeakMap` keys on object identity.
//!
//! Faithfulness notes:
//!
//! * `new Uint32Array(totalTiles)` / `new Int32Array(totalTiles)` run the JS
//!   `ToIndex` coercion: `NaN` (and `±0`) allocate length `0`, a fractional
//!   length truncates toward zero (`1.5` -> length `1`), and a negative or
//!   `≥ 2^53` or `±Infinity` length throws `RangeError`. The `totalTiles` that
//!   reaches here is `width() * height()` of a real
//!   `GameMap`, i.e. a non-negative exact integer, so the throw branch is a
//!   boundary the capture pins directly (returned as `[1]`, mirroring the TS
//!   `throw`, never panicking the wasm instance).
//! * The reuse test is `scratch.visited.length < totalTiles` — a *shrinking*
//!   game keeps its oversized buffers (the `visited.length` stays the old,
//!   larger allocation), only a *growing* one reallocates. An equal size reuses.
//! * `bumpTraversalGeneration` is `gen++` then `if (gen === 0xffffffff)
//!   { visited.fill(0); gen = 1 }`. `gen` is a plain `f64`; `0xffffffff` is
//!   exactly representable, so the wrap is a straight comparison.
//! * `visited` / `clusterIndexMap` element writes are typed-array writes:
//!   out-of-range reads are `undefined` (crossed as `NaN`), out-of-range writes
//!   are silently dropped, and in-range values pass through `ToUint32` /
//!   `ToInt32`.

use std::collections::HashMap;

use crate::jsnum::{to_int32, to_uint32};

/// `0xffffffff` — the generation value that triggers the `fill(0)` wrap.
const GEN_WRAP: f64 = 4_294_967_295.0;

/// One `TileTraversalScratch`. `visited` / `cluster_index_map` are the typed
/// arrays; `stack` is a plain `TileRef[]` (only its length is observable here —
/// the module never pushes to it, its owners do).
#[derive(Debug)]
struct Scratch {
    visited: Vec<u32>,
    stack: Vec<f64>,
    cluster_index_map: Vec<i32>,
    gen: f64,
}

/// JS `ToIndex` for the `new UintNArray(len)` allocation: `ToIntegerOrInfinity`
/// truncation, then the `0 ..= 2^53 - 1` interval check (`RangeError` outside).
/// Returns the length, or `None` when the coercion throws.
fn to_index(v: f64) -> Option<usize> {
    // ToIntegerOrInfinity: NaN -> +0, ±0 -> +0, ±Inf -> ±Inf, else trunc.
    let t = if v.is_nan() || v == 0.0 {
        0.0
    } else if !v.is_finite() {
        return None; // ±Infinity outside the interval -> RangeError
    } else {
        v.trunc()
    };
    if t < 0.0 {
        return None; // negative -> RangeError
    }
    if t > 9_007_199_254_740_991.0 {
        return None; // > 2^53 - 1 -> RangeError
    }
    Some(t as usize)
}

/// Canonical numeric index for typed-array element access: unlike allocation's
/// `ToIndex`, a fractional index is NOT coerced — it addresses a plain object
/// property (invisible to the buffer) and the write is dropped / the read is
/// `undefined`. Only exact integers `0 ..< 2^53` hit the buffer.
fn canon_index(v: f64) -> Option<usize> {
    if !v.is_finite() || v < 0.0 || v != v.trunc() {
        return None;
    }
    if v > 9_007_199_254_740_991.0 {
        return None;
    }
    Some(v as usize)
}

/// Stateful parity harness: owns the `WeakMap` cache (keyed by game refid) and
/// replays the recorded op stream. `kind`:
/// 0 `tileTraversalScratch(refid, width, height)` → `[1]` (ToIndex throw) or
/// `[0, visited_len, stack_len, cluster_len, gen]`;
/// 1 `bumpTraversalGeneration(refid)` → `[gen]`;
/// 2 `set_gen(refid, g)` (capture pre-arms the wrap) → `[gen]`;
/// 3 `write_visited(refid, i, v)` (typed-array write + read-back) → `[val]`;
/// 4 `push_stack(refid, tile)` → `[stack_len]`;
/// 5 `read_visited(refid, i)` → `[val]` (`NaN` for an out-of-range read);
/// 6 `write_cluster(refid, i, v)` → `[val]` (`ToInt32` write + read-back).
#[derive(Default)]
pub struct RigHarness {
    scratches: HashMap<u64, Scratch>,
}

fn ref_key(v: f64) -> u64 {
    if v == 0.0 {
        0.0f64.to_bits()
    } else {
        v.to_bits()
    }
}

impl RigHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.scratches.clear();
    }

    fn get(&self, refid: f64) -> Option<&Scratch> {
        self.scratches.get(&ref_key(refid))
    }

    /// `tileTraversalScratch(game)`.
    pub fn tile_traversal_scratch(&mut self, refid: f64, w: f64, h: f64) -> Vec<f64> {
        let total = w * h;
        let n = match to_index(total) {
            Some(n) => n,
            None => return vec![1.0],
        };
        let reuse = match self.get(refid) {
            // `scratch.visited.length < totalTiles` -> reallocate; otherwise
            // keep the existing (possibly larger) buffers. The negation is
            // load-bearing: when `total` is NaN the `<` is false, so `!` yields
            // `true` (reuse) — exactly the TS `!scratch || (len < NaN)` branch.
            #[allow(clippy::neg_cmp_op_on_partial_ord)]
            Some(s) => !((s.visited.len() as f64) < total),
            None => false,
        };
        if !reuse {
            self.scratches.insert(
                ref_key(refid),
                Scratch {
                    visited: vec![0u32; n],
                    stack: Vec::new(),
                    cluster_index_map: vec![0i32; n],
                    gen: 0.0,
                },
            );
        }
        let s = self.get(refid).unwrap();
        vec![
            0.0,
            s.visited.len() as f64,
            s.stack.len() as f64,
            s.cluster_index_map.len() as f64,
            s.gen,
        ]
    }

    /// `bumpTraversalGeneration(scratch)`.
    pub fn bump(&mut self, refid: f64) -> Vec<f64> {
        let s = self.expect(refid);
        s.gen += 1.0;
        if s.gen == GEN_WRAP {
            s.visited.fill(0);
            s.gen = 1.0;
        }
        vec![s.gen]
    }

    fn expect(&mut self, refid: f64) -> &mut Scratch {
        self.scratches
            .get_mut(&ref_key(refid))
            .expect("tile_traversal_scratch harness: no scratch for refid")
    }

    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        match kind {
            0 => self.tile_traversal_scratch(args[0], args[1], args[2]),
            1 => self.bump(args[0]),
            2 => {
                let s = self.expect(args[0]);
                s.gen = args[1];
                vec![s.gen]
            }
            3 => {
                let s = self.expect(args[0]);
                let i = args[1];
                let v = args[2];
                write_typed_u32(&mut s.visited, i, v)
            }
            4 => {
                let s = self.expect(args[0]);
                s.stack.push(args[1]);
                vec![s.stack.len() as f64]
            }
            5 => {
                let s = self.expect(args[0]);
                read_typed_u32(&s.visited, args[1])
            }
            6 => {
                let s = self.expect(args[0]);
                let i = args[1];
                let v = args[2];
                write_typed_i32(&mut s.cluster_index_map, i, v)
            }
            _ => panic!("bad tile_traversal_scratch op kind {kind}"),
        }
    }
}

/// `visited[i] = v` on a `Uint32Array` (ToUint32 write, out-of-range dropped),
/// then read back `[visited[i]]` (out-of-range read is `undefined` -> `NaN`).
/// Non-canonical indices (NaN / fractional / negative) address plain object
/// properties in JS; the capture never emits them and the harness drops the
/// write / reads `NaN`.
fn write_typed_u32(arr: &mut [u32], i: f64, v: f64) -> Vec<f64> {
    if let Some(k) = canon_index(i) {
        if k < arr.len() {
            arr[k] = to_uint32(v);
        }
    }
    read_typed_u32(arr, i)
}

fn read_typed_u32(arr: &[u32], i: f64) -> Vec<f64> {
    match canon_index(i) {
        Some(k) if k < arr.len() => vec![arr[k] as f64],
        _ => vec![f64::NAN],
    }
}

/// `clusterIndexMap[i] = v` on an `Int32Array` (ToInt32 write, out-of-range
/// dropped), then read back (out-of-range -> `NaN`).
fn write_typed_i32(arr: &mut [i32], i: f64, v: f64) -> Vec<f64> {
    if let Some(k) = canon_index(i) {
        if k < arr.len() {
            arr[k] = to_int32(v);
        }
    }
    match canon_index(i) {
        Some(k) if k < arr.len() => vec![arr[k] as f64],
        _ => vec![f64::NAN],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_index_matches_js() {
        assert_eq!(to_index(f64::NAN), Some(0));
        assert_eq!(to_index(-0.0), Some(0));
        assert_eq!(to_index(4.0), Some(4));
        assert_eq!(to_index(-1.0), None);
        assert_eq!(to_index(1.5), Some(1)); // truncates, does not throw
        assert_eq!(to_index(f64::INFINITY), None);
        assert_eq!(to_index(9_007_199_254_740_992.0), None);
    }

    #[test]
    fn canon_index_matches_typed_array_access() {
        assert_eq!(canon_index(2.0), Some(2));
        assert_eq!(canon_index(1.5), None); // plain-property index, not the buffer
        assert_eq!(canon_index(-1.0), None);
        assert_eq!(canon_index(f64::NAN), None);
    }

    #[test]
    fn allocate_and_reuse() {
        let mut h = RigHarness::new();
        assert_eq!(h.tile_traversal_scratch(1.0, 2.0, 2.0), vec![0.0, 4.0, 0.0, 4.0, 0.0]);
        // Same size reuses (not <).
        assert_eq!(h.tile_traversal_scratch(1.0, 2.0, 2.0), vec![0.0, 4.0, 0.0, 4.0, 0.0]);
        // Shrinking keeps the oversized buffers.
        assert_eq!(h.tile_traversal_scratch(1.0, 1.0, 2.0), vec![0.0, 4.0, 0.0, 4.0, 0.0]);
        // Growing reallocates (gen resets to 0).
        h.run_op(2, &[1.0, 7.0]);
        assert_eq!(h.tile_traversal_scratch(1.0, 3.0, 3.0), vec![0.0, 9.0, 0.0, 9.0, 0.0]);
    }

    #[test]
    fn distinct_games_get_distinct_scratches() {
        let mut h = RigHarness::new();
        h.tile_traversal_scratch(1.0, 2.0, 2.0);
        h.tile_traversal_scratch(2.0, 4.0, 4.0);
        // game 2 keeps its own 16-slot buffer.
        assert_eq!(h.tile_traversal_scratch(2.0, 2.0, 2.0), vec![0.0, 16.0, 0.0, 16.0, 0.0]);
    }

    #[test]
    fn throw_on_negative_total() {
        let mut h = RigHarness::new();
        assert_eq!(h.tile_traversal_scratch(1.0, -1.0, 5.0), vec![1.0]);
        // Fractional total truncates to length 1 (no throw).
        assert_eq!(h.tile_traversal_scratch(3.0, 1.5, 1.0), vec![0.0, 1.0, 0.0, 1.0, 0.0]);
    }

    #[test]
    fn bump_wraps_and_fills() {
        let mut h = RigHarness::new();
        h.tile_traversal_scratch(1.0, 2.0, 2.0);
        h.run_op(3, &[1.0, 0.0, 99.0]); // visited[0] = 99
        assert_eq!(h.run_op(5,&[1.0, 0.0]), vec![99.0]);
        h.run_op(2,&[1.0, GEN_WRAP - 1.0]); // arm the wrap
        assert_eq!(h.bump(1.0), vec![1.0]); // 0xffffffff -> fill(0), gen=1
        assert_eq!(h.run_op(5,&[1.0, 0.0]), vec![0.0]);
    }

    #[test]
    fn typed_array_out_of_range() {
        let mut h = RigHarness::new();
        h.tile_traversal_scratch(1.0, 2.0, 2.0); // len 4
        // In-range write/read.
        assert_eq!(h.run_op(3, &[1.0, 1.0, 4_294_967_296.0 + 5.0]), vec![5.0]); // ToUint32 wrap
        // Out-of-range write is dropped, read is undefined -> NaN.
        assert!(h.run_op(3, &[1.0, 99.0, 7.0])[0].is_nan());
        assert!(h.run_op(5,&[1.0, 99.0])[0].is_nan());
    }

    #[test]
    fn cluster_int32_write() {
        let mut h = RigHarness::new();
        h.tile_traversal_scratch(1.0, 2.0, 2.0);
        assert_eq!(h.run_op(6,&[1.0, 0.0, -1.0]), vec![-1.0]);
        assert_eq!(h.run_op(6,&[1.0, 1.0, 4_294_967_295.0]), vec![-1.0]); // ToInt32 wrap
    }

    #[test]
    fn stack_push_observable() {
        let mut h = RigHarness::new();
        h.tile_traversal_scratch(1.0, 2.0, 2.0);
        assert_eq!(h.run_op(4,&[1.0, 10.0]), vec![1.0]);
        assert_eq!(h.run_op(4,&[1.0, 20.0]), vec![2.0]);
    }
}
