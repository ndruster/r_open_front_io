//! Bit-exact port of `src/core/game/UnitImpl.ts`: the game unit entity. The
//! `Unit` struct replicates every private field of the TS class; construction
//! and every method run as a `kind` of [`UnitHarness::run_op`] over the
//! `js_json` codec (precedent `config`). The `mg` (GameImpl) and owner
//! (PlayerImpl) surfaces are scripted facades whose every call is pinned into
//! a flat trace, so the facade call order, the `??` / `||` / `?.` / `in`
//! gates, the `toUpdate` key order and the `_units` / `_myUnitsVersion`
//! mutation semantics are all part of the compared stream.
//!
//! Ported surface (TS line anchors):
//!
//! * the field block (24-50), the ctor (52-114), every public / private
//!   method (116-761): `setTargetable`..`setLoaded`, incl. `toUpdate`
//!   (145-184), `setOwner` (231-263), `modifyHealth` (275-299), `delete`
//!   (325-366), `displayMessageOnDeleted` (368-391), the warship / transport /
//!   nuke state getters + `update*State` Partial merges (405-507), `hash`
//!   (521), `toString` (525), the SAM missile queue (529-553), the trajectory
//!   accessors (567-597), the veterancy cluster (622-698) and the level
//!   cluster (700-746).
//!
//! Facade (mock) surface — the capture scripts these, every call traced:
//!
//! * `mg.unitInfo(type)` (50), `mg.config().{samRange,samUpgradeDuration,
//!   deletionMarkDuration,warshipVeterancyHealthBonus,warshipMaxVeterancy,
//!   warshipVeterancyTransportKills,warshipVeterancyTradeCaptures,
//!   safeFromPiratesCooldownMax,dynamicSamRange}` (51-59), `mg.ticks()` (60),
//!   `mg.stats()` (61) + `unitBuild/unitCapture/unitLose/boatCapturedTroops/
//!   boatDestroyTroops/boatDestroyTrade/unitDestroy` (62-68), `mg.onUnitMoved`
//!   (69), `mg.removeUnit` (70), `mg.addUpdate` (71, the full `toUpdate`
//!   codec rides the trace), `mg.bumpUnitsVersion` (72), `mg.displayMessage`
//!   (73), `owner.{smallID,id,name}` (80-82), `targetUnit.id()` (84).
//!   `mg.config()` / `mg.stats()` / `owner()` themselves: only `stats()` is
//!   traced (61); `config()` is an untraced getter.
//! * the owner's `_units` / `_myUnitsVersion` are REAL harness state (the
//!   `filter`/`push`/`++` mutations), pinned by the `dumpOwner` op (mid 71).
//!   `_units` slots cross as tokens: `0` = the unit under test (reference
//!   identity `!== this` modelled as the token), `k > 0` = opaque other units.
//! * `targetUnit` / attacker / destroyer cross as pid / token numbers (the
//!   capture maps the mock object to its scripted token; Rust stores the raw
//!   codec value and only ever calls `id()` on a non-nullish `targetUnit`).
//!
//! Arena note: the TS `TileRef` values are plain numbers in the capture
//! domain; the `arena` machinery (`GameImpl`'s unit registry, `removeUnit`
//! bookkeeping) is deferred to G6 — here `removeUnit` is a pure trace event.
//!
//! Faithfulness notes:
//!
//! * `"x" in params` is a PRESENCE gate: a present-but-`undefined` key still
//!   passes (the codec `Obj` field list distinguishes absent from `Undef`).
//! * `??` fires only on nullish (absent / `undefined` / `null`); `||` and
//!   truthiness differ; `?.` short-circuits on BOTH `undefined` and `null`.
//! * `_trainType = "trainType" in params ? params.trainType : undefined` has
//!   NO `??` — a present `null` stays `null` and rides `toUpdate` as `[2]`.
//! * `move(null)` throws `"tile cannot be null"` (the `=== null` gate;
//!   `undefined` passes and poisons `_tile` — the capture pins one case).
//! * `delete` on an inactive unit throws `cannot delete Unit:<type>,owner:
//!   <name> not active` — the `${this}` interpolation runs `toString`, which
//!   calls `owner().name()` (facade 82) BEFORE the throw, so the trace
//!   survives in the error result.
//! * `_nukeState!.targetedBySam` / `setTargetedBySAM` on a non-nuke unit
//!   reproduce the V8 TypeError messages verbatim ("Cannot read properties
//!   of undefined (reading 'targetedBySam')" / "Cannot set properties of
//!   undefined (setting 'targetedBySam')"); `isInCombat` likewise pins
//!   "Cannot read properties of undefined (reading 'lastCombatTick')".
//! * `warshipState()` MUTATES the stored field list (appends `isInCombat` at
//!   the END on first call — JS insertion order), and `updateWarshipState`
//!   REBUILDS it as `{state,patrolTile,retreatPort,lastCombatTick,veterancy,
//!   veterancyProgress}` (retreatPort present even when undefined, isInCombat
//!   dropped). The state objects are modelled as ordered field lists so the
//!   spread / dump key order matches.
//! * `updateWarshipState`'s `if (update.isInCombat)` is a TRUTHINESS gate
//!   (`false` / `0` / `""` skip `markInCombat`); the no-change early return
//!   compares `merged.state/patrolTile/retreatPort` with `===` (codec value
//!   equality; absent reads `undefined` so absent===absent is true).
//! * `updateNukeState` rebuilds `{targetedBySam,trajectoryIndex,waitTicks,
//!   trajectory}` — `trajectory` is ALWAYS the old value (the update's
//!   `trajectory` is ignored by the rebuild).
//! * `transportShipState()` returns a FRESH `{isRetreating, troops:
//!   this._troops}` — `troops` comes from the unit, not the stored state.
//! * `toUpdate` calls `warshipState()` (ticks facade) and `info()` (unitInfo
//!   facade) on EVERY addUpdate; `targetUnitId` calls `targetUnit.id()` only
//!   when the reference is non-nullish.
//! * `modifyHealth`: `toInt(delta)` throws BEFORE any facade; `maxHealth()`
//!   then runs `info()` (50) + `config().warshipVeterancyHealthBonus()` (54).
//!   The `attacker !== undefined && delta < 0 && _warshipState !== undefined`
//!   gate stamps `lastCombatTick = mg.ticks()`. `_health === 0n` chains into
//!   `delete(true, attacker)`. bigints cross as i64 in the |v| <= 2^53
//!   domain (config.rs precedent); `toInt` = ±Inf clamp then floor then
//!   BigInt (only NaN throws, the V8 message pinned).
//! * `setTroops`: `Math.max(0, troops)` via `js_max` (NaN poisons); the
//!   `_troops === nextTroops` early return is `===` (NaN never equals, `-0`
//!   equals `0`); a string `_troops` from params never equals the numeric
//!   next, so the raw JsVal comparison is kept.
//! * `addVeterancyProgress`: `pointsPerLevel = transport * capture` (f64);
//!   the while loop's `veterancy < maxVeterancy` uses the OUTER captured
//!   `maxVeterancy`, while `increaseVeterancy` RE-READS
//!   `config().warshipMaxVeterancy()` (facade) on every call and each level
//!   gain fires its own `addUpdate`; at the cap `veterancyProgress = 0`.
//! * `increaseLevel` (SAMLauncher): rebuilds `_samLauncherState` as
//!   `{upgradeStartTick, startRange, targetLevel, duration}` (upgradeStartTick
//!   FIRST — the ctor's shape omits it, so `toUpdate.samUpgrade` stays
//!   undefined until the first upgrade). `[MissileSilo,SAMLauncher].includes
//!   (type)` pushes a ticks entry; `decreaseLevel` pops, and `_level <= 0`
//!   routes through `delete` WITHOUT the bump / addUpdate.
//! * `delete`'s stats fan-out: `mg.stats()` is called ONCE PER chained
//!   method (`unitDestroy` + `unitLose` = two 61 events); `displayMessage`
//!   fires unless the RAW arg `=== false` (`undefined`/absent fires, `null`
//!   fires — `null !== false`).
//! * `hash` = `tile + simpleHash(typeStr) * id` (f64 arithmetic over the
//!   raw tile / id codec values).

