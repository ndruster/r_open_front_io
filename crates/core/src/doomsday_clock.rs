//! Port of `src/core/game/DoomsdayClock.ts`.
//!
//! Wave-schedule threshold math for the doomsday clock (required share, HUD
//! wave state, troop floor / drain curves, rot noise and rot quota). The TS
//! module has zero imports and is pure integer/f64 math, so the port is a
//! direct transcription.
//!
//! Faithfulness notes:
//!
//! * All schedule fields are integer constants, but the public entry points
//!   accept arbitrary JS numbers, so every comparison and arithmetic step is
//!   kept in `f64` evaluation order (`(target - prev) * t / ramp` floors the
//!   *product-then-quotient*, never an integer division).
//! * `Math.floor` / `Math.ceil` map onto Rust `f64::floor` / `f64::ceil`,
//!   which agree with V8 on `NaN`, `±0` and `±Infinity`.
//! * `Math.max` needs `js_max` (NaN-propagating, `-0` rule) — Rust's
//!   `f64::max` differs.
//! * The rot noises are pure int32 bit math: `Math.imul` is
//!   `i32::wrapping_mul` (with `ToInt32` on the constants — `R2_X`, `R2_Y`
//!   and the golden-ratio constant exceed `INT32_MAX` and wrap negative),
//!   `>>> 0` is [`to_uint32`], and `^` operates on `i32` after `ToInt32`.
//!   `h >>> 15` / `h >>> 13` / `h >>> 16` are unsigned shifts of the int32.
//! * `(h >>> 16) % ROT_NOISE_SCALE` keeps the JS `%` even though the shifted
//!   value is already `< 2^16` — transcription, not simplification.
//! * `SCHEDULES[profile.speed] ?? SCHEDULES.normal`: an unknown speed code
//!   falls back to `normal`. The TS prototype-chain edge (`speed: "toString"`
//!   resolving to a function and then crashing on `.levels`) is out of scope
//!   — the enum model treats any non-preset code as `normal`, matching the
//!   `??` for plain unknown strings.
//! * `drainCurveFraction`'s loop counter is kept in `f64` (`i < exponent`)
//!   so a fractional exponent iterates exactly as the JS `for` loop does;
//!   `NaN` iterates zero times, which `curveExponent <= 1` already routes
//!   to the convex branch (mirroring JS's `NaN <= 1 === false`).

use crate::game_map::js_max;
use crate::jsnum::{to_int32, to_uint32};

/// `DoomsdayClockSpeed` — the four presets, in selector order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockSpeed {
    Slow,
    Normal,
    Fast,
    VeryFast,
}

/// `DoomsdayClockProfile`. `team` is `profile.teamGame === true` (only the
/// literal `true` selects the team ladder).
#[derive(Clone, Copy, Debug)]
pub struct Profile {
    pub speed: ClockSpeed,
    pub team: bool,
}

/// Decode a run_op speed token: 0 slow, 1 normal, 2 fast, 3 veryfast; any
/// other value (including `NaN`) models the `?? SCHEDULES.normal` fallback.
pub fn speed_from_code(code: f64) -> ClockSpeed {
    if code == 0.0 {
        ClockSpeed::Slow
    } else if code == 1.0 {
        ClockSpeed::Normal
    } else if code == 2.0 {
        ClockSpeed::Fast
    } else if code == 3.0 {
        ClockSpeed::VeryFast
    } else {
        ClockSpeed::Normal
    }
}

const LEVELS: [f64; 7] = [200.0, 400.0, 700.0, 1100.0, 1700.0, 2500.0, 3500.0];
const LEVELS_TEAM: [f64; 7] = [300.0, 600.0, 1000.0, 1500.0, 2100.0, 2800.0, 3500.0];

/// `WaveSchedule` — the seven ramps/pauses/levels are fixed-length ladders.
struct WaveSchedule {
    grace_seconds: f64,
    ramp_seconds: [f64; 7],
    pause_seconds: [f64; 7],
    levels: &'static [f64; 7],
}

