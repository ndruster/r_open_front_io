//! Bit-exact port of the G5a execution layer part 1: the four alliance
//! executions plus eight trivial leaf executions, and the `Game`-facade helper
//! `wouldNukeBreakAlliance` (`src/core/execution/Util.ts`), which is needed by
//! `AllianceRequestExecution.cancelNukesBetweenAlliedPlayers`.
//!
//! Ported TS files (READ-ONLY source `d:\work_space\OpenFrontIO`):
//!
//! * `src/core/execution/NoOpExecution.ts`
//! * `src/core/execution/PauseExecution.ts`
//! * `src/core/execution/SpawnTimerExecution.ts`
//! * `src/core/execution/RecomputeRailClusterExecution.ts`
//! * `src/core/execution/MarkDisconnectedExecution.ts`
//! * `src/core/execution/EmbargoExecution.ts`
//! * `src/core/execution/EmbargoAllExecution.ts`
//! * `src/core/execution/TargetPlayerExecution.ts`
//! * `src/core/execution/alliance/AllianceRequestExecution.ts`
//! * `src/core/execution/alliance/AllianceExtensionExecution.ts`
//! * `src/core/execution/alliance/BreakAllianceExecution.ts`
//! * `src/core/execution/alliance/AllianceRejectExecution.ts`
//! * `src/core/execution/Util.ts` (`wouldNukeBreakAlliance` only; the pure
//!   `GameMap` subset — `computeNukeBlastCounts` / `getSpawnTiles` /
//!   `closestTile` / `nearestTileDist` / `nearestTileDistCapped` /
//!   `closestTwoTiles` — already lives in [`crate::exec_util`]).
//!
//! # Harness shape
//!
//! One [`ExecHarness`] carries the scenario state across ops (like a scenario's
//! class instance plus its scripted facade mocks). Each call to
//! [`ExecHarness::run_op`] is one `kind`:
//!
//! * `kind 0` construct — `args` is ONE `js_json`-encoded object
//!   `{c, a, mg, players, alliances, requests, units, neighbors}` (see the
//!   script format below). Returns `[0, 0]` (trace length 0, then the `0`
//!   marker): no ctor makes a facade call.
//! * `kind 1` `init(ticks)` — `args = [ticks]`.
//! * `kind 2` `tick(ticks)` — `args = [ticks]`.
//! * `kind 3` `isActive()` — `args = []`.
//! * `kind 4` `activeDuringSpawnPhase()` — `args = []`.
//!
//! Method ops return `[traceLen, (trace)*, payload]` where `payload` is
//! `[0, ...encVal(ret)]` on success or `[1, ...encS(errMessage)]` for a thrown
//! JS `Error` / `TypeError` (the `[1, …]` arm mirrors the capture's
//! `[1, ...encS(msg)]`).
//!
//! # Class id table (mirrored by `tools/gen_vectors.mjs`)
//!
//! `0` NoOp, `1` Pause, `2` SpawnTimer, `3` RecomputeRailCluster,
//! `4` MarkDisconnected, `5` Embargo, `6` EmbargoAll, `7` TargetPlayer,
//! `8` AllianceRequest, `9` AllianceExtension, `10` BreakAlliance,
//! `11` AllianceReject.
//!
//! G5b appends `12` Retreat, `13` BoatRetreat, `14` MoveWarship, `15`
//! DeleteUnit, `16` QuickChat, `17` MissileSilo, `18` UpgradeStructure,
//! `19` City, `20` Factory, `21` DefensePost.
//!
//! # G5b facade additions (codes ≥ 300)
//!
//! `mg`: `300` isValidRef, `301` getWaterComponent, `302` hasWaterComponent,
//! `303` unit, `304` isLand, `305` hasUnitNearby, `306` nearbyUnits,
//! `307` addExecution (construction record), `308` displayChat,
//! `309` config().trainStationMaxRange(), `310` config().SiloCooldown(),
//! `311` config().defensePostShellAttackRate() (only reachable from the
//! unreachable `DefensePost.shoot`).
//! `player`: `320` orderRetreat, `321` executeRetreat, `322` canSendQuickChat,
//! `323` recordQuickChat, `324` canDeleteUnit, `325` recordDeleteUnit,
//! `326` canUpgradeUnit, `327` upgradeUnit.
//! `unit`: `340` id, `341` updateTransportShipState, `342` updateWarshipState,
//! `343` setTargetTile, `344` tile, `345` isUnderConstruction,
//! `346` missileTimerQueue, `347` reloadMissile, `348` markForDeletion,
//! `349` isOverdueDeletion, `350` hasTrainStation, `351` setTrainStation,
//! `352` hasHealth (only reachable from the unreachable `DefensePost.shoot`).
//!
//! # Constructing un-ported downstream executions
//!
//! `TrainStationExecution` (and `ShellExecution`) are intentionally NOT ported.
//! `mg.addExecution(new TrainStationExecution(unit, spawnTrains?))` is modelled
//! as an `addExecution` facade event (code 307) whose prefix is the construct
//! tag plus the encoded ctor args — exactly how `execution_manager.rs` records
//! `new XxxExecution(...)`. On the capture side we import the REAL
//! `TrainStationExecution` class (option (a)): its ctor runs against the
//! scripted unit mock and therefore emits `unit.setTrainStation(true)` FIRST
//! (code 351), then the `addExecution` record — this keeps the trace faithful
//! to the actual TS runtime and is mirrored verbatim on the Rust side
//! (`mg_add_train_station` calls `un_set_train_station` then pushes the
//! record). `ShellExecution` is likewise un-ported; its only construction site
//! is the unreachable `DefensePost.shoot`, so it is modelled by
//! `mg_add_execution_shell` (tag 1) but is never exercised by any scenario.
//!
//! # Facade (mock) surface — the capture scripts these, every call traced
//!
//! Players / alliances / alliance-requests / units / neighbors cross as TOKENS
//! (index into the harness tables); the TS `===` reference-identity gates are
//! modelled as token equality (the `ui_` / `alliance_impl` token convention).
//! Scripted returns are consumed FIFO per scenario; an exhausted list REPEATS
//! its last value on both sides, and an empty list throws on the first call
//! (so every used method must be scripted with at least one value).
//!
//! ## `mg` (Game) facade — slots 0..18
//!
//! | slot | code | method |
//! |------|------|--------|
//! | 0  | 210 | `hasPlayer(id)` |
//! | 1  | 211 | `player(id)` |
//! | 2  | 212 | `ticks()` |
//! | 3  | 213 | `config().allianceRequestDuration()` |
//! | 4  | 214 | `config().gameConfig().gameType` (read) |
//! | 5  | 215 | `config().numSpawnPhaseTurns()` |
//! | 6  | 216 | `inSpawnPhase()` |
//! | 7  | 217 | `endSpawnPhase()` |
//! | 8  | 218 | `setPaused(paused)` |
//! | 9  | 219 | `teamTilesOwned(team)` |
//! | 10 | 220 | `totalLandTiles()` |
//! | 11 | 221 | `players()` |
//! | 12 | 222 | `owner(tile)` |
//! | 13 | 223 | `ownerID(tile)` |
//! | 14 | 224 | `config().nukeMagnitudes(type)` |
//! | 15 | 225 | `config().nukeAllianceBreakThreshold()` |
//! | 16 | 226 | `displayMessage(...)` (7 args) |
//! | 17 | 227 | `anyUnitNearby(tile, outer, types, cb)` |
//! | 18 | 228 | `circleSearch(tile, outer, cb)` |
//! | 19 | 300 | `isValidRef(tile)` |
//! | 20 | 301 | `getWaterComponent(tile)` |
//! | 21 | 302 | `hasWaterComponent(tile, comp)` |
//! | 22 | 303 | `unit(id)` |
//! | 23 | 304 | `isLand(tile)` |
//! | 24 | 305 | `hasUnitNearby(tile, outer, type)` |
//! | 25 | 306 | `nearbyUnits(tile, outer, types)` |
//! | 28 | 309 | `config().trainStationMaxRange()` |
//! | 29 | 310 | `config().SiloCooldown()` |
//! | 30 | 311 | `config().defensePostShellAttackRate()` (unreachable) |
//!
//! `307` `addExecution(...)` (construction record) and `308` `displayChat(...)`
//! are synthetic events: they consume no script slot on either side.
//!
//! `config()` / `gameConfig()` are live-object accessors; only the leaf
//! accessor is traced (one event per full chain read).
//!
//! ## `player` facade — slots 0..33
//!
//! `0` 230 `smallID()`, `1` 231 `id()`, `2` 232 `displayName()`,
//! `3` 233 `isAlive()`, `4` 234 `isLobbyCreator()`,
//! `5` 235 `canSendAllianceRequest(other)` (BOOL),
//! `6` 236 `outgoingAllianceRequests()`, `7` 237 `updateRelation(other, amt)`,
//! `8` 238 `hasEmbargoAgainst(other)` (BOOL),
//! `9` 239 `endTemporaryEmbargo(other)`, `10` 240 `createAllianceRequest(other)`,
//! `11` 241 `units(types)`, `12` 242 `isFriendly(other)` (BOOL),
//! `13` 243 `allianceWith(other)`, `14` 244 `breakAlliance(alliance)`,
//! `15` 245 `nearby()`, `16` 246 `canTarget(other)` (BOOL), `17` 247 `target(other)`,
//! `18` 248 `team()`, `19` 249 `markDisconnected(flag, obj|none)`,
//! `20` 250 `isOnSameTeam(other)` (BOOL), `21` 251 `type()`,
//! `22` 252 `canEmbargoAll()` (BOOL), `23` 253 `addEmbargo(other, flag)`,
//! `24` 254 `stopEmbargo(other)`, `25` 255 `recordEmbargoAll()`,
//! `26` 259 `isPlayer()` (BOOL — the `anyUnitNearby` callback reads the unit's
//! owner through it), `29` 322 `canSendQuickChat(other)` (BOOL),
//! `31` 324 `canDeleteUnit()` (BOOL), `33` 326 `canUpgradeUnit(unit)` (BOOL).
//! The void G5b additions (`320` orderRetreat, `321` executeRetreat,
//! `323` recordQuickChat, `325` recordDeleteUnit, `327` upgradeUnit) consume
//! no slot.
//!
//! ## `alliance` facade — slots 0..3
//!
//! `0` 260 `onlyOneAgreedToExtend()`, `1` 261 `addExtensionRequest(player)`,
//! `2` 262 `bothAgreedToExtend()`, `3` 263 `extend()`.
//!
//! ## `allianceRequest` facade — slots 0..4
//!
//! `0` 270 `recipient()`, `1` 271 `accept()`, `2` 272 `reject()`,
//! `3` 273 `status()`, `4` 274 `createdAt()`.
//!
//! ## `unit` facade — slots 0..19
//!
//! `0` 280 `isActive()`, `1` 281 `reachedTarget()`, `2` 282 `targetTile()`,
//! `3` 283 `type()`, `4` 284 `targetPlayer()`, `5` 285 `delete(flag)`,
//! `6` 286 `owner()`, `7` 340 `id()`, `11` 344 `tile()`,
//! `12` 345 `isUnderConstruction()`, `13` 346 `missileTimerQueue()`,
//! `16` 349 `isOverdueDeletion()`, `17` 350 `hasTrainStation()`,
//! `19` 352 `hasHealth()` (unreachable). The void G5b additions
//! (`341` updateTransportShipState, `342` updateWarshipState,
//! `343` setTargetTile, `347` reloadMissile, `348` markForDeletion,
//! `351` setTrainStation) consume no slot.
//!
//! ## `neighbor` facade — slots 0..2
//!
//! `0` 257 `isPlayer()`, `1` 258 `isOnSameTeam(other)`,
//! `2` 256 `updateRelation(other, amt)`.
//!
//! ## `railNetwork` facade
//!
//! `290` `recomputeClusters()`.
//!
//! # Event framing
//!
//! Every non-callback facade event is `[code, (prefix)*, ...encVal(ret)]`. The
//! two callback-driven facades (`anyUnitNearby` 227 / `circleSearch` 228) push
//! their code and arguments BEFORE iterating their scripted replay list, then
//! the callback's own events, then the return — the `anyUnitNearby` script is a
//! list of unit tokens, the `circleSearch` script a list of `(tile, d2)` pairs.
//!
//! `Structures.types` (the `wouldNukeBreakAlliance` import) is
//! `["City", "Defense Post", "SAM Launcher", "Missile Silo", "Port", "Factory"]`
//! (Game.ts:233-240, declaration order of the `unitTypeGroup`).

