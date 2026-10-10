//! Bit-exact port of `src/core/game/AllianceImpl.ts`: the alliance entity
//! between two players. The `Alliance` struct replicates every private field
//! of the TS class; construction and every method run as a `kind` of
//! [`AllianceHarness::run_op`] over the `js_json` codec (precedent `unit_impl`).
//! The `mg` (Game) surface and the two `Player`s are scripted facades whose
//! every call is pinned into a flat trace, so the facade call order — notably
//! `extend`'s LEFT-to-RIGHT `mg.ticks()` before `mg.config().allianceDuration()`
//! — and the `===` reference-identity gates are part of the compared stream.
//!
//! Ported surface (TS line anchors): the field block (5-8), the ctor (10-18),
//! `other` (20-25), `requestor` / `recipient` / `createdAt` (27-37), `expire`
//! (39-41), `addExtensionRequest` (43-54), `bothAgreedToExtend` (56-60),
//! `onlyOneAgreedToExtend` (62-69), `agreedToExtend` (71-76), `id` (78-80),
//! `extend` (82-86), `expiresAt` (88-90).
//!
//! Facade (mock) surface — the capture scripts these, every call traced:
//!
//! * `mg.config().allianceDuration()` (90), `mg.ticks()` (91),
//!   `mg.expireAlliance(this)` (92, `this` crosses as the identity token `0`),
//!   `mg.addUpdate({type: AllianceExtension, playerID, allianceID})` (93, the
//!   full update object rides the trace), `player.smallID()` (94, keyed by pid).
//! * Players cross as TOKENS (index into the harness `players` table): the
//!   `===` gates in `other` / `addExtensionRequest` / `agreedToExtend` are
//!   reference equality, modelled as token equality (the `ui_` token-0
//!   convention extended to players — G3b3 reuses this identity model).
//!
//! Faithfulness notes:
//!
//! * `other(player)`: `requestor_ === player` returns the `recipient_` token,
//!   ELSE the `requestor_` token — a third player falls into the else branch.
//! * `addExtensionRequest`: the requestor / recipient flags are set by the
//!   `if / else if` `===` chain (a non-member sets NEITHER), but the
//!   `addUpdate` fires UNCONDITIONALLY with `player.smallID()` of the ARG.
//! * `extend()` resets BOTH flags, then `expiresAt_ = ticks() +
//!   allianceDuration()` — the ticks facade (91) is traced BEFORE the config
//!   facade (90) (JS left-to-right evaluation).
//! * `GameUpdateType.AllianceExtension` is the numeric enum member `9`
//!   (GameUpdates.ts:83-109, zero-based declaration order).

use crate::js_json::{push_val, JsVal};

/// `GameUpdateType.AllianceExtension` (numeric enum member 9).
const UPDATE_TYPE_ALLIANCE_EXTENSION: f64 = 9.0;

// ---- facade trace event codes (G3b1 segment 90-98) --------------------------
//
// Scripted returns are consumed FIFO per scenario; an exhausted list REPEATS
// its last value on both sides (the `ui_` rule; an empty list throws on the
// first call, so the capture must script at least one value per used method).
//
// 90 config.allianceDuration [90, ret]
// 91 mg.ticks [91, ret]
// 92 mg.expireAlliance [92, 0]            (this = the alliance under test)
// 93 mg.addUpdate [93, ...encVal(update)]
// 94 player.smallID [94, pid, ret]
// (95/96 belong to alliance_request_impl; 97/98 to attack_impl.)

/// FIFO script consumption with the capture's exhausted-repeat rule.
fn take_f64(list: &[f64], i: &mut usize) -> f64 {
    let idx = (*i).min(list.len() - 1);
    *i += 1;
    list[idx]
}

/// Arg cursor over the flat f64 token stream.
struct Cur<'a>(&'a [f64], usize);
impl<'a> Cur<'a> {
    fn f(&mut self) -> f64 {
        let v = self.0[self.1];
        self.1 += 1;
        v
    }
    fn u(&mut self) -> usize {
        self.f() as usize
    }
    fn list(&mut self) -> Vec<f64> {
        let n = self.u();
        (0..n).map(|_| self.f()).collect()
    }
}

/// One scripted player mock: pid + FIFO `smallID` script.
#[derive(Clone, Debug)]
struct AlPlayer {
    pid: f64,
    small_id: Vec<f64>,
    si: usize,
}

/// The scripted `mg` (Game) facade for one scenario.
#[derive(Clone, Debug, Default)]
struct AlMg {
    alliance_duration: Vec<f64>,
    ad_i: usize,
    ticks: Vec<f64>,
    tk_i: usize,
}

/// The `AllianceImpl` private field block, replicated 1:1.
#[derive(Clone, Debug, Default)]
struct Alliance {
    ext_req_requestor: bool,
    ext_req_recipient: bool,
    expires_at: f64,
    created_at: f64,
    id: f64,
    /// player tokens (indices into the harness `players` table).
    req_tok: usize,
    rec_tok: usize,
}

/// Success payload `[0, ...encVal(v)]`.
fn push_ok(out: &mut Vec<f64>, v: &JsVal) {
    out.push(0.0);
    push_val(out, v);
}

fn push_ok_num(out: &mut Vec<f64>, v: f64) {
    push_ok(out, &JsVal::Num(v));
}

fn push_ok_bool(out: &mut Vec<f64>, v: bool) {
    push_ok(out, &JsVal::Bool(v));
}

#[derive(Default)]
pub struct AllianceHarness {
    a: Alliance,
    players: Vec<AlPlayer>,
    mg: AlMg,
}

