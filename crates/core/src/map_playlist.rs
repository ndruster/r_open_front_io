//! Port of the deterministic layer of `src/server/MapPlaylist.ts`: the
//! module tables, the playlist generation chain (`buildMapsList` /
//! `playlistKey` / `addNextMapNonConsecutive` / `generateNewPlaylist` /
//! `getNextMap`) and the pure team-count helpers.
//!
//! The TS `Date.now()` seed of `generateNewPlaylist` is scripted through
//! `globalThis.__MP_SEED` in `tools/ts_load.mjs` (precedent:
//! [`crate::listing_state`]); the port takes the same seed as harness state
//! (op kind 1). `log.info` / `log.warn` are not observable in the port
//! domain, but the attempt count they carry IS: the capture rewrites the
//! logger into a message sink and the golden `res` pins the exact
//! `Generated map playlist in N attempts` / fallback strings, so the Rust
//! replay must land on the same attempt count (and fallback path) as the
//! real TS for the same seed.
//!
//! Faithfulness notes:
//!
//! * `Duos` / `Trios` / `Quads` / `HumansVsNations` are STRING constants in
//!   `Game.ts` (`"Duos"`, `"Trios"`, `"Quads"`, `"Humans Vs Nations"`), so
//!   `typeof playerTeams !== "number"` in
//!   `adjustTeamCountForPlayerCapacity` is TRUE for all four presets and
//!   they pass through untouched; only the numeric team counts 2..7 reach
//!   the `supportsTeamPlayerCount` gate (pinned by `mpl_adjust_capacity_*`).
//! * `buildMapsList`'s frequency switch is asymmetric: the `ffa` / `team`
//!   cases take the per-mode frequency when it is `>= 0` (a deliberate 0
//!   opts the map OUT), while the `special` case only counts the per-mode
//!   fallback when it is `> 0`, so opted-out maps still reach special via
//!   `multiplayerFrequency` (pinned by `mpl_build_*` against the real
//!   127-map table, which contains both `-1` and `0` frequencies).
//! * `addNextMapNonConsecutive` splices the FIRST source entry not in the
//!   last-5 window (`playlist.slice(-5)`) and pushes it; the source array
//!   is mutated in place and the caller's `while (source.length > 0)` loop
//!   drains it (the splice order is pinned by `mpl_addnext_*`).
//! * `getNextMap` refills a queue only when it is empty, then `shift()`s;
//!   the refill consumes the current `__MP_SEED`, so the four queues'
//!   contents are a pure function of (seed, op sequence).
//! * `calculateMapPlayerCounts` uses JS `Math.round` (half-up toward
//!   +Infinity, [`crate::jsnum::js_round`]) — pinned with `.5` land-tile
//!   values (`mpl_counts_*`).
//! * `SAM_CONSTRUCTION_TICKS` (= `30 * 10`) is inlined from
//!   `src/core/configuration/Config.ts` by the prepare block (the heavy
//!   Config graph stays unprepared); the value is pinned by the upstream
//!   sync check.
//! * `gameConfig` / `rollConfig` / `getSpecialConfig` / `getTeamCount` /
//!   `getRandomSpecialGameModifiers` / `get1v1Config` / `get2v2Config` /
//!   `lobbyMaxPlayers` / `supportsCompactMapForTeams` /
//!   `getCrowdedMaxPlayers` are NOT ported here (Math.random orchestration,
//!   the `.sort(() => Math.random() - 0.5)` V8-ordering problem, or the
//!   async `getMapLandTiles` facade) — a later phase.

use crate::js_json::{push_str, push_val, read_str, read_val, JsVal};
use crate::jsnum::js_round;
use crate::maps_gen::MAPS;
use crate::pseudo_random::PseudoRandom;

