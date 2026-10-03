//! Port of the pure-decision subset of `src/server/Privilege.ts`:
//! `decideClanTag` (module-private, exposed through
//! `PrivilegeCheckerImpl.resolveClanTag`), `FailOpenPrivilegeChecker`,
//! `resolveVerifiedJoin`, `isTemporaryUsername` (re-exported from
//! `api_schemas`) and the `PrivilegeCheckerImpl.isAllowed` ORCHESTRATION
//! layer. The six leaf validators (`isPatternAllowed` / `isColorAllowed` /
//! `isSkinAllowed` / `isCrownAllowed` / `isFlagAllowed` / `isEffectAllowed`)
//! depend on the cosmetics catalog graph (PatternDecoder, countries.json,
//! `findEffectForSlot`) and are NOT replicated — they ride as a BLACK-BOX
//! scripted facade (precedent: the Censor obscenity matcher): the capture
//! monkey-patches the prototype methods to consult `globalThis.__PV_LEAF`,
//! and every leaf call is a trace event in the res stream (`res = [traceLen,
//! (trace)*, payload...]`), pinning the gate order, the short-circuit on the
//! first throw and the exact leaf inputs.
//!
//! Faithfulness notes:
//!
//! * `decideClanTag`: `censoredTag === null` -> `{tag:null,dropped:false}`
//!   (key order tag,dropped); else `tag = censoredTag.toUpperCase()` and
//!   `isMember = ownedClanTags.some(t => t.toUpperCase() === tag)`;
//!   `isMember || !reservedTags.has(tag)` -> `{tag: censoredTag, dropped:
//!   false}` — the KEPT tag is the ORIGINAL censoredTag, NOT the uppercased
//!   one; otherwise `{tag:null,dropped:true}`. `toUpperCase()` is JS
//!   Unicode-aware; the port uses Rust's `str::to_uppercase` over the
//!   capture's ASCII inputs (where the two agree byte-for-byte, precedent:
//!   `censor`). `reservedTags.has` is exact string equality (JS `Set`
//!   SameValueZero on strings).
//! * `FailOpenPrivilegeChecker.resolveClanTag` is the identity passthrough
//!   `{tag: censoredTag, dropped: false}`. Its `isAllowed` gates on
//!   `refs.verified === true` — STRICT, so `1` / `"true"` / truthy objects
//!   all fail the gate and yield `{type:"allowed",cosmetics:{}}`.
//! * `resolveVerifiedJoin` MUTATES its `cosmetics` argument in place: the
//!   fall-through path runs `delete cosmetics.verified`, which REMOVES the
//!   key entirely (the capture dumps the post-call object, pinning
//!   delete-vs-undefined). The gate order: `cosmetics.verified !== true`
//!   (strict — absent / undefined / null / non-true bool all -> "custom") ->
//!   `account === null` -> "dev" -> `entitled = usernameStatus === "premium"
//!   || === "indefinite"` -> `bare = typeof username === "string" &&
//!   username.length > 0 && username === usernameBase &&
//!   !isTemporaryUsername(usernameBase)` (the `&&` short-circuits: a
//!   non-string username never reads past the `typeof` guard, and
//!   `username.length` is the UTF-16 length) -> `entitled && bare &&
//!   joinUsername === account.username` -> "verified", else delete +
//!   "custom".
//! * `isTemporaryUsername` = `/^TEMPORARY\d{4}$/.test(base)` with NO `u`
//!   flag: `\d` matches ASCII 0-9 only (replicated by
//!   `api_schemas::is_temporary_username` over UTF-16 units).
//! * `PrivilegeCheckerImpl.isAllowed` orchestration: the refs key gates run
//!   in the FIXED order patternName -> color -> flag -> skinName -> crownName
//!   -> effects, each behind a TRUTHY gate ("" / 0 / NaN / null / undefined /
//!   absent are falsy). Every leaf call is wrapped in try/catch; on throw
//!   `e instanceof Error ? e.message : String(e)` and the forbidden reason is
//!   the verbatim prefix + message: `"invalid pattern: "`, `"invalid color: "`,
//!   `"invalid flag: "`, `"invalid skin: "`, `"invalid crown: "`,
//!   `"invalid effect: "`. The first throw returns immediately — later leaves
//!   are NEVER called (pinned by the trace).
//! * `effects` iterates `Object.entries(refs.effects)` — JS own-string-key
//!   insertion order. QUIRK: integer-like keys sort FIRST in JS; the capture
//!   deliberately uses non-integer ASCII slot names so insertion order is
//!   observable. `cosmetics.effects ??= {}` lazily creates the effects key on
//!   the FIRST entry assignment — an empty `refs.effects` object passes the
//!   truthy gate but the loop never runs, so the result carries NO `effects`
//!   key.
//! * The cosmetics result object's key insertion order is
//!   pattern,color,flag,skin,crown,effects,verified — a key appears only if
//!   its gate passed AND its leaf returned (no throw). `refs.verified ===
//!   true` (STRICT) appends `verified: true` last.
//! * The leaf facade keys are built from the REAL call arguments, mirroring
//!   the TS signatures: `pattern:<name>|<palette ?? "~">`, `color:<color>`,
//!   `flag:<flagRef>`, `skin:<name>`, `crown:<name>`, `effect:<slot>|<name>`.

