//! Port of the pure-`GameMap` subset of `src/core/execution/Util.ts`:
//! `computeNukeBlastCounts`, `getSpawnTiles`, `closestTile`, `nearestTileDist`,
//! `nearestTileDistCapped` and `closestTwoTiles`.
//!
//! The three `Game`-facade functions in the same file (`wouldNukeBreakAlliance`,
//! `listNukeBreakAlliance`, `calculateTerritoryCenter`) are **not** ported —
//! they need `Game.anyUnitNearby` / `nearbyUnits` / `Player.borderTiles`, which
//! belong to the `Game.ts` graph, not the `GameMap` surface.
//!
//! Faithfulness notes:
//!
//! * `computeNukeBlastCounts` walks `circleSearch` (insertion-ordered) and
//!   accumulates a `Map<number, number>`; the returned stream is that map's
//!   insertion order, so the same-owner tiles collapse in first-seen order.
//!   `weight` is `1` or `0.5` — exact binary fractions, no rounding surprises.
//! * `getSpawnTiles` uses `bfs(tile, euclDistFN(tile, 4, true))` — the centred
//!   Euclidean filter (root shifted by `-0.5, -0.5`), radius 4. `isInvalid` is
//!   `hasOwner || !isLand || isImpassable`.
//! * `closestTile` / `nearestTileDist` keep the strict `<` so the *first*
//!   minimum wins; `minDistance` starts at `Infinity` and `manhattanDist`
//!   propagates `NaN` (a `NaN` ref never beats `Infinity`).
//! * `nearestTileDistCapped` branches on the TS `isTileSetLike` duck test
//!   (`typeof tiles.has === "function"`): a `TileSet`-like walks Manhattan
//!   rings (`d = 1..cap`, `dx = -d..d`, `dy = d - |dx|`, checking `y1*w+x`
//!   then `y2*w+x` when `dy !== 0`); a plain iterable does the linear scan
//!   then clamps `d <= cap`. The ring scan's `tiles.has` is the `TileSet`
//!   probe (`Uint32Array` storage quirk), while the linear branch's
//!   `manhattanDist` is the map surface.
//! * `closestTwoTiles` sorts each list by `a % w` with a *stable* comparator
//!   (V8 `SortCompare` treats a `NaN` comparator result as `+0`, i.e. equal —
//!   reproduced by the `Less/Greater/else Equal` chain, which yields `Equal`
//!   for `NaN`). The two-pointer sweep advances `i` at the tail of `x`, else
//!   `j` at the tail of `y`, else the pointer with the smaller `x`-column.
//!   `(ref / w) | 0` is `to_int32(ref / w)`.

use crate::game_map::{eucl_dist_fn, GameMap};
use crate::jsnum::to_int32;
use crate::tile_set::TileSet;

/// `computeNukeBlastCounts` — weighted tile counts per owner in the blast
/// zone, as an insertion-ordered `Vec<(ownerId, weight)>`.
pub fn compute_nuke_blast_counts(gm: &GameMap, target_tile: f64, inner: f64, outer: f64) -> Vec<(f64, f64)> {
    let inner2 = inner * inner;
    // `circle_search` takes an `Fn`, so the accumulator is cell-refilled.
    let counts = std::cell::RefCell::new(Vec::<(f64, f64)>::new());
    gm.circle_search(target_tile, outer, |tile, d2| {
        let owner = gm.owner_id(tile);
        if owner > 0.0 {
            let weight = if d2 <= inner2 { 1.0 } else { 0.5 };
            let mut c = counts.borrow_mut();
            match c.iter_mut().find(|(id, _)| *id == owner) {
                Some(e) => e.1 += weight,
                None => c.push((owner, weight)),
            }
        }
        true
    });
    counts.into_inner()
}

/// `getSpawnTiles` — the valid spawn tiles reachable from `tile` within the
/// centred radius-4 Euclidean ball. `require_all_valid` selects the strict
/// overload: any invalid tile yields `None` instead of a filtered list.
pub fn get_spawn_tiles(gm: &GameMap, tile: f64, require_all_valid: bool) -> Option<Vec<f64>> {
    let spawn_tiles = gm.bfs(tile, &|gm, n| eucl_dist_fn(gm, tile, 4.0, true, n));
    let is_invalid = |t: f64| gm.has_owner(t) || !gm.is_land(t) || gm.is_impassable(t);
    if !require_all_valid {
        return Some(spawn_tiles.into_iter().filter(|t| !is_invalid(*t)).collect());
    }
    if spawn_tiles.iter().any(|t| is_invalid(*t)) {
        return None;
    }
    Some(spawn_tiles)
}

