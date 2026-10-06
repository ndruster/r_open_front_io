//! Shared JS `JSON.stringify` fidelity helper for the server-cluster ports
//! (`consensus` keys candidates by `JSON.stringify(msg.winner ?? null)` and
//! `JSON.stringify(stats)`).
//!
//! [`JsonValue`] models the JS value domain `JSON.stringify` sees: `Null` is
//! JS `null`, `Undef` is JS `undefined` / an absent property, `Num` is a JS
//! `Number`, `Obj` preserves property **insertion order** (JS own-string-key
//! order for the shapes the capture feeds in). [`json_stringify`] reproduces
//! V8's serialisation byte-for-byte for that domain:
//!
//! * top-level `undefined` (and functions, which are not modelled) serialises
//!   to `None` — JS returns the `undefined` value, not a string;
//! * `undefined` object-property values omit the key entirely; `null` values
//!   serialise as `null`;
//! * `undefined` / `null` array elements serialise as `null`;
//! * numbers go through [`crate::game_ts::js_num_str`] (integers print
//!   without `.0`, `-0` prints as `0`), except `NaN` / `±Infinity`, which
//!   `JSON.stringify` writes as the literal `null`;
//! * strings escape `"` `\` and the control characters: `\b \f \n \r \t` for
//!   0x08/0x0C/0x0A/0x0D/0x09 and four-hex-digit `\uXXXX` (lowercase) for the
//!   rest below 0x20; lone UTF-16 surrogates are escaped as `\udXXX` (ES2019
//!   well-formed JSON.stringify).

use crate::game_ts::js_num_str;

/// A JS value in the `JSON.stringify` domain. `Obj` keeps `(key, value)`
/// pairs in insertion order; an absent property is modelled by not storing
/// the pair, a property explicitly set to `undefined` by [`JsonValue::Undef`]
/// (which `json_stringify` omits from the output — the two are observably
/// identical to `JSON.stringify` but distinct to `in` / `!== undefined`
/// checks, which is why the tri-state is kept).
#[derive(Clone, Debug, PartialEq)]
pub enum JsonValue {
    Undef,
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<JsonValue>),
    Obj(Vec<(String, JsonValue)>),
}

/// `JSON.stringify(value)` — `None` for the JS `undefined` return (top-level
/// `undefined`).
pub fn json_stringify(v: &JsonValue) -> Option<String> {
    if matches!(v, JsonValue::Undef) {
        return None;
    }
    let mut out = String::new();
    ser(v, &mut out);
    Some(out)
}

fn ser(v: &JsonValue, out: &mut String) {
    match v {
        // Only reachable inside containers; top-level Undef is handled by
        // json_stringify. Arrays write null, objects omit the pair.
        JsonValue::Undef => out.push_str("null"),
        JsonValue::Null => out.push_str("null"),
        JsonValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        JsonValue::Num(n) => {
            if n.is_nan() || n.is_infinite() {
                out.push_str("null");
            } else {
                out.push_str(&js_num_str(*n));
            }
        }
        JsonValue::Str(s) => ser_str(s, out),
        JsonValue::Arr(items) => {
            out.push('[');
            for (i, it) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                ser(it, out);
            }
            out.push(']');
        }
        JsonValue::Obj(entries) => {
            out.push('{');
            let mut first = true;
            for (k, val) in entries {
                if matches!(val, JsonValue::Undef) {
                    continue;
                }
                if !first {
                    out.push(',');
                }
                first = false;
                ser_str(k, out);
                out.push(':');
                ser(val, out);
            }
            out.push('}');
        }
    }
}

/// Escape a JS string as `JSON.stringify` would.
fn ser_str(s: &str, out: &mut String) {
    let units: Vec<u16> = s.encode_utf16().collect();
    ser_units(&units, out);
}

