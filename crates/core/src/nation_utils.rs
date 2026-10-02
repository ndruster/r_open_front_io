//! Port of `src/core/execution/nation/NationUtils.ts`: the
//! `randTerritoryTileArray` / `randTerritoryTile` sampling orchestration and
//! `findJuiciestTarget`'s normalize / strict-`>` best scan. The `Game` /
//! `Player` / `PseudoRandom` facades are scripted mocks (precedent:
//! `water_path_memo`'s inner mock): the capture feeds the real TS code
//! hand-scripted return streams and records every facade call (arguments +
//! return) into a flat trace, so the loop order, the `&&` short-circuits, the
//! `===` owner identity (modeled as a player id), the `Array.from(tiles())`
//! iteration order, the reduce filter and the normalize math are pinned in
//! the res stream; the port replays the same logic over the same token
//! stream. The facade *internals* (nextInt math, randElement's throw, the
//! GameMap geometry beyond `x` / `y`) are mock domain and out of the ported
//! surface.
//!
//! Faithfulness notes:
//!
//! * `boundingBox ??=` never fires through the public API:
//!   `randTerritoryTileArray` always passes the `calculateBoundingBox` result
//!   (an object, never null — an empty `borderTiles` yields
//!   `min=(Infinity,Infinity)`, `max=(-Infinity,-Infinity)`), so the default
//!   parameter path is dead and the capture pins that.
//! * `p.numTilesOwned() > 0 && p.numTilesOwned() <= 100` calls the facade
//!   twice (the second only when the first passes `> 0`), exactly as JS
//!   `&&` evaluates.
//! * `Math.min(...values)` / `Math.max(...values)` fold the binary
//!   NaN-propagating / ±0-correct [`crate::game_map::js_min`] / [`crate::game_map::js_max`]
//!   (spread semantics equal the left fold); `max > min` is false for NaN, so
//!   a NaN column normalizes everything to `0`.
//! * `troopGapRatio = maxTroops > 0 ? 1 - troops/maxTroops : 0` — the `>`
//!   test is false for `NaN` / `-0` / `0`, and `troops()` is only called on
//!   the true branch (the trace pins the absence).
//! * `juiciness > bestScore` is strict, so ties keep the first candidate
//!   (`bestScore` starts at `-Infinity`).
//! * `reduce` sums from `0` (`0 + -0` folds to `+0`); the structure filter
//!   (`Structures.has(type) && type !== UnitType.DefensePost && type !==
//!   UnitType.MissileSilo`) runs on the real `Game.ts` values — `UnitType` is
//!   a **string** enum (`DefensePost = "Defense Post"`, `MissileSilo =
//!   "Missile Silo"`) and `Structures = {City, DefensePost, SAMLauncher,
//!   MissileSilo, Port, Factory}`.
//! * `randTerritoryTileArray` pushes on `tile !== null` (an `undefined`
//!   randElement result would still push — the `undefined !== null` quirk);
//!   the capture keeps `randElement`'s scripted value inside `tiles()` so
//!   that path stays outside the domain.
//! * The bounding box reuses [`crate::util::calculate_bounding_box`] over a
//!   mock-shaped `GameMap` (`x = tile % width`, `y = (tile / width) | 0`),
//!   which is exactly the `GameMap` geometry the TS mock reproduces.

use crate::game_map::{js_max, js_min, GameMap};
use crate::util::calculate_bounding_box;

/// `UnitType` values the `Structures` group contains (Game.ts string enum).
const STRUCTURE_TYPES: [&str; 6] = [
    "City",
    "Defense Post",
    "SAM Launcher",
    "Missile Silo",
    "Port",
    "Factory",
];

/// `Structures.has(type) && type !== UnitType.DefensePost && type !==
/// UnitType.MissileSilo` — the reduce filter.
fn counted_structure(t: &[u16]) -> bool {
    let s: String = String::from_utf16_lossy(t);
    STRUCTURE_TYPES.contains(&s.as_str()) && s != "Defense Post" && s != "Missile Silo"
}

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
    fn units(&mut self) -> Vec<u16> {
        let len = self.u();
        (0..len).map(|_| self.f() as u16).collect()
    }
    fn list(&mut self) -> Vec<f64> {
        let len = self.u();
        (0..len).map(|_| self.f()).collect()
    }
}

/// `Math.min(...values)` over the spread — binary fold (equal multi-arg
/// semantics for NaN / ±0).
fn spread_min(values: &[f64]) -> f64 {
    values.iter().fold(f64::INFINITY, |a, &b| js_min(a, b))
}

