//! Port of `src/client/CreatorCode.ts` — the `/c/<code>` share-link stash /
//! resume flow over a scripted localStorage + `window.location` +
//! `history.replaceState` + `Date.now()` facade.
//!
//! Faithfulness notes (quirk list):
//!
//! * `normalizeCreatorCodeInput`: JS `trim` (the WhiteSpace ∪ LineTerminator
//!   set — U+0085 NEL is NOT trimmed, U+FEFF IS; Rust's `str::trim` differs
//!   on both, so [`js_trim`] transcribes the JS set), then `toUpperCase`
//!   (length-changing full mappings: `ß`→`SS`, `ﬀ`→`FF`, `ﬅ`→`ST`), then the
//!   explicit `/^[A-Z0-9_-]{3,22}$/` test on the UPPERCASED candidate.
//! * `parseCreatorCodePath`: `decodeURIComponent` throws on a malformed
//!   escape → fall back to the RAW pathname ([`decode_uri_component`]
//!   `None` models the throw); the `/^\/c\/([^/]+)$/` exec is transcribed
//!   (greedy `[^/]+` eats any trailing `\n`, so `$` degenerates to
//!   end-of-input).
//! * `takePendingCreatorCode`: `raw === null` STRICT gate BEFORE the
//!   consume-style `removeItem` (which runs before the parse);
//!   `typeof parsed === "object" && parsed !== null` lets ARRAYS through
//!   (their absent `stashedAt` then expires); the TTL gate is STRICT `>`
//!   (an entry exactly at the TTL survives); a non-string `code` falls
//!   through to null; a JSON.parse throw (legacy raw string / malformed)
//!   falls through to null.
//! * `stashPendingCreatorCode`: `JSON.stringify({code, stashedAt})` — the
//!   key order is code, stashedAt; `Date.now()` pops the scripted FIFO.
//! * `consumeCreatorCodePath`: reads pathname FIRST; a null segment returns
//!   WITHOUT touching search/hash/history; an invalid code still strips the
//!   path (no stash, but the replaceState runs); the new URL is
//!   `"/" + search + hash` (search/hash carry their own `?`/`#` or are "").
//! * `resumePendingCreatorCode`: `open` is a black-box traced callback; the
//!   return is the boolean.

use crate::js_json::{json_stringify, push_str, push_val, read_str, read_val, JsVal, JsonValue};
use crate::server_list::decode_uri_component;

/// `PENDING_CREATOR_CODE_KEY`.
pub const PENDING_CREATOR_CODE_KEY: &str = "creator-code-pending";
/// `PENDING_CREATOR_CODE_TTL_MS`.
pub const PENDING_CREATOR_CODE_TTL_MS: f64 = 7.0 * 24.0 * 60.0 * 60.0 * 1000.0;

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

/// JS `String.prototype.toUpperCase` over the well-formed subset (the same
/// model every other string port uses: lone surrogates arrive U+FFFD and
/// fail the charset gate either way).
fn js_to_uppercase(s: &str) -> String {
    s.to_uppercase()
}

/// `/^[A-Z0-9_-]{3,22}$/` over the uppercased candidate: explicit charset,
/// length in UTF-16 units (the charset is ASCII-only, so chars == units on
/// the pass path).
fn code_pattern_test(s: &str) -> bool {
    let units = s.encode_utf16().count();
    (3..=22).contains(&units)
        && s.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

/// `normalizeCreatorCodeInput(raw)`.
pub fn normalize_creator_code_input(raw: &str) -> Option<String> {
    let candidate = js_to_uppercase(js_trim(raw));
    if code_pattern_test(&candidate) {
        Some(candidate)
    } else {
        None
    }
}

/// `/^\/c\/([^/]+)$/` exec: the greedy `[^/]+` consumes every non-slash
/// character (including any `\n`) to the end, so `$` is end-of-input.
fn match_c_path(s: &str) -> Option<String> {
    let rest = s.strip_prefix("/c/")?;
    if rest.is_empty() || rest.contains('/') {
        return None;
    }
    Some(rest.to_string())
}

/// `parseCreatorCodePath(pathname)`.
pub fn parse_creator_code_path(pathname: &str) -> Option<String> {
    let decoded = decode_uri_component(pathname).unwrap_or_else(|| pathname.to_string());
    match_c_path(&decoded)
}

/// A JS-value domain the `JSON.parse` subset can produce (the gates only
/// inspect `typeof` / the two fields, so numbers stay f64 and everything
/// else is structural).
#[derive(Debug, Clone, PartialEq)]
enum Parsed {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Parsed>),
    Obj(Vec<(String, Parsed)>),
}

