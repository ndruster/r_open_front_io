//! Port of `src/client/render/gl/debug/EffectEditorState.ts` — the
//! editor-state constants and the pure slot defaults. `slotAttributes` /
//! `catalogSnippet` (zod round-trips) stay in TS.
//!
//! Faithfulness notes:
//!
//! * `maxColorsFor` compares the effect type with `===` against
//!   `"nukeExplosion"`; everything else (including unrecognised strings)
//!   gets 8.
//! * `EFFECT_EDITOR_TYPES` is a 7-key record; the runner dumps it in
//!   insertion order with the option arrays.
//! * `defaultSlotState` field order is the object-literal order below — the
//!   dump carries it as a codec object so key order is pinned.
//!   `type` takes `EFFECT_EDITOR_TYPES[effectType][0]`; an unknown slot
//!   indexes `undefined` and the `[0]` read would throw — outside the typed
//!   domain, modelled as the runner rejecting unknown slots.
//! * `fieldsForType` returns a Set in insertion order: base fields first,
//!   then the type-specific pushes. The nuke branch pushes `density` only
//!   when `type !== "shockwave"`; the trail/other branch is an if/else-if/
//!   else chain. Set semantics: re-adding an existing key keeps its FIRST
//!   position (no duplicates in these paths anyway).

use crate::js_json::{push_map, push_str, push_val, read_val, JsVal};

/// `EFFECT_EDITOR_MAX_COLORS`.
pub const EFFECT_EDITOR_MAX_COLORS: f64 = 8.0;
/// `MAX_NUKE_EXPLOSION_COLORS` (Renderer.ts).
pub const MAX_NUKE_EXPLOSION_COLORS: f64 = 4.0;
/// `NUKE_EXPLOSION_TYPES[0]`.
pub const NUKE_EXPLOSION_TYPE_DEFAULT: &str = "atom";

/// `maxColorsFor(effectType)`.
pub fn max_colors_for(effect_type: &str) -> f64 {
    if effect_type == "nukeExplosion" {
        MAX_NUKE_EXPLOSION_COLORS
    } else {
        EFFECT_EDITOR_MAX_COLORS
    }
}

/// `EFFECT_EDITOR_TYPES` in insertion order.
pub const EFFECT_EDITOR_TYPES: [(&str, &[&str]); 7] = [
    ("transportShipTrail", &["gradient", "transition"]),
    ("nukeTrail", &["gradient", "transition", "spiral"]),
    ("structures", &["gradient", "transition"]),
    ("warship", &["gradient", "transition"]),
    ("train", &["gradient", "transition"]),
    ("railroad", &["gradient", "transition"]),
    ("nukeExplosion", &["shockwave", "sparkles", "embers"]),
];

/// The seven slot keys in `EFFECT_EDITOR_TYPES` order.
pub fn editor_type_options(effect_type: &str) -> Option<&'static [&'static str]> {
    EFFECT_EDITOR_TYPES
        .iter()
        .find(|(k, _)| *k == effect_type)
        .map(|(_, v)| *v)
}

/// `defaultSlotState(effectType)` as a codec object (literal field order).
pub fn default_slot_state(effect_type: &str) -> Option<Vec<(String, JsVal)>> {
    let opts = editor_type_options(effect_type)?;
    let s = |k: &str, v: JsVal| (k.to_string(), v);
    Some(vec![
        s("enabled", JsVal::Bool(false)),
        s("type", JsVal::Str(opts[0].to_string())),
        s("nukeType", JsVal::Str(NUKE_EXPLOSION_TYPE_DEFAULT.to_string())),
        s("colorCount", JsVal::Num(2.0)),
        s(
            "colors",
            JsVal::Arr(
                [
                    "#ff4dd2", "#4dd2ff", "#ffffff", "#ffb84d", "#b84dff", "#4dff88", "#ff4d4d",
                    "#ffff4d",
                ]
                .iter()
                .map(|c| JsVal::Str(c.to_string()))
                .collect(),
            ),
        ),
        s("colorSize", JsVal::Num(4.0)),
        s("movementSpeed", JsVal::Num(10.0)),
        s("frequency", JsVal::Num(1.0)),
        s("radius", JsVal::Num(6.0)),
        s("strands", JsVal::Num(3.0)),
        s("rotationSpeed", JsVal::Num(4.0)),
        s("size", JsVal::Num(210.0)),
        s("speed", JsVal::Num(140.0)),
        s("thickness", JsVal::Num(4.0)),
        s("transitionSpeed", JsVal::Num(0.0)),
        s("density", JsVal::Num(300.0)),
    ])
}

