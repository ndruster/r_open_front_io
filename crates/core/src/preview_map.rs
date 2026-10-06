//! Port of `src/client/render/preview/PreviewMap.ts` — the preview-map data
//! wrapper (fixed 1000x750 Australia dimensions, land bit 7), `previewTileRef`
//! and the module-singleton `getPreviewRailLoop` closed rail rectangle built
//! through `computeRailTiles` (reused from [`crate::railroad_cache`]).
//!
//! Faithfulness notes (quirk list):
//!
//! * `buildPreviewMap(terrainBytes, mapW = PREVIEW_MAP_W, mapH = PREVIEW_MAP_H)`
//!   — the defaults fire ONLY on `undefined`, so an explicit `null` / `0` /
//!   `NaN` size flows through the `!==` length check unchanged (the capture
//!   scripts both). A length mismatch throws with the exact template message
//!   (modelled as `Err(String)`; the harness records status 1 + the message).
//! * The land test is `terrainBytes[i] & 0x80` — a JS bitwise AND (ToInt32 on
//!   a `Uint8Array` read, so the element is always 0..255 and only bit 7
//!   matters). The loop runs over `tileState.length` (= `mapW * mapH`), NOT
//!   the terrain length: on the throw-free path they agree, and the capture
//!   only reaches the loop when they do.
//! * `tileState[i] = 1` writes through a `Uint16Array` (`to_uint16`), the
//!   terrain elements arrive through a `Uint8Array` (`to_uint8`) — both
//!   coercions run on the Rust side too so out-of-domain scripted tokens
//!   agree with V8.
//! * `previewTileRef(x, y) = y * PREVIEW_MAP_W + x` — plain f64 arithmetic, no
//!   integer coercion (a fractional `y` yields a fractional ref; the map only
//!   ever feeds integers, but the wire keeps f64).
//! * `getPreviewRailLoop` is the module-level `let railLoop` singleton: the
//!   first call builds, every later call returns the SAME object (truthiness
//!   of a defined object, so no nullish gate). The Rust latch lives in a
//!   `thread_local!` `RefCell<Option<..>>` and the harness `reset()` clears
//!   it, so a scenario can pin the built/cached distinction.
//! * The loop path is the four-edge walk (eastbound `x < right`, southbound
//!   `y < bottom`, westbound `x > left`, northbound `y > top`) — corners are
//!   visited once each, `n = 180`. Orientation needs neighbours on both sides,
//!   hence `computeRailTiles([path[n-1], ...path, path[0]], w)` and the
//!   `.slice(1, n + 1)` trim back to the `n` real tiles.
//! * `railroadState` is a fresh `Uint8Array(w * PREVIEW_MAP_H)` (750 000
//!   bytes) and only the loop tiles are written (`rt.type + 1`, so 1..6); the
//!   capture dumps it SPARSELY (length + `(index, value)` pairs in ascending
//!   index order) — precedent: the `atlas_data` kern table dump.

use crate::jsnum::{to_uint16, to_uint8};
use crate::railroad_cache::{compute_rail_tiles, RailType};
use std::cell::RefCell;

/// `PREVIEW_MAP_W`.
pub const PREVIEW_MAP_W: f64 = 1000.0;
/// `PREVIEW_MAP_H`.
pub const PREVIEW_MAP_H: f64 = 750.0;

/// `PREVIEW_SCENE.land` — Central Australia (skin anchor, nuke target, rail
/// loop).
pub const PREVIEW_SCENE_LAND: (f64, f64) = (520.0, 380.0);
/// `PREVIEW_SCENE.ocean` — Great Australian Bight patrol.
pub const PREVIEW_SCENE_OCEAN: (f64, f64) = (400.0, 600.0);
/// `PREVIEW_SCENE.coast` — the Bight's north shore (buildings).
pub const PREVIEW_SCENE_COAST: (f64, f64) = (400.0, 480.0);

