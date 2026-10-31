//! G6-0 (M1-0) — integration harness (`game_loop`): the real-assembly layer
//! the M1 milestone grows into the tick-loop parity rig.
//!
//! Unlike the G4a/G4b `game_impl` harness (scripted facades + op-by-op
//! traces), this module assembles REAL ported components — [`GameMap`] and
//! [`WaterManager`], with the map living inside the water manager exactly as
//! the G4a `debug_map` convention — and compares aggregate OUTPUT streams.
//! The recorder (`tools/gen_m1_vectors.mjs`) drives the identical real TS
//! classes (`GameMapImpl` + `WaterManager`) over the same synthetic terrain
//! bytes and pins the whole per-op result stream. Both the native test
//! (`tests/parity_m1.rs`) and the wasm scan (`tools/m1_scan.mjs`) replay it
//! bit-for-bit.
//!
//! M1a extended the construct with the real Config surface ([`LoopConfig`])
//! and real players ([`LoopPlayer`]); M1b adds the real `executeNextTick`
//! body (the G4b mid-300 script-facade version stays frozen). Nothing here
//! traces facade calls — the comparison payload is state digests and packed
//! update drains.
//!
//! ## Op table (`run_op(kind, args)`)
//!
//! | kind | op | args | result payload |
//! |------|----|------|----------------|
//! | 0 | construct | `w, h, numLand, miniW, miniH, miniNumLand, terrainLen, ..terrain, miniLen, ..mini` | `[]` |
//! | 1 | `setFallout` (TS 230-240) | `tile, value` | `[]` |
//! | 2 | `setWater` (TS 242-254) | `tile` | `[]` |
//! | 3 | `queueWaterConversion` (TS 256-266) | `tile` | `[]` |
//! | 4 | water tick | `ticks` | `n, ..changedTiles` |
//! | 5 | drain packed tile updates | — | `len, ..pairs` |
//! | 6 | map digest | — | `territoryVersion, ticks, numLand, waterVersion, falloutCount, waterGraphVersion, ..perTilePacked` |
//! | 7 | `construct_game` (M1a: `createGame` TS 55-74 + ctor 118-151 + `populateTeams` 155 / `addPlayers` 163 / `addPlayer` 680 / `maybeAssignTeam` 694) | op-0 terrain prefix, then config codes, then humans/nations `PlayerInfo` runs | `[]` or throw |
//! | 8 | game snapshot (M1a) | — | `nextPlayerID, ..botTeam(encS), teamCount, ..teams(encS), playerCount, ..perPlayer` |
//! | 9 | `executeNextTick` (M1b: TS 488-546) | — | `ticksAfter, gameHash, updatesJSON(encS)` |
//! | 10 | stats snapshot (M1b: `StatsImpl.data`) | — | `entryCount, ..perEntry(cid encS, tiles opt, peakTroops opt, alliances opt)` |
//! | 11 | `PlayerImpl.setSpawnTile` (TS 726-728) | `pidx, tile` | `[]` |
//! | 12 | `PlayerImpl.addTroops` (TS 1345-1359) | `pidx, amount` | `[]` |
//! | 13 | `GameImpl.conquer` (TS 761-783) | `pidx, tile` | `[]` or throw |
//! | 14 | `drainPackedPlayerUpdates` (TS 567-573) | — | `-1` when null, else `len, ..quads` (after the `[0]` ok tag) |
//! | 15 | `drainPackedAttackUpdates` (TS 575-581) | — | same shape as 14 |
//! | 16 | `endSpawnPhase` (M1c: TS 473-482) | — | `startTick, updatesJSON(encS)` — the CURRENT map (ctor-fresh or the previous tick's leftovers) plus the appended SpawnPhaseEnd |
//! | 17 | spawn-immunity probe (M1c: TS 921-945) | — | `inSpawn, ticksSinceStart, elapsedGameSeconds, isSpawnImmunityActive, isNationSpawnImmunityActive` (booleans 0/1) |
//! | 18 | `addExecution` (M1d stub spec) | `adsp, bornDead, killAt, initUpdate, tickUpdate` | `[]` |
//! | 19 | `executions()` (M1d: TS 661-663) | — | `nExecs, nUnInit, ..per exec (id, dead?1:0)` |
//! | 20 | `PlayerImpl.createAttack` (M1d: TS 1855-1875) | `pidx, targetPidx(-1=TerraNullius), troops` | `[]` |
//! | 21 | `relinquish` (M1e: TS 785-803) | `tile` | `[]` or throw |
//! | 22 | `createAllianceRequest` (M1e: TS 371-399) | `reqPidx, recPidx` | `null?(1|0), (liveReqIdx), updatesJSON(encS)` |
//! | 23 | `accept/rejectAllianceRequest` (M1e: TS 401-451) | `reqIdx, accept(0|1)` | `updatesJSON(encS)` or throw |
//! | 24 | `breakAlliance` (M1e: TS 836-870) | `breakerPidx, allianceIdx` | `updatesJSON(encS)` or throw |
//! | 25 | `expireAlliance` (M1e: TS 872-893) | `allianceIdx` | `updatesJSON(encS)` or throw |
//! | 26 | `removeAlliancesByPlayerSilently` (M1e: TS 895-911) | `pidx` | `[]` |
//! | 27 | alliance snapshot | — | `nReq, ..(reqPidx, recPidx, createdAt)*, nAll, ..(id, reqPidx, recPidx, createdAt, expiresAt)*, nextAllianceID, ..per player (nToks, ..toks*, nPast, ..(recPidx, createdAt)*, markedTraitorTick, betrayalCount, traitorRemaining)*` |
//! | 28 | nearby probe (M1e: `sharesBorderWith` + `nearby()`) | `pidx` | `nOthers, ..sharesBorder(0|1)*, nNearby, ..smallIDs*` |
//! | 29 | `UNIT_DEFINE` (M1e: `UnitImpl` ctor) | `id, type(str), tile, ownerPidx(-1=TerraNullius), active, uc` | `unitIdx` |
//! | 30 | `GameImpl.addUnit` (M1e: TS 1135-1139) | `unitIdx` | `[]` |
//! | 31 | `GameImpl.removeUnit` (M1e: TS 1140-1148) | `unitIdx` | `[]` |
//! | 32 | `unit move` (M1e: `_lastTile = _tile; _tile = newTile; updateUnitCell`) | `unitIdx, newTile` | `[]` |
//! | 33 | unit-nearby probe (M1e: `UnitGrid` scans) | `mode(0=array/1=has/2=any/3=scalar), tile, range, nTypes, ..types(str)*, pidMode, (pid pidx), incUC` | mode 0/3: `n, ..(unitIdx, d2)*`; mode 1/2: `0|1` |
//! | 34 | units snapshot | — | `unitsVersion, n, ..(id, type(str), tile, inMap?1:0)*` |
//!
//! Result convention: `[0, ..payload]` for a return / `undefined`, or
//! `[1, ..encS(message)]` for a throw — same tag pair as `game_impl`. Error
//! strings are the verbatim TS messages. Payloads are RAW numbers (no JsVal
//! wrapping): the M1-0 comparison domain is flat numeric streams. Strings
//! ride the `encS` runs (`[len, ..utf16]`) of `js_json::read_str`.
//!
//! ## M1-0 pins (kind 0-6 flows only)
//!
//! * Kind-0 `construct` installs no `Config`, so `queueWaterConversion`
//!   (op 3) runs with `config.waterNukes()` PINNED `true` (the
//!   `!waterNukes()` fallback arm is exactly op 1 with `value=1`).
//! * No players exist yet, so the `hasOwner` throw arms of ops 1-3 are
//!   unreachable — their `hasOwner` reads still run against the real map
//!   (always false) and the arms arrive with the M1a players.
//!
//! ## M1a (kinds 7-8)
//!
//! Kind 7 assembles the real game: the real [`GameMap`] / [`WaterManager`]
//! exactly like op 0 (plus the real `disableNavMesh` value), the real
//! `populateTeams` -> [`resolve_teams_list`], and the real `addPlayers`
//! three-path chain over [`assign_teams`] (FFA / HumansVsNations / Team with
//! clan+friend balancing and the `"kicked"` skip). Every player is a real
//! [`LoopPlayer`]: `startManpower`/`startingGold` mirrored line-for-line from
//! Config.ts (1000-1019 / 436-441 / 743-750), `troops = toInt(startTroops)`,
//! and the per-player `PseudoRandom::new(simpleHash(id))` seed. Kind 8 dumps
//! the construction-observable state: `nextPlayerID`, the team list, and per
//! player the identity, team, troops/gold and one RNG draw. `maybeAssignTeam`
//! is ported with its Bot / `simpleHash(id) % len` branches, though in the
//! ctor flows it is only reachable through the FFA path (where it returns
//! `null` before the hash branch) — the later-join callers arrive with M1c+.

use crate::game_map::GameMap;
use crate::game_ts::format_player_display_name;
use crate::js_json::{push_str, read_str, JsonValue, json_stringify};
use crate::pseudo_random::PseudoRandom;
use crate::stats_impl::{MP, StatsImpl, Val};
use crate::team_assignment::{
    assign_teams, resolve_teams_list, Assignment, PlayerType, ResolveTeamsError, TeamCountConfig,
};
use crate::util::simple_hash;
use crate::water_manager::WaterManager;

/// The `Config` surface GameImpl's construction chain reads, as a plain
/// struct (the `config.rs` Config harness drives the full class op-by-op;
/// the loop rig needs only these construction-time reads, mirrored
/// line-for-line from Config.ts).
pub struct LoopConfig {
    /// `config.gameConfig().gameMode === GameMode.Team` (Game.ts 137-140).
    pub game_mode_team: bool,
    /// `config.playerTeams()` (Config.ts 386: `gameConfig.playerTeams ?? 0`).
    pub player_teams: TeamCountConfig,
    /// `config.disableNavMesh()` (Config.ts 404).
    pub disable_nav_mesh: bool,
    /// `config.waterNukes()` (Config.ts 415).
    pub water_nukes: bool,
    /// `gameConfig.difficulty` (Game.ts 77-82) as the startManpower index.
    pub difficulty: u8,
    /// `gameConfig.startingGold ?? 0` (Config.ts 744).
    pub starting_gold: Option<f64>,
    /// `gameConfig.hostCheats?.startingGold` (Config.ts 745-748).
    pub hc_starting_gold: Option<f64>,
    /// `gameConfig.hostCheats?.infiniteTroops ?? false` (Config.ts 718).
    pub hc_infinite_troops: bool,
    /// `gameConfig.infiniteTroops ?? false` (Config.ts 421).
    pub infinite_troops: bool,
    /// `gameConfig.spawnImmunityDuration ?? 50` (Config.ts 332-334; the
    /// explicit-0 override is the `Some(0.0)` case, `None` is absent).
    pub spawn_immunity_duration: Option<f64>,
    /// `config.allianceDuration()` (Config.ts 810-816):
    /// `customAllianceDuration > 0 ? m * 600 : 3000`.
    pub alliance_duration: f64,
}

/// The `PlayerInfo` fields the construction chain reads (Game.ts 436-460;
/// the recorder feeds plain objects with the same property names).
#[derive(Clone)]
pub struct LoopPlayerInfo {
    pub id: String,
    pub name: String,
    pub player_type: PlayerType,
    pub client_id: Option<String>,
    pub is_lobby_creator: bool,
    pub clan_tag: Option<String>,
    pub friends: Vec<String>,
    pub team_index: Option<f64>,
    /// `nationFlag` (M1b: rides the full PlayerUpdate JSON).
    pub nation_flag: Option<String>,
}

impl LoopPlayerInfo {
    /// The `team_assignment::PlayerInfo` subset `assignTeams` reads.
    fn ta(&self) -> crate::team_assignment::PlayerInfo {
        crate::team_assignment::PlayerInfo {
            id: self.id.clone(),
            player_type: match self.player_type {
                PlayerType::Bot => crate::team_assignment::PlayerType::Bot,
                PlayerType::Human => crate::team_assignment::PlayerType::Human,
                PlayerType::Nation => crate::team_assignment::PlayerType::Nation,
            },
            client_id: self.client_id.clone(),
            clan_tag: self.clan_tag.clone(),
            friends: self.friends.clone(),
            team_index: self.team_index,
        }
    }
}

/// One real constructed player: the `PlayerImpl` ctor (TS 191-201) semantics
/// — `_troops = toInt(startTroops)`, `_gold = mg.config().startingGold(...)`,
/// `_pseudo_random = new PseudoRandom(simpleHash(id))` — plus the identity
/// and team `addPlayer` (TS 680-692) assigns, and the M1b live state the
/// tick loop reads (`_tiles` / `_borderTiles` / `_spawnTile` /
/// `_lastTileChange` / `_tileChangeVersion` / `lastSentUpdate`).
pub struct LoopPlayer {
    pub info: LoopPlayerInfo,
    pub small_id: f64,
    pub team: Option<String>,
    /// `playerInfo.displayName` — the ctor's bracketed-clan-tag formatting.
    pub display_name: String,
    pub troops: f64,
    pub gold: f64,
    pub rng: PseudoRandom,
    pub rng_draws: u32,
    /// `_tiles` — owned tile refs (insertion order, like the TileSet).
    pub tiles: Vec<f64>,
    /// `_borderTiles`.
    pub border_tiles: Vec<f64>,
    /// `_spawnTile` — `None` is JS `undefined` (`hasSpawned()` false).
    pub spawn_tile: Option<f64>,
    pub tile_change_version: f64,
    pub last_tile_change: f64,
    /// `lastSentUpdate` — the diffable core of the previous full update
    /// (`None` until the first `toUpdate`).
    last_sent: Option<PuCore>,
    /// `_outgoingAttacks` — indices into the harness `attacks` table.
    pub out_attacks: Vec<usize>,
    /// `_incomingAttacks` — indices into the harness `attacks` table. The
    /// `attacker().isAlive()` filter applies at emission time (TS
    /// `incomingAttacks()`), so the raw list outlives its serialized form.
    pub in_attacks: Vec<usize>,
    /// `_alliances` — indices into the harness `alliances` table (M1e).
    pub alliances: Vec<usize>,
    /// `pastOutgoingAllianceRequests` — `(recipient pidx, request createdAt)`
    /// pairs (M1e; the request identity fields the snapshot dumps).
    pub past_outgoing: Vec<(usize, f64)>,
    /// `markedTraitorTick` — `-1` never marked (M1e).
    pub marked_traitor_tick: f64,
    /// `_betrayalCount` (M1e).
    pub betrayal_count: f64,
    /// `nearbyMemo` — `(territoryVersion, waterVersion, result smallIDs)`,
    /// `None` until the first `nearby()` (M1e).
    pub nearby_memo: Option<(f64, f64, Vec<f64>)>,
}

