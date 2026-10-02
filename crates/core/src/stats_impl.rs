//! Port of `src/core/game/StatsImpl.ts`: the pure bigint stats accumulator.
//! The `Player` / `TerraNullius` facade is a scripted mock (precedent:
//! `nation_utils`): every facade call (`clientID()` / `type()` / `isPlayer()`)
//! consumes the player table and records its arguments + return into a flat
//! trace, so the facade call order and the per-op `_bigint` throw points are
//! pinned in the res stream; the final `stats()` dump pins the `data` object
//! key insertion order and, per player, the `PlayerStats` field insertion
//! order and the boats/bombs/units object key order.
//!
//! Faithfulness notes:
//!
//! * bigint crosses the boundary as `i64` (precedent `stats_schemas::toBigInt`):
//!   the capture domain keeps every value |v| <= 2^53 so the f64 token is
//!   exact; overflow scenarios are deliberately not captured.
//! * `_bigint`: bigint passthrough; number -> `BigInt(Math.floor(v))`.
//!   `Math.floor(NaN)`/`Math.floor(±Inf)` make `BigInt` throw a `RangeError`
//!   (`None` here, surfaced as the `[3, opKind, 1]` trace event). The throw
//!   lands *where the TS lands it*: after the `??=` init and the `while`
//!   growth (e.g. `_addAttack` leaves the zero-grown array behind), after the
//!   conquest `type()` facade call (`goldWar`), or before any write
//!   (`_addPlayerKilled` evaluates the RHS first). `-0` floors to `-0` and
//!   `BigInt(-0)` is `0n`; fractional numbers floor toward -Infinity
//!   (`BigInt(Math.floor(-1.5))` is `-2n`).
//! * `attackCancel`'s `-troops` follows the input shape: a bigint negates as
//!   bigint (passthrough in `_bigint`), a number negates as number and then
//!   floors (`-2.5` -> `-3n`, `-NaN` -> throw, `-(-0)` -> `0n`).
//! * `while (arr.length <= index) arr.push(0n)` with a negative index would
//!   skip the loop and turn `arr[-1] += v` into a JS *property* write — every
//!   call site passes a `*_INDEX_*` constant (non-negative), so the negative
//!   path is dead through the ported surface and is not captured.
//! * `clientID in this.data` walks the prototype chain: a clientID of
//!   `"constructor"` / `"toString"` would resolve to the inherited member and
//!   `_makePlayerStats` would return the function object (later `??=` writes
//!   then mutate `Object`'s static function, invisible to the dump). Real
//!   client IDs are nanoid strings; the capture domain keeps non-prototype
//!   keys and the `in` check is modelled as an own-key lookup. Out of domain.
//! * Computed keys stringify: `p.boats ??= { [type]: [0n] }` and the
//!   `unitTypeToBombUnit[nukeType]` / `unitTypeToOtherUnit[otherUnitType]`
//!   lookups yield the key `"undefined"` for a type outside the table (the TS
//!   type system forbids it; the scripted mock can still pass one — captured).
//! * `??=` fires only on null/undefined, so field presence == "written at
//!   least once"; `killedBy` stores JS `null` as a *valid* recorded value
//!   (first-write-wins via the `=== undefined` test), and `deathPosition`
//!   `??=` keeps the first position (including `-0`).
//! * `recordTickSample`: `p.tiles ??= [0n, 0n, 0n]` starts at length 3 and
//!   `while (length <= TILE_INDEX_DRAWDOWN_TROUGH)` (2) never grows it. The
//!   drawdown update compares cross-multiplied products — exact integer
//!   arithmetic, modelled in `i128` (products of |v| <= 2^53 operands fit).
//!   `peak - t` and `ddPeak - ddTrough` are bigint (negative allowed); the
//!   comparison is strict `>`, ties never rewrite the pair.
//! * `conquest_by_type` is the `PlayerType` string-enum keyed table
//!   (`"HUMAN"`/`"NATION"`/`"BOT"` -> `PLAYER_INDEX_*`); `goldWar` reads
//!   `conquest_by_type[captured.type()]` and skips `_addConquest` when the
//!   (mocked) type is outside the table — the `undefined` branch.
//! * `recordKill` filters before touching stats: `victim.type() !== "HUMAN"`
//!   returns after the one `type()` facade call; a null `victim.clientID()`
//!   returns after `type()` + `clientID()`; a null *killer* clientID returns
//!   after the victim's facade calls but before any stats write.
//! * `lobbyFillTime` is a no-op in the TS — no trace event is emitted.
//! * `getPlayerStats` returns `undefined` for a null clientID *and* for an
//!   untouched player; both pin as the `[4, refid, 0]` result event.

use crate::stats_schemas::{
    ALLIANCE_INDEX_BROKEN_BY_OTHER, ALLIANCE_INDEX_EXPIRED, ALLIANCE_INDEX_FORMED,
    ALLIANCE_INDEX_HELD_TO_END, ALLIANCE_INDEX_LONGEST_HELD, ALLIANCE_INDEX_PEAK_CONCURRENT,
    ATTACK_INDEX_CANCEL, ATTACK_INDEX_MAX_RECV, ATTACK_INDEX_RECV, ATTACK_INDEX_SENT,
    BOAT_INDEX_ARRIVE, BOAT_INDEX_CAPTURE, BOAT_INDEX_DESTROY, BOAT_INDEX_SENT,
    BOMB_INDEX_INTERCEPT, BOMB_INDEX_LAND, BOMB_INDEX_LAUNCH, GOLD_INDEX_STEAL,
    GOLD_INDEX_TRADE, GOLD_INDEX_TRAIN_OTHER, GOLD_INDEX_TRAIN_SELF, GOLD_INDEX_WAR,
    GOLD_INDEX_WORK, OTHER_INDEX_BUILT, OTHER_INDEX_CAPTURE, OTHER_INDEX_DESTROY,
    OTHER_INDEX_LOST, OTHER_INDEX_UPGRADE, PLAYER_INDEX_BOT, PLAYER_INDEX_HUMAN,
    PLAYER_INDEX_NATION, TILE_INDEX_DRAWDOWN_PEAK, TILE_INDEX_DRAWDOWN_TROUGH,
    TILE_INDEX_PEAK, unit_type_to_bomb_unit, unit_type_to_other_unit,
};

/// `PlayerType.Human` / `PlayerType.MIRV`-adjacent enum *values* (Game.ts is a
/// string enum; the prepared copy inlines them as plain objects).
const PLAYER_TYPE_HUMAN: &str = "HUMAN";
const PLAYER_TYPE_NATION: &str = "NATION";
const PLAYER_TYPE_BOT: &str = "BOT";
const UNIT_TYPE_MIRV: &str = "MIRV";

