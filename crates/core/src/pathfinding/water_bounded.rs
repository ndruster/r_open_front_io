//! Port of `src/core/pathfinding/algorithms/AStar.WaterBounded.ts`.
//!
//! `AStarWaterBounded` is the bounded-window sibling of
//! [`AStarWater`](super::water): same inline four-neighbour expansion, same
//! [`MinHeap`](super::priority_queue::MinHeap) (f32-rounded priorities) and the
//! same cross-product tie-breaker, but every stamp-tracked array is indexed by
//! a *local* id inside a `[minX..maxX] × [minY..maxY]` window derived from the
//! starts and the goal. The window mapping is fully observable — the returned
//! path is re-expanded to global tiles via `toGlobal`, and the four arrays
//! retain per-search local state — so the port transcribes the closures
//! literally:
//!
//! * `findPath` derives the bounds with JS `Math.min`/`Math.max` (NaN
//!   propagation) over `s % width` and `(s / width) | 0` (ToInt32), then
//!   delegates to `searchBounded`.
//! * `searchBounded` returns `null` when `numLocalNodes` exceeds the array
//!   length, and when the *clamped* `goalLocal` falls outside the window —
//!   clamping (`Math.max(min, Math.min(max, x))`) can move the goal to a
//!   different tile than the caller asked for (`wb_clamped_goal` pins this).
//! * `toLocal`/`toGlobal` use JS `%` and `| 0`, so a negative `boundsWidth`
//!   (inverted bounds) still yields in-range locals and the search runs on the
//!   degenerate window (`wb_inverted_bounds`).
//! * The magnitude penalty curve differs from `AStarWater`: `magnitude < 3`
//!   costs `3 * COST_SCALE` (not 10×), `> 10` costs `1 * COST_SCALE`.
//! * Defaults: `heuristicWeight ?? 3`, `maxIterations ?? 100_000` (the
//!   nullish-coalescing only replaces `null`/`undefined`).
//! * `terrain` is the map's private packed `Uint8Array`; OOB / fractional /
//!   negative refs read `undefined`, whose bit tests see `0` — passable water
//!   with magnitude 0 (a 300-cost "too close to shore" penalty).
//! * `gScore`/`gScoreStamp`/`closedStamp` are `Uint32Array`s and `cameFrom` an
//!   `Int32Array`; all element access goes through the JS-semantics helpers
//!   shared with [`a_star`](super::a_star) (OOB reads `undefined`, OOB writes
//!   drop).
//! * `stamp` starts at 1 and is *incremented* at the top of each search, so
//!   the first search runs with stamp 2; refill past `0xffffffff` zeroes both
//!   stamp arrays.

use crate::game_map::{js_max, js_min};
use crate::jsnum::to_int32;
use crate::pathfinding::a_star::{get_i32, get_u32, set_i32, set_u32};
use crate::pathfinding::priority_queue::{MinHeap, PriorityQueue};

const COST_SCALE: f64 = 100.0;
const BASE_COST: f64 = 1.0 * COST_SCALE;
const LAND_MASK: u8 = 1 << 7;
const MAGNITUDE_MASK: u8 = 0x1f;

