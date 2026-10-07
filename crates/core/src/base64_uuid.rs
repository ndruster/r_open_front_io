//! Port of `src/core/Base64.ts` — `uuidToBase64url` / `base64urlToUuid`.
//! The `jose` `base64url` codec is NOT in the capture graph; its exact
//! behaviour was pinned against the real jose 6.2.3 package (the version
//! `package-lock.json` locks) and against Node's native
//! `Uint8Array.fromBase64/toString("base64url")` path that jose delegates to
//! on this runtime: 200k fuzzed strings over the full ASCII alphabet plus
//! whitespace / junk, and an exhaustive sweep of {A,B,=,space,tab} up to
//! length 5, all with zero value/throw-domain divergence from the inlined
//! JS shim the golden capture runs.
//!
//! Faithfulness notes (quirk list):
//!
//! * `uuidToBase64url`: `uuid.replace(/-/g,"")` removes ASCII hyphen (U+002D)
//!   code units only, then SIXTEEN fixed slots read `hex.slice(i*2, i*2+2)` —
//!   a JS slice clamps (a short hex yields 1-unit or EMPTY slices, never an
//!   error). Each slice goes through `parseInt(s, 16)`: leading/trailing
//!   `StrWhiteSpace` trimmed from BOTH ends, optional sign, optional `0x`/`0X`
//!   prefix (radix 16 — `"0x"` alone strips the prefix, finds no digit and is
//!   `NaN`, while `"0z"` truncates after the digit and is `0`), then the
//!   longest valid hex-digit prefix (`"1g"` -> 1, `"z1"` -> NaN). U+000B IS
//!   parseInt whitespace but U+180E / U+0000 are NOT (pinned against V8).
//!   `NaN` / `-0` / out-of-range results are stored through the `ToUint8`
//!   typed-array contract (`jsnum::to_uint8`), so a missing pair is byte 0.
//!   The 16 bytes are encoded with the URL-safe alphabet (`-` / `_`) and NO
//!   padding (`omitPadding`).
//! * `base64urlToUuid`: `base64url.decode` is the WHATWG forgiving-base64
//!   state machine (the `Uint8Array.fromBase64({alphabet:"base64url"})`
//!   semantics jose 6.2.3 delegates to): strip ASCII whitespace
//!   {09,0A,0C,0D,20} (NOT 0B), reject `len % 4 === 1`, reject `+`, `/` and
//!   every other non-alphabet char, accept `=` ONLY as the final one or two
//!   chars in their chunk slot (`"AAA="` -> 2 bytes, `"AA=="` -> 1, `"AAAA="`
//!   / `"AB=A"` / `"AA=A "` -> throw; a `=`-pair must be complete: `"AA=A"`
//!   throws). A throw is the `[1]` status token (the TS `TypeError` has no
//!   observable payload). The decoded byte length is NOT validated against
//!   16: a short decode yields a short hex, the five `slice` positions clamp
//!   to empty segments, and the empty segments still join with dashes —
//!   `""` -> `"----"`, `"AA"` -> `"00----"`, `"AAAA"` -> `"000000----"`.
//!   `b.toString(16).padStart(2,"0")` over 0..=255 is exactly the two-digit
//!   lowercase hex.

use crate::js_json::{push_str, read_str};
use crate::jsnum::to_uint8;

/// The URL-safe alphabet (jose's `base64url.encode`: `-` / `_`, no padding).
const B64URL: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// JS `StrWhiteSpace` (WhiteSpace ∪ LineTerminator) over UTF-16 code units —
/// the set `parseInt` trims from both ends.
fn is_str_whitespace(u: u16) -> bool {
    matches!(
        u,
        0x0009
            | 0x000A
            | 0x000B
            | 0x000C
            | 0x000D
            | 0x0020
            | 0x00A0
            | 0x1680
            | 0x2000..=0x200A
            | 0x2028
            | 0x2029
            | 0x202F
            | 0x205F
            | 0x3000
            | 0xFEFF
    )
}

/// `parseInt(s, 16)` over a UTF-16 code-unit slice: trim both ends, optional
/// sign, optional `0x` / `0X` prefix, longest valid hex-digit prefix; no
/// digits -> `NaN`. `"-0"` is `-0` (the IEEE negative zero, which `ToUint8`
/// then stores as 0).
pub fn parse_int_radix16(units: &[u16]) -> f64 {
    let mut lo = 0usize;
    let mut hi = units.len();
    while lo < hi && is_str_whitespace(units[lo]) {
        lo += 1;
    }
    while hi > lo && is_str_whitespace(units[hi - 1]) {
        hi -= 1;
    }
    let s = &units[lo..hi];
    if s.is_empty() {
        return f64::NAN;
    }
    let mut i = 0usize;
    let mut neg = false;
    if s[0] == 0x2D {
        neg = true;
        i = 1;
    } else if s[0] == 0x2B {
        i = 1;
    }
    // radix-16 `0x` prefix: stripped unconditionally, even when no digit
    // follows ("0x" -> NaN, not 0).
    if i + 1 < s.len() && s[i] == 0x30 && (s[i + 1] == 0x78 || s[i + 1] == 0x58) {
        i += 2;
    }
    let digits_at = i;
    let mut val = 0.0f64;
    while i < s.len() {
        let d = match s[i] {
            0x30..=0x39 => (s[i] - 0x30) as f64,
            0x41..=0x46 => (s[i] - 0x41) as f64 + 10.0,
            0x61..=0x66 => (s[i] - 0x61) as f64 + 10.0,
            _ => break,
        };
        val = val * 16.0 + d;
        i += 1;
    }
    if i == digits_at {
        return f64::NAN;
    }
    if neg {
        -val
    } else {
        val
    }
}

