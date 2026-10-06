//! Port of `src/client/render/frame/derive/PlayerStatus.ts` — the per-player
//! name/status-icon flags.
//!
//! Faithfulness notes (quirk list):
//!
//! * `NUKE_ACTIVE_TYPES` = `NUKE_TYPES` (3) plus `MIRV Warhead` — 4 entries,
//!   spread insertion order.
//! * Crown scan uses STRICT `>`: on a tilesOwned tie the FIRST alive player
//!   in players-iteration order keeps the crown.
//! * Nuke pass gate: `!isActive || !NUKE_ACTIVE_TYPES.has(unitType)` — the
//!   `nukeTargetsMe` gate order is `lpsid > 0` → `tileState !== undefined` →
//!   `targetTile !== null` (STRICT) → `(tileState[targetTile] & 0xfff) ===
//!   lpsid`. A `Uint16Array` read at a negative / fractional / out-of-range
//!   index yields `undefined`, whose `& 0xfff` coerces to `0` — never equal
//!   to a positive lpsid, so those tiles read as ownerless.
//! * Doomsday math: `under = (opts.tick ?? 0) - markedDoomsdayClockTick`,
//!   `warn = opts.doomsdayClockWarnTicks ?? 0`; draining gate `inClock &&
//!   under >= warn` (STRICT `>=`), warnProgress `inClock && warn > 0 ?
//!   max(0, min(1, under / warn)) : 0`.
//! * Relative flags gate: `localPlayer !== undefined && sid !==
//!   localPlayerSmallID`. `isTransitiveTarget` is a TRUTHY gate — present
//!   function wins over `targets.includes` (a scripted sid-set here).
//! * Alliance fraction gate: `alliance && opts.tick !== undefined &&
//!   opts.allianceDuration !== undefined && opts.localPlayerID` — the last
//!   term is the RAW option's TRUTHINESS: `""` is falsy and disables the
//!   progress bar even though `localPlayerID` (the `?? ""` copy) still feeds
//!   `allianceReq`. `find` compares `a.other === opts.localPlayerID` (raw).
//! * `remaining = max(0, expiresAt - tick)`, `fraction = max(0, min(1,
//!   remaining / max(1, duration)))` — the `max(1, ·)` duration guard.
//! * The result map only gets an entry when 11 flags OR together (crown,
//!   traitor, disconnected, inDoomsdayClock, traitorRemainingTicks > 0,
//!   nukeActive, alliance, allianceReq, target, embargo, nukeTargetsMe);
//!   draining/decaying/warnProgress alone do NOT qualify a player.
//! * Array `includes` is SameValueZero (`-0` matches `0`, `NaN` matches
//!   `NaN`).

use crate::desync_detector::{svz_key, NumMap};
use crate::game_map::{js_max, js_min};
use crate::js_json::{push_str, read_str};
use crate::jsnum::to_int32;
use crate::renderer_consts::{read_player, read_unit, PlayerState, UnitState};

/// `NUKE_ACTIVE_TYPES` — `[...NUKE_TYPES, UT_MIRV_WARHEAD]`.
pub const NUKE_ACTIVE_TYPES: [&str; 4] =
    ["Atom Bomb", "Hydrogen Bomb", "MIRV", "MIRV Warhead"];

/// `OWNER_MASK` — the smallID field of the packed `Uint16Array` state.
pub const OWNER_MASK: f64 = 4095.0;

/// The options bag (`ComputePlayerStatusOptions`). `Option` fields model
/// `undefined` vs. present; `local_player_id` keeps the RAW value so the
/// truthiness gate (`"" falsy`) and the `?? ""` copy are both faithful.
#[derive(Debug, Default)]
pub struct ComputeOptions {
    pub local_player_small_id: Option<f64>,
    pub local_player_id: Option<String>,
    /// The `Uint16Array` values (already `ToUint16`-wrapped by the capture).
    pub tile_state: Option<Vec<f64>>,
    pub tick: Option<f64>,
    pub alliance_duration: Option<f64>,
    /// `Some` = the callback is present; membership = the scripted result.
    pub transitive_targets: Option<Vec<f64>>,
    pub doomsday_clock_warn_ticks: Option<f64>,
}

/// One `PlayerStatusData` (16 fields, object key order).
#[derive(Debug, Clone, Copy, Default)]
pub struct PlayerStatusData {
    pub crown: bool,
    pub traitor: bool,
    pub disconnected: bool,
    pub in_doomsday_clock: bool,
    pub draining: bool,
    pub decaying: bool,
    pub warn_progress: f64,
    pub alliance: bool,
    pub alliance_req: bool,
    pub target: bool,
    pub embargo: bool,
    pub nuke_active: bool,
    pub nuke_targets_me: bool,
    pub traitor_remaining_ticks: f64,
    pub alliance_fraction: f64,
    pub alliance_remaining_ticks: f64,
}

