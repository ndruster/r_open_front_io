//! Port of `src/core/game/TileSet.ts`.
//!
//! `TileSet` is an insertion-ordered set of tile refs backed by a `Uint32Array`
//! (`dense`) plus an open-addressing `Int32Array` hash table. Every typed-array
//! coercion and the deferred-compaction bookkeeping are observable, so this is a
//! literal transcription:
//!
//! * `dense[di] = value` stores `ToUint32(value)`; the membership test is
//!   `dense[di] === value` where the left side is the *stored* u32 widened to
//!   f64 and the right side is the raw argument. So `add(-1)` stores
//!   `0xffffffff`, `has(-1)` then compares `4294967295 === -1` → **false**, and
//!   `has(4294967295)` → true. Two args that share a `ToUint32` image but differ
//!   as f64 (e.g. `0` and `2**32`) are treated as distinct on the probe yet
//!   collide in storage — reproduced exactly.
//! * `hash` is `Math.imul(v, 0x9e3779b1)` (both sides `ToInt32`) then
//!   `(h ^ (h >>> 15)) >>> 0`. `rehash` feeds the *stored* u32 to `hash`; that
//!   agrees with the raw-arg probe because `ToInt32(ToUint32(x)) === ToInt32(x)`.
//! * `table` holds dense indices, `EMPTY = -1`, `DELETED = -2`. Probes advance
//!   `slot = (slot + 1) & mask` and always terminate because the load factor
//!   keeps at least one `EMPTY` slot.
//! * Tombstones (`TOMBSTONE = 0xffffffff`) are reclaimed by `compact`, which is
//!   deferred while `iterDepth > 0` so live iteration never sees positions shift.
//! * `clear()` reallocates both buffers to the constructor defaults (dense 16,
//!   table 32 filled `EMPTY`); `iterDepth` is *not* reset (matching the TS).

use crate::jsnum::{to_int32, to_uint32};

const TOMBSTONE: u32 = 0xffff_ffff;
const EMPTY: i32 = -1;
const DELETED: i32 = -2;

/// `TileSet.hash(value)`: `Math.imul(value, 0x9e3779b1)` then
/// `(h ^ (h >>> 15)) >>> 0`, as a u32.
fn hash(value: f64) -> u32 {
    // Math.imul: ToInt32 both operands, multiply, truncate to i32.
    let a = to_int32(value) as i64;
    let b = to_int32(0x9e37_79b1_u32 as f64) as i64; // = -1640531535
    let h = (a.wrapping_mul(b) as u32) as i32; // the i32 Math.imul returns
    let hu = h as u32;
    hu ^ (hu >> 15)
}

/// `nextCapacity(n)`: smallest power of two `>= n` (and `>= 16`).
fn next_capacity(n: f64) -> usize {
    let mut cap = 16usize;
    while (cap as f64) < n {
        cap *= 2;
    }
    cap
}

/// `TileSet` from the TS source.
pub struct TileSet {
    dense: Vec<u32>,
    dense_len: usize,
    size_: f64,
    table: Vec<i32>,
    table_used: f64,
    iter_depth: f64,
}

impl TileSet {
    /// `new TileSet(values?)`.
    pub fn new(values: Option<&[f64]>) -> Self {
        let mut s = Self {
            dense: vec![0u32; 16],
            dense_len: 0,
            size_: 0.0,
            table: vec![EMPTY; 32],
            table_used: 0.0,
            iter_depth: 0.0,
        };
        if let Some(vs) = values {
            for &v in vs {
                s.add(v);
            }
        }
        s
    }

    /// `get size()`.
    pub fn size(&self) -> f64 {
        self.size_
    }

    /// `has(value)`.
    pub fn has(&self, value: f64) -> bool {
        let mask = self.table.len() as i64 - 1;
        let mut slot = (hash(value) as i64) & mask;
        loop {
            let di = self.table[slot as usize];
            if di == EMPTY {
                return false;
            }
            if di != DELETED && self.dense[di as usize] as f64 == value {
                return true;
            }
            slot = (slot + 1) & mask;
        }
    }

