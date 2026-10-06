//! Port of `src/client/components/baseComponents/ranking/GameInfoRanking.ts`
//! — the `Ranking` class over an `AnalyticsRecord`: the `hasPlayed` gate, the
//! insertion-ordered `Record<string, PlayerInfo>` summariser with the
//! `winner` block pass, the twelve-branch `getScore` switch (no `default`:
//! an out-of-domain rank type reads `undefined` in TS) and the
//! subtraction-comparator `sortedBy` (V8 TimSort stable, NaN comparator →
//! `+0`).
//!
//! Faithfulness notes (quirk list):
//!
//! * `RankType` is a string enum whose values equal the member names; the
//!   capture inlines it as a plain object (precedent: `GameMode` /
//!   `UnitType`). `RANK_TYPE_LABEL_KEYS` is a computed-key object whose
//!   insertion order starts with `Lifetime`, then the eleven enum members in
//!   declaration order.
//! * `summarizePlayers` builds a plain object keyed by `player.clientID`
//!   (V8 own-key order: canonical decimal integer keys sort ascending AHEAD
//!   of the string keys, which keep insertion order; a repeat `clientID`
//!   overwrites the value in place — the `map_set` semantics).
//!   `Object.values` therefore reports integer keys first, then the
//!   first-encounter order of the string keys.
//! * `hasPlayed`: `stats !== undefined` AND any of `units` / `killedAt` /
//!   `conquests` present. A present-`undefined` `stats` fails the first gate.
//! * `gold` / `conquests` cross the wire element-tagged (null / undefined /
//!   bigint / number). `BigInt(v ?? 0)`: nullish → `0n`, bigint passthrough,
//!   a NUMBER must be an integral finite value or `BigInt` throws a
//!   `RangeError` (the harness records status 1 and the ranking is never
//!   built). bigints are modelled as `i64` (the golden domain stays ≤ 2^53,
//!   same precedent as `stats_schemas::to_bigint` / `util::to_int`).
//! * `killedAt` uses the double nullish gate (`=== undefined || === null`),
//!   everything else goes through `Number()`.
//! * `atoms` / `hydros` / `mirv`: `Number(bombs?.X?.[0]) || 0` — the `|| 0`
//!   fallback catches NaN, `0` and `-0` alike.
//! * The winner block gates `!== undefined && Array.isArray && length > 0`;
//!   `"player"` marks `winnerBlock[1]` (truthy `players[id]` gate), `"team"`
//!   marks every element from index 2; any other first element is inert.
//!   Non-string ids stringify through the JS `ToPropertyKey` subset the
//!   capture feeds (number / bool / null; anything else reads `"undefined"`).
//! * `getScore` has NO `default` arm: an out-of-domain type yields TS
//!   `undefined`, modelled as `None` here; `getAdjustedScore` then computes
//!   `undefined + 0.1` → `NaN` for a winner and stays `undefined` otherwise
//!   — both make the subtraction comparator `NaN`, which V8 `SortCompare`
//!   treats as `+0` (original order survives, stable).
//! * `sortedBy` compares `adjusted(b) - adjusted(a)` (descending) over a
//!   stable sort so equal scores keep the `Object.values` order.

use crate::game_config_helpers::js_number;
use crate::game_ts::js_num_str;
use crate::js_json::{read_str, read_val, val_field, JsVal};
use crate::stats_schemas::{
    GOLD_INDEX_STEAL, GOLD_INDEX_TRADE, GOLD_INDEX_TRAIN_OTHER, GOLD_INDEX_TRAIN_SELF,
    GOLD_INDEX_WAR, PLAYER_INDEX_BOT, PLAYER_INDEX_HUMAN, PLAYER_INDEX_NATION,
};

/// `RankType` — the twelve string-enum members (value == name).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RankType {
    ConquestHumans,
    ConquestNations,
    ConquestBots,
    Atoms,
    Hydros,
    Mirv,
    TotalGold,
    StolenGold,
    NavalTrade,
    TrainTrade,
    ConqueredGold,
    Lifetime,
}

