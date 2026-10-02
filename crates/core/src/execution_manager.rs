//! Port of `src/core/execution/ExecutionManager.ts`: the `Executor` client-
//! intent dispatcher. The 24 `XxxExecution` classes and the `TribeSpawner` /
//! `PlayerSpawner` helpers are **not** ported (existing exclusion decision)
//! and are stubbed as construction recorders (precedent: `nation_utils`'
//! facade mocks + trace pinning): every `new XxxExecution(args...)` lands in
//! the trace as `[2, tag, encoded args...]` and the orchestration — the
//! `playerByClientID` facade call, the `!player` warn branch, the per-case
//! argument extraction order, the `nations().map(n => n.spawnCell).filter(c
//! !== undefined)` pipeline and the default-throw message interpolation — is
//! replayed token-by-token over the same scripted stream. The ctor op pins the
//! real `simpleHash(gameID) + 1` seed feeding the (never-used afterwards)
//! `PseudoRandom` through the ported [`crate::util::simple_hash_units`] /
//! [`crate::pseudo_random::PseudoRandom`] (the construction side effect —
//! seed math — is the observable part; the instance state is dead).
//!
//! Faithfulness notes:
//!
//! * `simpleHash(gameID) + 1` is an f64 add (the hash lives in the integer
//!   domain, so `+ 1` is exact); `PseudoRandom::new` then applies the JS
//!   `| 0` ToInt32 truncation internally, exactly like the TS ctor.
//! * The default `throw new Error(\`intent type ${intent} not found\`)`
//!   interpolates the intent **object**: JS `String(obj)` is
//!   `"intent type [object Object] not found"` for a plain object literal.
//!   An intent with a `toString` method would differ — out of the capture
//!   domain.
//! * `!player` is truthy for `undefined` **and** `null` (the facade mock
//!   returns `undefined` on a miss; a refid `0` would also be falsy, but the
//!   capture domain starts refs at 1).
//! * The `c !== undefined` filter keeps `null`, `-0` and every other value;
//!   only `undefined` drops out.
//! * `purchasedTribeNames = []` default: `undefined` (enc tag 0) at the ctor
//!   becomes `[]` (enc `[6, 0]`) and rides into the `spawnTribes` call args.
//! * The warn message interpolates the clientID **field** directly — a
//!   missing field spells `"undefined"`, `null` spells `"null"`, a string
//!   rides its UTF-16 units. A numeric clientID would go through the JS
//!   number→string repr (`js_number_to_string`); the capture keeps clientIDs
//!   in the string / undefined / null domain.
//! * Execution class tags follow the stub-definition order in
//!   `tools/ts_load.mjs` (0 NoOp, 1 Attack, 2 Retreat, 3 BoatRetreat,
//!   4 MoveWarship, 5 Spawn, 6 TransportShip, 7 AllianceRequest,
//!   8 AllianceReject, 9 BreakAlliance, 10 TargetPlayer, 11 Emoji,
//!   12 DonateTroops, 13 DonateGold, 14 Embargo, 15 EmbargoAll,
//!   16 Construction, 17 AllianceExtension, 18 UpgradeStructure,
//!   19 DeleteUnit, 20 QuickChat, 21 MarkDisconnected, 22 Pause, 23 Nation)
//!   — capture and Rust agree on this table.
//! * Refids: players / nations carry the scripted refs; constructed executions
//!   take a scenario-global counter starting at 1 (the stub returns
//!   `{__ref: ++counter}`); spawner return lists ride the `retLists` script.
//! * The `switch` matches `intent.type` with JS `===` string comparison:
//!   `undefined` (missing type) and any non-string value fall through to the
//!   default throw. A throw inside `createExecs` aborts the `map` — the
//!   already-built executions are dropped (status 1, empty ref list) and the
//!   remaining intents are only skipped, never dispatched.

use crate::pseudo_random::PseudoRandom;
use crate::util::simple_hash_units;

