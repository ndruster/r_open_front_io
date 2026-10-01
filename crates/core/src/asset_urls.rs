//! Port of `src/core/AssetUrls.ts`.
//!
//! Only the pure path/URL helpers are ported: `normalizeAssetPath`,
//! `encodeAssetPath`, `buildAssetUrl` and the module-private
//! `safeDecodeAssetSegment` / `assertSafeAssetSegment` / `isAbsoluteUrl`.
//! The runtime-environment readers (`getAssetManifest`, `getCdnBase`,
//! `assetUrl`) touch `window` / `globalThis` and are out of scope, as is
//! `rewriteAssetsForCdn` (an HTML template rewrite).
//!
//! Faithfulness notes:
//!
//! * `isAbsoluteUrl` is `/^[a-z][a-z0-9+.-]*:\/\//i`. The `i` flag only
//!   folds ASCII here — V8's canonical case folding reaches `K`/`s`/`i`
//!   variants (`U+212A`, `U+017F`, `U+0131`) but none of them are in the
//!   `[a-z]` class, and the probe confirmed they all fail the match — so
//!   ASCII byte tests are exact. The greedy middle class stops at the first
//!   non-class byte and `:` is not in the class, so no backtracking is
//!   possible and a prefix scan is equivalent to the regex.
//! * `normalizeAssetPath` strips a leading run of `/`, splits on `/`,
//!   drops empty segments, then percent-decodes each survivor
//!   (`decodeURIComponent` inside a `try`, falling back to the raw segment)
//!   and rejects the segment when either the raw or the decoded form is
//!   `.` / `..`. The joined result may still contain `//` or `.` segments
//!   (an encoded `%2F` / `%2E` decodes to a literal delimiter *inside* a
//!   segment, e.g. `a/%2E%2Fb` → `a/./b`), which is why `buildAssetUrl`'s
//!   fallback re-runs `normalizeAssetPath` over the already-normalised path
//!   and can throw on a path `normalizeAssetPath` itself accepted.
//! * `encodeAssetPath` runs `normalizeAssetPath` once on its input, then
//!   splits the *result* and percent-encodes each non-empty segment — it
//!   does not re-check the decoded delimiters, so `encodeAssetPath`
//!   succeeds on `a/%2E%2Fb` (→ `a/./b`) while `buildAssetUrl` throws on
//!   the same path via its `encodeAssetPath(normalizedPath)` fallback.
//! * `buildAssetUrl`'s manifest lookup is exact-key only. JS property
//!   access walks the prototype chain (`manifest["toString"]` finds
//!   `Object.prototype.toString`, a truthy function that would be
//!   stringified into the URL); that quirk is outside the domain, same
//!   exclusion as `server_list`. A manifest hit counts only when the value
//!   is truthy (empty string falls through to the encode branch).
//! * `encodeURIComponent` keeps `A-Za-z0-9 - _ . ! ~ * ' ( )` and emits
//!   uppercase `%XX` per UTF-8 byte. JS throws `URIError` on a lone
//!   surrogate; Rust `String` cannot hold one (the token stream's
//!   `from_utf16_lossy` would already have replaced it), so lone-surrogate
//!   inputs are out of scope and the encode branch never needs the throw.
//! * `decodeURIComponent` mirrors the JS-accurate decoder shared with
//!   `server_list`: malformed escapes, overlong sequences and encoded
//!   surrogates raise `URIError`, modelled as `None` (caller keeps the raw
//!   segment).

use crate::server_list::decode_uri_component;

/// `safeDecodeAssetSegment`: `decodeURIComponent` with the raw segment as
/// the `URIError` fallback.
fn safe_decode_asset_segment(segment: &str) -> String {
    decode_uri_component(segment).unwrap_or_else(|| segment.to_string())
}

/// `assertSafeAssetSegment`: `Err(())` models the thrown
/// `Invalid asset path segment: ${segment}`.
fn assert_safe_asset_segment(segment: &str) -> Result<String, ()> {
    let decoded = safe_decode_asset_segment(segment);
    if segment == "." || segment == ".." || decoded == "." || decoded == ".." {
        return Err(());
    }
    Ok(decoded)
}

/// `normalizeAssetPath(path)`: `Err(())` models a throw (unsafe segment or
/// the empty result).
// `()` is the faithful error carrier: the TS `throw new Error(msg)` has no
// observable payload for the parity surface (the vectors pin only *that* it
// threw), so a custom Error type would add nothing.
#[allow(clippy::result_unit_err)]
pub fn normalize_asset_path(path: &str) -> Result<String, ()> {
    let stripped = path.trim_start_matches('/');
    let mut parts = Vec::new();
    for segment in stripped.split('/') {
        if segment.is_empty() {
            continue;
        }
        parts.push(assert_safe_asset_segment(segment)?);
    }
    let normalized = parts.join("/");
    if normalized.is_empty() {
        return Err(());
    }
    Ok(normalized)
}

