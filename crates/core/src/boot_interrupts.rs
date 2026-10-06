//! Port of `src/client/BootInterrupts.ts` — the boot-interrupt ordering rule
//! and the claim-prompt decay store.
//!
//! Faithfulness notes (quirk list):
//!
//! * `isCleanHomepage`: hash must be `""` (STRICT), then `pathname === "/"`
//!   wins outright; only otherwise does the desktop-shell `/index.html`
//!   branch apply (`desktopShell && pathname === "/index.html"` — `&&`
//!   returns the boolean here since both sides are booleans).
//! * `bootInterruptsAllowed` gates `lobby.lobbyHandle === null` STRICTLY —
//!   `undefined` does NOT pass. The runner crosses a `handleNull` bit.
//! * `joinOwnsInFlightFlag` is `===` on numbers: `NaN === NaN` is false.
//! * `nextBootInterrupt` gate order is the contract: cleanHomepage, then
//!   entitled+TEMPORARY base, then entitled+`!username` (TRUTHY — `""`
//!   counts as no name) + due + strings-ready, then lapse, then
//!   `rewardCount > 0` (NaN > 0 is false).
//! * `parseClaimPromptStore`: `raw === null` STRICT (undefined would throw
//!   at JSON.parse — outside the typed domain); a parse throw → `{}`;
//!   `typeof !== "object" || null || Array.isArray` → `{}`; per entry:
//!   empty publicId skipped, non-object (or null) value skipped — an ARRAY
//!   value passes the typeof gate but its destructured fields read
//!   `undefined` and fail the number gate; `shows` / `lastShownAt` must be
//!   `typeof number && Number.isFinite` (NaN and ±Infinity dropped).
//! * JS object key order: canonical decimal integer keys (0 .. 2^53-1) sort
//!   NUMERICALLY FIRST, string keys follow in insertion order. The store is
//!   built through [`map_set_v8`] so spread / computed-key / re-insertion
//!   reproduce it.
//! * `claimPromptShown`: the spread keeps the store's order; the recorded
//!   account's `shows` is `(store[publicId]?.shows ?? 0) + 1` (absent /
//!   null / undefined → 1; non-numeric shows is outside the typed domain
//!   and models as NaN). `ids.length <= 8` returns the spread UNPRUNED.
//!   Otherwise the others are sorted by the subtraction comparator
//!   `next[b].lastShownAt - next[a].lastShownAt` — NaN comparator results
//!   are treated as 0 by SortCompare (equal, stable), `-0` likewise — then
//!   `slice(0, 7)`. Rust's `sort_by` is a stable TimSort like V8's
//!   `Array#sort`, so equal-comparator sessions agree; the golden capture
//!   pins scripted sessions (ties, future timestamps, an absent-field NaN)
//!   to prove it. The pruned map is rebuilt `{[publicId]: …}` first, then
//!   the keep list in order, through [`map_set_v8`].
//! * `claimPromptStringsReady` is modelled over the three translate RESULTS
//!   (the capture records them): strict `!==` against each key, short
//!   circuit in key order body → heading → confirm.

use crate::api_schemas::is_temporary_username;
use crate::js_json::{push_map, push_str, push_val, read_map, read_str, read_val, val_field, JsVal};

/// `CLAIM_PROMPT_KEY`.
pub const CLAIM_PROMPT_KEY: &str = "usernameClaimPrompt";
/// `CLAIM_PROMPT_MAX_SHOWS`.
pub const CLAIM_PROMPT_MAX_SHOWS: f64 = 3.0;
/// `CLAIM_PROMPT_INTERVAL_MS` = 24 * 60 * 60 * 1000.
pub const CLAIM_PROMPT_INTERVAL_MS: f64 = 24.0 * 60.0 * 60.0 * 1000.0;
/// `CLAIM_PROMPT_MAX_ACCOUNTS`.
pub const CLAIM_PROMPT_MAX_ACCOUNTS: usize = 8;
/// `USERNAME_FORM_HASH`.
pub const USERNAME_FORM_HASH: &str = "modal=change-username";