use crate::api_schemas::is_temporary_username;
use crate::js_json::{map_set, push_str, push_val, read_str, read_val, val_field, JsVal};

/// JS truthiness restricted to the codec domain.
fn truthy(v: &JsVal) -> bool {
    match v {
        JsVal::Absent | JsVal::Undef | JsVal::Null => false,
        JsVal::Bool(b) => *b,
        JsVal::Num(n) => *n != 0.0 && !n.is_nan(),
        JsVal::Str(s) => !s.is_empty(),
        JsVal::Obj(_) | JsVal::Arr(_) => true,
    }
}

fn s(v: &str) -> JsVal {
    JsVal::Str(v.to_string())
}

/// JS `String(x)` for the codec domain (only reached for `String(e)` on a
/// non-Error throw; the capture scripts Errors, so this models the value
/// forms the harness could see).
fn js_to_string(v: &JsVal) -> String {
    match v {
        JsVal::Str(x) => x.clone(),
        JsVal::Num(n) => crate::game_ts::js_num_str(*n),
        JsVal::Bool(b) => b.to_string(),
        JsVal::Null => "null".to_string(),
        JsVal::Undef | JsVal::Absent => "undefined".to_string(),
        JsVal::Arr(_) | JsVal::Obj(_) => "[object Object]".to_string(),
    }
}

/// `decideClanTag(censoredTag, ownedClanTags, reservedTags)` — the module
/// private ownership rule.
pub fn decide_clan_tag(
    censored_tag: &JsVal,
    owned_clan_tags: &[String],
    reserved_tags: &[String],
) -> JsVal {
    // `censoredTag === null` — strict null (absent/undefined do NOT take
    // this branch in the TS type, but the codec never feeds them here).
    if matches!(censored_tag, JsVal::Null) {
        return JsVal::Obj(vec![
            ("tag".to_string(), JsVal::Null),
            ("dropped".to_string(), JsVal::Bool(false)),
        ]);
    }
    let original = match censored_tag {
        JsVal::Str(t) => t.clone(),
        other => js_to_string(other),
    };
    let tag = original.to_uppercase();
    let is_member = owned_clan_tags.iter().any(|t| t.to_uppercase() == tag);
    if is_member || !reserved_tags.contains(&tag) {
        // The KEPT tag is the ORIGINAL censoredTag, not the uppercased one.
        return JsVal::Obj(vec![
            ("tag".to_string(), JsVal::Str(original)),
            ("dropped".to_string(), JsVal::Bool(false)),
        ]);
    }
    JsVal::Obj(vec![
        ("tag".to_string(), JsVal::Null),
        ("dropped".to_string(), JsVal::Bool(true)),
    ])
}

/// `FailOpenPrivilegeChecker.resolveClanTag` — identity passthrough.
pub fn fail_open_resolve_clan_tag(censored_tag: &JsVal) -> JsVal {
    JsVal::Obj(vec![
        ("tag".to_string(), censored_tag.clone()),
        ("dropped".to_string(), JsVal::Bool(false)),
    ])
}