impl RankType {
    /// The enum member spelling (value == key in the TS string enum).
    pub fn as_str(self) -> &'static str {
        match self {
            RankType::ConquestHumans => "ConquestHumans",
            RankType::ConquestNations => "ConquestNations",
            RankType::ConquestBots => "ConquestBots",
            RankType::Atoms => "Atoms",
            RankType::Hydros => "Hydros",
            RankType::Mirv => "MIRV",
            RankType::TotalGold => "TotalGold",
            RankType::StolenGold => "StolenGold",
            RankType::NavalTrade => "NavalTrade",
            RankType::TrainTrade => "TrainTrade",
            RankType::ConqueredGold => "ConqueredGold",
            RankType::Lifetime => "Lifetime",
        }
    }

    /// Parse a rank-type string; `None` models an out-of-enum value (the TS
    /// `getScore` switch then falls through and returns `undefined`).
    pub fn parse(s: &str) -> Option<RankType> {
        Some(match s {
            "ConquestHumans" => RankType::ConquestHumans,
            "ConquestNations" => RankType::ConquestNations,
            "ConquestBots" => RankType::ConquestBots,
            "Atoms" => RankType::Atoms,
            "Hydros" => RankType::Hydros,
            "MIRV" => RankType::Mirv,
            "TotalGold" => RankType::TotalGold,
            "StolenGold" => RankType::StolenGold,
            "NavalTrade" => RankType::NavalTrade,
            "TrainTrade" => RankType::TrainTrade,
            "ConqueredGold" => RankType::ConqueredGold,
            "Lifetime" => RankType::Lifetime,
            _ => return None,
        })
    }
}

/// `RankType` members in TS declaration order (the enum-key table dump).
pub const RANK_TYPE_MEMBERS: [RankType; 12] = [
    RankType::ConquestHumans,
    RankType::ConquestNations,
    RankType::ConquestBots,
    RankType::Atoms,
    RankType::Hydros,
    RankType::Mirv,
    RankType::TotalGold,
    RankType::StolenGold,
    RankType::NavalTrade,
    RankType::TrainTrade,
    RankType::ConqueredGold,
    RankType::Lifetime,
];

/// `RANK_TYPE_LABEL_KEYS` — `(rank-type value, i18n key)` in TS computed-key
/// insertion order (`Lifetime` first, then the declaration order).
pub const RANK_TYPE_LABEL_KEYS: [(&str, &str); 12] = [
    ("Lifetime", "game_info_modal.survival_time"),
    ("ConquestHumans", "game_info_modal.num_of_conquests_humans"),
    ("ConquestNations", "game_info_modal.num_of_conquests_nations"),
    ("ConquestBots", "game_info_modal.num_of_conquests_bots"),
    ("Atoms", "game_info_modal.atoms"),
    ("Hydros", "game_info_modal.hydros"),
    ("MIRV", "game_info_modal.mirv"),
    ("TotalGold", "game_info_modal.all_gold"),
    ("StolenGold", "game_info_modal.stolen_gold"),
    ("NavalTrade", "game_info_modal.naval_trade"),
    ("TrainTrade", "game_info_modal.train_trade"),
    ("ConqueredGold", "game_info_modal.conquest_gold"),
];

/// `PlayerInfo` — the summarised row. bigints ride as `i64` (≤ 2^53 domain);
/// `clanTag` / `flag` keep the raw JS value (`flag` `Absent` models the
/// `?? undefined` result).
#[derive(Debug, Clone)]
pub struct PlayerInfo {
    pub id: String,
    pub username: String,
    pub clan_tag: JsVal,
    /// `undefined` -> `None`; otherwise `Number(stats.killedAt)`.
    pub killed_at: Option<f64>,
    pub gold: Vec<i64>,
    pub conquests: Vec<i64>,
    pub flag: JsVal,
    pub winner: bool,
    pub atoms: f64,
    pub hydros: f64,
    pub mirv: f64,
}

/// `hasPlayed(player)`.
fn has_played(stats_present: bool, units: bool, killed_at: &JsVal, conquests_present: bool) -> bool {
    stats_present && (units || !matches!(killed_at, JsVal::Absent | JsVal::Undef) || conquests_present)
}

/// The session slice the `Ranking` constructor consumes.
pub struct SessionInput<'a> {
    pub duration: f64,
    pub winner: &'a JsVal,
    pub players: &'a [PlayerInput<'a>],
}

/// One `session.info.players` entry.
pub struct PlayerInput<'a> {
    /// `false` models the `player === undefined` skip gate.
    pub present: bool,
    pub client_id: &'a str,
    pub username: &'a str,
    pub clan_tag: &'a JsVal,
    /// `stats` present (not `undefined`).
    pub stats: bool,
    pub units: bool,
    pub killed_at: &'a JsVal,
    pub conquests: Option<&'a [BigIntTok]>,
    pub gold: Option<&'a [BigIntTok]>,
    pub cosmetics: &'a JsVal,
    pub bombs: &'a JsVal,
}

