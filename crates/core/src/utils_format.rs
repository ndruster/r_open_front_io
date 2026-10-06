//! Port of the pure formatting subset of `src/client/Utils.ts`:
//! `renderNumber` / `renderTroops` / `formatPercentage` / `normaliseMapKey` /
//! `presenceMapKey` / `formatKeyForDisplay` / `formatDebugTranslation`.
//!
//! The translation-bound functions (`getMapName`, `getGameModeLabel`,
//! `getActiveModifiers`, `getModifierLabels`, `renderDuration`,
//! `translateText`, `isRTL`, `textDirection`) depend on `translateText` ->
//! `intl-messageformat` + the DOM `lang-selector` and stay EXCLUDED; the
//! host-bound ones (`copyToClipboard`, `createCanvas`,
//! `generateCryptoRandomUUID`, `getSvgAspectRatio`, `showToast*`,
//! `reloadForUpdate`, `homeHref`, `isInIframe`, `getGamesPlayed`,
//! `getModifierKey`/`getAltKey`) stay excluded too. `apexPathFor`,
//! `currentPagePath`, the three `Date.now()` time functions and
//! `getDiscordAvatarUrl` are ported in [`crate::utils_nav`].
//! `normaliseMapKey`'s `maps.find(m => m.type === mapName)?.id` rides the
//! ported `maps_gen::MAPS` table (the same data the TS `maps` export holds).
//!
//! Faithfulness notes (quirk list):
//!
//! * `renderNumber` clamps through NaN-propagating `Math.max(num, 0)`: NaN
//!   survives the clamp, fails every `>=` gate and lands on the `else`
//!   branch — `Math.floor(NaN).toString()` is `"NaN"`. `Infinity` passes the
//!   first gate and `toFixed` renders `"Infinity"` (`"InfinityB"`).
//! * `fixedPoints ?? d` is a NULLISH gate, not falsy: an explicit `0` is
//!   honoured (`toFixed(0)` half-up: 1.5 -> `"2"`), an explicit `NaN` becomes
//!   `toFixed`'s `ToIntegerOrInfinity` 0, and fractional digits truncate
//!   toward zero. Digits outside `0..=100` THROW a RangeError in JS — out of
//!   the captured domain.
//! * The `>= 100_000` branch has NO `toFixed`: `Math.floor(num / 1000)` is
//!   string-concatenated straight onto `"K"` through `Number::toString`
//!   (`1e21` would print `"1e+21"` — unreachable in this branch, but the
//!   `toFixed` fallback at `|v| >= 1e21` is reachable from the B branches).
//! * `formatPercentage`'s NaN gate is `Number.isNaN` (strict): `Infinity`
//!   renders `"Infinity%"`, and `-0 * 100 = -0` goes through `toFixed`'s
//!   strict `x < 0` sign gate (unsigned `"0.0%"`).
//! * `normaliseMapKey` lowercases with JS Unicode `toLowerCase` (final-sigma
//!   context included — Rust's `str::to_lowercase` implements the same
//!   Unicode Default mappings) BEFORE stripping `/[\s.]+/g`; the JS `\s` set
//!   equals the `trim` set (U+0085 NOT whitespace, U+FEFF IS).
//! * `formatKeyForDisplay` recurses on `Shift+` (so `"Shift+Shift+KeyA"` ->
//!   `"Shift+Shift+A"`), the `Digit\d` / `Key[A-Z]` regexes are anchored and
//!   ASCII-only (`"Keya"` and `"Digit12"` fall through), and the fallback
//!   `charAt(0).toUpperCase() + slice(1)` is UTF-16-unit based: `"ß"` grows
//!   to `"SS"` (length quirk), while an astral first unit (a lone high
//!   surrogate, whose `toUpperCase` is the identity) reassembles to the same
//!   string the char-based Rust form produces — the observable output is
//!   equal either way, so the port stays char-based over the BMP domain.
//! * `formatDebugTranslation` serialises `Object.entries(params)` in V8 own
//!   key order (the codec carries it) and `String(value)` follows
//!   `Number::toString` for numbers (`-0` -> `"0"`, `1e21` -> `"1e+21"`).

use crate::js_fixed::{js_to_string, to_fixed};
use crate::js_json::{push_val, read_val, JsVal};
use crate::jsnum::js_max;

/// `Number.prototype.toFixed`'s digits coercion over the CAPTURED domain
/// (`0..=100` after truncation): NaN -> 0, fractions truncate toward zero.
/// Digits outside `0..=100` (and the infinities) THROW a RangeError in JS —
/// out of domain, never captured; the defensive clamp below only keeps the
/// Rust side total.
fn fixed_digits(v: f64) -> u32 {
    if v.is_nan() || v <= 0.0 {
        0
    } else if v.is_infinite() {
        100
    } else {
        (v.trunc() as u32).min(100)
    }
}

