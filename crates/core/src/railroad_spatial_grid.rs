//! `src/core/game/RailroadSpatialGrid.ts` — coarse spatial index over rail
//! tiles, bucketed into `cellSize x cellSize` cells keyed by `"cx:cy"`.
//!
//! The grid is a pure geometry + identity structure: it reads only
//! `game.x(tile)` / `game.y(tile)` (the JS-accurate packed-`TileRef` decode the
//! ported [`GameMap`] already provides) and treats each `Railroad` as an opaque
//! identity carrying a `tiles: TileRef[]`. JS `Map`/`Set` key on *object
//! identity*, so — exactly as in [`crate::railroad`] — a rail rides in as a
//! capture-assigned `refid`: two handles to one rail share a refid, and
//! `Set<Railroad>` / `Map<Railroad, …>` key on it.
//!
//! Faithfulness notes:
//!
//! * JS `Map`/`Set` iterate in *insertion order* (SameValueZero keys). [`StrSet`]
//!   and [`RefSet`] replicate that; `delete` keeps the survivors' relative order.
//! * `key(cx, cy)` is the template string `` `${cx}:${cy}` ``. `cellOf` yields
//!   `Math.floor(x / cellSize)`, always an integer-valued `f64` (or `NaN` /
//!   `±Infinity` when the inputs are non-finite). [`js_int_str`] reproduces
//!   `String(Number)` for that domain: `-0` prints `"0"`, `NaN` prints `"NaN"`,
//!   `±Infinity` print `"Infinity"` / `"-Infinity"`, exact integers below `1e21`
//!   print as plain decimal digits.
//! * `cellSize <= 0` throws in the constructor; `cellSize = NaN` does *not*
//!   (`NaN <= 0` is false) and every cell then collapses to the `"NaN:NaN"` key.
//!   Both edges are on the parity surface.

use std::collections::HashMap;

use crate::game_map::GameMap;

/// `String(Number)` for the `Math.floor` domain: integer-valued `f64` or
/// `NaN` / `±Infinity`. `-0` collapses to `"0"`.
fn js_int_str(v: f64) -> String {
    if v.is_nan() {
        "NaN".to_string()
    } else if v == f64::INFINITY {
        "Infinity".to_string()
    } else if v == f64::NEG_INFINITY {
        "-Infinity".to_string()
    } else if v == 0.0 {
        "0".to_string()
    } else if v.abs() < 1e21 {
        // Rust's `Display` for f64 prints the full decimal expansion without
        // an exponent — identical to JS `String()` for integer-valued numbers
        // below 1e21 (e.g. 1e20 -> "100000000000000000000").
        format!("{v}")
    } else {
        // Outside the reachable grid domain; never exercised by the capture.
        format!("{v:e}")
    }
}

/// Ordered set of string keys (a `railToCells` value is a `Set<string>`).
#[derive(Debug, Default, Clone)]
struct StrSet {
    vals: Vec<String>,
}

fn ref_key(v: f64) -> u64 {
    if v == 0.0 {
        0.0f64.to_bits()
    } else {
        v.to_bits()
    }
}

/// Ordered set of rail refids with order-preserving `delete`.
#[derive(Debug, Default)]
struct RefSet {
    vals: Vec<f64>,
    idx: HashMap<u64, usize>,
}

impl RefSet {
    fn add(&mut self, v: f64) {
        let k = ref_key(v);
        if self.idx.contains_key(&k) {
            return;
        }
        self.idx.insert(k, self.vals.len());
        self.vals.push(v);
    }
    fn delete(&mut self, v: f64) -> bool {
        match self.idx.remove(&ref_key(v)) {
            Some(pos) => {
                self.vals.remove(pos);
                for (j, &x) in self.vals.iter().enumerate().skip(pos) {
                    self.idx.insert(ref_key(x), j);
                }
                true
            }
            None => false,
        }
    }
    fn is_empty(&self) -> bool {
        self.vals.is_empty()
    }
    fn iter(&self) -> impl Iterator<Item = f64> + '_ {
        self.vals.iter().copied()
    }
}

