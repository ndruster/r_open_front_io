//! Port of the pure subset of `src/server/ClusterCheckin.ts`: the
//! `CHECKIN_INTERVAL_MS` constant, the `ServerStateSchema` z.enum literal
//! table, `isRefusal`, `registeredSite`, `checkinBody` and
//! `applyCheckinState`. `sendCheckin` (fetch / AbortSignal.timeout / zod
//! safeParse) and the two wire schemas are EXCLUDED.
//!
//! `ServerEnv` rides as a scripted black-box facade table (precedent: the
//! Censor matcher / MapPlaylist `__MP_LOG`): the capture rewrites the import
//! to `globalThis.__CK_ENV` and every method call pushes a trace event
//! `[72, method, ...codec(value)]` (method 0=siteHost, 1=publicHost,
//! 2=instanceLetter, 3=gitCommit, 4=numWorkers, 5=machine, 6=pageHostFor —
//! the 6-form carries `[72, 6, ...codec(host-arg), ...codec(value)]`). The
//! traced ops return `[traceLen,(trace)*,payload...]`, pinning the exact
//! read ORDER and the short-circuits. `setActive` is a callback facade
//! traced as `[73, bool]`.
//!
//! Faithfulness notes (quirk list):
//!
//! * `isRefusal(result)` = `typeof result === "object" && result !== null`.
//!   Codec domain: Str -> false (the `typeof "open"` short-circuit — a
//!   string state never reaches the null check), Null -> false, Obj -> true,
//!   Arr -> true (`typeof [] === "object"`), Undef/Absent -> false
//!   (`typeof undefined === "undefined"`).
//! * `registeredSite()` = `ServerEnv.siteHost() ?? ServerEnv.publicHost()`.
//!   The `??` fires ONLY for null / undefined; the TS domain is
//!   `string | undefined` (`siteHost()` returns `v && v.length > 0 ? v :
//!   undefined`, never null), so a string hit returns WITHOUT reading
//!   publicHost (pinned by the trace: zero `publicHost` events).
//! * `checkinBody(liveGames)`: evaluation order is publicHost (host), then
//!   machine, then the object literal's properties in SOURCE order — site
//!   (registeredSite: siteHost, then publicHost only when siteHost is
//!   undefined), letter, version, numWorkers. QUIRK: when siteHost is
//!   undefined this reads `publicHost` a SECOND time (inside registeredSite);
//!   the outer `?? host` then never fires because registeredSite already
//!   returned the same host string. The result object's key insertion order
//!   is site,letter,host,version,numWorkers,liveGames,(machine?) — the
//!   conditional spread adds `machine` LAST only when `machine !== undefined`
//!   (STRICT: a scripted null would PASS the gate and ride as `machine:
//!   null`; the real `machine()` returns undefined | trimmed string, so the
//!   capture only feeds those two).
//! * `host === undefined` uses STRICT `===`: a null host would pass the gate
//!   (again impossible in the real domain).
//! * `applyCheckinState(result, setActive)`: `result === null` is STRICT, so
//!   `undefined` (the codec Undef) FALLS THROUGH and calls
//!   `setActive(undefined === "open")` -> `setActive(false)` — the null-gate
//!   and the string compare disagree on undefined (pinned by the trace).
//!   Only the exact string `"open"` yields true; "draining", "fenced" and a
//!   refusal object all yield `setActive(false)`.
//! * `CHECKIN_INTERVAL_MS = 10_000` and the `["open","draining","fenced"]`
//!   z.enum literal array are dumped verbatim (the capture reads them off the
//!   real module: `CK.CHECKIN_INTERVAL_MS` and `ServerStateSchema.options`
//!   through the functional-enum zod shim).

use crate::js_json::{push_str, push_val, read_str, read_val, JsVal};

/// `CHECKIN_INTERVAL_MS` — the beat of the check-in loop.
pub const CHECKIN_INTERVAL_MS: f64 = 10_000.0;

/// The `z.enum(["open", "draining", "fenced"])` literal array.
pub const SERVER_STATES: [&str; 3] = ["open", "draining", "fenced"];

