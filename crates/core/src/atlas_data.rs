//! Port of `src/client/render/gl/passes/name-pass/AtlasData.ts` — the two
//! CPU-side table builders (`buildGlyphTables` / `buildKernTable`). The
//! fetch/preload/parse path and `buildEmojiLookup` (JSON import) stay in TS.
//!
//! Faithfulness notes:
//!
//! * `buildGlyphTables`: three `Float32Array(CHAR_RANGE)`; the gate is
//!   `ch.id < CHAR_RANGE` — a non-numeric or negative id: `-1 < 384` is TRUE
//!   (JS), and the typed-array write at index -1 is a non-index key (dropped
//!   for elements). Float32 writes go through [`to_float32`] (nearest-even).
//!   `xadvance` / `xoffset` / `width` read `undefined` → NaN when the field
//!   is absent (the capture crosses values as f64; the runner models absent
//!   fields as NaN explicitly).
//! * `buildKernTable`: `Int8Array(384*384)`; gate `k.first < 384 &&
//!   k.second < 384` (negatives pass the JS `<` gate but index to nothing —
//!   element writes outside the array are silently discarded, and the
//!   `first * CHAR_RANGE + second` arithmetic is f64 so a fractional index
//!   is a non-index key, also discarded). Amount writes go through
//!   [`to_int8`] (trunc-then-mod-256, signed reinterpret).
//! * The dump is SPARSE (nonzero cells only) like the other typed-array
//!   ports, plus the table length so the harness pins the allocation.

use crate::js_json::{read_val, val_field, JsVal};
use crate::jsnum::{to_float32, to_int8};

/// `CHAR_RANGE`.
pub const CHAR_RANGE: usize = 384;

/// A glyph row as the capture crosses it: `[id, xadvance, xoffset, width]`
/// with absent fields encoded NaN (the runner's convention).
fn glyph_fields(ch: &JsVal) -> (f64, f64, f64, f64) {
    let f = |k: &str| match val_field(ch, k) {
        Some(JsVal::Num(n)) => *n,
        _ => f64::NAN,
    };
    (f("id"), f("xadvance"), f("xoffset"), f("width"))
}

fn kern_fields(k: &JsVal) -> (f64, f64, f64) {
    let f = |key: &str| match val_field(k, key) {
        Some(JsVal::Num(n)) => *n,
        _ => f64::NAN,
    };
    (f("first"), f("second"), f("amount"))
}

/// `buildGlyphTables(chars)` -> ([advance], [xOffset], [visW]) as
/// f32-rounded f64s, each CHAR_RANGE long.
pub fn build_glyph_tables(chars: &[JsVal]) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let mut advance = vec![0f64; CHAR_RANGE];
    let mut x_offset = vec![0f64; CHAR_RANGE];
    let mut vis_w = vec![0f64; CHAR_RANGE];
    for ch in chars {
        let (id, xadv, xoff, width) = glyph_fields(ch);
        // `ch.id < CHAR_RANGE` on a non-number: NaN < 384 is false; a
        // fractional id passes the gate but writes to a non-index.
        if id >= CHAR_RANGE as f64 || id.is_nan() {
            continue;
        }
        for (tbl, v) in [
            &mut advance as &mut Vec<f64>,
            &mut x_offset,
            &mut vis_w,
        ]
        .into_iter()
        .zip([xadv, xoff, width])
        {
            if id.fract() == 0.0 && id >= 0.0 {
                tbl[id as usize] = to_float32(v) as f64;
            }
        }
    }
    (advance, x_offset, vis_w)
}

/// `buildKernTable(kernings)` -> the 384×384 Int8Array as i8 values.
pub fn build_kern_table(kernings: &[JsVal]) -> Vec<i8> {
    let mut table = vec![0i8; CHAR_RANGE * CHAR_RANGE];
    for k in kernings {
        let (first, second, amount) = kern_fields(k);
        if !(first < CHAR_RANGE as f64 && second < CHAR_RANGE as f64) {
            continue;
        }
        let idx = first * (CHAR_RANGE as f64) + second;
        if idx.fract() == 0.0 && idx >= 0.0 && (idx as usize) < table.len() {
            table[idx as usize] = to_int8(amount);
        }
    }
    table
}