/// jose `base64url.encode` (URL-safe alphabet, `omitPadding`).
pub fn base64url_encode(bytes: &[u8]) -> String {
    let mut s = String::new();
    let mut i = 0usize;
    while i + 3 <= bytes.len() {
        let n =
            ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8) | bytes[i + 2] as u32;
        s.push(B64URL[((n >> 18) & 63) as usize] as char);
        s.push(B64URL[((n >> 12) & 63) as usize] as char);
        s.push(B64URL[((n >> 6) & 63) as usize] as char);
        s.push(B64URL[(n & 63) as usize] as char);
        i += 3;
    }
    match bytes.len() - i {
        1 => {
            let n = (bytes[i] as u32) << 16;
            s.push(B64URL[((n >> 18) & 63) as usize] as char);
            s.push(B64URL[((n >> 12) & 63) as usize] as char);
        }
        2 => {
            let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8);
            s.push(B64URL[((n >> 18) & 63) as usize] as char);
            s.push(B64URL[((n >> 12) & 63) as usize] as char);
            s.push(B64URL[((n >> 6) & 63) as usize] as char);
        }
        _ => {}
    }
    s
}

/// jose `base64url.decode` — the WHATWG forgiving-base64 state machine over
/// the base64url alphabet. `Err(())` models the thrown
/// `The input to be decoded is not correctly encoded.`.
#[allow(clippy::result_unit_err)] // () is the faithful error carrier (asset_urls precedent)
pub fn base64url_decode(input: &str) -> Result<Vec<u8>, ()> {
    let s: Vec<u16> = input
        .encode_utf16()
        .filter(|c| !matches!(c, 0x0009 | 0x000A | 0x000C | 0x000D | 0x0020))
        .collect();
    let len = s.len();
    if len % 4 == 1 {
        return Err(());
    }
    let mut out = Vec::new();
    let mut chunk: u32 = 0;
    let mut bits: u32 = 0;
    let mut pos = 0usize;
    while pos < len {
        let c = s[pos];
        if c == 0x3D {
            // `=` only as the final two (pos%4==2) or final one (pos%3==3)
            // chars of the last chunk; a lone `=` at pos%4==2 must be
            // followed by the second `=`.
            if pos % 4 == 2 && pos == len - 2 {
                if s[pos + 1] != 0x3D {
                    return Err(());
                }
                break;
            }
            if pos % 4 == 3 && pos == len - 1 {
                break;
            }
            return Err(());
        }
        let v = match c {
            0x41..=0x5A => (c - 0x41) as u32,
            0x61..=0x7A => (c - 0x61) as u32 + 26,
            0x30..=0x39 => (c - 0x30) as u32 + 52,
            0x2D => 62,
            0x5F => 63,
            _ => return Err(()),
        };
        chunk = (chunk << 6) | v;
        bits += 6;
        if bits == 24 {
            out.push((chunk >> 16) as u8);
            out.push((chunk >> 8) as u8);
            out.push(chunk as u8);
            chunk = 0;
            bits = 0;
        }
        pos += 1;
    }
    match bits {
        0 => {}
        12 => out.push((chunk >> 4) as u8),
        18 => {
            out.push((chunk >> 10) as u8);
            out.push((chunk >> 2) as u8);
        }
        // 6 leftover bits cannot happen (len % 4 === 1 was rejected).
        _ => return Err(()),
    }
    Ok(out)
}

/// `uuidToBase64url(uuid)`.
pub fn uuid_to_base64url(uuid: &str) -> String {
    let hex: Vec<u16> = uuid.encode_utf16().filter(|u| *u != 0x2D).collect();
    let mut bytes = [0u8; 16];
    #[allow(clippy::needless_range_loop)] // verbatim TS transcription (util precedent)
    for i in 0..16 {
        // JS slice clamps: a short hex yields 1-unit / empty slices.
        let start = (i * 2).min(hex.len());
        let end = (i * 2 + 2).min(hex.len());
        bytes[i] = to_uint8(parse_int_radix16(&hex[start..end]));
    }
    base64url_encode(&bytes)
}

