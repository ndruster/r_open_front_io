//! Port of the pure gate functions of `src/client/GameModeSelector.ts` plus
//! the `DesktopShell.ts` predicates they defer to
//! (`multiplayerAllowed` / `failedAllowsMultiplayer` /
//! `multiplayerAllowedForSession`). The lit component, the DOM/refusal
//! feedback (`reportMultiplayerRefusal`) and the effectful ServerList reads
//! stay in TS.
//!
//! Faithfulness notes (quirk list):
//!
//! * `multiplayerAllowed`: the status allow-list is `current` / `checking` /
//!   `blocked` (the first two gates are one `||` chain, order irrelevant);
//!   `failed` delegates to `failedAllowsMultiplayer(state.error?.kind)` —
//!   an absent / nullish error reads `undefined` → allowed; anything else
//!   (`downloading`, `staged`, or an unrecognised status) → false.
//! * `failedAllowsMultiplayer` is an allow-list switch: `undefined` → true,
//!   `refused` / `parse` → true, `network` / `verify` → false, and the
//!   `default` (a kind from a newer shell, or a non-string) → false.
//! * `multiplayerAllowedForSession`: `null` (web build) → true; `unknown` /
//!   `signed-in` → true; `retrying` / `signed-out` / unrecognised → false.
//! * `joinIsGateable`: the optional chain `lobby.gameStartInfo?.config
//!   .gameType` — an absent gameStartInfo reads `undefined`, and
//!   `undefined !== "Singleplayer"` is TRUE (the gate passes); `gameRecord
//!   === undefined` treats an absent key and a present-`undefined` alike.
//!   `GameType.Singleplayer` is the STRING enum value `"Singleplayer"`.
//! * `shouldBlockMultiplayerAction` short-circuits in order update →
//!   session → backend; `shouldBlockSocketSourcedAction` is the same call
//!   with `backendOutage` nailed to false; `shouldBlockJoin` returns false
//!   BEFORE consulting any gate when the join is not gateable.

use crate::js_json::{read_val, val_field, JsVal};

/// `GameType.Singleplayer` (string enum, Game.ts).
pub const GAME_TYPE_SINGLEPLAYER: &str = "Singleplayer";

/// `multiplayerAllowedForBackend(backendOutage)` — `!backendOutage`.
pub fn multiplayer_allowed_for_backend(backend_outage: bool) -> bool {
    !backend_outage
}

/// `failedAllowsMultiplayer(kind)` — `None` models `undefined` ONLY (a
/// present-but-`null` kind hits the switch default → false).
pub fn failed_allows_multiplayer(kind: Option<&str>) -> bool {
    match kind {
        None => true,
        Some("refused") | Some("parse") => true,
        Some("network") | Some("verify") => false,
        Some(_) => false, // default: a newer shell's kind, or non-string
    }
}

/// `multiplayerAllowed(state)` — the codec update state (Obj with `status`,
/// optional `error.kind`).
pub fn multiplayer_allowed(state: &JsVal) -> bool {
    let status = val_field(state, "status");
    match status {
        Some(JsVal::Str(s)) if s == "current" || s == "checking" || s == "blocked" => true,
        Some(JsVal::Str(s)) if s == "failed" => {
            // `state.error?.kind`: `?.` guards null/undefined only — a
            // present non-object error reads `.kind` undefined too, but
            // the typed domain keeps the distinction honest: nullish
            // error → undefined kind; object error → its kind field
            // (absent/undefined → None, present non-string → default).
            let kind = match val_field(state, "error") {
                None | Some(JsVal::Absent) | Some(JsVal::Undef) | Some(JsVal::Null) => None,
                Some(err @ JsVal::Obj(_)) => match val_field(err, "kind") {
                    Some(JsVal::Str(k)) => Some(k.as_str()),
                    Some(JsVal::Absent) | Some(JsVal::Undef) | None => None,
                    // A present-but-null kind is NOT undefined → default.
                    Some(_) => Some("\u{0}nonstring"),
                },
                Some(_) => None,
            };
            failed_allows_multiplayer(kind)
        }
        _ => false,
    }
}

