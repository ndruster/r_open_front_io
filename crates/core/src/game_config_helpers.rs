//! Port of the pure subset of `src/client/utilities/GameConfigHelpers.ts` —
//! the nations-slider mappers, `toOptionalNumber`, the compact-map bots/
//! nations adjusters, `getRandomMapType` and `getUpdatedDisabledUnits`. The
//! three DOM functions (`preventDisallowedKeys`, `parseBoundedIntegerFromInput`,
//! `parseBoundedFloatFromInput`) are out of scope.
//!
//! Faithfulness notes (quirk list):
//!
//! * `sliderToNationsConfig` gates with STRICT `===`: `-0 === 0` is `true`
//!   (→ `"disabled"`), `NaN === 0` is `false`. `nationsConfigToSlider`
//!   compares against the two magic strings and passes anything else through
//!   UNCHANGED (the TS return type lies for off-domain input; the capture
//!   pins a stray string coming back verbatim).
//! * `toOptionalNumber`'s string branch is JS `value.trim()` (the WhiteSpace
//!   ∪ LineTerminator set — U+0085 is NOT trimmed, U+FEFF IS) then `!trimmed`
//!   (empty → `undefined`), then `Number(trimmed)` with full JS numeric
//!   literal coercion (`"0x10"` → 16, `"1e3"` → 1000, `" 12abc"` → NaN,
//!   `"Infinity"` → ±Infinity), then `Number.isFinite` (so `Infinity` →
//!   `undefined`). The number branch is `Number.isFinite` only.
//! * `getNationsForCompactMap` computes `compactCount = Math.max(0,
//!   Math.floor(defaultNationCount * 0.25))` — `Math.floor` (toward
//!   -Infinity, so `-0.5` → `-1` → max → `0`) and `Math.max` through
//!   [`js_max`] (a NaN default yields a NaN `compactCount`, which compares
//!   `=== false` against everything — so on the compact path a NaN default
//!   can only ever pass `nations` through, never return the NaN count).
//! * `getRandomMapType` is `Object.values(GameMapType)` — the 127 canonical
//!   wire NAMES in declaration order ([`crate::maps_gen::GAME_MAP_TYPES`],
//!   the string-enum values only) — indexed by `Math.floor(Math.random() *
//!   127)`. `Math.random()` is scripted through the capture facade (precedent
//!   `__MP_RAND`); out-of-domain indices (`NaN`, `1.0`, negatives) read
//!   `undefined` like a JS array hole.
//! * `getUpdatedDisabledUnits` ALWAYS builds a new array (spread or `filter`
//!   — the old identity never survives); the `!==` filter is STRICT.

use crate::js_json::{push_val, read_val, JsVal};
use crate::jsnum::js_max;

/// JS `Math.floor` — Rust's `f64::floor` matches it on the whole f64 domain
/// (NaN, ±Infinity and -0 included).
#[inline]
fn js_floor(v: f64) -> f64 {
    v.floor()
}

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

/// JS `Number(s)` over the JS-trimmed domain: empty → `0`, the exact
/// `Infinity` spellings, a decimal / exponent literal and the radix prefixes
/// (`0x`/`0X` hex, `0b`/`0B` binary, `0o`/`0O` octal — each with an optional
/// sign, an empty or bad body is `NaN`); anything else is `NaN`. Rust's
/// parser accepts `inf` / `NaN` / `1e999` spellings JS reads differently, so
/// the character gate runs first and the JS-trim (not Rust-trim) is what
/// strips the literal.
pub(crate) fn js_number(s: &str) -> f64 {
    let t = js_trim(s);
    if t.is_empty() {
        return 0.0;
    }
    if t == "Infinity" || t == "+Infinity" {
        return f64::INFINITY;
    }
    if t == "-Infinity" {
        return f64::NEG_INFINITY;
    }
    for (radix, prefixes, digits) in [
        (16.0, ["0x", "0X", "+0x", "+0X", "-0x", "-0X"] as [&str; 6], 16u32),
        (2.0, ["0b", "0B", "+0b", "+0B", "-0b", "-0B"] as [&str; 6], 2u32),
        (8.0, ["0o", "0O", "+0o", "+0O", "-0o", "-0O"] as [&str; 6], 8u32),
    ] {
        for pfx in prefixes {
            if let Some(body) = t.strip_prefix(pfx) {
                if body.is_empty() || !body.chars().all(|c| c.is_digit(digits)) {
                    return f64::NAN;
                }
                let mut mag = 0.0f64;
                for c in body.chars() {
                    mag = mag * radix + f64::from(c.to_digit(digits).unwrap());
                }
                return if t.starts_with('-') { -mag } else { mag };
            }
        }
    }
    let ok = t
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | '.' | 'e' | 'E'));
    if !ok {
        return f64::NAN;
    }
    t.parse::<f64>().unwrap_or(f64::NAN)
}

