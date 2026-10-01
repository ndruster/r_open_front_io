//! Port of `src/core/pathfinding/transformers/SmoothingWaterTransformer.ts`.
//!
//! A `PathFinder` decorator for water routes: delegates to `inner`, then
//! smooths the result in three passes — LOS binary-search smoothing
//! (min magnitude 2), local `AStarWaterBounded` refinement of the first/last
//! ~50 manhattan tiles, and LOS smoothing again (min magnitude 3, farther
//! from shore).
//!
//! JS-isms pinned here:
//! * `path ? smooth(path) : null` — an *empty* inner result is truthy in JS,
//!   so `[]` flows through `smooth` (length `<= 2` early-return) and the
//!   output is `[]`, not `null`.
//! * `DebugSpan.wrap` is a pass-through when `__DEBUG_SPAN_ENABLED__` is
//!   unset (the default), so the span nesting is not observable and is
//!   dropped.
//! * `mid = (lo + hi) >>> 1` — `lo + hi < 2^31` for any real path, so the
//!   unsigned shift equals Rust's `usize` `>> 1`.
//! * `x = tile % mapWidth` (JS `%`, sign follows the dividend) and
//!   `y = (tile / mapWidth) | 0` (ToInt32) for the Bresenham endpoints and
//!   `manhattanDist`; the loop tiles `y * mapWidth + x` stay integer-valued.
//! * `terrain[tile] & 0x1f` reads the map's private packed `Uint8Array`
//!   directly: OOB / negative / fractional indices read `undefined`, whose
//!   bit test sees `0` — magnitude 0, which fails every `minMagnitude` gate
//!   (and `map.isWater` of an OOB ref is `true`, so the traversable check
//!   passes first).
//! * The diagonal Bresenham step checks the intermediate tile `(new x, old
//!   y)`; on failure it rolls back and takes `(old x, new y)` instead — and
//!   in `canSee` both candidates are gated on traversable *and* magnitude,
//!   while `tracePath` gates only on traversable (and pushes the detour tile
//!   it validated).
//! * `refineSegment` clamps the padded bounds with JS `Math.max`/`Math.min`
//!   (NaN-propagating [`js_max`]/[`js_min`]) and calls
//!   `searchBounded(from, to, bounds)` with a *single* start — the TS
//!   normalisation `Array.isArray(start) ? start : [start]` inside
//!   `searchBounded` makes `&[from]` the exact equivalent.
//! * `terrain` is captured once in the constructor as a live reference to
//!   the map's buffer; the port clones it (same convention as
//!   [`water_hierarchical`](super::water_hierarchical) — the parity
//!   scenarios never mutate the map after construction).
//! * The `numLocalNodes > 10000` null branch of `searchBounded` is
//!   unreachable through this transformer: refinement pairs sit ~50-60
//!   manhattan apart, so the padded window never exceeds ~80x80.

use crate::game_map::{js_max, js_min, GameMap};
use crate::jsnum::to_int32;
use crate::pathfinding::water_bounded::AStarWaterBounded;

use super::{PathFinder, PathStart};

const ENDPOINT_REFINEMENT_TILES: f64 = 50.0;
const LOCAL_ASTAR_MAX_AREA: f64 = 100.0 * 100.0;
const LOS_MIN_MAGNITUDE_PASS1: f64 = 2.0;
const LOS_MIN_MAGNITUDE_PASS2: f64 = 3.0;
const MAGNITUDE_MASK: u8 = 0x1f;
const PADDING: f64 = 10.0;
const MAX_TILES: f64 = 100_000.0;

/// The injected `(tile: TileRef) => boolean` traversability predicate. The
/// TS default is `(t) => map.isWater(t)` — [`WaterTraversable`].
pub trait Traversable {
    fn is_traversable(&mut self, tile: f64) -> bool;
}

/// `WaterTraversable` — the constructor's default predicate.
pub struct WaterTraversable<'a>(pub &'a GameMap);