/// Unit-level escaper: a valid surrogate pair passes through as the astral
/// character, a lone surrogate escapes as `\udXXX` (ES2019 well-formed
/// `JSON.stringify`).
fn ser_units(units: &[u16], out: &mut String) {
    out.push('"');
    let mut i = 0;
    while i < units.len() {
        let cu = units[i];
        match cu {
            0x22 => {
                out.push_str("\\\"");
                i += 1;
            }
            0x5C => {
                out.push_str("\\\\");
                i += 1;
            }
            0x08 => {
                out.push_str("\\b");
                i += 1;
            }
            0x0C => {
                out.push_str("\\f");
                i += 1;
            }
            0x0A => {
                out.push_str("\\n");
                i += 1;
            }
            0x0D => {
                out.push_str("\\r");
                i += 1;
            }
            0x09 => {
                out.push_str("\\t");
                i += 1;
            }
            c if c < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c));
                i += 1;
            }
            lo if (0xD800..0xDC00).contains(&lo) => {
                if i + 1 < units.len() && (0xDC00..0xE000).contains(&units[i + 1]) {
                    let cp = 0x1_0000 + ((lo as u32 - 0xD800) << 10)
                        + (units[i + 1] as u32 - 0xDC00);
                    out.push(char::from_u32(cp).unwrap_or('\u{fffd}'));
                    i += 2;
                } else {
                    out.push_str(&format!("\\u{:04x}", lo));
                    i += 1;
                }
            }
            // A trailing surrogate with no leading one before it.
            hi @ 0xDC00..0xE000 => {
                out.push_str(&format!("\\u{:04x}", hi));
                i += 1;
            }
            c => {
                out.push(char::from_u32(c as u32).unwrap_or('\u{fffd}'));
                i += 1;
            }
        }
    }
    out.push('"');
}

// ---------------------------------------------------------------- JSON.parse
//
// Strict `JSON.parse` for the embedded render-settings / theme JSON (S13).
// `None` models the `SyntaxError` V8 throws (the golden captures the throw
// path as a status token, never a value). Objects come back with V8 own-key
// order — integer-like keys sort numerically ahead of the string keys, which
// keep insertion order — exactly like the private parser `boot_interrupts`
// uses for the claim-prompt store; `JSON.parse` materialises properties in
// that ordinary order, so `Object.keys` over a parsed theme / settings tree
// follows it (the embedded data has no integer-like object keys, but the
// capture scripts arbitrary JSON strings too).

/// Canonical decimal-integer object key (V8 integer-key section): digits
/// only, no leading zeros (except "0" itself), value < 2^53.
fn canon_int_key(k: &str) -> Option<u64> {
    let b = k.as_bytes();
    if b.is_empty() || !b.iter().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if b[0] == b'0' && b.len() > 1 {
        return None;
    }
    if b.len() > 16 {
        return None;
    }
    k.parse::<u64>().ok().filter(|v| *v < (1u64 << 53))
}

/// `map[key] = value` in V8 own-key order (integer keys first, ascending;
/// string keys keep insertion order; an existing key overwrites in place).
fn obj_set_v8(m: &mut Vec<(String, JsVal)>, key: String, v: JsVal) {
    if let Some(slot) = m.iter_mut().find(|(k, _)| *k == key) {
        slot.1 = v;
        return;
    }
    match canon_int_key(&key) {
        Some(val) => {
            let pos = m
                .iter()
                .position(|(k, _)| match canon_int_key(k) {
                    Some(kv) => kv > val,
                    None => true,
                })
                .unwrap_or(m.len());
            m.insert(pos, (key, v));
        }
        None => m.push((key, v)),
    }
}

fn jv_ws(b: &[u8], i: &mut usize) {
    while *i < b.len() && matches!(b[*i], b' ' | b'\t' | b'\n' | b'\r') {
        *i += 1;
    }
}

fn jv_lit(b: &[u8], i: &mut usize, word: &str) -> bool {
    if b.len() >= *i + word.len() && &b[*i..*i + word.len()] == word.as_bytes() {
        *i += word.len();
        true
    } else {
        false
    }
}

