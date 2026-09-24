//! Port of `src/core/game/TeamAssignment.ts`.
//!
//! Pure lobby-balancing logic: team assignment (pins, clans, friend
//! preferences, nation shuffling) and the team-list resolver. Every
//! observable — the returned map's **iteration order**, the tie-breaking in
//! team selection, the stable clan sort, and the `PseudoRandom` shuffle seed
//! — is reproduced exactly.
//!
//! Faithfulness notes:
//!
//! * The TS `Map<PlayerInfo, Team | "kicked">` is keyed by object identity.
//!   The port keys by the player's **index** in the input slice; callers must
//!   pass distinct entries (real lobbies always do). The returned `Vec`
//!   preserves the JS Map insertion order: a `set` on an existing key updates
//!   the value *in place*, keeping the original position.
//! * `Team` is a plain string in the TS (`ColoredTeams` values, `Team N`), so
//!   `===` comparisons are string equality. `teamPlayerCount` and
//!   `teamByClientID` are keyed by the team *string*, so a duplicate team
//!   name in the list merges counts exactly like a JS `Map`.
//! * `p.clanTag` is tested for **truthiness**: an empty string counts as no
//!   clan, same as `null`.
//! * `p.teamIndex` indexes the team array like a JS property lookup: only a
//!   non-negative integer below `teams.len()` resolves; `NaN`, fractional,
//!   negative or out-of-range values leave the player unpinned.
//! * The clan sort `b.length - a.length` is a stable descending-by-size sort
//!   (V8's `Array#sort` is stable; Rust's `sort_by` is too), so equal-size
//!   clans keep their first-seen order.
//! * Team selection in the clan pass and in `placePlayer` keeps the *first*
//!   best (strict `<` / `>` comparisons), so ties resolve to the earliest
//!   team in the list.
//! * `resolveTeamsList` mirrors the two `throw` paths as
//!   [`ResolveTeamsError`] variants; the `Array.from({length})` oversized
//!   `RangeError` is reported as [`ResolveTeamsError::InvalidLength`].

use std::collections::{HashMap, HashSet};

use crate::game_map::js_max;
use crate::pseudo_random::PseudoRandom;
use crate::util::simple_hash;

/// `PlayerType` from `src/core/game/Game.ts` (string enum).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerType {
    Bot,
    Human,
    Nation,
}

/// The fields of `PlayerInfo` that `assignTeams` reads.
#[derive(Clone, Debug)]
pub struct PlayerInfo {
    /// `PlayerID` — only used as the nation-shuffle seed.
    pub id: String,
    pub player_type: PlayerType,
    /// `null` for tribe players.
    pub client_id: Option<String>,
    /// Truthiness-tested: `Some("")` behaves like `None`.
    pub clan_tag: Option<String>,
    pub friends: Vec<String>,
    /// Server-pinned team slot; `None` = assign normally.
    pub team_index: Option<f64>,
}

/// `Team` is a string in the TS.
pub type Team = String;

/// Value of the returned `Map<PlayerInfo, Team | "kicked">`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Assignment {
    Team(Team),
    Kicked,
}

/// `TeamCountConfig` from `src/core/Schemas.ts`: a number or one of the
/// string literals; `Other` carries any unknown string (the throw path).
#[derive(Clone, Debug, PartialEq)]
pub enum TeamCountConfig {
    Num(f64),
    Duos,
    Trios,
    Quads,
    HumansVsNations,
    Other(String),
}

/// `Array.from({ length })` throws `RangeError` past the array-length limit.
pub const MAX_ARRAY_LENGTH: f64 = 4_294_967_295.0; // 2^32 - 1

/// JS `ToLength` for the `Array.from({length})` path (the port only needs it
/// for non-negative finite inputs; `NaN` maps to 0 like the spec).
#[allow(clippy::neg_cmp_op_on_partial_ord)] // `!(n > 0.0)`: NaN must be false
fn to_length(n: f64) -> usize {
    if !(n > 0.0) {
        return 0; // NaN, -0, negatives
    }
    if n > MAX_ARRAY_LENGTH {
        return usize::MAX; // sentinel: caller must reject
    }
    n.floor() as usize
}

/// Insertion-ordered `Map<usize, Assignment>` mirroring the TS result map.
#[derive(Default)]
struct ResultMap {
    entries: Vec<(usize, Assignment)>,
    pos: HashMap<usize, usize>,
}

impl ResultMap {
    fn set(&mut self, player: usize, value: Assignment) {
        match self.pos.get(&player) {
            Some(&i) => self.entries[i].1 = value,
            None => {
                self.pos.insert(player, self.entries.len());
                self.entries.push((player, value));
            }
        }
    }
}

/// `teams[p.teamIndex]` as a JS property read: only in-range integer indices
/// resolve.
#[allow(clippy::neg_cmp_op_on_partial_ord)] // `!(idx >= 0.0)`: NaN must be false
fn team_at(teams: &[Team], idx: f64) -> Option<&Team> {
    if idx != idx.floor() || !(idx >= 0.0) || idx >= teams.len() as f64 {
        return None;
    }
    Some(&teams[idx as usize])
}

