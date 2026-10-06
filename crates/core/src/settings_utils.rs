//! Port of `src/client/render/gl/SettingsUtils.ts`: `deepAssign` and
//! `deepDiff` for the RenderSettings persistence layer. Both ride plain JS
//! data objects, so the harness models them with the [`JsVal`] codec
//! (insertion-ordered fields, `undefined` vs `null` vs value).
//!
//! Faithfulness notes (quirk list):
//!
//! * `deepAssign` iterates `Object.keys(source)` and gates EVERY write on
//!   `key in target`: a source key the target does not have is silently
//!   dropped — the function never grows the target.
//! * The `Array.isArray` branch comes FIRST and clones (`structuredClone`)
//!   — arrays are replaced wholesale, never merged per index.
//! * The recursion branch tests `typeof x === "object" && x !== null` on
//!   BOTH sides, so `null` source or target values fall through to the
//!   plain-assign branch (`typeof null === "object"` but the `!== null`
//!   guard rejects them).
//! * `deepDiff` walks `Object.keys(defaults)` (the DEFAULTS drive the key
//!   set), recursing when both sides are non-null objects — arrays included,
//!   which is why `deepDiff([1,2],[1,3])` yields `{"1":3}` with index-string
//!   keys (arrays are read as their own-index key lists, exactly like JS).
//! * A current key missing from the object reads `undefined`, and
//!   `dv !== cv` is STRICT: `NaN !== NaN` records a diff, `-0 !== 0` does
//!   NOT, and two distinct objects always differ by reference — but the
//!   recursion branch catches both-objects before that comparison.
//! * `result ??= {}` lazily creates the sparse partial; a recursive `sub`
//!   is written only when `sub !== undefined` (no-difference branches leave
//!   the key out entirely). No differences at all → the JS `undefined`
//!   return, modelled as [`JsVal::Undef`].

use crate::js_json::{map_set, push_val, read_val, JsVal};

/// Own-key list of a value: an object's fields as given, an array's elements
/// as ascending index strings (JS `Object.keys` order). `None` for
/// non-object values.
fn fields_of(v: &JsVal) -> Option<Vec<(String, JsVal)>> {
    match v {
        JsVal::Obj(fields) => Some(fields.clone()),
        JsVal::Arr(items) => Some(
            items
                .iter()
                .enumerate()
                .map(|(i, x)| (i.to_string(), x.clone()))
                .collect(),
        ),
        _ => None,
    }
}

/// `typeof x === "object" && x !== null` — arrays count.
fn is_js_object(v: &JsVal) -> bool {
    matches!(v, JsVal::Obj(_) | JsVal::Arr(_))
}

/// `Object.prototype` own keys: the `key in target` test walks the
/// prototype chain, so a plain object literal answers TRUE for these even
/// with no own field (the capture pins this: a `valueOf` source key onto an
/// empty target DOES get assigned as a new own property).
const OBJECT_PROTO_KEYS: &[&str] = &[
    "constructor",
    "hasOwnProperty",
    "isPrototypeOf",
    "propertyIsEnumerable",
    "toLocaleString",
    "toString",
    "valueOf",
    "__defineGetter__",
    "__defineSetter__",
    "__lookupGetter__",
    "__lookupSetter__",
    "__proto__",
];

/// `key in target` for the codec's field list plus the prototype chain
/// (targets are plain object literals in the capture's domain).
fn has_key(target: &[(String, JsVal)], key: &str) -> bool {
    target.iter().any(|(k, _)| k == key) || OBJECT_PROTO_KEYS.contains(&key)
}

/// `target[key]` read: an absent key reads JS `undefined`.
fn read_key(src: &[(String, JsVal)], key: &str) -> JsVal {
    src.iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
        .unwrap_or(JsVal::Undef)
}

/// `deepAssign(target, source)` — in place on an object field list.
fn deep_assign(target: &mut Vec<(String, JsVal)>, source: &[(String, JsVal)]) {
    for (key, sv) in source {
        if matches!(sv, JsVal::Arr(_)) {
            // Array branch: wholesale structuredClone, still gated on `in`.
            if has_key(target, key) {
                map_set(target, key, sv.clone());
            }
            continue;
        }
        let tv = read_key(target, key);
        if is_js_object(sv) && is_js_object(&tv) {
            // Both non-null objects: JS recurses with target[key] as the
            // nested target. The capture stays in the object/object domain
            // (render settings nest plain objects with array leaves); an
            // array-valued target under an object source is out of scope
            // and skipped, never recorded.
            if let (JsVal::Obj(ssub), JsVal::Obj(tsub)) = (sv, &tv) {
                let mut inner = tsub.clone();
                deep_assign(&mut inner, ssub);
                map_set(target, key, JsVal::Obj(inner));
            }
            continue;
        }
        if has_key(target, key) {
            map_set(target, key, sv.clone());
        }
    }
}

/// Strict JS `!==` over the codec domain (absent already normalised to
/// `Undef` by [`read_key`]): `NaN !== NaN` is TRUE (f64 PartialEq), `-0`
/// equals `0`. Two containers reaching this comparison are always distinct
/// JS references, but the recursion branch keeps both-object pairs out of
/// it, so content equality only matters for scalar / undefined pairs.
fn strict_ne(a: &JsVal, b: &JsVal) -> bool {
    a != b
}

/// `deepDiff(defaults, current)` — `None` models the JS `undefined` return.
fn deep_diff(
    defaults: &[(String, JsVal)],
    current: &[(String, JsVal)],
) -> Option<Vec<(String, JsVal)>> {
    let mut result: Option<Vec<(String, JsVal)>> = None;
    for (key, dv) in defaults {
        let cv = read_key(current, key);
        if is_js_object(dv) && is_js_object(&cv) {
            let dsub = fields_of(dv).unwrap();
            let csub = fields_of(&cv).unwrap();
            if let Some(sub) = deep_diff(&dsub, &csub) {
                result.get_or_insert_with(Vec::new).push((key.clone(), JsVal::Obj(sub)));
            }
        } else if strict_ne(dv, &cv) {
            result.get_or_insert_with(Vec::new).push((key.clone(), cv));
        }
    }
    result
}

// ---------------------------------------------------------------- vectors op
//
// kind 0: deepAssign [...codec(target), ...codec(source)] -> [...codec(target after)]
// kind 1: deepDiff   [...codec(defaults), ...codec(current)] -> [...codec(result|undefined)]

/// Flat-token runner shared by the golden replay and the wasm probe.
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut i = 0usize;
    match kind {
        0 => {
            let t = read_val(args, &mut i);
            let s = read_val(args, &mut i);
            let (JsVal::Obj(mut target), JsVal::Obj(source)) = (t, s) else {
                unreachable!("settings_utils: deepAssign args must be objects");
            };
            deep_assign(&mut target, &source);
            let mut out = Vec::new();
            push_val(&mut out, &JsVal::Obj(target));
            out
        }
        1 => {
            let d = read_val(args, &mut i);
            let c = read_val(args, &mut i);
            let (JsVal::Obj(defaults), JsVal::Obj(current)) = (d, c) else {
                unreachable!("settings_utils: deepDiff args must be objects");
            };
            let res = match deep_diff(&defaults, &current) {
                Some(fields) => JsVal::Obj(fields),
                None => JsVal::Undef,
            };
            let mut out = Vec::new();
            push_val(&mut out, &res);
            out
        }
        k => unreachable!("settings_utils: unknown op kind {k}"),
    }
}
