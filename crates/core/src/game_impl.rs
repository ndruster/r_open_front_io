//! G4a — `GameImpl` facade harness (`src/core/game/GameImpl.ts`).
//!
//! This module ports the in-scope methods of `GameImpl` (the game facade the
//! simulation drives) and exposes them as a scripted op harness so the golden
//! vector pipeline can diff the Rust port against the real TS class.
//!
//! ## Shape of an op
//!
//! The harness mirrors the `PlayerImpl` conventions exactly: every op is
//! `run_op(kind, args)`; the returned `Vec<f64>` is
//! `[traceLen, ..trace, ..result]`. `result` is `[0, ..encVal(v)]` for a
//! return / `undefined` / `null`, or `[1, ..encS(errorMessage)]` for a throw.
//! The `trace` records the facade (mock) calls the port makes, so call COUNTS
//! and CALL ORDER are compared bit-for-bit as much as the values.
//!
//! ## Real vs scripted surfaces
//!
//! Per the stage plan the following stay REAL on both sides (untraced, driven
//! through the identical ported implementations):
//!
//! * `_map` / `miniGameMap` — the ported [`crate::game_map::GameMap`];
//! * `_waterManager` — the ported [`crate::water_manager::WaterManager`]
//!   (constructed with `disable_nav_mesh = false`, matching the capture);
//! * `_terraNullius` — the ported [`crate::terra_nullius`] value.
//!
//! Everything else is a SCRIPTED facade whose every call pushes a trace event:
//! players, unit mocks, `unitGrid`, `_railNetwork`, `_stats`, `_config`,
//! `_sharedWaterCache`, executions, and the `updates` sink (observed through
//! the harness-only `dumpUpdates` op).
//!
//! ## Opaque live-object returns
//!
//! `map()`, `miniMap()`, `config()`, `stats()`, `railNetwork()`,
//! `terraNullius()`, `miniWaterHPA()` and `miniWaterGraph()` hand back a live
//! object reference, which has no cross-world comparable value; both sides
//! return the single-token marker `Num(0)` (documented convention, the mid
//! names the type, so there is no ambiguity with a real numeric `0`).
//!
//! ## Construct block (op `kind = 0`)
//!
//! Fields are read in this order (all scripts are constants; empty tolerant):
//! `mapBlock`, `ticks`, `startTick`, `nextPlayerID`, `nextUnitID`, `width`,
//! `height`, `nukeQueue`, `unInitExecs`, `execs`, `execTable`, `playersMap`,
//! `playersBySmallID`, `playerTable`, `unitTable`, `unitMap`, `planDriven`,
//! `playerTeams`, `botTeam`, `isPaused`, `winner`, `teamSpawnAreas`, `nations`,
//! `unitsVersion`, `territoryVersion`, `tileUpdates`, `playerStatsQuads`,
//! `attackTroopsQuads`, `motionRecords`, `unitsByTypeMemo`, `unitCountMemo`,
//! `cfg`, `ug`, `swc`.
//!
//! ## Mid table (op `kind = 1`; `mid` = `args[0]`)
//!
//! Declaration-order in-scope methods, then the map pass-throughs:
//!
//! | mid | method | mid | method |
//! |----|--------|-----|--------|
//! | 0 | `isOnEdgeOfMap` | 47 | `target` |
//! | 1 | `owner` | 48 | `isSpawnImmunityActive` |
//! | 2 | `playerBySmallID` | 49 | `elapsedGameSeconds` |
//! | 3 | `map` | 50 | `isNationSpawnImmunityActive` |
//! | 4 | `miniMap` | 51 | `ticksSinceStart` |
//! | 5 | `addUpdate` | 52 | `sendEmojiUpdate` |
//! | 6 | `nextUnitID` | 53 | `setWinner` |
//! | 7 | `setFallout` | 54 | `getWinner` |
//! | 8 | `setWater` | 55 | `isEligibleForTeamWin` |
//! | 9 | `queueWaterConversion` | 56 | `makeWinner` |
//! | 10 | `queueNukeImpact` | 57 | `teams` |
//! | 11 | `drainNukeImpacts` | 58 | `teamTilesOwned` |
//! | 12 | `unit` | 59 | `totalLandTiles` |
//! | 13 | `units` (all overloads) | 60 | `teamSpawnArea` |
//! | 14 | `unitCount` | 61 | `displayMessage` |
//! | 15 | `unitInfo` | 62 | `displayChat` |
//! | 16 | `nations` | 63 | `displayIncomingUnit` |
//! | 17 | `hasPlayer` | 64 | `bumpUnitsVersion` |
//! | 18 | `config` | 65 | `territoryVersion` |
//! | 19 | `isPaused` | 66 | `addUnit` |
//! | 20 | `setPaused` | 67 | `removeUnit` |
//! | 21 | `inSpawnPhase` | 68 | `updateUnitTile` |
//! | 22 | `endSpawnPhase` | 69 | `hasUnitNearby` |
//! | 23 | `ticks` | 70 | `anyUnitNearby` |
//! | 24 | `recordTileUpdate` | 71 | `nearbyUnits` |
//! | 25 | `drainPackedTileUpdates` | 72..114 | `_map` pass-throughs |
//! | 26 | `drainPackedPlayerUpdates` | 115..123 | stats / rail / water |
//! | 27 | `drainPackedAttackUpdates` | 900 | `dumpUpdates` (only harness op) |
//! | 28 | `recordMotionPlan` |  | |
//! | 29 | `isUnitPlanDriven` |  | |
//! | 30 | `maybeAddUnitUpdate` |  | |
//! | 31 | `onUnitMoved` |  | |
//! | 32 | `drainPackedMotionPlans` |  | |
//! | 33 | `hash` |  | |
//! | 34 | `terraNullius` |  | |
//! | 35 | `removeInactiveExecutions` |  | |
//! | 36 | `players` |  | |
//! | 37 | `allPlayers` |  | |
//! | 38 | `executions` |  | |
//! | 39 | `addExecution` |  | |
//! | 40 | `removeExecution` |  | |
//! | 41 | `playerView` |  | |
//! | 42 | `player` |  | |
//! | 43 | `playerByClientID` |  | |
//! | 44 | `isOnMap` |  | |
//! | 45 | `neighborsWithDiag` |  | |
//! | 46 | `forEachNeighborWithDiag` |  | |
//!
//! Map pass-through mids (declaration order): 72 `ref`, 73 `isValidRef`,
//! 74 `x`, 75 `y`, 76 `cell`, 77 `width`, 78 `height`, 79 `numLandTiles`,
//! 80 `waterVersion`, 81 `isValidCoord`, 82 `isLand`, 83 `isImpassable`,
//! 84 `isOceanShore`, 85 `isOcean`, 86 `isShoreline`, 87 `magnitude`,
//! 88 `terrainByte`, 89 `setShorelineBit`, 90 `clearShorelineBit`,
//! 91 `setOcean`, 92 `setMagnitude`, 93 `ownerID`, 94 `hasOwner`,
//! 95 `setOwnerID`, 96 `hasFallout`, 97 `isBorder`, 98 `neighbors`,
//! 99 `forEachNeighbor`, 100 `neighbors4`, 101 `neighbors8`, 102 `isWater`,
//! 103 `isShore`, 104 `cost`, 105 `terrainType`, 106 `forEachTile`,
//! 107 `manhattanDist`, 108 `euclideanDistSquared`, 109 `circleSearch`,
//! 110 `bfs`, 111 `tileState`, 112 `tileStateBuffer`, 113 `updateTile`,
//! 114 `numTilesWithFallout`; then 115 `stats`, 116 `railNetwork`,
//! 117 `miniWaterHPA`, 118 `miniWaterGraph`, 119 `waterGraphVersion`,
//! 120 `getWaterComponent`, 121 `hasWaterComponent`, 122 `getWaterComponentSize`,
//! 123 `sharedWaterComponents`.
//!
//! ## Facade trace event codes
//!
//! Players: 100 `isAlive`, 101 `hasSpawned`, 102 `numTilesOwned`, 104 `hash`,
//! 105 `clientID`, 106 `team`, 108 `name`, 111 `disconnectSnapshot`,
//! 112 `isDisconnected`, 113 `alliances`, 114 `units`, 115 `unitCount`,
//! 117 `smallID` (103 `troops` / 107 `type` / 109 `displayName` / 110
//! `isPlayer` / 116 `id` were reserved in the G4a slice and are exercised by
//! G4b).
//! Units: 120 `toUpdate`, 121 `hasTrainStation`, 122 `id`, 123 `type`.
//! `unitGrid`: 130 `addUnit`, 131 `removeUnit`, 132 `updateUnitCell`,
//! 133 `hasUnitNearby`, 134 `anyUnitNearby`, 135 `nearbyUnits`.
//! Rail: 140 `removeStation`. Stats: 150 `recordFinalTiles`,
//! 151 `recordAlliancesAtEnd`. Config: 160 `gameConfig`, 161 `unitInfo`,
//! 162 `waterNukes`, 163 `spawnImmunityDuration`,
//! 164 `nationSpawnImmunityDuration`, 165 `teamLandShareWinThresholdTenths`.
//! Water cache: 170 `get`. Exec: 180 `activeDuringSpawnPhase`, 181 `isActive`.
//! Callbacks: 190 `forEachNeighbor`, 191 `forEachNeighborWithDiag`,
//! 192 `forEachTile`, 193 `circleSearch` filter, 194 `bfs` filter.
//!
//! ## `dumpUpdates` (mid 900)
//!
//! `updates` is the internal `createGameUpdatesMap()` object: 24 numeric keys
//! (the `GameUpdateType` values) mapping to per-key arrays in push order. The
//! dump value is a flat array of primitive tokens
//! `[24, (type, len, ..flattenedUpdate)*24]` where each update flattens as
//! `[1]` undefined, `[2]` null, `[3,gmNum]` number/bigint (with
//! `"n"`/`"i"`/`"-i"`/`"-0"` special tokens), `[4,b]` bool, `[5,..encS]`
//! string, `[7,n,..]` array/Set, `[6,n,(..encS key,..value)*]` object.
//!
//! ## G4b additions
//!
//! New GameImpl method mids: 300 `executeNextTick` (488-546), 301 `conquer`
//! (761-783), 302 `relinquish` (785-803), 303 `updateBorders` (808-814),
//! 304 `updateBorderStatus` (816-826), 305 `breakAlliance` (836-870),
//! 306 `expireAlliance` (872-893), 307 `removeAlliancesByPlayerSilently`
//! (895-911), 308 `detachAlliance` (914-919), 309 `conquerPlayer` (1371-1452),
//! and the harness observation mid 310 `dumpPlayer`.
//!
//! Player mocks now carry REAL `_tiles` / `_borderTiles` [`TileSet`]s seeded
//! from the construct, plus `_lastTileChange`, `_tileChangeVersion` and
//! `_alliances`. `_alliances` holds indices into a shared alliance table
//! (constructed after `swc`), so identity across two players is preserved
//! (`expireAlliance`'s `Set.has`, `detachAlliance`'s `!==`). `alliances()`
//! returns that array; its trace still reports the per-entry `createdAt`
//! values, so the G4a `alliances` observation is unchanged.
//!
//! New facade trace codes. Players: 103 `troops`, 107 `type`, 109
//! `displayName`, 110 `isPlayer`, 116 `id`, 118 `isOnSameTeam`, 119
//! `captureUnit`, 124 `gold`, 125 `removeGold`, 126 `addGold`, 127 `toUpdate`,
//! 128 `isTraitor`, 129 `markTraitor`, 136 `isAlliedWith`. Alliances: 141
//! `requestor`, 142 `recipient`, 143 `id`, 144 `createdAt`. Stats: 152
//! `recordTickSample`, 153 `getPlayerStats`, 154 `goldWar`, 155 `recordKill`,
//! 156 `recordKilledBy`, 157 `recordDeathPosition`, 158 `allianceEnded`.
//! Config: 166 `conquerGoldAmount`. Exec: 182 `tick`, 183 `init`.
//!
//! `dumpPlayer` (mid 310) flattens one mock as a `gmNorm` token array
//! `[pid, nTiles, ..tiles, nBorder, ..border, _lastTileChange,
//! _tileChangeVersion, nAll, (id,createdAt)*]` (mirroring `gfaNorm`), wrapped
//! as one [`JsVal::Arr`] so special numeric leaves expand to `"n"`/`"i"`/
//! `"-i"`/`"-0"` tokens the same way `dumpUpdates` does.
//!
//! Because `alliances()` now hands back the real shared alliance mocks,
//! `setWinner`'s `for (const a of standing) a.createdAt()` loop emits one 144
//! per entry (on top of the untouched 113); `p_alliances` mirrors that, while
//! the `.length`-only callers (the tick sample, `expireAlliance`'s `Set`) use
//! `p_alliance_toks` and stay 144-free.
//!
//! `conquerPlayer`'s skip branch stores the TS literal `0n` (a bigint) into its
//! `ConquestEvent.gold`; `gfaNorm` has no bigint branch and encodes it as a
//! keyless object `[6,0]`, so the twin uses an empty [`JsVal::Obj`] for that
//! leaf (the non-skip branch carries a plain number).
//!
//! [`TileSet`]: crate::tile_set::TileSet

use crate::game_map::{GameMap, TerrainType};
use crate::js_json::{push_str, push_val, read_str, read_val, val_field, JsVal};
use crate::jsnum::{to_int32, to_uint32};
use crate::motion_plans::{pack_motion_plans, MotionPlanInput};
use crate::tile_set::TileSet;
use crate::utils_format::render_number;
use crate::water_manager::WaterManager;
use std::cell::RefCell;

/// `GameUpdateType` numeric values (contiguous, ascending) — mirrors the TS
/// enum used by the capture (`GameUpdates.ts` lines 83-108).
const UPDATE_TYPE_COUNT: usize = 24;
const UPD_TARGET_PLAYER: f64 = 10.0;
const UPD_EMOJI: f64 = 11.0;
const UPD_WIN: f64 = 12.0;
const UPD_UNIT_INCOMING: f64 = 14.0;
const UPD_DISPLAY_EVENT: f64 = 3.0;
const UPD_DISPLAY_CHAT: f64 = 4.0;
const UPD_SPAWN_PHASE_END: f64 = 21.0;
const UPD_GAME_PAUSED: f64 = 22.0;
const UPD_BROKE_ALLIANCE: f64 = 7.0;
const UPD_ALLIANCE_EXPIRED: f64 = 8.0;
const UPD_HASH: f64 = 13.0;
const UPD_CONQUEST: f64 = 19.0;