/// `schedule(profile)` — the per-speed table, with the team ladder swapped
/// in when `teamGame === true`.
fn schedule(profile: &Profile) -> WaveSchedule {
    let (grace, ramp, pause) = match profile.speed {
        ClockSpeed::Normal => (600.0, [168.0; 7], [54.0, 54.0, 54.0, 54.0, 54.0, 54.0, 0.0]),
        ClockSpeed::Slow => (600.0, [240.0; 7], [70.0, 70.0, 70.0, 70.0, 70.0, 70.0, 0.0]),
        ClockSpeed::Fast => (600.0, [102.0; 7], [31.0, 31.0, 31.0, 31.0, 31.0, 31.0, 0.0]),
        ClockSpeed::VeryFast => (600.0, [36.0; 7], [8.0, 8.0, 8.0, 8.0, 8.0, 8.0, 0.0]),
    };
    let levels = if profile.team { &LEVELS_TEAM } else { &LEVELS };
    WaveSchedule {
        grace_seconds: grace,
        ramp_seconds: ramp,
        pause_seconds: pause,
        levels,
    }
}

/// `requiredBasisPoints(profile, elapsed)` — module-private in TS.
fn required_basis_points(profile: &Profile, elapsed: f64) -> f64 {
    let s = schedule(profile);
    if elapsed <= s.grace_seconds {
        return 0.0;
    }
    let mut t = elapsed - s.grace_seconds;
    let mut prev = 0.0;
    for i in 0..s.levels.len() {
        let ramp = s.ramp_seconds[i];
        let target = s.levels[i];
        if t < ramp {
            return prev + ((target - prev) * t / ramp).floor(); // ramping
        }
        t -= ramp;
        if t < s.pause_seconds[i] {
            return target; // in the pause: hold
        }
        t -= s.pause_seconds[i];
        prev = target;
    }
    s.levels[s.levels.len() - 1]
}

/// `doomsdayClockRequiredTiles(profile, land, elapsed)`.
pub fn required_tiles(profile: &Profile, land: f64, elapsed: f64) -> f64 {
    if land <= 0.0 {
        return 0.0;
    }
    ((required_basis_points(profile, elapsed) * land) / 10000.0).floor()
}

/// `DoomsdayClockWaveState` — the HUD readout.
#[derive(Clone, Copy, Debug)]
pub struct WaveState {
    pub current_percent: f64,
    pub target_percent: f64,
    pub growing: bool,
    pub seconds_to_next_growth: f64,
    pub seconds_to_target: f64,
    pub wave_flash: bool,
    pub done: bool,
}

/// `doomsdayClockWaveState(profile, elapsed)`.
pub fn wave_state(profile: &Profile, elapsed: f64) -> WaveState {
    let s = schedule(profile);
    let current_percent = required_basis_points(profile, elapsed) / 100.0;
    let n = s.levels.len();
    let last = s.levels[n - 1] / 100.0;

    // Grace: flat 0; the first ramp starts at graceSeconds.
    if elapsed <= s.grace_seconds {
        return WaveState {
            current_percent: 0.0,
            target_percent: s.levels[0] / 100.0,
            growing: false,
            seconds_to_next_growth: s.grace_seconds - elapsed,
            seconds_to_target: 0.0,
            wave_flash: s.grace_seconds - elapsed <= 5.0,
            done: false,
        };
    }

    // Walk the per-wave ramp/pause segments to locate the current wave.
    let mut t = elapsed - s.grace_seconds;
    for i in 0..n {
        let ramp = s.ramp_seconds[i];
        let pause = s.pause_seconds[i];
        let is_last = i == n - 1;
        if t < ramp {
            return WaveState {
                current_percent,
                target_percent: s.levels[i] / 100.0,
                growing: true,
                seconds_to_next_growth: 0.0,
                seconds_to_target: ramp - t,
                wave_flash: t <= 5.0,
                done: false,
            };
        }
        t -= ramp;
        if t < pause {
            return WaveState {
                current_percent,
                target_percent: (if is_last { s.levels[i] } else { s.levels[i + 1] }) / 100.0,
                growing: false,
                seconds_to_next_growth: if is_last { 0.0 } else { pause - t },
                seconds_to_target: 0.0,
                wave_flash: !is_last && pause - t <= 5.0,
                done: is_last,
            };
        }
        t -= pause;
    }
    WaveState {
        current_percent,
        target_percent: last,
        growing: false,
        seconds_to_next_growth: 0.0,
        seconds_to_target: 0.0,
        wave_flash: false,
        done: true,
    }
}