/// The six `BOOT_INTERRUPT_KEYS` in insertion order.
pub const BOOT_INTERRUPT_KEYS: [(&str, &str); 6] = [
    ("temporaryBody", "account_modal.username_temporary_prompt"),
    ("temporaryHeading", "account_modal.username_title"),
    ("temporaryConfirm", "account_modal.username_temporary_prompt_confirm"),
    ("claimBody", "account_modal.username_claim_prompt"),
    ("claimHeading", "account_modal.username_claim_heading"),
    ("claimConfirm", "account_modal.username_claim_prompt_confirm"),
];

/// `isCleanHomepage(location, desktopShell)`.
pub fn is_clean_homepage(hash: &str, pathname: &str, desktop_shell: bool) -> bool {
    if !hash.is_empty() {
        return false;
    }
    if pathname == "/" {
        return true;
    }
    desktop_shell && pathname == "/index.html"
}

/// `bootInterruptsAllowed(location, desktopShell, lobby)`.
pub fn boot_interrupts_allowed(
    hash: &str,
    pathname: &str,
    desktop_shell: bool,
    join_in_flight: bool,
    handle_null: bool,
) -> bool {
    is_clean_homepage(hash, pathname, desktop_shell) && !join_in_flight && handle_null
}

/// `joinOwnsInFlightFlag(mostRecentJoinEvent, joinEvent)` — `===` on numbers.
pub fn join_owns_in_flight_flag(most_recent: f64, join: f64) -> bool {
    most_recent == join
}

fn entitled(status: &JsVal) -> bool {
    matches!(status, JsVal::Str(s) if s == "premium" || s == "indefinite")
}

fn truthy(v: &JsVal) -> bool {
    match v {
        JsVal::Absent | JsVal::Undef | JsVal::Null => false,
        JsVal::Num(n) => !n.is_nan() && *n != 0.0,
        JsVal::Bool(b) => *b,
        JsVal::Str(s) => !s.is_empty(),
        JsVal::Obj(_) | JsVal::Arr(_) => true,
    }
}

/// The four interrupt names in ranking order (index 0 = "username-temporary").
const INTERRUPTS: [&str; 4] = [
    "username-temporary",
    "username-claim",
    "lapse-notice",
    "rewards",
];

/// `nextBootInterrupt(inputs)` — returns the interrupt name or None (null).
/// The eight flat inputs mirror the TS `BootInterruptInputs` object fields;
/// they cross the vector runner flat, so the arity is above the clippy
/// default (the JS object is the single argument).
#[allow(clippy::too_many_arguments)]
pub fn next_boot_interrupt(
    clean_homepage: bool,
    status: &JsVal,
    username: &JsVal,
    username_base: &JsVal,
    lapse_notice_due: bool,
    reward_count: f64,
    claim_prompt_due: bool,
    claim_strings_ready: bool,
) -> Option<&'static str> {
    if !clean_homepage {
        return None;
    }
    let base_temp = match username_base {
        JsVal::Str(s) => is_temporary_username(&s.encode_utf16().collect::<Vec<u16>>()),
        _ => false,
    };
    if entitled(status) && base_temp {
        return Some(INTERRUPTS[0]);
    }
    if entitled(status) && !truthy(username) && claim_prompt_due && claim_strings_ready {
        return Some(INTERRUPTS[1]);
    }
    if lapse_notice_due {
        return Some(INTERRUPTS[2]);
    }
    if reward_count > 0.0 {
        return Some(INTERRUPTS[3]);
    }
    None
}

// ------------------------------------------------------------- store helpers