// ------------------------------------------------------------------ helpers

fn push_ok_val(o: &mut Vec<f64>, v: &JsVal) {
    o.push(0.0);
    push_val(o, v);
}
fn push_ok_num(o: &mut Vec<f64>, n: f64) {
    push_ok_val(o, &JsVal::Num(n));
}
fn push_ok_bool(o: &mut Vec<f64>, b: bool) {
    push_ok_val(o, &JsVal::Bool(b));
}
fn push_ok_undef(o: &mut Vec<f64>) {
    push_ok_val(o, &JsVal::Undef);
}
fn push_ok_null(o: &mut Vec<f64>) {
    push_ok_val(o, &JsVal::Null);
}
fn push_opt_num(o: &mut Vec<f64>, v: Option<f64>) {
    match v {
        Some(n) => push_ok_num(o, n),
        None => push_ok_null(o),
    }
}
fn push_err(o: &mut Vec<f64>, m: &str) {
    o.push(1.0);
    push_str(o, m);
}
fn push_arr_nums(o: &mut Vec<f64>, xs: &[f64]) {
    o.push(0.0);
    o.push(7.0);
    o.push(xs.len() as f64);
    for x in xs {
        push_val(o, &JsVal::Num(*x));
    }
}

/// `uenc` for a numeric leaf (NaN / ±Infinity / `-0` become string tokens).
fn uenc_val(v: f64) -> JsVal {
    if v.is_nan() {
        JsVal::Str("n".to_string())
    } else if v == f64::INFINITY {
        JsVal::Str("i".to_string())
    } else if v == f64::NEG_INFINITY {
        JsVal::Str("-i".to_string())
    } else if v == 0.0 && v.is_sign_negative() {
        JsVal::Str("-0".to_string())
    } else {
        JsVal::Num(v)
    }
}

/// JS `JsVal::Str` → `String` (an empty string for any other codec state).
fn jsval_str(v: &JsVal) -> String {
    match v {
        JsVal::Str(s) => s.clone(),
        _ => String::new(),
    }
}

/// Read one codec value at `*i`, advancing the cursor.
struct C<'a> {
    a: &'a [f64],
    i: usize,
}
impl<'a> C<'a> {
    fn new(a: &'a [f64]) -> Self {
        Self { a, i: 0 }
    }
    fn f(&mut self) -> f64 {
        let v = self.a[self.i];
        self.i += 1;
        v
    }
    fn b(&mut self) -> bool {
        self.f() != 0.0
    }
    fn s(&mut self) -> String {
        read_str(self.a, &mut self.i)
    }
    fn v(&mut self) -> JsVal {
        read_val(self.a, &mut self.i)
    }
    fn nums(&mut self) -> Vec<f64> {
        let n = self.f() as usize;
        (0..n).map(|_| self.f()).collect()
    }
}

/// A ported motion-plan record kept for `drainPackedMotionPlans`.
enum GmPlan {
    Grid {
        unit_id: f64,
        plan_id: f64,
        start_tick: f64,
        ticks_per_step: f64,
        path: Vec<f64>,
    },
    Train {
        engine_unit_id: f64,
        car_unit_ids: Vec<f64>,
        plan_id: f64,
        start_tick: f64,
        speed: f64,
        spacing: f64,
        path: Vec<f64>,
    },
}

/// A scripted player mock.
struct GmPlayer {
    pid: f64,
    sid: f64,
    alive: bool,
    spawned: bool,
    num_tiles: f64,
    hash: f64,
    client_id: JsVal,
    team: JsVal,
    name: JsVal,
    disconnect_snapshot: JsVal,
    is_disconnected: bool,
    /// `_alliances` — indices into [`GameHarness::alliance_table`].
    alliance_toks: Vec<usize>,
    units: Vec<usize>,
    unit_count: Vec<(String, f64)>,
    // ---- G4b scripted surface ----
    troops: f64,
    typ: String,
    display_name: JsVal,
    id_val: JsVal,
    is_player: bool,
    on_same_team: bool,
    allied_with: bool,
    is_traitor: bool,
    gold: f64,
    tu_stats: Vec<f64>,
    tu_atk: Vec<f64>,
    tu_ret: JsVal,
    // ---- G4b mutable territory ----
    tiles: TileSet,
    border_tiles: TileSet,
    last_tile_change: f64,
    tile_change_version: f64,
}

impl Default for GmPlayer {
    fn default() -> Self {
        Self {
            pid: 0.0,
            sid: 0.0,
            alive: false,
            spawned: false,
            num_tiles: 0.0,
            hash: 0.0,
            client_id: JsVal::Null,
            team: JsVal::Null,
            name: JsVal::Null,
            disconnect_snapshot: JsVal::Null,
            is_disconnected: false,
            alliance_toks: Vec::new(),
            units: Vec::new(),
            unit_count: Vec::new(),
            troops: 0.0,
            typ: String::new(),
            display_name: JsVal::Null,
            id_val: JsVal::Null,
            is_player: false,
            on_same_team: false,
            allied_with: false,
            is_traitor: false,
            gold: 0.0,
            tu_stats: Vec::new(),
            tu_atk: Vec::new(),
            tu_ret: JsVal::Null,
            tiles: TileSet::new(None),
            border_tiles: TileSet::new(None),
            last_tile_change: 0.0,
            tile_change_version: 0.0,
        }
    }
}

/// A scripted alliance mock shared (by table index) between players.
#[derive(Default, Clone)]
struct GmAlliance {
    id: f64,
    created_at: f64,
    /// Player token for `requestor()` (`-2` TN / `-1` undefined / table index).
    req: f64,
    /// Player token for `recipient()`.
    rec: f64,
}

/// A scripted unit mock.
#[derive(Default)]
struct GmUnit {
    uid: f64,
    typ: String,
    has_train_station: bool,
    to_update: JsVal,
}

/// A scripted execution mock.
#[derive(Default)]
struct GmExec {
    adsp: bool,
    active: bool,
}

/// Scripted constants for the `_config` facade.
#[derive(Default)]
struct Cfg {
    gtype: String,
    gmode: String,
    water_nukes: bool,
    spawn_immunity: f64,
    nation_spawn_immunity: f64,
    team_land_share: f64,
    conquer_gold_amount: f64,
}

/// Scripted constants for the `unitGrid` facade.
#[derive(Default)]
struct Ug {
    has_unit_nearby: bool,
    any_unit_nearby: bool,
    nearby_units: JsVal,
    circle_filter: bool,
    bfs_filter: bool,
}

pub struct GameHarness {
    water: Option<WaterManager>,
    constructed: bool,

    players: Vec<GmPlayer>,
    players_map: Vec<(f64, usize)>,
    players_by_small_id: Vec<Option<usize>>,
    units: Vec<GmUnit>,
    exec_table: Vec<GmExec>,
    unit_map: Vec<(f64, usize)>,
    plan_driven: Vec<f64>,
    tile_updates: Vec<f64>,
    player_stats_quads: Vec<f64>,
    attack_troops_quads: Vec<f64>,
    motion_records: Vec<GmPlan>,
    updates: Vec<Vec<JsVal>>,
    units_by_type_memo: Vec<(String, (f64, Vec<usize>))>,
    unit_count_memo: Vec<(String, (f64, f64))>,

    ticks: f64,
    start_tick: Option<f64>,
    next_player_id: f64,
    next_unit_id: f64,
    width: f64,
    height: f64,
    nuke_queue: Vec<f64>,
    uninit_execs: Vec<usize>,
    execs_list: Vec<usize>,
    player_teams: Vec<String>,
    bot_team: String,
    is_paused: bool,
    winner: JsVal,
    team_spawn_areas: JsVal,
    nations: JsVal,
    units_version: f64,
    territory_version: f64,

    cfg: Cfg,
    ug: Ug,
    swc: JsVal,
    /// Scripted `_stats.getPlayerStats(player)` return.
    sps: JsVal,
    /// Shared alliance table (identity for `_alliances`).
    alliance_table: Vec<GmAlliance>,

    tr: Vec<f64>,
    res: Vec<f64>,
}

impl Default for GameHarness {
    fn default() -> Self {
        Self {
            water: None,
            constructed: false,
            players: Vec::new(),
            players_map: Vec::new(),
            players_by_small_id: Vec::new(),
            units: Vec::new(),
            exec_table: Vec::new(),
            unit_map: Vec::new(),
            plan_driven: Vec::new(),
            tile_updates: Vec::new(),
            player_stats_quads: Vec::new(),
            attack_troops_quads: Vec::new(),
            motion_records: Vec::new(),
            updates: vec![Vec::new(); UPDATE_TYPE_COUNT],
            units_by_type_memo: Vec::new(),
            unit_count_memo: Vec::new(),
            ticks: 0.0,
            start_tick: None,
            next_player_id: 0.0,
            next_unit_id: 0.0,
            width: 0.0,
            height: 0.0,
            nuke_queue: Vec::new(),
            uninit_execs: Vec::new(),
            execs_list: Vec::new(),
            player_teams: Vec::new(),
            bot_team: String::new(),
            is_paused: false,
            winner: JsVal::Null,
            team_spawn_areas: JsVal::Undef,
            nations: JsVal::Arr(Vec::new()),
            units_version: 0.0,
            territory_version: 0.0,
            cfg: Cfg::default(),
            ug: Ug::default(),
            swc: JsVal::Null,
            sps: JsVal::Null,
            alliance_table: Vec::new(),
            tr: Vec::new(),
            res: Vec::new(),
        }
    }
}