/// `PREVIEW_RAIL_STATIONS.city` (`land.x - 30, land.y`).
pub const PREVIEW_RAIL_CITY: (f64, f64) = (490.0, 380.0);
/// `PREVIEW_RAIL_STATIONS.factory` (`land.x + 30, land.y`).
pub const PREVIEW_RAIL_FACTORY: (f64, f64) = (550.0, 380.0);

/// `PreviewMapData`.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewMapData {
    pub map_w: f64,
    pub map_h: f64,
    pub terrain_bytes: Vec<u8>,
    pub tile_state: Vec<u16>,
}

/// `buildPreviewMap(terrainBytes, mapW = PREVIEW_MAP_W, mapH = PREVIEW_MAP_H)`.
///
/// `Err(String)` models the thrown `Error` (the message is part of the parity
/// contract). The raw terrain tokens are `Uint8Array` writes, hence
/// [`to_uint8`].
pub fn build_preview_map(
    terrain: &[f64],
    map_w: Option<f64>,
    map_h: Option<f64>,
) -> Result<PreviewMapData, String> {
    let map_w = map_w.unwrap_or(PREVIEW_MAP_W);
    let map_h = map_h.unwrap_or(PREVIEW_MAP_H);
    let expected = map_w * map_h;
    if (terrain.len() as f64) != expected {
        return Err(format!(
            "Preview map: expected {}x{} terrain bytes, got {}",
            js_num(map_w),
            js_num(map_h),
            terrain.len()
        ));
    }
    let terrain_bytes: Vec<u8> = terrain.iter().map(|v| to_uint8(*v)).collect();
    let len = to_index(expected);
    let mut tile_state = vec![0u16; len];
    // `len == terrain_bytes.len()` after the exact-length gate above; the
    // typed-array read is 0..255 already (`terrainBytes[i] & 0x80`).
    for (i, b) in terrain_bytes.iter().enumerate() {
        if (*b as i32) & 0x80 != 0 {
            tile_state[i] = to_uint16(1.0);
        }
    }
    Ok(PreviewMapData { map_w, map_h, terrain_bytes, tile_state })
}

/// `mapW * mapH` -> a Rust length: JS `new Uint16Array(n)` truncates a
/// fractional / negative / NaN length to `0` (CanonicalNumericIndex +
/// `ToIndex`-style clamp the typed-array constructor applies). The capture
/// only feeds integral non-negative sizes; `NaN` / negatives land on `0`.
fn to_index(n: f64) -> usize {
    if !n.is_finite() || n <= 0.0 {
        0
    } else {
        n as usize
    }
}

/// `String(number)` for the error template (JS `Number::toString`).
fn js_num(v: f64) -> String {
    crate::game_ts::js_num_str(v)
}

/// `previewTileRef(x, y)`.
pub fn preview_tile_ref(x: f64, y: f64) -> f64 {
    y * PREVIEW_MAP_W + x
}

/// `PreviewRailLoop`.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewRailLoop {
    /// Ordered, closed path of tile refs the train follows.
    pub path: Vec<f64>,
    /// Per-tile rail orientation (0 = none, `RailType + 1`).
    pub railroad_state: Vec<u8>,
}

