//! Port of `src/client/PlayerName.ts` — the name-precedence rules. The
//! crypto-dependent (`genAnonUsername` / `fallbackPlayerName`) and the
//! NFKD/validateAccountUsername-dependent (`sanitizeAccountPersona`)
//! functions stay in TS.
//!
//! Faithfulness notes (quirk list):
//!
//! * `clampUsername` / `truncateToCap` measure JS `string.length` = UTF-16
//!   code UNITS (astral chars count 2), and `slice` cuts units — a cut can
//!   split a surrogate pair (the capture crosses units so the Rust port
//!   keeps the unit domain).
//! * `accountVerifiedName`: `!player.username` is TRUTHY (empty string
//!   fails); the base gate reads `isTemporaryUsername(player.usernameBase)`
//!   (non-string → false); the bare-name gate is `username !== base`
//!   STRICT.
//! * `verifiedClaimGrace`: `!name || !at` — an EMPTY-STRING `at` is falsy
//!   → null. `new Date(iso).getTime()` over the `z.iso.datetime()` domain
//!   is pure UTC arithmetic ([`iso_to_epoch_ms`]); an unparseable string
//!   yields NaN, and `NaN <= now` is false → `atRisk` false. The `now`
//!   default argument (`new Date()`) is scripted by the capture: the runner
//!   takes `now_ms` explicitly (JS only applies the default when the
//!   argument is `undefined`).
//! * `looksGenerated`: `/^Anon([A-Za-z]+)\d?$/u` — the greedy `[A-Za-z]+`
//!   backtracks for the optional digit: `"AnonCat12"` does NOT match (two
//!   trailing digits), `"AnonCat1"` does. `ANON_WORDS.includes` compares
//!   the captured group.
//! * `sanitizePersona`: `Array.from` iterates CODE POINTS (a surrogate pair
//!   is one `ch`, never in the renderable class → one space, not two);
//!   `replace(/\s+/g, " ")` uses the JS `\s` set (U+0085 is NOT whitespace,
//!   U+FEFF IS); the length gate is UTF-16 units (`name.length < 3`).
//! * `truncateToCap` is module-private in TS; ported as pub(crate) so the
//!   runner and `sanitizePersona` share it.
//! * `resolvePlayerName`: branch 1 gates `verifiedOptIn && verifiedName !==
//!   null` (STRICT; the typed domain is string|null so undefined never
//!   arrives); branch 2 `storedName?.trim()` then TRUTHY (a whitespace-only
//!   stored name trims to `""` → falls through).

use crate::api_schemas::is_temporary_username;
use crate::js_json::{push_str, push_val, read_str, read_val, val_field, JsVal};
use crate::schemas::{has_renderable_alnum, is_renderable_name_char};

/// `MIN_USERNAME_LENGTH`.
pub const MIN_USERNAME_LENGTH: usize = 3;
/// `MAX_USERNAME_LENGTH`.
pub const MAX_USERNAME_LENGTH: usize = 20;
/// `LAPSE_NOTICE_KEY`.
pub const LAPSE_NOTICE_KEY: &str = "verifiedLapseNotice";

/// JS `String.prototype.trim` (WhiteSpace ∪ LineTerminator; U+0085 NOT in
/// the set, U+FEFF IS).
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

/// The JS `\s` set for the collapse regex (same members as js_trim).
pub(crate) fn is_js_space(c: char) -> bool {
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

fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn from_units(u: &[u16]) -> String {
    String::from_utf16_lossy(u)
}

/// `clampUsername(name)`.
pub fn clamp_username(name: &str) -> String {
    let u = units(name);
    if u.len() > MAX_USERNAME_LENGTH {
        js_trim(&from_units(&u[..MAX_USERNAME_LENGTH])).to_string()
    } else {
        name.to_string()
    }
}

fn field_str<'a>(v: &'a JsVal, key: &str) -> Option<&'a str> {
    match val_field(v, key) {
        Some(JsVal::Str(s)) => Some(s.as_str()),
        _ => None,
    }
}

fn truthy(v: Option<&JsVal>) -> bool {
    match v {
        None | Some(JsVal::Absent | JsVal::Undef | JsVal::Null) => false,
        Some(JsVal::Num(n)) => !n.is_nan() && *n != 0.0,
        Some(JsVal::Bool(b)) => *b,
        Some(JsVal::Str(s)) => !s.is_empty(),
        Some(JsVal::Obj(_) | JsVal::Arr(_)) => true,
    }
}

/// The `userMe.player` sub-object, or `Undef` when the path is absent
/// (`userMe` null/false is gated by the callers before this read).
fn player_of(user_me: &JsVal) -> JsVal {
    val_field(user_me, "player").cloned().unwrap_or(JsVal::Undef)
}