impl Traversable for WaterTraversable<'_> {
    fn is_traversable(&mut self, tile: f64) -> bool {
        self.0.is_water(tile)
    }
}

/// `SmoothingWaterTransformer` from the TS source.
pub struct SmoothingWaterTransformer<'a, I: PathFinder, T: Traversable> {
    inner: I,
    map: &'a GameMap,
    local_astar: AStarWaterBounded,
    terrain: Vec<u8>,
    map_width: f64,
    is_traversable: T,
}

impl<'a, I: PathFinder, T: Traversable> PathFinder for SmoothingWaterTransformer<'a, I, T> {
    fn find_path(&mut self, starts: PathStart<'_>, goal: f64) -> Option<Vec<f64>> {
        SmoothingWaterTransformer::find_path(self, starts, goal)
    }
}

impl<'a, I: PathFinder, T: Traversable> SmoothingWaterTransformer<'a, I, T> {
    /// `constructor(inner, map, isTraversable?)`. The local A* is built with
    /// `LOCAL_ASTAR_MAX_AREA` and the TS `??` defaults for the config.
    pub fn new(inner: I, map: &'a GameMap, is_traversable: T) -> Self {
        let map_width = map.width();
        let terrain = map.debug_terrain().to_vec();
        Self {
            inner,
            map,
            local_astar: AStarWaterBounded::new(
                map_width,
                terrain.clone(),
                LOCAL_ASTAR_MAX_AREA,
                None,
                None,
            ),
            terrain,
            map_width,
            is_traversable,
        }
    }

    /// `findPath(from, to)`. DebugSpan wrapping is a pass-through (disabled
    /// by default), so only the smoothing is observable.
    pub fn find_path(&mut self, from: PathStart<'_>, to: f64) -> Option<Vec<f64>> {
        let path = self.inner.find_path(from, to);
        path.map(|p| self.smooth(p))
    }

    /// `smooth(path)`: LOS -> endpoint refinement -> LOS again.
    fn smooth(&mut self, path: Vec<f64>) -> Vec<f64> {
        if path.len() <= 2 {
            return path;
        }
        let smoothed = self.los_smooth(path, LOS_MIN_MAGNITUDE_PASS1);
        let smoothed = self.refine_endpoints(smoothed);
        self.los_smooth(smoothed, LOS_MIN_MAGNITUDE_PASS2)
    }

    /// `losSmooth(path, minMagnitude)`: greedy binary search for the
    /// farthest LOS waypoint, splicing the Bresenham trace in between.
    fn los_smooth(&mut self, path: Vec<f64>, min_magnitude: f64) -> Vec<f64> {
        let mut result = vec![path[0]];
        let mut current = 0usize;

        while current < path.len() - 1 {
            let mut lo = current + 1;
            let mut hi = path.len() - 1;
            let mut farthest = lo;

            while lo <= hi {
                let mid = (lo + hi) >> 1;
                if self.can_seen(path[current], path[mid], min_magnitude) {
                    farthest = mid;
                    lo = mid + 1;
                } else {
                    hi = mid - 1;
                }
            }

            if farthest > current + 1 {
                if let Some(trace) = self.trace_path(path[current], path[farthest]) {
                    // TS `for (i = 1; i < trace.length - 1; i++)` — empty
                    // when the trace is shorter than 3 tiles.
                    for t in trace.iter().skip(1).take(trace.len().saturating_sub(2)) {
                        result.push(*t);
                    }
                }
            }

            current = farthest;
            if current < path.len() - 1 {
                result.push(path[current]);
            }
        }

        result.push(path[path.len() - 1]);
        result
    }

