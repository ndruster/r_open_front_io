//! Port of `src/client/render/gl/RenderSettings.ts` — the theme / render
//! settings factories. The three JSON modules the TS file imports are copied
//! byte-for-byte into `crates/core/data/` and embedded with `include_str!`
//! (a wasm build cannot read the upstream checkout), then parsed once through
//! [`crate::js_json::json_parse`] into the [`JsVal`] codec domain.
//!
//! `dumpSettings` is NOT ported: it is host-bound (`Blob`, `URL.createObjectURL`,
//! `document.createElement`, `URL.revokeObjectURL`) and has no pure-data
//! observable — the same exclusion rule as every other DOM/URL surface in the
//! port.
//!
//! Faithfulness notes (quirk list):
//!
//! * `createThemeSettings(name = "default")`: the default parameter fires ONLY
//!   on `undefined` — an explicit `null` or `""` argument reads `THEMES[null]`
//!   / `THEMES[""]`, which is `undefined`, and `JSON.stringify(undefined)`
//!   returns the `undefined` VALUE (not a string), so `JSON.parse(undefined)`
//!   coerces to the text `"undefined"` and V8 throws
//!   `SyntaxError: undefined is not valid JSON`. The port models every throw
//!   path (out-of-domain name, non-string name) as the `[1]` status token.
//! * `THEMES` has exactly the two `PALETTE_NAMES` keys; any other string (or
//!   number / boolean / null / object) lands on the throw path.
//! * `JSON.parse(JSON.stringify(x))` is a deep copy. The embedded trees
//!   contain no `undefined` / NaN / Infinity / -0 (verified over the data), so
//!   the round trip is an identity on the codec value and a plain `clone()` of
//!   the parsed tree is equivalent — every call hands out an INDEPENDENT copy
//!   (the golden mutates one result and re-reads the next to pin this).
//! * `createRenderSettings()` is `{ ...deepCopy(defaults), theme:
//!   createThemeSettings() }`: the 24 spread keys keep render-settings.json's
//!   own-key order and `theme` is APPENDED LAST (25 keys total). Object.keys
//!   order rides the [`JsVal::Obj`] insertion order.
//! * Parsed objects carry V8 own-key order (integer-like keys first) because
//!   `JSON.parse` materialises properties in ordinary order — see
//!   [`crate::js_json::json_parse`].

use std::sync::OnceLock;

use crate::js_json::{json_parse, map_set, push_val, read_val, JsVal};

/// `render-settings.json` (byte-identical copy of the upstream module).
pub const RENDER_SETTINGS_JSON: &str = include_str!("../data/render-settings.json");
/// `default-theme.json` (byte-identical copy of the upstream module).
pub const DEFAULT_THEME_JSON: &str = include_str!("../data/default-theme.json");
/// `colorblind-theme.json` (byte-identical copy of the upstream module).
pub const COLORBLIND_THEME_JSON: &str = include_str!("../data/colorblind-theme.json");

/// `PALETTE_NAMES` from `GraphicsOverrides.ts` (`as const` tuple). The zod
/// `GraphicsOverridesSchema` itself stays in TS (schema-exclusion precedent);
/// only this constant is needed here and by `render_overrides`.
pub const PALETTE_NAMES: &[&str] = &["default", "colorblind"];

fn parsed_static(slot: &OnceLock<JsVal>, src: &str) -> JsVal {
    slot.get_or_init(|| {
        json_parse(src).expect("embedded render JSON must parse")
    })
    .clone()
}

fn defaults_tree() -> JsVal {
    static DEFAULTS: OnceLock<JsVal> = OnceLock::new();
    parsed_static(&DEFAULTS, RENDER_SETTINGS_JSON)
}

fn theme_tree(name: &str) -> Option<JsVal> {
    match name {
        "default" => {
            static T: OnceLock<JsVal> = OnceLock::new();
            Some(parsed_static(&T, DEFAULT_THEME_JSON))
        }
        "colorblind" => {
            static T: OnceLock<JsVal> = OnceLock::new();
            Some(parsed_static(&T, COLORBLIND_THEME_JSON))
        }
        _ => None,
    }
}

/// `createThemeSettings(name)` — `name` is the raw argument (`None` models the
/// `undefined` that triggers the `"default"` default parameter). `None` return
/// models the `SyntaxError` throw (`THEMES[name]` undefined →
/// `JSON.parse(undefined)`).
pub fn create_theme_settings(name: Option<&JsVal>) -> Option<JsVal> {
    let key: &str = match name {
        None | Some(JsVal::Absent) | Some(JsVal::Undef) => "default",
        Some(JsVal::Str(s)) => s.as_str(),
        // null / number / bool / object / array: THEMES[x] is undefined and
        // the stringify/parse chain throws. Modelled without stringifying the
        // key because no theme key can ever match them.
        _ => return None,
    };
    theme_tree(key)
}

/// `createRenderSettings()` — always succeeds.
pub fn create_render_settings() -> JsVal {
    let mut fields = match defaults_tree() {
        JsVal::Obj(fields) => fields,
        _ => unreachable!("render_settings: defaults must be an object"),
    };
    let theme = create_theme_settings(None).expect("default theme is present");
    // The object literal appends `theme` after the spread keys; the key does
    // not exist in render-settings.json, so map_set appends it last.
    map_set(&mut fields, "theme", theme);
    JsVal::Obj(fields)
}

/// The scripted mutation the independence capture applies to a factory
/// result (`passEnabled.terrain = false`, `theme.teamColors.Red = "#000000"`):
/// if the port ever shared structure with the embedded static tree, the
/// mutation would leak into the FOLLOWING clean dump and diverge from the
/// golden (where the TS deep copy isolates it).
fn mutate_independence_probe(settings: &mut JsVal) {
    set_nested(settings, &["passEnabled", "terrain"], JsVal::Bool(false));
    set_nested(
        settings,
        &["theme", "teamColors", "Red"],
        JsVal::Str("#000000".to_string()),
    );
}

