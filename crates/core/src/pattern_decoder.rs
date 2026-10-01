//! Port of `src/core/PatternDecoder.ts`.
//!
//! Decodes a player-cosmetic pattern from its packed byte form and answers
//! "is tile (x, y) the primary colour?" The `base64urlDecode` dependency is an
//! injected function in TS, so the port takes the already-decoded `&[u8]`
//! directly — the base64 layer is not ported (it is external codec, not
//! simulation logic). `PlayerPattern` is a type-only import.
//!
//! Faithfulness notes:
//!
//! * `x >> this.scale` is JS `>>` = `ToInt32(x) >> ToInt32(scale)`, so a huge
//!   or fractional `x` first wraps through [`to_int32`]; `scale` is always in
//!   `[0, 7]` (from `byte1 & 0x07`).
//! * `% this.width` / `% this.height` are JS remainder on the (already
//!   integer) shift results — sign-of-dividend. Rust `i32 % i32` matches for
//!   integer operands, so a negative shift result yields a negative `px`/`py`,
//!   and `idx = py * width + px` can go negative.
//! * `this.bytes[3 + byteIndex]` is a typed-array element read: an index
//!   outside `[0, len)` is `undefined`, which `isPrimary` turns into a throw.
//!   A *negative* `byteIndex` still lands on a valid slot (e.g. `-1` → `bytes[2]`,
//!   a metadata byte), so the bounds test is on `3 + byteIndex`, not on
//!   `byteIndex` alone.
//! * The three `decodePatternData` throws are distinct messages; the port
//!   returns a numeric code (`1` too short, `2` bad version, `3` too short for
//!   dimensions) so parity can tell them apart. `isPrimary`'s throw is code
//!   `4`.

use crate::jsnum::to_int32;

/// Which `decodePatternData` guard fired (or the `isPrimary` bounds throw).
pub const ERR_TOO_SHORT: u8 = 1;
pub const ERR_VERSION: u8 = 2;
pub const ERR_DIMENSIONS: u8 = 3;
pub const ERR_INVALID_PATTERN: u8 = 4;

/// A decoded pattern: the header fields plus the raw byte buffer `isPrimary`
/// indexes (kept whole, including the 3-byte header, exactly as TS does).
#[derive(Clone, Debug)]
pub struct PatternDecoder {
    pub height: f64,
    pub width: f64,
    pub scale: f64,
    bytes: Vec<u8>,
}

impl PatternDecoder {
    /// `decodePatternData(b64, decode)` + the constructor's field assignment.
    /// `Err(code)` models the corresponding `throw`.
    pub fn new(bytes: &[u8]) -> Result<Self, u8> {
        if bytes.len() < 3 {
            return Err(ERR_TOO_SHORT);
        }
        let version = bytes[0];
        if version != 0 {
            return Err(ERR_VERSION);
        }
        let byte1 = bytes[1];
        let byte2 = bytes[2];
        let scale = (byte1 & 0x07) as f64;
        let width = ((((byte2 & 0x03) as i32) << 5) | (((byte1 >> 3) & 0x1f) as i32)) as f64 + 2.0;
        let height = (((byte2 >> 2) & 0x3f) as i32) as f64 + 2.0;
        let expected_bits = width * height;
        let expected_bytes = (to_int32(expected_bits) + 7) >> 3;
        if (bytes.len() as i32) - 3 < expected_bytes {
            return Err(ERR_DIMENSIONS);
        }
        Ok(Self {
            height,
            width,
            scale,
            bytes: bytes.to_vec(),
        })
    }

    /// `isPrimary(x, y)`. `Err(ERR_INVALID_PATTERN)` models the
    /// `bytes[3 + byteIndex] === undefined` throw.
    pub fn is_primary(&self, x: f64, y: f64) -> Result<bool, u8> {
        let scale = to_int32(self.scale) as u32;
        let width = to_int32(self.width);
        let height = to_int32(self.height);
        let px = (to_int32(x) >> scale) % width;
        let py = (to_int32(y) >> scale) % height;
        let idx = py * width + px;
        let byte_index = idx >> 3;
        let bit_index = idx & 7;
        let pos = 3 + byte_index;
        let byte = if pos < 0 || (pos as usize) >= self.bytes.len() {
            return Err(ERR_INVALID_PATTERN);
        } else {
            self.bytes[pos as usize]
        };
        Ok(((byte as i32) & (1 << bit_index)) == 0)
    }

    /// `scaledHeight()` — `height << scale` (JS `<<` = ToInt32 both sides).
    pub fn scaled_height(&self) -> f64 {
        (to_int32(self.height) << to_int32(self.scale)) as f64
    }

    /// `scaledWidth()` — `width << scale`.
    pub fn scaled_width(&self) -> f64 {
        (to_int32(self.width) << to_int32(self.scale)) as f64
    }
}

// ---------------------------------------------------------------- vectors op
//
// Flat `f64` token runner shared by the golden replay and the wasm probe. A
// byte buffer crosses as `[len, b0, .. ]`; `x` / `y` use the `uenc` special
// tokens on the JS side. Results: kind 0 emits `[0, h, w, scale, sh, sw]` on
// success or `[code]` on a decode throw; kind 1 first decodes (a decode throw
// short-circuits to `[code]`), then emits `[0]`/`[1]` for the primary verdict
// or `[ERR_INVALID_PATTERN]`.

