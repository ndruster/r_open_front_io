//! Port of `src/client/AccountIdentity.ts` — the three linked-identity
//! predicates over a `/users/@me` user object.
//!
//! Faithfulness notes (quirk list):
//!
//! * `isSteamPrimaryUser` guards with `user?.steam` (optional chain: an
//!   `undefined` user reads `undefined` → falsy) then checks the other three
//!   fields with `!` TRUTHINESS — an empty-string email or a `false` flag
//!   counts as "no identity", a present object/string counts as one.
//! * `hasLinkedIdentity` uses `!== undefined` PRESENCE checks (a present
//!   `null` discord counts as linked!), in the source order discord,
//!   google, steam, then `(email ?? "") !== ""` — the `??` falls through
//!   null/undefined/absent to `""`, and any non-string email value is
//!   strictly `!== ""` so it counts as linked.
//! * `responseHasLinkedIdentity` gates `!== false` (STRICT) before unwrapping
//!   `.user`; a non-object response's absent `user` reads `undefined` →
//!   `hasLinkedIdentity(undefined)` → false, no throw modelled (the capture
//!   stays inside the typed domain `UserMeResponse | false`).
//!
//! Users cross as [`JsVal`] codec values: `Obj` fields carry the
//! absent-vs-`undefined` distinction (an omitted key reads `undefined`).

use crate::js_json::{read_val, val_field, JsVal};

/// JS truthiness restricted to the codec domain.
fn truthy(v: &JsVal) -> bool {
    match v {
        JsVal::Absent | JsVal::Undef | JsVal::Null => false,
        JsVal::Num(n) => !n.is_nan() && *n != 0.0,
        JsVal::Bool(b) => *b,
        JsVal::Str(s) => !s.is_empty(),
        JsVal::Obj(_) | JsVal::Arr(_) => true,
    }
}

/// `obj[key]` truthiness: an absent field reads `undefined` → falsy.
fn field_truthy(obj: &JsVal, key: &str) -> bool {
    val_field(obj, key).map(truthy).unwrap_or(false)
}

/// `isSteamPrimaryUser(user)` — `!!user?.steam && !user.discord &&
/// !user.google && !user.email`.
pub fn is_steam_primary_user(user: &JsVal) -> bool {
    if !field_truthy(user, "steam") {
        return false;
    }
    !field_truthy(user, "discord")
        && !field_truthy(user, "google")
        && !field_truthy(user, "email")
}

/// `x !== undefined` on a field read: absent and present-`undefined` are
/// both `undefined`; present `null` is NOT.
fn ne_undefined(v: Option<&JsVal>) -> bool {
    !matches!(v, None | Some(JsVal::Absent) | Some(JsVal::Undef))
}

/// `hasLinkedIdentity(user)` — explicit presence checks, order discord,
/// google, steam, email. `undefined` user → false.
pub fn has_linked_identity(user: &JsVal) -> bool {
    if matches!(user, JsVal::Absent | JsVal::Undef) {
        return false;
    }
    ne_undefined(val_field(user, "discord"))
        || ne_undefined(val_field(user, "google"))
        || ne_undefined(val_field(user, "steam"))
        || match val_field(user, "email") {
            None | Some(JsVal::Absent | JsVal::Undef | JsVal::Null) => false,
            Some(JsVal::Str(s)) => !s.is_empty(),
            Some(_) => true,
        }
}

/// `responseHasLinkedIdentity(userMeResponse)` — `!== false` then unwrap
/// `.user`.
pub fn response_has_linked_identity(response: &JsVal) -> bool {
    if matches!(response, JsVal::Bool(false)) {
        return false;
    }
    has_linked_identity(val_field(response, "user").unwrap_or(&JsVal::Undef))
}

// ---------------------------------------------------------------- vectors op
//
// kind 0: [n, (enc user)*n] -> [n, (0/1)*n]     isSteamPrimaryUser batch
// kind 1: [n, (enc user)*n] -> [n, (0/1)*n]     hasLinkedIdentity batch
// kind 2: [n, (enc resp)*n] -> [n, (0/1)*n]     responseHasLinkedIdentity batch
//   enc user: codec value — [6,n,(k,v)*n] object (fields absent = key
//   omitted, present-undefined = [1], null = [2], string = [5,...]) or [1]
//   for an undefined user.

/// Flat-token runner shared by the golden replay and the wasm probe.
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    match kind {
        0..=2 => {
            let n = args[0] as usize;
            let mut i = 1usize;
            out.push(n as f64);
            for _ in 0..n {
                let v = read_val(args, &mut i);
                let r = match kind {
                    0 => is_steam_primary_user(&v),
                    1 => has_linked_identity(&v),
                    _ => response_has_linked_identity(&v),
                };
                out.push(if r { 1.0 } else { 0.0 });
            }
        }
        k => unreachable!("account_identity: unknown op kind {k}"),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(fields: &[(&str, JsVal)]) -> JsVal {
        JsVal::Obj(
            fields
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        )
    }

    #[test]
    fn steam_primary_gates() {
        let steam_only = obj(&[("steam", JsVal::Str("7656".into()))]);
        assert!(is_steam_primary_user(&steam_only));
        // Empty-string email is falsy → still Steam-primary.
        let with_empty_email =
            obj(&[("steam", JsVal::Str("s".into())), ("email", JsVal::Str(String::new()))]);
        assert!(is_steam_primary_user(&with_empty_email));
        // Non-empty email breaks it.
        let with_email = obj(&[
            ("steam", JsVal::Str("s".into())),
            ("email", JsVal::Str("a@b.c".into())),
        ]);
        assert!(!is_steam_primary_user(&with_email));
        // steam = "" is falsy.
        let empty_steam = obj(&[("steam", JsVal::Str(String::new()))]);
        assert!(!is_steam_primary_user(&empty_steam));
        // undefined user short-circuits the optional chain.
        assert!(!is_steam_primary_user(&JsVal::Undef));
    }

    #[test]
    fn linked_identity_presence_semantics() {
        // Present-but-null discord counts as linked (!== undefined).
        let null_discord = obj(&[("discord", JsVal::Null)]);
        assert!(has_linked_identity(&null_discord));
        // Present-undefined does NOT.
        let undef_google = obj(&[("google", JsVal::Undef)]);
        assert!(!has_linked_identity(&undef_google));
        // Empty-string email is NOT linked; any non-string email IS.
        let empty_email = obj(&[("email", JsVal::Str(String::new()))]);
        assert!(!has_linked_identity(&empty_email));
        let num_email = obj(&[("email", JsVal::Num(0.0))]);
        assert!(has_linked_identity(&num_email));
        assert!(!has_linked_identity(&JsVal::Undef));
        // Response false gate.
        assert!(!response_has_linked_identity(&JsVal::Bool(false)));
        let resp = obj(&[("user", obj(&[("steam", JsVal::Str("s".into()))]))]);
        assert!(response_has_linked_identity(&resp));
        // Response without a user key → undefined → false.
        assert!(!response_has_linked_identity(&obj(&[])));
    }
}
