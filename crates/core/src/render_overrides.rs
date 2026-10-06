//! Port of `src/client/render/gl/RenderOverrides.ts` —
//! `applyGraphicsOverrides(settings, overrides)`, the in-place override pass
//! over a [`crate::render_settings`] tree. Pure data transform, no host
//! dependency; the harness models both trees with the [`JsVal`] codec.
//!
//! Faithfulness notes (quirk list):
//!
//! * Every `overrides.X?.Y !== undefined` gate is STRICT: the `?.` swallows a
//!   nullish / non-object CONTAINER (the read yields `undefined` → gate
//!   fails), but a present key whose value is `null` PASSES (`null !==
//!   undefined`) and the raw `null` is assigned — the zod domain never
//!   produces it, the capture pins it anyway.
//! * `overrides.structure?.classicIcons ?? true`: `??` falls back ONLY on
//!   nullish, so absent / `undefined` / `null` all yield `true` and run the
//!   four classic-look writes; `false`, `0` and `""` are NOT nullish and skip
//!   (no else branch — nothing happens).
//! * `overrides.structure?.showDots === false`: only the literal `false`
//!   writes `dotsZoomThreshold = 0`; `true`, absent, `null` and every other
//!   value leave the default alone.
//! * Hex gates (`staleNukeColor`, the two tints, the three affiliation
//!   colors): the strict gate passes any non-`undefined` value; a NON-STRING
//!   then reaches `hexToRgb` and throws a TypeError (`null.trim()` /
//!   `5.trim is not a function`) — the port models that as the throw status.
//!   A valid-format string yields 0–255 channels divided by 255 (f64); an
//!   unparseable string yields `null` and writes NOTHING.
//! * `lighting.ambient`: after assigning the raw value, `enabled = ambient <
//!   1` — relational comparison with JS ToNumber: `NaN < 1` is false,
//!   `Infinity < 1` false, `1 < 1` false, `-0 < 1` TRUE, `null < 1` true
//!   (0), `true < 1` false (1), `false < 1` true (0). Object / array operands
//!   would go through `ToPrimitive`; the scripted domain never feeds them and
//!   the port treats them as NaN (false).
//! * `name.darkNames`: `fillUsePlayerColor = !dark` is a BOOLEAN (logical
//!   not), but `outlineUsePlayerColor = dark` assigns the RAW value (a
//!   non-boolean passes through untouched); the channel picks `0` / `1` by
//!   truthiness.
//! * `passEnabled.fallout` is one toggle driving BOTH `falloutBloom` and
//!   `falloutLight`.
//! * `overrides.palette !== undefined` (a top-level key, no `?.`): the RHS
//!   `createThemeSettings(overrides.palette)` is evaluated BEFORE the
//!   assignment, so an out-of-domain palette throws the upstream
//!   `SyntaxError` with `settings.theme` left at its previous value — the
//!   gates before it already applied (the dump pins the partial mutation).
//!   A valid palette REPLACES the theme in place: the `theme` key exists from
//!   `createRenderSettings` (appended last), so its key position survives.
//! * A nullish `overrides` argument throws on the very first property read
//!   (`overrides.name?.` on null → TypeError) before any mutation.
//! * Settings writes: an existing key overwrites in place (insertion order
//!   preserved); the factories guarantee every target key exists, so no write
//!   ever appends — the port `unreachable!`s on a missing container instead
//!   of modelling the JS TypeError (fx_settings domain precedent).

use crate::color_utils::hex_to_rgb;
use crate::js_json::{map_set, push_val, read_val, val_field, JsVal};
use crate::render_settings::create_theme_settings;

/// Throw codes recorded in the op stream (`0` = the call completed), keyed
/// by the JS error CLASS the golden captures (`e instanceof SyntaxError ? 2
/// : 1`): 1 = TypeError — nullish `overrides` on the first property read, or
/// a non-string hex value reaching `hexToRgb`'s `.trim()`; 2 = SyntaxError —
/// an out-of-domain `palette` through the upstream `JSON.parse(undefined)`.
pub const THREW_TYPE: u8 = 1;
pub const THREW_SYNTAX: u8 = 2;