use crate::js_json::{push_str, push_val, read_val, val_field, JsVal};

/// `MessageType.ALLIANCE_ACCEPTED` (numeric enum member 13).
const MSG_ALLIANCE_ACCEPTED: f64 = 13.0;
/// `MessageType.RENEW_ALLIANCE` (numeric enum member 21).
const MSG_RENEW_ALLIANCE: f64 = 21.0;
/// `PlayerType.Bot`.
const PLAYER_TYPE_BOT: &str = "BOT";
/// `GameType.Singleplayer`.
const GAME_TYPE_SINGLEPLAYER: &str = "Singleplayer";
/// `UnitType.MIRV` / `UnitType.MIRVWarhead` string members.
const UNIT_MIRV: &str = "MIRV";
const UNIT_MIRV_WARHEAD: &str = "MIRV Warhead";
/// `Structures.types` (Game.ts:233-240).
const STRUCTURES_TYPES: [&str; 6] = [
    "City",
    "Defense Post",
    "SAM Launcher",
    "Missile Silo",
    "Port",
    "Factory",
];

/// `UnitType` string members reused by the G5b executions.
const UNIT_TRANSPORT: &str = "Transport";
const UNIT_WARSHIP: &str = "Warship";
const UNIT_CITY: &str = "City";
const UNIT_PORT: &str = "Port";
const UNIT_FACTORY: &str = "Factory";
/// `MessageType.UNIT_DESTROYED` (numeric enum member 12).
const MSG_UNIT_DESTROYED: f64 = 12.0;
/// `RetreatExecution.cancelDelay`.
const RETREAT_CANCEL_DELAY: f64 = 20.0;
/// Construction-record tags (the order the capture's recorder assigns).
const TAG_TRAIN_STATION: f64 = 0.0;
const TAG_SHELL: f64 = 1.0;

// ---- codec / script helpers ------------------------------------------------

/// FIFO script consumption with the capture's exhausted-repeat rule.
#[derive(Clone, Debug, Default)]
struct Script {
    vals: Vec<JsVal>,
    i: usize,
}

impl Script {
    fn take(&mut self, tr: &mut Vec<f64>, code: f64, prefix: &[f64]) -> JsVal {
        assert!(
            !self.vals.is_empty(),
            "exec harness: unscripted facade method (code {code})"
        );
        let idx = self.i.min(self.vals.len() - 1);
        self.i += 1;
        let v = self.vals[idx].clone();
        tr.push(code);
        tr.extend_from_slice(prefix);
        push_val(tr, &v);
        v
    }
}

/// One mock object: a fixed-order array of per-method scripts.
#[derive(Clone, Debug, Default)]
struct Mock {
    slots: Vec<Script>,
}

fn build_script(v: &JsVal) -> Script {
    match v {
        JsVal::Arr(items) => Script {
            vals: items.clone(),
            i: 0,
        },
        _ => Script::default(),
    }
}

fn build_mock(v: &JsVal) -> Mock {
    match v {
        JsVal::Arr(items) => Mock {
            slots: items.iter().map(build_script).collect(),
        },
        _ => Mock::default(),
    }
}

fn as_arr(v: &JsVal) -> Vec<JsVal> {
    if let JsVal::Arr(a) = v {
        a.clone()
    } else {
        Vec::new()
    }
}

fn as_num(v: &JsVal) -> f64 {
    if let JsVal::Num(n) = v {
        *n
    } else {
        f64::NAN
    }
}

fn as_bool(v: &JsVal) -> bool {
    matches!(v, JsVal::Bool(true))
}

fn as_str(v: &JsVal) -> String {
    if let JsVal::Str(s) = v {
        s.clone()
    } else {
        String::new()
    }
}

fn as_str_opt(v: &JsVal) -> Option<String> {
    if let JsVal::Str(s) = v {
        Some(s.clone())
    } else {
        None
    }
}

fn as_opt_tok(v: &JsVal) -> Option<usize> {
    if let JsVal::Num(n) = v {
        Some(*n as usize)
    } else {
        None
    }
}

fn as_tok_list(v: &JsVal) -> Vec<usize> {
    if let JsVal::Arr(a) = v {
        a.iter().filter_map(as_opt_tok).collect()
    } else {
        Vec::new()
    }
}

fn as_num_list(v: &JsVal) -> Vec<f64> {
    if let JsVal::Arr(a) = v {
        a.iter().map(as_num).collect()
    } else {
        Vec::new()
    }
}

/// JS `SameValueZero` (the key relation used by `Set` / `Map`).
fn svz(a: f64, b: f64) -> bool {
    a == b || (a.is_nan() && b.is_nan())
}

/// JS `String.prototype.split(".")` for the `QuickChatExecution` chat key.
fn split_dot(s: &str) -> Vec<String> {
    s.split('.').map(|x| x.to_string()).collect()
}

fn as_pairs(v: &JsVal) -> Vec<(f64, f64)> {
    if let JsVal::Arr(a) = v {
        a.iter()
            .filter_map(|p| {
                if let JsVal::Arr(q) = p {
                    if q.len() >= 2 {
                        return Some((as_num(&q[0]), as_num(&q[1])));
                    }
                }
                None
            })
            .collect()
    } else {
        Vec::new()
    }
}

/// JS truthiness of a `TileRef` (tile `0` and `NaN` are falsy).
fn truthy_tile(t: Option<f64>) -> bool {
    match t {
        Some(x) => x != 0.0 && !x.is_nan(),
        None => false,
    }
}

// ---- encoded-argument helpers ---------------------------------------------

fn ev1(v: f64) -> Vec<f64> {
    vec![3.0, v]
}

fn evb(b: bool) -> Vec<f64> {
    vec![4.0, if b { 1.0 } else { 0.0 }]
}

fn evs(s: &str) -> Vec<f64> {
    let mut o = vec![5.0];
    push_str(&mut o, s);
    o
}

fn evtok(t: Option<usize>) -> Vec<f64> {
    match t {
        Some(i) => vec![3.0, i as f64],
        None => vec![1.0],
    }
}

fn evstr_opt(s: Option<&str>) -> Vec<f64> {
    match s {
        Some(x) => evs(x),
        None => vec![2.0],
    }
}

fn ev_enc(v: &JsVal) -> Vec<f64> {
    let mut o = Vec::new();
    push_val(&mut o, v);
    o
}

fn ev_strs(items: &[String]) -> Vec<f64> {
    let mut o = vec![7.0, items.len() as f64];
    for it in items {
        o.extend_from_slice(&evs(it));
    }
    o
}

fn ev_tok_list(items: &[usize]) -> Vec<f64> {
    let mut o = vec![7.0, items.len() as f64];
    for it in items {
        o.extend_from_slice(&ev1(*it as f64));
    }
    o
}

fn mk_take(m: &mut Mock, slot: usize, tr: &mut Vec<f64>, code: f64, prefix: &[f64]) -> JsVal {
    m.slots[slot].take(tr, code, prefix)
}

fn mk_void(tr: &mut Vec<f64>, code: f64, prefix: &[f64]) {
    tr.push(code);
    tr.extend_from_slice(prefix);
    push_val(tr, &JsVal::Undef);
}

// ---- the harness -----------------------------------------------------------

/// The mutable class state shared by the 12 executions. The names are the
/// reused slots: `a_tok` is requestor / player / from / launcher, `b_tok` the
/// AllianceReject recipient, `id_a` the ctor's string id (recipientID /
/// targetID / toID / requestorID), `flag` is `active` / `paused` /
/// `isDisconnected`, `target` / `req` are the init-resolved tokens.
#[derive(Clone, Debug, Default)]
pub struct ExecHarness {
    class_id: u8,
    a_tok: usize,
    b_tok: usize,
    id_a: String,
    action: String,
    flag: bool,
    target: Option<usize>,
    req: Option<usize>,
    mg_ready: bool,

    // G5b state.
    unit_id: f64,
    unit_ids: Vec<f64>,
    position: f64,
    amount: f64,
    quick_key: String,
    chat_target: Option<String>,
    start_tick: f64,
    retreat_ordered: bool,
    station_created: bool,
    unit: Option<usize>,
    recipient: Option<usize>,
    structure: Option<usize>,
    #[allow(dead_code)]
    last_shell_attack: f64,
    #[allow(dead_code)]
    already_sent_shell: Vec<usize>,

    mg: Mock,
    players: Vec<Mock>,
    alliances: Vec<Mock>,
    requests: Vec<Mock>,
    units: Vec<Mock>,
    neighbors: Vec<Mock>,
}

// mg slot indices.
const MG_HAS_PLAYER: usize = 0;
const MG_PLAYER: usize = 1;
const MG_TICKS: usize = 2;
const MG_ALLIANCE_REQUEST_DURATION: usize = 3;
const MG_GAME_TYPE: usize = 4;
const MG_NUM_SPAWN_PHASE_TURNS: usize = 5;
const MG_IN_SPAWN_PHASE: usize = 6;
const MG_TEAM_TILES_OWNED: usize = 9;
const MG_TOTAL_LAND_TILES: usize = 10;
const MG_PLAYERS: usize = 11;
const MG_OWNER: usize = 12;
const MG_OWNER_ID: usize = 13;
const MG_NUKE_MAGNITUDES: usize = 14;
const MG_NUKE_ALLIANCE_BREAK_THRESHOLD: usize = 15;
const MG_ANY_UNIT_NEARBY: usize = 17;
const MG_CIRCLE_SEARCH: usize = 18;
const MG_IS_VALID_REF: usize = 19;
const MG_GET_WATER_COMPONENT: usize = 20;
const MG_HAS_WATER_COMPONENT: usize = 21;
const MG_UNIT: usize = 22;
const MG_IS_LAND: usize = 23;
const MG_HAS_UNIT_NEARBY: usize = 24;
const MG_NEARBY_UNITS: usize = 25;
const MG_TRAIN_STATION_MAX_RANGE: usize = 28;
const MG_SILO_COOLDOWN: usize = 29;
const MG_DEFENSE_POST_SHELL_ATTACK_RATE: usize = 30;

