//! `src/core/game/Railroad.ts` — rail segment geometry and orientation.
//!
//! The module's three collaborators are all reached through a narrow surface,
//! so the port models them by reference identity plus the few observable
//! calls, exactly as the capture's duck-typed stubs do:
//!
//! * `game.x(ref)` / `game.y(ref)` decode a packed `TileRef` (`ref % width`,
//!   `(ref / width) | 0`) — the same JS-accurate formulas `game_map` uses.
//! * `game.addUpdate(u)` is recorded as the emitted `[type, id]` pair.
//! * `station.getRailroadTo(to)` is a neighbor→railroad lookup; `
//!   station.removeRailroad(rr)` is recorded as a `(caller, rr)` call. The
//!   real mutation lives inside `TrainStation`, which is a later port — the
//!   only thing `Railroad.delete` itself decides is *which* stations it calls
//!   and with what, so that call sequence is the parity surface here.
//!
//! Reference identity (`railroad.to === to`, `railRoad.from === this`) rides
//! in as capture-assigned `refid`s: two handles to one object share a refid.

use crate::jsnum::to_int32;

/// `GameUpdateType.RailroadDestructionEvent` — the update `delete` emits.
pub const RAILROAD_DESTRUCTION_EVENT: f64 = 16.0;

/// `game.x(ref)`: JS `%` (sign follows the dividend, `-0` preserved).
fn tile_x(width: f64, tile: f64) -> f64 {
    tile % width
}

/// `game.y(ref)`: `(ref / width) | 0` — ToInt32 truncation toward zero.
fn tile_y(width: f64, tile: f64) -> f64 {
    to_int32(tile / width) as f64
}

/// `Railroad.getClosestTileIndex`: index of the tile nearest `to` by squared
/// Euclidean distance; strict `<` keeps the first on ties, `-1` when empty.
pub fn get_closest_tile_index(width: f64, tiles: &[f64], to: f64) -> f64 {
    if tiles.is_empty() {
        return -1.0;
    }
    let to_x = tile_x(width, to);
    let to_y = tile_y(width, to);
    let mut closest_index = 0usize;
    let mut min_dist_squared = f64::INFINITY;
    for (i, &tile) in tiles.iter().enumerate() {
        let dx = tile_x(width, tile) - to_x;
        let dy = tile_y(width, tile) - to_y;
        let dist_squared = dx * dx + dy * dy;
        if dist_squared < min_dist_squared {
            min_dist_squared = dist_squared;
            closest_index = i;
        }
    }
    closest_index as f64
}

/// A `Railroad` handle: its endpoints (station refids), tile refs, id and
/// its own refid for identity comparisons.
#[derive(Clone, Debug)]
pub struct Railroad {
    pub refid: f64,
    pub from: f64,
    pub to: f64,
    pub tiles: Vec<f64>,
    pub id: f64,
}

/// `OrientedRailroad` — a rail wrapped so `tiles` always start at index 0.
#[derive(Clone, Debug)]
pub struct OrientedRailroad {
    pub tiles: Vec<f64>,
    pub start: f64,
    pub end: f64,
    /// Whether the rail was stored `from -> to` (`railroad.to === to`).
    pub forward: bool,
}

/// `getOrientedRailroad(from, to)`: look up the rail `from` has to `to`
/// (`railroadByNeighbor`), decide direction by `railroad.to === to`, then
/// orient the tiles. `None` when there is no such rail.
pub fn get_oriented_railroad(
    by_neighbor: &[(f64, f64)],
    rails: &[Railroad],
    to: f64,
) -> Option<OrientedRailroad> {
    let rr_refid = by_neighbor.iter().find(|(n, _)| *n == to).map(|(_, r)| *r)?;
    let railroad = rails.iter().find(|r| r.refid == rr_refid)?;
    let forward = railroad.to == to;
    let tiles = if forward {
        railroad.tiles.clone()
    } else {
        let mut t = railroad.tiles.clone();
        t.reverse();
        t
    };
    let (start, end) = if forward {
        (railroad.from, railroad.to)
    } else {
        (railroad.to, railroad.from)
    };
    Some(OrientedRailroad { tiles, start, end, forward })
}

/// `Railroad.delete(game)`: the observable effects — one destruction update
/// plus the two `removeRailroad` calls (`from` then `to`, each with `this`).
/// Returns `[type, id, caller_from, rr, caller_to, rr]`.
pub fn delete(railroad: &Railroad) -> Vec<f64> {
    vec![
        RAILROAD_DESTRUCTION_EVENT,
        railroad.id,
        railroad.from,
        railroad.refid,
        railroad.to,
        railroad.refid,
    ]
}