/// `getMagnitudePenalty`: prefer magnitude 3-10 (3-10 tiles from shore).
/// Note the `< 3` branch is 3× the cost scale here, unlike `water.rs` (10×).
#[inline]
fn magnitude_penalty(magnitude: f64) -> f64 {
    if magnitude < 3.0 {
        3.0 * COST_SCALE
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

/// `BoundedAStarConfig` + `SearchBounds` folded into plain parameters.
pub struct AStarWaterBounded {
    stamp: u64,
    closed_stamp: Vec<u32>,
    g_score_stamp: Vec<u32>,
    g_score: Vec<u32>,
    came_from: Vec<i32>,
    queue: MinHeap,
    terrain: Vec<u8>,
    map_width: f64,
    heuristic_weight: f64,
    max_iterations: f64,
}

impl AStarWaterBounded {
    /// `new AStarWaterBounded(map, maxSearchArea, config?)`. `width` comes from
    /// `map.width()`, `terrain` from the map's packed byte buffer;
    /// `max_search_area` sizes the four local arrays and the heap
    /// (`maxSearchArea * 4`). `None` config fields take the TS `??` defaults.
    pub fn new(
        width: f64,
        terrain: Vec<u8>,
        max_search_area: f64,
        heuristic_weight: Option<f64>,
        max_iterations: Option<f64>,
    ) -> Self {
        // `new Uint32Array(maxSearchArea)` — ToIndex truncation; the recorded
        // scenarios always use non-negative integers.
        let cap = max_search_area.max(0.0) as usize;
        Self {
            stamp: 1,
            closed_stamp: vec![0; cap],
            g_score_stamp: vec![0; cap],
            g_score: vec![0; cap],
            came_from: vec![0; cap],
            queue: MinHeap::new(max_search_area * 4.0),
            terrain,
            map_width: width,
            heuristic_weight: heuristic_weight.unwrap_or(3.0),
            max_iterations: max_iterations.unwrap_or(100_000.0),
        }
    }

    /// `findPath(start, goal)`. `starts` is the TS `Array.isArray(start) ?
    /// start : [start]` normalisation, passed pre-expanded. The bounds are the
    /// JS `Math.min`/`Math.max` hull of every start and the goal.
    pub fn find_path(&mut self, starts: &[f64], goal: f64) -> Option<Vec<f64>> {
        let width = self.map_width;
        let goal_x = goal % width;
        let goal_y = to_int32(goal / width) as f64;

        let mut min_x = goal_x;
        let mut max_x = goal_x;
        let mut min_y = goal_y;
        let mut max_y = goal_y;
        for &s in starts {
            let sx = s % width;
            let sy = to_int32(s / width) as f64;
            min_x = js_min(min_x, sx);
            max_x = js_max(max_x, sx);
            min_y = js_min(min_y, sy);
            max_y = js_max(max_y, sy);
        }

        self.search_bounded(starts, goal, min_x, max_x, min_y, max_y)
    }

    /// `searchBounded(start, goal, bounds)`. Returns `None` for the TS `null`.
    pub fn search_bounded(
        &mut self,
        starts: &[f64],
        goal: f64,
        min_x: f64,
        max_x: f64,
        min_y: f64,
        max_y: f64,
    ) -> Option<Vec<f64>> {
        self.stamp += 1;
        if self.stamp > 0xffff_ffff {
            self.closed_stamp.fill(0);
            self.g_score_stamp.fill(0);
            self.stamp = 1;
        }
        let stamp = self.stamp as f64;

        // Disjoint field borrows so the four expansion blocks read like the TS.
        let map_width = self.map_width;
        let weight = self.heuristic_weight;
        let terrain = &self.terrain;
        let closed_stamp = &mut self.closed_stamp;
        let g_score_stamp = &mut self.g_score_stamp;
        let g_score = &mut self.g_score;
        let came_from = &mut self.came_from;
        let queue = &mut self.queue;

        let bounds_width = max_x - min_x + 1.0;
        let goal_x = goal % map_width;
        let goal_y = to_int32(goal / map_width) as f64;
        let bounds_height = max_y - min_y + 1.0;
        let num_local_nodes = bounds_width * bounds_height;

        if num_local_nodes > closed_stamp.len() as f64 {
            return None;
        }

        // `toLocal(tile, clamp)` — global ref -> window index.
        let to_local = |tile: f64, clamp: bool| -> f64 {
            let mut x = tile % map_width;
            let mut y = to_int32(tile / map_width) as f64;
            if clamp {
                x = js_max(min_x, js_min(max_x, x));
                y = js_max(min_y, js_min(max_y, y));
            }
            (y - min_y) * bounds_width + (x - min_x)
        };

        // `toGlobal(local)` — window index -> global ref.
        let to_global = |local: f64| -> f64 {
            let local_x = local % bounds_width;
            let local_y = to_int32(local / bounds_width) as f64;
            (local_y + min_y) * map_width + (local_x + min_x)
        };

        let goal_local = to_local(goal, true);
        if goal_local < 0.0 || goal_local >= num_local_nodes {
            return None;
        }

        queue.clear();

        // Cross-product tie-breaker geometry, seeded from the first start.
        let s0 = *starts.first().unwrap_or(&f64::NAN);
        let start_x = s0 % map_width;
        let start_y = to_int32(s0 / map_width) as f64;
        let dx_goal = goal_x - start_x;
        let dy_goal = goal_y - start_y;
        // Normalization factor to keep the tie-breaker small (< COST_SCALE).
        let cross_norm = max_one(dx_goal.abs() + dy_goal.abs());

        // `crossTieBreaker(nx, ny)` — deviation of (nx, ny) from the
        // start→goal line, scaled to stay below COST_SCALE.
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
            let start_local = to_local(s, true);
            if start_local < 0.0 || start_local >= num_local_nodes {
                continue;
            }
            set_u32(g_score, start_local, 0.0);
            set_u32(g_score_stamp, start_local, stamp);
            set_i32(came_from, start_local, -1.0);
            let sx = s % map_width;
            let sy = to_int32(s / map_width) as f64;
            let h = weight * BASE_COST * ((sx - goal_x).abs() + (sy - goal_y).abs());
            queue.push(start_local, h);
        }

        let mut iterations = self.max_iterations;

        while !queue.is_empty() {
            iterations -= 1.0;
            if iterations <= 0.0 {
                return None;
            }

            let current_local = queue.pop().unwrap_or(f64::NAN);

            if get_u32(closed_stamp, current_local) == Some(stamp) {
                continue;
            }
            set_u32(closed_stamp, current_local, stamp);

            if current_local == goal_local {
                return Some(build_path(came_from, goal_local, num_local_nodes, to_global));
            }

            let current_g = get_u32(g_score, current_local).unwrap_or(f64::NAN);

            // Convert to global coords for neighbour calculation.
            let current = to_global(current_local);
            let current_x = current % map_width;
            let current_y = to_int32(current / map_width) as f64;

            // ---- North: current - mapWidth ----
            if current_y > min_y {
                let neighbor = current - map_width;
                let neighbor_local = current_local - bounds_width;
                let neighbor_terrain = tbyte(neighbor);
                if get_u32(closed_stamp, neighbor_local) != Some(stamp)
                    && (neighbor == goal || (neighbor_terrain & LAND_MASK) == 0)
                {
                    let ny = current_y - 1.0;
                    let dist_to_goal = (current_x - goal_x).abs() + (ny - goal_y).abs();
                    let magnitude = (neighbor_terrain & MAGNITUDE_MASK) as f64;
                    let cost = BASE_COST + magnitude_penalty(magnitude);
                    let tentative_g = current_g + cost;
                    if get_u32(g_score_stamp, neighbor_local) != Some(stamp)
                        || tentative_g < get_u32(g_score, neighbor_local).unwrap_or(f64::NAN)
                    {
                        set_i32(came_from, neighbor_local, current_local);
                        set_u32(g_score, neighbor_local, tentative_g);
                        set_u32(g_score_stamp, neighbor_local, stamp);
                        let h = weight * BASE_COST * dist_to_goal;
                        let f = tentative_g + h + cross_tie_breaker(current_x, ny);
                        queue.push(neighbor_local, f);
                    }
                }
            }

            // ---- South: current + mapWidth ----
            if current_y < max_y {
                let neighbor = current + map_width;
                let neighbor_local = current_local + bounds_width;
                let neighbor_terrain = tbyte(neighbor);
                if get_u32(closed_stamp, neighbor_local) != Some(stamp)
                    && (neighbor == goal || (neighbor_terrain & LAND_MASK) == 0)
                {
                    let ny = current_y + 1.0;
                    let dist_to_goal = (current_x - goal_x).abs() + (ny - goal_y).abs();
                    let magnitude = (neighbor_terrain & MAGNITUDE_MASK) as f64;
                    let cost = BASE_COST + magnitude_penalty(magnitude);
                    let tentative_g = current_g + cost;
                    if get_u32(g_score_stamp, neighbor_local) != Some(stamp)
                        || tentative_g < get_u32(g_score, neighbor_local).unwrap_or(f64::NAN)
                    {
                        set_i32(came_from, neighbor_local, current_local);
                        set_u32(g_score, neighbor_local, tentative_g);
                        set_u32(g_score_stamp, neighbor_local, stamp);
                        let h = weight * BASE_COST * dist_to_goal;
                        let f = tentative_g + h + cross_tie_breaker(current_x, ny);
                        queue.push(neighbor_local, f);
                    }
                }
            }

            // ---- West: current - 1 ----
            if current_x > min_x {
                let neighbor = current - 1.0;
                let neighbor_local = current_local - 1.0;
                let neighbor_terrain = tbyte(neighbor);
                if get_u32(closed_stamp, neighbor_local) != Some(stamp)
                    && (neighbor == goal || (neighbor_terrain & LAND_MASK) == 0)
                {
                    let nx = current_x - 1.0;
                    let dist_to_goal = (nx - goal_x).abs() + (current_y - goal_y).abs();
                    let magnitude = (neighbor_terrain & MAGNITUDE_MASK) as f64;
                    let cost = BASE_COST + magnitude_penalty(magnitude);
                    let tentative_g = current_g + cost;
                    if get_u32(g_score_stamp, neighbor_local) != Some(stamp)
                        || tentative_g < get_u32(g_score, neighbor_local).unwrap_or(f64::NAN)
                    {
                        set_i32(came_from, neighbor_local, current_local);
                        set_u32(g_score, neighbor_local, tentative_g);
                        set_u32(g_score_stamp, neighbor_local, stamp);
                        let h = weight * BASE_COST * dist_to_goal;
                        let f = tentative_g + h + cross_tie_breaker(nx, current_y);
                        queue.push(neighbor_local, f);
                    }
                }
            }

            // ---- East: current + 1 ----
            if current_x < max_x {
                let neighbor = current + 1.0;
                let neighbor_local = current_local + 1.0;
                let neighbor_terrain = tbyte(neighbor);
                if get_u32(closed_stamp, neighbor_local) != Some(stamp)
                    && (neighbor == goal || (neighbor_terrain & LAND_MASK) == 0)
                {
                    let nx = current_x + 1.0;
                    let dist_to_goal = (nx - goal_x).abs() + (current_y - goal_y).abs();
                    let magnitude = (neighbor_terrain & MAGNITUDE_MASK) as f64;
                    let cost = BASE_COST + magnitude_penalty(magnitude);
                    let tentative_g = current_g + cost;
                    if get_u32(g_score_stamp, neighbor_local) != Some(stamp)
                        || tentative_g < get_u32(g_score, neighbor_local).unwrap_or(f64::NAN)
                    {
                        set_i32(came_from, neighbor_local, current_local);
                        set_u32(g_score, neighbor_local, tentative_g);
                        set_u32(g_score_stamp, neighbor_local, stamp);
                        let h = weight * BASE_COST * dist_to_goal;
                        let f = tentative_g + h + cross_tie_breaker(nx, current_y);
                        queue.push(neighbor_local, f);
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

/// `buildPath(goalLocal, toGlobal, maxPathLength)`: walk `cameFrom` back to
/// the `-1` sentinel (or the iteration cap), mapping each local id to a
/// global tile, then reverse.
fn build_path<F: Fn(f64) -> f64>(
    came_from: &[i32],
    goal_local: f64,
    max_path_length: f64,
    to_global: F,
) -> Vec<f64> {
    let mut path = Vec::new();
    let mut current = goal_local;
    let mut iterations = 0.0;
    while current != -1.0 && iterations < max_path_length {
        path.push(to_global(current));
        // The chain is always seeded through valid starts, so an OOB read
        // (None -> NaN, which JS would push as a NaN tile) is unreachable
        // through the public API; the cap mirrors the TS safety check.
        current = get_i32(came_from, current).unwrap_or(f64::NAN);
        iterations += 1.0;
    }
    path.reverse();
    path
}
