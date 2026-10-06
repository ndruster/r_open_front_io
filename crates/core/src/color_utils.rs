//! Port of `src/client/render/gl/utils/ColorUtils.ts`: the terrain RGBA
//! encoder (the single source of truth for terrain colors) plus `hexToRgb`
//! and the palette block constants.
//!
//! Faithfulness notes (quirk list):
//!
//! * `hexToRgb` trims with the JS whitespace set (U+0085 is NOT trimmed,
//!   U+FEFF IS) before the anchored `/^#?([0-9a-fA-F]{6})$/` test, so
//!   `"##aabbcc"` fails (only one optional `#`) while `" #aabbcc "` matches.
//! * `encodeTerrainTile` coerces `tb` through the bitwise ops: `NaN` /
//!   out-of-range terrain bytes (JS `undefined`) become `ToInt32` `0` and
//!   take the deep-water branch at magnitude 0 — the base ocean colour, not
//!   black. `-1` becomes all-ones: `& 0x80` and `& 0x40` both set, magnitude
//!   31 → the peak gate wins before the shoreline gate.
//! * The plains branch has NO clamp: `g = base[1] - 2 * magnitude` can go
//!   negative and the `Uint8Array` write wraps mod 256 (override
//!   `plainsColor: [0, 0, 0]` at magnitude 9 stores 238).
//! * Highland adds `2 * (magnitude - 10)` per channel through `Math.min`
//!   (NaN-propagating); mountain adds `Math.floor(magnitude / 2)` the same
//!   way. Shoreline water blends `Math.round(0.7 * base + 76.5)` per channel;
//!   deep water subtracts `Math.min(magnitude, 10)` through `Math.max(0, …)`.
//! * Overrides ride `??` — an explicit `null`/`undefined`/absent field falls
//!   back; an EMPTY array override does NOT (it indexes to `undefined` → NaN
//!   → the `Uint8Array` write stores 0).
//! * `buildTerrainRGBA` allocates `new Uint8Array(w * h * 4)` (ToIndex:
//!   fractional truncates, NaN → 0) but loops `i < w * h` (fractional rounds
//!   the iteration count UP), so a fractional `w` leaves the last pixel's
//!   tail bytes (and its alpha!) unwritten at 0; writes past the allocation
//!   are dropped.
//! * `DEEP_WATER_BASE` / `BACKGROUND_BASE` are module-private in TS and only
//!   observable through behaviour; the Rust constants below are the
//!   `hexToRgb` results of `render-settings.json`'s `terrain.oceanColor` /
//!   `terrain.backgroundColor` — if the JSON changes, the capture changes and
//!   these literals must be re-derived.

use crate::js_json::{push_val, read_val, JsVal};
use crate::jsnum::{js_max, js_min, js_round, to_int32, to_uint8};

/// JS `String.prototype.trim`: the WhiteSpace ∪ LineTerminator set (U+0085
/// is NOT in it, U+FEFF IS — unlike Rust's `str::trim`).
fn js_trim(s: &str) -> &str {
    s.trim_matches(|c: char| {
        matches!(
            c,
            '\t' | '\n'
                | '\u{b}'
                | '\u{c}'
                | '\r'
                | ' '
                | '\u{a0}'
                | '\u{1680}'
                | '\u{2000}'..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
        )
    })
}

/// `hexToRgb`: optional single leading `#`, then exactly six hex digits.
/// Returns the three channels as JS numbers.
pub fn hex_to_rgb(hex: &str) -> Option<[f64; 3]> {
    let t = js_trim(hex);
    let body = t.strip_prefix('#').unwrap_or(t);
    if body.len() != 6 {
        return None; // hex digits are ASCII: a byte-count gate is exact
    }
    let mut n: u32 = 0;
    for c in body.chars() {
        n = n * 16 + c.to_digit(16)?;
    }
    Some([
        ((n >> 16) & 0xff) as f64,
        ((n >> 8) & 0xff) as f64,
        (n & 0xff) as f64,
    ])
}

