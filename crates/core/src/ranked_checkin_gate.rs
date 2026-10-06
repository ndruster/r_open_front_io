//! Port of the pure-decision subset of `src/server/RankedCheckin.ts`: the
//! two verbatim log constants, the `RankedCheckinGate` flip detector and the
//! `buildVersionField` / `buildSiteField` object-literal builders.
//! `rankedCheckinPass` (fetch / AbortController / GameManager),
//! `startRankedCheckinLoops` (Math.random / timers) and the
//! `MatchmakingAssignmentSchema` (zod) are EXCLUDED.
//!
//! `isActive` is a scripted closure popped from a queue (each call returns
//! the next scripted boolean — the TS gate reads it FRESH every pass, never
//! latches). `log` is a message sink facade (precedent: MapPlaylist
//! `__MP_LOG`): every `info` message is traced as `[70, ...codec(string)]`.
//! `ServerEnv.gitCommit()` rides on the shared `cluster_checkin::EnvTable`
//! trace facade (`[72, 3, ...codec]`), and `buildSiteField` reuses
//! `cluster_checkin::registered_site` verbatim (the TS import edge).
//!
//! Faithfulness notes (quirk list):
//!
//! * `lastActive` is SEEDED `true`: the first pass of a healthy (active)
//!   worker logs NOTHING — only a real flip is worth a line. A worker that
//!   never hears from its master keeps taking matches (the pre-OPE-469
//!   default).
//! * `shouldCheckIn`: `active !== lastActive` is a strict boolean compare;
//!   on a flip lastActive updates AND the log fires — `active ? RESUMED :
//!   PAUSED` (true -> resumed, false -> paused). The return value is the
//!   FRESH `active`, not `lastActive`. Repeated same-value passes produce
//!   zero log events (pinned by the trace).
//! * `buildVersionField`: `isCommitLike(ServerEnv.gitCommit()) ? {version:
//!   commit.toLowerCase()} : {}`. `toLowerCase()` is JS Unicode-aware; the
//!   capture feeds ASCII hex shas and labels, where it agrees with Rust's
//!   `str::to_lowercase` byte-for-byte. The non-commit branch returns an
//!   EMPTY object literal — the `version` key is ABSENT, not present-
//!   undefined (the codec dump pins the `{}` vs `{version:...}` shapes;
//!   the TS comment is explicit: "a spread of {} rather than `version:
//!   undefined`").
//! * `buildSiteField`: `site !== undefined && isSiteLike(site) ? {site} :
//!   {}`. The gate is STRICT `!== undefined` — a null site would PASS it and
//!   then `isSiteLike(null)` reads `null.length` -> TypeError. The real
//!   domain (`registeredSite()` over ServerEnv getters returning `string |
//!   undefined`) makes null impossible, so the capture never feeds it; the
//!   Rust port models the gate as Undef/Absent -> `{}`, Str -> isSiteLike
//!   check. The `{site}` shorthand carries the ORIGINAL string (no
//!   normalisation).

use crate::cluster_checkin::{registered_site, EnvTable};
use crate::js_json::{push_str, push_val, read_str, read_val, JsVal};
use crate::server_list::{is_commit_like, is_site_like};

/// `RANKED_PAUSED_LOG` — the drain flip message, verbatim.
pub const RANKED_PAUSED_LOG: &str = "ranked matchmaking paused: deployment draining";

/// `RANKED_RESUMED_LOG` — the resume flip message, verbatim.
pub const RANKED_RESUMED_LOG: &str = "ranked matchmaking resumed: deployment active";

/// `RankedCheckinGate` — the flip detector over the scripted `isActive`
/// closure; the log sink calls ride the shared trace (`[70, ...codec]`).
#[derive(Debug, Default)]
pub struct RankedCheckinGate {
    last_active: Option<bool>,
    /// Scripted `isActive()` return queue (shifted per call).
    active_queue: Vec<bool>,
}

impl RankedCheckinGate {
    pub fn new() -> Self {
        Self {
            // Seeded active: the first pass of a healthy worker logs nothing.
            last_active: Some(true),
            active_queue: Vec::new(),
        }
    }

    /// `shouldCheckIn()` — reads `isActive()` fresh (never latches), logs on
    /// a flip, returns the fresh active.
    pub fn should_check_in(&mut self, trace: &mut Vec<f64>) -> bool {
        if self.active_queue.is_empty() {
            panic!("ranked_checkin_gate harness: unscripted isActive()");
        }
        let active = self.active_queue.remove(0);
        if Some(active) != self.last_active {
            self.last_active = Some(active);
            trace.push(70.0);
            push_str(
                trace,
                if active {
                    RANKED_RESUMED_LOG
                } else {
                    RANKED_PAUSED_LOG
                },
            );
        }
        active
    }
}