/// Canonical decimal-integer object key (V8 integer-key section): digits
/// only, no leading zeros (except "0" itself), value < 2^53.
fn canon_int_key(k: &str) -> Option<u64> {
    let b = k.as_bytes();
    if b.is_empty() || !b.iter().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if b[0] == b'0' && b.len() > 1 {
        return None;
    }
    if b.len() > 16 {
        return None;
    }
    k.parse::<u64>().ok().filter(|v| *v < (1u64 << 53))
}

/// `map[key] = value` reproducing V8 own-key order: integer keys sort
/// numerically ahead of string keys (which keep insertion order); an
/// existing key is overwritten in place.
fn map_set_v8(m: &mut Vec<(String, JsVal)>, key: &str, v: JsVal) {
    if let Some(slot) = m.iter_mut().find(|(k, _)| k == key) {
        slot.1 = v;
        return;
    }
    match canon_int_key(key) {
        Some(val) => {
            let pos = m
                .iter()
                .position(|(k, _)| match canon_int_key(k) {
                    Some(kv) => kv > val,
                    None => true,
                })
                .unwrap_or(m.len());
            m.insert(pos, (key.to_string(), v));
        }
        None => m.push((key.to_string(), v)),
    }
}

/// The strict-JSON subset parser: `None` models the `JSON.parse` throw.
/// Objects come back with V8 own-key order (integers first).
fn json_parse(s: &str) -> Option<JsVal> {
    let b = s.as_bytes();
    let mut i = 0usize;
    fn ws(b: &[u8], i: &mut usize) {
        while *i < b.len() && matches!(b[*i], b' ' | b'\t' | b'\n' | b'\r') {
            *i += 1;
        }
    }
    fn lit(b: &[u8], i: &mut usize, word: &str) -> bool {
        if b.len() >= *i + word.len() && &b[*i..*i + word.len()] == word.as_bytes() {
            *i += word.len();
            true
        } else {
            false
        }
    }
    fn value(b: &[u8], i: &mut usize) -> Option<JsVal> {
        ws(b, i);
        if *i >= b.len() {
            return None;
        }
        match b[*i] {
            b'n' => lit(b, i, "null").then_some(JsVal::Null),
            b't' => lit(b, i, "true").then_some(JsVal::Bool(true)),
            b'f' => lit(b, i, "false").then_some(JsVal::Bool(false)),
            b'"' => string(b, i).map(JsVal::Str),
            b'[' => {
                *i += 1;
                let mut items = Vec::new();
                ws(b, i);
                if *i < b.len() && b[*i] == b']' {
                    *i += 1;
                    return Some(JsVal::Arr(items));
                }
                loop {
                    items.push(value(b, i)?);
                    ws(b, i);
                    match b.get(*i)? {
                        b',' => *i += 1,
                        b']' => {
                            *i += 1;
                            return Some(JsVal::Arr(items));
                        }
                        _ => return None,
                    }
                }
            }
            b'{' => {
                *i += 1;
                let mut fields: Vec<(String, JsVal)> = Vec::new();
                ws(b, i);
                if *i < b.len() && b[*i] == b'}' {
                    *i += 1;
                    return Some(JsVal::Obj(fields));
                }
                loop {
                    ws(b, i);
                    let key = string(b, i)?;
                    ws(b, i);
                    if b.get(*i)? != &b':' {
                        return None;
                    }
                    *i += 1;
                    let v = value(b, i)?;
                    map_set_v8(&mut fields, &key, v);
                    ws(b, i);
                    match b.get(*i)? {
                        b',' => *i += 1,
                        b'}' => {
                            *i += 1;
                            return Some(JsVal::Obj(fields));
                        }
                        _ => return None,
                    }
                }
            }
            _ => number(b, i),
        }
    }
    fn string(b: &[u8], i: &mut usize) -> Option<String> {
        if b.get(*i)? != &b'"' {
            return None;
        }
        *i += 1;
        let mut out = String::new();
        loop {
            let c = *b.get(*i)?;
            *i += 1;
            match c {
                b'"' => return Some(out),
                b'\\' => {
                    let e = *b.get(*i)?;
                    *i += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            if *i + 4 > b.len() {
                                return None;
                            }
                            let hex = std::str::from_utf8(&b[*i..*i + 4]).ok()?;
                            let cp = u32::from_str_radix(hex, 16).ok()?;
                            *i += 4;
                            out.push(char::from_u32(cp).unwrap_or('\u{fffd}'));
                        }
                        _ => return None,
                    }
                }
                _ => out.push(c as char),
            }
        }
    }
    fn number(b: &[u8], i: &mut usize) -> Option<JsVal> {
        let start = *i;
        if b.get(*i)? == &b'-' {
            *i += 1;
        }
        while *i < b.len() && matches!(b[*i], b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-') {
            *i += 1;
        }
        if *i == start {
            return None;
        }
        let s = std::str::from_utf8(&b[start..*i]).ok()?;
        // JSON numbers are IEEE-754 doubles; Rust's parse is correctly
        // rounded like V8's fast path.
        s.parse::<f64>().ok().map(JsVal::Num)
    }
    let v = value(b, &mut i)?;
    ws(b, &mut i);
    if i != b.len() {
        return None; // trailing garbage throws
    }
    Some(v)
}

