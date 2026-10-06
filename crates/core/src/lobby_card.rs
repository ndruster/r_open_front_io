//! Port of the pure predicate subset of `src/client/components/LobbyCard.ts`
//! — `viewerIsTrusted`, `canJoinTrustedLobby` and `viewerIsSignedIn` (the
//! `trustRequiredDialog` / `MapAspectRatioCache` half is lit / fetch
//! host-bound and stays out of scope).
//!
//! Faithfulness notes (quirk list):
//!
//! * `viewerIsTrusted(userMe)`: `userMe !== false && userMe.player.trustTier
//!   === "trusted"`. The first gate is a STRICT `!== false` — `undefined` /
//!   `null` / any object pass it, and the property access then throws a
//!   `TypeError` for `undefined` / `null` (the harness models the throw as
//!   status 1). A present object without a `player` key reads `undefined`
//!   and `.trustTier` throws too (status 1). `trustTier` missing on the
//!   player reads `undefined` -> `=== "trusted"` false (status 0, result 0).
//! * `canJoinTrustedLobby(lobby, viewerTrusted)`: `lobby.gameConfig?.trusted
//!   !== true || viewerTrusted`. The optional chain protects only
//!   `gameConfig` itself (a nullish `lobby` throws — out of the capture
//!   domain, objects only); `undefined` / `null` / `false` / any non-`true`
//!   value passes the first gate, so ONLY a literal `true` defers to
//!   `viewerTrusted`.
//! * `viewerIsSignedIn(userMe)` delegates to the ported
//!   `account_identity::response_has_linked_identity` — one door, one lock.

use crate::account_identity::response_has_linked_identity;
use crate::js_json::{read_val, val_field, JsVal};

/// `viewerIsTrusted(userMe)` — `Err(())` models the TypeError the TS reads
/// raise on `undefined` / `null` / a player-less response.
#[allow(clippy::result_unit_err)]
pub fn viewer_is_trusted(user_me: &JsVal) -> Result<bool, ()> {
    if matches!(user_me, JsVal::Bool(false)) {
        return Ok(false);
    }
    if matches!(user_me, JsVal::Absent | JsVal::Undef | JsVal::Null) {
        return Err(());
    }
    // `userMe.player`: a missing key / nullish reads throw on the
    // `.trustTier` access (status 1); a primitive player (string / number /
    // bool) boxes and reads `undefined` for `trustTier` -> not "trusted"
    // (Ok(false)); only an object player can carry the tier.
    let player = match val_field(user_me, "player") {
        Some(p @ JsVal::Obj(_)) => p,
        Some(JsVal::Absent) | Some(JsVal::Undef) | Some(JsVal::Null) | None => {
            return Err(())
        }
        Some(_) => return Ok(false),
    };
    Ok(matches!(val_field(player, "trustTier"), Some(JsVal::Str(s)) if s == "trusted"))
}

/// `canJoinTrustedLobby(lobby, viewerTrusted)`.
pub fn can_join_trusted_lobby(lobby: &JsVal, viewer_trusted: bool) -> bool {
    let trusted = val_field(lobby, "gameConfig")
        .and_then(|gc| val_field(gc, "trusted"));
    !matches!(trusted, Some(JsVal::Bool(true))) || viewer_trusted
}

/// `viewerIsSignedIn(userMe)` — the delegation.
pub fn viewer_is_signed_in(user_me: &JsVal) -> bool {
    response_has_linked_identity(user_me)
}

// ---------------------------------------------------------------- vectors op
//
// kind 0: [n, (encVal userMe)*n] -> (status, bool?)*n
//         viewerIsTrusted batch; status 0 ok (bool follows), 1 TypeError.
// kind 1: [n, (encVal lobby, bool viewerTrusted)*n] -> (0|1)*n
//         canJoinTrustedLobby batch.
// kind 2: [n, (encVal userMe)*n] -> (0|1)*n
//         viewerIsSignedIn batch (the account_identity delegation).

pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    let mut i = 0usize;
    match kind {
        0 => {
            let n = args[i] as usize;
            i += 1;
            for _ in 0..n {
                let v = read_val(args, &mut i);
                match viewer_is_trusted(&v) {
                    Ok(b) => {
                        out.push(0.0);
                        out.push(if b { 1.0 } else { 0.0 });
                    }
                    Err(()) => out.push(1.0),
                }
            }
        }
        1 => {
            let n = args[i] as usize;
            i += 1;
            for _ in 0..n {
                let lobby = read_val(args, &mut i);
                let vt = args[i] != 0.0;
                i += 1;
                out.push(if can_join_trusted_lobby(&lobby, vt) {
                    1.0
                } else {
                    0.0
                });
            }
        }
        2 => {
            let n = args[i] as usize;
            i += 1;
            for _ in 0..n {
                let v = read_val(args, &mut i);
                out.push(if viewer_is_signed_in(&v) { 1.0 } else { 0.0 });
            }
        }
        k => unreachable!("lobby_card: unknown op kind {k}"),
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
    fn trusted_strict_false_short_circuit() {
        assert_eq!(viewer_is_trusted(&JsVal::Bool(false)), Ok(false));
        // false && short-circuits: even a would-be-throwing value is inert.
        // (The TS `&&` never evaluates `userMe.player` for `false`.)
    }

    #[test]
    fn trusted_object_paths() {
        let trusted = obj(vec![(
            "player",
            obj(vec![("trustTier", JsVal::Str("trusted".into()))]),
        )]);
        assert_eq!(viewer_is_trusted(&trusted), Ok(true));
        let untrusted = obj(vec![(
            "player",
            obj(vec![("trustTier", JsVal::Str("untrusted".into()))]),
        )]);
        assert_eq!(viewer_is_trusted(&untrusted), Ok(false));
        // trustTier missing -> undefined !== "trusted".
        assert_eq!(viewer_is_trusted(&obj(vec![("player", obj(vec![]))])), Ok(false));
        // trustTier null -> not the string.
        let null_tier = obj(vec![("player", obj(vec![("trustTier", JsVal::Null)]))]);
        assert_eq!(viewer_is_trusted(&null_tier), Ok(false));
    }

    #[test]
    fn trusted_type_error_domain() {
        assert_eq!(viewer_is_trusted(&JsVal::Undef), Err(()));
        assert_eq!(viewer_is_trusted(&JsVal::Null), Err(()));
        // player missing / null -> the `.trustTier` read throws.
        assert_eq!(viewer_is_trusted(&obj(vec![])), Err(()));
        assert_eq!(
            viewer_is_trusted(&obj(vec![("player", JsVal::Null)])),
            Err(())
        );
    }

    #[test]
    fn trusted_only_literal_true_defers() {
        let t = obj(vec![("gameConfig", obj(vec![("trusted", JsVal::Bool(true))]))]);
        assert!(!can_join_trusted_lobby(&t, false));
        assert!(can_join_trusted_lobby(&t, true));
        // Every non-true value passes the first gate.
        for gc in [
            None,
            Some(JsVal::Undef),
            Some(JsVal::Null),
            Some(JsVal::Bool(false)),
            Some(JsVal::Str("true".into())),
        ] {
            let mut fields = vec![];
            let dbg = format!("{gc:?}");
            if let Some(v) = &gc {
                fields.push(("gameConfig", v.clone()));
            }
            let lobby = obj(fields);
            assert!(can_join_trusted_lobby(&lobby, false), "gc {dbg}");
        }
        // gameConfig present but trusted absent -> undefined !== true.
        let lobby = obj(vec![("gameConfig", obj(vec![]))]);
        assert!(can_join_trusted_lobby(&lobby, false));
    }

    #[test]
    fn signed_in_delegates_to_account_identity() {
        assert!(!viewer_is_signed_in(&JsVal::Bool(false)));
        let resp = obj(vec![(
            "user",
            obj(vec![("steam", JsVal::Str("s".into()))]),
        )]);
        assert!(viewer_is_signed_in(&resp));
        assert!(!viewer_is_signed_in(&obj(vec![])));
    }

    #[test]
    fn run_op_wire_roundtrip() {
        // kind 0: [false, {player:{trustTier:"trusted"}}] -> [0,0, 0,1].
        // encVal(false) = [4,0]; encVal(obj) = [6,1, encS("player"),
        // [6,1, encS("trustTier"), [5, ...encS("trusted")]]].
        let r = run_op(
            0,
            &[
                2.0,
                4.0,
                0.0,
                6.0,
                1.0,
                6.0,
                112.0,
                108.0,
                97.0,
                121.0,
                101.0,
                114.0,
                6.0,
                1.0,
                9.0,
                116.0,
                114.0,
                117.0,
                115.0,
                116.0,
                84.0,
                105.0,
                101.0,
                114.0,
                5.0,
                7.0,
                116.0,
                114.0,
                117.0,
                115.0,
                116.0,
                101.0,
                100.0,
            ],
        );
        assert_eq!(r, [0.0, 0.0, 0.0, 1.0]);
    }
}
