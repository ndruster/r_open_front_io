//! Port of `src/core/pathfinding/algorithms/AStar.ts`.
//!
//! A* over integer node ids with the `BucketQueue` from [`super::priority_queue`]
//! and stamp-tracking arrays, so a single instance is reused across searches
//! without clearing anything. The simulation calls this for every army/ship
//! move on every client, so the *expansion order* (which node pops first among
//! equal priorities, which neighbour order feeds the bucket) is observable
//! through the final path. That makes typed-array semantics load-bearing here:
//!
//! * `gScore` is a **`Uint32Array`**: stores truncate+wrap (a negative tentative
//!   g-score comes back as a huge number, and the strict-f64 comparison
//!   `tentativeG < gScore[n]` then sees that wrapped value, not the original);
//! * `cameFrom` is an `Int32Array`; its `-1` start sentinel is mapped to
//!   `undefined` when passed to `adapter.cost` -- the adapter therefore never
//!   sees `-1`, only real predecessors or `None`;
//! * out-of-range array reads are `undefined` in JS: every comparison against
//!   them is false, every arithmetic on them is NaN, which propagates through
//!   tentative g-scores exactly like the TS does (the negative-cost scenario
//!   in the vectors exercises this via the bucket queue's `-1` pop sentinel);
//! * `neighbors` writes through an `Int32Array` buffer: JS `%`/`Math.floor`
//!   sign rules apply to whatever node value arrives (including `-1`).
//!
//! The stamp counter starts at 1 and is *incremented* at the top of each
//! `findPath`, so the first search runs with stamp 2; refill past `0xffffffff`
//! zeroes both stamp arrays. Reusing an instance across calls is covered by the
//! recorded per-call traces.

use crate::jsnum::{to_int32, to_uint32};
use crate::pathfinding::priority_queue::{BucketQueue, PriorityQueue};

/// The `AStarAdapter` interface of the TS source. All node values travel as
/// `f64` because that is what JS hands the adapter; implementations must use
/// JS arithmetic semantics (see [`crate::jsnum`]).
pub trait AStarAdapter {
    /// Write neighbours into `buffer` (JS `Int32Array`), return the count.
    fn neighbors(&mut self, node: f64, buffer: &mut [i32]) -> usize;
    /// `prev` is `None` where the TS passes `undefined`.
    fn cost(&mut self, from: f64, to: f64, prev: Option<f64>) -> f64;
    fn heuristic(&mut self, node: f64, goal: f64) -> f64;
    fn num_nodes(&self) -> f64;
    fn max_priority(&self) -> f64;
    fn max_neighbors(&self) -> usize;
}

/// `Math.floor(node / w)` for JS numbers (i64 div truncates the wrong way for
/// negative operands; typed-array OOB nodes make `-1` reachable).
#[inline]
fn js_floor_div(node: f64, w: f64) -> f64 {
    (node / w).floor()
}

/// `AStar` from the TS source, generic over the adapter.
pub struct AStar<A: AStarAdapter> {
    stamp: u64,
    closed_stamp: Vec<u32>,
    g_score_stamp: Vec<u32>,
    g_score: Vec<u32>,
    came_from: Vec<i32>,
    queue: BucketQueue,
    adapter: A,
    neighbor_buffer: Vec<i32>,
    max_iterations: f64,
}

// ---- typed-array element semantics (dense i32/u32, OOB -> undefined) ----

#[inline]
pub(crate) fn index_of(i: f64) -> Option<usize> {
    // Canonical numeric property: integer >= 0 only; -0 is "0".
    if i == 0.0 {
        return Some(0); // covers 0 and -0
    }
    if i.fract() != 0.0 || i < 0.0 || !i.is_finite() {
        return None;
    }
    Some(i as usize)
}

#[inline]
pub(crate) fn get_u32(arr: &[u32], i: f64) -> Option<f64> {
    index_of(i).and_then(|j| arr.get(j)).map(|&v| v as f64)
}

#[inline]
pub(crate) fn set_u32(arr: &mut [u32], i: f64, v: f64) {
    if let Some(j) = index_of(i) {
        if j < arr.len() {
            arr[j] = to_uint32(v);
        }
    }
}

#[inline]
pub(crate) fn get_i32(arr: &[i32], i: f64) -> Option<f64> {
    index_of(i).and_then(|j| arr.get(j)).map(|&v| v as f64)
}

#[inline]
pub(crate) fn set_i32(arr: &mut [i32], i: f64, v: f64) {
    if let Some(j) = index_of(i) {
        if j < arr.len() {
            arr[j] = to_int32(v);
        }
    }
}

#[inline]
fn set_stamp(arr: &mut [u32], i: f64, v: u32) {
    set_u32(arr, i, v as f64);
}

impl<A: AStarAdapter> AStar<A> {
    pub fn new(adapter: A, max_iterations: Option<f64>) -> Self {
        let num_nodes = adapter.num_nodes().max(0.0) as usize; // JS ToIndex-ish
        let max_priority = adapter.max_priority();
        let neighbor_buffer = vec![0; adapter.max_neighbors()];
        Self {
            stamp: 1,
            closed_stamp: vec![0; num_nodes],
            g_score_stamp: vec![0; num_nodes],
            g_score: vec![0; num_nodes],
            came_from: vec![0; num_nodes],
            queue: BucketQueue::new(max_priority),
            adapter,
            neighbor_buffer,
            max_iterations: max_iterations.unwrap_or(500_000.0),
        }
    }