/// `renderNumber(num, fixedPoints)` over the numeric domain. `fp_flag == 0`
/// models the parameter being ABSENT (or nullish): the branch default `d`
/// applies; `fp_flag != 0` uses the explicit `fp` through the `toFixed`
/// digits coercion (`??` is a nullish gate, so an explicit `0` / `NaN` is
/// honoured — the capture never passes `null`/`undefined` as a raw f64).
fn render_number(num: f64, fp_flag: f64, fp: f64) -> String {
    let num = js_max(num, 0.0);
    let digits = |dflt: f64| -> u32 {
        if fp_flag == 0.0 {
            dflt as u32
        } else {
            fixed_digits(fp)
        }
    };
    if num >= 10_000_000_000.0 {
        let value = (num / 100_000_000.0).floor() / 10.0;
        format!("{}B", to_fixed(value, digits(1.0)))
    } else if num >= 1_000_000_000.0 {
        let value = (num / 10_000_000.0).floor() / 100.0;
        format!("{}B", to_fixed(value, digits(2.0)))
    } else if num >= 10_000_000.0 {
        let value = (num / 100_000.0).floor() / 10.0;
        format!("{}M", to_fixed(value, digits(1.0)))
    } else if num >= 1_000_000.0 {
        let value = (num / 10_000.0).floor() / 100.0;
        format!("{}M", to_fixed(value, digits(2.0)))
    } else if num >= 100_000.0 {
        format!("{}K", js_to_string((num / 1000.0).floor()))
    } else if num >= 10_000.0 {
        let value = (num / 100.0).floor() / 10.0;
        format!("{}K", to_fixed(value, digits(1.0)))
    } else if num >= 1000.0 {
        let value = (num / 10.0).floor() / 100.0;
        format!("{}K", to_fixed(value, digits(2.0)))
    } else {
        js_to_string(num.floor())
    }
}

/// `renderTroops(troops)` — `renderNumber(troops / 10)` with the default
/// fixedPoints (the TS call site never passes one).
fn render_troops(troops: f64) -> String {
    render_number(troops / 10.0, 0.0, 0.0)
}

/// `formatPercentage(value)`.
fn format_percentage(value: f64) -> String {
    let perc = value * 100.0;
    if perc.is_nan() {
        return "0%".to_string();
    }
    format!("{}%", to_fixed(perc, 1))
}

/// JS `\s` (WhiteSpace ∪ LineTerminator) — the same set as `String.trim`.
fn is_js_ws(c: char) -> bool {
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
}

/// `normaliseMapKey(mapName)`: the `maps.find(m => m.type === mapName)?.id ??
/// mapName` lookup over the ported MAPS table, then lowercase, then strip
/// `/[\s.]+/g`.
pub fn normalise_map_key(map_name: &str) -> String {
    let id = crate::maps_gen::MAPS
        .iter()
        .find(|m| m.type_ == map_name)
        .map(|m| m.id);
    let base = id.unwrap_or(map_name);
    base.to_lowercase()
        .chars()
        .filter(|c| !is_js_ws(*c) && *c != '.')
        .collect()
}

/// `presenceMapKey(gameMap)` over the codec domain (string | undefined).
fn presence_map_key(game_map: &JsVal) -> JsVal {
    match game_map {
        JsVal::Undef => JsVal::Undef,
        JsVal::Str(s) => JsVal::Str(normalise_map_key(s)),
        _ => unreachable!("utils_format: presenceMapKey arg must be str|undef"),
    }
}

/// `/^Digit\d$/` — anchored, exactly six UTF-16 units, ASCII-only tail.
fn is_digit_pattern(s: &str) -> bool {
    s.len() == 6 && s.starts_with("Digit") && s.as_bytes()[5].is_ascii_digit()
}

/// `/^Key[A-Z]$/`.
fn is_key_pattern(s: &str) -> bool {
    s.len() == 4 && s.starts_with("Key") && s.as_bytes()[3].is_ascii_uppercase()
}