/// JS `Array#includes` (SameValueZero).
fn arr_includes(a: &[f64], v: f64) -> bool {
    let kk = svz_key(v);
    a.iter().any(|x| svz_key(*x) == kk)
}

/// `Uint16Array` element read with JS index semantics: canonical unsigned
/// integer indices read the value, everything else reads `undefined` (the
/// caller's `& 0xfff` then sees `0`).
fn typed_read(arr: &[f64], idx: f64) -> Option<f64> {
    if !idx.is_finite() || idx.fract() != 0.0 || idx < 0.0 {
        return None;
    }
    let i = idx as usize;
    if i < arr.len() {
        Some(arr[i])
    } else {
        None
    }
}

/// `computePlayerStatus(players, units, opts)` — the result map as an
/// insertion-ordered `(smallID, data)` list.
pub(crate) fn compute_player_status(
    players: &NumMap<PlayerState>,
    units: &NumMap<UnitState>,
    opts: &ComputeOptions,
) -> Vec<(f64, PlayerStatusData)> {
    let lpsid = opts.local_player_small_id.unwrap_or(0.0);
    let lpid_eff = opts
        .local_player_id
        .clone()
        .unwrap_or_default();
    let local_player = if lpsid > 0.0 { players.get(lpsid) } else { None };

    // Crown: alive player with most tiles owned (STRICT `>` keeps first).
    let mut crown_small_id = -1.0;
    let mut max_tiles = 0.0;
    for ps in players.values() {
        if !ps.is_alive {
            continue;
        }
        if ps.tiles_owned > max_tiles {
            max_tiles = ps.tiles_owned;
            crown_small_id = ps.small_id;
        }
    }

    // Nukes: single pass over units -> per-owner flags.
    let mut nuke_active_owners: Vec<f64> = Vec::new();
    let mut nuke_targets_me_owners: Vec<f64> = Vec::new();
    for u in units.values() {
        if !u.is_active || !NUKE_ACTIVE_TYPES.contains(&u.unit_type.as_str()) {
            continue;
        }
        if !arr_includes(&nuke_active_owners, u.owner_id) {
            nuke_active_owners.push(u.owner_id);
        }
        if lpsid > 0.0
            && opts.tile_state.is_some()
            && u.target_tile.is_some_and(|t| {
                let raw = typed_read(opts.tile_state.as_ref().unwrap(), t);
                // `undefined & 0xfff` -> 0; a value goes through ToInt32.
                (to_int32(raw.unwrap_or(0.0)) & to_int32(OWNER_MASK)) as f64 == lpsid
            })
            && !arr_includes(&nuke_targets_me_owners, u.owner_id)
        {
            nuke_targets_me_owners.push(u.owner_id);
        }
    }

    let mut result: Vec<(f64, PlayerStatusData)> = Vec::new();
    for ps in players.values() {
        if !ps.is_alive {
            continue;
        }
        let sid = ps.small_id;
        let mut d = PlayerStatusData {
            crown: sid == crown_small_id,
            traitor: ps.is_traitor,
            disconnected: ps.is_disconnected,
            in_doomsday_clock: ps.in_doomsday_clock,
            ..Default::default()
        };
        let under = opts.tick.unwrap_or(0.0) - ps.marked_doomsday_clock_tick;
        let warn = opts.doomsday_clock_warn_ticks.unwrap_or(0.0);
        d.draining = ps.in_doomsday_clock && under >= warn;
        d.decaying = ps.in_doomsday_clock && ps.is_decaying;
        d.warn_progress = if ps.in_doomsday_clock && warn > 0.0 {
            js_max(0.0, js_min(1.0, under / warn))
        } else {
            0.0
        };
        d.traitor_remaining_ticks = ps.traitor_remaining_ticks;

        d.nuke_active = arr_includes(&nuke_active_owners, sid);
        d.nuke_targets_me = arr_includes(&nuke_targets_me_owners, sid);

        if let Some(lp) = local_player {
            if sid != lpsid {
                d.alliance = arr_includes(&lp.allies, sid);
                d.alliance_req = ps.outgoing_alliance_requests.contains(&lpid_eff);
                d.target = match &opts.transitive_targets {
                    Some(set) => arr_includes(set, sid),
                    None => arr_includes(&lp.targets, sid),
                };
                d.embargo = arr_includes(&lp.embargoes, sid)
                    || arr_includes(&ps.embargoes, lpsid);

                if let (Some(tick), Some(dur), Some(raw)) = (
                    opts.tick,
                    opts.alliance_duration,
                    opts.local_player_id.as_deref(),
                ) {
                    if d.alliance && !raw.is_empty() {
                        if let Some(found) = ps.alliances.iter().find(|a| a.other == raw) {
                            let remaining = js_max(0.0, found.expires_at - tick);
                            d.alliance_fraction = js_max(0.0, js_min(1.0, remaining / js_max(1.0, dur)));
                            d.alliance_remaining_ticks = remaining;
                        }
                    }
                }
            }
        }

        if d.crown
            || d.traitor
            || d.disconnected
            || d.in_doomsday_clock
            || d.traitor_remaining_ticks > 0.0
            || d.nuke_active
            || d.alliance
            || d.alliance_req
            || d.target
            || d.embargo
            || d.nuke_targets_me
        {
            result.push((sid, d));
        }
    }
    result
}