impl Parsed {
    fn typeof_str(&self) -> &'static str {
        match self {
            Parsed::Null => "object", // typeof null is "object" (the gate adds !== null)
            Parsed::Bool(_) => "boolean",
            Parsed::Num(_) => "number",
            Parsed::Str(_) => "string",
            Parsed::Arr(_) | Parsed::Obj(_) => "object",
        }
    }
    /// `candidate.stashedAt` / `candidate.code` on a non-object yields
    /// `undefined` (modelled as `None`).
    fn field(&self, key: &str) -> Option<&Parsed> {
        match self {
            Parsed::Obj(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

/// The `JSON.parse` subset: strict JSON grammar, `None` models the throw.
fn json_parse(s: &str) -> Option<Parsed> {
    let b = s.as_bytes();
    let mut i = 0usize;
    fn ws(b: &[u8], i: &mut usize) {
        while *i < b.len() && matches!(b[*i], b' ' | b'\t' | b'\n' | b'\r') {
            *i += 1;
        }
    }
    fn lit(b: &[u8], i: &mut usize, word: &str) -> bool {
        if b.len() - *i >= word.len() && &b[*i..*i + word.len()] == word.as_bytes() {
            *i += word.len();
            true
        } else {
            false
        }
    }
    fn parse_value(b: &[u8], i: &mut usize) -> Option<Parsed> {
        ws(b, i);
        if *i >= b.len() {
            return None;
        }
        match b[*i] {
            b'n' => {
                if lit(b, i, "null") {
                    Some(Parsed::Null)
                } else {
                    None
                }
            }
            b't' => {
                if lit(b, i, "true") {
                    Some(Parsed::Bool(true))
                } else {
                    None
                }
            }
            b'f' => {
                if lit(b, i, "false") {
                    Some(Parsed::Bool(false))
                } else {
                    None
                }
            }
            b'"' => parse_string(b, i).map(Parsed::Str),
            b'[' => {
                *i += 1;
                let mut items = Vec::new();
                ws(b, i);
                if *i < b.len() && b[*i] == b']' {
                    *i += 1;
                    return Some(Parsed::Arr(items));
                }
                loop {
                    items.push(parse_value(b, i)?);
                    ws(b, i);
                    if *i >= b.len() {
                        return None;
                    }
                    match b[*i] {
                        b',' => *i += 1,
                        b']' => {
                            *i += 1;
                            return Some(Parsed::Arr(items));
                        }
                        _ => return None,
                    }
                }
            }
            b'{' => {
                *i += 1;
                let mut fields = Vec::new();
                ws(b, i);
                if *i < b.len() && b[*i] == b'}' {
                    *i += 1;
                    return Some(Parsed::Obj(fields));
                }
                loop {
                    ws(b, i);
                    let key = parse_string(b, i)?;
                    ws(b, i);
                    if *i >= b.len() || b[*i] != b':' {
                        return None;
                    }
                    *i += 1;
                    let value = parse_value(b, i)?;
                    // JS object key order: a duplicate key OVERWRITES in
                    // place (the first position survives).
                    if let Some(slot) = fields.iter_mut().find(|(k, _)| *k == key) {
                        slot.1 = value;
                    } else {
                        fields.push((key, value));
                    }
                    ws(b, i);
                    if *i >= b.len() {
                        return None;
                    }
                    match b[*i] {
                        b',' => *i += 1,
                        b'}' => {
                            *i += 1;
                            return Some(Parsed::Obj(fields));
                        }
                        _ => return None,
                    }
                }
            }
            _ => parse_number(b, i),
        }
    }
    fn parse_string(b: &[u8], i: &mut usize) -> Option<String> {
        if *i >= b.len() || b[*i] != b'"' {
            return None;
        }
        *i += 1;
        let mut out = Vec::<u8>::new();
        while *i < b.len() {
            let c = b[*i];
            if c == b'"' {
                *i += 1;
                return String::from_utf8(out).ok();
            }
            if c == b'\\' {
                *i += 1;
                if *i >= b.len() {
                    return None;
                }
                match b[*i] {
                    b'"' => out.push(b'"'),
                    b'\\' => out.push(b'\\'),
                    b'/' => out.push(b'/'),
                    b'b' => out.push(0x08),
                    b'f' => out.push(0x0C),
                    b'n' => out.push(b'\n'),
                    b'r' => out.push(b'\r'),
                    b't' => out.push(b'\t'),
                    b'u' => {
                        let cp = hex4(b, *i + 1)?;
                        *i += 4;
                        // Surrogates: re-encode the code point as UTF-8
                        // (a lone surrogate arrives lossy, same model as the
                        // harness codec; the gates never see it).
                        let ch = char::from_u32(cp).unwrap_or('\u{fffd}');
                        let mut tmp = [0u8; 4];
                        out.extend_from_slice(ch.encode_utf8(&mut tmp).as_bytes());
                    }
                    _ => return None,
                }
                *i += 1;
            } else {
                out.push(c);
                *i += 1;
            }
        }
        None
    }
    fn hex4(b: &[u8], p: usize) -> Option<u32> {
        if p + 4 > b.len() {
            return None;
        }
        let mut v = 0u32;
        for k in 0..4 {
            let d = (b[p + k] as char).to_digit(16)?;
            v = v * 16 + d;
        }
        Some(v)
    }
    fn parse_number(b: &[u8], i: &mut usize) -> Option<Parsed> {
        let start = *i;
        if *i < b.len() && b[*i] == b'-' {
            *i += 1;
        }
        if *i < b.len() && b[*i] == b'0' {
            *i += 1;
            if *i < b.len() && b[*i].is_ascii_digit() {
                return None; // leading zero
            }
        } else {
            let d0 = *i;
            while *i < b.len() && b[*i].is_ascii_digit() {
                *i += 1;
            }
            if *i == d0 {
                return None;
            }
        }
        if *i < b.len() && b[*i] == b'.' {
            *i += 1;
            let d0 = *i;
            while *i < b.len() && b[*i].is_ascii_digit() {
                *i += 1;
            }
            if *i == d0 {
                return None;
            }
        }
        if *i < b.len() && (b[*i] == b'e' || b[*i] == b'E') {
            *i += 1;
            if *i < b.len() && (b[*i] == b'+' || b[*i] == b'-') {
                *i += 1;
            }
            let d0 = *i;
            while *i < b.len() && b[*i].is_ascii_digit() {
                *i += 1;
            }
            if *i == d0 {
                return None;
            }
        }
        let text = std::str::from_utf8(&b[start..*i]).ok()?;
        text.parse::<f64>().ok().map(Parsed::Num)
    }
    let v = parse_value(b, &mut i)?;
    ws(b, &mut i);
    if i != b.len() {
        return None; // trailing garbage
    }
    Some(v)
}

/// The capture harness: scripted localStorage + location + history +
/// Date.now FIFO, all facade calls traced into the res stream. `storage`
/// models the getItem domain (string | null): `None` = absent key.
#[derive(Debug, Default)]
pub struct RigHarness {
    storage: Option<String>,
    now_queue: Vec<f64>,
    pathname: JsVal,
    search: JsVal,
    hash: JsVal,
}

impl RigHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn pop_now(&mut self, trace: &mut Vec<f64>) -> f64 {
        let v = self.now_queue.remove(0);
        trace.push(81.0);
        trace.push(v);
        v
    }

    /// `stashPendingCreatorCode(code)` — returns the traced events.
    fn stash(&mut self, code: &str, trace: &mut Vec<f64>) {
        let now = self.pop_now(trace);
        let value = json_stringify(&JsonValue::Obj(vec![
            ("code".to_string(), JsonValue::Str(code.to_string())),
            ("stashedAt".to_string(), JsonValue::Num(now)),
        ]))
        .expect("object always stringifies");
        trace.push(75.0);
        push_str(trace, PENDING_CREATOR_CODE_KEY);
        push_str(trace, &value);
        self.storage = Some(value);
    }

    /// `takePendingCreatorCode()`.
    fn take(&mut self, trace: &mut Vec<f64>) -> JsVal {
        trace.push(74.0);
        push_str(trace, PENDING_CREATOR_CODE_KEY);
        match &self.storage {
            None => {
                trace.push(2.0); // getItem -> null
                JsVal::Null
            }
            Some(raw) => {
                let raw = raw.clone();
                trace.push(5.0);
                push_str(trace, &raw);
                // removeItem FIRST (consume-style, before the parse).
                trace.push(76.0);
                push_str(trace, PENDING_CREATOR_CODE_KEY);
                self.storage = None;
                let Some(parsed) = json_parse(&raw) else {
                    // JSON.parse threw (legacy raw string / malformed).
                    return JsVal::Null;
                };
                if parsed.typeof_str() != "object" || matches!(parsed, Parsed::Null) {
                    return JsVal::Null;
                }
                let stashed = match parsed.field("stashedAt") {
                    Some(Parsed::Num(n)) => *n,
                    // Non-numeric stashedAt short-circuits the `||` BEFORE
                    // `Date.now()` — no 81 trace event on this path.
                    _ => return JsVal::Null,
                };
                let now = self.pop_now(trace);
                if now - stashed > PENDING_CREATOR_CODE_TTL_MS {
                    return JsVal::Null;
                }
                match parsed.field("code") {
                    Some(Parsed::Str(c)) => JsVal::Str(c.clone()),
                    _ => JsVal::Null,
                }
            }
        }
    }

    /// Run one op. Kind table (see `tools/gen_vectors.mjs`):
    /// 0 setup `[...codec(pathname), ...codec(search), ...codec(hash), n,
    ///   (now)*n, ...codec(initStorage)]` -> `[0]` (resets everything);
    /// 1 stash `[...codec(code)]` -> `[traceLen, (trace)*]`;
    /// 2 take -> `[...codec(result), traceLen, (trace)*]`;
    /// 3 normalize `[...codec(raw)]` -> `[...codec(result)]`;
    /// 4 parsePath `[...codec(pathname)]` -> `[...codec(result)]`;
    /// 5 consume -> `[traceLen, (trace)*]`;
    /// 6 resume -> `[...codec(bool), traceLen, (trace)*]`;
    /// 7 constants -> `[klen, (key)*klen, TTL_MS]`;
    /// 8 dumpStorage -> `[...codec(storage ?? undefined-absent 0)]`.
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut i = 0usize;
        match kind {
            0 => {
                *self = Self::default();
                self.pathname = read_val(args, &mut i);
                self.search = read_val(args, &mut i);
                self.hash = read_val(args, &mut i);
                let n = args[i] as usize;
                i += 1;
                self.now_queue = args[i..i + n].to_vec();
                i += n;
                self.storage = match read_val(args, &mut i) {
                    JsVal::Str(s) => Some(s),
                    _ => None, // absent / null -> getItem returns null
                };
                vec![0.0]
            }
            1 => {
                let code = read_str(args, &mut i);
                let mut trace = Vec::new();
                self.stash(&code, &mut trace);
                let mut out = vec![trace.len() as f64];
                out.extend_from_slice(&trace);
                out
            }
            2 => {
                let mut trace = Vec::new();
                let r = self.take(&mut trace);
                let mut out = Vec::new();
                push_val(&mut out, &r);
                out.push(trace.len() as f64);
                out.extend_from_slice(&trace);
                out
            }
            3 => {
                let raw = read_str(args, &mut i);
                let mut out = Vec::new();
                match normalize_creator_code_input(&raw) {
                    Some(s) => push_val(&mut out, &JsVal::Str(s)),
                    None => push_val(&mut out, &JsVal::Null),
                }
                out
            }
            4 => {
                let pathname = read_str(args, &mut i);
                let mut out = Vec::new();
                match parse_creator_code_path(&pathname) {
                    Some(s) => push_val(&mut out, &JsVal::Str(s)),
                    None => push_val(&mut out, &JsVal::Null),
                }
                out
            }
            5 => {
                let mut trace = Vec::new();
                // window.location.pathname read FIRST.
                trace.push(78.0);
                push_val(&mut trace, &self.pathname);
                let pathname = match &self.pathname {
                    JsVal::Str(s) => s.clone(),
                    _ => String::new(),
                };
                let Some(segment) = parse_creator_code_path(&pathname) else {
                    let mut out = vec![trace.len() as f64];
                    out.extend_from_slice(&trace);
                    return out;
                };
                if let Some(code) = normalize_creator_code_input(&segment) {
                    self.stash(&code, &mut trace);
                }
                // search + hash read only on the strip path.
                trace.push(79.0);
                push_val(&mut trace, &self.search);
                trace.push(80.0);
                push_val(&mut trace, &self.hash);
                let search = match &self.search {
                    JsVal::Str(s) => s.clone(),
                    _ => String::new(),
                };
                let hash = match &self.hash {
                    JsVal::Str(s) => s.clone(),
                    _ => String::new(),
                };
                trace.push(77.0);
                push_str(&mut trace, &format!("/{search}{hash}"));
                let mut out = vec![trace.len() as f64];
                out.extend_from_slice(&trace);
                out
            }
            6 => {
                let mut trace = Vec::new();
                let r = self.take(&mut trace);
                let out_bool = match &r {
                    JsVal::Str(c) => {
                        trace.push(82.0);
                        push_str(&mut trace, c);
                        true
                    }
                    _ => false,
                };
                let mut out = Vec::new();
                push_val(&mut out, &JsVal::Bool(out_bool));
                out.push(trace.len() as f64);
                out.extend_from_slice(&trace);
                out
            }
            7 => {
                let mut out = Vec::new();
                push_str(&mut out, PENDING_CREATOR_CODE_KEY);
                out.push(PENDING_CREATOR_CODE_TTL_MS);
                out
            }
            8 => {
                let mut out = Vec::new();
                match &self.storage {
                    // dump as what getItem would see: null when absent.
                    None => out.push(2.0),
                    Some(s) => push_val(&mut out, &JsVal::Str(s.clone())),
                }
                out
            }
            k => unreachable!("creator_code harness: unknown op kind {k}"),
        }
    }
}