/// JS truthiness restricted to the codec domain.
fn truthy(v: &JsVal) -> bool {
    match v {
        JsVal::Absent | JsVal::Undef | JsVal::Null => false,
        JsVal::Num(n) => !n.is_nan() && *n != 0.0,
        JsVal::Bool(b) => *b,
        JsVal::Str(s) => !s.is_empty(),
        JsVal::Obj(_) | JsVal::Arr(_) => true,
    }
}

/// `overrides[cont]?.[key] !== undefined` — `None` models the gate failing
/// (nullish / non-object container, absent key, or a present `undefined`).
/// `Some(v)` hands the raw value to the caller (a `null` value PASSES).
fn gate<'a>(ov: &'a JsVal, cont: &str, key: &str) -> Option<&'a JsVal> {
    let c = val_field(ov, cont)?;
    if !matches!(c, JsVal::Obj(_)) {
        return None; // `?.` on a non-object reads undefined
    }
    match val_field(c, key)? {
        JsVal::Absent | JsVal::Undef => None,
        v => Some(v),
    }
}

/// `overrides[key] !== undefined` for a top-level (unguarded) read.
fn gate_top<'a>(ov: &'a JsVal, key: &str) -> Option<&'a JsVal> {
    match val_field(ov, key)? {
        JsVal::Absent | JsVal::Undef => None,
        v => Some(v),
    }
}

/// `overrides.structure?.classicIcons ?? true` — the nullish fallback, then
/// the `if (...)` truthiness of whatever survives `??`.
fn classic_icons_active(ov: &JsVal) -> bool {
    match gate(ov, "structure", "classicIcons") {
        // Absent container / absent key / present undefined all read
        // `undefined`; a present `null` value reads `null`. Both are nullish.
        None => true,
        Some(JsVal::Null) => true,
        Some(v) => truthy(v),
    }
}

/// `settings[cont][key] = value` — the container must be an object (the
/// factories guarantee it; see the module doc).
fn set_in(settings: &mut JsVal, cont: &str, key: &str, v: JsVal) {
    let JsVal::Obj(fields) = settings else {
        unreachable!("render_overrides: settings root must be an object");
    };
    let Some((_, c)) = fields.iter_mut().find(|(k, _)| k == cont) else {
        unreachable!("render_overrides: settings container {cont} missing");
    };
    let JsVal::Obj(sub) = c else {
        unreachable!("render_overrides: settings container {cont} is not an object");
    };
    map_set(sub, key, v);
}

/// `hexToRgb(hex)` over a gate-passed value: `Err(())` models the TypeError
/// a non-string raises inside `hexToRgb`; `Ok(None)` the `null` return for an
/// unparseable string (no writes); `Ok(Some)` the channels already divided
/// by 255 (the f64 division the renderer uniforms want).
fn hex_channels(v: &JsVal) -> Result<Option<[f64; 3]>, ()> {
    match v {
        JsVal::Str(s) => Ok(hex_to_rgb(s).map(|r| [r[0] / 255.0, r[1] / 255.0, r[2] / 255.0])),
        _ => Err(()),
    }
}

/// JS `Number(s)` for the relational `<` coercion, restricted to the
/// domain the capture scripts: whitespace-trimmed decimal / exponent forms,
/// the `Infinity` literals and the empty string. Anything else (hex,
/// garbage) is NaN. The trim set matches `color_utils::js_trim`.
fn js_number_str(s: &str) -> f64 {
    let t = s.trim_matches(|c: char| {
        matches!(
            c,
            '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}'
                | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}'
                | '\u{205f}' | '\u{3000}' | '\u{feff}'
        )
    });
    if t.is_empty() {
        return 0.0;
    }
    if t == "Infinity" || t == "+Infinity" {
        return f64::INFINITY;
    }
    if t == "-Infinity" {
        return f64::NEG_INFINITY;
    }
    t.parse::<f64>().unwrap_or(f64::NAN)
}