/// `buildVersionField()` — `{version: commit.toLowerCase()}` when the
/// GIT_COMMIT is commit-shaped, else the empty `{}` (key ABSENT). Traced
/// through the shared env facade (`[72, 3, ...codec]` for gitCommit).
pub fn build_version_field(env: &EnvTable, trace: &mut Vec<f64>) -> JsVal {
    let commit = env.read(3, trace);
    let s = match commit {
        JsVal::Str(ref c) => c.clone(),
        other => {
            let _ = other;
            // The real gitCommit() throws on unset and always returns a
            // string otherwise; the capture never scripts other forms.
            unreachable!("ranked_checkin_gate: gitCommit must be a string")
        }
    };
    if is_commit_like(&s) {
        JsVal::Obj(vec![("version".to_string(), JsVal::Str(s.to_lowercase()))])
    } else {
        JsVal::Obj(vec![])
    }
}

/// `buildSiteField()` — `{site}` when `registeredSite()` is defined and
/// site-shaped, else `{}`. Traced through the env facade (siteHost /
/// publicHost reads) exactly as the TS call chain.
pub fn build_site_field(env: &EnvTable, trace: &mut Vec<f64>) -> JsVal {
    let site = registered_site(env, trace);
    // `site !== undefined` — STRICT; null would pass into isSiteLike and
    // throw in JS (impossible in the string | undefined domain).
    match site {
        JsVal::Undef | JsVal::Absent => JsVal::Obj(vec![]),
        JsVal::Str(ref s) if is_site_like(s) => {
            JsVal::Obj(vec![("site".to_string(), site.clone())])
        }
        JsVal::Str(_) => JsVal::Obj(vec![]),
        _ => JsVal::Obj(vec![]),
    }
}

/// The capture harness: a gate + env table, replaying an op stream. Traced
/// ops (2 shouldCheckIn, 4 buildVersionField, 5 buildSiteField) prefix
/// their res with `[traceLen,(trace)*]`.
#[derive(Debug)]
pub struct RigHarness {
    gate: RankedCheckinGate,
    env: EnvTable,
}

impl Default for RigHarness {
    fn default() -> Self {
        Self {
            // NOT `RankedCheckinGate::default()`: the derived Default would
            // leave lastActive unseeded; the TS field initializer is `true`.
            gate: RankedCheckinGate::new(),
            env: EnvTable::default(),
        }
    }
}

