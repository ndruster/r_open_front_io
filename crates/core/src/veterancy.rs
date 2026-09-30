//! Port of `src/core/game/Veterancy.ts` — shared warship-veterancy math.
//!
//! The engine and the renderer must derive identical effective max health, so
//! this lives in the determinism layer. The only JS-isms to pin:
//!
//! * `veterancy <= 0` — JS relational comparison with `NaN` is `false`, so a
//!   `NaN` veterancy falls through to the bonus branch (Rust `f64 <= 0.0`
//!   matches: `NaN <= 0.0` is `false`).
//! * `Math.floor` — IEEE-754 correctly-rounded, so Rust's `f64::floor` is
//!   bit-identical to V8's, including `-0` and `NaN` passthrough.
//! * Association order — `baseMaxHealth * veterancy * healthBonusPercent` is
//!   left-associative and the divide is by `100`, not a reciprocal multiply;
//!   reordering changes the last bit.

/// `maxHealthWithVeterancy(baseMaxHealth, veterancy, healthBonusPercent)`.
///
/// Each veterancy level adds `health_bonus_percent`% of `base_max_health`,
/// floored to keep the result deterministic. Returns `base_max_health`
/// unchanged when `veterancy <= 0` (and thus for non-veteran units).
pub fn max_health_with_veterancy(
    base_max_health: f64,
    veterancy: f64,
    health_bonus_percent: f64,
) -> f64 {
    if veterancy <= 0.0 {
        return base_max_health;
    }
    base_max_health + ((base_max_health * veterancy * health_bonus_percent) / 100.0).floor()
}