// ---------------------------------------------------------------- vectors op
//
// kind 0: [n, (codec char)*n] -> [1152, ...advance, ...xOffset, ...visW]
//         buildGlyphTables — the FULL tables as f64 (3×384).
// kind 1: [n, (codec kerning)*n] -> [k, (index, value)*k] buildKernTable —
//         sparse nonzero dump of the Int8Array (values as i8→f64).
// kind 2: [] -> [384]                                      CHAR_RANGE dump

/// Flat-token runner shared by the golden replay and the wasm probe.
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    match kind {
        0 => {
            let n = args[0] as usize;
            let mut i = 1usize;
            let chars: Vec<JsVal> = (0..n).map(|_| read_val(args, &mut i)).collect();
            let (a, x, w) = build_glyph_tables(&chars);
            out.extend(a.iter().chain(x.iter()).chain(w.iter()));
        }
        1 => {
            let n = args[0] as usize;
            let mut i = 1usize;
            let kerns: Vec<JsVal> = (0..n).map(|_| read_val(args, &mut i)).collect();
            let table = build_kern_table(&kerns);
            let nz: Vec<usize> = (0..table.len()).filter(|&j| table[j] != 0).collect();
            out.push(nz.len() as f64);
            for j in nz {
                out.push(j as f64);
                out.push(table[j] as f64);
            }
        }
        2 => out.push(CHAR_RANGE as f64),
        k => unreachable!("atlas_data: unknown op kind {k}"),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(fields: &[(&str, f64)]) -> JsVal {
        JsVal::Obj(
            fields
                .iter()
                .map(|(k, v)| (k.to_string(), JsVal::Num(*v)))
                .collect(),
        )
    }

    #[test]
    fn glyph_gate() {
        let chars = vec![
            obj(&[("id", 65.0), ("xadvance", 10.5), ("xoffset", -1.25), ("width", 7.0)]),
            obj(&[("id", 384.0), ("xadvance", 1.0), ("xoffset", 1.0), ("width", 1.0)]),
            obj(&[("id", 383.0), ("xadvance", 2.0), ("xoffset", 2.0), ("width", 2.0)]),
        ];
        let (a, x, w) = build_glyph_tables(&chars);
        assert_eq!(a[65], 10.5);
        assert_eq!(x[65], -1.25);
        assert_eq!(w[65], 7.0);
        assert!(a.get(384).is_none()); // gate: table stays 384 long, write dropped
        assert_eq!(a.len(), CHAR_RANGE);
        assert_eq!(a[383], 2.0);
        // Float32 rounding: 0.1 stores as f32.
        let c = vec![obj(&[("id", 1.0), ("xadvance", 0.1), ("xoffset", 0.0), ("width", 0.0)])];
        let (a, _, _) = build_glyph_tables(&c);
        assert_eq!(a[1], (0.1f64 as f32) as f64);
    }

    #[test]
    fn kern_gate_and_wrap() {
        let kerns = vec![
            obj(&[("first", 65.0), ("second", 66.0), ("amount", -3.0)]),
            obj(&[("first", 65.0), ("second", 66.0), ("amount", 257.0)]), // wraps to 1
            obj(&[("first", 384.0), ("second", 1.0), ("amount", 9.0)]),    // gate
            obj(&[("first", 65.0), ("second", 384.0), ("amount", 9.0)]),   // gate
            obj(&[("first", 0.0), ("second", 0.0), ("amount", 127.0)]),
            obj(&[("first", 0.0), ("second", 1.0), ("amount", 128.0)]), // wraps to -128
        ];
        let t = build_kern_table(&kerns);
        assert_eq!(t[65 * CHAR_RANGE + 66], 1);
        assert!(t.get(384 * CHAR_RANGE + 1).is_none()); // gate: write dropped
        assert_eq!(t.len(), CHAR_RANGE * CHAR_RANGE);
        assert_eq!(t[0], 127);
        assert_eq!(t[1], -128);
    }

    #[test]
    fn sparse_dump_shape() {
        let mut tbl = vec![0i8; CHAR_RANGE * CHAR_RANGE];
        tbl[5] = 3;
        let nz: Vec<usize> = (0..tbl.len()).filter(|&j| tbl[j] != 0).collect();
        assert_eq!(nz, vec![5]);
    }
}
