//! Port of `src/client/render/gl/passes/fx-pass/FxSettings.ts` — the pure
//! `nukeExplosionRadius` switch. The unit-type strings are the `UT_*`
//! constants (`Atom Bomb` / `Hydrogen Bomb` / `MIRV Warhead`).
//!
//! Faithfulness notes:
//!
//! * The `switch` compares the raw `unitType` string with `===` semantics
//!   (no coercion); anything else returns `undefined`.
//! * The `fx` field reads are plain property reads: an absent / nullish `fx`
//!   would throw in JS (`fx.nukeRadiusAtom` on null) — outside the typed
//!   domain, so the runner requires an object and models a missing field as
//!   `undefined` ([`JsVal::Undef`]).

use crate::js_json::{push_val, read_val, val_field, JsVal};

/// `UT_ATOM_BOMB`.
pub const UT_ATOM_BOMB: &str = "Atom Bomb";
/// `UT_HYDROGEN_BOMB`.
pub const UT_HYDROGEN_BOMB: &str = "Hydrogen Bomb";
/// `UT_MIRV_WARHEAD`.
pub const UT_MIRV_WARHEAD: &str = "MIRV Warhead";

/// `nukeExplosionRadius(fx, unitType)` — the raw field value or undefined.
pub fn nuke_explosion_radius(fx: &JsVal, unit_type: &str) -> JsVal {
    let key = match unit_type {
        UT_ATOM_BOMB => "nukeRadiusAtom",
        UT_HYDROGEN_BOMB => "nukeRadiusHydro",
        UT_MIRV_WARHEAD => "nukeRadiusMirv",
        _ => return JsVal::Undef,
    };
    val_field(fx, key).cloned().unwrap_or(JsVal::Undef)
}

// ---------------------------------------------------------------- vectors op
//
// kind 0: [...codec fx, ...codec unitType] -> [...codec value]

/// Flat-token runner shared by the golden replay and the wasm probe.
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    match kind {
        0 => {
            let mut i = 0usize;
            let fx = read_val(args, &mut i);
            let JsVal::Str(unit_type) = read_val(args, &mut i) else {
                unreachable!("fx_settings: unitType must be a string");
            };
            push_val(&mut out, &nuke_explosion_radius(&fx, &unit_type));
        }
        k => unreachable!("fx_settings: unknown op kind {k}"),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switch_routes() {
        let fx = JsVal::Obj(vec![
            ("nukeRadiusAtom".to_string(), JsVal::Num(10.0)),
            ("nukeRadiusHydro".to_string(), JsVal::Num(20.0)),
            ("nukeRadiusMirv".to_string(), JsVal::Num(15.0)),
        ]);
        assert_eq!(nuke_explosion_radius(&fx, "Atom Bomb"), JsVal::Num(10.0));
        assert_eq!(nuke_explosion_radius(&fx, "Hydrogen Bomb"), JsVal::Num(20.0));
        assert_eq!(nuke_explosion_radius(&fx, "MIRV Warhead"), JsVal::Num(15.0));
        assert_eq!(nuke_explosion_radius(&fx, "Steam Tank"), JsVal::Undef);
        // Case-sensitive; a missing field reads undefined.
        assert_eq!(nuke_explosion_radius(&fx, "atom bomb"), JsVal::Undef);
        assert_eq!(
            nuke_explosion_radius(&JsVal::Obj(vec![]), "Atom Bomb"),
            JsVal::Undef
        );
    }
}