/// `conquest_by_type` — computed keys over the `PlayerType` string enum; an
/// unknown (mocked) type reads `undefined` and skips the conquest.
fn conquest_by_type(t: &str) -> Option<usize> {
    match t {
        PLAYER_TYPE_HUMAN => Some(PLAYER_INDEX_HUMAN),
        PLAYER_TYPE_NATION => Some(PLAYER_INDEX_NATION),
        PLAYER_TYPE_BOT => Some(PLAYER_INDEX_BOT),
        _ => None,
    }
}

/// One `BigIntLike` runtime shape: a JS `bigint` (modelled as `i64`; capture
/// domain |v| <= 2^53) or a JS `number`.
#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Bi(i64),
    Num(f64),
}

/// `-troops` at the `attackCancel` call sites: bigint negates as bigint,
/// number negates as number (then `_bigint` floors; NaN throws).
fn neg(v: &Val) -> Val {
    match v {
        Val::Bi(b) => Val::Bi(-*b),
        Val::Num(n) => Val::Num(-*n),
    }
}

/// `_bigint(value)` — `None` is the `BigInt(NaN)` / `BigInt(±Infinity)`
/// RangeError throw. `BigInt(Math.floor(-0))` is `0n`.
fn to_big(v: &Val) -> Option<i64> {
    match v {
        Val::Bi(b) => Some(*b),
        Val::Num(n) => {
            let f = n.floor();
            if f.is_finite() {
                Some(f as i64)
            } else {
                None
            }
        }
    }
}

// PlayerStats field ids (the dump walks `order`, so the TS property-name
// insertion order is pinned field-by-field).
const F_ATTACKS: u8 = 0;
const F_BETRAYALS: u8 = 1;
const F_KILLED_AT: u8 = 2;
const F_KILLED_BY: u8 = 3;
const F_DEATH_POSITION: u8 = 4;
const F_FINAL_TILES: u8 = 5;
const F_KILLS: u8 = 6;
const F_CONQUESTS: u8 = 7;
const F_BOATS: u8 = 8;
const F_BOMBS: u8 = 9;
const F_GOLD: u8 = 10;
const F_UNITS: u8 = 11;
const F_TILES: u8 = 12;
const F_ALLIANCES: u8 = 13;
const F_PEAK_TROOPS: u8 = 14;

/// The `PlayerStats` property names (TS spelling), indexed by field id.
const FIELD_NAMES: [&str; 15] = [
    "attacks",
    "betrayals",
    "killedAt",
    "killedBy",
    "deathPosition",
    "finalTiles",
    "kills",
    "conquests",
    "boats",
    "bombs",
    "gold",
    "units",
    "tiles",
    "alliances",
    "peakTroops",
];

/// An ordered JS object of bigint arrays (`boats` / `bombs` / `units`).
type ObjArrays = Vec<(String, Vec<i64>)>;

/// `PlayerStats` — every field is tri-state (absent / present), with the
/// `order` list pinning the JS key insertion order.
#[derive(Default)]
pub struct PlayerStats {
    order: Vec<u8>,
    attacks: Option<Vec<i64>>,
    betrayals: Option<i64>,
    killed_at: Option<i64>,
    /// `None` = absent; `Some(None)` = JS `null` (a valid recorded value).
    killed_by: Option<Option<String>>,
    death_position: Option<f64>,
    final_tiles: Option<i64>,
    kills: Option<Vec<(String, i64)>>,
    conquests: Option<Vec<i64>>,
    boats: Option<ObjArrays>,
    bombs: Option<ObjArrays>,
    gold: Option<Vec<i64>>,
    units: Option<ObjArrays>,
    tiles: Option<Vec<i64>>,
    alliances: Option<Vec<i64>>,
    peak_troops: Option<i64>,
}

impl PlayerStats {
    fn mark(&mut self, f: u8) {
        if !self.order.contains(&f) {
            self.order.push(f);
        }
    }

    /// `arr ??= [0n; init]` + `while (arr.length <= index) arr.push(0n)`.
    fn grow_arr(o: &mut Option<Vec<i64>>, init: usize, index: usize) -> &mut Vec<i64> {
        let a = o.get_or_insert_with(|| vec![0; init]);
        while a.len() <= index {
            a.push(0);
        }
        a
    }

    /// `obj ??= {}` + `obj[key] ??= []` — computed keys stringify, so `key`
    /// may be the literal `"undefined"` (the caller maps the TS `undefined`).
    fn obj_arr<'a>(o: &'a mut Option<ObjArrays>, key: &str) -> &'a mut Vec<i64> {
        let m = o.get_or_insert_with(Vec::new);
        if let Some(i) = m.iter().position(|(k, _)| k == key) {
            return &mut m[i].1;
        }
        m.push((key.to_string(), Vec::new()));
        &mut m.last_mut().unwrap().1
    }
}

/// `StatsImpl`. `data` is a JS `{}` — insertion-ordered string keys.
#[derive(Default)]
pub struct StatsImpl {
    pub data: Vec<(String, PlayerStats)>,
    pub num_mirv_launched: i64,
}

/// One scripted facade player: `clientID()` / `type()` / `isPlayer()` returns.
#[derive(Debug, Clone)]
pub struct MP {
    pub cid: Option<String>,
    pub ptype: String,
    pub is_player: bool,
}

fn push_string(out: &mut Vec<f64>, s: &str) {
    let units: Vec<u16> = s.encode_utf16().collect();
    out.push(units.len() as f64);
    out.extend(units.iter().map(|&u| u as f64));
}

fn enc_cid(out: &mut Vec<f64>, cid: &Option<String>) {
    match cid {
        None => out.push(0.0),
        Some(s) => {
            out.push(1.0);
            push_string(out, s);
        }
    }
}

/// `player.clientID()` — trace event `[0, refid, cidEnc]`.
fn fac_client_id(trace: &mut Vec<f64>, players: &[MP], r: usize) -> Option<String> {
    trace.push(0.0);
    trace.push(r as f64);
    let cid = players[r].cid.clone();
    enc_cid(trace, &cid);
    cid
}

/// `player.type()` — trace event `[1, refid, (str)type]`.
fn fac_type(trace: &mut Vec<f64>, players: &[MP], r: usize) -> String {
    trace.push(1.0);
    trace.push(r as f64);
    let t = players[r].ptype.clone();
    push_string(trace, &t);
    t
}

/// `player.isPlayer()` — trace event `[2, refid, 0|1]`.
fn fac_is_player(trace: &mut Vec<f64>, players: &[MP], r: usize) -> bool {
    trace.push(2.0);
    trace.push(r as f64);
    let b = players[r].is_player;
    trace.push(if b { 1.0 } else { 0.0 });
    b
}

/// The TS `undefined` / throw signalling: `Err(())` unwinds the public call
/// exactly where the `RangeError` escapes the TS method.
type Js = Result<(), ()>;

