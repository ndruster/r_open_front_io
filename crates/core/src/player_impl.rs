//! Bit-exact port of `src/core/game/PlayerImpl.ts` (G3c1 slice: the
//! identity / info / state / gold / troops / team / disconnect cluster).
//! The harness replicates every private field the slice touches;
//! construction and every method run as a `kind` of
//! [`PlayerHarness::run_op`] over the `js_json` codec (precedent
//! `unit_impl` / `attack_impl`). The `mg` (GameImpl) surface and the other
//! Players are scripted facades whose every call rides a flat trace, so the
//! facade call ORDER and the `===` reference-identity gates are compared,
//! not just the return values.
//!
//! Ported surface (TS line anchors): the ctor (191-201, class-field order:
//! `_troops = toInt(startTroops)` FIRST — a NaN startTroops throws before
//! facade 100 — then `_gold = mg.config().startingGold(playerInfo)`, then
//! `new PseudoRandom(simpleHash(playerInfo.id))` whose only observable
//! effect is the `str.length` TypeError on a null/undefined id); the
//! identity accessors (403-429, plain `playerInfo` field reads); `info` /
//! `isLobbyCreator` / `isAlive` / `hasSpawned` / `setSpawnTile` /
//! `spawnTile` (710-732); `isPlayer` (679); `numTilesOwned` / `tiles` /
//! `borderTiles` (569-579) over REAL ported `TileSet`s (the `ts_` vectors
//! prove the iteration order); the alliance gates `alliances` /
//! `isAlliedWith` / `allianceWith` (742-770); the team / gold / troops
//! cluster `team` / `isOnSameTeam` / `isFriendly` (1255-1280), `gold` /
//! `tradeGold` / `addTradeGold` / `trainGold` / `addTrainGold` /
//! `piracyGold` / `addPiracyGold` / `goldEarned` / `addGold` / `removeGold`
//! (1282-1339) and `troops` / `setTroops` / `addTroops` / `removeTroops`
//! (1341-1359); `tileChangeVersion` / `lastTileChange` (1788-1794) and the
//! disconnect cluster `isDisconnected` / `markDisconnected` /
//! `disconnectSnapshot` / `disconnectedAtTick` (1796-1826). G3c2 adds the
//! `expiredAlliances` (746) / `allies` (750) reads and the traitor / doomsday
//! cluster `isTraitor` / `getTraitorRemainingTicks` / `markTraitor` /
//! `betrayals` (851-911) and `inDoomsdayClock` / `doomsdayClockTicks` /
//! `enterDoomsdayClock` / `clearDoomsdayClock` / `markRotted` / `isDecaying`
//! (874-907) over the `markedTraitorTick` / `markedDoomsdayClockTick` /
//! `rottedAtTick` / `_betrayalCount` fields and the module-level
//! `DECAY_CUE_GRACE_TICKS = 30` (71). G3c3 adds the player-to-player
//! social surface: `incomingAllianceRequests` / `outgoingAllianceRequests`
//! (734-740, over the mg `allianceRequests` property array),
//! `allianceInfo` / `canSendAllianceRequest` (772-841), `breakAlliance` /
//! `removeAllAlliances` / `createAllianceRequest` (843-849, 913-918),
//! `relation` / `allRelationsSorted` / `updateRelation` / `decayRelations`
//! (920-970), `canTarget` / `target` / `targets` / `transitiveTargets`
//! (972-1006), `sendEmoji` / `outgoingEmojis` / `canSendEmoji` (1008-1050),
//! `canSendQuickChat` / `recordQuickChat` (1052-1065), the donations
//! cluster `canDonateGold` / `canDonateTroops` / `donateTroops` /
//! `donateGold` (1067-1165), `canDeleteUnit` / `recordDeleteUnit`
//! (1167-1176) and the embargo cluster `canEmbargoAll` /
//! `recordEmbargoAll` / `hasEmbargoAgainst` / `canTrade` / `getEmbargoes` /
//! `addEmbargo` / `stopEmbargo` / `endTemporaryEmbargo` /
//! `tradingPartners` (1178-1253) over the `relations` / `targets_` /
//! `outgoingEmojis_` / `outgoingQuickChats_` / `sentDonations` /
//! `embargoes` / `lastDeleteUnitTick` / `lastEmbargoAllTick` fields and
//! the scripted `pastOutgoingAllianceRequests` constant.
//!
//! G3c3 facade surface (event segment 109-130, all scripted):
//!
//! * 109-121 `mg.config().X()` — 109 allianceExtensionPromptOffset, 110
//!   disableAlliances (codec value, truthy gate), 111
//!   allianceRequestCooldown, 112 targetCooldown, 113 targetDuration, 114
//!   emojiMessageDuration, 115 emojiMessageCooldown, 116 quickChatCooldown,
//!   117 donateGold (codec value, `=== false` gate), 118 donateTroops
//!   (same), 119 donateCooldown, 120 deleteUnitCooldown, 121
//!   embargoAllCooldown. The f64 scripts trace `[code, ret]`, the codec
//!   scripts `[code, ...encVal(v)]`.
//! * 122 `mg.createAllianceRequest(this, recipient)` -> `[122, tok,
//!   ...encVal(ret)]` (the return is scripted: null or a request token).
//! * 123 `mg.breakAlliance(this, alliance)` -> `[123, alIdx]`.
//! * 124 `mg.removeAlliancesByPlayerSilently(this)` -> `[124]`.
//! * 125 `mg.target(this, other)` -> `[125, otherPid]`.
//! * 126 `mg.sendEmojiUpdate(msg)` -> `[126, ...encVal(msg)]` (key order
//!   message/senderID/recipientID/createdAt).
//! * 127 `other.isAlive()` -> `[127, pid, ret01]` (`!!v` normalised).
//! * 128 `other.addTroops(n)` -> `[128, pid, n]`.
//! * 129 `other.addGold(n)` -> `[129, pid, n]` (n is the removed bigint,
//!   crossing as a number).
//! * 130 `mg.players()` -> `[130, n, (tok)*]` (the scripted token list).
//! * `other.smallID()` / `other.id()` / `other.type()` /
//!   `other.hasEmbargoAgainst(this)` / `other.targets()` and the alliance
//!   mocks' `expiresAt()` / `agreedToExtend()` are untraced constant reads.
//!
//! Facade (mock) surface — the capture scripts these, every call traced
//! (G3c1 event segment 100-106):
//!
//! * 100 `mg.config().startingGold(playerInfo)` -> `[100, ...encVal(gold)]`
//!   (the scripted f64 crosses `BigInt(v)` FIRST — a non-integer script
//!   value throws before the trace push, pinned by construction order).
//! * 101 `mg.addUpdate(u)` -> `[101, ...encVal(u)]` (full codec dump of the
//!   BonusEvent update, key order type/player/tile/gold/troops).
//! * 102 `mg.ticks()` -> `[102, ret]`.
//! * 103 `mg.teamTilesOwned(team)` -> `[103, ...encVal(team), ret]`.
//! * 104 `mg.totalLandTiles()` -> `[104, ret]`.
//! * 105 `other.isDisconnected()` -> `[105, pid, ret01]` (the mock's
//!   `!!v` normalisation).
//! * 106 `other.team()` -> `[106, pid, ...encVal(v)]`.
//! * 107 `mg.stats().betray(this)` -> `[107, smallID]` (the capture pushes
//!   `this.smallID()`).
//! * 108 `mg.config().traitorDuration()` -> `[108, ret]`.
//! * Players cross as TOKENS (indices into the harness players table);
//!   `selfTok >= 0` puts the object under test into a table slot so the
//!   `other === this` identity gates fire naturally (the `ui_`/`ak_`
//!   identity model extended). `ColoredTeams.Bot === "Bot"` (Game.ts:110).
//! * `GameUpdateType.BonusEvent === 15` (zero-based declaration order).
//!
//! Faithfulness notes:
//!
//! * bigint domain: gold / troops are i64, crossing as f64 in the
//!   |v| <= 2^53 domain. `toInt` = ±Infinity clamp to MAX_SAFE then floor
//!   (only NaN throws); `BigInt(x)` over the raw `big()`-wrapped arg has NO
//!   clamp and NO floor (7.9 throws). The V8 throw matrix: non-integer
//!   number -> "The number X cannot be converted to a BigInt because it is
//!   not an integer"; `bigint OP non-bigint` -> "Cannot mix BigInt and
//!   other types, use explicit conversions"; cross-type RELATIONAL
//!   comparisons (`toRemove <= 0n`, `a < b` in minInt) convert the non-
//!   bigint side with ToNumber and NEVER throw — so `1n <= null` is true,
//!   `1n <= "x"` is false, and `minInt` can hand back the RAW non-bigint
//!   which then throws on the subtraction.
//! * `addGold`: `_gold +=` then `_goldEarned +=` (both before the tile
//!   gate); `if (tile)` is JS truthiness (0 / NaN / "" / null / undefined
//!   all skip the update); `gold: Number(toAdd)` converts the bigint back.
//! * `isOnSameTeam`: self -> false; `this.team() === null` short-circuits
//!   BEFORE the first `other.team()` facade; the Bot gate checks `this`
//!   side first (no facade); the final `===` fires the THIRD other-team
//!   facade — the call count is pinned by the trace (FIFO scripts repeat
//!   the last value when exhausted).
//! * `isFriendly(other, treatAFKFriendly = false)`: self -> true; the
//!   `other.isDisconnected() && !treatAFKFriendly` gate; then same-team OR
//!   allied (the alliance mocks' `requestor()`/`recipient()` are plain
//!   untraced identity reads).
//! * `markDisconnected`: the flag stores the RAW arg; on truthy + a null
//!   snapshot, `snapshot ?? {built}` — a non-nullish arg snapshot skips
//!   ALL facades; the built object evaluates left-to-right 102 -> team
//!   truthy gate -> 103 -> 104 -> `isAlive()` (untraced); on falsy the
//!   snapshot clears. `disconnectedAtTick` = `snapshot?.currentTick ?? null`.
//! * `_tiles` / `_borderTiles` are REAL ported `TileSet`s; mids 42/43/45/46
//!   call `add`/`delete` directly (bypassing `conquer`, so
//!   `tileChangeVersion` stays 0 — the capture pins that).
//! * A failed construct leaves the JS harness `pl === null`: every later
//!   method op throws "Cannot read properties of null (reading 'X')" with
//!   an empty trace (property access precedes any facade call).

use crate::js_json::{push_str, push_val, read_str, read_val, val_field, JsVal};
use crate::tile_set::TileSet;

const MAX_SAFE: i64 = 9_007_199_254_740_991;
const MIX_ERR: &str = "Cannot mix BigInt and other types, use explicit conversions";
/// `DECAY_CUE_GRACE_TICKS` (PlayerImpl.ts:71).
const DECAY_CUE_GRACE_TICKS: f64 = 30.0;

/// FIFO script consumption with the capture's exhausted-repeat rule (the
/// `ui_` rule; event codes 100-106 are documented above).
fn take_f64(list: &[f64], i: &mut usize) -> f64 {
    let idx = (*i).min(list.len() - 1);
    *i += 1;
    list[idx]
}

/// Same rule for the team script; an exhausted-to-empty edge reads JS
/// `arr[-1]` -> `undefined` (never hit by the scenarios, kept total).
fn take_jsval(list: &[JsVal], i: &mut usize) -> JsVal {
    if list.is_empty() {
        *i += 1;
        return JsVal::Undef;
    }
    let idx = (*i).min(list.len() - 1);
    *i += 1;
    list[idx].clone()
}

/// JS `ToNumber` over the codec domain (the `unit_impl` helper).
fn to_num(v: &JsVal) -> f64 {
    match v {
        JsVal::Num(n) => *n,
        JsVal::Bool(b) => if *b { 1.0 } else { 0.0 },
        JsVal::Null => 0.0,
        JsVal::Str(s) => crate::game_config_helpers::js_number(s),
        JsVal::Absent | JsVal::Undef => f64::NAN,
        JsVal::Obj(_) | JsVal::Arr(_) => f64::NAN,
    }
}

/// JS truthiness over the codec domain (the `debug_gui` model).
fn js_truthy(v: &JsVal) -> bool {
    match v {
        JsVal::Absent | JsVal::Undef | JsVal::Null => false,
        JsVal::Bool(b) => *b,
        JsVal::Num(n) => !n.is_nan() && *n != 0.0,
        JsVal::Str(s) => !s.is_empty(),
        _ => true,
    }
}