/// One live `AllianceRequestImpl` (M1e): the `GameImpl.allianceRequests`
/// array is the single source of truth (`incoming`/`outgoing` filter it).
struct LoopAllianceRequest {
    requestor: usize,
    recipient: usize,
    created_at: f64,
}

/// One live `AllianceImpl` (M1e). `created_at` is the ACCEPT tick (`new
/// AllianceImpl(this, ..., this._ticks, ...)` — not the request's creation
/// tick, which only rides on the past-outgoing entries).
struct LoopAlliance {
    id: f64,
    requestor: usize,
    recipient: usize,
    created_at: f64,
    /// `createdAt + config.allianceDuration()`.
    expires_at: f64,
}

/// One defined unit (M1e): the ctor-observable `UnitImpl` state the loop
/// domain drives. `owner` is the owner's smallID (TerraNullius = 0);
/// `in_map` mirrors `unitMap.has(id)` (addUnit/removeUnit toggle it).
pub struct LoopUnit {
    id: f64,
    utype: String,
    tile: f64,
    last_tile: f64,
    owner: f64,
    active: bool,
    uc: bool,
    in_map: bool,
}

/// The `AllianceView` serialization of a live alliance (PlayerImpl.ts
/// 296-308 map): `{id, other, createdAt, expiresAt, hasExtensionRequest}`,
/// where `other` is the other player's `info.id` string and
/// `hasExtensionRequest = expiresAt <= ticks + allianceExtensionPromptOffset`
/// (the constant 300, Config.ts 1279-1281).
#[derive(Clone, PartialEq)]
struct AllianceWire {
    id: f64,
    other: String,
    created_at: f64,
    expires_at: f64,
    has_ext: bool,
}

/// `target.isPlayer()` discriminator for [`LoopAttack`] — TerraNullius is the
/// shared singleton (`TerraNulliusImpl.ts`, `smallID()` 0).
enum AttackTarget {
    Player(usize),
    TerraNullius,
}

/// One live `AttackImpl`. Entries are immutable in the M1 domain (no attack
/// execution: no `setTroops` / `orderRetreat` / `delete`), so the same attack
/// reachable from two players' lists is one table entry and index-sequence
/// equality is exactly `attackArrayMembershipEqual`'s value equality
/// (attackerID / targetID / id / retreating — troops excluded).
struct LoopAttack {
    attacker: usize,
    target: AttackTarget,
    troops: f64,
    id: String,
    /// `_retreating` — never flipped in this domain.
    retreating: bool,
}

/// The recorder-side stub-exec spec (M1d): the orchestration is the test
/// object, so both sides replay the same parameterized behavior. `kill_at`
/// flips `dead` inside `tick()` AFTER the update fires (the dying tick still
/// emits); `init` ignores `dead` entirely — that gate lives in the tick loop.
struct ExecSpec {
    adsp: bool,
    kill_at: f64,
    init_update: bool,
    tick_update: bool,
}

/// One deterministic stub `Execution`: `isActive()` is `!dead`, the update
/// payload is a DisplayEvent whose message carries the ticks argument (the
/// same pre-increment clock both sides pass into init/tick).
struct StubExec {
    id: f64,
    spec: ExecSpec,
    dead: bool,
}

impl StubExec {
    fn emit_update(o: &mut Vec<JsonValue>, msg: String) {
        o.push(JsonValue::Obj(vec![
            ("type".to_string(), JsonValue::Num(3.0)),
            ("message".to_string(), JsonValue::Str(msg)),
            ("messageType".to_string(), JsonValue::Num(0.0)),
            ("playerID".to_string(), JsonValue::Null),
        ]));
    }
}

/// The slice of a full `PlayerUpdate` that can change in the M1b domain.
/// Everything else `toFullUpdate` emits is constant per player (identity /
/// config-driven fields) or an always-empty collection, so `diffPlayerUpdate`
/// can only ever surface these fields — the diff compares exactly these, in
/// the `setIfDifferent` order (isAlive 100 / hasSpawned 123 / spawnTile 124),
/// and the quint compare uses the four stat fields (TS 230-247).
#[derive(Clone, PartialEq)]
struct PuCore {
    is_alive: bool,
    has_spawned: bool,
    spawn_tile: Option<f64>,
    tiles_owned: f64,
    gold: f64,
    troops: f64,
    gold_earned: f64,
    /// The SERIALIZED attack memberships at emission time — outgoing unfiltered,
    /// incoming already passed through the `attacker().isAlive()` filter (the
    /// diff and the troop-delta packing both compare these post-filter arrays).
    out_attacks: Vec<usize>,
    in_attacks: Vec<usize>,
    // M1e diplomatic fields (the remaining setIfDifferent-comparable state):
    is_traitor: bool,
    traitor_remaining: f64,
    betrayals: f64,
    /// `allies` — the other side's smallIDs, alliance insertion order.
    allies: Vec<f64>,
    /// `outgoingAllianceRequests` — the recipient `info.id` strings of the
    /// live requests this player sent.
    out_all_reqs: Vec<String>,
    /// `alliances` — the serialized `AllianceView`s.
    alliances: Vec<AllianceWire>,
}

pub struct GameLoopHarness {
    water: Option<WaterManager>,
    territory_version: f64,
    ticks: f64,
    tile_updates: Vec<f64>,
    config: Option<LoopConfig>,
    players: Vec<LoopPlayer>,
    next_player_id: f64,
    player_teams: Vec<String>,
    /// The game's own `StatsImpl` (`_stats`); real `recordTickSample` path.
    stats: StatsImpl,
    /// `MP` facades parallel to `players` (the stats methods' player table).
    mp: Vec<MP>,
    /// `playerStatsQuads` — `[smallID, tilesOwned, gold, troops, goldEarned]`.
    player_quads: Vec<f64>,
    /// `attackTroopsQuads` — `[smallID, direction, index, troops]`. Empty in
    /// this domain even with live attacks: entries are immutable, so the
    /// membership-equal delta loops never see a troop change.
    attack_quads: Vec<f64>,
    /// The live `AttackImpl` table (`GameImpl` holds no such table; attacks
    /// live on the players' out/in lists — the table is the identity-sharing
    /// mirror of that).
    attacks: Vec<LoopAttack>,
    /// `GameImpl.startTick` — `None` is the spawn phase (`inSpawnPhase()`
    /// true); `endSpawnPhase` stamps it with `_ticks` (M1c).
    start_tick: Option<f64>,
    /// The live `GameImpl.updates` map — ctor-fresh at construct, rebuilt by
    /// every `executeNextTick`, and appended to in place by `endSpawnPhase`
    /// (the next tick's `createGameUpdatesMap` wipes it, M1c).
    last_updates: JsonValue,
    /// `GameImpl.execs` — initialized executions (M1d).
    execs: Vec<StubExec>,
    /// `GameImpl.unInitExecs` — added but not yet init'd executions.
    un_init_execs: Vec<StubExec>,
    /// Stub-exec identity counter (mirrors the recorder's `nextExecId`).
    next_exec_id: f64,
    /// `GameImpl.allianceRequests` — the live request tokens (M1e). A
    /// request's identity is its append-only ledger position (the wire
    /// token): TS filters by object identity, so a stale accept — the object
    /// already removed — mirrors as a no-op retain, and the object's fields
    /// stay readable through the ledger.
    alliance_requests: Vec<usize>,
    /// Every request ever created, indexed by token (append-only).
    request_ledger: Vec<LoopAllianceRequest>,
    /// The live `AllianceImpl` table (M1e; players hold indices into it).
    alliances: Vec<LoopAlliance>,
    /// `GameImpl.nextAllianceID`.
    next_alliance_id: f64,
    /// The defined units (M1e) — indices are the unit identities the grid
    /// stores and the ops exchange.
    units: Vec<LoopUnit>,
    /// The real `UnitGrid` over an independent `GameMap` copy (M1e; `GameMap`
    /// is not `Clone`, so the grid's map is built alongside the water
    /// manager's from the same terrain bytes — x/y behavior is identical).
    unit_grid: Option<crate::unit_grid::UnitGrid>,
    /// `GameImpl._unitsVersion`.
    units_version: f64,
}

impl Default for GameLoopHarness {
    fn default() -> Self {
        Self {
            water: None,
            territory_version: 0.0,
            ticks: 0.0,
            tile_updates: Vec::new(),
            config: None,
            players: Vec::new(),
            next_player_id: 1.0,
            player_teams: Vec::new(),
            stats: StatsImpl::default(),
            mp: Vec::new(),
            player_quads: Vec::new(),
            attack_quads: Vec::new(),
            attacks: Vec::new(),
            start_tick: None,
            last_updates: empty_updates(),
            execs: Vec::new(),
            un_init_execs: Vec::new(),
            next_exec_id: 0.0,
            alliance_requests: Vec::new(),
            request_ledger: Vec::new(),
            alliances: Vec::new(),
            next_alliance_id: 0.0,
            units: Vec::new(),
            unit_grid: None,
            units_version: 0.0,
        }
    }
}

/// Flat arg cursor over one op's `args` (mirrors the `C` helper of the other
/// harnesses).
struct C<'a> {
    a: &'a [f64],
    i: usize,
}

impl<'a> C<'a> {
    fn f(&mut self) -> f64 {
        let v = self.a[self.i];
        self.i += 1;
        v
    }
    fn done(&self) -> bool {
        self.i >= self.a.len()
    }
}

impl GameLoopHarness {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// The shared real map (owned by the water manager, G4a convention).
    fn gm(&self) -> &GameMap {
        self.water.as_ref().expect("map built").debug_map()
    }
    fn gm_mut(&mut self) -> &mut GameMap {
        self.water.as_mut().expect("map built").debug_map_mut()
    }

    /// `GameImpl.recordTileUpdate` (TS 548-555): low 16 bits tile state,
    /// bits 16-23 terrain byte.
    fn record_tile_update(&mut self, tile: f64) {
        let state = self.gm().tile_state(tile).expect("valid ref") as u16;
        let terrain = self.gm().terrain_byte(tile).expect("valid ref") as u16;
        self.tile_updates.push(tile);
        self.tile_updates
            .push(((state as u32) | ((terrain as u32) << 16)) as f64);
    }

    /// `GameImpl.setFallout` (TS 230-240).
    fn op_set_fallout(&mut self, tile: f64, value: bool, o: &mut Vec<f64>) {
        if value && self.gm().has_owner(tile) {
            push_err(o, &format!("cannot set fallout, tile {tile} has owner"));
            return;
        }
        if self.gm().has_fallout(tile) {
            o.push(0.0);
            return;
        }
        self.territory_version += 1.0;
        self.gm_mut().set_fallout(tile, value);
        self.record_tile_update(tile);
        o.push(0.0);
    }

    /// `GameImpl.setWater` (TS 242-254).
    fn op_set_water(&mut self, tile: f64, o: &mut Vec<f64>) {
        if !self.gm().is_land(tile) {
            o.push(0.0);
            return;
        }
        if self.gm().has_owner(tile) {
            push_err(o, &format!("cannot set water, tile {tile} has owner"));
            return;
        }
        if self.gm().has_fallout(tile) {
            self.gm_mut().set_fallout(tile, false);
        }
        self.territory_version += 1.0;
        self.gm_mut().set_water(tile);
        self.record_tile_update(tile);
        o.push(0.0);
    }

    /// `GameImpl.queueWaterConversion` (TS 256-266). Kind-0 games keep the
    /// M1-0 pin (`waterNukes()` true — the fallback arm is op 1 with
    /// `value=1`); kind-7 games read the real `LoopConfig.water_nukes`.
    fn op_queue_water(&mut self, tile: f64, o: &mut Vec<f64>) {
        if !self.gm().is_land(tile) {
            o.push(0.0);
            return;
        }
        if self.gm().has_owner(tile) {
            push_err(
                o,
                &format!("cannot queue water conversion, tile {tile} has owner"),
            );
            return;
        }
        let water_nukes = match &self.config {
            Some(cfg) => cfg.water_nukes,
            None => true, // M1-0 pin (no Config installed)
        };
        if !water_nukes {
            self.op_set_fallout(tile, true, o);
            return;
        }
        self.water
            .as_mut()
            .expect("map built")
            .queue_tile(tile);
        o.push(0.0);
    }

    /// Water-manager tick flush; every changed tile is recorded like
    /// `executeNextTick`'s flush loop (TS 540-543).
    fn op_water_tick(&mut self, ticks: f64, o: &mut Vec<f64>) {
        self.ticks = ticks;
        let changed = self.water.as_mut().expect("map built").tick(ticks);
        for t in &changed {
            self.record_tile_update(*t);
        }
        o.push(0.0);
        o.push(changed.len() as f64);
        for t in changed {
            o.push(t);
        }
    }

    fn op_drain(&mut self, o: &mut Vec<f64>) {
        let pairs = std::mem::take(&mut self.tile_updates);
        o.push(0.0);
        o.push(pairs.len() as f64);
        for p in pairs {
            o.push(p);
        }
    }