    /// Find a path from one or more starts to `goal`, or `None` (TS `null`).
    pub fn find_path(&mut self, starts: &[f64], goal: f64) -> Option<Vec<f64>> {
        self.stamp += 1;
        if self.stamp > 0xffff_ffff {
            self.closed_stamp.iter_mut().for_each(|s| *s = 0);
            self.g_score_stamp.iter_mut().for_each(|s| *s = 0);
            self.stamp = 1;
        }
        let stamp = self.stamp as u32;

        let queue = &mut self.queue;
        queue.clear();

        let closed_stamp = &mut self.closed_stamp;
        let g_score_stamp = &mut self.g_score_stamp;
        let g_score = &mut self.g_score;
        let came_from = &mut self.came_from;
        let buffer = &mut self.neighbor_buffer;
        let adapter = &mut self.adapter;

        for &s in starts {
            set_u32(g_score, s, 0.0);
            set_stamp(g_score_stamp, s, stamp);
            set_i32(came_from, s, -1.0);
            let h = adapter.heuristic(s, goal);
            queue.push(s, h);
        }

        let mut iterations = self.max_iterations;

        while !queue.is_empty() {
            iterations -= 1.0;
            if iterations <= 0.0 {
                return None;
            }

            // TS: pop() -> number (the exhausted sentinel is -1).
            let current = queue.pop().unwrap_or(f64::NAN);

            if get_u32(closed_stamp, current) == Some(stamp as f64) {
                continue;
            }
            set_stamp(closed_stamp, current, stamp);

            if current == goal {
                return Some(self.build_path(goal));
            }

            let current_g = get_u32(g_score, current).unwrap_or(f64::NAN);
            let prev_raw = get_i32(came_from, current);
            let count = adapter.neighbors(current, buffer);

            for i in 0..count {
                let neighbor = buffer[i] as f64;

                if get_u32(closed_stamp, neighbor) == Some(stamp as f64) {
                    continue;
                }

                // TS: prev === -1 ? undefined : prev  (start sentinel).
                let prev = match prev_raw {
                    Some(p) if p != -1.0 => Some(p),
                    _ => None,
                };
                let cost = adapter.cost(current, neighbor, prev);
                let tentative_g = current_g + cost;

                let stored_g = get_u32(g_score, neighbor);
                let improved = match get_u32(g_score_stamp, neighbor) {
                    Some(st) if st == stamp as f64 => {
                        tentative_g < stored_g.unwrap_or(f64::NAN)
                    }
                    _ => true,
                };
                if improved {
                    set_i32(came_from, neighbor, current);
                    set_u32(g_score, neighbor, tentative_g);
                    set_stamp(g_score_stamp, neighbor, stamp);
                    let h = adapter.heuristic(neighbor, goal);
                    queue.push(neighbor, tentative_g + h);
                }
            }
        }

        None
    }

    fn build_path(&mut self, goal: f64) -> Vec<f64> {
        let mut path = Vec::new();
        let mut current = goal;
        loop {
            if current == -1.0 {
                break;
            }
            path.push(current);
            // An OOB read (None) can only occur if the chain was never seeded
            // through a valid start; TS would then push `undefined` forever --
            // unreachable through the public API, so the port terminates.
            match get_i32(&self.came_from, current) {
                Some(n) => current = n,
                None => break,
            }
        }
        path.reverse();
        path
    }

    // ---- debug views for parity traces ----

    pub fn debug_stamp(&self) -> u64 {
        self.stamp
    }
    pub fn debug_g_score(&self) -> &[u32] {
        &self.g_score
    }
    pub fn debug_g_score_stamp(&self) -> &[u32] {
        &self.g_score_stamp
    }
    pub fn debug_closed_stamp(&self) -> &[u32] {
        &self.closed_stamp
    }
    pub fn debug_came_from(&self) -> &[i32] {
        &self.came_from
    }
    pub fn adapter_mut(&mut self) -> &mut A {
        &mut self.adapter
    }
}

/// Deterministic grid adapter mirroring the vector generator's TS twin
/// (`tools/gen_vectors.mjs`): N/S/W/E neighbour order, integer turn-penalty
/// cost, heuristics kind 0 = manhattan, 1 = scale * manhattan, 2 = zero.
///
/// This is not simulation code — the real adapters live in
/// `src/core/pathfinding/**` and will be ported with their own vectors. It is
/// the reference adapter the parity traces are recorded against, exposed
/// publicly so `tests/parity_structures.rs` and the wasm probe can replay the
/// exact same adapter logic the TS recorded.
pub struct GridAdapter {
    w: f64,
    h: f64,
    blocked: std::collections::HashSet<i64>,
    const_cost: f64,
    turn_penalty: f64,
    heur_kind: u8,
    heur_scale: f64,
}