fn num_field(v: &JsVal, key: &str) -> Option<f64> {
    match val_field(v, key) {
        Some(JsVal::Num(n)) => Some(*n),
        _ => None,
    }
}

/// `parseClaimPromptStore(raw)` — `None` models the JS `null`.
pub fn parse_claim_prompt_store(raw: Option<&str>) -> Vec<(String, JsVal)> {
    let Some(raw) = raw else { return Vec::new() };
    let Some(parsed) = json_parse(raw) else {
        return Vec::new();
    };
    let JsVal::Obj(fields) = parsed else {
        return Vec::new(); // typeof gate, null gate, array gate all land here
    };
    let mut store: Vec<(String, JsVal)> = Vec::new();
    for (public_id, value) in fields {
        if public_id.is_empty() {
            continue;
        }
        // typeof value !== "object" || value === null → skip. An Arr passes
        // the typeof gate but its destructured fields fail the number gate.
        let shows = num_field(&value, "shows");
        let last = num_field(&value, "lastShownAt");
        let (Some(shows), Some(last)) = (shows, last) else {
            continue;
        };
        if !shows.is_finite() || !last.is_finite() {
            continue;
        }
        map_set_v8(
            &mut store,
            &public_id,
            JsVal::Obj(vec![
                ("shows".to_string(), JsVal::Num(shows)),
                ("lastShownAt".to_string(), JsVal::Num(last)),
            ]),
        );
    }
    store
}

/// `claimPromptDue(store, now, publicId)`.
pub fn claim_prompt_due(store: &[(String, JsVal)], now: f64, public_id: &str) -> bool {
    let record = store.iter().find(|(k, _)| k == public_id).map(|(_, v)| v);
    let Some(record) = record else { return true };
    let shows = num_field(record, "shows").unwrap_or(f64::NAN);
    if shows >= CLAIM_PROMPT_MAX_SHOWS {
        return false;
    }
    let last = num_field(record, "lastShownAt").unwrap_or(f64::NAN);
    let elapsed = now - last;
    if elapsed < 0.0 {
        return false;
    }
    elapsed >= CLAIM_PROMPT_INTERVAL_MS
}

