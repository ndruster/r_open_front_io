//! Port of `src/server/MatchTelemetryRecorder.ts` (the full recorder) and
//! `identityFor`. The `emitter` is a construction-injected BLACK-BOX facade
//! (precedent: the Censor matcher): the capture passes a real JS emitter
//! whose `emit(event)` consults a scripted outcome queue — `"enqueued"`,
//! `"dropped"`, or `throw` — and every call is a trace event (`[60,
//! ...codec(event), outcome 0|1|2]`) pinning the event object byte-for-byte
//! (key order, the post-incremented sequence, the scripted `Date.now()`).
//! `Date.now()` is scripted through `globalThis.__MT_NOW` (precedent:
//! `__LISTING_NOW` / `__MP_SEED`).
//!
//! Faithfulness notes (quirk list):
//!
//! * `identityFor(client)` -> `{clientId: client.clientID, publicId:
//!   client.publicId}` — key order pinned; `publicId` may be `undefined`
//!   (present-undefined, NOT absent).
//! * `emit` builds the event as an object literal with the FIXED key order
//!   `{schemaVersion:1, type, matchId, sequence, observedAt, serverTick,
//!   payload}`. `sequence` POST-INCREMENTS (`this.sequence++`): the event
//!   carries the pre-call value and the counter advances even when the
//!   emitter throws — a gap in the sequence means a drop.
//! * An emitter that THROWS is caught and reported as `"dropped"`; the
//!   trace outcome distinguishes 0=enqueued / 1=dropped(return) / 2=throw,
//!   so both drop flavours are pinned separately.
//! * `intentObserved`: `counts = tickCounts.get(tick) ?? {observed:0,
//!   enqueued:0, dropped:0}`; `counts.observed++` happens BEFORE the emit;
//!   the payload literal `{identity, intentType, outcome, reasonCode,
//!   reasonDetail, intent}` has SIX KEYS ALWAYS PRESENT — omitted
//!   `reasonCode` / `reasonDetail` arguments are present-`undefined` (Undef,
//!   NOT Absent; the codec dump pins this). `result === "enqueued" ?
//!   enqueued++ : dropped++`; the counts are set back into the map (a JS
//!   `Map<number, TickCounts>`, SameValueZero numeric keys — `+0`/`-0`
//!   collapse, `NaN` by bits — via the shared `NumMap`).
//! * `takeTickCounts`: `get ?? default`, then `delete` the tick (the entry
//!   is GONE — a later `intentObserved` for the same tick starts a fresh
//!   default), return the counts.
//! * `matchFinished(totalTurns)`: the `finished` latch fires ONCE; the emit
//!   is `("match_finished", {endedAt: Date.now(), totalTurns, buildHash,
//!   replayArchiveAttempted}, totalTurns)` — NOTE the serverTick argument is
//!   `totalTurns` itself. `endedAt` is a SECOND scripted `Date.now()` call
//!   (the queue pops in call order).
//! * `noteArchiveAttempted` sets the latch; `match_finished` reports it.

use crate::desync_detector::NumMap;
use crate::js_json::{push_val, read_str, read_val, JsVal};

/// The `TickCounts` triple (key order observed, enqueued, dropped — the
/// default literal's order).
#[derive(Debug, Clone, Copy, Default)]
struct TickCounts {
    observed: f64,
    enqueued: f64,
    dropped: f64,
}

impl TickCounts {
    fn to_val(self) -> JsVal {
        JsVal::Obj(vec![
            ("observed".to_string(), JsVal::Num(self.observed)),
            ("enqueued".to_string(), JsVal::Num(self.enqueued)),
            ("dropped".to_string(), JsVal::Num(self.dropped)),
        ])
    }
}

/// Emitter-script outcome: 0 enqueued, 1 dropped(return), 2 throw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EmitOutcome {
    Enqueued,
    Dropped,
    Threw,
}

/// `identityFor(client)` — the narrow stub reads `clientID` / `publicId`.
pub fn identity_for(client_id: &str, public_id: &JsVal) -> JsVal {
    JsVal::Obj(vec![
        ("clientId".to_string(), JsVal::Str(client_id.to_string())),
        ("publicId".to_string(), public_id.clone()),
    ])
}