/// `Math.max(...values)` over the spread.
fn spread_max(values: &[f64]) -> f64 {
    values.iter().fold(f64::NEG_INFINITY, |a, &b| js_max(a, b))
}

/// `normalize(value, values)`.
fn normalize(value: f64, values: &[f64]) -> f64 {
    let min = spread_min(values);
    let max = spread_max(values);
    if max > min {
        (value - min) / (max - min)
    } else {
        0.0
    }
}

fn push_units(out: &mut Vec<f64>, u: &[u16]) {
    out.push(u.len() as f64);
    out.extend(u.iter().map(|&x| f64::from(x)));
}

/// One scripted `nextInt(min, max)` call: trace ev 0, return the script
/// value at `idx`.
fn next_int(
    trace: &mut Vec<f64>,
    min: f64,
    max: f64,
    idx: &mut usize,
    script: &[f64],
) -> f64 {
    let ret = script[*idx];
    *idx += 1;
    trace.push(0.0);
    trace.push(min);
    trace.push(max);
    trace.push(ret);
    ret
}

/// kind 0: `randTerritoryTileArray(random, mg, player, numTiles)`.
fn run_array(c: &mut Cur, trace: &mut Vec<f64>, payload: &mut Vec<f64>) {
    let width = c.f();
    let height = c.f();
    let num_tiles = c.f();
    let border = c.list();
    let next_script = c.list();
    let on_script = c.list();
    let ref_script = c.list();
    let own_script = c.list();
    let rand_ret = c.f();
    let n_owned = c.f();
    let tiles = c.list();
    let player_id = c.f();

    // The mock GameMap surface calculateBoundingBox reads is x = tile % width
    // (JS %, ±0 kept) and y = (tile / width) | 0 — exactly `GameMap::x/y`.
    let gm = GameMap::new(width, height, vec![0u8; (width * height) as usize], 0.0);
    let bb = calculate_bounding_box(&gm, border.iter().copied());

    let mut ni = 0usize;
    let mut oi = 0usize;
    let mut ri = 0usize;
    let mut wi = 0usize;

    trace.push(5.0);
    trace.push(border.len() as f64);
    trace.extend(border.iter().copied());

    let mut result: Vec<f64> = Vec::new();
    let mut i = 0.0f64;
    while i < num_tiles {
        // randTerritoryTile: boundingBox always passed (??= dead).
        let mut tile: Option<f64> = None;
        for _ in 0..100 {
            let rand_x = next_int(trace, bb.min.x, bb.max.x, &mut ni, &next_script);
            let rand_y = next_int(trace, bb.min.y, bb.max.y, &mut ni, &next_script);
            let on = on_script[oi];
            oi += 1;
            trace.push(2.0);
            trace.push(rand_x);
            trace.push(rand_y);
            trace.push(on);
            if on != 1.0 {
                continue;
            }
            let rand_tile = ref_script[ri];
            ri += 1;
            trace.push(3.0);
            trace.push(rand_x);
            trace.push(rand_y);
            trace.push(rand_tile);
            let id = own_script[wi];
            wi += 1;
            trace.push(4.0);
            trace.push(rand_tile);
            trace.push(id);
            if id == player_id {
                tile = Some(rand_tile);
                break;
            }
        }
        if tile.is_none() {
            // p.numTilesOwned() > 0 && p.numTilesOwned() <= 100 — two calls.
            trace.push(6.0);
            trace.push(n_owned);
            if n_owned > 0.0 {
                trace.push(6.0);
                trace.push(n_owned);
                if n_owned <= 100.0 {
                    trace.push(7.0);
                    trace.push(tiles.len() as f64);
                    trace.extend(tiles.iter().copied());
                    trace.push(1.0);
                    trace.push(tiles.len() as f64);
                    trace.extend(tiles.iter().copied());
                    trace.push(rand_ret);
                    tile = Some(rand_ret);
                }
            }
        }
        // `tile !== null` — Some pushes; the capture keeps randElement's
        // scripted value inside tiles().
        if let Some(t) = tile {
            result.push(t);
        }
        i += 1.0;
    }

    payload.push(result.len() as f64);
    payload.extend(result.iter().copied());
}

