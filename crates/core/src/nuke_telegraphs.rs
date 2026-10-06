//! Port of `src/client/render/frame/derive/NukeTelegraphs.ts` — the in-flight
//! nuke target circles, classified by launcher relation.
//!
//! Faithfulness notes (quirk list):
//!
//! * `classifyOwner` gate order: `localPlayerID <= 0` → ENEMY (replay /
//!   spectator colors everything as enemy); `ownerID === localPlayerID` →
//!   SELF; then the relation-matrix branch — the matrix must be TRUTHY
//!   (undefined skips it), `ownerID > 0 && ownerID < relationSize &&
//!   localPlayerID < relationSize`, and `matrix[lpi * size + oi] === 1`
//!   (STRICT `===`, RELATION_FRIENDLY). Falls through to ENEMY.
//! * Main-loop gate order: `targetTile === null || !isActive || waitTicks >
//!   0` continue (STRICT null; `waitTicks > 0` STRICT — a 0 wait passes).
//! * `motionPlans?.get(u.id)` — an absent map reads undefined; `plan &&
//!   plan.startTick > currentTick` continue (`>` STRICT — a plan starting
//!   exactly now still telegraphs).
//! * `NUKE_MAGNITUDES[u.unitType]` truthy gate — only Atom Bomb / Hydrogen
//!   Bomb / MIRV Warhead have magnitudes; a MIRV (unit type "MIRV") has NO
//!   mag entry and is SKIPPED even though it is a NUKE_TYPE.
//! * Default parameters: `localPlayerID = 0`, `relationSize = 0`,
//!   `currentTick = 0` (relationMatrix has no default — undefined).
//! * The FromIds variant iterates the nukeIds ARRAY order and looks units up
//!   by id (a missing id `!u` continues); otherwise the body is identical.

use crate::desync_detector::NumMap;
use crate::jsnum::js_mod;
use crate::renderer_consts::{read_unit, UnitState};
use crate::unit_types::nuke_magnitude;

/// `RELATION_FRIENDLY` — must match RelationMatrix.ts.
pub const RELATION_FRIENDLY: f64 = 1.0;
/// `TELEGRAPH_SELF`.
pub const TELEGRAPH_SELF: f64 = 0.0;
/// `TELEGRAPH_FRIENDLY`.
pub const TELEGRAPH_FRIENDLY: f64 = 1.0;
/// `TELEGRAPH_ENEMY`.
pub const TELEGRAPH_ENEMY: f64 = 2.0;

/// One `NukeTelegraphData`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NukeTelegraphData {
    pub x: f64,
    pub y: f64,
    pub inner_radius: f64,
    pub outer_radius: f64,
    pub relation: f64,
}

/// `classifyOwner(ownerID, localPlayerID, relationMatrix, relationSize)`.
pub fn classify_owner(
    owner_id: f64,
    local_player_id: f64,
    relation_matrix: Option<&[u8]>,
    relation_size: f64,
) -> f64 {
    if local_player_id <= 0.0 {
        return TELEGRAPH_ENEMY;
    }
    if owner_id == local_player_id {
        return TELEGRAPH_SELF;
    }
    if let Some(m) = relation_matrix {
        if owner_id > 0.0
            && owner_id < relation_size
            && local_player_id < relation_size
            && m[(local_player_id * relation_size + owner_id) as usize] == RELATION_FRIENDLY as u8
        {
            return TELEGRAPH_FRIENDLY;
        }
    }
    TELEGRAPH_ENEMY
}

/// The shared telegraph body for one unit (the two exported functions differ
/// only in the iteration source).
fn telegraph_for(
    u: &UnitState,
    map_w: f64,
    local_player_id: f64,
    relation_matrix: Option<&[u8]>,
    relation_size: f64,
    motion_plans: Option<&NumMap<f64>>,
    current_tick: f64,
) -> Option<NukeTelegraphData> {
    let t = u.target_tile?;
    if !u.is_active || u.wait_ticks > 0.0 {
        return None;
    }
    if let Some(plans) = motion_plans {
        if let Some(start) = plans.get(u.id) {
            if *start > current_tick {
                return None;
            }
        }
    }
    let (inner, outer) = nuke_magnitude(&u.unit_type)?;
    Some(NukeTelegraphData {
        x: js_mod(t, map_w),
        y: (t - js_mod(t, map_w)) / map_w,
        inner_radius: inner,
        outer_radius: outer,
        relation: classify_owner(u.owner_id, local_player_id, relation_matrix, relation_size),
    })
}

/// `extractNukeTelegraphs(units, mapW, localPlayerID=0, relationMatrix?,
/// relationSize=0, motionPlans?, currentTick=0)`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn extract_nuke_telegraphs(
    units: &NumMap<UnitState>,
    map_w: f64,
    local_player_id: f64,
    relation_matrix: Option<&[u8]>,
    relation_size: f64,
    motion_plans: Option<&NumMap<f64>>,
    current_tick: f64,
) -> Vec<NukeTelegraphData> {
    let mut telegraphs = Vec::new();
    for u in units.values() {
        if let Some(d) = telegraph_for(
            u,
            map_w,
            local_player_id,
            relation_matrix,
            relation_size,
            motion_plans,
            current_tick,
        ) {
            telegraphs.push(d);
        }
    }
    telegraphs
}

