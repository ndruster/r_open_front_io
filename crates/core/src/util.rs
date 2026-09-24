//! Port of the deterministic core of `src/core/Util.ts`.
//!
//! Only the pure, simulation-relevant helpers are ported; the TS file's UI /
//! record / sanitising surface (DOMPurify, nanoid, zod-schema plumbing,
//! emoji tables, name sanitizers) is deliberately out of scope.
//!
//! Faithfulness notes:
//!
//! * [`simple_hash`] iterates **UTF-16 code units** (JS `charCodeAt`), and the
//!   `<<` / `&` steps are ToInt32 — the intermediate `(hash << 5) - hash + c`
//!   is an exact integer that may leave the i32 range before the final
//!   `hash & hash` wraps it back.
//! * [`find_minimum_by`] keeps the *first* minimum (strict `<` against
//!   `bestScore`, which starts at `+Infinity`), so a `NaN` score never wins
//!   and ties never displace an earlier candidate. The indexed loop mirrors
//!   the TS `for (let i = 0, len = values.length; ...)` verbatim.
//! * [`get_mode`] takes the pairs in **JS `Map` insertion order** (a slice of
//!   `(key, count)` pairs is the port's carrier) and replaces the incumbent
//!   only on a strictly greater count, so ties keep the earliest key.
//! * [`to_int`] mirrors `BigInt(Math.floor(num))`: `±Infinity` clamp to
//!   `±MAX_SAFE_INTEGER`; `NaN` throws in JS and is reported as `None` here.
//!   Callers must stay inside the exact-integer range (`|num| < 2^63`); the
//!   parity vectors keep `|num| <= 2^53`.
//! * [`manhattan_dist_wrapped`] / [`within`] use the JS `Math.min`/`Math.max`
//!   NaN-propagating variants (shared with `game_map`).
//! * [`sigmoid`] uses [`crate::detmath::exp`], never a platform
//!   transcendental, and keeps the TS association order
//!   `(-decayRate) * (value - midpoint)`.

use crate::detmath;
use crate::game_map::{js_max, js_min};

/// `Cell` from `src/core/game/Game.ts` — the plain `{x, y}` pair the util
/// functions construct and read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    pub x: f64,
    pub y: f64,
}

/// `{ min: Cell; max: Cell }` as returned by `calculateBoundingBox`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoundingBox {
    pub min: Cell,
    pub max: Cell,
}

/// `manhattanDistWrapped(c1, c2, width)`.
pub fn manhattan_dist_wrapped(c1: &Cell, c2: &Cell, width: f64) -> f64 {
    let mut dx = (c1.x - c2.x).abs();
    dx = js_min(dx, width - dx);
    let dy = (c1.y - c2.y).abs();
    dx + dy
}

/// `within(value, min, max)` = `Math.min(Math.max(value, min), max)`.
pub fn within(value: f64, min: f64, max: f64) -> f64 {
    js_min(js_max(value, min), max)
}

/// `simpleHash(str)`: djb2-style rolling hash over UTF-16 code units with
/// ToInt32 wrapping, returned as `Math.abs` of the final i32.
pub fn simple_hash(s: &str) -> f64 {
    simple_hash_units(&s.encode_utf16().collect::<Vec<u16>>())
}

/// The code-unit core of [`simple_hash`], exposed so the wasm probe can feed
/// the exact UTF-16 units the JS `charCodeAt` loop sees.
pub fn simple_hash_units(units: &[u16]) -> f64 {
    let mut hash: i32 = 0;
    for &unit in units {
        // `hash << 5` is a 32-bit truncating shift; the subtraction and
        // addition are exact integers; `hash & hash` wraps back to i32.
        let shifted = hash.wrapping_shl(5) as i64;
        let t = shifted - hash as i64 + unit as i64;
        hash = t as i32;
    }
    f64::from(hash).abs()
}