/// `v == null` (the `??` nullish gate).
fn nullish(v: &JsVal) -> bool {
    matches!(v, JsVal::Absent | JsVal::Undef | JsVal::Null)
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
    fn has(&self) -> bool {
        self.1 < self.0.len()
    }
    fn list(&mut self) -> Vec<f64> {
        let n = self.u();
        (0..n).map(|_| self.f()).collect()
    }
    fn val(&mut self) -> JsVal {
        read_val(self.0, &mut self.1)
    }
    fn s(&mut self) -> String {
        read_str(self.0, &mut self.1)
    }
    /// `[n, (encVal)*n]` — the JsVal script blocks (da / dg / dt / car, emb).
    fn val_list(&mut self) -> Vec<JsVal> {
        let n = self.u();
        (0..n).map(|_| self.val()).collect()
    }
    /// The team script block `[n, (0 = null | 1 = string)*n]`.
    fn team_list(&mut self) -> Vec<JsVal> {
        let n = self.u();
        (0..n)
            .map(|_| {
                if self.f() == 0.0 {
                    JsVal::Null
                } else {
                    JsVal::Str(read_str(self.0, &mut self.1))
                }
            })
            .collect()
    }
}

/// `BigInt(x)` for a number `x` — `Err(V8 message)` on the throw matrix
/// (no clamp, no floor — the `big()`-wrapped gold args).
fn bigint_from_num(x: f64) -> Result<i64, String> {
    if x.is_nan() || x.is_infinite() || x != x.floor() {
        return Err(format!(
            "The number {} cannot be converted to a BigInt because it is not an integer",
            crate::js_fixed::js_to_string(x)
        ));
    }
    Ok(x as i64)
}

/// `toInt(num)` (Util.ts:397) — ±Infinity clamp BEFORE the floor; only NaN
/// throws.
fn to_int_big(x: f64) -> Result<i64, String> {
    if x == f64::INFINITY {
        return Ok(MAX_SAFE);
    }
    if x == f64::NEG_INFINITY {
        return Ok(-MAX_SAFE);
    }
    bigint_from_num(x.floor())
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

/// Throw payload `[1, ...encS(msg)]`.
fn push_err(out: &mut Vec<f64>, msg: &str) {
    out.push(1.0);
    push_str(out, msg);
}

/// The property name in the `pl === null` TypeError for each mid (mirrors
/// the capture's `pl.X(...)` access; mids 42/43 read `_tiles`, 45/46 read
/// `_borderTiles`).
fn mid_property(mid: u8) -> &'static str {
    match mid {
        0 => "smallID",
        1 => "name",
        2 => "displayName",
        3 => "clanTag",
        4 => "nationFlag",
        5 => "clientID",
        6 => "id",
        7 => "type",
        8 => "info",
        9 => "isLobbyCreator",
        10 => "isAlive",
        11 => "hasSpawned",
        12 => "setSpawnTile",
        13 => "spawnTile",
        14 => "troops",
        15 => "setTroops",
        16 => "addTroops",
        17 => "removeTroops",
        18 => "gold",
        19 => "tradeGold",
        20 => "addTradeGold",
        21 => "trainGold",
        22 => "addTrainGold",
        23 => "piracyGold",
        24 => "addPiracyGold",
        25 => "goldEarned",
        26 => "addGold",
        27 => "removeGold",
        28 => "tileChangeVersion",
        29 => "lastTileChange",
        30 => "isDisconnected",
        31 => "markDisconnected",
        32 => "disconnectSnapshot",
        33 => "disconnectedAtTick",
        34 => "team",
        35 => "isOnSameTeam",
        36 => "isFriendly",
        37 => "isAlliedWith",
        38 => "allianceWith",
        39 => "alliances",
        40 => "numTilesOwned",
        41 => "tiles",
        42 | 43 => "_tiles",
        44 => "borderTiles",
        45 | 46 => "_borderTiles",
        47 => "isPlayer",
        48 => "expiredAlliances",
        49 => "allies",
        50 => "isTraitor",
        51 => "getTraitorRemainingTicks",
        52 => "markTraitor",
        53 => "inDoomsdayClock",
        54 => "doomsdayClockTicks",
        55 => "enterDoomsdayClock",
        56 => "clearDoomsdayClock",
        57 => "markRotted",
        58 => "isDecaying",
        59 => "betrayals",
        // G3c3: the player-to-player social surface.
        60 => "incomingAllianceRequests",
        61 => "outgoingAllianceRequests",
        62 => "allianceInfo",
        63 => "canSendAllianceRequest",
        64 => "breakAlliance",
        65 => "removeAllAlliances",
        66 => "createAllianceRequest",
        67 => "relation",
        68 => "allRelationsSorted",
        69 => "updateRelation",
        70 => "decayRelations",
        71 => "canTarget",
        72 => "target",
        73 => "targets",
        74 => "transitiveTargets",
        75 => "sendEmoji",
        76 => "outgoingEmojis",
        77 => "canSendEmoji",
        78 => "canSendQuickChat",
        79 => "recordQuickChat",
        80 => "canDonateGold",
        81 => "canDonateTroops",
        82 => "donateTroops",
        83 => "donateGold",
        84 => "canDeleteUnit",
        85 => "recordDeleteUnit",
        86 => "canEmbargoAll",
        87 => "recordEmbargoAll",
        88 => "hasEmbargoAgainst",
        89 => "canTrade",
        90 => "getEmbargoes",
        91 => "addEmbargo",
        92 => "stopEmbargo",
        93 => "endTemporaryEmbargo",
        94 => "tradingPartners",
        _ => "unknown",
    }
}

/// One scripted player mock: pid, the `team()` script (codec values —
/// `Null` / `Str`) and the `isDisconnected()` script (f64, `!!`-normalised
/// into the trace). G3c3 adds the constant `type()` string, the scripted
/// `isAlive()` FIFO (event 127), the constant embargo id list
/// (`hasEmbargoAgainst`, untraced) and the constant `targets()` token list
/// (untraced).
#[derive(Clone, Debug)]
struct PlPlayer {
    pid: f64,
    teams: Vec<JsVal>,
    team_i: usize,
    discs: Vec<f64>,
    disc_i: usize,
    typ: String,
    alives: Vec<f64>,
    alive_i: usize,
    emb: Vec<JsVal>,
    tgts: Vec<usize>,
}

/// The bigint-or-string accumulator domain. The gold fields are typed
/// `bigint` in TS, but JS `+=` follows the `+` operator: `bigint + string`
/// is STRING concatenation (not a mix TypeError), so an accumulator that
/// receives a string operand becomes a `Str` and stays one. Only the three
/// `add*Gold` methods ever receive non-number operands in the capture (the
/// `big()` wrapper turns numbers into `BigInt` — throwing on non-integers —
/// and passes everything else through raw).
#[derive(Clone, Debug, PartialEq)]
enum Acc {
    Big(i64),
    Str(String),
}

impl Acc {
    /// The codec value the getter returns: a bigint crosses as a number
    /// (`plEnc` → `akEnc(Number(v))`), a string as a string.
    fn to_jsval(&self) -> JsVal {
        match self {
            Acc::Big(b) => JsVal::Num(*b as f64),
            Acc::Str(s) => JsVal::Str(s.clone()),
        }
    }
}

/// The operand after the capture's `big()` wrapper: numbers are already
/// `BigInt`-converted (`Op::Big`, throwing at conversion), everything else
/// crosses raw.
enum Op {
    Big(i64),
    Str(String),
    Null,
    Bool(bool),
    Undef,
    Obj,
}

/// `Number(op)` / the ToNumber used by the cross-type relational comparisons.
fn op_to_num(o: &Op) -> f64 {
    match o {
        Op::Big(b) => *b as f64,
        Op::Str(s) => crate::game_config_helpers::js_number(s),
        Op::Null => 0.0,
        Op::Bool(b) => if *b { 1.0 } else { 0.0 },
        Op::Undef | Op::Obj => f64::NAN,
    }
}

/// `String(op)` for the string-concatenation branch of `+`.
fn op_to_string(o: &Op) -> String {
    match o {
        Op::Big(b) => b.to_string(),
        Op::Str(s) => s.clone(),
        Op::Null => "null".to_string(),
        Op::Bool(true) => "true".to_string(),
        Op::Bool(false) => "false".to_string(),
        Op::Undef => "undefined".to_string(),
        Op::Obj => "[object Object]".to_string(),
    }
}

/// `acc + op` over the JS `+` semantics (the `+=` body). A `Big` accumulator
/// plus a non-`Big`/non-`Str` operand throws the mix TypeError; a `Str`
/// accumulator concatenates whatever the operand stringifies to.
fn acc_plus(f: &Acc, o: &Op) -> Result<Acc, String> {
    match f {
        Acc::Big(a) => match o {
            Op::Big(b) => Ok(Acc::Big(a + b)),
            Op::Str(s) => Ok(Acc::Str(format!("{}{}", a, s))),
            _ => Err(MIX_ERR.to_string()),
        },
        Acc::Str(s) => Ok(Acc::Str(format!("{}{}", s, op_to_string(o)))),
    }
}

/// The capture's `big()` wrapper as an operand: a number crosses
/// `BigInt(v)` (throwing on a non-integer BEFORE the method runs, so the
/// field is untouched), every other codec type passes through raw.
fn big_operand(v: &JsVal) -> Result<Op, String> {
    match v {
        JsVal::Num(n) => bigint_from_num(*n).map(Op::Big),
        JsVal::Str(s) => Ok(Op::Str(s.clone())),
        JsVal::Null => Ok(Op::Null),
        JsVal::Bool(b) => Ok(Op::Bool(*b)),
        JsVal::Absent | JsVal::Undef => Ok(Op::Undef),
        JsVal::Obj(_) | JsVal::Arr(_) => Ok(Op::Obj),
    }
}

// ---- G3c3 helpers ----

/// JS `===` over the codec domain (`SameValueZero` semantics for the
/// scalar arm: NaN !== NaN, -0 === +0, cross-type always false). Obj/Arr
/// compare by identity in JS; the social cluster never ===-compares them,
/// so they simply never match here.
fn js_strict_eq(a: &JsVal, b: &JsVal) -> bool {
    match (a, b) {
        (JsVal::Num(x), JsVal::Num(y)) => x == y,
        (JsVal::Str(x), JsVal::Str(y)) => x == y,
        (JsVal::Bool(x), JsVal::Bool(y)) => x == y,
        (JsVal::Null, JsVal::Null) => true,
        (JsVal::Undef, JsVal::Undef) | (JsVal::Absent, JsVal::Absent) => true,
        _ => false,
    }
}