/// `accountVerifiedName(userMe)` — `None` models null.
pub fn account_verified_name(user_me: &JsVal) -> Option<String> {
    if matches!(user_me, JsVal::Null | JsVal::Bool(false)) {
        return None;
    }
    let player = player_of(user_me);
    let status = val_field(&player, "usernameStatus");
    let entitled = matches!(status, Some(JsVal::Str(s)) if s == "premium" || s == "indefinite");
    if !entitled {
        return None;
    }
    let username = val_field(&player, "username");
    if !truthy(username) {
        return None;
    }
    if match username {
        Some(JsVal::Str(s)) => is_temporary_username(&units(s)),
        _ => false,
    } {
        return None;
    }
    let base = field_str(&player, "usernameBase");
    if base.is_some_and(|b| is_temporary_username(&units(b))) {
        return None;
    }
    let name = field_str(&player, "username")?;
    if Some(name) != base {
        return None;
    }
    Some(name.to_string())
}

/// `accountNameHeld(userMe)`.
pub fn account_name_held(user_me: &JsVal) -> bool {
    if matches!(user_me, JsVal::Null | JsVal::Bool(false)) {
        return false;
    }
    let player = player_of(user_me);
    let entitled = matches!(val_field(&player, "usernameStatus"), Some(JsVal::Str(s)) if s == "premium" || s == "indefinite");
    if !entitled {
        return false;
    }
    let username = val_field(&player, "username");
    let base = val_field(&player, "usernameBase");
    if !truthy(username) || !truthy(base) {
        return false;
    }
    username != base
}

/// `verifiedNameOptIn(stored, defaultAllowed)` — `None` models null.
pub fn verified_name_opt_in(stored: Option<&str>, default_allowed: bool) -> bool {
    match stored {
        Some("true") => true,
        Some("false") => false,
        _ => default_allowed,
    }
}