/// `encodeAssetPath(path)`: normalise, then re-split (empty segments from
/// decoded slashes are dropped again) and percent-encode each segment.
#[allow(clippy::result_unit_err)]
pub fn encode_asset_path(path: &str) -> Result<String, ()> {
    let normalized = normalize_asset_path(path)?;
    let mut parts = Vec::new();
    for segment in normalized.split('/') {
        if segment.is_empty() {
            continue;
        }
        parts.push(encode_uri_component(segment));
    }
    Ok(parts.join("/"))
}

/// `isAbsoluteUrl(path)` — `/^[a-z][a-z0-9+.-]*:\/\//i`.
pub fn is_absolute_url(path: &str) -> bool {
    let b = path.as_bytes();
    let Some(&first) = b.first() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    let mut i = 1;
    while i < b.len()
        && (b[i].is_ascii_alphanumeric() || matches!(b[i], b'+' | b'.' | b'-'))
    {
        i += 1;
    }
    b.get(i..i + 3) == Some(&b"://"[..])
}

/// `buildAssetUrl(path, assetManifest, baseUrl)`. The manifest rides in as
/// an ordered exact-key record; `Err(())` models a throw from the normalise
/// / re-encode paths.
#[allow(clippy::result_unit_err)]
pub fn build_asset_url(
    path: &str,
    asset_manifest: &[(String, String)],
    base_url: &str,
) -> Result<String, ()> {
    if is_absolute_url(path) {
        return Ok(path.to_string());
    }
    let normalized = normalize_asset_path(path)?;
    let direct = asset_manifest
        .iter()
        .find(|(k, _)| k == &normalized)
        .map(|(_, v)| v);
    if let Some(url) = direct {
        if !url.is_empty() {
            if base_url.is_empty() {
                return Ok(url.clone());
            }
            let base = base_url.trim_end_matches('/');
            return Ok(format!("{base}{url}"));
        }
    }
    Ok(format!("/{}", encode_asset_path(&normalized)?))
}

// ---------------------------------------------------------------- vectors op
//
// A flat `f64` token runner shared by the golden replay and the wasm probe.
// Strings cross as `[len, u0, .. u(len-1)]` (UTF-16 code units); the
// manifest is `[n, (key, value) * n]`. A result is `[0, len, u0, ..]` on
// success or `[1]` when the TS function throws.

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
    fn string(&mut self) -> String {
        let len = self.u();
        let units: Vec<u16> = (0..len).map(|_| self.f() as u16).collect();
        String::from_utf16_lossy(&units)
    }
}

fn push_string(out: &mut Vec<f64>, s: &str) {
    let units: Vec<u16> = s.encode_utf16().collect();
    out.push(units.len() as f64);
    out.extend(units.iter().map(|&u| u as f64));
}

fn push_result(out: &mut Vec<f64>, r: Option<&str>) {
    match r {
        Some(v) => {
            out.push(0.0);
            push_string(out, v);
        }
        None => out.push(1.0),
    }
}

/// `kind`: 0 normalizeAssetPath `[path]`, 1 encodeAssetPath `[path]`,
/// 2 buildAssetUrl `[path, n, (key, value)*n, baseUrl]`.
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut c = Cur(args, 0);
    let mut out = Vec::new();
    match kind {
        0 => {
            let p = c.string();
            push_result(&mut out, normalize_asset_path(&p).ok().as_deref());
        }
        1 => {
            let p = c.string();
            push_result(&mut out, encode_asset_path(&p).ok().as_deref());
        }
        _ => {
            let p = c.string();
            let n = c.u();
            let manifest: Vec<(String, String)> =
                (0..n).map(|_| (c.string(), c.string())).collect();
            let base = c.string();
            push_result(
                &mut out,
                build_asset_url(&p, &manifest, &base).ok().as_deref(),
            );
        }
    }
    out
}

