//! Bit-exact port of `src/core/game/AllianceRequestImpl.ts`: a pending
//! alliance request between two players. The `Req` struct replicates the
//! private field block; construction and every method run as a `kind` of
//! [`AllianceRequestHarness::run_op`] over the `js_json` codec (precedent
//! `alliance_impl`). The `game` (GameImpl) surface and the two `Player`s are
//! scripted facades whose every call is pinned into a flat trace.
//!
//! Ported surface (TS line anchors): the field block (6), the ctor (8-13),
//! `status` (15-17), `requestor` / `recipient` (19-25), `createdAt` (27-29),
//! `accept` (31-34), `reject` (35-38), `toUpdate` (40-47).
//!
//! Facade (mock) surface — the capture scripts these, every call traced:
//!
//! * `game.acceptAllianceRequest(this)` (95, `this` = identity token `0`),
//!   `game.rejectAllianceRequest(this)` (96), `player.smallID()` (94, keyed by
//!   pid — the same event code as the alliance harness).
//! * Players cross as TOKENS (indices into the harness `players` table), same
//!   identity model as `alliance_impl` (G3b3 reuses it).
//!
//! Faithfulness notes:
//!
//! * `accept()` / `reject()` set the status string BEFORE the facade call —
//!   the trace order is [status write (invisible), 95/96 event].
//! * `toUpdate()` key order = TS declaration order: `type`
//!   (`GameUpdateType.AllianceRequest` = numeric member 5), `requestorID`
//!   (the requestor player's `smallID()` facade FIRST), `recipientID`
//!   (SECOND), `createdAt`.
//! * The status strings cross as `[5, len, units...]` codec strings; the
//!   initial value is `"pending"`.

use crate::js_json::{push_val, JsVal};

/// `GameUpdateType.AllianceRequest` (numeric enum member 5).
const UPDATE_TYPE_ALLIANCE_REQUEST: f64 = 5.0;

/// FIFO script consumption with the capture's exhausted-repeat rule (the
/// `ui_` / `al_` rule; event codes 90-98 are documented in `alliance_impl`).
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
struct ArPlayer {
    pid: f64,
    small_id: Vec<f64>,
    si: usize,
}

/// The `AllianceRequestImpl` private field block, replicated 1:1.
#[derive(Clone, Debug)]
struct Req {
    status: String,
    req_tok: usize,
    rec_tok: usize,
    tick_created: f64,
}

impl Default for Req {
    fn default() -> Self {
        Self {
            status: "pending".to_string(),
            req_tok: 0,
            rec_tok: 0,
            tick_created: 0.0,
        }
    }
}

/// Success payload `[0, ...encVal(v)]`.
fn push_ok(out: &mut Vec<f64>, v: &JsVal) {
    out.push(0.0);
    push_val(out, v);
}

fn push_ok_num(out: &mut Vec<f64>, v: f64) {
    push_ok(out, &JsVal::Num(v));
}

#[derive(Default)]
pub struct AllianceRequestHarness {
    r: Req,
    players: Vec<ArPlayer>,
}

impl AllianceRequestHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn player_small_id(&mut self, tr: &mut Vec<f64>, tok: usize) -> f64 {
        let p = &mut self.players[tok];
        let v = take_f64(&p.small_id, &mut p.si);
        tr.push(94.0);
        tr.push(p.pid);
        tr.push(v);
        v
    }

    /// Run one op. Kind table (mirrored by `tools/gen_vectors.mjs`):
    /// 0 construct `[reqTok, recTok, tickCreated, playersBlock]` -> `[0]`
    /// (the ctor has no facade calls); kind 1 method `[mid,...]` ->
    /// `[traceLen,(trace)*,[0,...encVal]]`. mid follows the TS declaration
    /// order: 0 status 1 requestor 2 recipient 3 createdAt 4 accept 5 reject
    /// 6 toUpdate.
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut c = Cur(args, 0);
        let mut tr: Vec<f64> = Vec::new();
        let mut out: Vec<f64> = Vec::new();
        match kind {
            0 => {
                self.reset();
                let req_tok = c.u();
                let rec_tok = c.u();
                let tick_created = c.f();
                let n = c.u();
                let players = (0..n)
                    .map(|_| {
                        let pid = c.f();
                        let small_id = c.list();
                        ArPlayer {
                            pid,
                            small_id,
                            si: 0,
                        }
                    })
                    .collect();
                self.players = players;
                self.r = Req {
                    req_tok,
                    rec_tok,
                    tick_created,
                    ..Default::default()
                };
                out.push(tr.len() as f64);
                out.extend(tr.iter().copied());
                out.push(0.0);
                out
            }
            1 => {
                let mid = c.u() as u8;
                match mid {
                    0 => push_ok(&mut out, &JsVal::Str(self.r.status.clone())),
                    1 => push_ok_num(&mut out, self.r.req_tok as f64),
                    2 => push_ok_num(&mut out, self.r.rec_tok as f64),
                    3 => push_ok_num(&mut out, self.r.tick_created),
                    4 => {
                        // accept(): status_ = "accepted" THEN the facade.
                        self.r.status = "accepted".to_string();
                        tr.push(95.0);
                        tr.push(0.0);
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    5 => {
                        self.r.status = "rejected".to_string();
                        tr.push(96.0);
                        tr.push(0.0);
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    6 => {
                        // toUpdate(): requestorID smallID FIRST, recipientID
                        // second (declaration order).
                        let rid = self.player_small_id(&mut tr, self.r.req_tok);
                        let cid = self.player_small_id(&mut tr, self.r.rec_tok);
                        let upd = vec![
                            (
                                "type".to_string(),
                                JsVal::Num(UPDATE_TYPE_ALLIANCE_REQUEST),
                            ),
                            ("requestorID".to_string(), JsVal::Num(rid)),
                            ("recipientID".to_string(), JsVal::Num(cid)),
                            ("createdAt".to_string(), JsVal::Num(self.r.tick_created)),
                        ];
                        push_ok(&mut out, &JsVal::Obj(upd));
                    }
                    k => unreachable!("alliance request harness: unknown method id {k}"),
                }
                let mut full = Vec::with_capacity(1 + tr.len() + out.len());
                full.push(tr.len() as f64);
                full.extend(tr.iter().copied());
                full.extend(out.iter().copied());
                full
            }
            k => unreachable!("alliance request harness: unknown op kind {k}"),
        }
    }
}