/// `RailSpatialGrid`. `game` supplies the `x`/`y` decode; rails cross by
/// `refid` with their tile lists attached at `register` time.
pub struct RailSpatialGrid {
    game: GameMap,
    cell_size: f64,
    /// `Map<string, Set<Railroad>>` — cell key -> rail refids.
    cells: HashMap<String, RefSet>,
    /// Cell-key insertion order (JS `Map` iteration order).
    cells_order: Vec<String>,
    cells_idx: HashMap<String, usize>,
    /// `Map<Railroad, Set<string>>` — rail refid -> its cell keys.
    rail_cells: HashMap<u64, StrSet>,
    /// Rail-refid insertion order for `rail_cells`.
    rail_order: Vec<f64>,
    rail_idx: HashMap<u64, usize>,
}

impl RailSpatialGrid {
    /// `new(game, cellSize)`; throws (panics) when `cellSize <= 0`.
    pub fn new(game: GameMap, cell_size: f64) -> Self {
        if cell_size <= 0.0 {
            panic!("cellSize must be > 0");
        }
        Self {
            game,
            cell_size,
            cells: HashMap::new(),
            cells_order: Vec::new(),
            cells_idx: HashMap::new(),
            rail_cells: HashMap::new(),
            rail_order: Vec::new(),
            rail_idx: HashMap::new(),
        }
    }

    fn key(&self, cx: f64, cy: f64) -> String {
        format!("{}:{}", js_int_str(cx), js_int_str(cy))
    }

    fn cell_of(&self, x: f64, y: f64) -> (f64, f64) {
        ((x / self.cell_size).floor(), (y / self.cell_size).floor())
    }

    /// `register(rail)`: defensive `unregister` first, then bucket each tile's
    /// cell (deduped per rail) and record the rail in it. A rail with no tiles
    /// leaves `railToCells` untouched (`railCells.size > 0` guard).
    pub fn register(&mut self, refid: f64, tiles: &[f64]) {
        self.unregister(refid);
        let mut rail_cells = StrSet::default();
        for &tile in tiles {
            let (cx, cy) = self.cell_of(self.game.x(tile), self.game.y(tile));
            let k = self.key(cx, cy);
            if rail_cells.vals.iter().any(|s| s == &k) {
                continue;
            }
            rail_cells.vals.push(k.clone());
            if !self.cells.contains_key(&k) {
                self.cells_idx.insert(k.clone(), self.cells_order.len());
                self.cells_order.push(k.clone());
                self.cells.insert(k.clone(), RefSet::default());
            }
            self.cells.get_mut(&k).unwrap().add(refid);
        }
        if !rail_cells.vals.is_empty() {
            let k = ref_key(refid);
            if !self.rail_idx.contains_key(&k) {
                self.rail_idx.insert(k, self.rail_order.len());
                self.rail_order.push(refid);
            }
            self.rail_cells.insert(k, rail_cells);
        }
    }

    /// `unregister(rail)`: drop the rail from every recorded cell, deleting an
    /// emptied cell; then forget the rail. No-op when the rail is unknown.
    pub fn unregister(&mut self, refid: f64) {
        let k = ref_key(refid);
        let keys = match self.rail_cells.get(&k) {
            Some(s) => s.vals.clone(),
            None => return,
        };
        for key in &keys {
            if let Some(set) = self.cells.get_mut(key) {
                set.delete(refid);
                if set.is_empty() {
                    if let Some(pos) = self.cells_idx.remove(key) {
                        self.cells.remove(key);
                        self.cells_order.remove(pos);
                        for j in pos..self.cells_order.len() {
                            self.cells_idx.insert(self.cells_order[j].clone(), j);
                        }
                    }
                }
            }
        }
        self.rail_cells.remove(&k);
        if let Some(pos) = self.rail_idx.remove(&k) {
            self.rail_order.remove(pos);
            for j in pos..self.rail_order.len() {
                self.rail_idx.insert(ref_key(self.rail_order[j]), j);
            }
        }
    }

    /// `query(tile, radius)`: union of the rails in every cell overlapping the
    /// `[x±radius, y±radius]` box, in nested `cx`-then-`cy` scan order.
    /// Non-finite bounds make the loop condition false immediately (JS
    /// `NaN <= x` / `-Infinity <= Infinity` semantics preserved by `f64`
    /// comparisons), yielding an empty result.
    pub fn query(&self, tile: f64, radius: f64) -> Vec<f64> {
        let x = self.game.x(tile);
        let y = self.game.y(tile);
        let c0 = self.cell_of(x - radius, y - radius);
        let c1 = self.cell_of(x + radius, y + radius);
        let mut result = RefSet::default();
        let mut cx = c0.0;
        while cx <= c1.0 {
            let mut cy = c0.1;
            while cy <= c1.1 {
                if let Some(set) = self.cells.get(&self.key(cx, cy)) {
                    for r in set.iter() {
                        result.add(r);
                    }
                }
                cy += 1.0;
            }
            cx += 1.0;
        }
        result.iter().collect()
    }

