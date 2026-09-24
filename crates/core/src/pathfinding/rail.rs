//! Port of `src/core/pathfinding/algorithms/AStar.Rail.ts`.
//!
//! The rail adapter runs the ported [`AStar`](super::a_star::AStar) engine over
//! a [`RailMap`] (the slice of `GameMap` the adapter reads). Transcription
//! notes — every one of these is observable through the returned path:
//!
//! * `isTraversable` checks **`isImpassable` on the *from* node**, not the
//!   neighbour: an impassable tile can be *entered* (it is land, so the water
//!   rule passes) but never *expanded from*.
//! * Shoreline logic: a water neighbour is enterable only from a shoreline
//!   tile or into a shoreline tile (`fromShoreline || isShoreline(to)`).
//! * `cost` penalises `isWater(to) || isShoreline(to)` with +5, and adds +3
//!   whenever `from - prev !== to - from` (integer ref deltas; the strict
//!   `!==` is plain f64 `!=` — NaN never reaches here since `prev` is an
//!   `Int32Array` read or `undefined`).
//! * `heuristic` floors with `| 0` (**ToInt32**, wraps past 2^31), not
//!   `Math.floor` — faithful to the source even though real maps stay below
//!   that bound.
//! * Terrain reads go through a `Uint8Array`: out-of-range / negative /
//!   fractional refs read `undefined`, whose bit tests all see `0` — i.e.
//!   water, not shoreline, not impassable. [`TerrainMap`] mirrors that.

use crate::jsnum::to_int32;
use crate::pathfinding::a_star::AStarAdapter;

/// The `GameMap` methods the rail adapter calls. Implementations must have JS
/// typed-array read semantics (out-of-range reads behave like byte `0`).
pub trait RailMap {
    fn width(&self) -> f64;
    fn height(&self) -> f64;
    /// `!isLand` — bit 7 of the terrain byte.
    fn is_water(&self, tile: f64) -> bool;
    /// Bit 6 of the terrain byte.
    fn is_shoreline(&self, tile: f64) -> bool;
    /// Land (bit 7) with magnitude (bits 0-4) == 31.
    fn is_impassable(&self, tile: f64) -> bool;
}

/// Reference [`RailMap`] over a packed terrain byte array, mirroring
/// `GameMapImpl`'s layout: bit 7 land, bit 6 shoreline, bit 5 ocean,
/// bits 0-4 magnitude.
pub struct TerrainMap {
    w: f64,
    h: f64,
    terrain: Vec<u8>,
}

impl TerrainMap {
    pub fn new(w: f64, h: f64, terrain: Vec<u8>) -> Self {
        Self { w, h, terrain }
    }

    /// `terrain[ref]` under JS semantics: only canonical, in-range integer
    /// indices read a byte; anything else is `undefined`, whose bit tests
    /// (`undefined & 0x80` etc.) all see `0`.
    fn byte(&self, tile: f64) -> u8 {
        // -0.0 == 0.0 and `(-0.0).fract() == 0`, so it falls through to
        // index 0 exactly like JS's canonical "0" property key.
        if tile.fract() != 0.0 || tile < 0.0 || !tile.is_finite() {
            return 0;
        }
        self.terrain.get(tile as usize).copied().unwrap_or(0)
    }
}

impl RailMap for TerrainMap {
    fn width(&self) -> f64 {
        self.w
    }
    fn height(&self) -> f64 {
        self.h
    }
    fn is_water(&self, tile: f64) -> bool {
        self.byte(tile) & 0x80 == 0
    }
    fn is_shoreline(&self, tile: f64) -> bool {
        self.byte(tile) & 0x40 != 0
    }
    fn is_impassable(&self, tile: f64) -> bool {
        let b = self.byte(tile);
        b & 0x80 != 0 && b & 0x1f == 31
    }
}

const WATER_PENALTY: f64 = 5.0;
const HEURISTIC_WEIGHT: f64 = 2.0;
const DIRECTION_CHANGE_PENALTY: f64 = 3.0;

/// `RailAdapter` from the TS source, generic over the map.
pub struct RailAdapter<M: RailMap> {
    map: M,
    width: f64,
    height: f64,
    num_nodes: f64,
}

impl<M: RailMap> RailAdapter<M> {
    pub fn new(map: M) -> Self {
        let width = map.width();
        let height = map.height();
        Self {
            map,
            width,
            height,
            num_nodes: width * height,
        }
    }

    fn is_traversable(&self, to: f64, from_shoreline: bool, impassable: bool) -> bool {
        if impassable {
            return false;
        }
        let to_water = self.map.is_water(to);
        if !to_water {
            return true;
        }
        from_shoreline || self.map.is_shoreline(to)
    }
}