/// JS `Number -> String` for the warn-message template interpolation. The
/// capture keeps clientIDs in the string / undefined / null domain, so only
/// the plain-decimal path is exercised; NaN / ±Infinity / ±0 spellings follow
/// JS (exponential form beyond 1e21 / below 1e-6 is out of the domain).
fn js_number_to_string(v: f64) -> String {
    if v.is_nan() {
        return "NaN".to_string();
    }
    if v.is_infinite() {
        return if v > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    if v == 0.0 {
        return "0".to_string(); // +0 and -0 both spell "0"
    }
    let s = if v.fract() == 0.0 && v.abs() < 1e15 {
        v.trunc().to_string()
    } else {
        format!("{v}")
    };
    s
}

/// One scripted value crossing the wire: `[0]` undefined, `[1]` null,
/// `[2]` true, `[3]` false, `[4,v]` number, `[5,len,u0,..]` string,
/// `[6,len,(v)*)]` array, `[7,ref]` player ref, `[8,ref]` info ref,
/// `[12,ref]` nation ref.
#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Undef,
    Null,
    True,
    False,
    Num(f64),
    Str(Vec<u16>),
    Arr(Vec<Tok>),
    Player(f64),
    Info(f64),
    Nation(f64),
}

impl Tok {
    /// The stub `__enc` of the TS capture: how a constructed value rides into
    /// the trace.
    fn enc(&self, out: &mut Vec<f64>) {
        match self {
            Tok::Undef => out.push(0.0),
            Tok::Null => out.push(1.0),
            Tok::True => out.push(2.0),
            Tok::False => out.push(3.0),
            Tok::Num(v) => {
                out.push(4.0);
                out.push(*v);
            }
            Tok::Str(s) => {
                out.push(5.0);
                out.push(s.len() as f64);
                out.extend(s.iter().map(|&u| f64::from(u)));
            }
            Tok::Arr(a) => {
                out.push(6.0);
                out.push(a.len() as f64);
                for v in a {
                    v.enc(out);
                }
            }
            Tok::Player(r) => {
                out.push(7.0);
                out.push(*r);
            }
            Tok::Info(r) => {
                out.push(8.0);
                out.push(*r);
            }
            Tok::Nation(r) => {
                out.push(12.0);
                out.push(*r);
            }
        }
    }

    /// The warn-message template interpolation of the `clientID` field:
    /// undefined -> "undefined", null -> "null", number -> JS Number repr,
    /// string -> its UTF-16 units, anything else -> "[object Object]".
    fn to_str_units(&self) -> Vec<u16> {
        match self {
            Tok::Undef => "undefined".encode_utf16().collect(),
            Tok::Null => "null".encode_utf16().collect(),
            Tok::True => "true".encode_utf16().collect(),
            Tok::False => "false".encode_utf16().collect(),
            Tok::Num(v) => js_number_to_string(*v).encode_utf16().collect(),
            Tok::Str(s) => s.clone(),
            Tok::Arr(_) | Tok::Player(_) | Tok::Info(_) | Tok::Nation(_) => {
                "[object Object]".encode_utf16().collect()
            }
        }
    }
}

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
    fn str(&mut self) -> Vec<u16> {
        let len = self.u();
        (0..len).map(|_| self.f() as u16).collect()
    }
    /// A tagged `[5, len, u0, ..]` string (the args-side encoding).
    fn enc_str(&mut self) -> Vec<u16> {
        match self.u() {
            5 => self.str(),
            k => unreachable!("em: bad str tag {k}"),
        }
    }
    fn tok(&mut self) -> Tok {
        match self.u() {
            0 => Tok::Undef,
            1 => Tok::Null,
            2 => Tok::True,
            3 => Tok::False,
            4 => Tok::Num(self.f()),
            5 => Tok::Str(self.str()),
            6 => {
                let n = self.u();
                Tok::Arr((0..n).map(|_| self.tok()).collect())
            }
            7 => Tok::Player(self.f()),
            8 => Tok::Info(self.f()),
            _ => Tok::Nation(self.f()),
        }
    }
}

/// The Executor state: the stored ctor fields + the refid counter + the
/// spawner-return cursor. `clientID` is stored by the TS ctor but never read
/// by any ported method, so the replay only keeps what the methods consume.
struct Exec {
    game_id: Vec<u16>,
    purchased: Tok,
    refid: f64,
    ret_idx: usize,
}

/// Skip one intent token block without dispatching (the `createExecs` throw
/// aborts the map; the remaining intents still ride the args stream).
fn skip_intent(c: &mut Cur) {
    let _typ = c.tok();
    let n = c.u();
    for _ in 0..n {
        let _k = c.enc_str();
        let _v = c.tok();
    }
}