/// The scripted `ServerEnv` facade table shared by the S7 cluster ports
/// (`cluster_checkin`, `ranked_checkin_gate`, `game_api_cors`). The domain
/// of siteHost / publicHost / machine is `string | undefined` (never null —
/// the real ServerEnv getters collapse empty/unset env to `undefined`);
/// instanceLetter / gitCommit are required strings and numWorkers a number
/// (the capture never scripts the throw paths).
#[derive(Debug, Clone)]
pub struct EnvTable {
    pub(crate) site_host: JsVal,
    pub(crate) public_host: JsVal,
    pub(crate) instance_letter: String,
    pub(crate) git_commit: String,
    pub(crate) num_workers: f64,
    pub(crate) machine: JsVal,
    pub(crate) page_host_for: Vec<(String, JsVal)>,
}

impl Default for EnvTable {
    fn default() -> Self {
        Self {
            site_host: JsVal::Undef,
            public_host: JsVal::Undef,
            instance_letter: "a".to_string(),
            git_commit: "unknown".to_string(),
            num_workers: 2.0,
            machine: JsVal::Undef,
            page_host_for: Vec::new(),
        }
    }
}

impl EnvTable {
    /// One traced `ServerEnv` read: `[72, method, ...codec(value)]`.
    pub(crate) fn read(&self, method: u8, trace: &mut Vec<f64>) -> JsVal {
        trace.push(72.0);
        trace.push(method as f64);
        let v = match method {
            0 => self.site_host.clone(),
            1 => self.public_host.clone(),
            2 => JsVal::Str(self.instance_letter.clone()),
            3 => JsVal::Str(self.git_commit.clone()),
            4 => JsVal::Num(self.num_workers),
            5 => self.machine.clone(),
            m => unreachable!("cluster_checkin env: method {m}"),
        };
        push_val(trace, &v);
        v
    }

    /// The traced `ServerEnv.pageHostFor(host)` read: `[72, 6,
    /// ...codec(host), ...codec(value)]`; a miss in the scripted map reads
    /// as `undefined` (the real getter returns undefined off GAME_DOMAIN).
    pub(crate) fn read_page_host_for(&self, host: &str, trace: &mut Vec<f64>) -> JsVal {
        trace.push(72.0);
        trace.push(6.0);
        push_str(trace, host);
        let v = self
            .page_host_for
            .iter()
            .find(|(h, _)| h == host)
            .map(|(_, r)| r.clone())
            .unwrap_or(JsVal::Undef);
        push_val(trace, &v);
        v
    }

    /// Decode a scriptEnv op: siteHost val, publicHost val, instanceLetter
    /// str, gitCommit str, numWorkers num, machine val, n, (host str, val)*n.
    pub(crate) fn script(args: &[f64], i: &mut usize) -> Self {
        let site_host = read_val(args, i);
        let public_host = read_val(args, i);
        let instance_letter = read_str(args, i);
        let git_commit = read_str(args, i);
        let num_workers = args[*i];
        *i += 1;
        let machine = read_val(args, i);
        let n = args[*i] as usize;
        *i += 1;
        let mut page_host_for = Vec::with_capacity(n);
        for _ in 0..n {
            let h = read_str(args, i);
            let r = read_val(args, i);
            page_host_for.push((h, r));
        }
        Self {
            site_host,
            public_host,
            instance_letter,
            git_commit,
            num_workers,
            machine,
            page_host_for,
        }
    }
}

/// `isRefusal(result)` — `typeof result === "object" && result !== null`.
pub fn is_refusal(result: &JsVal) -> bool {
    matches!(result, JsVal::Obj(_) | JsVal::Arr(_))
}

/// `registeredSite()` — the page host, else the game host. Traced; shared
/// with `ranked_checkin_gate::build_site_field` (the TS module imports it).
pub fn registered_site(env: &EnvTable, trace: &mut Vec<f64>) -> JsVal {
    let site = env.read(0, trace);
    // `??` fires only for null / undefined (the domain is string | undefined).
    if matches!(site, JsVal::Undef | JsVal::Absent | JsVal::Null) {
        return env.read(1, trace);
    }
    site
}