/// `CROWDED_COMPACT_PLAYER_COUNT`.
pub const CROWDED_COMPACT_PLAYER_COUNT: f64 = 60.0;
/// `CROWDED_PLAYER_COUNT`.
pub const CROWDED_PLAYER_COUNT: f64 = 125.0;
/// `TRUSTED_PUBLIC_EVERY`.
pub const TRUSTED_PUBLIC_EVERY: f64 = 7.0;
/// `TRUSTED_MAX_PLAYER_COUNT`.
pub const TRUSTED_MAX_PLAYER_COUNT: f64 = 25.0;
/// `SPECIAL_TEAM_FORCE_CHANCE`.
pub const SPECIAL_TEAM_FORCE_CHANCE: f64 = 0.75;
/// `SAM_CONSTRUCTION_TICKS` from `core/configuration/Config.ts`
/// (`30 * 10`), inlined by the prepare block.
const SAM_CONSTRUCTION_TICKS: f64 = 30.0 * 10.0;

/// `TeamCountConfig`: a numeric team count or one of the four string
/// presets (`Duos` / `Trios` / `Quads` / `"Humans Vs Nations"`).
#[derive(Clone, Debug, PartialEq)]
pub enum TeamCfg {
    Num(f64),
    Str(String),
}

impl TeamCfg {
    fn from_jsval(v: &JsVal) -> Option<TeamCfg> {
        match v {
            JsVal::Num(n) => Some(TeamCfg::Num(*n)),
            JsVal::Str(s) => Some(TeamCfg::Str(s.clone())),
            JsVal::Undef | JsVal::Absent => None,
            other => Some(TeamCfg::Str(js_to_string(other))),
        }
    }

    fn to_jsval(&self) -> JsVal {
        match self {
            TeamCfg::Num(n) => JsVal::Num(*n),
            TeamCfg::Str(s) => JsVal::Str(s.clone()),
        }
    }

    fn as_str(&self) -> &str {
        match self {
            TeamCfg::Str(s) => s,
            TeamCfg::Num(_) => "",
        }
    }
}

/// JS `String(x)` for the non-string codec kinds (never fed by the S3
/// scenarios; kept total so the switch default cannot panic).
fn js_to_string(v: &JsVal) -> String {
    match v {
        JsVal::Null => "null".to_string(),
        JsVal::Bool(b) => b.to_string(),
        JsVal::Num(n) => n.to_string(),
        JsVal::Str(s) => s.clone(),
        _ => "undefined".to_string(),
    }
}

/// JS `Math.min` over the finite, non-negative domain the port uses.
fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a < b {
        a
    } else {
        b
    }
}

/// JS `Math.max` over the finite, non-negative domain the port uses.
fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a > b {
        a
    } else {
        b
    }
}

/// `TEAM_WEIGHTS` (declaration order; the special configs are the STRING
/// presets from `Game.ts`).
pub fn team_weights() -> Vec<(TeamCfg, f64)> {
    vec![
        (TeamCfg::Num(2.0), 10.0),
        (TeamCfg::Num(3.0), 10.0),
        (TeamCfg::Num(4.0), 10.0),
        (TeamCfg::Num(5.0), 10.0),
        (TeamCfg::Num(6.0), 10.0),
        (TeamCfg::Num(7.0), 10.0),
        (TeamCfg::Str("Duos".to_string()), 5.0),
        (TeamCfg::Str("Trios".to_string()), 7.5),
        (TeamCfg::Str("Quads".to_string()), 7.5),
        (TeamCfg::Str("Humans Vs Nations".to_string()), 20.0),
    ]
}