/// `terrain.oceanColor` in render-settings.json (the JSON import's value).
const OCEAN_COLOR_JSON: &str = "#4785b5";
/// `terrain.backgroundColor` in render-settings.json.
const BACKGROUND_COLOR_JSON: &str = "#3c3c3c";

fn deep_water_base() -> [f64; 3] {
    hex_to_rgb(OCEAN_COLOR_JSON).expect("render-settings oceanColor must be hex")
}
fn background_base() -> [f64; 3] {
    hex_to_rgb(BACKGROUND_COLOR_JSON).expect("render-settings backgroundColor must be hex")
}

/// `colors?.key ?? fallback`: an absent / `undefined` / `null` field (or a
/// non-object `colors`) yields the fallback; a present value yields its
/// three indexed channels. The capture stays in the numeric domain: a
/// non-array or a non-number element indexes to JS `undefined` → NaN.
fn resolve(colors: &JsVal, key: &str, fallback: [f64; 3]) -> [f64; 3] {
    if let JsVal::Obj(fields) = colors {
        if let Some((_, v)) = fields.iter().find(|(k, _)| k == key) {
            if matches!(v, JsVal::Undef | JsVal::Null | JsVal::Absent) {
                return fallback;
            }
            let chan = |i: usize| match v {
                JsVal::Arr(items) => match items.get(i) {
                    Some(JsVal::Num(n)) => *n,
                    _ => f64::NAN,
                },
                _ => f64::NAN,
            };
            return [chan(0), chan(1), chan(2)];
        }
    }
    fallback
}

/// The pure colour computation of `encodeTerrainTile` (before the
/// `Uint8Array` writes): the four RGBA values as JS numbers.
fn encode_rgba(tb: f64, colors: &JsVal) -> [f64; 4] {
    let tbi = to_int32(tb);
    let is_land = (tbi & 0x80) != 0;
    let is_shoreline = (tbi & 0x40) != 0;
    let magnitude = (tbi & 0x1f) as f64;

    let peak = resolve(colors, "backgroundColor", background_base());
    let ocean = resolve(colors, "oceanColor", deep_water_base());
    let sand = resolve(colors, "sandColor", [204.0, 203.0, 158.0]);
    let plains = resolve(colors, "plainsColor", [190.0, 220.0, 138.0]);
    let highland = resolve(colors, "highlandColor", [200.0, 183.0, 138.0]);
    let mountain = resolve(colors, "mountainColor", [230.0, 230.0, 230.0]);

    let (r, g, b) = if is_land && magnitude == 31.0 {
        (peak[0], peak[1], peak[2])
    } else if is_land && is_shoreline {
        (sand[0], sand[1], sand[2])
    } else if is_land {
        if magnitude < 10.0 {
            // Plains — g is UNCLAMPED (can go negative, wraps at the write).
            (plains[0], plains[1] - 2.0 * magnitude, plains[2])
        } else if magnitude < 20.0 {
            let m = magnitude - 10.0;
            (
                js_min(255.0, highland[0] + 2.0 * m),
                js_min(255.0, highland[1] + 2.0 * m),
                js_min(255.0, highland[2] + 2.0 * m),
            )
        } else {
            let m = (magnitude / 2.0).floor();
            (
                js_min(255.0, mountain[0] + m),
                js_min(255.0, mountain[1] + m),
                js_min(255.0, mountain[2] + m),
            )
        }
    } else if is_shoreline {
        (
            js_round(0.7 * ocean[0] + 76.5),
            js_round(0.7 * ocean[1] + 76.5),
            js_round(0.7 * ocean[2] + 76.5),
        )
    } else {
        let m = js_min(magnitude, 10.0);
        (
            js_max(0.0, ocean[0] - m),
            js_max(0.0, ocean[1] - m),
            js_max(0.0, ocean[2] - m),
        )
    };
    [r, g, b, 255.0]
}