    /// Whole-map digest: counters plus the per-tile `recordTileUpdate`
    /// packing over every ref in order. `waterGraphVersion` rides along so
    /// the throttled graph rebuild (at most once every 20 ticks) is
    /// observable without reaching into the water manager's private state.
    fn op_digest(&self, o: &mut Vec<f64>) {
        let m = self.gm();
        let n = (m.width() * m.height()) as usize;
        o.push(0.0);
        o.push(self.territory_version);
        o.push(self.ticks);
        o.push(m.num_land_tiles());
        o.push(m.water_version());
        o.push(m.num_tiles_with_fallout());
        o.push(
            self.water
                .as_ref()
                .expect("map built")
                .water_graph_version(),
        );
        for i in 0..n {
            let t = i as f64;
            let state = m.tile_state(t).expect("valid ref") as u16;
            let terrain = m.terrain_byte(t).expect("valid ref") as u16;
            o.push(((state as u32) | ((terrain as u32) << 16)) as f64);
        }
    }

    fn construct(&mut self, a: &[f64], o: &mut Vec<f64>) {
        let mut c = C { a, i: 0 };
        let w = c.f();
        let h = c.f();
        let nl = c.f();
        let mw = c.f();
        let mh = c.f();
        let mnl = c.f();
        let tn = c.f() as usize;
        let terrain: Vec<u8> = (0..tn).map(|_| c.f() as u8).collect();
        let mtn = c.f() as usize;
        let mterrain: Vec<u8> = (0..mtn).map(|_| c.f() as u8).collect();
        if !c.done() {
            panic!("gl construct: trailing args");
        }
        let map = GameMap::new(w, h, terrain, nl);
        let mini = GameMap::new(mw, mh, mterrain, mnl);
        self.water = Some(WaterManager::new(map, mini, false));
        self.territory_version = 0.0;
        self.ticks = 0.0;
        self.tile_updates.clear();
        self.config = None;
        self.players.clear();
        self.next_player_id = 1.0;
        self.player_teams.clear();
        self.stats = StatsImpl::default();
        self.mp.clear();
        self.player_quads.clear();
        self.attack_quads.clear();
        self.start_tick = None;
        self.last_updates = empty_updates();
        self.execs.clear();
        self.un_init_execs.clear();
        self.next_exec_id = 0.0;
        self.alliance_requests.clear();
        self.request_ledger.clear();
        self.alliances.clear();
        self.next_alliance_id = 0.0;
        self.units.clear();
        self.unit_grid = None;
        self.units_version = 0.0;
        o.push(0.0);
    }

    /// M1a `construct_game` (op 7): the real `createGame` factory + ctor
    /// chain (GameImpl.ts 55-74, 118-151, 155-198, 680-703). Everything
    /// builds in locals and commits only on success — a TS throw
    /// (`resolveTeamsList`) leaves the Game object unconstructed, and the
    /// error scenarios never run follow-up ops.
    fn construct_game(&mut self, a: &[f64], o: &mut Vec<f64>) {
        let mut c = C { a, i: 0 };
        let w = c.f();
        let h = c.f();
        let nl = c.f();
        let mw = c.f();
        let mh = c.f();
        let mnl = c.f();
        let tn = c.f() as usize;
        let terrain: Vec<u8> = (0..tn).map(|_| c.f() as u8).collect();
        let mtn = c.f() as usize;
        let mterrain: Vec<u8> = (0..mtn).map(|_| c.f() as u8).collect();
        let cfg = LoopConfig {
            game_mode_team: c.f() != 0.0,
            player_teams: read_team_count(&mut c),
            disable_nav_mesh: c.f() != 0.0,
            water_nukes: c.f() != 0.0,
            difficulty: c.f() as u8,
            starting_gold: opt_f64(&mut c),
            hc_starting_gold: opt_f64(&mut c),
            hc_infinite_troops: c.f() != 0.0,
            infinite_troops: c.f() != 0.0,
            spawn_immunity_duration: opt_f64(&mut c),
            alliance_duration: c.f(),
        };
        let humans = read_infos(&mut c);
        let nations = read_infos(&mut c);
        if !c.done() {
            panic!("gl construct_game: trailing args");
        }

        // Ctor body (TS 118-151): the collaborators without construction-
        // observable state (TerraNulliusImpl / SharedWaterCache /
        // RailNetwork) are not modeled; the water manager is (ops 3/4/6) and
        // M1e adds the real UnitGrid (M1e: TS 133-134 `new UnitGrid(this._map)`)
        // over an independent GameMap copy (GameMap is not Clone; the grid
        // only reads x/y off identical geometry).
        let map = GameMap::new(w, h, terrain.clone(), nl);
        let mini = GameMap::new(mw, mh, mterrain, mnl);
        let water = WaterManager::new(map, mini, cfg.disable_nav_mesh);
        let unit_grid = crate::unit_grid::UnitGrid::new(GameMap::new(w, h, terrain, nl));

        // populateTeams (TS 155-161) — only when gameMode === Team.
        let mut player_teams: Vec<String> = Vec::new();
        if cfg.game_mode_team {
            let total = (humans.len() + nations.len()) as f64;
            match resolve_teams_list(&cfg.player_teams, total) {
                Ok(teams) => player_teams = teams,
                Err(e) => {
                    push_err(o, &resolve_err_msg(&cfg.player_teams, e));
                    return;
                }
            }
        }

        // addPlayers (TS 163-198), one loop per path.
        let mut players: Vec<LoopPlayer> = Vec::new();
        let mut mp: Vec<MP> = Vec::new();
        let mut next_id = 1.0;
        if !cfg.game_mode_team {
            // FFA (TS 164-168): humans then nations, maybeAssignTeam team.
            for info in &humans {
                add_loop_player(
                    &mut players,
                    &mut mp,
                    &mut next_id,
                    &cfg,
                    &player_teams,
                    info.clone(),
                    None,
                );
            }
            for info in &nations {
                add_loop_player(
                    &mut players,
                    &mut mp,
                    &mut next_id,
                    &cfg,
                    &player_teams,
                    info.clone(),
                    None,
                );
            }
        } else if matches!(cfg.player_teams, TeamCountConfig::HumansVsNations) {
            // HumansVsNations (TS 169-178): fixed colored teams.
            for info in &humans {
                add_loop_player(
                    &mut players,
                    &mut mp,
                    &mut next_id,
                    &cfg,
                    &player_teams,
                    info.clone(),
                    Some("Humans".to_string()),
                );
            }
            for info in &nations {
                add_loop_player(
                    &mut players,
                    &mut mp,
                    &mut next_id,
                    &cfg,
                    &player_teams,
                    info.clone(),
                    Some("Nations".to_string()),
                );
            }
        } else {
            // Team mode (TS 180-198): assignTeams over humans ++ nations.
            let mut all: Vec<LoopPlayerInfo> = humans.clone();
            all.extend(nations.clone());
            let ta_infos: Vec<_> = all.iter().map(|i| i.ta()).collect();
            let is_duos_trios_quads = matches!(
                cfg.player_teams,
                TeamCountConfig::Duos | TeamCountConfig::Trios | TeamCountConfig::Quads
            );
            for (idx, assignment) in assign_teams(&ta_infos, &player_teams, is_duos_trios_quads) {
                match assignment {
                    Assignment::Team(team) => add_loop_player(
                        &mut players,
                        &mut mp,
                        &mut next_id,
                        &cfg,
                        &player_teams,
                        all[idx].clone(),
                        Some(team),
                    ),
                    // `console.warn("Player X was kicked from team")` — the
                    // skip (no addPlayer, nextPlayerID not bumped) is the
                    // observable part.
                    Assignment::Kicked => {}
                }
            }
        }

        self.water = Some(water);
        self.config = Some(cfg);
        self.players = players;
        self.mp = mp;
        self.next_player_id = next_id;
        self.player_teams = player_teams;
        self.territory_version = 0.0;
        self.ticks = 0.0;
        self.tile_updates.clear();
        self.stats = StatsImpl::default();
        self.player_quads.clear();
        self.attack_quads.clear();
        self.attacks.clear();
        self.start_tick = None;
        self.last_updates = empty_updates();
        self.execs.clear();
        self.un_init_execs.clear();
        self.next_exec_id = 0.0;
        self.alliance_requests.clear();
        self.request_ledger.clear();
        self.alliances.clear();
        self.next_alliance_id = 0.0;
        self.units.clear();
        self.unit_grid = Some(unit_grid);
        self.units_version = 0.0;
        o.push(0.0);
    }

    /// M1a snapshot (op 8): the construction-observable game state, in the
    /// token order the recorder's `snapshot` mirror emits.
    fn op_snapshot(&mut self, o: &mut Vec<f64>) {
        o.push(0.0);
        o.push(self.next_player_id);
        push_str(o, "Bot"); // `botTeam: Team = ColoredTeams.Bot` (TS 102)
        o.push(self.player_teams.len() as f64);
        for t in &self.player_teams {
            push_str(o, t);
        }
        o.push(self.players.len() as f64);
        for p in &mut self.players {
            o.push(p.small_id);
            push_str(o, &p.info.id);
            push_str(o, &p.info.name);
            o.push(match p.info.player_type {
                PlayerType::Human => 0.0,
                PlayerType::Bot => 1.0,
                PlayerType::Nation => 2.0,
            });
            match &p.team {
                None => o.push(-1.0),
                Some(t) => {
                    o.push(-2.0);
                    push_str(o, t);
                }
            }
            o.push(p.troops);
            o.push(p.gold);
            o.push(p.rng.next());
            p.rng_draws += 1;
        }
    }

    // ------------------------------- M1b: the real executeNextTick loop ----

    /// `PlayerImpl.hash` (TS 1828-1833): `simpleHash(id) * (troops +
    /// numTilesOwned)` — `_units` is always empty in the M1b domain (no unit
    /// ops), so the unit-hash reduce is 0.
    fn player_hash(p: &LoopPlayer) -> f64 {
        simple_hash(&p.info.id) * (p.troops + p.tiles.len() as f64)
    }

    /// One `AttackUpdate` JSON entry (PlayerImpl.ts 330-336 map order:
    /// attackerID, targetID, troops, id, retreating). TerraNullius targets
    /// carry `smallID()` 0.
    fn attack_value(attacks: &[LoopAttack], players: &[LoopPlayer], ai: usize) -> JsonValue {
        let a = &attacks[ai];
        let target_id = match a.target {
            AttackTarget::Player(t) => players[t].small_id,
            AttackTarget::TerraNullius => 0.0,
        };
        JsonValue::Obj(vec![
            (
                "attackerID".to_string(),
                JsonValue::Num(players[a.attacker].small_id),
            ),
            ("targetID".to_string(), JsonValue::Num(target_id)),
            ("troops".to_string(), JsonValue::Num(a.troops)),
            ("id".to_string(), JsonValue::Str(a.id.clone())),
            ("retreating".to_string(), JsonValue::Bool(a.retreating)),
        ])
    }

    /// The `outgoingAttacks` / `incomingAttacks` arrays of `toFullUpdate`
    /// (PlayerImpl.ts 326-353): outgoing maps the raw list unfiltered,
    /// incoming applies the `attacker().isAlive()` filter — a non-empty raw
    /// list can serialize empty (the empty result equals the shared empty
    /// singleton TS reuses).
    fn attack_updates_value(
        attacks: &[LoopAttack],
        players: &[LoopPlayer],
        idxs: &[usize],
        filter_alive: bool,
    ) -> JsonValue {
        let mut arr = Vec::new();
        for &ai in idxs {
            if filter_alive && players[attacks[ai].attacker].tiles.is_empty() {
                continue;
            }
            arr.push(Self::attack_value(attacks, players, ai));
        }
        JsonValue::Arr(arr)
    }