/// `base64urlToUuid(encoded)` — `Err(())` models the decode throw.
#[allow(clippy::result_unit_err)]
pub fn base64url_to_uuid(encoded: &str) -> Result<String, ()> {
    let bytes = base64url_decode(encoded)?;
    let mut hex = String::new();
    for b in bytes {
        hex.push_str(&format!("{b:02x}"));
    }
    let h = hex.as_bytes();
    // The five JS slices clamp independently; short hex still joins the
    // empty tail segments with dashes.
    let seg = |a: usize, b: usize| -> String {
        let x = a.min(h.len());
        let y = b.min(h.len());
        if y <= x {
            String::new()
        } else {
            String::from_utf8_lossy(&h[x..y]).into_owned()
        }
    };
    Ok([
        seg(0, 8),
        seg(8, 12),
        seg(12, 16),
        seg(16, 20),
        seg(20, usize::MAX),
    ]
    .join("-"))
}

// ---------------------------------------------------------------- vectors op
//
// kind 0: [n, (encS uuid)*n] -> (encS base64url)*n — `uuidToBase64url`
//         batch (never throws).
// kind 1: [n, (encS encoded)*n] -> ([0, encS uuid] | [1])*n —
//         `base64urlToUuid` batch; `[1]` models the decode throw.

pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    let mut i = 0usize;
    match kind {
        0 => {
            let n = args[i] as usize;
            i += 1;
            for _ in 0..n {
                let u = read_str(args, &mut i);
                push_str(&mut out, &uuid_to_base64url(&u));
            }
        }
        1 => {
            let n = args[i] as usize;
            i += 1;
            for _ in 0..n {
                let e = read_str(args, &mut i);
                match base64url_to_uuid(&e) {
                    Ok(v) => {
                        out.push(0.0);
                        push_str(&mut out, &v);
                    }
                    Err(()) => out.push(1.0),
                }
            }
        }
        k => unreachable!("base64_uuid: unknown op kind {k}"),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_roundtrip() {
        let u = "123e4567-e89b-12d3-a456-426614174000";
        let e = uuid_to_base64url(u);
        assert_eq!(e, "Ej5FZ-ibEtOkVkJmFBdAAA");
        assert_eq!(base64url_to_uuid(&e).unwrap(), u);
    }

    #[test]
    fn dash_insensitive_and_case() {
        // Dashes are stripped, so any dash placement (even leading /
        // trailing / doubled) gives the same bytes; hex digits are case
        // insensitive through parseInt.
        let a = uuid_to_base64url("123e4567e89b12d3a456426614174000");
        let b = uuid_to_base64url("-1-2-3-e--4567e89b12d3a456426614174000");
        assert_eq!(a, b);
        assert_eq!(
            uuid_to_base64url("123E4567-E89B-12D3-A456-426614174000"),
            a
        );
    }

    #[test]
    fn non_hex_slices_truncate_or_zero() {
        // "1g" -> 1; "zz" -> NaN -> 0; "0x" -> NaN -> 0; "0z" -> 0.
        assert_eq!(uuid_to_base64url(""), uuid_to_base64url("zzzz"));
        assert_eq!(uuid_to_base64url("0x000000-0000-0000-0000-000000000000"), "AAAAAAAAAAAAAAAAAAAAAA");
        let one = uuid_to_base64url("1g000000-0000-0000-0000-000000000000");
        assert_eq!(base64url_to_uuid(&one).unwrap(), "01000000-0000-0000-0000-000000000000");
    }

    #[test]
    fn short_uuid_zero_fills() {
        // "abc" -> bytes [171, 12, 0*14].
        let e = uuid_to_base64url("abc");
        let bytes = base64url_decode(&e).unwrap();
        assert_eq!(bytes, vec![171, 12, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn decode_throw_domain() {
        for bad in ["A", "AAAA=", "AA=A", "AB=A", "!!!!", "+", "/", "é", "AA\u{b}", "\u{a0}AA", "A=", "===="] {
            assert!(base64url_to_uuid(bad).is_err(), "{bad:?} must throw");
        }
        assert_eq!(base64url_to_uuid("").unwrap(), "----");
        assert_eq!(base64url_to_uuid("AA").unwrap(), "00----");
        assert_eq!(base64url_to_uuid("AAAA").unwrap(), "000000----");
        assert_eq!(base64url_to_uuid("AAA=").unwrap(), "0000----");
        assert_eq!(base64url_to_uuid("AA==").unwrap(), "00----");
    }

    #[test]
    fn parse_int_edges() {
        assert!(parse_int_radix16(&[]).is_nan());
        assert!(parse_int_radix16(&[b'z' as u16]).is_nan());
        assert!(parse_int_radix16(&[b'0' as u16, b'x' as u16]).is_nan());
        assert_eq!(parse_int_radix16(&[b'0' as u16, b'z' as u16]), 0.0);
        assert_eq!(parse_int_radix16(&[b'1' as u16, b'g' as u16]), 1.0);
        assert!(parse_int_radix16(&[0x2D, b'0' as u16]).is_sign_negative()); // -0
        assert_eq!(parse_int_radix16(&[0x0B, b'1' as u16]), 1.0); // VT is parseInt ws
        assert_eq!(parse_int_radix16(&[b'f' as u16, b'f' as u16]), 255.0);
    }
}