/// `checkinBody(liveGames)` — the registration record, or `null` under local
/// development. Traced; the result object pins the key insertion order.
pub fn checkin_body(env: &EnvTable, live_games: f64, trace: &mut Vec<f64>) -> JsVal {
    let host = env.read(1, trace);
    // `host === undefined` — STRICT (a null would pass; impossible domain).
    if matches!(host, JsVal::Undef | JsVal::Absent) {
        return JsVal::Null;
    }
    let machine = env.read(5, trace);
    let mut site = registered_site(env, trace);
    if matches!(site, JsVal::Undef | JsVal::Absent | JsVal::Null) {
        // `?? host` — unreachable in the domain (registeredSite just read
        // the same defined publicHost), kept for the JS-faithful `??`.
        site = host.clone();
    }
    let letter = env.read(2, trace);
    let version = env.read(3, trace);
    let num_workers = env.read(4, trace);
    let mut fields: Vec<(String, JsVal)> = vec![
        ("site".to_string(), site),
        ("letter".to_string(), letter),
        ("host".to_string(), host),
        ("version".to_string(), version),
        ("numWorkers".to_string(), num_workers),
        ("liveGames".to_string(), JsVal::Num(live_games)),
    ];
    // Conditional spread: `machine !== undefined` STRICT — null passes.
    if !matches!(machine, JsVal::Undef | JsVal::Absent) {
        fields.push(("machine".to_string(), machine));
    }
    JsVal::Obj(fields)
}

/// `applyCheckinState(result, setActive)` — the setActive callback is the
/// traced facade: `[73, bool]` per call.
pub fn apply_checkin_state(result: &JsVal, trace: &mut Vec<f64>) {
    // `result === null` STRICT: Undef/Absent FALL THROUGH to setActive(false)
    // (the quirk pinned by the ck_apply_* scenarios).
    if matches!(result, JsVal::Null) {
        return;
    }
    let active = matches!(result, JsVal::Str(s) if s == "open");
    trace.push(73.0);
    trace.push(if active { 1.0 } else { 0.0 });
}

/// The capture harness: an env table, replaying an op stream. Traced ops
/// (5 registeredSite, 6 checkinBody, 7 applyCheckinState) prefix their res
/// with `[traceLen,(trace)*]`.
#[derive(Debug, Default)]
pub struct RigHarness {
    env: EnvTable,
}

