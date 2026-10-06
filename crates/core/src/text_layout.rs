//! Port of `src/client/render/gl/passes/name-pass/TextLayout.ts` —
//! `layoutString`, the pure-CPU text shaping that encodes char codes,
//! computes advance/kerning cursor positions and centres them on the visual
//! bounds. The `GlyphTables` import is type-only (ts_load drops it);
//! `CHAR_RANGE = 384` / `MAX_CHARS = 32` ride from `name-pass/Types.ts`.
//!
//! Faithfulness notes (quirk list):
//!
//! * `charCodes[i] = text.charCodeAt(i)` writes through a `Uint8Array` —
//!   every UTF-16 code unit (including surrogates of astral pairs) is
//!   `to_uint8`-truncated, so `Ā` (0x100) lands as 0 and `😀`'s high
//!   surrogate 0xD83D lands as 61.
//! * `cursors` is a `Float32Array`: every write rounds through
//!   [`to_float32`], every read widens back to f64, so the centring
//!   subtraction `cursors[i] -= visualCenter` is `f32(f32(c) - vc)` and can
//!   produce `-0` (Object.is-compared on the wire).
//! * `kernTable[prevCode * 384 + code]` reads an `Int8Array` whose capture
//!   length is far below `384 * 384`: an out-of-range index reads JS
//!   `undefined`, `adv += undefined` poisons the accumulator to NaN, and
//!   NaN then flows through every later cursor and the returned halfWidth.
//! * Empty string: `len = 0` skips all loops, but the visual-bounds block
//!   still runs — `charCodes[0]` reads the zero fill (firstCode 0, valid),
//!   while `charCodes[len - 1]` is `charCodes[-1]` → `undefined`, so the
//!   `xOffset[undefined]` / `visW[undefined]` / `cursors[-1]` reads are all
//!   NaN and the function returns NaN (the buffers stay zero-filled).
//! * `glyph.advance[code]` is always in range (code ≤ 255 < 384), but the
//!   table itself stores f32 values.

use crate::jsnum::{to_float32, to_uint8};

const CHAR_RANGE: usize = 384;
const MAX_CHARS: usize = 32;

/// Harness holding the scripted glyph tables (setup op), then running
/// stateless layouts against them.
#[derive(Default)]
pub struct RigHarness {
    advance: Vec<f64>,
    x_offset: Vec<f64>,
    vis_w: Vec<f64>,
    kern: Vec<f64>,
    half_width: f64,
}

impl RigHarness {
    pub fn new() -> Self {
        RigHarness::default()
    }

    pub fn reset(&mut self) {
        *self = RigHarness::default();
    }

    /// kind table (mirrors the capture):
    ///   0 setup   [adv*384, xoff*384, visw*384, klen, (kidx, kval)*klen] -> [1]
    ///   1 layout  [tlen, (u0..)*tlen] -> [halfWidth, charCodes*32, cursors*32]
    ///
    /// Table entries arrive already f32-rounded (the capture records them
    /// through the Float32Array); only the NON-ZERO kern pairs are carried —
    /// the Int8Array is zero-filled, so an in-range miss reads `0` (a real
    /// number), while an index at/above `klen` reads `undefined` -> NaN.
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        match kind {
            0 => {
                let n = 3 * CHAR_RANGE;
                self.advance = args[..CHAR_RANGE].to_vec();
                self.x_offset = args[CHAR_RANGE..2 * CHAR_RANGE].to_vec();
                self.vis_w = args[2 * CHAR_RANGE..n].to_vec();
                let klen = args[n] as usize;
                let mut kern = vec![0.0f64; klen];
                for pair in args[n + 1..].chunks(2) {
                    let idx = pair[0] as usize;
                    if idx < klen {
                        kern[idx] = pair[1];
                    }
                }
                self.kern = kern;
                vec![1.0]
            }
            1 => {
                let tlen = args[0] as usize;
                let units: Vec<u16> = args[1..1 + tlen].iter().map(|u| *u as u16).collect();
                let mut out = self.layout(&units);
                out.insert(0, self.half_width);
                out
            }
            k => unreachable!("text_layout: unknown op kind {k}"),
        }
    }

    /// `layoutString(text, glyph, kernTable, charCodes, cursors)` — the two
    /// caller buffers are locals here (the capture dumps them after each
    /// call, so they must start zero-filled every time, like `.fill(0)`).
    fn layout(&mut self, units: &[u16]) -> Vec<f64> {
        let mut char_codes = [0u8; MAX_CHARS];
        let mut cursors = [0.0f64; MAX_CHARS];
        let len = units.len().min(MAX_CHARS);

        for (i, ch) in char_codes.iter_mut().enumerate().take(len) {
            *ch = to_uint8(units[i] as f64);
        }

        // Advance-based cursor positions.
        let mut cumulative = 0.0f64;
        let mut prev_code = 0.0f64;
        for i in 0..len {
            let code = char_codes[i] as f64;
            cursors[i] = to_float32(cumulative) as f64;
            let mut adv = self.advance[code as usize];
            if i > 0 {
                let idx = prev_code * CHAR_RANGE as f64 + code;
                // Int8Array read: only canonical own indices below length
                // return a value; everything else is undefined -> NaN.
                let k = if (idx as usize) < self.kern.len() {
                    self.kern[idx as usize]
                } else {
                    f64::NAN
                };
                adv += k;
            }
            cumulative += adv;
            prev_code = code;
        }

        // Center on visual bounds.
        let first_code = char_codes[0] as usize; // zero fill when len == 0
        let visual_left = cursors[0] + self.x_offset[first_code];
        let (cursors_last, xoff_last, visw_last) = if len == 0 {
            // charCodes[-1] / cursors[-1] read undefined; indexing the
            // Float32Array with `undefined` is a non-index property -> NaN.
            (f64::NAN, f64::NAN, f64::NAN)
        } else {
            let last_code = char_codes[len - 1] as usize;
            (
                cursors[len - 1],
                self.x_offset[last_code],
                self.vis_w[last_code],
            )
        };
        let visual_right = cursors_last + xoff_last + visw_last;
        let visual_center = (visual_left + visual_right) * 0.5;
        for c in cursors.iter_mut().take(len) {
            *c = to_float32(*c - visual_center) as f64;
        }

        self.half_width = (visual_right - visual_left) * 0.5;
        let mut out = Vec::with_capacity(1 + 2 * MAX_CHARS);
        for c in char_codes {
            out.push(c as f64);
        }
        out.extend_from_slice(&cursors);
        out
    }
}