fn read_bytes(c: &mut PdCur) -> Vec<u8> {
    let len = c.u();
    (0..len).map(|_| c.f() as u8).collect()
}

struct PdCur<'a>(&'a [f64], usize);
impl<'a> PdCur<'a> {
    fn f(&mut self) -> f64 {
        let v = self.0[self.1];
        self.1 += 1;
        v
    }
    fn u(&mut self) -> usize {
        self.f() as usize
    }
}

/// `kind`: 0 construct (decode header), 1 `isPrimary(bytes, x, y)`.
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut c = PdCur(args, 0);
    let mut out = Vec::new();
    match kind {
        0 => {
            let bytes = read_bytes(&mut c);
            match PatternDecoder::new(&bytes) {
                Ok(d) => {
                    out.push(0.0);
                    out.push(d.height);
                    out.push(d.width);
                    out.push(d.scale);
                    out.push(d.scaled_height());
                    out.push(d.scaled_width());
                }
                Err(code) => out.push(code as f64),
            }
        }
        _ => {
            let bytes = read_bytes(&mut c);
            let x = c.f();
            let y = c.f();
            match PatternDecoder::new(&bytes) {
                Err(code) => out.push(code as f64),
                Ok(d) => match d.is_primary(x, y) {
                    Ok(b) => out.push(f64::from(b)),
                    Err(code) => out.push(code as f64),
                },
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // Header: version 0, byte1 = scale | (width-2)<<3, byte2 = (height-2)<<2 | ((width-2)>>5).
    // width = ((byte2&3)<<5) | ((byte1>>3)&0x1f) + 2 ; height = ((byte2>>2)&0x3f) + 2.
    fn header(scale: u8, width: u8, height: u8) -> Vec<u8> {
        let w = (width - 2) as u16;
        let h = (height - 2) as u16;
        let byte1 = (scale & 0x07) | (((w & 0x1f) as u8) << 3);
        let byte2 = (((h as u8) & 0x3f) << 2) | ((w >> 5) as u8 & 0x03);
        vec![0, byte1, byte2]
    }

    #[test]
    fn decode_header_roundtrip() {
        let mut b = header(2, 5, 4);
        // 5*4 = 20 bits -> 3 bytes of payload.
        b.extend([0x00, 0x00, 0x00]);
        let d = PatternDecoder::new(&b).unwrap();
        assert_eq!(d.scale, 2.0);
        assert_eq!(d.width, 5.0);
        assert_eq!(d.height, 4.0);
        assert_eq!(d.scaled_width(), 5.0 * 4.0);
        assert_eq!(d.scaled_height(), 4.0 * 4.0);
    }

    #[test]
    fn decode_throws() {
        assert_eq!(PatternDecoder::new(&[0, 0]).err(), Some(ERR_TOO_SHORT));
        assert_eq!(PatternDecoder::new(&[1, 0, 0]).err(), Some(ERR_VERSION));
        // Declares 5x4 (3 payload bytes) but carries none.
        assert_eq!(
            PatternDecoder::new(&header(2, 5, 4)[..]).err(),
            Some(ERR_DIMENSIONS)
        );
    }

    #[test]
    fn is_primary_bit_and_bounds() {
        // scale 0, width 2, height 2 -> 4 bits in 1 payload byte.
        let mut b = header(0, 2, 2);
        b.push(0b0000_0010); // bit 1 set -> that cell is NOT primary (===0 false)
        let d = PatternDecoder::new(&b).unwrap();
        // idx = y*2 + x (scale 0). (1,0) -> idx 1 -> bit1 set -> false.
        assert_eq!(d.is_primary(1.0, 0.0), Ok(false));
        // (0,0) -> idx 0 -> bit0 clear -> true.
        assert_eq!(d.is_primary(0.0, 0.0), Ok(true));
        // Small dims: negative modulo keeps `3 + byteIndex` in [2, len) —
        // never throws. (-1,-1) -> idx -3 -> byteIndex -1 -> pos 2 (metadata).
        assert!(d.is_primary(-1.0, -1.0).is_ok());

        // Big dims: width 33, height 33 -> 1089 bits -> 137 payload bytes.
        let mut b = header(0, 33, 33);
        b.extend([0u8; 137]);
        let d = PatternDecoder::new(&b).unwrap();
        // (-32,-32) -> idx = -32*33 - 32 = -1088 -> byteIndex -136 -> pos -133
        // -> out of range -> throw.
        assert_eq!(d.is_primary(-32.0, -32.0), Err(ERR_INVALID_PATTERN));
    }

    #[test]
    fn negative_index_reads_metadata_byte() {
        // A negative shift result can wrap `3 + byteIndex` back into range.
        let mut b = header(0, 8, 2);
        b.extend([0; 2]);
        let d = PatternDecoder::new(&b).unwrap();
        // x = -1 -> px = -1 % 8 = -1; y=0 -> py=0; idx=-1; byteIndex=-1>>3=-1;
        // pos = 2 -> bytes[2] = the header byte2 (valid). No throw.
        assert!(d.is_primary(-1.0, 0.0).is_ok());
    }
}
