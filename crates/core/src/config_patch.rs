//! Port of `src/server/ConfigPatch.ts`: `applyGameConfigPatch(target, patch)`
//! and `hostCheatsEnabled(hc)`. Both ride plain JS data objects (no zod
//! validation is involved), so the harness models `GameConfig` as an
//! insertion-ordered string-keyed field list with the absent / `undefined` /
//! `null` / value tri-state ([`crate::js_json::JsVal`]).
//!
//! Faithfulness notes:
//!
//! * `copy` (COPIED_KEYS loop) writes `target[key] = patch[key]` only when
//!   `patch[key] !== undefined`: an absent key and a present-`undefined` key
//!   both skip, but a `null` patch value COPIES through as `null`.
//! * `copyNullable` (NULLABLE_KEYS loop) keeps the `!== undefined` gate but
//!   writes `value ?? undefined`: a `null` in the patch CLEARS the target
//!   field to `undefined` (the key still exists on the target afterwards —
//!   JS assignment creates it), any other value copies through.
//! * `target.hostCheats = patch.hostCheats` is UNCONDITIONAL: an absent patch
//!   key makes the target gain a `hostCheats: undefined` field (present in
//!   `Object.keys`, visible in the dump as `[1]` undefined).
//! * `target[key] = ...` on an existing key overwrites in place (position
//!   survives); a new key is appended — JS object insertion order.
//! * `hostCheatsEnabled`: `hc !== undefined` first, then the four-field
//!   truth table. `typeof goldMultiplier === "number"` is true for NaN; a
//!   string multiplier is false. The dump reads the fields with `?.`-style
//!   absent-field access (an absent field reads `undefined`).

use crate::js_json::{map_set, read_map, read_val, val_field, JsVal};

/// `COPIED_KEYS` — the TS declaration order (the loop order is observable
/// through the dump's key insertion order when several keys are new).
const COPIED_KEYS: &[&str] = &[
    "gameMap",
    "gameMapSize",
    "difficulty",
    "nations",
    "bots",
    "infiniteGold",
    "donateGold",
    "infiniteTroops",
    "donateTroops",
    "instantBuild",
    "randomSpawn",
    "gameMode",
    "disabledUnits",
    "playerTeams",
    "allowedPublicIds",
    "trusted",
    "doomsdayClock",
    "overtime",
    "anonymizeNames",
    "nameReveals",
    "nameRevealPublicIds",
];

/// `NULLABLE_KEYS` — the TS declaration order.
const NULLABLE_KEYS: &[&str] = &[
    "maxTimerValue",
    "startDelay",
    "spawnImmunityDuration",
    "goldMultiplier",
    "startingGold",
    "disableAlliances",
    "customAllianceDuration",
    "waterNukes",
];

/// `patch[key] !== undefined` — an absent key reads `undefined` in JS, so
/// `None` and `Some(Undef)` gate alike.
fn patch_get<'a>(patch: &'a [(String, JsVal)], key: &str) -> Option<&'a JsVal> {
    patch.iter().find(|(k, _)| k == key).map(|(_, v)| v).filter(|v| **v != JsVal::Undef)
}

/// `applyGameConfigPatch(target, patch)` — in place.
pub(crate) fn apply_game_config_patch(
    target: &mut Vec<(String, JsVal)>,
    patch: &[(String, JsVal)],
) {
    for key in COPIED_KEYS {
        if let Some(v) = patch_get(patch, key) {
            map_set(target, key, v.clone());
        }
    }
    for key in NULLABLE_KEYS {
        if let Some(v) = patch_get(patch, key) {
            // `value ?? undefined`: null clears, everything else rides.
            let w = match v {
                JsVal::Null => JsVal::Undef,
                other => other.clone(),
            };
            map_set(target, key, w);
        }
    }
    // Unconditional on purpose: absent / undefined patch.hostCheats CLEARS
    // (the key is created with value `undefined`).
    let hc = patch
        .iter()
        .find(|(k, _)| k == "hostCheats")
        .map(|(_, v)| v.clone())
        .unwrap_or(JsVal::Undef);
    map_set(target, "hostCheats", hc);
}

/// `hostCheatsEnabled(hc)` — `hc` is the field read off a config (an absent
/// field is `None`, treated as `undefined`).
pub(crate) fn host_cheats_enabled(hc: Option<&JsVal>) -> bool {
    let Some(hc) = hc else { return false };
    if hc == &JsVal::Undef {
        return false;
    }
    let field = |k: &str| val_field(hc, k);
    let is_true = |k: &str| matches!(field(k), Some(JsVal::Bool(true)));
    let is_num = |k: &str| matches!(field(k), Some(JsVal::Num(_)));
    is_true("infiniteGold") || is_true("infiniteTroops") || is_num("goldMultiplier") || is_num("startingGold")
}

