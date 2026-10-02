//! Port of the runtime-value subset of `src/core/CosmeticSchemas.ts`: the
//! three `as const` effect-type arrays, the four pure effect/slot functions,
//! and the `DefaultPattern` literal.
//!
//! Scope: the string-comparison surface the UI uses to route effect
//! cosmetics. Faithfulness notes:
//!
//! * The `z.*` schema declarations (`ProductSchema` … `CosmeticsSchema`) are
//!   zod wire-validation and are not ported — the pure functions never touch
//!   them at runtime (the capture loads the module with an inert zod Proxy,
//!   same shim as `server_list`).
//! * `findEffect` / `findEffectForSlot` are not ported: they ride on the
//!   nested `Cosmetics` catalog object (`effects[effectType][name]`), which
//!   has no runtime representation here yet. `lenientRecord` is zod-internal.
//! * `DefaultPattern.colorPalette` is `undefined` in the TS; the Rust twin
//!   carries the two string fields only (there is no `undefined` to model,
//!   and the capture never reads that field).
//! * All comparisons are JS `===` on strings; every string in this module is
//!   ASCII, so Rust `&str` equality matches code-unit-for-code-unit.

/// `EFFECT_TYPES` — the seven known effect types (`as const` in the TS).
pub const EFFECT_TYPES: [&str; 7] = [
    "transportShipTrail",
    "nukeTrail",
    "nukeExplosion",
    "structures",
    "warship",
    "train",
    "railroad",
];

/// `TRAIL_EFFECT_TYPES` — the subset rendering through the shared trail
/// palette (block order matches trail.frag.glsl).
pub const TRAIL_EFFECT_TYPES: [&str; 2] = ["transportShipTrail", "nukeTrail"];

/// `NUKE_EXPLOSION_TYPES` — the bombs a nuke-explosion effect applies to.
pub const NUKE_EXPLOSION_TYPES: [&str; 3] = ["atom", "hydro", "mirvWarhead"];

/// `isTrailEffect(effect)` — the effect's type is in `TRAIL_EFFECT_TYPES`.
pub fn is_trail_effect(effect_type: &str) -> bool {
    TRAIL_EFFECT_TYPES.contains(&effect_type)
}

/// `isNukeExplosionEffect(effect)` — the effect's type is exactly
/// `"nukeExplosion"`.
pub fn is_nuke_explosion_effect(effect_type: &str) -> bool {
    effect_type == "nukeExplosion"
}

/// `effectTypeForSlot(slot)` — the effectType a selection slot resolves to,
/// or `None` for an unknown/stale slot. Nuke types map to `"nukeExplosion"`;
/// the bare `"nukeExplosion"` key (pre per-nukeType split) resolves to
/// `None`, matching the TS's explicit `slot !== "nukeExplosion"` guard.
pub fn effect_type_for_slot(slot: &str) -> Option<&'static str> {
    if NUKE_EXPLOSION_TYPES.contains(&slot) {
        return Some("nukeExplosion");
    }
    if let Some(t) = EFFECT_TYPES.iter().copied().find(|t| *t == slot) {
        if slot != "nukeExplosion" {
            return Some(t);
        }
    }
    None
}

/// `effectMatchesSlot(effect, slot)` — whether `effect` may occupy `slot`.
/// `nuke_type` is `Some(attributes.nukeType)` exactly when the effect's type
/// is `"nukeExplosion"` (mirroring the TS narrowing via
/// `isNukeExplosionEffect`).
pub fn effect_matches_slot(effect_type: &str, nuke_type: Option<&str>, slot: &str) -> bool {
    if effect_type_for_slot(slot) != Some(effect_type) {
        return false;
    }
    if is_nuke_explosion_effect(effect_type) {
        return nuke_type == Some(slot);
    }
    true
}

/// `DefaultPattern.name`.
pub const DEFAULT_PATTERN_NAME: &str = "default";
/// `DefaultPattern.patternData`.
pub const DEFAULT_PATTERN_DATA: &str = "AAAAAA";

// ---------------------------------------------------------------- vectors op
//
// A flat `f64` token runner shared by the golden replay and the wasm probe.
// Strings cross as `[len, u0, .. u(len-1)]` (UTF-16 code units).
//
// kind table (mirrors the capture):
//   0 [0] -> [7, (str)*7]                       EFFECT_TYPES dump
//   1 [0] -> [2, (str)*2]                       TRAIL_EFFECT_TYPES dump
//   2 [0] -> [3, (str)*3]                       NUKE_EXPLOSION_TYPES dump
//   3 [0] -> [(name, data)]                     DefaultPattern
//   4 [n, (str)*n] -> [n, (0/1)*n]              is_trail_effect batch
//   5 [n, (str)*n] -> [n, (0/1)*n]              is_nuke_explosion_effect batch
//   6 [n, (str)*n] -> [n, ([1,str] | [0])*n]    effect_type_for_slot batch
//   7 [n, (et, np, ns?, slot)*n] -> [n, (0/1)*n] effect_matches_slot batch
//     (et = effectType, np = nuke present 0/1, ns = attributes.nukeType when
//     np=1, slot = the selection slot)

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
    fn string(&mut self) -> String {
        let len = self.u();
        let units: Vec<u16> = (0..len).map(|_| self.f() as u16).collect();
        String::from_utf16_lossy(&units)
    }
}

fn push_string(out: &mut Vec<f64>, s: &str) {
    let units: Vec<u16> = s.encode_utf16().collect();
    out.push(units.len() as f64);
    out.extend(units.iter().map(|&u| u as f64));
}

pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    let mut c = Cur(args, 0);
    match kind {
        0 => {
            out.push(EFFECT_TYPES.len() as f64);
            for e in EFFECT_TYPES {
                push_string(&mut out, e);
            }
        }
        1 => {
            out.push(TRAIL_EFFECT_TYPES.len() as f64);
            for e in TRAIL_EFFECT_TYPES {
                push_string(&mut out, e);
            }
        }
        2 => {
            out.push(NUKE_EXPLOSION_TYPES.len() as f64);
            for e in NUKE_EXPLOSION_TYPES {
                push_string(&mut out, e);
            }
        }
        3 => {
            push_string(&mut out, DEFAULT_PATTERN_NAME);
            push_string(&mut out, DEFAULT_PATTERN_DATA);
        }
        4 => {
            let n = c.u();
            out.push(n as f64);
            for _ in 0..n {
                let s = c.string();
                out.push(if is_trail_effect(&s) { 1.0 } else { 0.0 });
            }
        }
        5 => {
            let n = c.u();
            out.push(n as f64);
            for _ in 0..n {
                let s = c.string();
                out.push(if is_nuke_explosion_effect(&s) { 1.0 } else { 0.0 });
            }
        }
        6 => {
            let n = c.u();
            out.push(n as f64);
            for _ in 0..n {
                let s = c.string();
                match effect_type_for_slot(&s) {
                    Some(t) => {
                        out.push(1.0);
                        push_string(&mut out, t);
                    }
                    None => out.push(0.0),
                }
            }
        }
        7 => {
            let n = c.u();
            out.push(n as f64);
            for _ in 0..n {
                let et = c.string();
                let np = c.u();
                let nt = if np == 1 { Some(c.string()) } else { None };
                let slot = c.string();
                out.push(if effect_matches_slot(&et, nt.as_deref(), &slot) { 1.0 } else { 0.0 });
            }
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_type_membership() {
        assert!(is_trail_effect("transportShipTrail"));
        assert!(is_trail_effect("nukeTrail"));
        assert!(!is_trail_effect("nukeExplosion"));
        assert!(!is_trail_effect("structures"));
        assert!(!is_trail_effect(""));
        assert!(is_nuke_explosion_effect("nukeExplosion"));
        assert!(!is_nuke_explosion_effect("nukeTrail"));
    }

    #[test]
    fn slot_resolution() {
        assert_eq!(effect_type_for_slot("atom"), Some("nukeExplosion"));
        assert_eq!(effect_type_for_slot("hydro"), Some("nukeExplosion"));
        assert_eq!(effect_type_for_slot("mirvWarhead"), Some("nukeExplosion"));
        assert_eq!(effect_type_for_slot("train"), Some("train"));
        assert_eq!(effect_type_for_slot("railroad"), Some("railroad"));
        // The bare legacy key resolves to nothing.
        assert_eq!(effect_type_for_slot("nukeExplosion"), None);
        assert_eq!(effect_type_for_slot(""), None);
        assert_eq!(effect_type_for_slot("bogus"), None);
        assert_eq!(effect_type_for_slot("atom "), None);
    }

    #[test]
    fn slot_matching() {
        assert!(effect_matches_slot("nukeTrail", None, "nukeTrail"));
        assert!(effect_matches_slot("nukeExplosion", Some("atom"), "atom"));
        assert!(!effect_matches_slot("nukeExplosion", Some("atom"), "hydro"));
        assert!(!effect_matches_slot("nukeExplosion", Some("atom"), "nukeExplosion"));
        assert!(!effect_matches_slot("transportShipTrail", None, "nukeTrail"));
        assert!(effect_matches_slot("structures", None, "structures"));
        assert!(!effect_matches_slot("train", None, "bogus"));
        // effectType "atom" is not a nukeExplosion effect; forSlot("atom")
        // resolves to "nukeExplosion", which mismatches.
        assert!(!effect_matches_slot("atom", None, "atom"));
    }

    #[test]
    fn run_op_dump_shapes() {
        let e = run_op(0, &[0.0]);
        assert_eq!(e[0], 7.0);
        assert_eq!(e[1], 18.0); // "transportShipTrail" length
        let t = run_op(1, &[0.0]);
        assert_eq!(t[0], 2.0);
        let n = run_op(2, &[0.0]);
        assert_eq!(n[0], 3.0);
        let d = run_op(3, &[0.0]);
        assert_eq!(d, vec![7.0, 100.0, 101.0, 102.0, 97.0, 117.0, 108.0, 116.0, 6.0, 65.0, 65.0, 65.0, 65.0, 65.0, 65.0]);
    }

    fn enc_str(s: &str) -> Vec<f64> {
        let units: Vec<u16> = s.encode_utf16().collect();
        let mut v = vec![units.len() as f64];
        v.extend(units.iter().map(|&u| u as f64));
        v
    }

    #[test]
    fn run_op_batches() {
        let mut args = vec![2.0];
        args.extend(enc_str("nukeTrail"));
        args.extend(enc_str("bogus"));
        let res = run_op(4, &args);
        assert_eq!(res, vec![2.0, 1.0, 0.0]);

        let mut args = vec![2.0];
        args.extend(enc_str("atom"));
        args.extend(enc_str("nukeExplosion"));
        let res = run_op(6, &args);
        assert_eq!(
            res,
            vec![2.0, 1.0, 13.0, 110.0, 117.0, 107.0, 101.0, 69.0, 120.0, 112.0, 108.0, 111.0, 115.0, 105.0, 111.0, 110.0, 0.0]
        );

        let mut args = vec![1.0];
        args.extend(enc_str("nukeExplosion"));
        args.push(1.0);
        args.extend(enc_str("atom"));
        args.extend(enc_str("atom"));
        let res = run_op(7, &args);
        assert_eq!(res, vec![1.0, 1.0]);
    }
}
