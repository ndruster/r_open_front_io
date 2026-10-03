//! Port of `src/server/IntentAuthorization.ts`: `authorizeIntent(intent,
//! actor, game)` — the pure actor / game-state guard table of
//! `GameServer.handleIntent`. `intent`, `actor` and `game` ride as plain JS
//! data (no zod), so the harness reads them through the `js_json` codec
//! ([`crate::js_json::JsVal`]).
//!
//! Faithfulness notes:
//!
//! * The admin-bot / public guard runs BEFORE the switch, so it wins over
//!   every intent type (`ia_adminbot_public_*`).
//! * Guards fire in source order; the first failing one is the outcome
//!   (pinned by the `ia_ug_*` ordering scenarios).
//! * `intent.config.gameType === GameType.Public`: `GameType` is the string
//!   enum already ported in `game_ts` (`Public` = `"Public"`), so the port
//!   compares the config field against the literal string. An absent field
//!   reads `undefined` and never matches.
//! * `hostCheatsEnabled(intent.config.hostCheats)` is the `config_patch`
//!   predicate (absent field = `undefined` = false).
//! * `intent.config.allowedPublicIds?.length ?? 0`: an absent, `undefined`
//!   or `null` field yields 0; an array yields its length. The `> 0` gate is
//!   only reached when `game.isListed` (JS `&&` short-circuit).
//! * Outcome encoding on the wire: `[0]` = `null` (intent allowed),
//!   `[1, status, 0|1, (error-str)?]` — the `error` key is present only on
//!   the rejected branches (every TS branch that returns a status also
//!   returns an error string, so `hasError` is always 1 in the capture).

use crate::config_patch::host_cheats_enabled;
use crate::js_json::{read_str, read_val, val_field, JsVal};

/// `IntentOutcome` — `None` models the JS `null` ("intent may go ahead").
#[derive(Clone, Debug, PartialEq)]
pub struct IntentOutcome {
    pub status: f64,
    pub error: String,
}

/// The actor / game guards read as plain booleans + the intent switch.
#[derive(Clone, Debug)]
pub struct IntentActor {
    pub is_lobby_creator: bool,
    pub is_admin: bool,
    pub is_admin_bot: bool,
}

#[derive(Clone, Debug)]
pub struct IntentGameState {
    pub is_public: bool,
    pub is_listed: bool,
    pub has_started: bool,
}

/// `authorizeIntent(intent, actor, game)`. `intent_type` is the `intent.type`
/// string; `config` is `intent.config` (only read by the
/// `update_game_config` branch).
pub fn authorize_intent(
    intent_type: &str,
    config: Option<&JsVal>,
    actor: &IntentActor,
    game: &IntentGameState,
) -> Option<IntentOutcome> {
    let out = |status: f64, error: &str| {
        Some(IntentOutcome { status, error: error.to_string() })
    };
    // The admin bot only manages private games.
    if actor.is_admin_bot && game.is_public {
        return out(403.0, "admin bot cannot act on public games");
    }
    match intent_type {
        "mark_disconnected" => out(400.0, "mark_disconnected is server-internal"),
        "kick_player" => {
            if !actor.is_lobby_creator && !actor.is_admin {
                return out(403.0, "only the lobby creator or an admin can kick players");
            }
            if game.is_listed && !actor.is_admin {
                return out(
                    403.0,
                    "the host cannot kick players in a publicly listed lobby",
                );
            }
            None
        }
        "update_game_config" => {
            if !actor.is_lobby_creator && !actor.is_admin_bot {
                return out(403.0, "only the lobby creator can update game config");
            }
            if game.is_public {
                return out(403.0, "cannot update a public game");
            }
            if game.has_started {
                return out(409.0, "game already started");
            }
            let cfg = config.unwrap_or(&JsVal::Undef);
            let game_type = val_field(cfg, "gameType");
            if game_type == Some(&JsVal::Str("Public".to_string())) {
                return out(400.0, "cannot change a game to public");
            }
            if game.is_listed
                && host_cheats_enabled(val_field(cfg, "hostCheats"))
            {
                return out(
                    409.0,
                    "cannot enable host cheats in a publicly listed lobby",
                );
            }
            if game.is_listed && allowed_len(val_field(cfg, "allowedPublicIds")) > 0 {
                return out(
                    409.0,
                    "cannot enable a join whitelist in a publicly listed lobby",
                );
            }
            None
        }
        "toggle_game_start_timer" => {
            if !actor.is_lobby_creator && !actor.is_admin_bot {
                return out(403.0, "only the lobby creator can start");
            }
            if game.is_public {
                return out(403.0, "cannot start a public game");
            }
            if game.has_started {
                return out(409.0, "game already started");
            }
            None
        }
        "toggle_pause" => {
            if !actor.is_lobby_creator && !actor.is_admin_bot {
                return out(403.0, "only the lobby creator can pause");
            }
            if game.is_listed && !actor.is_admin_bot {
                return out(403.0, "the host cannot pause a publicly listed game");
            }
            if !game.has_started {
                return out(409.0, "game not started");
            }
            None
        }
        _ => {
            // Gameplay intents: websocket players only.
            if actor.is_admin_bot {
                return out(400.0, "intent not permitted for admin bot");
            }
            None
        }
    }
}