/// `ROT_NOISE_SCALE` — upper bound (exclusive) of both rot noise fields.
pub const ROT_NOISE_SCALE: u32 = 1 << 16;

// R2 low-discrepancy lattice constants at 2^32. R2_X / R2_Y exceed INT32_MAX
// and wrap negative under ToInt32, exactly as Math.imul coerces them.
const R2_X: i32 = 3_242_174_889u32 as i32;
const R2_Y: i32 = 2_447_445_413u32 as i32;
const GOLDEN: i32 = 0x9e37_79b9u32 as i32;

/// `rotSpeckleNoise(x, y, salt)` — even blue-noise lattice value.
pub fn rot_speckle_noise(x: f64, y: f64, salt: f64) -> f64 {
    let a = to_int32(x).wrapping_mul(R2_X);
    let b = to_int32(y).wrapping_mul(R2_Y);
    let c = to_int32(salt).wrapping_mul(GOLDEN);
    // JS adds the three int32 results as f64 numbers, then `>>> 0`.
    let sum = (a as i64) + (b as i64) + (c as i64);
    let h = to_uint32(sum as f64);
    ((h >> 16) % ROT_NOISE_SCALE) as f64
}

/// `rotFrontNoise(tile, salt)` — hashed per-tile value for the rot front.
pub fn rot_front_noise(tile: f64, salt: f64) -> f64 {
    let mut h = to_int32(tile).wrapping_mul(0x27d4_eb2d) ^ to_int32(salt).wrapping_mul(GOLDEN);
    h ^= (h as u32 >> 15) as i32;
    h = h.wrapping_mul(0x2545_f491);
    h ^= (h as u32 >> 13) as i32;
    ((h as u32 >> 16) % ROT_NOISE_SCALE) as f64
}

/// `DoomsdayClockFloorConfig`.
#[derive(Clone, Copy, Debug)]
pub struct FloorConfig {
    pub drain_floor_percent: f64,
    pub floor_start_percent: f64,
    pub floor_decay_seconds: f64,
}

/// `doomsdayClockTroopFloor(maxTroops, secondsPastWarn, cfg)`.
pub fn troop_floor(max_troops: f64, seconds_past_warn: f64, cfg: &FloorConfig) -> f64 {
    let end = cfg.drain_floor_percent;
    let span = cfg.floor_start_percent - end;
    let t = js_max(0.0, seconds_past_warn);
    let pct = if span > 0.0 && cfg.floor_decay_seconds > 0.0 && t < cfg.floor_decay_seconds {
        cfg.floor_start_percent - ((span * t) / cfg.floor_decay_seconds).floor()
    } else {
        end
    };
    ((max_troops * pct) / 100.0).floor()
}

/// `DoomsdayClockDrainConfig`.
#[derive(Clone, Copy, Debug)]
pub struct DrainConfig {
    pub drain_start_percent: f64,
    pub drain_max_percent: f64,
    pub drain_ramp_seconds: f64,
}

// Fixed-point scale for the convex drain curve.
const DRAIN_CURVE_SCALE: f64 = 1_000_000.0;

/// `drainCurveFraction(t, r, exponent)` — `(t/r)^exponent` in fixed point.
/// The loop counter stays `f64` so a fractional exponent iterates like the
/// JS `for` loop.
fn drain_curve_fraction(t: f64, r: f64, exponent: f64) -> f64 {
    let ratio = ((t * DRAIN_CURVE_SCALE) / r).floor();
    let mut acc = DRAIN_CURVE_SCALE;
    let mut i = 0.0f64;
    while i < exponent {
        acc = ((acc * ratio) / DRAIN_CURVE_SCALE).floor();
        i += 1.0;
    }
    acc
}