/// `fieldsForType(effectType, type)` — the Set as an insertion-ordered Vec.
pub fn fields_for_type(effect_type: &str, type_: &str) -> Vec<&'static str> {
    let mut base: Vec<&'static str> = vec!["colorCount", "colors"];
    let mut push = |k: &'static str| {
        if !base.contains(&k) {
            base.push(k);
        }
    };
    if effect_type == "nukeExplosion" {
        push("nukeType");
        push("size");
        push("speed");
        push("thickness");
        push("transitionSpeed");
        if type_ != "shockwave" {
            push("density");
        }
        return base;
    }
    if type_ == "transition" {
        push("frequency");
    } else if type_ == "spiral" {
        push("radius");
        push("strands");
        push("rotationSpeed");
    } else {
        push("colorSize");
        push("movementSpeed");
    }
    base
}

// ---------------------------------------------------------------- vectors op
//
// kind 0: [...codec str(effectType)] -> [maxColors]           maxColorsFor
// kind 1: [] -> 7, (key, [2..3, opts...])*7                  EFFECT_EDITOR_TYPES dump
// kind 2: [...codec str(effectType)] -> codec object          defaultSlotState
// kind 3: [...codec str(effectType), ...codec str(type)] ->
//         [k, (field)*k]                                      fieldsForType
// kind 4: [] -> [MAX_COLORS]                                  constants

/// Flat-token runner shared by the golden replay and the wasm probe.
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    match kind {
        0 => {
            let mut i = 0usize;
            let JsVal::Str(t) = read_val(args, &mut i) else {
                unreachable!("effect_editor_state: effectType must be a string");
            };
            out.push(max_colors_for(&t));
        }
        1 => {
            out.push(EFFECT_EDITOR_TYPES.len() as f64);
            for (k, opts) in EFFECT_EDITOR_TYPES {
                push_str(&mut out, k);
                out.push(opts.len() as f64);
                for o in opts {
                    push_str(&mut out, o);
                }
            }
        }
        2 => {
            let mut i = 0usize;
            let JsVal::Str(t) = read_val(args, &mut i) else {
                unreachable!("effect_editor_state: effectType must be a string");
            };
            match default_slot_state(&t) {
                Some(fields) => push_map(&mut out, &fields),
                None => push_val(&mut out, &JsVal::Undef),
            }
        }
        3 => {
            let mut i = 0usize;
            let JsVal::Str(t) = read_val(args, &mut i) else {
                unreachable!("effect_editor_state: effectType must be a string");
            };
            let JsVal::Str(s) = read_val(args, &mut i) else {
                unreachable!("effect_editor_state: type must be a string");
            };
            let fields = fields_for_type(&t, &s);
            out.push(fields.len() as f64);
            for f in fields {
                push_str(&mut out, f);
            }
        }
        4 => out.push(EFFECT_EDITOR_MAX_COLORS),
        k => unreachable!("effect_editor_state: unknown op kind {k}"),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_colors_split() {
        assert_eq!(max_colors_for("nukeExplosion"), 4.0);
        assert_eq!(max_colors_for("nukeTrail"), 8.0);
        assert_eq!(max_colors_for("anything else"), 8.0);
    }

    #[test]
    fn defaults_first_type_per_slot() {
        let d = default_slot_state("nukeTrail").unwrap();
        assert_eq!(d[1], ("type".to_string(), JsVal::Str("gradient".into())));
        let e = default_slot_state("nukeExplosion").unwrap();
        assert_eq!(e[1], ("type".to_string(), JsVal::Str("shockwave".into())));
        assert_eq!(e.len(), 16);
        assert!(default_slot_state("bogus").is_none());
    }

    #[test]
    fn fields_order() {
        assert_eq!(
            fields_for_type("nukeExplosion", "shockwave"),
            vec![
                "colorCount",
                "colors",
                "nukeType",
                "size",
                "speed",
                "thickness",
                "transitionSpeed"
            ]
        );
        assert_eq!(
            fields_for_type("nukeExplosion", "sparkles"),
            vec![
                "colorCount",
                "colors",
                "nukeType",
                "size",
                "speed",
                "thickness",
                "transitionSpeed",
                "density"
            ]
        );
        assert_eq!(
            fields_for_type("nukeTrail", "spiral"),
            vec!["colorCount", "colors", "radius", "strands", "rotationSpeed"]
        );
        assert_eq!(
            fields_for_type("train", "gradient"),
            vec!["colorCount", "colors", "colorSize", "movementSpeed"]
        );
    }
}
