//! Port of the VALUE exports of `src/client/render/types/Renderer.ts`: the
//! `TrainType` and `PlayerTypeEnum` numeric enums, `MAX_NUKE_EXPLOSION_COLORS`
//! and `DEFAULT_NUKE_EXPLOSION_COLOR`. The interfaces (PlayerState /
//! UnitState / …) have no runtime and are modelled by the derive ports that
//! read them (`player_status`, `attack_rings`, `nuke_telegraphs`, …).
//!
//! The capture dumps the enum reverse-mapping tables (numeric TS enums carry
//! both name→value and value→name at runtime), the two constants, and the
//! default colour triple.

use crate::js_json::{push_str, read_str};

/// `TrainType` — name/value pairs in declaration order.
pub const TRAIN_TYPE: [(&str, f64); 3] =
    [("Engine", 0.0), ("TailEngine", 1.0), ("Carriage", 2.0)];

/// `PlayerTypeEnum` — name/value pairs in declaration order.
pub const PLAYER_TYPE: [(&str, f64); 3] =
    [("Human", 0.0), ("Bot", 1.0), ("Nation", 2.0)];

/// `MAX_NUKE_EXPLOSION_COLORS` — the vertex-attribute palette budget.
pub const MAX_NUKE_EXPLOSION_COLORS: f64 = 4.0;

/// `DEFAULT_NUKE_EXPLOSION_COLOR` — the fallback purple, `[0.6, 0.1, 1]`.
/// NOTE: `0.6` and `0.1` are NOT exact binary fractions; the capture pins the
/// doubles by their shortest round-tripping decimal, and `1` is an integer
/// (JS arrays are heterogeneous, but all three land as f64 here).
pub const DEFAULT_NUKE_EXPLOSION_COLOR: [f64; 3] = [0.6, 0.1, 1.0];

/// `run_op(kind, args)` — capture harness entry (stateless). Kind table:
/// 0 -> TrainType forward table `[3,(name-str,value)*3]`;
/// 1 -> TrainType full runtime-object key dump `[6,(key-str)*6]` — the
///   numeric enum's reverse mapping means `Object.keys` lists the integer
///   keys ascending ("0","1","2") BEFORE the declaration-order string keys;
/// 2 -> PlayerTypeEnum forward table `[3,(name-str,value)*3]`;
/// 3 -> PlayerTypeEnum full runtime-object key dump `[6,(key-str)*6]`;
/// 4 -> `[MAX_NUKE_EXPLOSION_COLORS, 3, (color f64)*3]`.
pub fn run_op(kind: u8, _args: &[f64]) -> Vec<f64> {
    match kind {
        0 | 2 => {
            let table = if kind == 0 { &TRAIN_TYPE[..] } else { &PLAYER_TYPE[..] };
            let mut out = vec![table.len() as f64];
            for (name, v) in table {
                push_str(&mut out, name);
                out.push(*v);
            }
            out
        }
        1 | 3 => {
            let table = if kind == 1 { &TRAIN_TYPE[..] } else { &PLAYER_TYPE[..] };
            let mut out = vec![(2 * table.len()) as f64];
            for (_, v) in table {
                push_str(&mut out, &v.to_string());
            }
            for (name, _) in table {
                push_str(&mut out, name);
            }
            out
        }
        4 => {
            let mut out = vec![MAX_NUKE_EXPLOSION_COLORS, 3.0];
            out.extend_from_slice(&DEFAULT_NUKE_EXPLOSION_COLOR);
            out
        }
        k => unreachable!("renderer_consts: unknown op kind {k}"),
    }
}

// ---------------------------------------------------------------------------
// The client render-layer host objects (Renderer.ts interfaces) that the
// frame-derive ports read. Only the fields the derive functions actually
// touch are modelled (precedent: the narrow Client stub in desync_detector).
// ---------------------------------------------------------------------------

/// One `AllianceData` — only `other` / `expiresAt` are read (PlayerStatus).
#[derive(Debug, Clone, Default)]
pub struct AllianceData {
    pub other: String,
    pub expires_at: f64,
}

/// The narrow `PlayerState` (Renderer.ts L61-96) — fields read by
/// `alliance_clusters`, `player_status` and `relation_matrix`.
#[derive(Debug, Clone, Default)]
pub struct PlayerState {
    pub small_id: f64,
    pub is_alive: bool,
    pub is_disconnected: bool,
    pub tiles_owned: f64,
    pub is_traitor: bool,
    pub traitor_remaining_ticks: f64,
    pub in_doomsday_clock: bool,
    pub is_decaying: bool,
    pub marked_doomsday_clock_tick: f64,
    pub allies: Vec<f64>,
    pub embargoes: Vec<f64>,
    pub targets: Vec<f64>,
    pub outgoing_alliance_requests: Vec<String>,
    pub alliances: Vec<AllianceData>,
}

/// The narrow `UnitState` (Renderer.ts L98-127) — fields read by
/// `attack_rings`, `nuke_telegraphs` and `player_status`. `target_tile`
/// models `number | null`.
#[derive(Debug, Clone, Default)]
pub struct UnitState {
    pub id: f64,
    pub unit_type: String,
    pub owner_id: f64,
    pub is_active: bool,
    pub retreating: bool,
    pub wait_ticks: f64,
    pub target_tile: Option<f64>,
}

/// The narrow `PlayerStatic` (Renderer.ts L18-36) — only `smallID` / `team`
/// are read (`buildTeamMap`). `team` models `string | null`.
#[derive(Debug, Clone)]
pub struct PlayerStatic {
    pub small_id: f64,
    pub team: Option<String>,
}

