//! Port of `src/client/render/gl/utils/EffectPalette.ts` —
//! `parseEffectColors` / `packEffectEntry` / `EFFECT_ENTRY_FLOATS`. The
//! `colord` dependency is NOT in the port graph (ThemeProvider precedent):
//! the capture runs the REAL colord 2.9.3 package in V8 and scripts each
//! input string's `(isValid, r, g, b)` observation into a facade table that
//! travels with every op, so the golden is pinned by the genuine parser and
//! the Rust twin replays the same table (facade precedent: the `cpl_`
//! counting getters).
//!
//! Faithfulness notes (quirk list, all V8-pinned):
//!
//! * `colord` without plugins: named colors ("red", "transparent"), hwb /
//!   cmyk / lab / lch / xyz spellings and non-string inputs are ALL invalid;
//!   hex 3/4/6/8 (case-insensitive, surrounding JS whitespace trimmed),
//!   `rgb()` / `rgba()` / `hsl()` with comma or space separators are valid.
//!   The observation table (not this file) encodes those values.
//! * `r / 255` is IEEE f64 division, then the Float32Array element write
//!   rounds to f32 (`to_float32`).
//! * `alphas[r] ?? 0` intercepts undefined BEFORE the write: an
//!   out-of-domain `attrs.type` with a missing `colorSize` writes 0, NOT NaN
//!   (the `?? 0` fires). NaN only reaches the buffer when the scalar EXISTS
//!   but is non-numeric (e.g. `colorSize: "abc"` -> ToNumber NaN -> f32 NaN).
//! * `out[off] = c ? c[0] : 0` keeps a truthy row even when its channel is
//!   0; the f32 write preserves -0.
//! * `attrs.colors` missing or non-array -> `.map` TypeError -> the whole op
//!   fails ([1]); non-string array elements never throw — the real package
//!   returns invalid for them (V8-pinned), so they are dropped.
//! * A non-nullish object / array attrs scalar reaches the Float32Array
//!   write through JS ToNumber: `Obj` -> NaN, `Arr` -> ToPrimitive ->
//!   `join(",")` -> `js_number` (V8-pinned: `[]` -> "" -> 0, `[5]` -> 5,
//!   `[1,2]` -> "1,2" -> NaN, `[true]` -> "true" -> NaN, undefined / null
//!   elements contribute empty segments, nested arrays recurse, `-0`
//!   stringifies to "0").

use crate::game_config_helpers::js_number;
use crate::js_fixed::js_to_string;
use crate::js_json::{read_str, read_val, val_field, JsVal};
use crate::jsnum::to_float32;

/// `MAX_TRAIL_COLORS` (ColorUtils.ts:25) — the palette row count.
pub const MAX_TRAIL_COLORS: usize = 8;

/// `EFFECT_ENTRY_FLOATS = MAX_TRAIL_COLORS * 4`.
pub const EFFECT_ENTRY_FLOATS: usize = MAX_TRAIL_COLORS * 4;

/// One scripted colord observation: `(valid, r, g, b)` over 0..255.
pub type ColordObs = (bool, f64, f64, f64);

/// Scripted colord table: `(input string, observation)` pairs.
pub type ColordTable = [(String, ColordObs)];

/// `parseEffectColors` over the scripted colord table: drop invalid inputs
/// (non-string elements are invalid in the real package), cap the survivors
/// at `MAX_TRAIL_COLORS`, map them to `r / 255` f64 triples.
pub fn parse_effect_colors(colors: &[JsVal], table: &ColordTable) -> Vec<[f64; 3]> {
    let mut out: Vec<[f64; 3]> = Vec::new();
    for it in colors {
        if out.len() == MAX_TRAIL_COLORS {
            break;
        }
        let JsVal::Str(s) = it else {
            continue; // colord(non-string) is invalid (V8-pinned domain)
        };
        // The capture observes every distinct input string before running
        // the function, so a table miss is a capture bug.
        let (valid, r, g, b) = table
            .iter()
            .find(|(k, _)| k == s)
            .map(|(_, v)| *v)
            .unwrap_or_else(|| panic!("effect_palette: colord table miss for {s:?}"));
        if valid {
            out.push([r / 255.0, g / 255.0, b / 255.0]);
        }
    }
    out
}

/// JS ToNumber over the codec field domain; `?? 0` intercepts the nullish
/// reads (absent / undefined / null) before the Float32Array write. A
/// non-nullish Obj reads NaN, an Arr goes through ToPrimitive ->
/// `join(",")` -> the string ToNumber (V8-pinned).
fn field_to_number(attrs: &JsVal, key: &str) -> f64 {
    match val_field(attrs, key) {
        Some(JsVal::Num(n)) => *n,
        Some(JsVal::Bool(b)) => f64::from(*b),
        Some(JsVal::Str(s)) => js_number(s),
        Some(JsVal::Obj(_)) => f64::NAN,
        Some(JsVal::Arr(items)) => js_number(&js_join(items)),
        // Undef / Null / Absent: `?? 0` fires (ToNumber would also give 0
        // for null, but the intercept is what V8 runs).
        _ => 0.0,
    }
}

