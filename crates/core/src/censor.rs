//! Port of the table + orchestration logic of `src/server/Censor.ts`:
//! `shadowNames` / `bannedWords` and `censorPlayer(username, clanTag)`.
//! The obscenity `profanityMatcher` is a BLACK-BOX scripted facade (the
//! library is unresolvable in the port repo, precedent: the NVS `Client`
//! facade): the capture scripts `hasMatch(input) -> bool` and
//! `getAllMatches(input) -> [{startIndex, endIndex}, ...]` per input, and
//! every facade call is a trace event in the res stream (`res = [traceLen,
//! (trace)*, payload...]`), pinning the call counts, the input strings and
//! the `||` / `.some()` short-circuit orders. The obscenity internals
//! (transformer chains, the `kkk` includes check) are NOT replicated.
//!
//! Faithfulness notes (`censorPlayer`):
//!
//! * `usernameIsProfane = matcher.hasMatch(username)` — always ONE
//!   `hasMatch` call.
//! * `clanTagIsProfane = clanTag ? (hasMatch(clanTag) ||
//!   clanTag.toLowerCase() === "ss") : false` — `clanTag ?` is TRUTHY:
//!   `null` AND the empty string take the false branch (zero facade calls).
//!   `||` short-circuits: when `hasMatch(clanTag)` is true the `"ss"`
//!   compare never runs (unobservable except through the facade trace: the
//!   hasMatch call still happened).
//! * `combinedSlurAcrossBoundary = clanTag ? getAllMatches(clanTag +
//!   username).some(m => m.startIndex < clanTag.length && m.endIndex >=
//!   clanTag.length) : false` — the CONCATENATION ORDER is tag+name; the
//!   `some` predicate `&&` short-circuits per element and stops at the
//!   first true. Empty-string clanTag -> false with NO getAllMatches call.
//! * `censoredName`: `usernameIsProfane || combinedSlurAcrossBoundary` ->
//!   `shadowNames[simpleHash(username) % shadowNames.length]` — `simpleHash`
//!   returns a non-negative integer (`Math.abs` of an i32), so JS `%` and
//!   Rust `%` agree; the hash is computed ONLY when the branch is taken.
//! * `censoredClanTag`: `clanTag && !clanTagIsProfane &&
//!   !combinedSlurAcrossBoundary` -> `clanTag.toUpperCase()`, else `null` —
//!   again the truthy gate (empty tag -> null).
//! * The result is `{username, clanTag}` in that key order; `clanTag` is
//!   always PRESENT (a JS `null` when dropped, never Absent).
//! * `toUpperCase()` / `toLowerCase()` are JS Unicode-aware; the port uses
//!   Rust's `str::to_uppercase` / `to_lowercase` over the capture's ASCII
//!   inputs (the clan tags / usernames the scenarios feed are ASCII, where
//!   the two agree byte-for-byte).

use crate::js_json::{push_str, push_val, read_str, read_val, JsVal};
use crate::util::simple_hash;

/// `shadowNames` — verbatim (including the two space-bearing entries).
pub const SHADOW_NAMES: [&str; 21] = [
    "UnhuggedToday",
    "DaddysLilChamp",
    "BunnyKisses67",
    "SnugglePuppy",
    "CuddleMonster67",
    "DaddysLilStar",
    "SnuggleMuffin",
    "PeesALittle",
    "PleaseFullSendMe",
    "NanasLilMan",
    "NoAlliances",
    "TryingTooHard67",
    "MommysLilStinker",
    "NeedHugs",
    "MommysLilPeanut",
    "IWillBetrayU",
    "DaddysLilTater",
    "PreciousBubbles",
    "67 Cringelord",
    "Peace And Love",
    "AlmostPottyTrained",
];

/// `bannedWords` — verbatim (table dump parity; the matcher facade
/// subsumes their runtime role).
pub const BANNED_WORDS: [&str; 13] = [
    "nigger",
    "nigga",
    "chink",
    "spic",
    "kike",
    "faggot",
    "retard",
    "hitler",
    "adolf",
    "nazi",
    "auschwitz",
    "whitepower",
    "heil",
];

/// Scripted matcher row: `input -> (hasMatch, matches)`.
type MatchRow = (String, bool, Vec<(f64, f64)>);

/// The scripted matcher facade.
/// A miss (input never scripted) is a capture bug — the harness panics,
/// mirroring the NVS facade contract that every real call is scripted.
#[derive(Debug, Default)]
struct Matcher {
    table: Vec<MatchRow>,
}