/// `getMaxTeamSize(numPlayers, numTeams)`.
pub fn get_max_team_size(num_players: f64, num_teams: f64) -> f64 {
    (num_players / num_teams).ceil()
}

/// `assignTeams(players, teams, isDuosTriosQuads)` with the default
/// `maxTeamSize` parameter.
pub fn assign_teams(
    players: &[PlayerInfo],
    teams: &[Team],
    is_duos_trios_quads: bool,
) -> Vec<(usize, Assignment)> {
    let max = get_max_team_size(players.len() as f64, teams.len() as f64);
    assign_teams_with_max(players, teams, is_duos_trios_quads, max)
}

/// `assignTeams(players, teams, isDuosTriosQuads, maxTeamSize)`. Returns the
/// result map entries in JS insertion order.
pub fn assign_teams_with_max(
    players: &[PlayerInfo],
    teams: &[Team],
    is_duos_trios_quads: bool,
    max_team_size: f64,
) -> Vec<(usize, Assignment)> {
    let mut result = ResultMap::default();
    let mut team_player_count: HashMap<Team, f64> = HashMap::new();

    // Matchmade games arrive with a server-pinned team slot (teamIndex). The
    // matchmaker already balanced those teams, so pins are honored
    // unconditionally — before and regardless of clan/friend grouping and
    // maxTeamSize — and seed the counts the balancing below sees.
    let mut unpinned: Vec<usize> = Vec::new();
    for (i, p) in players.iter().enumerate() {
        let pinned_team = p.team_index.and_then(|idx| team_at(teams, idx));
        match pinned_team {
            None => unpinned.push(i),
            Some(t) => {
                result.set(i, Assignment::Team(t.clone()));
                let c = team_player_count.entry(t.clone()).or_insert(0.0);
                *c += 1.0;
            }
        }
    }

    // Clans are strict: a clan goes to one team together, and any overflow
    // members get kicked. (You opted into the clan, so we honor "all or
    // nothing" for placement.)
    let mut clan_groups: Vec<Vec<usize>> = Vec::new();
    let mut clan_pos: HashMap<String, usize> = HashMap::new();
    let mut non_clan_players: Vec<usize> = Vec::new();
    for &i in &unpinned {
        let p = &players[i];
        match &p.clan_tag {
            Some(tag) if !tag.is_empty() => match clan_pos.get(tag) {
                Some(&g) => clan_groups[g].push(i),
                None => {
                    clan_pos.insert(tag.clone(), clan_groups.len());
                    clan_groups.push(vec![i]);
                }
            },
            _ => non_clan_players.push(i),
        }
    }

    let mut sorted_clans: Vec<&Vec<usize>> = clan_groups.iter().collect();
    sorted_clans.sort_by_key(|c| std::cmp::Reverse(c.len()));
    for clan in sorted_clans {
        let mut team: Option<&Team> = None;
        let mut team_size = 0.0f64;
        for t in teams {
            let p = team_player_count.get(t).copied().unwrap_or(0.0);
            if team.is_some() && team_size <= p {
                continue;
            }
            team_size = p;
            team = Some(t);
        }
        let Some(team) = team else { continue };
        for &player in clan {
            if team_size < max_team_size {
                team_size += 1.0;
                result.set(player, Assignment::Team(team.clone()));
            } else {
                result.set(player, Assignment::Kicked);
            }
        }
        team_player_count.insert(team.clone(), team_size);
    }

    // Friend edges are a soft preference: when placing a player, prefer the
    // team where the most of their friends already are. If that team is full
    // we spill onto the next-emptiest non-full team rather than kicking.
    let mut present_client_ids: HashSet<String> = HashSet::new();
    for p in players {
        if let Some(c) = &p.client_id {
            present_client_ids.insert(c.clone());
        }
    }
    let mut friend_graph: HashMap<String, HashSet<String>> = HashMap::new();
    for p in players {
        let Some(cid) = &p.client_id else { continue };
        for friend_id in &p.friends {
            if !present_client_ids.contains(friend_id) {
                continue;
            }
            add_edge(&mut friend_graph, cid, friend_id);
            add_edge(&mut friend_graph, friend_id, cid);
        }
    }

    let mut team_by_client_id: HashMap<String, Team> = HashMap::new();
    for (i, assignment) in result.entries.iter() {
        let p = &players[*i];
        if let (Some(c), Assignment::Team(t)) = (&p.client_id, assignment) {
            team_by_client_id.insert(c.clone(), t.clone());
        }
    }

    let mut nation_players: Vec<usize> = non_clan_players
        .iter()
        .copied()
        .filter(|&i| players[i].player_type == PlayerType::Nation)
        .collect();
    if !nation_players.is_empty() {
        let mut random = PseudoRandom::new(simple_hash(&players[nation_players[0]].id));
        nation_players = random.shuffle_array(&nation_players);
    }
    let other_players: Vec<usize> = non_clan_players
        .iter()
        .copied()
        .filter(|&i| players[i].player_type != PlayerType::Nation)
        .collect();

    for &p in other_players.iter().chain(nation_players.iter()) {
        place_player(
            players,
            teams,
            &mut result,
            &mut team_player_count,
            &mut team_by_client_id,
            &friend_graph,
            is_duos_trios_quads,
            max_team_size,
            p,
        );
    }

    result.entries
}