    /// Debug: cell keys in insertion order, each with its rail refids.
    pub fn debug_cells(&self) -> Vec<(String, Vec<f64>)> {
        self.cells_order
            .iter()
            .map(|k| (k.clone(), self.cells.get(k).unwrap().iter().collect()))
            .collect()
    }

    /// Debug: rails with a `railToCells` entry, insertion order, each with its
    /// cell keys.
    pub fn debug_rail_cells(&self) -> Vec<(f64, Vec<String>)> {
        self.rail_order
            .iter()
            .map(|&r| {
                let s = &self.rail_cells[&ref_key(r)];
                (r, s.vals.clone())
            })
            .collect()
    }
}

/// Stateful parity harness: owns the grid (built by the construct op) and
/// replays the recorded op stream. `kind`:
/// 0 construct `[width, height, cellSize]` → `[0]` (ok) | `[1]` (throw:
/// `cellSize <= 0`);
/// 1 `register(refid, n, tiles…)` → `[]`;
/// 2 `unregister(refid)` → `[]`;
/// 3 `query(tile, radius)` → `[len, refids…]`;
/// 4 `debug_cells()` → `[ncells, (klen, key-bytes…, m, refids…)*]` — keys
/// cross as their ASCII bytes;
/// 5 `debug_rail_cells()` → `[nrails, (refid, m, (klen, bytes…)*m)*]`.
///
/// The map is all-land (terrain zeros); the grid only reads `x`/`y`, so the
/// terrain contents are irrelevant to parity.
#[derive(Default)]
pub struct RigHarness {
    grid: Option<RailSpatialGrid>,
}

impl RigHarness {
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop the current grid (one scenario ends; the next starts with a
    /// construct op).
    pub fn reset(&mut self) {
        self.grid = None;
    }

    fn g(&mut self) -> &mut RailSpatialGrid {
        self.grid.as_mut().expect("rig harness: grid not constructed")
    }

    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut c = Cur(args, 0);
        match kind {
            0 => {
                let w = c.f();
                let h = c.f();
                let cell_size = c.f();
                if cell_size <= 0.0 {
                    self.grid = None;
                    return vec![1.0];
                }
                let map = GameMap::new(w, h, vec![0u8; (w * h) as usize], w * h);
                self.grid = Some(RailSpatialGrid::new(map, cell_size));
                vec![0.0]
            }
            1 => {
                let refid = c.f();
                let tiles = c.tiles();
                self.g().register(refid, &tiles);
                Vec::new()
            }
            2 => {
                self.g().unregister(c.f());
                Vec::new()
            }
            3 => {
                let tile = c.f();
                let radius = c.f();
                let r = self.g().query(tile, radius);
                let mut out = Vec::with_capacity(r.len() + 1);
                out.push(r.len() as f64);
                out.extend(r);
                out
            }
            4 => {
                let cells = self.g().debug_cells();
                let mut out = vec![cells.len() as f64];
                for (k, rails) in cells {
                    out.push(k.len() as f64);
                    out.extend(k.as_bytes().iter().map(|&b| b as f64));
                    out.push(rails.len() as f64);
                    out.extend(rails);
                }
                out
            }
            5 => {
                let rails = self.g().debug_rail_cells();
                let mut out = vec![rails.len() as f64];
                for (r, keys) in rails {
                    out.push(r);
                    out.push(keys.len() as f64);
                    for k in keys {
                        out.push(k.len() as f64);
                        out.extend(k.as_bytes().iter().map(|&b| b as f64));
                    }
                }
                out
            }
            _ => panic!("bad rail_grid op kind {kind}"),
        }
    }
}

struct Cur<'a>(&'a [f64], usize);