// player slot indices.
const PL_SMALL_ID: usize = 0;
const PL_ID: usize = 1;
const PL_DISPLAY_NAME: usize = 2;
const PL_IS_ALIVE: usize = 3;
const PL_IS_LOBBY_CREATOR: usize = 4;
const PL_CAN_SEND_ALLIANCE_REQUEST: usize = 5;
const PL_OUTGOING_ALLIANCE_REQUESTS: usize = 6;
const PL_HAS_EMBARGO_AGAINST: usize = 8;
const PL_CREATE_ALLIANCE_REQUEST: usize = 10;
const PL_UNITS: usize = 11;
const PL_IS_FRIENDLY: usize = 12;
const PL_ALLIANCE_WITH: usize = 13;
const PL_NEARBY: usize = 15;
const PL_CAN_TARGET: usize = 16;
const PL_TEAM: usize = 18;
const PL_IS_ON_SAME_TEAM: usize = 20;
const PL_TYPE: usize = 21;
const PL_CAN_EMBARGO_ALL: usize = 22;
const PL_IS_PLAYER: usize = 26;
const PL_CAN_SEND_QUICK_CHAT: usize = 29;
const PL_CAN_DELETE_UNIT: usize = 31;
const PL_CAN_UPGRADE_UNIT: usize = 33;

// alliance slot indices.
const AL_ONLY_ONE_AGREED: usize = 0;
const AL_BOTH_AGREED: usize = 2;

// alliance-request slot indices.
const RQ_RECIPIENT: usize = 0;
const RQ_STATUS: usize = 3;
const RQ_CREATED_AT: usize = 4;

// unit slot indices.
const UN_IS_ACTIVE: usize = 0;
const UN_REACHED_TARGET: usize = 1;
const UN_TARGET_TILE: usize = 2;
const UN_TYPE: usize = 3;
const UN_TARGET_PLAYER: usize = 4;
const UN_OWNER: usize = 6;
const UN_ID: usize = 7;
const UN_TILE: usize = 11;
const UN_IS_UNDER_CONSTRUCTION: usize = 12;
const UN_MISSILE_TIMER_QUEUE: usize = 13;
const UN_IS_OVERDUE_DELETION: usize = 16;
const UN_HAS_TRAIN_STATION: usize = 17;
const UN_HAS_HEALTH: usize = 19;

// neighbor slot indices.
const NB_IS_PLAYER: usize = 0;
const NB_IS_ON_SAME_TEAM: usize = 1;