    /// `PlayerImpl.toFullUpdate` (TS 267-401) as a `JsonValue` tree — the 40
    /// keys in TS declaration order, with `team` / `spawnTile` as `Undef`
    /// (JSON-stringify omits them) exactly when TS holds `undefined`. Every
    /// collection except the attack arrays is empty in the M1 domain (no
    /// alliances / embargoes / units / execs); `embargoes` is the Set → `"{}"`
    /// shape. Outgoing attacks serialize the raw list unfiltered; incoming
    /// applies the `attacker().isAlive()` filter (PlayerImpl.ts 326-353).
    fn pu_full_value(&self, pidx: usize) -> JsonValue {
        let p = &self.players[pidx];
        let opt_str_v = |s: &Option<String>| match s {
            Some(v) => JsonValue::Str(v.clone()),
            None => JsonValue::Null,
        };
        let traitor_remaining = self.traitor_remaining(pidx);
        JsonValue::Obj(vec![
            ("type".to_string(), JsonValue::Num(2.0)),
            ("clientID".to_string(), opt_str_v(&p.info.client_id)),
            ("name".to_string(), JsonValue::Str(p.info.name.clone())),
            (
                "displayName".to_string(),
                JsonValue::Str(p.display_name.clone()),
            ),
            ("clanTag".to_string(), opt_str_v(&p.info.clan_tag)),
            ("nationFlag".to_string(), opt_str_v(&p.info.nation_flag)),
            ("id".to_string(), JsonValue::Str(p.info.id.clone())),
            (
                "team".to_string(),
                match &p.team {
                    Some(t) => JsonValue::Str(t.clone()),
                    // `this.team() ?? undefined` — omitted from the JSON.
                    None => JsonValue::Undef,
                },
            ),
            ("smallID".to_string(), JsonValue::Num(p.small_id)),
            (
                "playerType".to_string(),
                JsonValue::Str(
                    match p.info.player_type {
                        PlayerType::Human => "HUMAN",
                        PlayerType::Bot => "BOT",
                        PlayerType::Nation => "NATION",
                    }
                    .to_string(),
                ),
            ),
            ("isAlive".to_string(), JsonValue::Bool(!p.tiles.is_empty())),
            ("isDisconnected".to_string(), JsonValue::Bool(false)),
            // deathStats?.killedBy ?? null — the stats entries M1b creates
            // never carry killedBy/deathPosition.
            ("killedBy".to_string(), JsonValue::Null),
            ("deathPosition".to_string(), JsonValue::Null),
            (
                "tilesOwned".to_string(),
                JsonValue::Num(p.tiles.len() as f64),
            ),
            ("gold".to_string(), JsonValue::Num(p.gold)),
            ("tradeGold".to_string(), JsonValue::Num(0.0)),
            ("trainGold".to_string(), JsonValue::Num(0.0)),
            ("piracyGold".to_string(), JsonValue::Num(0.0)),
            ("goldEarned".to_string(), JsonValue::Num(0.0)),
            ("troops".to_string(), JsonValue::Num(p.troops)),
            // allies: the other side's smallIDs, alliance insertion order
            // (PlayerImpl.ts 296: alliances.map(a => a.other(this).smallID())).
            (
                "allies".to_string(),
                JsonValue::Arr(
                    p.alliances
                        .iter()
                        .map(|&ai| {
                            JsonValue::Num(alliance_other_small(
                                &self.alliances[ai],
                                pidx,
                                &self.players,
                            ))
                        })
                        .collect(),
                ),
            ),
            // `embargoes` is a `Set` → JSON.stringify writes `"{}"`.
            ("embargoes".to_string(), JsonValue::Obj(Vec::new())),
            (
                "isTraitor".to_string(),
                JsonValue::Bool(traitor_remaining > 0.0),
            ),
            (
                "traitorRemainingTicks".to_string(),
                JsonValue::Num(traitor_remaining),
            ),
            ("inDoomsdayClock".to_string(), JsonValue::Bool(false)),
            ("isDecaying".to_string(), JsonValue::Bool(false)),
            (
                "markedDoomsdayClockTick".to_string(),
                JsonValue::Num(-1.0),
            ),
            ("targets".to_string(), JsonValue::Arr(Vec::new())),
            ("outgoingEmojis".to_string(), JsonValue::Arr(Vec::new())),
            (
                "outgoingAttacks".to_string(),
                Self::attack_updates_value(&self.attacks, &self.players, &p.out_attacks, false),
            ),
            (
                "incomingAttacks".to_string(),
                Self::attack_updates_value(&self.attacks, &self.players, &p.in_attacks, true),
            ),
            (
                "outgoingAllianceRequests".to_string(),
                JsonValue::Arr(
                    self.alliance_requests
                        .iter()
                        .filter(|&&t| self.request_ledger[t].requestor == pidx)
                        .map(|&t| {
                            JsonValue::Str(
                                self.players[self.request_ledger[t].recipient]
                                    .info
                                    .id
                                    .clone(),
                            )
                        })
                        .collect(),
                ),
            ),
            // AllianceView serialization (PlayerImpl.ts 296-308): `other` is
            // the other player's info.id string; hasExtensionRequest = 
            // expiresAt <= ticks + allianceExtensionPromptOffset (the
            // constant 300, Config.ts 1279-1281).
            (
                "alliances".to_string(),
                JsonValue::Arr(
                    p.alliances
                        .iter()
                        .map(|&ai| alliance_wire_value(&self.alliance_wire(ai, pidx)))
                        .collect(),
                ),
            ),
            (
                "hasSpawned".to_string(),
                JsonValue::Bool(p.spawn_tile.is_some()),
            ),
            (
                "spawnTile".to_string(),
                match p.spawn_tile {
                    Some(t) => JsonValue::Num(t),
                    None => JsonValue::Undef,
                },
            ),
            ("betrayals".to_string(), JsonValue::Num(p.betrayal_count)),
            ("lastDeleteUnitTick".to_string(), JsonValue::Num(-1.0)),
            (
                "isLobbyCreator".to_string(),
                JsonValue::Bool(p.info.is_lobby_creator),
            ),
        ])
    }

    /// `diffPlayerUpdate` (GameUpdateUtils.ts 79-162) restricted to the
    /// M1-reachable mutable fields, in `setIfDifferent` order: `{"type":2,
    /// "id":..}` plus the changed ones (spawnTile always numeric when it
    /// changes — the None→Some transition; Some→None never happens; the
    /// attack arrays follow spawnTile in that order). Attack arrays resent in
    /// full when `attackArrayMembershipEqual` fails — the PuCore lists are
    /// the serialized (post-filter) memberships, and entry immutability makes
    /// index-sequence equality that value equality.
    fn pu_diff_value(&self, p: &LoopPlayer, prev: &PuCore, next: &PuCore) -> Option<JsonValue> {
        let mut changed: Vec<(String, JsonValue)> = Vec::new();
        // setIfDifferent order (GameUpdateUtils.ts 92-160), restricted to the
        // M1-reachable mutable fields: isAlive(100) / isTraitor(109) /
        // traitorRemainingTicks(110) / hasSpawned(123) / spawnTile(124) /
        // betrayals(125) / allies(131) / outgoingAllianceRequests(133) /
        // outgoingAttacks(149) / incomingAttacks(153) / alliances(157).
        // isLobbyCreator sits between betrayals and allies in TS but is
        // construction-constant, so it can never surface here.
        if prev.is_alive != next.is_alive {
            changed.push(("isAlive".to_string(), JsonValue::Bool(next.is_alive)));
        }
        if prev.is_traitor != next.is_traitor {
            changed.push(("isTraitor".to_string(), JsonValue::Bool(next.is_traitor)));
        }
        if prev.traitor_remaining != next.traitor_remaining {
            changed.push((
                "traitorRemainingTicks".to_string(),
                JsonValue::Num(next.traitor_remaining),
            ));
        }
        if prev.has_spawned != next.has_spawned {
            changed.push((
                "hasSpawned".to_string(),
                JsonValue::Bool(next.has_spawned),
            ));
        }
        if prev.spawn_tile != next.spawn_tile {
            changed.push((
                "spawnTile".to_string(),
                match next.spawn_tile {
                    Some(t) => JsonValue::Num(t),
                    None => JsonValue::Undef,
                },
            ));
        }
        if prev.betrayals != next.betrayals {
            changed.push(("betrayals".to_string(), JsonValue::Num(next.betrayals)));
        }
        if prev.allies != next.allies {
            changed.push((
                "allies".to_string(),
                JsonValue::Arr(next.allies.iter().map(|&v| JsonValue::Num(v)).collect()),
            ));
        }
        if prev.out_all_reqs != next.out_all_reqs {
            changed.push((
                "outgoingAllianceRequests".to_string(),
                JsonValue::Arr(
                    next.out_all_reqs
                        .iter()
                        .map(|s| JsonValue::Str(s.clone()))
                        .collect(),
                ),
            ));
        }
        if prev.out_attacks != next.out_attacks {
            changed.push((
                "outgoingAttacks".to_string(),
                Self::attack_updates_value(&self.attacks, &self.players, &next.out_attacks, false),
            ));
        }
        if prev.in_attacks != next.in_attacks {
            changed.push((
                "incomingAttacks".to_string(),
                Self::attack_updates_value(&self.attacks, &self.players, &next.in_attacks, true),
            ));
        }
        if prev.alliances != next.alliances {
            changed.push((
                "alliances".to_string(),
                JsonValue::Arr(
                    next.alliances
                        .iter()
                        .map(alliance_wire_value)
                        .collect(),
                ),
            ));
        }
        if changed.is_empty() {
            return None;
        }
        let mut entries = vec![
            ("type".to_string(), JsonValue::Num(2.0)),
            ("id".to_string(), JsonValue::Str(p.info.id.clone())),
        ];
        entries.extend(changed);
        Some(JsonValue::Obj(entries))
    }

    /// `GameImpl.executeNextTick` (TS 488-546) — the real loop with the
    /// M1d stub-exec orchestration: fresh updates map, tileUpdatePairs reset,
    /// the exec tick pass (spawn gate + isActive), the init pass (spawn gate
    /// only), removeInactiveExecutions (BEFORE the init'd batch is pushed —
    /// the one-tick exemption), per-player recordTickSample + toUpdate, the
    /// %10 Hash update, the real waterManager tick flush, `_ticks++`. Emits
    /// `[0, ticksAfter, gameHash, updatesJSON(encS)]`.
    fn op_execute_next_tick(&mut self, o: &mut Vec<f64>) {
        let tick = self.ticks;
        // TS 489-490: fresh `createGameUpdatesMap()` + tileUpdatePairs reset
        // (drops whatever conquer/ops accumulated between ticks).
        self.tile_updates.clear();
        // TS 491-498: the tick pass over initialized execs — spawn gate AND
        // isActive; tick() emits first, then killAt may flip dead. The gate
        // is `!inSpawnPhase() || activeDuringSpawnPhase()` and
        // inSpawnPhase() is startTick === null, so the non-spawn side is
        // `is_some()`.
        let mut display_updates: Vec<JsonValue> = Vec::new();
        for e in self.execs.iter_mut() {
            let adsp = e.spec.adsp;
            if (self.start_tick.is_some() || adsp) && !e.dead {
                if e.spec.tick_update {
                    StubExec::emit_update(&mut display_updates, format!("tick {}", tick));
                }
                if tick >= e.spec.kill_at {
                    e.dead = true;
                }
            }
        }
        // TS 499-508: the init pass over unInitExecs — spawn gate ONLY (a
        // born-dead exec is still init'd); the deferred ones stay parked.
        let mut inited: Vec<StubExec> = Vec::new();
        let mut parked: Vec<StubExec> = Vec::new();
        for e in self.un_init_execs.drain(..) {
            if self.start_tick.is_some() || e.spec.adsp {
                if e.spec.init_update {
                    StubExec::emit_update(&mut display_updates, format!("init {}", tick));
                }
                inited.push(e);
            } else {
                parked.push(e);
            }
        }
        // TS 510 + 635-651: compact execs in place — spawn branch keeps
        // `!adsp || isActive`, otherwise `isActive`. Runs BEFORE the init'd
        // batch is pushed, so a fresh exec never sees its own removal pass.
        let in_spawn = self.start_tick.is_none();
        let mut w = 0usize;
        for i in 0..self.execs.len() {
            let keep = if in_spawn {
                !self.execs[i].spec.adsp || !self.execs[i].dead
            } else {
                !self.execs[i].dead
            };
            if keep {
                self.execs.swap(w, i);
                w += 1;
            }
        }
        self.execs.truncate(w);
        // TS 512-513: the init'd batch joins execs after the removal pass.
        self.execs.extend(inited);
        self.un_init_execs = parked;
        let mut player_updates: Vec<JsonValue> = Vec::new();
        let mut hash_update: Option<JsonValue> = None;
        for i in 0..self.players.len() {
            let (alive, spawned, tiles_owned, troops, gold, small_id) = {
                let p = &self.players[i];
                (
                    !p.tiles.is_empty(),
                    p.spawn_tile.is_some(),
                    p.tiles.len() as f64,
                    p.troops,
                    p.gold,
                    p.small_id,
                )
            };
            // TS 518-525: sampled before toUpdate; dead and unspawned
            // players are skipped. The fourth stat argument is
            // `p.alliances().length` (the peak-concurrent-alliances input).
            if alive && spawned {
                let mut trace: Vec<f64> = Vec::new(); // facade trace discarded
                let _ = self.stats.record_tick_sample(
                    &mut trace,
                    &self.mp,
                    i,
                    &Val::Num(tiles_owned),
                    &Val::Num(troops),
                    self.players[i].alliances.len() as f64,
                );
            }
            // TS 226-264 `toUpdate`: full on the first call, then quints +
            // attack deltas + field diff. The PuCore memberships are the
            // SERIALIZED arrays: outgoing raw, incoming passed through the
            // `attacker().isAlive()` filter (GameUpdateUtils.ts 251-268
            // compares those post-filter arrays).
            let traitor_remaining = self.traitor_remaining(i);
            let core = PuCore {
                is_alive: alive,
                has_spawned: spawned,
                spawn_tile: self.players[i].spawn_tile,
                tiles_owned,
                gold,
                troops,
                gold_earned: 0.0,
                out_attacks: self.players[i].out_attacks.clone(),
                in_attacks: self.players[i]
                    .in_attacks
                    .iter()
                    .copied()
                    .filter(|&ai| !self.players[self.attacks[ai].attacker].tiles.is_empty())
                    .collect(),
                is_traitor: traitor_remaining > 0.0,
                traitor_remaining,
                betrayals: self.players[i].betrayal_count,
                allies: self.players[i]
                    .alliances
                    .iter()
                    .map(|&ai| {
                        alliance_other_small(&self.alliances[ai], i, &self.players)
                    })
                    .collect(),
                out_all_reqs: self
                    .alliance_requests
                    .iter()
                    .filter(|&&t| self.request_ledger[t].requestor == i)
                    .map(|&t| self.players[self.request_ledger[t].recipient].info.id.clone())
                    .collect(),
                alliances: self
                    .players[i]
                    .alliances
                    .iter()
                    .map(|&ai| self.alliance_wire(ai, i))
                    .collect(),
            };
            let prev = self.players[i].last_sent.replace(core.clone());
            let pu = match prev {
                None => self.pu_full_value(i),
                Some(prev) => {
                    if prev.tiles_owned != core.tiles_owned
                        || prev.gold != core.gold
                        || prev.troops != core.troops
                        || prev.gold_earned != core.gold_earned
                    {
                        self.player_quads.extend_from_slice(&[
                            small_id,
                            core.tiles_owned,
                            core.gold,
                            core.troops,
                            core.gold_earned,
                        ]);
                    }
                    // packAttackTroopDeltas (GameUpdateUtils.ts 289-303):
                    // quads only when membership is equal — an unequal
                    // membership resends the whole array through the diff
                    // instead (and invalidates the indexes). The `prev ===
                    // next` refid fast path (the shared empty singleton) is
                    // subsumed by the zero-length loop. Direction lanes:
                    // ATTACK_DELTA_OUTGOING 0 / INCOMING 1.
                    if prev.out_attacks == core.out_attacks {
                        for (k, &ai) in core.out_attacks.iter().enumerate() {
                            let troops = self.attacks[ai].troops;
                            if self.attacks[prev.out_attacks[k]].troops != troops {
                                self.attack_quads
                                    .extend_from_slice(&[small_id, 0.0, k as f64, troops]);
                            }
                        }
                    }
                    if prev.in_attacks == core.in_attacks {
                        for (k, &ai) in core.in_attacks.iter().enumerate() {
                            let troops = self.attacks[ai].troops;
                            if self.attacks[prev.in_attacks[k]].troops != troops {
                                self.attack_quads
                                    .extend_from_slice(&[small_id, 1.0, k as f64, troops]);
                            }
                        }
                    }
                    match self.pu_diff_value(&self.players[i], &prev, &core) {
                        Some(v) => v,
                        None => continue, // diffPlayerUpdate null → no addUpdate
                    }
                }
            };
            player_updates.push(pu);
        }
        // TS 532-538: the %10 hash update (`this.ticks() % 10 === 0`).
        if tick % 10.0 == 0.0 {
            let gh = self.game_hash();
            hash_update = Some(JsonValue::Obj(vec![
                ("type".to_string(), JsonValue::Num(13.0)),
                ("tick".to_string(), JsonValue::Num(tick)),
                ("hash".to_string(), JsonValue::Num(gh)),
            ]));
        }
        // TS 540-543: flush pending water conversions + throttled graph
        // rebuild; every changed tile is recorded through the game's packer.
        let changed = self.water.as_mut().expect("map built").tick(tick);
        for t in &changed {
            self.record_tile_update(*t);
        }
        self.ticks += 1.0;
        // The 24-bucket updates map → the JSON the worker channel carries.
        // The map stays live on the game until the next tick's reset (M1c's
        // endSpawnPhase appends into it), so pin it on the harness.
        let mut buckets: Vec<(String, JsonValue)> = (0..24)
            .map(|i| (i.to_string(), JsonValue::Arr(Vec::new())))
            .collect();
        buckets[2].1 = JsonValue::Arr(player_updates);
        buckets[3].1 = JsonValue::Arr(display_updates);
        if let Some(hv) = hash_update {
            buckets[13].1 = JsonValue::Arr(vec![hv]);
        }
        let map = JsonValue::Obj(buckets);
        self.last_updates = map.clone();
        let json = json_stringify(&map).expect("updates serialize");
        o.push(0.0);
        o.push(self.ticks);
        o.push(self.game_hash());
        push_str(o, &json);
    }