fn jv_value(b: &[u8], i: &mut usize) -> Option<JsVal> {
    jv_ws(b, i);
    if *i >= b.len() {
        return None;
    }
    match b[*i] {
        b'n' => jv_lit(b, i, "null").then_some(JsVal::Null),
        b't' => jv_lit(b, i, "true").then_some(JsVal::Bool(true)),
        b'f' => jv_lit(b, i, "false").then_some(JsVal::Bool(false)),
        b'"' => jv_string(b, i).map(JsVal::Str),
        b'[' => {
            *i += 1;
            let mut items = Vec::new();
            jv_ws(b, i);
            if b.get(*i)? == &b']' {
                *i += 1;
                return Some(JsVal::Arr(items));
            }
            loop {
                items.push(jv_value(b, i)?);
                jv_ws(b, i);
                match b.get(*i)? {
                    b',' => *i += 1,
                    b']' => {
                        *i += 1;
                        return Some(JsVal::Arr(items));
                    }
                    _ => return None,
                }
            }
        }
        b'{' => {
            *i += 1;
            let mut fields: Vec<(String, JsVal)> = Vec::new();
            jv_ws(b, i);
            if b.get(*i)? == &b'}' {
                *i += 1;
                return Some(JsVal::Obj(fields));
            }
            loop {
                jv_ws(b, i);
                let key = jv_string(b, i)?;
                jv_ws(b, i);
                if b.get(*i)? != &b':' {
                    return None;
                }
                *i += 1;
                let v = jv_value(b, i)?;
                obj_set_v8(&mut fields, key, v);
                jv_ws(b, i);
                match b.get(*i)? {
                    b',' => *i += 1,
                    b'}' => {
                        *i += 1;
                        return Some(JsVal::Obj(fields));
                    }
                    _ => return None,
                }
            }
        }
        _ => jv_number(b, i),
    }
}

fn jv_string(b: &[u8], i: &mut usize) -> Option<String> {
    if b.get(*i)? != &b'"' {
        return None;
    }
    *i += 1;
    // Collect UTF-16 units so `\uXXXX` escapes (and astral pairs) decode like
    // JS. Raw non-ASCII bytes are re-encoded from their UTF-8 sequence.
    let mut units: Vec<u16> = Vec::new();
    loop {
        let c = *b.get(*i)?;
        *i += 1;
        match c {
            b'"' => return Some(String::from_utf16_lossy(&units)),
            0x5C => {
                let e = *b.get(*i)?;
                *i += 1;
                match e {
                    b'"' => units.push(0x22),
                    b'\\' => units.push(0x5C),
                    b'/' => units.push(0x2F),
                    b'b' => units.push(0x08),
                    b'f' => units.push(0x0C),
                    b'n' => units.push(0x0A),
                    b'r' => units.push(0x0D),
                    b't' => units.push(0x09),
                    b'u' => {
                        if *i + 4 > b.len() {
                            return None;
                        }
                        let hex = std::str::from_utf8(&b[*i..*i + 4]).ok()?;
                        if !hex.bytes().all(|x| x.is_ascii_hexdigit()) {
                            return None;
                        }
                        let cp = u32::from_str_radix(hex, 16).ok()?;
                        *i += 4;
                        units.push(cp as u16);
                        // A high surrogate followed by a `\u` low surrogate
                        // forms one astral character; `from_utf16_lossy`
                        // reassembles valid pairs (lone surrogates become
                        // U+FFFD — the repo's standard lossy UTF-16 model).
                    }
                    _ => return None,
                }
            }
            // Unescaped control characters are invalid JSON.
            0x00..=0x1F => return None,
            0x20..=0x7F => units.push(c as u16),
            lead => {
                // Multi-byte UTF-8 sequence: decode the char, push its
                // UTF-16 units.
                let len = if lead >= 0xF0 {
                    4
                } else if lead >= 0xE0 {
                    3
                } else if lead >= 0xC0 {
                    2
                } else {
                    return None; // stray continuation byte
                };
                if *i + len - 1 > b.len() {
                    return None;
                }
                let s = std::str::from_utf8(&b[*i - 1..*i + len - 1]).ok()?;
                let mut chars = s.chars();
                let ch = chars.next()?;
                if chars.next().is_some() {
                    return None;
                }
                *i += len - 1;
                let mut buf = [0u16; 2];
                units.extend_from_slice(ch.encode_utf16(&mut buf));
            }
        }
    }
}

