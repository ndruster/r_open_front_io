//! Port of `src/server/GameApiCors.ts` (the pure decision pair
//! `isAllowedOrigin` / `applyGameApiCorsHeaders`) and
//! `src/server/NoStoreHeaders.ts` (`setNoStoreHeaders`). The express
//! middleware `gameApiCors` (req/res/next host objects, `sendStatus`) is
//! EXCLUDED.
//!
//! `ServerEnv` rides on the shared `cluster_checkin::EnvTable` trace facade
//! (every read is `[72, method, ...codec(value)]`); `setHeader` is a
//! callback facade traced as `[71, ...codec(name), ...codec(value)]` — the
//! header ORDER and exact strings are the observable.
//!
//! Faithfulness notes (quirk list):
//!
//! * `isAllowedOrigin(origin)` gate chain, in source order: (1) `origin ===
//!   DESKTOP_APP_ORIGIN` — a desktop hit returns with ZERO `ServerEnv` reads
//!   (pinned by the trace); (2) `siteHost !== undefined && origin ===
//!   \`https://${siteHost}\`` — STRICT `!== undefined`: a null siteHost
//!   would pass the gate and template-stringify to `"https://null"`, which
//!   no real Origin equals (the real `siteHost()` returns `string |
//!   undefined`, never null, so the capture only feeds strings/undefined —
//!   the Rust codec models Undef/Absent -> skip the branch; a scripted Null
//!   would ride the template and simply not match); (3) `own = publicHost();
//!   own === undefined -> false` — the undefined host ENDS the chain false
//!   without ever reading pageHostFor; (4) `origin === \`https://${own}\``;
//!   (5) `pageHostFor(own)` — a pageHostFor MISS (undefined) fails the
//!   final `pageHost !== undefined` gate, so the return is false (the same
//!   strict-gate template as (2), null templates to `"https://null"`).
//! * `applyGameApiCorsHeaders`: `setHeader("Vary","Origin")` fires
//!   UNCONDITIONALLY first. `requestOrigin === undefined` short-circuits
//!   BEFORE `isAllowedOrigin` (zero env reads); the empty string `""` IS
//!   defined, so it walks the full gate chain (desktop miss, then every env
//!   read) and ends false — Vary-only. A granted origin gets FOUR more
//!   headers in the verbatim order Access-Control-Allow-Origin (= the
//!   request origin, echoed), Access-Control-Allow-Methods ("GET, POST,
//!   OPTIONS"), Access-Control-Allow-Headers ("Authorization,
//!   Content-Type"), Access-Control-Max-Age ("86400"). Deliberately NO
//!   Access-Control-Allow-Credentials.
//! * `setNoStoreHeaders` (NoStoreHeaders.ts): three setHeader calls in the
//!   verbatim order Cache-Control ("no-store, no-cache, must-revalidate,
//!   proxy-revalidate"), Pragma ("no-cache"), Expires ("0").

use crate::cluster_checkin::EnvTable;
use crate::js_json::{push_str, read_str, read_val, JsVal};

/// `DESKTOP_APP_ORIGIN` — the renderer's fixed custom-scheme origin.
pub const DESKTOP_APP_ORIGIN: &str = "app://openfront";

/// The `https://` + host template (JS template literal, no escaping needed
/// for the ASCII capture domain).
fn https_of(host: &JsVal) -> String {
    format!("https://{}", js_str(host))
}

/// JS string interpolation of a codec value (the template-literal operands
/// are `string | undefined` in the domain; a null would spell "null").
fn js_str(v: &JsVal) -> String {
    match v {
        JsVal::Str(s) => s.clone(),
        JsVal::Null => "null".to_string(),
        JsVal::Undef | JsVal::Absent => "undefined".to_string(),
        JsVal::Num(n) => crate::game_ts::js_num_str(*n),
        JsVal::Bool(b) => b.to_string(),
        _ => "[object Object]".to_string(),
    }
}

/// `isAllowed(origin)` — the five-gate chain, every `ServerEnv` read traced.
pub fn is_allowed_origin(origin: &str, env: &EnvTable, trace: &mut Vec<f64>) -> bool {
    // (1) desktop hit: zero env reads.
    if origin == DESKTOP_APP_ORIGIN {
        return true;
    }
    // (2) `siteHost !== undefined && origin === https://${siteHost}`.
    let site_host = env.read(0, trace);
    if !matches!(site_host, JsVal::Undef | JsVal::Absent) && origin == https_of(&site_host) {
        return true;
    }
    // (3) `own === undefined -> false` (strict; a null passes the gate and
    // templates to "https://null", matching nothing real).
    let own = env.read(1, trace);
    if matches!(own, JsVal::Undef | JsVal::Absent) {
        return false;
    }
    // (4) own game host.
    if origin == https_of(&own) {
        return true;
    }
    // (5) the GAME_DOMAIN page host paired with the own game host.
    let own_s = js_str(&own);
    let page_host = env.read_page_host_for(&own_s, trace);
    !matches!(page_host, JsVal::Undef | JsVal::Absent) && origin == https_of(&page_host)
}