// ---------------------------------------------------------------------------
// token codecs (shared with tools/gen_vectors.mjs / run_wasm_parity.mjs)
// ---------------------------------------------------------------------------

struct Cur<'a>(&'a [f64], usize);

impl<'a> Cur<'a> {
    fn f(&mut self) -> f64 {
        let v = self.0[self.1];
        self.1 += 1;
        v
    }
    fn u(&mut self) -> usize {
        self.f() as usize
    }
    fn tiles(&mut self) -> Vec<f64> {
        let n = self.u();
        (0..n).map(|_| self.f()).collect()
    }
    fn railroad(&mut self) -> Railroad {
        let refid = self.f();
        let from = self.f();
        let to = self.f();
        let id = self.f();
        let tiles = self.tiles();
        Railroad { refid, from, to, tiles, id }
    }
}

/// `kind`: 0 = `getClosestTileIndex` — args `[width, to, n, tiles…]`, res
/// `[index]`; 1 = `getOrientedRailroad` — args `[to, k, (neighbor, rr)*k, m,
/// (railroad)*m]` (railroad = `[refid, from, to, id, n, tiles…]`), res `[0]`
/// when absent else `[1, forward, n, tiles…, start, end]`; 2 = `delete` —
/// args `[railroad]`, res `[type, id, caller_from, rr, caller_to, rr]`.
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut c = Cur(args, 0);
    match kind {
        0 => {
            let width = c.f();
            let to = c.f();
            let tiles = c.tiles();
            vec![get_closest_tile_index(width, &tiles, to)]
        }
        1 => {
            let to = c.f();
            let k = c.u();
            let by_neighbor: Vec<(f64, f64)> = (0..k).map(|_| (c.f(), c.f())).collect();
            let m = c.u();
            let rails: Vec<Railroad> = (0..m).map(|_| c.railroad()).collect();
            match get_oriented_railroad(&by_neighbor, &rails, to) {
                None => vec![0.0],
                Some(o) => {
                    let mut out = vec![1.0, if o.forward { 1.0 } else { 0.0 }];
                    out.push(o.tiles.len() as f64);
                    out.extend(o.tiles);
                    out.push(o.start);
                    out.push(o.end);
                    out
                }
            }
        }
        _ => {
            let railroad = c.railroad();
            delete(&railroad)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closest_empty_is_neg_one() {
        assert_eq!(get_closest_tile_index(10.0, &[], 5.0), -1.0);
    }

    #[test]
    fn closest_picks_nearest_and_keeps_first_on_tie() {
        // width 10: tile 23 -> (3,2) d²=9, tile 41 -> (1,4) d²=5, to 20 -> (0,2).
        let got = get_closest_tile_index(10.0, &[23.0, 41.0], 20.0);
        assert_eq!(got, 1.0);
        // tie: two equidistant tiles keep the first index.
        let tie = get_closest_tile_index(10.0, &[10.0, 30.0], 20.0);
        assert_eq!(tie, 0.0);
    }

    #[test]
    fn oriented_forward_and_reverse() {
        let rails = vec![Railroad {
            refid: 1.0,
            from: 10.0,
            to: 20.0,
            tiles: vec![1.0, 2.0, 3.0],
            id: 7.0,
        }];
        let by = vec![(20.0, 1.0)];
        let fwd = get_oriented_railroad(&by, &rails, 20.0).unwrap();
        assert_eq!(fwd.tiles, vec![1.0, 2.0, 3.0]);
        assert_eq!((fwd.start, fwd.end), (10.0, 20.0));
        let rev = get_oriented_railroad(&by, &rails, 10.0).is_none();
        assert!(rev); // no neighbor entry for 10
    }

    #[test]
    fn oriented_reverse_tiles_when_backward() {
        let rails = vec![Railroad {
            refid: 1.0,
            from: 10.0,
            to: 20.0,
            tiles: vec![1.0, 2.0, 3.0],
            id: 7.0,
        }];
        // from-station is 20, neighbor 10 maps to the rail -> backward.
        let by = vec![(10.0, 1.0)];
        let o = get_oriented_railroad(&by, &rails, 10.0).unwrap();
        assert_eq!(o.tiles, vec![3.0, 2.0, 1.0]);
        assert_eq!((o.start, o.end), (20.0, 10.0));
    }

    #[test]
    fn delete_emits_update_and_two_calls() {
        let rr = Railroad { refid: 5.0, from: 1.0, to: 2.0, tiles: vec![], id: 9.0 };
        assert_eq!(delete(&rr), vec![16.0, 9.0, 1.0, 5.0, 2.0, 5.0]);
    }
}
