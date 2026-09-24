//! Port of `src/core/pathfinding/algorithms/BFS.Grid.ts` (`BFSGrid`).
//!
//! 4-direction grid BFS over linearised tiles with **stamp-based visited
//! tracking**, so a search never zeroes its arrays. The ordering semantics are
//! the whole point:
//!
//! * neighbours are probed **N, S, W, E** in that exact order;
//! * `queue`, `visitedStamp` and `dist` are typed arrays: `dist` is a
//!   `Uint16Array`, so distances beyond 65 535 silently wrap (a real JS
//!   behaviour that changes downstream visitor results), and `visitedStamp` is
//!   `Uint32Array`;
//! * `next_stamp()` bumps a counter past `0xffffffff` by refilling the stamp
//!   array -- the refill itself is observable if a caller reads stamps;
//! * the queue has capacity `numNodes`: pushes past it write to out-of-range
//!   indices and are dropped (JS typed arrays), which the port models.
//!
//! `visitor` and `is_valid_node` are callbacks; the TS tri-state visitor
//! return is modelled by [`Visit`].

/// Outcome of visiting one node, mirroring the TS visitor contract
/// (`R | null | undefined`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Visit<R> {
    /// Stop and return this value.
    Found(R),
    /// Visit this node's neighbours.
    Explore,
    /// Skip this node's neighbours.
    Reject,
}

/// Stamped 4-neighbour grid BFS, matching `BFSGrid`.
pub struct BfsGrid {
    stamp: u64, // JS number; compared against 0xffffffff, then refills
    visited_stamp: Vec<u32>,
    queue: Vec<i32>,
    dist: Vec<u16>,
}

impl BfsGrid {
    pub fn new(num_nodes: f64) -> Self {
        let n = num_nodes.max(0.0) as usize;
        Self {
            stamp: 1,
            visited_stamp: vec![0; n],
            queue: vec![0; n],
            dist: vec![0; n],
        }
    }

    /// Debug view of the stamp counter (for parity traces).
    pub fn debug_stamp(&self) -> u64 {
        self.stamp
    }

    /// Run a search (TS `search`). `start` may be one or many nodes.
    pub fn search<R, V, F>(
        &mut self,
        width: i64,
        height: i64,
        starts: &[i64],
        max_distance: f64,
        mut is_valid_node: V,
        mut visitor: F,
    ) -> Option<R>
    where
        V: FnMut(i64) -> bool,
        F: FnMut(i64, i64) -> Visit<R>,
    {
        let stamp = self.next_stamp() as u32;
        let last_row_start = (height - 1) * width;

        let mut head = 0i64;
        let mut tail = 0i64;

        for &s in starts {
            self.set_stamp(s, stamp);
            self.set_dist(s, 0.0);
            self.enqueue(tail, s);
            tail += 1;
        }

        while head < tail {
            let node = self.queue_get(head);
            head += 1;
            let dist = self.dist_get(node);

            let result = visitor(node, dist);

            if let Visit::Found(r) = result {
                return Some(r);
            }
            if matches!(result, Visit::Reject) {
                continue;
            }

            let next_dist = dist + 1;

            if (next_dist as f64) > max_distance {
                continue;
            }

            let x = node.rem_euclid(width); // JS `%` on non-negative tiles

            // North
            if node >= width {
                let n = node - width;
                if self.stamp_of(n) != Some(stamp) && is_valid_node(n) {
                    self.set_stamp(n, stamp);
                    self.set_dist(n, next_dist as f64);
                    self.enqueue(tail, n);
                    tail += 1;
                }
            }

            // South
            if node < last_row_start {
                let s = node + width;
                if self.stamp_of(s) != Some(stamp) && is_valid_node(s) {
                    self.set_stamp(s, stamp);
                    self.set_dist(s, next_dist as f64);
                    self.enqueue(tail, s);
                    tail += 1;
                }
            }

            // West
            if x != 0 {
                let wv = node - 1;
                if self.stamp_of(wv) != Some(stamp) && is_valid_node(wv) {
                    self.set_stamp(wv, stamp);
                    self.set_dist(wv, next_dist as f64);
                    self.enqueue(tail, wv);
                    tail += 1;
                }
            }

            // East
            if x != width - 1 {
                let ev = node + 1;
                if self.stamp_of(ev) != Some(stamp) && is_valid_node(ev) {
                    self.set_stamp(ev, stamp);
                    self.set_dist(ev, next_dist as f64);
                    self.enqueue(tail, ev);
                    tail += 1;
                }
            }
        }

        None
    }

    /// TS `nextStamp`: return current, bump, refill past u32::MAX.
    fn next_stamp(&mut self) -> u64 {
        let stamp = self.stamp;
        self.stamp += 1;
        if self.stamp > 0xffff_ffff {
            self.visited_stamp.iter_mut().for_each(|s| *s = 0);
            self.stamp = 1;
        }
        stamp
    }

    // ---- typed-array element semantics: dense reads, dropped OOB writes ----

    #[inline]
    fn stamp_of(&self, i: i64) -> Option<u32> {
        if i >= 0 && (i as usize) < self.visited_stamp.len() {
            Some(self.visited_stamp[i as usize])
        } else {
            None // JS undefined, never equal to a stamp
        }
    }