/// `multiplayerAllowedForSession(state)` — `None` models the JS `null`
/// (web build).
pub fn multiplayer_allowed_for_session(state: Option<&JsVal>) -> bool {
    let Some(state) = state else { return true };
    matches!(val_field(state, "status"), Some(JsVal::Str(s)) if s == "unknown" || s == "signed-in")
}

/// `shouldBlockMultiplayerAction(update, session, backendOutage)` — the
/// codec states; `update`/`session` `None` models null.
pub fn should_block_multiplayer_action(
    update: Option<&JsVal>,
    session: Option<&JsVal>,
    backend_outage: bool,
) -> bool {
    if let Some(u) = update {
        if !multiplayer_allowed(u) {
            return true;
        }
    }
    if !multiplayer_allowed_for_session(session) {
        return true;
    }
    !multiplayer_allowed_for_backend(backend_outage)
}

/// `lobbyFeedSuspended(session)`.
pub fn lobby_feed_suspended(session: Option<&JsVal>) -> bool {
    session.is_some() && !multiplayer_allowed_for_session(session)
}

/// `shouldBlockSocketSourcedAction(update, session)`.
pub fn should_block_socket_sourced_action(update: Option<&JsVal>, session: Option<&JsVal>) -> bool {
    should_block_multiplayer_action(update, session, false)
}

/// `joinIsGateable(lobby)`.
pub fn join_is_gateable(lobby: &JsVal) -> bool {
    let game_type = match val_field(lobby, "gameStartInfo") {
        Some(gsi @ JsVal::Obj(_)) => match val_field(gsi, "config") {
            Some(cfg @ JsVal::Obj(_)) => val_field(cfg, "gameType").cloned(),
            _ => None,
        },
        _ => None,
    };
    let gateable_type = match game_type {
        None | Some(JsVal::Absent) | Some(JsVal::Undef) => true,
        Some(JsVal::Str(s)) => s != GAME_TYPE_SINGLEPLAYER,
        Some(_) => true,
    };
    let no_record = match val_field(lobby, "gameRecord") {
        None | Some(JsVal::Absent) | Some(JsVal::Undef) => true,
        Some(_) => false,
    };
    gateable_type && no_record
}

/// `shouldBlockJoin(lobby, update, session)`.
pub fn should_block_join(
    lobby: &JsVal,
    update: Option<&JsVal>,
    session: Option<&JsVal>,
) -> bool {
    if !join_is_gateable(lobby) {
        return false;
    }
    should_block_socket_sourced_action(update, session)
}

// ---------------------------------------------------------------- vectors op
//
// kind 0: [n, (outage)*n] -> [n,(0/1)*n]                     allowedForBackend
// kind 1: [n, (codec update)*n] -> [n,(0/1)*n]              multiplayerAllowed
// kind 2: [n, (codec session|null)*n] -> [n,(0/1)*n]        allowedForSession
// kind 3: [n, (update|null, session|null, outage)*n] -> [n,(0/1)*n]
//         shouldBlockMultiplayerAction
// kind 4: [n, (session|null)*n] -> [n,(0/1)*n]              lobbyFeedSuspended
// kind 5: [n, (update|null, session|null)*n] -> [n,(0/1)*n]
//         shouldBlockSocketSourcedAction
// kind 6: [n, (codec lobby)*n] -> [n,(0/1)*n]               joinIsGateable
// kind 7: [n, (lobby, update|null, session|null)*n] -> [n,(0/1)*n]
//         shouldBlockJoin
// kind 8: [n, (codec kind|null|undefined)*n] -> [n,(0/1)*n] failedAllowsMultiplayer