/// `claimPromptShown(store, now, publicId)` — the store to write after a
/// showing.
pub fn claim_prompt_shown(
    store: &[(String, JsVal)],
    now: f64,
    public_id: &str,
) -> Vec<(String, JsVal)> {
    let prev_shows = store
        .iter()
        .find(|(k, _)| k == public_id)
        .map(|(_, v)| match val_field(v, "shows") {
            Some(JsVal::Num(n)) => *n,
            Some(JsVal::Absent) | Some(JsVal::Undef) | Some(JsVal::Null) | None => 0.0,
            Some(JsVal::Bool(b)) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            // Non-numeric shows is outside the typed domain (JS would
            // string-concat); NaN keeps the arithmetic honest if a capture
            // ever probes it.
            Some(_) => f64::NAN,
        })
        .unwrap_or(0.0);
    let rec = JsVal::Obj(vec![
        ("shows".to_string(), JsVal::Num(prev_shows + 1.0)),
        ("lastShownAt".to_string(), JsVal::Num(now)),
    ]);
    let mut next: Vec<(String, JsVal)> = store.to_vec();
    map_set_v8(&mut next, public_id, rec);
    if next.len() <= CLAIM_PROMPT_MAX_ACCOUNTS {
        return next;
    }
    let last_of = |id: &str| -> f64 {
        next.iter()
            .find(|(k, _)| k == id)
            .and_then(|(_, v)| num_field(v, "lastShownAt"))
            .unwrap_or(f64::NAN)
    };
    let mut others: Vec<String> = next
        .iter()
        .filter(|(k, _)| k != public_id)
        .map(|(k, _)| k.clone())
        .collect();
    // Comparator(a, b) = next[b].lastShownAt - next[a].lastShownAt; SortCompare
    // treats a NaN result as +0 (equal), and -0 likewise. Rust's stable
    // sort_by with the same comparator results reproduces V8's output.
    others.sort_by(|a, b| {
        let d = last_of(b) - last_of(a);
        if d.is_nan() || d == 0.0 {
            std::cmp::Ordering::Equal
        } else if d < 0.0 {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        }
    });
    let keep = &others[..CLAIM_PROMPT_MAX_ACCOUNTS - 1];
    let mut pruned: Vec<(String, JsVal)> = Vec::new();
    let cur = next
        .iter()
        .find(|(k, _)| k == public_id)
        .map(|(_, v)| v.clone())
        .unwrap_or(JsVal::Undef);
    map_set_v8(&mut pruned, public_id, cur);
    for id in keep {
        let v = next
            .iter()
            .find(|(k, _)| k == id)
            .map(|(_, v)| v.clone())
            .unwrap_or(JsVal::Undef);
        map_set_v8(&mut pruned, id, v);
    }
    pruned
}

/// `claimPromptStringsReady(translate)` over the three recorded results.
pub fn claim_prompt_strings_ready(t_body: &str, t_heading: &str, t_confirm: &str) -> bool {
    t_body != "account_modal.username_claim_prompt"
        && t_heading != "account_modal.username_claim_heading"
        && t_confirm != "account_modal.username_claim_prompt_confirm"
}

// ---------------------------------------------------------------- vectors op
//
// Strings cross as `[len, u0, ..]`; stores as codec maps `[n,(k,v)*n]`.
//
// kind 0: [n, (hash, pathname, shell)*n] -> [n,(0/1)*n]  isCleanHomepage
// kind 1: [n, (hash, pathname, shell, inFlight, handleNull)*n] -> [n,(0/1)*n]
//         bootInterruptsAllowed
// kind 2: [n, (a, b)*n] -> [n,(0/1)*n]                   joinOwnsInFlightFlag
// kind 3: [n, (clean, ...codec status, ...codec username, ...codec base,
//              lapseDue, rewardCount, claimDue, claimReady)*n]
//         -> [n, (codec str|null)*n]                     nextBootInterrupt
// kind 4: [n, (codec str|null)*n] -> codec maps          parseClaimPromptStore
// kind 5: [n, (map, now, pubId)*n] -> [n,(0/1)*n]        claimPromptDue
// kind 6: [n, (map, now, pubId)*n] -> codec maps         claimPromptShown
// kind 7: [n, (tBody, tHeading, tConfirm)*n] -> [n,(0/1)*n] stringsReady
// kind 8: [] -> [MAX_SHOWS, INTERVAL, MAX_ACCOUNTS, str CLAIM_PROMPT_KEY,
//              str USERNAME_FORM_HASH, 6, (key,value)*6] constants dump