impl StatsImpl {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// `_makePlayerStats` — null clientID returns `undefined`; the `in` test
    /// is an own-key lookup (prototype keys out of domain, see module doc).
    fn make_player_stats(&mut self, trace: &mut Vec<f64>, players: &[MP], r: usize) -> Option<usize> {
        let cid = fac_client_id(trace, players, r)?;
        if let Some(i) = self.data.iter().position(|(k, _)| *k == cid) {
            return Some(i);
        }
        self.data.push((cid, PlayerStats::default()));
        Some(self.data.len() - 1)
    }

    fn add_attack(&mut self, trace: &mut Vec<f64>, players: &[MP], r: usize, index: usize, value: &Val) -> Js {
        let Some(pi) = self.make_player_stats(trace, players, r) else {
            return Ok(());
        };
        PlayerStats::grow_arr(&mut self.data[pi].1.attacks, 1, index);
        self.data[pi].1.mark(F_ATTACKS);
        let v = to_big(value).ok_or(())?;
        self.data[pi].1.attacks.as_mut().unwrap()[index] += v;
        Ok(())
    }

    fn max_attack(&mut self, trace: &mut Vec<f64>, players: &[MP], r: usize, index: usize, value: &Val) -> Js {
        let Some(pi) = self.make_player_stats(trace, players, r) else {
            return Ok(());
        };
        PlayerStats::grow_arr(&mut self.data[pi].1.attacks, 1, index);
        self.data[pi].1.mark(F_ATTACKS);
        let v = to_big(value).ok_or(())?;
        let a = self.data[pi].1.attacks.as_mut().unwrap();
        if v > a[index] {
            a[index] = v;
        }
        Ok(())
    }

    fn add_betrayal(&mut self, trace: &mut Vec<f64>, players: &[MP], r: usize, value: &Val) -> Js {
        let Some(pi) = self.make_player_stats(trace, players, r) else {
            return Ok(());
        };
        self.data[pi].1.mark(F_BETRAYALS);
        self.data[pi].1.betrayals.get_or_insert(0);
        let v = to_big(value).ok_or(())?;
        *self.data[pi].1.betrayals.as_mut().unwrap() += v;
        Ok(())
    }

    fn add_boat(&mut self, trace: &mut Vec<f64>, players: &[MP], r: usize, btype: &str, index: usize, value: &Val) -> Js {
        let Some(pi) = self.make_player_stats(trace, players, r) else {
            return Ok(());
        };
        self.data[pi].1.mark(F_BOATS);
        let p = &mut self.data[pi].1;
        let a = PlayerStats::obj_arr(&mut p.boats, btype);
        while a.len() <= index {
            a.push(0);
        }
        let v = to_big(value).ok_or(())?;
        a[index] += v;
        Ok(())
    }

    fn add_bomb(&mut self, trace: &mut Vec<f64>, players: &[MP], r: usize, nuke: &str, index: usize, value: &Val) -> Js {
        // `unitTypeToBombUnit[nukeType]` — an off-table nuke type reads
        // `undefined`, whose computed key stringifies to "undefined".
        let btype = unit_type_to_bomb_unit(nuke).unwrap_or("undefined");
        let Some(pi) = self.make_player_stats(trace, players, r) else {
            return Ok(());
        };
        self.data[pi].1.mark(F_BOMBS);
        let p = &mut self.data[pi].1;
        let a = PlayerStats::obj_arr(&mut p.bombs, btype);
        while a.len() <= index {
            a.push(0);
        }
        let v = to_big(value).ok_or(())?;
        a[index] += v;
        Ok(())
    }

    fn add_gold(&mut self, trace: &mut Vec<f64>, players: &[MP], r: usize, index: usize, value: &Val) -> Js {
        let Some(pi) = self.make_player_stats(trace, players, r) else {
            return Ok(());
        };
        PlayerStats::grow_arr(&mut self.data[pi].1.gold, 1, index);
        self.data[pi].1.mark(F_GOLD);
        let v = to_big(value).ok_or(())?;
        self.data[pi].1.gold.as_mut().unwrap()[index] += v;
        Ok(())
    }

    fn add_other_unit(&mut self, trace: &mut Vec<f64>, players: &[MP], r: usize, otype: &str, index: usize, value: &Val) -> Js {
        let utype = unit_type_to_other_unit(otype).unwrap_or("undefined");
        let Some(pi) = self.make_player_stats(trace, players, r) else {
            return Ok(());
        };
        self.data[pi].1.mark(F_UNITS);
        let p = &mut self.data[pi].1;
        let a = PlayerStats::obj_arr(&mut p.units, utype);
        while a.len() <= index {
            a.push(0);
        }
        let v = to_big(value).ok_or(())?;
        a[index] += v;
        Ok(())
    }

    fn add_conquest(&mut self, trace: &mut Vec<f64>, players: &[MP], r: usize, index: usize) -> Js {
        let Some(pi) = self.make_player_stats(trace, players, r) else {
            return Ok(());
        };
        PlayerStats::grow_arr(&mut self.data[pi].1.conquests, 1, index);
        self.data[pi].1.mark(F_CONQUESTS);
        self.data[pi].1.conquests.as_mut().unwrap()[index] += 1;
        Ok(())
    }

    fn add_player_killed(&mut self, trace: &mut Vec<f64>, players: &[MP], r: usize, tick: &Val) -> Js {
        let Some(pi) = self.make_player_stats(trace, players, r) else {
            return Ok(());
        };
        let v = to_big(tick).ok_or(())?;
        self.data[pi].1.killed_at = Some(v);
        self.data[pi].1.mark(F_KILLED_AT);
        Ok(())
    }

    fn alliance_array(&mut self, pi: usize, index: usize) {
        let p = &mut self.data[pi].1;
        PlayerStats::grow_arr(&mut p.alliances, 1, index);
        p.mark(F_ALLIANCES);
    }

    fn max_alliance(&mut self, trace: &mut Vec<f64>, players: &[MP], r: usize, index: usize, value: &Val) -> Js {
        let Some(pi) = self.make_player_stats(trace, players, r) else {
            return Ok(());
        };
        self.alliance_array(pi, index);
        let v = to_big(value).ok_or(())?;
        let a = self.data[pi].1.alliances.as_mut().unwrap();
        if v > a[index] {
            a[index] = v;
        }
        Ok(())
    }

    fn add_alliance(&mut self, trace: &mut Vec<f64>, players: &[MP], r: usize, index: usize, value: &Val) -> Js {
        let Some(pi) = self.make_player_stats(trace, players, r) else {
            return Ok(());
        };
        self.alliance_array(pi, index);
        let v = to_big(value).ok_or(())?;
        self.data[pi].1.alliances.as_mut().unwrap()[index] += v;
        Ok(())
    }

    fn set_alliance(&mut self, trace: &mut Vec<f64>, players: &[MP], r: usize, index: usize, value: &Val) -> Js {
        let Some(pi) = self.make_player_stats(trace, players, r) else {
            return Ok(());
        };
        self.alliance_array(pi, index);
        let v = to_big(value).ok_or(())?;
        self.data[pi].1.alliances.as_mut().unwrap()[index] = v;
        Ok(())
    }