impl Exec {
    /// `new XxxExecution(args...)`: record the construction event and hand
    /// back the next refid (the stub's `{__ref: ++counter}`).
    fn construct(&mut self, trace: &mut Vec<f64>, tag: f64, args: &[&Tok]) -> f64 {
        trace.push(2.0);
        trace.push(tag);
        for a in args {
            (*a).enc(trace);
        }
        self.refid += 1.0;
        self.refid
    }

    /// `createExec(intent)`: the playerByClientID facade event, the `!player`
    /// warn + NoOp branch and the 24-case switch (arg extraction order per
    /// case, exactly as the TS source reads the fields). `None` = the default
    /// throw (the event is already recorded with `opk`).
    fn create_exec(
        &mut self,
        c: &mut Cur,
        players: &[(Tok, f64, Option<f64>)],
        opk: f64,
        trace: &mut Vec<f64>,
    ) -> Option<f64> {
        let typ = c.tok();
        let n = c.u();
        let mut fields: Vec<(Vec<u16>, Tok)> = Vec::with_capacity(n);
        for _ in 0..n {
            let k = c.enc_str();
            let v = c.tok();
            fields.push((k, v));
        }
        let get = |name: &str| -> Tok {
            for (k, v) in &fields {
                if String::from_utf16_lossy(k) == name {
                    return v.clone();
                }
            }
            Tok::Undef
        };
        let cid = get("clientID");
        // this.mg.playerByClientID(intent.clientID)
        let mut found: Option<(f64, Option<f64>)> = None;
        for (pc, pref, info) in players {
            if *pc == cid {
                found = Some((*pref, *info));
                break;
            }
        }
        trace.push(1.0);
        cid.enc(trace);
        match found {
            None => trace.push(0.0),
            Some((r, _)) => {
                trace.push(7.0);
                trace.push(r);
            }
        }
        let player = match found {
            None => {
                // console.warn(`player with clientID ${intent.clientID} not found`)
                trace.push(4.0);
                trace.push(5.0);
                let msg: Vec<u16> = "player with clientID "
                    .encode_utf16()
                    .chain(cid.to_str_units())
                    .chain(" not found".encode_utf16())
                    .collect();
                trace.push(msg.len() as f64);
                trace.extend(msg.iter().map(|&u| f64::from(u)));
                return Some(self.construct(trace, 0.0, &[]));
            }
            Some((r, _)) => Tok::Player(r),
        };
        let ty: String = match &typ {
            Tok::Str(s) => String::from_utf16_lossy(s),
            _ => String::new(),
        };
        let r = match ty.as_str() {
            "attack" => {
                let troops = get("troops");
                let target = get("targetID");
                self.construct(trace, 1.0, &[&troops, &player, &target, &Tok::Null])
            }
            "cancel_attack" => {
                let attack_id = get("attackID");
                self.construct(trace, 2.0, &[&player, &attack_id])
            }
            "cancel_boat" => {
                let unit_id = get("unitID");
                self.construct(trace, 3.0, &[&player, &unit_id])
            }
            "move_warship" => {
                let unit_ids = get("unitIds");
                let tile = get("tile");
                self.construct(trace, 4.0, &[&player, &unit_ids, &tile])
            }
            "spawn" => {
                // player.info() is a facade call: [3, pref, iref].
                let info = found.and_then(|(_, i)| i).unwrap_or(f64::NAN);
                trace.push(3.0);
                trace.push(player_ref(&player));
                trace.push(info);
                let tile = get("tile");
                self.construct(
                    trace,
                    5.0,
                    &[
                        &Tok::Str(self.game_id.clone()),
                        &Tok::Info(info),
                        &tile,
                        &Tok::True,
                    ],
                )
            }
            "boat" => {
                let dst = get("dst");
                let troops = get("troops");
                self.construct(trace, 6.0, &[&player, &dst, &troops])
            }
            "allianceRequest" => {
                let recipient = get("recipient");
                self.construct(trace, 7.0, &[&player, &recipient])
            }
            "allianceReject" => {
                let requestor = get("requestor");
                self.construct(trace, 8.0, &[&requestor, &player])
            }
            "breakAlliance" => {
                let recipient = get("recipient");
                self.construct(trace, 9.0, &[&player, &recipient])
            }
            "targetPlayer" => {
                let target = get("target");
                self.construct(trace, 10.0, &[&player, &target])
            }
            "emoji" => {
                let recipient = get("recipient");
                let emoji = get("emoji");
                self.construct(trace, 11.0, &[&player, &recipient, &emoji])
            }
            "donate_troops" => {
                let recipient = get("recipient");
                let troops = get("troops");
                self.construct(trace, 12.0, &[&player, &recipient, &troops])
            }
            "donate_gold" => {
                let recipient = get("recipient");
                let gold = get("gold");
                self.construct(trace, 13.0, &[&player, &recipient, &gold])
            }
            "embargo" => {
                let target = get("targetID");
                let action = get("action");
                self.construct(trace, 14.0, &[&player, &target, &action])
            }
            "embargo_all" => {
                let action = get("action");
                self.construct(trace, 15.0, &[&player, &action])
            }
            "build_unit" => {
                let unit = get("unit");
                let tile = get("tile");
                let rocket = get("rocketDirectionUp");
                let amount = get("amount");
                self.construct(trace, 16.0, &[&player, &unit, &tile, &rocket, &amount])
            }
            "allianceExtension" => {
                let recipient = get("recipient");
                self.construct(trace, 17.0, &[&player, &recipient])
            }
            "upgrade_structure" => {
                let unit_id = get("unitId");
                let amount = get("amount");
                self.construct(trace, 18.0, &[&player, &unit_id, &amount])
            }
            "delete_unit" => {
                let unit_id = get("unitId");
                self.construct(trace, 19.0, &[&player, &unit_id])
            }
            "quick_chat" => {
                let recipient = get("recipient");
                let key = get("quickChatKey");
                let target = get("target");
                self.construct(trace, 20.0, &[&player, &recipient, &key, &target])
            }
            "mark_disconnected" => {
                let disc = get("isDisconnected");
                self.construct(trace, 21.0, &[&player, &disc])
            }
            "toggle_pause" => {
                let paused = get("paused");
                self.construct(trace, 22.0, &[&player, &paused])
            }
            _ => {
                // throw new Error(`intent type ${intent} not found`) — the
                // intent is a plain object -> "[object Object]".
                trace.push(5.0);
                trace.push(opk);
                let msg: Vec<u16> = "intent type "
                    .encode_utf16()
                    .chain("[object Object]".encode_utf16())
                    .chain(" not found".encode_utf16())
                    .collect();
                trace.push(5.0);
                trace.push(msg.len() as f64);
                trace.extend(msg.iter().map(|&u| f64::from(u)));
                return None;
            }
        };
        Some(r)
    }