impl<'a> Cur<'a> {
    fn f(&mut self) -> f64 {
        let v = self.0[self.1];
        self.1 += 1;
        v
    }
    fn tiles(&mut self) -> Vec<f64> {
        let n = self.f() as usize;
        (0..n).map(|_| self.f()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gm(w: f64, h: f64) -> GameMap {
        GameMap::new(w, h, vec![0u8; (w * h) as usize], w * h)
    }

    #[test]
    fn ctor_rejects_nonpositive_cellsize() {
        assert!(std::panic::catch_unwind(|| RailSpatialGrid::new(gm(4.0, 4.0), 0.0)).is_err());
        assert!(std::panic::catch_unwind(|| RailSpatialGrid::new(gm(4.0, 4.0), -1.0)).is_err());
        // NaN passes the guard (NaN <= 0 is false).
        let g = RailSpatialGrid::new(gm(4.0, 4.0), f64::NAN);
        assert!(g.debug_cells().is_empty());
    }

    #[test]
    fn register_and_query_basic() {
        let mut g = RailSpatialGrid::new(gm(10.0, 10.0), 5.0);
        // Rail 1 tiles (2,2)=22 and (7,7)=77 -> cells "0:0" and "1:1".
        g.register(1.0, &[22.0, 77.0]);
        // Rail 2 tile (6,1)=16 -> cell "1:0".
        g.register(2.0, &[16.0]);
        // Query (2,2) radius 0 -> cell "0:0" -> rail 1.
        assert_eq!(g.query(22.0, 0.0), vec![1.0]);
        // Query (7,7) radius 0 -> cell "1:1" -> rail 1.
        assert_eq!(g.query(77.0, 0.0), vec![1.0]);
        // Query (6,1) radius 0 -> cell "1:0" -> rail 2.
        assert_eq!(g.query(16.0, 0.0), vec![2.0]);
    }

    #[test]
    fn query_radius_spans_cells_in_scan_order() {
        let mut g = RailSpatialGrid::new(gm(10.0, 10.0), 5.0);
        g.register(1.0, &[0.0]); // cell "0:0"
        g.register(2.0, &[6.0]); // cell "1:1"
        // Center (5,0)=5, radius 6: box x[-1,11] y[-6,6] -> cells cx in
        // [-1,2], cy in [-2,1]; union hits "0:0" (cx0,cy0) then "1:1".
        let r = g.query(5.0, 6.0);
        assert_eq!(r, vec![1.0, 2.0]);
    }

    #[test]
    fn unregister_prunes_empty_cells() {
        let mut g = RailSpatialGrid::new(gm(10.0, 10.0), 5.0);
        g.register(1.0, &[22.0]); // cell "0:0"
        g.unregister(1.0);
        assert!(g.debug_cells().is_empty());
        assert_eq!(g.query(22.0, 0.0), vec![]);
    }

    #[test]
    fn register_is_idempotent_replacement() {
        let mut g = RailSpatialGrid::new(gm(10.0, 10.0), 5.0);
        g.register(1.0, &[22.0]); // "0:0"
        g.register(1.0, &[77.0]); // moves to "1:1"
        assert_eq!(g.debug_cells().len(), 1);
        assert_eq!(g.debug_cells()[0].0, "1:1");
        assert_eq!(g.query(22.0, 0.0), vec![]);
        assert_eq!(g.query(77.0, 0.0), vec![1.0]);
    }

    #[test]
    fn empty_tiles_rail_is_not_tracked() {
        let mut g = RailSpatialGrid::new(gm(10.0, 10.0), 5.0);
        g.register(1.0, &[]);
        assert!(g.debug_rail_cells().is_empty());
    }

    #[test]
    fn key_string_formatting_edges() {
        assert_eq!(js_int_str(-0.0), "0");
        assert_eq!(js_int_str(f64::NAN), "NaN");
        assert_eq!(js_int_str(f64::INFINITY), "Infinity");
        assert_eq!(js_int_str(-3.0), "-3");
        assert_eq!(js_int_str(1e20), "100000000000000000000");
    }

    #[test]
    fn nan_cellsize_collapses_to_nan_key() {
        let mut g = RailSpatialGrid::new(gm(10.0, 10.0), f64::NAN);
        g.register(1.0, &[22.0]);
        let cells = g.debug_cells();
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].0, "NaN:NaN");
        assert_eq!(cells[0].1, vec![1.0]);
    }
}