    pub(crate) fn attack(&mut self, trace: &mut Vec<f64>, players: &[MP], pr: usize, tr: usize, troops: &Val) -> Js {
        self.add_attack(trace, players, pr, ATTACK_INDEX_SENT, troops)?;
        if fac_is_player(trace, players, tr) {
            self.add_attack(trace, players, tr, ATTACK_INDEX_RECV, troops)?;
        }
        Ok(())
    }

    pub(crate) fn attack_max_incoming(&mut self, trace: &mut Vec<f64>, players: &[MP], tr: usize, troops: &Val) -> Js {
        if !fac_is_player(trace, players, tr) {
            return Ok(());
        }
        self.max_attack(trace, players, tr, ATTACK_INDEX_MAX_RECV, troops)
    }

    pub(crate) fn attack_cancel(&mut self, trace: &mut Vec<f64>, players: &[MP], pr: usize, tr: usize, troops: &Val) -> Js {
        self.add_attack(trace, players, pr, ATTACK_INDEX_CANCEL, troops)?;
        self.add_attack(trace, players, pr, ATTACK_INDEX_SENT, &neg(troops))?;
        if fac_is_player(trace, players, tr) {
            self.add_attack(trace, players, tr, ATTACK_INDEX_RECV, &neg(troops))?;
        }
        Ok(())
    }

    pub(crate) fn betray(&mut self, trace: &mut Vec<f64>, players: &[MP], pr: usize) -> Js {
        self.add_betrayal(trace, players, pr, &Val::Bi(1))
    }

    pub(crate) fn alliance_formed(&mut self, trace: &mut Vec<f64>, players: &[MP], pr: usize) -> Js {
        self.add_alliance(trace, players, pr, ALLIANCE_INDEX_FORMED, &Val::Bi(1))
    }

    pub(crate) fn alliance_ended(
        &mut self,
        trace: &mut Vec<f64>,
        players: &[MP],
        pr: usize,
        duration_ticks: &Val,
        counter: Option<&str>,
    ) -> Js {
        if counter == Some("brokenByOther") {
            self.add_alliance(trace, players, pr, ALLIANCE_INDEX_BROKEN_BY_OTHER, &Val::Bi(1))?;
        } else if counter == Some("expired") {
            self.add_alliance(trace, players, pr, ALLIANCE_INDEX_EXPIRED, &Val::Bi(1))?;
        }
        self.max_alliance(trace, players, pr, ALLIANCE_INDEX_LONGEST_HELD, duration_ticks)
    }

    pub(crate) fn boat_send_trade(&mut self, trace: &mut Vec<f64>, players: &[MP], pr: usize) -> Js {
        self.add_boat(trace, players, pr, "trade", BOAT_INDEX_SENT, &Val::Bi(1))
    }

    pub(crate) fn boat_arrive_trade(&mut self, trace: &mut Vec<f64>, players: &[MP], pr: usize, tr: usize, gold: &Val) -> Js {
        self.add_boat(trace, players, pr, "trade", BOAT_INDEX_ARRIVE, &Val::Bi(1))?;
        self.add_gold(trace, players, pr, GOLD_INDEX_TRADE, gold)?;
        self.add_gold(trace, players, tr, GOLD_INDEX_TRADE, gold)
    }

    /// `boatCapturedTrade(player, target, gold)` — the TS ignores `target`
    /// (the steal is credited to the captor only); the argument is still read
    /// from the op stream so the token layout matches the capture.
    pub(crate) fn boat_captured_trade(&mut self, trace: &mut Vec<f64>, players: &[MP], pr: usize, _tr: usize, gold: &Val) -> Js {
        self.add_boat(trace, players, pr, "trade", BOAT_INDEX_CAPTURE, &Val::Bi(1))?;
        self.add_gold(trace, players, pr, GOLD_INDEX_STEAL, gold)
    }

    pub(crate) fn boat_destroy_trade(&mut self, trace: &mut Vec<f64>, players: &[MP], pr: usize) -> Js {
        self.add_boat(trace, players, pr, "trade", BOAT_INDEX_DESTROY, &Val::Bi(1))
    }

    pub(crate) fn boat_send_troops(&mut self, trace: &mut Vec<f64>, players: &[MP], pr: usize) -> Js {
        self.add_boat(trace, players, pr, "trans", BOAT_INDEX_SENT, &Val::Bi(1))
    }

    pub(crate) fn boat_arrive_troops(&mut self, trace: &mut Vec<f64>, players: &[MP], pr: usize) -> Js {
        self.add_boat(trace, players, pr, "trans", BOAT_INDEX_ARRIVE, &Val::Bi(1))
    }

    pub(crate) fn boat_destroy_troops(&mut self, trace: &mut Vec<f64>, players: &[MP], pr: usize) -> Js {
        self.add_boat(trace, players, pr, "trans", BOAT_INDEX_DESTROY, &Val::Bi(1))
    }

    pub(crate) fn boat_captured_troops(&mut self, trace: &mut Vec<f64>, players: &[MP], pr: usize) -> Js {
        self.add_boat(trace, players, pr, "trans", BOAT_INDEX_CAPTURE, &Val::Bi(1))
    }

    /// `bombLaunch(player, target, type)` — `target` is unused by the TS but
    /// rides the op stream; the MIRV counter increments before `_addBomb`.
    pub(crate) fn bomb_launch(&mut self, trace: &mut Vec<f64>, players: &[MP], pr: usize, _tr: usize, nuke: &str) -> Js {
        if nuke == UNIT_TYPE_MIRV {
            self.num_mirv_launched += 1;
        }
        self.add_bomb(trace, players, pr, nuke, BOMB_INDEX_LAUNCH, &Val::Bi(1))
    }

    pub(crate) fn bomb_land(&mut self, trace: &mut Vec<f64>, players: &[MP], pr: usize, _tr: usize, nuke: &str) -> Js {
        self.add_bomb(trace, players, pr, nuke, BOMB_INDEX_LAND, &Val::Bi(1))
    }

    pub(crate) fn bomb_intercept(&mut self, trace: &mut Vec<f64>, players: &[MP], pr: usize, nuke: &str, count: &Val) -> Js {
        self.add_bomb(trace, players, pr, nuke, BOMB_INDEX_INTERCEPT, count)
    }

    pub(crate) fn gold_work(&mut self, trace: &mut Vec<f64>, players: &[MP], pr: usize, gold: &Val) -> Js {
        self.add_gold(trace, players, pr, GOLD_INDEX_WORK, gold)
    }

    pub(crate) fn gold_war(&mut self, trace: &mut Vec<f64>, players: &[MP], pr: usize, cr: usize, gold: &Val) -> Js {
        self.add_gold(trace, players, pr, GOLD_INDEX_WAR, gold)?;
        let ctype = fac_type(trace, players, cr);
        if let Some(index) = conquest_by_type(&ctype) {
            self.add_conquest(trace, players, pr, index)?;
        }
        Ok(())
    }