    /// `this.mg.nations()` facade event: [10, n, (ref)*].
    fn nations_event(nations: &[(Tok, f64)], trace: &mut Vec<f64>) {
        trace.push(10.0);
        trace.push(nations.len() as f64);
        for (_, r) in nations {
            trace.push(*r);
        }
    }

    /// `spawnTribes(numTribes)`: nations() -> map(spawnCell) -> filter(!==
    /// undefined) -> TribeSpawner ctor + spawnTribes call + scripted return.
    fn spawn_tribes(
        &mut self,
        c: &mut Cur,
        nations: &[(Tok, f64)],
        ret_lists: &[Vec<f64>],
        trace: &mut Vec<f64>,
    ) -> Vec<f64> {
        let num = c.tok();
        Self::nations_event(nations, trace);
        let cells: Vec<Tok> = nations
            .iter()
            .map(|(cell, _)| cell.clone())
            .filter(|cell| *cell != Tok::Undef)
            .collect();
        let cells_tok = Tok::Arr(cells);
        trace.push(6.0);
        Tok::Str(self.game_id.clone()).enc(trace);
        cells_tok.enc(trace);
        trace.push(7.0);
        num.enc(trace);
        self.purchased.enc(trace);
        let ret = ret_lists[self.ret_idx].clone();
        self.ret_idx += 1;
        trace.push(ret.len() as f64);
        trace.extend(ret.iter().copied());
        ret
    }

    /// `spawnPlayers()`: PlayerSpawner ctor + call + scripted return (no
    /// nations() call on this path).
    fn spawn_players(&mut self, ret_lists: &[Vec<f64>], trace: &mut Vec<f64>) -> Vec<f64> {
        trace.push(8.0);
        Tok::Str(self.game_id.clone()).enc(trace);
        trace.push(9.0);
        let ret = ret_lists[self.ret_idx].clone();
        self.ret_idx += 1;
        trace.push(ret.len() as f64);
        trace.extend(ret.iter().copied());
        ret
    }