impl RigHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self {
            gate: RankedCheckinGate::new(),
            env: EnvTable::default(),
        };
    }

    /// Run one op. Kind table (see `tools/gen_vectors.mjs`):
    /// 0 reset -> `[0]`;
    /// 1 scriptActive `[n, (bool)*n]` -> `[0]`;
    /// 2 shouldCheckIn -> `[traceLen,(trace)*,active 0|1]`;
    /// 3 scriptEnv `[(commit-str), siteHost, publicHost, n, (host-str,
    ///   val)*n]` -> `[0]` (ranked slice: gitCommit + the registeredSite
    ///   pair; unused env fields keep the cluster_checkin defaults);
    /// 4 buildVersionField -> `[traceLen,(trace)*, ...codec(result)]`;
    /// 5 buildSiteField -> `[traceLen,(trace)*, ...codec(result)]`;
    /// 6 dumpLogs -> `[2, (paused-str), (resumed-str)]`.
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
                self.gate.active_queue = (0..n)
                    .map(|_| {
                        let b = args[i] != 0.0;
                        i += 1;
                        b
                    })
                    .collect();
                vec![0.0]
            }
            2 => vec![if self.gate.should_check_in(&mut trace) {
                1.0
            } else {
                0.0
            }],
            3 => {
                let git_commit = read_str(args, &mut i);
                let site_host = read_val(args, &mut i);
                let public_host = read_val(args, &mut i);
                let n = args[i] as usize;
                i += 1;
                let mut page_host_for = Vec::with_capacity(n);
                for _ in 0..n {
                    let h = read_str(args, &mut i);
                    let r = read_val(args, &mut i);
                    page_host_for.push((h, r));
                }
                self.env = EnvTable {
                    site_host,
                    public_host,
                    git_commit,
                    page_host_for,
                    ..EnvTable::default()
                };
                vec![0.0]
            }
            4 => {
                let mut out = Vec::new();
                push_val(&mut out, &build_version_field(&self.env, &mut trace));
                out
            }
            5 => {
                let mut out = Vec::new();
                push_val(&mut out, &build_site_field(&self.env, &mut trace));
                out
            }
            6 => {
                let mut out = vec![2.0];
                push_str(&mut out, RANKED_PAUSED_LOG);
                push_str(&mut out, RANKED_RESUMED_LOG);
                out
            }
            k => unreachable!("ranked_checkin_gate harness: unknown op kind {k}"),
        };
        if kind == 2 || kind == 4 || kind == 5 {
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

    #[test]
    fn gate_seeded_true_first_pass_logs_nothing() {
        let mut h = RigHarness::new();
        h.run_op(1, &[1.0, 1.0]); // queue [true]
        let r = h.run_op(2, &[]);
        assert_eq!(r[0], 0.0); // empty trace: no log
        assert_eq!(*r.last().unwrap(), 1.0); // returns active
    }

    #[test]
    fn gate_flip_logs_paused_then_resumed() {
        let mut h = RigHarness::new();
        h.run_op(1, &[3.0, 0.0, 0.0, 1.0]);
        let r = h.run_op(2, &[]); // true -> false flip: PAUSED
        assert!(r[0] > 0.0);
        assert_eq!(r[1], 70.0);
        assert_eq!(*r.last().unwrap(), 0.0); // returns false
        let r = h.run_op(2, &[]); // same value: zero log
        assert_eq!(r[0], 0.0);
        let r = h.run_op(2, &[]); // false -> true flip: RESUMED
        assert!(r[0] > 0.0);
        let mut j = 2usize;
        let msg = read_str(&r, &mut j);
        assert_eq!(msg, RANKED_RESUMED_LOG);
    }

    #[test]
    fn version_field_branches() {
        let mut h = RigHarness::new();
        // commit-like uppercase sha -> lowercased version key.
        h.run_op(3, &{
            let mut a = enc("DEADBEEF0123456789ABCDEF");
            a.push(1.0); // Undef siteHost
            a.push(1.0); // Undef publicHost
            a.push(0.0);
            a
        });
        let r = h.run_op(4, &[]);
        let tl = r[0] as usize;
        let mut j = 1 + tl;
        let v = read_val(&r, &mut j);
        assert_eq!(
            v,
            JsVal::Obj(vec![("version".to_string(), JsVal::Str("deadbeef0123456789abcdef".into()))])
        );
        // "DEV" label -> empty object, key ABSENT.
        h.run_op(3, &{
            let mut a = enc("DEV");
            a.push(1.0);
            a.push(1.0);
            a.push(0.0);
            a
        });
        let r = h.run_op(4, &[]);
        let tl = r[0] as usize;
        let mut j = 1 + tl;
        assert_eq!(read_val(&r, &mut j), JsVal::Obj(vec![]));
    }

    #[test]
    fn site_field_branches() {
        let mut h = RigHarness::new();
        // site-like -> {site} shorthand with the original string.
        h.run_op(3, &{
            let mut a = enc("unknown");
            push_val(&mut a, &JsVal::Str("openfront.io".into()));
            a.push(1.0);
            a.push(0.0);
            a
        });
        let r = h.run_op(5, &[]);
        let tl = r[0] as usize;
        let mut j = 1 + tl;
        assert_eq!(
            read_val(&r, &mut j),
            JsVal::Obj(vec![("site".to_string(), JsVal::Str("openfront.io".into()))])
        );
        // malformed "localhost:9000" (colon) -> isSiteLike false -> {}.
        h.run_op(3, &{
            let mut a = enc("unknown");
            push_val(&mut a, &JsVal::Str("localhost:9000".into()));
            a.push(1.0);
            a.push(0.0);
            a
        });
        let r = h.run_op(5, &[]);
        let tl = r[0] as usize;
        let mut j = 1 + tl;
        assert_eq!(read_val(&r, &mut j), JsVal::Obj(vec![]));
        // undefined site (local dev) -> {} and the trace still pins both
        // registeredSite env reads.
        h.run_op(3, &{
            let mut a = enc("unknown");
            a.push(1.0);
            a.push(1.0);
            a.push(0.0);
            a
        });
        let r = h.run_op(5, &[]);
        assert_eq!(r[0], 6.0); // two [72, m, 1] events
    }
}
