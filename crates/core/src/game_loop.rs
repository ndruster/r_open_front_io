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
}

/// The slice of a full `PlayerUpdate` that can change in the M1b domain.
/// Everything else `toFullUpdate` emits is constant per player (identity /
/// config-driven fields) or an always-empty collection, so `diffPlayerUpdate`
/// can only ever surface these fields — the diff compares exactly these, in
/// the `setIfDifferent` order (isAlive 100 / hasSpawned 123 / spawnTile 124),
/// and the quint compare uses the four stat fields (TS 230-247).
#[derive(Clone, Copy, PartialEq)]
struct PuCore {
    is_alive: bool,
    has_spawned: bool,
    spawn_tile: Option<f64>,
    tiles_owned: f64,
    gold: f64,
    troops: f64,
    gold_earned: f64,
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
    /// `attackTroopsQuads` — always empty in the M1b domain (no attacks).
    attack_quads: Vec<f64>,
    /// `GameImpl.startTick` — `None` is the spawn phase (`inSpawnPhase()`
    /// true); `endSpawnPhase` stamps it with `_ticks` (M1c).
    start_tick: Option<f64>,
    /// The live `GameImpl.updates` map — ctor-fresh at construct, rebuilt by
    /// every `executeNextTick`, and appended to in place by `endSpawnPhase`
    /// (the next tick's `createGameUpdatesMap` wipes it, M1c).
    last_updates: JsonValue,
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
            start_tick: None,
            last_updates: empty_updates(),
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
        };
        let humans = read_infos(&mut c);
        let nations = read_infos(&mut c);
        if !c.done() {
            panic!("gl construct_game: trailing args");
        }

        // Ctor body (TS 118-151): the collaborators without construction-
        // observable state (TerraNulliusImpl / UnitGrid / SharedWaterCache /
        // RailNetwork) are not modeled; the water manager is (ops 3/4/6).
        let map = GameMap::new(w, h, terrain, nl);
        let mini = GameMap::new(mw, mh, mterrain, mnl);
        let water = WaterManager::new(map, mini, cfg.disable_nav_mesh);

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
        self.start_tick = None;
        self.last_updates = empty_updates();
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

    /// `PlayerImpl.toFullUpdate` (TS 267-401) as a `JsonValue` tree — the 40
    /// keys in TS declaration order, with `team` / `spawnTile` as `Undef`
    /// (JSON-stringify omits them) exactly when TS holds `undefined`. Every
    /// collection is empty in the M1b domain (no alliances / attacks /
    /// embargoes / units / execs); `embargoes` is the Set → `"{}"` shape.
    fn pu_full_value(p: &LoopPlayer) -> JsonValue {
        let opt_str_v = |s: &Option<String>| match s {
            Some(v) => JsonValue::Str(v.clone()),
            None => JsonValue::Null,
        };
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
            ("allies".to_string(), JsonValue::Arr(Vec::new())),
            // `embargoes` is a `Set` → JSON.stringify writes `"{}"`.
            ("embargoes".to_string(), JsonValue::Obj(Vec::new())),
            ("isTraitor".to_string(), JsonValue::Bool(false)),
            ("traitorRemainingTicks".to_string(), JsonValue::Num(0.0)),
            ("inDoomsdayClock".to_string(), JsonValue::Bool(false)),
            ("isDecaying".to_string(), JsonValue::Bool(false)),
            (
                "markedDoomsdayClockTick".to_string(),
                JsonValue::Num(-1.0),
            ),
            ("targets".to_string(), JsonValue::Arr(Vec::new())),
            ("outgoingEmojis".to_string(), JsonValue::Arr(Vec::new())),
            ("outgoingAttacks".to_string(), JsonValue::Arr(Vec::new())),
            ("incomingAttacks".to_string(), JsonValue::Arr(Vec::new())),
            (
                "outgoingAllianceRequests".to_string(),
                JsonValue::Arr(Vec::new()),
            ),
            ("alliances".to_string(), JsonValue::Arr(Vec::new())),
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
            ("betrayals".to_string(), JsonValue::Num(0.0)),
            ("lastDeleteUnitTick".to_string(), JsonValue::Num(-1.0)),
            (
                "isLobbyCreator".to_string(),
                JsonValue::Bool(p.info.is_lobby_creator),
            ),
        ])
    }

    /// `diffPlayerUpdate` (GameUpdateUtils.ts 79-162) restricted to the
    /// M1b-reachable mutable fields, in `setIfDifferent` order: `{"type":2,
    /// "id":..}` plus the changed ones (spawnTile always numeric when it
    /// changes — the None→Some transition; Some→None never happens).
    fn pu_diff_value(p: &LoopPlayer, prev: &PuCore, next: &PuCore) -> Option<JsonValue> {
        let mut changed: Vec<(String, JsonValue)> = Vec::new();
        if prev.is_alive != next.is_alive {
            changed.push(("isAlive".to_string(), JsonValue::Bool(next.is_alive)));
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

    /// `GameImpl.executeNextTick` (TS 488-546) — the real loop over the
    /// empty-exec game: fresh updates map, tileUpdatePairs reset, per-player
    /// recordTickSample + toUpdate, the %10 Hash update, the real
    /// waterManager tick flush, `_ticks++`. Emits
    /// `[0, ticksAfter, gameHash, updatesJSON(encS)]`.
    fn op_execute_next_tick(&mut self, o: &mut Vec<f64>) {
        let tick = self.ticks;
        // TS 489-490: fresh `createGameUpdatesMap()` + tileUpdatePairs reset
        // (drops whatever conquer/ops accumulated between ticks).
        self.tile_updates.clear();
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
            // players are skipped. `alliances().length` is always 0 here.
            if alive && spawned {
                let mut trace: Vec<f64> = Vec::new(); // facade trace discarded
                let _ = self.stats.record_tick_sample(
                    &mut trace,
                    &self.mp,
                    i,
                    &Val::Num(tiles_owned),
                    &Val::Num(troops),
                    0.0,
                );
            }
            // TS 226-264 `toUpdate`: full on the first call, then quints +
            // attack deltas + field diff.
            let core = PuCore {
                is_alive: alive,
                has_spawned: spawned,
                spawn_tile: self.players[i].spawn_tile,
                tiles_owned,
                gold,
                troops,
                gold_earned: 0.0,
            };
            let prev = self.players[i].last_sent.replace(core);
            let pu = match prev {
                None => Self::pu_full_value(&self.players[i]),
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
                    // packAttackTroopDeltas: both sides are the shared empty
                    // singleton every tick (no attack ops), so the refid
                    // equality early-returns — no quads, no updates.
                    match Self::pu_diff_value(&self.players[i], &prev, &core) {
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
            _ => panic!("gl: unknown kind {kind}"),
        }
        o
    }
}

fn push_err(o: &mut Vec<f64>, m: &str) {
    o.push(1.0);
    push_str(o, m);
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
