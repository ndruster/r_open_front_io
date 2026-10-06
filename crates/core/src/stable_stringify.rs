//! Port of the `stableStringify` subset of
//! `src/client/GraphicsPresets.ts` (L22-32) — recursively key-sorted JSON
//! used for preset equality. The rest of the module (the zod-parsed
//! `BUILTIN_PRESETS`, `parseGraphicsOverridesJson`, the migration table) is
//! out of scope.
//!
//! Faithfulness notes (quirk list):
//!
//! * The scalar branch is `value === null || typeof value !== "object"` →
//!   `JSON.stringify(value)` — a top-level `undefined` therefore returns the
//!   JS `undefined` value, not a string ([`json_stringify`]'s `None`), while
//!   `null` serialises to `"null"`.
//! * Arrays go through `value.map(stableStringify).join(",")`: an `undefined`
//!   *element* stringifies to `undefined` and `Array#join` renders it as the
//!   EMPTY STRING (not `null` like `JSON.stringify`), so `[1, undefined]`
//!   becomes `"[1,]"`.
//! * Object keys are `Object.entries` pairs — the capture's codec already
//!   carries V8's own-key order (integer-like keys first, ascending), so the
//!   Rust side consumes the field order as given; the `v !== undefined`
//!   filter is STRICT (a `null` value survives), and the sort comparator is
//!   `a < b ? -1 : 1` — a UTF-16 code-unit string compare, NOT
//!   `localeCompare` (so `"Z" < "a"`).
//! * Keys are emitted through `JSON.stringify(k)` (full escaping), values
//!   recurse through `stableStringify`.
//!
//! Only the pure subset is ported; the TS module's zod / JSON / UserSettings
//! imports never enter the capture graph (ts_load drops them).

use crate::js_json::{json_stringify, push_str, read_val, to_json, JsVal, JsonValue};
use std::cmp::Ordering;

/// JS string `<` over the UTF-16 code-unit sequence (the domain the codec
/// carries; lone surrogates arrive U+FFFD on the Rust side, the same lossy
/// model every other string port uses).
fn js_str_lt(a: &str, b: &str) -> bool {
    let ua = a.encode_utf16();
    let ub = b.encode_utf16();
    ua.cmp(ub) == Ordering::Less
}

/// `stableStringify(value)` — `None` models the JS `undefined` return.
pub fn stable_stringify(v: &JsVal) -> Option<String> {
    match v {
        JsVal::Obj(fields) => {
            let mut entries: Vec<&(String, JsVal)> = fields
                .iter()
                .filter(|(_, x)| !matches!(x, JsVal::Undef | JsVal::Absent))
                .collect();
            entries.sort_by(|a, b| {
                if js_str_lt(&a.0, &b.0) {
                    Ordering::Less
                } else if a.0 == b.0 {
                    Ordering::Equal
                } else {
                    Ordering::Greater
                }
            });
            let mut out = String::from("{");
            for (i, (k, val)) in entries.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                let key = json_stringify(&JsonValue::Str((*k).clone())).unwrap();
                out.push_str(&key);
                out.push(':');
                // A surviving (non-undefined) value never stringifies to
                // undefined; unwrap_or_default keeps the join-empty quirk
                // total just in case.
                out.push_str(&stable_stringify(val).unwrap_or_default());
            }
            out.push('}');
            Some(out)
        }
        JsVal::Arr(items) => {
            let mut out = String::from("[");
            for (i, it) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                // `join(",")` renders an `undefined` element as "" — the
                // `[1,]` quirk.
                out.push_str(&stable_stringify(it).unwrap_or_default());
            }
            out.push(']');
            Some(out)
        }
        other => json_stringify(&to_json(other)),
    }
}

/// `run_op(kind, args)` — capture harness entry (stateless). Kind table (see
/// `tools/gen_vectors.mjs`):
/// 0 stableStringify `[...codec(value)]` -> `[...codec(string|undefined)]`
///   (the string crosses as a codec [`5,len,u..]`, the JS `undefined` return
///   as [`1]).
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    let mut i = 0usize;
    match kind {
        0 => {
            let v = read_val(args, &mut i);
            match stable_stringify(&v) {
                Some(s) => {
                    out.push(5.0);
                    push_str(&mut out, &s);
                }
                None => out.push(1.0),
            }
        }
        k => unreachable!("stable_stringify: unknown op kind {k}"),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> JsVal {
        JsVal::Str(v.to_string())
    }
    fn n(v: f64) -> JsVal {
        JsVal::Num(v)
    }
    fn obj(fields: Vec<(&str, JsVal)>) -> JsVal {
        JsVal::Obj(fields.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }

    #[test]
    fn scalars_follow_json_stringify() {
        assert_eq!(stable_stringify(&JsVal::Undef), None);
        assert_eq!(stable_stringify(&JsVal::Null).as_deref(), Some("null"));
        assert_eq!(stable_stringify(&n(2.5)).as_deref(), Some("2.5"));
        assert_eq!(stable_stringify(&s("a\"b")).as_deref(), Some("\"a\\\"b\""));
    }

    #[test]
    fn keys_sort_utf16_not_locale() {
        // "Z" (0x5A) < "a" (0x61) — localeCompare would order them the other
        // way.
        let v = obj(vec![("a", n(1.0)), ("Z", n(2.0))]);
        assert_eq!(stable_stringify(&v).as_deref(), Some("{\"Z\":2,\"a\":1}"));
    }

    #[test]
    fn undefined_values_filtered_nulls_kept() {
        let v = obj(vec![
            ("u", JsVal::Undef),
            ("z", JsVal::Null),
            ("b", n(1.0)),
        ]);
        assert_eq!(stable_stringify(&v).as_deref(), Some("{\"b\":1,\"z\":null}"));
    }

    #[test]
    fn array_undefined_element_joins_empty() {
        let v = JsVal::Arr(vec![n(1.0), JsVal::Undef]);
        assert_eq!(stable_stringify(&v).as_deref(), Some("[1,]"));
        // JSON.stringify would say [null,null] — the join quirk differs.
        let e = JsVal::Arr(vec![]);
        assert_eq!(stable_stringify(&e).as_deref(), Some("[]"));
    }

    #[test]
    fn nested_objects_recurse() {
        let v = obj(vec![
            ("w", obj(vec![("p", n(3.0)), ("q", JsVal::Undef)])),
            ("t", JsVal::Arr(vec![obj(vec![("b", n(1.0)), ("a", n(2.0))])])),
        ]);
        assert_eq!(
            stable_stringify(&v).as_deref(),
            Some("{\"t\":[{\"a\":2,\"b\":1}],\"w\":{\"p\":3}}")
        );
    }
}
