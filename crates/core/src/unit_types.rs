//! Port of `src/client/render/types/UnitType.ts` — the canonical unit-type
//! string constants, the derived `ReadonlySet`s, the `NUKE_MAGNITUDES` blast
//! radii table, and the atlas-ordered `ALL_UNIT_TYPES` list.
//!
//! Pure data. The capture dumps: `ALL_UNIT_TYPES` in canonical order, the
//! set membership tables (STRUCTURE_TYPES 6 / NUKE_TYPES 3 /
//! SMOOTHED_NUKE_TYPES 4 — insertion order, probed by membership), and the
//! three `NUKE_MAGNITUDES` entries with their inner/outer radii plus
//! property-read probes.

use crate::js_json::{push_str, read_str};

/// The 16 canonical unit-type strings in `ALL_UNIT_TYPES` order (atlas
/// column order).
pub const ALL_UNIT_TYPES: [&str; 16] = [
    "Transport",
    "Trade Ship",
    "Warship",
    "Atom Bomb",
    "Hydrogen Bomb",
    "MIRV",
    "SAMMissile",
    "Shell",
    "MIRV Warhead",
    "City",
    "Port",
    "Factory",
    "Defense Post",
    "SAM Launcher",
    "Missile Silo",
    "Train",
];

/// `STRUCTURE_TYPES` — insertion order (City, Port, Factory, Defense Post,
/// SAM Launcher, Missile Silo).
pub const STRUCTURE_TYPES: [&str; 6] = [
    "City",
    "Port",
    "Factory",
    "Defense Post",
    "SAM Launcher",
    "Missile Silo",
];

/// `NUKE_TYPES` — insertion order (Atom Bomb, Hydrogen Bomb, MIRV). NOTE:
/// `MIRV Warhead` is deliberately NOT a nuke type here (it only joins
/// SMOOTHED_NUKE_TYPES and NUKE_ACTIVE_TYPES in PlayerStatus).
pub const NUKE_TYPES: [&str; 3] = ["Atom Bomb", "Hydrogen Bomb", "MIRV"];

/// `SMOOTHED_NUKE_TYPES` — insertion order (Atom Bomb, Hydrogen Bomb, MIRV,
/// MIRV Warhead).
pub const SMOOTHED_NUKE_TYPES: [&str; 4] =
    ["Atom Bomb", "Hydrogen Bomb", "MIRV", "MIRV Warhead"];

/// One `NUKE_MAGNITUDES` entry (blast radii in tiles). Only these THREE keys
/// have magnitudes — `MIRV` itself does NOT (the quirk the nuke telegraph
/// `if (!mag) continue` gate relies on).
pub const NUKE_MAGNITUDES: [(&str, f64, f64); 3] = [
    ("Atom Bomb", 12.0, 30.0),
    ("Hydrogen Bomb", 80.0, 100.0),
    ("MIRV Warhead", 12.0, 18.0),
];

/// `NUKE_MAGNITUDES[u.unitType]` — the object-property read (undefined for
/// every other type, including "MIRV").
pub fn nuke_magnitude(unit_type: &str) -> Option<(f64, f64)> {
    NUKE_MAGNITUDES
        .iter()
        .find(|(k, _, _)| *k == unit_type)
        .map(|(_, inner, outer)| (*inner, *outer))
}

/// `[n, (member-str)*n, m, (probe-str, has 0|1)*m]` — set dump + membership
/// probes over the args `[m,(str)*m]`.
fn dump_set(out: &mut Vec<f64>, members: &[&str], args: &[f64], i: &mut usize) {
    out.push(members.len() as f64);
    for m in members {
        push_str(out, m);
    }
    let n = args[*i] as usize;
    *i += 1;
    out.push(n as f64);
    for _ in 0..n {
        let p = read_str(args, i);
        push_str(out, &p);
        out.push(if members.iter().any(|m| *m == p) { 1.0 } else { 0.0 });
    }
}

/// `run_op(kind, args)` — capture harness entry (stateless). Kind table:
/// 0 -> `[16, (str)*16]` ALL_UNIT_TYPES in order;
/// 1 -> STRUCTURE_TYPES dump + probes (args `[m,(str)*m]`);
/// 2 -> NUKE_TYPES dump + probes;
/// 3 -> SMOOTHED_NUKE_TYPES dump + probes;
/// 4 -> NUKE_MAGNITUDES dump `[3,(key-str,inner,outer)*3]` + property-read
///   probes `[m,(probe-str,present 0|1,inner,outer)*m]` (absent entries push
///   inner=outer=0.0 beside the present flag; args `[m,(str)*m]`).
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut i = 0usize;
    match kind {
        0 => {
            let mut out = vec![ALL_UNIT_TYPES.len() as f64];
            for s in ALL_UNIT_TYPES {
                push_str(&mut out, s);
            }
            out
        }
        1 => {
            let mut out = Vec::new();
            dump_set(&mut out, &STRUCTURE_TYPES, args, &mut i);
            out
        }
        2 => {
            let mut out = Vec::new();
            dump_set(&mut out, &NUKE_TYPES, args, &mut i);
            out
        }
        3 => {
            let mut out = Vec::new();
            dump_set(&mut out, &SMOOTHED_NUKE_TYPES, args, &mut i);
            out
        }
        4 => {
            let mut out = vec![NUKE_MAGNITUDES.len() as f64];
            for (k, inner, outer) in NUKE_MAGNITUDES {
                push_str(&mut out, k);
                out.push(inner);
                out.push(outer);
            }
            let n = args[i] as usize;
            i += 1;
            out.push(n as f64);
            for _ in 0..n {
                let p = read_str(args, &mut i);
                push_str(&mut out, &p);
                match nuke_magnitude(&p) {
                    Some((inner, outer)) => {
                        out.push(1.0);
                        out.push(inner);
                        out.push(outer);
                    }
                    None => {
                        out.push(0.0);
                        out.push(0.0);
                        out.push(0.0);
                    }
                }
            }
            out
        }
        k => unreachable!("unit_types: unknown op kind {k}"),
    }
}
