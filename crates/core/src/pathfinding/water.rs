//! Port of `src/core/pathfinding/algorithms/AStar.Water.ts`.
//!
//! `AStarWater` is a *self-contained* A* variant: unlike [`AStar`](super::a_star::AStar)
//! (which drives a pluggable adapter over a [`BucketQueue`](super::priority_queue::BucketQueue)),
//! this one inlines the neighbour expansion and runs on a
//! [`MinHeap`](super::priority_queue::MinHeap) with a cross-product tie-breaker
//! that prefers paths hugging the start→goal line. Every one of these choices is
//! observable through the returned path and the four stamp-tracked arrays, so the
//! port is a literal transcription of the four near-identical expansion blocks:
//!
//! * `terrain` is read straight off the map's private `Uint8Array` (bit 7 land,
//!   bits 0-4 magnitude). Out-of-range / negative / fractional refs read
//!   `undefined`, whose bit tests see `0` — i.e. *passable water* with magnitude
//!   0 (a 1000-cost "too close to shore" penalty). The `tbyte` closure in
//!   `find_path` mirrors that.
//! * `gScore` / `gScoreStamp` / `closedStamp` are `Uint32Array`s and `cameFrom`
//!   an `Int32Array`; all element access goes through the JS-semantics helpers
//!   shared with [`a_star`](super::a_star) (OOB reads `undefined`, OOB writes
//!   drop).
//! * The queue is a `MinHeap`, so every pushed priority is rounded to **f32**
//!   before comparison — two distinct f64 `f` values can tie and then pop in
//!   insertion order. This is the single biggest desync risk versus a
//!   `BinaryHeap<f64>` and is why the port reuses the ported `MinHeap` verbatim.
//! * `x()` is JS `%` (sign follows the dividend); `y()` is `| 0` (ToInt32).
//! * `stamp` starts at 1 and is *incremented* at the top of each `findPath`, so
//!   the first search runs with stamp 2; refill past `0xffffffff` zeroes both
//!   stamp arrays.
//! * `heuristicWeight ?? 5` and `maxIterations ?? 1_000_000` — the nullish
//!   coalescing only replaces `null`/`undefined`, so an explicit `NaN` survives
//!   (mirrored by `unwrap_or`, which likewise only fires on `None`).

use crate::jsnum::to_int32;
use crate::pathfinding::a_star::{get_i32, get_u32, set_i32, set_u32};
use crate::pathfinding::priority_queue::{MinHeap, PriorityQueue};

const COST_SCALE: f64 = 100.0;
const BASE_COST: f64 = 1.0 * COST_SCALE;
const LAND_MASK: u8 = 1 << 7;
const MAGNITUDE_MASK: u8 = 0x1f;

/// `getMagnitudePenalty`: prefer magnitude 3-10 (3-10 tiles from shore).
#[inline]
fn magnitude_penalty(magnitude: f64) -> f64 {
    if magnitude < 3.0 {
        10.0 * COST_SCALE
    } else if magnitude <= 10.0 {
        0.0
    } else {
        1.0 * COST_SCALE
    }
}

/// `Math.max(1, v)` with JS NaN propagation (a NaN `v` yields NaN, not 1).
#[inline]
fn max_one(v: f64) -> f64 {
    if v.is_nan() {
        f64::NAN
    } else if v > 1.0 {
        v
    } else {
        1.0
    }
}

/// `AStarWater` from the TS source. Owns the packed terrain bytes and the four
/// stamp-tracked arrays exactly as the class does.
pub struct AStarWater {
    stamp: u64,
    closed_stamp: Vec<u32>,
    g_score_stamp: Vec<u32>,
    g_score: Vec<u32>,
    came_from: Vec<i32>,
    queue: MinHeap,
    terrain: Vec<u8>,
    width: f64,
    num_nodes: f64,
    heuristic_weight: f64,
    max_iterations: f64,
}