/// The ported `MatchTelemetryRecorder` over the scripted emitter / clock.
#[derive(Debug, Default)]
pub struct MatchTelemetryRecorder {
    sequence: f64,
    tick_counts: NumMap<TickCounts>,
    replay_archive_attempted: bool,
    finished: bool,
    match_id: String,
    build_hash: String,
    /// Scripted `Date.now()` queue (popped in call order).
    nows: Vec<f64>,
    /// Scripted emitter outcome queue (popped per `emit` call).
    emitter: Vec<EmitOutcome>,
}

impl MatchTelemetryRecorder {
    fn pop_now(&mut self) -> f64 {
        if self.nows.is_empty() {
            panic!("match_telemetry harness: unscripted Date.now()");
        }
        self.nows.remove(0)
    }

    fn pop_emitter(&mut self) -> EmitOutcome {
        if self.emitter.is_empty() {
            panic!("match_telemetry harness: unscripted emitter outcome");
        }
        self.emitter.remove(0)
    }

    /// `emit(type, payload, serverTick)` — traced; returns the JS-visible
    /// result ("enqueued" / "dropped").
    fn emit(&mut self, type_: &str, payload: JsVal, server_tick: f64, trace: &mut Vec<f64>) -> &'static str {
        let event = JsVal::Obj(vec![
            ("schemaVersion".to_string(), JsVal::Num(1.0)),
            ("type".to_string(), JsVal::Str(type_.to_string())),
            ("matchId".to_string(), JsVal::Str(self.match_id.clone())),
            // POST-increment: the event carries the pre-call value.
            ("sequence".to_string(), JsVal::Num(self.sequence)),
            ("observedAt".to_string(), JsVal::Num(self.pop_now())),
            ("serverTick".to_string(), JsVal::Num(server_tick)),
            ("payload".to_string(), payload),
        ]);
        self.sequence += 1.0;
        trace.push(60.0);
        push_val(trace, &event);
        match self.pop_emitter() {
            EmitOutcome::Enqueued => {
                trace.push(0.0);
                "enqueued"
            }
            EmitOutcome::Dropped => {
                trace.push(1.0);
                "dropped"
            }
            EmitOutcome::Threw => {
                // try/catch: the throw is invisible to the caller, only the
                // trace distinguishes it.
                trace.push(2.0);
                "dropped"
            }
        }
    }

    /// `intentObserved(...)`.
    #[allow(clippy::too_many_arguments)] // faithful to the TS parameter list
    fn intent_observed(
        &mut self,
        client_id: &str,
        public_id: &JsVal,
        intent: JsVal,
        intent_type: JsVal,
        outcome: &str,
        server_tick: f64,
        reason_code: JsVal,
        reason_detail: JsVal,
        trace: &mut Vec<f64>,
    ) {
        let mut counts = *self
            .tick_counts
            .get(server_tick)
            .unwrap_or(&TickCounts::default());
        counts.observed += 1.0;
        // SIX keys always present: omitted reason args ride as
        // present-undefined (Undef), NOT absent.
        let payload = JsVal::Obj(vec![
            ("identity".to_string(), identity_for(client_id, public_id)),
            ("intentType".to_string(), intent_type),
            ("outcome".to_string(), JsVal::Str(outcome.to_string())),
            ("reasonCode".to_string(), reason_code),
            ("reasonDetail".to_string(), reason_detail),
            ("intent".to_string(), intent),
        ]);
        let result = self.emit("intent_observed", payload, server_tick, trace);
        if result == "enqueued" {
            counts.enqueued += 1.0;
        } else {
            counts.dropped += 1.0;
        }
        self.tick_counts.set(server_tick, counts);
    }

    /// `takeTickCounts(serverTick)` — get-or-default then DELETE the tick.
    fn take_tick_counts(&mut self, server_tick: f64) -> TickCounts {
        let counts = *self
            .tick_counts
            .get(server_tick)
            .unwrap_or(&TickCounts::default());
        self.tick_counts.delete(server_tick);
        counts
    }

    /// `matchFinished(totalTurns)` — the latch fires once.
    fn match_finished(&mut self, total_turns: f64, trace: &mut Vec<f64>) {
        if self.finished {
            return;
        }
        self.finished = true;
        let payload = JsVal::Obj(vec![
            ("endedAt".to_string(), JsVal::Num(self.pop_now())),
            ("totalTurns".to_string(), JsVal::Num(total_turns)),
            ("buildHash".to_string(), JsVal::Str(self.build_hash.clone())),
            (
                "replayArchiveAttempted".to_string(),
                JsVal::Bool(self.replay_archive_attempted),
            ),
        ]);
        // NOTE: the serverTick argument is totalTurns itself.
        self.emit("match_finished", payload, total_turns, trace);
    }
}