/// Build the loop (the `getPreviewRailLoop` body, latch aside).
fn build_rail_loop() -> PreviewRailLoop {
    let w = PREVIEW_MAP_W;
    let left = PREVIEW_RAIL_CITY.0;
    let right = PREVIEW_RAIL_FACTORY.0;
    let top = PREVIEW_RAIL_CITY.1 - 15.0;
    let bottom = PREVIEW_RAIL_CITY.1 + 15.0;
    let mut path: Vec<f64> = Vec::new();
    let mut x = left;
    while x < right {
        path.push(top * w + x); // eastbound
        x += 1.0;
    }
    let mut y = top;
    while y < bottom {
        path.push(y * w + right); // southbound
        y += 1.0;
    }
    let mut x = right;
    while x > left {
        path.push(bottom * w + x); // westbound
        x -= 1.0;
    }
    let mut y = bottom;
    while y > top {
        path.push(y * w + left); // northbound
        y -= 1.0;
    }
    // Wrap the loop so the corners resolve instead of being treated as line
    // ends: `[path[n - 1], ...path, path[0]]`, then trim the two extras.
    let n = path.len();
    let mut tiles_in: Vec<f64> = Vec::with_capacity(n + 2);
    tiles_in.push(path[n - 1]);
    tiles_in.extend_from_slice(&path);
    tiles_in.push(path[0]);
    let computed = compute_rail_tiles(&tiles_in, w);
    let tiles = &computed[1..(n + 1).min(computed.len())];
    let mut railroad_state = vec![0u8; to_index(w * PREVIEW_MAP_H)];
    for rt in tiles {
        let v = rt.rtype.value() + 1.0;
        let idx = to_index(rt.ref_tile);
        if idx < railroad_state.len() {
            railroad_state[idx] = to_uint8(v);
        }
    }
    PreviewRailLoop { path, railroad_state }
}

thread_local! {
    /// The module-level `let railLoop` singleton.
    static RAIL_LOOP: RefCell<Option<PreviewRailLoop>> = const { RefCell::new(None) };
}

/// `getPreviewRailLoop()` — build once, share thereafter. `built` reports
/// whether THIS call constructed the loop (the harness dumps it so the cached
/// path is pinned).
pub fn get_preview_rail_loop() -> (PreviewRailLoop, bool) {
    RAIL_LOOP.with(|l| {
        let mut l = l.borrow_mut();
        if let Some(loop_) = l.as_ref() {
            return (loop_.clone(), false);
        }
        let fresh = build_rail_loop();
        *l = Some(fresh.clone());
        (fresh, true)
    })
}

/// Clear the module singleton (the harness `reset`).
pub fn reset_preview_rail_loop() {
    RAIL_LOOP.with(|l| *l.borrow_mut() = None);
}

/// `RailType` — the six regular-enum member NAMES (re-exported for the wire
/// dump; the TS enum spells them UPPER_SNAKE).
pub const RAIL_TYPE_NAMES: [&str; 6] = [
    "VERTICAL",
    "HORIZONTAL",
    "TOP_LEFT",
    "TOP_RIGHT",
    "BOTTOM_LEFT",
    "BOTTOM_RIGHT",
];

#[allow(dead_code)]
fn rail_type_name(t: RailType) -> &'static str {
    RAIL_TYPE_NAMES[t.value() as usize]
}

// ---------------------------------------------------------------- vectors op
//
// kind table:
//   0 [n, (terrain f64)*n, mapW tag 0|1[?], mapH tag 0|1[?]]
//        -> throw-free: [0, mapW, mapH, terrainLen, (byte)*, tileLen, (state)*]
//           throw:     [1, msgLen, (code)*]
//   1 [x, y] -> [ref]
//   2 [] -> [cached 0|1, pathLen, (ref)*, stateLen, pairCount, (idx, val)*]
//        (getPreviewRailLoop over the module latch; kind 3 clears the latch)
//   3 [] -> []
//   4 [] -> [1000, 750, 520, 380, 400, 600, 400, 480, 490, 380, 550, 380, 6,
//            (name)*6]  constant table dump

pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    match kind {
        0 => {
            let n = args[0] as usize;
            let terrain = &args[1..1 + n];
            let mut i = 1 + n;
            let take = |i: &mut usize| -> Option<f64> {
                if args[*i] == 0.0 {
                    *i += 1;
                    None
                } else {
                    *i += 1;
                    let v = args[*i];
                    *i += 1;
                    Some(v)
                }
            };
            let map_w = take(&mut i);
            let map_h = take(&mut i);
            match build_preview_map(terrain, map_w, map_h) {
                Ok(d) => {
                    out.push(0.0);
                    out.push(d.map_w);
                    out.push(d.map_h);
                    out.push(d.terrain_bytes.len() as f64);
                    for b in &d.terrain_bytes {
                        out.push(*b as f64);
                    }
                    out.push(d.tile_state.len() as f64);
                    for s in &d.tile_state {
                        out.push(*s as f64);
                    }
                }
                Err(msg) => {
                    out.push(1.0);
                    crate::js_json::push_str(&mut out, &msg);
                }
            }
        }
        1 => {
            out.push(preview_tile_ref(args[0], args[1]));
        }
        2 => {
            let (loop_, built) = get_preview_rail_loop();
            out.push(if built { 0.0 } else { 1.0 });
            out.push(loop_.path.len() as f64);
            for p in &loop_.path {
                out.push(*p);
            }
            out.push(loop_.railroad_state.len() as f64);
            let pairs: Vec<(usize, u8)> = loop_
                .railroad_state
                .iter()
                .enumerate()
                .filter(|(_, v)| **v != 0)
                .map(|(i, v)| (i, *v))
                .collect();
            out.push(pairs.len() as f64);
            for (i, v) in pairs {
                out.push(i as f64);
                out.push(v as f64);
            }
        }
        3 => {
            reset_preview_rail_loop();
        }
        4 => {
            out.push(PREVIEW_MAP_W);
            out.push(PREVIEW_MAP_H);
            out.push(PREVIEW_SCENE_LAND.0);
            out.push(PREVIEW_SCENE_LAND.1);
            out.push(PREVIEW_SCENE_OCEAN.0);
            out.push(PREVIEW_SCENE_OCEAN.1);
            out.push(PREVIEW_SCENE_COAST.0);
            out.push(PREVIEW_SCENE_COAST.1);
            out.push(PREVIEW_RAIL_CITY.0);
            out.push(PREVIEW_RAIL_CITY.1);
            out.push(PREVIEW_RAIL_FACTORY.0);
            out.push(PREVIEW_RAIL_FACTORY.1);
            out.push(RAIL_TYPE_NAMES.len() as f64);
            for name in RAIL_TYPE_NAMES {
                crate::js_json::push_str(&mut out, name);
            }
        }
        k => unreachable!("preview_map: unknown op kind {k}"),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_and_stations() {
        assert_eq!(PREVIEW_RAIL_CITY, (PREVIEW_SCENE_LAND.0 - 30.0, PREVIEW_SCENE_LAND.1));
        assert_eq!(PREVIEW_RAIL_FACTORY, (PREVIEW_SCENE_LAND.0 + 30.0, PREVIEW_SCENE_LAND.1));
        assert_eq!(preview_tile_ref(520.0, 380.0), 380.0 * 1000.0 + 520.0);
        assert_eq!(preview_tile_ref(0.5, 1.5), 1500.5);
    }

    #[test]
    fn build_small_map() {
        let terrain: Vec<f64> = vec![128.0, 0.0, 64.0, 129.0, 32.0, 255.0];
        let d = build_preview_map(&terrain, Some(3.0), Some(2.0)).unwrap();
        assert_eq!(d.map_w, 3.0);
        assert_eq!(d.map_h, 2.0);
        assert_eq!(d.tile_state, vec![1, 0, 0, 1, 0, 1]);
        assert_eq!(d.terrain_bytes, vec![0x80, 0x00, 0x40, 0x81, 0x20, 0xff]);
        // Default params: 1000x750 terrain -> ok; bit 6 / bit 5 are NOT land.
        let terrain: Vec<f64> = vec![128.0; 750_000];
        let d = build_preview_map(&terrain, None, None).unwrap();
        assert_eq!(d.map_w, 1000.0);
        assert_eq!(d.tile_state.iter().filter(|s| **s == 1).count(), 750_000);
        let mut mixed = vec![64.0; 6];
        mixed[0] = 31.0; // mag only, not land
        mixed[1] = 127.0; // shore+ocean+mag, bit 7 clear
        let d = build_preview_map(&mixed, Some(3.0), Some(2.0)).unwrap();
        assert_eq!(d.tile_state, vec![0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn terrain_tokens_wrap_through_uint8() {
        // Out-of-domain tokens (300, -1, NaN) land as Uint8Array writes:
        // 300 -> 44, -1 -> 255, NaN -> 0.
        let d = build_preview_map(&[300.0, -1.0, f64::NAN], Some(3.0), Some(1.0)).unwrap();
        assert_eq!(d.terrain_bytes, vec![44, 255, 0]);
        assert_eq!(d.tile_state, vec![0, 1, 0]); // 255 & 0x80 -> land
    }

    #[test]
    fn build_throw_message_exact() {
        let terrain: Vec<f64> = vec![0.0; 5];
        let err = build_preview_map(&terrain, None, None).unwrap_err();
        assert_eq!(err, "Preview map: expected 1000x750 terrain bytes, got 5");
        let err = build_preview_map(&terrain, Some(2.0), Some(2.0)).unwrap_err();
        assert_eq!(err, "Preview map: expected 2x2 terrain bytes, got 5");
        // An explicit null size is NOT the default (the capture sends the
        // value through): 1000*750 !== 5 -> throw with the scripted size.
        let err = build_preview_map(&terrain, Some(f64::NAN), Some(2.0)).unwrap_err();
        assert_eq!(err, "Preview map: expected NaNx2 terrain bytes, got 5");
    }

    #[test]
    fn rail_loop_shape_and_latch() {
        reset_preview_rail_loop();
        let (l1, built1) = get_preview_rail_loop();
        assert!(built1);
        let (l2, built2) = get_preview_rail_loop();
        assert!(!built2);
        assert_eq!(l1, l2);
        assert_eq!(l1.path.len(), 180);
        assert_eq!(l1.railroad_state.len(), 750_000);
        // Eastbound first tile, then the corner turns.
        assert_eq!(l1.path[0], 365.0 * 1000.0 + 490.0);
        assert_eq!(l1.path[60], 365.0 * 1000.0 + 550.0);
        assert_eq!(l1.path[90], 395.0 * 1000.0 + 550.0);
        assert_eq!(l1.path[150], 395.0 * 1000.0 + 490.0);
        // Every loop tile carries an orientation (RailType + 1 in 1..6).
        let nonzero: Vec<usize> = l1
            .railroad_state
            .iter()
            .enumerate()
            .filter(|(_, v)| **v != 0)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(nonzero.len(), 180);
        for i in &nonzero {
            assert!(l1.railroad_state[*i] >= 1 && l1.railroad_state[*i] <= 6);
        }
        // The path refs all carry a state.
        for p in &l1.path {
            assert_ne!(l1.railroad_state[*p as usize], 0);
        }
        reset_preview_rail_loop();
        let (l3, built3) = get_preview_rail_loop();
        assert!(built3);
        assert_eq!(l3, l1);
    }

    #[test]
    fn run_op_wire_roundtrip() {
        let mut args = vec![6.0];
        args.extend_from_slice(&[128.0, 0.0, 64.0, 129.0, 32.0, 255.0]);
        args.extend_from_slice(&[1.0, 3.0, 1.0, 2.0]);
        let res = run_op(0, &args);
        assert_eq!(res[0], 0.0);
        assert_eq!(res[1], 3.0);
        assert_eq!(res.len(), 2 + 2 + 6 + 1 + 6);
        // Throw path: 2 tokens but 3x2 expected.
        let args = vec![2.0, 0.0, 0.0, 1.0, 3.0, 1.0, 2.0];
        let res = run_op(0, &args);
        assert_eq!(res[0], 1.0);
        assert_eq!(run_op(1, &[7.0, 9.0]), vec![9007.0]);
        let table = run_op(4, &[]);
        assert_eq!(table[0], 1000.0);
        assert_eq!(table[12], 6.0);
    }
}
