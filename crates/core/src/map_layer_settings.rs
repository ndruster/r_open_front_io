//! Port of `src/client/MapLayerSettings.ts` — the two `??`-chained reads of
//! stored graphics overrides (`isLayerVisible` / `layerAlpha`). The
//! imperative `pushMapLayerState` (renderer callbacks) stays in TS.
//!
//! Faithfulness notes:
//!
//! * `overrides.mapLayerVisibility?.[layerId] ?? true`: the `?.` guards only
//!   the map field (null/undefined/absent → `undefined` → `??` fallback);
//!   the per-layer value falls back ONLY on `null`/`undefined` — a `false`
//!   boolean is kept, and any non-boolean value passes through `??` and is
//!   returned as the truthiness the caller sees. The capture records the
//!   RAW value (codec), not a coercion.
//! * `layerAlpha` chains two `??`: override value, then `manifestDefault`
//!   (absent parameter reads `undefined`), then `1`. A non-number override
//!   (e.g. a string) crosses through unchanged — the codec keeps it.

use crate::js_json::{push_val, read_val, val_field, JsVal};

/// `isLayerVisible(overrides, layerId)` — the raw `??`-resolved value.
pub fn is_layer_visible(overrides: &JsVal, layer_id: &str) -> JsVal {
    let map = val_field(overrides, "mapLayerVisibility");
    let v = match map {
        Some(m @ JsVal::Obj(_)) => val_field(m, layer_id).cloned(),
        _ => None,
    };
    match v {
        None | Some(JsVal::Absent) | Some(JsVal::Undef) | Some(JsVal::Null) => JsVal::Bool(true),
        Some(x) => x,
    }
}

/// `layerAlpha(overrides, layerId, manifestDefault)` — the raw `??`-resolved
/// value (`manifestDefault` may be absent → `undefined`).
pub fn layer_alpha(overrides: &JsVal, layer_id: &str, manifest_default: &JsVal) -> JsVal {
    let map = val_field(overrides, "mapLayerAlpha");
    let v = match map {
        Some(m @ JsVal::Obj(_)) => val_field(m, layer_id).cloned(),
        _ => None,
    };
    match v {
        None | Some(JsVal::Absent) | Some(JsVal::Undef) | Some(JsVal::Null) => {
            match manifest_default {
                JsVal::Absent | JsVal::Undef | JsVal::Null => JsVal::Num(1.0),
                x => x.clone(),
            }
        }
        Some(x) => x,
    }
}

// ---------------------------------------------------------------- vectors op
//
// kind 0: [...codec overrides, ...codec layerId] -> [...codec value]
//         isLayerVisible
// kind 1: [...codec overrides, ...codec layerId, ...codec manifestDefault]
//         -> [...codec value]  layerAlpha

/// Flat-token runner shared by the golden replay and the wasm probe.
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    let mut i = 0usize;
    match kind {
        0 => {
            let overrides = read_val(args, &mut i);
            let JsVal::Str(layer_id) = read_val(args, &mut i) else {
                unreachable!("map_layer_settings: layerId must be a string");
            };
            push_val(&mut out, &is_layer_visible(&overrides, &layer_id));
        }
        1 => {
            let overrides = read_val(args, &mut i);
            let JsVal::Str(layer_id) = read_val(args, &mut i) else {
                unreachable!("map_layer_settings: layerId must be a string");
            };
            let md = read_val(args, &mut i);
            push_val(&mut out, &layer_alpha(&overrides, &layer_id, &md));
        }
        k => unreachable!("map_layer_settings: unknown op kind {k}"),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(fields: Vec<(&str, JsVal)>) -> JsVal {
        JsVal::Obj(fields.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }

    #[test]
    fn visibility_chain() {
        let ov = obj(vec![(
            "mapLayerVisibility",
            obj(vec![("a", JsVal::Bool(false)), ("b", JsVal::Null)]),
        )]);
        assert_eq!(is_layer_visible(&ov, "a"), JsVal::Bool(false));
        // null falls through ?? to true.
        assert_eq!(is_layer_visible(&ov, "b"), JsVal::Bool(true));
        // absent layer, absent map, non-object overrides all → true.
        assert_eq!(is_layer_visible(&ov, "c"), JsVal::Bool(true));
        assert_eq!(is_layer_visible(&obj(vec![]), "a"), JsVal::Bool(true));
        assert_eq!(is_layer_visible(&JsVal::Null, "a"), JsVal::Bool(true));
    }

    #[test]
    fn alpha_chain() {
        let ov = obj(vec![(
            "mapLayerAlpha",
            obj(vec![("a", JsVal::Num(0.5)), ("z", JsVal::Num(0.0))]),
        )]);
        assert_eq!(layer_alpha(&ov, "a", &JsVal::Num(0.8)), JsVal::Num(0.5));
        // 0 is NOT nullish — the override wins.
        assert_eq!(layer_alpha(&ov, "z", &JsVal::Num(0.8)), JsVal::Num(0.0));
        assert_eq!(layer_alpha(&ov, "b", &JsVal::Num(0.8)), JsVal::Num(0.8));
        assert_eq!(layer_alpha(&ov, "b", &JsVal::Absent), JsVal::Num(1.0));
        assert_eq!(layer_alpha(&ov, "b", &JsVal::Undef), JsVal::Num(1.0));
        assert_eq!(layer_alpha(&ov, "b", &JsVal::Null), JsVal::Num(1.0));
    }
}