/// `sliderToNationsConfig(sliderValue, defaultNationCount)`.
pub fn slider_to_nations_config(slider_value: f64, default_nation_count: f64) -> JsVal {
    if slider_value == 0.0 {
        // JS `-0 === 0` is true; the f64 `==` covers it.
        return JsVal::Str("disabled".to_string());
    }
    if slider_value == default_nation_count {
        return JsVal::Str("default".to_string());
    }
    JsVal::Num(slider_value)
}

/// `nationsConfigToSlider(nations, defaultNationCount)` — off-domain values
/// pass through unchanged (the TS signature's lie).
pub fn nations_config_to_slider(nations: &JsVal, default_nation_count: f64) -> JsVal {
    if matches!(nations, JsVal::Str(s) if s == "disabled") {
        return JsVal::Num(0.0);
    }
    if matches!(nations, JsVal::Str(s) if s == "default") {
        return JsVal::Num(default_nation_count);
    }
    nations.clone()
}

/// `toOptionalNumber(value)`.
pub fn to_optional_number(value: &JsVal) -> JsVal {
    match value {
        JsVal::Num(n) => {
            if n.is_finite() {
                JsVal::Num(*n)
            } else {
                JsVal::Undef
            }
        }
        JsVal::Str(s) => {
            let trimmed = js_trim(s);
            if trimmed.is_empty() {
                return JsVal::Undef;
            }
            let numeric = js_number(trimmed);
            if numeric.is_finite() {
                JsVal::Num(numeric)
            } else {
                JsVal::Undef
            }
        }
        _ => JsVal::Undef,
    }
}

/// `getBotsForCompactMap(bots, compactMapEnabled)`.
pub fn get_bots_for_compact_map(bots: f64, compact_map_enabled: bool) -> f64 {
    if compact_map_enabled && bots == 400.0 {
        return 100.0;
    }
    if !compact_map_enabled && bots == 100.0 {
        return 400.0;
    }
    bots
}

/// `getNationsForCompactMap(nations, defaultNationCount, compactMapEnabled)`.
pub fn get_nations_for_compact_map(
    nations: f64,
    default_nation_count: f64,
    compact_map_enabled: bool,
) -> f64 {
    let compact_count = js_max(0.0, js_floor(default_nation_count * 0.25));
    if compact_map_enabled {
        // Only reduce if at the full default
        if nations == default_nation_count {
            return compact_count;
        }
        return nations;
    }
    // Restoring from compact: if at the compact default, go back to full default
    if nations == compact_count {
        return default_nation_count;
    }
    nations
}

/// `getRandomMapType()` for a scripted `Math.random()` draw — the
/// `Object.values(GameMapType)` table is the 127 canonical names in
/// declaration order; an out-of-domain index reads `undefined` (JS array
/// hole).
pub fn get_random_map_type(rand: f64) -> JsVal {
    let maps_len = crate::maps_gen::GAME_MAP_TYPES.len() as f64;
    let rand_idx = js_floor(rand * maps_len);
    if !rand_idx.is_finite() || rand_idx < 0.0 {
        return JsVal::Undef;
    }
    let idx = rand_idx as usize;
    match crate::maps_gen::GAME_MAP_TYPES.get(idx) {
        Some(e) => JsVal::Str(e.value.to_string()),
        None => JsVal::Undef,
    }
}

/// `getUpdatedDisabledUnits(disabledUnits, unit, checked)` — always a NEW
/// array (spread or filter); the `!==` filter is STRICT.
pub fn get_updated_disabled_units(
    disabled_units: &JsVal,
    unit: &JsVal,
    checked: bool,
) -> JsVal {
    let items: &[JsVal] = match disabled_units {
        JsVal::Arr(v) => v,
        _ => &[],
    };
    if checked {
        let mut out = items.to_vec();
        out.push(unit.clone());
        JsVal::Arr(out)
    } else {
        JsVal::Arr(items.iter().filter(|u| !strict_eq(u, unit)).cloned().collect())
    }
}

/// JS `!==` over the codec domain (strings compare by value; the capture
/// feeds scalars).
fn strict_eq(a: &JsVal, b: &JsVal) -> bool {
    match (a, b) {
        (JsVal::Absent, JsVal::Undef) | (JsVal::Undef, JsVal::Absent) => true,
        (JsVal::Absent, JsVal::Absent) | (JsVal::Undef, JsVal::Undef) | (JsVal::Null, JsVal::Null) => true,
        (JsVal::Bool(x), JsVal::Bool(y)) => x == y,
        (JsVal::Num(x), JsVal::Num(y)) => x == y,
        (JsVal::Str(x), JsVal::Str(y)) => x == y,
        _ => false,
    }
}