    /// `nationExecutions()`: one NationExecution(gameID, nation) per nation in
    /// iteration order.
    fn nation_execs(&mut self, nations: &[(Tok, f64)], trace: &mut Vec<f64>) -> Vec<f64> {
        Self::nations_event(nations, trace);
        let mut refs = Vec::with_capacity(nations.len());
        for (_, r) in nations {
            refs.push(self.construct(
                trace,
                23.0,
                &[&Tok::Str(self.game_id.clone()), &Tok::Nation(*r)],
            ));
        }
        refs
    }
}

fn player_ref(p: &Tok) -> f64 {
    match p {
        Tok::Player(r) => *r,
        _ => f64::NAN,
    }
}

/// kind 0: replay one whole scripted Executor scenario. args:
/// `[0, gameIDEnc, clientIDEnc, purchasedEnc, nPlayers,(cidEnc,ref,infoEnc)*,
/// nNations,(spawnCellEnc,ref)*, nRet,(len,refs*)*, nOps,(op)*]` (op 0 ctor,
/// 1 createExecs, 2 createExec, 3 spawnTribes, 4 spawnPlayers, 5
/// nationExecs; enc table in the module docs). res:
/// `[traceLen,(trace)*,(opResult)*]` with opResult `[opKind,status,n,refs*]`
/// (status 1 = the op threw).
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut c = Cur(args, 0);
    let k = c.u() as u8;
    let mut trace: Vec<f64> = Vec::new();
    let mut payload: Vec<f64> = Vec::new();
    if k == kind {
        let game_id = c.enc_str();
        let _client_id = c.tok();
        let mut purchased = c.tok();
        if purchased == Tok::Undef {
            // The `= []` default parameter.
            purchased = Tok::Arr(Vec::new());
        }
        let np = c.u();
        let mut players: Vec<(Tok, f64, Option<f64>)> = Vec::with_capacity(np);
        for _ in 0..np {
            let cid = c.tok();
            let r = c.f();
            let info = match c.u() {
                8 => Some(c.f()),
                _ => None,
            };
            players.push((cid, r, info));
        }
        let nn = c.u();
        let mut nations: Vec<(Tok, f64)> = Vec::with_capacity(nn);
        for _ in 0..nn {
            let cell = c.tok();
            let r = c.f();
            nations.push((cell, r));
        }
        let nr = c.u();
        let mut ret_lists: Vec<Vec<f64>> = Vec::with_capacity(nr);
        for _ in 0..nr {
            let len = c.u();
            ret_lists.push((0..len).map(|_| c.f()).collect());
        }
        let nops = c.u();
        let mut ex = Exec {
            game_id: game_id.clone(),
            purchased,
            refid: 0.0,
            ret_idx: 0,
        };
        for _ in 0..nops {
            let opk = c.u();
            let mut status = 0.0;
            let mut refs: Vec<f64> = Vec::new();
            match opk {
                0 => {
                    // ctor: the real simpleHash + 1 seed feeding PseudoRandom
                    // (the instance is dead afterwards; the seed math is the
                    // observable construction side effect).
                    let h = simple_hash_units(&game_id);
                    let seed = h + 1.0;
                    let _prng = PseudoRandom::new(seed);
                    trace.push(0.0);
                    trace.push(h);
                    trace.push(seed);
                }
                1 => {
                    let n = c.u();
                    let mut thrown = false;
                    for _ in 0..n {
                        if thrown {
                            skip_intent(&mut c);
                            continue;
                        }
                        match ex.create_exec(&mut c, &players, 1.0, &mut trace) {
                            Some(r) => refs.push(r),
                            None => {
                                thrown = true;
                                status = 1.0;
                                refs.clear();
                            }
                        }
                    }
                }
                2 => {
                    status = match ex.create_exec(&mut c, &players, 2.0, &mut trace) {
                        Some(r) => {
                            refs.push(r);
                            0.0
                        }
                        None => 1.0,
                    };
                }
                3 => {
                    refs = ex.spawn_tribes(&mut c, &nations, &ret_lists, &mut trace);
                }
                4 => {
                    refs = ex.spawn_players(&ret_lists, &mut trace);
                }
                _ => {
                    refs = ex.nation_execs(&nations, &mut trace);
                }
            }
            payload.push(opk as f64);
            payload.push(status);
            payload.push(refs.len() as f64);
            payload.extend(refs.iter().copied());
        }
    }
    let mut out = Vec::with_capacity(1 + trace.len() + payload.len());
    out.push(trace.len() as f64);
    out.extend(trace.iter().copied());
    out.extend(payload.iter().copied());
    out
}
