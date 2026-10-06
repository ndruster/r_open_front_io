//! Port of `src/client/render/gl/utils/TileCodec.ts` — the R16UI tile-state
//! bit layout, a single source of truth shared with the GLSL shaders.
//!
//! Pure constants: `OWNER_MASK = 0xfff` (bits 0-11), `FALLOUT_BIT = 1 << 13`,
//! `DEFENSE_BIT = 1 << 14`, and the `TILE_DEFINES` table whose values are the
//! BIT INDICES (13 / 14), not the masks — the shader injects them as
//! `(1u << FALLOUT_BIT)`. The capture dumps every value and the key order of
//! `TILE_DEFINES` (OWNER_MASK, FALLOUT_BIT, DEFENSE_BIT).

use crate::js_json::push_str;

/// `OWNER_MASK` — bits 0-11 of the packed tile word.
pub const OWNER_MASK: f64 = 4095.0;
/// `FALLOUT_BIT` — the mask form (`1 << 13`).
pub const FALLOUT_BIT: f64 = 8192.0;
/// `DEFENSE_BIT` — the mask form (`1 << 14`).
pub const DEFENSE_BIT: f64 = 16384.0;

/// `run_op(kind, args)` — capture harness entry (stateless). Kind table (see
/// `tools/gen_vectors.mjs`):
/// 0 -> `[OWNER_MASK, FALLOUT_BIT, DEFENSE_BIT]` (mask forms);
/// 1 -> TILE_DEFINES dump `[3, (key-str, value)*3]` in source key order
///   (OWNER_MASK 0xfff, FALLOUT_BIT 13, DEFENSE_BIT 14 — bit INDICES).
pub fn run_op(kind: u8, _args: &[f64]) -> Vec<f64> {
    match kind {
        0 => vec![OWNER_MASK, FALLOUT_BIT, DEFENSE_BIT],
        1 => {
            let mut out = vec![3.0];
            push_str(&mut out, "OWNER_MASK");
            out.push(4095.0);
            push_str(&mut out, "FALLOUT_BIT");
            out.push(13.0);
            push_str(&mut out, "DEFENSE_BIT");
            out.push(14.0);
            out
        }
        k => unreachable!("tile_codec: unknown op kind {k}"),
    }
}