/// Flat-token runner shared by the golden replay and the wasm probe.
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    match kind {
        0 => {
            let n = args[0] as usize;
            let mut i = 1usize;
            out.push(n as f64);
            for _ in 0..n {
                let hash = read_str(args, &mut i);
                let path = read_str(args, &mut i);
                let shell = args[i] != 0.0;
                i += 1;
                out.push(if is_clean_homepage(&hash, &path, shell) { 1.0 } else { 0.0 });
            }
        }
        1 => {
            let n = args[0] as usize;
            let mut i = 1usize;
            out.push(n as f64);
            for _ in 0..n {
                let hash = read_str(args, &mut i);
                let path = read_str(args, &mut i);
                let shell = args[i] != 0.0;
                let inflight = args[i + 1] != 0.0;
                let handle_null = args[i + 2] != 0.0;
                i += 3;
                out.push(if boot_interrupts_allowed(&hash, &path, shell, inflight, handle_null) {
                    1.0
                } else {
                    0.0
                });
            }
        }
        2 => {
            let n = args[0] as usize;
            out.push(n as f64);
            for k in 0..n {
                let a = args[1 + k * 2];
                let b = args[2 + k * 2];
                out.push(if join_owns_in_flight_flag(a, b) { 1.0 } else { 0.0 });
            }
        }
        3 => {
            let n = args[0] as usize;
            let mut i = 1usize;
            out.push(n as f64);
            for _ in 0..n {
                let clean = args[i] != 0.0;
                i += 1;
                let status = read_val(args, &mut i);
                let username = read_val(args, &mut i);
                let base = read_val(args, &mut i);
                let lapse = args[i] != 0.0;
                let rewards = args[i + 1];
                let due = args[i + 2] != 0.0;
                let ready = args[i + 3] != 0.0;
                i += 4;
                let res = next_boot_interrupt(
                    clean, &status, &username, &base, lapse, rewards, due, ready,
                );
                push_val(&mut out, &res.map(|s| JsVal::Str(s.to_string())).unwrap_or(JsVal::Null));
            }
        }
        4 => {
            let n = args[0] as usize;
            let mut i = 1usize;
            out.push(n as f64);
            for _ in 0..n {
                let raw = read_val(args, &mut i);
                let s = match raw {
                    JsVal::Str(s) => Some(s),
                    JsVal::Null => None,
                    _ => unreachable!("boot_interrupts: parse arg must be str|null"),
                };
                push_map(&mut out, &parse_claim_prompt_store(s.as_deref()));
            }
        }
        5 => {
            let n = args[0] as usize;
            let mut i = 1usize;
            out.push(n as f64);
            for _ in 0..n {
                let store = read_map(args, &mut i);
                let now = args[i];
                i += 1;
                let pid = read_str(args, &mut i);
                out.push(if claim_prompt_due(&store, now, &pid) { 1.0 } else { 0.0 });
            }
        }
        6 => {
            let n = args[0] as usize;
            let mut i = 1usize;
            out.push(n as f64);
            for _ in 0..n {
                let store = read_map(args, &mut i);
                let now = args[i];
                i += 1;
                let pid = read_str(args, &mut i);
                push_map(&mut out, &claim_prompt_shown(&store, now, &pid));
            }
        }
        7 => {
            let n = args[0] as usize;
            let mut i = 1usize;
            out.push(n as f64);
            for _ in 0..n {
                let a = read_str(args, &mut i);
                let b = read_str(args, &mut i);
                let c = read_str(args, &mut i);
                out.push(if claim_prompt_strings_ready(&a, &b, &c) { 1.0 } else { 0.0 });
            }
        }
        8 => {
            out.push(CLAIM_PROMPT_MAX_SHOWS);
            out.push(CLAIM_PROMPT_INTERVAL_MS);
            out.push(CLAIM_PROMPT_MAX_ACCOUNTS as f64);
            push_str(&mut out, CLAIM_PROMPT_KEY);
            push_str(&mut out, USERNAME_FORM_HASH);
            out.push(BOOT_INTERRUPT_KEYS.len() as f64);
            for (k, v) in BOOT_INTERRUPT_KEYS {
                push_str(&mut out, k);
                push_str(&mut out, v);
            }
        }
        k => unreachable!("boot_interrupts: unknown op kind {k}"),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(shows: f64, last: f64) -> JsVal {
        JsVal::Obj(vec![
            ("shows".to_string(), JsVal::Num(shows)),
            ("lastShownAt".to_string(), JsVal::Num(last)),
        ])
    }

    fn store(pairs: &[(&str, f64, f64)]) -> Vec<(String, JsVal)> {
        pairs.iter().map(|(k, s, l)| (k.to_string(), rec(*s, *l))).collect()
    }

    #[test]
    fn homepage_gates() {
        assert!(!is_clean_homepage("#x", "/", false));
        assert!(is_clean_homepage("", "/", false));
        assert!(!is_clean_homepage("", "/index.html", false));
        assert!(is_clean_homepage("", "/index.html", true));
        assert!(boot_interrupts_allowed("", "/", false, false, true));
        assert!(!boot_interrupts_allowed("", "/", false, true, true));
        assert!(!boot_interrupts_allowed("", "/", false, false, false));
        assert!(!join_owns_in_flight_flag(f64::NAN, f64::NAN));
        assert!(join_owns_in_flight_flag(7.0, 7.0));
    }

    #[test]
    fn ranking_order() {
        let prem = JsVal::Str("premium".into());
        let temp = JsVal::Str("TEMPORARY1234".into());
        assert_eq!(
            next_boot_interrupt(true, &prem, &JsVal::Null, &temp, false, 0.0, false, false),
            Some("username-temporary")
        );
        // Empty display name is falsy → claim prompt wins when due+ready.
        assert_eq!(
            next_boot_interrupt(
                true,
                &prem,
                &JsVal::Str(String::new()),
                &JsVal::Str("abc".into()),
                true,
                5.0,
                true,
                true
            ),
            Some("username-claim")
        );
        // Strings not ready → lapse notice takes the boot instead.
        assert_eq!(
            next_boot_interrupt(
                true,
                &prem,
                &JsVal::Null,
                &JsVal::Str("abc".into()),
                true,
                5.0,
                true,
                false
            ),
            Some("lapse-notice")
        );
        assert_eq!(
            next_boot_interrupt(true, &JsVal::Undef, &JsVal::Null, &JsVal::Null, false, 1.0, false, false),
            Some("rewards")
        );
        assert_eq!(
            next_boot_interrupt(true, &JsVal::Undef, &JsVal::Null, &JsVal::Null, false, f64::NAN, false, false),
            None
        );
        assert_eq!(
            next_boot_interrupt(false, &prem, &JsVal::Null, &temp, true, 3.0, true, true),
            None
        );
    }

    #[test]
    fn parse_store_gates() {
        assert!(parse_claim_prompt_store(None).is_empty());
        assert!(parse_claim_prompt_store(Some("not json")).is_empty());
        assert!(parse_claim_prompt_store(Some("[1,2]")).is_empty());
        assert!(parse_claim_prompt_store(Some("null")).is_empty());
        assert!(parse_claim_prompt_store(Some("\"x\"")).is_empty());
        let s = parse_claim_prompt_store(Some(
            r#"{"": {"shows":1,"lastShownAt":2}, "a": {"shows":1,"lastShownAt":2},
               "b": null, "c": [1], "d": {"shows":"x","lastShownAt":1},
               "e": {"shows":null,"lastShownAt":1}, "f": {"shows":1}}"#,
        ));
        let keys: Vec<&str> = s.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["a"]);
        // Trailing garbage throws.
        assert!(parse_claim_prompt_store(Some("{}x")).is_empty());
        // Integer keys sort first, numerically; "01" (leading zero) is a
        // string key and keeps insertion order after "z".
        let s = parse_claim_prompt_store(Some(
            r#"{"z":{"shows":0,"lastShownAt":0},"10":{"shows":0,"lastShownAt":0},"2":{"shows":0,"lastShownAt":0},"01":{"shows":0,"lastShownAt":0}}"#,
        ));
        let keys: Vec<&str> = s.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["2", "10", "z", "01"]);
    }

    #[test]
    fn due_rule() {
        let st = store(&[("a", 0.0, 0.0)]);
        assert!(claim_prompt_due(&st, 0.0, "missing"));
        assert!(!claim_prompt_due(&st, 0.0, "a")); // elapsed 0 < interval
        assert!(claim_prompt_due(&st, CLAIM_PROMPT_INTERVAL_MS, "a"));
        assert!(!claim_prompt_due(&st, -1.0, "a")); // clock backwards
        let spent = store(&[("a", 3.0, 0.0)]);
        assert!(!claim_prompt_due(&spent, 1e12, "a"));
        let over = store(&[("a", 2.9, 0.0)]);
        assert!(claim_prompt_due(&over, CLAIM_PROMPT_INTERVAL_MS, "a"));
    }

    #[test]
    fn shown_prune_and_order() {
        // 9 accounts: the recorded one is pinned out of the sort and first in
        // the pruned map; the oldest (lowest lastShownAt) is dropped.
        let mut st: Vec<(String, JsVal)> = (0..8)
            .map(|i| (format!("p{i}"), rec(0.0, i as f64)))
            .collect();
        st.push(("new".to_string(), rec(0.0, 0.0)));
        let next = claim_prompt_shown(&st, 100.0, "new");
        assert_eq!(next.len(), 8);
        assert_eq!(next[0].0, "new");
        assert_eq!(next[0].1, rec(1.0, 100.0));
        // p0 (lastShownAt 0) is the oldest and gets pruned.
        let keys: Vec<&str> = next.iter().map(|(k, _)| k.as_str()).collect();
        assert!(!keys.contains(&"p0"));
        // Descending lastShownAt after the pinned entry.
        assert_eq!(keys[1], "p7");
        // Exactly 9 → no prune when 8 or fewer.
        let small = store(&[("a", 1.0, 5.0)]);
        let n2 = claim_prompt_shown(&small, 6.0, "b");
        assert_eq!(n2.len(), 2);
        assert_eq!(n2[1].0, "b");
        // Integer publicId lands in the integer section, not appended. The
        // store itself is built through map_set_v8 (V8 key order: "5" before
        // "z", matching the TS object literal {z:…, 5:…}).
        let mut st2: Vec<(String, JsVal)> = Vec::new();
        map_set_v8(&mut st2, "z", rec(0.0, 1.0));
        map_set_v8(&mut st2, "5", rec(0.0, 2.0));
        let n3 = claim_prompt_shown(&st2, 3.0, "1");
        let keys: Vec<&str> = n3.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["1", "5", "z"]);
    }

    #[test]
    fn strings_ready_gate() {
        assert!(!claim_prompt_strings_ready(
            "account_modal.username_claim_prompt",
            "h",
            "c"
        ));
        assert!(claim_prompt_strings_ready("b", "h", "c"));
        assert!(!claim_prompt_strings_ready(
            "b",
            "account_modal.username_claim_heading",
            "c"
        ));
        assert!(!claim_prompt_strings_ready(
            "b",
            "h",
            "account_modal.username_claim_prompt_confirm"
        ));
    }
}