/// A `gold` / `conquests` array element as it crosses the wire.
#[derive(Debug, Clone, Copy)]
pub enum BigIntTok {
    Null,
    Undef,
    BigInt(f64),
    Number(f64),
}

/// `BigInt(v ?? 0)` — `Err(())` models the `RangeError` a non-integral
/// finite number raises (`BigInt(1.5)` / `BigInt(NaN)` / `BigInt(Infinity)`).
fn to_bigint_elem(t: BigIntTok) -> Result<i64, ()> {
    match t {
        BigIntTok::Null | BigIntTok::Undef => Ok(0),
        BigIntTok::BigInt(v) => Ok(v as i64),
        BigIntTok::Number(v) => {
            if v.is_finite() && v.fract() == 0.0 {
                Ok(v as i64)
            } else {
                Err(())
            }
        }
    }
}

fn map_bigints(items: &[BigIntTok]) -> Result<Vec<i64>, ()> {
    items.iter().copied().map(to_bigint_elem).collect()
}

/// `Number(v)` over the codec domain (objects / arrays are out of the
/// capture's domain and read NaN, as does `map_playlist::js_number` — but
/// `Number(null)` is `0`, which `game_config_helpers::js_number` does not
/// see because it is string-only).
fn js_number_val(v: &JsVal) -> f64 {
    match v {
        JsVal::Num(n) => *n,
        JsVal::Bool(b) => if *b { 1.0 } else { 0.0 },
        JsVal::Null => 0.0,
        JsVal::Str(s) => js_number(s),
        JsVal::Absent | JsVal::Undef | JsVal::Obj(_) | JsVal::Arr(_) => f64::NAN,
    }
}

/// `Number(x) || 0` — the falsy fallback catches `NaN`, `0` and `-0`.
fn number_or_zero(v: f64) -> f64 {
    if v == 0.0 || v.is_nan() {
        0.0
    } else {
        v
    }
}

/// `stats.bombs?.abomb?.[0]` style read: `bombs` must be an object, the
/// named slot an array, and index 0 present.
fn bomb_first(bombs: &JsVal, key: &str) -> f64 {
    let Some(arr) = val_field(bombs, key) else {
        return f64::NAN;
    };
    let JsVal::Arr(items) = arr else {
        return f64::NAN;
    };
    match items.first() {
        Some(v) => js_number_val(v),
        None => f64::NAN,
    }
}

/// JS `ToPropertyKey` restricted to the winner-block domain the capture
/// feeds (strings, numbers, booleans, null; anything else reads
/// `"undefined"` — objects would `toString`, out of domain).
fn prop_key(v: &JsVal) -> String {
    match v {
        JsVal::Str(s) => s.clone(),
        JsVal::Num(n) => js_num_str(*n),
        JsVal::Bool(true) => "true".to_string(),
        JsVal::Bool(false) => "false".to_string(),
        JsVal::Null => "null".to_string(),
        _ => "undefined".to_string(),
    }
}

/// Canonical decimal-integer array index (0 .. 2^53-1) for the V8 own-key
/// order; `None` models a plain string key.
fn canon_int_key(k: &str) -> Option<u64> {
    k.parse::<u64>().ok().filter(|v| *v < (1u64 << 53))
}