impl GameHarness {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// The real ported map (`_map`), shared with `_waterManager`.
    fn gm(&self) -> &GameMap {
        self.water.as_ref().expect("map built").debug_map()
    }
    /// The real ported map, mutable (`setOwnerID` / `setWater` / `setFallout`).
    fn gm_mut(&mut self) -> &mut GameMap {
        self.water.as_mut().expect("map built").debug_map_mut()
    }

    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        self.tr.clear();
        self.res.clear();
        if kind == 0 {
            self.construct(args);
        } else {
            self.dispatch(args);
        }
        let mut out = Vec::with_capacity(self.tr.len() + self.res.len() + 1);
        out.push(self.tr.len() as f64);
        out.extend(self.tr.iter().copied());
        out.extend(self.res.iter().copied());
        out
    }

    // ---------------------------------------------------------------- facades

    fn p_alive(&mut self, idx: usize) -> bool {
        let (pid, v) = (self.players[idx].pid, self.players[idx].alive);
        self.tr.push(100.0);
        self.tr.push(pid);
        self.tr.push(if v { 1.0 } else { 0.0 });
        v
    }
    fn p_spawned(&mut self, idx: usize) -> bool {
        let (pid, v) = (self.players[idx].pid, self.players[idx].spawned);
        self.tr.push(101.0);
        self.tr.push(pid);
        self.tr.push(if v { 1.0 } else { 0.0 });
        v
    }
    fn p_num_tiles(&mut self, idx: usize) -> f64 {
        let (pid, v) = (self.players[idx].pid, self.players[idx].num_tiles);
        self.tr.push(102.0);
        self.tr.push(pid);
        self.tr.push(v);
        v
    }
    fn p_hash(&mut self, idx: usize) -> f64 {
        let (pid, v) = (self.players[idx].pid, self.players[idx].hash);
        self.tr.push(104.0);
        self.tr.push(pid);
        self.tr.push(v);
        v
    }
    fn p_client_id(&mut self, idx: usize) -> JsVal {
        let (pid, v) = (self.players[idx].pid, self.players[idx].client_id.clone());
        self.tr.push(105.0);
        self.tr.push(pid);
        push_val(&mut self.tr, &v);
        v
    }
    fn p_team(&mut self, idx: usize) -> JsVal {
        let (pid, v) = (self.players[idx].pid, self.players[idx].team.clone());
        self.tr.push(106.0);
        self.tr.push(pid);
        push_val(&mut self.tr, &v);
        v
    }
    fn p_name(&mut self, idx: usize) -> JsVal {
        let (pid, v) = (self.players[idx].pid, self.players[idx].name.clone());
        self.tr.push(108.0);
        self.tr.push(pid);
        push_val(&mut self.tr, &v);
        v
    }
    fn p_disconnect_snapshot(&mut self, idx: usize) -> JsVal {
        let (pid, v) = (
            self.players[idx].pid,
            self.players[idx].disconnect_snapshot.clone(),
        );
        self.tr.push(111.0);
        self.tr.push(pid);
        push_val(&mut self.tr, &v);
        v
    }
    fn p_is_disconnected(&mut self, idx: usize) -> bool {
        let (pid, v) = (self.players[idx].pid, self.players[idx].is_disconnected);
        self.tr.push(112.0);
        self.tr.push(pid);
        self.tr.push(if v { 1.0 } else { 0.0 });
        v
    }
    /// `player.alliances()` — trace reports the per-entry `createdAt` values
    /// (matching G4a) while the returned value is the `_alliances` token list.
    fn p_alliance_toks(&mut self, idx: usize) -> Vec<usize> {
        let (pid, toks) = (
            self.players[idx].pid,
            self.players[idx].alliance_toks.clone(),
        );
        self.tr.push(113.0);
        self.tr.push(pid);
        self.tr.push(toks.len() as f64);
        for t in &toks {
            let v = self.alliance_table[*t].created_at;
            self.tr.push(v);
        }
        toks
    }
    fn p_alliances(&mut self, idx: usize) -> Vec<f64> {
        let toks = self.p_alliance_toks(idx);
        toks.iter().map(|t| self.a_created_at(*t)).collect()
    }
    fn p_units(&mut self, idx: usize) -> Vec<usize> {
        let (pid, v) = (self.players[idx].pid, self.players[idx].units.clone());
        self.tr.push(114.0);
        self.tr.push(pid);
        self.tr.push(v.len() as f64);
        for x in &v {
            self.tr.push(*x as f64);
        }
        v
    }
    fn p_unit_count(&mut self, idx: usize, typ: &str) -> f64 {
        let pid = self.players[idx].pid;
        let v = self.players[idx]
            .unit_count
            .iter()
            .find(|(k, _)| k == typ)
            .map(|(_, n)| *n)
            .unwrap_or(0.0);
        self.tr.push(115.0);
        self.tr.push(pid);
        push_str(&mut self.tr, typ);
        self.tr.push(v);
        v
    }
    fn p_small_id(&mut self, idx: usize) -> f64 {
        let (pid, sid) = (self.players[idx].pid, self.players[idx].sid);
        self.tr.push(117.0);
        self.tr.push(pid);
        self.tr.push(sid);
        sid
    }

    // ------------------------------------------------ G4b player/alliance facades

    fn p_troops(&mut self, idx: usize) -> f64 {
        let (pid, v) = (self.players[idx].pid, self.players[idx].troops);
        self.tr.push(103.0);
        self.tr.push(pid);
        self.tr.push(v);
        v
    }
    fn p_type(&mut self, idx: usize) -> String {
        let (pid, v) = (self.players[idx].pid, self.players[idx].typ.clone());
        self.tr.push(107.0);
        self.tr.push(pid);
        push_str(&mut self.tr, &v);
        v
    }
    fn p_display_name(&mut self, idx: usize) -> JsVal {
        let (pid, v) = (
            self.players[idx].pid,
            self.players[idx].display_name.clone(),
        );
        self.tr.push(109.0);
        self.tr.push(pid);
        push_val(&mut self.tr, &v);
        v
    }
    fn p_id(&mut self, idx: usize) -> JsVal {
        let (pid, v) = (self.players[idx].pid, self.players[idx].id_val.clone());
        self.tr.push(116.0);
        self.tr.push(pid);
        push_val(&mut self.tr, &v);
        v
    }
    /// `previousOwner.isPlayer()`: `-2` is TerraNullius (always false).
    fn p_is_player_tok(&mut self, tok: f64) -> bool {
        if tok == -2.0 {
            self.tr.push(110.0);
            self.tr.push(-2.0);
            self.tr.push(0.0);
            return false;
        }
        let idx = tok as usize;
        let (pid, v) = (self.players[idx].pid, self.players[idx].is_player);
        self.tr.push(110.0);
        self.tr.push(pid);
        self.tr.push(if v { 1.0 } else { 0.0 });
        v
    }
    fn p_is_on_same_team(&mut self, idx: usize, other: f64) -> bool {
        let (pid, v) = (self.players[idx].pid, self.players[idx].on_same_team);
        self.tr.push(118.0);
        self.tr.push(pid);
        self.tr.push(other);
        self.tr.push(if v { 1.0 } else { 0.0 });
        v
    }
    fn p_capture_unit(&mut self, idx: usize, unit: usize) {
        let pid = self.players[idx].pid;
        self.tr.push(119.0);
        self.tr.push(pid);
        self.tr.push(unit as f64);
    }
    fn p_gold(&mut self, idx: usize) -> f64 {
        let (pid, v) = (self.players[idx].pid, self.players[idx].gold);
        self.tr.push(124.0);
        self.tr.push(pid);
        self.tr.push(v);
        v
    }
    fn p_remove_gold(&mut self, idx: usize, amount: f64) {
        let pid = self.players[idx].pid;
        self.tr.push(125.0);
        self.tr.push(pid);
        self.tr.push(amount);
        if amount <= 0.0 {
            return;
        }
        let actual = self.players[idx].gold.min(amount);
        self.players[idx].gold -= actual;
    }
    fn p_add_gold(&mut self, idx: usize, amount: f64) {
        let pid = self.players[idx].pid;
        self.tr.push(126.0);
        self.tr.push(pid);
        self.tr.push(amount);
        self.players[idx].gold += amount;
    }
    fn p_to_update(&mut self, idx: usize) -> JsVal {
        let pid = self.players[idx].pid;
        let stats = self.players[idx].tu_stats.clone();
        let atk = self.players[idx].tu_atk.clone();
        let ret = self.players[idx].tu_ret.clone();
        self.tr.push(127.0);
        self.tr.push(pid);
        self.tr.push(stats.len() as f64);
        self.tr.extend(stats.iter().copied());
        self.tr.push(atk.len() as f64);
        self.tr.extend(atk.iter().copied());
        push_val(&mut self.tr, &ret);
        self.player_stats_quads.extend(stats);
        self.attack_troops_quads.extend(atk);
        ret
    }
    fn p_is_traitor(&mut self, idx: usize) -> bool {
        let (pid, v) = (self.players[idx].pid, self.players[idx].is_traitor);
        self.tr.push(128.0);
        self.tr.push(pid);
        self.tr.push(if v { 1.0 } else { 0.0 });
        v
    }
    fn p_mark_traitor(&mut self, idx: usize) {
        let pid = self.players[idx].pid;
        self.players[idx].is_traitor = true;
        self.tr.push(129.0);
        self.tr.push(pid);
    }
    fn p_is_allied_with(&mut self, idx: usize, other: f64) -> bool {
        let (pid, v) = (self.players[idx].pid, self.players[idx].allied_with);
        self.tr.push(136.0);
        self.tr.push(pid);
        self.tr.push(other);
        self.tr.push(if v { 1.0 } else { 0.0 });
        v
    }
    fn a_requestor(&mut self, a: usize) -> f64 {
        let v = self.alliance_table[a].req;
        self.tr.push(141.0);
        self.tr.push(a as f64);
        self.tr.push(v);
        v
    }
    fn a_recipient(&mut self, a: usize) -> f64 {
        let v = self.alliance_table[a].rec;
        self.tr.push(142.0);
        self.tr.push(a as f64);
        self.tr.push(v);
        v
    }
    fn a_id(&mut self, a: usize) -> f64 {
        let v = self.alliance_table[a].id;
        self.tr.push(143.0);
        self.tr.push(a as f64);
        self.tr.push(v);
        v
    }
    fn a_created_at(&mut self, a: usize) -> f64 {
        let v = self.alliance_table[a].created_at;
        self.tr.push(144.0);
        self.tr.push(a as f64);
        self.tr.push(v);
        v
    }
    fn stats_record_tick_sample(&mut self, idx: usize, nt: f64, troops: f64, al: f64) {
        let pid = self.players[idx].pid;
        self.tr.push(152.0);
        self.tr.push(pid);
        self.tr.push(nt);
        self.tr.push(troops);
        self.tr.push(al);
    }
    fn stats_get_player_stats(&mut self, idx: usize) -> JsVal {
        let pid = self.players[idx].pid;
        let v = self.sps.clone();
        self.tr.push(153.0);
        self.tr.push(pid);
        push_val(&mut self.tr, &v);
        v
    }
    fn stats_gold_war(&mut self, a: usize, b: usize, gold: f64) {
        let (pa, pb) = (self.players[a].pid, self.players[b].pid);
        self.tr.push(154.0);
        self.tr.push(pa);
        self.tr.push(pb);
        self.tr.push(gold);
    }
    fn stats_record_kill(&mut self, a: usize, b: usize, tick: f64) {
        let (pa, pb) = (self.players[a].pid, self.players[b].pid);
        self.tr.push(155.0);
        self.tr.push(pa);
        self.tr.push(pb);
        self.tr.push(tick);
    }
    fn stats_record_killed_by(&mut self, idx: usize, cid: &JsVal) {
        let pid = self.players[idx].pid;
        self.tr.push(156.0);
        self.tr.push(pid);
        push_val(&mut self.tr, cid);
    }
    fn stats_record_death_position(&mut self, idx: usize, pos: f64) {
        let pid = self.players[idx].pid;
        self.tr.push(157.0);
        self.tr.push(pid);
        self.tr.push(pos);
    }
    fn stats_alliance_ended(&mut self, idx: usize, dur: f64, typ: &JsVal) {
        let pid = self.players[idx].pid;
        self.tr.push(158.0);
        self.tr.push(pid);
        self.tr.push(dur);
        push_val(&mut self.tr, typ);
    }
    fn cfg_conquer_gold_amount(&mut self, idx: usize) -> f64 {
        let pid = self.players[idx].pid;
        let v = self.cfg.conquer_gold_amount;
        self.tr.push(166.0);
        self.tr.push(pid);
        self.tr.push(v);
        v
    }
    fn exec_tick(&mut self, idx: usize, ticks: f64) {
        self.tr.push(182.0);
        self.tr.push(idx as f64);
        self.tr.push(ticks);
    }
    fn exec_init(&mut self, idx: usize, ticks: f64) {
        self.tr.push(183.0);
        self.tr.push(idx as f64);
        self.tr.push(ticks);
    }

    fn u_to_update(&mut self, idx: usize) -> JsVal {
        let v = self.units[idx].to_update.clone();
        self.tr.push(120.0);
        push_val(&mut self.tr, &v);
        v
    }
    fn u_has_train_station(&mut self, idx: usize) -> bool {
        let (uid, v) = (
            self.units[idx].uid,
            self.units[idx].has_train_station,
        );
        self.tr.push(121.0);
        self.tr.push(uid);
        self.tr.push(if v { 1.0 } else { 0.0 });
        v
    }
    fn u_id(&mut self, idx: usize) -> f64 {
        let v = self.units[idx].uid;
        self.tr.push(122.0);
        self.tr.push(v);
        v
    }
    fn u_type(&mut self, idx: usize) -> String {
        let v = self.units[idx].typ.clone();
        self.tr.push(123.0);
        push_str(&mut self.tr, &v);
        v
    }

    fn ug_add_unit(&mut self, idx: usize) {
        self.tr.push(130.0);
        self.tr.push(idx as f64);
    }
    fn ug_remove_unit(&mut self, idx: usize) {
        self.tr.push(131.0);
        self.tr.push(idx as f64);
    }
    fn ug_update_unit_cell(&mut self, idx: usize) {
        self.tr.push(132.0);
        self.tr.push(idx as f64);
    }
    fn ug_has_unit_nearby(&mut self, tile: f64, range: f64, typ: &str, pid: &JsVal, uc: &JsVal) -> bool {
        let v = self.ug.has_unit_nearby;
        self.tr.push(133.0);
        self.tr.push(tile);
        self.tr.push(range);
        push_str(&mut self.tr, typ);
        push_val(&mut self.tr, pid);
        push_val(&mut self.tr, uc);
        self.tr.push(if v { 1.0 } else { 0.0 });
        v
    }
    fn ug_any_unit_nearby(&mut self, tile: f64, range: f64, types: &[String], pid: &JsVal, uc: &JsVal) -> bool {
        let v = self.ug.any_unit_nearby;
        self.tr.push(134.0);
        self.tr.push(tile);
        self.tr.push(range);
        self.tr.push(types.len() as f64);
        for t in types {
            push_str(&mut self.tr, t);
        }
        push_val(&mut self.tr, pid);
        push_val(&mut self.tr, uc);
        self.tr.push(if v { 1.0 } else { 0.0 });
        v
    }
    fn ug_nearby_units(&mut self, tile: f64, range: f64, types: &JsVal, pred: &JsVal, uc: &JsVal) -> JsVal {
        let v = self.ug.nearby_units.clone();
        self.tr.push(135.0);
        self.tr.push(tile);
        self.tr.push(range);
        push_val(&mut self.tr, types);
        push_val(&mut self.tr, pred);
        push_val(&mut self.tr, uc);
        push_val(&mut self.tr, &v);
        v
    }

    fn rail_remove_station(&mut self, idx: usize) {
        self.tr.push(140.0);
        self.tr.push(idx as f64);
    }

    fn stats_record_final_tiles(&mut self, idx: usize, n: f64) {
        let pid = self.players[idx].pid;
        self.tr.push(150.0);
        self.tr.push(pid);
        self.tr.push(n);
    }
    fn stats_record_alliances_at_end(&mut self, idx: usize, len: f64, longest: f64) {
        let pid = self.players[idx].pid;
        self.tr.push(151.0);
        self.tr.push(pid);
        self.tr.push(len);
        self.tr.push(longest);
    }

    fn cfg_game_config(&mut self) -> (String, String) {
        let gmode = self.cfg.gmode.clone();
        let gtype = self.cfg.gtype.clone();
        self.tr.push(160.0);
        push_val(
            &mut self.tr,
            &JsVal::Str(gmode.clone()),
        );
        (gtype, gmode)
    }
    fn cfg_unit_info(&mut self, typ: &str) -> JsVal {
        self.tr.push(161.0);
        push_str(&mut self.tr, typ);
        JsVal::Null
    }
    fn cfg_water_nukes(&mut self) -> bool {
        let v = self.cfg.water_nukes;
        self.tr.push(162.0);
        self.tr.push(if v { 1.0 } else { 0.0 });
        v
    }
    fn cfg_spawn_immunity(&mut self) -> f64 {
        let v = self.cfg.spawn_immunity;
        self.tr.push(163.0);
        self.tr.push(v);
        v
    }
    fn cfg_nation_spawn_immunity(&mut self) -> f64 {
        let v = self.cfg.nation_spawn_immunity;
        self.tr.push(164.0);
        self.tr.push(v);
        v
    }
    fn cfg_team_land_share(&mut self) -> f64 {
        let v = self.cfg.team_land_share;
        self.tr.push(165.0);
        self.tr.push(v);
        v
    }

    fn swc_get(&mut self, idx: usize) -> JsVal {
        let pid = self.players[idx].pid;
        let v = self.swc.clone();
        self.tr.push(170.0);
        self.tr.push(pid);
        v
    }

    fn exec_adsp(&mut self, idx: usize) -> bool {
        let v = self.exec_table[idx].adsp;
        self.tr.push(180.0);
        self.tr.push(idx as f64);
        self.tr.push(if v { 1.0 } else { 0.0 });
        v
    }
    fn exec_active(&mut self, idx: usize) -> bool {
        let v = self.exec_table[idx].active;
        self.tr.push(181.0);
        self.tr.push(idx as f64);
        self.tr.push(if v { 1.0 } else { 0.0 });
        v
    }

    /// `addUpdate(update)` — push into the type-keyed array.
    fn add_update(&mut self, update: JsVal) {
        let ty = val_field(&update, "type")
            .and_then(|v| match v {
                JsVal::Num(n) => Some(*n),
                _ => None,
            })
            .unwrap_or(f64::NAN);
        if ty.is_nan() || ty < 0.0 || ty >= UPDATE_TYPE_COUNT as f64 {
            return;
        }
        self.updates[ty as usize].push(update);
    }

    /// `playerBySmallID(id)` → `-2` TN, `-1` undefined, else the table index.
    fn player_by_small_id(&self, id: f64) -> JsVal {
        if id == 0.0 {
            return JsVal::Num(-2.0);
        }
        if id >= 1.0 && id.fract() == 0.0 {
            let k = (id as i64 - 1) as usize;
            if let Some(Some(idx)) = self.players_by_small_id.get(k) {
                return JsVal::Num(*idx as f64);
            }
        }
        JsVal::Num(-1.0)
    }

    // ---------------------------------------------------------------- construct

    fn construct(&mut self, args: &[f64]) {
        let mut c = C::new(args);
        self.build_water(&mut c);
        self.ticks = c.f();
        let st = c.f();
        self.start_tick = if st == 0.0 { None } else { Some(c.f()) };
        self.next_player_id = c.f();
        self.next_unit_id = c.f();
        self.width = c.f();
        self.height = c.f();
        self.nuke_queue = c.nums();
        self.uninit_execs = c.nums().into_iter().map(|v| v as usize).collect();
        self.execs_list = c.nums().into_iter().map(|v| v as usize).collect();
        let ne = c.f() as usize;
        self.exec_table = (0..ne)
            .map(|_| GmExec {
                adsp: c.b(),
                active: c.b(),
            })
            .collect();
        let np = c.f() as usize;
        self.players_map = (0..np).map(|_| (c.f(), c.f() as usize)).collect();
        let nm = c.f() as usize;
        self.players_by_small_id = (0..nm)
            .map(|_| {
                let v = c.f();
                if v < 0.0 {
                    None
                } else {
                    Some(v as usize)
                }
            })
            .collect();
        let npl = c.f() as usize;
        self.players = (0..npl)
            .map(|_| {
                let pid = c.f();
                let sid = c.f();
                let alive = c.b();
                let spawned = c.b();
                let num_tiles = c.f();
                let hash = c.f();
                let client_id = c.v();
                let team = c.v();
                let name = c.v();
                let disconnect_snapshot = c.v();
                let is_disconnected = c.b();
                let alliance_toks: Vec<usize> =
                    c.nums().into_iter().map(|v| v as usize).collect();
                let units: Vec<usize> = c.nums().into_iter().map(|v| v as usize).collect();
                let nuc = c.f() as usize;
                let unit_count = (0..nuc).map(|_| (c.s(), c.f())).collect();
                let tiles = TileSet::new(Some(&c.nums()));
                let border_tiles = TileSet::new(Some(&c.nums()));
                let last_tile_change = c.f();
                let tile_change_version = c.f();
                let troops = c.f();
                let typ = c.s();
                let display_name = c.v();
                let id_val = c.v();
                let is_player = c.b();
                let on_same_team = c.b();
                let allied_with = c.b();
                let is_traitor = c.b();
                let gold = c.f();
                let tu_stats = c.nums();
                let tu_atk = c.nums();
                let tu_ret = c.v();
                GmPlayer {
                    pid,
                    sid,
                    alive,
                    spawned,
                    num_tiles,
                    hash,
                    client_id,
                    team,
                    name,
                    disconnect_snapshot,
                    is_disconnected,
                    alliance_toks,
                    units,
                    unit_count,
                    troops,
                    typ,
                    display_name,
                    id_val,
                    is_player,
                    on_same_team,
                    allied_with,
                    is_traitor,
                    gold,
                    tu_stats,
                    tu_atk,
                    tu_ret,
                    tiles,
                    border_tiles,
                    last_tile_change,
                    tile_change_version,
                }
            })
            .collect();
        let nun = c.f() as usize;
        self.units = (0..nun)
            .map(|_| GmUnit {
                uid: c.f(),
                typ: c.s(),
                has_train_station: c.b(),
                to_update: c.v(),
            })
            .collect();
        let num = c.f() as usize;
        self.unit_map = (0..num).map(|_| (c.f(), c.f() as usize)).collect();
        self.plan_driven = c.nums();
        let npt = c.f() as usize;
        self.player_teams = (0..npt).map(|_| c.s()).collect();
        self.bot_team = c.s();
        self.is_paused = c.b();
        self.winner = c.v();
        self.team_spawn_areas = c.v();
        self.nations = c.v();
        self.units_version = c.f();
        self.territory_version = c.f();
        self.tile_updates = c.nums();
        self.player_stats_quads = c.nums();
        self.attack_troops_quads = c.nums();
        let nmr = c.f() as usize;
        self.motion_records = (0..nmr).map(|_| decode_plan(&c.v())).collect();
        let nutm = c.f() as usize;
        for _ in 0..nutm {
            let typ = c.s();
            let ver = c.f();
            let n = c.f() as usize;
            let us: Vec<usize> = (0..n).map(|_| c.f() as usize).collect();
            self.units_by_type_memo.push((typ, (ver, us)));
        }
        let nucm = c.f() as usize;
        for _ in 0..nucm {
            let typ = c.s();
            let ver = c.f();
            let count = c.f();
            self.unit_count_memo.push((typ, (ver, count)));
        }
        self.cfg.gtype = c.s();
        self.cfg.gmode = c.s();
        self.cfg.water_nukes = c.b();
        self.cfg.spawn_immunity = c.f();
        self.cfg.nation_spawn_immunity = c.f();
        self.cfg.team_land_share = c.f();
        self.cfg.conquer_gold_amount = c.f();
        self.ug.has_unit_nearby = c.b();
        self.ug.any_unit_nearby = c.b();
        self.ug.nearby_units = c.v();
        self.ug.circle_filter = c.b();
        self.ug.bfs_filter = c.b();
        self.swc = c.v();
        self.sps = c.v();
        let nat = c.f() as usize;
        self.alliance_table = (0..nat)
            .map(|_| GmAlliance {
                id: c.f(),
                created_at: c.f(),
                req: c.f(),
                rec: c.f(),
            })
            .collect();
        self.constructed = true;
        self.res.push(0.0);
    }

    fn build_water(&mut self, c: &mut C) {
        if !c.b() {
            self.water = None;
            return;
        }
        let w = c.f();
        let h = c.f();
        let nl = c.f();
        let tn = c.f() as usize;
        let terrain: Vec<u8> = (0..tn).map(|_| c.f() as u8).collect();
        let mw = c.f();
        let mh = c.f();
        let mnl = c.f();
        let mtn = c.f() as usize;
        let mter: Vec<u8> = (0..mtn).map(|_| c.f() as u8).collect();
        let on = c.f() as usize;
        let owners: Vec<(f64, f64)> = (0..on).map(|_| (c.f(), c.f())).collect();
        let mut map = GameMap::new(w, h, terrain, nl);
        for (t, o) in &owners {
            map.set_owner_id(*t, *o);
        }
        let mini = GameMap::new(mw, mh, mter, mnl);
        self.water = Some(WaterManager::new(map, mini, false));
    }

    // ------------------------------------------------------- G4b method bodies

    /// `updateBorderStatus(tile)` (TS 815-826) — no traced facades.
    fn update_border_status(&mut self, t: f64) {
        if !self.gm().has_owner(t) {
            return;
        }
        let oid = self.gm().owner_id(t);
        let idx = match self.player_by_small_id(oid) {
            JsVal::Num(n) if n >= 0.0 && n.fract() == 0.0 => n as usize,
            _ => return,
        };
        if self.gm().is_border(t) {
            self.players[idx].border_tiles.add(t);
        } else {
            self.players[idx].border_tiles.delete(t);
        }
    }

    /// `updateBorders(tile)` (TS 807-813).
    fn update_borders(&mut self, tile: f64) {
        self.update_border_status(tile);
        let (n, buf) = {
            let m = self.gm();
            let mut b = [0.0f64; 4];
            let n = m.neighbors4(tile, &mut b);
            (n, b)
        };
        for t in &buf[..n] {
            self.update_border_status(*t);
        }
    }

    /// `detachAlliance(alliance)` (TS 913-919) — identity by table index.
    fn detach_alliance(&mut self, a: usize) {
        let req = self.a_requestor(a);
        let rec = self.a_recipient(a);
        let ri = req as usize;
        let ci = rec as usize;
        self.players[ri].alliance_toks.retain(|x| *x != a);
        self.players[ci].alliance_toks.retain(|x| *x != a);
    }

    /// `this.hash()` — `1 + Σ player.hash()` in `_players` order.
    fn hash_all(&mut self) -> f64 {
        let order: Vec<usize> = self.players_map.iter().map(|(_, i)| *i).collect();
        let mut hash = 1.0;
        for idx in order {
            hash += self.p_hash(idx);
        }
        hash
    }

    /// `executeNextTick()` (TS 488-546).
    fn m_execute_next_tick(&mut self) {
        self.updates = vec![Vec::new(); UPDATE_TYPE_COUNT];
        self.tile_updates.clear();
        let isp = self.start_tick.is_none();
        let ticks = self.ticks;
        let execs_snapshot: Vec<usize> = self.execs_list.clone();
        for e in execs_snapshot {
            let first = if isp { self.exec_adsp(e) } else { true };
            let act = if first { self.exec_active(e) } else { false };
            if first && act {
                self.exec_tick(e, ticks);
            }
        }
        let uninit_snapshot: Vec<usize> = self.uninit_execs.clone();
        let mut inited: Vec<usize> = Vec::new();
        let mut un_inited: Vec<usize> = Vec::new();
        for e in uninit_snapshot {
            let cond = if !isp { true } else { self.exec_adsp(e) };
            if cond {
                self.exec_init(e, ticks);
                inited.push(e);
            } else {
                un_inited.push(e);
            }
        }
        // removeInactiveExecutions() — `execs` only, before `inited` is pushed.
        let list = std::mem::take(&mut self.execs_list);
        let mut kept: Vec<usize> = Vec::new();
        for e in &list {
            let keep = if isp {
                !self.exec_adsp(*e) || self.exec_active(*e)
            } else {
                self.exec_active(*e)
            };
            if keep {
                kept.push(*e);
            }
        }
        self.execs_list = kept;
        self.execs_list.extend(inited);
        self.uninit_execs = un_inited;

        let order: Vec<usize> = self.players_map.iter().map(|(_, i)| *i).collect();
        for idx in order {
            if self.p_alive(idx) && self.p_spawned(idx) {
                let nt = self.p_num_tiles(idx);
                let tr = self.p_troops(idx);
                let al = self.p_alliance_toks(idx).len() as f64;
                self.stats_record_tick_sample(idx, nt, tr, al);
            }
            let upd = self.p_to_update(idx);
            if !matches!(upd, JsVal::Null) {
                self.add_update(upd);
            }
        }
        if self.ticks % 10.0 == 0.0 {
            let h = self.hash_all();
            self.add_update(JsVal::Obj(vec![
                ("type".to_string(), JsVal::Num(UPD_HASH)),
                ("tick".to_string(), JsVal::Num(self.ticks)),
                ("hash".to_string(), JsVal::Num(h)),
            ]));
        }
        let changed = self.water.as_mut().expect("map built").tick(self.ticks);
        for t in changed {
            self.record_tile_update(t);
        }
        self.ticks += 1.0;
    }

    /// `conquer(owner, tile)` (TS 759-782).
    fn m_conquer(&mut self, c: &mut C) {
        let pidx = c.f() as usize;
        let tile = c.f();
        if !self.gm().is_land(tile) {
            push_err(&mut self.res, "cannot conquer water");
            return;
        }
        if self.gm().is_impassable(tile) {
            push_err(&mut self.res, "cannot conquer impassable terrain");
            return;
        }
        let oid = self.gm().owner_id(tile);
        let potok = match self.player_by_small_id(oid) {
            JsVal::Num(n) => n,
            _ => -1.0,
        };
        if potok == -1.0 {
            push_err(
                &mut self.res,
                "Cannot read properties of undefined (reading 'isPlayer')",
            );
            return;
        }
        let isp = self.p_is_player_tok(potok);
        if isp {
            let i = potok as usize;
            self.players[i].last_tile_change = self.ticks;
            self.players[i].tile_change_version += 1.0;
            self.players[i].tiles.delete(tile);
            self.players[i].border_tiles.delete(tile);
        }
        self.territory_version += 1.0;
        let sid = self.p_small_id(pidx);
        self.gm_mut().set_owner_id(tile, sid);
        self.players[pidx].tiles.add(tile);
        self.players[pidx].last_tile_change = self.ticks;
        self.players[pidx].tile_change_version += 1.0;
        self.update_borders(tile);
        self.gm_mut().set_fallout(tile, false);
        self.record_tile_update(tile);
        push_ok_undef(&mut self.res);
    }

    /// `relinquish(tile)` (TS 783-802).
    fn m_relinquish(&mut self, c: &mut C) {
        let tile = c.f();
        if !self.gm().has_owner(tile) {
            push_err(&mut self.res, "Cannot relinquish tile because it is unowned");
            return;
        }
        if self.gm().is_water(tile) {
            push_err(&mut self.res, "Cannot relinquish water");
            return;
        }
        let oid = self.gm().owner_id(tile);
        let idx = match self.player_by_small_id(oid) {
            JsVal::Num(n) if n >= 0.0 && n.fract() == 0.0 => n as usize,
            _ => {
                push_err(
                    &mut self.res,
                    "Cannot set properties of undefined (setting '_lastTileChange')",
                );
                return;
            }
        };
        self.players[idx].last_tile_change = self.ticks;
        self.players[idx].tile_change_version += 1.0;
        self.players[idx].tiles.delete(tile);
        self.players[idx].border_tiles.delete(tile);
        self.territory_version += 1.0;
        self.gm_mut().set_owner_id(tile, 0.0);
        self.update_borders(tile);
        self.record_tile_update(tile);
        push_ok_undef(&mut self.res);
    }

    /// `breakAlliance(breaker, alliance)` (TS 836-870).
    fn m_break_alliance(&mut self, c: &mut C) {
        let b = c.f() as usize;
        let a = c.f() as usize;
        let req = self.a_requestor(a);
        let other = if req == b as f64 {
            self.a_recipient(a)
        } else {
            self.a_requestor(a)
        };
        if !self.p_is_allied_with(b, other) {
            push_err(
                &mut self.res,
                "[object Object] not allied with [object Object], cannot break alliance",
            );
            return;
        }
        let oidx = other as usize;
        let dur = self.ticks - self.a_created_at(a);
        let mut cond = !self.p_is_traitor(oidx);
        if cond {
            cond = !self.p_is_disconnected(oidx);
        }
        if cond {
            self.p_mark_traitor(b);
            self.stats_alliance_ended(
                oidx,
                dur,
                &JsVal::Str("brokenByOther".to_string()),
            );
        } else {
            self.stats_alliance_ended(oidx, dur, &JsVal::Null);
        }
        self.stats_alliance_ended(b, dur, &JsVal::Null);
        self.detach_alliance(a);
        let tsid = self.p_small_id(b);
        let bsid = self.p_small_id(oidx);
        let aid = self.a_id(a);
        self.add_update(JsVal::Obj(vec![
            ("type".to_string(), JsVal::Num(UPD_BROKE_ALLIANCE)),
            ("traitorID".to_string(), JsVal::Num(tsid)),
            ("betrayedID".to_string(), JsVal::Num(bsid)),
            ("allianceID".to_string(), JsVal::Num(aid)),
        ]));
        push_ok_undef(&mut self.res);
    }

    /// `expireAlliance(alliance)` (TS 872-893).
    fn m_expire_alliance(&mut self, c: &mut C) {
        let a = c.f() as usize;
        let rec = self.a_recipient(a);
        let p1set = self.p_alliance_toks(rec as usize);
        let req = self.a_requestor(a);
        let req_toks = self.p_alliance_toks(req as usize);
        let filtered: Vec<usize> = req_toks
            .iter()
            .copied()
            .filter(|x| p1set.contains(x))
            .collect();
        if filtered.len() != 1 {
            push_err(
                &mut self.res,
                &format!(
                    "cannot expire alliance: must have exactly one alliance, have {}",
                    filtered.len()
                ),
            );
            return;
        }
        let expiring = filtered[0];
        let dur = self.ticks - self.a_created_at(expiring);
        let er = self.a_requestor(expiring);
        self.stats_alliance_ended(er as usize, dur, &JsVal::Str("expired".to_string()));
        let ec = self.a_recipient(expiring);
        self.stats_alliance_ended(ec as usize, dur, &JsVal::Str("expired".to_string()));
        self.detach_alliance(expiring);
        let r2 = self.a_requestor(a);
        let s1 = self.p_small_id(r2 as usize);
        let c2 = self.a_recipient(a);
        let s2 = self.p_small_id(c2 as usize);
        self.add_update(JsVal::Obj(vec![
            ("type".to_string(), JsVal::Num(UPD_ALLIANCE_EXPIRED)),
            ("player1ID".to_string(), JsVal::Num(s1)),
            ("player2ID".to_string(), JsVal::Num(s2)),
        ]));
        push_ok_undef(&mut self.res);
    }

    /// `removeAlliancesByPlayerSilently(player)` (TS 895-911).
    fn m_remove_alliances_by_player_silently(&mut self, c: &mut C) {
        let p = c.f() as usize;
        let snapshot: Vec<usize> = self.players[p].alliance_toks.clone();
        for a in snapshot {
            let dur = self.ticks - self.a_created_at(a);
            let rq = self.a_requestor(a);
            self.stats_alliance_ended(rq as usize, dur, &JsVal::Null);
            let rc = self.a_recipient(a);
            self.stats_alliance_ended(rc as usize, dur, &JsVal::Null);
            self.detach_alliance(a);
        }
        push_ok_undef(&mut self.res);
    }

    /// `conquerPlayer(conqueror, conquered)` (TS 1371-1452).
    fn m_conquer_player(&mut self, c: &mut C) {
        let conq = c.f() as usize;
        let conned = c.f() as usize;
        let disc = self.p_is_disconnected(conned);
        let mut same = false;
        if disc {
            same = self.p_is_on_same_team(conq, conned as f64);
        }
        if disc && same {
            let units = self.p_units(conned);
            let mut ships: Vec<usize> = Vec::new();
            for u in units {
                let t = self.u_type(u);
                let mut is = t == "Warship";
                if !is {
                    is = self.u_type(u) == "Transport";
                }
                if is {
                    ships.push(u);
                }
            }
            for s in ships {
                self.p_capture_unit(conq, s);
            }
        }
        let stats = self.stats_get_player_stats(conned);
        let attacks_sent = match val_field(&stats, "attacks") {
            Some(JsVal::Arr(a)) => match a.first() {
                Some(JsVal::Num(n)) => *n,
                _ => 0.0,
            },
            _ => 0.0,
        };
        let mut skip = attacks_sent == 0.0;
        if skip {
            skip = self.p_type(conned) == "HUMAN";
        }
        let gold = if skip { 0.0 } else { self.p_gold(conned) };
        let gold_captured = if skip {
            0.0
        } else {
            self.cfg_conquer_gold_amount(conned)
        };
        if skip {
            let cid = self.p_id(conq);
            let dn = self.p_display_name(conned);
            let fid = self.p_id(conned);
            let params = JsVal::Obj(vec![("name".to_string(), dn)]);
            self.display_message(
                "events_display.conquered_no_gold".to_string(),
                JsVal::Num(3.0),
                cid,
                JsVal::Undef,
                params,
                JsVal::Undef,
                fid,
            );
        } else {
            let cid = self.p_id(conq);
            let dn = self.p_display_name(conned);
            let gold_str = JsVal::Str(render_number(gold_captured, 0.0, 0.0));
            let fid = self.p_id(conned);
            let params = JsVal::Obj(vec![
                ("gold".to_string(), gold_str),
                ("name".to_string(), dn),
            ]);
            self.display_message(
                "events_display.received_gold_from_conquest".to_string(),
                JsVal::Num(3.0),
                cid,
                JsVal::Num(gold),
                params,
                JsVal::Undef,
                fid,
            );
            self.p_add_gold(conq, gold_captured);
            self.p_remove_gold(conned, gold);
            self.stats_gold_war(conq, conned, gold_captured);
        }
        let tick = self.ticks;
        self.stats_record_kill(conq, conned, tick);
        let cid2 = self.p_client_id(conq);
        self.stats_record_killed_by(conned, &cid2);
        let order: Vec<usize> = self.players_map.iter().map(|(_, i)| *i).collect();
        let mut alive: Vec<usize> = Vec::new();
        for idx in order {
            if self.p_alive(idx) {
                alive.push(idx);
            }
        }
        let mut cnt = 0.0;
        for idx in alive {
            if idx != conned && self.p_type(idx) != "BOT" {
                cnt += 1.0;
            }
        }
        self.stats_record_death_position(conned, cnt + 1.0);
        let cid3 = self.p_id(conq);
        let cid4 = self.p_id(conned);
        // In the skip branch `goldCaptured` is the TS literal `0n` (a bigint).
        // The dump codec's `gfaNorm` has no bigint branch, so it treats it as a
        // keyless object -> `[6, 0]`; Rust mirrors that exact leaf.
        let gold_field = if skip {
            JsVal::Obj(Vec::new())
        } else {
            JsVal::Num(gold_captured)
        };
        self.add_update(JsVal::Obj(vec![
            ("type".to_string(), JsVal::Num(UPD_CONQUEST)),
            ("conquerorId".to_string(), cid3),
            ("conqueredId".to_string(), cid4),
            ("gold".to_string(), gold_field),
        ]));
        push_ok_undef(&mut self.res);
    }

    /// `dumpPlayer` (harness observation, mid 310): a flat `gmNorm`-encoded
    /// token array `[pid, nTiles, ..tiles, nBorder, ..border, lastTileChange,
    /// tileChangeVersion, nAll, (id,createdAt)*]`, wrapped as an array.
    fn dump_player(&mut self, idx: usize) {
        let mut parts: Vec<f64> = Vec::new();
        {
            let leaf = |v: f64, out: &mut Vec<f64>| norm_val(&JsVal::Num(v), out);
            leaf(self.players[idx].pid, &mut parts);
            let tiles = self.players[idx].tiles.collect_values();
            leaf(tiles.len() as f64, &mut parts);
            for t in tiles {
                leaf(t, &mut parts);
            }
            let border = self.players[idx].border_tiles.collect_values();
            leaf(border.len() as f64, &mut parts);
            for t in border {
                leaf(t, &mut parts);
            }
            leaf(self.players[idx].last_tile_change, &mut parts);
            leaf(self.players[idx].tile_change_version, &mut parts);
            let toks = self.players[idx].alliance_toks.clone();
            leaf(toks.len() as f64, &mut parts);
            for t in toks {
                leaf(self.alliance_table[t].id, &mut parts);
                leaf(self.alliance_table[t].created_at, &mut parts);
            }
        }
        push_arr_nums(&mut self.res, &parts);
    }

    // ---------------------------------------------------------------- dispatch

    fn dispatch(&mut self, args: &[f64]) {
        let mid = args[0] as usize;
        let mut c = C::new(&args[1..]);
        match mid {
            0 => {
                let r = c.f();
                let v = self.gm().is_on_edge_of_map(r);
                push_ok_bool(&mut self.res, v);
            }
            1 => {
                let r = c.f();
                let oid = self.gm().owner_id(r);
                let v = self.player_by_small_id(oid);
                push_ok_val(&mut self.res, &v);
            }
            2 => {
                let id = c.f();
                let v = self.player_by_small_id(id);
                push_ok_val(&mut self.res, &v);
            }
            // map() / miniMap() / config() / stats() / railNetwork() /
            // terraNullius() — the opaque live-object marker.
            3 | 4 | 18 | 34 | 115 | 116 => push_ok_num(&mut self.res, 0.0),
            5 => {
                let u = c.v();
                self.add_update(u);
                push_ok_undef(&mut self.res);
            }
            6 => {
                let old = self.next_unit_id;
                self.next_unit_id += 1.0;
                push_ok_num(&mut self.res, old);
            }
            7 => {
                let tile = c.f();
                let value = c.b();
                // `if (value && this.hasOwner(tile)) throw`
                if value {
                    let ho = self.gm().has_owner(tile);
                    if ho {
                        push_err(
                            &mut self.res,
                            &format!("cannot set fallout, tile {tile} has owner"),
                        );
                        return;
                    }
                }
                let hf = self.gm().has_fallout(tile);
                if hf {
                    push_ok_undef(&mut self.res);
                    return;
                }
                self.territory_version += 1.0;
                self.gm_mut().set_fallout(tile, value);
                self.record_tile_update(tile);
                push_ok_undef(&mut self.res);
            }
            8 => {
                let tile = c.f();
                let is_land = self.gm().is_land(tile);
                if !is_land {
                    push_ok_undef(&mut self.res);
                    return;
                }
                let ho = self.gm().has_owner(tile);
                if ho {
                    push_err(
                        &mut self.res,
                        &format!("cannot set water, tile {tile} has owner"),
                    );
                    return;
                }
                let hf = self.gm().has_fallout(tile);
                if hf {
                    self.gm_mut().set_fallout(tile, false);
                }
                self.territory_version += 1.0;
                self.gm_mut().set_water(tile);
                self.record_tile_update(tile);
                push_ok_undef(&mut self.res);
            }
            9 => {
                let tile = c.f();
                let is_land = self.gm().is_land(tile);
                if !is_land {
                    push_ok_undef(&mut self.res);
                    return;
                }
                let ho = self.gm().has_owner(tile);
                if ho {
                    push_err(
                        &mut self.res,
                        &format!("cannot queue water conversion, tile {tile} has owner"),
                    );
                    return;
                }
                let wn = self.cfg_water_nukes();
                if !wn {
                    // delegate to setFallout(tile, true)
                    if self.gm().has_fallout(tile) {
                        push_ok_undef(&mut self.res);
                        return;
                    }
                    self.territory_version += 1.0;
                    self.gm_mut().set_fallout(tile, true);
                    self.record_tile_update(tile);
                    push_ok_undef(&mut self.res);
                    return;
                }
                self.water.as_mut().expect("map built").queue_tile(tile);
                push_ok_undef(&mut self.res);
            }
            10 => {
                let tile = c.f();
                self.nuke_queue.push(tile);
                push_ok_undef(&mut self.res);
            }
            11 => {
                let old = std::mem::take(&mut self.nuke_queue);
                push_arr_nums(&mut self.res, &old);
            }
            12 => {
                let id = c.f();
                let tok = self
                    .unit_map
                    .iter()
                    .find(|(u, _)| *u == id)
                    .map(|(_, i)| *i);
                match tok {
                    Some(i) => push_ok_num(&mut self.res, i as f64),
                    None => push_ok_undef(&mut self.res),
                }
            }
            13 => {
                let first = c.v();
                let second = c.v();
                let third = c.v();
                let out = self.units_impl(first, second, third);
                let arr: Vec<JsVal> = out.iter().map(|i| JsVal::Num(*i as f64)).collect();
                push_ok_val(&mut self.res, &JsVal::Arr(arr));
            }
            14 => {
                let typ = jsval_str(&c.v());
                let n = self.unit_count_impl(&typ);
                push_ok_num(&mut self.res, n);
            }
            15 => {
                let typ = jsval_str(&c.v());
                let v = self.cfg_unit_info(&typ);
                push_ok_val(&mut self.res, &v);
            }
            16 => {
                let v = self.nations.clone();
                push_ok_val(&mut self.res, &v);
            }
            17 => {
                let id = c.f();
                let has = self.players_map.iter().any(|(pid, _)| *pid == id);
                push_ok_bool(&mut self.res, has);
            }
            19 => {
                let v = self.is_paused;
                push_ok_bool(&mut self.res, v);
            }
            20 => {
                let v = c.v();
                self.is_paused = js_truthy(&v);
                let paused = self.is_paused;
                self.add_update(JsVal::Obj(vec![
                    ("type".to_string(), JsVal::Num(UPD_GAME_PAUSED)),
                    ("paused".to_string(), JsVal::Bool(paused)),
                ]));
                push_ok_undef(&mut self.res);
            }
            21 => {
                let v = self.start_tick.is_none();
                push_ok_bool(&mut self.res, v);
            }
            22 => {
                if self.start_tick.is_some() {
                    push_ok_undef(&mut self.res);
                    return;
                }
                self.start_tick = Some(self.ticks);
                let st = self.start_tick.unwrap();
                self.add_update(JsVal::Obj(vec![
                    ("type".to_string(), JsVal::Num(UPD_SPAWN_PHASE_END)),
                    ("startTick".to_string(), JsVal::Num(st)),
                ]));
                push_ok_undef(&mut self.res);
            }
            23 => {
                let v = self.ticks;
                push_ok_num(&mut self.res, v);
            }
            24 => {
                let tile = c.f();
                self.record_tile_update(tile);
                push_ok_undef(&mut self.res);
            }
            25 => {
                let pairs = std::mem::take(&mut self.tile_updates);
                let packed: Vec<f64> = pairs.iter().map(|v| to_uint32(*v) as f64).collect();
                push_arr_nums(&mut self.res, &packed);
            }
            26 => {
                if self.player_stats_quads.is_empty() {
                    push_ok_null(&mut self.res);
                } else {
                    let q = std::mem::take(&mut self.player_stats_quads);
                    push_arr_nums(&mut self.res, &q);
                }
            }
            27 => {
                if self.attack_troops_quads.is_empty() {
                    push_ok_null(&mut self.res);
                } else {
                    let q = std::mem::take(&mut self.attack_troops_quads);
                    push_arr_nums(&mut self.res, &q);
                }
            }
            28 => {
                let r = c.v();
                self.record_motion_plan(&r);
                push_ok_undef(&mut self.res);
            }
            29 => {
                let id = c.f();
                let has = self.plan_driven.contains(&id);
                push_ok_bool(&mut self.res, has);
            }
            30 => {
                let uidx = c.f() as usize;
                let id = self.u_id(uidx);
                if !self.plan_driven.contains(&id) {
                    let upd = self.u_to_update(uidx);
                    self.add_update(upd);
                }
                push_ok_undef(&mut self.res);
            }
            31 => {
                let uidx = c.f() as usize;
                self.ug_update_unit_cell(uidx);
                let id = self.u_id(uidx);
                if !self.plan_driven.contains(&id) {
                    let upd = self.u_to_update(uidx);
                    self.add_update(upd);
                }
                push_ok_undef(&mut self.res);
            }
            32 => {
                if self.motion_records.is_empty() {
                    push_ok_null(&mut self.res);
                } else {
                    let records = std::mem::take(&mut self.motion_records);
                    let inputs: Vec<MotionPlanInput> = records
                        .iter()
                        .map(|r| match r {
                            GmPlan::Grid {
                                unit_id,
                                plan_id,
                                start_tick,
                                ticks_per_step,
                                path,
                            } => MotionPlanInput::Grid {
                                unit_id: *unit_id,
                                plan_id: *plan_id,
                                start_tick: *start_tick,
                                ticks_per_step: *ticks_per_step,
                                path,
                            },
                            GmPlan::Train {
                                engine_unit_id,
                                car_unit_ids,
                                plan_id,
                                start_tick,
                                speed,
                                spacing,
                                path,
                            } => MotionPlanInput::Train {
                                engine_unit_id: *engine_unit_id,
                                car_unit_ids,
                                plan_id: *plan_id,
                                start_tick: *start_tick,
                                speed: *speed,
                                spacing: *spacing,
                                path,
                            },
                        })
                        .collect();
                    let packed = pack_motion_plans(&inputs);
                    let vals: Vec<f64> = packed.iter().map(|v| *v as f64).collect();
                    push_arr_nums(&mut self.res, &vals);
                }
            }
            33 => {
                let order: Vec<usize> = self.players_map.iter().map(|(_, i)| *i).collect();
                let mut hash = 1.0;
                for idx in order {
                    hash += self.p_hash(idx);
                }
                push_ok_num(&mut self.res, hash);
            }
            35 => {
                let isp = self.start_tick.is_none();
                let list = std::mem::take(&mut self.execs_list);
                let mut kept: Vec<usize> = Vec::new();
                for e in &list {
                    let keep = if isp {
                        !self.exec_adsp(*e) || self.exec_active(*e)
                    } else {
                        self.exec_active(*e)
                    };
                    if keep {
                        kept.push(*e);
                    }
                }
                self.execs_list = kept;
                push_ok_undef(&mut self.res);
            }
            36 => {
                let order: Vec<usize> = self.players_map.iter().map(|(_, i)| *i).collect();
                let mut alive = Vec::new();
                for idx in order {
                    if self.p_alive(idx) {
                        alive.push(idx);
                    }
                }
                let arr: Vec<JsVal> = alive.iter().map(|i| JsVal::Num(*i as f64)).collect();
                push_ok_val(&mut self.res, &JsVal::Arr(arr));
            }
            37 => {
                let arr: Vec<JsVal> = self
                    .players_map
                    .iter()
                    .map(|(_, i)| JsVal::Num(*i as f64))
                    .collect();
                push_ok_val(&mut self.res, &JsVal::Arr(arr));
            }
            38 => {
                let mut all: Vec<JsVal> = self
                    .execs_list
                    .iter()
                    .map(|i| JsVal::Num(*i as f64))
                    .collect();
                all.extend(self.uninit_execs.iter().map(|i| JsVal::Num(*i as f64)));
                push_ok_val(&mut self.res, &JsVal::Arr(all));
            }
            39 => {
                let n = c.f() as usize;
                for _ in 0..n {
                    let t = c.f() as usize;
                    self.uninit_execs.push(t);
                }
                push_ok_undef(&mut self.res);
            }
            40 => {
                let t = c.f() as usize;
                self.execs_list.retain(|e| *e != t);
                self.uninit_execs.retain(|e| *e != t);
                push_ok_undef(&mut self.res);
            }
            41 | 42 => {
                let id = c.f();
                let found = self
                    .players_map
                    .iter()
                    .find(|(pid, _)| *pid == id)
                    .map(|(_, i)| *i);
                match found {
                    Some(i) => push_ok_num(&mut self.res, i as f64),
                    None => push_err(
                        &mut self.res,
                        &format!("Player with id {id} not found"),
                    ),
                }
            }
            43 => {
                let id = c.v();
                let order: Vec<usize> = self.players_map.iter().map(|(_, i)| *i).collect();
                let mut found = None;
                for idx in order {
                    let cid = self.p_client_id(idx);
                    if obj_is(&cid, &id) {
                        found = Some(idx);
                        break;
                    }
                }
                match found {
                    Some(i) => push_ok_num(&mut self.res, i as f64),
                    None => push_ok_null(&mut self.res),
                }
            }
            44 => {
                let cell = c.v();
                let x = num_field(&cell, "x");
                let y = num_field(&cell, "y");
                let v = x >= 0.0 && x < self.width && y >= 0.0 && y < self.height;
                push_ok_bool(&mut self.res, v);
            }
            45 => {
                let tile = c.f();
                let x = self.gm().x(tile);
                let y = self.gm().y(tile);
                let mut ns: Vec<f64> = Vec::new();
                for dx in -1..=1 {
                    for dy in -1..=1 {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        let nx = x + dx as f64;
                        let ny = y + dy as f64;
                        if nx >= 0.0 && nx < self.width && ny >= 0.0 && ny < self.height {
                            ns.push(self.gm().tile_ref(nx, ny));
                        }
                    }
                }
                push_arr_nums(&mut self.res, &ns);
            }
            46 => {
                let tile = c.f();
                let mut cb_tr: Vec<f64> = Vec::new();
                self.gm().for_each_neighbor_with_diag(tile, |n| {
                    cb_tr.push(191.0);
                    cb_tr.push(n);
                });
                self.tr.extend(cb_tr);
                push_ok_undef(&mut self.res);
            }
            47 => {
                let a = c.f() as usize;
                let b = c.f() as usize;
                let a_sid = self.p_small_id(a);
                let b_sid = self.p_small_id(b);
                self.add_update(JsVal::Obj(vec![
                    ("type".to_string(), JsVal::Num(UPD_TARGET_PLAYER)),
                    ("playerID".to_string(), JsVal::Num(a_sid)),
                    ("targetID".to_string(), JsVal::Num(b_sid)),
                ]));
                push_ok_undef(&mut self.res);
            }
            48 => {
                let isp = self.start_tick.is_none();
                let v = if isp {
                    true
                } else {
                    let ts = self.ticks_since_start();
                    ts < self.cfg_spawn_immunity()
                };
                push_ok_bool(&mut self.res, v);
            }
            49 => {
                let ts = if let Some(st) = self.start_tick {
                    js_max(0.0, self.ticks - st)
                } else {
                    0.0
                };
                push_ok_num(&mut self.res, ts / 10.0);
            }
            50 => {
                let isp = self.start_tick.is_none();
                let v = if isp {
                    true
                } else {
                    let ts = self.ticks_since_start();
                    ts < self.cfg_nation_spawn_immunity()
                };
                push_ok_bool(&mut self.res, v);
            }
            51 => {
                let ts = self.ticks_since_start();
                push_ok_num(&mut self.res, ts);
            }
            52 => {
                let msg = c.v();
                self.add_update(JsVal::Obj(vec![
                    ("type".to_string(), JsVal::Num(UPD_EMOJI)),
                    ("emoji".to_string(), msg),
                ]));
                push_ok_undef(&mut self.res);
            }
            53 => {
                let winner = c.v();
                let stats = c.v();
                self.set_winner(winner, stats);
                push_ok_undef(&mut self.res);
            }
            54 => {
                let v = self.winner.clone();
                push_ok_val(&mut self.res, &v);
            }
            55 => {
                let p = c.f() as usize;
                let team = jsval_str(&c.v());
                let threshold = num_of(&c.v());
                let v = self.is_eligible_for_team_win(p, &team, threshold);
                push_ok_bool(&mut self.res, v);
            }
            56 => {
                let winner = c.v();
                let v = self.make_winner(&winner);
                match v {
                    Some(x) => push_ok_val(&mut self.res, &x),
                    None => push_ok_undef(&mut self.res),
                }
            }
            57 => {
                let (_, gmode) = self.cfg_game_config();
                if gmode != "Team" {
                    push_ok_val(&mut self.res, &JsVal::Arr(Vec::new()));
                    return;
                }
                let mut teams: Vec<JsVal> = vec![JsVal::Str(self.bot_team.clone())];
                teams.extend(self.player_teams.iter().map(|t| JsVal::Str(t.clone())));
                push_ok_val(&mut self.res, &JsVal::Arr(teams));
            }
            58 => {
                let team = jsval_str(&c.v());
                let order: Vec<usize> = self.players_map.iter().map(|(_, i)| *i).collect();
                let mut total = 0.0;
                for idx in order {
                    let t = self.p_team(idx);
                    if jsval_str(&t) == team {
                        total += self.p_num_tiles(idx);
                    }
                }
                push_ok_num(&mut self.res, total);
            }
            59 => {
                let v = js_max(0.0, self.gm().num_land_tiles() - self.gm().num_tiles_with_fallout());
                push_ok_num(&mut self.res, v);
            }
            60 => {
                let team = jsval_str(&c.v());
                let v = self.team_spawn_area(&team);
                match v {
                    Some(x) => push_ok_val(&mut self.res, &x),
                    None => push_ok_undef(&mut self.res),
                }
            }
            61 => {
                let message = jsval_str(&c.v());
                let mtype = c.v();
                let player_id = c.v();
                let gold = c.v();
                let params = c.v();
                let unit_id = c.v();
                let focus = c.v();
                self.display_message(message, mtype, player_id, gold, params, unit_id, focus);
                push_ok_undef(&mut self.res);
            }
            62 => {
                let message = jsval_str(&c.v());
                let category = jsval_str(&c.v());
                let target = c.v();
                let player_id = c.v();
                let is_from = js_truthy(&c.v());
                let recipient = jsval_str(&c.v());
                let id = if matches!(player_id, JsVal::Null) {
                    JsVal::Null
                } else {
                    let pid = num_of(&player_id);
                    let found = self
                        .players_map
                        .iter()
                        .find(|(k, _)| *k == pid)
                        .map(|(_, i)| *i);
                    match found {
                        Some(i) => JsVal::Num(self.p_small_id(i)),
                        None => {
                            push_err(
                                &mut self.res,
                                &format!("Player with id {pid} not found"),
                            );
                            return;
                        }
                    }
                };
                self.add_update(JsVal::Obj(vec![
                    ("type".to_string(), JsVal::Num(UPD_DISPLAY_CHAT)),
                    ("key".to_string(), JsVal::Str(message)),
                    ("category".to_string(), JsVal::Str(category)),
                    ("target".to_string(), target),
                    ("playerID".to_string(), id),
                    ("isFrom".to_string(), JsVal::Bool(is_from)),
                    ("recipient".to_string(), JsVal::Str(recipient)),
                ]));
                push_ok_undef(&mut self.res);
            }
            63 => {
                let unit_id = c.v();
                let message = jsval_str(&c.v());
                let mtype = c.v();
                let player_id = c.v();
                let pid = num_of(&player_id);
                let found = self
                    .players_map
                    .iter()
                    .find(|(k, _)| *k == pid)
                    .map(|(_, i)| *i);
                let id = match found {
                    Some(i) => JsVal::Num(self.p_small_id(i)),
                    None => {
                        push_err(
                            &mut self.res,
                            &format!("Player with id {pid} not found"),
                        );
                        return;
                    }
                };
                self.add_update(JsVal::Obj(vec![
                    ("type".to_string(), JsVal::Num(UPD_UNIT_INCOMING)),
                    ("unitID".to_string(), unit_id),
                    ("message".to_string(), JsVal::Str(message)),
                    ("messageType".to_string(), mtype),
                    ("playerID".to_string(), id),
                ]));
                push_ok_undef(&mut self.res);
            }
            64 => {
                self.units_version += 1.0;
                push_ok_undef(&mut self.res);
            }
            65 => {
                let v = self.territory_version;
                push_ok_num(&mut self.res, v);
            }
            66 => {
                let uidx = c.f() as usize;
                self.units_version += 1.0;
                self.ug_add_unit(uidx);
                let id = self.u_id(uidx);
                match self.unit_map.iter_mut().find(|(u, _)| *u == id) {
                    Some(slot) => slot.1 = uidx,
                    None => self.unit_map.push((id, uidx)),
                }
                push_ok_undef(&mut self.res);
            }
            67 => {
                let uidx = c.f() as usize;
                self.units_version += 1.0;
                self.ug_remove_unit(uidx);
                let id = self.u_id(uidx);
                self.unit_map.retain(|(u, _)| *u != id);
                let id2 = self.u_id(uidx);
                self.plan_driven.retain(|x| *x != id2);
                let hts = self.u_has_train_station(uidx);
                if hts {
                    self.rail_remove_station(uidx);
                }
                push_ok_undef(&mut self.res);
            }
            68 => {
                let uidx = c.f() as usize;
                self.ug_update_unit_cell(uidx);
                push_ok_undef(&mut self.res);
            }
            69 => {
                let tile = c.f();
                let range = c.f();
                let typ = jsval_str(&c.v());
                let pid = c.v();
                let uc = c.v();
                let v = self.ug_has_unit_nearby(tile, range, &typ, &pid, &uc);
                push_ok_bool(&mut self.res, v);
            }
            70 => {
                let tile = c.f();
                let range = c.f();
                let types = c.v();
                let types_str: Vec<String> = match &types {
                    JsVal::Arr(items) => items.iter().map(jsval_str).collect(),
                    other => vec![jsval_str(other)],
                };
                let pred = c.v();
                let pid = c.v();
                let uc = c.v();
                let v = self.ug_any_unit_nearby(tile, range, &types_str, &pid, &uc);
                let _ = pred;
                push_ok_bool(&mut self.res, v);
            }
            71 => {
                let tile = c.f();
                let range = c.f();
                let types = c.v();
                let pred = c.v();
                let uc = c.v();
                let v = self.ug_nearby_units(tile, range, &types, &pred, &uc);
                push_ok_val(&mut self.res, &v);
            }
            72..=114 => self.map_pass(mid, &mut c),
            117 | 118 => push_ok_num(&mut self.res, 0.0),
            119 => {
                let v = self.water.as_ref().expect("map built").water_graph_version();
                push_ok_num(&mut self.res, v);
            }
            120 => {
                let tile = c.f();
                let v = self.water.as_mut().expect("map built").get_water_component(tile);
                push_opt_num(&mut self.res, v);
            }
            121 => {
                let tile = c.f();
                let comp = c.f();
                let v = self
                    .water
                    .as_mut()
                    .expect("map built")
                    .has_water_component(tile, comp);
                push_ok_bool(&mut self.res, v);
            }
            122 => {
                let tile = c.f();
                let v = self
                    .water
                    .as_mut()
                    .expect("map built")
                    .get_water_component_size(tile);
                push_opt_num(&mut self.res, v);
            }
            123 => {
                let pidx = c.f() as usize;
                let v = self.swc_get(pidx);
                push_ok_val(&mut self.res, &v);
            }
            300 => {
                self.m_execute_next_tick();
                push_ok_num(&mut self.res, 0.0);
            }
            301 => self.m_conquer(&mut c),
            302 => self.m_relinquish(&mut c),
            303 => {
                let tile = c.f();
                self.update_borders(tile);
                push_ok_undef(&mut self.res);
            }
            304 => {
                let tile = c.f();
                self.update_border_status(tile);
                push_ok_undef(&mut self.res);
            }
            305 => self.m_break_alliance(&mut c),
            306 => self.m_expire_alliance(&mut c),
            307 => self.m_remove_alliances_by_player_silently(&mut c),
            308 => {
                let a = c.f() as usize;
                self.detach_alliance(a);
                push_ok_undef(&mut self.res);
            }
            309 => self.m_conquer_player(&mut c),
            310 => {
                let idx = c.f() as usize;
                self.dump_player(idx);
            }
            900 => {
                let v = self.dump_updates();
                push_arr_nums(&mut self.res, &v);
            }
            _ => push_ok_undef(&mut self.res),
        }
    }

    // ------------------------------------------------------- internal methods

    fn record_tile_update(&mut self, tile: f64) {
        let state = self.gm().tile_state(tile).map(to_int32).unwrap_or(0);
        let tb = self.gm().terrain_byte(tile).map(to_int32).unwrap_or(0);
        self.tile_updates.push(tile);
        self.tile_updates.push(((state & 0xffff) | (tb << 16)) as f64);
    }

    fn record_motion_plan(&mut self, r: &JsVal) {
        let kind = jsval_str(&val_field(r, "kind").cloned().unwrap_or(JsVal::Undef));
        // Mirror the TS `switch(record.kind)`: only "grid"/"train" mutate
        // `planDrivenUnitIds`; any other kind falls through with no add, and
        // the record is pushed onto `motionPlanRecords` regardless.
        match kind.as_str() {
            "grid" => {
                let unit_id = num_of(&val_field(r, "unitId").cloned().unwrap_or(JsVal::Undef));
                if !self.plan_driven.contains(&unit_id) {
                    self.plan_driven.push(unit_id);
                }
            }
            "train" => {
                let engine = num_of(&val_field(r, "engineUnitId").cloned().unwrap_or(JsVal::Undef));
                if !self.plan_driven.contains(&engine) {
                    self.plan_driven.push(engine);
                }
                if let Some(JsVal::Arr(cars)) = val_field(r, "carUnitIds") {
                    for u in cars {
                        let v = num_of(u);
                        if !self.plan_driven.contains(&v) {
                            self.plan_driven.push(v);
                        }
                    }
                }
            }
            _ => {}
        }
        self.motion_records.push(decode_plan(r));
    }

    fn ticks_since_start(&mut self) -> f64 {
        if self.start_tick.is_none() {
            return 0.0;
        }
        js_max(0.0, self.ticks - self.start_tick.unwrap())
    }

    fn units_impl(&mut self, first: JsVal, second: JsVal, third: JsVal) -> Vec<usize> {
        let order: Vec<usize> = self.players_map.iter().map(|(_, i)| *i).collect();
        let mut out: Vec<usize> = Vec::new();
        if let JsVal::Arr(items) = &first {
            if !items.is_empty() {
                let set: Vec<String> = items.iter().map(jsval_str).collect();
                for idx in order {
                    let us = self.p_units(idx);
                    for u in us {
                        let t = self.u_type(u);
                        if set.contains(&t) {
                            out.push(u);
                        }
                    }
                }
                return out;
            }
        }
        if matches!(first, JsVal::Undef | JsVal::Absent | JsVal::Arr(_)) {
            for idx in order {
                let us = self.p_units(idx);
                out.extend(us);
            }
            return out;
        }
        let type0 = jsval_str(&first);
        if matches!(second, JsVal::Undef | JsVal::Absent) {
            if let Some((ver, us)) = self
                .units_by_type_memo
                .iter()
                .find(|(k, _)| *k == type0)
                .map(|(_, v)| v.clone())
            {
                if ver == self.units_version {
                    return us;
                }
            }
            for idx in order {
                let us = self.p_units(idx);
                for u in us {
                    let t = self.u_type(u);
                    if t == type0 {
                        out.push(u);
                    }
                }
            }
            let slot = self
                .units_by_type_memo
                .iter_mut()
                .find(|(k, _)| *k == type0);
            match slot {
                Some(s) => s.1 = (self.units_version, out.clone()),
                None => self
                    .units_by_type_memo
                    .push((type0, (self.units_version, out.clone()))),
            }
            return out;
        }
        let t2 = jsval_str(&second);
        let t3 = if matches!(third, JsVal::Undef | JsVal::Absent) {
            None
        } else {
            Some(jsval_str(&third))
        };
        for idx in order {
            let us = self.p_units(idx);
            for u in us {
                let t = self.u_type(u);
                if t == type0 || t == t2 || (t3.is_some() && t == *t3.as_ref().unwrap()) {
                    out.push(u);
                }
            }
        }
        out
    }

    fn unit_count_impl(&mut self, typ: &str) -> f64 {
        if let Some((ver, count)) = self
            .unit_count_memo
            .iter()
            .find(|(k, _)| k == typ)
            .map(|(_, v)| *v)
        {
            if ver == self.units_version {
                return count;
            }
        }
        let order: Vec<usize> = self.players_map.iter().map(|(_, i)| *i).collect();
        let mut total = 0.0;
        for idx in order {
            total += self.p_unit_count(idx, typ);
        }
        let slot = self.unit_count_memo.iter_mut().find(|(k, _)| k == typ);
        match slot {
            Some(s) => s.1 = (self.units_version, total),
            None => self
                .unit_count_memo
                .push((typ.to_string(), (self.units_version, total))),
        }
        total
    }

    fn is_eligible_for_team_win(&mut self, p: usize, team: &str, threshold: f64) -> bool {
        let t = self.p_team(p);
        if jsval_str(&t) != team {
            return false;
        }
        let cid = self.p_client_id(p);
        if matches!(cid, JsVal::Null) {
            return false;
        }
        if !self.p_spawned(p) {
            return false;
        }
        if !self.p_is_disconnected(p) {
            return true;
        }
        let snap = self.p_disconnect_snapshot(p);
        let was_alive = match val_field(&snap, "wasAlive") {
            Some(JsVal::Bool(b)) => *b,
            _ => false,
        };
        if matches!(snap, JsVal::Null) || !was_alive {
            return true;
        }
        let total_land = num_of(&val_field(&snap, "totalLand").cloned().unwrap_or(JsVal::Undef));
        let team_tiles = num_of(&val_field(&snap, "teamTiles").cloned().unwrap_or(JsVal::Undef));
        total_land > 0.0 && 10.0 * team_tiles >= threshold * total_land
    }

    fn make_winner(&mut self, winner: &JsVal) -> Option<JsVal> {
        if let JsVal::Str(team) = winner {
            let threshold = self.cfg_team_land_share();
            let order: Vec<usize> = self.players_map.iter().map(|(_, i)| *i).collect();
            let mut arr: Vec<JsVal> = vec![
                JsVal::Str("team".to_string()),
                JsVal::Str(team.clone()),
            ];
            // Mirror `allPlayers().filter(isEligible).map(clientID)`: every
            // eligibility check runs (in order) before any `clientID()` read.
            let mut eligible: Vec<usize> = Vec::new();
            for idx in order {
                if self.is_eligible_for_team_win(idx, team, threshold) {
                    eligible.push(idx);
                }
            }
            for idx in eligible {
                let cid = self.p_client_id(idx);
                arr.push(cid);
            }
            return Some(JsVal::Arr(arr));
        }
        let idx = num_of(winner) as usize;
        let client_id = self.p_client_id(idx);
        if matches!(client_id, JsVal::Null) {
            let nm = self.p_name(idx);
            Some(JsVal::Arr(vec![
                JsVal::Str("nation".to_string()),
                nm,
            ]))
        } else {
            Some(JsVal::Arr(vec![
                JsVal::Str("player".to_string()),
                client_id,
            ]))
        }
    }

    fn set_winner(&mut self, winner: JsVal, stats: JsVal) {
        self.winner = winner;
        let order: Vec<usize> = self.players_map.iter().map(|(_, i)| *i).collect();
        // `this.players()` materialises the alive filter first (one `isAlive()`
        // per player, in order) before the per-player stats loop runs.
        let mut alive: Vec<usize> = Vec::new();
        for idx in order {
            if self.p_alive(idx) {
                alive.push(idx);
            }
        }
        for idx in alive {
            let n = self.p_num_tiles(idx);
            self.stats_record_final_tiles(idx, n);
            let standing = self.p_alliances(idx);
            let mut longest = 0.0;
            for a in &standing {
                let held = self.ticks - *a;
                if held > longest {
                    longest = held;
                }
            }
            self.stats_record_alliances_at_end(idx, standing.len() as f64, longest);
        }
        let win_val = if matches!(self.winner, JsVal::Null) {
            JsVal::Undef
        } else {
            let w = self.winner.clone();
            self.make_winner(&w).unwrap_or(JsVal::Undef)
        };
        self.add_update(JsVal::Obj(vec![
            ("type".to_string(), JsVal::Num(UPD_WIN)),
            ("winner".to_string(), win_val),
            ("allPlayersStats".to_string(), stats),
        ]));
    }

    fn team_spawn_area(&mut self, team: &str) -> Option<JsVal> {
        if matches!(self.team_spawn_areas, JsVal::Undef | JsVal::Absent | JsVal::Null) {
            return None;
        }
        let num_teams = self.player_teams.len();
        let areas_v = val_field(&self.team_spawn_areas, &num_teams.to_string())?.clone();
        let areas = match areas_v {
            JsVal::Arr(a) => a,
            _ => return None,
        };
        let team_index = self.player_teams.iter().position(|t| t == team)?;
        if team_index >= areas.len() {
            return None;
        }
        Some(areas[team_index].clone())
    }

    #[allow(clippy::too_many_arguments)] // faithful to the TS parameter list
    fn display_message(
        &mut self,
        message: String,
        mtype: JsVal,
        player_id: JsVal,
        gold: JsVal,
        params: JsVal,
        unit_id: JsVal,
        focus: JsVal,
    ) {
        let id = if matches!(player_id, JsVal::Null) {
            JsVal::Null
        } else {
            let pid = num_of(&player_id);
            let found = self
                .players_map
                .iter()
                .find(|(k, _)| *k == pid)
                .map(|(_, i)| *i);
            match found {
                Some(i) => JsVal::Num(self.p_small_id(i)),
                None => {
                    push_err(
                        &mut self.res,
                        &format!("Player with id {pid} not found"),
                    );
                    return;
                }
            }
        };
        let focus_id = if matches!(focus, JsVal::Undef | JsVal::Absent) {
            JsVal::Undef
        } else {
            let pid = num_of(&focus);
            let found = self
                .players_map
                .iter()
                .find(|(k, _)| *k == pid)
                .map(|(_, i)| *i);
            match found {
                Some(i) => JsVal::Num(self.p_small_id(i)),
                None => {
                    push_err(
                        &mut self.res,
                        &format!("Player with id {pid} not found"),
                    );
                    return;
                }
            }
        };
        self.add_update(JsVal::Obj(vec![
            ("type".to_string(), JsVal::Num(UPD_DISPLAY_EVENT)),
            ("messageType".to_string(), mtype),
            ("message".to_string(), JsVal::Str(message)),
            ("playerID".to_string(), id),
            ("goldAmount".to_string(), gold),
            ("params".to_string(), params),
            ("unitID".to_string(), unit_id),
            ("focusPlayerID".to_string(), focus_id),
        ]));
    }

    // ---------------------------------------------------- map pass-throughs

    fn map_pass(&mut self, mid: usize, c: &mut C) {
        match mid {
            72 => {
                let x = c.f();
                let y = c.f();
                let v = self.gm().tile_ref(x, y);
                push_ok_num(&mut self.res, v);
            }
            73 => {
                let r = c.f();
                let v = self.gm().is_valid_ref(r);
                push_ok_bool(&mut self.res, v);
            }
            74 => {
                let r = c.f();
                let v = self.gm().x(r);
                push_ok_num(&mut self.res, v);
            }
            75 => {
                let r = c.f();
                let v = self.gm().y(r);
                push_ok_num(&mut self.res, v);
            }
            76 => {
                let r = c.f();
                let x = self.gm().x(r);
                let y = self.gm().y(r);
                push_ok_val(
                    &mut self.res,
                    &JsVal::Obj(vec![
                        ("x".to_string(), JsVal::Num(x)),
                        ("y".to_string(), JsVal::Num(y)),
                    ]),
                );
            }
            77 => {
                let v = self.gm().width();
                push_ok_num(&mut self.res, v);
            }
            78 => {
                let v = self.gm().height();
                push_ok_num(&mut self.res, v);
            }
            79 => {
                let v = self.gm().num_land_tiles();
                push_ok_num(&mut self.res, v);
            }
            80 => {
                let v = self.gm().water_version();
                push_ok_num(&mut self.res, v);
            }
            81 => {
                let x = c.f();
                let y = c.f();
                let v = self.gm().is_valid_coord(x, y);
                push_ok_bool(&mut self.res, v);
            }
            82 => {
                let r = c.f();
                let v = self.gm().is_land(r);
                push_ok_bool(&mut self.res, v);
            }
            83 => {
                let r = c.f();
                let v = self.gm().is_impassable(r);
                push_ok_bool(&mut self.res, v);
            }
            84 => {
                let r = c.f();
                let v = self.gm().is_ocean_shore(r);
                push_ok_bool(&mut self.res, v);
            }
            85 => {
                let r = c.f();
                let v = self.gm().is_ocean(r);
                push_ok_bool(&mut self.res, v);
            }
            86 => {
                let r = c.f();
                let v = self.gm().is_shoreline(r);
                push_ok_bool(&mut self.res, v);
            }
            87 => {
                let r = c.f();
                let v = self.gm().magnitude(r);
                push_ok_num(&mut self.res, v);
            }
            88 => {
                let r = c.f();
                let v = self.gm().terrain_byte(r);
                push_opt_num(&mut self.res, v);
            }
            89 => {
                let r = c.f();
                self.gm_mut().set_shoreline_bit(r);
                push_ok_undef(&mut self.res);
            }
            90 => {
                let r = c.f();
                self.gm_mut().clear_shoreline_bit(r);
                push_ok_undef(&mut self.res);
            }
            91 => {
                let r = c.f();
                self.gm_mut().set_ocean(r);
                push_ok_undef(&mut self.res);
            }
            92 => {
                let r = c.f();
                let value = c.f();
                self.gm_mut().set_magnitude(r, value);
                push_ok_undef(&mut self.res);
            }
            93 => {
                let r = c.f();
                let v = self.gm().owner_id(r);
                push_ok_num(&mut self.res, v);
            }
            94 => {
                let r = c.f();
                let v = self.gm().has_owner(r);
                push_ok_bool(&mut self.res, v);
            }
            95 => {
                let r = c.f();
                let player_id = c.f();
                self.territory_version += 1.0;
                self.gm_mut().set_owner_id(r, player_id);
                push_ok_undef(&mut self.res);
            }
            96 => {
                let r = c.f();
                let v = self.gm().has_fallout(r);
                push_ok_bool(&mut self.res, v);
            }
            97 => {
                let r = c.f();
                let v = self.gm().is_border(r);
                push_ok_bool(&mut self.res, v);
            }
            98 => {
                let r = c.f();
                let v = self.gm().neighbors(r);
                push_arr_nums(&mut self.res, &v);
            }
            99 => {
                let tile = c.f();
                let mut cb_tr: Vec<f64> = Vec::new();
                self.gm().for_each_neighbor(tile, |n| {
                    cb_tr.push(190.0);
                    cb_tr.push(n);
                });
                self.tr.extend(cb_tr);
                push_ok_undef(&mut self.res);
            }
            100 | 101 => {
                let r = c.f();
                let n = if let JsVal::Arr(items) = c.v() {
                    items.len()
                } else {
                    0
                };
                let mut out = vec![0.0; n];
                let count = if mid == 100 {
                    self.gm().neighbors4(r, &mut out)
                } else {
                    self.gm().neighbors8(r, &mut out)
                };
                // TS returns the count and the caller keeps the mutated buffer;
                // the capture echoes `count` followed by the written cells.
                let mut res = vec![count as f64];
                res.extend(out.iter().take(count).copied());
                push_arr_nums(&mut self.res, &res);
            }
            102 => {
                let r = c.f();
                let v = self.gm().is_water(r);
                push_ok_bool(&mut self.res, v);
            }
            103 => {
                let r = c.f();
                let v = self.gm().is_shore(r);
                push_ok_bool(&mut self.res, v);
            }
            104 => {
                let r = c.f();
                let v = self.gm().cost(r);
                push_ok_num(&mut self.res, v);
            }
            105 => {
                let r = c.f();
                let v = terrain_num(self.gm().terrain_type(r));
                push_ok_num(&mut self.res, v);
            }
            106 => {
                let mut cb_tr: Vec<f64> = Vec::new();
                self.gm().for_each_tile(|t| {
                    cb_tr.push(192.0);
                    cb_tr.push(t);
                });
                self.tr.extend(cb_tr);
                push_ok_undef(&mut self.res);
            }
            107 => {
                let a = c.f();
                let b = c.f();
                let v = self.gm().manhattan_dist(a, b);
                push_ok_num(&mut self.res, v);
            }
            108 => {
                let a = c.f();
                let b = c.f();
                let v = self.gm().euclidean_dist_squared(a, b);
                push_ok_num(&mut self.res, v);
            }
            109 => {
                let tile = c.f();
                let radius = c.f();
                let ret = self.ug.circle_filter;
                let cb_tr = RefCell::new(Vec::<f64>::new());
                let res = self.gm().circle_search(tile, radius, |t, d2| {
                    let mut b = cb_tr.borrow_mut();
                    b.push(193.0);
                    b.push(t);
                    b.push(d2);
                    b.push(if ret { 1.0 } else { 0.0 });
                    ret
                });
                self.tr.extend(cb_tr.into_inner());
                push_arr_nums(&mut self.res, &res);
            }
            110 => {
                let tile = c.f();
                let ret = self.ug.bfs_filter;
                let cb_tr = RefCell::new(Vec::<f64>::new());
                let res = {
                    let filter = |_m: &GameMap, t: f64| -> bool {
                        let mut b = cb_tr.borrow_mut();
                        b.push(194.0);
                        b.push(t);
                        b.push(if ret { 1.0 } else { 0.0 });
                        ret
                    };
                    self.gm().bfs(tile, &filter)
                };
                self.tr.extend(cb_tr.into_inner());
                push_arr_nums(&mut self.res, &res);
            }
            111 => {
                let tile = c.f();
                let v = self.gm().tile_state(tile).unwrap_or(f64::NAN);
                push_ok_num(&mut self.res, v);
            }
            112 => {
                let buf: Vec<f64> = self
                    .gm()
                    .tile_state_buffer()
                    .iter()
                    .map(|v| *v as f64)
                    .collect();
                push_arr_nums(&mut self.res, &buf);
            }
            113 => {
                let tile = c.f();
                let state = c.f();
                let v = self.gm_mut().update_tile(tile, state);
                push_ok_bool(&mut self.res, v);
            }
            114 => {
                let v = self.gm().num_tiles_with_fallout();
                push_ok_num(&mut self.res, v);
            }
            _ => push_ok_undef(&mut self.res),
        }
    }

    // ---------------------------------------------------------- dumpUpdates

    fn dump_updates(&self) -> Vec<f64> {
        let mut parts: Vec<f64> = vec![UPDATE_TYPE_COUNT as f64];
        for t in 0..UPDATE_TYPE_COUNT {
            parts.push(t as f64);
            parts.push(self.updates[t].len() as f64);
            for u in &self.updates[t] {
                norm_val(u, &mut parts);
            }
        }
        parts
    }
}

