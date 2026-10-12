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
//! `disconnectSnapshot` / `disconnectedAtTick` (1796-1826).
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
        _ => "unknown",
    }
}

/// One scripted player mock: pid, the `team()` script (codec values —
/// `Null` / `Str`) and the `isDisconnected()` script (f64, `!!`-normalised
/// into the trace).
#[derive(Clone, Debug)]
struct PlPlayer {
    pid: f64,
    teams: Vec<JsVal>,
    team_i: usize,
    discs: Vec<f64>,
    disc_i: usize,
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
    alliances: Vec<(usize, usize)>,
    spawn_tile: JsVal,
    is_disconnected: JsVal,
    disconnect_snapshot: JsVal,
    tile_change_version: f64,
    last_tile_change: f64,
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
            spawn_tile: JsVal::Undef,
            is_disconnected: JsVal::Bool(false),
            disconnect_snapshot: JsVal::Null,
            tile_change_version: 0.0,
            last_tile_change: 0.0,
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

    // --- identity helpers ---

    fn pi_field(&self, key: &str) -> JsVal {
        val_field(&self.p.player_info, key)
            .cloned()
            .unwrap_or(JsVal::Undef)
    }

    fn is_alive(&self) -> bool {
        self.p.tiles.size() > 0.0
    }

    fn alliance_with(&self, tok: usize) -> Option<(usize, usize)> {
        if self.self_tok == Some(tok) {
            return None;
        }
        self.p
            .alliances
            .iter()
            .find(|(req, rec)| *rec == tok || *req == tok)
            .copied()
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

    // --- the op dispatcher ---

    /// Run one op. Kind table (mirrored by `tools/gen_vectors.mjs`):
    /// 0 construct `[encVal playerInfo, smallID, encVal startTroops, encVal
    /// team, nAlliances, (req,rec)*, nTiles,(t)*, nBorder,(t)*, sgScript,
    /// ticksScript, teamTilesScript, totalLandScript, playersBlock, selfTok]`
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
    /// dumpBorderTiles 45 addBorderTile 46 removeBorderTile 47 isPlayer.
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
                let alliances: Vec<(usize, usize)> = (0..n_all)
                    .map(|_| (c.u(), c.u()))
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
                        PlPlayer {
                            pid,
                            teams,
                            team_i: 0,
                            discs,
                            disc_i: 0,
                        }
                    })
                    .collect();
                let self_tok = c.f();
                self.sg = sg;
                self.ticks = ticks;
                self.team_tiles = team_tiles;
                self.total_land = total_land;
                self.players = players;
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
                        // (a non-integer number); the method body then runs
                        // the cross-type relational gates (ToNumber, never
                        // throws) and the `_gold -= actual` subtraction.
                        match big_operand(&c.val()) {
                            Err(m) => push_err(&mut out, &m),
                            Ok(o) => {
                                let le0 = match &o {
                                    Op::Big(b) => *b <= 0,
                                    other => op_to_num(other) <= 0.0,
                                };
                                if le0 {
                                    push_ok_num(&mut out, 0.0);
                                } else {
                                    // minInt(_gold, toRemove): cross-type
                                    // relational converts the non-bigint
                                    // side with ToNumber (NaN -> false).
                                    let gold_lt = match (&self.p.gold, &o) {
                                        (Acc::Big(a), Op::Big(b)) => *a < *b,
                                        (Acc::Big(a), other) => {
                                            (*a as f64) < op_to_num(other)
                                        }
                                        (Acc::Str(s), Op::Big(b)) => {
                                            crate::game_config_helpers::js_number(s) < *b as f64
                                        }
                                        (Acc::Str(_), _) => false,
                                    };
                                    if gold_lt {
                                        // actual = _gold (only ever a Big —
                                        // the Str rows above are false);
                                        // _gold -= actual -> 0n.
                                        if let Acc::Big(a) = self.p.gold {
                                            self.p.gold = Acc::Big(0);
                                            push_ok_num(&mut out, a as f64);
                                        } else {
                                            unreachable!()
                                        }
                                    } else {
                                        // actual = toRemove; the subtraction
                                        // `_gold - actual` throws the mix
                                        // TypeError whenever actual is not a
                                        // bigint (or _gold is a Str).
                                        match (&self.p.gold, &o) {
                                            (Acc::Big(a), Op::Big(b)) => {
                                                let r = *b;
                                                self.p.gold = Acc::Big(a - b);
                                                push_ok_num(&mut out, r as f64);
                                            }
                                            _ => push_err(&mut out, MIX_ERR),
                                        }
                                    }
                                }
                            }
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
                            Some((req, rec)) => push_ok(
                                &mut out,
                                &JsVal::Arr(vec![
                                    JsVal::Num(req as f64),
                                    JsVal::Num(rec as f64),
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
                                .map(|(req, rec)| {
                                    JsVal::Arr(vec![JsVal::Num(*req as f64), JsVal::Num(*rec as f64)])
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