/// `Array.prototype.join(",")` over the codec domain (the ToPrimitive path
/// of an array attrs scalar): undefined / null / absent elements contribute
/// empty segments, nested arrays recurse, objects stringify to
/// "[object Object]", numbers go through ECMAScript Number::toString
/// (both zeros print "0", NaN prints "NaN").
fn js_join(items: &[JsVal]) -> String {
    items
        .iter()
        .map(|it| match it {
            JsVal::Num(n) => js_to_string(*n),
            JsVal::Str(s) => s.clone(),
            JsVal::Bool(b) => if *b { "true" } else { "false" }.to_string(),
            JsVal::Arr(inner) => js_join(inner),
            JsVal::Undef | JsVal::Null | JsVal::Absent => String::new(),
            JsVal::Obj(_) => "[object Object]".to_string(),
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// `packEffectEntry` over the scripted colord table: parse the colors, route
/// the style by `attrs.type`, then write the 8 RGBA rows of the entry into
/// `out` (EFFECT_ENTRY_FLOATS f32 slots, JS Float32Array store semantics).
pub fn pack_effect_entry(attrs: &JsVal, table: &ColordTable, out: &mut [f32]) {
    let JsVal::Arr(items) = val_field(attrs, "colors").unwrap_or(&JsVal::Undef) else {
        // `attrs.colors.map` on a non-array: JS TypeError, the op fails.
        unreachable!("effect_palette: pack called with non-array colors");
    };
    let colors = parse_effect_colors(items, table);
    let (style_id, scalar0, scalar1): (f64, f64, f64) = match val_field(attrs, "type") {
        Some(JsVal::Str(s)) if s == "transition" => (1.0, field_to_number(attrs, "frequency"), 0.0),
        Some(JsVal::Str(s)) if s == "spiral" => (2.0, field_to_number(attrs, "rotationSpeed"), 0.0),
        _ => (
            0.0,
            field_to_number(attrs, "colorSize"),
            field_to_number(attrs, "movementSpeed"),
        ),
    };
    let alphas = [colors.len() as f64, style_id, scalar0, scalar1];
    // The row loop indexes three tables (colors / alphas / out) through the
    // same r, so the range-loop lint does not apply (base64_uuid precedent).
    #[allow(clippy::needless_range_loop)]
    for r in 0..MAX_TRAIL_COLORS {
        let off = r * 4;
        // `c ? c[k] : 0`: a truthy row keeps its channel verbatim; `alphas[r]
        // ?? 0` zeroes rows past the four-entry head (r >= 4 reads undefined).
        let (c0, c1, c2) = match colors.get(r) {
            Some(t) => (t[0], t[1], t[2]),
            None => (0.0, 0.0, 0.0),
        };
        out[off] = to_float32(c0);
        out[off + 1] = to_float32(c1);
        out[off + 2] = to_float32(c2);
        out[off + 3] = to_float32(if r < 4 { alphas[r] } else { 0.0 });
    }
}

// ---------------------------------------------------------------- vectors op
//
// kind 0: parseEffectColors batch — args `[n, (colors list codec)*n, m,
//         (encS input, valid 1|0, r, g, b)*m]` -> `([0, k, (triple f64)*k] |
//         [1])*n` ([1] models the `.map` TypeError on a non-array / missing
//         list).
// kind 1: packEffectEntry batch — args `[n, (attrs codec)*n, m, (encS input,
//         valid 1|0, r, g, b)*m]` -> `([0, (f32-stored float)*32] | [1])*n`
//         ([1] models the TypeError from a non-object attrs or a missing /
//         non-array attrs.colors). The capture observes every distinct color
//         string of a batch once into the shared table.

pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut i = 0usize;
    match kind {
        0 => {
            let n = args[i] as usize;
            i += 1;
            let lists: Vec<JsVal> = (0..n).map(|_| read_val(args, &mut i)).collect();
            let table = read_table(args, &mut i);
            let mut out = Vec::new();
            for list in lists {
                match list {
                    JsVal::Arr(items) => {
                        let triples = parse_effect_colors(&items, &table);
                        out.push(0.0);
                        out.push(triples.len() as f64);
                        for t in triples {
                            out.extend_from_slice(&t);
                        }
                    }
                    _ => out.push(1.0), // `.map` TypeError
                }
            }
            out
        }
        1 => {
            let n = args[i] as usize;
            i += 1;
            let batch: Vec<JsVal> = (0..n).map(|_| read_val(args, &mut i)).collect();
            let table = read_table(args, &mut i);
            let mut out = Vec::new();
            for attrs in batch {
                let colors_ok = matches!(
                    val_field(&attrs, "colors"),
                    Some(JsVal::Arr(_))
                );
                if matches!(attrs, JsVal::Obj(_)) && colors_ok {
                    let mut buf = vec![0f32; EFFECT_ENTRY_FLOATS];
                    pack_effect_entry(&attrs, &table, &mut buf);
                    out.push(0.0);
                    for f in buf {
                        out.push(f as f64);
                    }
                } else {
                    out.push(1.0);
                }
            }
            out
        }
        k => unreachable!("effect_palette: unknown op kind {k}"),
    }
}

/// `[m, (encS input, valid 1|0, r, g, b)*m]` scripted colord table (the
/// input is a bare `[len, u*]` string token, matching the capture's encS).
fn read_table(a: &[f64], i: &mut usize) -> Vec<(String, ColordObs)> {
    let m = a[*i] as usize;
    *i += 1;
    (0..m)
        .map(|_| {
            let s = read_str(a, i);
            let valid = a[*i] != 0.0;
            *i += 1;
            let r = a[*i];
            let g = a[*i + 1];
            let b = a[*i + 2];
            *i += 3;
            (s, (valid, r, g, b))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::js_json::JsVal;

    fn table(entries: &[(&str, ColordObs)]) -> Vec<(String, ColordObs)> {
        entries
            .iter()
            .map(|(s, o)| (s.to_string(), *o))
            .collect()
    }

    fn obj(fields: &[(&str, JsVal)]) -> JsVal {
        JsVal::Obj(fields.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
    }

    fn arr(items: &[&str]) -> JsVal {
        JsVal::Arr(items.iter().map(|s| JsVal::Str(s.to_string())).collect())
    }

    #[test]
    fn parse_drops_invalid_and_caps() {
        let t = table(&[
            ("#ff0000", (true, 255.0, 0.0, 0.0)),
            ("bad", (false, 0.0, 0.0, 0.0)),
            ("#00ff00", (true, 0.0, 255.0, 0.0)),
        ]);
        let colors = vec![
            JsVal::Str("#ff0000".into()),
            JsVal::Str("bad".into()),
            JsVal::Str("#00ff00".into()),
        ];
        let got = parse_effect_colors(&colors, &t);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0], [1.0, 0.0, 0.0]);
        assert_eq!(got[1], [0.0, 1.0, 0.0]);
        // cap at MAX_TRAIL_COLORS even with more valid rows
        let many: Vec<JsVal> = (0..12)
            .map(|_| JsVal::Str("#ff0000".into()))
            .collect();
        assert_eq!(parse_effect_colors(&many, &t).len(), MAX_TRAIL_COLORS);
    }

    #[test]
    fn pack_gradient_rows_and_alphas() {
        let t = table(&[("#ff0000", (true, 255.0, 0.0, 0.0))]);
        let attrs = obj(&[
            ("type", JsVal::Str("gradient".into())),
            ("colors", arr(&["#ff0000"])),
            ("colorSize", JsVal::Num(1.5)),
            ("movementSpeed", JsVal::Num(3.0)),
        ]);
        let mut buf = vec![0f32; EFFECT_ENTRY_FLOATS];
        pack_effect_entry(&attrs, &t, &mut buf);
        assert_eq!(&buf[0..4], &[1.0, 0.0, 0.0, 1.0]); // row0: color + count
        assert_eq!(&buf[4..8], &[0.0, 0.0, 0.0, 0.0]); // row1: zero + styleId 0
        assert_eq!(&buf[8..12], &[0.0, 0.0, 0.0, 1.5]); // row2: colorSize
        assert_eq!(&buf[12..16], &[0.0, 0.0, 0.0, 3.0]); // row3: movementSpeed
        assert!(buf[16..].iter().all(|&x| x == 0.0)); // rows 4..7 zero alphas
    }

    #[test]
    fn pack_out_of_domain_missing_scalars_write_zero_not_nan() {
        let t = table(&[]);
        let attrs = obj(&[
            ("type", JsVal::Str("weird".into())),
            ("colors", arr(&[])),
        ]);
        let mut buf = vec![0f32; EFFECT_ENTRY_FLOATS];
        pack_effect_entry(&attrs, &t, &mut buf);
        // `?? 0` intercepts the undefined colorSize / movementSpeed.
        assert_eq!(buf[11], 0.0);
        assert_eq!(buf[15], 0.0);
    }

    #[test]
    fn pack_non_numeric_scalar_writes_nan() {
        let t = table(&[]);
        let attrs = obj(&[
            ("type", JsVal::Str("gradient".into())),
            ("colors", arr(&[])),
            ("colorSize", JsVal::Str("abc".into())),
            ("movementSpeed", JsVal::Num(2.0)),
        ]);
        let mut buf = vec![0f32; EFFECT_ENTRY_FLOATS];
        pack_effect_entry(&attrs, &t, &mut buf);
        assert!(buf[11].is_nan());
        assert_eq!(buf[15], 2.0);
    }

    #[test]
    fn pack_object_scalar_writes_nan_array_scalar_joins() {
        let t = table(&[]);
        let mut buf = vec![0f32; EFFECT_ENTRY_FLOATS];
        // Obj -> ToNumber NaN (NOT the ?? 0 intercept); [] -> "" -> 0.
        pack_effect_entry(
            &obj(&[
                ("type", JsVal::Str("gradient".into())),
                ("colors", arr(&[])),
                ("colorSize", JsVal::Obj(vec![])),
                ("movementSpeed", JsVal::Arr(vec![])),
            ]),
            &t,
            &mut buf,
        );
        assert!(buf[11].is_nan());
        assert_eq!(buf[15], 0.0);
        // [5] -> "5" -> 5; [1,2] -> "1,2" -> NaN.
        pack_effect_entry(
            &obj(&[
                ("type", JsVal::Str("transition".into())),
                ("colors", arr(&[])),
                ("frequency", JsVal::Arr(vec![JsVal::Num(5.0)])),
            ]),
            &t,
            &mut buf,
        );
        assert_eq!(buf[11], 5.0);
        pack_effect_entry(
            &obj(&[
                ("type", JsVal::Str("transition".into())),
                ("colors", arr(&[])),
                (
                    "frequency",
                    JsVal::Arr(vec![JsVal::Num(1.0), JsVal::Num(2.0)]),
                ),
            ]),
            &t,
            &mut buf,
        );
        assert!(buf[11].is_nan());
        // null / undefined stay on the ?? 0 side of the divide.
        pack_effect_entry(
            &obj(&[
                ("type", JsVal::Str("gradient".into())),
                ("colors", arr(&[])),
                ("colorSize", JsVal::Null),
                ("movementSpeed", JsVal::Undef),
            ]),
            &t,
            &mut buf,
        );
        assert_eq!(buf[11], 0.0);
        assert_eq!(buf[15], 0.0);
    }

    #[test]
    fn pack_transition_and_spiral_styles() {
        let t = table(&[]);
        let mut buf = vec![0f32; EFFECT_ENTRY_FLOATS];
        pack_effect_entry(
            &obj(&[
                ("type", JsVal::Str("transition".into())),
                ("colors", arr(&[])),
                ("frequency", JsVal::Num(0.25)),
            ]),
            &t,
            &mut buf,
        );
        assert_eq!(buf[7], 1.0); // styleId
        assert_eq!(buf[11], 0.25); // frequency
        assert_eq!(buf[15], 0.0); // scalar1 = 0
        pack_effect_entry(
            &obj(&[
                ("type", JsVal::Str("spiral".into())),
                ("colors", arr(&[])),
                ("rotationSpeed", JsVal::Num(7.0)),
            ]),
            &t,
            &mut buf,
        );
        assert_eq!(buf[7], 2.0);
        assert_eq!(buf[11], 7.0);
    }

    #[test]
    fn runner_kind0_with_typeerror_token() {
        // args: [2, <arr codec>, <undef token>, 1, encS("#ff0000"), 1, 255,
        // 0, 0]; the codec tokens lead with their own code byte.
        let mut args = vec![2.0, 7.0, 1.0, 5.0, 7.0];
        for u in "#ff0000".encode_utf16() {
            args.push(u as f64);
        }
        args.push(1.0); // JsVal::Undef token for the missing list
        args.push(1.0); // table size
        args.push(7.0); // encS("#ff0000")
        for u in "#ff0000".encode_utf16() {
            args.push(u as f64);
        }
        args.extend([1.0, 255.0, 0.0, 0.0]);
        let r = run_op(0, &args);
        assert_eq!(&r[..5], &[0.0, 1.0, 1.0, 0.0, 0.0]);
        assert_eq!(&r[5..6], &[1.0]);
    }

    #[test]
    fn runner_kind1_rejects_non_object_attrs() {
        let mut args = vec![1.0, 5.0, 4.0]; // [n, str codec]
        for u in "nope".encode_utf16() {
            args.push(u as f64);
        }
        args.push(0.0); // empty table
        let r = run_op(1, &args);
        assert_eq!(r, vec![1.0]);
    }
}
