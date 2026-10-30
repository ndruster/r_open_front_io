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
use crate::js_json::{push_str, read_str};
use crate::pseudo_random::PseudoRandom;
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
/// and team `addPlayer` (TS 680-692) assigns.
pub struct LoopPlayer {
    pub info: LoopPlayerInfo,
    pub small_id: f64,
    pub team: Option<String>,
    pub troops: f64,
    pub gold: f64,
    pub rng: PseudoRandom,
    pub rng_draws: u32,
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
        let mut next_id = 1.0;
        if !cfg.game_mode_team {
            // FFA (TS 164-168): humans then nations, maybeAssignTeam team.
            for info in &humans {
                add_loop_player(
                    &mut players, &mut next_id, &cfg, &player_teams, info.clone(), None,
                );
            }
            for info in &nations {
                add_loop_player(
                    &mut players, &mut next_id, &cfg, &player_teams, info.clone(), None,
                );
            }
        } else if matches!(cfg.player_teams, TeamCountConfig::HumansVsNations) {
            // HumansVsNations (TS 169-178): fixed colored teams.
            for info in &humans {
                add_loop_player(
                    &mut players, &mut next_id, &cfg, &player_teams, info.clone(),
                    Some("Humans".to_string()),
                );
            }
            for info in &nations {
                add_loop_player(
                    &mut players, &mut next_id, &cfg, &player_teams, info.clone(),
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
                        &mut players, &mut next_id, &cfg, &player_teams, all[idx].clone(),
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
        self.next_player_id = next_id;
        self.player_teams = player_teams;
        self.territory_version = 0.0;
        self.ticks = 0.0;
        self.tile_updates.clear();
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
            _ => panic!("gl: unknown kind {kind}"),
        }
        o
    }
}

fn push_err(o: &mut Vec<f64>, m: &str) {
    o.push(1.0);
    push_str(o, m);
}

// ------------------------------------------------------------ M1a helpers

/// `GameImpl.addPlayer` (TS 680-692) + the `PlayerImpl` ctor state the
/// snapshot reads (TS 191-201): `_troops = toInt(startManpower)`,
/// `_gold = config.startingGold(...)`,
/// `_pseudo_random = new PseudoRandom(simpleHash(id))`.
fn add_loop_player(
    players: &mut Vec<LoopPlayer>,
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
    players.push(LoopPlayer {
        info,
        small_id,
        team,
        troops,
        gold,
        rng,
        rng_draws: 0,
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
        infos.push(LoopPlayerInfo {
            id,
            name,
            player_type,
            client_id,
            is_lobby_creator,
            clan_tag,
            friends,
            team_index,
        });
    }
    infos
}