/// `gmNorm` — flatten a stored update [`JsVal`] into primitive tokens.
fn norm_val(v: &JsVal, out: &mut Vec<f64>) {
    match v {
        JsVal::Absent | JsVal::Undef => out.push(1.0),
        JsVal::Null => out.push(2.0),
        JsVal::Num(n) => {
            out.push(3.0);
            match uenc_val(*n) {
                JsVal::Str(s) => {
                    out.push(5.0);
                    out.extend(s.encode_utf16().map(|u| u as f64));
                }
                JsVal::Num(x) => out.push(x),
                _ => {}
            }
        }
        JsVal::Bool(b) => {
            out.push(4.0);
            out.push(if *b { 1.0 } else { 0.0 });
        }
        JsVal::Str(s) => {
            out.push(5.0);
            out.push(s.encode_utf16().count() as f64);
            out.extend(s.encode_utf16().map(|u| u as f64));
        }
        JsVal::Arr(items) => {
            out.push(7.0);
            out.push(items.len() as f64);
            for it in items {
                norm_val(it, out);
            }
        }
        JsVal::Obj(fields) => {
            out.push(6.0);
            out.push(fields.len() as f64);
            for (k, val) in fields {
                out.push(5.0);
                out.push(k.encode_utf16().count() as f64);
                out.extend(k.encode_utf16().map(|u| u as f64));
                norm_val(val, out);
            }
        }
    }
}