/// `x < 1` with JS relational semantics over the codec domain.
fn lt_one(v: &JsVal) -> bool {
    let n = match v {
        JsVal::Num(n) => *n,
        JsVal::Bool(b) => *b as u8 as f64,
        JsVal::Null => 0.0,
        JsVal::Str(s) => js_number_str(s),
        // ToPrimitive over objects/arrays is outside the scripted domain;
        // the JS result there is NaN (both valueOf and the fallback string
        // fail the numeric grammar) -> false.
        JsVal::Absent | JsVal::Undef | JsVal::Obj(_) | JsVal::Arr(_) => f64::NAN,
    };
    n < 1.0
}

/// `applyHexColor` specialised to the three-channel write shape: the writes
/// only happen when `hexToRgb` yields non-null; a non-string throws
/// (TypeError), which the caller maps to `THREW_TYPE`.
fn apply_hex_color(
    settings: &mut JsVal,
    cont: &str,
    keys: [&str; 3],
    v: &JsVal,
) -> Result<(), ()> {
    if let Some([r, g, b]) = hex_channels(v)? {
        set_in(settings, cont, keys[0], JsVal::Num(r));
        set_in(settings, cont, keys[1], JsVal::Num(g));
        set_in(settings, cont, keys[2], JsVal::Num(b));
    }
    Ok(())
}