/// `SPECIAL_MODIFIER_POOL`: one entry per ticket in the draw pool.
pub static SPECIAL_MODIFIER_POOL: &[&str] = &[
    "isRandomSpawn",
    "isRandomSpawn",
    "isRandomSpawn",
    "isRandomSpawn",
    "isCompact",
    "isCompact",
    "isCompact",
    "isCompact",
    "isCrowded",
    "isCrowded",
    "isHardNations",
    "startingGold1M",
    "startingGold1M",
    "startingGold5M",
    "startingGold5M",
    "startingGold5M",
    "startingGold5M",
    "startingGold25M",
    "startingGold25M",
    "startingGold25M",
    "goldMultiplier",
    "goldMultiplier",
    "goldMultiplier",
    "goldMultiplier",
    "goldMultiplier",
    "goldMultiplier",
    "isAlliancesDisabled",
    "isNukesDisabled",
    "isSAMsDisabled",
    "isPeaceTime",
    "isWaterNukes",
    "isWaterNukes",
    "isWaterNukes",
    "isWaterNukes",
    "isDoomsdayClock",
    "isDoomsdayClock",
    "isDoomsdayClock",
    "isDoomsdayClock",
];

/// `DOOMSDAY_ROTATION_SPEEDS`.
pub static DOOMSDAY_ROTATION_SPEEDS: &[&str] =
    &["slow", "normal", "fast", "veryfast"];

/// `MUTUALLY_EXCLUSIVE_MODIFIERS` (pair order is observable through the
/// excluded-modifier push order in the later `getSpecialConfig` port).
pub static MUTUALLY_EXCLUSIVE_MODIFIERS: &[(&str, &str)] = &[
    ("startingGold5M", "startingGold25M"),
    ("startingGold5M", "startingGold1M"),
    ("startingGold25M", "startingGold1M"),
    ("isHardNations", "startingGold25M"),
    ("isNukesDisabled", "isSAMsDisabled"),
    ("isNukesDisabled", "isWaterNukes"),
];

/// `SPECIAL_TEAM_MAPS`: the `allMaps` entries declaring a
/// `specialTeamCount`, in table (insertion) order.
pub fn special_team_maps() -> Vec<(&'static str, f64)> {
    MAPS.iter()
        .filter(|m| m.special_team_count.is_some())
        .map(|m| (m.type_, m.special_team_count.unwrap()))
        .collect()
}

/// `buildMapsList(type, mode)`: the frequency switch with the `>= 0` vs
/// `> 0` asymmetry, iterating the real map table in order.
pub fn build_maps_list(type_: &str, mode: Option<&str>) -> Vec<&'static str> {
    let mut maps: Vec<&'static str> = Vec::new();
    for map_info in MAPS.iter() {
        let freq: f64 = match type_ {
            "ffa" => {
                if map_info.ffa_frequency >= 0.0 {
                    map_info.ffa_frequency
                } else {
                    map_info.multiplayer_frequency
                }
            }
            "team" => {
                if map_info.team_frequency >= 0.0 {
                    map_info.team_frequency
                } else {
                    map_info.multiplayer_frequency
                }
            }
            _ => {
                // "special"
                if map_info.special_frequency >= 0.0 {
                    map_info.special_frequency
                } else if mode == Some("Team") {
                    if map_info.team_frequency > 0.0 {
                        map_info.team_frequency
                    } else {
                        map_info.multiplayer_frequency
                    }
                } else if map_info.ffa_frequency > 0.0 {
                    map_info.ffa_frequency
                } else {
                    map_info.multiplayer_frequency
                }
            }
        };
        // JS `for (let i = 0; i < freq; i++)`: 0 or negative pushes nothing.
        let mut i = 0.0;
        while i < freq {
            maps.push(map_info.type_);
            i += 1.0;
        }
    }
    maps
}

/// `playlistKey(type, mode)`.
pub fn playlist_key(type_: &str, mode: Option<&str>) -> String {
    if type_ == "special" {
        return if mode == Some("Team") {
            "specialTeam".to_string()
        } else {
            "specialFfa".to_string()
        };
    }
    type_.to_string()
}

/// Queue index for a `PlaylistKey` (dump order: ffa, team, specialFfa,
/// specialTeam — the `playlists` object literal's key order).
fn key_index(key: &str) -> usize {
    match key {
        "ffa" => 0,
        "team" => 1,
        "specialFfa" => 2,
        _ => 3,
    }
}