/// `intent.config.allowedPublicIds?.length ?? 0` — absent / undefined / null
/// read 0; an array its length; anything else has no `length` (undefined ->
/// 0 through `??`).
fn allowed_len(v: Option<&JsVal>) -> usize {
    match v {
        Some(JsVal::Arr(items)) => items.len(),
        _ => 0,
    }
}

/// The capture harness (stateless function, but the op-stream shape matches
/// the other server-cluster ports: kind 0 resets, kind 1 calls).
#[derive(Debug, Default)]
pub struct RigHarness;

impl RigHarness {
    pub fn new() -> Self {
        RigHarness
    }

    pub fn reset(&mut self) {}

    /// Run one op. Kind table (see `tools/gen_vectors.mjs`):
    /// 0 construct -> `[0]`; 1 authorizeIntent `[n,(key,value)*n]` where the
    /// map fields are `type` (string), `config` (object), `isLobbyCreator` /
    /// `isAdmin` / `isAdminBot` / `isPublic` / `isListed` / `hasStarted`
    /// (bools) -> outcome `[0]` null | `[1,status,1,(error-str)]`.
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut i = 0usize;
        match kind {
            0 => vec![0.0],
            1 => {
                let n = args[i] as usize;
                i += 1;
                let mut fields: Vec<(String, JsVal)> = Vec::with_capacity(n);
                for _ in 0..n {
                    let k = read_str(args, &mut i);
                    let v = read_val(args, &mut i);
                    fields.push((k, v));
                }
                let get = |k: &str| fields.iter().find(|(x, _)| x == k).map(|(_, v)| v);
                let flag = |k: &str| matches!(get(k), Some(JsVal::Bool(true)));
                let type_str = match get("type") {
                    Some(JsVal::Str(s)) => s.clone(),
                    other => json_of(other),
                };
                let config = get("config");
                let actor = IntentActor {
                    is_lobby_creator: flag("isLobbyCreator"),
                    is_admin: flag("isAdmin"),
                    is_admin_bot: flag("isAdminBot"),
                };
                let game = IntentGameState {
                    is_public: flag("isPublic"),
                    is_listed: flag("isListed"),
                    has_started: flag("hasStarted"),
                };
                match authorize_intent(&type_str, config, &actor, &game) {
                    None => vec![0.0],
                    Some(o) => {
                        let mut out = vec![1.0, o.status, 1.0];
                        crate::js_json::push_str(&mut out, &o.error);
                        out
                    }
                }
            }
            k => unreachable!("intent_authorization harness: unknown op kind {k}"),
        }
    }
}