/// kind 1: `findJuiciestTarget(game, candidates)`.
fn run_juice(c: &mut Cur, trace: &mut Vec<f64>, payload: &mut Vec<f64>) {
    let nc = c.u();
    struct Cand {
        id: f64,
        troops: f64,
        n_owned: f64,
        max_troops: f64,
        units: Vec<(Vec<u16>, f64)>,
    }
    let mut cands: Vec<Cand> = Vec::new();
    for _ in 0..nc {
        let id = c.f();
        let troops = c.f();
        let n_owned = c.f();
        let max_troops = c.f();
        let nu = c.u();
        let mut units = Vec::with_capacity(nu);
        for _ in 0..nu {
            let t = c.units();
            let l = c.f();
            units.push((t, l));
        }
        cands.push(Cand {
            id,
            troops,
            n_owned,
            max_troops,
            units,
        });
    }

    // stats = candidates.map(...): per candidate units() (+ reduce filter
    // trace), config(), maxTroops(p), [troops() only when maxTroops > 0],
    // numTilesOwned().
    let mut structure_counts: Vec<f64> = Vec::new();
    let mut gap_ratios: Vec<f64> = Vec::new();
    let mut tile_counts: Vec<f64> = Vec::new();
    for cd in &cands {
        trace.push(10.0);
        trace.push(cd.id);
        trace.push(cd.units.len() as f64);
        let mut sum = 0.0f64;
        for (t, l) in &cd.units {
            let counted = counted_structure(t);
            trace.push(12.0);
            trace.push(cd.id);
            push_units(trace, t);
            trace.push(*l);
            trace.push(if counted { 1.0 } else { 0.0 });
            if counted {
                sum += l;
            }
        }
        trace.push(8.0);
        trace.push(9.0);
        trace.push(cd.id);
        trace.push(cd.max_troops);
        let ratio = if cd.max_troops > 0.0 {
            trace.push(11.0);
            trace.push(cd.id);
            trace.push(cd.troops);
            1.0 - cd.troops / cd.max_troops
        } else {
            0.0
        };
        trace.push(6.0);
        trace.push(cd.id);
        trace.push(cd.n_owned);
        structure_counts.push(sum);
        gap_ratios.push(ratio);
        tile_counts.push(cd.n_owned);
    }

    let mut juiciness: Vec<f64> = Vec::with_capacity(nc);
    for i in 0..nc {
        juiciness.push(
            normalize(structure_counts[i], &structure_counts)
                + normalize(gap_ratios[i], &gap_ratios)
                + normalize(tile_counts[i], &tile_counts),
        );
    }

    let mut best: Option<usize> = None;
    let mut best_score = f64::NEG_INFINITY;
    for (i, &j) in juiciness.iter().enumerate() {
        if j > best_score {
            best_score = j;
            best = Some(i);
        }
    }

    payload.push(nc as f64);
    payload.extend(juiciness.iter().copied());
    match best {
        None => payload.push(0.0),
        Some(i) => {
            payload.push(1.0);
            payload.push(cands[i].id);
        }
    }
}

pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut c = Cur(args, 0);
    let k = c.u() as u8;
    let mut trace: Vec<f64> = Vec::new();
    let mut payload: Vec<f64> = Vec::new();
    if k == kind {
        match kind {
            0 => run_array(&mut c, &mut trace, &mut payload),
            1 => run_juice(&mut c, &mut trace, &mut payload),
            _ => {}
        }
    }
    let mut out = Vec::with_capacity(1 + trace.len() + payload.len());
    out.push(trace.len() as f64);
    out.extend(trace.iter().copied());
    out.extend(payload.iter().copied());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc_str(s: &str) -> Vec<f64> {
        let u: Vec<u16> = s.encode_utf16().collect();
        let mut v = vec![u.len() as f64];
        v.extend(u.iter().map(|&x| f64::from(x)));
        v
    }

    /// kind 0 args builder: the scripted-mock scenario fields.
    struct Spec0<'a> {
        width: f64,
        height: f64,
        num_tiles: f64,
        border: &'a [f64],
        next: &'a [f64],
        on: &'a [f64],
        refs: &'a [f64],
        own: &'a [f64],
        rand_ret: f64,
        n_owned: f64,
        tiles: &'a [f64],
        player_id: f64,
    }
    impl<'a> Spec0<'a> {
        fn build(&self) -> Vec<f64> {
            let mut v = vec![0.0, self.width, self.height, self.num_tiles];
            v.push(self.border.len() as f64);
            v.extend(self.border.iter().copied());
            v.push(self.next.len() as f64);
            v.extend(self.next.iter().copied());
            v.push(self.on.len() as f64);
            v.extend(self.on.iter().copied());
            v.push(self.refs.len() as f64);
            v.extend(self.refs.iter().copied());
            v.push(self.own.len() as f64);
            v.extend(self.own.iter().copied());
            v.push(self.rand_ret);
            v.push(self.n_owned);
            v.push(self.tiles.len() as f64);
            v.extend(self.tiles.iter().copied());
            v.push(self.player_id);
            v
        }
    }

    /// A miss-loop `Spec0`: 10x10, one tile, 100 off-map iterations.
    fn miss_spec<'a>(next: &'a [f64], on: &'a [f64], tiles: &'a [f64]) -> Spec0<'a> {
        Spec0 {
            width: 10.0,
            height: 10.0,
            num_tiles: 1.0,
            border: &[12.0],
            next,
            on,
            refs: &[],
            own: &[],
            rand_ret: 0.0,
            n_owned: 0.0,
            tiles,
            player_id: 7.0,
        }
    }

    type UnitSpec<'a> = (&'a str, f64);
    type CandSpec<'a> = (f64, f64, f64, f64, &'a [UnitSpec<'a>]);

    fn a1(cands: &[CandSpec]) -> Vec<f64> {
        let mut v = vec![1.0, cands.len() as f64];
        for (id, troops, n_owned, max_troops, units) in cands {
            v.push(*id);
            v.push(*troops);
            v.push(*n_owned);
            v.push(*max_troops);
            v.push(units.len() as f64);
            for (t, l) in units.iter() {
                v.extend(enc_str(t));
                v.push(*l);
            }
        }
        v
    }

    /// The payload slice (everything after `[traceLen] + trace`).
    fn payload(res: &[f64]) -> &[f64] {
        &res[1 + res[0] as usize..]
    }

    #[test]
    fn array_hit_first_try() {
        let s = Spec0 {
            width: 10.0,
            height: 10.0,
            num_tiles: 1.0,
            border: &[12.0, 34.0],
            next: &[5.0, 6.0],
            on: &[1.0],
            refs: &[56.0],
            own: &[7.0],
            rand_ret: 0.0,
            n_owned: 0.0,
            tiles: &[],
            player_id: 7.0,
        };
        let res = run_op(0, &s.build());
        assert_eq!(payload(&res), &[1.0, 56.0]);
        // bb from (12 -> x=2,y=1) and (34 -> x=4,y=3): nextInt(2,4)=5.
        assert_eq!(&res[5..9], &[0.0, 2.0, 4.0, 5.0]);
        // isOnMap(5,6) -> 1, ref(5,6) -> 56, owner(56) -> 7.
        assert_eq!(&res[13..16], &[2.0, 5.0, 6.0]);
    }

    #[test]
    fn array_miss_then_fallback() {
        let next: Vec<f64> = (0..200).map(|_| 5.0).collect();
        let on: Vec<f64> = (0..100).map(|_| 0.0).collect();
        let mut s = miss_spec(&next, &on, &[10.0, 20.0, 30.0]);
        s.rand_ret = 20.0;
        s.n_owned = 3.0;
        let res = run_op(0, &s.build());
        assert_eq!(payload(&res), &[1.0, 20.0]);
        // border ev 3 + 100*(2 nextInt 8 + isOnMap 4) + 2*numTilesOwned 4 +
        // tiles ev 5 + randElement ev 6 = 1218 trace tokens.
        assert_eq!(res[0], 1218.0);
        // The fallback tail: tiles then randElement over the same array.
        let n = 1 + res[0] as usize;
        assert_eq!(
            &res[n - 11..n],
            &[7.0, 3.0, 10.0, 20.0, 30.0, 1.0, 3.0, 10.0, 20.0, 30.0, 20.0]
        );
    }

    #[test]
    fn array_owned0_short_circuits_second_call() {
        let next: Vec<f64> = (0..200).map(|_| 5.0).collect();
        let on: Vec<f64> = (0..100).map(|_| 0.0).collect();
        let s = miss_spec(&next, &on, &[]);
        let res = run_op(0, &s.build());
        assert_eq!(payload(&res), &[0.0]);
        // Exactly one numTilesOwned event, no tiles()/randElement.
        let ev6 = res[..1 + res[0] as usize].windows(2).filter(|w| w[0] == 6.0).count();
        assert_eq!(ev6, 1);
        let ev7 = res[..1 + res[0] as usize].iter().filter(|&&v| v == 7.0).count();
        assert_eq!(ev7, 0);
    }

    #[test]
    fn array_gt100_null_not_pushed() {
        let next: Vec<f64> = (0..200).map(|_| 5.0).collect();
        let on: Vec<f64> = (0..100).map(|_| 0.0).collect();
        let mut s = miss_spec(&next, &on, &[]);
        s.n_owned = 150.0;
        let res = run_op(0, &s.build());
        assert_eq!(payload(&res), &[0.0]);
        // Two numTilesOwned calls (150 > 0 true, 150 <= 100 false), no tiles.
        let ev6 = res[..1 + res[0] as usize].windows(2).filter(|w| w[0] == 6.0).count();
        assert_eq!(ev6, 2);
    }

    #[test]
    fn empty_border_bb_is_infinite_bounds() {
        let s = Spec0 {
            width: 10.0,
            height: 10.0,
            num_tiles: 1.0,
            border: &[],
            next: &[5.0, 6.0],
            on: &[1.0],
            refs: &[56.0],
            own: &[7.0],
            rand_ret: 0.0,
            n_owned: 0.0,
            tiles: &[],
            player_id: 7.0,
        };
        let res = run_op(0, &s.build());
        // nextInt(Infinity, -Infinity) then nextInt(Infinity, -Infinity).
        // trace: ev5 [5,0] at res[1..3], ev0 x [0,Inf,-Inf,5] at res[3..7].
        assert!(res[4].is_infinite() && res[4] > 0.0);
        assert!(res[5].is_infinite() && res[5] < 0.0);
        assert_eq!(payload(&res), &[1.0, 56.0]);
    }

    #[test]
    fn juice_empty_returns_absent() {
        let res = run_op(1, &a1(&[]));
        assert_eq!(res, vec![0.0, 0.0, 0.0]); // traceLen 0, payload [nc, present]
    }

    #[test]
    fn juice_single_zero_normalize() {
        let res = run_op(
            1,
            &a1(&[(1.0, 5.0, 10.0, 20.0, &[("City", 3.0), ("Defense Post", 2.0)])]),
        );
        // structureCount 3 (Defense Post excluded), ratio 0.75, tiles 10 —
        // single-value columns all normalize to 0 -> juiciness 0 beats -Inf.
        assert_eq!(payload(&res), &[1.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn juice_tie_keeps_first() {
        let res = run_op(
            1,
            &a1(&[
                (1.0, 5.0, 10.0, 20.0, &[("City", 3.0)]),
                (2.0, 5.0, 10.0, 20.0, &[("City", 3.0)]),
            ]),
        );
        assert_eq!(payload(&res), &[2.0, 0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn juice_maxtroops0_skips_troops_call() {
        let res = run_op(
            1,
            &a1(&[
                (1.0, 9.0, 5.0, 0.0, &[("City", 1.0)]),
                (2.0, 1.0, 5.0, 4.0, &[("City", 1.0)]),
            ]),
        );
        // Candidate 1 never reaches troops(): exactly one ev 11 in the trace.
        let ev11 = res[..1 + res[0] as usize].iter().filter(|&&v| v == 11.0).count();
        assert_eq!(ev11, 1);
        assert_eq!(payload(&res)[3..], [1.0, 2.0]); // candidate 2 wins
    }

    #[test]
    fn juice_nan_troops_normalizes_to_zero() {
        let res = run_op(
            1,
            &a1(&[
                (1.0, f64::NAN, 5.0, 10.0, &[]),
                (2.0, 0.0, 5.0, 10.0, &[]),
            ]),
        );
        assert_eq!(payload(&res), &[2.0, 0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn juice_neg_ratio_spread() {
        let res = run_op(
            1,
            &a1(&[
                (1.0, 20.0, 3.0, 10.0, &[("City", 0.0)]),
                (2.0, 0.0, 7.0, 10.0, &[]),
            ]),
        );
        // gaps [-1, 1]: c1 0, c2 1; tiles [3,7]: c1 0, c2 1 -> c2 wins 2.
        assert_eq!(payload(&res), &[2.0, 0.0, 2.0, 1.0, 2.0]);
    }

    #[test]
    fn structure_membership() {
        assert!(counted_structure(&"City".encode_utf16().collect::<Vec<_>>()));
        assert!(!counted_structure(&"Defense Post".encode_utf16().collect::<Vec<_>>()));
        assert!(!counted_structure(&"Missile Silo".encode_utf16().collect::<Vec<_>>()));
        assert!(!counted_structure(&"Warship".encode_utf16().collect::<Vec<_>>()));
        assert!(counted_structure(&"SAM Launcher".encode_utf16().collect::<Vec<_>>()));
        assert!(counted_structure(&"Port".encode_utf16().collect::<Vec<_>>()));
        assert!(counted_structure(&"Factory".encode_utf16().collect::<Vec<_>>()));
    }

    #[test]
    fn reduce_zero_plus_negzero() {
        let res = run_op(
            1,
            &a1(&[(1.0, 0.0, 2.0, 5.0, &[("City", -0.0)]), (2.0, 0.0, 2.0, 5.0, &[("Port", 0.0)])]),
        );
        // 0 + -0 folds to +0; both candidates tie at 0 -> first wins.
        assert_eq!(payload(&res), &[2.0, 0.0, 0.0, 1.0, 1.0]);
    }
}