impl AllianceHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    // --- facade plumbing ---

    fn cfg_alliance_duration(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = take_f64(&self.mg.alliance_duration, &mut self.mg.ad_i);
        tr.push(90.0);
        tr.push(v);
        v
    }
    fn mg_ticks(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = take_f64(&self.mg.ticks, &mut self.mg.tk_i);
        tr.push(91.0);
        tr.push(v);
        v
    }
    fn player_small_id(&mut self, tr: &mut Vec<f64>, tok: usize) -> f64 {
        let p = &mut self.players[tok];
        let v = take_f64(&p.small_id, &mut p.si);
        tr.push(94.0);
        tr.push(p.pid);
        tr.push(v);
        v
    }

    // --- the op dispatcher ---

    /// Run one op. Kind table (mirrored by `tools/gen_vectors.mjs`):
    /// 0 construct `[reqTok, recTok, createdAt, id, durScript, ticksScript,
    /// playersBlock]` -> `[traceLen,(trace)*,0]` (the ctor's
    /// `allianceDuration` call rides the trace); kind 1 method `[mid,...]` ->
    /// `[traceLen,(trace)*,[0,...encVal]]`. mid follows the TS declaration
    /// order: 0 other(tok) 1 requestor 2 recipient 3 createdAt 4 expire
    /// 5 addExtensionRequest(tok) 6 bothAgreedToExtend 7 onlyOneAgreedToExtend
    /// 8 agreedToExtend(tok) 9 id 10 extend 11 expiresAt.
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut c = Cur(args, 0);
        let mut tr: Vec<f64> = Vec::new();
        let mut out: Vec<f64> = Vec::new();
        match kind {
            0 => {
                self.reset();
                let req_tok = c.u();
                let rec_tok = c.u();
                let created_at = c.f();
                let id = c.f();
                let alliance_duration = c.list();
                let ticks = c.list();
                let n = c.u();
                let players = (0..n)
                    .map(|_| {
                        let pid = c.f();
                        let small_id = c.list();
                        AlPlayer {
                            pid,
                            small_id,
                            si: 0,
                        }
                    })
                    .collect();
                self.players = players;
                self.mg = AlMg {
                    alliance_duration,
                    ad_i: 0,
                    ticks,
                    tk_i: 0,
                };
                self.a = Alliance {
                    created_at,
                    id,
                    req_tok,
                    rec_tok,
                    ..Default::default()
                };
                // ctor (17): expiresAt_ = createdAt_ + allianceDuration().
                let dur = self.cfg_alliance_duration(&mut tr);
                self.a.expires_at = created_at + dur;
                out.push(tr.len() as f64);
                out.extend(tr.iter().copied());
                out.push(0.0);
                out
            }
            1 => {
                let mid = c.u() as u8;
                match mid {
                    0 => {
                        // other(player)
                        let tok = c.u();
                        let r = if tok == self.a.req_tok {
                            self.a.rec_tok
                        } else {
                            self.a.req_tok
                        };
                        push_ok_num(&mut out, r as f64);
                    }
                    1 => push_ok_num(&mut out, self.a.req_tok as f64),
                    2 => push_ok_num(&mut out, self.a.rec_tok as f64),
                    3 => push_ok_num(&mut out, self.a.created_at),
                    4 => {
                        // expire(): mg.expireAlliance(this) — `this` is the
                        // alliance under test, identity token 0.
                        tr.push(92.0);
                        tr.push(0.0);
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    5 => {
                        // addExtensionRequest(player)
                        let tok = c.u();
                        if tok == self.a.req_tok {
                            self.a.ext_req_requestor = true;
                        } else if tok == self.a.rec_tok {
                            self.a.ext_req_recipient = true;
                        }
                        let sid = self.player_small_id(&mut tr, tok);
                        let id = self.a.id;
                        let upd = vec![
                            (
                                "type".to_string(),
                                JsVal::Num(UPDATE_TYPE_ALLIANCE_EXTENSION),
                            ),
                            ("playerID".to_string(), JsVal::Num(sid)),
                            ("allianceID".to_string(), JsVal::Num(id)),
                        ];
                        tr.push(93.0);
                        push_val(&mut tr, &JsVal::Obj(upd));
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    6 => push_ok_bool(&mut out, self.a.ext_req_requestor && self.a.ext_req_recipient),
                    7 => push_ok_bool(&mut out, self.a.ext_req_requestor != self.a.ext_req_recipient),
                    8 => {
                        // agreedToExtend(player)
                        let tok = c.u();
                        let r = (tok == self.a.req_tok && self.a.ext_req_requestor)
                            || (tok == self.a.rec_tok && self.a.ext_req_recipient);
                        push_ok_bool(&mut out, r);
                    }
                    9 => push_ok_num(&mut out, self.a.id),
                    10 => {
                        // extend(): reset both flags, then ticks() FIRST,
                        // allianceDuration() second (JS left-to-right).
                        self.a.ext_req_requestor = false;
                        self.a.ext_req_recipient = false;
                        let t = self.mg_ticks(&mut tr);
                        let d = self.cfg_alliance_duration(&mut tr);
                        self.a.expires_at = t + d;
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    11 => push_ok_num(&mut out, self.a.expires_at),
                    k => unreachable!("alliance harness: unknown method id {k}"),
                }
                let mut full = Vec::with_capacity(1 + tr.len() + out.len());
                full.push(tr.len() as f64);
                full.extend(tr.iter().copied());
                full.extend(out.iter().copied());
                full
            }
            k => unreachable!("alliance harness: unknown op kind {k}"),
        }
    }
}
