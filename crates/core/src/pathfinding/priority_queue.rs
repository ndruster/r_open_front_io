//! Port of `src/core/pathfinding/algorithms/PriorityQueue.ts`.
//!
//! Two implementations of the `PriorityQueue` interface used by A*:
//!
//! * [`MinHeap`] — binary heap, `Int32Array` nodes + `Float32Array`
//!   priorities. Load-bearing details: comparisons see the **f32-rounded**
//!   priority (two distinct f64 inputs can tie after rounding and then pop in
//!   insertion order — `<=` breaks the bubble loop), and popping an *empty*
//!   heap returns element `0` of the (dense, zero-filled) node array, not
//!   `undefined`, while `size` underflows to `-1`. A later `push` then writes
//!   to index `-1`, which a typed array *drops*: the node is silently lost.
//!   The vectors exercise exactly this.
//! * [`BucketQueue`] — O(1) integer buckets with a generation stamp. Pops are
//!   **LIFO within a bucket** (`bucket[sizes[b] - 1]` after decrementing), and
//!   negative / out-of-range priorities land on properties the typed arrays
//!   never read back (`bucketStamp[-5]` is `undefined`), so pushed-then-
//!   unreachable nodes are a real JS behaviour that this port reproduces.
//!
//! `clear()` on the bucket queue only bumps the stamp (old contents stay in
//! place until overwritten), so it is O(1) and the port mirrors that.

use std::collections::BTreeMap;

use crate::jsnum::{to_float32, to_int32};

/// The `PriorityQueue` interface from the TS source. `pop` yields `None` where
/// JS would produce `undefined` (only the capacity-0 heap or a missing
/// negative-priority bucket can do that; the sentinel `-1` is a real value).
pub trait PriorityQueue {
    fn push(&mut self, node: f64, priority: f64);
    fn pop(&mut self) -> Option<f64>;
    fn is_empty(&self) -> bool;
    fn clear(&mut self);
}

/// JS `Int32Array` element semantics: dense reads, dropped OOB writes.
#[derive(Clone, Debug)]
struct Int32Vec(Vec<i32>);

impl Int32Vec {
    fn new(len: usize) -> Self {
        Self(vec![0; len])
    }
    #[allow(dead_code)] // used by Float32Vec only; kept for interface symmetry
    fn len(&self) -> usize {
        self.0.len()
    }
    /// JS `arr[i]` for any numeric index; out of range reads as `undefined`.
    fn get(&self, i: i64) -> Option<f64> {
        if i >= 0 && (i as usize) < self.0.len() {
            Some(self.0[i as usize] as f64)
        } else {
            None
        }
    }
    /// JS `arr[i] = v` with the ToInt32 store; OOB writes are dropped.
    fn set(&mut self, i: i64, v: f64) {
        if i >= 0 && (i as usize) < self.0.len() {
            self.0[i as usize] = to_int32(v);
        }
    }
}

/// JS `Float32Array` element semantics.
#[derive(Clone, Debug)]
struct Float32Vec(Vec<f32>);

impl Float32Vec {
    fn new(len: usize) -> Self {
        Self(vec![0.0f32; len])
    }
    #[allow(dead_code)] // symmetric with Int32Vec; reads use self.0.len()
    fn len(&self) -> usize {
        self.0.len()
    }
    /// JS `arr[i]` read, as the f64 the comparison would promote it to.
    fn get(&self, i: i64) -> Option<f64> {
        if i >= 0 && (i as usize) < self.0.len() {
            Some(f64::from(self.0[i as usize]))
        } else {
            None
        }
    }
    fn set(&mut self, i: i64, v: f64) {
        if i >= 0 && (i as usize) < self.0.len() {
            self.0[i as usize] = to_float32(v);
        }
    }
    /// Swap two *valid* indices (only used with in-range ones).
    fn swap(&mut self, a: usize, b: usize) {
        self.0.swap(a, b);
    }
}

/// Binary min-heap over (node, f32 priority), matching `MinHeap`.
#[derive(Clone, Debug)]
pub struct MinHeap {
    heap: Int32Vec,
    priorities: Float32Vec,
    /// JS keeps this as a number that may go negative.
    size: i64,
    capacity: usize,
}