fn jv_number(b: &[u8], i: &mut usize) -> Option<JsVal> {
    let start = *i;
    if b.get(*i)? == &b'-' {
        *i += 1;
    }
    // int part: 0 | [1-9][0-9]*
    match b.get(*i)? {
        b'0' => *i += 1,
        b'1'..=b'9' => {
            while *i < b.len() && b[*i].is_ascii_digit() {
                *i += 1;
            }
        }
        _ => return None,
    }
    if b.get(*i) == Some(&b'.') {
        *i += 1;
        if !matches!(b.get(*i), Some(c) if c.is_ascii_digit()) {
            return None;
        }
        while matches!(b.get(*i), Some(c) if c.is_ascii_digit()) {
            *i += 1;
        }
    }
    if matches!(b.get(*i), Some(b'e') | Some(b'E')) {
        *i += 1;
        if matches!(b.get(*i), Some(b'+') | Some(b'-')) {
            *i += 1;
        }
        if !matches!(b.get(*i), Some(c) if c.is_ascii_digit()) {
            return None;
        }
        while matches!(b.get(*i), Some(c) if c.is_ascii_digit()) {
            *i += 1;
        }
    }
    let s = std::str::from_utf8(&b[start..*i]).ok()?;
    // JSON numbers are IEEE-754 doubles; Rust's parse is correctly rounded
    // like V8's fast path.
    s.parse::<f64>().ok().map(JsVal::Num)
}

/// `JSON.parse(text)` — `None` models the `SyntaxError` throw.
pub fn json_parse(s: &str) -> Option<JsVal> {
    let b = s.as_bytes();
    let mut i = 0usize;
    let v = jv_value(b, &mut i)?;
    jv_ws(b, &mut i);
    if i != b.len() {
        return None; // trailing garbage throws
    }
    Some(v)
}

// ---------------------------------------------------------------- harness codec
//
// The `cp_` / `ia_` / `cv_` op streams carry JS object values with the
// absent / `undefined` / `null` / value tri-plus-state the ports depend on
// (`patch[key] !== undefined` gates, `value ?? undefined` writes,
// `JSON.stringify` key omission). Wire form, one value per token run:
// `[0]` absent, `[1]` undefined, `[2]` null, `[3,v]` number, `[4,b]` bool,
// `[5,len,u0..]` string, `[6,n,(len,u0..,value)*n]` object (fields in JS
// insertion order). Strings cross as `[len, u0, ..]` UTF-16 units.

/// A JS value with the absent-vs-undefined distinction the `ConfigPatch`
/// dump needs (both serialise identically under `JSON.stringify`, but a
/// present-`undefined` key shows up in `Object.keys` while an absent one
/// does not).
#[derive(Clone, Debug, Default, PartialEq)]
pub enum JsVal {
    /// `Absent` is the natural empty for harness fields (a not-yet-scripted read).
    #[default]
    Absent,
    Undef,
    Null,
    Num(f64),
    Bool(bool),
    Str(String),
    Obj(Vec<(String, JsVal)>),
    Arr(Vec<JsVal>),
}

/// Read one encoded value at `*i`, advancing the cursor.
pub(crate) fn read_val(a: &[f64], i: &mut usize) -> JsVal {
    let code = a[*i] as i32;
    *i += 1;
    match code {
        0 => JsVal::Absent,
        1 => JsVal::Undef,
        2 => JsVal::Null,
        3 => {
            let v = a[*i];
            *i += 1;
            JsVal::Num(v)
        }
        4 => {
            let b = a[*i] != 0.0;
            *i += 1;
            JsVal::Bool(b)
        }
        5 => JsVal::Str(read_str(a, i)),
        6 => {
            let n = a[*i] as usize;
            *i += 1;
            let mut fields = Vec::with_capacity(n);
            for _ in 0..n {
                let k = read_str(a, i);
                let v = read_val(a, i);
                fields.push((k, v));
            }
            JsVal::Obj(fields)
        }
        7 => {
            let n = a[*i] as usize;
            *i += 1;
            JsVal::Arr((0..n).map(|_| read_val(a, i)).collect())
        }
        c => unreachable!("js_json codec: unknown value code {c}"),
    }
}