    /// `refineEndpoints(path)`: replace the start segment (up to
    /// `ENDPOINT_REFINEMENT_TILES` manhattan) with a local A* result, then
    /// the end segment (searched destination-backwards and reversed).
    fn refine_endpoints(&mut self, path: Vec<f64>) -> Vec<f64> {
        if path.len() <= 2 {
            return path;
        }

        let refine_dist = ENDPOINT_REFINEMENT_TILES;
        let mut result = path;

        let start_end_idx = self.find_tile_at_distance(&result, 0, refine_dist, true);

        if start_end_idx > 1 {
            let from = result[0];
            let to = result[start_end_idx];
            let segment = self.refine_segment(from, to);
            if let Some(segment) = segment {
                if !segment.is_empty() {
                    // [...startSegment.slice(0, -1), ...result.slice(startEndIdx)]
                    let mut new_result = segment[..segment.len() - 1].to_vec();
                    new_result.extend_from_slice(&result[start_end_idx..]);
                    result = new_result;
                }
            }
        }

        let last = result.len() - 1;
        let end_start_idx = self.find_tile_at_distance(&result, last, refine_dist, false);

        if (end_start_idx as f64) < result.len() as f64 - 2.0 {
            let from = result[last];
            let to = result[end_start_idx];
            let segment = self.refine_segment(from, to);
            if let Some(mut segment) = segment {
                if !segment.is_empty() {
                    segment.reverse();
                    // [...result.slice(0, endStartIdx), ...endSegment]
                    let mut new_result = result[..end_start_idx].to_vec();
                    new_result.extend_from_slice(&segment);
                    result = new_result;
                }
            }
        }

        result
    }

    /// `findTileAtDistance(path, startIdx, distance, forward)`: walk until
    /// the cumulative manhattan distance reaches `distance` (or the path
    /// ends).
    fn find_tile_at_distance(
        &self,
        path: &[f64],
        start_idx: usize,
        distance: f64,
        forward: bool,
    ) -> usize {
        let mut cum_dist = 0.0;
        let mut idx = start_idx;

        if forward {
            while idx < path.len() - 1 && cum_dist < distance {
                cum_dist += self.manhattan_dist(path[idx], path[idx + 1]);
                idx += 1;
            }
        } else {
            while idx > 0 && cum_dist < distance {
                cum_dist += self.manhattan_dist(path[idx], path[idx - 1]);
                idx -= 1;
            }
        }

        idx
    }

    /// `refineSegment(from, to)`: bounded local A* over the padded bounding
    /// box of the two endpoints. The TS single-start argument normalises to
    /// a one-element array inside `searchBounded`.
    fn refine_segment(&mut self, from: f64, to: f64) -> Option<Vec<f64>> {
        let x0 = self.map.x(from);
        let y0 = self.map.y(from);
        let x1 = self.map.x(to);
        let y1 = self.map.y(to);

        let min_x = js_max(0.0, js_min(x0, x1) - PADDING);
        let max_x = js_min(self.map.width() - 1.0, js_max(x0, x1) + PADDING);
        let min_y = js_max(0.0, js_min(y0, y1) - PADDING);
        let max_y = js_min(self.map.height() - 1.0, js_max(y0, y1) + PADDING);

        self.local_astar
            .search_bounded(&[from], to, min_x, max_x, min_y, max_y)
    }

    /// `canSee(from, to, minMagnitude)`: Bresenham line-of-sight gated on
    /// traversability *and* water depth.
    fn can_seen(&mut self, from: f64, to: f64, min_magnitude: f64) -> bool {
        let x0 = from % self.map_width;
        let y0 = to_int32(from / self.map_width) as f64;
        let x1 = to % self.map_width;
        let y1 = to_int32(to / self.map_width) as f64;

        let dx = (x1 - x0).abs();
        let dy = (y1 - y0).abs();
        let sx = if x0 < x1 { 1.0 } else { -1.0 };
        let sy = if y0 < y1 { 1.0 } else { -1.0 };
        let mut err = dx - dy;

        let mut x = x0;
        let mut y = y0;

        let mut iterations = 0.0;

        loop {
            if iterations > MAX_TILES {
                return false;
            }
            iterations += 1.0;

            let tile = y * self.map_width + x;
            if !self.is_traversable.is_traversable(tile) {
                return false;
            }
            if self.magnitude_at(tile) < min_magnitude {
                return false;
            }

            if x == x1 && y == y1 {
                return true;
            }

            let e2 = 2.0 * err;
            let should_move_x = e2 > -dy;
            let should_move_y = e2 < dx;

            if should_move_x && should_move_y {
                x += sx;
                err -= dy;

                let intermediate = y * self.map_width + x;
                if !self.is_traversable.is_traversable(intermediate)
                    || self.magnitude_at(intermediate) < min_magnitude
                {
                    x -= sx;
                    err += dy;
                    y += sy;
                    err += dx;

                    let alt = y * self.map_width + x;
                    if !self.is_traversable.is_traversable(alt)
                        || self.magnitude_at(alt) < min_magnitude
                    {
                        return false;
                    }

                    x += sx;
                    err -= dy;
                } else {
                    y += sy;
                    err += dx;
                }
            } else {
                if should_move_x {
                    x += sx;
                    err -= dy;
                }
                if should_move_y {
                    y += sy;
                    err += dx;
                }
            }
        }
    }