/// `closestTile` — `(ref, manhattanDist)` of the nearest member of `refs` to
/// `tile`; `(None, Infinity)` when `refs` is empty. First minimum wins.
pub fn closest_tile(gm: &GameMap, refs: &[f64], tile: f64) -> (Option<f64>, f64) {
    let mut min_distance = f64::INFINITY;
    let mut min_ref: Option<f64> = None;
    for &r in refs {
        let distance = gm.manhattan_dist(r, tile);
        if distance < min_distance {
            min_distance = distance;
            min_ref = Some(r);
        }
    }
    (min_ref, min_distance)
}

/// `nearestTileDist` — Manhattan distance to the nearest member, or `Infinity`
/// when empty. First minimum wins.
pub fn nearest_tile_dist(gm: &GameMap, tiles: &[f64], tile: f64) -> f64 {
    let mut best = f64::INFINITY;
    for &t in tiles {
        let d = gm.manhattan_dist(t, tile);
        if d < best {
            best = d;
        }
    }
    best
}

/// `nearestTileDistCapped` — the `isTileSetLike` duck test decides the branch:
/// `tileset_like` walks Manhattan rings; otherwise a linear scan clamped to
/// `cap`. Returns `Infinity` when nothing is within `cap`.
pub fn nearest_tile_dist_capped(
    gm: &GameMap,
    tiles: &[f64],
    tile: f64,
    cap: f64,
    tileset_like: bool,
) -> f64 {
    if !tileset_like {
        let d = nearest_tile_dist(gm, tiles, tile);
        return if d <= cap { d } else { f64::INFINITY };
    }
    let set = TileSet::new(Some(tiles));
    if set.size() == 0.0 {
        return f64::INFINITY;
    }
    if set.has(tile) {
        return 0.0;
    }
    let w = gm.width();
    let h = gm.height();
    let cx = gm.x(tile);
    let cy = gm.y(tile);
    let mut d = 1.0f64;
    while d <= cap {
        let mut dx = -d;
        while dx <= d {
            let x = cx + dx;
            if !(x < 0.0 || x >= w) {
                let dy = d - dx.abs();
                let y1 = cy - dy;
                if y1 >= 0.0 && set.has(y1 * w + x) {
                    return d;
                }
                if dy != 0.0 {
                    let y2 = cy + dy;
                    if y2 < h && set.has(y2 * w + x) {
                        return d;
                    }
                }
            }
            dx += 1.0;
        }
        d += 1.0;
    }
    f64::INFINITY
}