/// `applyGraphicsOverrides(settings, overrides)` — in place; `Err(code)`
/// models the JS throw propagating out of the function (settings keeps the
/// partial mutation the gates before the throw already applied).
pub fn apply_graphics_overrides(settings: &mut JsVal, overrides: &JsVal) -> Result<(), u8> {
    // The very first property read (`overrides.name?.`) throws on nullish.
    if matches!(overrides, JsVal::Absent | JsVal::Undef | JsVal::Null) {
        return Err(THREW_TYPE);
    }

    if let Some(v) = gate(overrides, "name", "nameScaleFactor") {
        set_in(settings, "name", "nameScaleFactor", v.clone());
    }
    if let Some(v) = gate(overrides, "name", "cullThreshold") {
        set_in(settings, "name", "cullThreshold", v.clone());
    }
    if let Some(v) = gate(overrides, "name", "hoverFadeAlpha") {
        set_in(settings, "name", "hoverFadeAlpha", v.clone());
    }
    if let Some(v) = gate(overrides, "name", "hoverGlowWidth") {
        set_in(settings, "name", "hoverGlowWidth", v.clone());
    }
    if let Some(v) = gate(overrides, "name", "hoverGlowAlpha") {
        set_in(settings, "name", "hoverGlowAlpha", v.clone());
    }
    if let Some(v) = gate(overrides, "cosmetics", "flagOpacity") {
        set_in(settings, "name", "flagAlpha", v.clone());
    }
    if let Some(v) = gate(overrides, "structure", "iconSize") {
        set_in(settings, "structure", "iconSize", v.clone());
    }
    if classic_icons_active(overrides) {
        set_in(settings, "structure", "borderDarken", JsVal::Num(0.7));
        set_in(settings, "structure", "fillDarken", JsVal::Num(1.0));
        set_in(settings, "structure", "iconDarken", JsVal::Num(0.3));
        set_in(settings, "structure", "iconAlpha", JsVal::Num(0.9));
    }
    if let Some(v) = gate(overrides, "structure", "classicNumbers") {
        set_in(settings, "structureLevel", "classicFont", v.clone());
    }
    if gate(overrides, "structure", "showDots") == Some(&JsVal::Bool(false)) {
        set_in(settings, "structure", "dotsZoomThreshold", JsVal::Num(0.0));
    }
    if let Some(v) = gate(overrides, "mapOverlay", "navalHighlight") {
        set_in(settings, "mapOverlay", "navalHighlight", v.clone());
    }
    if let Some(v) = gate(overrides, "mapOverlay", "highlightFillBrighten") {
        set_in(settings, "mapOverlay", "highlightFillBrighten", v.clone());
    }
    if let Some(v) = gate(overrides, "mapOverlay", "highlightBrighten") {
        set_in(settings, "mapOverlay", "highlightBrighten", v.clone());
    }
    if let Some(v) = gate(overrides, "mapOverlay", "highlightThicken") {
        set_in(settings, "mapOverlay", "highlightThicken", v.clone());
    }
    if let Some(v) = gate(overrides, "mapOverlay", "territorySaturation") {
        set_in(settings, "mapOverlay", "territorySaturation", v.clone());
    }
    if let Some(v) = gate(overrides, "mapOverlay", "territoryAlpha") {
        set_in(settings, "mapOverlay", "territoryAlpha", v.clone());
    }
    if let Some(v) = gate(overrides, "mapOverlay", "coordinateGridOpacity") {
        set_in(settings, "mapOverlay", "coordinateGridOpacity", v.clone());
    }
    if let Some(v) = gate(overrides, "mapOverlay", "staleNukeColor") {
        // `if (rgb !== null) { … }` with NO else and NO return: an
        // unparseable hex writes NOTHING but the function CONTINUES to the
        // remaining gates (the friendly/embargo tints, the palette swap, …).
        if let Some([r, g, b]) = hex_channels(v).map_err(|_| THREW_TYPE)? {
            set_in(settings, "mapOverlay", "staleNukeR", JsVal::Num(r));
            set_in(settings, "mapOverlay", "staleNukeG", JsVal::Num(g));
            set_in(settings, "mapOverlay", "staleNukeB", JsVal::Num(b));
        }
    }
    if let Some(v) = gate(overrides, "mapOverlay", "friendlyTintColor") {
        apply_hex_color(settings, "mapOverlay", ["friendlyTintR", "friendlyTintG", "friendlyTintB"], v)
            .map_err(|_| THREW_TYPE)?;
    }
    if let Some(v) = gate(overrides, "mapOverlay", "embargoTintColor") {
        apply_hex_color(settings, "mapOverlay", ["embargoTintR", "embargoTintG", "embargoTintB"], v)
            .map_err(|_| THREW_TYPE)?;
    }
    if let Some(v) = gate(overrides, "mapOverlay", "friendlyTintRatio") {
        set_in(settings, "mapOverlay", "friendlyTintRatio", v.clone());
    }
    if let Some(v) = gate(overrides, "mapOverlay", "embargoTintRatio") {
        set_in(settings, "mapOverlay", "embargoTintRatio", v.clone());
    }
    if let Some(v) = gate(overrides, "altView", "fillAlpha") {
        set_in(settings, "altView", "fillAlpha", v.clone());
    }
    if let Some(v) = gate(overrides, "affiliation", "selfColor") {
        apply_hex_color(settings, "affiliation", ["selfR", "selfG", "selfB"], v)
            .map_err(|_| THREW_TYPE)?;
    }
    if let Some(v) = gate(overrides, "affiliation", "allyColor") {
        apply_hex_color(settings, "affiliation", ["allyR", "allyG", "allyB"], v)
            .map_err(|_| THREW_TYPE)?;
    }
    if let Some(v) = gate(overrides, "affiliation", "enemyColor") {
        apply_hex_color(settings, "affiliation", ["enemyR", "enemyG", "enemyB"], v)
            .map_err(|_| THREW_TYPE)?;
    }
    if let Some(v) = gate(overrides, "railroad", "railMinZoom") {
        set_in(settings, "railroad", "railMinZoom", v.clone());
    }
    if let Some(v) = gate(overrides, "railroad", "railThickness") {
        set_in(settings, "railroad", "railThickness", v.clone());
    }
    if let Some(v) = gate(overrides, "smallPlayerGlow", "strength") {
        set_in(settings, "smallPlayerGlow", "strength", v.clone());
    }
    if let Some(v) = gate(overrides, "passEnabled", "fx") {
        set_in(settings, "passEnabled", "fx", v.clone());
    }
    if let Some(v) = gate(overrides, "passEnabled", "fallout") {
        set_in(settings, "passEnabled", "falloutBloom", v.clone());
        set_in(settings, "passEnabled", "falloutLight", v.clone());
    }
    for key in [
        "backgroundColor",
        "oceanColor",
        "sandColor",
        "plainsColor",
        "highlandColor",
        "mountainColor",
    ] {
        if let Some(v) = gate(overrides, "terrain", key) {
            set_in(settings, "terrain", key, v.clone());
        }
    }
    if let Some(v) = gate(overrides, "lighting", "ambient") {
        let raw = v.clone();
        let enabled = lt_one(v);
        set_in(settings, "lighting", "ambient", raw);
        set_in(settings, "lighting", "enabled", JsVal::Bool(enabled));
    }
    if let Some(v) = gate(overrides, "lighting", "falloffPower") {
        set_in(settings, "lighting", "falloffPower", v.clone());
    }
    if let Some(v) = gate(overrides, "name", "darkNames") {
        let dark = truthy(v);
        set_in(settings, "name", "fillUsePlayerColor", JsVal::Bool(!dark));
        // The RAW value crosses into outlineUsePlayerColor (no coercion).
        set_in(settings, "name", "outlineUsePlayerColor", v.clone());
        let channel = if dark { 0.0 } else { 1.0 };
        set_in(settings, "name", "outlineR", JsVal::Num(channel));
        set_in(settings, "name", "outlineG", JsVal::Num(channel));
        set_in(settings, "name", "outlineB", JsVal::Num(channel));
    }
    if let Some(v) = gate_top(overrides, "palette") {
        let theme = create_theme_settings(Some(v)).ok_or(THREW_SYNTAX)?;
        // The `theme` key exists from createRenderSettings (appended last):
        // map_set overwrites in place, so its key position survives.
        let JsVal::Obj(fields) = settings else {
            unreachable!("render_overrides: settings root must be an object");
        };
        map_set(fields, "theme", theme);
    }
    Ok(())
}