/// `addNextMapNonConsecutive(playlist, source)`: splice the first source
/// entry absent from the last-5 window, push it onto the playlist and
/// return `true`; `false` leaves both arrays untouched.
pub fn add_next_map_non_consecutive(
    playlist: &mut Vec<&'static str>,
    source: &mut Vec<&'static str>,
) -> bool {
    const NON_CONSECUTIVE_NUM: usize = 5;
    let last_maps = playlist[playlist.len().saturating_sub(NON_CONSECUTIVE_NUM)..].to_vec();
    for i in 0..source.len() {
        let map = source[i];
        if !last_maps.contains(&map) {
            source.remove(i);
            playlist.push(map);
            return true;
        }
    }
    false
}

/// `generateNewPlaylist(type, mode)` with the scripted `Date.now()` seed.
/// Returns the playlist plus the exact `log.info` / `log.warn` message the
/// TS emits (the attempt count is observable through it).
pub fn generate_new_playlist(
    seed: f64,
    type_: &str,
    mode: Option<&str>,
) -> (Vec<&'static str>, String) {
    let maps = build_maps_list(type_, mode);
    let mut rand = PseudoRandom::new(seed);
    let mut playlist: Vec<&'static str> = Vec::new();

    const NUM_ATTEMPTS: u32 = 10000;
    for attempt in 0..NUM_ATTEMPTS {
        playlist.clear();
        // Re-shuffle every attempt so retries can explore different orderings.
        let mut source = rand.shuffle_array(&maps);

        let mut success = true;
        while !source.is_empty() {
            if !add_next_map_non_consecutive(&mut playlist, &mut source) {
                success = false;
                break;
            }
        }

        if success {
            return (
                playlist,
                format!("Generated map playlist in {attempt} attempts"),
            );
        }
    }

    (
        rand.shuffle_array(&maps),
        "Failed to generate non-consecutive playlist after 10000 attempts, \
         falling back to shuffle"
            .to_string(),
    )
}

/// `calculateMapPlayerCounts(landTiles)`.
pub fn calculate_map_player_counts(land_tiles: f64) -> [f64; 3] {
    let round_to_nearest_5 = |n: f64| js_round(n / 5.0) * 5.0;

    let base = js_max(round_to_nearest_5((land_tiles / 1_000_000.0) * 50.0), 5.0);
    [
        base,
        round_to_nearest_5(base * 0.75),
        round_to_nearest_5(base * 0.5),
    ]
}

/// `playersPerTeam(adjustedPlayerCount, playerTeams)`.
pub fn players_per_team(p: f64, cfg: &TeamCfg) -> f64 {
    match cfg {
        TeamCfg::Str(s) => match s.as_str() {
            "Duos" => js_min(2.0, p),
            "Trios" => js_min(3.0, p),
            "Quads" => js_min(4.0, p),
            "Humans Vs Nations" => p,
            // switch default: `Math.floor(p / "other string")` is NaN.
            _ => f64::NAN,
        },
        TeamCfg::Num(t) => (p / t).floor(),
    }
}

/// `numberOfTeams(adjustedPlayerCount, playerTeams)`.
pub fn number_of_teams(p: f64, cfg: &TeamCfg) -> f64 {
    match cfg {
        TeamCfg::Str(s) => match s.as_str() {
            "Duos" => (p / 2.0).floor(),
            "Trios" => (p / 3.0).floor(),
            "Quads" => (p / 4.0).floor(),
            "Humans Vs Nations" => 2.0,
            // switch default returns the (string) config itself; the
            // `>= 2` comparison the only caller applies is NaN-false for
            // non-numeric strings, matching the never-fed case.
            _ => f64::NAN,
        },
        TeamCfg::Num(t) => *t,
    }
}

/// `supportsTeamPlayerCount(adjustedPlayerCount, playerTeams)`.
pub fn supports_team_player_count(p: f64, cfg: &TeamCfg) -> bool {
    players_per_team(p, cfg) >= 2.0 && number_of_teams(p, cfg) >= 2.0
}

