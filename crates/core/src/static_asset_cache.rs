//! Port of `src/server/StaticAssetCache.ts` — the immutable-asset
//! `Cache-Control` gate.
//!
//! Faithfulness notes (quirk list):
//!
//! * `stripQueryString` is `urlPath.split("?", 1)[0]` — JS `split` with
//!   limit 1 keeps only the first segment: everything before the FIRST `?`
//!   (no `?` -> the whole string, leading `?` -> `""`, several `?` only the
//!   first one cuts).
//! * `getStaticAssetCacheControl` gates on `!urlPath` — a TRUTHY gate, so the
//!   empty string `""` returns `undefined` just like `undefined` does; only a
//!   non-empty string proceeds. The prefix test is `startsWith("/assets/") ||
//!   startsWith("/_assets/")` — case-sensitive, slash required (`/asset/x`
//!   and `/assets` miss).
//! * `applyStaticAssetCacheControl` re-reads through the same truthy gate:
//!   `setHeader` fires only when `cacheControl` is truthy (the constant is
//!   non-empty, so every `Some` calls; `None` skips). The harness models
//!   `setHeader` as a facade trace: each call emits the
//!   `("Cache-Control", IMMUTABLE)` pair, no call emits nothing.

use crate::js_json::{push_str, read_str, JsVal};

/// `IMMUTABLE_CACHE_CONTROL` (private in TS, observable through the return
/// value and the `setHeader` trace).
pub const IMMUTABLE_CACHE_CONTROL: &str = "public, max-age=31536000, immutable";

/// `stripQueryString(urlPath)` — `urlPath.split("?", 1)[0]`.
pub fn strip_query_string(url_path: &str) -> &str {
    match url_path.find('?') {
        Some(i) => &url_path[..i],
        None => url_path,
    }
}

/// `getStaticAssetCacheControl(urlPath)` — the falsy gate plus the two
/// prefixes. `None` models the TS `undefined`.
pub fn get_static_asset_cache_control(url_path: Option<&str>) -> Option<&'static str> {
    let s = match url_path {
        None | Some("") => return None,
        Some(s) => s,
    };
    let normalized = strip_query_string(s);
    if normalized.starts_with("/assets/") || normalized.starts_with("/_assets/") {
        Some(IMMUTABLE_CACHE_CONTROL)
    } else {
        None
    }
}

/// `applyStaticAssetCacheControl(setHeader, urlPath)` — the harness collects
/// the `("Cache-Control", value)` pairs into `trace` (the facade-trace
/// precedent: `creator_code`'s localStorage log).
pub fn apply_static_asset_cache_control(
    url_path: Option<&str>,
    trace: &mut Vec<(&'static str, &'static str)>,
) {
    if let Some(cc) = get_static_asset_cache_control(url_path) {
        trace.push(("Cache-Control", cc));
    }
}

/// Codec presence tag -> the `string | undefined` input domain. `Absent` /
/// `Undef` model `undefined`; a string rides as `Some`. (The falsy gate also
/// catches `""`, handled inside the pure fn.)
fn opt_str(v: &JsVal) -> Option<&str> {
    match v {
        JsVal::Str(s) => Some(s),
        _ => None,
    }
}

// ---------------------------------------------------------------- vectors op
//
// kind 0: [n, (encVal urlPath)*n] -> (0|1)*n
//         getStaticAssetCacheControl batch; 1 = the IMMUTABLE constant,
//         0 = undefined.
// kind 1: [n, (encS urlPath)*n] -> (encS out)*n
//         stripQueryString batch (always a string in, always a string out).
// kind 2: [n, (encVal urlPath)*n] -> the concatenated apply trace:
//         per urlPath, [1, encS("Cache-Control"), encS(IMMUTABLE)] when
//         setHeader fired, [0] when it did not.
// kind 3: [] -> [encS(IMMUTABLE)] the private constant dump.

pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    let mut i = 0usize;
    match kind {
        0 => {
            let n = args[i] as usize;
            i += 1;
            for _ in 0..n {
                let v = crate::js_json::read_val(args, &mut i);
                let hit = get_static_asset_cache_control(opt_str(&v)).is_some();
                out.push(if hit { 1.0 } else { 0.0 });
            }
        }
        1 => {
            let n = args[i] as usize;
            i += 1;
            for _ in 0..n {
                let s = read_str(args, &mut i);
                push_str(&mut out, strip_query_string(&s));
            }
        }
        2 => {
            let n = args[i] as usize;
            i += 1;
            let mut trace = Vec::new();
            for _ in 0..n {
                let v = crate::js_json::read_val(args, &mut i);
                trace.clear();
                apply_static_asset_cache_control(opt_str(&v), &mut trace);
                if trace.is_empty() {
                    out.push(0.0);
                } else {
                    out.push(1.0);
                    push_str(&mut out, "Cache-Control");
                    push_str(&mut out, IMMUTABLE_CACHE_CONTROL);
                }
            }
        }
        3 => push_str(&mut out, IMMUTABLE_CACHE_CONTROL),
        k => unreachable!("static_asset_cache: unknown op kind {k}"),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_query_string_forms() {
        assert_eq!(strip_query_string("/assets/a.js"), "/assets/a.js");
        assert_eq!(strip_query_string("/assets/a.js?v=1"), "/assets/a.js");
        assert_eq!(strip_query_string("?lead"), "");
        assert_eq!(strip_query_string("a?b?c"), "a");
        assert_eq!(strip_query_string(""), "");
        assert_eq!(strip_query_string("?"), "");
    }

    #[test]
    fn falsy_gate_and_prefixes() {
        assert_eq!(get_static_asset_cache_control(None), None);
        assert_eq!(get_static_asset_cache_control(Some("")), None);
        assert_eq!(
            get_static_asset_cache_control(Some("/assets/x")),
            Some(IMMUTABLE_CACHE_CONTROL)
        );
        assert_eq!(
            get_static_asset_cache_control(Some("/_assets/x?v=2")),
            Some(IMMUTABLE_CACHE_CONTROL)
        );
        // One letter off / no trailing slash / case: all miss.
        assert_eq!(get_static_asset_cache_control(Some("/asset/x")), None);
        assert_eq!(get_static_asset_cache_control(Some("/assets")), None);
        assert_eq!(get_static_asset_cache_control(Some("/Assets/x")), None);
        assert_eq!(get_static_asset_cache_control(Some("assets/x")), None);
    }

    #[test]
    fn apply_trace_fires_only_on_hit() {
        let mut t = Vec::new();
        apply_static_asset_cache_control(Some("/assets/a"), &mut t);
        assert_eq!(t, [("Cache-Control", IMMUTABLE_CACHE_CONTROL)]);
        t.clear();
        apply_static_asset_cache_control(Some(""), &mut t);
        assert!(t.is_empty());
        t.clear();
        apply_static_asset_cache_control(None, &mut t);
        assert!(t.is_empty());
    }

    #[test]
    fn run_op_wire_shapes() {
        // kind 3: the constant dump.
        let d = run_op(3, &[]);
        let mut want = Vec::new();
        push_str(&mut want, IMMUTABLE_CACHE_CONTROL);
        assert_eq!(d, want);
        // kind 0: [undefined, "/assets/x"] -> [0, 1].
        let a = run_op(
            0,
            &[
                2.0,
                1.0,
                5.0,
                9.0,
                47.0,
                97.0,
                115.0,
                115.0,
                101.0,
                116.0,
                115.0,
                47.0,
                120.0,
            ],
        );
        assert_eq!(a, [0.0, 1.0]);
    }
}