    /// `GameImpl.hash` (TS 623-629): 1 + Σ player hashes over `_players`
    /// insertion order (== `players` order here).
    fn game_hash(&self) -> f64 {
        let mut h = 1.0;
        for p in &self.players {
            h += Self::player_hash(p);
        }
        h
    }

    /// `StatsImpl.data` snapshot (op 10): clientID-keyed insertion order,
    /// with optional tags for the three fields `recordTickSample` touches.
    fn op_stats_snapshot(&self, o: &mut Vec<f64>) {
        o.push(0.0);
        o.push(self.stats.data.len() as f64);
        for (cid, ps) in &self.stats.data {
            push_str(o, cid);
            match &ps.tiles {
                None => o.push(-1.0),
                Some(t) => {
                    o.push(-2.0);
                    o.push(t.len() as f64);
                    for v in t {
                        o.push(*v as f64);
                    }
                }
            }
            match ps.peak_troops {
                None => o.push(-1.0),
                Some(v) => {
                    o.push(-2.0);
                    o.push(v as f64);
                }
            }
            match &ps.alliances {
                None => o.push(-1.0),
                Some(a) => {
                    o.push(-2.0);
                    o.push(a.len() as f64);
                    for v in a {
                        o.push(*v as f64);
                    }
                }
            }
        }
    }

    /// `PlayerImpl.setSpawnTile` (TS 726-728).
    fn op_set_spawn_tile(&mut self, pidx: f64, tile: f64, o: &mut Vec<f64>) {
        self.players[pidx as usize].spawn_tile = Some(tile);
        o.push(0.0);
    }

    /// `PlayerImpl.addTroops` (TS 1345-1359): negative amounts delegate to
    /// `removeTroops(-x)`; positive floor through `toInt` into the troops.
    fn op_add_troops(&mut self, pidx: f64, amount: f64, o: &mut Vec<f64>) {
        let p = &mut self.players[pidx as usize];
        if amount < 0.0 {
            let t = -amount; // removeTroops(-1 * troops)
            if t > 0.0 {
                let to_remove = to_int_clamped(t).min(p.troops); // minInt(_troops, toInt(t))
                p.troops -= to_remove;
            }
        } else {
            p.troops += to_int_clamped(amount);
        }
        o.push(0.0);
    }

    /// `GameImpl.conquer` (TS 761-783): water / impassable throw paths,
    /// previous-owner bookkeeping, `_territoryVersion++`, `setOwnerID`,
    /// tile + lastTileChange bookkeeping, `updateBorders`, fallout clear,
    /// and the single `recordTileUpdate`.
    fn op_conquer(&mut self, pidx: f64, tile: f64, o: &mut Vec<f64>) {
        if !self.gm().is_land(tile) {
            push_err(o, "cannot conquer water");
            return;
        }
        if self.gm().is_impassable(tile) {
            push_err(o, "cannot conquer impassable terrain");
            return;
        }
        let prev_owner = self.gm().owner_id(tile);
        if prev_owner != 0.0 {
            // `previousOwner.isPlayer()` — smallID 0 is TerraNullius.
            let p = &mut self.players[(prev_owner - 1.0) as usize];
            p.last_tile_change = self.ticks;
            p.tile_change_version += 1.0;
            p.tiles.retain(|t| *t != tile);
            p.border_tiles.retain(|t| *t != tile);
        }
        self.territory_version += 1.0;
        let sid = self.players[pidx as usize].small_id;
        self.gm_mut().set_owner_id(tile, sid);
        let p = &mut self.players[pidx as usize];
        p.tiles.push(tile);
        p.last_tile_change = self.ticks;
        p.tile_change_version += 1.0;
        self.update_borders(tile);
        self.gm_mut().set_fallout(tile, false);
        self.record_tile_update(tile);
        o.push(0.0);
    }

    /// `GameImpl.updateBorders` (TS 808-814): the tile plus its 4-neighbours.
    fn update_borders(&mut self, tile: f64) {
        self.update_border_status(tile);
        let mut buf = [0.0f64; 4];
        let n = self.gm().neighbors4(tile, &mut buf);
        for &t in &buf[..n] {
            self.update_border_status(t);
        }
    }

    /// `GameImpl.updateBorderStatus` (TS 816-826).
    fn update_border_status(&mut self, t: f64) {
        if !self.gm().has_owner(t) {
            return;
        }
        let owner = self.gm().owner_id(t);
        let is_border = self.gm().is_border(t);
        let p = &mut self.players[(owner - 1.0) as usize];
        if is_border {
            if !p.border_tiles.contains(&t) {
                p.border_tiles.push(t);
            }
        } else {
            p.border_tiles.retain(|x| *x != t);
        }
    }

    /// `drainPackedPlayerUpdates` / `drainPackedAttackUpdates` (TS 567-581):
    /// null (encoded -1) when nothing accumulated, else the packed quads.
    fn op_drain_quads(quads: &mut Vec<f64>, o: &mut Vec<f64>) {
        o.push(0.0);
        if quads.is_empty() {
            o.push(-1.0);
            return;
        }
        let q = std::mem::take(quads);
        o.push(q.len() as f64);
        o.extend_from_slice(&q);
    }

    /// `GameImpl.endSpawnPhase` (M1c, TS 473-482): idempotent — once
    /// `startTick` is stamped the early return leaves everything untouched
    /// (second call, same JSON). The first call stamps `startTick = _ticks`
    /// and appends the SpawnPhaseEnd update into the LIVE updates map, which
    /// is then serialized in full: ctor-fresh (24 empty buckets) before the
    /// first tick, or the previous tick's leftovers (still unreset) plus the
    /// new update. Emits `[0, startTick, updatesJSON(encS)]`.
    fn op_end_spawn_phase(&mut self, o: &mut Vec<f64>) {
        if self.start_tick.is_none() {
            let st = self.ticks;
            self.start_tick = Some(st);
            if let JsonValue::Obj(buckets) = &mut self.last_updates {
                buckets[21].1 = JsonValue::Arr(vec![JsonValue::Obj(vec![
                    ("type".to_string(), JsonValue::Num(21.0)),
                    ("startTick".to_string(), JsonValue::Num(st)),
                ])]);
            }
        }
        o.push(0.0);
        o.push(self.start_tick.unwrap_or(-1.0));
        let json = json_stringify(&self.last_updates).expect("updates serialize");
        push_str(o, &json);
    }

    /// The spawn-immunity cluster on the real clock (M1c, TS 921-945):
    /// `inSpawnPhase` (`startTick === null`), private `ticksSinceStart` (0
    /// in the spawn phase, else `max(0, ticks - startTick)`),
    /// `elapsedGameSeconds` (`ticksSinceStart / 10`), and both immunity
    /// gates — the player one against
    /// `config.spawnImmunityDuration ?? 50` (Config.ts 332-334), the nation
    /// one against the constant `DEFAULT_SPAWN_IMMUNITY_TICKS` (Config.ts
    /// 337-339). Emits
    /// `[0, inSpawn, sinceStart, elapsed, immunity, nationImmunity]`.
    fn op_probe_spawn(&mut self, o: &mut Vec<f64>) {
        let in_spawn = self.start_tick.is_none();
        let since = if in_spawn {
            0.0
        } else {
            (self.ticks - self.start_tick.unwrap()).max(0.0)
        };
        let dur = self
            .config
            .as_ref()
            .and_then(|c| c.spawn_immunity_duration)
            .unwrap_or(50.0);
        o.push(0.0);
        o.push(if in_spawn { 1.0 } else { 0.0 });
        o.push(since);
        o.push(since / 10.0);
        o.push(if in_spawn || since < dur { 1.0 } else { 0.0 });
        o.push(if in_spawn || since < 50.0 { 1.0 } else { 0.0 });
    }

    /// `GameImpl.addExecution` (op 18): decode the stub spec and park the
    /// exec in `unInitExecs` (TS 665-667); init happens on the first tick
    /// whose spawn gate admits it.
    fn op_add_exec(&mut self, a: &[f64], o: &mut Vec<f64>) {
        let mut c = C { a, i: 0 };
        // Arg order matches the recorder: [adsp, bornDead, killAt,
        // initUpdate, tickUpdate].
        let adsp = c.f() != 0.0;
        let born_dead = c.f() != 0.0;
        let kill_at = c.f();
        let init_update = c.f() != 0.0;
        let tick_update = c.f() != 0.0;
        let id = self.next_exec_id;
        self.next_exec_id += 1.0;
        self.un_init_execs.push(StubExec {
            id,
            spec: ExecSpec {
                adsp,
                kill_at,
                init_update,
                tick_update,
            },
            dead: born_dead,
        });
        o.push(0.0);
    }

    /// `executions()` (op 19) = `[...execs, ...unInitExecs]` (TS 661-663):
    /// membership, order, and the isActive flips side by side. Per entry:
    /// `[id, dead?1:0]`.
    fn op_exec_info(&self, o: &mut Vec<f64>) {
        o.push(0.0);
        o.push(self.execs.len() as f64);
        o.push(self.un_init_execs.len() as f64);
        for e in &self.execs {
            o.push(e.id);
            o.push(if e.dead { 1.0 } else { 0.0 });
        }
        for e in &self.un_init_execs {
            o.push(e.id);
            o.push(if e.dead { 1.0 } else { 0.0 });
        }
    }

    /// `PlayerImpl.createAttack` (TS 1855-1875) as op 20: the id is
    /// `nextID()` off the ATTACKER's own pseudo-random (one draw), the attack
    /// joins the attacker's outgoing list and — player targets only — the
    /// target's incoming list. `target_idx < 0` is TerraNullius;
    /// `sourceTile`/`border` are the inert M1 params (null / empty — nothing
    /// in this domain reads either).
    fn op_player_create_attack(
        &mut self,
        pidx: f64,
        target_idx: f64,
        troops: f64,
        o: &mut Vec<f64>,
    ) {
        let attacker = pidx as usize;
        let id = self.players[attacker].rng.next_id();
        self.players[attacker].rng_draws += 1;
        let target_player = (target_idx >= 0.0).then_some(target_idx as usize);
        let target = match target_player {
            Some(t) => AttackTarget::Player(t),
            None => AttackTarget::TerraNullius,
        };
        let ai = self.attacks.len();
        self.attacks.push(LoopAttack {
            attacker,
            target,
            troops,
            id,
            retreating: false,
        });
        self.players[attacker].out_attacks.push(ai);
        if let Some(t) = target_player {
            self.players[t].in_attacks.push(ai);
        }
        o.push(0.0);
    }

    // --------------------- M1e: territory + alliance + units ----------------

    /// `PlayerImpl.getTraitorRemainingTicks` (TS 855-861) against the
    /// constant `traitorDuration()` (Config.ts 290-292: 30 * 10).
    fn traitor_remaining(&self, pidx: usize) -> f64 {
        let p = &self.players[pidx];
        if p.marked_traitor_tick < 0.0 {
            return 0.0;
        }
        let elapsed = self.ticks - p.marked_traitor_tick;
        let remaining = 300.0 - elapsed;
        if remaining > 0.0 {
            remaining
        } else {
            0.0
        }
    }

    /// `PlayerImpl.allianceWith` (TS 761-770): the first of this player's
    /// alliances involving `other` (insertion order); self never allies
    /// (TS 762-764).
    fn alliance_between(&self, pidx: usize, other: usize) -> Option<usize> {
        if pidx == other {
            return None;
        }
        self.players[pidx]
            .alliances
            .iter()
            .copied()
            .find(|&ai| {
                let a = &self.alliances[ai];
                a.requestor == other || a.recipient == other
            })
    }

    /// `detachAlliance` (TS 914-919): drop the alliance from both sides'
    /// per-player lists.
    fn detach_alliance(&mut self, ai: usize) {
        let (req, rec) = (
            self.alliances[ai].requestor,
            self.alliances[ai].recipient,
        );
        self.players[req].alliances.retain(|&x| x != ai);
        self.players[rec].alliances.retain(|&x| x != ai);
    }