/// A non-string `intent.type` would not match any case (JS `switch` uses
/// `===`); encode its stringify form for the default branch. Only reachable
/// if the capture feeds a non-string type; the capture always feeds strings.
fn json_of(v: Option<&JsVal>) -> String {
    match v {
        Some(JsVal::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(c: bool, a: bool, b: bool) -> IntentActor {
        IntentActor { is_lobby_creator: c, is_admin: a, is_admin_bot: b }
    }
    fn game(p: bool, l: bool, st: bool) -> IntentGameState {
        IntentGameState { is_public: p, is_listed: l, has_started: st }
    }
    fn st(s: &str) -> JsVal {
        JsVal::Str(s.to_string())
    }

    #[test]
    fn adminbot_public_short_circuits_everything() {
        let r = authorize_intent("kick_player", None, &actor(false, false, true), &game(true, false, false));
        assert_eq!(r.unwrap().status, 403.0);
    }

    #[test]
    fn kick_branches() {
        assert_eq!(
            authorize_intent("kick_player", None, &actor(false, false, false), &game(false, false, false))
                .unwrap()
                .status,
            403.0
        );
        assert!(authorize_intent("kick_player", None, &actor(true, false, false), &game(false, false, false)).is_none());
        assert!(authorize_intent("kick_player", None, &actor(false, true, false), &game(false, true, false)).is_none());
        assert_eq!(
            authorize_intent("kick_player", None, &actor(true, false, false), &game(false, true, false))
                .unwrap()
                .status,
            403.0
        );
    }

    #[test]
    fn update_config_guard_order() {
        // creator + public -> 403 "cannot update a public game" (not the
        // creator gate).
        let cfg = JsVal::Obj(vec![("gameType".to_string(), st("Private"))]);
        let r = authorize_intent(
            "update_game_config",
            Some(&cfg),
            &actor(true, false, false),
            &game(true, false, false),
        )
        .unwrap();
        assert_eq!((r.status, r.error.as_str()), (403.0, "cannot update a public game"));
        // started -> 409 before the gameType check.
        let r = authorize_intent(
            "update_game_config",
            Some(&JsVal::Obj(vec![("gameType".to_string(), st("Public"))])),
            &actor(true, false, false),
            &game(false, false, true),
        )
        .unwrap();
        assert_eq!(r.status, 409.0);
        // Public gameType -> 400.
        let r = authorize_intent(
            "update_game_config",
            Some(&JsVal::Obj(vec![("gameType".to_string(), st("Public"))])),
            &actor(true, false, false),
            &game(false, false, false),
        )
        .unwrap();
        assert_eq!(r.status, 400.0);
        // listed + cheats -> 409; listed + whitelist -> 409; whitelist length
        // via `?.length ?? 0`.
        let hc = JsVal::Obj(vec![("infiniteGold".to_string(), JsVal::Bool(true))]);
        let cfg = JsVal::Obj(vec![("hostCheats".to_string(), hc)]);
        let r = authorize_intent(
            "update_game_config",
            Some(&cfg),
            &actor(true, false, false),
            &game(false, true, false),
        )
        .unwrap();
        assert_eq!(r.status, 409.0);
        let ids = JsVal::Obj(vec![("allowedPublicIds".to_string(), JsVal::Arr(vec![st("a")]))]);
        let r = authorize_intent(
            "update_game_config",
            Some(&ids),
            &actor(true, false, false),
            &game(false, true, false),
        )
        .unwrap();
        assert_eq!(r.status, 409.0);
        // null allowedPublicIds -> length 0 -> allowed.
        let ids = JsVal::Obj(vec![("allowedPublicIds".to_string(), JsVal::Null)]);
        assert!(authorize_intent(
            "update_game_config",
            Some(&ids),
            &actor(true, false, false),
            &game(false, true, false),
        )
        .is_none());
    }

    #[test]
    fn pause_and_default() {
        // listed + creator(non-adminBot) -> 403 before the started check.
        let r = authorize_intent("toggle_pause", None, &actor(true, false, false), &game(false, true, true)).unwrap();
        assert_eq!(r.status, 403.0);
        // !started -> 409.
        let r = authorize_intent("toggle_pause", None, &actor(true, false, false), &game(false, false, false)).unwrap();
        assert_eq!(r.status, 409.0);
        // adminBot can pause a listed started game.
        assert!(authorize_intent("toggle_pause", None, &actor(false, false, true), &game(false, true, true)).is_none());
        // default gameplay intent.
        let r = authorize_intent("attack", None, &actor(false, false, true), &game(false, false, false)).unwrap();
        assert_eq!((r.status, r.error.as_str()), (400.0, "intent not permitted for admin bot"));
        assert!(authorize_intent("attack", None, &actor(false, false, false), &game(false, false, false)).is_none());
    }
}