    pub(crate) fn unit_build(&mut self, trace: &mut Vec<f64>, players: &[MP], pr: usize, otype: &str) -> Js {
        self.add_other_unit(trace, players, pr, otype, OTHER_INDEX_BUILT, &Val::Bi(1))
    }

    pub(crate) fn unit_capture(&mut self, trace: &mut Vec<f64>, players: &[MP], pr: usize, otype: &str) -> Js {
        self.add_other_unit(trace, players, pr, otype, OTHER_INDEX_CAPTURE, &Val::Bi(1))
    }

    pub(crate) fn unit_upgrade(&mut self, trace: &mut Vec<f64>, players: &[MP], pr: usize, otype: &str) -> Js {
        self.add_other_unit(trace, players, pr, otype, OTHER_INDEX_UPGRADE, &Val::Bi(1))
    }

    pub(crate) fn unit_destroy(&mut self, trace: &mut Vec<f64>, players: &[MP], pr: usize, otype: &str) -> Js {
        self.add_other_unit(trace, players, pr, otype, OTHER_INDEX_DESTROY, &Val::Bi(1))
    }

    pub(crate) fn unit_lose(&mut self, trace: &mut Vec<f64>, players: &[MP], pr: usize, otype: &str) -> Js {
        self.add_other_unit(trace, players, pr, otype, OTHER_INDEX_LOST, &Val::Bi(1))
    }

    pub(crate) fn player_killed(&mut self, trace: &mut Vec<f64>, players: &[MP], pr: usize, tick: &Val) -> Js {
        self.add_player_killed(trace, players, pr, tick)
    }

    pub(crate) fn record_final_tiles(&mut self, trace: &mut Vec<f64>, players: &[MP], pr: usize, tiles: &Val) -> Js {
        let Some(pi) = self.make_player_stats(trace, players, pr) else {
            return Ok(());
        };
        let v = to_big(tiles).ok_or(())?;
        self.data[pi].1.final_tiles = Some(v);
        self.data[pi].1.mark(F_FINAL_TILES);
        Ok(())
    }

    pub(crate) fn record_alliances_at_end(&mut self, trace: &mut Vec<f64>, players: &[MP], pr: usize, still_standing: &Val, longest: &Val) -> Js {
        self.set_alliance(trace, players, pr, ALLIANCE_INDEX_HELD_TO_END, still_standing)?;
        self.max_alliance(trace, players, pr, ALLIANCE_INDEX_LONGEST_HELD, longest)
    }

    pub(crate) fn record_tick_sample(
        &mut self,
        trace: &mut Vec<f64>,
        players: &[MP],
        pr: usize,
        tiles: &Val,
        troops: &Val,
        alliance_count: f64,
    ) -> Js {
        let Some(pi) = self.make_player_stats(trace, players, pr) else {
            return Ok(());
        };
        // `const t = _bigint(tiles)` runs before the ??=, so a throw leaves
        // the player entry (made by _makePlayerStats) but no tiles field.
        let t = to_big(tiles).ok_or(())?;
        {
            let p = &mut self.data[pi].1;
            if p.tiles.is_none() {
                p.tiles = Some(vec![0, 0, 0]);
                p.mark(F_TILES);
            }
            let a = p.tiles.as_mut().unwrap();
            // length 3 > TILE_INDEX_DRAWDOWN_TROUGH (2): the while never grows.
            while a.len() <= TILE_INDEX_DRAWDOWN_TROUGH {
                a.push(0);
            }
            if t > a[TILE_INDEX_PEAK] {
                a[TILE_INDEX_PEAK] = t;
            }
            let peak = a[TILE_INDEX_PEAK];
            let dd_peak = a[TILE_INDEX_DRAWDOWN_PEAK];
            let dd_trough = a[TILE_INDEX_DRAWDOWN_TROUGH];
            // Cross-multiplied exact bigint compare, in i128 (|v| <= 2^53
            // operands keep every product inside i128).
            if dd_peak == 0
                || i128::from(peak - t) * i128::from(dd_peak)
                    > i128::from(dd_peak - dd_trough) * i128::from(peak)
            {
                a[TILE_INDEX_DRAWDOWN_PEAK] = peak;
                a[TILE_INDEX_DRAWDOWN_TROUGH] = t;
            }
        }
        // The tiles update has already landed when the troops coercion throws.
        let tr = to_big(troops).ok_or(())?;
        {
            let p = &mut self.data[pi].1;
            if p.peak_troops.is_none() || tr > p.peak_troops.unwrap() {
                p.peak_troops = Some(tr);
                p.mark(F_PEAK_TROOPS);
            }
        }
        self.max_alliance(
            trace,
            players,
            pr,
            ALLIANCE_INDEX_PEAK_CONCURRENT,
            &Val::Num(alliance_count),
        )
    }

    pub(crate) fn record_killed_by(&mut self, trace: &mut Vec<f64>, players: &[MP], vr: usize, killer: Option<String>) -> Js {
        let Some(pi) = self.make_player_stats(trace, players, vr) else {
            return Ok(());
        };
        if self.data[pi].1.killed_by.is_none() {
            self.data[pi].1.killed_by = Some(killer);
            self.data[pi].1.mark(F_KILLED_BY);
        }
        Ok(())
    }

    pub(crate) fn record_death_position(&mut self, trace: &mut Vec<f64>, players: &[MP], vr: usize, position: f64) -> Js {
        let Some(pi) = self.make_player_stats(trace, players, vr) else {
            return Ok(());
        };
        if self.data[pi].1.death_position.is_none() {
            self.data[pi].1.death_position = Some(position);
            self.data[pi].1.mark(F_DEATH_POSITION);
        }
        Ok(())
    }

    pub(crate) fn record_kill(&mut self, trace: &mut Vec<f64>, players: &[MP], pr: usize, vr: usize, tick: &Val) -> Js {
        if fac_type(trace, players, vr) != PLAYER_TYPE_HUMAN {
            return Ok(());
        }
        let Some(victim_id) = fac_client_id(trace, players, vr) else {
            return Ok(());
        };
        let Some(pi) = self.make_player_stats(trace, players, pr) else {
            return Ok(());
        };
        // `p.kills ??= []` inserts the key before the object literal (and its
        // `_bigint(tick)` argument) is evaluated — a throw leaves `kills: []`.
        self.data[pi].1.kills.get_or_insert_with(Vec::new);
        self.data[pi].1.mark(F_KILLS);
        let v = to_big(tick).ok_or(())?;
        self.data[pi]
            .1
            .kills
            .as_mut()
            .unwrap()
            .push((victim_id, v));
        Ok(())
    }

    pub(crate) fn train_self_trade(&mut self, trace: &mut Vec<f64>, players: &[MP], pr: usize, gold: &Val) -> Js {
        self.add_gold(trace, players, pr, GOLD_INDEX_TRAIN_SELF, gold)
    }

