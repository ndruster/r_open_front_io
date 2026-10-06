//! Port of `src/client/render/frame/derive/AttackRings.ts` — the attack-ring
//! indicators for transport ships with active targets.
//!
//! Faithfulness notes (quirk list):
//!
//! * Gate order: `unitType !== UT_TRANSPORT` → `targetTile === null` →
//!   `!isActive` → `retreating` → `ownerID !== owner` (the owner filter is
//!   LAST — a foreign transport still walks the first four gates).
//! * `targetTile === null` is STRICT: `undefined` would pass the gate (the
//!   codec keeps null as the only absent form, so this is pinned by the
//!   interface domain `number | null`).
//! * The position split is JS number math: `x = t % mapW` (f64 modulo),
//!   `y = (t - (t % mapW)) / mapW` — NOT integer division; a fractional
//!   targetTile would yield fractional x/y (the capture pins the integer
//!   domain, the formula is transcribed verbatim).
//! * Iteration order is the units Map insertion order; `unitId` is `u.id`.

use crate::desync_detector::NumMap;
use crate::jsnum::js_mod;
use crate::renderer_consts::{read_unit, UnitState};
use crate::unit_types::ALL_UNIT_TYPES;

/// The `UT_TRANSPORT` string (the value import the TS module keeps).
pub const UT_TRANSPORT: &str = ALL_UNIT_TYPES[0]; // "Transport"

/// One `AttackRingInput`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AttackRingInput {
    pub x: f64,
    pub y: f64,
    pub unit_id: f64,
}

/// `extractAttackRings(units, mapW, owner)` over the scripted units Map
/// (insertion order preserved by [`NumMap`]).
pub(crate) fn extract_attack_rings(units: &NumMap<UnitState>, map_w: f64, owner: f64) -> Vec<AttackRingInput> {
    let mut rings = Vec::new();
    for u in units.values() {
        if u.unit_type != UT_TRANSPORT {
            continue;
        }
        let Some(t) = u.target_tile else { continue };
        if !u.is_active || u.retreating {
            continue;
        }
        if u.owner_id != owner {
            continue;
        }
        rings.push(AttackRingInput {
            x: js_mod(t, map_w),
            y: (t - js_mod(t, map_w)) / map_w,
            unit_id: u.id,
        });
    }
    rings
}

/// `run_op(kind, args)` — capture harness entry. Kind table:
/// 0 -> extractAttackRings over `[mapW, owner, n, (UnitState)*n]`; res is
///   `[m, (x, y, unitId)*m]`.
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut i = 0usize;
    match kind {
        0 => {
            let map_w = args[i];
            i += 1;
            let owner = args[i];
            i += 1;
            let n = args[i] as usize;
            i += 1;
            let mut units: NumMap<UnitState> = NumMap::default();
            for _ in 0..n {
                let u = read_unit(args, &mut i);
                units.set(u.id, u);
            }
            let rings = extract_attack_rings(&units, map_w, owner);
            let mut out = vec![rings.len() as f64];
            for r in rings {
                out.push(r.x);
                out.push(r.y);
                out.push(r.unit_id);
            }
            out
        }
        k => unreachable!("attack_rings: unknown op kind {k}"),
    }
}