fn add_edge(graph: &mut HashMap<String, HashSet<String>>, a: &str, b: &str) {
    graph.entry(a.to_string()).or_default().insert(b.to_string());
}

#[allow(clippy::too_many_arguments)]
fn place_player(
    players: &[PlayerInfo],
    teams: &[Team],
    result: &mut ResultMap,
    team_player_count: &mut HashMap<Team, f64>,
    team_by_client_id: &mut HashMap<String, Team>,
    friend_graph: &HashMap<String, HashSet<String>>,
    is_duos_trios_quads: bool,
    max_team_size: f64,
    i: usize,
) {
    let p = &players[i];
    let my_friends = p.client_id.as_ref().and_then(|c| friend_graph.get(c));
    let mut best_team: Option<&Team> = None;
    let mut best_friend_count = -1.0f64;
    let mut best_size = if is_duos_trios_quads {
        -1.0
    } else {
        f64::INFINITY
    };
    for t in teams {
        let size = team_player_count.get(t).copied().unwrap_or(0.0);
        if size >= max_team_size {
            continue;
        }
        let mut friends_on_team = 0.0f64;
        if let Some(my_friends) = my_friends {
            for friend_id in my_friends {
                if team_by_client_id.get(friend_id).is_some_and(|x| x == t) {
                    friends_on_team += 1.0;
                }
            }
        }
        if friends_on_team > best_friend_count
            || (friends_on_team == best_friend_count
                && if is_duos_trios_quads {
                    size > best_size
                } else {
                    size < best_size
                })
        {
            best_friend_count = friends_on_team;
            best_size = size;
            best_team = Some(t);
        }
    }
    let Some(best_team) = best_team else {
        result.set(i, Assignment::Kicked);
        return;
    };
    let c = team_player_count.entry(best_team.clone()).or_insert(0.0);
    *c += 1.0;
    result.set(i, Assignment::Team(best_team.clone()));
    if let Some(cid) = &p.client_id {
        team_by_client_id.insert(cid.clone(), best_team.clone());
    }
}

/// `assignTeamsLobbyPreview(players, teams, teamCount, nationCount)`.
pub fn assign_teams_lobby_preview(
    players: &[PlayerInfo],
    teams: &[Team],
    team_count: &TeamCountConfig,
    nation_count: usize,
) -> Vec<(usize, Assignment)> {
    let max_team_size = get_max_team_size(
        players.len() as f64 + nation_count as f64,
        teams.len() as f64,
    );
    let is_duos_trios_quads = matches!(
        team_count,
        TeamCountConfig::Duos | TeamCountConfig::Trios | TeamCountConfig::Quads
    );
    assign_teams_with_max(players, teams, is_duos_trios_quads, max_team_size)
}

/// The `throw` paths of `resolveTeamsList`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolveTeamsError {
    /// `Unknown TeamCountConfig ${config}`
    UnknownConfig,
    /// `Too few teams: ${numTeams}`
    TooFewTeams,
    /// JS `RangeError` from `Array.from({ length })` past 2^32-1.
    InvalidLength,
}

/// `resolveTeamsList(config, totalPlayers)`.
pub fn resolve_teams_list(
    config: &TeamCountConfig,
    total_players: f64,
) -> Result<Vec<Team>, ResolveTeamsError> {
    if matches!(config, TeamCountConfig::HumansVsNations) {
        return Ok(vec!["Humans".to_string(), "Nations".to_string()]);
    }
    let num_teams: f64 = match config {
        TeamCountConfig::Num(n) => *n,
        TeamCountConfig::Duos => {
            js_max(2.0, (total_players / 2.0).ceil())
        }
        TeamCountConfig::Trios => {
            js_max(2.0, (total_players / 3.0).ceil())
        }
        TeamCountConfig::Quads => {
            js_max(2.0, (total_players / 4.0).ceil())
        }
        _ => return Err(ResolveTeamsError::UnknownConfig),
    };
    // Numeric configs state the team count outright, so below 2 is a
    // misconfiguration and should stay loud rather than be silently
    // reshaped.
    if num_teams < 2.0 {
        return Err(ResolveTeamsError::TooFewTeams);
    }
    if num_teams < 8.0 {
        let mut teams: Vec<Team> = vec!["Red".to_string(), "Blue".to_string()];
        if num_teams >= 3.0 {
            teams.push("Yellow".to_string());
        }
        if num_teams >= 4.0 {
            teams.push("Green".to_string());
        }
        if num_teams >= 5.0 {
            teams.push("Purple".to_string());
        }
        if num_teams >= 6.0 {
            teams.push("Orange".to_string());
        }
        if num_teams >= 7.0 {
            teams.push("Teal".to_string());
        }
        return Ok(teams);
    }
    let len = to_length(num_teams);
    if len == usize::MAX {
        return Err(ResolveTeamsError::InvalidLength);
    }
    Ok((0..len).map(|i| format!("Team {}", i + 1)).collect())
}