    pub(crate) fn train_external_trade(&mut self, trace: &mut Vec<f64>, players: &[MP], pr: usize, gold: &Val) -> Js {
        self.add_gold(trace, players, pr, GOLD_INDEX_TRAIN_OTHER, gold)
    }

    /// `numMirvsLaunched()` — trace event `[5, value]`.
    pub(crate) fn num_mirvs_launched(&mut self, trace: &mut Vec<f64>) -> i64 {
        trace.push(5.0);
        trace.push(self.num_mirv_launched as f64);
        self.num_mirv_launched
    }

    /// `getPlayerStats(player)` — the clientID facade call, then the result
    /// event `[4, refid, present]` (undefined encodes as present 0; a present
    /// entry's *value* is not surfaced — only presence is observable here).
    pub(crate) fn get_player_stats(&mut self, trace: &mut Vec<f64>, players: &[MP], r: usize) {
        let present = match fac_client_id(trace, players, r) {
            None => false,
            Some(cid) => self.data.iter().any(|(k, _)| *k == cid),
        };
        trace.push(4.0);
        trace.push(r as f64);
        trace.push(if present { 1.0 } else { 0.0 });
    }

    /// `stats()` — trace event `[6]`; the dump rides in the payload.
    pub(crate) fn stats(&mut self, trace: &mut Vec<f64>) {
        trace.push(6.0);
    }

    /// The final `stats()` dump: `[nP, (cid, nFields, (name, enc)*)*]`.
    pub(crate) fn dump(&self, out: &mut Vec<f64>) {
        out.push(self.data.len() as f64);
        for (cid, p) in &self.data {
            push_string(out, cid);
            out.push(p.order.len() as f64);
            for &f in &p.order {
                push_string(out, FIELD_NAMES[f as usize]);
                match f {
                    F_ATTACKS => dump_arr(out, p.attacks.as_ref().unwrap()),
                    F_BETRAYALS => dump_scalar(out, p.betrayals.unwrap()),
                    F_KILLED_AT => dump_scalar(out, p.killed_at.unwrap()),
                    F_KILLED_BY => {
                        out.push(3.0);
                        enc_cid(out, p.killed_by.as_ref().unwrap());
                    }
                    F_DEATH_POSITION => {
                        out.push(4.0);
                        out.push(p.death_position.unwrap());
                    }
                    F_FINAL_TILES => dump_scalar(out, p.final_tiles.unwrap()),
                    F_KILLS => {
                        let k = p.kills.as_ref().unwrap();
                        out.push(5.0);
                        out.push(k.len() as f64);
                        for (v, t) in k {
                            push_string(out, v);
                            out.push(*t as f64);
                        }
                    }
                    F_CONQUESTS => dump_arr(out, p.conquests.as_ref().unwrap()),
                    F_BOATS => dump_obj(out, p.boats.as_ref().unwrap()),
                    F_BOMBS => dump_obj(out, p.bombs.as_ref().unwrap()),
                    F_GOLD => dump_arr(out, p.gold.as_ref().unwrap()),
                    F_UNITS => dump_obj(out, p.units.as_ref().unwrap()),
                    F_TILES => dump_arr(out, p.tiles.as_ref().unwrap()),
                    F_ALLIANCES => dump_arr(out, p.alliances.as_ref().unwrap()),
                    _ => dump_scalar(out, p.peak_troops.unwrap()),
                }
            }
        }
    }
}

fn dump_scalar(out: &mut Vec<f64>, v: i64) {
    out.push(0.0);
    out.push(v as f64);
}

fn dump_arr(out: &mut Vec<f64>, a: &[i64]) {
    out.push(1.0);
    out.push(a.len() as f64);
    out.extend(a.iter().map(|&v| v as f64));
}

fn dump_obj(out: &mut Vec<f64>, m: &ObjArrays) {
    out.push(2.0);
    out.push(m.len() as f64);
    for (k, a) in m {
        push_string(out, k);
        out.push(a.len() as f64);
        out.extend(a.iter().map(|&v| v as f64));
    }
}

// ---------------------------------------------------------------- run_op
//
// kind 0 replays one whole scripted scenario. args (flat f64, strings
// `[len, u0, ..]`):
//   [0, nPlayers, (cidEnc, typeStr, isPlayer)*, nOps, (op)*]
//     cidEnc: [0] null | [1, str]; val: [0, str] bigint | [1, num]
//   op kinds (params after the kind token):
//     0 attack [p,t,val]        1 attackMaxIncoming [t,val]
//     2 attackCancel [p,t,val]  3 betray [p]            4 allianceFormed [p]
//     5 allianceEnded [p,val,cidEnc]                    6 boatSendTrade [p]
//     7 boatArriveTrade [p,t,val]                       8 boatCapturedTrade [p,t,val]
//     9 boatDestroyTrade [p]                           10 boatSendTroops [p]
//    11 boatArriveTroops [p]                           12 boatDestroyTroops [p]
//    13 boatCapturedTroops [p]                         14 bombLaunch [p,t,typeStr]
//    15 bombLand [p,t,typeStr]                          16 bombIntercept [p,typeStr,val]
//    17 goldWork [p,val]                               18 goldWar [p,cap,val]
//    19 unitBuild [p,typeStr]  20 unitCapture [p,typeStr] 21 unitUpgrade [p,typeStr]
//    22 unitDestroy [p,typeStr] 23 unitLose [p,typeStr]
//    24 playerKilled [p,val]                           25 recordFinalTiles [p,val]
//    26 recordAlliancesAtEnd [p,val,val]              27 recordTickSample [p,val,val,num]
//    28 recordKilledBy [v,cidEnc]                     29 recordDeathPosition [v,num]
//    30 recordKill [p,v,val]                          31 trainSelfTrade [p,val]
//    32 trainExternalTrade [p,val]                    33 lobbyFillTime [num]
//    34 numMirvsLaunched []                           35 getPlayerStats [p]
//    36 stats []
// res: [traceLen, (trace)*, numMirv, dump]
//   trace: 0 clientID [0,r,cidEnc] | 1 type [1,r,(str)] | 2 isPlayer [2,r,0|1]
//     3 op threw [3,kind,1] | 4 getPlayerStats [4,r,0|1] | 5 numMirvs [5,v]
//     6 stats called [6]

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
    fn string(&mut self) -> String {
        let len = self.u();
        let units: Vec<u16> = (0..len).map(|_| self.f() as u16).collect();
        String::from_utf16_lossy(&units)
    }
    fn cid(&mut self) -> Option<String> {
        if self.u() == 0 {
            None
        } else {
            Some(self.string())
        }
    }
    fn val(&mut self) -> Val {
        if self.u() == 0 {
            Val::Bi(self.string().parse::<i64>().unwrap_or(0))
        } else {
            Val::Num(self.f())
        }
    }
}