/// `adjustForTeams(playerCount, numPlayerTeams)`. Rust `f64 %` has the
/// sign-of-dividend semantics of JS `%` for finite operands.
pub fn adjust_for_teams(p: f64, cfg: Option<&TeamCfg>) -> f64 {
    let Some(cfg) = cfg else { return p };
    let mut p = p;
    match cfg {
        TeamCfg::Str(s) => match s.as_str() {
            "Duos" => p -= p % 2.0,
            "Trios" => p -= p % 3.0,
            "Quads" => p -= p % 4.0,
            "Humans Vs Nations" => p = (p / 2.0).floor(),
            _ => p = f64::NAN,
        },
        TeamCfg::Num(t) => p -= p % t,
    }
    p
}

/// `adjustTeamCountForPlayerCapacity(playerTeams, unadjustedMaxPlayers)`:
/// the `typeof playerTeams !== "number"` gate passes for `undefined` AND
/// for the four string presets; only numeric counts reach the capacity
/// check.
pub fn adjust_team_count_for_player_capacity(
    cfg: Option<TeamCfg>,
    unadjusted: f64,
) -> Option<TeamCfg> {
    let numeric = match &cfg {
        Some(TeamCfg::Num(t)) => *t,
        _ => return cfg,
    };
    if supports_team_player_count(adjust_for_teams(unadjusted, cfg.as_ref()), &TeamCfg::Num(numeric)) {
        return cfg;
    }
    Some(TeamCfg::Num(js_max(
        2.0,
        (unadjusted / 2.0).floor(),
    )))
}

/// `getSpawnImmunityDuration(playerTeams, startingGold)`.
pub fn get_spawn_immunity_duration(cfg: Option<&TeamCfg>, gold: Option<f64>) -> f64 {
    if matches!(cfg, Some(c) if c.as_str() == "Humans Vs Nations") {
        return 5.0 * 10.0;
    }
    if let Some(g) = gold {
        if g >= 25_000_000.0 {
            return 150.0 * 10.0;
        }
        if g >= 5_000_000.0 {
            return SAM_CONSTRUCTION_TICKS + 15.0 * 10.0;
        }
    }
    5.0 * 10.0
}