use crate::js_json::{map_set, push_str, push_val, read_str, read_val, JsVal};
use crate::jsnum::js_max;
use crate::util::{simple_hash, within_int};
use crate::veterancy::max_health_with_veterancy;

/// `Number.MAX_SAFE_INTEGER` (the `toInt` ±Infinity clamp).
const MAX_SAFE: i64 = 9_007_199_254_740_991;

// ---- codec helpers (config.rs precedent) --------------------------------------

fn truthy(v: &JsVal) -> bool {
    match v {
        JsVal::Absent | JsVal::Undef | JsVal::Null => false,
        JsVal::Num(n) => !n.is_nan() && *n != 0.0,
        JsVal::Bool(b) => *b,
        JsVal::Str(s) => !s.is_empty(),
        JsVal::Obj(_) | JsVal::Arr(_) => true,
    }
}

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

/// `obj[key]` — an absent field reads `undefined`.
fn field<'a>(m: &'a [(String, JsVal)], key: &str) -> Option<&'a JsVal> {
    m.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn nullish(v: Option<&JsVal>) -> bool {
    matches!(v, None | Some(JsVal::Undef) | Some(JsVal::Null))
}

/// `v ?? d` over the field-read domain.
fn coalesce<'a>(v: Option<&'a JsVal>, d: &'a JsVal) -> &'a JsVal {
    if nullish(v) {
        d
    } else {
        v.unwrap()
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

fn push_ok_bool(out: &mut Vec<f64>, v: bool) {
    push_ok(out, &JsVal::Bool(v));
}

/// Throw payload `[1, ...encS(msg)]`.
fn push_err(out: &mut Vec<f64>, msg: &str) {
    out.push(1.0);
    push_str(out, msg);
}

/// `BigInt(x)` for a number `x` — `Err(V8 message)` on the throw matrix.
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

// ---- facade trace event codes ------------------------------------------------
//
// Scripted returns are consumed FIFO per scenario; an exhausted list REPEATS
// its last value on both sides (the trace pins the call count independently,
// so the repetition can never mask a divergence — an empty list panics /
// throws on the first call).
//
// 50 mg.unitInfo [50, ...encVal(v)]
// 51 config.samRange [51, level, ret]        52 config.samUpgradeDuration [52, ret]
// 53 config.deletionMarkDuration [53, ret]   54 config.warshipVeterancyHealthBonus [54, ret]
// 55 config.warshipMaxVeterancy [55, ret]    56 config.warshipVeterancyTransportKills [56, ret]
// 57 config.warshipVeterancyTradeCaptures [57, ret]
// 58 config.safeFromPiratesCooldownMax [58, ret]
// 59 config.dynamicSamRange [59, tick, ret]  60 mg.ticks [60, ret]
// 61 mg.stats [61]                           62 stats.unitBuild [62, ...encVal(ownerPid), ...encS(type)]
// 63 stats.unitCapture [63, ...encVal(newPid), ...encS(type)]
// 64 stats.unitLose [64, ...encVal(pid), ...encS(type)]
// 65 stats.boatCapturedTroops [65, ...encVal(newPid), ...encVal(oldPid)]
// 66 stats.boatDestroyTroops [66, ...encVal(destroyer), ...encVal(ownerPid), ...encVal(troops)]
// 67 stats.boatDestroyTrade [67, ...encVal(destroyer), ...encVal(ownerPid)]
// 68 stats.unitDestroy [68, ...encVal(destroyer), ...encS(type)]
// 69 mg.onUnitMoved [69]                     70 mg.removeUnit [70]
// 71 mg.addUpdate [71, ...encVal(update)]    72 mg.bumpUnitsVersion [72]
// 73 mg.displayMessage [73, (encVal)*6]      80 owner.smallID [80, pid, ret]
// 81 owner.id [81, pid, ret]                82 owner.name [82, pid, ...encS(name)]
// 84 targetUnit.id [84, ret]
//
// (83 is reserved-unassigned; the G3b owner facade reuses 80-82.)

/// The scripted `mg` (GameImpl) facade for one scenario: per-method FIFO
/// return scripts consumed in call order across the whole scenario.
#[derive(Clone, Debug, Default)]
struct Mg {
    unit_info: Vec<JsVal>,
    ui_i: usize,
    ticks: Vec<f64>,
    tk_i: usize,
    sam_range: Vec<f64>,
    samr_i: usize,
    sam_dur: Vec<f64>,
    samd_i: usize,
    del_mark: Vec<f64>,
    delm_i: usize,
    wh_bonus: Vec<f64>,
    whb_i: usize,
    max_vet: Vec<f64>,
    maxv_i: usize,
    vet_transport: Vec<f64>,
    tt_i: usize,
    vet_trade: Vec<f64>,
    ct_i: usize,
    safe_pirates: Vec<f64>,
    sfp_i: usize,
    dyn_sam: Vec<f64>,
    dsr_i: usize,
    tu_id: Vec<JsVal>,
    tuid_i: usize,
}

/// One scripted owner (PlayerImpl) mock: method FIFOs plus the REAL
/// `_units` / `_myUnitsVersion` mutation state.
#[derive(Clone, Debug)]
struct Owner {
    pid: f64,
    small_id: Vec<f64>,
    si: usize,
    ids: Vec<f64>,
    ii: usize,
    names: Vec<String>,
    ni: usize,
    /// slot tokens: `0` = the unit under test, `k > 0` = other units.
    units: Vec<f64>,
    my_units_version: f64,
}

impl Default for Owner {
    fn default() -> Self {
        Self {
            pid: 0.0,
            small_id: Vec::new(),
            si: 0,
            ids: Vec::new(),
            ii: 0,
            names: Vec::new(),
            ni: 0,
            units: Vec::new(),
            my_units_version: 0.0,
        }
    }
}

/// The `UnitImpl` private field block, replicated 1:1. The three state
/// objects ride as ordered field lists so the JS spread / dump key order
/// survives (see the faithfulness notes).
#[derive(Clone, Debug)]
struct Unit {
    type_name: String,
    tile: JsVal,
    id: JsVal,
    owner_idx: usize,
    active: bool,
    target_tile: JsVal,
    target_player: JsVal,
    target_unit: JsVal,
    health: i64,
    last_tile: JsVal,
    transport_ship_state: Option<Vec<(String, JsVal)>>,
    warship_state: Option<Vec<(String, JsVal)>>,
    nuke_state: Option<Vec<(String, JsVal)>>,
    reached_target: bool,
    was_destroyed_by_enemy: bool,
    destroyer: JsVal,
    last_set_safe_from_pirates: JsVal,
    under_construction: JsVal,
    last_owner: Option<usize>,
    troops: JsVal,
    missile_timer_queue: Vec<f64>,
    has_train_station: JsVal,
    level: f64,
    targetable: JsVal,
    loaded: JsVal,
    train_type: JsVal,
    deletion_at: JsVal,
    sam_launcher_state: Option<Vec<(String, JsVal)>>,
}

impl Default for Unit {
    fn default() -> Self {
        Self {
            type_name: String::new(),
            tile: JsVal::Undef,
            id: JsVal::Undef,
            owner_idx: 0,
            active: true,
            target_tile: JsVal::Undef,
            target_player: JsVal::Undef,
            target_unit: JsVal::Undef,
            health: 0,
            last_tile: JsVal::Undef,
            transport_ship_state: None,
            warship_state: None,
            nuke_state: None,
            reached_target: false,
            was_destroyed_by_enemy: false,
            destroyer: JsVal::Undef,
            last_set_safe_from_pirates: JsVal::Num(0.0),
            under_construction: JsVal::Bool(false),
            last_owner: None,
            troops: JsVal::Num(0.0),
            missile_timer_queue: Vec::new(),
            has_train_station: JsVal::Bool(false),
            level: 1.0,
            targetable: JsVal::Bool(true),
            loaded: JsVal::Undef,
            train_type: JsVal::Undef,
            deletion_at: JsVal::Null,
            sam_launcher_state: None,
        }
    }
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
    fn s(&mut self) -> String {
        read_str(self.0, &mut self.1)
    }
    fn list(&mut self) -> Vec<f64> {
        let n = self.u();
        (0..n).map(|_| self.f()).collect()
    }
    fn val(&mut self) -> JsVal {
        read_val(self.0, &mut self.1)
    }
    fn vlist(&mut self) -> Vec<JsVal> {
        let n = self.u();
        (0..n).map(|_| self.val()).collect()
    }
    fn owner(&mut self) -> Owner {
        let pid = self.f();
        let small_id = self.list();
        let ids = self.list();
        let nn = self.u();
        let names = (0..nn).map(|_| self.s()).collect();
        let units = self.list();
        let my_units_version = self.f();
        Owner {
            pid,
            small_id,
            si: 0,
            ids,
            ii: 0,
            names,
            ni: 0,
            units,
            my_units_version,
        }
    }
    fn mg(&mut self) -> Mg {
        Mg {
            unit_info: self.vlist(),
            ticks: self.list(),
            sam_range: self.list(),
            sam_dur: self.list(),
            del_mark: self.list(),
            wh_bonus: self.list(),
            max_vet: self.list(),
            vet_transport: self.list(),
            vet_trade: self.list(),
            safe_pirates: self.list(),
            dyn_sam: self.list(),
            tu_id: self.vlist(),
            ..Default::default()
        }
    }
}

/// `GameUpdateType.Unit` (GameUpdates.ts, numeric enum member 1).
const UPDATE_TYPE_UNIT: f64 = 1.0;
/// `MessageType.UNIT_DESTROYED` (Game.ts, numeric enum member 12).
const MSG_UNIT_DESTROYED: f64 = 12.0;

fn is_structure(t: &str) -> bool {
    matches!(
        t,
        "Warship" | "Port" | "Missile Silo" | "Defense Post" | "SAM Launcher" | "City" | "Factory"
    )
}

fn is_silo_or_sam(t: &str) -> bool {
    t == "Missile Silo" || t == "SAM Launcher"
}

// ---- facade plumbing -----------------------------------------------------------

/// FIFO script consumption with the capture's exhausted-repeat rule (see the
/// event-code table): an index past the end yields the LAST scripted value.
/// An empty list is a scenario bug on both sides.
fn take_f64(list: &[f64], i: &mut usize) -> f64 {
    let idx = (*i).min(list.len() - 1);
    *i += 1;
    list[idx]
}
fn take_val(list: &[JsVal], i: &mut usize) -> JsVal {
    let idx = (*i).min(list.len() - 1);
    *i += 1;
    list[idx].clone()
}
fn take_str(list: &[String], i: &mut usize) -> String {
    let idx = (*i).min(list.len() - 1);
    *i += 1;
    list[idx].clone()
}

// ---- the harness ---------------------------------------------------------------

#[derive(Default)]
pub struct UnitHarness {
    u: Unit,
    owners: Vec<Owner>,
    mg: Mg,
}

impl UnitHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    // --- facade plumbing ---

    fn mg_unit_info(&mut self, tr: &mut Vec<f64>) -> JsVal {
        let v = take_val(&self.mg.unit_info, &mut self.mg.ui_i);
        tr.push(50.0);
        push_val(tr, &v);
        v
    }
    fn mg_ticks(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = take_f64(&self.mg.ticks, &mut self.mg.tk_i);
        tr.push(60.0);
        tr.push(v);
        v
    }
    fn cfg_sam_range(&mut self, tr: &mut Vec<f64>, level: f64) -> f64 {
        let v = take_f64(&self.mg.sam_range, &mut self.mg.samr_i);
        tr.push(51.0);
        tr.push(level);
        tr.push(v);
        v
    }
    fn cfg_sam_dur(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = take_f64(&self.mg.sam_dur, &mut self.mg.samd_i);
        tr.push(52.0);
        tr.push(v);
        v
    }
    fn cfg_del_mark(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = take_f64(&self.mg.del_mark, &mut self.mg.delm_i);
        tr.push(53.0);
        tr.push(v);
        v
    }
    fn cfg_wh_bonus(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = take_f64(&self.mg.wh_bonus, &mut self.mg.whb_i);
        tr.push(54.0);
        tr.push(v);
        v
    }
    fn cfg_max_vet(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = take_f64(&self.mg.max_vet, &mut self.mg.maxv_i);
        tr.push(55.0);
        tr.push(v);
        v
    }
    fn cfg_vet_transport(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = take_f64(&self.mg.vet_transport, &mut self.mg.tt_i);
        tr.push(56.0);
        tr.push(v);
        v
    }
    fn cfg_vet_trade(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = take_f64(&self.mg.vet_trade, &mut self.mg.ct_i);
        tr.push(57.0);
        tr.push(v);
        v
    }
    fn cfg_safe_pirates(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = take_f64(&self.mg.safe_pirates, &mut self.mg.sfp_i);
        tr.push(58.0);
        tr.push(v);
        v
    }
    fn cfg_dyn_sam(&mut self, tr: &mut Vec<f64>, tick: f64) -> f64 {
        let v = take_f64(&self.mg.dyn_sam, &mut self.mg.dsr_i);
        tr.push(59.0);
        tr.push(tick);
        tr.push(v);
        v
    }
    fn tu_id(&mut self, tr: &mut Vec<f64>) -> JsVal {
        let v = take_val(&self.mg.tu_id, &mut self.mg.tuid_i);
        tr.push(84.0);
        push_val(tr, &v);
        v
    }
    fn stats(&mut self, tr: &mut Vec<f64>) {
        tr.push(61.0);
    }
    fn on_unit_moved(&mut self, tr: &mut Vec<f64>) {
        tr.push(69.0);
    }
    fn remove_unit(&mut self, tr: &mut Vec<f64>) {
        tr.push(70.0);
    }
    fn bump_units_version(&mut self, tr: &mut Vec<f64>) {
        tr.push(72.0);
    }

    fn owner_small_id(&mut self, tr: &mut Vec<f64>, idx: usize) -> f64 {
        let o = &mut self.owners[idx];
        let v = take_f64(&o.small_id, &mut o.si);
        tr.push(80.0);
        tr.push(o.pid);
        tr.push(v);
        v
    }
    fn owner_id(&mut self, tr: &mut Vec<f64>, idx: usize) -> f64 {
        let o = &mut self.owners[idx];
        let v = take_f64(&o.ids, &mut o.ii);
        tr.push(81.0);
        tr.push(o.pid);
        tr.push(v);
        v
    }
    fn owner_name(&mut self, tr: &mut Vec<f64>, idx: usize) -> String {
        let o = &mut self.owners[idx];
        let v = take_str(&o.names, &mut o.ni);
        tr.push(82.0);
        tr.push(o.pid);
        push_str(tr, &v);
        v
    }

    fn add_update(&mut self, tr: &mut Vec<f64>) {
        let upd = self.build_update(tr);
        tr.push(71.0);
        push_val(tr, &JsVal::Obj(upd));
    }

    fn display_message(&mut self, tr: &mut Vec<f64>, vars_unit: &str) {
        let pid = self.owner_id(tr, self.u.owner_idx);
        tr.push(73.0);
        push_val(tr, &JsVal::Str("events_display.unit_destroyed".to_string()));
        push_val(tr, &JsVal::Num(MSG_UNIT_DESTROYED));
        push_val(tr, &JsVal::Num(pid));
        push_val(tr, &JsVal::Undef);
        push_val(
            tr,
            &JsVal::Obj(vec![("unit".to_string(), JsVal::Str(vars_unit.to_string()))]),
        );
        push_val(tr, &self.u.id);
    }

    // --- unit internals ---

    /// `this.info()` — the scripted `mg.unitInfo(_type)` read.
    fn info(&mut self, tr: &mut Vec<f64>) -> JsVal {
        self.mg_unit_info(tr)
    }

    /// `hasHealth()`: `info().maxHealth !== undefined`.
    fn has_health(&mut self, tr: &mut Vec<f64>) -> bool {
        let info = self.info(tr);
        match &info {
            JsVal::Obj(m) => !matches!(field(m, "maxHealth"), None | Some(JsVal::Undef)),
            // a nullish info would be a V8 TypeError; the capture domain
            // always scripts an object.
            _ => false,
        }
    }

    /// `veterancy()`: `_warshipState?.veterancy ?? 0`.
    fn veterancy(&self) -> f64 {
        match &self.u.warship_state {
            Some(m) => to_num(coalesce(field(m, "veterancy"), &JsVal::Num(0.0))),
            None => 0.0,
        }
    }

    /// `maxHealth()`.
    fn max_health(&mut self, tr: &mut Vec<f64>) -> f64 {
        let info = self.info(tr);
        let base = match &info {
            JsVal::Obj(m) => to_num(coalesce(field(m, "maxHealth"), &JsVal::Num(1.0))),
            _ => 1.0,
        };
        let vet = self.veterancy();
        let bonus = self.cfg_wh_bonus(tr);
        max_health_with_veterancy(base, vet, bonus)
    }

    /// `isInCombat()`: `mg.ticks() - _warshipState!.lastCombatTick <= 3` —
    /// the LEFT operand (`mg.ticks()`, facade 60) evaluates BEFORE the
    /// `_warshipState!` read, so a non-warship still traces the ticks call
    /// and then raises `Err(TypeError msg)`.
    fn is_in_combat(&mut self, tr: &mut Vec<f64>) -> Result<bool, String> {
        let t = self.mg_ticks(tr);
        let lct = match &self.u.warship_state {
            Some(m) => to_num(coalesce(field(m, "lastCombatTick"), &JsVal::Undef)),
            None => {
                return Err(
                    "Cannot read properties of undefined (reading 'lastCombatTick')".to_string(),
                )
            }
        };
        Ok(t - lct <= 3.0)
    }

    /// `warshipState()`: throws on a non-warship, else MUTATES `isInCombat`
    /// (appended at the end on first call) and returns the field list.
    fn get_warship_state(&mut self, tr: &mut Vec<f64>) -> Result<Vec<(String, JsVal)>, String> {
        if self.u.warship_state.is_none() {
            return Err("warshipState called on non-warship unit".to_string());
        }
        let combat = self.is_in_combat(tr)?;
        let m = self.u.warship_state.as_mut().unwrap();
        map_set(m, "isInCombat", JsVal::Bool(combat));
        Ok(m.clone())
    }

    /// `transportShipState()`: fresh `{isRetreating, troops: this._troops}`.
    fn get_transport_ship_state(&self) -> Result<Vec<(String, JsVal)>, String> {
        match &self.u.transport_ship_state {
            Some(m) => Ok(vec![
                ("isRetreating".to_string(), field(m, "isRetreating").cloned().unwrap_or(JsVal::Undef)),
                ("troops".to_string(), self.u.troops.clone()),
            ]),
            None => Err("transportShipState called on non-transport-ship unit".to_string()),
        }
    }

    /// `nukeState()`: the live object (or the throw).
    fn get_nuke_state(&self) -> Result<Vec<(String, JsVal)>, String> {
        match &self.u.nuke_state {
            Some(m) => Ok(m.clone()),
            None => Err("nukeState called on non-nuke unit".to_string()),
        }
    }

    /// `markInCombat()` (private): `was = isInCombat(); lastCombatTick =
    /// ticks(); !was -> addUpdate`.
    fn mark_in_combat(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        let was = self.is_in_combat(tr)?;
        let t = self.mg_ticks(tr);
        if let Some(m) = self.u.warship_state.as_mut() {
            map_set(m, "lastCombatTick", JsVal::Num(t));
        }
        if !was {
            self.add_update(tr);
        }
        Ok(())
    }

    /// `increaseVeterancy()` (private): non-warship return; at the (freshly
    /// read) cap return; else `veterancy++` + addUpdate.
    fn increase_veterancy(&mut self, tr: &mut Vec<f64>) {
        if self.u.warship_state.is_none() {
            return;
        }
        let maxv = self.cfg_max_vet(tr);
        let vet = {
            let m = self.u.warship_state.as_ref().unwrap();
            to_num(coalesce(field(m, "veterancy"), &JsVal::Undef))
        };
        if vet >= maxv {
            return;
        }
        let m = self.u.warship_state.as_mut().unwrap();
        map_set(m, "veterancy", JsVal::Num(vet + 1.0));
        self.add_update(tr);
    }

    /// `addVeterancyProgress(source)` (private).
    fn add_veterancy_progress(&mut self, tr: &mut Vec<f64>, source: &str) {
        if self.u.warship_state.is_none() {
            return;
        }
        let max_vet = self.cfg_max_vet(tr);
        let vet = {
            let m = self.u.warship_state.as_ref().unwrap();
            to_num(coalesce(field(m, "veterancy"), &JsVal::Undef))
        };
        if vet >= max_vet {
            return;
        }
        let transport_threshold = self.cfg_vet_transport(tr);
        let capture_threshold = self.cfg_vet_trade(tr);
        let points_per_level = transport_threshold * capture_threshold;
        let add = if source == "Transport" {
            capture_threshold
        } else {
            transport_threshold
        };
        {
            let m = self.u.warship_state.as_mut().unwrap();
            let prog = to_num(coalesce(field(m, "veterancyProgress"), &JsVal::Undef));
            map_set(m, "veterancyProgress", JsVal::Num(prog + add));
        }
        loop {
            let (prog, vet) = {
                let m = self.u.warship_state.as_ref().unwrap();
                (
                    to_num(coalesce(field(m, "veterancyProgress"), &JsVal::Undef)),
                    to_num(coalesce(field(m, "veterancy"), &JsVal::Undef)),
                )
            };
            if !(prog >= points_per_level && vet < max_vet) {
                break;
            }
            let m = self.u.warship_state.as_mut().unwrap();
            map_set(m, "veterancyProgress", JsVal::Num(prog - points_per_level));
            self.increase_veterancy(tr);
        }
        let vet = {
            let m = self.u.warship_state.as_ref().unwrap();
            to_num(coalesce(field(m, "veterancy"), &JsVal::Undef))
        };
        if vet >= max_vet {
            let m = self.u.warship_state.as_mut().unwrap();
            map_set(m, "veterancyProgress", JsVal::Num(0.0));
        }
    }

    /// `toUpdate()` — the full UnitUpdate object in TS declaration order.
    fn build_update(&mut self, tr: &mut Vec<f64>) -> Vec<(String, JsVal)> {
        let mut m: Vec<(String, JsVal)> =
            vec![("type".to_string(), JsVal::Num(UPDATE_TYPE_UNIT))];
        m.push(("unitType".to_string(), JsVal::Str(self.u.type_name.clone())));
        m.push(("id".to_string(), self.u.id.clone()));
        m.push(("troops".to_string(), self.u.troops.clone()));
        let oid = self.owner_small_id(tr, self.u.owner_idx);
        m.push(("ownerID".to_string(), JsVal::Num(oid)));
        let lo = match self.u.last_owner {
            Some(i) => {
                let v = self.owner_small_id(tr, i);
                JsVal::Num(v)
            }
            None => JsVal::Undef,
        };
        m.push(("lastOwnerID".to_string(), lo));
        m.push(("isActive".to_string(), JsVal::Bool(self.u.active)));
        m.push(("reachedTarget".to_string(), JsVal::Bool(self.u.reached_target)));
        let ws = if self.u.warship_state.is_some() {
            match self.get_warship_state(tr) {
                Ok(v) => JsVal::Obj(v),
                Err(_) => JsVal::Undef,
            }
        } else {
            JsVal::Undef
        };
        m.push(("warshipState".to_string(), ws));
        let ts = if self.u.transport_ship_state.is_some() {
            match self.get_transport_ship_state() {
                Ok(v) => JsVal::Obj(v),
                Err(_) => JsVal::Undef,
            }
        } else {
            JsVal::Undef
        };
        m.push(("transportShipState".to_string(), ts));
        let ns = if self.u.nuke_state.is_some() {
            JsVal::Obj(self.u.nuke_state.clone().unwrap())
        } else {
            JsVal::Undef
        };
        m.push(("nukeState".to_string(), ns));
        let su = match &self.u.sam_launcher_state {
            Some(st) if !matches!(field(st, "upgradeStartTick"), None | Some(JsVal::Undef)) => {
                JsVal::Obj(st.clone())
            }
            _ => JsVal::Undef,
        };
        m.push(("samUpgrade".to_string(), su));
        m.push(("pos".to_string(), self.u.tile.clone()));
        m.push((
            "markedForDeletion".to_string(),
            coalesce(Some(&self.u.deletion_at), &JsVal::Bool(false)).clone(),
        ));
        m.push(("targetable".to_string(), self.u.targetable.clone()));
        m.push(("lastPos".to_string(), self.u.last_tile.clone()));
        let health = if self.has_health(tr) {
            JsVal::Num(self.u.health as f64)
        } else {
            JsVal::Undef
        };
        m.push(("health".to_string(), health));
        m.push(("underConstruction".to_string(), self.u.under_construction.clone()));
        let tu_id = if nullish(Some(&self.u.target_unit)) {
            JsVal::Undef
        } else {
            let v = self.tu_id(tr);
            if nullish(Some(&v)) {
                JsVal::Undef
            } else {
                v
            }
        };
        m.push(("targetUnitId".to_string(), tu_id));
        m.push((
            "targetTile".to_string(),
            coalesce(Some(&self.u.target_tile), &JsVal::Undef).clone(),
        ));
        m.push((
            "missileTimerQueue".to_string(),
            JsVal::Arr(self.u.missile_timer_queue.iter().map(|&x| JsVal::Num(x)).collect()),
        ));
        m.push(("level".to_string(), JsVal::Num(self.u.level)));
        m.push(("hasTrainStation".to_string(), self.u.has_train_station.clone()));
        m.push(("trainType".to_string(), self.u.train_type.clone()));
        m.push(("loaded".to_string(), self.u.loaded.clone()));
        m
    }

    /// `delete(displayMessage, destroyer)` — shared by the op and the
    /// `modifyHealth` / `decreaseLevel` chains. `Err(msg)` on an inactive
    /// unit (the `toString` name facade already rode `tr`).
    fn do_delete(&mut self, dm: &JsVal, destroyer: JsVal, tr: &mut Vec<f64>) -> Result<(), String> {
        if !self.u.active {
            let name = self.owner_name(tr, self.u.owner_idx);
            return Err(format!(
                "cannot delete Unit:{},owner:{} not active",
                self.u.type_name, name
            ));
        }
        self.u.was_destroyed_by_enemy = destroyer != JsVal::Undef && destroyer != JsVal::Absent;
        self.u.destroyer = coalesce(Some(&destroyer), &JsVal::Undef).clone();
        {
            let o = &mut self.owners[self.u.owner_idx];
            o.units.retain(|&s| s != 0.0);
            o.my_units_version += 1.0;
        }
        self.u.active = false;
        self.add_update(tr);
        self.remove_unit(tr);
        if dm != &JsVal::Bool(false) {
            // displayMessageOnDeleted: only Warship / TransportShip.
            let t = self.u.type_name.clone();
            if t == "Warship" || t == "Transport" {
                let vars = if t == "Transport" {
                    "unit_type.boat"
                } else {
                    "unit_type.warship"
                };
                self.display_message(tr, vars);
            }
        }
        if destroyer != JsVal::Undef && destroyer != JsVal::Absent {
            let owner_pid = JsVal::Num(self.owners[self.u.owner_idx].pid);
            let troops = self.u.troops.clone();
            let t = self.u.type_name.clone();
            if t == "Transport" {
                self.stats(tr);
                tr.push(66.0);
                push_val(tr, &destroyer);
                push_val(tr, &owner_pid);
                push_val(tr, &troops);
            } else if t == "Trade Ship" {
                self.stats(tr);
                tr.push(67.0);
                push_val(tr, &destroyer);
                push_val(tr, &owner_pid);
            } else if is_structure(&t) {
                self.stats(tr);
                tr.push(68.0);
                push_val(tr, &destroyer);
                push_str(tr, &t);
                self.stats(tr);
                tr.push(64.0);
                push_val(tr, &owner_pid);
                push_str(tr, &t);
            }
        }
        Ok(())
    }

    // --- the op dispatcher ---

    /// Run one op. Kind table (see `tools/gen_vectors.mjs`):
    /// 0 construct `[encS type, tile, id, ownerBlock, params, mgBlock]` ->
    /// `[traceLen,(trace)*,0]` (the ctor's facade calls ride the trace);
    /// 1 method call `[mid, ...args]` -> `[traceLen,(trace)*,[0,...encVal] |
    /// [1,(msg)]]`. mid follows the TS declaration order (the table in
    /// `tools/gen_vectors.mjs` documents both sides). Facade trace events are
    /// documented at the event-code table.
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut c = Cur(args, 0);
        let mut tr: Vec<f64> = Vec::new();
        let mut out: Vec<f64> = Vec::new();
        match kind {
            0 => {
                self.reset();
                let type_name = c.s();
                let tile = c.val();
                let id = c.val();
                let owner = c.owner();
                let params = c.val();
                let mg = c.mg();
                self.owners.push(owner);
                self.mg = mg;
                self.u = Unit {
                    type_name: type_name.clone(),
                    tile: tile.clone(),
                    id: id.clone(),
                    ..Unit::default()
                };
                self.ctor(&params, &mut tr);
                out.push(tr.len() as f64);
                out.extend(tr.iter().copied());
                out.push(0.0); // ctor success payload
                out
            }
            1 => {
                let mid = c.u() as u8;
                self.run_method(mid, &mut c, &mut tr, &mut out);
                let mut full = Vec::with_capacity(1 + tr.len() + out.len());
                full.push(tr.len() as f64);
                full.extend(tr.iter().copied());
                full.extend(out.iter().copied());
                full
            }
            k => unreachable!("unit harness: unknown op kind {k}"),
        }
    }

    /// The ctor body (UnitImpl.ts:52-114), in TS execution order.
    fn ctor(&mut self, params: &JsVal, tr: &mut Vec<f64>) {
        let p: Vec<(String, JsVal)> = match params {
            JsVal::Obj(m) => m.clone(),
            _ => Vec::new(), // `params: AllUnitParams = {}` — undefined /
                             // absent / null all default to the empty object
        };
        self.u.last_tile = self.u.tile.clone();
        let info = self.mg_unit_info(tr);
        let mh = match &info {
            JsVal::Obj(m) => to_num(coalesce(field(m, "maxHealth"), &JsVal::Num(1.0))),
            _ => 1.0,
        };
        // `toInt` on a NaN maxHealth throws; the capture domain keeps it
        // numeric, and the ctor error path is out of the scenario domain.
        self.u.health = to_int_big(mh).unwrap_or(0);
        self.u.target_tile = match field(&p, "targetTile") {
            Some(v) => coalesce(Some(v), &JsVal::Undef).clone(),
            None => JsVal::Undef,
        };
        self.u.target_player = match field(&p, "targetPlayer") {
            Some(v) => coalesce(Some(v), &JsVal::Undef).clone(),
            None => JsVal::Undef,
        };
        if p.iter().any(|(k, _)| k == "trajectory") || p.iter().any(|(k, _)| k == "waitTicks") {
            // `params.trajectory ?? []` — `??` fires only on nullish, so a
            // non-array raw value is preserved (the capture domain scripts
            // arrays / nullish only).
            let traj = match field(&p, "trajectory") {
                Some(v) => coalesce(Some(v), &JsVal::Arr(Vec::new())).clone(),
                None => JsVal::Arr(Vec::new()),
            };
            self.u.nuke_state = Some(vec![
                ("trajectory".to_string(), traj),
                ("waitTicks".to_string(), JsVal::Num(0.0)),
                ("trajectoryIndex".to_string(), JsVal::Num(0.0)),
                ("targetedBySam".to_string(), JsVal::Bool(false)),
            ]);
        }
        self.u.troops = match field(&p, "troops") {
            Some(v) => coalesce(Some(v), &JsVal::Num(0.0)).clone(),
            None => JsVal::Num(0.0),
        };
        self.u.last_set_safe_from_pirates = match field(&p, "lastSetSafeFromPirates") {
            Some(v) => coalesce(Some(v), &JsVal::Num(0.0)).clone(),
            None => JsVal::Num(0.0),
        };
        if self.u.type_name == "Transport" {
            self.u.transport_ship_state = Some(vec![
                ("isRetreating".to_string(), JsVal::Bool(false)),
                ("troops".to_string(), JsVal::Num(0.0)),
            ]);
        }
        if self.u.type_name == "SAM Launcher" {
            let sr = self.cfg_sam_range(tr, 1.0);
            let dur = self.cfg_sam_dur(tr);
            self.u.sam_launcher_state = Some(vec![
                ("startRange".to_string(), JsVal::Num(sr)),
                ("targetLevel".to_string(), JsVal::Num(1.0)),
                ("duration".to_string(), JsVal::Num(dur)),
            ]);
        }
        if let Some(pt) = field(&p, "patrolTile") {
            self.u.warship_state = Some(vec![
                ("state".to_string(), JsVal::Str("patrolling".to_string())),
                ("patrolTile".to_string(), pt.clone()),
                ("lastCombatTick".to_string(), JsVal::Num(-100.0)),
                ("veterancy".to_string(), JsVal::Num(0.0)),
                ("veterancyProgress".to_string(), JsVal::Num(0.0)),
            ]);
        }
        self.u.target_unit = match field(&p, "targetUnit") {
            Some(v) => coalesce(Some(v), &JsVal::Undef).clone(),
            None => JsVal::Undef,
        };
        self.u.loaded = match field(&p, "loaded") {
            Some(v) => coalesce(Some(v), &JsVal::Undef).clone(),
            None => JsVal::Undef,
        };
        // NO `??` here: a present null stays null.
        self.u.train_type = match field(&p, "trainType") {
            Some(v) => v.clone(),
            None => JsVal::Undef,
        };
        if is_structure(&self.u.type_name) {
            self.stats(tr);
            let owner_pid = JsVal::Num(self.owners[self.u.owner_idx].pid);
            tr.push(62.0);
            push_val(tr, &owner_pid);
            push_str(tr, &self.u.type_name.clone());
        }
    }

    /// Method dispatch for kind 1. `mid` follows the TS declaration order.
    fn run_method(&mut self, mid: u8, c: &mut Cur, tr: &mut Vec<f64>, out: &mut Vec<f64>) {
        match mid {
            0 => {
                // setTargetable
                let v = c.val();
                if self.u.targetable != v {
                    self.u.targetable = v;
                    self.add_update(tr);
                }
                push_ok(out, &JsVal::Undef);
            }
            1 => push_ok(out, &self.u.targetable.clone()),
            2 => push_ok_bool(out, true),
            3 => {
                // touch
                self.add_update(tr);
                push_ok(out, &JsVal::Undef);
            }
            4 => {
                // setTileTarget
                self.u.target_tile = c.val();
                push_ok(out, &JsVal::Undef);
            }
            5 => push_ok(out, &self.u.target_tile.clone()),
            6 => push_ok(out, &self.u.id.clone()),
            7 => {
                let upd = self.build_update(tr);
                push_ok(out, &JsVal::Obj(upd));
            }
            8 => push_ok(out, &JsVal::Str(self.u.type_name.clone())),
            9 => push_ok(out, &self.u.last_tile.clone()),
            10 => {
                // move
                let tile = c.val();
                if tile == JsVal::Null {
                    push_err(out, "tile cannot be null");
                    return;
                }
                self.u.last_tile = self.u.tile.clone();
                self.u.tile = tile;
                self.on_unit_moved(tr);
                push_ok(out, &JsVal::Undef);
            }
            11 => {
                // setTroops
                let troops = c.val();
                let next = JsVal::Num(js_max(0.0, to_num(&troops)));
                if self.u.troops == next {
                    push_ok(out, &JsVal::Undef);
                    return;
                }
                self.u.troops = next;
                self.add_update(tr);
                push_ok(out, &JsVal::Undef);
            }
            12 => push_ok(out, &self.u.troops.clone()),
            13 => push_ok_num(out, self.u.health as f64),
            14 => {
                let h = self.has_health(tr);
                push_ok_bool(out, h);
            }
            15 => push_ok(out, &self.u.tile.clone()),
            16 => push_ok_num(out, self.owners[self.u.owner_idx].pid),
            17 => {
                let v = self.info(tr);
                push_ok(out, &v);
            }
            18 => {
                // setOwner [ownerBlock]
                let nb = c.owner();
                self.clear_pending_deletion();
                let t = self.u.type_name.clone();
                let old_idx = self.u.owner_idx;
                if is_structure(&t) {
                    let new_pid_placeholder = nb.pid;
                    self.stats(tr);
                    tr.push(63.0);
                    push_val(tr, &JsVal::Num(new_pid_placeholder));
                    push_str(tr, &t.clone());
                    self.stats(tr);
                    tr.push(64.0);
                    let old_pid = JsVal::Num(self.owners[old_idx].pid);
                    push_val(tr, &old_pid);
                    push_str(tr, &t.clone());
                } else if t == "Transport" {
                    self.stats(tr);
                    tr.push(65.0);
                    push_val(tr, &JsVal::Num(nb.pid));
                    let old_pid = JsVal::Num(self.owners[old_idx].pid);
                    push_val(tr, &old_pid);
                }
                self.owners.push(nb);
                let new_idx = self.owners.len() - 1;
                self.u.last_owner = Some(old_idx);
                {
                    let o = &mut self.owners[old_idx];
                    o.units.retain(|&s| s != 0.0);
                    o.my_units_version += 1.0;
                }
                self.u.owner_idx = new_idx;
                self.owners[new_idx].units.push(0.0);
                self.owners[new_idx].my_units_version += 1.0;
                self.bump_units_version(tr);
                self.add_update(tr);
                push_ok(out, &JsVal::Undef);
            }
            19 => {
                let mh = self.max_health(tr);
                push_ok_num(out, mh);
            }
            20 => {
                // modifyHealth [delta, attacker]
                let delta = c.val();
                let attacker = c.val();
                let d = match to_int_big(to_num(&delta)) {
                    Ok(d) => d,
                    Err(m) => {
                        push_err(out, &m);
                        return;
                    }
                };
                let mh = self.max_health(tr);
                let mh_b = match to_int_big(mh) {
                    Ok(m) => m,
                    Err(e) => {
                        push_err(out, &e);
                        return;
                    }
                };
                let previous = self.u.health;
                let next = within_int(previous + d, 0, mh_b);
                if next == previous {
                    push_ok(out, &JsVal::Undef);
                    return;
                }
                if attacker != JsVal::Undef
                    && attacker != JsVal::Absent
                    && to_num(&delta) < 0.0
                    && self.u.warship_state.is_some()
                {
                    let t = self.mg_ticks(tr);
                    if let Some(m) = self.u.warship_state.as_mut() {
                        map_set(m, "lastCombatTick", JsVal::Num(t));
                    }
                }
                self.u.health = next;
                self.add_update(tr);
                if self.u.health == 0 {
                    if let Err(e) = self.do_delete(&JsVal::Bool(true), attacker, tr) {
                        push_err(out, &e);
                        return;
                    }
                }
                push_ok(out, &JsVal::Undef);
            }
            21 => {
                self.clear_pending_deletion();
                push_ok(out, &JsVal::Undef);
            }
            22 => push_ok_bool(out, self.u.deletion_at != JsVal::Null),
            23 => {
                // markForDeletion
                if !self.u.active {
                    push_ok(out, &JsVal::Undef);
                    return;
                }
                let t = self.mg_ticks(tr);
                let dur = self.cfg_del_mark(tr);
                self.u.deletion_at = JsVal::Num(t + dur);
                self.add_update(tr);
                push_ok(out, &JsVal::Undef);
            }
            24 => {
                // isOverdueDeletion
                if !self.u.active {
                    push_ok_bool(out, false);
                    return;
                }
                let d = match &self.u.deletion_at {
                    JsVal::Null => None,
                    JsVal::Num(d) => Some(*d),
                    _ => None,
                };
                let r = match d {
                    None => false,
                    Some(d) => {
                        let t = self.mg_ticks(tr);
                        t - d > 0.0
                    }
                };
                push_ok_bool(out, r);
            }
            25 => {
                // delete [displayMessage, destroyer]
                let dm = c.val();
                let destroyer = c.val();
                match self.do_delete(&dm, destroyer, tr) {
                    Ok(()) => push_ok(out, &JsVal::Undef),
                    Err(m) => push_err(out, &m),
                }
            }
            26 => push_ok_bool(out, self.u.active),
            27 => push_ok_bool(out, self.u.was_destroyed_by_enemy),
            28 => push_ok(out, &self.u.destroyer.clone()),
            29 => match self.get_warship_state(tr) {
                Ok(v) => push_ok(out, &JsVal::Obj(v)),
                Err(m) => push_err(out, &m),
            },
            30 => {
                // updateWarshipState [update]
                let update = c.val();
                if self.u.warship_state.is_none() {
                    push_err(out, "updateWarshipState called on non-warship unit");
                    return;
                }
                let up: Vec<(String, JsVal)> = match &update {
                    JsVal::Obj(m) => m.clone(),
                    _ => Vec::new(),
                };
                if let Some(ic) = field(&up, "isInCombat") {
                    if truthy(ic) {
                        if let Err(e) = self.mark_in_combat(tr) {
                            push_err(out, &e);
                            return;
                        }
                    }
                }
                // merged = {...this._warshipState, ...update}
                let mut merged = self.u.warship_state.clone().unwrap();
                for (k, v) in up.clone() {
                    map_set(&mut merged, &k, v);
                }
                let cur = self.u.warship_state.clone().unwrap();
                let same = |k: &str| -> bool {
                    let a = coalesce(field(&merged, k), &JsVal::Undef);
                    let b = coalesce(field(&cur, k), &JsVal::Undef);
                    a == b
                };
                if same("state") && same("patrolTile") && same("retreatPort") {
                    push_ok(out, &JsVal::Undef);
                    return;
                }
                let g = |k: &str, src: &Vec<(String, JsVal)>| {
                    src.iter()
                        .find(|(x, _)| x == k)
                        .map(|(_, v)| v.clone())
                        .unwrap_or(JsVal::Undef)
                };
                self.u.warship_state = Some(vec![
                    ("state".to_string(), g("state", &merged)),
                    ("patrolTile".to_string(), g("patrolTile", &merged)),
                    ("retreatPort".to_string(), g("retreatPort", &merged)),
                    ("lastCombatTick".to_string(), g("lastCombatTick", &cur)),
                    ("veterancy".to_string(), g("veterancy", &cur)),
                    ("veterancyProgress".to_string(), g("veterancyProgress", &cur)),
                ]);
                self.add_update(tr);
                push_ok(out, &JsVal::Undef);
            }
            31 => match self.is_in_combat(tr) {
                Ok(v) => push_ok_bool(out, v),
                Err(m) => push_err(out, &m),
            },
            32 => match self.get_transport_ship_state() {
                Ok(v) => push_ok(out, &JsVal::Obj(v)),
                Err(m) => push_err(out, &m),
            },
            33 => {
                // updateTransportShipState [update]
                let update = c.val();
                if self.u.transport_ship_state.is_none() {
                    push_err(
                        out,
                        "updateTransportShipState called on non-transport-ship unit",
                    );
                    return;
                }
                let up: Vec<(String, JsVal)> = match &update {
                    JsVal::Obj(m) => m.clone(),
                    _ => Vec::new(),
                };
                let mut changed = false;
                if let Some(ir) = field(&up, "isRetreating") {
                    if *ir != JsVal::Undef {
                        let cur = self.u.transport_ship_state.as_ref().unwrap();
                        let cur_ir = coalesce(field(cur, "isRetreating"), &JsVal::Undef);
                        if cur_ir != ir {
                            let m = self.u.transport_ship_state.as_mut().unwrap();
                            map_set(m, "isRetreating", ir.clone());
                            changed = true;
                        }
                    }
                }
                if changed {
                    self.add_update(tr);
                }
                push_ok(out, &JsVal::Undef);
            }
            34 => match self.get_nuke_state() {
                Ok(v) => push_ok(out, &JsVal::Obj(v)),
                Err(m) => push_err(out, &m),
            },
            35 => {
                // updateNukeState [update]
                let update = c.val();
                if self.u.nuke_state.is_none() {
                    push_err(out, "updateNukeState called on non-nuke unit");
                    return;
                }
                let up: Vec<(String, JsVal)> = match &update {
                    JsVal::Obj(m) => m.clone(),
                    _ => Vec::new(),
                };
                let mut merged = self.u.nuke_state.clone().unwrap();
                for (k, v) in up {
                    map_set(&mut merged, &k, v);
                }
                let cur = self.u.nuke_state.clone().unwrap();
                let same = |k: &str| -> bool {
                    let a = coalesce(field(&merged, k), &JsVal::Undef);
                    let b = coalesce(field(&cur, k), &JsVal::Undef);
                    a == b
                };
                if same("targetedBySam") && same("trajectoryIndex") && same("waitTicks") {
                    push_ok(out, &JsVal::Undef);
                    return;
                }
                let g = |k: &str, src: &Vec<(String, JsVal)>| {
                    src.iter()
                        .find(|(x, _)| x == k)
                        .map(|(_, v)| v.clone())
                        .unwrap_or(JsVal::Undef)
                };
                self.u.nuke_state = Some(vec![
                    ("targetedBySam".to_string(), g("targetedBySam", &merged)),
                    ("trajectoryIndex".to_string(), g("trajectoryIndex", &merged)),
                    ("waitTicks".to_string(), g("waitTicks", &merged)),
                    ("trajectory".to_string(), g("trajectory", &cur)),
                ]);
                self.add_update(tr);
                push_ok(out, &JsVal::Undef);
            }
            36 => push_ok(out, &self.u.under_construction.clone()),
            37 => {
                // setUnderConstruction
                let v = c.val();
                if self.u.under_construction != v {
                    self.u.under_construction = v;
                    self.owners[self.u.owner_idx].my_units_version += 1.0;
                    self.add_update(tr);
                }
                push_ok(out, &JsVal::Undef);
            }
            38 => {
                // hash
                let sh = simple_hash(&self.u.type_name.clone());
                let h = to_num(&self.u.tile.clone()) + sh * to_num(&self.u.id.clone());
                push_ok_num(out, h);
            }
            39 => {
                // toString
                let name = self.owner_name(tr, self.u.owner_idx);
                let s = format!("Unit:{},owner:{}", self.u.type_name, name);
                push_ok(out, &JsVal::Str(s));
            }
            40 => {
                // launch
                let t = self.mg_ticks(tr);
                self.u.missile_timer_queue.push(t);
                self.add_update(tr);
                push_ok(out, &JsVal::Undef);
            }
            41 => match self.u.missile_timer_queue.first() {
                Some(v) => push_ok_num(out, *v),
                None => push_ok(out, &JsVal::Undef),
            },
            42 => push_ok_bool(
                out,
                self.u.missile_timer_queue.len() as f64 == self.u.level,
            ),
            43 => push_ok(
                out,
                &JsVal::Arr(
                    self.u
                        .missile_timer_queue
                        .iter()
                        .map(|&x| JsVal::Num(x))
                        .collect(),
                ),
            ),
            44 => match &self.u.sam_launcher_state {
                Some(m) => push_ok(out, &JsVal::Obj(m.clone())),
                None => push_ok(out, &JsVal::Undef),
            },
            45 => {
                // reloadMissile — shift(): an empty queue is a no-op.
                if !self.u.missile_timer_queue.is_empty() {
                    self.u.missile_timer_queue.remove(0);
                }
                self.add_update(tr);
                push_ok(out, &JsVal::Undef);
            }
            46 => {
                // setTargetTile
                self.u.target_tile = c.val();
                push_ok(out, &JsVal::Undef);
            }
            47 => push_ok(out, &self.u.target_tile.clone()),
            48 => push_ok(out, &self.u.target_player.clone()),
            49 => {
                // setTrajectoryIndex
                let i = c.f();
                if self.u.nuke_state.is_none() {
                    push_err(out, "setTrajectoryIndex called on non-nuke unit");
                    return;
                }
                // `this.trajectory().length` — the getter coalesces nullish to
                // `[]` (length 0); a non-array, non-nullish raw value reads
                // `.length` as `undefined` -> `max` is NaN and BOTH relational
                // gates fail, so the index passes through unclamped.
                let traj_len = match &self.u.nuke_state {
                    Some(m) => match field(m, "trajectory") {
                        Some(JsVal::Arr(items)) => items.len() as f64,
                        Some(JsVal::Str(s)) => s.encode_utf16().count() as f64,
                        Some(JsVal::Absent) | Some(JsVal::Undef) | Some(JsVal::Null) => 0.0,
                        _ => f64::NAN,
                    },
                    None => 0.0,
                };
                let max = traj_len - 1.0;
                let idx = if i < 0.0 {
                    0.0
                } else if i > max {
                    max
                } else {
                    i
                };
                let m = self.u.nuke_state.as_mut().unwrap();
                map_set(m, "trajectoryIndex", JsVal::Num(idx));
                push_ok(out, &JsVal::Undef);
            }
            50 => {
                let v = match &self.u.nuke_state {
                    Some(m) => coalesce(field(m, "trajectoryIndex"), &JsVal::Num(0.0)).clone(),
                    None => JsVal::Num(0.0),
                };
                push_ok(out, &v);
            }
            51 => {
                let v = match &self.u.nuke_state {
                    Some(m) => coalesce(field(m, "trajectory"), &JsVal::Arr(Vec::new())).clone(),
                    None => JsVal::Arr(Vec::new()),
                };
                push_ok(out, &v);
            }
            52 => {
                // setTargetUnit
                self.u.target_unit = c.val();
                push_ok(out, &JsVal::Undef);
            }
            53 => push_ok(out, &self.u.target_unit.clone()),
            54 => {
                // setTargetedBySAM
                let v = c.val();
                match &mut self.u.nuke_state {
                    Some(m) => map_set(m, "targetedBySam", v),
                    None => {
                        push_err(out, "Cannot set properties of undefined (setting 'targetedBySam')");
                        return;
                    }
                }
                push_ok(out, &JsVal::Undef);
            }
            55 => {
                // targetedBySAM
                match &self.u.nuke_state {
                    Some(m) => {
                        let v = field(m, "targetedBySam").cloned().unwrap_or(JsVal::Undef);
                        push_ok(out, &v);
                    }
                    None => push_err(
                        out,
                        "Cannot read properties of undefined (reading 'targetedBySam')",
                    ),
                }
            }
            56 => {
                self.u.reached_target = true;
                push_ok(out, &JsVal::Undef);
            }
            57 => push_ok_bool(out, self.u.reached_target),
            58 => {
                // setSafeFromPirates
                let t = self.mg_ticks(tr);
                self.u.last_set_safe_from_pirates = JsVal::Num(t);
                push_ok(out, &JsVal::Undef);
            }
            59 => {
                // isSafeFromPirates
                let t = self.mg_ticks(tr);
                let cmax = self.cfg_safe_pirates(tr);
                let lss = to_num(&self.u.last_set_safe_from_pirates.clone());
                push_ok_bool(out, t - lss < cmax);
            }
            60 => push_ok_num(out, self.u.level),
            61 => {
                let v = match &self.u.warship_state {
                    Some(m) => coalesce(field(m, "veterancy"), &JsVal::Num(0.0)).clone(),
                    None => JsVal::Num(0.0),
                };
                push_ok(out, &v);
            }
            62 => {
                // recordKill [targetType]
                let tt = c.s();
                if self.u.warship_state.is_none() {
                    push_ok(out, &JsVal::Undef);
                    return;
                }
                if tt == "Warship" {
                    let m = self.u.warship_state.as_mut().unwrap();
                    map_set(m, "veterancyProgress", JsVal::Num(0.0));
                    self.increase_veterancy(tr);
                } else if tt == "Transport" {
                    self.add_veterancy_progress(tr, "Transport");
                }
                push_ok(out, &JsVal::Undef);
            }
            63 => {
                // recordTradeCapture
                if self.u.warship_state.is_none() {
                    push_ok(out, &JsVal::Undef);
                    return;
                }
                self.add_veterancy_progress(tr, "Trade Ship");
                push_ok(out, &JsVal::Undef);
            }
            64 => {
                // setTrainStation
                let v = c.val();
                self.u.has_train_station = v;
                self.add_update(tr);
                push_ok(out, &JsVal::Undef);
            }
            65 => push_ok(out, &self.u.has_train_station.clone()),
            66 => {
                // increaseLevel
                if self.u.type_name == "SAM Launcher" {
                    let tick = self.mg_ticks(tr);
                    let range = self.cfg_dyn_sam(tr, tick);
                    let dur = self.cfg_sam_dur(tr);
                    self.u.sam_launcher_state = Some(vec![
                        ("upgradeStartTick".to_string(), JsVal::Num(tick)),
                        ("startRange".to_string(), JsVal::Num(range)),
                        ("targetLevel".to_string(), JsVal::Num(self.u.level + 1.0)),
                        ("duration".to_string(), JsVal::Num(dur)),
                    ]);
                }
                self.u.level += 1.0;
                self.bump_units_version(tr);
                self.owners[self.u.owner_idx].my_units_version += 1.0;
                if is_silo_or_sam(&self.u.type_name.clone()) {
                    let t = self.mg_ticks(tr);
                    self.u.missile_timer_queue.push(t);
                }
                self.add_update(tr);
                push_ok(out, &JsVal::Undef);
            }
            67 => {
                // decreaseLevel [destroyer]
                let destroyer = c.val();
                self.u.level -= 1.0;
                if is_silo_or_sam(&self.u.type_name.clone()) {
                    self.u.missile_timer_queue.pop();
                }
                if self.u.type_name == "SAM Launcher" {
                    self.u.sam_launcher_state = None;
                }
                if self.u.level <= 0.0 {
                    match self.do_delete(&JsVal::Bool(true), destroyer, tr) {
                        Ok(()) => push_ok(out, &JsVal::Undef),
                        Err(m) => push_err(out, &m),
                    }
                    return;
                }
                self.bump_units_version(tr);
                self.owners[self.u.owner_idx].my_units_version += 1.0;
                self.add_update(tr);
                push_ok(out, &JsVal::Undef);
            }
            68 => push_ok(out, &self.u.train_type.clone()),
            69 => push_ok(out, &self.u.loaded.clone()),
            70 => {
                // setLoaded
                let v = c.val();
                if self.u.loaded != v {
                    self.u.loaded = v;
                    self.add_update(tr);
                }
                push_ok(out, &JsVal::Undef);
            }
            71 => {
                // dumpOwner [idx]
                let idx = c.u();
                let o = &self.owners[idx];
                out.push(0.0);
                out.push(o.pid);
                out.push(o.units.len() as f64);
                out.extend(o.units.iter().copied());
                out.push(o.my_units_version);
            }
            k => unreachable!("unit harness: unknown method id {k}"),
        }
    }

    fn clear_pending_deletion(&mut self) {
        self.u.deletion_at = JsVal::Null;
    }
}