impl ExecHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    // ---- facade accessors ----

    fn mg_has_player(&mut self, tr: &mut Vec<f64>, id: &str) -> bool {
        let v = mk_take(&mut self.mg, MG_HAS_PLAYER, tr, 210.0, &evs(id));
        as_bool(&v)
    }
    fn mg_player(&mut self, tr: &mut Vec<f64>, id: &str) -> Option<usize> {
        let v = mk_take(&mut self.mg, MG_PLAYER, tr, 211.0, &evs(id));
        as_opt_tok(&v)
    }
    fn mg_ticks(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(&mut self.mg, MG_TICKS, tr, 212.0, &[]);
        as_num(&v)
    }
    fn mg_alliance_request_duration(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(&mut self.mg, MG_ALLIANCE_REQUEST_DURATION, tr, 213.0, &[]);
        as_num(&v)
    }
    fn mg_game_type(&mut self, tr: &mut Vec<f64>) -> String {
        let v = mk_take(&mut self.mg, MG_GAME_TYPE, tr, 214.0, &[]);
        as_str(&v)
    }
    fn mg_num_spawn_phase_turns(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(&mut self.mg, MG_NUM_SPAWN_PHASE_TURNS, tr, 215.0, &[]);
        as_num(&v)
    }
    fn mg_in_spawn_phase(&mut self, tr: &mut Vec<f64>) -> bool {
        let v = mk_take(&mut self.mg, MG_IN_SPAWN_PHASE, tr, 216.0, &[]);
        as_bool(&v)
    }
    fn mg_end_spawn_phase(&mut self, tr: &mut Vec<f64>) {
        mk_void(tr, 217.0, &[]);
    }
    fn mg_set_paused(&mut self, tr: &mut Vec<f64>, paused: bool) {
        mk_void(tr, 218.0, &evb(paused));
    }
    fn mg_team_tiles_owned(&mut self, tr: &mut Vec<f64>, team: Option<&str>) -> f64 {
        let v = mk_take(&mut self.mg, MG_TEAM_TILES_OWNED, tr, 219.0, &evstr_opt(team));
        as_num(&v)
    }
    fn mg_total_land_tiles(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(&mut self.mg, MG_TOTAL_LAND_TILES, tr, 220.0, &[]);
        as_num(&v)
    }
    fn mg_players(&mut self, tr: &mut Vec<f64>) -> Vec<usize> {
        let v = mk_take(&mut self.mg, MG_PLAYERS, tr, 221.0, &[]);
        as_tok_list(&v)
    }
    fn mg_owner(&mut self, tr: &mut Vec<f64>, tile: f64) -> Option<usize> {
        let v = mk_take(&mut self.mg, MG_OWNER, tr, 222.0, &ev1(tile));
        as_opt_tok(&v)
    }
    fn mg_owner_id(&mut self, tr: &mut Vec<f64>, tile: f64) -> f64 {
        let v = mk_take(&mut self.mg, MG_OWNER_ID, tr, 223.0, &ev1(tile));
        as_num(&v)
    }
    fn mg_nuke_magnitudes(&mut self, tr: &mut Vec<f64>, ty: &str) -> (f64, f64) {
        let v = mk_take(&mut self.mg, MG_NUKE_MAGNITUDES, tr, 224.0, &evs(ty));
        let inner = val_field(&v, "inner").map(as_num).unwrap_or(f64::NAN);
        let outer = val_field(&v, "outer").map(as_num).unwrap_or(f64::NAN);
        (inner, outer)
    }
    fn mg_nuke_alliance_break_threshold(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(
            &mut self.mg,
            MG_NUKE_ALLIANCE_BREAK_THRESHOLD,
            tr,
            225.0,
            &[],
        );
        as_num(&v)
    }
    fn mg_display_message(&mut self, tr: &mut Vec<f64>, args: &[JsVal]) {
        tr.push(226.0);
        for a in args {
            push_val(tr, a);
        }
        push_val(tr, &JsVal::Undef);
    }
    fn mg_is_valid_ref(&mut self, tr: &mut Vec<f64>, tile: f64) -> bool {
        let v = mk_take(&mut self.mg, MG_IS_VALID_REF, tr, 300.0, &ev1(tile));
        as_bool(&v)
    }
    fn mg_get_water_component(&mut self, tr: &mut Vec<f64>, tile: f64) -> JsVal {
        mk_take(&mut self.mg, MG_GET_WATER_COMPONENT, tr, 301.0, &ev1(tile))
    }
    fn mg_has_water_component(&mut self, tr: &mut Vec<f64>, tile: f64, comp: &JsVal) -> bool {
        let mut prefix = ev1(tile);
        push_val(&mut prefix, comp);
        let v = mk_take(&mut self.mg, MG_HAS_WATER_COMPONENT, tr, 302.0, &prefix);
        as_bool(&v)
    }
    fn mg_unit(&mut self, tr: &mut Vec<f64>, id: f64) -> Option<usize> {
        let v = mk_take(&mut self.mg, MG_UNIT, tr, 303.0, &ev1(id));
        as_opt_tok(&v)
    }
    fn mg_is_land(&mut self, tr: &mut Vec<f64>, tile: f64) -> bool {
        let v = mk_take(&mut self.mg, MG_IS_LAND, tr, 304.0, &ev1(tile));
        as_bool(&v)
    }
    fn mg_has_unit_nearby(&mut self, tr: &mut Vec<f64>, tile: f64, outer: f64, ty: &str) -> bool {
        let mut prefix = ev1(tile);
        prefix.extend_from_slice(&ev1(outer));
        prefix.extend_from_slice(&evs(ty));
        let v = mk_take(&mut self.mg, MG_HAS_UNIT_NEARBY, tr, 305.0, &prefix);
        as_bool(&v)
    }
    fn mg_nearby_units(
        &mut self,
        tr: &mut Vec<f64>,
        tile: f64,
        outer: f64,
        types: &[String],
    ) -> Vec<usize> {
        let mut prefix = ev1(tile);
        prefix.extend_from_slice(&ev1(outer));
        prefix.extend_from_slice(&ev_strs(types));
        let v = mk_take(&mut self.mg, MG_NEARBY_UNITS, tr, 306.0, &prefix);
        as_tok_list(&v)
    }
    /// `mg.addExecution(new TrainStationExecution(unit, spawnTrains?))` — the
    /// ctor side effect (`unit.setTrainStation(true)`, code 351) fires first,
    /// then the construction record (code 307, tag 0, ctor args).
    fn mg_add_train_station(&mut self, tr: &mut Vec<f64>, unit_tok: usize, spawn: Option<bool>) {
        self.un_set_train_station(tr, unit_tok, true);
        tr.push(307.0);
        tr.push(TAG_TRAIN_STATION);
        push_val(tr, &JsVal::Num(unit_tok as f64));
        push_val(
            tr,
            &match spawn {
                Some(b) => JsVal::Bool(b),
                None => JsVal::Undef,
            },
        );
        push_val(tr, &JsVal::Undef);
    }
    /// `mg.addExecution(new ShellExecution(...))` — only reachable from the
    /// unreachable `DefensePost.shoot`.
    #[allow(dead_code)]
    fn mg_add_execution_shell(
        &mut self,
        tr: &mut Vec<f64>,
        spawn: f64,
        owner: Option<usize>,
        owner_unit: usize,
        target: usize,
    ) {
        tr.push(307.0);
        tr.push(TAG_SHELL);
        push_val(tr, &JsVal::Num(spawn));
        push_val(
            tr,
            &match owner {
                Some(o) => JsVal::Num(o as f64),
                None => JsVal::Undef,
            },
        );
        push_val(tr, &JsVal::Num(owner_unit as f64));
        push_val(tr, &JsVal::Num(target as f64));
        push_val(tr, &JsVal::Undef);
    }
    #[allow(clippy::too_many_arguments)]
    fn mg_display_chat(
        &mut self,
        tr: &mut Vec<f64>,
        m1: Option<&str>,
        m0: Option<&str>,
        target: Option<&str>,
        pid: Option<&str>,
        is_from: bool,
        rec: Option<&str>,
    ) {
        tr.push(308.0);
        tr.extend_from_slice(&evstr_opt(m1));
        tr.extend_from_slice(&evstr_opt(m0));
        tr.extend_from_slice(&evstr_opt(target));
        tr.extend_from_slice(&evstr_opt(pid));
        tr.extend_from_slice(&evb(is_from));
        tr.extend_from_slice(&evstr_opt(rec));
        push_val(tr, &JsVal::Undef);
    }
    fn mg_train_station_max_range(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(&mut self.mg, MG_TRAIN_STATION_MAX_RANGE, tr, 309.0, &[]);
        as_num(&v)
    }
    fn mg_silo_cooldown(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(&mut self.mg, MG_SILO_COOLDOWN, tr, 310.0, &[]);
        as_num(&v)
    }
    #[allow(dead_code)]
    fn mg_defense_post_shell_attack_rate(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(
            &mut self.mg,
            MG_DEFENSE_POST_SHELL_ATTACK_RATE,
            tr,
            311.0,
            &[],
        );
        as_num(&v)
    }

    fn mg_any_unit_nearby(
        &mut self,
        tr: &mut Vec<f64>,
        tile: f64,
        outer: f64,
        types: &[String],
        mut cb: impl FnMut(&mut Self, &mut Vec<f64>, usize) -> bool,
    ) -> bool {
        let v = {
            let s = &mut self.mg.slots[MG_ANY_UNIT_NEARBY];
            assert!(!s.vals.is_empty(), "exec harness: unscripted anyUnitNearby");
            let idx = s.i.min(s.vals.len() - 1);
            s.i += 1;
            s.vals[idx].clone()
        };
        let units = as_tok_list(&v);
        tr.push(227.0);
        tr.extend_from_slice(&ev1(tile));
        tr.extend_from_slice(&ev1(outer));
        tr.extend_from_slice(&ev_strs(types));
        tr.extend_from_slice(&ev_tok_list(&units));
        let mut result = false;
        for uid in units {
            if cb(self, tr, uid) {
                result = true;
                break;
            }
        }
        push_val(tr, &JsVal::Bool(result));
        result
    }

    fn mg_circle_search(
        &mut self,
        tr: &mut Vec<f64>,
        tile: f64,
        outer: f64,
        mut cb: impl FnMut(&mut Self, &mut Vec<f64>, f64, f64) -> bool,
    ) {
        let v = {
            let s = &mut self.mg.slots[MG_CIRCLE_SEARCH];
            assert!(!s.vals.is_empty(), "exec harness: unscripted circleSearch");
            let idx = s.i.min(s.vals.len() - 1);
            s.i += 1;
            s.vals[idx].clone()
        };
        let pairs = as_pairs(&v);
        tr.push(228.0);
        tr.extend_from_slice(&ev1(tile));
        tr.extend_from_slice(&ev1(outer));
        for (t, d2) in pairs {
            if !cb(self, tr, t, d2) {
                break;
            }
        }
        push_val(tr, &JsVal::Undef);
    }

    // player facade
    fn pl(&mut self, tok: usize, slot: usize, tr: &mut Vec<f64>, code: f64, extra: &[f64]) -> JsVal {
        let mut prefix = vec![tok as f64];
        prefix.extend_from_slice(extra);
        mk_take(&mut self.players[tok], slot, tr, code, &prefix)
    }
    fn pl_void(&mut self, tok: usize, tr: &mut Vec<f64>, code: f64, extra: &[f64]) {
        let mut prefix = vec![tok as f64];
        prefix.extend_from_slice(extra);
        mk_void(tr, code, &prefix);
    }

    fn pl_small_id(&mut self, tr: &mut Vec<f64>, tok: usize) -> f64 {
        let v = self.pl(tok, PL_SMALL_ID, tr, 230.0, &[]);
        as_num(&v)
    }
    fn pl_id(&mut self, tr: &mut Vec<f64>, tok: usize) -> String {
        let v = self.pl(tok, PL_ID, tr, 231.0, &[]);
        as_str(&v)
    }
    fn pl_display_name(&mut self, tr: &mut Vec<f64>, tok: usize) -> String {
        let v = self.pl(tok, PL_DISPLAY_NAME, tr, 232.0, &[]);
        as_str(&v)
    }
    fn pl_is_alive(&mut self, tr: &mut Vec<f64>, tok: usize) -> bool {
        let v = self.pl(tok, PL_IS_ALIVE, tr, 233.0, &[]);
        as_bool(&v)
    }
    fn pl_is_lobby_creator(&mut self, tr: &mut Vec<f64>, tok: usize) -> bool {
        let v = self.pl(tok, PL_IS_LOBBY_CREATOR, tr, 234.0, &[]);
        as_bool(&v)
    }
    fn pl_can_send_alliance_request(&mut self, tr: &mut Vec<f64>, tok: usize, other: Option<usize>) -> bool {
        let v = self.pl(tok, PL_CAN_SEND_ALLIANCE_REQUEST, tr, 235.0, &evtok(other));
        as_bool(&v)
    }
    fn pl_outgoing_alliance_requests(&mut self, tr: &mut Vec<f64>, tok: usize) -> Vec<usize> {
        let v = self.pl(tok, PL_OUTGOING_ALLIANCE_REQUESTS, tr, 236.0, &[]);
        as_tok_list(&v)
    }
    fn pl_has_embargo_against(&mut self, tr: &mut Vec<f64>, tok: usize, other: Option<usize>) -> bool {
        let v = self.pl(tok, PL_HAS_EMBARGO_AGAINST, tr, 238.0, &evtok(other));
        as_bool(&v)
    }
    fn pl_create_alliance_request(&mut self, tr: &mut Vec<f64>, tok: usize, other: Option<usize>) -> Option<usize> {
        let v = self.pl(tok, PL_CREATE_ALLIANCE_REQUEST, tr, 240.0, &evtok(other));
        as_opt_tok(&v)
    }
    fn pl_units(&mut self, tr: &mut Vec<f64>, tok: usize, types: &[String]) -> Vec<usize> {
        let v = self.pl(tok, PL_UNITS, tr, 241.0, &ev_strs(types));
        as_tok_list(&v)
    }
    fn pl_is_friendly(&mut self, tr: &mut Vec<f64>, tok: usize, other: Option<usize>) -> bool {
        let v = self.pl(tok, PL_IS_FRIENDLY, tr, 242.0, &evtok(other));
        as_bool(&v)
    }
    fn pl_alliance_with(&mut self, tr: &mut Vec<f64>, tok: usize, other: Option<usize>) -> Option<usize> {
        let v = self.pl(tok, PL_ALLIANCE_WITH, tr, 243.0, &evtok(other));
        as_opt_tok(&v)
    }
    fn pl_nearby(&mut self, tr: &mut Vec<f64>, tok: usize) -> Vec<usize> {
        let v = self.pl(tok, PL_NEARBY, tr, 245.0, &[]);
        as_tok_list(&v)
    }
    fn pl_can_target(&mut self, tr: &mut Vec<f64>, tok: usize, other: Option<usize>) -> bool {
        let v = self.pl(tok, PL_CAN_TARGET, tr, 246.0, &evtok(other));
        as_bool(&v)
    }
    fn pl_team(&mut self, tr: &mut Vec<f64>, tok: usize) -> JsVal {
        self.pl(tok, PL_TEAM, tr, 248.0, &[])
    }
    fn pl_is_on_same_team(&mut self, tr: &mut Vec<f64>, tok: usize, other: Option<usize>) -> bool {
        let v = self.pl(tok, PL_IS_ON_SAME_TEAM, tr, 250.0, &evtok(other));
        as_bool(&v)
    }
    fn pl_type(&mut self, tr: &mut Vec<f64>, tok: usize) -> String {
        let v = self.pl(tok, PL_TYPE, tr, 251.0, &[]);
        as_str(&v)
    }
    fn pl_can_embargo_all(&mut self, tr: &mut Vec<f64>, tok: usize) -> bool {
        let v = self.pl(tok, PL_CAN_EMBARGO_ALL, tr, 252.0, &[]);
        as_bool(&v)
    }
    fn pl_is_player(&mut self, tr: &mut Vec<f64>, tok: usize) -> bool {
        let v = self.pl(tok, PL_IS_PLAYER, tr, 259.0, &[]);
        as_bool(&v)
    }
    fn pl_update_relation(&mut self, tr: &mut Vec<f64>, tok: usize, other: Option<usize>, amt: f64) {
        let mut extra = evtok(other);
        extra.extend_from_slice(&ev1(amt));
        self.pl_void(tok, tr, 237.0, &extra);
    }
    fn pl_end_temporary_embargo(&mut self, tr: &mut Vec<f64>, tok: usize, other: Option<usize>) {
        self.pl_void(tok, tr, 239.0, &evtok(other));
    }
    fn pl_break_alliance(&mut self, tr: &mut Vec<f64>, tok: usize, alliance: usize) {
        self.pl_void(tok, tr, 244.0, &ev1(alliance as f64));
    }
    fn pl_target(&mut self, tr: &mut Vec<f64>, tok: usize, other: Option<usize>) {
        self.pl_void(tok, tr, 247.0, &evtok(other));
    }
    fn pl_add_embargo(&mut self, tr: &mut Vec<f64>, tok: usize, other: Option<usize>, flag: bool) {
        let mut extra = evtok(other);
        extra.extend_from_slice(&evb(flag));
        self.pl_void(tok, tr, 253.0, &extra);
    }
    fn pl_stop_embargo(&mut self, tr: &mut Vec<f64>, tok: usize, other: Option<usize>) {
        self.pl_void(tok, tr, 254.0, &evtok(other));
    }
    fn pl_record_embargo_all(&mut self, tr: &mut Vec<f64>, tok: usize) {
        self.pl_void(tok, tr, 255.0, &[]);
    }
    fn pl_mark_disconnected(&mut self, tr: &mut Vec<f64>, tok: usize, flag: bool, obj: JsVal) {
        let mut extra = evb(flag);
        extra.extend_from_slice(&ev_enc(&obj));
        self.pl_void(tok, tr, 249.0, &extra);
    }
    fn pl_order_retreat(&mut self, tr: &mut Vec<f64>, tok: usize, attack_id: &str) {
        self.pl_void(tok, tr, 320.0, &evs(attack_id));
    }
    fn pl_execute_retreat(&mut self, tr: &mut Vec<f64>, tok: usize, attack_id: &str) {
        self.pl_void(tok, tr, 321.0, &evs(attack_id));
    }
    fn pl_can_send_quick_chat(&mut self, tr: &mut Vec<f64>, tok: usize, other: Option<usize>) -> bool {
        let v = self.pl(tok, PL_CAN_SEND_QUICK_CHAT, tr, 322.0, &evtok(other));
        as_bool(&v)
    }
    fn pl_record_quick_chat(&mut self, tr: &mut Vec<f64>, tok: usize, other: Option<usize>) {
        self.pl_void(tok, tr, 323.0, &evtok(other));
    }
    fn pl_can_delete_unit(&mut self, tr: &mut Vec<f64>, tok: usize) -> bool {
        let v = self.pl(tok, PL_CAN_DELETE_UNIT, tr, 324.0, &[]);
        as_bool(&v)
    }
    fn pl_record_delete_unit(&mut self, tr: &mut Vec<f64>, tok: usize) {
        self.pl_void(tok, tr, 325.0, &[]);
    }
    fn pl_can_upgrade_unit(&mut self, tr: &mut Vec<f64>, tok: usize, unit: usize) -> bool {
        let v = self.pl(tok, PL_CAN_UPGRADE_UNIT, tr, 326.0, &ev1(unit as f64));
        as_bool(&v)
    }
    fn pl_upgrade_unit(&mut self, tr: &mut Vec<f64>, tok: usize, unit: usize) {
        self.pl_void(tok, tr, 327.0, &ev1(unit as f64));
    }

    // alliance facade
    fn al(&mut self, aid: usize, slot: usize, tr: &mut Vec<f64>, code: f64, extra: &[f64]) -> JsVal {
        let mut prefix = vec![aid as f64];
        prefix.extend_from_slice(extra);
        mk_take(&mut self.alliances[aid], slot, tr, code, &prefix)
    }
    fn al_void(&mut self, aid: usize, tr: &mut Vec<f64>, code: f64, extra: &[f64]) {
        let mut prefix = vec![aid as f64];
        prefix.extend_from_slice(extra);
        mk_void(tr, code, &prefix);
    }
    fn al_only_one(&mut self, tr: &mut Vec<f64>, aid: usize) -> bool {
        let v = self.al(aid, AL_ONLY_ONE_AGREED, tr, 260.0, &[]);
        as_bool(&v)
    }
    fn al_add_extension_request(&mut self, tr: &mut Vec<f64>, aid: usize, tok: usize) {
        self.al_void(aid, tr, 261.0, &ev1(tok as f64));
    }
    fn al_both_agreed(&mut self, tr: &mut Vec<f64>, aid: usize) -> bool {
        let v = self.al(aid, AL_BOTH_AGREED, tr, 262.0, &[]);
        as_bool(&v)
    }
    fn al_extend(&mut self, tr: &mut Vec<f64>, aid: usize) {
        self.al_void(aid, tr, 263.0, &[]);
    }

    // alliance-request facade
    fn rq(&mut self, rid: usize, slot: usize, tr: &mut Vec<f64>, code: f64, extra: &[f64]) -> JsVal {
        let mut prefix = vec![rid as f64];
        prefix.extend_from_slice(extra);
        mk_take(&mut self.requests[rid], slot, tr, code, &prefix)
    }
    fn rq_void(&mut self, rid: usize, tr: &mut Vec<f64>, code: f64) {
        mk_void(tr, code, &[rid as f64]);
    }
    fn rq_recipient(&mut self, tr: &mut Vec<f64>, rid: usize) -> Option<usize> {
        let v = self.rq(rid, RQ_RECIPIENT, tr, 270.0, &[]);
        as_opt_tok(&v)
    }
    fn rq_accept(&mut self, tr: &mut Vec<f64>, rid: usize) {
        self.rq_void(rid, tr, 271.0);
    }
    fn rq_reject(&mut self, tr: &mut Vec<f64>, rid: usize) {
        self.rq_void(rid, tr, 272.0);
    }
    fn rq_status(&mut self, tr: &mut Vec<f64>, rid: usize) -> String {
        let v = self.rq(rid, RQ_STATUS, tr, 273.0, &[]);
        as_str(&v)
    }
    fn rq_created_at(&mut self, tr: &mut Vec<f64>, rid: usize) -> f64 {
        let v = self.rq(rid, RQ_CREATED_AT, tr, 274.0, &[]);
        as_num(&v)
    }

    // unit facade
    fn un(&mut self, uid: usize, slot: usize, tr: &mut Vec<f64>, code: f64, extra: &[f64]) -> JsVal {
        let mut prefix = vec![uid as f64];
        prefix.extend_from_slice(extra);
        mk_take(&mut self.units[uid], slot, tr, code, &prefix)
    }
    fn un_is_active(&mut self, tr: &mut Vec<f64>, uid: usize) -> bool {
        let v = self.un(uid, UN_IS_ACTIVE, tr, 280.0, &[]);
        as_bool(&v)
    }
    fn un_reached_target(&mut self, tr: &mut Vec<f64>, uid: usize) -> bool {
        let v = self.un(uid, UN_REACHED_TARGET, tr, 281.0, &[]);
        as_bool(&v)
    }
    fn un_target_tile(&mut self, tr: &mut Vec<f64>, uid: usize) -> Option<f64> {
        let v = self.un(uid, UN_TARGET_TILE, tr, 282.0, &[]);
        if let JsVal::Num(n) = v {
            Some(n)
        } else {
            None
        }
    }
    fn un_type(&mut self, tr: &mut Vec<f64>, uid: usize) -> String {
        let v = self.un(uid, UN_TYPE, tr, 283.0, &[]);
        as_str(&v)
    }
    fn un_target_player(&mut self, tr: &mut Vec<f64>, uid: usize) -> Option<usize> {
        let v = self.un(uid, UN_TARGET_PLAYER, tr, 284.0, &[]);
        as_opt_tok(&v)
    }
    fn un_delete(&mut self, tr: &mut Vec<f64>, uid: usize, flag: bool) {
        let mut prefix = vec![uid as f64];
        prefix.extend_from_slice(&evb(flag));
        mk_void(tr, 285.0, &prefix);
    }
    fn un_owner(&mut self, tr: &mut Vec<f64>, uid: usize) -> Option<usize> {
        let v = self.un(uid, UN_OWNER, tr, 286.0, &[]);
        as_opt_tok(&v)
    }
    fn un_id(&mut self, tr: &mut Vec<f64>, uid: usize) -> f64 {
        let v = self.un(uid, UN_ID, tr, 340.0, &[]);
        as_num(&v)
    }
    fn un_update_transport_ship_state(&mut self, tr: &mut Vec<f64>, uid: usize) {
        let obj = JsVal::Obj(vec![("isRetreating".to_string(), JsVal::Bool(true))]);
        let mut prefix = vec![uid as f64];
        prefix.extend_from_slice(&ev_enc(&obj));
        mk_void(tr, 341.0, &prefix);
    }
    fn un_update_warship_state(&mut self, tr: &mut Vec<f64>, uid: usize, position: f64) {
        let obj = JsVal::Obj(vec![("patrolTile".to_string(), JsVal::Num(position))]);
        let mut prefix = vec![uid as f64];
        prefix.extend_from_slice(&ev_enc(&obj));
        mk_void(tr, 342.0, &prefix);
    }
    fn un_set_target_tile(&mut self, tr: &mut Vec<f64>, uid: usize) {
        let mut prefix = vec![uid as f64];
        prefix.extend_from_slice(&ev_enc(&JsVal::Undef));
        mk_void(tr, 343.0, &prefix);
    }
    fn un_tile(&mut self, tr: &mut Vec<f64>, uid: usize) -> f64 {
        let v = self.un(uid, UN_TILE, tr, 344.0, &[]);
        as_num(&v)
    }
    fn un_is_under_construction(&mut self, tr: &mut Vec<f64>, uid: usize) -> bool {
        let v = self.un(uid, UN_IS_UNDER_CONSTRUCTION, tr, 345.0, &[]);
        as_bool(&v)
    }
    fn un_missile_timer_queue(&mut self, tr: &mut Vec<f64>, uid: usize) -> JsVal {
        self.un(uid, UN_MISSILE_TIMER_QUEUE, tr, 346.0, &[])
    }
    fn un_reload_missile(&mut self, tr: &mut Vec<f64>, uid: usize) {
        let prefix = vec![uid as f64];
        mk_void(tr, 347.0, &prefix);
    }
    fn un_mark_for_deletion(&mut self, tr: &mut Vec<f64>, uid: usize) {
        let prefix = vec![uid as f64];
        mk_void(tr, 348.0, &prefix);
    }
    fn un_is_overdue_deletion(&mut self, tr: &mut Vec<f64>, uid: usize) -> bool {
        let v = self.un(uid, UN_IS_OVERDUE_DELETION, tr, 349.0, &[]);
        as_bool(&v)
    }
    fn un_has_train_station(&mut self, tr: &mut Vec<f64>, uid: usize) -> bool {
        let v = self.un(uid, UN_HAS_TRAIN_STATION, tr, 350.0, &[]);
        as_bool(&v)
    }
    fn un_set_train_station(&mut self, tr: &mut Vec<f64>, uid: usize, flag: bool) {
        let mut prefix = vec![uid as f64];
        prefix.extend_from_slice(&evb(flag));
        mk_void(tr, 351.0, &prefix);
    }
    #[allow(dead_code)]
    fn un_has_health(&mut self, tr: &mut Vec<f64>, uid: usize) -> bool {
        let v = self.un(uid, UN_HAS_HEALTH, tr, 352.0, &[]);
        as_bool(&v)
    }

    // neighbor facade
    fn nb(&mut self, nid: usize, slot: usize, tr: &mut Vec<f64>, code: f64, extra: &[f64]) -> JsVal {
        let mut prefix = vec![nid as f64];
        prefix.extend_from_slice(extra);
        mk_take(&mut self.neighbors[nid], slot, tr, code, &prefix)
    }
    fn nb_is_player(&mut self, tr: &mut Vec<f64>, nid: usize) -> bool {
        let v = self.nb(nid, NB_IS_PLAYER, tr, 257.0, &[]);
        as_bool(&v)
    }
    fn nb_is_on_same_team(&mut self, tr: &mut Vec<f64>, nid: usize, other: Option<usize>) -> bool {
        let v = self.nb(nid, NB_IS_ON_SAME_TEAM, tr, 258.0, &evtok(other));
        as_bool(&v)
    }
    fn nb_update_relation(&mut self, tr: &mut Vec<f64>, nid: usize, other: Option<usize>, amt: f64) {
        let mut prefix = vec![nid as f64];
        prefix.extend_from_slice(&evtok(other));
        prefix.extend_from_slice(&ev1(amt));
        mk_void(tr, 256.0, &prefix);
    }

    fn rail_recompute(&mut self, tr: &mut Vec<f64>) {
        mk_void(tr, 290.0, &[]);
    }

    // ---- wouldNukeBreakAlliance (Util.ts:51-95) ----

    fn would_nuke_break_alliance(
        &mut self,
        tr: &mut Vec<f64>,
        target_tile: f64,
        inner: f64,
        outer: f64,
        ally_small_ids: &[f64],
        threshold: f64,
    ) -> bool {
        if ally_small_ids.is_empty() {
            return false;
        }
        let types: Vec<String> = STRUCTURES_TYPES.iter().map(|s| s.to_string()).collect();
        let ids = ally_small_ids.to_vec();
        let hit = self.mg_any_unit_nearby(tr, target_tile, outer, &types, |h, tr, uid| {
            let is_player = match h.un_owner(tr, uid) {
                Some(o) => h.pl_is_player(tr, o),
                None => false,
            };
            if !is_player {
                return false;
            }
            let small = match h.un_owner(tr, uid) {
                Some(o) => h.pl_small_id(tr, o),
                None => f64::NAN,
            };
            ids.contains(&small)
        });
        if hit {
            return true;
        }

        let inner2 = inner * inner;
        let mut result = false;
        let mut counts: Vec<(f64, f64)> = Vec::new();
        self.mg_circle_search(tr, target_tile, outer, |h, tr, tile, d2| {
            let owner_small = h.mg_owner_id(tr, tile);
            if owner_small > 0.0 && ids.contains(&owner_small) {
                let weight = if d2 <= inner2 { 1.0 } else { 0.5 };
                let new_count = match counts.iter_mut().find(|(id, _)| *id == owner_small) {
                    Some(e) => {
                        e.1 += weight;
                        e.1
                    }
                    None => {
                        counts.push((owner_small, weight));
                        weight
                    }
                };
                if new_count > threshold {
                    result = true;
                    return false;
                }
            }
            true
        });
        result
    }

    // ---- cancelNukesBetweenAlliedPlayers (AllianceRequestExecution.ts:89-171) ----

    fn cancel_nukes(&mut self, tr: &mut Vec<f64>, recipient: usize) -> Result<(), String> {
        let requestor = self.a_tok;
        // insertion-ordered Map<Player-token, count> and Set<Player-token>.
        let mut neutralized: Vec<(usize, f64)> = Vec::new();
        let mut cancelled: Vec<usize> = Vec::new();

        let players = [requestor, recipient];
        let nuke_types: Vec<String> = ["Atom Bomb", "Hydrogen Bomb", "MIRV", "MIRV Warhead"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        for launcher in players {
            let other = if launcher == requestor {
                recipient
            } else {
                requestor
            };
            let units = self.pl_units(tr, launcher, &nuke_types);
            for uid in units {
                if !self.un_is_active(tr, uid) || self.un_reached_target(tr, uid) {
                    continue;
                }
                let target_tile = self.un_target_tile(tr, uid);

                if self.un_type(tr, uid) == UNIT_MIRV {
                    let tp = self.un_target_player(tr, uid);
                    let target = match tp {
                        Some(t) => Some(t),
                        None => {
                            if truthy_tile(target_tile) {
                                self.mg_owner(tr, target_tile.unwrap())
                            } else {
                                None
                            }
                        }
                    };
                    if target != Some(other) {
                        continue;
                    }
                } else if self.un_type(tr, uid) == UNIT_MIRV_WARHEAD {
                    if !truthy_tile(target_tile) {
                        continue;
                    }
                    if self.mg_owner(tr, target_tile.unwrap()) != Some(other) {
                        continue;
                    }
                } else {
                    if !truthy_tile(target_tile) {
                        continue;
                    }
                    let ty = self.un_type(tr, uid);
                    let (inner, outer) = self.mg_nuke_magnitudes(tr, &ty);
                    let small = self.pl_small_id(tr, other);
                    let threshold = self.mg_nuke_alliance_break_threshold(tr);
                    if !self.would_nuke_break_alliance(
                        tr,
                        target_tile.unwrap(),
                        inner,
                        outer,
                        &[small],
                        threshold,
                    ) {
                        continue;
                    }
                }

                self.un_delete(tr, uid, false);
                if self.un_type(tr, uid) == UNIT_MIRV_WARHEAD {
                    if !cancelled.contains(&launcher) {
                        cancelled.push(launcher);
                    }
                } else {
                    match neutralized.iter_mut().find(|(l, _)| *l == launcher) {
                        Some(e) => e.1 += 1.0,
                        None => neutralized.push((launcher, 1.0)),
                    }
                }
            }
        }

        for launcher in cancelled {
            match neutralized.iter_mut().find(|(l, _)| *l == launcher) {
                Some(e) => e.1 += 1.0,
                None => neutralized.push((launcher, 1.0)),
            }
        }

        for &(launcher, count) in &neutralized {
            let other = if launcher == requestor {
                recipient
            } else {
                requestor
            };
            let l_id = self.pl_id(tr, launcher);
            let o_name = self.pl_display_name(tr, other);
            let o_id = self.pl_id(tr, other);
            self.mg_display_message(
                tr,
                &[
                    JsVal::Str("events_display.alliance_nukes_destroyed_outgoing".into()),
                    JsVal::Num(MSG_ALLIANCE_ACCEPTED),
                    JsVal::Str(l_id),
                    JsVal::Undef,
                    JsVal::Obj(vec![
                        ("name".into(), JsVal::Str(o_name)),
                        ("count".into(), JsVal::Num(count)),
                    ]),
                    JsVal::Undef,
                    JsVal::Str(o_id),
                ],
            );

            let o_id2 = self.pl_id(tr, other);
            let l_name = self.pl_display_name(tr, launcher);
            let l_id2 = self.pl_id(tr, launcher);
            self.mg_display_message(
                tr,
                &[
                    JsVal::Str("events_display.alliance_nukes_destroyed_incoming".into()),
                    JsVal::Num(MSG_ALLIANCE_ACCEPTED),
                    JsVal::Str(o_id2),
                    JsVal::Undef,
                    JsVal::Obj(vec![
                        ("name".into(), JsVal::Str(l_name)),
                        ("count".into(), JsVal::Num(count)),
                    ]),
                    JsVal::Undef,
                    JsVal::Str(l_id2),
                ],
            );
        }
        Ok(())
    }

    // ---- construct ----

    fn construct(&mut self, args: &[f64]) {
        let mut i = 0;
        let v = read_val(args, &mut i);
        self.class_id = val_field(&v, "c").map(as_num).unwrap_or(0.0) as u8;
        let ctor = as_arr(val_field(&v, "a").unwrap_or(&JsVal::Absent));

        self.mg = val_field(&v, "mg").map(build_mock).unwrap_or_default();
        self.players = val_field(&v, "players")
            .map(as_arr)
            .unwrap_or_default()
            .iter()
            .map(build_mock)
            .collect();
        self.alliances = val_field(&v, "alliances")
            .map(as_arr)
            .unwrap_or_default()
            .iter()
            .map(build_mock)
            .collect();
        self.requests = val_field(&v, "requests")
            .map(as_arr)
            .unwrap_or_default()
            .iter()
            .map(build_mock)
            .collect();
        self.units = val_field(&v, "units")
            .map(as_arr)
            .unwrap_or_default()
            .iter()
            .map(build_mock)
            .collect();
        self.neighbors = val_field(&v, "neighbors")
            .map(as_arr)
            .unwrap_or_default()
            .iter()
            .map(build_mock)
            .collect();

        let n = |k: usize| -> f64 { ctor.get(k).map(as_num).unwrap_or(f64::NAN) };
        match self.class_id {
            1 | 4 => {
                self.a_tok = n(0) as usize;
                self.flag = ctor.get(1).map(as_bool).unwrap_or(false);
            }
            5 => {
                self.a_tok = n(0) as usize;
                self.id_a = ctor.get(1).map(as_str).unwrap_or_default();
                self.action = ctor.get(2).map(as_str).unwrap_or_default();
                // EmbargoExecution: `private active = true`.
                self.flag = true;
            }
            6 => {
                self.a_tok = n(0) as usize;
                self.action = ctor.get(1).map(as_str).unwrap_or_default();
            }
            7 | 8 | 10 => {
                self.a_tok = n(0) as usize;
                self.id_a = ctor.get(1).map(as_str).unwrap_or_default();
                // TargetPlayer / AllianceRequest / BreakAlliance: `active = true`.
                self.flag = true;
            }
            9 => {
                self.a_tok = n(0) as usize;
                self.id_a = ctor.get(1).map(as_str).unwrap_or_default();
            }
            11 => {
                self.id_a = ctor.first().map(as_str).unwrap_or_default();
                self.b_tok = n(1) as usize;
                // AllianceRejectExecution: `private active = true`.
                self.flag = true;
            }
            12 => {
                self.a_tok = n(0) as usize;
                self.id_a = ctor.get(1).map(as_str).unwrap_or_default();
                // RetreatExecution: `private active = true`.
                self.flag = true;
            }
            13 => {
                self.a_tok = n(0) as usize;
                self.unit_id = n(1);
                // BoatRetreatExecution: `private active = true`.
                self.flag = true;
            }
            14 => {
                self.a_tok = n(0) as usize;
                self.unit_ids = ctor.get(1).map(as_num_list).unwrap_or_default();
                self.position = n(2);
            }
            15 => {
                self.a_tok = n(0) as usize;
                self.unit_id = n(1);
                // DeleteUnitExecution: `private active = true`.
                self.flag = true;
            }
            16 => {
                self.a_tok = n(0) as usize;
                self.id_a = ctor.get(1).map(as_str).unwrap_or_default();
                self.quick_key = ctor.get(2).map(as_str).unwrap_or_default();
                self.chat_target = ctor.get(3).and_then(as_str_opt);
                // QuickChatExecution: `private active = true`.
                self.flag = true;
            }
            17 => {
                self.a_tok = n(0) as usize;
                // MissileSiloExecution: `private active = true`.
                self.flag = true;
            }
            18 => {
                self.a_tok = n(0) as usize;
                self.unit_id = n(1);
                self.amount = match ctor.get(2) {
                    Some(JsVal::Num(m)) => *m,
                    _ => 1.0,
                };
            }
            19..=21 => {
                self.a_tok = n(0) as usize;
                // City / Factory / DefensePost: `private active = true`.
                self.flag = true;
            }
            _ => {}
        }
    }

    // ---- the op dispatcher ----

    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        if kind == 0 {
            self.reset();
            self.construct(args);
            return vec![0.0, 0.0];
        }
        let ticks = if args.is_empty() { 0.0 } else { args[0] };
        let mut tr: Vec<f64> = Vec::new();
        let res = self.dispatch(kind, ticks, &mut tr);
        let mut out: Vec<f64> = Vec::new();
        match res {
            Ok(v) => {
                out.push(0.0);
                push_val(&mut out, &v);
            }
            Err(msg) => {
                out.push(1.0);
                push_str(&mut out, &msg);
            }
        }
        let mut full = Vec::with_capacity(1 + tr.len() + out.len());
        full.push(tr.len() as f64);
        full.extend_from_slice(&tr);
        full.extend_from_slice(&out);
        full
    }

    fn dispatch(&mut self, kind: u8, ticks: f64, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match self.class_id {
            0 => Ok(self.noop(kind)),
            1 => self.pause(kind, tr),
            2 => self.spawn_timer(kind, tr),
            3 => self.recompute_rail_cluster(kind, tr),
            4 => self.mark_disconnected(kind, ticks, tr),
            5 => self.embargo(kind, tr),
            6 => self.embargo_all(kind, tr),
            7 => self.target_player(kind, tr),
            8 => self.alliance_request(kind, tr),
            9 => self.alliance_extension(kind, tr),
            10 => self.break_alliance(kind, tr),
            11 => self.alliance_reject(kind, tr),
            12 => self.retreat(kind, tr),
            13 => self.boat_retreat(kind, tr),
            14 => self.move_warship(kind, tr),
            15 => self.delete_unit(kind, tr),
            16 => self.quick_chat(kind, tr),
            17 => self.missile_silo(kind, tr),
            18 => self.upgrade_structure(kind, tr),
            19 => self.city(kind, tr),
            20 => self.factory(kind, tr),
            21 => self.defense_post(kind, tr),
            k => unreachable!("exec harness: unknown class id {k}"),
        }
    }

    fn noop(&mut self, kind: u8) -> JsVal {
        match kind {
            3 | 4 => JsVal::Bool(false),
            _ => JsVal::Undef,
        }
    }

    // 1 Pause
    fn pause(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(false)),
            4 => Ok(JsVal::Bool(true)),
            1 => {
                let creator = self.pl_is_lobby_creator(tr, self.a_tok);
                if creator || self.mg_game_type(tr) == GAME_TYPE_SINGLEPLAYER {
                    self.mg_set_paused(tr, self.flag);
                }
                Ok(JsVal::Undef)
            }
            2 => Ok(JsVal::Undef),
            _ => unreachable!(),
        }
    }

    // 2 SpawnTimer
    fn spawn_timer(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => {
                if !self.mg_ready {
                    return Err("Cannot read properties of undefined (reading 'inSpawnPhase')".into());
                }
                Ok(JsVal::Bool(self.mg_in_spawn_phase(tr)))
            }
            4 => Ok(JsVal::Bool(true)),
            1 => {
                self.mg_ready = true;
                Ok(JsVal::Undef)
            }
            2 => {
                if !self.mg_ready {
                    return Err("Cannot read properties of undefined (reading 'ticks')".into());
                }
                let t = self.mg_ticks(tr);
                if t > self.mg_num_spawn_phase_turns(tr) {
                    self.mg_end_spawn_phase(tr);
                }
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    // 3 RecomputeRailCluster
    fn recompute_rail_cluster(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(true)),
            4 => Ok(JsVal::Bool(false)),
            2 => {
                self.rail_recompute(tr);
                Ok(JsVal::Undef)
            }
            _ => Ok(JsVal::Undef),
        }
    }

    // 4 MarkDisconnected
    fn mark_disconnected(&mut self, kind: u8, ticks: f64, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 | 4 => Ok(JsVal::Bool(false)),
            1 => {
                if self.flag {
                    let team_v = self.pl_team(tr, self.a_tok);
                    let team = as_str_opt(&team_v);
                    let team_tiles = match &team {
                        Some(s) if !s.is_empty() => self.mg_team_tiles_owned(tr, Some(s)),
                        _ => 0.0,
                    };
                    let total_land = self.mg_total_land_tiles(tr);
                    let alive = self.pl_is_alive(tr, self.a_tok);
                    let obj = JsVal::Obj(vec![
                        ("currentTick".into(), JsVal::Num(ticks)),
                        ("teamTiles".into(), JsVal::Num(team_tiles)),
                        ("totalLand".into(), JsVal::Num(total_land)),
                        ("wasAlive".into(), JsVal::Bool(alive)),
                    ]);
                    self.pl_mark_disconnected(tr, self.a_tok, true, obj);
                } else {
                    self.pl_mark_disconnected(tr, self.a_tok, false, JsVal::Undef);
                }
                Ok(JsVal::Undef)
            }
            _ => Ok(JsVal::Undef),
        }
    }

    // 5 Embargo
    fn embargo(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => {
                let id_a = self.id_a.clone();
                if !self.mg_has_player(tr, &id_a) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                self.target = self.mg_player(tr, &id_a);
                Ok(JsVal::Undef)
            }
            2 => {
                if self.action == "start" {
                    self.pl_add_embargo(tr, self.a_tok, self.target, false);
                } else {
                    self.pl_stop_embargo(tr, self.a_tok, self.target);
                }
                self.flag = false;
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    // 6 EmbargoAll
    fn embargo_all(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 | 4 => Ok(JsVal::Bool(false)),
            1 => {
                if !self.pl_can_embargo_all(tr, self.a_tok) {
                    return Ok(JsVal::Undef);
                }
                let players = self.mg_players(tr);
                for p in players {
                    let pid = self.pl_id(tr, p);
                    let me_id = self.pl_id(tr, self.a_tok);
                    if pid == me_id {
                        continue;
                    }
                    if self.pl_type(tr, p) == PLAYER_TYPE_BOT {
                        continue;
                    }
                    if self.pl_is_on_same_team(tr, self.a_tok, Some(p)) {
                        continue;
                    }
                    if self.action == "start" {
                        if !self.pl_has_embargo_against(tr, self.a_tok, Some(p)) {
                            self.pl_add_embargo(tr, self.a_tok, Some(p), false);
                        }
                    } else {
                        if self.pl_has_embargo_against(tr, self.a_tok, Some(p)) {
                            self.pl_stop_embargo(tr, self.a_tok, Some(p));
                        }
                    }
                }
                self.pl_record_embargo_all(tr, self.a_tok);
                Ok(JsVal::Undef)
            }
            2 => Ok(JsVal::Undef),
            _ => unreachable!(),
        }
    }

    // 7 TargetPlayer
    fn target_player(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => {
                let id_a = self.id_a.clone();
                if !self.mg_has_player(tr, &id_a) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                self.target = self.mg_player(tr, &id_a);
                Ok(JsVal::Undef)
            }
            2 => {
                if self.pl_can_target(tr, self.a_tok, self.target) {
                    self.pl_target(tr, self.a_tok, self.target);
                    if let Some(t) = self.target {
                        self.pl_update_relation(tr, t, Some(self.a_tok), -40.0);
                    }
                }
                self.flag = false;
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    // 8 AllianceRequest
    fn alliance_request(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => {
                self.mg_ready = true;
                let id_a = self.id_a.clone();
                if !self.mg_has_player(tr, &id_a) {
                    return Ok(JsVal::Undef);
                }
                let recipient = self.mg_player(tr, &id_a).unwrap_or(0);
                if !self.pl_can_send_alliance_request(tr, self.a_tok, Some(recipient)) {
                    self.flag = false;
                } else {
                    let reqs = self.pl_outgoing_alliance_requests(tr, recipient);
                    let mut incoming = None;
                    for rid in reqs {
                        if self.rq_recipient(tr, rid) == Some(self.a_tok) {
                            incoming = Some(rid);
                            break;
                        }
                    }
                    if let Some(rid) = incoming {
                        self.flag = false;
                        self.rq_accept(tr, rid);
                        self.pl_update_relation(tr, self.a_tok, Some(recipient), 100.0);
                        self.pl_update_relation(tr, recipient, Some(self.a_tok), 100.0);
                        if self.pl_has_embargo_against(tr, self.a_tok, Some(recipient)) {
                            self.pl_end_temporary_embargo(tr, self.a_tok, Some(recipient));
                        }
                        if self.pl_has_embargo_against(tr, recipient, Some(self.a_tok)) {
                            self.pl_end_temporary_embargo(tr, recipient, Some(self.a_tok));
                        }
                        self.cancel_nukes(tr, recipient)?;
                    } else {
                        self.req = self.pl_create_alliance_request(tr, self.a_tok, Some(recipient));
                    }
                }
                Ok(JsVal::Undef)
            }
            2 => {
                if !self.mg_ready {
                    return Err("Cannot read properties of undefined (reading 'ticks')".into());
                }
                let req = self.req;
                let st1 = req.map(|r| self.rq_status(tr, r));
                if st1.as_deref() == Some("accepted") {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                let st2 = req.map(|r| self.rq_status(tr, r));
                if st2.as_deref() == Some("rejected") {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                let ticks_now = self.mg_ticks(tr);
                let created = match self.req {
                    Some(r) => self.rq_created_at(tr, r),
                    None => 0.0,
                };
                if ticks_now - created > self.mg_alliance_request_duration(tr) {
                    if let Some(r) = self.req {
                        self.rq_reject(tr, r);
                    }
                    self.flag = false;
                }
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    // 9 AllianceExtension
    fn alliance_extension(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 | 4 => Ok(JsVal::Bool(false)),
            1 => {
                let from = self.a_tok;
                let id_a = self.id_a.clone();
                if !self.mg_has_player(tr, &id_a) {
                    return Ok(JsVal::Undef);
                }
                let to = self.mg_player(tr, &id_a).unwrap_or(0);
                if !self.pl_is_alive(tr, from) || !self.pl_is_alive(tr, to) {
                    // `console.info` reads `this.from.id()`.
                    let _ = self.pl_id(tr, from);
                    return Ok(JsVal::Undef);
                }
                let alliance = self.pl_alliance_with(tr, from, Some(to));
                let aid = match alliance {
                    Some(a) => a,
                    None => {
                        // `console.warn` reads `this.from.id()`.
                        let _ = self.pl_id(tr, from);
                        return Ok(JsVal::Undef);
                    }
                };
                let was_only_one = self.al_only_one(tr, aid);
                self.al_add_extension_request(tr, aid, from);
                if self.al_both_agreed(tr, aid) {
                    self.al_extend(tr, aid);
                    let f_id = self.pl_id(tr, from);
                    let t_name = self.pl_display_name(tr, to);
                    let t_id = self.pl_id(tr, to);
                    self.mg_display_message(
                        tr,
                        &[
                            JsVal::Str("events_display.alliance_renewed".into()),
                            JsVal::Num(MSG_ALLIANCE_ACCEPTED),
                            JsVal::Str(f_id),
                            JsVal::Undef,
                            JsVal::Obj(vec![("name".into(), JsVal::Str(t_name))]),
                            JsVal::Undef,
                            JsVal::Str(t_id),
                        ],
                    );
                    let f_name = self.pl_display_name(tr, from);
                    let f_id2 = self.pl_id(tr, from);
                    self.mg_display_message(
                        tr,
                        &[
                            JsVal::Str("events_display.alliance_renewed".into()),
                            JsVal::Num(MSG_ALLIANCE_ACCEPTED),
                            JsVal::Str(self.id_a.clone()),
                            JsVal::Undef,
                            JsVal::Obj(vec![("name".into(), JsVal::Str(f_name))]),
                            JsVal::Undef,
                            JsVal::Str(f_id2),
                        ],
                    );
                } else if self.al_only_one(tr, aid) && !was_only_one {
                    let f_name = self.pl_display_name(tr, from);
                    let f_id = self.pl_id(tr, from);
                    self.mg_display_message(
                        tr,
                        &[
                            JsVal::Str("events_display.wants_to_renew_alliance".into()),
                            JsVal::Num(MSG_RENEW_ALLIANCE),
                            JsVal::Str(self.id_a.clone()),
                            JsVal::Undef,
                            JsVal::Obj(vec![("name".into(), JsVal::Str(f_name))]),
                            JsVal::Undef,
                            JsVal::Str(f_id),
                        ],
                    );
                }
                Ok(JsVal::Undef)
            }
            2 => Ok(JsVal::Undef),
            _ => unreachable!(),
        }
    }

    // 10 BreakAlliance
    fn break_alliance(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => {
                let id_a = self.id_a.clone();
                if !self.mg_has_player(tr, &id_a) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                self.target = self.mg_player(tr, &id_a);
                self.mg_ready = true;
                Ok(JsVal::Undef)
            }
            2 => {
                if !self.mg_ready {
                    return Err("Not initialized".into());
                }
                let recipient = self.target.unwrap_or(0);
                let alliance = self.pl_alliance_with(tr, self.a_tok, Some(recipient));
                match alliance {
                    None => { /* warn: cant break alliance, not allied */ }
                    Some(al) => {
                        self.pl_break_alliance(tr, self.a_tok, al);
                        self.pl_update_relation(tr, recipient, Some(self.a_tok), -100.0);
                        let nbrs = self.pl_nearby(tr, self.a_tok);
                        let mut filtered = Vec::new();
                        for nid in nbrs {
                            if self.nb_is_player(tr, nid)
                                && !self.nb_is_on_same_team(tr, nid, Some(recipient))
                            {
                                filtered.push(nid);
                            }
                        }
                        for nid in filtered {
                            self.nb_update_relation(tr, nid, Some(self.a_tok), -40.0);
                        }
                    }
                }
                self.flag = false;
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    // 11 AllianceReject
    fn alliance_reject(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => {
                let id_a = self.id_a.clone();
                if !self.mg_has_player(tr, &id_a) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                let requestor = self.mg_player(tr, &id_a).unwrap_or(0);
                if self.pl_is_friendly(tr, requestor, Some(self.b_tok)) {
                    // warn: already allied (the TS `this.recipient.id` has no
                    // call parens, so it does not touch the facade).
                } else {
                    let reqs = self.pl_outgoing_alliance_requests(tr, requestor);
                    let mut found = None;
                    for rid in reqs {
                        if self.rq_recipient(tr, rid) == Some(self.b_tok) {
                            found = Some(rid);
                            break;
                        }
                    }
                    match found {
                        None => { /* warn: no alliance request found */ }
                        Some(rid) => self.rq_reject(tr, rid),
                    }
                }
                self.flag = false;
                Ok(JsVal::Undef)
            }
            2 => Ok(JsVal::Undef),
            _ => unreachable!(),
        }
    }

    // 12 Retreat
    fn retreat(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => {
                self.mg_ready = true;
                self.start_tick = self.mg_ticks(tr);
                Ok(JsVal::Undef)
            }
            2 => {
                if !self.retreat_ordered {
                    let id_a = self.id_a.clone();
                    self.pl_order_retreat(tr, self.a_tok, &id_a);
                    self.retreat_ordered = true;
                }
                if !self.mg_ready {
                    return Err("Cannot read properties of undefined (reading 'ticks')".into());
                }
                let t = self.mg_ticks(tr);
                if t >= self.start_tick + RETREAT_CANCEL_DELAY {
                    let id_a = self.id_a.clone();
                    self.pl_execute_retreat(tr, self.a_tok, &id_a);
                    self.flag = false;
                }
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    // 13 BoatRetreat
    fn boat_retreat(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => Ok(JsVal::Undef),
            2 => {
                let units = self.pl_units(tr, self.a_tok, &[]);
                let mut found = None;
                for uid in units {
                    let id = self.un_id(tr, uid);
                    if id == self.unit_id && self.un_type(tr, uid) == UNIT_TRANSPORT {
                        found = Some(uid);
                        break;
                    }
                }
                match found {
                    None => { /* warn: outgoing boat not found */ }
                    Some(uid) => self.un_update_transport_ship_state(tr, uid),
                }
                self.flag = false;
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    // 14 MoveWarship
    fn move_warship(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 | 4 => Ok(JsVal::Bool(false)),
            2 => Ok(JsVal::Undef),
            1 => {
                if !self.mg_is_valid_ref(tr, self.position) {
                    /* warn: position not valid */
                    return Ok(JsVal::Undef);
                }
                let comp = self.mg_get_water_component(tr, self.position);
                let warship_ty = vec![UNIT_WARSHIP.to_string()];
                let warships = self.pl_units(tr, self.a_tok, &warship_ty);
                // `new Map(units.map(u => [u.id(), u]))` — SameValueZero keys,
                // overwrite in place keeps the first insertion position.
                let mut map: Vec<(f64, usize)> = Vec::new();
                for uid in warships {
                    let id = self.un_id(tr, uid);
                    match map.iter_mut().find(|(k, _)| svz(*k, id)) {
                        Some(e) => e.1 = uid,
                        None => map.push((id, uid)),
                    }
                }
                // `for (const unitId of new Set(this.unitIds))`.
                let mut dedup: Vec<f64> = Vec::new();
                for id in self.unit_ids.clone() {
                    if !dedup.iter().any(|x| svz(*x, id)) {
                        dedup.push(id);
                    }
                }
                for id in dedup {
                    let uid = match map.iter().find(|(k, _)| svz(*k, id)) {
                        Some((_, t)) => *t,
                        None => continue,
                    };
                    if !self.un_is_active(tr, uid) {
                        continue;
                    }
                    let tile = self.un_tile(tr, uid);
                    if !self.mg_has_water_component(tr, tile, &comp) {
                        continue;
                    }
                    self.un_update_warship_state(tr, uid, self.position);
                    self.un_set_target_tile(tr, uid);
                }
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    // 15 DeleteUnit
    fn delete_unit(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            2 => {
                if !self.flag || self.unit.is_none() {
                    return Ok(JsVal::Undef);
                }
                let ui = self.unit.unwrap();
                if !self.un_is_active(tr, ui) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                if self.un_is_overdue_deletion(tr, ui) {
                    self.un_delete(tr, ui, false);
                    let pid = self.pl_id(tr, self.a_tok);
                    self.mg_display_message(
                        tr,
                        &[
                            JsVal::Str("events_display.unit_voluntarily_deleted".into()),
                            JsVal::Num(MSG_UNIT_DESTROYED),
                            JsVal::Str(pid),
                        ],
                    );
                    self.flag = false;
                }
                Ok(JsVal::Undef)
            }
            1 => {
                if !self.flag {
                    return Ok(JsVal::Undef);
                }
                self.mg_ready = true;
                let unit = self.mg_unit(tr, self.unit_id);
                let bad = match unit {
                    None => true,
                    Some(u) => self.un_owner(tr, u) != Some(self.a_tok),
                };
                if bad {
                    // warn reads `this.player.displayName()`.
                    let _ = self.pl_display_name(tr, self.a_tok);
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                let ui = unit.unwrap();
                if !self.un_is_active(tr, ui) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                self.unit = Some(ui);
                let tile = self.un_tile(tr, ui);
                let tile_owner = match self.mg_owner(tr, tile) {
                    Some(o) => o,
                    None => {
                        return Err("Cannot read properties of null (reading 'isPlayer')".into());
                    }
                };
                let bad2 = if !self.pl_is_player(tr, tile_owner) {
                    true
                } else {
                    self.pl_id(tr, tile_owner) != self.pl_id(tr, self.a_tok)
                };
                if bad2 {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                let tile2 = self.un_tile(tr, ui);
                if !self.mg_is_land(tr, tile2) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                if self.mg_in_spawn_phase(tr) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                if !self.pl_can_delete_unit(tr, self.a_tok) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                self.pl_record_delete_unit(tr, self.a_tok);
                self.un_mark_for_deletion(tr, ui);
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    // 16 QuickChat
    fn quick_chat(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => {
                self.mg_ready = true;
                let id_a = self.id_a.clone();
                if !self.mg_has_player(tr, &id_a) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                self.recipient = self.mg_player(tr, &id_a);
                Ok(JsVal::Undef)
            }
            2 => {
                if !self.pl_can_send_quick_chat(tr, self.a_tok, self.recipient) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                let parts = split_dot(&self.quick_key);
                let m0 = parts.first().cloned();
                let m1 = parts.get(1).cloned();
                self.pl_record_quick_chat(tr, self.a_tok, self.recipient);
                if !self.mg_ready {
                    return Err("Cannot read properties of undefined (reading 'displayChat')".into());
                }
                let target = self.chat_target.clone();
                let rid = self.recipient.map(|r| self.pl_id(tr, r));
                let sid = self.pl_id(tr, self.a_tok);
                self.mg_display_chat(
                    tr,
                    m1.as_deref(),
                    m0.as_deref(),
                    target.as_deref(),
                    rid.as_deref(),
                    true,
                    Some(&sid),
                );
                let sid2 = self.pl_id(tr, self.a_tok);
                let rid2 = self.recipient.map(|r| self.pl_id(tr, r));
                self.mg_display_chat(
                    tr,
                    m1.as_deref(),
                    m0.as_deref(),
                    target.as_deref(),
                    Some(&sid2),
                    false,
                    rid2.as_deref(),
                );
                self.flag = false;
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    // 17 MissileSilo
    fn missile_silo(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => {
                self.mg_ready = true;
                Ok(JsVal::Undef)
            }
            2 => {
                if self.un_is_under_construction(tr, self.a_tok) {
                    return Ok(JsVal::Undef);
                }
                if !self.un_is_active(tr, self.a_tok) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                let q = self.un_missile_timer_queue(tr, self.a_tok);
                let front = match as_num_list(&q).first() {
                    Some(f) => *f,
                    None => return Ok(JsVal::Undef),
                };
                if !self.mg_ready {
                    return Err("Cannot read properties of undefined (reading 'config')".into());
                }
                let cd = self.mg_silo_cooldown(tr);
                let ticks = self.mg_ticks(tr);
                if cd - (ticks - front) <= 0.0 {
                    self.un_reload_missile(tr, self.a_tok);
                }
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    // 18 UpgradeStructure
    fn upgrade_structure(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 | 4 => Ok(JsVal::Bool(false)),
            2 => Ok(JsVal::Undef),
            1 => {
                self.structure = self.mg_unit(tr, self.unit_id);
                if let Some(ui) = self.structure {
                    if self.un_owner(tr, ui) != Some(self.a_tok) {
                        /* warn: structure not owned by player */
                        self.structure = None;
                    }
                }
                let ui = match self.structure {
                    Some(u) => u,
                    None => {
                        /* warn: structure is undefined */
                        return Ok(JsVal::Undef);
                    }
                };
                let mut i = 0.0;
                while i < self.amount {
                    if !self.pl_can_upgrade_unit(tr, self.a_tok, ui) {
                        if i == 0.0 {
                            // warn reads `this.structure.type()`.
                            let _ = self.un_type(tr, ui);
                        }
                        break;
                    }
                    self.pl_upgrade_unit(tr, self.a_tok, ui);
                    i += 1.0;
                }
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    // 19 City
    fn city(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => {
                self.mg_ready = true;
                Ok(JsVal::Undef)
            }
            2 => {
                if !self.station_created {
                    if !self.mg_ready {
                        return Err(
                            "Cannot read properties of undefined (reading 'hasUnitNearby')".into(),
                        );
                    }
                    let tile = self.un_tile(tr, self.a_tok);
                    let range = self.mg_train_station_max_range(tr);
                    if self.mg_has_unit_nearby(tr, tile, range, UNIT_FACTORY) {
                        self.mg_add_train_station(tr, self.a_tok, None);
                    }
                    self.station_created = true;
                }
                if !self.un_is_active(tr, self.a_tok) {
                    self.flag = false;
                }
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    // 20 Factory
    fn factory(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => {
                self.mg_ready = true;
                Ok(JsVal::Undef)
            }
            2 => {
                if !self.station_created {
                    if !self.mg_ready {
                        return Err(
                            "Cannot read properties of undefined (reading 'nearbyUnits')".into(),
                        );
                    }
                    let tile = self.un_tile(tr, self.a_tok);
                    let range = self.mg_train_station_max_range(tr);
                    let types = vec![
                        UNIT_CITY.to_string(),
                        UNIT_PORT.to_string(),
                        UNIT_FACTORY.to_string(),
                    ];
                    let units = self.mg_nearby_units(tr, tile, range, &types);
                    self.mg_add_train_station(tr, self.a_tok, Some(true));
                    for uid in units {
                        if !self.un_has_train_station(tr, uid) {
                            self.mg_add_train_station(tr, uid, None);
                        }
                    }
                    self.station_created = true;
                }
                if !self.un_is_active(tr, self.a_tok) {
                    self.flag = false;
                }
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    // 21 DefensePost
    fn defense_post(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => {
                self.mg_ready = true;
                Ok(JsVal::Undef)
            }
            2 => {
                if !self.un_is_active(tr, self.a_tok) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                if self.un_is_under_construction(tr, self.a_tok) {
                    return Ok(JsVal::Undef);
                }
                // `this.target` is never set (the ship-targeting block below is
                // commented out in TS), so it stays null and `shoot()` — which
                // lives only in the commented branch — is never called.
                if self.target.is_some() {
                    let t = self.target.unwrap();
                    if !self.un_is_active(tr, t) {
                        self.target = None;
                    }
                }
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    /// `DefensePostExecution.shoot` (TS:19-39) — unreachable from `tick` (the
    /// ship-targeting block is commented out, so `target` stays null); ported
    /// for completeness. `new ShellExecution(...)` is modelled as an
    /// `addExecution` construction record (tag 1), exactly like the
    /// `TrainStationExecution` constructions.
    #[allow(dead_code)]
    fn defense_post_shoot(&mut self, tr: &mut Vec<f64>) {
        let target = match self.target {
            Some(t) => t,
            None => return,
        };
        let rate = self.mg_defense_post_shell_attack_rate(tr);
        let ticks = self.mg_ticks(tr);
        if ticks - self.last_shell_attack > rate {
            self.last_shell_attack = ticks;
            let spawn = self.un_tile(tr, self.a_tok);
            let owner = self.un_owner(tr, self.a_tok);
            self.mg_add_execution_shell(tr, spawn, owner, self.a_tok, target);
            if !self.un_has_health(tr, target) {
                self.already_sent_shell.push(target);
                self.target = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_ally_small_ids_returns_false() {
        // wouldNukeBreakAlliance short-circuits on an empty ally set without
        // touching any facade script.
        let mut h = ExecHarness::new();
        let mut tr = Vec::new();
        assert!(!h.would_nuke_break_alliance(&mut tr, 1.0, 1.0, 2.0, &[], 5.0));
        assert!(tr.is_empty());
    }
}