/// `FailOpenPrivilegeChecker.isAllowed` — only the verified intent passes
/// through, on the STRICT `refs.verified === true` gate.
pub fn fail_open_is_allowed(refs: &JsVal) -> JsVal {
    let cosmetics = match val_field(refs, "verified") {
        Some(JsVal::Bool(true)) => vec![("verified".to_string(), JsVal::Bool(true))],
        _ => Vec::new(),
    };
    JsVal::Obj(vec![
        ("type".to_string(), s("allowed")),
        ("cosmetics".to_string(), JsVal::Obj(cosmetics)),
    ])
}

/// `resolveVerifiedJoin(cosmetics, joinUsername, account)` — returns
/// `(verdict, mutated cosmetics)`: the fall-through path deletes
/// `cosmetics.verified` IN PLACE (the key is REMOVED, not set undefined).
pub fn resolve_verified_join(
    cosmetics: &JsVal,
    join_username: &str,
    account: &JsVal,
) -> (&'static str, JsVal) {
    let mut out = match cosmetics {
        JsVal::Obj(fields) => fields.clone(),
        other => match other {
            JsVal::Absent | JsVal::Undef => Vec::new(),
            _ => unreachable!("privilege: resolveVerifiedJoin cosmetics must be an object"),
        },
    };
    // `cosmetics.verified !== true` — strict; anything else is "custom"
    // WITHOUT touching the object.
    if !matches!(out.iter().find(|(k, _)| k == "verified").map(|(_, v)| v), Some(JsVal::Bool(true))) {
        return ("custom", JsVal::Obj(out));
    }
    if matches!(account, JsVal::Null) {
        return ("dev", JsVal::Obj(out));
    }
    let entitled = matches!(val_field(account, "usernameStatus"), Some(JsVal::Str(st)) if st == "premium" || st == "indefinite");
    let username = val_field(account, "username");
    let username_base = val_field(account, "usernameBase");
    let bare = matches!(username, Some(JsVal::Str(u)) if !u.is_empty())
        && match (username, username_base) {
            (Some(JsVal::Str(u)), Some(JsVal::Str(b))) => {
                u == b && !is_temporary_username(&b.encode_utf16().collect::<Vec<u16>>())
            }
            _ => false,
        };
    let name_matches = matches!(username, Some(JsVal::Str(u)) if *u == join_username);
    if entitled && bare && name_matches {
        return ("verified", JsVal::Obj(out));
    }
    // `delete cosmetics.verified` — remove the key entirely.
    out.retain(|(k, _)| k != "verified");
    ("custom", JsVal::Obj(out))
}

/// One scripted leaf outcome: a returned value, or a thrown Error message.
#[derive(Debug, Clone)]
enum LeafOutcome {
    Value(JsVal),
    Error(String),
}

/// The scripted leaf facade table: input key -> outcome. A miss is a
/// capture bug — the harness panics (the TS capture would throw through the
/// unpatched prototype).
#[derive(Debug, Default)]
struct LeafTable {
    rows: Vec<(String, LeafOutcome)>,
}

impl LeafTable {
    fn lookup(&self, key: &str) -> &LeafOutcome {
        self.rows
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, o)| o)
            .unwrap_or_else(|| panic!("privilege leaf facade: unscripted key {key:?}"))
    }
}

/// Trace codes: `40` isPatternAllowed, `41` isColorAllowed, `42`
/// isFlagAllowed, `43` isSkinAllowed, `44` isCrownAllowed, `45`
/// isEffectAllowed. Each event is `[code, (key-str), outcome 0|1, payload]`
/// where outcome 0 carries `val(value)` and 1 carries `(message-str)`.
#[derive(Debug, Default)]
pub struct RigHarness {
    reserved_tags: Vec<String>,
    leaves: LeafTable,
}