/// `run_op(kind, args)` — capture harness entry (stateless). Kind table (see
/// `tools/gen_vectors.mjs`):
/// 0 sliderToNationsConfig `[slider, default]` -> codec;
/// 1 nationsConfigToSlider `[...codec(nations), default]` -> codec;
/// 2 toOptionalNumber `[...codec(value)]` -> codec;
/// 3 getBotsForCompactMap `[bots, compact]` -> `[f64]`;
/// 4 getNationsForCompactMap `[nations, default, compact]` -> `[f64]`;
/// 5 getRandomMapType `[rand]` -> `[rand, ...codec(map|undefined)]` (the
///   scripted draw echoes first, mirroring the __MP_RAND-style log);
/// 6 getUpdatedDisabledUnits `[...codec(arr), ...codec(unit), checked]` ->
///   codec(arr).
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    let mut i = 0usize;
    match kind {
        0 => {
            let slider = args[i];
            i += 1;
            let default = args[i];
            push_val(&mut out, &slider_to_nations_config(slider, default));
        }
        1 => {
            let nations = read_val(args, &mut i);
            let default = args[i];
            push_val(&mut out, &nations_config_to_slider(&nations, default));
        }
        2 => {
            let value = read_val(args, &mut i);
            push_val(&mut out, &to_optional_number(&value));
        }
        3 => {
            let bots = args[i];
            i += 1;
            let compact = args[i] != 0.0;
            out.push(get_bots_for_compact_map(bots, compact));
        }
        4 => {
            let nations = args[i];
            i += 1;
            let default = args[i];
            i += 1;
            let compact = args[i] != 0.0;
            out.push(get_nations_for_compact_map(nations, default, compact));
        }
        5 => {
            let rand = args[i];
            out.push(rand);
            push_val(&mut out, &get_random_map_type(rand));
        }
        6 => {
            let arr = read_val(args, &mut i);
            let unit = read_val(args, &mut i);
            let checked = args[i] != 0.0;
            push_val(&mut out, &get_updated_disabled_units(&arr, &unit, checked));
        }
        k => unreachable!("game_config_helpers: unknown op kind {k}"),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn undef(v: &JsVal) -> bool {
        matches!(v, JsVal::Undef)
    }

    #[test]
    fn negative_zero_slider_is_disabled() {
        assert_eq!(
            slider_to_nations_config(-0.0, 25.0),
            JsVal::Str("disabled".to_string())
        );
        assert!(matches!(
            slider_to_nations_config(f64::NAN, 25.0),
            JsVal::Num(n) if n.is_nan()
        ));
    }

    #[test]
    fn number_coercion_matrix() {
        assert_eq!(to_optional_number(&JsVal::Str("0x10".into())), JsVal::Num(16.0));
        assert_eq!(to_optional_number(&JsVal::Str("1e3".into())), JsVal::Num(1000.0));
        assert!(undef(&to_optional_number(&JsVal::Str(" 12abc".into()))));
        assert!(undef(&to_optional_number(&JsVal::Str("Infinity".into()))));
        assert!(undef(&to_optional_number(&JsVal::Str("   ".into()))));
        assert!(undef(&to_optional_number(&JsVal::Num(f64::INFINITY))));
    }

    #[test]
    fn compact_floor_not_trunc() {
        // default = -1.5 -> floor(-0.375) = -1 -> max(0, -1) = 0; nations 0
        // === compactCount 0 on the restore path -> back to -1.5.
        assert_eq!(get_nations_for_compact_map(0.0, -1.5, false), -1.5);
        // NaN default -> compactCount NaN; NaN === anything is false, so the
        // NaN compactCount can never be RETURNED — nations passes through.
        assert_eq!(get_nations_for_compact_map(5.0, f64::NAN, true), 5.0);
        assert_eq!(get_nations_for_compact_map(5.0, f64::NAN, false), 5.0);
    }

    #[test]
    fn random_map_edges() {
        assert!(undef(&get_random_map_type(f64::NAN)));
        assert!(undef(&get_random_map_type(1.0)));
        assert!(undef(&get_random_map_type(-0.5)));
        match get_random_map_type(0.0) {
            JsVal::Str(s) => assert_eq!(s, "Achiran"),
            v => panic!("got {v:?}"),
        }
    }
}