/// The capture harness: the live `target` config plus the op stream.
#[derive(Debug, Default)]
pub struct RigHarness {
    target: Vec<(String, JsVal)>,
}

impl RigHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Run one op. Kind table (see `tools/gen_vectors.mjs`):
    /// 0 construct `[n,(key,value)*n]` -> `[0]` (fresh target);
    /// 1 applyGameConfigPatch `[n,(key,value)*n]` -> `[0]`;
    /// 2 dump target -> `[n,(key,value)*n]`;
    /// 3 hostCheatsEnabled `[value]` -> `[0|1]`.
    /// Values cross in the `js_json` codec form (`[0]` absent, `[1]`
    /// undefined, `[2]` null, `[3,v]` number, `[4,b]` bool, `[5,len,u..]`
    /// string, `[6,n,(key,value)*n]` object, `[7,n,(value)*n]` array).
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut i = 0usize;
        match kind {
            0 => {
                self.reset();
                self.target = read_map(args, &mut i);
                vec![0.0]
            }
            1 => {
                let patch = read_map(args, &mut i);
                apply_game_config_patch(&mut self.target, &patch);
                vec![0.0]
            }
            2 => {
                let mut out = Vec::new();
                crate::js_json::push_map(&mut out, &self.target);
                out
            }
            3 => {
                let v = read_val(args, &mut i);
                let hc = if v == JsVal::Absent { None } else { Some(&v) };
                vec![if host_cheats_enabled(hc) { 1.0 } else { 0.0 }]
            }
            k => unreachable!("config_patch harness: unknown op kind {k}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kv(k: &str, v: JsVal) -> (String, JsVal) {
        (k.to_string(), v)
    }

    #[test]
    fn copy_skips_absent_and_undefined() {
        let mut t = vec![kv("gameMap", JsVal::Str("g".into()))];
        let patch = vec![
            kv("difficulty", JsVal::Undef), // present-but-undefined: no overwrite
            kv("bots", JsVal::Null),        // null copies through
        ];
        apply_game_config_patch(&mut t, &patch);
        assert_eq!(t[0].1, JsVal::Str("g".into()));
        assert_eq!(t.iter().find(|(k, _)| k == "difficulty"), None);
        assert_eq!(t.iter().find(|(k, _)| k == "bots").unwrap().1, JsVal::Null);
    }

    #[test]
    fn nullable_null_clears_to_undefined() {
        let mut t = vec![kv("goldMultiplier", JsVal::Num(3.0))];
        let patch = vec![kv("goldMultiplier", JsVal::Null)];
        apply_game_config_patch(&mut t, &patch);
        // Key survives in place, value becomes undefined.
        assert_eq!(t[0], kv("goldMultiplier", JsVal::Undef));
    }

    #[test]
    fn host_cheats_unconditional_creates_key() {
        let mut t: Vec<(String, JsVal)> = vec![];
        apply_game_config_patch(&mut t, &[]);
        assert_eq!(t, vec![kv("hostCheats", JsVal::Undef)]);
    }

    #[test]
    fn host_cheats_enabled_truth_table() {
        assert!(!host_cheats_enabled(None));
        let undef = JsVal::Undef;
        assert!(!host_cheats_enabled(Some(&undef)));
        let empty = JsVal::Obj(vec![]);
        assert!(!host_cheats_enabled(Some(&empty)));
        let ig = JsVal::Obj(vec![kv("infiniteGold", JsVal::Bool(true))]);
        assert!(host_cheats_enabled(Some(&ig)));
        let gm_num = JsVal::Obj(vec![kv("goldMultiplier", JsVal::Num(2.0))]);
        assert!(host_cheats_enabled(Some(&gm_num)));
        let gm_nan = JsVal::Obj(vec![kv("goldMultiplier", JsVal::Num(f64::NAN))]);
        assert!(host_cheats_enabled(Some(&gm_nan)));
        let gm_str = JsVal::Obj(vec![kv("goldMultiplier", JsVal::Str("2".into()))]);
        assert!(!host_cheats_enabled(Some(&gm_str)));
        let sg = JsVal::Obj(vec![kv("startingGold", JsVal::Num(100.0))]);
        assert!(host_cheats_enabled(Some(&sg)));
        let ig_false = JsVal::Obj(vec![kv("infiniteGold", JsVal::Bool(false))]);
        assert!(!host_cheats_enabled(Some(&ig_false)));
    }
}