/// `applyGameApiCorsHeaders(requestOrigin, setHeader)` — the traced header
/// sequence: Vary always, then the four grant headers on a hit.
pub fn apply_game_api_cors_headers(
    request_origin: &JsVal,
    env: &EnvTable,
    trace: &mut Vec<f64>,
) {
    let set = |name: &str, value: &str, trace: &mut Vec<f64>| {
        trace.push(71.0);
        push_str(trace, name);
        push_str(trace, value);
    };
    // Set unconditionally: the response differs by Origin.
    set("Vary", "Origin", trace);
    // `requestOrigin === undefined` short-circuits BEFORE isAllowedOrigin.
    if matches!(request_origin, JsVal::Undef | JsVal::Absent) {
        return;
    }
    let origin = js_str(request_origin);
    if !is_allowed_origin(&origin, env, trace) {
        return;
    }
    set("Access-Control-Allow-Origin", &origin, trace);
    set("Access-Control-Allow-Methods", "GET, POST, OPTIONS", trace);
    set("Access-Control-Allow-Headers", "Authorization, Content-Type", trace);
    set("Access-Control-Max-Age", "86400", trace);
}

/// `setNoStoreHeaders(res)` — the three verbatim cache-killer headers.
pub fn set_no_store_headers(trace: &mut Vec<f64>) {
    let set = |name: &str, value: &str, trace: &mut Vec<f64>| {
        trace.push(71.0);
        push_str(trace, name);
        push_str(trace, value);
    };
    set(
        "Cache-Control",
        "no-store, no-cache, must-revalidate, proxy-revalidate",
        trace,
    );
    set("Pragma", "no-cache", trace);
    set("Expires", "0", trace);
}

/// The capture harness: an env table, replaying an op stream. Traced ops
/// (3 isAllowedOrigin, 4 applyCorsHeaders, 5 setNoStoreHeaders) prefix
/// their res with `[traceLen,(trace)*]`.
#[derive(Debug, Default)]
pub struct RigHarness {
    env: EnvTable,
}