/// `extractNukeTelegraphsFromIds(nukeIds, units, ...)` — the targeted variant
/// iterating the ID array order.
#[allow(clippy::too_many_arguments)]
pub(crate) fn extract_nuke_telegraphs_from_ids(
    nuke_ids: &[f64],
    units: &NumMap<UnitState>,
    map_w: f64,
    local_player_id: f64,
    relation_matrix: Option<&[u8]>,
    relation_size: f64,
    motion_plans: Option<&NumMap<f64>>,
    current_tick: f64,
) -> Vec<NukeTelegraphData> {
    let mut telegraphs = Vec::new();
    for id in nuke_ids {
        let Some(u) = units.get(*id) else { continue };
        if let Some(d) = telegraph_for(
            u,
            map_w,
            local_player_id,
            relation_matrix,
            relation_size,
            motion_plans,
            current_tick,
        ) {
            telegraphs.push(d);
        }
    }
    telegraphs
}

/// `run_op(kind, args)` — capture harness entry. Kind table:
/// 0 -> extractNukeTelegraphs over
///   `[mapW, localPlayerID, matrixFlag 0|1, relationSize, plansFlag 0|1,
///   currentTick, n, (UnitState)*n, (p,(id,startTick)*p when plansFlag),
///   (k,(index,value)*k when matrixFlag)]` — the relation matrix rides
///   SPARSE (nonzero cells only, the rest 0); res `[m, (x,y,inner,
///   outer,relation)*m]`;
/// 1 -> extractNukeTelegraphsFromIds — same arg shape plus
///   `[k, (nukeId)*k]` BEFORE the units;
/// 2 -> classifyOwner direct: `[ownerID, localPlayerID, matrixFlag,
///   relationSize, (k,(index,value)*k when flag)]` -> `[relation]`.
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut i = 0usize;
    let read_common = |i: &mut usize| -> (f64, f64, bool, f64, bool, f64) {
        let map_w = args[*i];
        *i += 1;
        let lpi = args[*i];
        *i += 1;
        let matrix_flag = args[*i] != 0.0;
        *i += 1;
        let relation_size = args[*i];
        *i += 1;
        let plans_flag = args[*i] != 0.0;
        *i += 1;
        let current_tick = args[*i];
        *i += 1;
        (map_w, lpi, matrix_flag, relation_size, plans_flag, current_tick)
    };
    let read_matrix = |i: &mut usize, flag: bool, size: f64| -> Vec<u8> {
        if !flag {
            return Vec::new();
        }
        let mut m = vec![0u8; (size * size) as usize];
        let k = args[*i] as usize;
        *i += 1;
        for _ in 0..k {
            let idx = args[*i] as usize;
            *i += 1;
            let v = args[*i] as u8;
            *i += 1;
            if idx < m.len() {
                m[idx] = v;
            }
        }
        m
    };
    match kind {
        0 | 1 => {
            let (map_w, lpi, matrix_flag, relation_size, plans_flag, current_tick) =
                read_common(&mut i);
            let ids = if kind == 1 {
                let k = args[i] as usize;
                i += 1;
                let v: Vec<f64> = args[i..i + k].to_vec();
                i += k;
                v
            } else {
                Vec::new()
            };
            let n = args[i] as usize;
            i += 1;
            let mut units: NumMap<UnitState> = NumMap::default();
            for _ in 0..n {
                let u = read_unit(args, &mut i);
                units.set(u.id, u);
            }
            let mut plans: NumMap<f64> = NumMap::default();
            if plans_flag {
                let p = args[i] as usize;
                i += 1;
                for _ in 0..p {
                    let id = args[i];
                    i += 1;
                    let start = args[i];
                    i += 1;
                    plans.set(id, start);
                }
            }
            let matrix = read_matrix(&mut i, matrix_flag, relation_size);
            let mf = if matrix_flag { Some(&matrix[..]) } else { None };
            let pf = if plans_flag { Some(&plans) } else { None };
            let telegraphs = if kind == 0 {
                extract_nuke_telegraphs(&units, map_w, lpi, mf, relation_size, pf, current_tick)
            } else {
                extract_nuke_telegraphs_from_ids(
                    &ids,
                    &units,
                    map_w,
                    lpi,
                    mf,
                    relation_size,
                    pf,
                    current_tick,
                )
            };
            let mut out = vec![telegraphs.len() as f64];
            for t in telegraphs {
                out.push(t.x);
                out.push(t.y);
                out.push(t.inner_radius);
                out.push(t.outer_radius);
                out.push(t.relation);
            }
            out
        }
        2 => {
            let owner_id = args[i];
            i += 1;
            let lpi = args[i];
            i += 1;
            let matrix_flag = args[i] != 0.0;
            i += 1;
            let relation_size = args[i];
            i += 1;
            let matrix = read_matrix(&mut i, matrix_flag, relation_size);
            let mf = if matrix_flag { Some(&matrix[..]) } else { None };
            vec![classify_owner(owner_id, lpi, mf, relation_size)]
        }
        k => unreachable!("nuke_telegraphs: unknown op kind {k}"),
    }
}
