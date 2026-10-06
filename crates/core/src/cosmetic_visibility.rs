//! Port of `src/client/view/CosmeticVisibility.ts`: `visibleCosmetics`, the
//! per-category cosmetic filter. All three arguments ride plain JS data
//! objects, so the harness models them with the [`JsVal`] codec.
//!
//! Faithfulness notes (quirk list):
//!
//! * `owner === "self"` short-circuits BEFORE the `visibility.showFrom` read
//!   — a broken `visibility` is never touched for your own cosmetics.
//! * `showFrom = visibility.showFrom ?? "everyone"`: `null` and `undefined`
//!   (or an absent key) both default; any other string ("friends", 0, …)
//!   fails both hide gates and takes the FULL visible path.
//! * The hidden branch returns `{ verified: cosmetics.verified }` — a
//!   one-key object where `verified` is PRESENT EVEN WHEN UNDEFINED (it shows
//!   up in `Object.keys`); the codec keeps that as a `Undef` field.
//! * The category gates are STRICT `=== false`: `0`, `""`, `null`,
//!   `undefined` and `"false"` all leave the category visible.
//! * `delete visible.pattern` on an absent key is a no-op — it never creates
//!   the key; when present, the key's insertion position is removed and the
//!   remaining order survives.
//! * The effects re-filter runs whenever `cosmetics.effects !== undefined`
//!   (strict — `null` passes the gate and `Object.entries(null)` would throw;
//!   out of the capture's domain). `Object.entries` of an ARRAY yields index
//!   string keys and `Object.fromEntries` rebuilds a plain OBJECT — the
//!   Arr→Obj transition is observable through the codec.
//! * A slot with no `effectTypeForSlot` resolution (unknown / stale bare
//!   `"nukeExplosion"`) is KEPT unconditionally; otherwise the gate reads
//!   `visibility[effectType] !== false` dynamically.
//! * `visible.effects = ...` overwrites the spread's existing `effects` key
//!   in place (its position survives).

use crate::cosmetic_schemas::effect_type_for_slot;
use crate::js_json::{read_val, JsVal};

/// `obj[key]` read: an absent key reads JS `undefined`.
fn read_key(src: &[(String, JsVal)], key: &str) -> JsVal {
    src.iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
        .unwrap_or(JsVal::Undef)
}

/// `delete obj[key]`: remove an own key if present, never create one.
fn delete_key(fields: &mut Vec<(String, JsVal)>, key: &str) {
    fields.retain(|(k, _)| k != key);
}

/// `Object.entries(v)` for the codec domain: an object's fields as given, an
/// array's elements as ascending index strings, a STRING's UTF-16 code units
/// as one-char string VALUES (JS `Object.entries("xy")` is
/// `[["0","x"],["1","y"]]`). Anything else yields nothing (a number /
/// boolean / null has no own enumerable keys). A string carrying an ASTRAL
/// character is out of the capture's domain: the codec crosses strings as
/// UTF-16 units, but a lone surrogate cannot be rebuilt into a Rust `String`
/// the way JS keeps it, so such units land lossy (U+FFFD) on this side while
/// the TS keeps the raw surrogate — the capture pins BMP-only strings.
fn entries_of(v: &JsVal) -> Vec<(String, JsVal)> {
    match v {
        JsVal::Obj(fields) => fields.clone(),
        JsVal::Arr(items) => items
            .iter()
            .enumerate()
            .map(|(i, x)| (i.to_string(), x.clone()))
            .collect(),
        JsVal::Str(s) => s
            .encode_utf16()
            .enumerate()
            .map(|(i, u)| {
                (
                    i.to_string(),
                    JsVal::Str(char::from_u32(u as u32).unwrap_or('\u{fffd}').to_string()),
                )
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// `visibleCosmetics(cosmetics, visibility, owner)` over the codec domain.
/// `owner` is the raw string; non-canonical values behave like "other".
pub fn visible_cosmetics(cosmetics: &JsVal, visibility: &JsVal, owner: &str) -> JsVal {
    if owner == "self" {
        return cosmetics.clone();
    }
    let vis_fields = match visibility {
        JsVal::Obj(fields) => fields.clone(),
        _ => Vec::new(),
    };
    let show_from = match read_key(&vis_fields, "showFrom") {
        JsVal::Undef | JsVal::Null | JsVal::Absent => JsVal::Str("everyone".to_string()),
        v => v,
    };
    let hidden = match &show_from {
        JsVal::Str(s) => {
            s == "self" || (s == "teammates" && owner != "teammate")
        }
        _ => false,
    };
    if hidden {
        // { verified: cosmetics.verified } — the key is present even when the
        // value reads undefined.
        let verified = match cosmetics {
            JsVal::Obj(fields) => read_key(fields, "verified"),
            _ => JsVal::Undef,
        };
        return JsVal::Obj(vec![("verified".to_string(), verified)]);
    }

    let mut visible: Vec<(String, JsVal)> = match cosmetics {
        JsVal::Obj(fields) => fields.clone(),
        JsVal::Arr(items) => items
            .iter()
            .enumerate()
            .map(|(i, x)| (i.to_string(), x.clone()))
            .collect(),
        _ => Vec::new(),
    };

    let gate_false = |key: &str| matches!(read_key(&vis_fields, key), JsVal::Bool(false));
    if gate_false("territorySkins") {
        delete_key(&mut visible, "pattern");
        delete_key(&mut visible, "skin");
    }
    if gate_false("flags") {
        delete_key(&mut visible, "flag");
    }
    if gate_false("crowns") {
        delete_key(&mut visible, "crown");
    }

    let effects = match cosmetics {
        JsVal::Obj(fields) => read_key(fields, "effects"),
        _ => JsVal::Undef,
    };
    if effects != JsVal::Undef {
        let kept: Vec<(String, JsVal)> = entries_of(&effects)
            .into_iter()
            .filter(|(slot, _)| match effect_type_for_slot(slot) {
                None => true,
                Some(et) => !matches!(read_key(&vis_fields, et), JsVal::Bool(false)),
            })
            .collect();
        // `visible.effects = Object.fromEntries(kept)` — the spread already
        // owns the key (the gate proved it is not undefined), so the write
        // replaces it in place; a non-object cosmetics has no key to replace.
        if let Some(slot) = visible.iter_mut().find(|(k, _)| k == "effects") {
            slot.1 = JsVal::Obj(kept);
        } else {
            visible.push(("effects".to_string(), JsVal::Obj(kept)));
        }
    }

    JsVal::Obj(visible)
}

// ---------------------------------------------------------------- vectors op
//
// kind 0: visibleCosmetics [...codec(cosmetics), ...codec(visibility),
//   ...codec(owner)] -> [...codec(result)]

/// Flat-token runner shared by the golden replay and the wasm probe.
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    match kind {
        0 => {
            let mut i = 0usize;
            let cosmetics = read_val(args, &mut i);
            let visibility = read_val(args, &mut i);
            let owner = read_val(args, &mut i);
            let JsVal::Str(o) = owner else {
                unreachable!("cosmetic_visibility: owner must be a string");
            };
            let res = visible_cosmetics(&cosmetics, &visibility, &o);
            let mut out = Vec::new();
            crate::js_json::push_val(&mut out, &res);
            out
        }
        k => unreachable!("cosmetic_visibility: unknown op kind {k}"),
    }
}