/// `closestTwoTiles` — the closest `(x, y)` pair across two lists, by the
/// inlined-coordinate Manhattan metric. `None` when either list is empty.
pub fn closest_two_tiles(gm: &GameMap, x: &[f64], y: &[f64]) -> Option<(f64, f64)> {
    let w = gm.width();
    let mut x_sorted = x.to_vec();
    let mut y_sorted = y.to_vec();
    // `sort((a, b) => (a % w) - (b % w))` — stable; a NaN comparator result is
    // `+0` (equal) per V8 `SortCompare`, which the `else Equal` arm reproduces.
    x_sorted.sort_by(|a, b| {
        let d = (a % w) - (b % w);
        if d < 0.0 {
            std::cmp::Ordering::Less
        } else if d > 0.0 {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    });
    y_sorted.sort_by(|a, b| {
        let d = (a % w) - (b % w);
        if d < 0.0 {
            std::cmp::Ordering::Less
        } else if d > 0.0 {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    });
    if x_sorted.is_empty() || y_sorted.is_empty() {
        return None;
    }
    let mut i = 0usize;
    let mut j = 0usize;
    let mut min_distance = f64::INFINITY;
    let mut result = (x_sorted[0], y_sorted[0]);
    while i < x_sorted.len() && j < y_sorted.len() {
        let current_x = x_sorted[i];
        let current_y = y_sorted[j];
        let cx_x = current_x % w;
        let cy_x = current_y % w;
        let distance = (cx_x - cy_x).abs()
            + ((to_int32(current_x / w) as f64) - (to_int32(current_y / w) as f64)).abs();
        if distance < min_distance {
            min_distance = distance;
            result = (current_x, current_y);
        }
        // The TS has two distinct arms that both advance `i`
        // (`j === ySorted.length - 1` and `cxX < cyX`); merged with `||`
        // — identical behavior.
        if i == x_sorted.len() - 1 {
            j += 1;
        } else if j == y_sorted.len() - 1 || cx_x < cy_x {
            i += 1;
        } else {
            j += 1;
        }
    }
    Some(result)
}

// ================================================================ run_op
// Shared native-replay / wasm-probe entry. The map is built by the caller
// (terrain + owners) and passed in; `args` carry the per-op scalars and the
// flat tile lists. Kind / arg / result token table:
//
// * kind 0 `computeNukeBlastCounts` args `[target, inner, outer]`
//   -> `[owner0, weight0, owner1, weight1, ...]` (insertion order)
// * kind 1 `getSpawnTiles` args `[tile, requireAll]`
//   -> `[1]` for the strict `null`, else `[0, len, tiles...]`
// * kind 2 `closestTile` args `[tile, refs...]` -> `[ref_or_NaN, dist]`
// * kind 3 `nearestTileDist` args `[tile, tiles...]` -> `[best]`
// * kind 4 `nearestTileDistCapped` args `[tile, cap, mode, tiles...]`
//   (`mode` 1 = TileSet-like, 0 = plain iterable) -> `[dist]`
// * kind 5 `closestTwoTiles` args `[nx, x..., y...]`
//   -> `[1]` for `null`, else `[0, x, y]`

/// One `run_op` call; see the module header for the kind/token table.
pub fn run_op(gm: &GameMap, kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    match kind {
        0 => {
            for (id, w) in compute_nuke_blast_counts(gm, args[0], args[1], args[2]) {
                out.push(id);
                out.push(w);
            }
        }
        1 => match get_spawn_tiles(gm, args[0], args[1] != 0.0) {
            None => out.push(1.0),
            Some(tiles) => {
                out.push(0.0);
                out.push(tiles.len() as f64);
                out.extend(tiles);
            }
        },
        2 => {
            let (r, d) = closest_tile(gm, &args[1..], args[0]);
            out.push(r.unwrap_or(f64::NAN));
            out.push(d);
        }
        3 => out.push(nearest_tile_dist(gm, &args[1..], args[0])),
        4 => out.push(nearest_tile_dist_capped(
            gm,
            &args[3..],
            args[0],
            args[1],
            args[2] != 0.0,
        )),
        _ => {
            let nx = args[0] as usize;
            let xs = &args[1..1 + nx];
            let ys = &args[1 + nx..];
            match closest_two_tiles(gm, xs, ys) {
                None => out.push(1.0),
                Some((x, y)) => {
                    out.push(0.0);
                    out.push(x);
                    out.push(y);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn land_map(w: f64, h: f64) -> GameMap {
        GameMap::new(w, h, vec![0x85; (w * h) as usize], w * h)
    }

    #[test]
    fn nuke_counts_weight_by_radius() {
        // 5x5 all-land, owner 1 on a 3x3 block centred at (2,2)... target the
        // centre; inner radius covers the centre tile only.
        let mut gm = land_map(5.0, 5.0);
        for t in 0..25 {
            gm.set_owner_id(t as f64, 1.0);
        }
        // inner=0 -> inner2=0: only d2==0 (the target itself) weighs 1; the
        // rest of the outer=1 ring weighs 0.5.
        let counts = compute_nuke_blast_counts(&gm, 12.0, 0.0, 1.0);
        assert_eq!(counts.len(), 1);
        assert_eq!(counts[0].0, 1.0);
        // target (d2=0 ->1) + 4 neighbours (d2=1 ->0.5) = 1 + 2 = 3.
        assert_eq!(counts[0].1, 3.0);
    }

    #[test]
    fn nuke_counts_skip_ownerless() {
        let gm = land_map(3.0, 3.0);
        // No owners -> empty map.
        assert!(compute_nuke_blast_counts(&gm, 4.0, 1.0, 1.0).is_empty());
    }

    #[test]
    fn spawn_tiles_strict_vs_filter() {
        let gm = land_map(5.0, 5.0);
        // All land, unowned: every bfs tile is valid, so both overloads
        // return the full list (non-null).
        let loose = get_spawn_tiles(&gm, 12.0, false).unwrap();
        let strict = get_spawn_tiles(&gm, 12.0, true).unwrap();
        assert_eq!(loose, strict);
        assert!(!loose.is_empty());
        assert!(loose.contains(&12.0));
    }

    #[test]
    fn spawn_tiles_strict_null_on_owned() {
        let mut gm = land_map(5.0, 5.0);
        gm.set_owner_id(12.0, 1.0);
        // Centre tile owned -> invalid -> strict returns null.
        assert!(get_spawn_tiles(&gm, 12.0, true).is_none());
        // Loose filters it out but still returns the rest.
        let loose = get_spawn_tiles(&gm, 12.0, false).unwrap();
        assert!(!loose.contains(&12.0));
    }

    #[test]
    fn closest_tile_first_minimum_wins() {
        let gm = land_map(10.0, 1.0);
        // refs at x=2 and x=5, target x=3: dist 1 vs 2 -> x=2 (ref 2).
        let (r, d) = closest_tile(&gm, &[2.0, 5.0], 3.0);
        assert_eq!(r, Some(2.0));
        assert_eq!(d, 1.0);
        // Tie: two refs equidistant -> first seen.
        let (r, d) = closest_tile(&gm, &[1.0, 3.0], 2.0);
        assert_eq!(r, Some(1.0));
        assert_eq!(d, 1.0);
        // Empty -> (None, Infinity).
        let (r, d) = closest_tile(&gm, &[], 2.0);
        assert_eq!(r, None);
        assert!(d.is_infinite());
    }

    #[test]
    fn nearest_tile_dist_matches_closest() {
        let gm = land_map(10.0, 1.0);
        assert_eq!(nearest_tile_dist(&gm, &[2.0, 5.0], 3.0), 1.0);
        assert!(nearest_tile_dist(&gm, &[], 3.0).is_infinite());
    }

    #[test]
    fn capped_linear_branch_clamps() {
        let gm = land_map(10.0, 1.0);
        // Plain iterable (mode false): dist 2, cap 1 -> Infinity; cap 2 -> 2.
        assert!(nearest_tile_dist_capped(&gm, &[5.0], 3.0, 1.0, false).is_infinite());
        assert_eq!(nearest_tile_dist_capped(&gm, &[5.0], 3.0, 2.0, false), 2.0);
    }

    #[test]
    fn capped_tileset_ring_scan() {
        // 5x5 map, target centre (12 => x2,y2). TileSet holds ref 14 (x4,y2),
        // manhattan distance 2.
        let gm = land_map(5.0, 5.0);
        assert_eq!(nearest_tile_dist_capped(&gm, &[14.0], 12.0, 2.0, true), 2.0);
        // cap 1 misses it.
        assert!(nearest_tile_dist_capped(&gm, &[14.0], 12.0, 1.0, true).is_infinite());
        // tile itself in the set -> 0.
        assert_eq!(nearest_tile_dist_capped(&gm, &[12.0], 12.0, 0.0, true), 0.0);
        // empty set -> Infinity.
        assert!(nearest_tile_dist_capped(&gm, &[], 12.0, 5.0, true).is_infinite());
    }

    #[test]
    fn closest_two_pair_sweep() {
        // 10x1: x refs {2}, y refs {5} -> pair (2,5) dist 3.
        let gm = land_map(10.0, 1.0);
        assert_eq!(closest_two_tiles(&gm, &[2.0], &[5.0]), Some((2.0, 5.0)));
        // empty -> None.
        assert_eq!(closest_two_tiles(&gm, &[], &[5.0]), None);
        // Multi: pick the genuinely closest pair after sorting by column.
        let gm2 = land_map(10.0, 2.0);
        // x at col 1 (ref 1) and col 8 (ref 8); y at col 3 (ref 3).
        // sorted x [1,8], y [3]. sweep: (1,3) d2, then advance... closest (1,3).
        assert_eq!(closest_two_tiles(&gm2, &[1.0, 8.0], &[3.0]), Some((1.0, 3.0)));
    }

    #[test]
    fn run_op_tokens() {
        let gm = land_map(5.0, 5.0);
        // kind 3 nearestTileDist tile=12 tiles=[14] -> [2].
        assert_eq!(run_op(&gm, 3, &[12.0, 14.0]), vec![2.0]);
        // kind 5 closestTwoTiles nx=1 x=[2] y=[4] -> [0,2,4].
        assert_eq!(run_op(&gm, 5, &[1.0, 2.0, 4.0]), vec![0.0, 2.0, 4.0]);
        // kind 5 empty y -> [1].
        assert_eq!(run_op(&gm, 5, &[1.0, 2.0]), vec![1.0]);
    }
}