/// `summarizePlayers(session)` — the V8-ordered record, then the
/// winner pass, then `Object.values`.
#[allow(clippy::result_unit_err)]
pub fn summarize_players(session: &SessionInput) -> Result<Vec<PlayerInfo>, ()> {
    // (clientID, PlayerInfo) in V8 own-key order (integer keys ascending
    // first, then string keys in insertion order).
    let mut players: Vec<(String, PlayerInfo)> = Vec::new();
    for p in session.players {
        if !p.present || !has_played(p.stats, p.units, p.killed_at, p.conquests.is_some()) {
            continue;
        }
        let gold = match p.gold {
            Some(items) => map_bigints(items)?,
            None => Vec::new(),
        };
        let conquests = match p.conquests {
            Some(items) => map_bigints(items)?,
            None => Vec::new(),
        };
        // `player.cosmetics?.flag ?? undefined`
        let flag = match val_field(p.cosmetics, "flag") {
            None => JsVal::Undef,
            Some(JsVal::Absent) | Some(JsVal::Undef) | Some(JsVal::Null) => JsVal::Undef,
            Some(v) => v.clone(),
        };
        // `stats.killedAt === undefined || === null ? undefined : Number(...)`
        let killed_at = match p.killed_at {
            JsVal::Absent | JsVal::Undef | JsVal::Null => None,
            v => Some(js_number_val(v)),
        };
        let info = PlayerInfo {
            id: p.client_id.to_string(),
            username: p.username.to_string(),
            clan_tag: p.clan_tag.clone(),
            killed_at,
            gold,
            conquests,
            flag,
            winner: false,
            atoms: number_or_zero(bomb_first(p.bombs, "abomb")),
            hydros: number_or_zero(bomb_first(p.bombs, "hbomb")),
            mirv: number_or_zero(bomb_first(p.bombs, "mirv")),
        };
        // `players[player.clientID] = {...}` — V8 own-key order: an
        // existing key overwrites in place, a canonical integer key
        // inserts before the first larger integer key / first string key.
        if let Some(slot) = players.iter_mut().find(|(k, _)| *k == info.id) {
            slot.1 = info;
        } else {
            match canon_int_key(&info.id) {
                Some(val) => {
                    let pos = players
                        .iter()
                        .position(|(k, _)| match canon_int_key(k) {
                            Some(kv) => kv > val,
                            None => true,
                        })
                        .unwrap_or(players.len());
                    players.insert(pos, (info.id.clone(), info));
                }
                None => players.push((info.id.clone(), info)),
            }
        }
    }

    // The winner block: `!== undefined && Array.isArray && length > 0`.
    if let JsVal::Arr(block) = session.winner {
        if !block.is_empty() {
            let mut mark = |id: &JsVal| {
                let key = prop_key(id);
                if let Some((_, pi)) = players.iter_mut().find(|(k, _)| *k == key) {
                    pi.winner = true;
                }
            };
            match &block[0] {
                JsVal::Str(s) if s == "player" => {
                    // `const id = winnerBlock[1]` — absent reads undefined.
                    let id = block.get(1).cloned().unwrap_or(JsVal::Undef);
                    mark(&id);
                }
                JsVal::Str(s) if s == "team" => {
                    for id in block.iter().skip(2) {
                        mark(id);
                    }
                }
                _ => {}
            }
        }
    }

    Ok(players.into_iter().map(|(_, p)| p).collect())
}

/// The ported `Ranking`.
#[derive(Debug, Clone)]
pub struct Ranking {
    duration: f64,
    players: Vec<PlayerInfo>,
}

impl Ranking {
    /// `new Ranking(session)`.
    #[allow(clippy::result_unit_err)]
    pub fn new(session: &SessionInput) -> Result<Self, ()> {
        Ok(Self {
            duration: session.duration,
            players: summarize_players(session)?,
        })
    }

    /// `get allPlayers`.
    pub fn all_players(&self) -> &[PlayerInfo] {
        &self.players
    }