/// `formatKeyForDisplay(value)`.
fn format_key_for_display(value: &str) -> String {
    if value.is_empty() {
        return String::new(); // `!value` on the string domain: only "" is falsy
    }
    // "Shift+" is ASCII: the byte prefix is the code-unit prefix.
    if let Some(rest) = value.strip_prefix("Shift+") {
        return format!("Shift+{}", format_key_for_display(rest));
    }
    if value == " " || value == "Space" {
        return "Space".to_string();
    }
    if is_digit_pattern(value) {
        return value[5..].to_string();
    }
    if is_key_pattern(value) {
        return value[3..].to_string();
    }
    let mut chars = value.chars();
    let first = chars.next().unwrap();
    let mut out: String = first.to_uppercase().collect();
    out.push_str(chars.as_str());
    out
}

/// `String(value)` over the codec domain (the TS signature says
/// `string | number`; the capture also pins the nullish / boolean renders
/// the template literal would produce for out-of-signature values).
fn string_of(v: &JsVal) -> String {
    match v {
        JsVal::Undef => "undefined".to_string(),
        JsVal::Null | JsVal::Absent => "null".to_string(),
        JsVal::Num(n) => js_to_string(*n),
        JsVal::Bool(b) => (if *b { "true" } else { "false" }).to_string(),
        JsVal::Str(s) => s.clone(),
        JsVal::Arr(items) => items
            .iter()
            .map(|i| match i {
                JsVal::Undef | JsVal::Null | JsVal::Absent => String::new(),
                other => string_of(other),
            })
            .collect::<Vec<_>>()
            .join(","),
        JsVal::Obj(_) => "[object Object]".to_string(),
    }
}

/// `Object.entries` over the codec domain: Obj fields in carried order, Arr
/// index keys, Str one-character index keys (BMP domain — the same model as
/// `cosmetic_visibility::entries_of`; JS indexes by UTF-16 unit, the capture
/// stays BMP so chars == units), everything else empty.
fn entries_of(v: &JsVal) -> Vec<(String, JsVal)> {
    match v {
        JsVal::Obj(fields) => fields.clone(),
        JsVal::Arr(items) => items
            .iter()
            .enumerate()
            .map(|(i, it)| (i.to_string(), it.clone()))
            .collect(),
        JsVal::Str(s) => s
            .chars()
            .enumerate()
            .map(|(i, c)| (i.to_string(), JsVal::Str(c.to_string())))
            .collect(),
        _ => Vec::new(),
    }
}

/// `formatDebugTranslation(key, params)`.
fn format_debug_translation(key: &str, params: &JsVal) -> String {
    let entries = entries_of(params);
    if entries.is_empty() {
        return key.to_string();
    }
    let serialized = entries
        .iter()
        .map(|(k, v)| format!("{k}={}", string_of(v)))
        .collect::<Vec<_>>()
        .join(",");
    format!("{key}::{serialized}")
}

/// Stateless op dispatcher for the parity probes / golden replay.
///
/// Kind table (matches `tools/gen_vectors.mjs` `runUF`):
/// * `0` renderNumber `[num, fpFlag, fp]` -> `[...codec(str)]`
/// * `1` renderTroops `[troops]` -> `[...codec(str)]`
/// * `2` formatPercentage `[value]` -> `[...codec(str)]`
/// * `3` normaliseMapKey `[...codec(str)]` -> `[...codec(str)]`
/// * `4` presenceMapKey `[...codec(str|undef)]` -> `[...codec(str|undef)]`
/// * `5` formatKeyForDisplay `[...codec(str)]` -> `[...codec(str)]`
/// * `6` formatDebugTranslation `[...codec(key), ...codec(params)]` -> `[...codec(str)]`
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let res = match kind {
        0 => JsVal::Str(render_number(args[0], args[1], args[2])),
        1 => JsVal::Str(render_troops(args[0])),
        2 => JsVal::Str(format_percentage(args[0])),
        3 => {
            let mut i = 0usize;
            let JsVal::Str(s) = read_val(args, &mut i) else {
                unreachable!("utils_format: normaliseMapKey arg must be a string");
            };
            JsVal::Str(normalise_map_key(&s))
        }
        4 => {
            let mut i = 0usize;
            let v = read_val(args, &mut i);
            presence_map_key(&v)
        }
        5 => {
            let mut i = 0usize;
            let JsVal::Str(s) = read_val(args, &mut i) else {
                unreachable!("utils_format: formatKeyForDisplay arg must be a string");
            };
            JsVal::Str(format_key_for_display(&s))
        }
        6 => {
            let mut i = 0usize;
            let JsVal::Str(key) = read_val(args, &mut i) else {
                unreachable!("utils_format: formatDebugTranslation key must be a string");
            };
            let params = read_val(args, &mut i);
            JsVal::Str(format_debug_translation(&key, &params))
        }
        _ => unreachable!("utils_format: bad op kind {kind}"),
    };
    let mut out = Vec::new();
    push_val(&mut out, &res);
    out
}