// ---------------------------------------------------------------- vectors op
//
// kind 0: constants -> [paletteSize, MAX_TRAIL_COLORS, EFFECT_PALETTE_BLOCKS,
//   STRUCTURES_EFFECT_BLOCK, WARSHIP_EFFECT_BLOCK, TRAIN_EFFECT_BLOCK,
//   RAILROAD_EFFECT_BLOCK]
// kind 1: hexToRgb [...codec(str)] -> [...codec(tuple|null)]
// kind 2: encodeTerrainTile [tb, outLen, offset, ...codec(colors?)]
//   -> [...out bytes] (a fresh Uint8Array(outLen) written at `offset`)
// kind 3: buildTerrainRGBA [...codec(bytes), w, h, ...codec(colors?)]
//   -> [...pixels bytes]

/// Flat-token runner shared by the golden replay and the wasm probe.
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    match kind {
        0 => vec![4096.0, 8.0, 6.0, 2.0, 3.0, 4.0, 5.0],
        1 => {
            let mut i = 0usize;
            let s = read_val(args, &mut i);
            let JsVal::Str(hex) = s else {
                unreachable!("color_utils: hexToRgb arg must be a string");
            };
            let res = match hex_to_rgb(&hex) {
                Some([r, g, b]) => JsVal::Arr(vec![JsVal::Num(r), JsVal::Num(g), JsVal::Num(b)]),
                None => JsVal::Null,
            };
            let mut out = Vec::new();
            push_val(&mut out, &res);
            out
        }
        2 => {
            let mut i = 3usize;
            let colors = read_val(args, &mut i);
            let out_len = args[1] as usize;
            let rgba = encode_rgba(args[0], &colors);
            let mut out = vec![0u8; out_len];
            // `out[offset + k] = v`: a non-integer offset is a non-index key
            // (property, not element); negative / out-of-range typed-array
            // element writes are silently discarded.
            let off = args[2];
            if off.is_finite()
                && off.fract() == 0.0
                && off >= -4.0
                && off < out_len as f64
            {
                let off = off as i64;
                for (k, &v) in rgba.iter().enumerate() {
                    let p = off + k as i64;
                    if p >= 0 && (p as usize) < out_len {
                        out[p as usize] = to_uint8(v);
                    }
                }
            }
            out.into_iter().map(|b| b as f64).collect()
        }
        3 => {
            let mut i = 0usize;
            let bytes = read_val(args, &mut i);
            let JsVal::Arr(items) = bytes else {
                unreachable!("color_utils: buildTerrainRGBA bytes must be an array");
            };
            let w = args[i];
            let h = args[i + 1];
            let mut ci = i + 2;
            let colors = read_val(args, &mut ci);
            let wh = w * h;
            // `new Uint8Array(w * h * 4)`: ToIndex truncates fractions and
            // maps NaN to 0 (negative sizes throw in JS — out of domain).
            let wh4 = wh * 4.0;
            let plen = if wh4.is_finite() && wh4 > 0.0 {
                wh4 as usize
            } else {
                0
            };
            let mut pixels = vec![0u8; plen];
            // `for (i = 0; i < w * h; i++)`: a fractional product rounds the
            // iteration count UP; NaN / negative runs zero iterations.
            let n = if wh.is_finite() && wh > 0.0 {
                wh.ceil() as usize
            } else {
                0
            };
            for t in 0..n {
                // `terrainBytes[i]` past the end reads JS `undefined` → the
                // bitwise ops coerce it to 0 (deep water, magnitude 0).
                let tb = match items.get(t) {
                    Some(JsVal::Num(v)) => *v,
                    _ => f64::NAN,
                };
                let rgba = encode_rgba(tb, &colors);
                let off = t * 4;
                for (k, &v) in rgba.iter().enumerate() {
                    let p = off + k;
                    if p < plen {
                        pixels[p] = to_uint8(v);
                    }
                }
            }
            pixels.into_iter().map(|b| b as f64).collect()
        }
        k => unreachable!("color_utils: unknown op kind {k}"),
    }
}