/// The capture harness: one recorder + scripted emitter / clock, replaying
/// an op stream. Traced ops (3 emit, 4 intentObserved, 6 matchFinished)
/// prefix their res with `[traceLen,(trace)*]`.
#[derive(Debug, Default)]
pub struct RigHarness {
    rec: MatchTelemetryRecorder,
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
    /// 1 construct `[(matchId-str),(buildHash-str)]` -> `[0]`;
    /// 2 identityFor `[(clientID-str),(publicId val)]` -> codec
    ///   `{clientId,publicId}`;
    /// 3 emit `[(type-str), payload, serverTick]` -> `[traceLen,(trace)*,
    ///   result 0=enqueued|1=dropped]`;
    /// 4 intentObserved `[(clientID-str), publicId, intent, intentType,
    ///   (outcome-str), serverTick, reasonCode, reasonDetail]` ->
    ///   `[traceLen,(trace)*]`;
    /// 5 takeTickCounts `[serverTick]` -> codec `{observed,enqueued,dropped}`;
    /// 6 matchFinished `[totalTurns]` -> `[traceLen,(trace)*]`;
    /// 7 noteArchiveAttempted -> `[0]`;
    /// 8 scriptEmitter `[n, (outcome 0|1|2)*n]` -> `[0]`;
    /// 9 scriptNow `[n, (num)*n]` -> `[0]`.
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut i = 0usize;
        let mut trace: Vec<f64> = Vec::new();
        let payload: Vec<f64> = match kind {
            0 => {
                self.reset();
                vec![0.0]
            }
            1 => {
                let match_id = read_str(args, &mut i);
                let build_hash = read_str(args, &mut i);
                self.rec.match_id = match_id;
                self.rec.build_hash = build_hash;
                vec![0.0]
            }
            2 => {
                let client_id = read_str(args, &mut i);
                let public_id = read_val(args, &mut i);
                let mut out = Vec::new();
                push_val(&mut out, &identity_for(&client_id, &public_id));
                out
            }
            3 => {
                let type_ = read_str(args, &mut i);
                let payload = read_val(args, &mut i);
                let tick = args[i];
                let r = self.rec.emit(&type_, payload, tick, &mut trace);
                vec![if r == "enqueued" { 0.0 } else { 1.0 }]
            }
            4 => {
                let client_id = read_str(args, &mut i);
                let public_id = read_val(args, &mut i);
                let intent = read_val(args, &mut i);
                let intent_type = read_val(args, &mut i);
                let outcome = read_str(args, &mut i);
                let tick = args[i];
                i += 1;
                let reason_code = read_val(args, &mut i);
                let reason_detail = read_val(args, &mut i);
                self.rec.intent_observed(
                    &client_id,
                    &public_id,
                    intent,
                    intent_type,
                    &outcome,
                    tick,
                    reason_code,
                    reason_detail,
                    &mut trace,
                );
                vec![0.0]
            }
            5 => {
                let tick = args[i];
                let mut out = Vec::new();
                push_val(&mut out, &self.rec.take_tick_counts(tick).to_val());
                out
            }
            6 => {
                let total_turns = args[i];
                self.rec.match_finished(total_turns, &mut trace);
                vec![0.0]
            }
            7 => {
                self.rec.replay_archive_attempted = true;
                vec![0.0]
            }
            8 => {
                let n = args[i] as usize;
                i += 1;
                for _ in 0..n {
                    let o = args[i] as u8;
                    i += 1;
                    self.rec.emitter.push(match o {
                        0 => EmitOutcome::Enqueued,
                        1 => EmitOutcome::Dropped,
                        2 => EmitOutcome::Threw,
                        x => unreachable!("match_telemetry: emitter outcome {x}"),
                    });
                }
                vec![0.0]
            }
            9 => {
                let n = args[i] as usize;
                i += 1;
                for _ in 0..n {
                    self.rec.nows.push(args[i]);
                    i += 1;
                }
                vec![0.0]
            }
            k => unreachable!("match_telemetry harness: unknown op kind {k}"),
        };
        if kind == 3 || kind == 4 || kind == 6 {
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

    fn setup(h: &mut RigHarness, emitter: &[u8], nows: &[f64]) {
        h.run_op(0, &[]);
        h.run_op(1, &{
            let mut a = enc("m1");
            a.extend(enc("abc123"));
            a
        });
        h.run_op(8, &{
            let mut a = vec![emitter.len() as f64];
            a.extend(emitter.iter().map(|x| *x as f64));
            a
        });
        h.run_op(9, &{
            let mut a = vec![nows.len() as f64];
            a.extend(nows.iter().copied());
            a
        });
    }

    #[test]
    fn identity_key_order() {
        let id = identity_for("c1", &JsVal::Undef);
        if let JsVal::Obj(f) = &id {
            assert_eq!(f[0].0, "clientId");
            assert_eq!(f[1].0, "publicId");
            assert_eq!(f[1].1, JsVal::Undef); // present-undefined, not absent
        } else {
            panic!()
        }
    }

    #[test]
    fn emit_sequence_consumed_on_throw() {
        let mut h = RigHarness::new();
        setup(&mut h, &[2, 0], &[111.0, 222.0]);
        let mut a = enc("intent_observed");
        push_val(&mut a, &JsVal::Obj(vec![]));
        a.push(5.0);
        let r = h.run_op(3, &a); // throw -> dropped, sequence 0 consumed
        let tl = r[0] as usize;
        assert!(tl > 0);
        assert_eq!(*r.last().unwrap(), 1.0); // dropped
        let mut b = enc("intent_observed");
        push_val(&mut b, &JsVal::Obj(vec![]));
        b.push(6.0);
        let r2 = h.run_op(3, &b); // enqueued at sequence 1 (gap-free: 0 then 1)
        assert_eq!(*r2.last().unwrap(), 0.0);
    }

    #[test]
    fn intent_observed_undef_reason_keys() {
        let mut h = RigHarness::new();
        setup(&mut h, &[0], &[1000.0]);
        let mut a = enc("c1");
        push_val(&mut a, &JsVal::Str("p1".into()));
        push_val(&mut a, &JsVal::Num(42.0)); // intent
        push_val(&mut a, &JsVal::Str("attack".into())); // intentType
        a.extend(enc("accepted"));
        a.push(7.0); // serverTick
        push_val(&mut a, &JsVal::Undef); // reasonCode omitted -> present-undef
        push_val(&mut a, &JsVal::Undef); // reasonDetail
        let r = h.run_op(4, &a);
        let tl = r[0] as usize;
        assert!(tl > 0);
        let counts = h.run_op(5, &[7.0]);
        let mut j = 0usize;
        let c = read_val(&counts, &mut j);
        assert_eq!(
            c,
            JsVal::Obj(vec![
                ("observed".to_string(), JsVal::Num(1.0)),
                ("enqueued".to_string(), JsVal::Num(1.0)),
                ("dropped".to_string(), JsVal::Num(0.0)),
            ])
        );
        // takeTickCounts DELETED the tick -> a second take is the default.
        let counts2 = h.run_op(5, &[7.0]);
        let mut j = 0usize;
        assert_eq!(
            read_val(&counts2, &mut j),
            JsVal::Obj(vec![
                ("observed".to_string(), JsVal::Num(0.0)),
                ("enqueued".to_string(), JsVal::Num(0.0)),
                ("dropped".to_string(), JsVal::Num(0.0)),
            ])
        );
    }

    #[test]
    fn match_finished_latch_once() {
        let mut h = RigHarness::new();
        setup(&mut h, &[0], &[5.0, 6.0]);
        h.run_op(7, &[]); // noteArchiveAttempted
        let r = h.run_op(6, &[9.0]);
        let tl = r[0] as usize;
        assert!(tl > 0);
        let r2 = h.run_op(6, &[9.0]);
        assert_eq!(r2[0], 0.0); // latch: no second emit, empty trace
    }
}