impl Matcher {
    fn lookup(&self, input: &str) -> (bool, &Vec<(f64, f64)>) {
        self.table
            .iter()
            .find(|(k, _, _)| k == input)
            .map(|(_, h, m)| (*h, m))
            .unwrap_or_else(|| panic!("censor facade: unscripted input {input:?}"))
    }
}

/// The capture harness: the matcher table plus traced facade calls.
/// Trace codes: `30` = `hasMatch(input)` (`[30, (input-str), 0|1]`),
/// `31` = `getAllMatches(input)` (`[31, (input-str), n, (start,end)*n]`).
#[derive(Debug, Default)]
pub struct RigHarness {
    matcher: Matcher,
}

impl RigHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// `censorPlayer(username, clanTag)` — clanTag codec `Null` models JS
    /// `null`; a `Str("")` models the empty string (falsy gate).
    fn censor_player(&self, username: &str, clan_tag: &JsVal, trace: &mut Vec<f64>) -> JsVal {
        let has_match = |m: &Matcher, input: &str, trace: &mut Vec<f64>| -> bool {
            let (r, _) = m.lookup(input);
            trace.push(30.0);
            push_str(trace, input);
            trace.push(if r { 1.0 } else { 0.0 });
            r
        };
        let get_all = |m: &Matcher, input: &str, trace: &mut Vec<f64>| -> Vec<(f64, f64)> {
            let (_, ms) = m.lookup(input);
            trace.push(31.0);
            push_str(trace, input);
            trace.push(ms.len() as f64);
            for (s, e) in ms {
                trace.push(*s);
                trace.push(*e);
            }
            ms.clone()
        };

        let username_is_profane = has_match(&self.matcher, username, trace);
        // `clanTag ?` — truthy gate: null / absent / undefined / "" -> false.
        let tag_str: Option<&str> = match clan_tag {
            JsVal::Str(s) if !s.is_empty() => Some(s.as_str()),
            _ => None,
        };
        let clan_tag_is_profane = match tag_str {
            None => false,
            Some(t) => {
                has_match(&self.matcher, t, trace) || t.to_lowercase() == "ss"
            }
        };
        let combined_slur = match tag_str {
            None => false,
            Some(t) => {
                let joined = format!("{}{}", t, username);
                get_all(&self.matcher, &joined, trace)
                    .into_iter()
                    .any(|(start, end)| start < (t.encode_utf16().count() as f64) && end >= (t.encode_utf16().count() as f64))
            }
        };
        let censored_name: String = if username_is_profane || combined_slur {
            let h = simple_hash(username);
            SHADOW_NAMES[(h % SHADOW_NAMES.len() as f64) as usize].to_string()
        } else {
            username.to_string()
        };
        let censored_clan_tag: JsVal = match (tag_str, clan_tag_is_profane, combined_slur) {
            (Some(t), false, false) => JsVal::Str(t.to_uppercase()),
            _ => JsVal::Null,
        };
        JsVal::Obj(vec![
            ("username".to_string(), JsVal::Str(censored_name)),
            ("clanTag".to_string(), censored_clan_tag),
        ])
    }

    /// Run one op. Kind table (see `tools/gen_vectors.mjs`):
    /// 0 construct/reset -> `[0]`;
    /// 1 scriptMatcher `[n, (input-str, has 0|1, m, (start,end)*m)*n]` ->
    ///   `[0]`;
    /// 2 censorPlayer `[(username-str), clanTag]` -> `[traceLen,(trace)*,
    ///   val(result)]`;
    /// 3 dump shadowNames -> `[21, (str)*]`;
    /// 4 dump bannedWords -> `[13, (str)*]`.
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
                    let input = read_str(args, &mut i);
                    let has = args[i] != 0.0;
                    i += 1;
                    let m = args[i] as usize;
                    i += 1;
                    let mut ms = Vec::with_capacity(m);
                    for _ in 0..m {
                        let s = args[i];
                        let e = args[i + 1];
                        i += 2;
                        ms.push((s, e));
                    }
                    // Append; `lookup` finds the FIRST row (mirrors the TS
                    // facade's `cnRows.find` over the capture's push order).
                    self.matcher.table.push((input, has, ms));
                }
                vec![0.0]
            }
            2 => {
                let username = read_str(args, &mut i);
                let clan_tag = read_val(args, &mut i);
                let r = self.censor_player(&username, &clan_tag, &mut trace);
                let mut out = Vec::new();
                push_val(&mut out, &r);
                out
            }
            3 => {
                let mut out = vec![SHADOW_NAMES.len() as f64];
                for s in SHADOW_NAMES {
                    push_str(&mut out, s);
                }
                out
            }
            4 => {
                let mut out = vec![BANNED_WORDS.len() as f64];
                for s in BANNED_WORDS {
                    push_str(&mut out, s);
                }
                out
            }
            k => unreachable!("censor harness: unknown op kind {k}"),
        };
        if kind == 2 {
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

    fn s(x: &str) -> Vec<f64> {
        let mut v = vec![x.len() as f64];
        v.extend(x.encode_utf16().map(|u| u as f64));
        v
    }

    #[allow(clippy::type_complexity)]
    fn script(rows: &[(&str, bool, &[(f64, f64)])]) -> Vec<f64> {
        let mut a = vec![rows.len() as f64];
        for (input, has, ms) in rows {
            a.extend(s(input));
            a.push(if *has { 1.0 } else { 0.0 });
            a.push(ms.len() as f64);
            for (st, e) in *ms {
                a.push(*st);
                a.push(*e);
            }
        }
        a
    }

    fn censor(username: &str, clan_tag: JsVal) -> (Vec<f64>, JsVal) {
        let mut h = RigHarness::new();
        h.run_op(
            1,
            &script(&[
                ("Clean", false, &[]),
                ("BadName", true, &[]),
                ("ok", false, &[]),
                ("ss", false, &[]),
                ("Hit", false, &[]),
                ("LER", false, &[]),
                ("HitLER", true, &[(0.0, 6.0)]),
                ("okClean", false, &[]),
                ("ssClean", false, &[]),
            ]),
        );
        let mut a = s(username);
        push_val(&mut a, &clan_tag);
        let r = h.run_op(2, &a);
        let tl = r[0] as usize;
        let mut i = 1 + tl;
        (r[..1 + tl].to_vec(), read_val(&r, &mut i))
    }

    #[test]
    fn clean_passthrough_uppercases_tag() {
        let (trace, res) = censor("Clean", JsVal::Str("ok".into()));
        // hasMatch(Clean), hasMatch(ok), getAllMatches(okClean) -> 3 calls.
        let n_has = trace[1..].iter().filter(|&&x| x == 30.0).count();
        assert_eq!(n_has, 2);
        assert_eq!(
            res,
            JsVal::Obj(vec![
                ("username".to_string(), JsVal::Str("Clean".into())),
                ("clanTag".to_string(), JsVal::Str("OK".into())),
            ])
        );
    }

    #[test]
    fn profane_username_shadow() {
        let (_, res) = censor("BadName", JsVal::Null);
        let h = simple_hash("BadName");
        let want = SHADOW_NAMES[(h % 21.0) as usize];
        if let JsVal::Obj(f) = &res {
            assert_eq!(f[0].1, JsVal::Str(want.into()));
            assert_eq!(f[1].1, JsVal::Null);
        } else {
            panic!();
        }
    }

    #[test]
    fn ss_tag_null_even_if_match_false() {
        let (_, res) = censor("Clean", JsVal::Str("ss".into()));
        if let JsVal::Obj(f) = &res {
            assert_eq!(f[1].1, JsVal::Null);
        }
    }

    #[test]
    fn boundary_slur_shadows_and_drops_tag() {
        // tag "Hit" + name "LER" -> "HitLER" match [0,6) crosses the
        // boundary (6 >= 3 and 0 < 3): name shadowed, tag nulled.
        let (_, res) = censor("LER", JsVal::Str("Hit".into()));
        let h = simple_hash("LER");
        let want = SHADOW_NAMES[(h % 21.0) as usize];
        if let JsVal::Obj(f) = &res {
            assert_eq!(f[0].1, JsVal::Str(want.into()));
            assert_eq!(f[1].1, JsVal::Null);
        }
    }

    #[test]
    fn empty_tag_truthy_gate_no_facade() {
        let (trace, res) = censor("Clean", JsVal::Str("".into()));
        // ONLY hasMatch(Clean) — the falsy "" skips both clan branches.
        assert_eq!(trace[1..].iter().filter(|&&x| x == 30.0).count(), 1);
        assert_eq!(trace[1..].iter().filter(|&&x| x == 31.0).count(), 0);
        if let JsVal::Obj(f) = &res {
            assert_eq!(f[1].1, JsVal::Null);
        }
    }

    #[test]
    fn tables_dump() {
        let mut h = RigHarness::new();
        let d = h.run_op(3, &[]);
        assert_eq!(d[0], 21.0);
        let d2 = h.run_op(4, &[]);
        assert_eq!(d2[0], 13.0);
    }
}