impl RigHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// One traced leaf facade call: emits the trace event and returns the
    /// leaf value, or the thrown Error's message.
    fn call_leaf(&self, code: f64, key: &str, trace: &mut Vec<f64>) -> Result<JsVal, String> {
        let outcome = self.leaves.lookup(key);
        trace.push(code);
        push_str(trace, key);
        match outcome {
            LeafOutcome::Value(v) => {
                trace.push(0.0);
                push_val(trace, v);
                Ok(v.clone())
            }
            LeafOutcome::Error(msg) => {
                trace.push(1.0);
                push_str(trace, msg);
                Err(msg.clone())
            }
        }
    }

    /// `PrivilegeCheckerImpl.isAllowed(flares, refs)` — the orchestration
    /// layer over the scripted leaves, traced.
    fn is_allowed(&self, refs: &JsVal, trace: &mut Vec<f64>) -> JsVal {
        let mut cosmetics: Vec<(String, JsVal)> = Vec::new();
        let forbidden = |prefix: &str, msg: String| -> JsVal {
            JsVal::Obj(vec![
                ("type".to_string(), s("forbidden")),
                ("reason".to_string(), s(&(prefix.to_string() + &msg))),
            ])
        };
        let gate = |k: &str| truthy(val_field(refs, k).unwrap_or(&JsVal::Absent));
        let str_of = |k: &str| js_to_string(val_field(refs, k).unwrap_or(&JsVal::Absent));

        if gate("patternName") {
            // TS passes `refs.patternColorPaletteName ?? null` to the leaf;
            // the scripted key builder collapses null / undefined / absent
            // to "~" (the `?? "~"` in the capture's patch), so all three
            // read alike here.
            let palette = match val_field(refs, "patternColorPaletteName") {
                Some(JsVal::Absent) | Some(JsVal::Undef) | Some(JsVal::Null) | None => "~".to_string(),
                Some(v) => js_to_string(v),
            };
            let key = format!("pattern:{}|{palette}", str_of("patternName"));
            match self.call_leaf(40.0, &key, trace) {
                Ok(v) => map_set(&mut cosmetics, "pattern", v),
                Err(msg) => return forbidden("invalid pattern: ", msg),
            }
        }
        if gate("color") {
            let key = format!("color:{}", str_of("color"));
            match self.call_leaf(41.0, &key, trace) {
                Ok(v) => map_set(&mut cosmetics, "color", v),
                Err(msg) => return forbidden("invalid color: ", msg),
            }
        }
        if gate("flag") {
            let key = format!("flag:{}", str_of("flag"));
            match self.call_leaf(42.0, &key, trace) {
                Ok(v) => map_set(&mut cosmetics, "flag", v),
                Err(msg) => return forbidden("invalid flag: ", msg),
            }
        }
        if gate("skinName") {
            let key = format!("skin:{}", str_of("skinName"));
            match self.call_leaf(43.0, &key, trace) {
                Ok(v) => map_set(&mut cosmetics, "skin", v),
                Err(msg) => return forbidden("invalid skin: ", msg),
            }
        }
        if gate("crownName") {
            let key = format!("crown:{}", str_of("crownName"));
            match self.call_leaf(44.0, &key, trace) {
                Ok(v) => map_set(&mut cosmetics, "crown", v),
                Err(msg) => return forbidden("invalid crown: ", msg),
            }
        }
        if gate("effects") {
            // `Object.entries(refs.effects)` — insertion order of the codec
            // object (the capture pins non-integer ASCII slot names so JS
            // integer-key reordering cannot bite).
            let entries: Vec<(String, JsVal)> = match val_field(refs, "effects") {
                Some(JsVal::Obj(fields)) => fields.clone(),
                _ => Vec::new(),
            };
            for (slot, name_val) in entries {
                let name = js_to_string(&name_val);
                let key = format!("effect:{slot}|{name}");
                match self.call_leaf(45.0, &key, trace) {
                    Ok(v) => {
                        // `cosmetics.effects ??= {}` then `[slot] = value`.
                        if !cosmetics.iter().any(|(k, _)| k == "effects") {
                            cosmetics.push(("effects".to_string(), JsVal::Obj(Vec::new())));
                        }
                        let holder = cosmetics
                            .iter_mut()
                            .find(|(k, _)| k == "effects")
                            .map(|(_, v)| v)
                            .unwrap();
                        if let JsVal::Obj(inner) = holder {
                            map_set(inner, &slot, v);
                        }
                    }
                    Err(msg) => return forbidden("invalid effect: ", msg),
                }
            }
        }
        // `refs.verified === true` — STRICT gate, appended last.
        if matches!(val_field(refs, "verified"), Some(JsVal::Bool(true))) {
            map_set(&mut cosmetics, "verified", JsVal::Bool(true));
        }
        JsVal::Obj(vec![
            ("type".to_string(), s("allowed")),
            ("cosmetics".to_string(), JsVal::Obj(cosmetics)),
        ])
    }

    /// Run one op. Kind table (see `tools/gen_vectors.mjs`):
    /// 0 reset -> `[0]`;
    /// 1 scriptReservedTags `[n, (tag-str)*n]` -> `[0]`;
    /// 2 resolveClanTag `(Impl)` `[(censoredTag val), n, (owned-str)*n]` ->
    ///   codec `{tag,dropped}`;
    /// 3 failOpenResolveClanTag `[(censoredTag val)]` -> codec `{tag,dropped}`;
    /// 4 failOpenIsAllowed `[refs]` -> codec result;
    /// 5 resolveVerifiedJoin `[cosmetics, joinUsername-str, account]` ->
    ///   `(verdict-str, ...codec mutated cosmetics)`;
    /// 6 isTemporaryUsername `[(str)]` -> `[0|1]`;
    /// 7 scriptLeaves `[n, (key-str, outcome 0|1, payload)*n]` -> `[0]`;
    /// 8 isAllowed `[refs]` -> `[traceLen,(trace)*,codec result]`.
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut i = 0usize;
        let mut trace: Vec<f64> = Vec::new();
        let payload: Vec<f64> = match kind {
            0 => {
                self.reset();
                vec![0.0]
            }
            1 => {
                let n = args[i] as usize;
                i += 1;
                for _ in 0..n {
                    let t = read_str(args, &mut i);
                    self.reserved_tags.push(t);
                }
                vec![0.0]
            }
            2 => {
                let censored_tag = read_val(args, &mut i);
                let n = args[i] as usize;
                i += 1;
                let owned: Vec<String> = (0..n).map(|_| read_str(args, &mut i)).collect();
                let mut out = Vec::new();
                push_val(&mut out, &decide_clan_tag(&censored_tag, &owned, &self.reserved_tags));
                out
            }
            3 => {
                let censored_tag = read_val(args, &mut i);
                let mut out = Vec::new();
                push_val(&mut out, &fail_open_resolve_clan_tag(&censored_tag));
                out
            }
            4 => {
                let refs = read_val(args, &mut i);
                let mut out = Vec::new();
                push_val(&mut out, &fail_open_is_allowed(&refs));
                out
            }
            5 => {
                let cosmetics = read_val(args, &mut i);
                let join_username = read_str(args, &mut i);
                let account = read_val(args, &mut i);
                let (verdict, mutated) = resolve_verified_join(&cosmetics, &join_username, &account);
                let mut out = Vec::new();
                push_str(&mut out, verdict);
                push_val(&mut out, &mutated);
                out
            }
            6 => {
                let t = read_str(args, &mut i);
                vec![if is_temporary_username(&t.encode_utf16().collect::<Vec<u16>>()) {
                    1.0
                } else {
                    0.0
                }]
            }
            7 => {
                let n = args[i] as usize;
                i += 1;
                for _ in 0..n {
                    let key = read_str(args, &mut i);
                    let outcome = args[i] != 0.0;
                    i += 1;
                    if outcome {
                        let msg = read_str(args, &mut i);
                        self.leaves.rows.push((key, LeafOutcome::Error(msg)));
                    } else {
                        let v = read_val(args, &mut i);
                        self.leaves.rows.push((key, LeafOutcome::Value(v)));
                    }
                }
                vec![0.0]
            }
            8 => {
                let refs = read_val(args, &mut i);
                let r = self.is_allowed(&refs, &mut trace);
                let mut out = Vec::new();
                push_val(&mut out, &r);
                out
            }
            k => unreachable!("privilege harness: unknown op kind {k}"),
        };
        if kind == 8 {
            let mut out = Vec::with_capacity(1 + trace.len() + payload.len());
            out.push(trace.len() as f64);
            out.extend(trace.iter().copied());
            out.extend(payload);
            out
        } else {
            payload
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn clan_tag_branches() {
        let reserved = tags(&["EVIL"]);
        // null -> {tag:null,dropped:false}
        let r = decide_clan_tag(&JsVal::Null, &tags(&[]), &reserved);
        assert_eq!(
            r,
            JsVal::Obj(vec![
                ("tag".to_string(), JsVal::Null),
                ("dropped".to_string(), JsVal::Bool(false)),
            ])
        );
        // member (case-insensitive) -> keeps the ORIGINAL tag
        let r = decide_clan_tag(&JsVal::Str("abc".into()), &tags(&["AbC"]), &reserved);
        if let JsVal::Obj(f) = &r {
            assert_eq!(f[0].1, JsVal::Str("abc".into()));
            assert_eq!(f[1].1, JsVal::Bool(false));
        }
        // non-member, not reserved -> original passthrough
        let r = decide_clan_tag(&JsVal::Str("xyz".into()), &tags(&[]), &reserved);
        assert!(matches!(&r, JsVal::Obj(f) if f[0].1 == JsVal::Str("xyz".into())));
        // non-member, reserved (uppercase compare) -> dropped
        let r = decide_clan_tag(&JsVal::Str("evil".into()), &tags(&[]), &reserved);
        assert_eq!(
            r,
            JsVal::Obj(vec![
                ("tag".to_string(), JsVal::Null),
                ("dropped".to_string(), JsVal::Bool(true)),
            ])
        );
    }

    #[test]
    fn fail_open_strict_verified_gate() {
        // 1 is NOT true -> empty cosmetics.
        let refs = JsVal::Obj(vec![("verified".to_string(), JsVal::Num(1.0))]);
        let r = fail_open_is_allowed(&refs);
        assert!(matches!(&r, JsVal::Obj(f) if f[1].1 == JsVal::Obj(vec![])));
        let refs = JsVal::Obj(vec![("verified".to_string(), JsVal::Bool(true))]);
        let r = fail_open_is_allowed(&refs);
        assert!(matches!(&r, JsVal::Obj(f) if
            f[0].1 == JsVal::Str("allowed".into()) &&
            f[1].1 == JsVal::Obj(vec![("verified".to_string(), JsVal::Bool(true))])));
    }

    #[test]
    fn verified_join_delete_mutates_in_place() {
        let cosmetics = JsVal::Obj(vec![
            ("color".to_string(), JsVal::Obj(vec![("color".to_string(), s("red"))])),
            ("verified".to_string(), JsVal::Bool(true)),
        ]);
        let account = JsVal::Obj(vec![
            ("username".to_string(), s("Alice")),
            ("usernameBase".to_string(), s("Alice")),
            ("usernameStatus".to_string(), s("premium")),
        ]);
        let (v, _) = resolve_verified_join(&cosmetics, "Alice", &account);
        assert_eq!(v, "verified");
        // Entitled but display != base -> fall-through DELETES verified.
        let account = JsVal::Obj(vec![
            ("username".to_string(), s("Alice.7")),
            ("usernameBase".to_string(), s("Alice")),
            ("usernameStatus".to_string(), s("premium")),
        ]);
        let (v, mutated) = resolve_verified_join(&cosmetics, "Alice.7", &account);
        assert_eq!(v, "custom");
        if let JsVal::Obj(f) = &mutated {
            assert_eq!(f.len(), 1);
            assert_eq!(f[0].0, "color");
        }
        // null account -> dev, no mutation.
        let (v, mutated) = resolve_verified_join(&cosmetics, "x", &JsVal::Null);
        assert_eq!(v, "dev");
        assert_eq!(mutated, cosmetics);
    }

    #[test]
    fn temporary_username_boundary() {
        assert!(is_temporary_username(&"TEMPORARY1234".encode_utf16().collect::<Vec<u16>>()));
        assert!(!is_temporary_username(&"TEMPORARY12345".encode_utf16().collect::<Vec<u16>>()));
    }

    fn enc(x: &str) -> Vec<f64> {
        let mut v = vec![x.encode_utf16().count() as f64];
        v.extend(x.encode_utf16().map(|u| u as f64));
        v
    }

    fn script_leaves(rows: &[(&str, bool, JsVal, &str)]) -> Vec<f64> {
        let mut a = vec![rows.len() as f64];
        for (key, is_err, val, msg) in rows {
            a.extend(enc(key));
            a.push(if *is_err { 1.0 } else { 0.0 });
            if *is_err {
                a.extend(enc(msg));
            } else {
                push_val(&mut a, val);
            }
        }
        a
    }

    #[test]
    fn orchestration_key_order_and_short_circuit() {
        let mut h = RigHarness::new();
        h.run_op(
            7,
            &script_leaves(&[
                (
                    "color:red",
                    false,
                    JsVal::Obj(vec![("color".to_string(), s("red"))]),
                    "",
                ),
                (
                    "flag:us",
                    true,
                    JsVal::Absent,
                    "no flares for flag",
                ),
            ]),
        );
        let mut refs_a = Vec::new();
        push_val(
            &mut refs_a,
            &JsVal::Obj(vec![
                ("color".to_string(), s("red")),
                ("flag".to_string(), s("us")),
                ("verified".to_string(), JsVal::Bool(true)),
            ]),
        );
        let r = h.run_op(8, &refs_a);
        let tl = r[0] as usize;
        // trace: 41 value, 42 error -> forbidden; verified never reached.
        assert!(tl > 0);
        assert_eq!(r[1], 41.0);
        let mut j = 1usize;
        // skip the 41 event
        j += 1;
        j += 1 + (r[j] as usize); // key
        j += 1; // outcome 0
        let _ = read_val(&r, &mut j); // value
        assert_eq!(r[j], 42.0);
        let mut out = Vec::new();
        out.extend_from_slice(&r[1 + tl..]);
        let mut k = 0usize;
        let res = read_val(&out, &mut k);
        assert!(matches!(&res, JsVal::Obj(f) if
            f[0].1 == JsVal::Str("forbidden".into()) &&
            f[1].1 == JsVal::Str("invalid flag: no flares for flag".into())));
    }

    #[test]
    fn effects_lazy_init_and_empty_object() {
        let mut h = RigHarness::new();
        h.run_op(
            7,
            &script_leaves(&[(
                "effect:trail|comet",
                false,
                JsVal::Obj(vec![("name".to_string(), s("comet"))]),
                "",
            )]),
        );
        // Empty effects object: truthy gate passes, loop never runs -> NO
        // effects key in the result.
        let mut a = Vec::new();
        push_val(&mut a, &JsVal::Obj(vec![("effects".to_string(), JsVal::Obj(vec![]))]));
        let r = h.run_op(8, &a);
        let tl = r[0] as usize;
        assert_eq!(tl, 0);
        let mut j = 1 + tl;
        let res = read_val(&r, &mut j);
        assert!(matches!(&res, JsVal::Obj(f) if f[1].1 == JsVal::Obj(vec![])));
        // One entry: effects key lazily created, slot key inside.
        let mut b = Vec::new();
        push_val(
            &mut b,
            &JsVal::Obj(vec![(
                "effects".to_string(),
                JsVal::Obj(vec![("trail".to_string(), s("comet"))]),
            )]),
        );
        let r = h.run_op(8, &b);
        let tl = r[0] as usize;
        let mut j = 1 + tl;
        let res = read_val(&r, &mut j);
        if let JsVal::Obj(f) = &res {
            let cosmetics = match &f[1].1 {
                JsVal::Obj(c) => c,
                _ => panic!(),
            };
            assert_eq!(cosmetics.len(), 1);
            assert_eq!(cosmetics[0].0, "effects");
            assert_eq!(
                cosmetics[0].1,
                JsVal::Obj(vec![(
                    "trail".to_string(),
                    JsVal::Obj(vec![("name".to_string(), s("comet"))])
                )])
            );
        } else {
            panic!()
        }
    }
}
