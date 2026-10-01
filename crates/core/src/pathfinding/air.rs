//! Port of `src/core/pathfinding/PathFinder.Air.ts` (`AirPathFinder`).
//!
//! A deterministic straight-ish line walk for aircraft: each step nudges one
//! tile toward the goal, choosing the X or Y axis by a seeded coin whose bias
//! is the destination's slope (`ratio`). The randomness is the whole contract,
//! so the port reuses the already-verified [`PseudoRandom`] and pins the exact
//! `chance(ratio)` call sequence.
//!
//! JS-isms transcribed literally:
//!
//! * `game.ticks()` seeds the generator **once at construction**; every
//!   `findPath` call builds a *fresh* `PseudoRandom(this.seed)`, so repeated
//!   queries from the same finder replay the identical random stream (the TS
//!   `new PseudoRandom(this.seed)` inside `findPath`).
//! * `Array.isArray(from)` **throws** — the union is not collapsed, unlike the
//!   grid engines. Modelled as a panic on [`PathStart::Multi`]; the parity
//!   replay catches it and the wasm probe observes it via would-throw.
//! * the loop guard `if (next === current) break` uses strict equality on
//!   `TileRef` (`f64`), so `+0`/`-0` are equal but `NaN` is never equal to
//!   itself (a NaN step would loop forever — unreachable given valid inputs).
//! * `computeNext` ends by `game.ref(nextX, nextY)`, which **throws** on
//!   out-of-range coordinates. The port panics there too (`GameMap::tile_ref`).
//! * `Math.floor(1 + |dy| / (|dx| + 1))` is a plain IEEE floor; the `+ 1`
//!   guards the divide-by-zero when the tile is directly above/below.

use crate::game_map::GameMap;
use crate::pseudo_random::PseudoRandom;

use super::PathStart;

/// `AirPathFinder` from `PathFinder.Air.ts`. The TS `game: Game` is narrowed
/// to the `GameMap` the algorithm actually reads (`x`/`y`/`ref`), with the
/// construction-time `game.ticks()` seed passed in.
pub struct AirPathFinder<'a> {
    game: &'a GameMap,
    seed: f64,
}

impl<'a> AirPathFinder<'a> {
    /// `new AirPathFinder(game)` — `seed = game.ticks()`.
    pub fn new(game: &'a GameMap, ticks: f64) -> Self {
        Self { game, seed: ticks }
    }

    /// TS `findPath`. A `Multi` start throws like the TS `Array.isArray`
    /// guard; the walk always returns a (possibly single-element) path.
    pub fn find_path(&self, from: PathStart<'_>, to: f64) -> Vec<f64> {
        let from = match from {
            PathStart::Single(f) => f,
            PathStart::Multi(_) => {
                panic!("AirPathFinder does not support multiple start points")
            }
        };

        let mut random = PseudoRandom::new(self.seed);
        let mut path = vec![from];
        let mut current = from;

        while current != to {
            let next = self.compute_next(current, to, &mut random);
            if next == current {
                break; // Prevent infinite loop if something breaks
            }
            current = next;
            path.push(current);
        }

        path
    }

    /// TS `computeNext`.
    fn compute_next(&self, from: f64, to: f64, random: &mut PseudoRandom) -> f64 {
        let x = self.game.x(from);
        let y = self.game.y(from);
        let dst_x = self.game.x(to);
        let dst_y = self.game.y(to);

        if x == dst_x && y == dst_y {
            return to;
        }

        let mut next_x = x;
        let mut next_y = y;
        let ratio = (1.0 + (dst_y - y).abs() / ((dst_x - x).abs() + 1.0)).floor();

        if x == dst_x {
            // Can only move in Y
            next_y += if y < dst_y { 1.0 } else { -1.0 };
        } else if y == dst_y || random.chance(ratio) {
            // Can only move in X, or the coin says X. `||` short-circuits so
            // `chance` is drawn only when `y != dst_y` — exactly the TS order.
            next_x += if x < dst_x { 1.0 } else { -1.0 };
        } else {
            next_y += if y < dst_y { 1.0 } else { -1.0 };
        }

        self.game.tile_ref(next_x, next_y)
    }