/// `new Date(iso).getTime()` over the `z.iso.datetime()` domain:
/// `YYYY-MM-DDTHH:mm:ss(.sss)?(Z|±hh:mm)`. `None` models the Invalid Date
/// (NaN). The day count is Howard Hinnant's civil-algorithm (exact for the
/// proleptic Gregorian range JS Date covers, years 0..=9999 + the two-digit
/// 00-99 → 2000-2099 mapping).
pub fn iso_to_epoch_ms(iso: &str) -> Option<f64> {
    let b = iso.as_bytes();
    let digit = |i: usize| b.get(i).is_some_and(|c| c.is_ascii_digit());
    if b.len() < 10 || !digit(0) || !digit(1) || !digit(2) || !digit(3) || b[4] != b'-' {
        return None;
    }
    if !digit(5) || !digit(6) || b[7] != b'-' || !digit(8) || !digit(9) {
        return None;
    }
    let year: i64 = iso[0..4].parse().ok()?;
    let month: i64 = iso[5..7].parse().ok()?;
    let day: i64 = iso[8..10].parse().ok()?;
    let mut hour = 0i64;
    let mut min = 0i64;
    let mut sec = 0i64;
    let mut ms = 0i64;
    let mut rest = 10usize;
    if b.get(rest) == Some(&b'T') || b.get(rest) == Some(&b't') {
        rest += 1;
        if !digit(rest) || !digit(rest + 1) || b.get(rest + 2) != Some(&b':') {
            return None;
        }
        hour = iso[rest..rest + 2].parse().ok()?;
        rest += 3;
        if !digit(rest) || !digit(rest + 1) || b.get(rest + 2) != Some(&b':') {
            return None;
        }
        min = iso[rest..rest + 2].parse().ok()?;
        rest += 3;
        if !digit(rest) || !digit(rest + 1) {
            return None;
        }
        sec = iso[rest..rest + 2].parse().ok()?;
        rest += 2;
        if b.get(rest) == Some(&b'.') {
            rest += 1;
            let start = rest;
            while digit(rest) {
                rest += 1;
            }
            if rest == start {
                return None;
            }
            let frac = &iso[start..rest];
            ms = if frac.len() <= 3 {
                frac.parse::<i64>().ok()? * 10i64.pow(3 - frac.len() as u32)
            } else {
                // JS keeps only the first three digits (millisecond field).
                frac[..3].parse::<i64>().ok()?
            };
        }
    } else if !matches!(b.get(rest), None | Some(&b'Z') | Some(&b'z')) {
        // Date-only (`YYYY-MM-DD`, end of string) and `…Z` are UTC; any
        // other separator character is an Invalid Date.
        return None;
    }
    // Time zone: Z / absent (date-only is UTC) / ±hh:mm / ±hhmm. V8's
    // Date.parse accepts the colon-less `+0530` form and REJECTS anything
    // trailing the Z designator (`…Zjunk` is an Invalid Date); both probed
    // against the golden capture.
    let mut offset_ms = 0i64;
    match b.get(rest) {
        None => {}
        Some(b'Z') | Some(b'z') => {
            if rest + 1 != b.len() {
                return None;
            }
        }
        Some(c @ (b'+' | b'-')) => {
            let (oh, om) = if b.len() == rest + 6 && b[rest + 3] == b':' {
                (rest + 1..rest + 3, rest + 4..rest + 6)
            } else if b.len() == rest + 5 {
                (rest + 1..rest + 3, rest + 3..rest + 5)
            } else {
                return None;
            };
            if !(oh.clone().all(digit) && om.clone().all(digit)) {
                return None;
            }
            let oh: i64 = iso[oh].parse().ok()?;
            let om: i64 = iso[om].parse().ok()?;
            let sign = if *c == b'+' { 1 } else { -1 };
            offset_ms = sign * (oh * 60 + om) * 60_000;
        }
        Some(_) => return None,
    }
    if !(0..=23).contains(&hour) || !(0..=59).contains(&min) {
        return None;
    }
    // JS accepts 60 for `sec` (leap second normalisation) but it only ever
    // appears at 23:59:60; the z.iso.datetime() domain never emits it.
    if !(0..=59).contains(&sec) {
        return None;
    }
    if !(1..=12).contains(&month) || day < 1 {
        return None;
    }
    let days = days_from_civil(year, month, day);
    let epoch = days * 86_400_000 + hour * 3_600_000 + min * 60_000 + sec * 1000 + ms - offset_ms;
    // JS Date range: |epoch| <= 8.64e15 ms.
    if epoch.abs() > 8_640_000_000_000_000 {
        return None;
    }
    Some(epoch as f64)
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The grace triple `(name, expiresAt_ms, atRisk)`.
#[derive(Clone, Debug, PartialEq)]
pub struct ClaimGrace {
    pub name: String,
    pub expires_at_ms: f64,
    pub at_risk: bool,
}

/// `verifiedClaimGrace(userMe, now)` — `now_ms` models `now.getTime()`.
pub fn verified_claim_grace(user_me: &JsVal, now_ms: f64) -> Option<ClaimGrace> {
    if matches!(user_me, JsVal::Null | JsVal::Bool(false)) {
        return None;
    }
    let player = player_of(user_me);
    if !matches!(val_field(&player, "usernameStatus"), Some(JsVal::Str(s)) if s == "claimed") {
        return None;
    }
    let name = val_field(&player, "usernameBase");
    let at = val_field(&player, "usernameClaimExpiresAt");
    if !truthy(name) || !truthy(at) {
        return None;
    }
    let JsVal::Str(name_s) = name.cloned().unwrap_or(JsVal::Undef) else { return None };
    if is_temporary_username(&units(&name_s)) {
        return None;
    }
    let at_v = at.cloned().unwrap_or(JsVal::Undef);
    let JsVal::Str(at_s) = at_v else {
        // A non-string `at` still built a Date in JS (Invalid → NaN).
        return Some(ClaimGrace {
            name: name_s.clone(),
            expires_at_ms: f64::NAN,
            at_risk: false,
        });
    };
    let expires = iso_to_epoch_ms(&at_s).unwrap_or(f64::NAN);
    Some(ClaimGrace {
        name: name_s.clone(),
        expires_at_ms: expires,
        at_risk: expires <= now_ms,
    })
}

/// `lapseNoticeMarker(grace)`.
pub fn lapse_notice_marker(grace: &ClaimGrace) -> String {
    let phase = if grace.at_risk { "atrisk" } else { "reserved" };
    format!("{}:{}", grace.name, phase)
}

/// `lapseNoticeDue(userMe, storedMarker, now)`.
pub fn lapse_notice_due(user_me: &JsVal, stored_marker: Option<&str>, now_ms: f64) -> bool {
    if account_verified_name(user_me).is_some() {
        return false;
    }
    let Some(grace) = verified_claim_grace(user_me, now_ms) else {
        return false;
    };
    stored_marker != Some(lapse_notice_marker(&grace).as_str())
}

/// `/^Anon([A-Za-z]+)\d?$/u` exec: returns the captured group when the
/// whole string matches (greedy letters, optional single trailing digit).
pub fn looks_generated(name: &str) -> bool {
    let b = name.as_bytes();
    let rest = match b.strip_prefix(b"Anon") {
        Some(r) => r,
        None => return false,
    };
    if rest.is_empty() {
        return false;
    }
    // `[A-Za-z]+` greedy, then `\d?`, then `$`. Backtracking: the letters
    // run must end either at the string end or right before ONE digit.
    let mut letters = 0usize;
    while letters < rest.len() && rest[letters].is_ascii_alphabetic() {
        letters += 1;
    }
    if letters == 0 {
        return false;
    }
    // Greedy `[A-Za-z]+` stops at the first non-letter; the tail must be
    // either empty or exactly one ASCII digit (then `$`).
    if letters < rest.len() && !(letters + 1 == rest.len() && rest[letters].is_ascii_digit()) {
        return false;
    }
    // The `u` flag iterates code points: astral chars are not in [A-Za-z]
    // and the letters run must cover whole code points.
    if std::str::from_utf8(&rest[..letters]).is_err() {
        return false;
    }
    let word = std::str::from_utf8(&rest[..letters]).ok();
    match word {
        Some(w) => crate::anon_names::ANON_WORDS.contains(&w),
        None => false,
    }
}

fn code_points(s: &str) -> Vec<u32> {
    let u = units(s);
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < u.len() {
        let c = u[i];
        if (0xD800..=0xDBFF).contains(&c)
            && i + 1 < u.len()
            && (0xDC00..=0xDFFF).contains(&u[i + 1])
        {
            let cp = 0x10000 + ((c as u32 - 0xD800) << 10) + (u[i + 1] as u32 - 0xDC00);
            out.push(cp);
            i += 2;
        } else {
            out.push(c as u32);
            i += 1;
        }
    }
    out
}

/// `truncateToCap(name)` (module-private in TS).
pub fn truncate_to_cap(name: &str) -> String {
    let u = units(name);
    if u.len() <= MAX_USERNAME_LENGTH {
        return name.to_string();
    }
    let hard = js_trim(&from_units(&u[..MAX_USERNAME_LENGTH])).to_string();
    let hu = units(&hard);
    if hu.len() < MAX_USERNAME_LENGTH {
        return hard;
    }
    if u.get(MAX_USERNAME_LENGTH) == Some(&0x20) {
        return hard;
    }
    let last_space = hu.iter().rposition(|&c| c == 0x20);
    match last_space {
        Some(ls) if ls >= MIN_USERNAME_LENGTH => from_units(&hu[..ls]),
        _ => hard,
    }
}

/// `sanitizePersona(persona)` — `None` models null.
pub fn sanitize_persona(persona: Option<&str>) -> Option<String> {
    let p = persona?;
    if p.is_empty() {
        return None;
    }
    let mapped: String = code_points(p)
        .into_iter()
        .map(|cp| if is_renderable_name_char(cp) { char::from_u32(cp).unwrap_or('\u{fffd}') } else { ' ' })
        .collect();
    // replace(/\s+/g, " ") then trim. Leading whitespace collapses to a
    // space that the trim removes; interior runs collapse to one space.
    let mut collapsed = String::new();
    let mut in_space = false;
    for c in mapped.chars() {
        if is_js_space(c) {
            in_space = true;
        } else {
            if in_space {
                collapsed.push(' ');
            }
            in_space = false;
            collapsed.push(c);
        }
    }
    let collapsed = collapsed;
    let name = truncate_to_cap(js_trim(&collapsed));
    if units(&name).len() < MIN_USERNAME_LENGTH {
        return None;
    }
    if !has_renderable_alnum(&units(&name)) {
        return None;
    }
    Some(name)
}

/// The four source labels in branch order.
const SOURCES: [&str; 4] = ["verified", "stored", "persona", "generated"];

/// `resolvePlayerName(inputs)` — the resolved triple.
pub fn resolve_player_name(
    verified_name: &JsVal,
    verified_opt_in: bool,
    stored_name: &JsVal,
    persona: &JsVal,
    generated_name: &str,
) -> (String, &'static str, bool) {
    if verified_opt_in && !matches!(verified_name, JsVal::Null) {
        if let JsVal::Str(s) = verified_name {
            return (s.clone(), SOURCES[0], true);
        }
    }
    let stored = match stored_name {
        JsVal::Str(s) => Some(js_trim(s).to_string()),
        _ => None,
    };
    if let Some(stored) = stored.filter(|s| !s.is_empty()) {
        return (clamp_username(&stored), SOURCES[1], false);
    }
    let sanitized = sanitize_persona(match persona {
        JsVal::Str(s) => Some(s.as_str()),
        _ => None,
    });
    if let Some(name) = sanitized {
        return (name, SOURCES[2], false);
    }
    (generated_name.to_string(), SOURCES[3], false)
}

// ---------------------------------------------------------------- vectors op
//
// kind 0: [n, (str)*n] -> [n, (str)*n]                       clampUsername
// kind 1: [n, (codec userMe)*n] -> [n, (codec str|null)*n]   accountVerifiedName
// kind 2: [n, (codec userMe)*n] -> [n, (0/1)*n]              accountNameHeld
// kind 3: [n, (codec stored|null, defaultAllowed)*n] -> [n,(0/1)*n] optIn
// kind 4: [n, (codec userMe, now_ms)*n] ->
//         [n, (codec null | {name, expiresAt, atRisk})*n]     verifiedClaimGrace
// kind 5: [n, (name, atRisk)*n] -> [n, (str)*n]              lapseNoticeMarker
// kind 6: [n, (str)*n] -> [n, (0/1)*n]                       looksGenerated
// kind 7: [codec inputs obj] -> [codec {name, source, verified}] resolvePlayerName
// kind 8: [n, (codec str|null|undefined)*n] -> [n, (codec str|null)*n] sanitizePersona
// kind 9: [n, (iso)*n] -> [n, (num|NaN)*n]                   iso_to_epoch_ms
// kind 10: [codec userMe, codec marker|null, now_ms] -> [0/1] lapseNoticeDue
// kind 11: [] -> [MIN, MAX, str LAPSE_NOTICE_KEY]            constants

/// Flat-token runner shared by the golden replay and the wasm probe.
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    match kind {
        0 => {
            let n = args[0] as usize;
            let mut i = 1usize;
            out.push(n as f64);
            for _ in 0..n {
                let s = read_str(args, &mut i);
                let c = clamp_username(&s);
                push_str(&mut out, &c);
            }
        }
        1 | 2 => {
            let n = args[0] as usize;
            let mut i = 1usize;
            out.push(n as f64);
            for _ in 0..n {
                let user_me = read_val(args, &mut i);
                if kind == 1 {
                    let res = match account_verified_name(&user_me) {
                        Some(s) => JsVal::Str(s),
                        None => JsVal::Null,
                    };
                    push_val(&mut out, &res);
                } else {
                    out.push(if account_name_held(&user_me) { 1.0 } else { 0.0 });
                }
            }
        }
        3 => {
            let n = args[0] as usize;
            let mut i = 1usize;
            out.push(n as f64);
            for _ in 0..n {
                let stored = read_val(args, &mut i);
                let default_allowed = args[i] != 0.0;
                i += 1;
                let s = match stored {
                    JsVal::Str(s) => Some(s),
                    JsVal::Null => None,
                    _ => unreachable!("player_name: stored must be str|null"),
                };
                out.push(if verified_name_opt_in(s.as_deref(), default_allowed) { 1.0 } else { 0.0 });
            }
        }
        4 => {
            let n = args[0] as usize;
            let mut i = 1usize;
            out.push(n as f64);
            for _ in 0..n {
                let user_me = read_val(args, &mut i);
                let now_ms = args[i];
                i += 1;
                let res = match verified_claim_grace(&user_me, now_ms) {
                    Some(g) => JsVal::Obj(vec![
                        ("name".to_string(), JsVal::Str(g.name)),
                        ("expiresAt".to_string(), JsVal::Num(g.expires_at_ms)),
                        ("atRisk".to_string(), JsVal::Bool(g.at_risk)),
                    ]),
                    None => JsVal::Null,
                };
                push_val(&mut out, &res);
            }
        }
        5 => {
            let n = args[0] as usize;
            let mut i = 1usize;
            out.push(n as f64);
            for _ in 0..n {
                let name = read_str(args, &mut i);
                let at_risk = args[i] != 0.0;
                i += 1;
                let g = ClaimGrace { name, expires_at_ms: 0.0, at_risk };
                push_str(&mut out, &lapse_notice_marker(&g));
            }
        }
        6 => {
            let n = args[0] as usize;
            let mut i = 1usize;
            out.push(n as f64);
            for _ in 0..n {
                let s = read_str(args, &mut i);
                out.push(if looks_generated(&s) { 1.0 } else { 0.0 });
            }
        }
        7 => {
            let mut i = 0usize;
            let inputs = read_val(args, &mut i);
            let g = val_field(&inputs, "generatedName").cloned().unwrap_or(JsVal::Undef);
            let JsVal::Str(generated) = g else {
                unreachable!("player_name: generatedName must be a string");
            };
            let opt_in = truthy(val_field(&inputs, "verifiedOptIn"));
            let (name, source, verified) = resolve_player_name(
                val_field(&inputs, "verifiedName").unwrap_or(&JsVal::Undef),
                opt_in,
                val_field(&inputs, "storedName").unwrap_or(&JsVal::Undef),
                val_field(&inputs, "persona").unwrap_or(&JsVal::Undef),
                &generated,
            );
            push_val(
                &mut out,
                &JsVal::Obj(vec![
                    ("name".to_string(), JsVal::Str(name)),
                    ("source".to_string(), JsVal::Str(source.to_string())),
                    ("verified".to_string(), JsVal::Bool(verified)),
                ]),
            );
        }
        8 => {
            let n = args[0] as usize;
            let mut i = 1usize;
            out.push(n as f64);
            for _ in 0..n {
                let v = read_val(args, &mut i);
                let p = match v {
                    JsVal::Str(s) => Some(s),
                    JsVal::Null | JsVal::Undef | JsVal::Absent => None,
                    _ => unreachable!("player_name: persona must be str|null|undefined"),
                };
                let res = match sanitize_persona(p.as_deref()) {
                    Some(s) => JsVal::Str(s),
                    None => JsVal::Null,
                };
                push_val(&mut out, &res);
            }
        }
        9 => {
            let n = args[0] as usize;
            let mut i = 1usize;
            out.push(n as f64);
            for _ in 0..n {
                let s = read_str(args, &mut i);
                out.push(iso_to_epoch_ms(&s).unwrap_or(f64::NAN));
            }
        }
        10 => {
            let mut i = 0usize;
            let user_me = read_val(args, &mut i);
            let marker = read_val(args, &mut i);
            let now_ms = args[i];
            let m = match marker {
                JsVal::Str(s) => Some(s),
                JsVal::Null => None,
                _ => unreachable!("player_name: marker must be str|null"),
            };
            out.push(if lapse_notice_due(&user_me, m.as_deref(), now_ms) { 1.0 } else { 0.0 });
        }
        11 => {
            out.push(MIN_USERNAME_LENGTH as f64);
            out.push(MAX_USERNAME_LENGTH as f64);
            push_str(&mut out, LAPSE_NOTICE_KEY);
        }
        k => unreachable!("player_name: unknown op kind {k}"),
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
    fn clamp_measures_units() {
        assert_eq!(clamp_username("short"), "short");
        assert_eq!(clamp_username(&"a".repeat(20)), "a".repeat(20));
        assert_eq!(clamp_username(&"a".repeat(25)), "a".repeat(20));
        // Astral chars: 11 emoji = 22 units -> cut to 20 units = 10 emoji.
        let emoji = "😀".repeat(11);
        assert_eq!(clamp_username(&emoji), "😀".repeat(10));
        // Trailing space inside the cut window is trimmed.
        assert_eq!(clamp_username(&format!("{} x", "a".repeat(19))), "a".repeat(19));
    }

    #[test]
    fn verified_name_gates() {
        let prem = claimed_user(vec![
            ("usernameStatus", JsVal::Str("premium".into())),
            ("username", JsVal::Str("zoë".into())),
            ("usernameBase", JsVal::Str("zoë".into())),
        ]);
        assert_eq!(account_verified_name(&prem).as_deref(), Some("zoë"));
        assert!(!account_name_held(&prem));
        let held = claimed_user(vec![
            ("usernameStatus", JsVal::Str("indefinite".into())),
            ("username", JsVal::Str("zoë.1234".into())),
            ("usernameBase", JsVal::Str("zoë".into())),
        ]);
        assert_eq!(account_verified_name(&held), None);
        assert!(account_name_held(&held));
        // claimed status: not verified, not "held".
        let claimed = claimed_user(vec![
            ("usernameStatus", JsVal::Str("claimed".into())),
            ("username", JsVal::Str("zoë.1234".into())),
            ("usernameBase", JsVal::Str("zoë".into())),
        ]);
        assert_eq!(account_verified_name(&claimed), None);
        assert!(!account_name_held(&claimed));
        // TEMPORARY base never qualifies.
        let temp = claimed_user(vec![
            ("usernameStatus", JsVal::Str("premium".into())),
            ("username", JsVal::Str("TEMPORARY1234".into())),
            ("usernameBase", JsVal::Str("TEMPORARY1234".into())),
        ]);
        assert_eq!(account_verified_name(&temp), None);
        assert!(!account_name_held(&temp));
        assert_eq!(account_verified_name(&JsVal::Null), None);
        assert_eq!(account_verified_name(&JsVal::Bool(false)), None);
        assert!(!account_name_held(&JsVal::Bool(false)));
    }

    #[test]
    fn opt_in_tristate() {
        assert!(verified_name_opt_in(Some("true"), false));
        assert!(!verified_name_opt_in(Some("false"), true));
        assert!(verified_name_opt_in(Some("TRUE"), true));
        assert!(!verified_name_opt_in(Some("TRUE"), false));
        assert!(verified_name_opt_in(None, true));
        assert!(!verified_name_opt_in(None, false));
    }

    #[test]
    fn iso_epoch_known_values() {
        assert_eq!(iso_to_epoch_ms("1970-01-01T00:00:00.000Z"), Some(0.0));
        assert_eq!(iso_to_epoch_ms("1970-01-01T00:00:00Z"), Some(0.0));
        assert_eq!(iso_to_epoch_ms("2026-01-01T00:00:00.000Z"), Some(1767225600000.0));
        assert_eq!(iso_to_epoch_ms("1970-01-01T00:00:01Z"), Some(1000.0));
        assert_eq!(iso_to_epoch_ms("2000-02-29T12:30:45.500Z"), Some(951827445500.0));
        // Offset form: 12:00+02:00 is 10:00 UTC.
        assert_eq!(iso_to_epoch_ms("1970-01-01T12:00:00+02:00"), Some(36_000_000.0));
        // Date-only is UTC midnight.
        assert_eq!(iso_to_epoch_ms("1970-01-02"), Some(86_400_000.0));
        assert_eq!(iso_to_epoch_ms("garbage"), None);
        assert_eq!(iso_to_epoch_ms(""), None);
        assert_eq!(iso_to_epoch_ms("1970-13-01T00:00:00Z"), None);
        assert_eq!(iso_to_epoch_ms("1970-01-01T25:00:00Z"), None);
        // V8 accepts the colon-less offset and rejects a trailing Z tail.
        assert_eq!(iso_to_epoch_ms("2026-01-01T00:00:00+0530"), Some(1767205800000.0));
        assert_eq!(iso_to_epoch_ms("2026-01-01T00:00:00Zjunk"), None);
        assert_eq!(iso_to_epoch_ms("2026-01-01T00:00:00+5:30"), None);
    }

    fn claimed_user(fields: Vec<(&str, JsVal)>) -> JsVal {
        obj(vec![("player", obj(fields))])
    }

    #[test]
    fn grace_and_marker() {
        let claimed = claimed_user(vec![
            ("usernameStatus", JsVal::Str("claimed".into())),
            ("usernameBase", JsVal::Str("zoë".into())),
            ("usernameClaimExpiresAt", JsVal::Str("2026-01-01T00:00:00.000Z".into())),
        ]);
        let now = 1_767_225_600_000.0; // exactly the expiry instant
        let g = verified_claim_grace(&claimed, now).unwrap();
        assert_eq!(g.name, "zoë");
        assert!(g.at_risk); // <= is inclusive
        assert_eq!(lapse_notice_marker(&g), "zoë:atrisk");
        let g2 = verified_claim_grace(&claimed, now - 1.0).unwrap();
        assert!(!g2.at_risk);
        assert_eq!(lapse_notice_marker(&g2), "zoë:reserved");
        // Empty-string at is falsy.
        let empty_at = claimed_user(vec![
            ("usernameStatus", JsVal::Str("claimed".into())),
            ("usernameBase", JsVal::Str("zoë".into())),
            ("usernameClaimExpiresAt", JsVal::Str(String::new())),
        ]);
        assert!(verified_claim_grace(&empty_at, now).is_none());
        // Unparseable ISO → NaN expiry, atRisk false.
        let bad_at = claimed_user(vec![
            ("usernameStatus", JsVal::Str("claimed".into())),
            ("usernameBase", JsVal::Str("zoë".into())),
            ("usernameClaimExpiresAt", JsVal::Str("nope".into())),
        ]);
        let g3 = verified_claim_grace(&bad_at, now).unwrap();
        assert!(g3.expires_at_ms.is_nan());
        assert!(!g3.at_risk);
        // TEMPORARY base never gets a grace.
        let temp = claimed_user(vec![
            ("usernameStatus", JsVal::Str("claimed".into())),
            ("usernameBase", JsVal::Str("TEMPORARY1234".into())),
            ("usernameClaimExpiresAt", JsVal::Str("2026-01-01T00:00:00.000Z".into())),
        ]);
        assert!(verified_claim_grace(&temp, now).is_none());
    }

    #[test]
    fn lapse_due() {
        let claimed = claimed_user(vec![
            ("usernameStatus", JsVal::Str("claimed".into())),
            ("usernameBase", JsVal::Str("zoë".into())),
            ("usernameClaimExpiresAt", JsVal::Str("2026-01-01T00:00:00.000Z".into())),
        ]);
        let now = 1_767_225_600_000.0;
        assert!(lapse_notice_due(&claimed, None, now));
        assert!(lapse_notice_due(&claimed, Some("zoë:reserved"), now));
        assert!(!lapse_notice_due(&claimed, Some("zoë:atrisk"), now));
        // Eligible again (verified name) → not due.
        let prem = claimed_user(vec![
            ("usernameStatus", JsVal::Str("premium".into())),
            ("username", JsVal::Str("zoë".into())),
            ("usernameBase", JsVal::Str("zoë".into())),
        ]);
        assert!(!lapse_notice_due(&prem, None, now));
    }

    #[test]
    fn generated_shape() {
        let first = crate::anon_names::ANON_WORDS[0];
        assert!(looks_generated(&format!("Anon{first}")));
        assert!(looks_generated(&format!("Anon{first}7")));
        assert!(!looks_generated(&format!("Anon{first}77")));
        assert!(!looks_generated("Anon123"));
        assert!(!looks_generated("Anon"));
        assert!(!looks_generated("anonCat"));
        assert!(!looks_generated("AnonNotAWord"));
        assert!(!looks_generated("AnonCat!"));
    }

    #[test]
    fn persona_sanitize() {
        assert_eq!(sanitize_persona(None), None);
        assert_eq!(sanitize_persona(Some("")), None);
        assert_eq!(sanitize_persona(Some("Ada🔥Lovelace")).as_deref(), Some("Ada Lovelace"));
        assert_eq!(sanitize_persona(Some("  spaces   here  ")).as_deref(), Some("spaces here"));
        // Punctuation alone fails the alnum gate.
        assert_eq!(sanitize_persona(Some("★★★★")), None);
        assert_eq!(sanitize_persona(Some("...")), None);
        // Under three units after collapse.
        assert_eq!(sanitize_persona(Some("ab")), None);
        assert_eq!(sanitize_persona(Some("Zoë")).as_deref(), Some("Zoë"));
        // U+0085 is NOT \s: it maps to a space via the renderable gate
        // (it is not in the class), so it separates words.
        assert_eq!(sanitize_persona(Some("a\u{85}b")).as_deref(), Some("a b"));
        // U+FEFF is \s but also not renderable → space either way.
        assert_eq!(sanitize_persona(Some("a\u{feff}b")).as_deref(), Some("a b"));
        // Truncate to cap prefers a word boundary.
        let long = "Ada Lovelace the Countess";
        assert_eq!(sanitize_persona(Some(long)).as_deref(), Some("Ada Lovelace the"));
        // A single long word is cut where it falls.
        assert_eq!(sanitize_persona(Some(&"x".repeat(25))), Some("x".repeat(20)));
    }

    #[test]
    fn resolve_branches() {
        let r = resolve_player_name(&JsVal::Str("zoë".into()), true, &JsVal::Null, &JsVal::Null, "AnonCat");
        assert_eq!(r, ("zoë".to_string(), "verified", true));
        // verifiedName undefined does NOT take branch 1 (strict !== null).
        let r = resolve_player_name(&JsVal::Undef, true, &JsVal::Str("bob".into()), &JsVal::Null, "AnonCat");
        assert_eq!(r, ("bob".to_string(), "stored", false));
        // Whitespace-only stored falls through.
        let r = resolve_player_name(&JsVal::Null, false, &JsVal::Str("  ".into()), &JsVal::Str("Steam Name".into()), "AnonCat");
        assert_eq!(r, ("Steam Name".to_string(), "persona", false));
        let r = resolve_player_name(&JsVal::Null, false, &JsVal::Null, &JsVal::Null, "AnonCat");
        assert_eq!(r, ("AnonCat".to_string(), "generated", false));
        // Stored is clamped to the cap.
        let r = resolve_player_name(&JsVal::Null, false, &JsVal::Str("x".repeat(25)), &JsVal::Null, "AnonCat");
        assert_eq!(r, ("x".repeat(20), "stored", false));
    }

    #[test]
    fn truncate_to_cap_rules() {
        assert_eq!(truncate_to_cap("short"), "short");
        // Exactly 20 with a space at index 20: clean cut.
        let s = format!("{} x", "a".repeat(19));
        assert_eq!(truncate_to_cap(&s), "a".repeat(19));
        // Boundary worth cutting back to.
        assert_eq!(truncate_to_cap("Ada Lovelace the Countess"), "Ada Lovelace the");
        // No boundary worth it (last space before index 3).
        assert_eq!(truncate_to_cap(&format!("ab {}", "c".repeat(20))), "ab ".to_string() + &"c".repeat(17));
    }
}