    /// The serialized `AllianceView` of one live alliance (PlayerImpl.ts
    /// 296-308): `other` is the other endpoint's `info.id` string,
    /// `hasExtensionRequest = expiresAt <= ticks +
    /// allianceExtensionPromptOffset()` (the constant 300, Config.ts
    /// 1279-1281).
    fn alliance_wire(&self, ai: usize, pidx: usize) -> AllianceWire {
        let a = &self.alliances[ai];
        let other = if a.requestor == pidx {
            a.recipient
        } else {
            a.requestor
        };
        AllianceWire {
            id: a.id,
            other: self.players[other].info.id.clone(),
            created_at: a.created_at,
            expires_at: a.expires_at,
            has_ext: a.expires_at <= self.ticks + 300.0,
        }
    }

    /// `PlayerImpl.toString` (TS 1834-1840) — the breakAlliance error message
    /// interpolates players through it. `troops` is a TS bigint (integral f64
    /// here, printed as an integer); a null clientID stringifies as "null".
    fn player_to_string(&self, pidx: usize) -> String {
        let p = &self.players[pidx];
        format!(
            "Player:{{name:{},clientID:{},isAlive:{},troops:{},numTileOwned:{}}}]",
            p.info.name,
            p.info
                .client_id
                .clone()
                .unwrap_or_else(|| "null".to_string()),
            if p.tiles.is_empty() { "false" } else { "true" },
            f64_to_bigint_str(p.troops),
            p.tiles.len(),
        )
    }

    /// `GameImpl.addUpdate` (TS 209-211): push into the live updates map's
    /// `GameUpdateType[type]` bucket (the map stays live until the next
    /// tick's reset, exactly like `endSpawnPhase`'s append).
    fn add_update_value(&mut self, update_type: f64, v: JsonValue) {
        if let JsonValue::Obj(buckets) = &mut self.last_updates {
            if let Some(entry) = buckets
                .iter_mut()
                .find(|(k, _)| k == &update_type.to_string())
            {
                if let JsonValue::Arr(arr) = &mut entry.1 {
                    arr.push(v);
                }
            }
        }
    }

    /// Serialize the live updates map — the `updatesJSON` payload the
    /// alliance ops emit (the same shape `executeNextTick`/`endSpawnPhase`
    /// carry).
    fn updates_json(&self) -> String {
        json_stringify(&self.last_updates).expect("updates serialize")
    }

    /// `acceptAllianceRequest` shared body (TS 401-437): the filter runs
    /// FIRST (its removal survives the already-allied throw), then the
    /// alliance is created — `createdAt` = the accept tick, `expiresAt` =
    /// `createdAt + config.allianceDuration()` (AllianceImpl.ts 17) — both
    /// sides push it, `allianceFormed` fires twice, the request joins the
    /// requestor's past-outgoing list, and the Reply(true) update lands.
    fn accept_request_at(&mut self, token: usize) -> Result<(), String> {
        // TS 402-404 filters by object identity FIRST: a stale (already
        // removed) request makes the filter a no-op, and the object's fields
        // stay readable for the already-allied throw below.
        self.alliance_requests.retain(|&t| t != token);
        let (req, rec, created) = {
            let ar = &self.request_ledger[token];
            (ar.requestor, ar.recipient, ar.created_at)
        };
        if self.alliance_between(req, rec).is_some() {
            return Err(format!(
                "cannot accept alliance request, already allied with {}",
                self.players[rec].info.name
            ));
        }
        let id = self.next_alliance_id;
        self.next_alliance_id += 1.0;
        let created_at = self.ticks;
        let expires_at = created_at
            + self
                .config
                .as_ref()
                .map(|c| c.alliance_duration)
                .unwrap_or(3000.0);
        let ai = self.alliances.len();
        self.alliances.push(LoopAlliance {
            id,
            requestor: req,
            recipient: rec,
            created_at,
            expires_at,
        });
        self.players[req].alliances.push(ai);
        self.players[rec].alliances.push(ai);
        let mut trace: Vec<f64> = Vec::new();
        let _ = self.stats.alliance_formed(&mut trace, &self.mp, req);
        let mut trace: Vec<f64> = Vec::new();
        let _ = self.stats.alliance_formed(&mut trace, &self.mp, rec);
        self.players[req].past_outgoing.push((rec, created));
        self.add_update_value(
            6.0,
            JsonValue::Obj(vec![
                ("type".to_string(), JsonValue::Num(6.0)),
                (
                    "request".to_string(),
                    request_update_value(req, rec, created),
                ),
                ("accepted".to_string(), JsonValue::Bool(true)),
            ]),
        );
        Ok(())
    }

    /// `rejectAllianceRequest` (TS 439-451): filter, past-outgoing push,
    /// Reply(false) update.
    fn reject_request_at(&mut self, token: usize) {
        self.alliance_requests.retain(|&t| t != token);
        let (req, rec, created) = {
            let ar = &self.request_ledger[token];
            (ar.requestor, ar.recipient, ar.created_at)
        };
        self.players[req].past_outgoing.push((rec, created));
        self.add_update_value(
            6.0,
            JsonValue::Obj(vec![
                ("type".to_string(), JsonValue::Num(6.0)),
                (
                    "request".to_string(),
                    request_update_value(req, rec, created),
                ),
                ("accepted".to_string(), JsonValue::Bool(false)),
            ]),
        );
    }

    /// `GameImpl.relinquish` (TS 785-803): unowned/water throws, previous-
    /// owner bookkeeping, `_territoryVersion++`, `setOwnerID(tile, 0)` —
    /// fallout intentionally NOT cleared (the conquer mirror does).
    fn op_relinquish(&mut self, tile: f64, o: &mut Vec<f64>) {
        if !self.gm().has_owner(tile) {
            push_err(o, "Cannot relinquish tile because it is unowned");
            return;
        }
        if self.gm().is_water(tile) {
            push_err(o, "Cannot relinquish water");
            return;
        }
        let prev_owner = self.gm().owner_id(tile);
        let p = &mut self.players[(prev_owner - 1.0) as usize];
        p.last_tile_change = self.ticks;
        p.tile_change_version += 1.0;
        p.tiles.retain(|t| *t != tile);
        p.border_tiles.retain(|t| *t != tile);
        self.territory_version += 1.0;
        self.gm_mut().set_owner_id(tile, 0.0);
        self.update_borders(tile);
        self.record_tile_update(tile);
        o.push(0.0);
    }

    /// `GameImpl.createAllianceRequest` (TS 371-399). Payload:
    /// `[nullFlag, (token), updatesJSON]` — `nullFlag` 1 for the three
    /// null-return arms (already allied / duplicate incoming /
    /// corresponding-accept, the last of which still emits the accept-side
    /// updates), 0 with the new request's ledger token when a request is
    /// created.
    fn op_create_alliance_request(&mut self, req: usize, rec: usize, o: &mut Vec<f64>) {
        // No ok flag: the recorder's payload is [nullFlag, (token), json].
        // requestor.isAlliedWith(recipient) (TS 754-758: self never allies).
        if self.alliance_between(req, rec).is_some() {
            o.push(1.0);
            push_str(o, &self.updates_json());
            return;
        }
        // recipient.incomingAllianceRequests().find(ar => ar.requestor() ===
        // requestor) — the duplicate-incoming null.
        if self.alliance_requests.iter().any(|&t| {
            let ar = &self.request_ledger[t];
            ar.requestor == req && ar.recipient == rec
        }) {
            o.push(1.0);
            push_str(o, &self.updates_json());
            return;
        }
        // correspondingReq: requestor.incoming find(requestor === recipient)
        // → accept() — both sides asked, so the already-allied throw is
        // unreachable here.
        if let Some(pos) = self.alliance_requests.iter().position(|&t| {
            let ar = &self.request_ledger[t];
            ar.requestor == rec && ar.recipient == req
        }) {
            let token = self.alliance_requests[pos];
            self.accept_request_at(token).expect("corresponding accept");
            o.push(1.0);
            push_str(o, &self.updates_json());
            return;
        }
        let token = self.request_ledger.len();
        self.request_ledger.push(LoopAllianceRequest {
            requestor: req,
            recipient: rec,
            created_at: self.ticks,
        });
        self.alliance_requests.push(token);
        self.add_update_value(5.0, request_update_value(req, rec, self.ticks));
        o.push(0.0);
        o.push(token as f64);
        push_str(o, &self.updates_json());
    }

    /// `acceptAllianceRequest` / `rejectAllianceRequest` op (TS 401-451).
    /// The accept arm filters BEFORE the already-allied check — a throw
    /// still removed the request.
    fn op_alliance_reply(&mut self, ar_idx: usize, accept: bool, o: &mut Vec<f64>) {
        if accept {
            match self.accept_request_at(ar_idx) {
                Ok(()) => {
                    o.push(0.0);
                    push_str(o, &self.updates_json());
                }
                Err(msg) => {
                    push_err(o, &msg);
                    push_str(o, &self.updates_json());
                }
            }
        } else {
            self.reject_request_at(ar_idx);
            o.push(0.0);
            push_str(o, &self.updates_json());
        }
    }

    /// `GameImpl.breakAlliance` (TS 836-870). The token `allianceIdx`
    /// identifies the exact AllianceImpl object to detach — the
    /// already-allied guard reads the BREAKER's live list, so a stale token
    /// still detaches whenever another alliance with the same pair exists
    /// (TS reference equality, mirrored by the table index). The
    /// markTraitor order: betray stat, then `allianceEnded(other,
    /// "brokenByOther")`, then `allianceEnded(breaker, null)`.
    fn op_break_alliance(&mut self, breaker: usize, ai: usize, o: &mut Vec<f64>) {
        let (a_req, a_rec, a_created, a_id) = {
            let a = &self.alliances[ai];
            (a.requestor, a.recipient, a.created_at, a.id)
        };
        let other = if a_req == breaker { a_rec } else { a_req };
        if self.alliance_between(breaker, other).is_none() {
            push_err(
                o,
                &format!(
                    "{} not allied with {}, cannot break alliance",
                    self.player_to_string(breaker),
                    self.player_to_string(other)
                ),
            );
            // The recorder emits the live updates map even on a throw.
            push_str(o, &self.updates_json());
            return;
        }
        let duration = self.ticks - a_created;
        // other.isTraitor() — isDisconnected() is constant false here.
        if self.traitor_remaining(other) <= 0.0 {
            // breaker.markTraitor() (PlayerImpl.ts 863-869): the stamp, the
            // count, the betray stat.
            self.players[breaker].marked_traitor_tick = self.ticks;
            self.players[breaker].betrayal_count += 1.0;
            let mut trace: Vec<f64> = Vec::new();
            let _ = self.stats.betray(&mut trace, &self.mp, breaker);
            let mut trace: Vec<f64> = Vec::new();
            let _ = self.stats.alliance_ended(
                &mut trace,
                &self.mp,
                other,
                &Val::Num(duration),
                Some("brokenByOther"),
            );
        } else {
            let mut trace: Vec<f64> = Vec::new();
            let _ = self.stats.alliance_ended(
                &mut trace,
                &self.mp,
                other,
                &Val::Num(duration),
                None,
            );
        }
        // The breaker's side is already counted by betray(); this call is
        // only so their longest-held maximum sees the alliance.
        let mut trace: Vec<f64> = Vec::new();
        let _ = self.stats.alliance_ended(
            &mut trace,
            &self.mp,
            breaker,
            &Val::Num(duration),
            None,
        );
        self.detach_alliance(ai);
        self.add_update_value(
            7.0,
            JsonValue::Obj(vec![
                ("type".to_string(), JsonValue::Num(7.0)),
                (
                    "traitorID".to_string(),
                    JsonValue::Num(self.players[breaker].small_id),
                ),
                (
                    "betrayedID".to_string(),
                    JsonValue::Num(self.players[other].small_id),
                ),
                ("allianceID".to_string(), JsonValue::Num(a_id)),
            ]),
        );
        o.push(0.0);
        push_str(o, &self.updates_json());
    }

    /// `GameImpl.expireAlliance` (TS 872-893). The token's endpoints define
    /// the shared set (`Set(recipient.alliances())` ∩ `requestor.alliances()`
    /// — reference identity, so the table-index intersection mirrors it);
    /// the expiring alliance is the unique survivor, whatever token was
    /// passed.
    fn op_expire_alliance(&mut self, ai: usize, o: &mut Vec<f64>) {
        let (req, rec) = (
            self.alliances[ai].requestor,
            self.alliances[ai].recipient,
        );
        let shared: Vec<usize> = self.players[req]
            .alliances
            .iter()
            .copied()
            .filter(|&x| self.players[rec].alliances.contains(&x))
            .collect();
        if shared.len() != 1 {
            push_err(
                o,
                &format!(
                    "cannot expire alliance: must have exactly one alliance, have {}",
                    shared.len()
                ),
            );
            push_str(o, &self.updates_json());
            return;
        }
        let expiring = shared[0];
        let duration = self.ticks - self.alliances[expiring].created_at;
        let mut trace: Vec<f64> = Vec::new();
        let _ = self.stats.alliance_ended(
            &mut trace,
            &self.mp,
            req,
            &Val::Num(duration),
            Some("expired"),
        );
        let mut trace: Vec<f64> = Vec::new();
        let _ = self.stats.alliance_ended(
            &mut trace,
            &self.mp,
            rec,
            &Val::Num(duration),
            Some("expired"),
        );
        self.detach_alliance(expiring);
        self.add_update_value(
            8.0,
            JsonValue::Obj(vec![
                ("type".to_string(), JsonValue::Num(8.0)),
                (
                    "player1ID".to_string(),
                    JsonValue::Num(self.players[req].small_id),
                ),
                (
                    "player2ID".to_string(),
                    JsonValue::Num(self.players[rec].small_id),
                ),
            ]),
        );
        o.push(0.0);
        push_str(o, &self.updates_json());
    }