fn exec_op(si: &mut StatsImpl, trace: &mut Vec<f64>, players: &[MP], c: &mut Cur, k: usize) -> Js {
    match k {
        0 => {
            let p = c.u();
            let t = c.u();
            let v = c.val();
            si.attack(trace, players, p, t, &v)
        }
        1 => {
            let t = c.u();
            let v = c.val();
            si.attack_max_incoming(trace, players, t, &v)
        }
        2 => {
            let p = c.u();
            let t = c.u();
            let v = c.val();
            si.attack_cancel(trace, players, p, t, &v)
        }
        3 => si.betray(trace, players, c.u()),
        4 => si.alliance_formed(trace, players, c.u()),
        5 => {
            let p = c.u();
            let v = c.val();
            let counter = c.cid();
            si.alliance_ended(trace, players, p, &v, counter.as_deref())
        }
        6 => si.boat_send_trade(trace, players, c.u()),
        7 => {
            let p = c.u();
            let t = c.u();
            let v = c.val();
            si.boat_arrive_trade(trace, players, p, t, &v)
        }
        8 => {
            let p = c.u();
            let t = c.u();
            let v = c.val();
            si.boat_captured_trade(trace, players, p, t, &v)
        }
        9 => si.boat_destroy_trade(trace, players, c.u()),
        10 => si.boat_send_troops(trace, players, c.u()),
        11 => si.boat_arrive_troops(trace, players, c.u()),
        12 => si.boat_destroy_troops(trace, players, c.u()),
        13 => si.boat_captured_troops(trace, players, c.u()),
        14 => {
            let p = c.u();
            let t = c.u();
            let ty = c.string();
            si.bomb_launch(trace, players, p, t, &ty)
        }
        15 => {
            let p = c.u();
            let t = c.u();
            let ty = c.string();
            si.bomb_land(trace, players, p, t, &ty)
        }
        16 => {
            let p = c.u();
            let t = c.string();
            let v = c.val();
            si.bomb_intercept(trace, players, p, &t, &v)
        }
        17 => {
            let p = c.u();
            let v = c.val();
            si.gold_work(trace, players, p, &v)
        }
        18 => {
            let p = c.u();
            let cr = c.u();
            let v = c.val();
            si.gold_war(trace, players, p, cr, &v)
        }
        19 => {
            let p = c.u();
            let t = c.string();
            si.unit_build(trace, players, p, &t)
        }
        20 => {
            let p = c.u();
            let t = c.string();
            si.unit_capture(trace, players, p, &t)
        }
        21 => {
            let p = c.u();
            let t = c.string();
            si.unit_upgrade(trace, players, p, &t)
        }
        22 => {
            let p = c.u();
            let t = c.string();
            si.unit_destroy(trace, players, p, &t)
        }
        23 => {
            let p = c.u();
            let t = c.string();
            si.unit_lose(trace, players, p, &t)
        }
        24 => {
            let p = c.u();
            let v = c.val();
            si.player_killed(trace, players, p, &v)
        }
        25 => {
            let p = c.u();
            let v = c.val();
            si.record_final_tiles(trace, players, p, &v)
        }
        26 => {
            let p = c.u();
            let a = c.val();
            let b = c.val();
            si.record_alliances_at_end(trace, players, p, &a, &b)
        }
        27 => {
            let p = c.u();
            let t = c.val();
            let tr = c.val();
            let ac = c.f();
            si.record_tick_sample(trace, players, p, &t, &tr, ac)
        }
        28 => {
            let v = c.u();
            let killer = c.cid();
            si.record_killed_by(trace, players, v, killer)
        }
        29 => {
            let v = c.u();
            let pos = c.f();
            si.record_death_position(trace, players, v, pos)
        }
        30 => {
            let p = c.u();
            let v = c.u();
            let t = c.val();
            si.record_kill(trace, players, p, v, &t)
        }
        31 => {
            let p = c.u();
            let v = c.val();
            si.train_self_trade(trace, players, p, &v)
        }
        32 => {
            let p = c.u();
            let v = c.val();
            si.train_external_trade(trace, players, p, &v)
        }
        33 => {
            c.f();
            Ok(())
        }
        34 => {
            si.num_mirvs_launched(trace);
            Ok(())
        }
        35 => {
            let p = c.u();
            si.get_player_stats(trace, players, p);
            Ok(())
        }
        36 => {
            si.stats(trace);
            Ok(())
        }
        _ => Ok(()),
    }
}

fn run(c: &mut Cur, trace: &mut Vec<f64>, payload: &mut Vec<f64>) {
    let np = c.u();
    let mut players: Vec<MP> = Vec::with_capacity(np);
    for _ in 0..np {
        players.push(MP {
            cid: c.cid(),
            ptype: c.string(),
            is_player: c.f() == 1.0,
        });
    }
    let nops = c.u();
    let mut si = StatsImpl::new();
    for _ in 0..nops {
        let k = c.u();
        if exec_op(&mut si, trace, &players, c, k).is_err() {
            trace.push(3.0);
            trace.push(k as f64);
            trace.push(1.0);
        }
    }
    payload.push(si.num_mirv_launched as f64);
    si.dump(payload);
}

pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut c = Cur(args, 0);
    let k = c.u() as u8;
    let mut trace: Vec<f64> = Vec::new();
    let mut payload: Vec<f64> = Vec::new();
    if k == kind {
        run(&mut c, &mut trace, &mut payload);
    }
    let mut out = Vec::with_capacity(1 + trace.len() + payload.len());
    out.push(trace.len() as f64);
    out.extend(trace.iter().copied());
    out.extend(payload.iter().copied());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc_str(s: &str) -> Vec<f64> {
        let u: Vec<u16> = s.encode_utf16().collect();
        let mut v = vec![u.len() as f64];
        v.extend(u.iter().map(|&x| f64::from(x)));
        v
    }

    fn bi(s: &str) -> Vec<f64> {
        let mut v = vec![0.0];
        v.extend(enc_str(s));
        v
    }

    /// One scripted player table row + a single-op scenario builder.
    fn spec(players: &[(Option<&str>, &str, bool)], ops: &[Vec<f64>]) -> Vec<f64> {
        let mut v = vec![0.0, players.len() as f64];
        for (cid, t, ip) in players {
            match cid {
                None => v.push(0.0),
                Some(s) => {
                    v.push(1.0);
                    v.extend(enc_str(s));
                }
            }
            v.extend(enc_str(t));
            v.push(if *ip { 1.0 } else { 0.0 });
        }
        v.push(ops.len() as f64);
        for o in ops {
            v.extend(o.iter().copied());
        }
        v
    }

    fn payload(res: &[f64]) -> &[f64] {
        &res[1 + res[0] as usize..]
    }

    #[test]
    fn bigint_coercion_matrix() {
        assert_eq!(to_big(&Val::Bi(-7)), Some(-7));
        assert_eq!(to_big(&Val::Num(2.7)), Some(2));
        assert_eq!(to_big(&Val::Num(-1.5)), Some(-2));
        assert_eq!(to_big(&Val::Num(-0.0)), Some(0));
        assert!(to_big(&Val::Num(f64::NAN)).is_none());
        assert!(to_big(&Val::Num(f64::INFINITY)).is_none());
        assert!(to_big(&Val::Num(f64::NEG_INFINITY)).is_none());
    }

    #[test]
    fn neg_keeps_input_shape() {
        assert_eq!(neg(&Val::Bi(5)), Val::Bi(-5));
        assert_eq!(neg(&Val::Num(2.5)), Val::Num(-2.5));
        // -(-0) is +0; BigInt(-0) is 0n.
        assert_eq!(neg(&Val::Num(-0.0)), Val::Num(0.0));
        assert_eq!(to_big(&neg(&Val::Num(-0.0))), Some(0));
        // NaN negates to NaN and still throws.
        assert!(to_big(&neg(&Val::Num(f64::NAN))).is_none());
    }

    #[test]
    fn conquest_table() {
        assert_eq!(conquest_by_type("HUMAN"), Some(PLAYER_INDEX_HUMAN));
        assert_eq!(conquest_by_type("NATION"), Some(PLAYER_INDEX_NATION));
        assert_eq!(conquest_by_type("BOT"), Some(PLAYER_INDEX_BOT));
        assert_eq!(conquest_by_type("Alien"), None);
        assert_eq!(conquest_by_type("Human"), None);
    }

    #[test]
    fn attack_trace_and_dump() {
        let op = [0.0, 0.0, 1.0]
            .iter()
            .cloned()
            .chain(bi("10"))
            .collect::<Vec<_>>();
        let res = run_op(
            0,
            &spec(&[(Some("a"), "HUMAN", true), (Some("b"), "HUMAN", true)], &[op]),
        );
        // trace: clientID(a) [0,0,1,1,'a'] (5) + isPlayer(b) [2,1,1] (3) +
        // clientID(b) (5) = 13.
        assert_eq!(res[0], 13.0);
        let atk = enc_str("attacks");
        let mut want = vec![0.0, 2.0, 1.0, 97.0, 1.0];
        want.extend(atk.iter().copied());
        want.extend([1.0, 1.0, 10.0]); // a: attacks [10] (SENT, no growth)
        want.extend([1.0, 98.0, 1.0]); // b
        want.extend(atk.iter().copied());
        want.extend([1.0, 2.0, 0.0, 10.0]); // b: while-grew to [0, 10]
        assert_eq!(payload(&res), want);
    }

    #[test]
    fn throw_midway_leaves_grown_array() {
        // goldWork NaN: the player entry + gold key + the [0n] init survive.
        let res = run_op(0, &spec(&[(Some("a"), "HUMAN", true)], &[vec![17.0, 0.0, 1.0, f64::NAN]]));
        // trace: clientID [0,0,1,1,'a'] (5) + throw [3,17,1] (3).
        assert_eq!(res[0], 8.0);
        let mut want = vec![0.0, 1.0, 1.0, 97.0, 1.0];
        want.extend(enc_str("gold"));
        want.extend([1.0, 1.0, 0.0]); // gold [0n] — the add never landed
        assert_eq!(payload(&res), want);
    }

    #[test]
    fn tick_sample_drawdown_pair() {
        // 10 seeds (ddPeak 10, ddTrough 10); 5 declines to (10, 5); 20 raises
        // the peak only ((20-20)*10 = 0 > (10-5)*20 = 100 false); 8 rewrites
        // ((20-8)*10 = 120 > (10-5)*20 = 100) to (20, 8). peakTroops maxes at
        // 200; peak-concurrent maxes at 3 (2.5 floors and loses the strict >).
        let ops = vec![
            vec![27.0, 0.0, 1.0, 10.0, 1.0, 100.0, 2.0],
            vec![27.0, 0.0, 1.0, 5.0, 1.0, 50.0, 1.0],
            vec![27.0, 0.0, 1.0, 20.0, 1.0, 200.0, 3.0],
            vec![27.0, 0.0, 1.0, 8.0, 1.0, 8.0, 2.5],
        ];
        let res = run_op(0, &spec(&[(Some("a"), "HUMAN", true)], &ops));
        // Two clientID events per sample (the op + the _maxAlliance inner).
        assert_eq!(res[0], 40.0);
        let p = payload(&res);
        assert_eq!(p[0], 0.0); // numMirv
        assert_eq!(p[1], 1.0); // one player
        // Field order: tiles, peakTroops, alliances.
        let tiles = enc_str("tiles");
        let pt = enc_str("peakTroops");
        let al = enc_str("alliances");
        let mut want = vec![0.0, 1.0, 1.0, 97.0, 3.0];
        want.extend(tiles.iter().copied());
        want.extend([1.0, 3.0, 20.0, 20.0, 8.0]);
        want.extend(pt.iter().copied());
        want.extend([0.0, 200.0]);
        want.extend(al.iter().copied());
        want.extend([1.0, 5.0, 0.0, 0.0, 0.0, 0.0, 3.0]);
        assert_eq!(p, want);
    }

    #[test]
    fn killed_by_null_is_a_value() {
        let ops = vec![
            vec![28.0, 0.0, 0.0], // null first: recorded
            vec![28.0, 0.0, 1.0, 1.0, 107.0], // "k" second: loses
        ];
        let res = run_op(0, &spec(&[(Some("a"), "HUMAN", true)], &ops));
        assert_eq!(res[0], 10.0);
        let mut want = vec![0.0, 1.0, 1.0, 97.0, 1.0];
        want.extend(enc_str("killedBy"));
        want.extend([3.0, 0.0]); // present, JS null (cidEnc [0])
        assert_eq!(payload(&res), want);
    }

    #[test]
    fn mirv_counter_and_offtable_bomb_key() {
        let ops: Vec<Vec<f64>> = vec![
            vec![14.0, 0.0, 0.0, 4.0, 77.0, 73.0, 82.0, 86.0], // bombLaunch(a,a,"MIRV")
            vec![16.0, 0.0, 5.0, 84.0, 114.0, 97.0, 105.0, 110.0, 1.0, 3.0], // bombIntercept(a,"Train",3)
            vec![34.0],
        ];
        let res = run_op(0, &spec(&[(Some("a"), "HUMAN", true)], &ops));
        let p = payload(&res);
        assert_eq!(p[0], 1.0); // numMirv: the MIRV launch counted
        // bombs object: key "mirv" (launch 1), then the off-table "undefined"
        // key grown to length 3 with intercept 3 at index 2.
        let bombs = enc_str("bombs");
        let mut want = vec![1.0, 1.0, 1.0, 97.0, 1.0];
        want.extend(bombs.iter().copied());
        want.extend([2.0, 2.0]); // object, two keys
        want.extend(enc_str("mirv").iter().copied());
        want.extend([1.0, 1.0]); // [1n]
        want.extend(enc_str("undefined").iter().copied());
        want.extend([3.0, 0.0, 0.0, 3.0]); // [0n,0n,3n]
        assert_eq!(p, want);
    }
}