/// The capture harness: one `MapPlaylist` replaying an op stream.
#[derive(Debug, Default)]
pub struct RigHarness {
    playlists: [Vec<&'static str>; 4],
    seed: f64,
}

impl RigHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Run one op. Kind table (see `tools/gen_vectors.mjs`):
    /// 0 construct/reset -> `[0]`;
    /// 1 setSeed `[seed]` -> `[0]`;
    /// 2 buildMapsList `(type-str),(mode-val)` -> `[n,(map-str)*]`;
    /// 3 playlistKey `(type-str),(mode-val)` -> val(key-str);
    /// 4 addNextMapNonConsecutive `[nPl,(map-str)*,nSrc,(map-str)*]` ->
    ///   `[0|1, nPl,(map-str)*, nSrc,(map-str)*]` (mutated arrays);
    /// 5 generateNewPlaylist `(type-str),(mode-val)` ->
    ///   `[n,(map-str)*,1,(log-msg-str)]`;
    /// 6 getNextMap `(type-str),(mode-val)` -> val(map-str);
    /// 7 dump playlists -> `[4,(n,(map-str)*)*]` (ffa,team,specialFfa,
    ///   specialTeam order);
    /// 8 calculateMapPlayerCounts `[landTiles]` -> `[3,l,m,s]`;
    /// 9 supportsTeamPlayerCount `[p,(cfg-val)]` -> `[0|1]`;
    /// 10 playersPerTeam `[p,(cfg-val)]` -> `[v]`;
    /// 11 numberOfTeams `[p,(cfg-val)]` -> `[v]`;
    /// 12 adjustForTeams `[p,(cfg-val)]` -> `[v]`;
    /// 13 adjustTeamCountForPlayerCapacity `(cfg-val),[unadjusted]` ->
    ///   val(cfg);
    /// 14 getSpawnImmunityDuration `(cfg-val),(gold-val)` -> `[v]`;
    /// 15 dump TEAM_WEIGHTS -> `[10,(cfg-val,weight)*]`;
    /// 16 dump SPECIAL_MODIFIER_POOL -> `[n,(key-str)*]`;
    /// 17 dump MUTUALLY_EXCLUSIVE_MODIFIERS -> `[6,(a-str,b-str)*]`;
    /// 18 dump SPECIAL_TEAM_MAPS -> `[n,(map-str,num)*]`;
    /// 19 dump DOOMSDAY_ROTATION_SPEEDS -> `[4,(str)*]`;
    /// 20 dump module consts -> `[60,125,7,25,0.75]`.
    /// Strings cross as `[len, u0, ..]` UTF-16 units; `mode` / `cfg` /
    /// `gold` ride the `js_json` codec (`undefined` = `[1]`).
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut i = 0usize;
        match kind {
            0 => {
                self.reset();
                vec![0.0]
            }
            1 => {
                self.seed = args[i];
                vec![0.0]
            }
            2 => {
                let type_ = read_str(args, &mut i);
                let mode = mode_of(&read_val(args, &mut i));
                let maps = build_maps_list(&type_, mode.as_deref());
                dump_maps(&maps)
            }
            3 => {
                let type_ = read_str(args, &mut i);
                let mode = mode_of(&read_val(args, &mut i));
                let mut out = Vec::new();
                push_val(&mut out, &JsVal::Str(playlist_key(&type_, mode.as_deref())));
                out
            }
            4 => {
                let n_pl = args[i] as usize;
                i += 1;
                let mut playlist: Vec<&'static str> = (0..n_pl)
                    .map(|_| intern_map(&read_str(args, &mut i)))
                    .collect();
                let n_src = args[i] as usize;
                i += 1;
                let mut source: Vec<&'static str> = (0..n_src)
                    .map(|_| intern_map(&read_str(args, &mut i)))
                    .collect();
                let ok = add_next_map_non_consecutive(&mut playlist, &mut source);
                let mut out = vec![if ok { 1.0 } else { 0.0 }];
                out.extend(dump_maps(&playlist));
                out.extend(dump_maps(&source));
                out
            }
            5 => {
                let type_ = read_str(args, &mut i);
                let mode = mode_of(&read_val(args, &mut i));
                let (maps, msg) = generate_new_playlist(self.seed, &type_, mode.as_deref());
                let mut out = dump_maps(&maps);
                out.push(1.0);
                push_str(&mut out, &msg);
                out
            }
            6 => {
                let type_ = read_str(args, &mut i);
                let mode = mode_of(&read_val(args, &mut i));
                let key = playlist_key(&type_, mode.as_deref());
                let q = &mut self.playlists[key_index(&key)];
                if q.is_empty() {
                    let (fresh, _) = generate_new_playlist(self.seed, &type_, mode.as_deref());
                    q.extend(fresh);
                }
                let mut out = Vec::new();
                push_val(&mut out, &JsVal::Str(q.remove(0).to_string()));
                out
            }
            7 => {
                let mut out = vec![4.0];
                for q in self.playlists.iter() {
                    out.extend(dump_maps(q));
                }
                out
            }
            8 => {
                let [l, m, s] = calculate_map_player_counts(args[i]);
                vec![3.0, l, m, s]
            }
            9 => {
                let p = args[i];
                i += 1;
                let cfg = TeamCfg::from_jsval(&read_val(args, &mut i)).expect("cfg");
                vec![if supports_team_player_count(p, &cfg) { 1.0 } else { 0.0 }]
            }
            10 => {
                let p = args[i];
                i += 1;
                let cfg = TeamCfg::from_jsval(&read_val(args, &mut i)).expect("cfg");
                vec![players_per_team(p, &cfg)]
            }
            11 => {
                let p = args[i];
                i += 1;
                let cfg = TeamCfg::from_jsval(&read_val(args, &mut i)).expect("cfg");
                vec![number_of_teams(p, &cfg)]
            }
            12 => {
                let p = args[i];
                i += 1;
                let cfg = TeamCfg::from_jsval(&read_val(args, &mut i));
                vec![adjust_for_teams(p, cfg.as_ref())]
            }
            13 => {
                let cfg = TeamCfg::from_jsval(&read_val(args, &mut i));
                let unadjusted = args[i];
                let mut out = Vec::new();
                push_val(
                    &mut out,
                    &adjust_team_count_for_player_capacity(cfg, unadjusted)
                        .map(|c| c.to_jsval())
                        .unwrap_or(JsVal::Undef),
                );
                out
            }
            14 => {
                let cfg = TeamCfg::from_jsval(&read_val(args, &mut i));
                let gold = match read_val(args, &mut i) {
                    JsVal::Num(g) => Some(g),
                    JsVal::Undef | JsVal::Absent => None,
                    other => Some(js_number(&other)),
                };
                vec![get_spawn_immunity_duration(cfg.as_ref(), gold)]
            }
            15 => {
                let mut out = vec![10.0];
                for (cfg, weight) in team_weights() {
                    push_val(&mut out, &cfg.to_jsval());
                    out.push(weight);
                }
                out
            }
            16 => {
                let mut out = vec![SPECIAL_MODIFIER_POOL.len() as f64];
                for k in SPECIAL_MODIFIER_POOL {
                    push_str(&mut out, k);
                }
                out
            }
            17 => {
                let mut out = vec![MUTUALLY_EXCLUSIVE_MODIFIERS.len() as f64];
                for (a, b) in MUTUALLY_EXCLUSIVE_MODIFIERS {
                    push_str(&mut out, a);
                    push_str(&mut out, b);
                }
                out
            }
            18 => {
                let entries = special_team_maps();
                let mut out = vec![entries.len() as f64];
                for (map, count) in entries {
                    push_str(&mut out, map);
                    out.push(count);
                }
                out
            }
            19 => {
                let mut out = vec![DOOMSDAY_ROTATION_SPEEDS.len() as f64];
                for s in DOOMSDAY_ROTATION_SPEEDS {
                    push_str(&mut out, s);
                }
                out
            }
            20 => vec![
                CROWDED_COMPACT_PLAYER_COUNT,
                CROWDED_PLAYER_COUNT,
                TRUSTED_PUBLIC_EVERY,
                TRUSTED_MAX_PLAYER_COUNT,
                SPECIAL_TEAM_FORCE_CHANCE,
            ],
            k => unreachable!("map_playlist harness: unknown op kind {k}"),
        }
    }
}

