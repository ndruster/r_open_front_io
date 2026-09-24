//! Port of `src/core/execution/utils/FlatBinaryHeap.ts`.
//!
//! (tile, priority) min-heap specialised for `AttackExecution`'s conquest
//! frontier. Ordering-sensitive details transcribed as-is:
//!
//! * priorities live in a `Float32Array`, so *stored* comparisons (`last_pri
//!   <= pri[child]`, `pri[right] < pri[left]`) see f32-rounded values;
//! * the *incoming* `priority` argument of `enqueue` is a raw f64 and is
//!   compared directly against f32 array reads (`priority >= pri[parent]`)
//!   -- a mixed-precision comparison that decides when sifting stops;
//! * `tiles` is a JS array (holes read as `undefined`), modelled with
//!   `Option<f64>`;
//! * `dequeue` on an empty heap throws in TS -- modelled as a panic.

use crate::jsnum::to_float32;

/// Min-heap of `(tile, f32 priority)` pairs.
#[derive(Clone, Debug)]
pub struct FlatBinaryHeap {
    pri: Vec<f32>,
    tiles: Vec<Option<f64>>,
    len: usize,
}

impl FlatBinaryHeap {
    pub fn new(capacity: usize) -> Self {
        Self {
            pri: vec![0.0f32; capacity],
            tiles: vec![None; capacity],
            len: 0,
        }
    }

    /// Remove every element without reallocating (TS `clear`).
    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// Current number of elements (TS `size`).
    pub fn size(&self) -> usize {
        self.len
    }

    /// (priority bits, tiles, len) for parity traces.
    pub fn debug_state(&self) -> (Vec<u32>, Vec<Option<f64>>, usize) {
        (
            self.pri.iter().map(|p| p.to_bits()).collect(),
            self.tiles.clone(),
            self.len,
        )
    }

    /// Insert a tile (TS `enqueue`).
    pub fn enqueue(&mut self, tile: f64, priority: f64) {
        if self.len == self.pri.len() {
            self.grow();
        }
        let mut i = self.len;
        self.len += 1;

        // sift-up: raw f64 `priority` vs stored f32, exactly like JS.
        while i > 0 {
            let parent = (i - 1) >> 1;
            if priority >= f64::from(self.pri[parent]) {
                break;
            }
            self.pri[i] = self.pri[parent];
            self.tiles[i] = self.tiles[parent];
            i = parent;
        }
        self.pri[i] = to_float32(priority);
        self.tiles[i] = Some(tile);
    }

    /// Pop the lowest-priority tile (TS `dequeue`); `None` models JS
    /// `undefined` reads from array holes.
    pub fn dequeue(&mut self) -> Option<f64> {
        if self.len == 0 {
            // TS: throw new Error("heap empty")
            panic!("heap empty");
        }

        let top_tile = self.tiles[0];

        self.len -= 1;
        let last_pri32 = self.pri[self.len];
        let last_pri = f64::from(last_pri32);
        let last_tile = self.tiles[self.len];

        // sift-down
        let mut i = 0usize;
        loop {
            let left = (i << 1) + 1;
            if left >= self.len {
                break;
            }
            let right = left + 1;
            let child = if right < self.len
                && f64::from(self.pri[right]) < f64::from(self.pri[left])
            {
                right
            } else {
                left
            };
            if last_pri <= f64::from(self.pri[child]) {
                break;
            }
            self.pri[i] = self.pri[child];
            self.tiles[i] = self.tiles[child];
            i = child;
        }
        self.pri[i] = last_pri32;
        self.tiles[i] = last_tile;
        top_tile
    }

    /// Double the underlying storage (TS private `grow`).
    fn grow(&mut self) {
        let new_cap = self.pri.len() << 1;

        let mut new_pri = vec![0.0f32; new_cap];
        new_pri[..self.pri.len()].copy_from_slice(&self.pri);
        self.pri = new_pri;

        // `this.tiles.length = newCap`: JS array length assignment extends
        // with holes.
        self.tiles.resize(new_cap, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pops_in_priority_order() {
        let mut h = FlatBinaryHeap::new(4);
        for (t, p) in [(5.0, 3.0), (1.0, 1.0), (9.0, 2.0), (7.0, 1.5)] {
            h.enqueue(t, p);
        }
        assert_eq!(h.size(), 4);
        assert_eq!(h.dequeue(), Some(1.0));
        assert_eq!(h.dequeue(), Some(7.0));
        assert_eq!(h.dequeue(), Some(9.0));
        assert_eq!(h.dequeue(), Some(5.0));
        assert_eq!(h.size(), 0);
    }

    #[test]
    fn stores_priorities_as_f32() {
        let mut h = FlatBinaryHeap::new(4);
        h.enqueue(1.0, 1.0 + 2f64.powi(-30)); // rounds to 1.0f32
        let (bits, _, _) = h.debug_state();
        assert_eq!(f32::from_bits(bits[0]), 1.0f32);
    }

    #[test]
    fn mixed_precision_sift_up_uses_raw_f64() {
        // 1.0f32 <= (1.0 + 2^-30) f64 stops the sift even though the parent
        // *stores* 1.0 after rounding: the comparison saw the raw value.
        let mut h = FlatBinaryHeap::new(8);
        h.enqueue(1.0, 1.0);
        h.enqueue(2.0, 1.0 + 2f64.powi(-30));
        assert_eq!(h.dequeue(), Some(1.0));
        assert_eq!(h.dequeue(), Some(2.0));
    }

    #[test]
    fn grows_past_capacity_and_clear_keeps_buffer() {
        let mut h = FlatBinaryHeap::new(2);
        for i in 0..64i32 {
            let t = i as f64;
            h.enqueue(t, ((t * 7919.0) % 100.0).floor());
        }
        assert_eq!(h.size(), 64);
        let first = h.dequeue();
        assert!(first.is_some());
        h.clear();
        assert_eq!(h.size(), 0);
        h.enqueue(1.0, 0.0);
        assert_eq!(h.dequeue(), Some(1.0));
    }

    #[test]
    #[should_panic(expected = "heap empty")]
    fn dequeue_on_empty_panics_like_ts_throw() {
        let mut h = FlatBinaryHeap::new(2);
        let _ = h.dequeue();
    }
}