/// JS `String(v)` (the template-literal interpolation): Arr ToPrimitives
/// through `join(",")` (null/undefined elements -> "").
fn js_to_string_val(v: &JsVal) -> String {
    match v {
        JsVal::Str(s) => s.clone(),
        JsVal::Num(n) => crate::js_fixed::js_to_string(*n),
        JsVal::Bool(b) => {
            if *b {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        JsVal::Null => "null".to_string(),
        JsVal::Absent | JsVal::Undef => "undefined".to_string(),
        JsVal::Obj(_) => "[object Object]".to_string(),
        JsVal::Arr(items) => items
            .iter()
            .map(|x| match x {
                JsVal::Null | JsVal::Absent | JsVal::Undef => String::new(),
                other => js_to_string_val(other),
            })
            .collect::<Vec<_>>()
            .join(","),
    }
}

/// JS `Math.sign`: NaN -> NaN, ±0 preserved.
fn js_sign(v: f64) -> f64 {
    if v.is_nan() {
        f64::NAN
    } else if v > 0.0 {
        1.0
    } else if v < 0.0 {
        -1.0
    } else {
        v
    }
}

/// `relationFromValue` (PlayerImpl.ts:928): the Relation enum
/// (Game.ts:327) — Hostile 0 / Distrustful 1 / Neutral 2 / Friendly 3; a
/// NaN value fails all three `<` gates and lands on Friendly.
fn relation_from_value(v: f64) -> f64 {
    if v < -50.0 {
        0.0
    } else if v < 0.0 {
        1.0
    } else if v < 50.0 {
        2.0
    } else {
        3.0
    }
}

/// A scripted f64 facade read: FIFO with the exhausted-repeat rule, then
/// `[code, ret]` on the trace (the 102/109/111-116/119-121 pattern).
fn script_f64(list: &[f64], i: &mut usize, tr: &mut Vec<f64>, code: f64) -> f64 {
    let v = take_f64(list, i);
    tr.push(code);
    tr.push(v);
    v
}

/// Same for a codec-value script (`[code, ...encVal(v)]` — 110/117/118).
fn script_val(list: &[JsVal], i: &mut usize, tr: &mut Vec<f64>, code: f64) -> JsVal {
    let v = take_jsval(list, i);
    tr.push(code);
    push_val(tr, &v);
    v
}

/// V8 `Array.prototype.sort` with a subtraction comparator: stable, and a
/// NaN comparator result is treated as `+0` (Equal) — the
/// `outgoingEmojis` / past-request descending ordering rule.
fn js_sort_desc(pairs: &mut [(usize, f64)]) {
    pairs.sort_by(|a, b| {
        let d = b.1 - a.1;
        if d.is_nan() || d == 0.0 {
            std::cmp::Ordering::Equal
        } else if d < 0.0 {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        }
    });
}

/// Same rule ascending (`a.relation - b.relation`) — `allRelationsSorted`.
fn js_sort_asc(pairs: &mut [(usize, f64)]) {
    pairs.sort_by(|a, b| {
        let d = a.1 - b.1;
        if d.is_nan() || d == 0.0 {
            std::cmp::Ordering::Equal
        } else if d < 0.0 {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        }
    });
}

/// One alliance mock: the (req, rec) token pair plus the G3c3
/// `expiresAt()` / `agreedToExtend()` constants (`req_agree` is
/// `agreedToExtend(requestor())`; `agreedToExtend(p)` is
/// `p === requestor() ? reqAgree : recAgree`).
#[derive(Clone, Copy, Debug)]
struct AllianceMock {
    req: usize,
    rec: usize,
    expires_at: f64,
    req_agree: bool,
    rec_agree: bool,
}

/// One `embargoes` entry (PlayerImpl.ts:1225-1229 key order); the map is
/// keyed by `other.id()` (a JsVal — mock pids cross as `Num`) and keeps JS
/// insertion order.
#[derive(Clone, Debug)]
struct Embargo {
    created_at: f64,
    is_temporary: JsVal,
    target: usize,
}

/// The `PlayerImpl` field block the G3c1 slice touches, replicated 1:1.
struct Player {
    small_id: f64,
    player_info: JsVal,
    team: JsVal,
    troops: i64,
    gold: Acc,
    trade_gold: Acc,
    train_gold: Acc,
    piracy_gold: Acc,
    gold_earned: Acc,
    tiles: TileSet,
    border_tiles: TileSet,
    alliances: Vec<AllianceMock>,
    expired_alliances: Vec<(usize, usize)>,
    marked_traitor_tick: f64,
    marked_doomsday_clock_tick: f64,
    rotted_at_tick: f64,
    betrayal_count: f64,
    spawn_tile: JsVal,
    is_disconnected: JsVal,
    disconnect_snapshot: JsVal,
    tile_change_version: f64,
    last_tile_change: f64,
    // G3c3: the player-to-player social surface. `relations` /
    // `quick_chats` / `embargoes` are JS Maps (insertion-ordered, set
    // updates in place); `targets_` is `{tick, targetTok}`;
    // `outgoing_emojis` stores the full EmojiMessage objects (the
    // message/senderID/recipientID/createdAt key order is observable).
    relations: Vec<(usize, f64)>,
    targets_: Vec<(f64, usize)>,
    outgoing_emojis: Vec<JsVal>,
    quick_chats: Vec<(f64, f64)>,
    sent_donations: Vec<(usize, f64)>,
    embargoes: Vec<(JsVal, Embargo)>,
    last_delete_unit_tick: f64,
    last_embargo_all_tick: f64,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            small_id: 0.0,
            player_info: JsVal::Absent,
            team: JsVal::Null,
            troops: 0,
            gold: Acc::Big(0),
            trade_gold: Acc::Big(0),
            train_gold: Acc::Big(0),
            piracy_gold: Acc::Big(0),
            gold_earned: Acc::Big(0),
            tiles: TileSet::new(None),
            border_tiles: TileSet::new(None),
            alliances: Vec::new(),
            expired_alliances: Vec::new(),
            marked_traitor_tick: -1.0,
            marked_doomsday_clock_tick: -1.0,
            rotted_at_tick: -1.0,
            betrayal_count: 0.0,
            spawn_tile: JsVal::Undef,
            is_disconnected: JsVal::Bool(false),
            disconnect_snapshot: JsVal::Null,
            tile_change_version: 0.0,
            last_tile_change: 0.0,
            relations: Vec::new(),
            targets_: Vec::new(),
            outgoing_emojis: Vec::new(),
            quick_chats: Vec::new(),
            sent_donations: Vec::new(),
            embargoes: Vec::new(),
            last_delete_unit_tick: -1.0,
            last_embargo_all_tick: -1.0,
        }
    }
}

#[derive(Default)]
pub struct PlayerHarness {
    constructed: bool,
    p: Player,
    players: Vec<PlPlayer>,
    self_tok: Option<usize>,
    sg: Vec<f64>,
    sg_i: usize,
    ticks: Vec<f64>,
    ticks_i: usize,
    team_tiles: Vec<f64>,
    team_tiles_i: usize,
    total_land: Vec<f64>,
    total_land_i: usize,
    traitor_duration: Vec<f64>,
    traitor_duration_i: usize,
    // G3c3 mg surface: `allianceRequests` (property array of (req, rec)
    // token pairs), `players()` (scripted token list), the config FIFO
    // scripts and the `createAllianceRequest` return FIFO.
    alliance_requests: Vec<(usize, usize)>,
    past_outgoing: Vec<(usize, f64)>,
    players_list: Vec<usize>,
    off: Vec<f64>,
    off_i: usize,
    da: Vec<JsVal>,
    da_i: usize,
    arc: Vec<f64>,
    arc_i: usize,
    tc: Vec<f64>,
    tc_i: usize,
    tdur: Vec<f64>,
    tdur_i: usize,
    emd: Vec<f64>,
    emd_i: usize,
    emc: Vec<f64>,
    emc_i: usize,
    qc: Vec<f64>,
    qc_i: usize,
    dg: Vec<JsVal>,
    dg_i: usize,
    dt: Vec<JsVal>,
    dt_i: usize,
    dc: Vec<f64>,
    dc_i: usize,
    duc: Vec<f64>,
    duc_i: usize,
    eac: Vec<f64>,
    eac_i: usize,
    car: Vec<JsVal>,
    car_i: usize,
}