impl RigHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Run one op. Kind table (see `tools/gen_vectors.mjs`):
    /// 0 reset -> `[0]`;
    /// 1 scriptEnv `[siteHost, publicHost, (letter-str), (commit-str),
    ///   numWorkers, machine, n, (host-str, val)*n]` -> `[0]`;
    /// 2 dumpInterval -> `[10000]`;
    /// 3 dumpServerStates -> `[3, (str)*3]`;
    /// 4 isRefusal `[result val]` -> `[0|1]`;
    /// 5 registeredSite -> `[traceLen,(trace)*, ...codec(result)]`;
    /// 6 checkinBody `[liveGames]` -> `[traceLen,(trace)*, ...codec(result)]`;
    /// 7 applyCheckinState `[result val]` -> `[traceLen,(trace)*,0]`.
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut i = 0usize;
        let mut trace: Vec<f64> = Vec::new();
        let payload: Vec<f64> = match kind {
            0 => {
                self.reset();
                vec![0.0]
            }
            1 => {
                self.env = EnvTable::script(args, &mut i);
                vec![0.0]
            }
            2 => vec![CHECKIN_INTERVAL_MS],
            3 => {
                let mut out = vec![SERVER_STATES.len() as f64];
                for s in SERVER_STATES {
                    push_str(&mut out, s);
                }
                out
            }
            4 => {
                let result = read_val(args, &mut i);
                vec![if is_refusal(&result) { 1.0 } else { 0.0 }]
            }
            5 => {
                let mut out = Vec::new();
                push_val(&mut out, &registered_site(&self.env, &mut trace));
                out
            }
            6 => {
                let live_games = args[i];
                let mut out = Vec::new();
                push_val(&mut out, &checkin_body(&self.env, live_games, &mut trace));
                out
            }
            7 => {
                let result = read_val(args, &mut i);
                apply_checkin_state(&result, &mut trace);
                vec![0.0]
            }
            k => unreachable!("cluster_checkin harness: unknown op kind {k}"),
        };
        if kind == 5 || kind == 6 || kind == 7 {
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

    fn enc(x: &str) -> Vec<f64> {
        let mut v = vec![x.encode_utf16().count() as f64];
        v.extend(x.encode_utf16().map(|u| u as f64));
        v
    }

    fn script_env(site: JsVal, public: JsVal, machine: JsVal) -> Vec<f64> {
        let mut a = Vec::new();
        push_val(&mut a, &site);
        push_val(&mut a, &public);
        a.extend(enc("b"));
        a.extend(enc("cafe123"));
        a.push(3.0);
        push_val(&mut a, &machine);
        a.push(0.0);
        a
    }

    #[test]
    fn refusal_typeof_domain() {
        assert!(!is_refusal(&JsVal::Str("open".into()))); // typeof "string"
        assert!(!is_refusal(&JsVal::Null));
        assert!(is_refusal(&JsVal::Obj(vec![(
            "refused".to_string(),
            JsVal::Str("x".into())
        )])));
        assert!(!is_refusal(&JsVal::Undef)); // typeof undefined
        assert!(!is_refusal(&JsVal::Absent));
    }

    #[test]
    fn registered_site_short_circuit() {
        // siteHost hit -> publicHost NEVER read (one trace event).
        let env = EnvTable {
            site_host: JsVal::Str("openfront.io".into()),
            ..Default::default()
        };
        let mut t = Vec::new();
        assert_eq!(registered_site(&env, &mut t), JsVal::Str("openfront.io".into()));
        // Exactly one env event: [72, 0, ...codec] — publicHost never read.
        assert_eq!(&t[..2], &[72.0, 0.0]);
        assert_eq!(t.iter().filter(|&&x| x == 72.0).count(), 1);
        // undefined siteHost -> reads publicHost too (two events).
        let env = EnvTable::default();
        let mut t = Vec::new();
        assert_eq!(registered_site(&env, &mut t), JsVal::Undef);
        assert_eq!(t.iter().filter(|&&x| x == 72.0).count(), 2);
    }

    #[test]
    fn checkin_body_key_order_and_machine() {
        let mut h = RigHarness::new();
        h.run_op(1, &script_env(JsVal::Str("s.io".into()), JsVal::Str("h.io".into()), JsVal::Undef));
        let r = h.run_op(6, &[7.0]);
        let tl = r[0] as usize;
        let mut j = 1 + tl;
        let body = read_val(&r, &mut j);
        if let JsVal::Obj(f) = &body {
            let keys: Vec<&str> = f.iter().map(|(k, _)| k.as_str()).collect();
            assert_eq!(keys, ["site", "letter", "host", "version", "numWorkers", "liveGames"]);
        } else {
            panic!()
        }
        // machine present -> appended LAST (7 keys).
        h.run_op(
            1,
            &script_env(JsVal::Undef, JsVal::Str("h.io".into()), JsVal::Str("falk2".into())),
        );
        let r = h.run_op(6, &[0.0]);
        let tl = r[0] as usize;
        let mut j = 1 + tl;
        let body = read_val(&r, &mut j);
        if let JsVal::Obj(f) = &body {
            assert_eq!(f.len(), 7);
            assert_eq!(f[6].0, "machine");
            // site fell back through registeredSite's publicHost read.
            assert_eq!(f[0].1, JsVal::Str("h.io".into()));
        } else {
            panic!()
        }
        // null host -> Null result, machine never read (trace = 1 event).
        let mut h2 = RigHarness::new();
        h2.run_op(1, &script_env(JsVal::Undef, JsVal::Undef, JsVal::Str("m".into())));
        let r = h2.run_op(6, &[1.0]);
        assert_eq!(r[0], 3.0); // [72, 1, 1]
        assert_eq!(r[4], 2.0); // Null
    }

    #[test]
    fn apply_state_null_gate_is_strict() {
        let mut t = Vec::new();
        apply_checkin_state(&JsVal::Null, &mut t);
        assert!(t.is_empty());
        apply_checkin_state(&JsVal::Str("open".into()), &mut t);
        assert_eq!(&t[..2], &[73.0, 1.0]);
        // Undef FALLS THROUGH the `=== null` gate -> setActive(false).
        let mut t = Vec::new();
        apply_checkin_state(&JsVal::Undef, &mut t);
        assert_eq!(t, vec![73.0, 0.0]);
        let mut t = Vec::new();
        apply_checkin_state(&JsVal::Str("draining".into()), &mut t);
        assert_eq!(t, vec![73.0, 0.0]);
    }
}