    /// `sortedBy(type)` — the subtraction comparator over a stable sort.
    pub fn sorted_by(&self, rank: Option<RankType>) -> Vec<PlayerInfo> {
        let mut out = self.players.clone();
        out.sort_by(|a, b| {
            let d = sub_opt(
                self.get_adjusted_score(b, rank),
                self.get_adjusted_score(a, rank),
            );
            if d < 0.0 {
                std::cmp::Ordering::Less
            } else if d > 0.0 {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        });
        out
    }

    /// `score(player, type)` (the public wrapper).
    pub fn score(&self, player: &PlayerInfo, rank: Option<RankType>) -> Option<f64> {
        self.get_score(player, rank)
    }

    /// `getScore(player, type)` — no `default` arm; an out-of-domain type
    /// reads TS `undefined` (`None`).
    fn get_score(&self, player: &PlayerInfo, rank: Option<RankType>) -> Option<f64> {
        Some(match rank? {
            RankType::Lifetime => match player.killed_at {
                Some(k) => (k / crate::jsnum::js_max(self.duration, 1.0)) * 10.0,
                None => 100.0,
            },
            RankType::ConquestHumans => {
                player.conquests.get(PLAYER_INDEX_HUMAN).copied().unwrap_or(0) as f64
            }
            RankType::ConquestNations => {
                player.conquests.get(PLAYER_INDEX_NATION).copied().unwrap_or(0) as f64
            }
            RankType::ConquestBots => {
                player.conquests.get(PLAYER_INDEX_BOT).copied().unwrap_or(0) as f64
            }
            RankType::Atoms => player.atoms,
            RankType::Hydros => player.hydros,
            RankType::Mirv => player.mirv,
            RankType::TotalGold => {
                player.gold.iter().fold(0i64, |sum, g| sum + *g) as f64
            }
            RankType::StolenGold => player.gold.get(GOLD_INDEX_STEAL).copied().unwrap_or(0) as f64,
            RankType::NavalTrade => player.gold.get(GOLD_INDEX_TRADE).copied().unwrap_or(0) as f64,
            RankType::ConqueredGold => player.gold.get(GOLD_INDEX_WAR).copied().unwrap_or(0) as f64,
            RankType::TrainTrade => {
                let own = player.gold.get(GOLD_INDEX_TRAIN_SELF).copied().unwrap_or(0);
                let other = player.gold.get(GOLD_INDEX_TRAIN_OTHER).copied().unwrap_or(0);
                (own + other) as f64
            }
        })
    }

    /// `getAdjustedScore` — the raw switch result (`None` = TS `undefined`)
    /// plus the winner bonus: `undefined + 0.1` is `NaN`, a non-winner keeps
    /// `undefined` itself. The subtraction comparator turns either missing
    /// side into `NaN` (V8 `SortCompare` → `+0`), so the two paths only
    /// differ in the `score` dump, not in `sortedBy`.
    fn get_adjusted_score(&self, player: &PlayerInfo, rank: Option<RankType>) -> Option<f64> {
        match self.get_score(player, rank) {
            Some(s) => {
                if player.winner {
                    Some(s + 0.1)
                } else {
                    Some(s)
                }
            }
            None => {
                if player.winner {
                    Some(f64::NAN)
                } else {
                    None
                }
            }
        }
    }
}

/// JS subtraction over the adjusted-score domain: any `undefined` side makes
/// the difference `NaN` (V8 treats the NaN comparator result as `+0`).
fn sub_opt(a: Option<f64>, b: Option<f64>) -> f64 {
    match (a, b) {
        (Some(x), Some(y)) => x - y,
        _ => f64::NAN,
    }
}

// ---------------------------------------------------------------- vectors op
//
// Session wire form (flat f64 tokens; strings `[len, u0..]`, values the
// js_json codec):
//   duration, encVal(winner), n, per player:
//     present 0|1, clientID str, username str, encVal(clanTag),
//     stats 0|1, units 0|1, encVal(killedAt),
//     conquests tag 0|1 [n,(tag,payload)*n], gold tag 0|1 [n,(tag,payload)*n]
//       (elem tag 0=null, 1=undefined, 2=bigint, 3=number; payload f64),
//     encVal(cosmetics), encVal(bombs).
//
// kind table (mirrors the capture):
//   0 [session] -> [status, n, (player)*n]        construct + allPlayers dump
//   1 [session, type-str] -> [status, n, (player)*n]   sortedBy dump
//   2 [session, type-str] -> [status, n, (0|1, val?)*n] score per player
//   3 [] -> [12, (value-str)*12, 12, (key,val)*12, 34]  enum + label tables
//   (status 0 ok, 1 RangeError — a non-integral number in gold/conquests)
//   (player dump: id, username, clanTag, [0]|[1,killedAt], [n,(g)*n] gold,
//    [n,(c)*n] conquests, flag, winner 0|1, atoms, hydros, mirv)

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
    fn val(&mut self) -> JsVal {
        read_val(self.0, &mut self.1)
    }
    fn string(&mut self) -> String {
        read_str(self.0, &mut self.1)
    }
    fn bigints(&mut self) -> Option<Vec<BigIntTok>> {
        if self.u() == 0 {
            return None;
        }
        let n = self.u();
        Some(
            (0..n)
                .map(|_| {
                    let tag = self.u();
                    let v = self.f();
                    match tag {
                        0 => BigIntTok::Null,
                        1 => BigIntTok::Undef,
                        2 => BigIntTok::BigInt(v),
                        _ => BigIntTok::Number(v),
                    }
                })
                .collect(),
        )
    }
}