fn set_nested(root: &mut JsVal, path: &[&str], v: JsVal) {
    let mut cur = root;
    for seg in &path[..path.len() - 1] {
        let JsVal::Obj(fields) = cur else {
            unreachable!("render_settings: nested path {seg}");
        };
        cur = fields
            .iter_mut()
            .find(|(k, _)| k == seg)
            .map(|(_, x)| x)
            .unwrap();
    }
    let JsVal::Obj(fields) = cur else {
        unreachable!("render_settings: nested path leaf");
    };
    map_set(fields, path[path.len() - 1], v);
}

// ---------------------------------------------------------------- vectors op
//
// kind 0: createThemeSettings [...codec name] ->
//         [0, ...codec theme] ok | [1] threw (THEMES[name] undefined)
// kind 1: createRenderSettings [] -> [0, ...codec settings] (never throws)
// kind 2: createRenderSettings + the scripted independence mutation (see
//         [`mutate_independence_probe`]) -> [0, ...codec settings]

/// Flat-token runner shared by the golden replay and the wasm probe.
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    let mut i = 0usize;
    match kind {
        0 => {
            let name = read_val(args, &mut i);
            match create_theme_settings(Some(&name)) {
                Some(t) => {
                    out.push(0.0);
                    push_val(&mut out, &t);
                }
                None => out.push(1.0),
            }
        }
        1 => {
            out.push(0.0);
            push_val(&mut out, &create_render_settings());
        }
        2 => {
            let mut st = create_render_settings();
            mutate_independence_probe(&mut st);
            out.push(0.0);
            push_val(&mut out, &st);
        }
        k => unreachable!("render_settings: unknown op kind {k}"),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::js_json::val_field;

    #[test]
    fn theme_default_and_named() {
        let d = create_theme_settings(None).unwrap();
        let e = create_theme_settings(Some(&JsVal::Undef)).unwrap();
        let c = create_theme_settings(Some(&JsVal::Str("colorblind".to_string()))).unwrap();
        // default and explicit-undefined are equal trees; colorblind differs.
        assert_eq!(d, e);
        assert_ne!(d, c);
        let JsVal::Obj(fields) = &d else { panic!("object") };
        assert_eq!(fields[0].0, "teamColors");
        assert_eq!(fields.len(), 12);
    }

    #[test]
    fn theme_throw_paths() {
        // null / empty string / out-of-domain string / number all throw.
        assert_eq!(create_theme_settings(Some(&JsVal::Null)), None);
        assert_eq!(create_theme_settings(Some(&JsVal::Str(String::new()))), None);
        assert_eq!(create_theme_settings(Some(&JsVal::Str("deuteranopia".to_string()))), None);
        assert_eq!(create_theme_settings(Some(&JsVal::Num(0.0))), None);
        assert_eq!(create_theme_settings(Some(&JsVal::Bool(true))), None);
        // Absent models the omitted argument -> the default parameter fires.
        assert!(create_theme_settings(Some(&JsVal::Absent)).is_some());
    }

    #[test]
    fn render_settings_key_order_and_independence() {
        let s = create_render_settings();
        let JsVal::Obj(fields) = &s else { panic!("object") };
        let keys: Vec<&str> = fields.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys.len(), 25);
        assert_eq!(keys[0], "passEnabled");
        assert_eq!(keys[23], "lightConfigs");
        assert_eq!(keys[24], "theme");
        // Each call is an independent deep copy: mutate one, the next is clean.
        let mut s2 = s.clone();
        if let JsVal::Obj(f) = &mut s2 {
            f[0].1 = JsVal::Bool(true);
        }
        let s3 = create_render_settings();
        assert_ne!(s, s2);
        assert_eq!(s, s3);
    }

    #[test]
    fn codec_round_trip_via_run_op() {
        let mut args = Vec::new();
        push_val(&mut args, &JsVal::Str("colorblind".to_string()));
        let res = run_op(0, &args);
        assert_eq!(res[0], 0.0);
        let mut j = 1usize; // res[0] is the status token
        let v = read_val(&res, &mut j);
        assert_eq!(v, create_theme_settings(Some(&JsVal::Str("colorblind".to_string()))).unwrap());

        let res = run_op(0, &[2.0]); // null -> throw
        assert_eq!(res, vec![1.0]);

        let res = run_op(1, &[]);
        assert_eq!(res[0], 0.0);
        let mut j = 1usize;
        let v = read_val(&res, &mut j);
        assert_eq!(v, create_render_settings());
    }

    #[test]
    fn independence_mutation_isolates() {
        // kind 2 mutates its own tree; a following kind 1 dump is clean.
        let res2 = run_op(2, &[]);
        let mut j = 1usize;
        let mutated = read_val(&res2, &mut j);
        let JsVal::Obj(f) = &mutated else { panic!("object") };
        let pe = val_field(&mutated, "passEnabled").unwrap();
        assert_eq!(val_field(pe, "terrain"), Some(&JsVal::Bool(false)));
        let th = val_field(&mutated, "theme").unwrap();
        let tc = val_field(th, "teamColors").unwrap();
        assert_eq!(val_field(tc, "Red"), Some(&JsVal::Str("#000000".to_string())));
        assert_eq!(f.len(), 25);

        let res1 = run_op(1, &[]);
        let mut j = 1usize;
        let clean = read_val(&res1, &mut j);
        assert_eq!(clean, create_render_settings());
        assert_ne!(clean, mutated);
    }
}