impl GridAdapter {
    pub fn new(
        w: f64,
        h: f64,
        blocked: &[f64],
        const_cost: f64,
        turn_penalty: f64,
        heur_kind: u8,
        heur_scale: f64,
    ) -> Self {
        Self {
            w,
            h,
            blocked: blocked.iter().map(|b| to_int32(*b) as i64).collect(),
            const_cost,
            turn_penalty,
            heur_kind,
            heur_scale,
        }
    }
}

impl AStarAdapter for GridAdapter {
        fn neighbors(&mut self, node: f64, buffer: &mut [i32]) -> usize {
            let mut n = 0usize;
            let w = self.w;
            let h = self.h;
            let x = node % w; // JS %: sign follows dividend
            let blocked = &self.blocked;
            let hit = |v: f64| blocked.contains(&(v as i64));
            if node >= w && !hit(node - w) {
                buffer[n] = to_int32(node - w);
                n += 1;
            }
            if node < (h - 1.0) * w && !hit(node + w) {
                buffer[n] = to_int32(node + w);
                n += 1;
            }
            if x != 0.0 && !hit(node - 1.0) {
                buffer[n] = to_int32(node - 1.0);
                n += 1;
            }
            if x != w - 1.0 && !hit(node + 1.0) {
                buffer[n] = to_int32(node + 1.0);
                n += 1;
            }
            n
        }

        fn cost(&mut self, from: f64, to: f64, prev: Option<f64>) -> f64 {
            let mut c = self.const_cost;
            if let Some(p) = prev {
                // Transcribed literally from the JS vector adapter:
                // `(prev - from) === (to - from)`. What the predicate *means*
                // is irrelevant to parity; both sides must compute it the
                // same way.
                let straight = (p - from) == (to - from);
                if !straight {
                    c += self.turn_penalty;
                }
            }
            c
        }

        fn heuristic(&mut self, node: f64, goal: f64) -> f64 {
            let d = (node % self.w - goal % self.w).abs()
                + (js_floor_div(node, self.w) - js_floor_div(goal, self.w)).abs();
            match self.heur_kind {
                1 => self.heur_scale * d,
                2 => 0.0,
                _ => d,
            }
        }

        fn num_nodes(&self) -> f64 {
            self.w * self.h
        }
        fn max_priority(&self) -> f64 {
            (self.w + self.h) * (self.const_cost + self.turn_penalty) * 4.0 + 8.0
        }
        fn max_neighbors(&self) -> usize {
            4
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(w: f64, h: f64) -> GridAdapter {
        GridAdapter::new(w, h, &[], 1.0, 0.0, 0, 0.0)
    }

    #[test]
    fn straight_line_path() {
        let mut a = AStar::new(plain(5.0, 1.0), None);
        let path = a.find_path(&[0.0], 4.0).expect("path");
        assert_eq!(path, vec![0.0, 1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn blocked_wall_unreachable() {
        // 3x3, column 1 fully blocked.
        let adapter = GridAdapter::new(3.0, 3.0, &[1.0, 4.0, 7.0], 1.0, 0.0, 0, 0.0);
        let mut a = AStar::new(adapter, None);
        assert_eq!(a.find_path(&[0.0], 2.0), None);
    }

    #[test]
    fn start_equals_goal_returns_single_tile() {
        let mut a = AStar::new(plain(3.0, 3.0), None);
        assert_eq!(a.find_path(&[4.0], 4.0), Some(vec![4.0]));
    }

    #[test]
    fn iteration_cap_returns_null() {
        let mut a = AStar::new(plain(5.0, 5.0), Some(3.0));
        assert_eq!(a.find_path(&[0.0], 24.0), None);
    }

    #[test]
    fn multi_start_ties_resolve_lifo_like_bucket_queue() {
        // Both starts are manhattan-3 from the goal, so they share a bucket;
        // the TS pops the last-pushed start (21) first. Pinned by the
        // as_multistart golden vector.
        let mut a = AStar::new(plain(5.0, 5.0), None);
        let path = a.find_path(&[3.0, 21.0], 12.0).expect("path");
        assert_eq!(path, vec![21.0, 22.0, 17.0, 12.0]);
    }

    #[test]
    fn stamps_advance_and_reused_instance_is_stable() {
        let mut a = AStar::new(plain(5.0, 5.0), None);
        assert_eq!(a.debug_stamp(), 1);
        let p1 = a.find_path(&[0.0], 24.0);
        assert_eq!(a.debug_stamp(), 2);
        let p2 = a.find_path(&[0.0], 24.0);
        assert_eq!(a.debug_stamp(), 3);
        assert_eq!(p1, p2); // no stale-state leakage between searches
    }

    #[test]
    fn negative_cost_wraps_g_score_like_uint32() {
        // const_cost = -1: g-score wraps through Uint32Array storage and the
        // search gives up at the cap without panicking on any NaN path.
        let adapter = GridAdapter::new(3.0, 3.0, &[], -1.0, 2.0, 0, 0.0);
        let mut a = AStar::new(adapter, Some(40.0));
        let r = a.find_path(&[0.0], 8.0);
        assert!(r.is_none() || !r.unwrap().is_empty());
    }
}