    /// `add(value)`.
    pub fn add(&mut self, value: f64) {
        let mask = self.table.len() as i64 - 1;
        let mut slot = (hash(value) as i64) & mask;
        let mut insert_at: i64 = -1;
        loop {
            let di = self.table[slot as usize];
            if di == EMPTY {
                if insert_at == -1 {
                    insert_at = slot;
                }
                break;
            }
            if di == DELETED {
                if insert_at == -1 {
                    insert_at = slot;
                }
            } else if self.dense[di as usize] as f64 == value {
                return;
            }
            slot = (slot + 1) & mask;
        }

        if self.dense_len < self.dense.len()
            && (self.table_used + 1.0) * 4.0 <= (self.table.len() as f64) * 3.0
        {
            // Fast path: no growth or rehash needed.
            let di = self.dense_len;
            self.dense_len += 1;
            self.dense[di] = to_uint32(value);
            self.size_ += 1.0;
            if self.table[insert_at as usize] == EMPTY {
                self.table_used += 1.0;
            }
            self.table[insert_at as usize] = di as i32;
            return;
        }

        if self.dense_len == self.dense.len() {
            if self.iter_depth == 0.0 && ((self.dense_len as f64) - self.size_) >= self.size_ {
                self.compact(self.dense.len() as f64);
            } else {
                let mut grown = vec![0u32; self.dense.len() * 2];
                grown[..self.dense.len()].copy_from_slice(&self.dense);
                self.dense = grown;
            }
        }
        if (self.table_used + 1.0) * 4.0 > (self.table.len() as f64) * 3.0 {
            self.rehash(if self.size_ * 4.0 > self.table.len() as f64 {
                (self.table.len() * 2) as f64
            } else {
                self.table.len() as f64
            });
        }

        let di = self.dense_len;
        self.dense_len += 1;
        self.dense[di] = to_uint32(value);
        self.size_ += 1.0;
        let mask = self.table.len() as i64 - 1;
        let mut slot = (hash(value) as i64) & mask;
        while self.table[slot as usize] >= 0 {
            slot = (slot + 1) & mask;
        }
        if self.table[slot as usize] == EMPTY {
            self.table_used += 1.0;
        }
        self.table[slot as usize] = di as i32;
    }

    /// `delete(value)`.
    pub fn delete(&mut self, value: f64) -> bool {
        let mask = self.table.len() as i64 - 1;
        let mut slot = (hash(value) as i64) & mask;
        loop {
            let di = self.table[slot as usize];
            if di == EMPTY {
                return false;
            }
            if di != DELETED && self.dense[di as usize] as f64 == value {
                self.table[slot as usize] = DELETED;
                self.dense[di as usize] = TOMBSTONE;
                self.size_ -= 1.0;
                if self.iter_depth == 0.0
                    && self.dense_len >= 64
                    && ((self.dense_len as f64) - self.size_) > self.size_ * 2.0
                {
                    self.compact(next_capacity(self.size_) as f64);
                }
                return true;
            }
            slot = (slot + 1) & mask;
        }
    }

    /// `clear()`.
    pub fn clear(&mut self) {
        self.dense = vec![0u32; 16];
        self.dense_len = 0;
        self.size_ = 0.0;
        self.table = vec![EMPTY; 32];
        self.table_used = 0.0;
    }

    /// `values()` collected to a `Vec` (no mutation during iteration): the live
    /// dense entries in insertion order, tombstones skipped, each the stored u32
    /// widened to f64.
    pub fn collect_values(&self) -> Vec<f64> {
        let mut out = Vec::new();
        for i in 0..self.dense_len {
            let v = self.dense[i];
            if v != TOMBSTONE {
                out.push(v as f64);
            }
        }
        out
    }

    // ---- re-entrant iteration (the `forEach` loop, caller-driven) ----
    //
    // TS `forEach` runs `for (let i = 0; i < this.denseLen; i++)` with the
    // callback *inside* the loop body: the callback may `add` (the grown
    // buffer and the extended denseLen are re-read every step, so appended
    // entries are visited) or `delete` (not-yet-visited slots tombstone and
    // are skipped). `iterDepth` gates compaction for exactly this reason.
    // The port exposes the same loop as begin/next/end so a replay can
    // interleave mutations between visits just like a JS callback does.

    /// Enter a `forEach`: `iterDepth++`.
    pub fn iter_begin(&mut self) {
        self.iter_depth += 1.0;
    }

    /// Leave a `forEach`: `iterDepth--` (the TS `finally`).
    pub fn iter_end(&mut self) {
        self.iter_depth -= 1.0;
    }

    /// One loop step: read `denseLen`/`dense[i]` fresh, advance the cursor,
    /// and yield the next live value (`None` ends the loop).
    pub fn iter_next(&self, cursor: &mut usize) -> Option<f64> {
        while *cursor < self.dense_len {
            let v = self.dense[*cursor];
            *cursor += 1;
            if v != TOMBSTONE {
                return Some(v as f64);
            }
        }
        None
    }