fn decode_plan(v: &JsVal) -> GmPlan {
    let kind = jsval_str(&val_field(v, "kind").cloned().unwrap_or(JsVal::Undef));
    if kind == "grid" {
        GmPlan::Grid {
            unit_id: num_of(&val_field(v, "unitId").cloned().unwrap_or(JsVal::Undef)),
            plan_id: num_of(&val_field(v, "planId").cloned().unwrap_or(JsVal::Undef)),
            start_tick: num_of(&val_field(v, "startTick").cloned().unwrap_or(JsVal::Undef)),
            ticks_per_step: num_of(&val_field(v, "ticksPerStep").cloned().unwrap_or(JsVal::Undef)),
            path: nums_of(val_field(v, "path")),
        }
    } else {
        GmPlan::Train {
            engine_unit_id: num_of(&val_field(v, "engineUnitId").cloned().unwrap_or(JsVal::Undef)),
            car_unit_ids: nums_of(val_field(v, "carUnitIds")),
            plan_id: num_of(&val_field(v, "planId").cloned().unwrap_or(JsVal::Undef)),
            start_tick: num_of(&val_field(v, "startTick").cloned().unwrap_or(JsVal::Undef)),
            speed: num_of(&val_field(v, "speed").cloned().unwrap_or(JsVal::Undef)),
            spacing: num_of(&val_field(v, "spacing").cloned().unwrap_or(JsVal::Undef)),
            path: nums_of(val_field(v, "path")),
        }
    }
}

