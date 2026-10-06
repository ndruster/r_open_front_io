//! Port of `src/client/StatsConstants.ts` — the leaderboard column-id table
//! and the per-kind default column sets.
//!
//! Pure data. `COLUMN_IDS` is the 21-entry ordered list; `DEFAULT_STATS_COLUMNS`
//! is the two-key object (player, team) whose arrays are subsets of the ids.
//! The capture dumps both tables and the DEFAULT arrays' membership in
//! COLUMN_IDS order.

use crate::js_json::push_str;

/// `COLUMN_IDS` — the 21 stats column ids in declaration order.
pub const COLUMN_IDS: [&str; 21] = [
    "rank",
    "clan",
    "player",
    "playerType",
    "team",
    "tiles",
    "gold",
    "goldIncomePerMin",
    "shipTradeGoldPerMin",
    "piracyGoldPerMin",
    "trainTradeGoldPerMin",
    "troops",
    "maxtroops",
    "cities",
    "ports",
    "factories",
    "silos",
    "sams",
    "warships",
    "allies",
    "betrayals",
];

/// `DEFAULT_STATS_COLUMNS.player` — declaration order.
pub const DEFAULT_STATS_PLAYER: [&str; 4] = ["clan", "tiles", "gold", "maxtroops"];

/// `DEFAULT_STATS_COLUMNS.team` — declaration order.
pub const DEFAULT_STATS_TEAM: [&str; 3] = ["tiles", "gold", "maxtroops"];

/// `run_op(kind, args)` — capture harness entry (stateless). Kind table:
/// 0 -> COLUMN_IDS `[21, (str)*21]`;
/// 1 -> DEFAULT_STATS_COLUMNS dump `[2, (key-str, n, (str)*n)*2]` — key order
///   player,team (source object order).
pub fn run_op(kind: u8, _args: &[f64]) -> Vec<f64> {
    match kind {
        0 => {
            let mut out = vec![COLUMN_IDS.len() as f64];
            for s in COLUMN_IDS {
                push_str(&mut out, s);
            }
            out
        }
        1 => {
            let mut out = vec![2.0];
            push_str(&mut out, "player");
            out.push(DEFAULT_STATS_PLAYER.len() as f64);
            for s in DEFAULT_STATS_PLAYER {
                push_str(&mut out, s);
            }
            push_str(&mut out, "team");
            out.push(DEFAULT_STATS_TEAM.len() as f64);
            for s in DEFAULT_STATS_TEAM {
                push_str(&mut out, s);
            }
            out
        }
        k => unreachable!("stats_constants: unknown op kind {k}"),
    }
}