    /// `compact(capacity)`.
    fn compact(&mut self, capacity: f64) {
        let cap = (capacity.max(16.0)) as usize;
        let mut compacted = vec![0u32; cap];
        let mut n = 0usize;
        for i in 0..self.dense_len {
            let v = self.dense[i];
            if v != TOMBSTONE {
                compacted[n] = v;
                n += 1;
            }
        }
        self.dense = compacted;
        self.dense_len = n;
        self.rehash((next_capacity((n as f64) * 2.0).max(32)) as f64);
    }

    /// `rehash(tableLength)`.
    fn rehash(&mut self, table_length: f64) {
        let len = table_length as usize;
        let mut table = vec![EMPTY; len];
        let mask = len as i64 - 1;
        for di in 0..self.dense_len {
            if self.dense[di] == TOMBSTONE {
                continue;
            }
            let mut slot = (hash(self.dense[di] as f64) as i64) & mask;
            while table[slot as usize] != EMPTY {
                slot = (slot + 1) & mask;
            }
            table[slot as usize] = di as i32;
        }
        self.table = table;
        self.table_used = self.size_;
    }

    // ---- debug views for parity traces ----

    pub fn debug_dense(&self) -> &[u32] {
        &self.dense
    }
    pub fn debug_dense_len(&self) -> usize {
        self.dense_len
    }
    pub fn debug_table(&self) -> &[i32] {
        &self.table
    }
    pub fn debug_table_used(&self) -> f64 {
        self.table_used
    }
    pub fn debug_iter_depth(&self) -> f64 {
        self.iter_depth
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insertion_order_preserved() {
        let mut s = TileSet::new(None);
        for v in [5.0, 1.0, 9.0, 3.0] {
            s.add(v);
        }
        assert_eq!(s.collect_values(), vec![5.0, 1.0, 9.0, 3.0]);
        assert_eq!(s.size(), 4.0);
    }

    #[test]
    fn delete_then_readd_moves_to_end() {
        let mut s = TileSet::new(Some(&[1.0, 2.0, 3.0]));
        assert!(s.delete(2.0));
        s.add(2.0);
        assert_eq!(s.collect_values(), vec![1.0, 3.0, 2.0]);
        assert_eq!(s.size(), 3.0);
    }

    #[test]
    fn duplicate_add_is_noop() {
        let mut s = TileSet::new(None);
        s.add(7.0);
        s.add(7.0);
        assert_eq!(s.size(), 1.0);
        assert_eq!(s.collect_values(), vec![7.0]);
    }

    #[test]
    fn uint32_image_quirk_negative_vs_sentinel() {
        // add(-1) stores 0xffffffff. has(-1) is false (4294967295 !== -1),
        // has(0xffffffff) is true — and because the stored byte equals the
        // TOMBSTONE sentinel, iteration skips it (the TS has the same hole;
        // real tile refs never reach 2^32-1).
        let mut s = TileSet::new(None);
        s.add(-1.0);
        assert!(!s.has(-1.0));
        assert!(s.has(4294967295.0));
        assert_eq!(s.collect_values(), Vec::<f64>::new());
        assert_eq!(s.size(), 1.0);
        assert!(s.delete(4294967295.0));
        assert_eq!(s.size(), 0.0);
    }

    #[test]
    fn has_missing_is_false() {
        let s = TileSet::new(Some(&[10.0, 20.0]));
        assert!(!s.has(15.0));
        assert!(s.has(20.0));
    }

    #[test]
    fn clear_resets_buffers() {
        let mut s = TileSet::new(Some(&[1.0, 2.0, 3.0]));
        s.clear();
        assert_eq!(s.size(), 0.0);
        assert_eq!(s.debug_dense_len(), 0);
        assert_eq!(s.debug_dense().len(), 16);
        assert_eq!(s.debug_table().len(), 32);
        assert!(s.debug_table().iter().all(|&x| x == EMPTY));
    }

    #[test]
    fn growth_and_rehash_keep_membership() {
        let mut s = TileSet::new(None);
        for v in 0..200 {
            s.add(v as f64);
        }
        assert_eq!(s.size(), 200.0);
        for v in 0..200 {
            assert!(s.has(v as f64), "missing {v}");
        }
        assert!(!s.has(200.0));
        // dense grew past the initial 16.
        assert!(s.debug_dense().len() > 16);
    }
}