/// Score kinds mirroring the closures the parity capture feeds to
/// [`find_minimum_by`] (0 = identity, 1 = abs, 2 = JS `% 3`, 3 = NaN sentinel,
/// 4 = negate).
pub mod score {
    pub const IDENTITY: u8 = 0;
    pub const ABS: u8 = 1;
    pub const MOD3: u8 = 2;
    pub const NAN_SENTINEL: u8 = 3;
    pub const NEGATE: u8 = 4;

    /// The score closure for kind `k`. `NAN_SENTINEL` returns `NaN` for the
    /// value `-999` so the never-wins path is exercised.
    pub fn apply(k: u8, v: f64) -> f64 {
        match k {
            IDENTITY => v,
            ABS => v.abs(),
            MOD3 => v % 3.0, // JS `%`: sign follows the dividend
            NAN_SENTINEL => {
                if v == -999.0 {
                    f64::NAN
                } else {
                    v
                }
            }
            _ => -v,
        }
    }
}

/// Candidate-filter kinds (0 = none, 1 = `v >= 0`, 2 = reject the sentinel
/// `-999`, 3 = reject everything).
pub mod candidate {
    pub const NONE: u8 = 0;
    pub const NON_NEGATIVE: u8 = 1;
    pub const NOT_SENTINEL: u8 = 2;
    pub const NONE_ACCEPT: u8 = 3;

    pub fn apply(k: u8, v: f64) -> bool {
        match k {
            NONE => true,
            NON_NEGATIVE => v >= 0.0,
            NOT_SENTINEL => v != -999.0,
            _ => false,
        }
    }
}

/// `findMinimumBy(values, score, isCandidate?)` over `f64` values, with the
/// score/candidate closures selected by the kinds above (the parity capture
/// uses exactly these closures on both sides). Returns the winning **value**
/// (`None` = TS `null`).
#[allow(clippy::needless_range_loop)] // indexed loop: verbatim TS transcription
pub fn find_minimum_by(values: &[f64], score_kind: u8, cand_kind: u8) -> Option<f64> {
    let mut best: Option<f64> = None;
    let mut best_score = f64::INFINITY;

    if cand_kind == candidate::NONE {
        for i in 0..values.len() {
            let value = values[i];
            let current_score = score::apply(score_kind, value);
            if current_score < best_score {
                best_score = current_score;
                best = Some(value);
            }
        }
        return best;
    }

    for i in 0..values.len() {
        let value = values[i];
        if !candidate::apply(cand_kind, value) {
            continue;
        }
        let current_score = score::apply(score_kind, value);
        if current_score < best_score {
            best_score = current_score;
            best = Some(value);
        }
    }

    best
}

/// `findClosestBy` is a direct alias of `findMinimumBy` in the TS.
pub fn find_closest_by(values: &[f64], score_kind: u8, cand_kind: u8) -> Option<f64> {
    find_minimum_by(values, score_kind, cand_kind)
}

/// `getMode(counts)` over a `Map`-ordered slice of `(key, count)` pairs.
pub fn get_mode(counts: &[(f64, f64)]) -> Option<f64> {
    let mut mode: Option<f64> = None;
    let mut max_count = 0.0;

    for &(item, count) in counts {
        if count > max_count {
            max_count = count;
            mode = Some(item);
        }
    }

    mode
}

/// `toInt(num)`: `±Infinity` clamp to `±MAX_SAFE_INTEGER`; `NaN` throws in JS
/// and maps to `None` here. Inputs must satisfy `|num| < 2^63` for the
/// `i64` result to be exact (the parity vectors stay within `2^53`).
pub fn to_int(num: f64) -> Option<i64> {
    if num == f64::INFINITY {
        return Some(9_007_199_254_740_991); // Number.MAX_SAFE_INTEGER
    }
    if num == f64::NEG_INFINITY {
        return Some(-9_007_199_254_740_991); // Number.MIN_SAFE_INTEGER
    }
    if num.is_nan() {
        return None; // BigInt(NaN) throws
    }
    Some(num.floor() as i64)
}

/// `maxInt(a, b)`.
pub fn max_int(a: i64, b: i64) -> i64 {
    if a > b {
        a
    } else {
        b
    }
}