impl PlayerHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    // --- facade plumbing ---

    fn mg_starting_gold(&mut self, tr: &mut Vec<f64>) -> Result<i64, String> {
        let v = take_f64(&self.sg, &mut self.sg_i);
        // JS: `BigInt(v)` throws BEFORE the trace push.
        let b = bigint_from_num(v)?;
        tr.push(100.0);
        push_val(tr, &JsVal::Num(b as f64));
        Ok(b)
    }

    fn mg_ticks(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = take_f64(&self.ticks, &mut self.ticks_i);
        tr.push(102.0);
        tr.push(v);
        v
    }

    fn mg_team_tiles_owned(&mut self, tr: &mut Vec<f64>, team: &JsVal) -> f64 {
        let v = take_f64(&self.team_tiles, &mut self.team_tiles_i);
        tr.push(103.0);
        push_val(tr, team);
        tr.push(v);
        v
    }

    fn mg_total_land(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = take_f64(&self.total_land, &mut self.total_land_i);
        tr.push(104.0);
        tr.push(v);
        v
    }

    fn mg_add_update(&mut self, tr: &mut Vec<f64>, u: &JsVal) {
        tr.push(101.0);
        push_val(tr, u);
    }

    fn mg_traitor_duration(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = take_f64(&self.traitor_duration, &mut self.traitor_duration_i);
        tr.push(108.0);
        tr.push(v);
        v
    }

    /// `mg.stats().betray(this)` — the capture pushes `this.smallID()` into
    /// the trace (event 107).
    fn mg_betray(&mut self, tr: &mut Vec<f64>) {
        tr.push(107.0);
        tr.push(self.p.small_id);
    }

    /// `other.team()` — the mock facade (106); the self slot reads the REAL
    /// field (untraced), mirroring `mocks[selfTok] = pl`.
    fn other_team(&mut self, tr: &mut Vec<f64>, tok: usize) -> Result<JsVal, String> {
        if self.self_tok == Some(tok) {
            return Ok(self.p.team.clone());
        }
        let p = self
            .players
            .get_mut(tok)
            .ok_or_else(|| "Cannot read properties of undefined (reading 'team')".to_string())?;
        let v = take_jsval(&p.teams, &mut p.team_i);
        tr.push(106.0);
        tr.push(p.pid);
        push_val(tr, &v);
        Ok(v)
    }

    /// `other.isDisconnected()` — the mock facade (105) with the `!!v`
    /// normalisation the capture pushes.
    fn other_is_disconnected(&mut self, tr: &mut Vec<f64>, tok: usize) -> Result<bool, String> {
        if self.self_tok == Some(tok) {
            return Ok(js_truthy(&self.p.is_disconnected));
        }
        let p = self.players.get_mut(tok).ok_or_else(|| {
            "Cannot read properties of undefined (reading 'isDisconnected')".to_string()
        })?;
        let v = take_f64(&p.discs, &mut p.disc_i);
        let b = v != 0.0 && !v.is_nan();
        tr.push(105.0);
        tr.push(p.pid);
        tr.push(if b { 1.0 } else { 0.0 });
        Ok(b)
    }

    // --- G3c3 facade plumbing ---

    /// `other.isAlive()` — the mock facade (127) with the `!!v`
    /// normalisation; the self slot reads the REAL tile count (untraced),
    /// mirroring `mocks[selfTok] = pl`.
    fn other_is_alive(&mut self, tr: &mut Vec<f64>, tok: usize) -> Result<bool, String> {
        if self.self_tok == Some(tok) {
            return Ok(self.is_alive());
        }
        let p = self.players.get_mut(tok).ok_or_else(|| {
            "Cannot read properties of undefined (reading 'isAlive')".to_string()
        })?;
        let v = take_f64(&p.alives, &mut p.alive_i);
        let b = v != 0.0 && !v.is_nan();
        tr.push(127.0);
        tr.push(p.pid);
        tr.push(if b { 1.0 } else { 0.0 });
        Ok(b)
    }

    /// `other.smallID()` — the mock's constant pid (untraced); the self
    /// slot reads the real field.
    fn other_small_id(&self, tok: usize) -> Result<f64, String> {
        if self.self_tok == Some(tok) {
            return Ok(self.p.small_id);
        }
        self.players
            .get(tok)
            .map(|p| p.pid)
            .ok_or_else(|| "Cannot read properties of undefined (reading 'smallID')".to_string())
    }

    /// `other.id()` — the mock's constant pid as `Num` (untraced); the self
    /// slot reads `playerInfo.id`.
    fn other_id(&self, tok: usize) -> JsVal {
        if self.self_tok == Some(tok) {
            return self.pi_field("id");
        }
        match self.players.get(tok) {
            Some(p) => JsVal::Num(p.pid),
            None => JsVal::Undef,
        }
    }

    /// `other.type()` — the mock's constant type string (untraced); the self
    /// slot reads `playerInfo.playerType`.
    fn other_type(&self, tok: usize) -> JsVal {
        if self.self_tok == Some(tok) {
            return self.pi_field("playerType");
        }
        match self.players.get(tok) {
            Some(p) => JsVal::Str(p.typ.clone()),
            None => JsVal::Undef,
        }
    }

    /// `other.hasEmbargoAgainst(this)` — the mock's constant embargo id
    /// list (untraced); the self slot runs the REAL `hasEmbargoAgainst`.
    fn other_has_embargo_against(&self, tok: usize) -> bool {
        let my_id = self.pi_field("id");
        if self.self_tok == Some(tok) {
            return self.has_embargo_id(&my_id);
        }
        match self.players.get(tok) {
            Some(p) => p.emb.iter().any(|e| js_strict_eq(e, &my_id)),
            None => false,
        }
    }

    /// `other.targets()` — the mock's constant token list (untraced); the
    /// self slot is handled by the caller (the real `targets()` body needs
    /// the facade trace).
    fn other_targets(&self, tok: usize) -> Vec<usize> {
        match self.players.get(tok) {
            Some(p) => p.tgts.clone(),
            None => Vec::new(),
        }
    }

    /// `other.addTroops(n)` / `other.addGold(n)` — the traced mock writes
    /// (128 / 129); the self slot runs the REAL method (unreachable from
    /// the `donate*` self-gates, kept total).
    fn other_add_troops(&mut self, tr: &mut Vec<f64>, tok: usize, n: f64) -> Result<(), String> {
        if self.self_tok == Some(tok) {
            if n < 0.0 {
                self.remove_troops(-n)?;
            } else {
                self.p.troops += to_int_big(n)?;
            }
            return Ok(());
        }
        let pid = self
            .players
            .get(tok)
            .map(|p| p.pid)
            .ok_or_else(|| "Cannot read properties of undefined (reading 'addTroops')".to_string())?;
        tr.push(128.0);
        tr.push(pid);
        push_val(tr, &JsVal::Num(n));
        Ok(())
    }

    fn other_add_gold(&mut self, tr: &mut Vec<f64>, tok: usize, n: i64) -> Result<(), String> {
        if self.self_tok == Some(tok) {
            let o = Op::Big(n);
            if let Ok(v) = acc_plus(&self.p.gold, &o) {
                self.p.gold = v;
            }
            return Ok(());
        }
        let pid = self
            .players
            .get(tok)
            .map(|p| p.pid)
            .ok_or_else(|| "Cannot read properties of undefined (reading 'addGold')".to_string())?;
        tr.push(129.0);
        tr.push(pid);
        push_val(tr, &JsVal::Num(n as f64));
        Ok(())
    }

    /// `mg.players()` — event 130 dumps the scripted token list.
    fn mg_players(&mut self, tr: &mut Vec<f64>) -> Vec<usize> {
        tr.push(130.0);
        tr.push(self.players_list.len() as f64);
        for t in &self.players_list {
            tr.push(*t as f64);
        }
        self.players_list.clone()
    }

    // --- identity helpers ---

    fn pi_field(&self, key: &str) -> JsVal {
        val_field(&self.p.player_info, key)
            .cloned()
            .unwrap_or(JsVal::Undef)
    }

    fn is_alive(&self) -> bool {
        self.p.tiles.size() > 0.0
    }

    /// `hasEmbargoAgainst` (1200): the `embargoes` Map lookup by
    /// `other.id()` — strict `===` on the key (`SameValueZero` agrees for
    /// the string/number domain here).
    fn has_embargo_id(&self, id: &JsVal) -> bool {
        self.p.embargoes.iter().any(|(k, _)| js_strict_eq(k, id))
    }

    fn alliance_with(&self, tok: usize) -> Option<AllianceMock> {
        if self.self_tok == Some(tok) {
            return None;
        }
        self.p
            .alliances
            .iter()
            .find(|a| a.rec == tok || a.req == tok)
            .copied()
    }

    /// `agreedToExtend(p)` on an alliance mock:
    /// `p === requestor() ? reqAgree : recAgree` — the this-arg matches
    /// `requestor()` only when the self token sits at the req slot.
    fn agreed_to_extend(&self, a: &AllianceMock, arg_self: bool, tok: usize) -> bool {
        let at_req = if arg_self {
            self.self_tok == Some(a.req)
        } else {
            tok == a.req
        };
        if at_req { a.req_agree } else { a.rec_agree }
    }

    /// `isOnSameTeam` (PlayerImpl.ts:1259) — the facade call counts are the
    /// observable part: 0 (self / this-null), 1 (other-null), 2 (this-Bot),
    /// 3 (the final `===`).
    fn same_team(&mut self, tr: &mut Vec<f64>, tok: usize) -> Result<bool, String> {
        if self.self_tok == Some(tok) {
            return Ok(false);
        }
        if self.p.team == JsVal::Null {
            return Ok(false);
        }
        let t1 = self.other_team(tr, tok)?;
        if t1 == JsVal::Null {
            return Ok(false);
        }
        if self.p.team == JsVal::Str("Bot".to_string()) {
            return Ok(false);
        }
        let t2 = self.other_team(tr, tok)?;
        if t2 == JsVal::Str("Bot".to_string()) {
            return Ok(false);
        }
        let t3 = self.other_team(tr, tok)?;
        Ok(self.p.team == t3)
    }

    fn is_friendly(&mut self, tr: &mut Vec<f64>, tok: usize, treat: bool) -> Result<bool, String> {
        if self.self_tok == Some(tok) {
            return Ok(true);
        }
        let d = self.other_is_disconnected(tr, tok)?;
        if d && !treat {
            return Ok(false);
        }
        if self.same_team(tr, tok)? {
            return Ok(true);
        }
        Ok(self.alliance_with(tok).is_some())
    }

    /// `removeTroops` (1352) as a reusable body — `addTroops` delegates to
    /// it on the negative branch. Returns the removed amount.
    fn remove_troops(&mut self, troops: f64) -> Result<f64, String> {
        if troops <= 0.0 {
            return Ok(0.0);
        }
        let to_remove = to_int_big(troops)?;
        let actual = if self.p.troops < to_remove {
            self.p.troops
        } else {
            to_remove
        };
        self.p.troops -= actual;
        Ok(actual as f64)
    }

    /// `getTraitorRemainingTicks` (PlayerImpl.ts:855) — the `ticks()` facade
    /// (102) runs BEFORE the `traitorDuration()` facade (108), JS
    /// left-to-right; a never-marked player fires neither.
    fn traitor_remaining(&mut self, tr: &mut Vec<f64>) -> f64 {
        if self.p.marked_traitor_tick < 0.0 {
            return 0.0;
        }
        let elapsed = self.mg_ticks(tr) - self.p.marked_traitor_tick;
        let duration = self.mg_traitor_duration(tr);
        let remaining = duration - elapsed;
        if remaining > 0.0 {
            remaining
        } else {
            0.0
        }
    }

    /// `inDoomsdayClock` (PlayerImpl.ts:874) — `isAlive()` is untraced.
    fn in_doomsday(&self) -> bool {
        self.is_alive() && self.p.marked_doomsday_clock_tick >= 0.0
    }

    // --- G3c3 method bodies ---

    /// `toString()` (1833-1839) — only observable through the self-throw
    /// messages (`relation` / `updateRelation` / `sendEmoji`). `_troops` is
    /// a bigint (`${10n}` -> "10"); `numTilesOwned` interpolates as a JS
    /// number.
    fn player_to_string(&self) -> String {
        let name = js_to_string_val(&self.pi_field("name"));
        let client = js_to_string_val(&self.pi_field("clientID"));
        format!(
            "Player:{{name:{},clientID:{},isAlive:{},troops:{},numTileOwned:{}}}]",
            name,
            client,
            self.is_alive(),
            self.p.troops,
            crate::js_fixed::js_to_string(self.p.tiles.size())
        )
    }

    /// `removeGold` (1332-1339) as a reusable body — mid 27 and
    /// `donateGold` (1147) share it. The `toRemove <= 0n` gate (cross-type
    /// relational, ToNumber, never throws), then `minInt(_gold, toRemove)`
    /// (same rule) and `_gold -= actual` (the mix TypeError whenever both
    /// sides are not bigints). Returns the removed amount.
    fn remove_gold_body(&mut self, o: &Op) -> Result<i64, String> {
        let le0 = match o {
            Op::Big(b) => *b <= 0,
            other => op_to_num(other) <= 0.0,
        };
        if le0 {
            return Ok(0);
        }
        let gold_lt = match (&self.p.gold, o) {
            (Acc::Big(a), Op::Big(b)) => *a < *b,
            (Acc::Big(a), other) => (*a as f64) < op_to_num(other),
            (Acc::Str(s), Op::Big(b)) => {
                crate::game_config_helpers::js_number(s) < *b as f64
            }
            (Acc::Str(_), _) => false,
        };
        if gold_lt {
            // actual = _gold (only ever a Big — the Str rows above are
            // false); _gold -= actual -> 0n.
            if let Acc::Big(a) = self.p.gold {
                self.p.gold = Acc::Big(0);
                return Ok(a);
            }
            unreachable!()
        }
        match (&self.p.gold, o) {
            (Acc::Big(a), Op::Big(b)) => {
                let r = *b;
                self.p.gold = Acc::Big(a - b);
                Ok(r)
            }
            _ => Err(MIX_ERR.to_string()),
        }
    }

    /// `relations.get(other) ?? 0` — token-keyed insertion-ordered map.
    fn relation_get(&self, tok: usize) -> f64 {
        self.p
            .relations
            .iter()
            .find(|(k, _)| *k == tok)
            .map(|(_, v)| *v)
            .unwrap_or(0.0)
    }

    /// `relations.set(other, v)` — an existing key updates in place (the
    /// JS Map keeps its insertion position).
    fn relation_set(&mut self, tok: usize, v: f64) {
        if let Some(e) = self.p.relations.iter_mut().find(|(k, _)| *k == tok) {
            e.1 = v;
        } else {
            self.p.relations.push((tok, v));
        }
    }

    /// `targets()` (992-998) as a reusable body — mid 73 and
    /// `transitiveTargets` (1000) share it. The `ticks()` (102) and
    /// `targetDuration()` (113) facades fire PER element, in that order.
    fn targets_body(&mut self, tr: &mut Vec<f64>) -> Vec<usize> {
        let mut kept = Vec::new();
        for (tick, tok) in self.p.targets_.clone() {
            let t = script_f64(&self.ticks, &mut self.ticks_i, tr, 102.0) - tick;
            if t < script_f64(&self.tdur, &mut self.tdur_i, tr, 113.0) {
                kept.push(tok);
            }
        }
        kept
    }

    /// `canTrade` (1204-1208): the `other.hasEmbargoAgainst(this) ||
    /// this.hasEmbargoAgainst(other)` short-circuit, then the
    /// `other.id() !== this.id()` strict gate.
    fn can_trade(&self, tok: usize) -> bool {
        let embargo = self.other_has_embargo_against(tok) || {
            let oid = self.other_id(tok);
            self.has_embargo_id(&oid)
        };
        !embargo && !js_strict_eq(&self.other_id(tok), &self.pi_field("id"))
    }

    /// `allianceInfo` (772-794): null without an alliance; otherwise the
    /// 102 ticks + 109 offset facades, then the `canExtend` `&&` chain —
    /// `!this.isDisconnected()` (untraced field read), `!other.isDisconnected()`
    /// (105), `this.isAlive()` (untraced), `other.isAlive()` (127),
    /// `inExtensionWindow`, `!agreedToExtend(this)` — each short-circuit
    /// pinning the facade count. The returned object evaluates
    /// `expiresAt()` / `agreedToExtend(...)` again (untraced constants),
    /// key order expiresAt/inExtensionWindow/myPlayerAgreedToExtend/
    /// otherAgreedToExtend/canExtend.
    fn alliance_info(&mut self, tr: &mut Vec<f64>, tok: usize) -> Result<JsVal, String> {
        let a = match self.alliance_with(tok) {
            None => return Ok(JsVal::Null),
            Some(a) => a,
        };
        let t = self.mg_ticks(tr);
        let off = script_f64(&self.off, &mut self.off_i, tr, 109.0);
        let in_window = a.expires_at <= t + off;
        let can_extend = !js_truthy(&self.p.is_disconnected)
            && !self.other_is_disconnected(tr, tok)?
            && self.is_alive()
            && self.other_is_alive(tr, tok)?
            && in_window
            && !self.agreed_to_extend(&a, true, tok);
        Ok(JsVal::Obj(vec![
            ("expiresAt".to_string(), JsVal::Num(a.expires_at)),
            ("inExtensionWindow".to_string(), JsVal::Bool(in_window)),
            (
                "myPlayerAgreedToExtend".to_string(),
                JsVal::Bool(self.agreed_to_extend(&a, true, tok)),
            ),
            (
                "otherAgreedToExtend".to_string(),
                JsVal::Bool(self.agreed_to_extend(&a, false, tok)),
            ),
            ("canExtend".to_string(), JsVal::Bool(can_extend)),
        ]))
    }

    /// `canSendAllianceRequest` (796-841). The `recent` list is the
    /// harness-scripted `pastOutgoingAllianceRequests` filtered to the
    /// recipient and sorted by createdAt descending (stable, NaN → Equal);
    /// the cooldown gate reads 102 then 111 only when `recent` is non-empty.
    fn can_send_alliance_request(
        &mut self,
        tr: &mut Vec<f64>,
        tok: usize,
    ) -> Result<bool, String> {
        let da = script_val(&self.da, &mut self.da_i, tr, 110.0);
        if js_truthy(&da) {
            return Ok(false);
        }
        if self.self_tok == Some(tok) {
            return Ok(false);
        }
        // `this.isDisconnected() || other.isDisconnected()` — this-side is
        // the untraced field read; the 105 facade fires only on a
        // non-disconnected this.
        if js_truthy(&self.p.is_disconnected) || self.other_is_disconnected(tr, tok)? {
            return Ok(false);
        }
        if self.is_friendly(tr, tok, false)? || !self.is_alive() {
            return Ok(false);
        }
        let st = self.self_tok;
        let has_pending = self
            .alliance_requests
            .iter()
            .any(|(req, rec)| Some(*req) == st && *rec == tok);
        if has_pending {
            return Ok(false);
        }
        let has_incoming = self
            .alliance_requests
            .iter()
            .any(|(req, rec)| Some(*rec) == st && *req == tok);
        if has_incoming {
            return Ok(true);
        }
        let mut recent: Vec<(usize, f64)> = self
            .past_outgoing
            .iter()
            .copied()
            .filter(|(rec, _)| *rec == tok)
            .collect();
        js_sort_desc(&mut recent);
        if recent.is_empty() {
            return Ok(true);
        }
        let delta = self.mg_ticks(tr) - recent[0].1;
        let cd = script_f64(&self.arc, &mut self.arc_i, tr, 111.0);
        Ok(delta >= cd)
    }

    /// `allRelationsSorted` (941-949): one 127 facade per relations entry
    /// (insertion order, BEFORE the sort), then the stable ascending sort
    /// and the `relationFromValue` map. Encoded as `[[tok, relation], ...]`.
    fn all_relations_sorted(&mut self, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        let entries = self.p.relations.clone();
        let mut kept: Vec<(usize, f64)> = Vec::new();
        for (tok, v) in entries {
            if self.other_is_alive(tr, tok)? {
                kept.push((tok, v));
            }
        }
        js_sort_asc(&mut kept);
        Ok(JsVal::Arr(
            kept.into_iter()
                .map(|(tok, v)| {
                    JsVal::Arr(vec![JsVal::Num(tok as f64), JsVal::Num(relation_from_value(v))])
                })
                .collect(),
        ))
    }

    /// `updateRelation` (951-958): `relation + delta` is the JS `+` — a
    /// string-primitive delta (Str / Obj / Arr) concatenates, everything
    /// else adds numerically. `within(v, -100, 100)` = Math.min(Math.max(v,
    /// -100), 100) with NaN propagation.
    fn update_relation(&mut self, tok: usize, delta: &JsVal) {
        let relation = self.relation_get(tok);
        let sum = match delta {
            JsVal::Str(_) | JsVal::Obj(_) | JsVal::Arr(_) => JsVal::Str(format!(
                "{}{}",
                crate::js_fixed::js_to_string(relation),
                js_to_string_val(delta)
            )),
            other => JsVal::Num(relation + to_num(other)),
        };
        let v = crate::jsnum::js_min(
            crate::jsnum::js_max(to_num(&sum), -100.0),
            100.0,
        );
        self.relation_set(tok, v);
    }

    /// `decayRelations` (960-970): `-1 * Math.sign(r)` (NaN → NaN, ±0
    /// preserved), `r += sign * 0.05`, then the `|r| < 0.1` zero snap; the
    /// Map updates in place during the forEach (insertion order).
    fn decay_relations(&mut self) {
        let keys: Vec<usize> = self.p.relations.iter().map(|(k, _)| *k).collect();
        for k in keys {
            let r0 = self.relation_get(k);
            let delta = 0.05;
            let mut r = r0 + -js_sign(r0) * delta;
            if r.abs() < delta * 2.0 {
                r = 0.0;
            }
            self.relation_set(k, r);
        }
    }

    /// `canTarget` (972-985): self gate, `isFriendly` gate, then the
    /// targets_ scan — 102 ticks + 112 targetCooldown per element until the
    /// first cooldown hit.
    fn can_target(&mut self, tr: &mut Vec<f64>, tok: usize) -> Result<bool, String> {
        if self.self_tok == Some(tok) {
            return Ok(false);
        }
        if self.is_friendly(tr, tok, false)? {
            return Ok(false);
        }
        for (tick, _) in self.p.targets_.clone() {
            let t = self.mg_ticks(tr) - tick;
            if t < script_f64(&self.tc, &mut self.tc_i, tr, 112.0) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// `transitiveTargets` (1000-1006): allies via `a.other(this)` (the mid
    /// 49 token flip), their `targets()` are the mocks' constant lists
    /// (untraced), then the real `targets()` body; `[...new Set(ts)]`
    /// dedups by first occurrence.
    fn transitive_targets(&mut self, tr: &mut Vec<f64>) -> Vec<usize> {
        let self_tok = self.self_tok;
        let mut ts: Vec<usize> = Vec::new();
        for a in self.p.alliances.clone() {
            let ally = if self_tok == Some(a.req) { a.rec } else { a.req };
            ts.extend(self.other_targets(ally));
        }
        ts.extend(self.targets_body(tr));
        let mut seen: Vec<usize> = Vec::new();
        for t in ts {
            if !seen.contains(&t) {
                seen.push(t);
            }
        }
        seen
    }

    /// `outgoingEmojis` (1022-1030): 102 ticks + 114 emojiMessageDuration
    /// per stored message, then the stable createdAt-descending sort.
    fn outgoing_emojis_body(&mut self, tr: &mut Vec<f64>) -> JsVal {
        let mut kept: Vec<JsVal> = Vec::new();
        for e in self.p.outgoing_emojis.clone() {
            let created = to_num(&val_field(&e, "createdAt").cloned().unwrap_or(JsVal::Undef));
            let t = self.mg_ticks(tr) - created;
            if t < script_f64(&self.emd, &mut self.emd_i, tr, 114.0) {
                kept.push(e);
            }
        }
        let mut pairs: Vec<(usize, f64)> = kept
            .iter()
            .enumerate()
            .map(|(i, e)| {
                (
                    i,
                    to_num(&val_field(e, "createdAt").cloned().unwrap_or(JsVal::Undef)),
                )
            })
            .collect();
        js_sort_desc(&mut pairs);
        JsVal::Arr(pairs.into_iter().map(|(i, _)| kept[i].clone()).collect())
    }

    /// `canSendEmoji` (1032-1050): the `recipient === this` gate (a
    /// `"AllPlayers"` string never ===), the recipientID compute (untraced),
    /// then per matching message 102 ticks + 115 emojiMessageCooldown until
    /// the first hit.
    fn can_send_emoji(&mut self, tr: &mut Vec<f64>, recip: &JsVal) -> Result<bool, String> {
        if let JsVal::Num(t) = recip {
            if self.self_tok == Some(*t as usize) {
                return Ok(false);
            }
        }
        let rid = self.emoji_recipient_id(recip)?;
        for m in self.p.outgoing_emojis.clone() {
            let mrid = val_field(&m, "recipientID").cloned().unwrap_or(JsVal::Undef);
            if js_strict_eq(&mrid, &rid) {
                let created = to_num(&val_field(&m, "createdAt").cloned().unwrap_or(JsVal::Undef));
                let t = self.mg_ticks(tr) - created;
                if t < script_f64(&self.emc, &mut self.emc_i, tr, 115.0) {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    /// `recipient === AllPlayers ? recipient : recipient.smallID()` — the
    /// AllPlayers string crosses as `Str("AllPlayers")` (Game.ts:61).
    fn emoji_recipient_id(&self, recip: &JsVal) -> Result<JsVal, String> {
        match recip {
            JsVal::Str(s) if s == "AllPlayers" => Ok(JsVal::Str(s.clone())),
            JsVal::Num(t) => Ok(JsVal::Num(self.other_small_id(*t as usize)?)),
            _ => Ok(JsVal::Undef),
        }
    }

    /// `canDonateGold` / `canDonateTroops` (1067-1125): self gate, then
    /// `!this.isAlive() || !recipient.isAlive() || !this.isFriendly(...)`
    /// (127 once, then the isFriendly facades), then the
    /// `type() === Human && config === false` gate (117 gold / 118 troops,
    /// read only for humans), then the sentDonations scan (102 + 119 per
    /// matching donation until the first hit).
    fn can_donate(&mut self, tr: &mut Vec<f64>, tok: usize, gold: bool) -> Result<bool, String> {
        if self.self_tok == Some(tok) {
            return Ok(false);
        }
        if !self.is_alive() || !self.other_is_alive(tr, tok)? || !self.is_friendly(tr, tok, false)?
        {
            return Ok(false);
        }
        if js_strict_eq(&self.other_type(tok), &JsVal::Str("HUMAN".to_string())) {
            let v = if gold {
                take_jsval(&self.dg, &mut self.dg_i)
            } else {
                take_jsval(&self.dt, &mut self.dt_i)
            };
            tr.push(if gold { 117.0 } else { 118.0 });
            push_val(tr, &v);
            if js_strict_eq(&v, &JsVal::Bool(false)) {
                return Ok(false);
            }
        }
        for (rec, tick) in self.p.sent_donations.clone() {
            if rec == tok {
                let t = self.mg_ticks(tr) - tick;
                if t < script_f64(&self.dc, &mut self.dc_i, tr, 119.0) {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    /// The DonateEvent update (GameUpdateType.DonateEvent === 23, zero-based
    /// declaration order), key order type/donationType/senderId/recipientId/
    /// amount. `amount` is a bigint (`BigInt(removed)` / `removed`): the
    /// capture codec has no bigint branch, so it falls into the object branch
    /// and `Object.keys(100n) === []` crosses as an empty object [6,0].
    fn donate_update(&self, kind: &str, tok: usize) -> JsVal {
        JsVal::Obj(vec![
            ("type".to_string(), JsVal::Num(23.0)),
            ("donationType".to_string(), JsVal::Str(kind.to_string())),
            ("senderId".to_string(), self.pi_field("id")),
            ("recipientId".to_string(), self.other_id(tok)),
            ("amount".to_string(), JsVal::Obj(vec![])),
        ])
    }

    /// `canEmbargoAll` (1178-1194): 102 + 121 cooldown gate, then the
    /// `mg.players()` scan (130 once) — `p.id() === this.id()` and
    /// `p.type() === PlayerType.Bot` are untraced constant reads, the
    /// `isOnSameTeam(p)` continue carries the 106 facade counts.
    fn can_embargo_all(&mut self, tr: &mut Vec<f64>) -> Result<bool, String> {
        let t = self.mg_ticks(tr) - self.p.last_embargo_all_tick;
        if t < script_f64(&self.eac, &mut self.eac_i, tr, 121.0) {
            return Ok(false);
        }
        let players = self.mg_players(tr);
        let my_id = self.pi_field("id");
        for tok in players {
            if js_strict_eq(&self.other_id(tok), &my_id) {
                continue;
            }
            if js_strict_eq(&self.other_type(tok), &JsVal::Str("BOT".to_string())) {
                continue;
            }
            if self.same_team(tr, tok)? {
                continue;
            }
            return Ok(true);
        }
        Ok(false)
    }

    /// `addEmbargo` (1214-1230): a non-temporary existing entry returns
    /// BEFORE the facade; otherwise the EmbargoEvent "start" update (101)
    /// fires FIRST, then `createdAt: mg.ticks()` (102) inside the stored
    /// object. GameUpdateType.EmbargoEvent === 20.
    fn add_embargo(&mut self, tr: &mut Vec<f64>, tok: usize, temp: JsVal) -> Result<(), String> {
        let oid = self.other_id(tok);
        if let Some((_, e)) = self.p.embargoes.iter().find(|(k, _)| js_strict_eq(k, &oid)) {
            if !js_truthy(&e.is_temporary) {
                return Ok(());
            }
        }
        let u = JsVal::Obj(vec![
            ("type".to_string(), JsVal::Num(20.0)),
            ("event".to_string(), JsVal::Str("start".to_string())),
            ("playerID".to_string(), JsVal::Num(self.p.small_id)),
            (
                "embargoedID".to_string(),
                JsVal::Num(self.other_small_id(tok)?),
            ),
        ]);
        self.mg_add_update(tr, &u);
        let created = self.mg_ticks(tr);
        let e = Embargo {
            created_at: created,
            is_temporary: temp,
            target: tok,
        };
        if let Some(slot) = self.p.embargoes.iter_mut().find(|(k, _)| js_strict_eq(k, &oid)) {
            slot.1 = e;
        } else {
            self.p.embargoes.push((oid, e));
        }
        Ok(())
    }

    /// `stopEmbargo` (1232-1240): the Map delete THEN the unconditional
    /// EmbargoEvent "stop" update (101).
    fn stop_embargo(&mut self, tr: &mut Vec<f64>, tok: usize) -> Result<(), String> {
        let oid = self.other_id(tok);
        if let Some(pos) = self.p.embargoes.iter().position(|(k, _)| js_strict_eq(k, &oid)) {
            self.p.embargoes.remove(pos);
        }
        let u = JsVal::Obj(vec![
            ("type".to_string(), JsVal::Num(20.0)),
            ("event".to_string(), JsVal::Str("stop".to_string())),
            ("playerID".to_string(), JsVal::Num(self.p.small_id)),
            (
                "embargoedID".to_string(),
                JsVal::Num(self.other_small_id(tok)?),
            ),
        ]);
        self.mg_add_update(tr, &u);
        Ok(())
    }

    // --- the op dispatcher ---

    /// Run one op. Kind table (mirrored by `tools/gen_vectors.mjs`):
    /// 0 construct `[encVal playerInfo, smallID, encVal startTroops, encVal
    /// team, nAlliances, (req,rec,expiresAt,reqAgree,recAgree)*, nTiles,(t)*,
    /// nBorder,(t)*, sgScript, ticksScript, teamTilesScript, totalLandScript,
    /// playersBlock(pid, teamList, discList, encS type, aliveList, embVals,
    /// tgts)*, selfTok, nExpired, (req,rec)*, traitorDurationScript,
    /// nAllianceReqs, (req,rec)*, nPastOutgoing, (rec,createdAt)*,
    /// playersList, offScript, daScript, arcScript, tcScript, tdurScript,
    /// emdScript, emcScript, qcScript, dgScript, dtScript, dcScript,
    /// ducScript, eacScript, carScript]`
    /// -> `[traceLen,(trace)*,0]` or `[traceLen,(trace)*,1,encS]`; kind 1
    /// method `[mid,...]` -> `[traceLen,(trace)*,[0,...encVal] | [1,encS]]`.
    /// mid follows the TS declaration order: 0 smallID 1 name 2 displayName
    /// 3 clanTag 4 nationFlag 5 clientID 6 id 7 type 8 info 9
    /// isLobbyCreator 10 isAlive 11 hasSpawned 12 setSpawnTile 13 spawnTile
    /// 14 troops 15 setTroops 16 addTroops 17 removeTroops 18 gold 19
    /// tradeGold 20 addTradeGold 21 trainGold 22 addTrainGold 23 piracyGold
    /// 24 addPiracyGold 25 goldEarned 26 addGold 27 removeGold 28
    /// tileChangeVersion 29 lastTileChange 30 isDisconnected 31
    /// markDisconnected 32 disconnectSnapshot 33 disconnectedAtTick 34 team
    /// 35 isOnSameTeam 36 isFriendly 37 isAlliedWith 38 allianceWith 39
    /// alliances 40 numTilesOwned 41 dumpTiles 42 addTile 43 removeTile 44
    /// dumpBorderTiles 45 addBorderTile 46 removeBorderTile 47 isPlayer
    /// (G3c2:) 48 expiredAlliances 49 allies 50 isTraitor 51
    /// getTraitorRemainingTicks 52 markTraitor 53 inDoomsdayClock 54
    /// doomsdayClockTicks 55 enterDoomsdayClock 56 clearDoomsdayClock 57
    /// markRotted 58 isDecaying 59 betrayals. (G3c3:) 60
    /// incomingAllianceRequests 61 outgoingAllianceRequests 62 allianceInfo
    /// 63 canSendAllianceRequest 64 breakAlliance 65 removeAllAlliances 66
    /// createAllianceRequest 67 relation 68 allRelationsSorted 69
    /// updateRelation 70 decayRelations 71 canTarget 72 target 73 targets 74
    /// transitiveTargets 75 sendEmoji 76 outgoingEmojis 77 canSendEmoji 78
    /// canSendQuickChat 79 recordQuickChat 80 canDonateGold 81
    /// canDonateTroops 82 donateTroops 83 donateGold 84 canDeleteUnit 85
    /// recordDeleteUnit 86 canEmbargoAll 87 recordEmbargoAll 88
    /// hasEmbargoAgainst 89 canTrade 90 getEmbargoes 91 addEmbargo 92
    /// stopEmbargo 93 endTemporaryEmbargo 94 tradingPartners.
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut c = Cur(args, 0);
        let mut tr: Vec<f64> = Vec::new();
        let mut out: Vec<f64> = Vec::new();
        match kind {
            0 => {
                self.reset();
                let pi = c.val();
                let small_id = c.f();
                let start_troops = c.val();
                let team = c.val();
                let n_all = c.u();
                let alliances: Vec<AllianceMock> = (0..n_all)
                    .map(|_| AllianceMock {
                        req: c.u(),
                        rec: c.u(),
                        expires_at: c.f(),
                        req_agree: c.f() != 0.0,
                        rec_agree: c.f() != 0.0,
                    })
                    .collect();
                let tiles = c.list();
                let border = c.list();
                let sg = c.list();
                let ticks = c.list();
                let team_tiles = c.list();
                let total_land = c.list();
                let n = c.u();
                let players = (0..n)
                    .map(|_| {
                        let pid = c.f();
                        let teams = c.team_list();
                        let discs = c.list();
                        let typ = c.s();
                        let alives = c.list();
                        let emb = c.val_list();
                        let tgts = (0..c.u()).map(|_| c.u()).collect();
                        PlPlayer {
                            pid,
                            teams,
                            team_i: 0,
                            discs,
                            disc_i: 0,
                            typ,
                            alives,
                            alive_i: 0,
                            emb,
                            tgts,
                        }
                    })
                    .collect();
                let self_tok = c.f();
                let n_exp = c.u();
                let expired_alliances: Vec<(usize, usize)> =
                    (0..n_exp).map(|_| (c.u(), c.u())).collect();
                let traitor_duration = c.list();
                // G3c3 tail: the mg social surface and the config scripts.
                let alliance_requests = (0..c.u()).map(|_| (c.u(), c.u())).collect();
                let past_outgoing = (0..c.u()).map(|_| (c.u(), c.f())).collect();
                let players_list = (0..c.u()).map(|_| c.u()).collect();
                let off = c.list();
                let da = c.val_list();
                let arc = c.list();
                let tc = c.list();
                let tdur = c.list();
                let emd = c.list();
                let emc = c.list();
                let qc = c.list();
                let dg = c.val_list();
                let dt = c.val_list();
                let dc = c.list();
                let duc = c.list();
                let eac = c.list();
                let car = c.val_list();
                self.sg = sg;
                self.ticks = ticks;
                self.team_tiles = team_tiles;
                self.total_land = total_land;
                self.traitor_duration = traitor_duration;
                self.players = players;
                self.alliance_requests = alliance_requests;
                self.past_outgoing = past_outgoing;
                self.players_list = players_list;
                self.off = off;
                self.da = da;
                self.arc = arc;
                self.tc = tc;
                self.tdur = tdur;
                self.emd = emd;
                self.emc = emc;
                self.qc = qc;
                self.dg = dg;
                self.dt = dt;
                self.dc = dc;
                self.duc = duc;
                self.eac = eac;
                self.car = car;
                self.self_tok = if self_tok >= 0.0 {
                    Some(self_tok as usize)
                } else {
                    None
                };

                // ctor order (PlayerImpl.ts:198-200): _troops FIRST (a NaN
                // startTroops throws before facade 100), then _gold, then
                // the PseudoRandom seed read.
                let troops = to_int_big(to_num(&start_troops));
                let gold = troops.and_then(|b| self.mg_starting_gold(&mut tr).map(|g| (b, g)));
                let seeded = gold.and_then(|(b, g)| {
                    // simpleHash(this.playerInfo.id): `str.length` throws
                    // only on null / undefined; the value is unobservable.
                    // `playerInfo` is a parameter property — assigned before
                    // the ctor body runs, so the id read uses the local `pi`.
                    match val_field(&pi, "id") {
                        Some(JsVal::Null) => {
                            Err("Cannot read properties of null (reading 'length')".to_string())
                        }
                        None | Some(JsVal::Absent) | Some(JsVal::Undef) => Err(
                            "Cannot read properties of undefined (reading 'length')".to_string(),
                        ),
                        _ => Ok((b, g)),
                    }
                });
                match seeded {
                    Ok((b, g)) => {
                        self.p = Player {
                            small_id,
                            player_info: pi,
                            team,
                            troops: b,
                            gold: Acc::Big(g),
                            tiles: TileSet::new(Some(&tiles)),
                            border_tiles: TileSet::new(Some(&border)),
                            alliances,
                            expired_alliances,
                            ..Default::default()
                        };
                        self.constructed = true;
                        out.push(tr.len() as f64);
                        out.extend(tr.iter().copied());
                        out.push(0.0);
                    }
                    Err(msg) => {
                        out.push(tr.len() as f64);
                        out.extend(tr.iter().copied());
                        push_err(&mut out, &msg);
                    }
                }
                out
            }
            1 => {
                let mid = c.u() as u8;
                if !self.constructed {
                    // JS `pl === null`: the property access throws before
                    // anything else; the trace stays empty.
                    out.push(0.0);
                    out.push(1.0);
                    push_str(
                        &mut out,
                        &format!(
                            "Cannot read properties of null (reading '{}')",
                            mid_property(mid)
                        ),
                    );
                    return out;
                }
                match mid {
                    0 => push_ok_num(&mut out, self.p.small_id),
                    1 | 2 | 3 | 4 | 5 | 6 | 7 | 9 => {
                        let key = match mid {
                            1 => "name",
                            2 => "displayName",
                            3 => "clanTag",
                            4 => "nationFlag",
                            5 => "clientID",
                            6 => "id",
                            7 => "playerType",
                            _ => "isLobbyCreator",
                        };
                        let v = self.pi_field(key);
                        push_ok(&mut out, &v);
                    }
                    8 => push_ok(&mut out, &self.p.player_info.clone()),
                    10 => push_ok_bool(&mut out, self.is_alive()),
                    11 => push_ok_bool(&mut out, self.p.spawn_tile != JsVal::Undef),
                    12 => {
                        self.p.spawn_tile = c.val();
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    13 => push_ok(&mut out, &self.p.spawn_tile.clone()),
                    14 => push_ok_num(&mut out, self.p.troops as f64),
                    15 => match to_int_big(to_num(&c.val())) {
                        Ok(b) => {
                            self.p.troops = b;
                            push_ok(&mut out, &JsVal::Undef);
                        }
                        Err(m) => push_err(&mut out, &m),
                    },
                    16 => {
                        let t = to_num(&c.val());
                        if t < 0.0 {
                            // JS `-1 * troops` ≡ `-t` for every f64 (the
                            // -0 / NaN cases agree: -1*-0 = +0, -(-0) = +0).
                            match self.remove_troops(-t) {
                                Ok(_) => push_ok(&mut out, &JsVal::Undef),
                                Err(m) => push_err(&mut out, &m),
                            }
                        } else {
                            match to_int_big(t) {
                                Ok(b) => {
                                    self.p.troops += b;
                                    push_ok(&mut out, &JsVal::Undef);
                                }
                                Err(m) => push_err(&mut out, &m),
                            }
                        }
                    }
                    17 => match self.remove_troops(to_num(&c.val())) {
                        Ok(r) => push_ok_num(&mut out, r),
                        Err(m) => push_err(&mut out, &m),
                    },
                    18 => push_ok(&mut out, &self.p.gold.to_jsval()),
                    19 => push_ok(&mut out, &self.p.trade_gold.to_jsval()),
                    20 => match big_operand(&c.val()) {
                        Err(m) => push_err(&mut out, &m),
                        Ok(o) => match acc_plus(&self.p.trade_gold, &o) {
                            Err(m) => push_err(&mut out, &m),
                            Ok(v) => {
                                self.p.trade_gold = v;
                                push_ok(&mut out, &JsVal::Undef);
                            }
                        },
                    },
                    21 => push_ok(&mut out, &self.p.train_gold.to_jsval()),
                    22 => match big_operand(&c.val()) {
                        Err(m) => push_err(&mut out, &m),
                        Ok(o) => match acc_plus(&self.p.train_gold, &o) {
                            Err(m) => push_err(&mut out, &m),
                            Ok(v) => {
                                self.p.train_gold = v;
                                push_ok(&mut out, &JsVal::Undef);
                            }
                        },
                    },
                    23 => push_ok(&mut out, &self.p.piracy_gold.to_jsval()),
                    24 => match big_operand(&c.val()) {
                        Err(m) => push_err(&mut out, &m),
                        Ok(o) => match acc_plus(&self.p.piracy_gold, &o) {
                            Err(m) => push_err(&mut out, &m),
                            Ok(v) => {
                                self.p.piracy_gold = v;
                                push_ok(&mut out, &JsVal::Undef);
                            }
                        },
                    },
                    25 => push_ok(&mut out, &self.p.gold_earned.to_jsval()),
                    26 => {
                        let to_add = c.val();
                        let tile = if c.has() { c.val() } else { JsVal::Undef };
                        match big_operand(&to_add) {
                            Err(m) => push_err(&mut out, &m),
                            Ok(o) => match acc_plus(&self.p.gold, &o) {
                                Err(m) => push_err(&mut out, &m),
                                Ok(ng) => {
                                    self.p.gold = ng;
                                    match acc_plus(&self.p.gold_earned, &o) {
                                        Err(m) => push_err(&mut out, &m),
                                        Ok(ne) => {
                                            self.p.gold_earned = ne;
                                            if js_truthy(&tile) {
                                                let player_id = self.pi_field("id");
                                                let u = JsVal::Obj(vec![
                                                    ("type".to_string(), JsVal::Num(15.0)),
                                                    ("player".to_string(), player_id),
                                                    ("tile".to_string(), tile),
                                                    ("gold".to_string(), JsVal::Num(op_to_num(&o))),
                                                    ("troops".to_string(), JsVal::Num(0.0)),
                                                ]);
                                                self.mg_add_update(&mut tr, &u);
                                            }
                                            push_ok(&mut out, &JsVal::Undef);
                                        }
                                    }
                                }
                            },
                        }
                    }
                    27 => {
                        // `big()` operand conversion throws BEFORE the call
                        // (a non-integer number); the body is shared with
                        // `donateGold` — see `remove_gold_body`.
                        match big_operand(&c.val()) {
                            Err(m) => push_err(&mut out, &m),
                            Ok(o) => match self.remove_gold_body(&o) {
                                Ok(r) => push_ok_num(&mut out, r as f64),
                                Err(m) => push_err(&mut out, &m),
                            },
                        }
                    }
                    28 => push_ok_num(&mut out, self.p.tile_change_version),
                    29 => push_ok_num(&mut out, self.p.last_tile_change),
                    30 => push_ok(&mut out, &self.p.is_disconnected.clone()),
                    31 => {
                        let v = c.val();
                        let snap = if c.has() { c.val() } else { JsVal::Absent };
                        self.p.is_disconnected = v;
                        if js_truthy(&self.p.is_disconnected) {
                            if self.p.disconnect_snapshot == JsVal::Null {
                                if nullish(&snap) {
                                    // built object, left-to-right: 102 ->
                                    // team truthy gate -> 103 -> 104.
                                    let current_tick = self.mg_ticks(&mut tr);
                                    let team = self.p.team.clone();
                                    let team_tiles = if js_truthy(&team) {
                                        self.mg_team_tiles_owned(&mut tr, &team)
                                    } else {
                                        0.0
                                    };
                                    let total_land = self.mg_total_land(&mut tr);
                                    let was_alive = self.is_alive();
                                    self.p.disconnect_snapshot = JsVal::Obj(vec![
                                        ("currentTick".to_string(), JsVal::Num(current_tick)),
                                        ("teamTiles".to_string(), JsVal::Num(team_tiles)),
                                        ("totalLand".to_string(), JsVal::Num(total_land)),
                                        ("wasAlive".to_string(), JsVal::Bool(was_alive)),
                                    ]);
                                } else {
                                    self.p.disconnect_snapshot = snap;
                                }
                            }
                        } else {
                            self.p.disconnect_snapshot = JsVal::Null;
                        }
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    32 => push_ok(&mut out, &self.p.disconnect_snapshot.clone()),
                    33 => {
                        // `snapshot?.currentTick ?? null` — a non-nullish
                        // currentTick crosses RAW.
                        let tick = match &self.p.disconnect_snapshot {
                            JsVal::Obj(fields) => {
                                let f = fields
                                    .iter()
                                    .find(|(k, _)| k == "currentTick")
                                    .map(|(_, v)| v.clone())
                                    .unwrap_or(JsVal::Undef);
                                if nullish(&f) {
                                    None
                                } else {
                                    Some(f)
                                }
                            }
                            _ => None,
                        };
                        match tick {
                            Some(f) => push_ok(&mut out, &f),
                            None => push_ok(&mut out, &JsVal::Null),
                        }
                    }
                    34 => push_ok(&mut out, &self.p.team.clone()),
                    35 => {
                        let tok = to_num(&c.val()) as usize;
                        match self.same_team(&mut tr, tok) {
                            Ok(b) => push_ok_bool(&mut out, b),
                            Err(m) => push_err(&mut out, &m),
                        }
                    }
                    36 => {
                        let tok = to_num(&c.val()) as usize;
                        let treat = if c.has() { js_truthy(&c.val()) } else { false };
                        match self.is_friendly(&mut tr, tok, treat) {
                            Ok(b) => push_ok_bool(&mut out, b),
                            Err(m) => push_err(&mut out, &m),
                        }
                    }
                    37 => {
                        let tok = to_num(&c.val()) as usize;
                        push_ok_bool(&mut out, self.alliance_with(tok).is_some());
                    }
                    38 => {
                        let tok = to_num(&c.val()) as usize;
                        match self.alliance_with(tok) {
                            Some(a) => push_ok(
                                &mut out,
                                &JsVal::Arr(vec![
                                    JsVal::Num(a.req as f64),
                                    JsVal::Num(a.rec as f64),
                                ]),
                            ),
                            None => push_ok(&mut out, &JsVal::Null),
                        }
                    }
                    39 => push_ok(
                        &mut out,
                        &JsVal::Arr(
                            self.p
                                .alliances
                                .iter()
                                .map(|a| {
                                    JsVal::Arr(vec![
                                        JsVal::Num(a.req as f64),
                                        JsVal::Num(a.rec as f64),
                                    ])
                                })
                                .collect(),
                        ),
                    ),
                    40 => push_ok_num(&mut out, self.p.tiles.size()),
                    41 => {
                        self.p.tiles.iter_begin();
                        let mut cur = 0usize;
                        let mut vals = Vec::new();
                        while let Some(v) = self.p.tiles.iter_next(&mut cur) {
                            vals.push(v);
                        }
                        self.p.tiles.iter_end();
                        push_ok(&mut out, &JsVal::Arr(vals.into_iter().map(JsVal::Num).collect()));
                    }
                    42 => {
                        self.p.tiles.add(to_num(&c.val()));
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    43 => {
                        self.p.tiles.delete(to_num(&c.val()));
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    44 => {
                        self.p.border_tiles.iter_begin();
                        let mut cur = 0usize;
                        let mut vals = Vec::new();
                        while let Some(v) = self.p.border_tiles.iter_next(&mut cur) {
                            vals.push(v);
                        }
                        self.p.border_tiles.iter_end();
                        push_ok(&mut out, &JsVal::Arr(vals.into_iter().map(JsVal::Num).collect()));
                    }
                    45 => {
                        self.p.border_tiles.add(to_num(&c.val()));
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    46 => {
                        self.p.border_tiles.delete(to_num(&c.val()));
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    47 => push_ok_bool(&mut out, true),
                    48 => push_ok(
                        &mut out,
                        &JsVal::Arr(
                            self.p
                                .expired_alliances
                                .iter()
                                .map(|(req, rec)| {
                                    JsVal::Arr(vec![JsVal::Num(*req as f64), JsVal::Num(*rec as f64)])
                                })
                                .collect(),
                        ),
                    ),
                    49 => {
                        // `alliances().map((a) => a.other(this))` — the mock's
                        // `other(p)` is `p === mocks[req] ? mocks[rec] :
                        // mocks[req]`, i.e. the self slot at the REQ position
                        // flips to REC, everything else (self at REC, or a
                        // player under test not in the alliance) stays REQ.
                        let self_tok = self.self_tok;
                        push_ok(
                            &mut out,
                            &JsVal::Arr(
                                self.p
                                    .alliances
                                    .iter()
                                    .map(|a| {
                                        let tok = if self_tok == Some(a.req) {
                                            a.rec
                                        } else {
                                            a.req
                                        };
                                        JsVal::Num(tok as f64)
                                    })
                                    .collect(),
                            ),
                        )
                    }
                    50 => push_ok_bool(&mut out, self.traitor_remaining(&mut tr) > 0.0),
                    51 => {
                        let r = self.traitor_remaining(&mut tr);
                        push_ok_num(&mut out, r)
                    }
                    52 => {
                        self.p.marked_traitor_tick = self.mg_ticks(&mut tr);
                        self.p.betrayal_count += 1.0;
                        self.mg_betray(&mut tr);
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    53 => push_ok_bool(&mut out, self.in_doomsday()),
                    54 => {
                        if self.in_doomsday() {
                            let t = self.mg_ticks(&mut tr) - self.p.marked_doomsday_clock_tick;
                            push_ok_num(&mut out, t)
                        } else {
                            push_ok_num(&mut out, 0.0)
                        }
                    }
                    55 => {
                        if self.p.marked_doomsday_clock_tick < 0.0 {
                            self.p.marked_doomsday_clock_tick = self.mg_ticks(&mut tr);
                        }
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    56 => {
                        self.p.marked_doomsday_clock_tick = -1.0;
                        self.p.rotted_at_tick = -1.0;
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    57 => {
                        self.p.rotted_at_tick = self.mg_ticks(&mut tr);
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    58 => {
                        let b = if !self.in_doomsday() || self.p.rotted_at_tick < 0.0 {
                            false
                        } else {
                            (self.mg_ticks(&mut tr) - self.p.rotted_at_tick) <= DECAY_CUE_GRACE_TICKS
                        };
                        push_ok_bool(&mut out, b)
                    }
                    59 => push_ok_num(&mut out, self.p.betrayal_count),
                    // ---- G3c3: the player-to-player social surface ----
                    60 | 61 => {
                        // incoming: ar.recipient() === this; outgoing:
                        // ar.requestor() === this. `this` is the mocks table
                        // slot selfTok, so the filter is a token compare.
                        let st = self.self_tok;
                        push_ok(
                            &mut out,
                            &JsVal::Arr(
                                self.alliance_requests
                                    .iter()
                                    .filter(|(req, rec)| {
                                        Some(if mid == 60 { *rec } else { *req }) == st
                                    })
                                    .map(|(req, rec)| {
                                        JsVal::Arr(vec![
                                            JsVal::Num(*req as f64),
                                            JsVal::Num(*rec as f64),
                                        ])
                                    })
                                    .collect(),
                            ),
                        )
                    }
                    62 => {
                        let tok = to_num(&c.val()) as usize;
                        match self.alliance_info(&mut tr, tok) {
                            Ok(v) => push_ok(&mut out, &v),
                            Err(m) => push_err(&mut out, &m),
                        }
                    }
                    63 => {
                        let tok = to_num(&c.val()) as usize;
                        match self.can_send_alliance_request(&mut tr, tok) {
                            Ok(b) => push_ok_bool(&mut out, b),
                            Err(m) => push_err(&mut out, &m),
                        }
                    }
                    64 => {
                        // mg.breakAlliance(this, alliance) — the alliance
                        // crosses as its index into the _alliances array.
                        let al = to_num(&c.val());
                        tr.push(123.0);
                        tr.push(al);
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    65 => {
                        // mg.removeAlliancesByPlayerSilently(this)
                        tr.push(124.0);
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    66 => {
                        let tok = to_num(&c.val()) as usize;
                        if self.alliance_with(tok).is_some() {
                            push_err(
                                &mut out,
                                "cannot create alliance request, already allies",
                            );
                        } else {
                            let v = take_jsval(&self.car, &mut self.car_i);
                            tr.push(122.0);
                            tr.push(tok as f64);
                            push_val(&mut tr, &v);
                            push_ok(&mut out, &v);
                        }
                    }
                    67 => {
                        let tok = to_num(&c.val()) as usize;
                        if self.self_tok == Some(tok) {
                            let m = format!(
                                "cannot get relation with self: {}",
                                self.player_to_string()
                            );
                            push_err(&mut out, &m);
                        } else {
                            let r = relation_from_value(self.relation_get(tok));
                            push_ok_num(&mut out, r);
                        }
                    }
                    68 => match self.all_relations_sorted(&mut tr) {
                        Ok(v) => push_ok(&mut out, &v),
                        Err(m) => push_err(&mut out, &m),
                    },
                    69 => {
                        let tok = to_num(&c.val()) as usize;
                        let delta = c.val();
                        if self.self_tok == Some(tok) {
                            let m = format!(
                                "cannot update relation with self: {}",
                                self.player_to_string()
                            );
                            push_err(&mut out, &m);
                        } else {
                            self.update_relation(tok, &delta);
                            push_ok(&mut out, &JsVal::Undef);
                        }
                    }
                    70 => {
                        self.decay_relations();
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    71 => {
                        let tok = to_num(&c.val()) as usize;
                        match self.can_target(&mut tr, tok) {
                            Ok(b) => push_ok_bool(&mut out, b),
                            Err(m) => push_err(&mut out, &m),
                        }
                    }
                    72 => {
                        // targets_.push({tick: mg.ticks(), target: other})
                        // THEN mg.target(this, other).
                        let tok = to_num(&c.val()) as usize;
                        let t = self.mg_ticks(&mut tr);
                        self.p.targets_.push((t, tok));
                        match self.other_small_id(tok) {
                            Ok(pid) => {
                                tr.push(125.0);
                                tr.push(pid);
                                push_ok(&mut out, &JsVal::Undef);
                            }
                            Err(m) => push_err(&mut out, &m),
                        }
                    }
                    73 => {
                        let ts = self.targets_body(&mut tr);
                        push_ok(
                            &mut out,
                            &JsVal::Arr(ts.into_iter().map(|t| JsVal::Num(t as f64)).collect()),
                        );
                    }
                    74 => {
                        let ts = self.transitive_targets(&mut tr);
                        push_ok(
                            &mut out,
                            &JsVal::Arr(ts.into_iter().map(|t| JsVal::Num(t as f64)).collect()),
                        );
                    }
                    75 => {
                        let recip = c.val();
                        let emoji = c.val();
                        let is_self =
                            matches!(&recip, JsVal::Num(t) if self.self_tok == Some(*t as usize));
                        if is_self {
                            let m = format!(
                                "Cannot send emoji to oneself: {}",
                                self.player_to_string()
                            );
                            push_err(&mut out, &m);
                        } else {
                            match self.emoji_recipient_id(&recip) {
                                Err(m) => push_err(&mut out, &m),
                                Ok(rid) => {
                                    let created = self.mg_ticks(&mut tr);
                                    let msg = JsVal::Obj(vec![
                                        ("message".to_string(), emoji),
                                        (
                                            "senderID".to_string(),
                                            JsVal::Num(self.p.small_id),
                                        ),
                                        ("recipientID".to_string(), rid),
                                        ("createdAt".to_string(), JsVal::Num(created)),
                                    ]);
                                    self.p.outgoing_emojis.push(msg.clone());
                                    // mg.sendEmojiUpdate(msg)
                                    tr.push(126.0);
                                    push_val(&mut tr, &msg);
                                    push_ok(&mut out, &JsVal::Undef);
                                }
                            }
                        }
                    }
                    76 => {
                        let v = self.outgoing_emojis_body(&mut tr);
                        push_ok(&mut out, &v);
                    }
                    77 => {
                        let recip = c.val();
                        match self.can_send_emoji(&mut tr, &recip) {
                            Ok(b) => push_ok_bool(&mut out, b),
                            Err(m) => push_err(&mut out, &m),
                        }
                    }
                    78 => {
                        // lastSentAt === undefined short-circuits BOTH
                        // facades (102 ticks + 116 quickChatCooldown).
                        let tok = to_num(&c.val()) as usize;
                        if self.self_tok == Some(tok) {
                            push_ok_bool(&mut out, false);
                        } else {
                            match self.other_small_id(tok) {
                                Err(m) => push_err(&mut out, &m),
                                Ok(sid) => {
                                    let last = self
                                        .p
                                        .quick_chats
                                        .iter()
                                        .find(|(k, _)| *k == sid)
                                        .map(|(_, v)| *v);
                                    match last {
                                        None => push_ok_bool(&mut out, true),
                                        Some(l) => {
                                            let t = self.mg_ticks(&mut tr) - l;
                                            let cd = script_f64(
                                                &self.qc,
                                                &mut self.qc_i,
                                                &mut tr,
                                                116.0,
                                            );
                                            push_ok_bool(&mut out, t >= cd);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    79 => {
                        // Map.set(k, v): the key (untraced smallID) evaluates
                        // before the 102 ticks.
                        let tok = to_num(&c.val()) as usize;
                        match self.other_small_id(tok) {
                            Err(m) => push_err(&mut out, &m),
                            Ok(sid) => {
                                let t = self.mg_ticks(&mut tr);
                                if let Some(e) =
                                    self.p.quick_chats.iter_mut().find(|(k, _)| *k == sid)
                                {
                                    e.1 = t;
                                } else {
                                    self.p.quick_chats.push((sid, t));
                                }
                                push_ok(&mut out, &JsVal::Undef);
                            }
                        }
                    }
                    80 | 81 => {
                        let tok = to_num(&c.val()) as usize;
                        match self.can_donate(&mut tr, tok, mid == 80) {
                            Ok(b) => push_ok_bool(&mut out, b),
                            Err(m) => push_err(&mut out, &m),
                        }
                    }
                    82 => {
                        let tok = to_num(&c.val()) as usize;
                        let troops = to_num(&c.val());
                        if self.self_tok == Some(tok) || troops <= 0.0 {
                            push_ok_bool(&mut out, false);
                        } else {
                            match self.remove_troops(troops) {
                                Err(m) => push_err(&mut out, &m),
                                Ok(0.0) => {
                                    push_ok_bool(&mut out, false)
                                }
                                Ok(removed) => match self.other_add_troops(&mut tr, tok, removed)
                                {
                                    Err(m) => push_err(&mut out, &m),
                                    Ok(()) => {
                                        let t = self.mg_ticks(&mut tr);
                                        self.p.sent_donations.push((tok, t));
                                        let u = self.donate_update("troops", tok);
                                        self.mg_add_update(&mut tr, &u);
                                        push_ok_bool(&mut out, true);
                                    }
                                },
                            }
                        }
                    }
                    83 => {
                        // The capture wraps the Gold arg in big() (the
                        // removeGold precedent); the `gold <= 0n` gate and
                        // removeGold's own `toRemove <= 0n` are observably
                        // identical, both folding into remove_gold_body.
                        let tok = to_num(&c.val()) as usize;
                        let gold_v = c.val();
                        if self.self_tok == Some(tok) {
                            push_ok_bool(&mut out, false);
                        } else {
                            match big_operand(&gold_v) {
                                Err(m) => push_err(&mut out, &m),
                                Ok(o) => match self.remove_gold_body(&o) {
                                    Err(m) => push_err(&mut out, &m),
                                    Ok(0) => push_ok_bool(&mut out, false),
                                    Ok(removed) => {
                                        match self.other_add_gold(&mut tr, tok, removed) {
                                            Err(m) => push_err(&mut out, &m),
                                            Ok(()) => {
                                                let t = self.mg_ticks(&mut tr);
                                                self.p.sent_donations.push((tok, t));
                                                let u =
                                                    self.donate_update("gold", tok);
                                                self.mg_add_update(&mut tr, &u);
                                                push_ok_bool(&mut out, true);
                                            }
                                        }
                                    }
                                },
                            }
                        }
                    }
                    84 => {
                        let t = self.mg_ticks(&mut tr) - self.p.last_delete_unit_tick;
                        let cd = script_f64(&self.duc, &mut self.duc_i, &mut tr, 120.0);
                        push_ok_bool(&mut out, t >= cd);
                    }
                    85 => {
                        self.p.last_delete_unit_tick = self.mg_ticks(&mut tr);
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    86 => match self.can_embargo_all(&mut tr) {
                        Ok(b) => push_ok_bool(&mut out, b),
                        Err(m) => push_err(&mut out, &m),
                    },
                    87 => {
                        self.p.last_embargo_all_tick = self.mg_ticks(&mut tr);
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    88 => {
                        let tok = to_num(&c.val()) as usize;
                        let oid = self.other_id(tok);
                        push_ok_bool(&mut out, self.has_embargo_id(&oid));
                    }
                    89 => {
                        let tok = to_num(&c.val()) as usize;
                        push_ok_bool(&mut out, self.can_trade(tok));
                    }
                    90 => push_ok(
                        &mut out,
                        &JsVal::Arr(
                            self.p
                                .embargoes
                                .iter()
                                .map(|(_, e)| {
                                    JsVal::Obj(vec![
                                        ("createdAt".to_string(), JsVal::Num(e.created_at)),
                                        ("isTemporary".to_string(), e.is_temporary.clone()),
                                        ("target".to_string(), JsVal::Num(e.target as f64)),
                                    ])
                                })
                                .collect(),
                        ),
                    ),
                    91 => {
                        let tok = to_num(&c.val()) as usize;
                        let temp = c.val();
                        match self.add_embargo(&mut tr, tok, temp) {
                            Ok(()) => push_ok(&mut out, &JsVal::Undef),
                            Err(m) => push_err(&mut out, &m),
                        }
                    }
                    92 => match self.stop_embargo(&mut tr, to_num(&c.val()) as usize) {
                        Ok(()) => push_ok(&mut out, &JsVal::Undef),
                        Err(m) => push_err(&mut out, &m),
                    },
                    93 => {
                        // endTemporaryEmbargo: a missing entry still runs
                        // stopEmbargo (the addUpdate is unconditional).
                        let tok = to_num(&c.val()) as usize;
                        let oid = self.other_id(tok);
                        let blocked = self
                            .p
                            .embargoes
                            .iter()
                            .find(|(k, _)| js_strict_eq(k, &oid))
                            .is_some_and(|(_, e)| !js_truthy(&e.is_temporary));
                        if blocked {
                            push_ok(&mut out, &JsVal::Undef);
                        } else {
                            match self.stop_embargo(&mut tr, tok) {
                                Ok(()) => push_ok(&mut out, &JsVal::Undef),
                                Err(m) => push_err(&mut out, &m),
                            }
                        }
                    }
                    94 => {
                        let players = self.mg_players(&mut tr);
                        let st = self.self_tok;
                        push_ok(
                            &mut out,
                            &JsVal::Arr(
                                players
                                    .into_iter()
                                    .filter(|tok| Some(*tok) != st && self.can_trade(*tok))
                                    .map(|t| JsVal::Num(t as f64))
                                    .collect(),
                            ),
                        );
                    }
                    k => unreachable!("player harness: unknown method id {k}"),
                }
                let mut full = Vec::with_capacity(1 + tr.len() + out.len());
                full.push(tr.len() as f64);
                full.extend(tr.iter().copied());
                full.extend(out.iter().copied());
                full
            }
            k => unreachable!("player harness: unknown op kind {k}"),
        }
    }
}