    /// `GameImpl.removeAlliancesByPlayerSilently` (TS 895-911): snapshot the
    /// player's list, then per alliance fire the null-counter stats on both
    /// sides (so both longest-held maxima still see it) and detach. No
    /// update is emitted.
    fn op_remove_alliances_silently(&mut self, pidx: usize, o: &mut Vec<f64>) {
        let removed: Vec<usize> = self.players[pidx].alliances.clone();
        for ai in removed {
            let (req, rec, created) = {
                let a = &self.alliances[ai];
                (a.requestor, a.recipient, a.created_at)
            };
            let duration = self.ticks - created;
            let mut trace: Vec<f64> = Vec::new();
            let _ =
                self.stats
                    .alliance_ended(&mut trace, &self.mp, req, &Val::Num(duration), None);
            let mut trace: Vec<f64> = Vec::new();
            let _ =
                self.stats
                    .alliance_ended(&mut trace, &self.mp, rec, &Val::Num(duration), None);
            self.detach_alliance(ai);
        }
        o.push(0.0);
    }

    /// Alliance-state snapshot (op 27): the live request table, the alliance
    /// table, `nextAllianceID`, and per player the alliance tokens (their
    /// `_alliances` table indices), past-outgoing `(recipient, createdAt)`
    /// pairs, and the traitor state.
    fn op_alliance_snapshot(&mut self, o: &mut Vec<f64>) {
        o.push(0.0);
        o.push(self.alliance_requests.len() as f64);
        for &t in &self.alliance_requests {
            let ar = &self.request_ledger[t];
            o.push(ar.requestor as f64);
            o.push(ar.recipient as f64);
            o.push(ar.created_at);
        }
        o.push(self.alliances.len() as f64);
        for i in 0..self.alliances.len() {
            let a = &self.alliances[i];
            o.push(a.id);
            o.push(a.requestor as f64);
            o.push(a.recipient as f64);
            o.push(a.created_at);
            o.push(a.expires_at);
        }
        o.push(self.next_alliance_id);
        for i in 0..self.players.len() {
            let p = &self.players[i];
            o.push(p.alliances.len() as f64);
            for &ai in &p.alliances {
                o.push(ai as f64);
            }
            o.push(p.past_outgoing.len() as f64);
            for (rec, created) in &p.past_outgoing {
                o.push(*rec as f64);
                o.push(*created);
            }
            o.push(p.marked_traitor_tick);
            o.push(p.betrayal_count);
            o.push(self.traitor_remaining(i));
        }
    }

    /// Nearby probe (op 28): per-other `sharesBorderWith` (TS 554-567) plus
    /// `nearby()` (TS 587-632, memo keyed on `(territoryVersion,
    /// waterVersion)`) serialized as smallIDs (TerraNullius = 0).
    fn op_nearby_probe(&mut self, pidx: usize, o: &mut Vec<f64>) {
        o.push(0.0);
        o.push(self.players.len() as f64);
        for j in 0..self.players.len() {
            o.push(if self.shares_border_with(pidx, j) {
                1.0
            } else {
                0.0
            });
        }
        let tv = self.territory_version;
        let wv = self.gm().water_version();
        let cached = match &self.players[pidx].nearby_memo {
            Some((v, w, r)) if *v == tv && *w == wv => Some(r.clone()),
            _ => None,
        };
        let result = match cached {
            Some(r) => r,
            None => {
                let r = self.compute_nearby(pidx);
                self.players[pidx].nearby_memo = Some((tv, wv, r.clone()));
                r
            }
        };
        o.push(result.len() as f64);
        for sid in result {
            o.push(sid);
        }
    }

    /// `PlayerImpl.sharesBorderWith` (TS 554-567): any 4-neighbor of any
    /// border tile owned by the other smallID.
    fn shares_border_with(&self, pidx: usize, other: usize) -> bool {
        let other_id = self.players[other].small_id;
        let mut buf = [0.0f64; 4];
        for &b in &self.players[pidx].border_tiles {
            let n = self.gm().neighbors4(b, &mut buf);
            for &t in &buf[..n] {
                if self.gm().owner_id(t) == other_id {
                    return true;
                }
            }
        }
        false
    }

    /// `PlayerImpl.computeNearby` (TS 608-632) + `shoreReachableNeighbors`
    /// (TS 637-677), into an insertion-ordered set of smallIDs — the
    /// observable order of `Array.from(ns)`.
    fn compute_nearby(&self, pidx: usize) -> Vec<f64> {
        let small = self.players[pidx].small_id;
        let gm = self.gm();
        let mut set: Vec<f64> = Vec::new();
        let mut buf = [0.0f64; 4];
        for &b in &self.players[pidx].border_tiles {
            let n = gm.neighbors4(b, &mut buf);
            for &t in &buf[..n] {
                if gm.is_land(t) && !gm.is_impassable(t) {
                    if !gm.has_owner(t) && gm.has_fallout(t) {
                        continue;
                    }
                    let owner = gm.owner_id(t);
                    if owner != small && !set.contains(&owner) {
                        set.push(owner);
                    }
                }
            }
        }
        // shoreReachableNeighbors: every 10th shore border tile (the
        // post-increment counts only shore tiles), then the 5-steps-away land
        // tile in each immediately-water cardinal direction.
        let mut shore_idx = 0.0f64;
        for &b in &self.players[pidx].border_tiles {
            if !gm.is_shore(b) {
                continue;
            }
            let cur = shore_idx;
            shore_idx += 1.0;
            if cur % 10.0 != 0.0 {
                continue;
            }
            let bx = gm.x(b);
            let by = gm.y(b);
            for d in 0..4usize {
                let (dx, dy) = match d {
                    0 => (0.0, -1.0),  // N
                    1 => (0.0, 1.0),   // S
                    2 => (-1.0, 0.0),  // W
                    _ => (1.0, 0.0),   // E
                };
                let x1 = bx + dx;
                let y1 = by + dy;
                if !gm.is_valid_coord(x1, y1) || !gm.is_water(gm.tile_ref(x1, y1)) {
                    continue;
                }
                let nx = bx + dx * 5.0;
                let ny = by + dy * 5.0;
                if !gm.is_valid_coord(nx, ny) {
                    continue;
                }
                let tile = gm.tile_ref(nx, ny);
                if !gm.is_land(tile) || gm.is_impassable(tile) {
                    continue;
                }
                if !gm.has_owner(tile) && gm.has_fallout(tile) {
                    continue;
                }
                let owner = gm.owner_id(tile);
                if owner != small && !set.contains(&owner) {
                    set.push(owner);
                }
            }
        }
        set
    }

    /// `UnitImpl` ctor + table registration (op 29): `_lastTile = _tile`
    /// (TS 60), the `maxHealth ?? 1` read (unobservable here), the
    /// TransportShip state init, and the `stats().unitBuild` switch (only
    /// Warship/Port/MissileSilo/DefensePost/SAMLauncher/City/Factory — none
    /// of which the M1e scenarios define). Payload: `[unitIdx]`.
    fn op_unit_define(&mut self, a: &[f64], o: &mut Vec<f64>) {
        let mut c = C { a, i: 0 };
        let id = c.f();
        let utype = read_f64_str(&mut c);
        let tile = c.f();
        let owner_pidx = c.f();
        let active = c.f() != 0.0;
        let uc = c.f() != 0.0;
        let owner = if owner_pidx >= 0.0 {
            self.players[owner_pidx as usize].small_id
        } else {
            0.0 // TerraNullius singleton
        };
        let ui = self.units.len();
        self.units.push(LoopUnit {
            id,
            utype,
            tile,
            last_tile: tile,
            owner,
            active,
            uc,
            in_map: false,
        });
        o.push(0.0);
        o.push(ui as f64);
    }

    /// `GameImpl.addUnit` (TS 1135-1139): `_unitsVersion++`, the real
    /// `unitGrid.addUnit`, and `unitMap.set(u.id(), u)` (mirrored by
    /// `in_map`).
    fn op_unit_add(&mut self, ui: usize, o: &mut Vec<f64>) {
        self.units_version += 1.0;
        let (tile, utype) = {
            let u = &self.units[ui];
            (u.tile, u.utype.clone())
        };
        let key: Vec<u16> = utype.encode_utf16().collect();
        self.unit_grid
            .as_mut()
            .expect("unit grid built")
            .add_unit_cell(tile, &key, ui as f64);
        self.units[ui].in_map = true;
        o.push(0.0);
    }

    /// `GameImpl.removeUnit` (TS 1140-1148): the version bump, the grid
    /// removal, the unitMap delete, the (always-miss) planDriven delete, and
    /// the railNetwork arm — inert because `hasTrainStation()` is ctor-false
    /// for every M1e unit type.
    fn op_unit_remove(&mut self, ui: usize, o: &mut Vec<f64>) {
        self.units_version += 1.0;
        let (tile, utype) = {
            let u = &self.units[ui];
            (u.tile, u.utype.clone())
        };
        let key: Vec<u16> = utype.encode_utf16().collect();
        self.unit_grid
            .as_mut()
            .expect("unit grid built")
            .remove_unit_cell(tile, &key, ui as f64);
        self.units[ui].in_map = false;
        o.push(0.0);
    }

    /// Unit move (op 32): the recorder mirrors the `UnitImpl.move` field
    /// dance (`_lastTile = _tile; _tile = newTile`) and then the real
    /// `GameImpl.updateUnitTile` → `unitGrid.updateUnitCell` — deliberately
    /// NOT `move()` itself, whose `onUnitMoved` would push a UnitUpdate into
    /// the updates stream (outside the M1e unit domain).
    fn op_unit_move(&mut self, ui: usize, new_tile: f64, o: &mut Vec<f64>) {
        let (tile, last_tile, utype) = {
            let u = &mut self.units[ui];
            u.last_tile = u.tile;
            u.tile = new_tile;
            (u.tile, u.last_tile, u.utype.clone())
        };
        let key: Vec<u16> = utype.encode_utf16().collect();
        self.unit_grid
            .as_mut()
            .expect("unit grid built")
            .update_unit_cell(tile, last_tile, &key, ui as f64);
        o.push(0.0);
    }

    /// Unit-nearby probe (op 33): the UnitGrid scans with no predicate (the
    /// M1e closures are always-true / absent — observationally identical to
    /// no predicate). `playerId` rides as a player index: the TS comparison
    /// `unit.owner().id() !== playerId` is per-player unique identity, so
    /// smallID equality is the exact mirror. Grid members are probed off the
    /// units table.
    #[allow(clippy::too_many_lines)] // one op, four scan modes
    fn op_unit_nearby(&mut self, a: &[f64], o: &mut Vec<f64>) {
        let mut c = C { a, i: 0 };
        let mode = c.f();
        let tile = c.f();
        let range = c.f();
        let n_types = c.f() as usize;
        let types: Vec<Vec<u16>> = (0..n_types)
            .map(|_| read_f64_str(&mut c).encode_utf16().collect())
            .collect();
        let pid = if c.f() != 0.0 {
            Some(self.players[c.f() as usize].small_id)
        } else {
            None
        };
        let include_uc = c.f() != 0.0;
        let units = &self.units;
        let probe =
            |refid: f64| -> Option<(bool, bool, f64, f64)> {
                units
                    .get(refid as usize)
                    .map(|u| (u.active, u.uc, u.owner, u.tile))
            };
        let grid = self.unit_grid.as_ref().expect("unit grid built");
        o.push(0.0);
        match mode {
            // 0 = the array branch (`cy -> cx -> types -> unitSet`), 3 = the
            // scalar-type branch (`cy -> cx -> unitSet`).
            0.0 | 3.0 => {
                let array_branch = mode == 0.0;
                for (refid, d2) in grid.nearby_units_probe(
                    tile,
                    range,
                    &types,
                    array_branch,
                    pid,
                    include_uc,
                    &probe,
                ) {
                    o.push(refid);
                    o.push(d2);
                }
            }
            1.0 => {
                o.push(if grid.has_unit_nearby_probe(
                    tile,
                    range,
                    &types[0],
                    pid,
                    include_uc,
                    &probe,
                ) {
                    1.0
                } else {
                    0.0
                });
            }
            _ => {
                o.push(if grid.any_unit_nearby_probe(
                    tile,
                    range,
                    &types,
                    pid,
                    include_uc,
                    &probe,
                ) {
                    1.0
                } else {
                    0.0
                });
            }
        }
    }

