//! Port of `src/client/utilities/ReplaySpeedMultiplier.ts` — the replay
//! speed table. The enum values are the SECONDS-PER-TICK multipliers, not
//! speed factors: `slow = 2`, `normal = 1`, `fast = 0.5`, `fastest = 0`
//! (0 freezes the sim). `defaultReplaySpeedMultiplier` is `normal` (1).
//!
//! NOTE: the upstream `export enum` is inlined as a plain object for the
//! Node strip-loader (precedent: GameMap `TerrainType`, MotionPlans
//! `PackedMotionPlanKind`), so the capture dumps the forward name→value
//! table (declaration order) rather than the reverse mapping a real TS
//! numeric enum would carry.

use crate::js_json::push_str;

/// `ReplaySpeedMultiplier.slow` — 2 seconds per tick.
pub const SLOW: f64 = 2.0;
/// `ReplaySpeedMultiplier.normal` — 1 second per tick.
pub const NORMAL: f64 = 1.0;
/// `ReplaySpeedMultiplier.fast` — 0.5 seconds per tick.
pub const FAST: f64 = 0.5;
/// `ReplaySpeedMultiplier.fastest` — 0 seconds per tick (frozen clock).
pub const FASTEST: f64 = 0.0;
/// `defaultReplaySpeedMultiplier` — `ReplaySpeedMultiplier.normal`.
pub const DEFAULT_REPLAY_SPEED_MULTIPLIER: f64 = NORMAL;

/// `run_op(kind, args)` — capture harness entry (stateless). Kind table:
/// 0 -> `[4, (name-str, value)*4]` in declaration order
///   (slow 2, normal 1, fast 0.5, fastest 0);
/// 1 -> `[defaultReplaySpeedMultiplier]`.
pub fn run_op(kind: u8, _args: &[f64]) -> Vec<f64> {
    match kind {
        0 => {
            let mut out = vec![4.0];
            for (name, v) in [("slow", SLOW), ("normal", NORMAL), ("fast", FAST), ("fastest", FASTEST)] {
                push_str(&mut out, name);
                out.push(v);
            }
            out
        }
        1 => vec![DEFAULT_REPLAY_SPEED_MULTIPLIER],
        k => unreachable!("replay_speed: unknown op kind {k}"),
    }
}