/// `doomsdayClockDrain(maxTroops, secondsPastWarn, cfg, curveExponent)`.
pub fn drain(
    max_troops: f64,
    seconds_past_warn: f64,
    cfg: &DrainConfig,
    curve_exponent: f64,
) -> f64 {
    let t = js_max(0.0, seconds_past_warn);
    let r = cfg.drain_ramp_seconds;
    let span = cfg.drain_max_percent - cfg.drain_start_percent;
    let mut pct = cfg.drain_max_percent;
    if r > 0.0 && t < r {
        let grown = if curve_exponent <= 1.0 {
            ((span * t) / r).floor()
        } else {
            ((span * drain_curve_fraction(t, r, curve_exponent)) / DRAIN_CURVE_SCALE).floor()
        };
        pct = cfg.drain_start_percent + grown;
    }
    js_max(1.0, ((max_troops * pct) / 100.0).floor())
}

/// `doomsdayClockRotQuota(tilesLeft, secondsUnder, rotDeathSeconds)`.
pub fn rot_quota(tiles_left: f64, seconds_under: f64, rot_death_seconds: f64) -> f64 {
    if tiles_left <= 0.0 || rot_death_seconds <= 0.0 {
        return 0.0;
    }
    let seconds_left = js_max(1.0, rot_death_seconds - seconds_under);
    (tiles_left / seconds_left).ceil()
}

// ---------------------------------------------------------------- vectors op
//
// Flat `f64` token runner shared by the golden replay and the wasm probe.
// Profile tokens: speed code (0 slow / 1 normal / 2 fast / 3 veryfast, other
// = normal fallback), team code (1 = `teamGame === true`, other = false).
// Booleans in results are 0/1.
//
// * kind 0: `requiredTiles(speed, team, land, elapsed)` -> `[tiles]`
// * kind 1: `waveState(speed, team, elapsed)` ->
//   `[currentPercent, targetPercent, growing, secondsToNextGrowth,
//     secondsToTarget, waveFlash, done]`
// * kind 2: `rotSpeckleNoise(x, y, salt)` -> `[noise]`
// * kind 3: `rotFrontNoise(tile, salt)` -> `[noise]`
// * kind 4: `troopFloor(max, spw, drainFloor, start, decay)` -> `[floor]`
// * kind 5: `drain(max, spw, start, max, ramp, exponent)` -> `[drain]`
// * kind 6: `rotQuota(tilesLeft, secondsUnder, rotDeath)` -> `[quota]`