/// `mode` codec value -> `Option<String>` (`undefined` / absent = `None`).
fn mode_of(v: &JsVal) -> Option<String> {
    match v {
        JsVal::Str(s) => Some(s.clone()),
        JsVal::Undef | JsVal::Absent => None,
        other => Some(js_to_string(other)),
    }
}

fn js_number(v: &JsVal) -> f64 {
    match v {
        JsVal::Num(n) => *n,
        JsVal::Bool(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        JsVal::Null => f64::NAN,
        JsVal::Str(s) => s.parse::<f64>().unwrap_or(f64::NAN),
        _ => f64::NAN,
    }
}

/// Map values crossing the codec are the `GameMapType` strings the capture
/// feeds; the ported tables only ever emit `&'static str`, so the intern
/// maps the captured spelling back onto the table's static entry (and
/// falls back to a leaked copy for scripted spellings outside the table).
fn intern_map(s: &str) -> &'static str {
    MAPS.iter()
        .find(|m| m.type_ == s)
        .map(|m| m.type_)
        .unwrap_or_else(|| Box::leak(s.to_string().into_boxed_str()))
}

fn dump_maps(maps: &[&'static str]) -> Vec<f64> {
    let mut out = vec![maps.len() as f64];
    for m in maps {
        push_str(&mut out, m);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playlist_key_cases() {
        assert_eq!(playlist_key("ffa", None), "ffa");
        assert_eq!(playlist_key("team", Some("Team")), "team");
        assert_eq!(playlist_key("special", Some("Team")), "specialTeam");
        assert_eq!(playlist_key("special", Some("Free For All")), "specialFfa");
        assert_eq!(playlist_key("special", None), "specialFfa");
    }

    #[test]
    fn addnext_splices_first_match_and_window_is_five() {
        // Last-5 window is ["b","c","d","e","f"]: "a" is NOT in it (the
        // slice drops the oldest entry), so it is spliced and pushed.
        let mut playlist = vec!["a", "b", "c", "d", "e", "f"];
        let mut source = vec!["a", "e", "f", "g"];
        assert!(add_next_map_non_consecutive(&mut playlist, &mut source));
        assert_eq!(source, vec!["e", "f", "g"]);
        assert_eq!(playlist.last(), Some(&"a"));
    }

    #[test]
    fn addnext_fails_when_all_blocked() {
        let mut playlist = vec!["a", "b"];
        let mut source = vec!["a", "b"];
        assert!(!add_next_map_non_consecutive(&mut playlist, &mut source));
        assert_eq!(source, vec!["a", "b"]);
        assert_eq!(playlist, vec!["a", "b"]);
    }

    #[test]
    fn counts_round_half_up() {
        // 150000 -> base round(1.5)*5 = 10 (JS half-up); m = round(10*0.75/5)
        // = round(1.5)*5 = 10; s = round(1.0)*5 = 5.
        assert_eq!(calculate_map_player_counts(150_000.0), [10.0, 10.0, 5.0]);
        // 0 -> max(0, 5) floor.
        assert_eq!(calculate_map_player_counts(0.0), [5.0, 5.0, 5.0]);
    }

    #[test]
    fn string_presets_pass_the_capacity_gate() {
        for s in ["Duos", "Trios", "Quads", "Humans Vs Nations"] {
            let cfg = Some(TeamCfg::Str(s.to_string()));
            assert_eq!(adjust_team_count_for_player_capacity(cfg.clone(), 3.0), cfg);
        }
        // Numeric 7 with capacity 10: adjustForTeams(10, 7) = 7 ->
        // playersPerTeam 1 < 2 -> fallback max(2, floor(10/2)) = 5.
        assert_eq!(
            adjust_team_count_for_player_capacity(Some(TeamCfg::Num(7.0)), 10.0),
            Some(TeamCfg::Num(5.0))
        );
    }

    #[test]
    fn spawn_immunity_branches() {
        let hvn = TeamCfg::Str("Humans Vs Nations".to_string());
        assert_eq!(get_spawn_immunity_duration(Some(&hvn), Some(25_000_000.0)), 50.0);
        assert_eq!(get_spawn_immunity_duration(None, Some(25_000_000.0)), 1500.0);
        assert_eq!(get_spawn_immunity_duration(None, Some(5_000_000.0)), 450.0);
        assert_eq!(get_spawn_immunity_duration(None, Some(1_000_000.0)), 50.0);
        assert_eq!(get_spawn_immunity_duration(None, None), 50.0);
    }

    #[test]
    fn generate_is_deterministic_for_a_seed() {
        let (a, la) = generate_new_playlist(1_771_000_000_000.0, "ffa", None);
        let (b, lb) = generate_new_playlist(1_771_000_000_000.0, "ffa", None);
        assert_eq!(a, b);
        assert_eq!(la, lb);
        assert!(!a.is_empty());
        assert!(la.starts_with("Generated map playlist in "));
    }
}