    /// Would-throw variant of `find_path` for the wasm probe: returns `None`
    /// where the TS `game.ref(...)` would throw (an out-of-range step) instead
    /// of panicking. A `Multi` start is not expressible here — the probe
    /// reports it directly from the `Array.isArray` flag.
    pub fn debug_find_path(&self, from: f64, to: f64) -> Option<Vec<f64>> {
        let mut random = PseudoRandom::new(self.seed);
        let mut path = vec![from];
        let mut current = from;

        while current != to {
            let next = self.debug_compute_next(current, to, &mut random)?;
            if next == current {
                break;
            }
            current = next;
            path.push(current);
        }

        Some(path)
    }

    /// Would-throw `compute_next`: `None` when `game.ref` would throw.
    fn debug_compute_next(
        &self,
        from: f64,
        to: f64,
        random: &mut PseudoRandom,
    ) -> Option<f64> {
        let x = self.game.x(from);
        let y = self.game.y(from);
        let dst_x = self.game.x(to);
        let dst_y = self.game.y(to);

        if x == dst_x && y == dst_y {
            return Some(to);
        }

        let mut next_x = x;
        let mut next_y = y;
        let ratio = (1.0 + (dst_y - y).abs() / ((dst_x - x).abs() + 1.0)).floor();

        if x == dst_x {
            next_y += if y < dst_y { 1.0 } else { -1.0 };
        } else if y == dst_y || random.chance(ratio) {
            next_x += if x < dst_x { 1.0 } else { -1.0 };
        } else {
            next_y += if y < dst_y { 1.0 } else { -1.0 };
        }

        if self.game.is_valid_coord(next_x, next_y) {
            Some(self.game.tile_ref(next_x, next_y))
        } else {
            None // TS `game.ref` throws
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(w: f64, h: f64) -> GameMap {
        GameMap::new(w, h, vec![0x83u8; (w * h) as usize], w * h)
    }

    #[test]
    fn same_tile_returns_single_element_path() {
        let gm = map(10.0, 10.0);
        let pf = AirPathFinder::new(&gm, 0.0);
        let t = gm.tile_ref(3.0, 4.0);
        assert_eq!(pf.find_path(PathStart::Single(t), t), vec![t]);
    }

    #[test]
    fn pure_vertical_walk_never_draws_random() {
        // dx == 0: the `x === dstX` branch moves only in Y, so `chance` is
        // never called and the path is deterministic for any seed.
        let gm = map(10.0, 10.0);
        let from = gm.tile_ref(5.0, 1.0);
        let to = gm.tile_ref(5.0, 8.0);
        for seed in [0.0, 42.0, 777.0] {
            let pf = AirPathFinder::new(&gm, seed);
            let path = pf.find_path(PathStart::Single(from), to);
            let ys: Vec<f64> = path.iter().map(|&t| gm.y(t)).collect();
            assert_eq!(ys, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0], "seed {seed}");
        }
    }

    #[test]
    fn pure_horizontal_walk_is_deterministic() {
        let gm = map(10.0, 10.0);
        let from = gm.tile_ref(1.0, 6.0);
        let to = gm.tile_ref(7.0, 6.0);
        let pf = AirPathFinder::new(&gm, 123.0);
        let path = pf.find_path(PathStart::Single(from), to);
        let xs: Vec<f64> = path.iter().map(|&t| gm.x(t)).collect();
        assert_eq!(xs, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]);
    }

    #[test]
    fn diagonal_walk_is_seed_deterministic() {
        // Both axes differ: `chance(ratio)` decides the interleaving, so the
        // exact tile sequence depends on the seed. Pin one seed's full path.
        let gm = map(16.0, 16.0);
        let from = gm.tile_ref(2.0, 2.0);
        let to = gm.tile_ref(9.0, 12.0);
        let pf = AirPathFinder::new(&gm, 42.0);
        let path = pf.find_path(PathStart::Single(from), to);
        // Every step is a single-tile move and the walk terminates at `to`.
        assert_eq!(*path.first().unwrap(), from);
        assert_eq!(*path.last().unwrap(), to);
        for w in path.windows(2) {
            let d = (gm.x(w[1]) - gm.x(w[0])).abs() + (gm.y(w[1]) - gm.y(w[0])).abs();
            assert_eq!(d, 1.0, "non-adjacent step");
        }
    }

    #[test]
    fn multi_start_throws() {
        let gm = map(10.0, 10.0);
        let pf = AirPathFinder::new(&gm, 0.0);
        let a = gm.tile_ref(1.0, 1.0);
        let b = gm.tile_ref(2.0, 2.0);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pf.find_path(PathStart::Multi(&[a, b]), b)
        }));
        assert!(r.is_err());
    }
}