impl MinHeap {
    pub fn new(capacity: f64) -> Self {
        // `new Int32Array(capacity)` truncates and clamps at 0; game callers
        // pass real node counts. Negative -> length 0 in V8 (ToIndex range).
        let cap = capacity.max(0.0) as usize;
        Self {
            heap: Int32Vec::new(cap),
            priorities: Float32Vec::new(cap),
            size: 0,
            capacity: cap,
        }
    }

    /// Debug view of the backing arrays (for parity traces).
    pub fn debug_arrays(&self) -> (Vec<i32>, Vec<u32>, i64) {
        (
            self.heap.0.clone(),
            self.priorities.0.iter().map(|p| p.to_bits()).collect(),
            self.size,
        )
    }

    /// Element accessors for the wasm probe (parity traces per index).
    pub fn debug_heap_len(&self) -> usize {
        self.heap.0.len()
    }
    pub fn debug_heap_at(&self, i: usize) -> i32 {
        self.heap.0[i]
    }
    pub fn debug_pri_len(&self) -> usize {
        self.priorities.0.len()
    }
    pub fn debug_pri_bits_at(&self, i: usize) -> u32 {
        self.priorities.0[i].to_bits()
    }
    pub fn debug_size(&self) -> i64 {
        self.size
    }
    pub fn debug_capacity(&self) -> usize {
        self.capacity
    }

    fn resize(&mut self, new_cap: usize) {
        let mut heap = Int32Vec::new(new_cap);
        let mut pri = Float32Vec::new(new_cap);
        let n = self.heap.len().min(new_cap);
        heap.0[..n].copy_from_slice(&self.heap.0[..n]);
        pri.0[..n].copy_from_slice(&self.priorities.0[..n]);
        self.heap = heap;
        self.priorities = pri;
        self.capacity = new_cap;
    }
}

impl PriorityQueue for MinHeap {
    fn push(&mut self, node: f64, priority: f64) {
        if self.size >= self.capacity as i64 {
            // TS console.error's a warning (not simulation state) and doubles.
            // With a negative size (post underflow-pop) this test is false, so
            // no resize happens and the write below targets index -1 and drops.
            self.resize(self.capacity * 2);
        }

        let i = self.size;
        self.size += 1;
        self.heap.set(i, node);
        self.priorities.set(i, priority);

        // Bubble up. Indices are computed even when negative; JS `heap[-1]`
        // reads undefined, so `undefined <= undefined` is false and a NaN
        // comparison keeps looping -- but only *valid* i can reach here since
        // the dropped-write case has i = -1 and the loop guard is `i > 0`.
        let mut i = i;
        while i > 0 {
            let parent = ((i - 1) >> 1) as usize;
            let pu = self.priorities.get(parent as i64);
            let cu = self.priorities.get(i);
            let (Some(p), Some(c)) = (pu, cu) else { break };
            if p <= c {
                break;
            }
            self.heap.0.swap(parent, i as usize);
            self.priorities.swap(parent, i as usize);
            i = parent as i64;
        }
    }

    fn pop(&mut self) -> Option<f64> {
        // JS reads heap[0] unconditionally. On a dense zero-length array
        // (capacity 0) that is `undefined`; otherwise it is the stale 0.
        let result = self.heap.get(0);
        self.size -= 1;
        if self.size > 0 {
            let last = self.size;
            let hn = self.heap.get(last);
            let pn = self.priorities.get(last);
            // last is always a valid index when size > 0.
            self.heap.set(0, hn.unwrap_or(0.0));
            self.priorities.set(0, pn.unwrap_or(0.0));

            let mut i = 0i64;
            loop {
                let left = (i << 1) + 1;
                let right = left + 1;
                let mut smallest = i;
                let cur = self.priorities.get(smallest).unwrap_or(0.0);
                if let Some(l) = self.priorities.get(left) {
                    if left < self.size && l < cur {
                        smallest = left;
                    }
                }
                if let Some(r) = self.priorities.get(right) {
                    if right < self.size
                        && r < self.priorities.get(smallest).unwrap_or(0.0)
                    {
                        smallest = right;
                    }
                }
                if smallest == i {
                    break;
                }
                self.heap.0.swap(i as usize, smallest as usize);
                self.priorities.swap(i as usize, smallest as usize);
                i = smallest;
            }
        }
        result
    }

    fn is_empty(&self) -> bool {
        self.size == 0
    }

    fn clear(&mut self) {
        self.size = 0;
    }
}