/// Flat-token runner shared by the golden replay and the wasm probe.
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    let nullish = |v: &JsVal| matches!(v, JsVal::Null | JsVal::Absent | JsVal::Undef);
    match kind {
        0 => {
            let n = args[0] as usize;
            out.push(n as f64);
            for k in 0..n {
                out.push(if multiplayer_allowed_for_backend(args[1 + k] != 0.0) { 1.0 } else { 0.0 });
            }
        }
        1 => {
            let n = args[0] as usize;
            let mut i = 1usize;
            out.push(n as f64);
            for _ in 0..n {
                let u = read_val(args, &mut i);
                out.push(if multiplayer_allowed(&u) { 1.0 } else { 0.0 });
            }
        }
        2 | 4 => {
            let n = args[0] as usize;
            let mut i = 1usize;
            out.push(n as f64);
            for _ in 0..n {
                let s = read_val(args, &mut i);
                let s_opt = if nullish(&s) { None } else { Some(&s) };
                let r = if kind == 2 {
                    multiplayer_allowed_for_session(s_opt)
                } else {
                    lobby_feed_suspended(s_opt)
                };
                out.push(if r { 1.0 } else { 0.0 });
            }
        }
        3 | 5 => {
            let n = args[0] as usize;
            let mut i = 1usize;
            out.push(n as f64);
            for _ in 0..n {
                let u = read_val(args, &mut i);
                let s = read_val(args, &mut i);
                let outage = if kind == 3 {
                    let v = args[i] != 0.0;
                    i += 1;
                    v
                } else {
                    false
                };
                let u_opt = if nullish(&u) { None } else { Some(&u) };
                let s_opt = if nullish(&s) { None } else { Some(&s) };
                let r = if kind == 3 {
                    should_block_multiplayer_action(u_opt, s_opt, outage)
                } else {
                    should_block_socket_sourced_action(u_opt, s_opt)
                };
                out.push(if r { 1.0 } else { 0.0 });
            }
        }
        6 => {
            let n = args[0] as usize;
            let mut i = 1usize;
            out.push(n as f64);
            for _ in 0..n {
                let lobby = read_val(args, &mut i);
                out.push(if join_is_gateable(&lobby) { 1.0 } else { 0.0 });
            }
        }
        7 => {
            let n = args[0] as usize;
            let mut i = 1usize;
            out.push(n as f64);
            for _ in 0..n {
                let lobby = read_val(args, &mut i);
                let u = read_val(args, &mut i);
                let s = read_val(args, &mut i);
                let u_opt = if nullish(&u) { None } else { Some(&u) };
                let s_opt = if nullish(&s) { None } else { Some(&s) };
                out.push(if should_block_join(&lobby, u_opt, s_opt) { 1.0 } else { 0.0 });
            }
        }
        8 => {
            let n = args[0] as usize;
            let mut i = 1usize;
            out.push(n as f64);
            for _ in 0..n {
                let k = read_val(args, &mut i);
                let kind_s = match k {
                    JsVal::Str(ref s) => Some(s.as_str()),
                    // undefined / absent → the `case undefined` arm; a
                    // present null is NOT undefined → the switch default.
                    JsVal::Absent | JsVal::Undef => None,
                    _ => Some("\u{0}nonstring"),
                };
                out.push(if failed_allows_multiplayer(kind_s) { 1.0 } else { 0.0 });
            }
        }
        k => unreachable!("game_mode_gate: unknown op kind {k}"),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(fields: Vec<(&str, JsVal)>) -> JsVal {
        JsVal::Obj(fields.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }
    fn st(status: &str) -> JsVal {
        obj(vec![("status", JsVal::Str(status.into()))])
    }
    fn st_err(status: &str, kind: JsVal) -> JsVal {
        obj(vec![
            ("status", JsVal::Str(status.into())),
            ("error", obj(vec![("kind", kind)])),
        ])
    }

    #[test]
    fn update_allow_list() {
        assert!(multiplayer_allowed(&st("current")));
        assert!(multiplayer_allowed(&st("checking")));
        assert!(multiplayer_allowed(&st("blocked")));
        assert!(!multiplayer_allowed(&st("downloading")));
        assert!(!multiplayer_allowed(&st("staged")));
        assert!(!multiplayer_allowed(&st("bogus-status")));
        // failed + error kinds.
        assert!(multiplayer_allowed(&st("failed"))); // no error → undefined
        assert!(multiplayer_allowed(&st_err("failed", JsVal::Str("refused".into()))));
        assert!(multiplayer_allowed(&st_err("failed", JsVal::Str("parse".into()))));
        assert!(!multiplayer_allowed(&st_err("failed", JsVal::Str("network".into()))));
        assert!(!multiplayer_allowed(&st_err("failed", JsVal::Str("verify".into()))));
        assert!(!multiplayer_allowed(&st_err("failed", JsVal::Str("future-kind".into()))));
        assert!(!multiplayer_allowed(&st_err("failed", JsVal::Num(3.0))));
        // error explicitly undefined → `?.` reads undefined → allowed.
        assert!(multiplayer_allowed(&obj(vec![
            ("status", JsVal::Str("failed".into())),
            ("error", JsVal::Undef),
        ])));
    }

    #[test]
    fn session_and_composition() {
        assert!(multiplayer_allowed_for_session(None));
        assert!(multiplayer_allowed_for_session(Some(&st("unknown"))));
        assert!(multiplayer_allowed_for_session(Some(&st("signed-in"))));
        assert!(!multiplayer_allowed_for_session(Some(&st("retrying"))));
        assert!(!multiplayer_allowed_for_session(Some(&st("signed-out"))));
        assert!(!lobby_feed_suspended(None));
        assert!(lobby_feed_suspended(Some(&st("signed-out"))));
        assert!(!should_block_multiplayer_action(None, None, false));
        assert!(should_block_multiplayer_action(None, None, true));
        assert!(should_block_multiplayer_action(Some(&st("staged")), None, false));
        assert!(should_block_multiplayer_action(None, Some(&st("retrying")), false));
        assert!(!should_block_socket_sourced_action(None, None));
        assert!(should_block_socket_sourced_action(Some(&st("staged")), None));
    }

    #[test]
    fn join_gate() {
        let public = obj(vec![(
            "gameStartInfo",
            obj(vec![("config", obj(vec![("gameType", JsVal::Str("public".into()))]))]),
        )]);
        assert!(join_is_gateable(&public));
        let solo = obj(vec![(
            "gameStartInfo",
            obj(vec![("config", obj(vec![("gameType", JsVal::Str("Singleplayer".into()))]))]),
        )]);
        assert!(!join_is_gateable(&solo));
        // A replay (gameRecord present) is never gateable.
        let mut replay = solo.clone();
        if let JsVal::Obj(f) = &mut replay {
            f.push(("gameRecord".to_string(), JsVal::Obj(vec![])));
        }
        assert!(!join_is_gateable(&replay));
        let mut replay_public = public.clone();
        if let JsVal::Obj(f) = &mut replay_public {
            f.push(("gameRecord".to_string(), JsVal::Null)); // present-null ≠ undefined
        }
        assert!(!join_is_gateable(&replay_public));
        // gameStartInfo absent: undefined !== "Singleplayer" → gateable.
        assert!(join_is_gateable(&obj(vec![])));
        assert!(!should_block_join(&solo, Some(&st("staged")), None));
        assert!(should_block_join(&public, Some(&st("staged")), None));
        assert!(!should_block_join(&public, None, None));
    }

    #[test]
    fn failed_kind_direct() {
        assert!(failed_allows_multiplayer(None));
        assert!(failed_allows_multiplayer(Some("refused")));
        assert!(!failed_allows_multiplayer(Some("network")));
        assert!(!failed_allows_multiplayer(Some("unknown-kind")));
    }
}