impl AStarWater {
    /// `new AStarWater(map, config?)`. `width`/`height` come from
    /// `map.width()`/`map.height()`; `terrain` is the map's packed byte buffer.
    /// `heuristic_weight`/`max_iterations` are the optional config fields
    /// (`None` = TS `undefined` → the `??` defaults).
    pub fn new(
        width: f64,
        height: f64,
        terrain: Vec<u8>,
        heuristic_weight: Option<f64>,
        max_iterations: Option<f64>,
    ) -> Self {
        let num_nodes = width * height;
        let cap = num_nodes.max(0.0) as usize;
        Self {
            stamp: 1,
            closed_stamp: vec![0; cap],
            g_score_stamp: vec![0; cap],
            g_score: vec![0; cap],
            came_from: vec![0; cap],
            queue: MinHeap::new(num_nodes),
            terrain,
            width,
            num_nodes,
            heuristic_weight: heuristic_weight.unwrap_or(5.0),
            max_iterations: max_iterations.unwrap_or(1_000_000.0),
        }
    }

    /// `findPath(start, goal)`. `starts` is the TS `Array.isArray(start) ? start
    /// : [start]` normalisation, passed pre-expanded. Returns `None` for the TS
    /// `null`.
    pub fn find_path(&mut self, starts: &[f64], goal: f64) -> Option<Vec<f64>> {
        self.stamp += 1;
        if self.stamp > 0xffff_ffff {
            self.closed_stamp.iter_mut().for_each(|s| *s = 0);
            self.g_score_stamp.iter_mut().for_each(|s| *s = 0);
            self.stamp = 1;
        }
        let stamp = self.stamp as f64;

        // Disjoint field borrows so the four expansion blocks read like the TS
        // (which freely mixes `this.*` reads and writes inside one iteration).
        let width = self.width;
        let num_nodes = self.num_nodes;
        let weight = self.heuristic_weight;
        let terrain = &self.terrain;
        let closed_stamp = &mut self.closed_stamp;
        let g_score_stamp = &mut self.g_score_stamp;
        let g_score = &mut self.g_score;
        let came_from = &mut self.came_from;
        let queue = &mut self.queue;

        let goal_x = goal % width; // JS %
        let goal_y = to_int32(goal / width) as f64; // `(goal / width) | 0`

        queue.clear();

        // Cross-product tie-breaker geometry, seeded from the first start.
        let s0 = *starts.first().unwrap_or(&f64::NAN);
        let start_x = s0 % width;
        let start_y = to_int32(s0 / width) as f64;
        let dx_goal = goal_x - start_x;
        let dy_goal = goal_y - start_y;
        // Normalization factor to keep the tie-breaker small (< COST_SCALE).
        let cross_norm = max_one(dx_goal.abs() + dy_goal.abs());

        // `crossTieBreaker(nx, ny)` — deviation of (nx, ny) from the start→goal
        // line, scaled to stay below COST_SCALE.
        let cross_tie_breaker = |nx: f64, ny: f64| -> f64 {
            let dx_n = nx - goal_x;
            let dy_n = ny - goal_y;
            let cross = (dx_goal * dy_n - dy_goal * dx_n).abs();
            ((cross * (COST_SCALE - 1.0)) / cross_norm / cross_norm).floor()
        };

        // `terrain[neighbor]` with JS semantics (closure over the byte slice).
        let tbyte = |tile: f64| -> u8 {
            if tile.fract() != 0.0 || tile < 0.0 || !tile.is_finite() {
                0
            } else {
                terrain.get(tile as usize).copied().unwrap_or(0)
            }
        };

        for &s in starts {
            set_u32(g_score, s, 0.0);
            set_u32(g_score_stamp, s, stamp);
            set_i32(came_from, s, -1.0);
            let sx = s % width;
            let sy = to_int32(s / width) as f64;
            let h = weight * BASE_COST * ((sx - goal_x).abs() + (sy - goal_y).abs());
            queue.push(s, h);
        }

        let mut iterations = self.max_iterations;

        while !queue.is_empty() {
            iterations -= 1.0;
            if iterations <= 0.0 {
                return None;
            }

            let current = queue.pop().unwrap_or(f64::NAN);

            if get_u32(closed_stamp, current) == Some(stamp) {
                continue;
            }
            set_u32(closed_stamp, current, stamp);

            if current == goal {
                return Some(build_path(came_from, goal));
            }

            let current_g = get_u32(g_score, current).unwrap_or(f64::NAN);
            let current_x = current % width;
            let current_y = to_int32(current / width) as f64;

            // ---- North: current - width ----
            if current >= width {
                let neighbor = current - width;
                let neighbor_terrain = tbyte(neighbor);
                if get_u32(closed_stamp, neighbor) != Some(stamp)
                    && (neighbor == goal || (neighbor_terrain & LAND_MASK) == 0)
                {
                    let magnitude = (neighbor_terrain & MAGNITUDE_MASK) as f64;
                    let cost = BASE_COST + magnitude_penalty(magnitude);
                    let tentative_g = current_g + cost;
                    if get_u32(g_score_stamp, neighbor) != Some(stamp)
                        || tentative_g < get_u32(g_score, neighbor).unwrap_or(f64::NAN)
                    {
                        set_i32(came_from, neighbor, current);
                        set_u32(g_score, neighbor, tentative_g);
                        set_u32(g_score_stamp, neighbor, stamp);
                        let ny = current_y - 1.0;
                        let h = weight
                            * BASE_COST
                            * ((current_x - goal_x).abs() + (ny - goal_y).abs());
                        let f = tentative_g + h + cross_tie_breaker(current_x, ny);
                        queue.push(neighbor, f);
                    }
                }
            }

            // ---- South: current + width ----
            if current < num_nodes - width {
                let neighbor = current + width;
                let neighbor_terrain = tbyte(neighbor);
                if get_u32(closed_stamp, neighbor) != Some(stamp)
                    && (neighbor == goal || (neighbor_terrain & LAND_MASK) == 0)
                {
                    let magnitude = (neighbor_terrain & MAGNITUDE_MASK) as f64;
                    let cost = BASE_COST + magnitude_penalty(magnitude);
                    let tentative_g = current_g + cost;
                    if get_u32(g_score_stamp, neighbor) != Some(stamp)
                        || tentative_g < get_u32(g_score, neighbor).unwrap_or(f64::NAN)
                    {
                        set_i32(came_from, neighbor, current);
                        set_u32(g_score, neighbor, tentative_g);
                        set_u32(g_score_stamp, neighbor, stamp);
                        let ny = current_y + 1.0;
                        let h = weight
                            * BASE_COST
                            * ((current_x - goal_x).abs() + (ny - goal_y).abs());
                        let f = tentative_g + h + cross_tie_breaker(current_x, ny);
                        queue.push(neighbor, f);
                    }
                }
            }

            // ---- West: current - 1 ----
            if current_x != 0.0 {
                let neighbor = current - 1.0;
                let neighbor_terrain = tbyte(neighbor);
                if get_u32(closed_stamp, neighbor) != Some(stamp)
                    && (neighbor == goal || (neighbor_terrain & LAND_MASK) == 0)
                {
                    let magnitude = (neighbor_terrain & MAGNITUDE_MASK) as f64;
                    let cost = BASE_COST + magnitude_penalty(magnitude);
                    let tentative_g = current_g + cost;
                    if get_u32(g_score_stamp, neighbor) != Some(stamp)
                        || tentative_g < get_u32(g_score, neighbor).unwrap_or(f64::NAN)
                    {
                        set_i32(came_from, neighbor, current);
                        set_u32(g_score, neighbor, tentative_g);
                        set_u32(g_score_stamp, neighbor, stamp);
                        let nx = current_x - 1.0;
                        let h = weight
                            * BASE_COST
                            * ((nx - goal_x).abs() + (current_y - goal_y).abs());
                        let f = tentative_g + h + cross_tie_breaker(nx, current_y);
                        queue.push(neighbor, f);
                    }
                }
            }

            // ---- East: current + 1 ----
            if current_x != width - 1.0 {
                let neighbor = current + 1.0;
                let neighbor_terrain = tbyte(neighbor);
                if get_u32(closed_stamp, neighbor) != Some(stamp)
                    && (neighbor == goal || (neighbor_terrain & LAND_MASK) == 0)
                {
                    let magnitude = (neighbor_terrain & MAGNITUDE_MASK) as f64;
                    let cost = BASE_COST + magnitude_penalty(magnitude);
                    let tentative_g = current_g + cost;
                    if get_u32(g_score_stamp, neighbor) != Some(stamp)
                        || tentative_g < get_u32(g_score, neighbor).unwrap_or(f64::NAN)
                    {
                        set_i32(came_from, neighbor, current);
                        set_u32(g_score, neighbor, tentative_g);
                        set_u32(g_score_stamp, neighbor, stamp);
                        let nx = current_x + 1.0;
                        let h = weight
                            * BASE_COST
                            * ((nx - goal_x).abs() + (current_y - goal_y).abs());
                        let f = tentative_g + h + cross_tie_breaker(nx, current_y);
                        queue.push(neighbor, f);
                    }
                }
            }
        }

        None
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
}

/// `buildPath(goal)`: walk `cameFrom` back to the `-1` sentinel, reverse.
fn build_path(came_from: &[i32], goal: f64) -> Vec<f64> {
    let mut path = Vec::new();
    let mut current = goal;
    loop {
        if current == -1.0 {
            break;
        }
        path.push(current);
        // An OOB read (None) can only occur if the chain was never seeded
        // through a valid start; TS would then push `undefined` forever —
        // unreachable through the public API, so the port terminates.
        match get_i32(came_from, current) {
            Some(n) => current = n,
            None => break,
        }
    }
    path.reverse();
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    // Terrain bytes: deep water (mag 8, sweet spot), shallow water (mag 1,
    // heavy penalty), land (bit 7).
    const DEEP: u8 = 8; // water, magnitude 8
    const SHALLOW: u8 = 1; // water, magnitude 1 -> +1000 penalty
    const LAND: u8 = 0x80 | 5; // land

    fn water(rows: &[&[u8]]) -> AStarWater {
        let h = rows.len() as f64;
        let w = rows[0].len() as f64;
        let terrain = rows.iter().flat_map(|r| r.iter().copied()).collect();
        AStarWater::new(w, h, terrain, None, None)
    }

    #[test]
    fn straight_line_across_deep_water() {
        let mut a = water(&[&[DEEP, DEEP, DEEP, DEEP, DEEP]]);
        assert_eq!(
            a.find_path(&[0.0], 4.0),
            Some(vec![0.0, 1.0, 2.0, 3.0, 4.0])
        );
    }

    #[test]
    fn stamp_advances_and_search_is_reusable() {
        let mut a = water(&[&[DEEP, DEEP, DEEP], &[DEEP, DEEP, DEEP]]);
        assert_eq!(a.debug_stamp(), 1);
        let p1 = a.find_path(&[0.0], 2.0);
        assert_eq!(a.debug_stamp(), 2);
        let p2 = a.find_path(&[0.0], 2.0);
        assert_eq!(a.debug_stamp(), 3);
        assert_eq!(p1, p2);
    }

    #[test]
    fn land_is_avoided_but_goal_land_is_enterable() {
        // Row of 5: middle tile is land. Crossing to the far side is blocked
        // (land is not the goal), but reaching the land tile itself succeeds.
        let mut a = water(&[&[DEEP, DEEP, LAND, DEEP, DEEP]]);
        assert_eq!(a.find_path(&[0.0], 4.0), None);
        assert_eq!(a.find_path(&[0.0], 2.0), Some(vec![0.0, 1.0, 2.0]));
    }

    #[test]
    fn shallow_water_is_penalised() {
        // Top row shallow (expensive), bottom deep. Going 0 -> 2 (top-right)
        // detours through the cheap bottom row rather than paying +1000/tile.
        let mut a = water(&[&[SHALLOW, SHALLOW, SHALLOW], &[DEEP, DEEP, DEEP]]);
        let p = a.find_path(&[0.0], 2.0).expect("path");
        assert!(p.contains(&3.0) || p.contains(&4.0) || p.contains(&5.0));
    }
}