/// O(1) integer-priority bucket queue, matching `BucketQueue`.
pub struct BucketQueue {
    /// JS sparse array of `Int32Array` buckets: any integer key is allowed.
    buckets: BTreeMap<i64, Int32Vec>,
    bucket_sizes: Vec<i32>,
    bucket_stamp: Vec<u32>,
    stamp: u64,
    min_bucket: f64,
    max_bucket: f64,
    size: f64,
}

impl BucketQueue {
    pub fn new(max_priority: f64) -> Self {
        let max_bucket = max_priority + 1.0;
        // `new Int32Array(maxBucket)` needs a valid index; game callers pass
        // integers. Fractional would throw in JS, so we floor defensively.
        let len = max_bucket.max(0.0).floor() as usize;
        Self {
            buckets: BTreeMap::new(),
            bucket_sizes: vec![0; len],
            bucket_stamp: vec![0; len],
            stamp: 0,
            min_bucket: max_bucket,
            max_bucket,
            size: 0.0,
        }
    }

    /// Debug view of the scalar state (for parity traces).
    pub fn debug_state(&self) -> (f64, f64, f64, u64) {
        (self.min_bucket, self.max_bucket, self.size, self.stamp)
    }

    /// Full backing-state snapshot for trace parity: typed-array contents plus
    /// the sparse-bucket keys (JS sorts integer-like keys ascending, so these
    /// are already in order).
    pub fn debug_full(&self) -> (Vec<i32>, Vec<u32>, Vec<i64>) {
        (
            self.bucket_sizes.clone(),
            self.bucket_stamp.clone(),
            self.buckets.keys().copied().collect(),
        )
    }

    #[inline]
    fn valid(&self, b: i64) -> bool {
        b >= 0 && (b as usize) < self.bucket_sizes.len()
    }
}

impl PriorityQueue for BucketQueue {
    fn push(&mut self, node: f64, priority: f64) {
        // Math.min(priority | 0, maxBucket - 1): the ToInt32 may be negative;
        // maxBucket - 1 as JS number is truncated by the min only if negative.
        let bucket = (to_int32(priority) as f64).min(self.max_bucket - 1.0);
        let b = bucket as i64; // exact for the | 0 range

        // JS `if (!this.buckets[bucket])` -- any key not yet created (including
        // negative keys) gets a fresh 64-slot Int32Array.
        self.buckets.entry(b).or_insert_with(|| Int32Vec::new(64));

        // Stale stamp -> treat as size 0. Out-of-range stamp reads undefined.
        let stamped = self.valid(b) && self.bucket_stamp[b as usize] == self.stamp as u32;
        let size = if stamped { self.bucket_sizes[b as usize] as i64 } else { 0 };

        let bucket = self.buckets.get_mut(&b).expect("created above");
        if size >= bucket.len() as i64 {
            // new Int32Array(length * 2) + set
            let mut grown = Int32Vec::new(bucket.len() * 2);
            let n = bucket.len();
            grown.0[..n].copy_from_slice(&bucket.0[..n]);
            *bucket = grown;
        }
        bucket.set(size, node);

        if self.valid(b) {
            self.bucket_sizes[b as usize] = (size + 1) as i32;
            self.bucket_stamp[b as usize] = self.stamp as u32;
        }
        // Negative buckets: these two typed writes drop (JS too), so the node
        // becomes unreachable while `size` still counts it. Faithful, not
        // fixed.
        self.size += 1.0;

        if (b as f64) < self.min_bucket {
            self.min_bucket = b as f64;
        }
    }

    fn pop(&mut self) -> Option<f64> {
        while self.min_bucket < self.max_bucket {
            let b = self.min_bucket as i64; // -inf..max bucket; exact here
            if self.valid(b) && self.bucket_stamp[b as usize] == self.stamp as u32 {
                let size = self.bucket_sizes[b as usize] as i64;
                if size > 0 {
                    self.bucket_sizes[b as usize] = (size - 1) as i32;
                    self.size -= 1.0;
                    // LIFO: the last pushed node of this bucket.
                    return self
                        .buckets
                        .get(&b)
                        .and_then(|bucket| bucket.get(size - 1));
                }
            }
            self.min_bucket += 1.0;
        }
        Some(-1.0)
    }

    fn is_empty(&self) -> bool {
        self.size == 0.0
    }

