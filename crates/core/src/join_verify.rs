//! Port of the PURE decision functions of `src/server/JoinVerify.ts`:
//! `isSteamAuthenticated(claims)` and `planJoinVerify(args)`. `verifyJoin`
//! is fetch I/O and EXCLUDED (never ported); the zod verdict schema and
//! `ServerEnv` ride only as erased imports in the capture.
//!
//! Faithfulness notes:
//!
//! * `isSteamAuthenticated`: `claims?.provider === "steam"` — `null` /
//!   `undefined` claims short-circuit the optional chain to `undefined`,
//!   which is `!== "steam"` -> false. Only the `provider` field is read;
//!   claims cross the harness as a codec value (a plain object).
//! * `planJoinVerify` decision order (first join, `!isReadmit`): steam ->
//!   `{verify, token: null}`; else `!args.turnstileToken` — a FALSY test, so
//!   the empty string `""` rejects exactly like `null` (the SECURITY
//!   comment: never forward a first join with no token); else `{verify,
//!   token: turnstileToken}`. Re-admit: `gameStarted || identityUnchanged`
//!   -> `{skip}`; else `{verify, token: null}`.
//! * The returned plan objects are TS object LITERALS: `{action:"reject"}`
//!   and `{action:"skip"}` have NO `token` key at all (Absent, not
//!   present-`undefined`), while `{action:"verify", token: ...}` always
//!   carries the key (its value may be JS `null`). Key order is
//!   action-first, token-second, pinned by the codec dump.

use crate::js_json::{push_val, read_val, val_field, JsVal};

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

/// `claims?.provider === "steam"` — claims is a codec value (object, or
/// `null` / `undefined`).
pub fn is_steam_authenticated(claims: &JsVal) -> bool {
    if !matches!(claims, JsVal::Obj(_)) {
        return false;
    }
    matches!(val_field(claims, "provider"), Some(JsVal::Str(p)) if p == "steam")
}

/// The `JoinVerifyPlan` for `args` (a codec object with the five booleans /
/// token), returned as the exact JS object shape.
pub fn plan_join_verify(args: &JsVal) -> JsVal {
    let flag = |k: &str| -> bool {
        matches!(val_field(args, k), Some(v) if truthy(v))
    };
    if !flag("isReadmit") {
        if flag("steamAuthed") {
            return JsVal::Obj(vec![("action".to_string(), s("verify")), ("token".to_string(), JsVal::Null)]);
        }
        let token = val_field(args, "turnstileToken");
        if !token.is_some_and(truthy) {
            return JsVal::Obj(vec![("action".to_string(), s("reject"))]);
        }
        return JsVal::Obj(vec![
            ("action".to_string(), s("verify")),
            ("token".to_string(), token.expect("truthy token").clone()),
        ]);
    }
    if flag("gameStarted") || flag("identityUnchanged") {
        return JsVal::Obj(vec![("action".to_string(), s("skip"))]);
    }
    JsVal::Obj(vec![("action".to_string(), s("verify")), ("token".to_string(), JsVal::Null)])
}

/// The capture harness: pure functions over codec args, no state.
#[derive(Debug, Default)]
pub struct RigHarness {}

impl RigHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {}

    /// Run one op. Kind table (see `tools/gen_vectors.mjs`):
    /// 0 construct/reset -> `[0]`;
    /// 1 isSteamAuthenticated `[claims]` -> `[0|1]`;
    /// 2 planJoinVerify `[args]` -> codec value (the plan object).
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut i = 0usize;
        match kind {
            0 => {
                self.reset();
                vec![0.0]
            }
            1 => {
                let claims = read_val(args, &mut i);
                vec![if is_steam_authenticated(&claims) { 1.0 } else { 0.0 }]
            }
            2 => {
                let a = read_val(args, &mut i);
                let mut out = Vec::new();
                push_val(&mut out, &plan_join_verify(&a));
                out
            }
            k => unreachable!("join_verify harness: unknown op kind {k}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bv(x: bool) -> JsVal {
        JsVal::Bool(x)
    }

    fn args(is_readmit: bool, started: bool, token: JsVal, unchanged: bool, steam: bool) -> JsVal {
        JsVal::Obj(vec![
            ("isReadmit".to_string(), bv(is_readmit)),
            ("gameStarted".to_string(), bv(started)),
            ("turnstileToken".to_string(), token),
            ("identityUnchanged".to_string(), bv(unchanged)),
            ("steamAuthed".to_string(), bv(steam)),
        ])
    }

    #[test]
    fn steam_claims() {
        assert!(is_steam_authenticated(&JsVal::Obj(vec![("provider".to_string(), s("steam"))])));
        assert!(!is_steam_authenticated(&JsVal::Obj(vec![("provider".to_string(), s("google"))])));
        assert!(!is_steam_authenticated(&JsVal::Obj(vec![])));
        assert!(!is_steam_authenticated(&JsVal::Null));
        assert!(!is_steam_authenticated(&JsVal::Undef));
    }

    #[test]
    fn first_join_matrix() {
        // steam -> verify, token null (key PRESENT as null).
        let p = plan_join_verify(&args(false, false, JsVal::Null, false, true));
        match &p {
            JsVal::Obj(f) => {
                assert_eq!(f.len(), 2);
                assert_eq!(f[0].0, "action");
                assert_eq!(f[1].0, "token");
                assert_eq!(f[1].1, JsVal::Null);
            }
            _ => panic!(),
        }
        // no token -> reject: NO token key at all (Absent).
        let r = plan_join_verify(&args(false, false, JsVal::Null, false, false));
        assert!(matches!(&r, JsVal::Obj(f) if f.len() == 1 && f[0].1 == s("reject")));
        // EMPTY STRING token is falsy -> reject too.
        let e = plan_join_verify(&args(false, false, s(""), false, false));
        assert!(matches!(&e, JsVal::Obj(f) if f.len() == 1 && f[0].1 == s("reject")));
        // real token -> verify with it.
        let v = plan_join_verify(&args(false, false, s("tok"), false, false));
        assert!(matches!(&v, JsVal::Obj(f) if f.len() == 2 && f[1].1 == s("tok")));
    }

    #[test]
    fn readmit_matrix() {
        assert!(matches!(&plan_join_verify(&args(true, true, s("t"), false, false)),
            JsVal::Obj(f) if f.len() == 1 && f[0].1 == s("skip")));
        assert!(matches!(&plan_join_verify(&args(true, false, s("t"), true, false)),
            JsVal::Obj(f) if f.len() == 1 && f[0].1 == s("skip")));
        let v = plan_join_verify(&args(true, false, JsVal::Null, false, false));
        assert!(matches!(&v, JsVal::Obj(f) if f.len() == 2 && f[1].1 == JsVal::Null));
    }

    #[test]
    fn harness_codec_roundtrip() {
        let mut h = RigHarness::new();
        assert_eq!(h.run_op(0, &[]), vec![0.0]);
        let mut a = Vec::new();
        push_val(&mut a, &JsVal::Obj(vec![("provider".to_string(), s("steam"))]));
        assert_eq!(h.run_op(1, &a), vec![1.0]);
        let mut b = Vec::new();
        push_val(&mut b, &args(false, false, s("x"), false, false));
        let out = h.run_op(2, &b);
        let mut j = 0usize;
        let plan = read_val(&out, &mut j);
        match plan {
            JsVal::Obj(f) => {
                assert_eq!(f[0].1, s("verify"));
                assert_eq!(f[1].1, s("x"));
            }
            _ => panic!(),
        }
    }
}