/// `encodeURIComponent` with JS semantics over a well-formed UTF-8 string:
/// the unreserved set passes through, every other byte becomes uppercase
/// `%XX` of its UTF-8 encoding.
fn encode_uri_component(s: &str) -> String {
    let unreserved = |b: u8| {
        b.is_ascii_alphanumeric()
            || matches!(b, b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')')
    };
    let mut out = String::new();
    for &b in s.as_bytes() {
        if unreserved(b) {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(char::from_digit((b >> 4) as u32, 16).unwrap().to_ascii_uppercase());
            out.push(char::from_digit((b & 0xF) as u32, 16).unwrap().to_ascii_uppercase());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_basics() {
        assert_eq!(normalize_asset_path("/flags/US.svg").as_deref(), Ok("flags/US.svg"));
        assert_eq!(normalize_asset_path("///a//b/").as_deref(), Ok("a/b"));
        assert!(normalize_asset_path("").is_err());
        assert!(normalize_asset_path("/").is_err());
        assert!(normalize_asset_path("a/../b").is_err());
        assert!(normalize_asset_path("%2e%2e/x").is_err());
        assert!(normalize_asset_path("%2E/x").is_err());
        assert_eq!(normalize_asset_path("a/%2Fb").as_deref(), Ok("a//b"));
        assert_eq!(normalize_asset_path("caf%C3%A9").as_deref(), Ok("café"));
        assert_eq!(normalize_asset_path("a%20b").as_deref(), Ok("a b"));
        assert_eq!(normalize_asset_path("%zz").as_deref(), Ok("%zz"));
        assert_eq!(normalize_asset_path("%").as_deref(), Ok("%"));
        assert_eq!(normalize_asset_path("app://openfront/x").as_deref(), Ok("app:/openfront/x"));
        assert_eq!(normalize_asset_path("a/%2E%2Fb").as_deref(), Ok("a/./b"));
    }

    #[test]
    fn encode_basics() {
        assert_eq!(encode_asset_path("/a b/c").as_deref(), Ok("a%20b/c"));
        assert_eq!(encode_asset_path("a/%2Fb").as_deref(), Ok("a/b"));
        assert_eq!(encode_asset_path("café").as_deref(), Ok("caf%C3%A9"));
        assert_eq!(encode_asset_path("a%20b").as_deref(), Ok("a%20b"));
        assert_eq!(encode_asset_path("😀x").as_deref(), Ok("%F0%9F%98%80x"));
        assert!(encode_asset_path("a/..").is_err());
        assert_eq!(encode_asset_path("a/%2E%2Fb").as_deref(), Ok("a/./b"));
        assert_eq!(encode_asset_path("a!'()*~-_.b ").as_deref(), Ok("a!'()*~-_.b%20"));
        assert_eq!(encode_asset_path("a%26b").as_deref(), Ok("a%26b"));
    }

    #[test]
    fn absolute_url_regex() {
        assert!(is_absolute_url("app://openfront/x"));
        assert!(is_absolute_url("HTTP://x"));
        assert!(is_absolute_url("a+b://x"));
        assert!(is_absolute_url("a.-+://x"));
        assert!(!is_absolute_url("1a://x"));
        assert!(!is_absolute_url("a:x//y"));
        assert!(!is_absolute_url("://x"));
        assert!(!is_absolute_url("\u{212A}://x"));
        assert!(!is_absolute_url("\u{017F}://x"));
        assert!(!is_absolute_url("a\u{0131}://x"));
        assert!(!is_absolute_url(""));
    }

    #[test]
    fn build_url_branches() {
        let m = |pairs: &[(&str, &str)]| -> Vec<(String, String)> {
            pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
        };
        assert_eq!(
            build_asset_url("app://openfront/_assets/flags/US.svg", &m(&[]), "https://cdn/").as_deref(),
            Ok("app://openfront/_assets/flags/US.svg")
        );
        assert_eq!(
            build_asset_url("a/b", &m(&[("a/b", "https://cdn/z.svg")]), "https://cdn/").as_deref(),
            Ok("https://cdnhttps://cdn/z.svg")
        );
        assert_eq!(
            build_asset_url("a/b", &m(&[("a/b", "z.svg")]), "").as_deref(),
            Ok("z.svg")
        );
        assert_eq!(
            build_asset_url("a/b", &m(&[("a/b", "")]), "").as_deref(),
            Ok("/a/b")
        );
        assert_eq!(
            build_asset_url("a/b", &m(&[("a/c", "z")]), "").as_deref(),
            Ok("/a/b")
        );
        assert!(build_asset_url("", &m(&[]), "").is_err());
        assert!(build_asset_url("a/%2E%2Fb", &m(&[]), "").is_err());
        assert_eq!(
            build_asset_url("a/%2E%2Fb", &m(&[("a/./b", "hit")]), "").as_deref(),
            Ok("hit")
        );
        assert_eq!(
            build_asset_url("1a://x", &m(&[]), "").as_deref(),
            Ok("/1a%3A/x")
        );
        assert_eq!(build_asset_url("://x", &m(&[]), "").as_deref(), Ok("/%3A/x"));
        assert_eq!(build_asset_url("a/%2Fb", &m(&[]), "").as_deref(), Ok("/a/b"));
    }

    #[test]
    fn run_op_token_streams() {
        let s = |v: &str| -> Vec<f64> {
            let units: Vec<u16> = v.encode_utf16().collect();
            let mut t = vec![units.len() as f64];
            t.extend(units.iter().map(|&u| u as f64));
            t
        };
        let mut args = s("/a b/c");
        assert_eq!(run_op(0, &args), { let mut r = vec![0.0]; r.extend(s("a b/c")); r });
        args = s("/a b/c");
        assert_eq!(run_op(1, &args), { let mut r = vec![0.0]; r.extend(s("a%20b/c")); r });
        let mut args = s("a/b");
        args.extend([2.0]);
        args.extend(s("a/b"));
        args.extend(s("z.svg"));
        args.extend(s("x"));
        args.extend(s("y"));
        args.extend(s(""));
        assert_eq!(run_op(2, &args), { let mut r = vec![0.0]; r.extend(s("z.svg")); r });
    }
}