/// `run_op(kind, args)` — capture harness entry (stateless). Kind table:
/// 0 -> computePlayerStatus. args `[lpsidFlag, (lpsid), lpidFlag,
///   (lpid-str), tileFlag, tileN, (tileVal)*tileN, tickFlag, (tick),
///   durFlag, (dur), ttFlag, ttN, (sid)*ttN, warnFlag, (warn),
///   playersN, (PlayerState)*n, unitsN, (UnitState)*n]`; res `[resultN,
///   (sid, crown, traitor, disconnected, inDoomsdayClock, draining,
///   decaying, alliance, allianceReq, target, embargo, nukeActive,
///   nukeTargetsMe, warnProgress, traitorRemainingTicks, allianceFraction,
///   allianceRemainingTicks)*resultN]` (12 flags as 0|1, then the four
///   numeric fields);
/// 1 -> `[OWNER_MASK, 4, (NUKE_ACTIVE_TYPES-str)*4]`.
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut i = 0usize;
    match kind {
        0 => {
            let flag = |i: &mut usize| {
                let f = args[*i] != 0.0;
                *i += 1;
                f
            };
            let mut opts = ComputeOptions::default();
            if flag(&mut i) {
                opts.local_player_small_id = Some(args[i]);
                i += 1;
            }
            if flag(&mut i) {
                opts.local_player_id = Some(read_str(args, &mut i));
            }
            if flag(&mut i) {
                let n = args[i] as usize;
                i += 1;
                opts.tile_state = Some(args[i..i + n].to_vec());
                i += n;
            }
            if flag(&mut i) {
                opts.tick = Some(args[i]);
                i += 1;
            }
            if flag(&mut i) {
                opts.alliance_duration = Some(args[i]);
                i += 1;
            }
            if flag(&mut i) {
                let n = args[i] as usize;
                i += 1;
                opts.transitive_targets = Some(args[i..i + n].to_vec());
                i += n;
            }
            if flag(&mut i) {
                opts.doomsday_clock_warn_ticks = Some(args[i]);
                i += 1;
            }
            let np = args[i] as usize;
            i += 1;
            let mut players: NumMap<PlayerState> = NumMap::default();
            for _ in 0..np {
                let p = read_player(args, &mut i);
                players.set(p.small_id, p);
            }
            let nu = args[i] as usize;
            i += 1;
            let mut units: NumMap<UnitState> = NumMap::default();
            for _ in 0..nu {
                let u = read_unit(args, &mut i);
                units.set(u.id, u);
            }
            let result = compute_player_status(&players, &units, &opts);
            let mut out = vec![result.len() as f64];
            for (sid, d) in &result {
                out.push(*sid);
                for b in [
                    d.crown,
                    d.traitor,
                    d.disconnected,
                    d.in_doomsday_clock,
                    d.draining,
                    d.decaying,
                    d.alliance,
                    d.alliance_req,
                    d.target,
                    d.embargo,
                    d.nuke_active,
                    d.nuke_targets_me,
                ] {
                    out.push(if b { 1.0 } else { 0.0 });
                }
                out.push(d.warn_progress);
                out.push(d.traitor_remaining_ticks);
                out.push(d.alliance_fraction);
                out.push(d.alliance_remaining_ticks);
            }
            out
        }
        1 => {
            let mut out = vec![OWNER_MASK, NUKE_ACTIVE_TYPES.len() as f64];
            for s in NUKE_ACTIVE_TYPES {
                push_str(&mut out, s);
            }
            out
        }
        k => unreachable!("player_status: unknown op kind {k}"),
    }
}