/// Wire form of one PlayerState (see `tools/gen_vectors.mjs`):
/// `[smallID, alive, disconnected, traitor, inClock, decaying, tilesOwned,
/// traitorRemainingTicks, markedDoomsdayClockTick,
/// n, (ally)*n, n, (embargo)*n, n, (target)*n, n, (str)*n, n,
/// (other-str, expiresAt)*n]` (flags are 0|1 tokens).
pub fn push_player(out: &mut Vec<f64>, p: &PlayerState) {
    out.push(p.small_id);
    for b in [
        p.is_alive,
        p.is_disconnected,
        p.is_traitor,
        p.in_doomsday_clock,
        p.is_decaying,
    ] {
        out.push(if b { 1.0 } else { 0.0 });
    }
    out.push(p.tiles_owned);
    out.push(p.traitor_remaining_ticks);
    out.push(p.marked_doomsday_clock_tick);
    out.push(p.allies.len() as f64);
    out.extend(p.allies.iter().copied());
    out.push(p.embargoes.len() as f64);
    out.extend(p.embargoes.iter().copied());
    out.push(p.targets.len() as f64);
    out.extend(p.targets.iter().copied());
    out.push(p.outgoing_alliance_requests.len() as f64);
    for s in &p.outgoing_alliance_requests {
        push_str(out, s);
    }
    out.push(p.alliances.len() as f64);
    for a in &p.alliances {
        push_str(out, &a.other);
        out.push(a.expires_at);
    }
}

/// Read one PlayerState at `*i` (inverse of [`push_player`]).
pub fn read_player(args: &[f64], i: &mut usize) -> PlayerState {
    let read_list = |i: &mut usize| -> Vec<f64> {
        let n = args[*i] as usize;
        *i += 1;
        let v = args[*i..*i + n].to_vec();
        *i += n;
        v
    };
    let small_id = args[*i];
    *i += 1;
    let flag = |i: &mut usize| {
        let b = args[*i] != 0.0;
        *i += 1;
        b
    };
    let is_alive = flag(i);
    let is_disconnected = flag(i);
    let is_traitor = flag(i);
    let in_doomsday_clock = flag(i);
    let is_decaying = flag(i);
    let tiles_owned = args[*i];
    *i += 1;
    let traitor_remaining_ticks = args[*i];
    *i += 1;
    let marked_doomsday_clock_tick = args[*i];
    *i += 1;
    let allies = read_list(i);
    let embargoes = read_list(i);
    let targets = read_list(i);
    let n_req = args[*i] as usize;
    *i += 1;
    let outgoing_alliance_requests =
        (0..n_req).map(|_| read_str(args, i)).collect::<Vec<_>>();
    let n_al = args[*i] as usize;
    *i += 1;
    let alliances = (0..n_al)
        .map(|_| {
            let other = read_str(args, i);
            let expires_at = args[*i];
            *i += 1;
            AllianceData { other, expires_at }
        })
        .collect::<Vec<_>>();
    PlayerState {
        small_id,
        is_alive,
        is_disconnected,
        tiles_owned,
        is_traitor,
        traitor_remaining_ticks,
        in_doomsday_clock,
        is_decaying,
        marked_doomsday_clock_tick,
        allies,
        embargoes,
        targets,
        outgoing_alliance_requests,
        alliances,
    }
}

/// Wire form of one UnitState: `[id, (unitType-str), ownerID, active,
/// retreating, waitTicks, targetTile-val]` (targetTile crosses through the
/// js_json codec: `[2]` null, `[3,v]` number).
pub fn push_unit(out: &mut Vec<f64>, u: &UnitState) {
    out.push(u.id);
    push_str(out, &u.unit_type);
    out.push(u.owner_id);
    out.push(if u.is_active { 1.0 } else { 0.0 });
    out.push(if u.retreating { 1.0 } else { 0.0 });
    out.push(u.wait_ticks);
    match u.target_tile {
        None => out.push(2.0),
        Some(t) => {
            out.push(3.0);
            out.push(t);
        }
    }
}

/// Read one UnitState at `*i` (inverse of [`push_unit`]).
pub fn read_unit(args: &[f64], i: &mut usize) -> UnitState {
    let id = args[*i];
    *i += 1;
    let unit_type = read_str(args, i);
    let owner_id = args[*i];
    *i += 1;
    let is_active = args[*i] != 0.0;
    *i += 1;
    let retreating = args[*i] != 0.0;
    *i += 1;
    let wait_ticks = args[*i];
    *i += 1;
    let target_tile = match args[*i] as i32 {
        2 => {
            *i += 1;
            None
        }
        3 => {
            *i += 1;
            let t = args[*i];
            *i += 1;
            Some(t)
        }
        c => unreachable!("read_unit: bad targetTile code {c}"),
    };
    UnitState {
        id,
        unit_type,
        owner_id,
        is_active,
        retreating,
        wait_ticks,
        target_tile,
    }
}

/// Wire form of one PlayerStatic: `[smallID, team-val]` (`[2]` null,
/// `[5,len,u*]` string).
pub fn push_static(out: &mut Vec<f64>, p: &PlayerStatic) {
    out.push(p.small_id);
    match &p.team {
        None => out.push(2.0),
        Some(t) => {
            out.push(5.0);
            push_str(out, t);
        }
    }
}

/// Read one PlayerStatic at `*i` (inverse of [`push_static`]).
pub fn read_static(args: &[f64], i: &mut usize) -> PlayerStatic {
    let small_id = args[*i];
    *i += 1;
    let team = match args[*i] as i32 {
        2 => {
            *i += 1;
            None
        }
        5 => {
            *i += 1;
            Some(read_str(args, i))
        }
        c => unreachable!("read_static: bad team code {c}"),
    };
    PlayerStatic { small_id, team }
}