/// `minInt(a, b)`.
pub fn min_int(a: i64, b: i64) -> i64 {
    if a < b {
        a
    } else {
        b
    }
}

/// `withinInt(num, min, max)`.
pub fn within_int(num: i64, min: i64, max: i64) -> i64 {
    let at_least_min = max_int(num, min);
    min_int(at_least_min, max)
}

/// `sigmoid(value, decayRate, midpoint)` = `1 / (1 + exp(-decayRate *
/// (value - midpoint)))`, with the deterministic exp.
pub fn sigmoid(value: f64, decay_rate: f64, midpoint: f64) -> f64 {
    1.0 / (1.0 + detmath::exp(-decay_rate * (value - midpoint)))
}

/// `boundingBoxCenter(box)`: `min.x + floor((max.x - min.x) / 2)` (and same
/// for `y`).
pub fn bounding_box_center(bb: &BoundingBox) -> Cell {
    Cell {
        x: bb.min.x + ((bb.max.x - bb.min.x) / 2.0).floor(),
        y: bb.min.y + ((bb.max.y - bb.min.y) / 2.0).floor(),
    }
}

/// `inscribed(outer, inner)`: the four `<=` / `>=` comparisons.
pub fn inscribed(outer: &BoundingBox, inner: &BoundingBox) -> bool {
    outer.min.x <= inner.min.x
        && outer.min.y <= inner.min.y
        && outer.max.x >= inner.max.x
        && outer.max.y >= inner.max.y
}

/// `calculateBoundingBox(gm, borderTiles)`. The TS branches on
/// `Array` / `Set` / `TileSet` purely for allocation reasons — all three
/// visit the same refs in the same order — so the port takes any iterator of
/// tile refs.
pub fn calculate_bounding_box(
    gm: &crate::game_map::GameMap,
    border_tiles: impl IntoIterator<Item = f64>,
) -> BoundingBox {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for tile in border_tiles {
        let x = gm.x(tile);
        let y = gm.y(tile);
        min_x = js_min(min_x, x);
        min_y = js_min(min_y, y);
        max_x = js_max(max_x, x);
        max_y = js_max(max_y, y);
    }

    BoundingBox {
        min: Cell { x: min_x, y: min_y },
        max: Cell { x: max_x, y: max_y },
    }
}

/// `calculateBoundingBoxCenter(gm, borderTiles)`.
pub fn calculate_bounding_box_center(
    gm: &crate::game_map::GameMap,
    border_tiles: impl IntoIterator<Item = f64>,
) -> Cell {
    let bb = calculate_bounding_box(gm, border_tiles);
    bounding_box_center(&bb)
}

/// `boundingBoxTiles(gm, center, radius)`: the perimeter of the axis-aligned
/// square around `center`, clipped to valid coordinates, top/bottom edges
/// full-width and left/right edges excluding the corners.
pub fn bounding_box_tiles(
    gm: &crate::game_map::GameMap,
    center: f64,
    radius: f64,
) -> Vec<f64> {
    let mut tiles: Vec<f64> = Vec::new();

    let center_x = gm.x(center);
    let center_y = gm.y(center);

    let min_x = center_x - radius;
    let max_x = center_x + radius;
    let min_y = center_y - radius;
    let max_y = center_y + radius;

    // Top and bottom edges (full width)
    let mut x = min_x;
    while x <= max_x {
        if gm.is_valid_coord(x, min_y) {
            tiles.push(gm.tile_ref(x, min_y));
        }
        if gm.is_valid_coord(x, max_y) && min_y != max_y {
            tiles.push(gm.tile_ref(x, max_y));
        }
        x += 1.0;
    }

    // Left and right edges (exclude corners already added)
    let mut y = min_y + 1.0;
    while y < max_y {
        if gm.is_valid_coord(min_x, y) {
            tiles.push(gm.tile_ref(min_x, y));
        }
        if gm.is_valid_coord(max_x, y) && min_x != max_x {
            tiles.push(gm.tile_ref(max_x, y));
        }
        y += 1.0;
    }

    tiles
}
