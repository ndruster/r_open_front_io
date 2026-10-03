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
#[derive(Clone, Debug, PartialEq)]
pub enum JsVal {
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
}