fn nums_of(v: Option<&JsVal>) -> Vec<f64> {
    match v {
        Some(JsVal::Arr(items)) => items.iter().map(num_of).collect(),
        _ => Vec::new(),
    }
}

/// JS `Number(v)` for a codec value (`Null`/`Undef`/`bool` per ToNumber).
fn num_of(v: &JsVal) -> f64 {
    match v {
        JsVal::Num(n) => *n,
        JsVal::Null => 0.0,
        JsVal::Undef | JsVal::Absent => f64::NAN,
        JsVal::Bool(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        JsVal::Str(_) | JsVal::Obj(_) | JsVal::Arr(_) => f64::NAN,
    }
}

fn num_field(v: &JsVal, key: &str) -> f64 {
    val_field(v, key).map(num_of).unwrap_or(f64::NAN)
}

/// JS truthiness for a codec value.
fn js_truthy(v: &JsVal) -> bool {
    match v {
        JsVal::Absent | JsVal::Undef | JsVal::Null => false,
        JsVal::Bool(b) => *b,
        JsVal::Num(n) => *n != 0.0 && !n.is_nan(),
        JsVal::Str(s) => !s.is_empty(),
        JsVal::Obj(_) | JsVal::Arr(_) => true,
    }
}

/// JS `Object.is` for the codec domain (string `-0`/`n`/`i` tokens compare by
/// value, numbers by `Object.is`).
fn obj_is(a: &JsVal, b: &JsVal) -> bool {
    match (a, b) {
        (JsVal::Num(x), JsVal::Num(y)) => x == y && x.is_sign_negative() == y.is_sign_negative(),
        (JsVal::Null, JsVal::Null) => true,
        (JsVal::Undef, JsVal::Undef) | (JsVal::Absent, JsVal::Absent) => true,
        (JsVal::Bool(x), JsVal::Bool(y)) => x == y,
        (JsVal::Str(x), JsVal::Str(y)) => x == y,
        _ => false,
    }
}

fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a > b {
        a
    } else {
        b
    }
}

fn terrain_num(t: TerrainType) -> f64 {
    match t {
        TerrainType::Plains => 0.0,
        TerrainType::Highland => 1.0,
        TerrainType::Mountain => 2.0,
        TerrainType::Ocean => 3.0,
        TerrainType::Impassable => 4.0,
    }
}