impl RigHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Run one op. Kind table (see `tools/gen_vectors.mjs`):
    /// 0 reset -> `[0]`;
    /// 1 scriptEnv `[siteHost, publicHost, n, (host-str, val)*n]` -> `[0]`
    ///   (cors slice: the registeredSite pair + pageHostFor map; the other
    ///   env fields keep the cluster_checkin defaults);
    /// 2 dumpDesktopOrigin -> `[(str)]`;
    /// 3 isAllowedOrigin `[(origin-str)]` -> `[traceLen,(trace)*,0|1]`;
    /// 4 applyCorsHeaders `[(origin val)]` -> `[traceLen,(trace)*,0]` (the
    ///   origin crosses as a codec VALUE: Undef models the absent header);
    /// 5 setNoStoreHeaders -> `[traceLen,(trace)*,0]`.
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut i = 0usize;
        let mut trace: Vec<f64> = Vec::new();
        let payload: Vec<f64> = match kind {
            0 => {
                self.reset();
                vec![0.0]
            }
            1 => {
                let site_host = read_val(args, &mut i);
                let public_host = read_val(args, &mut i);
                let n = args[i] as usize;
                i += 1;
                let mut page_host_for = Vec::with_capacity(n);
                for _ in 0..n {
                    let h = read_str(args, &mut i);
                    let r = read_val(args, &mut i);
                    page_host_for.push((h, r));
                }
                self.env = EnvTable {
                    site_host,
                    public_host,
                    page_host_for,
                    ..EnvTable::default()
                };
                vec![0.0]
            }
            2 => {
                let mut out = Vec::new();
                push_str(&mut out, DESKTOP_APP_ORIGIN);
                out
            }
            3 => {
                let origin = read_str(args, &mut i);
                vec![if is_allowed_origin(&origin, &self.env, &mut trace) {
                    1.0
                } else {
                    0.0
                }]
            }
            4 => {
                let origin = read_val(args, &mut i);
                apply_game_api_cors_headers(&origin, &self.env, &mut trace);
                vec![0.0]
            }
            5 => {
                set_no_store_headers(&mut trace);
                vec![0.0]
            }
            k => unreachable!("game_api_cors harness: unknown op kind {k}"),
        };
        if kind == 3 || kind == 4 || kind == 5 {
            let mut out = Vec::with_capacity(1 + trace.len() + payload.len());
            out.push(trace.len() as f64);
            out.extend(trace.iter().copied());
            out.extend(payload);
            out
        } else {
            payload
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::js_json::push_val;

    fn enc(x: &str) -> Vec<f64> {
        let mut v = vec![x.encode_utf16().count() as f64];
        v.extend(x.encode_utf16().map(|u| u as f64));
        v
    }

    // Count trace events by code. A raw `x == 71` filter would also hit the
    // UTF-16 code unit 'G' (=71) inside "GET, POST, OPTIONS", so events are
    // WALKED structurally: res = [traceLen, (trace)*, payload] and each
    // event is [code, ...payload] with a decodable shape (71 = two
    // length-prefixed strings, 72 = method + optional host string + one
    // codec value, 73 = one bool, 70 = one codec string).
    fn count_events(r: &[f64], code: f64) -> usize {
        let tl = r[0] as usize; // ELEMENTS of the trace, not event count
        let mut n = 0usize;
        let mut i = 1usize;
        while i < 1 + tl {
            let c = r[i] as usize;
            if c as f64 == code {
                n += 1;
            }
            i += 1;
            match c {
                71 => {
                    let l = r[i] as usize;
                    i += 1 + l;
                    let l = r[i] as usize;
                    i += 1 + l;
                }
                72 => {
                    let m = r[i] as usize;
                    i += 1;
                    if m == 6 {
                        let l = r[i] as usize;
                        i += 1 + l;
                    }
                    i += codec_len(r, i);
                }
                70 => i += codec_len(r, i),
                73 => i += 1,
                _ => {}
            }
        }
        n
    }

    /// Length of one encoded codec value starting at `i` (tags: 0 Absent,
    /// 1 Undef, 2 Null, 3 Num, 4 Bool, 5 Str, 6 Obj, 7 Arr).
    fn codec_len(r: &[f64], i: usize) -> usize {
        match r[i] as usize {
            0..=2 => 1,
            3 | 4 => 2,
            5 => 2 + r[i + 1] as usize,
            6 => {
                let n = r[i + 1] as usize;
                let mut j = i + 2;
                for _ in 0..n {
                    let kl = r[j] as usize;
                    j += 1 + kl;
                    j += codec_len(r, j);
                }
                j - i
            }
            7 => {
                let n = r[i + 1] as usize;
                let mut j = i + 2;
                for _ in 0..n {
                    j += codec_len(r, j);
                }
                j - i
            }
            _ => 1,
        }
    }

    fn script_env(site: JsVal, public: JsVal, pages: &[(&str, JsVal)]) -> Vec<f64> {
        let mut a = Vec::new();
        push_val(&mut a, &site);
        push_val(&mut a, &public);
        a.push(pages.len() as f64);
        for (h, r) in pages {
            a.extend(enc(h));
            push_val(&mut a, r);
        }
        a
    }

    #[test]
    fn desktop_hit_zero_env_reads() {
        let mut h = RigHarness::new();
        let r = h.run_op(3, &enc("app://openfront"));
        assert_eq!(r[0], 0.0); // empty trace
        assert_eq!(r[1], 1.0);
    }

    #[test]
    fn gate_chain_orders() {
        let mut h = RigHarness::new();
        // siteHost miss -> publicHost -> pageHostFor: three env events.
        h.run_op(
            1,
            &script_env(
                JsVal::Str("s.io".into()),
                JsVal::Str("g.io".into()),
                &[("g.io", JsVal::Str("p.io".into()))],
            ),
        );
        let r = h.run_op(3, &enc("https://p.io"));
        assert_eq!(r.last().unwrap(), &1.0);
        // own undefined -> false WITHOUT pageHostFor (two env events:
        // siteHost, publicHost).
        h.run_op(1, &script_env(JsVal::Undef, JsVal::Undef, &[]));
        let r = h.run_op(3, &enc("https://x.io"));
        assert_eq!(r.last().unwrap(), &0.0);
        assert_eq!(count_events(&r, 72.0), 2);
    }

    #[test]
    fn cors_headers_sequence() {
        let mut h = RigHarness::new();
        // undefined origin: Vary only, zero env reads.
        let mut a = Vec::new();
        push_val(&mut a, &JsVal::Undef);
        let r = h.run_op(4, &a);
        assert_eq!(count_events(&r, 71.0), 1);
        assert_eq!(count_events(&r, 72.0), 0);
        // empty string: defined -> full chain, false -> Vary only.
        h.run_op(1, &script_env(JsVal::Undef, JsVal::Undef, &[]));
        let mut b = Vec::new();
        push_val(&mut b, &JsVal::Str(String::new()));
        let r = h.run_op(4, &b);
        assert_eq!(count_events(&r, 71.0), 1);
        // granted: Vary + four headers in order.
        h.run_op(1, &script_env(JsVal::Str("s.io".into()), JsVal::Undef, &[]));
        let mut c = Vec::new();
        push_val(&mut c, &JsVal::Str("https://s.io".into()));
        let r = h.run_op(4, &c);
        assert_eq!(count_events(&r, 71.0), 5);
    }

    #[test]
    fn no_store_three_headers() {
        let r = RigHarness::new().run_op(5, &[]);
        assert_eq!(count_events(&r, 71.0), 3);
    }
}