/// Append one encoded value to the token stream.
pub(crate) fn push_val(o: &mut Vec<f64>, v: &JsVal) {
    match v {
        JsVal::Absent => o.push(0.0),
        JsVal::Undef => o.push(1.0),
        JsVal::Null => o.push(2.0),
        JsVal::Num(n) => {
            o.push(3.0);
            o.push(*n);
        }
        JsVal::Bool(b) => {
            o.push(4.0);
            o.push(if *b { 1.0 } else { 0.0 });
        }
        JsVal::Str(s) => {
            o.push(5.0);
            push_str(o, s);
        }
        JsVal::Obj(fields) => {
            o.push(6.0);
            o.push(fields.len() as f64);
            for (k, val) in fields {
                push_str(o, k);
                push_val(o, val);
            }
        }
        JsVal::Arr(items) => {
            o.push(7.0);
            o.push(items.len() as f64);
            for it in items {
                push_val(o, it);
            }
        }
    }
}

/// Read a `[n, (key, value)*n]` object map (JS insertion order).
pub(crate) fn read_map(a: &[f64], i: &mut usize) -> Vec<(String, JsVal)> {
    let n = a[*i] as usize;
    *i += 1;
    (0..n).map(|_| (read_str(a, i), read_val(a, i))).collect()
}

/// Append a `[n, (key, value)*n]` object map.
pub(crate) fn push_map(o: &mut Vec<f64>, m: &[(String, JsVal)]) {
    o.push(m.len() as f64);
    for (k, v) in m {
        push_str(o, k);
        push_val(o, v);
    }
}

/// `obj[key]` for an [`JsVal::Obj`] — `None` models the JS `undefined` an
/// absent field reads (the caller treats `None` and `Some(Undef)` alike for
/// `=== undefined` / `typeof` checks).
pub(crate) fn val_field<'a>(v: &'a JsVal, key: &str) -> Option<&'a JsVal> {
    match v {
        JsVal::Obj(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, x)| x),
        _ => None,
    }
}

/// `map[key] = value` on a JS object: an existing key is overwritten in
/// place (its insertion position survives), a new key is appended.
pub(crate) fn map_set(m: &mut Vec<(String, JsVal)>, key: &str, v: JsVal) {
    if let Some(slot) = m.iter_mut().find(|(k, _)| k == key) {
        slot.1 = v;
    } else {
        m.push((key.to_string(), v));
    }
}

/// Read a `[len, u0, ..]` UTF-16 string. Valid surrogate pairs reassemble
/// into one astral character; lone surrogates become U+FFFD (the same
/// lossy model every other UTF-16 reader in the port uses).
pub(crate) fn read_str(a: &[f64], i: &mut usize) -> String {
    let n = a[*i] as usize;
    *i += 1;
    let units: Vec<u16> = (0..n).map(|_| {
        let u = a[*i] as u16;
        *i += 1;
        u
    }).collect();
    String::from_utf16_lossy(&units)
}

/// Append a `[len, u0, ..]` UTF-16 string.
pub(crate) fn push_str(o: &mut Vec<f64>, s: &str) {
    let units: Vec<u16> = s.encode_utf16().collect();
    o.push(units.len() as f64);
    for u in units {
        o.push(u as f64);
    }
}