fn push_player(out: &mut Vec<f64>, p: &PlayerInfo) {
    crate::js_json::push_str(out, &p.id);
    crate::js_json::push_str(out, &p.username);
    crate::js_json::push_val(out, &p.clan_tag);
    match p.killed_at {
        Some(k) => {
            out.push(1.0);
            out.push(k);
        }
        None => out.push(0.0),
    }
    out.push(p.gold.len() as f64);
    for g in &p.gold {
        out.push(*g as f64);
    }
    out.push(p.conquests.len() as f64);
    for c in &p.conquests {
        out.push(*c as f64);
    }
    crate::js_json::push_val(out, &p.flag);
    out.push(if p.winner { 1.0 } else { 0.0 });
    out.push(p.atoms);
    out.push(p.hydros);
    out.push(p.mirv);
}

fn read_session<'a>(c: &mut Cur<'a>) -> SessionInput<'a> {
    // Borrow the raw token slice: the codec values are rebuilt here, so the
    // session borrows owned storage living for the whole op.
    let duration = c.f();
    let winner: &'a JsVal = Box::leak(Box::new(c.val()));
    let n = c.u();
    let mut players: Vec<PlayerInput<'a>> = Vec::with_capacity(n);
    for _ in 0..n {
        let present = c.u() != 0;
        let client_id: &'a str = Box::leak(c.string().into_boxed_str());
        let username: &'a str = Box::leak(c.string().into_boxed_str());
        let clan_tag: &'a JsVal = Box::leak(Box::new(c.val()));
        let stats = c.u() != 0;
        let units = c.u() != 0;
        let killed_at: &'a JsVal = Box::leak(Box::new(c.val()));
        let conquests: Option<&'a [BigIntTok]> = c.bigints().map(|v| &*Box::leak(v.into_boxed_slice()));
        let gold = c.bigints().map(|v| &*Box::leak(v.into_boxed_slice()));
        let cosmetics: &'a JsVal = Box::leak(Box::new(c.val()));
        let bombs: &'a JsVal = Box::leak(Box::new(c.val()));
        players.push(PlayerInput {
            present,
            client_id,
            username,
            clan_tag,
            stats,
            units,
            killed_at,
            conquests,
            gold,
            cosmetics,
            bombs,
        });
    }
    SessionInput {
        duration,
        winner,
        players: Box::leak(players.into_boxed_slice()),
    }
}

fn read_type(c: &mut Cur) -> Option<RankType> {
    let s = c.string();
    RankType::parse(&s)
}

pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    let mut c = Cur(args, 0);
    match kind {
        0 => {
            let session = read_session(&mut c);
            match Ranking::new(&session) {
                Ok(r) => {
                    out.push(0.0);
                    let ps = r.all_players();
                    out.push(ps.len() as f64);
                    for p in ps {
                        push_player(&mut out, p);
                    }
                }
                Err(()) => out.push(1.0),
            }
        }
        1 => {
            let session = read_session(&mut c);
            let rank = read_type(&mut c);
            match Ranking::new(&session) {
                Ok(r) => {
                    out.push(0.0);
                    let ps = r.sorted_by(rank);
                    out.push(ps.len() as f64);
                    for p in &ps {
                        push_player(&mut out, p);
                    }
                }
                Err(()) => out.push(1.0),
            }
        }
        2 => {
            let session = read_session(&mut c);
            let rank = read_type(&mut c);
            match Ranking::new(&session) {
                Ok(r) => {
                    out.push(0.0);
                    let ps = r.all_players();
                    out.push(ps.len() as f64);
                    for p in ps {
                        match r.score(p, rank) {
                            Some(v) => {
                                out.push(0.0);
                                out.push(v);
                            }
                            None => out.push(1.0),
                        }
                    }
                }
                Err(()) => out.push(1.0),
            }
        }
        3 => {
            out.push(RANK_TYPE_MEMBERS.len() as f64);
            for m in RANK_TYPE_MEMBERS {
                crate::js_json::push_str(&mut out, m.as_str());
            }
            out.push(RANK_TYPE_LABEL_KEYS.len() as f64);
            for (k, v) in RANK_TYPE_LABEL_KEYS {
                crate::js_json::push_str(&mut out, k);
                crate::js_json::push_str(&mut out, v);
            }
        }
        k => unreachable!("game_info_ranking: unknown op kind {k}"),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v() -> JsVal {
        JsVal::Undef
    }

    fn player(client_id: &str, stats: bool) -> PlayerInput<'static> {
        PlayerInput {
            present: true,
            client_id: Box::leak(client_id.to_string().into_boxed_str()),
            username: Box::leak(format!("{client_id}U").into_boxed_str()),
            clan_tag: Box::leak(Box::new(JsVal::Null)),
            stats,
            units: false,
            killed_at: Box::leak(Box::new(JsVal::Absent)),
            conquests: None,
            gold: None,
            cosmetics: Box::leak(Box::new(JsVal::Absent)),
            bombs: Box::leak(Box::new(JsVal::Absent)),
        }
    }

    fn session<'a>(players: &'a [PlayerInput<'a>], winner: &'a JsVal) -> SessionInput<'a> {
        SessionInput { duration: 100.0, winner, players }
    }

    #[test]
    fn has_played_gates() {
        // stats absent -> false.
        assert!(!has_played(false, true, &JsVal::Absent, false));
        // units present -> true.
        assert!(has_played(true, true, &JsVal::Absent, false));
        // killedAt present -> true.
        assert!(has_played(true, false, &JsVal::Num(5.0), false));
        // conquests present -> true.
        assert!(has_played(true, false, &JsVal::Absent, true));
        // none of the three -> false.
        assert!(!has_played(true, false, &JsVal::Absent, false));
        // present-undefined killedAt fails that arm.
        assert!(!has_played(true, false, &JsVal::Undef, false));
    }

    #[test]
    fn insertion_order_and_duplicate_client_id() {
        let mut a = player("a", true);
        a.units = true;
        let mut b = player("b", true);
        b.units = true;
        let mut a2 = player("a", true);
        a2.username = Box::leak("a-second".to_string().into_boxed_str());
        a2.units = true;
        let ps = vec![a, b, a2];
        let r = Ranking::new(&session(&ps, &v())).unwrap();
        let all = r.all_players();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].id, "a");
        // The repeat overwrote in place (position preserved, value replaced).
        assert_eq!(all[0].username, "a-second");
        assert_eq!(all[1].id, "b");
    }

    #[test]
    fn bigint_wire_throw() {
        let mut a = player("a", true);
        a.units = true;
        a.gold = Some(Box::leak(vec![BigIntTok::Number(1.5)].into_boxed_slice()));
        assert!(Ranking::new(&session(&[a], &v())).is_err());
    }

    #[test]
    fn killed_at_and_bombs_coercions() {
        let mut a = player("a", true);
        a.units = true;
        a.killed_at = Box::leak(Box::new(JsVal::Null)); // null -> undefined
        a.bombs = Box::leak(Box::new(JsVal::Obj(vec![(
            "abomb".to_string(),
            JsVal::Arr(vec![JsVal::Str("3".to_string())]),
        )])));
        let r = Ranking::new(&session(&[a], &v())).unwrap();
        assert_eq!(r.all_players()[0].killed_at, None);
        assert_eq!(r.all_players()[0].atoms, 3.0);
        assert_eq!(r.all_players()[0].hydros, 0.0);
    }

    #[test]
    fn winner_blocks() {
        let mk = |ids: &[&str]| -> Vec<PlayerInput<'static>> {
            ids.iter()
                .map(|id| {
                    let mut p = player(id, true);
                    p.units = true;
                    p
                })
                .collect()
        };
        // player block
        let ps = mk(&["a", "b"]);
        let winner = JsVal::Arr(vec![JsVal::Str("player".into()), JsVal::Str("b".into())]);
        let r = Ranking::new(&session(&ps, &winner)).unwrap();
        assert!(!r.all_players()[0].winner);
        assert!(r.all_players()[1].winner);

        // team block: index 0 color skipped, 2.. marked.
        let winner = JsVal::Arr(vec![
            JsVal::Str("team".into()),
            JsVal::Str("red".into()),
            JsVal::Str("a".into()),
            JsVal::Num(99.0), // missing key: truthy gate no-ops
            JsVal::Str("b".into()),
        ]);
        let r = Ranking::new(&session(&ps, &winner)).unwrap();
        assert!(r.all_players()[0].winner && r.all_players()[1].winner);

        // other first element / empty / absent winner: inert.
        let winner = JsVal::Arr(vec![JsVal::Str("solo".into())]);
        let r = Ranking::new(&session(&ps, &winner)).unwrap();
        assert!(!r.all_players()[0].winner);
        let r = Ranking::new(&session(&ps, &JsVal::Arr(vec![]))).unwrap();
        assert!(!r.all_players()[0].winner);
        let r = Ranking::new(&session(&ps, &JsVal::Absent)).unwrap();
        assert!(!r.all_players()[0].winner);
    }

    #[test]
    fn score_branches() {
        let mut a = player("a", true);
        a.units = true;
        a.gold = Some(Box::leak(
            vec![
                BigIntTok::BigInt(10.0),
                BigIntTok::BigInt(20.0),
                BigIntTok::BigInt(30.0),
                BigIntTok::BigInt(40.0),
                BigIntTok::BigInt(5.0),
                BigIntTok::BigInt(6.0),
            ]
            .into_boxed_slice(),
        ));
        a.conquests = Some(Box::leak(vec![BigIntTok::BigInt(1.0), BigIntTok::BigInt(2.0)].into_boxed_slice()));
        a.killed_at = Box::leak(Box::new(JsVal::Num(50.0)));
        let r = Ranking::new(&session(&[a], &v())).unwrap();
        let p = &r.all_players()[0];
        assert_eq!(r.score(p, RankType::parse("Lifetime")), Some(5.0)); // (50/100)*10
        assert_eq!(r.score(p, RankType::parse("ConquestHumans")), Some(1.0));
        assert_eq!(r.score(p, RankType::parse("ConquestNations")), Some(2.0));
        assert_eq!(r.score(p, RankType::parse("ConquestBots")), Some(0.0)); // ?? 0n
        assert_eq!(r.score(p, RankType::parse("TotalGold")), Some(111.0));
        assert_eq!(r.score(p, RankType::parse("StolenGold")), Some(40.0));
        assert_eq!(r.score(p, RankType::parse("NavalTrade")), Some(30.0));
        assert_eq!(r.score(p, RankType::parse("ConqueredGold")), Some(20.0));
        assert_eq!(r.score(p, RankType::parse("TrainTrade")), Some(11.0));
        assert_eq!(r.score(p, RankType::parse("Atoms")), Some(0.0));
        assert_eq!(r.score(p, RankType::parse("Bogus")), None); // no default arm
    }

    #[test]
    fn lifetime_survivor_and_nan() {
        let mut a = player("a", true);
        a.units = true;
        let r = Ranking::new(&session(&[a], &v())).unwrap();
        assert_eq!(r.score(&r.all_players()[0], RankType::parse("Lifetime")), Some(100.0));

        let mut b = player("b", true);
        b.units = true;
        b.killed_at = Box::leak(Box::new(JsVal::Str("zz".to_string()))); // Number -> NaN
        let r = Ranking::new(&session(&[b], &v())).unwrap();
        let s = r.score(&r.all_players()[0], RankType::parse("Lifetime")).unwrap();
        assert!(s.is_nan());
    }

    #[test]
    fn sorted_by_descending_and_stable() {
        let mk = |id: &str, atoms: f64, winner: bool| -> PlayerInput<'static> {
            let mut p = player(id, true);
            p.units = true;
            p.bombs = Box::leak(Box::new(JsVal::Obj(vec![(
                "abomb".to_string(),
                JsVal::Arr(vec![JsVal::Num(atoms)]),
            )])));
            let _ = winner;
            p
        };
        let ps = vec![mk("a", 1.0, false), mk("b", 3.0, false), mk("c", 1.0, false)];
        let r = Ranking::new(&session(&ps, &v())).unwrap();
        let sorted = r.sorted_by(RankType::parse("Atoms"));
        assert_eq!(
            sorted.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            ["b", "a", "c"] // descending, ties keep insertion order
        );

        // Winner bonus: c(1.0)+0.1 beats a(1.0).
        let winner = JsVal::Arr(vec![JsVal::Str("player".into()), JsVal::Str("c".into())]);
        let r = Ranking::new(&session(&ps, &winner)).unwrap();
        let sorted = r.sorted_by(RankType::parse("Atoms"));
        assert_eq!(
            sorted.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            ["b", "c", "a"]
        );

        // Out-of-domain type: every comparator is NaN -> original order.
        let sorted = r.sorted_by(RankType::parse("Nope"));
        assert_eq!(
            sorted.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
    }

    #[test]
    fn label_key_order() {
        assert_eq!(RANK_TYPE_LABEL_KEYS[0].0, "Lifetime");
        assert_eq!(RANK_TYPE_LABEL_KEYS[1].0, "ConquestHumans");
        assert_eq!(RANK_TYPE_LABEL_KEYS[11].0, "ConqueredGold");
        assert_eq!(RANK_TYPE_MEMBERS[0], RankType::ConquestHumans);
        assert_eq!(RankType::Mirv.as_str(), "MIRV");
    }
}