    /// Units snapshot (op 34): `unitsVersion` + per defined unit
    /// `(id, type, tile, inMap)` — `id` is the ctor-assigned number
    /// (UnitImpl.ts 56), `inMap` mirrors `unitMap.has(id)`.
    fn op_units_snapshot(&mut self, o: &mut Vec<f64>) {
        o.push(0.0);
        o.push(self.units_version);
        o.push(self.units.len() as f64);
        for u in &self.units {
            o.push(u.id);
            push_str(o, &u.utype);
            o.push(u.tile);
            o.push(if u.in_map { 1.0 } else { 0.0 });
        }
    }

    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut o: Vec<f64> = Vec::new();
        match kind {
            0 => self.construct(args, &mut o),
            1 => {
                let mut c = C { a: args, i: 0 };
                let tile = c.f();
                let value = c.f() != 0.0;
                self.op_set_fallout(tile, value, &mut o);
            }
            2 => {
                let mut c = C { a: args, i: 0 };
                let tile = c.f();
                self.op_set_water(tile, &mut o);
            }
            3 => {
                let mut c = C { a: args, i: 0 };
                let tile = c.f();
                self.op_queue_water(tile, &mut o);
            }
            4 => {
                let mut c = C { a: args, i: 0 };
                let ticks = c.f();
                self.op_water_tick(ticks, &mut o);
            }
            5 => self.op_drain(&mut o),
            6 => self.op_digest(&mut o),
            7 => self.construct_game(args, &mut o),
            8 => self.op_snapshot(&mut o),
            9 => self.op_execute_next_tick(&mut o),
            10 => self.op_stats_snapshot(&mut o),
            11 => {
                let mut c = C { a: args, i: 0 };
                let pidx = c.f();
                let tile = c.f();
                self.op_set_spawn_tile(pidx, tile, &mut o);
            }
            12 => {
                let mut c = C { a: args, i: 0 };
                let pidx = c.f();
                let amount = c.f();
                self.op_add_troops(pidx, amount, &mut o);
            }
            13 => {
                let mut c = C { a: args, i: 0 };
                let pidx = c.f();
                let tile = c.f();
                self.op_conquer(pidx, tile, &mut o);
            }
            14 => Self::op_drain_quads(&mut self.player_quads, &mut o),
            15 => Self::op_drain_quads(&mut self.attack_quads, &mut o),
            16 => self.op_end_spawn_phase(&mut o),
            17 => self.op_probe_spawn(&mut o),
            18 => self.op_add_exec(args, &mut o),
            19 => self.op_exec_info(&mut o),
            20 => {
                let mut c = C { a: args, i: 0 };
                let pidx = c.f();
                let tidx = c.f();
                let troops = c.f();
                self.op_player_create_attack(pidx, tidx, troops, &mut o);
            }
            21 => {
                let mut c = C { a: args, i: 0 };
                let tile = c.f();
                self.op_relinquish(tile, &mut o);
            }
            22 => {
                let mut c = C { a: args, i: 0 };
                let req = c.f() as usize;
                let rec = c.f() as usize;
                self.op_create_alliance_request(req, rec, &mut o);
            }
            23 => {
                let mut c = C { a: args, i: 0 };
                let ar_idx = c.f() as usize;
                let accept = c.f() != 0.0;
                self.op_alliance_reply(ar_idx, accept, &mut o);
            }
            24 => {
                let mut c = C { a: args, i: 0 };
                let breaker = c.f() as usize;
                let ai = c.f() as usize;
                self.op_break_alliance(breaker, ai, &mut o);
            }
            25 => {
                let mut c = C { a: args, i: 0 };
                let ai = c.f() as usize;
                self.op_expire_alliance(ai, &mut o);
            }
            26 => {
                let mut c = C { a: args, i: 0 };
                let pidx = c.f() as usize;
                self.op_remove_alliances_silently(pidx, &mut o);
            }
            27 => self.op_alliance_snapshot(&mut o),
            28 => {
                let mut c = C { a: args, i: 0 };
                let pidx = c.f() as usize;
                self.op_nearby_probe(pidx, &mut o);
            }
            29 => self.op_unit_define(args, &mut o),
            30 => {
                let mut c = C { a: args, i: 0 };
                let ui = c.f() as usize;
                self.op_unit_add(ui, &mut o);
            }
            31 => {
                let mut c = C { a: args, i: 0 };
                let ui = c.f() as usize;
                self.op_unit_remove(ui, &mut o);
            }
            32 => {
                let mut c = C { a: args, i: 0 };
                let ui = c.f() as usize;
                let tile = c.f();
                self.op_unit_move(ui, tile, &mut o);
            }
            33 => self.op_unit_nearby(args, &mut o),
            34 => self.op_units_snapshot(&mut o),
            _ => panic!("gl: unknown kind {kind}"),
        }
        o
    }
}

fn push_err(o: &mut Vec<f64>, m: &str) {
    o.push(1.0);
    push_str(o, m);
}

// ------------------------------------------------------------ M1e helpers

/// smallID of the side opposite `pidx` on this alliance (PlayerImpl.ts 287:
/// `alliances.map((a) => a.other(this).smallID())`).
fn alliance_other_small(a: &LoopAlliance, pidx: usize, players: &[LoopPlayer]) -> f64 {
    let other = if a.requestor == pidx {
        a.recipient
    } else {
        a.requestor
    };
    players[other].small_id
}

/// `AllianceRequestUpdate` (GameUpdates.ts 283-288): `{type: 5, requestorID,
/// recipientID, createdAt}` — the IDs are smallIDs (pidx + 1 here).
fn request_update_value(req: usize, rec: usize, created: f64) -> JsonValue {
    JsonValue::Obj(vec![
        ("type".to_string(), JsonValue::Num(5.0)),
        ("requestorID".to_string(), JsonValue::Num(req as f64 + 1.0)),
        ("recipientID".to_string(), JsonValue::Num(rec as f64 + 1.0)),
        ("createdAt".to_string(), JsonValue::Num(created)),
    ])
}

/// `AllianceView` serialization (PlayerImpl.ts 290-299): field order is the
/// TS object literal's — `{id, other, createdAt, expiresAt,
/// hasExtensionRequest}`.
fn alliance_wire_value(w: &AllianceWire) -> JsonValue {
    JsonValue::Obj(vec![
        ("id".to_string(), JsonValue::Num(w.id)),
        ("other".to_string(), JsonValue::Str(w.other.clone())),
        ("createdAt".to_string(), JsonValue::Num(w.created_at)),
        ("expiresAt".to_string(), JsonValue::Num(w.expires_at)),
        ("hasExtensionRequest".to_string(), JsonValue::Bool(w.has_ext)),
    ])
}

/// Read a `[len, u0, ..]` UTF-16 wire string through the [`C`] cursor
/// (mirror of [`push_str`], via the shared lossy reader).
fn read_f64_str(c: &mut C) -> String {
    read_str(c.a, &mut c.i)
}

/// TS bigint → integer decimal string (PlayerImpl.toString's troops field;
/// integral f64 below 2^53).
fn f64_to_bigint_str(v: f64) -> String {
    format!("{}", v as i64)
}

/// `createGameUpdatesMap` (GameImpl.ts 1456-1464): the numeric enum members
/// 0-23 in ascending order, each an empty array — the ctor's initial
/// `updates` value and every tick's fresh map.
fn empty_updates() -> JsonValue {
    JsonValue::Obj(
        (0..24)
            .map(|i| (i.to_string(), JsonValue::Arr(Vec::new())))
            .collect(),
    )
}

// ------------------------------------------------------------ M1a helpers

/// `GameImpl.addPlayer` (TS 680-692) + the `PlayerImpl` ctor state the
/// snapshot reads (TS 191-201): `_troops = toInt(startManpower)`,
/// `_gold = config.startingGold(...)`,
/// `_pseudo_random = new PseudoRandom(simpleHash(id))`.
fn add_loop_player(
    players: &mut Vec<LoopPlayer>,
    mp: &mut Vec<MP>,
    next_id: &mut f64,
    cfg: &LoopConfig,
    player_teams: &[String],
    info: LoopPlayerInfo,
    team: Option<String>,
) {
    let team = match team {
        Some(t) => Some(t),
        None => maybe_assign_team(cfg, player_teams, &info),
    };
    let troops = to_int_clamped(start_manpower(cfg, &info));
    let gold = starting_gold(cfg, &info);
    let rng = PseudoRandom::new(simple_hash(&info.id));
    let small_id = *next_id;
    *next_id += 1.0;
    // PlayerInfo ctor's displayName formatting (PlayerInfo.ts, mirrored by
    // format_player_display_name): a truthy clanTag prepends the bracket.
    let display_name = format_player_display_name(&info.name, info.clan_tag.as_deref());
    // The stats facade table stays parallel to `players` (kicked players are
    // never added on either side).
    mp.push(MP {
        cid: info.client_id.clone(),
        ptype: match info.player_type {
            PlayerType::Human => "HUMAN",
            PlayerType::Bot => "BOT",
            PlayerType::Nation => "NATION",
        }
        .to_string(),
        is_player: true,
    });
    players.push(LoopPlayer {
        info,
        small_id,
        team,
        display_name,
        troops,
        gold,
        rng,
        rng_draws: 0,
        tiles: Vec::new(),
        border_tiles: Vec::new(),
        spawn_tile: None,
        tile_change_version: 0.0,
        last_tile_change: 0.0,
        last_sent: None,
        out_attacks: Vec::new(),
        in_attacks: Vec::new(),
        alliances: Vec::new(),
        past_outgoing: Vec::new(),
        marked_traitor_tick: -1.0,
        betrayal_count: 0.0,
        nearby_memo: None,
    });
}

/// `GameImpl.maybeAssignTeam` (TS 694-703). The Bot / hash branches are
/// unreachable in the ctor flows (FFA returns `null` first; the Team-mode
/// addPlayers always pass an explicit team) but are ported for the
/// later-join callers that arrive with M1c+.
fn maybe_assign_team(
    cfg: &LoopConfig,
    player_teams: &[String],
    info: &LoopPlayerInfo,
) -> Option<String> {
    if !cfg.game_mode_team {
        return None;
    }
    if info.player_type == PlayerType::Bot {
        return Some("Bot".to_string());
    }
    let idx = simple_hash(&info.id) % player_teams.len() as f64;
    // JS `arr[key]` with a negative or fractional float key is `undefined`;
    // Rust's `as usize` would truncate/saturate instead.
    if !(idx.is_finite() && idx >= 0.0 && idx.fract() == 0.0) {
        return None;
    }
    player_teams.get(idx as usize).cloned()
}

/// `Config.startManpower` (Config.ts 1000-1019).
fn start_manpower(cfg: &LoopConfig, info: &LoopPlayerInfo) -> f64 {
    match info.player_type {
        PlayerType::Bot => 10_000.0,
        PlayerType::Nation => match cfg.difficulty {
            0 => 12_500.0,  // Easy
            1 => 18_750.0,  // Medium
            2 => 25_000.0,  // Hard — like humans
            _ => 31_250.0,  // Impossible
        },
        PlayerType::Human => {
            if has_infinite_troops_for_info(cfg, info) {
                1_000_000.0
            } else {
                25_000.0
            }
        }
    }
}

/// `Config.hasInfiniteTroopsForInfo` (Config.ts 715-721).
fn has_infinite_troops_for_info(cfg: &LoopConfig, info: &LoopPlayerInfo) -> bool {
    if cfg.infinite_troops {
        return true;
    }
    cfg.hc_infinite_troops && info.is_lobby_creator
}

/// `Config.startingGold` (Config.ts 436-441) + `startingGoldFor` (743-750).
/// TS gold is `bigint`; the snapshot's `Number(gold())` read stays exact for
/// the integral config values fed here.
fn starting_gold(cfg: &LoopConfig, info: &LoopPlayerInfo) -> f64 {
    if info.player_type == PlayerType::Bot {
        return 0.0;
    }
    let base = cfg.starting_gold.unwrap_or(0.0);
    match cfg.hc_starting_gold {
        // `hc?.startingGold && playerInfo.isLobbyCreator` — truthy check.
        Some(hc) if hc != 0.0 && info.is_lobby_creator => base + hc,
        _ => base,
    }
}

/// `toInt` (Util.ts 397-405): ±Infinity clamps to ±MAX_SAFE_INTEGER,
/// everything else floors.
fn to_int_clamped(x: f64) -> f64 {
    const MAX_SAFE: f64 = 9_007_199_254_740_991.0;
    if x == f64::INFINITY {
        return MAX_SAFE;
    }
    if x == f64::NEG_INFINITY {
        return -MAX_SAFE;
    }
    x.floor()
}

/// The `resolveTeamsList` throw messages as the recorder's `catch` reports
/// them (`construct_game` maps them back 1:1).
fn resolve_err_msg(cfg: &TeamCountConfig, e: ResolveTeamsError) -> String {
    match e {
        // Only the Num path raises TooFewTeams (team_assignment.rs 398).
        ResolveTeamsError::TooFewTeams => match cfg {
            TeamCountConfig::Num(n) => format!("Too few teams: {n}"),
            _ => unreachable!("TooFewTeams outside the Num path"),
        },
        // Only Other reaches UnknownConfig (team_assignment.rs 392).
        ResolveTeamsError::UnknownConfig => match cfg {
            TeamCountConfig::Other(s) => format!("Unknown TeamCountConfig {s}"),
            _ => unreachable!("UnknownConfig outside the Other path"),
        },
        ResolveTeamsError::InvalidLength => "Invalid array length".to_string(),
    }
}

/// Optional f64 arg: -1 is the absent sentinel.
fn opt_f64(c: &mut C) -> Option<f64> {
    let v = c.f();
    if v == -1.0 {
        None
    } else {
        Some(v)
    }
}

/// Optional string arg: -1 absent, -2 + `read_str` present.
fn opt_str(c: &mut C) -> Option<String> {
    let v = c.f();
    if v == -1.0 {
        None
    } else {
        debug_assert_eq!(v, -2.0);
        Some(read_str(c.a, &mut c.i))
    }
}

/// `TeamCountConfig` wire tag (gen_m1_vectors.mjs `PT_TAG`).
fn read_team_count(c: &mut C) -> TeamCountConfig {
    match c.f() as u8 {
        0 => TeamCountConfig::Num(c.f()),
        1 => TeamCountConfig::Duos,
        2 => TeamCountConfig::Trios,
        3 => TeamCountConfig::Quads,
        4 => TeamCountConfig::HumansVsNations,
        _ => TeamCountConfig::Other(read_str(c.a, &mut c.i)),
    }
}

/// PlayerInfo run (gen_m1_vectors.mjs `encPlayer`).
fn read_infos(c: &mut C) -> Vec<LoopPlayerInfo> {
    let n = c.f() as usize;
    let mut infos = Vec::with_capacity(n);
    for _ in 0..n {
        let id = read_str(c.a, &mut c.i);
        let name = read_str(c.a, &mut c.i);
        let player_type = match c.f() as u8 {
            0 => PlayerType::Human,
            1 => PlayerType::Bot,
            _ => PlayerType::Nation,
        };
        let client_id = opt_str(c);
        let is_lobby_creator = c.f() != 0.0;
        let clan_tag = opt_str(c);
        let friends_n = c.f() as usize;
        let mut friends = Vec::with_capacity(friends_n);
        for _ in 0..friends_n {
            friends.push(read_str(c.a, &mut c.i));
        }
        let team_index = opt_f64(c);
        let nation_flag = opt_str(c);
        infos.push(LoopPlayerInfo {
            id,
            name,
            player_type,
            client_id,
            is_lobby_creator,
            clan_tag,
            friends,
            team_index,
            nation_flag,
        });
    }
    infos
}
