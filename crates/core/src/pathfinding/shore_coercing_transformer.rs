//! Port of `src/core/pathfinding/transformers/ShoreCoercingTransformer.ts`.
//!
//! A `PathFinder` decorator for water travel: shore/land starts are coerced
//! to their best adjacent water tile, the goal is coerced the same way, the
//! inner finder runs entirely on water tiles, and the original shore endpoints
//! are restored onto the returned path.
//!
//! JS-isms pinned here:
//! * `waterToOriginal` is a `Map` keyed by water tile: a raw water source
//!   `delete`s any earlier coercion to that tile (no restoration needed),
//!   and two shore tiles coercing to the same water tile overwrite (last
//!   write wins) — modelled as a `Vec<(water, original)>` with in-place
//!   `set`/`delete`/`get`.
//! * Both shore tiles still push the same water tile into `waterFrom`, so the
//!   inner finder can legitimately see duplicate starts.
//! * Single surviving water start collapses to a scalar start for `inner`
//!   (`waterFrom.length === 1 ? waterFrom[0] : waterFrom`).
//! * `bestWaterNeighbor` scores each water 4-neighbor by its own water-4-
//!   neighbor count, strict `>` (ties keep the *first* in `neighbors4` order),
//!   `-1` when none.
//! * `!path || path.length === 0` -> `null`; the module-level neighbor scratch
//!   buffers are reentrant-safe in single-threaded TS and become locals here
//!   (fully consumed before any re-entry, so not observable).

use crate::game_map::GameMap;

use super::{PathFinder, PathStart};

/// `ShoreCoercingTransformer` from the TS source.
pub struct ShoreCoercingTransformer<'a, I: PathFinder> {
    inner: I,
    map: &'a GameMap,
}

impl<'a, I: PathFinder> PathFinder for ShoreCoercingTransformer<'a, I> {
    fn find_path(&mut self, starts: PathStart<'_>, goal: f64) -> Option<Vec<f64>> {
        ShoreCoercingTransformer::find_path(self, starts, goal)
    }
}

impl<'a, I: PathFinder> ShoreCoercingTransformer<'a, I> {
    /// `constructor(inner, map)`.
    pub fn new(inner: I, map: &'a GameMap) -> Self {
        Self { inner, map }
    }

    /// `findPath(from, to)`.
    pub fn find_path(&mut self, from: PathStart<'_>, to: f64) -> Option<Vec<f64>> {
        let from_array = from.as_slice();
        // `Map<TileRef, TileRef>` keyed by water tile.
        let mut water_to_original: Vec<(f64, f64)> = Vec::new();
        let mut water_from: Vec<f64> = Vec::new();

        for &f in from_array {
            if self.map.is_water(f) {
                water_from.push(f);
                // A raw water source needs no shore restoration - and
                // overrides any earlier shore tile that coerced to this same
                // water tile (last write wins, matching processing order).
                map_delete(&mut water_to_original, f);
            } else {
                let water = self.best_water_neighbor(f);
                if water != -1.0 {
                    water_from.push(water);
                    map_set(&mut water_to_original, water, f);
                }
            }
        }

        if water_from.is_empty() {
            return None;
        }

        // Coerce the destination: shore tiles path to their best water
        // neighbor, with the original appended back afterwards.
        let mut water_to = to;
        let mut original_to: f64 = -1.0;
        if !self.map.is_water(to) {
            water_to = self.best_water_neighbor(to);
            if water_to == -1.0 {
                return None;
            }
            original_to = to;
        }

        let from_tiles = if water_from.len() == 1 {
            PathStart::Single(water_from[0])
        } else {
            PathStart::Multi(&water_from)
        };
        let mut path = match self.inner.find_path(from_tiles, water_to) {
            None => return None,
            Some(p) if p.is_empty() => return None,
            Some(p) => p,
        };

        // Restore original start shore tile.
        if let Some(original_shore) = map_get(&water_to_original, path[0]) {
            path.insert(0, original_shore);
        }

        // Append original to if different.
        if original_to != -1.0 && path[path.len() - 1] != original_to {
            path.push(original_to);
        }

        Some(path)
    }

    /// Best adjacent water neighbor of a shore tile (highest water-neighbor
    /// connectivity, first wins on ties), or `-1` if it has none.
    fn best_water_neighbor(&self, tile: f64) -> f64 {
        let mut best: f64 = -1.0;
        let mut max_score: f64 = -1.0;

        let mut nbuf = [0.0f64; 4];
        let num_neighbors = self.map.neighbors4(tile, &mut nbuf);
        for &n in nbuf.iter().take(num_neighbors) {
            if !self.map.is_water(n) {
                continue;
            }

            // Score by water neighbor count (connectivity).
            let score = self.count_water_neighbors(n) as f64;

            // Pick highest connectivity.
            if score > max_score {
                max_score = score;
                best = n;
            }
        }

        best
    }

    fn count_water_neighbors(&self, tile: f64) -> usize {
        let mut count = 0;
        let mut nbuf = [0.0f64; 4];
        let num_neighbors = self.map.neighbors4(tile, &mut nbuf);
        for &n in nbuf.iter().take(num_neighbors) {
            if self.map.is_water(n) {
                count += 1;
            }
        }
        count
    }
}

fn map_set(map: &mut Vec<(f64, f64)>, key: f64, value: f64) {
    if let Some(slot) = map.iter_mut().find(|(k, _)| *k == key) {
        slot.1 = value;
    } else {
        map.push((key, value));
    }
}

fn map_delete(map: &mut Vec<(f64, f64)>, key: f64) {
    map.retain(|(k, _)| *k != key);
}

fn map_get(map: &[(f64, f64)], key: f64) -> Option<f64> {
    map.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}