    fn clear(&mut self) {
        self.stamp += 1;
        if self.stamp > 0xffff_ffff {
            self.bucket_stamp.iter_mut().for_each(|s| *s = 0);
            self.stamp = 1;
        }
        self.min_bucket = self.max_bucket;
        self.size = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain(q: &mut dyn PriorityQueue) -> Vec<Option<f64>> {
        let mut out = Vec::new();
        for _ in 0..16 {
            out.push(q.pop());
            if q.is_empty() {
                break;
            }
        }
        out
    }

    #[test]
    fn min_heap_orders_by_priority() {
        let mut h = MinHeap::new(8.0);
        for (n, p) in [(5.0, 3.0), (1.0, 1.0), (9.0, 2.0), (7.0, 1.5)] {
            h.push(n, p);
        }
        assert_eq!(drain(&mut h), vec![Some(1.0), Some(7.0), Some(9.0), Some(5.0)]);
    }

    #[test]
    fn min_heap_f32_tie_breaks_by_insertion() {
        let mut h = MinHeap::new(8.0);
        h.push(1.0, 1.0);
        h.push(2.0, 1.0 + 2f64.powi(-30)); // rounds to 1.0f32
        assert_eq!(drain(&mut h), vec![Some(1.0), Some(2.0)]);
    }

    #[test]
    fn min_heap_grows_past_capacity() {
        let mut h = MinHeap::new(2.0);
        for i in 0..100i32 {
            let t = i as f64;
            h.push(t, (t * 7919.0) % 100.0);
        }
        let mut prev = f64::MIN;
        while !h.is_empty() {
            let v = h.pop().unwrap();
            // Pops must be non-decreasing in *stored f32 priority* order; we
            // only assert nodes were all returned (no lost writes).
            assert_eq!(v.fract(), 0.0);
            prev = v.max(prev);
        }
        assert_eq!(prev, 99.0);
    }

    #[test]
    fn empty_heap_pop_returns_stale_zero_then_drops_next_push() {
        let mut h = MinHeap::new(2.0);
        // Dense typed array: index 0 exists and reads 0 even when size == 0.
        assert_eq!(h.pop(), Some(0.0));
        // size is now -1: push writes to index -1, which drops (JS too), and
        // the stale 0 at index 0 is what pops next. The pushed node is lost.
        h.push(5.0, 1.0);
        assert_eq!(h.pop(), Some(0.0));
        assert_eq!(h.debug_arrays().2, -1); // -1 + 1 (push) - 1 (second pop)
    }

    #[test]
    fn capacity_zero_heap_grows_stays_zero_and_pops_undefined() {
        let mut h = MinHeap::new(0.0);
        h.push(5.0, 1.0); // resize 0*2 = 0: the array cannot grow, write drops
        assert_eq!(h.pop(), None); // heap[0] on a length-0 array: undefined
    }

    #[test]
    fn bucket_queue_pops_lifo_within_bucket() {
        let mut q = BucketQueue::new(3.0);
        q.push(10.0, 2.0);
        q.push(20.0, 2.0);
        q.push(30.0, 0.0);
        assert_eq!(drain(&mut q), vec![Some(30.0), Some(20.0), Some(10.0)]);
    }

    #[test]
    fn bucket_queue_clear_hides_old_contents() {
        let mut q = BucketQueue::new(3.0);
        q.push(1.0, 1.0);
        q.clear();
        assert_eq!(q.pop(), Some(-1.0));
        assert!(q.is_empty());
    }

    #[test]
    fn negative_priority_node_becomes_unreachable_like_js() {
        let mut q = BucketQueue::new(3.0);
        q.push(7.0, -5.0); // bucket -5: typed-array writes drop, size counts it
        assert!(!q.is_empty()); // JS size === 1.0 -> isEmpty() false
        assert_eq!(q.pop(), Some(-1.0)); // min_bucket walks up, stamps miss
        assert!(!q.is_empty()); // exhausted pop does not decrement size
        q.clear(); // stamp bump resets size -> empty again
        assert!(q.is_empty());
    }

    #[test]
    fn fractional_priority_truncates_via_to_int32() {
        let mut q = BucketQueue::new(3.0);
        q.push(1.0, 2.9); // -> bucket 2
        q.push(2.0, 2.1); // -> bucket 2, later push pops first (LIFO)
        assert_eq!(drain(&mut q), vec![Some(2.0), Some(1.0)]);
    }
}