    /// `tracePath(from, to)`: the Bresenham tile sequence, gated only on
    /// traversability. `None` mirrors the TS `null`.
    fn trace_path(&mut self, from: f64, to: f64) -> Option<Vec<f64>> {
        let x0 = from % self.map_width;
        let y0 = to_int32(from / self.map_width) as f64;
        let x1 = to % self.map_width;
        let y1 = to_int32(to / self.map_width) as f64;

        let mut tiles: Vec<f64> = Vec::new();

        let dx = (x1 - x0).abs();
        let dy = (y1 - y0).abs();
        let sx = if x0 < x1 { 1.0 } else { -1.0 };
        let sy = if y0 < y1 { 1.0 } else { -1.0 };
        let mut err = dx - dy;

        let mut x = x0;
        let mut y = y0;

        let mut iterations = 0.0;

        loop {
            if iterations > MAX_TILES {
                return None;
            }
            iterations += 1.0;

            let tile = y * self.map_width + x;
            if !self.is_traversable.is_traversable(tile) {
                return None;
            }

            tiles.push(tile);

            if x == x1 && y == y1 {
                break;
            }

            let e2 = 2.0 * err;
            let should_move_x = e2 > -dy;
            let should_move_y = e2 < dx;

            if should_move_x && should_move_y {
                x += sx;
                err -= dy;

                let intermediate = y * self.map_width + x;
                if !self.is_traversable.is_traversable(intermediate) {
                    x -= sx;
                    err += dy;
                    y += sy;
                    err += dx;

                    let alt = y * self.map_width + x;
                    if !self.is_traversable.is_traversable(alt) {
                        return None;
                    }
                    tiles.push(alt);

                    x += sx;
                    err -= dy;
                } else {
                    tiles.push(intermediate);
                    y += sy;
                    err += dx;
                }
            } else {
                if should_move_x {
                    x += sx;
                    err -= dy;
                }
                if should_move_y {
                    y += sy;
                    err += dx;
                }
            }
        }

        Some(tiles)
    }

    /// `manhattanDist(a, b)` over `tile % width` / `(tile / width) | 0`.
    fn manhattan_dist(&self, a: f64, b: f64) -> f64 {
        let ax = a % self.map_width;
        let ay = to_int32(a / self.map_width) as f64;
        let bx = b % self.map_width;
        let by = to_int32(b / self.map_width) as f64;
        (ax - bx).abs() + (ay - by).abs()
    }

    /// `terrain[tile] & 0x1f` with JS `Uint8Array` semantics: OOB /
    /// negative / fractional indices read `undefined` -> bit test sees `0`.
    #[inline]
    fn magnitude_at(&self, tile: f64) -> f64 {
        let byte = if tile.fract() != 0.0 || tile < 0.0 || !tile.is_finite() {
            0
        } else {
            self.terrain.get(tile as usize).copied().unwrap_or(0)
        };
        f64::from(byte & MAGNITUDE_MASK)
    }
}