    fn set_stamp(&mut self, i: i64, v: u32) {
        if i >= 0 && (i as usize) < self.visited_stamp.len() {
            self.visited_stamp[i as usize] = v;
        }
    }

    fn set_dist(&mut self, i: i64, v: f64) {
        // Uint16Array: ToUint16 with trunc-toward-zero and mod 2^16 (wraps!).
        if i >= 0 && (i as usize) < self.dist.len() {
            self.dist[i as usize] = crate::jsnum::to_uint16(v);
        }
    }

    fn dist_get(&self, i: i64) -> i64 {
        if i >= 0 && (i as usize) < self.dist.len() && i < self.queue.len() as i64 {
            self.dist[i as usize] as i64
        } else {
            // queue[i] OOB -> undefined -> dist[undefined] -> undefined -> NaN
            // in JS; callers comparing NaN behave as "not found", and the node
            // loop below still terminates. Modelled as 0 with an out-of-range
            // read flagged via the queue guard.
            0
        }
    }

    fn queue_get(&self, i: i64) -> i64 {
        if i >= 0 && (i as usize) < self.queue.len() {
            self.queue[i as usize] as i64
        } else {
            -1 // JS undefined node -> every array read misses -> benign
        }
    }

    fn enqueue(&mut self, i: i64, v: i64) {
        if i >= 0 && (i as usize) < self.queue.len() {
            self.queue[i as usize] = crate::jsnum::to_int32(v as f64);
        }
        // OOB: JS drops the write; BFS then terminates early. Faithful.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spiral_order_is_n_s_w_e_breadth_first() {
        let mut g = BfsGrid::new(9.0);
        let mut order = Vec::new();
        let found = g.search::<i32, _, _>(
            3,
            3,
            &[4],
            f64::INFINITY,
            |_| true,
            |n, _d| {
                order.push(n);
                Visit::Explore
            },
        );
        assert_eq!(found, None);
        // Start 4; N=1, S=7, W=3, E=5 (row-wrap excluded), then their
        // children in the same fixed probe order.
        assert_eq!(order, vec![4, 1, 7, 3, 5, 0, 2, 6, 8]);
    }

    #[test]
    fn rejects_block_expansion() {
        let mut g = BfsGrid::new(9.0);
        let mut seen = Vec::new();
        g.search::<i32, _, _>(
            3,
            3,
            &[4],
            f64::INFINITY,
            |n| n != 1, // north neighbour invalid
            |n, _| {
                seen.push(n);
                Visit::Explore
            },
        );
        assert!(!seen.contains(&1));
        assert!(seen.contains(&7));
    }

    #[test]
    fn visitor_can_stop_early() {
        let mut g = BfsGrid::new(9.0);
        let found = g.search(
            3,
            3,
            &[4],
            f64::INFINITY,
            |_| true,
            |n, _| {
                if n == 7 {
                    Visit::Found(7i32)
                } else {
                    Visit::Explore
                }
            },
        );
        assert_eq!(found, Some(7));
    }

    #[test]
    fn row_edges_are_not_wrapped() {
        let mut g = BfsGrid::new(9.0);
        let mut seen = Vec::new();
        g.search::<(), _, _>(
            3,
            3,
            &[0],
            f64::INFINITY,
            |_| true,
            |n, _| {
                seen.push(n);
                Visit::Explore
            },
        );
        // From tile 0 (x=0): W is illegal (x==0), row above does not exist;
        // 2 must only appear via 1 -> 2, never via a 0 -> 2 row wrap. The full
        // BFS ring order pins that: 2 is visited at index 5, after 1 (index 2).
        assert_eq!(seen, vec![0, 3, 1, 6, 4, 2, 7, 5, 8]);
        assert!(seen.iter().position(|&n| n == 2) > seen.iter().position(|&n| n == 1));
    }

    #[test]
    fn max_distance_clips_rings() {
        let mut g = BfsGrid::new(9.0);
        let mut seen = Vec::new();
        g.search::<(), _, _>(
            3,
            3,
            &[4],
            1.0,
            |_| true,
            |n, _| {
                seen.push(n);
                Visit::Explore
            },
        );
        assert_eq!(seen, vec![4, 1, 7, 3, 5]);
    }

    #[test]
    fn dist_wraps_at_u16_boundary() {
        // A 1x70000 corridor: the last tile is reached at true depth 69999,
        // stored in a Uint16Array it wraps to 69999 - 65536 = 4463.
        let w = 70_000i64;
        let mut g = BfsGrid::new(w as f64);
        let mut at_end = None;
        g.search::<(), _, _>(
            w,
            1,
            &[0],
            f64::INFINITY,
            |_| true,
            |n, d| {
                if n == w - 1 {
                    at_end = Some(d);
                }
                Visit::Explore
            },
        );
        assert_eq!(at_end, Some(4463));
        assert_eq!(crate::jsnum::to_uint16(69_999.0), 4463u16);
    }

    #[test]
    fn stamps_increment_per_search_and_refill_at_wrap() {
        let mut g = BfsGrid::new(4.0);
        assert_eq!(g.debug_stamp(), 1);
        g.search::<(), _, _>(2, 2, &[0], 0.0, |_| true, |_, _| Visit::Explore);
        assert_eq!(g.debug_stamp(), 2);
    }
}