/// Project a codec value onto the [`JsonValue`] serialisation domain
/// (absent and `undefined` are identical to `JSON.stringify`).
pub(crate) fn to_json(v: &JsVal) -> JsonValue {
    match v {
        JsVal::Absent | JsVal::Undef => JsonValue::Undef,
        JsVal::Null => JsonValue::Null,
        JsVal::Num(n) => JsonValue::Num(*n),
        JsVal::Bool(b) => JsonValue::Bool(*b),
        JsVal::Str(s) => JsonValue::Str(s.clone()),
        JsVal::Obj(fields) => {
            JsonValue::Obj(fields.iter().map(|(k, x)| (k.clone(), to_json(x))).collect())
        }
        JsVal::Arr(items) => JsonValue::Arr(items.iter().map(to_json).collect()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> JsonValue {
        JsonValue::Str(v.to_string())
    }
    fn n(v: f64) -> JsonValue {
        JsonValue::Num(v)
    }

    #[test]
    fn undefined_top_level_returns_none() {
        assert_eq!(json_stringify(&JsonValue::Undef), None);
    }

    #[test]
    fn scalars() {
        assert_eq!(json_stringify(&JsonValue::Null).as_deref(), Some("null"));
        assert_eq!(json_stringify(&JsonValue::Bool(true)).as_deref(), Some("true"));
        assert_eq!(json_stringify(&JsonValue::Bool(false)).as_deref(), Some("false"));
        assert_eq!(json_stringify(&n(1.0)).as_deref(), Some("1"));
        assert_eq!(json_stringify(&n(-0.0)).as_deref(), Some("0"));
        assert_eq!(json_stringify(&n(2.5)).as_deref(), Some("2.5"));
        assert_eq!(json_stringify(&n(f64::NAN)).as_deref(), Some("null"));
        assert_eq!(json_stringify(&n(f64::INFINITY)).as_deref(), Some("null"));
        assert_eq!(json_stringify(&n(f64::NEG_INFINITY)).as_deref(), Some("null"));
    }

    #[test]
    fn string_escapes() {
        assert_eq!(json_stringify(&s("a\"b\\c")).as_deref(), Some("\"a\\\"b\\\\c\""));
        assert_eq!(
            json_stringify(&s("\u{8}\u{c}\n\r\t")).as_deref(),
            Some("\"\\b\\f\\n\\r\\t\""),
        );
        assert_eq!(json_stringify(&s("\u{1}\u{1f}")).as_deref(), Some("\"\\u0001\\u001f\""));
    }

    #[test]
    fn lone_surrogate_escapes() {
        // A lone high surrogate (0xD83D) must serialise as \ud83d, matching
        // V8's well-formed stringify. Rust `String` cannot hold a lone
        // surrogate, so feed the UTF-16 units through the escaper directly.
        let mut out = String::new();
        ser_units(&[0xD83D], &mut out);
        assert_eq!(out, "\"\\ud83d\"");
        // A lone trailing surrogate escapes too; a valid pair passes through.
        let mut out = String::new();
        ser_units(&[0xDE00], &mut out);
        assert_eq!(out, "\"\\ude00\"");
        let mut out = String::new();
        ser_units(&[0xD83D, 0xDE00], &mut out);
        assert_eq!(out, "\"\u{1F600}\"");
    }

    #[test]
    fn object_insertion_order_and_undefined_omission() {
        let v = JsonValue::Obj(vec![
            ("b".to_string(), n(1.0)),
            ("a".to_string(), JsonValue::Undef),
            ("c".to_string(), JsonValue::Null),
        ]);
        assert_eq!(json_stringify(&v).as_deref(), Some("{\"b\":1,\"c\":null}"));
        // Empty object after all keys omitted.
        let e = JsonValue::Obj(vec![("x".to_string(), JsonValue::Undef)]);
        assert_eq!(json_stringify(&e).as_deref(), Some("{}"));
    }

    #[test]
    fn array_undefined_and_null_become_null() {
        let v = JsonValue::Arr(vec![JsonValue::Undef, JsonValue::Null, n(0.0)]);
        assert_eq!(json_stringify(&v).as_deref(), Some("[null,null,0]"));
        assert_eq!(json_stringify(&JsonValue::Arr(vec![])).as_deref(), Some("[]"));
    }

    #[test]
    fn nested() {
        let v = JsonValue::Obj(vec![
            ("w".to_string(), JsonValue::Obj(vec![("p".to_string(), n(3.0))])),
            ("t".to_string(), JsonValue::Bool(false)),
        ]);
        assert_eq!(json_stringify(&v).as_deref(), Some("{\"w\":{\"p\":3},\"t\":false}"));
    }

    #[test]
    fn parse_scalars_and_containers() {
        assert_eq!(json_parse("null"), Some(JsVal::Null));
        assert_eq!(json_parse("true"), Some(JsVal::Bool(true)));
        assert_eq!(json_parse(" false "), Some(JsVal::Bool(false)));
        assert_eq!(json_parse("42"), Some(JsVal::Num(42.0)));
        assert_eq!(json_parse("-1.5e3"), Some(JsVal::Num(-1500.0)));
        assert_eq!(json_parse("1E+2"), Some(JsVal::Num(100.0)));
        assert_eq!(json_parse("\"a\\nb\""), Some(JsVal::Str("a\nb".to_string())));
        assert_eq!(json_parse("[]"), Some(JsVal::Arr(vec![])));
        assert_eq!(json_parse("{}"), Some(JsVal::Obj(vec![])));
        assert_eq!(
            json_parse("[1,null,\"x\",[true],{\"a\":0}]"),
            Some(JsVal::Arr(vec![
                JsVal::Num(1.0),
                JsVal::Null,
                JsVal::Str("x".to_string()),
                JsVal::Arr(vec![JsVal::Bool(true)]),
                JsVal::Obj(vec![("a".to_string(), JsVal::Num(0.0))]),
            ]))
        );
    }

    #[test]
    fn parse_rejects_invalid() {
        // SyntaxError paths: bare undefined / NaN / Infinity are not JSON,
        // leading zeros, trailing garbage, trailing commas, control chars.
        for bad in [
            "", "undefined", "NaN", "Infinity", "-Infinity", "01", "+1", ".5",
            "1.", "1e", "{a:1}", "{'a':1}", "[1,]", "{\"a\":1,}", "\"a\tb\"",
            "{\"a\":1}{\"b\":2}", "tru", "nul", "[1 2]", "{\"a\"1}", "\"\\x41\"",
        ] {
            assert_eq!(json_parse(bad), None, "must reject {bad:?}");
        }
    }

    #[test]
    fn parse_string_escapes_and_surrogates() {
        assert_eq!(
            json_parse("\"\\ud83d\\ude00\""),
            Some(JsVal::Str("\u{1F600}".to_string()))
        );
        // A lone surrogate becomes U+FFFD (the repo's lossy UTF-16 model).
        assert_eq!(json_parse("\"\\ud83d\""), Some(JsVal::Str("\u{FFFD}".to_string())));
        assert_eq!(json_parse("\"\\/\\\\\\\"\""), Some(JsVal::Str("/\\\"".to_string())));
        assert_eq!(json_parse("\"\\b\\f\\n\\r\\t\""), Some(JsVal::Str("\u{8}\u{c}\n\r\t".to_string())));
        assert_eq!(json_parse("\"café\""), Some(JsVal::Str("café".to_string())));
        assert_eq!(json_parse("\"😀\""), Some(JsVal::Str("😀".to_string())));
    }

    #[test]
    fn parse_object_v8_key_order() {
        // Integer-like keys sort ahead of string keys; duplicates overwrite
        // the first slot's value but keep its position (V8 semantics).
        let v = json_parse("{\"b\":1,\"10\":2,\"a\":3,\"2\":4,\"b\":5}").unwrap();
        let JsVal::Obj(fields) = &v else { panic!("object expected") };
        let keys: Vec<&str> = fields.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["2", "10", "b", "a"]);
        assert_eq!(
            json_stringify(&to_json(&v)).unwrap(),
            "{\"2\":4,\"10\":2,\"b\":5,\"a\":3}"
        );
    }

    #[test]
    fn parse_round_trip_stable_stringify() {
        // parse -> stringify reproduces the canonical (whitespace-free) form
        // for the embedded-data shapes.
        let src = "{\"a\":[1,2.5,null,true],\"b\":{\"c\":\"x\"},\"d\":0}";
        let v = json_parse(src).unwrap();
        assert_eq!(json_stringify(&to_json(&v)).unwrap(), src);
        // -0 parses to -0.0 but JSON.stringify prints 0 (js_num_str).
        let z = json_parse("-0").unwrap();
        assert_eq!(json_stringify(&to_json(&z)).unwrap(), "0");
    }

    #[test]
    fn parse_embedded_data_parses() {
        // The three include_str! payloads must parse (guards the data files).
        for s in [
            crate::render_settings::RENDER_SETTINGS_JSON,
            crate::render_settings::DEFAULT_THEME_JSON,
            crate::render_settings::COLORBLIND_THEME_JSON,
        ] {
            assert!(json_parse(s).is_some());
        }
    }
}