/// One `run_op` call; see the module header for the kind/token table.
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    match kind {
        0 => {
            let p = Profile {
                speed: speed_from_code(args[0]),
                team: args[1] == 1.0,
            };
            out.push(required_tiles(&p, args[2], args[3]));
        }
        1 => {
            let p = Profile {
                speed: speed_from_code(args[0]),
                team: args[1] == 1.0,
            };
            let w = wave_state(&p, args[2]);
            out.push(w.current_percent);
            out.push(w.target_percent);
            out.push(f64::from(w.growing));
            out.push(w.seconds_to_next_growth);
            out.push(w.seconds_to_target);
            out.push(f64::from(w.wave_flash));
            out.push(f64::from(w.done));
        }
        2 => out.push(rot_speckle_noise(args[0], args[1], args[2])),
        3 => out.push(rot_front_noise(args[0], args[1])),
        4 => {
            let cfg = FloorConfig {
                drain_floor_percent: args[2],
                floor_start_percent: args[3],
                floor_decay_seconds: args[4],
            };
            out.push(troop_floor(args[0], args[1], &cfg));
        }
        5 => {
            let cfg = DrainConfig {
                drain_start_percent: args[2],
                drain_max_percent: args[3],
                drain_ramp_seconds: args[4],
            };
            out.push(drain(args[0], args[1], &cfg, args[5]));
        }
        _ => out.push(rot_quota(args[0], args[1], args[2])),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prof(speed: ClockSpeed, team: bool) -> Profile {
        Profile { speed, team }
    }

    #[test]
    fn required_tiles_grace_and_first_ramp() {
        let p = prof(ClockSpeed::Normal, false);
        assert_eq!(required_tiles(&p, 10000.0, 0.0), 0.0);
        assert_eq!(required_tiles(&p, 10000.0, 600.0), 0.0);
        // elapsed 601: t=1 of ramp 168 -> floor(200*1/168) = 1 bp -> 1 tile.
        assert_eq!(required_tiles(&p, 10000.0, 601.0), 1.0);
        // land <= 0 short-circuits before any schedule math.
        assert_eq!(required_tiles(&p, 0.0, 1e9), 0.0);
        assert_eq!(required_tiles(&p, -5.0, 1e9), 0.0);
        // Past the last wave: 35% of 10000 = 3500.
        assert_eq!(required_tiles(&p, 10000.0, 1e9), 3500.0);
        // NaN elapsed: every comparison false -> walks out at the last level.
        assert_eq!(required_tiles(&p, 10000.0, f64::NAN), 3500.0);
    }

    #[test]
    fn team_ladder_and_speed_fallback() {
        let team = prof(ClockSpeed::Normal, true);
        // Team final level is also 3500 bp, but wave 1 targets 300.
        assert_eq!(required_basis_points(&team, 600.0 + 168.0), 300.0);
        let ffa = prof(ClockSpeed::Normal, false);
        assert_eq!(required_basis_points(&ffa, 600.0 + 168.0), 200.0);
        // Unknown speed code behaves like normal (the `??` fallback).
        let weird = prof(speed_from_code(7.0), false);
        assert_eq!(required_basis_points(&weird, 600.0 + 168.0), 200.0);
    }

    #[test]
    fn wave_state_branches() {
        let p = prof(ClockSpeed::VeryFast, false);
        // Grace tail: 5s before the first ramp -> flash cue on.
        let w = wave_state(&p, 596.0);
        assert_eq!(w.seconds_to_next_growth, 4.0);
        assert!(w.wave_flash);
        assert!(!w.done);
        // Mid-ramp: growing, target = this wave's level.
        let w = wave_state(&p, 600.0 + 18.0);
        assert!(w.growing);
        assert_eq!(w.seconds_to_target, 18.0);
        assert_eq!(w.target_percent, 2.0);
        // Wave 6's pause (not last): holds, next ramp imminent.
        // 600 + 5*44 + 36 + 2 = 858: t=2 into the 8s pause before the last ramp.
        let w = wave_state(&p, 858.0);
        assert!(!w.growing);
        assert!(!w.done);
        assert_eq!(w.seconds_to_next_growth, 6.0);
        assert!(!w.wave_flash);
        assert_eq!(w.target_percent, 35.0);
        // Past every segment (600 + 7*36 + 6*8 = 900): the fall-through.
        let w = wave_state(&p, 900.0);
        assert!(w.done);
        assert_eq!(w.seconds_to_next_growth, 0.0);
        // Beyond everything: flat at the ceiling.
        let w = wave_state(&p, 1e9);
        assert!(w.done);
        assert_eq!(w.current_percent, 35.0);
        assert_eq!(w.target_percent, 35.0);
    }

    #[test]
    fn speckle_noise_pinned_values() {
        // (1,0,0): imul(1, R2_X) = -1052792407 -> >>>0 = 3242174889 -> >>16 = 49471.
        assert_eq!(rot_speckle_noise(1.0, 0.0, 0.0), 49471.0);
        // (0,0,0) = 0; the lattice is linear so (2,0,0) = 2*imul(1,..) mod 2^32.
        assert_eq!(rot_speckle_noise(0.0, 0.0, 0.0), 0.0);
        // ToInt32 coercion: 2^32 reads as 0, 2^31 reads as INT32_MIN.
        assert_eq!(rot_speckle_noise(4294967296.0, 0.0, 0.0), 0.0);
        assert_eq!(
            rot_speckle_noise(2147483648.0, 0.0, 0.0),
            rot_speckle_noise(-2147483648.0, 0.0, 0.0)
        );
        // NaN coords coerce to 0.
        assert_eq!(rot_speckle_noise(f64::NAN, 0.0, 0.0), 0.0);
    }

    #[test]
    fn front_noise_pinned_and_range() {
        assert_eq!(rot_front_noise(0.0, 0.0), 0.0);
        // Both noises stay in [0, 65536).
        for v in [1.0, -1.0, 1e9, 0.5, f64::NAN] {
            let s = rot_speckle_noise(v, v, v);
            assert!((0.0..65536.0).contains(&s), "speckle {v} -> {s}");
            let f = rot_front_noise(v, v);
            assert!((0.0..65536.0).contains(&f), "front {v} -> {f}");
        }
    }

    #[test]
    fn troop_floor_linear_decay() {
        let cfg = FloorConfig {
            drain_floor_percent: 10.0,
            floor_start_percent: 50.0,
            floor_decay_seconds: 100.0,
        };
        // t=50: pct = 50 - floor(40*50/100) = 30 -> floor(1000*30/100) = 300.
        assert_eq!(troop_floor(1000.0, 50.0, &cfg), 300.0);
        // Negative t clamps to 0 -> full start floor.
        assert_eq!(troop_floor(1000.0, -5.0, &cfg), 500.0);
        // Past the decay -> settled floor.
        assert_eq!(troop_floor(1000.0, 1e9, &cfg), 100.0);
        // NaN t: Math.max(0, NaN) = NaN -> comparisons false -> end floor.
        assert_eq!(troop_floor(1000.0, f64::NAN, &cfg), 100.0);
        // span == 0 short-circuits to end even mid-decay.
        let flat = FloorConfig {
            drain_floor_percent: 50.0,
            floor_start_percent: 50.0,
            floor_decay_seconds: 100.0,
        };
        assert_eq!(troop_floor(1000.0, 50.0, &flat), 500.0);
    }

    #[test]
    fn drain_linear_vs_convex_and_floor() {
        let cfg = DrainConfig {
            drain_start_percent: 1.0,
            drain_max_percent: 11.0,
            drain_ramp_seconds: 100.0,
        };
        // Linear (exp 1): t=50 -> grown = floor(10*50/100) = 5 -> pct 6 -> 60.
        assert_eq!(drain(1000.0, 50.0, &cfg, 1.0), 60.0);
        // Convex (exp 2): ratio = 500000, acc = 250000 -> grown = floor(10*0.25) = 2 -> 30.
        assert_eq!(drain(1000.0, 50.0, &cfg, 2.0), 30.0);
        // Fractional exponent iterates floor-like: exp 2.5 -> 3 steps (i=0,1,2).
        assert_eq!(drain(1000.0, 50.0, &cfg, 2.5), drain(1000.0, 50.0, &cfg, 3.0));
        // NaN exponent: `NaN <= 1` is false -> convex branch, loop runs 0 times
        // -> acc = SCALE -> grown = floor(span) = 10 -> pct 11 -> 110.
        assert_eq!(drain(1000.0, 50.0, &cfg, f64::NAN), 110.0);
        // Past the ramp: max pct, floored, and at least 1.
        assert_eq!(drain(1000.0, 1e9, &cfg, 1.0), 110.0);
        assert_eq!(drain(0.0, 1e9, &cfg, 1.0), 1.0);
        // ramp <= 0 skips the ramp entirely.
        let off = DrainConfig {
            drain_start_percent: 1.0,
            drain_max_percent: 11.0,
            drain_ramp_seconds: 0.0,
        };
        assert_eq!(drain(1000.0, 50.0, &off, 1.0), 110.0);
    }

    #[test]
    fn rot_quota_ceil_and_clamps() {
        // ceil(100 / max(1, 10-5)) = 20.
        assert_eq!(rot_quota(100.0, 5.0, 10.0), 20.0);
        // secondsLeft clamps to 1 once past the deadline.
        assert_eq!(rot_quota(100.0, 1e9, 10.0), 100.0);
        assert_eq!(rot_quota(0.0, 0.0, 10.0), 0.0);
        assert_eq!(rot_quota(100.0, 0.0, 0.0), 0.0);
        // NaN tilesLeft: `<= 0` false -> ceil(NaN) = NaN.
        assert!(rot_quota(f64::NAN, 0.0, 10.0).is_nan());
    }
}