// ---------------------------------------------------------------- vectors op
//
// kind 0: applyGraphicsOverrides [...codec settings, ...codec overrides] ->
//         [status, ...codec settings] — status 0 ok, 1 TypeError (nullish
//         overrides / non-string hex), 2 SyntaxError (out-of-domain
//         palette). The settings dump ALWAYS follows (the partial mutation
//         before a throw is part of the observable).

/// Flat-token runner shared by the golden replay and the wasm probe.
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    let mut i = 0usize;
    match kind {
        0 => {
            let mut settings = read_val(args, &mut i);
            let overrides = read_val(args, &mut i);
            let status = match apply_graphics_overrides(&mut settings, &overrides) {
                Ok(()) => 0.0,
                Err(c) => c as u32 as f64,
            };
            out.push(status);
            push_val(&mut out, &settings);
        }
        k => unreachable!("render_overrides: unknown op kind {k}"),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_settings::create_render_settings;

    fn obj(fields: Vec<(&str, JsVal)>) -> JsVal {
        JsVal::Obj(fields.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }
    fn s(v: &str) -> JsVal {
        JsVal::Str(v.to_string())
    }

    fn field<'a>(v: &'a JsVal, cont: &str, key: &str) -> &'a JsVal {
        val_field(val_field(v, cont).unwrap(), key).unwrap()
    }

    #[test]
    fn empty_overrides_runs_classic_icons() {
        let mut st = create_render_settings();
        apply_graphics_overrides(&mut st, &obj(vec![])).unwrap();
        // The `?? true` fallback fires with no structure key at all.
        assert_eq!(field(&st, "structure", "borderDarken"), &JsVal::Num(0.7));
        assert_eq!(field(&st, "structure", "fillDarken"), &JsVal::Num(1.0));
        assert_eq!(field(&st, "structure", "iconDarken"), &JsVal::Num(0.3));
        assert_eq!(field(&st, "structure", "iconAlpha"), &JsVal::Num(0.9));
        // showDots === false did not fire: the JSON default survives.
        assert_ne!(field(&st, "structure", "dotsZoomThreshold"), &JsVal::Num(0.0));
    }

    #[test]
    fn classic_icons_nullish_and_falsy() {
        // null falls back to true (writes run); false / 0 / "" skip.
        for (v, runs) in [
            (JsVal::Null, true),
            (JsVal::Undef, true),
            (JsVal::Bool(false), false),
            (JsVal::Num(0.0), false),
            (JsVal::Str(String::new()), false),
            (JsVal::Bool(true), true),
        ] {
            let mut st = create_render_settings();
            apply_graphics_overrides(
                &mut st,
                &obj(vec![("structure", obj(vec![("classicIcons", v.clone())]))]),
            )
            .unwrap();
            let runs_now = field(&st, "structure", "iconAlpha") == &JsVal::Num(0.9);
            assert_eq!(runs_now, runs, "classicIcons {v:?}");
        }
    }

    #[test]
    fn strict_gate_null_passes() {
        let mut st = create_render_settings();
        apply_graphics_overrides(
            &mut st,
            &obj(vec![("name", obj(vec![("cullThreshold", JsVal::Null)]))]),
        )
        .unwrap();
        // null !== undefined -> the gate passes and the raw null is assigned.
        assert_eq!(field(&st, "name", "cullThreshold"), &JsVal::Null);
    }

    #[test]
    fn show_dots_strict_false() {
        for (v, fires) in [
            (JsVal::Bool(false), true),
            (JsVal::Bool(true), false),
            (JsVal::Null, false),
            (JsVal::Num(0.0), false),
        ] {
            let mut st = create_render_settings();
            apply_graphics_overrides(
                &mut st,
                &obj(vec![("structure", obj(vec![("showDots", v.clone())]))]),
            )
            .unwrap();
            let fired = field(&st, "structure", "dotsZoomThreshold") == &JsVal::Num(0.0);
            assert_eq!(fired, fires, "showDots {v:?}");
        }
    }

    #[test]
    fn hex_gates() {
        // Valid hex divides by 255; invalid string writes nothing; non-string
        // throws.
        let base = create_render_settings();
        let mut st = base.clone();
        apply_graphics_overrides(
            &mut st,
            &obj(vec![(
                "mapOverlay",
                obj(vec![("staleNukeColor", s("#ff8000")), ("friendlyTintColor", s("zz"))]),
            )]),
        )
        .unwrap();
        assert_eq!(field(&st, "mapOverlay", "staleNukeR"), &JsVal::Num(1.0));
        assert_eq!(field(&st, "mapOverlay", "staleNukeG"), &JsVal::Num(128.0 / 255.0));
        assert_eq!(field(&st, "mapOverlay", "staleNukeB"), &JsVal::Num(0.0));
        // "zz" -> hexToRgb null -> the friendly tint keeps its defaults.
        assert_eq!(
            field(&st, "mapOverlay", "friendlyTintR"),
            field(&base, "mapOverlay", "friendlyTintR")
        );

        let mut st = create_render_settings();
        let r = apply_graphics_overrides(
            &mut st,
            &obj(vec![("mapOverlay", obj(vec![("staleNukeColor", JsVal::Null)]))]),
        );
        assert_eq!(r, Err(THREW_TYPE));
    }

    #[test]
    fn ambient_enabled_gate() {
        for (v, enabled) in [
            (JsVal::Num(f64::NAN), false),
            (JsVal::Num(f64::INFINITY), false),
            (JsVal::Num(1.0), false),
            (JsVal::Num(-0.0), true),
            (JsVal::Num(0.5), true),
            (JsVal::Null, true),
            (JsVal::Bool(true), false),
            (JsVal::Bool(false), true),
            (JsVal::Str("0.5".to_string()), true),
            (JsVal::Str("abc".to_string()), false),
            (JsVal::Str(String::new()), true),
        ] {
            let mut st = create_render_settings();
            apply_graphics_overrides(
                &mut st,
                &obj(vec![("lighting", obj(vec![("ambient", v.clone())]))]),
            )
            .unwrap();
            assert_eq!(
                field(&st, "lighting", "enabled"),
                &JsVal::Bool(enabled),
                "ambient {v:?}"
            );
            // The raw value is assigned to ambient untouched (NaN compared
            // by bit pattern, since PartialEq rejects NaN).
            let got = field(&st, "lighting", "ambient");
            match (&v, got) {
                (JsVal::Num(a), JsVal::Num(b)) => assert!(
                    (a == b && a.is_sign_negative() == b.is_sign_negative())
                        || (a.is_nan() && b.is_nan()),
                    "ambient passthrough"
                ),
                _ => assert_eq!(got, &v),
            }
        }
    }

    #[test]
    fn dark_names_raw_outline() {
        let mut st = create_render_settings();
        apply_graphics_overrides(
            &mut st,
            &obj(vec![("name", obj(vec![("darkNames", s("yes"))]))]),
        )
        .unwrap();
        assert_eq!(field(&st, "name", "fillUsePlayerColor"), &JsVal::Bool(false));
        assert_eq!(field(&st, "name", "outlineUsePlayerColor"), &JsVal::Str("yes".to_string()));
        assert_eq!(field(&st, "name", "outlineR"), &JsVal::Num(0.0));
        assert_eq!(field(&st, "name", "outlineB"), &JsVal::Num(0.0));
    }

    #[test]
    fn fallout_drives_two_passes() {
        let mut st = create_render_settings();
        apply_graphics_overrides(
            &mut st,
            &obj(vec![("passEnabled", obj(vec![("fallout", JsVal::Bool(false))]))]),
        )
        .unwrap();
        assert_eq!(field(&st, "passEnabled", "falloutBloom"), &JsVal::Bool(false));
        assert_eq!(field(&st, "passEnabled", "falloutLight"), &JsVal::Bool(false));
    }

    #[test]
    fn palette_swap_and_throw() {
        let mut st = create_render_settings();
        apply_graphics_overrides(&mut st, &obj(vec![("palette", s("colorblind"))])).unwrap();
        // theme stays the LAST key (in-place replacement).
        let JsVal::Obj(fields) = &st else { panic!("object") };
        assert_eq!(fields.last().unwrap().0, "theme");
        assert_eq!(
            val_field(&st, "theme").unwrap(),
            &create_theme_settings(Some(&s("colorblind"))).unwrap()
        );

        // Bad palette: SyntaxError propagates, theme keeps the previous tree.
        let mut st = create_render_settings();
        let before = val_field(&st, "theme").unwrap().clone();
        let r = apply_graphics_overrides(&mut st, &obj(vec![("palette", s("nope"))]));
        assert_eq!(r, Err(THREW_SYNTAX));
        assert_eq!(val_field(&st, "theme").unwrap(), &before);
    }

    #[test]
    fn stale_nuke_null_does_not_abort() {
        // An unparseable staleNukeColor writes nothing BUT the function keeps
        // going: the later friendly tint and the palette swap must both apply.
        let mut st = create_render_settings();
        apply_graphics_overrides(
            &mut st,
            &obj(vec![
                ("mapOverlay", obj(vec![("staleNukeColor", s("zz")), ("friendlyTintColor", s("#00ff00"))])),
                ("palette", s("colorblind")),
            ]),
        )
        .unwrap();
        // staleNuke channels left at their JSON defaults (untouched).
        let base = create_render_settings();
        assert_eq!(
            field(&st, "mapOverlay", "staleNukeR"),
            field(&base, "mapOverlay", "staleNukeR")
        );
        // friendly tint DID apply (proves no early return).
        assert_eq!(field(&st, "mapOverlay", "friendlyTintG"), &JsVal::Num(1.0));
        // palette swap DID apply.
        assert_eq!(
            val_field(&st, "theme").unwrap(),
            &create_theme_settings(Some(&s("colorblind"))).unwrap()
        );
    }

    #[test]
    fn nullish_overrides_throws_before_mutation() {
        let mut st = create_render_settings();
        let before = st.clone();
        assert_eq!(
            apply_graphics_overrides(&mut st, &JsVal::Null),
            Err(THREW_TYPE)
        );
        assert_eq!(st, before);
    }

    #[test]
    fn run_op_dumps_after_throw() {
        let mut args = Vec::new();
        push_val(&mut args, &create_render_settings());
        push_val(&mut args, &obj(vec![("palette", JsVal::Null)]));
        let res = run_op(0, &args);
        assert_eq!(res[0], THREW_SYNTAX as f64);
        assert!(res.len() > 1);
    }
}