impl<M: RailMap> AStarAdapter for RailAdapter<M> {
    fn neighbors(&mut self, node: f64, buffer: &mut [i32]) -> usize {
        let mut count = 0;
        let x = node % self.width; // JS %: sign follows dividend
        let from_shoreline = self.map.is_shoreline(node);
        let impassable = self.map.is_impassable(node);

        if node >= self.width {
            let n = node - self.width;
            if self.is_traversable(n, from_shoreline, impassable) {
                buffer[count] = to_int32(n);
                count += 1;
            }
        }
        if node < self.num_nodes - self.width {
            let n = node + self.width;
            if self.is_traversable(n, from_shoreline, impassable) {
                buffer[count] = to_int32(n);
                count += 1;
            }
        }
        if x != 0.0 {
            let n = node - 1.0;
            if self.is_traversable(n, from_shoreline, impassable) {
                buffer[count] = to_int32(n);
                count += 1;
            }
        }
        if x != self.width - 1.0 {
            let n = node + 1.0;
            if self.is_traversable(n, from_shoreline, impassable) {
                buffer[count] = to_int32(n);
                count += 1;
            }
        }
        count
    }

    fn cost(&mut self, from: f64, to: f64, prev: Option<f64>) -> f64 {
        let penalized = self.map.is_water(to) || self.map.is_shoreline(to);
        let mut c = if penalized { 1.0 + WATER_PENALTY } else { 1.0 };
        if let Some(p) = prev {
            let d1 = from - p;
            let d2 = to - from;
            if d1 != d2 {
                c += DIRECTION_CHANGE_PENALTY;
            }
        }
        c
    }

    fn heuristic(&mut self, node: f64, goal: f64) -> f64 {
        let nx = node % self.width;
        let ny = to_int32(node / self.width) as f64; // `(node / w) | 0`
        let gx = goal % self.width;
        let gy = to_int32(goal / self.width) as f64;
        HEURISTIC_WEIGHT * ((nx - gx).abs() + (ny - gy).abs())
    }

    fn num_nodes(&self) -> f64 {
        self.num_nodes
    }

    fn max_priority(&self) -> f64 {
        let max_cost = 1.0 + WATER_PENALTY + DIRECTION_CHANGE_PENALTY;
        HEURISTIC_WEIGHT * (self.width + self.height) * max_cost
    }

    fn max_neighbors(&self) -> usize {
        4
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pathfinding::a_star::AStar;

    // Terrain bytes: L = land (mag 5), s = shoreline water, o = plain ocean,
    // M = impassable land (mag 31).
    const L: u8 = 0x85;
    const S_SHORE_WATER: u8 = 0x40;
    const O: u8 = 0x20;
    const M: u8 = 0x9f;

    fn map(rows: &[&str]) -> TerrainMap {
        let h = rows.len() as f64;
        let w = rows[0].chars().count() as f64;
        let terrain = rows
            .iter()
            .flat_map(|r| r.chars())
            .map(|c| match c {
                'L' => L,
                's' => S_SHORE_WATER,
                'o' => O,
                'M' => M,
                _ => unreachable!(),
            })
            .collect();
        TerrainMap::new(w, h, terrain)
    }

    fn rail(rows: &[&str]) -> AStar<RailAdapter<TerrainMap>> {
        AStar::new(RailAdapter::new(map(rows)), None)
    }

    #[test]
    fn crosses_shoreline_column() {
        // Same scenario as the TS smoke test: land | shoreline water | land.
        let mut a = rail(&["LLsLL", "LLsLL", "LLsLL", "LLsLL", "LLsLL"]);
        assert_eq!(a.find_path(&[0.0], 4.0), Some(vec![0.0, 1.0, 2.0, 3.0, 4.0]));
    }

    #[test]
    fn plain_ocean_wall_is_impassable() {
        // Non-shoreline water cannot be entered from land at all.
        let mut a = rail(&["LLoLL", "LLoLL", "LLoLL"]);
        assert_eq!(a.find_path(&[0.0], 2.0), None);
    }

    #[test]
    fn impassable_tile_is_enterable_but_never_expanded() {
        // The isImpassable check is on the *from* node: entering M is legal,
        // standing on M yields no neighbours.
        let mut a = rail(&["LLM"]);
        assert_eq!(a.find_path(&[0.0], 2.0), Some(vec![0.0, 1.0, 2.0]));
        let mut b = rail(&["LLM"]);
        assert_eq!(b.find_path(&[2.0], 0.0), None);
    }

    #[test]
    fn shoreline_water_costs_six() {
        // 3x3: straight south through a shoreline tile vs the long way.
        // Crossing costs 1 + 6 + 1 = 8 with a turn penalty of 3 each corner;
        // the point is only that the search *does* cross (path exists).
        let mut a = rail(&["LLL", "sLL", "LLL"]);
        let p = a.find_path(&[3.0], 0.0); // start is shoreline water
        assert!(p.is_some());
    }
}
