//! Port of `src/client/render/gl/utils/NukeTrajectory.ts` — Bezier control
//! points and color thresholds for the nuke trajectory render.
//!
//! Pure math, no game dependencies (the TS header comment says so).
//! Faithfulness notes (quirk list):
//!
//! * `samRange(level)` is all-f64: `150 - 480 / (level + 5)`. `level = -5`
//!   divides by zero — JS f64 division yields `+Infinity`, so the result is
//!   `-Infinity` (never a throw); `level = -6` gives `150 - -480 = 630`.
//! * `clamp` is the ternary chain `v < lo ? lo : v > hi ? hi : v` — a NaN `v`
//!   fails BOTH comparisons and falls through returning NaN (an `f64::clamp`
//!   or min/max chain would swallow it).
//! * `computeNukeControlPoints` uses `Math.sqrt(dx*dx + dy*dy)` — the NAIVE
//!   sqrt, NOT `Math.hypot` (the two differ in the last bit; `js_hypot` must
//!   not be used here). `maxHeight = Math.max(dist/3, 50)` goes through
//!   [`js_max`] (NaN propagates like V8). `p1y`/`p2y` clamp to `[0, mapH-1]`.
//! * `refineCrossing` evaluates the curve through the Horner form
//!   `(((A*t + B)*t + C)*t + D + 0.5) | 0` — the `| 0` is JS ToInt32
//!   (truncate toward zero, NaN/±Infinity → 0; [`to_int32`]), applied AFTER
//!   the `+ 0.5`, so a negative `-0.4` truncates to `0`, not `-1`. Ten
//!   bisection steps; the shrink direction is `exitingRange ? inside :
//!   !inside`; the false-alarm fallback (`!exitingRange && !foundInside` and
//!   the `tHi` point still outside) returns exactly `1.0`.
//! * `computeTrajectoryThresholds` builds the polynomial from the ROUNDED
//!   integer target (`polyAx = dst - 3*p2 + 3*p1 - p0`, dst not p3) and seeds
//!   `prevX/prevY` with `(cp.p0x + 0.5) | 0`. The untargetable-zone gate is
//!   two-phase (exit source range uses `> RANGE_SQ` AND `>= RANGE_SQ`; the
//!   entry phase looks for `< RANGE_SQ`), the boundary recheck block and the
//!   SAM segment-distance block (fast reject, `dot<=0 / dot>=l2 / else`
//!   three-way, `lo` gate preferring `tUntargetableEnd`) run in TS order.
//! * `buildNukeTrajectory` rounds the target with JS `Math.round` (half UP,
//!   [`js_round`]) and merges `{...cpRender, ...th}` — the 11-key result
//!   order p0x..p3y, tUntargetableStart, tUntargetableEnd, tSamIntercept is
//!   pinned by the capture's key dump.

use crate::jsnum::{js_max, js_round, to_int32};

/// `PARABOLA_MIN_HEIGHT`.
pub const PARABOLA_MIN_HEIGHT: f64 = 50.0;
/// `TARGETABLE_RANGE`.
pub const TARGETABLE_RANGE: f64 = 150.0;
/// `TARGETABLE_RANGE_SQ` — computed as `150 * 150` upstream.
pub const TARGETABLE_RANGE_SQ: f64 = 22500.0;
/// `THRESHOLD_SAMPLES`.
pub const THRESHOLD_SAMPLES: usize = 32;
/// `MAX_SAM_RANGE`.
pub const MAX_SAM_RANGE: f64 = 150.0;
/// `SAM_RANGE_DIVISOR`.
pub const SAM_RANGE_DIVISOR: f64 = 480.0;
/// `SAM_RANGE_OFFSET`.
pub const SAM_RANGE_OFFSET: f64 = 5.0;
/// `SAM_SAFETY_MARGIN`.
pub const SAM_SAFETY_MARGIN: f64 = 0.75;

/// `samRange(level)` — `150 - 480 / (level + 5)`, all f64.
pub fn sam_range(level: f64) -> f64 {
    MAX_SAM_RANGE - SAM_RANGE_DIVISOR / (level + SAM_RANGE_OFFSET)
}

/// The module-private `clamp` — the ternary chain, NaN falls through.
fn nt_clamp(v: f64, lo: f64, hi: f64) -> f64 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

/// The eight control-point fields in key order p0x, p0y, p1x, p1y, p2x,
/// p2y, p3x, p3y.
pub type ControlPoints = [f64; 8];

/// `computeNukeControlPoints(srcX, srcY, dstX, dstY, mapH, directionUp)`.
pub fn compute_nuke_control_points(
    src_x: f64,
    src_y: f64,
    dst_x: f64,
    dst_y: f64,
    map_h: f64,
    direction_up: bool,
) -> ControlPoints {
    let dx = dst_x - src_x;
    let dy = dst_y - src_y;
    let dist = (dx * dx + dy * dy).sqrt();
    let max_height = js_max(dist / 3.0, PARABOLA_MIN_HEIGHT);
    let hm = if direction_up { -1.0 } else { 1.0 };

    [
        src_x,
        src_y,
        src_x + dx / 4.0,
        nt_clamp(src_y + dy / 4.0 + hm * max_height, 0.0, map_h - 1.0),
        src_x + (dx * 3.0) / 4.0,
        nt_clamp(src_y + (dy * 3.0) / 4.0 + hm * max_height, 0.0, map_h - 1.0),
        dst_x,
        dst_y,
    ]
}

/// The Horner x-sample `(curve(t) + 0.5) | 0` over `A, B, C, D`.
#[inline]
fn horner_i(a: f64, b: f64, c: f64, d: f64, t: f64) -> f64 {
    to_int32((((a * t + b) * t + c) * t + d) + 0.5) as f64
}

/// `refineCrossing(...)` — ten bisection steps over the squared-distance
/// gate, then the false-alarm fallback. `poly` is `[Ax, Bx, Cx, Dx, Ay, By,
/// Cy, Dy]`.
fn refine_crossing(
    poly: &[f64; 8],
    cx: f64,
    cy: f64,
    range_sq: f64,
    mut t_lo: f64,
    mut t_hi: f64,
    exiting_range: bool,
) -> f64 {
    let mut found_inside = false;

    for _ in 0..10 {
        let t_mid = (t_lo + t_hi) * 0.5;

        let x_mid = horner_i(poly[0], poly[1], poly[2], poly[3], t_mid);
        let y_mid = horner_i(poly[4], poly[5], poly[6], poly[7], t_mid);

        let dx = x_mid - cx;
        let dy = y_mid - cy;
        let inside = dx * dx + dy * dy <= range_sq;
        if inside {
            found_inside = true;
        }

        if if exiting_range { inside } else { !inside } {
            t_lo = t_mid;
        } else {
            t_hi = t_mid;
        }
    }

    // If testing entry and no point on the curve was inside rangeSq, reject
    // chord false-alarm.
    if !exiting_range && !found_inside {
        let x_hi = horner_i(poly[0], poly[1], poly[2], poly[3], t_hi);
        let y_hi = horner_i(poly[4], poly[5], poly[6], poly[7], t_hi);
        let dx_hi = x_hi - cx;
        let dy_hi = y_hi - cy;
        if dx_hi * dx_hi + dy_hi * dy_hi > range_sq {
            return 1.0;
        }
    }

    (t_lo + t_hi) * 0.5
}

/// One `SAMInfo { x, y, r }`.
pub type SamInfo = (f64, f64, f64);

/// `computeTrajectoryThresholds(cp, srcX, srcY, dstX, dstY, sams)` — returns
/// `[tUntargetableStart, tUntargetableEnd, tSamIntercept]`.
pub fn compute_trajectory_thresholds(
    cp: &ControlPoints,
    src_x: f64,
    src_y: f64,
    dst_x: f64,
    dst_y: f64,
    sams: &[SamInfo],
) -> [f64; 3] {
    let mut t_untargetable_start = -1.0;
    let mut t_untargetable_end = -1.0;
    let mut t_sam_intercept = 1.0;

    let dt = 1.0 / THRESHOLD_SAMPLES as f64;

    // dstX / dstY are the ROUNDED integer target (unlike cp.p3x/p3y).
    let poly_cx = 3.0 * (cp[2] - cp[0]);
    let poly_bx = 3.0 * (cp[4] - 2.0 * cp[2] + cp[0]);
    let poly_ax = dst_x - 3.0 * cp[4] + 3.0 * cp[2] - cp[0];
    let poly_dx = cp[0];

    let poly_cy = 3.0 * (cp[3] - cp[1]);
    let poly_by = 3.0 * (cp[5] - 2.0 * cp[3] + cp[1]);
    let poly_ay = dst_y - 3.0 * cp[5] + 3.0 * cp[3] - cp[1];
    let poly_dy = cp[1];

    let poly = [poly_ax, poly_bx, poly_cx, poly_dx, poly_ay, poly_by, poly_cy, poly_dy];

    let src_dst_dx = dst_x - src_x;
    let src_dst_dy = dst_y - src_y;
    let src_dst_dist_sq = src_dst_dx * src_dst_dx + src_dst_dy * src_dst_dy;

    let has_untargetable = src_dst_dist_sq > 4.0 * TARGETABLE_RANGE_SQ;
    let sam_len = sams.len();

    let mut prev_x = to_int32(cp[0] + 0.5) as f64;
    let mut prev_y = to_int32(cp[1] + 0.5) as f64;

    for i in 1..=THRESHOLD_SAMPLES {
        let t = i as f64 * dt;
        let t_prev = t - dt;
        let x = horner_i(poly_ax, poly_bx, poly_cx, poly_dx, t);
        let y = horner_i(poly_ay, poly_by, poly_cy, poly_dy, t);

        let mut is_untargetable_zone = false;

        if has_untargetable {
            if t_untargetable_start < 0.0 {
                // Looking for first point outside source range
                let dx_src = x - src_x;
                let dy_src = y - src_y;
                if dx_src * dx_src + dy_src * dy_src > TARGETABLE_RANGE_SQ {
                    let dx_dst = x - dst_x;
                    let dy_dst = y - dst_y;
                    if dx_dst * dx_dst + dy_dst * dy_dst >= TARGETABLE_RANGE_SQ {
                        t_untargetable_start =
                            refine_crossing(&poly, src_x, src_y, TARGETABLE_RANGE_SQ, t_prev, t, true);
                        is_untargetable_zone = true;
                    }
                }
            } else if t_untargetable_end < 0.0 {
                // Looking for first point inside target range
                let dx_dst = x - dst_x;
                let dy_dst = y - dst_y;
                if dx_dst * dx_dst + dy_dst * dy_dst < TARGETABLE_RANGE_SQ {
                    t_untargetable_end =
                        refine_crossing(&poly, dst_x, dst_y, TARGETABLE_RANGE_SQ, t_prev, t, false);
                } else {
                    is_untargetable_zone = true;
                }
            }
        }

        // Check exact boundary when crossing into the targetable terminal phase
        if t_untargetable_end >= 0.0 && t_prev < t_untargetable_end && t >= t_untargetable_end && sam_len > 0 {
            let xe = horner_i(poly_ax, poly_bx, poly_cx, poly_dx, t_untargetable_end);
            let ye = horner_i(poly_ay, poly_by, poly_cy, poly_dy, t_untargetable_end);
            for &(sx, sy, sr) in sams {
                let dx = xe - sx;
                let dy = ye - sy;
                if dx * dx + dy * dy <= sr * sr {
                    t_sam_intercept = t_untargetable_end;
                    break;
                }
            }
            if t_sam_intercept < 1.0 {
                break;
            }
        }

        if !is_untargetable_zone && sam_len > 0 {
            let seg_dx = x - prev_x;
            let seg_dy = y - prev_y;
            let l2 = seg_dx * seg_dx + seg_dy * seg_dy;
            let inv_l2 = if l2 == 0.0 { 0.0 } else { 1.0 / l2 };
            let max_dist = l2.sqrt() + MAX_SAM_RANGE + SAM_SAFETY_MARGIN;
            let max_d_src_sq = max_dist * max_dist;

            for &(sam_x, sam_y, sam_r) in sams {

                // Fast proximity rejection based on maximum reachable
                // distance of this segment
                let dx_sam = sam_x - prev_x;
                let dy_sam = sam_y - prev_y;
                let d_src_sq = dx_sam * dx_sam + dy_sam * dy_sam;
                if d_src_sq > max_d_src_sq {
                    continue;
                }

                let dot = dx_sam * seg_dx + dy_sam * seg_dy;
                let d_sq = if dot <= 0.0 {
                    d_src_sq
                } else if dot >= l2 {
                    d_src_sq + l2 - 2.0 * dot
                } else {
                    d_src_sq - dot * dot * inv_l2
                };
                let range_sq = sam_r * sam_r;
                // safety margin, since we compare straight lines to arcs
                let candidate_range_sq = (sam_r + SAM_SAFETY_MARGIN) * (sam_r + SAM_SAFETY_MARGIN);
                if d_sq <= candidate_range_sq {
                    let lo = if t_untargetable_end >= 0.0 && t_prev < t_untargetable_end {
                        t_untargetable_end
                    } else {
                        t_prev
                    };
                    let intercept =
                        refine_crossing(&poly, sam_x, sam_y, range_sq, lo, t, false);
                    if intercept < 1.0 {
                        t_sam_intercept = intercept;
                        break;
                    }
                }
            }
            if t_sam_intercept < 1.0 {
                break;
            }
        }

        prev_x = x;
        prev_y = y;
    }

    [t_untargetable_start, t_untargetable_end, t_sam_intercept]
}

/// `buildNukeTrajectory(...)` — the merged `{...cpRender, ...th}` object as
/// the eleven values in key order p0x, p0y, p1x, p1y, p2x, p2y, p3x, p3y,
/// tUntargetableStart, tUntargetableEnd, tSamIntercept.
pub fn build_nuke_trajectory(
    src_x: f64,
    src_y: f64,
    dst_x: f64,
    dst_y: f64,
    map_h: f64,
    direction_up: bool,
    sams: &[SamInfo],
) -> [f64; 11] {
    let cp_render = compute_nuke_control_points(src_x, src_y, dst_x, dst_y, map_h, direction_up);

    let target_x = js_round(dst_x);
    let target_y = js_round(dst_y);

    let th = compute_trajectory_thresholds(&cp_render, src_x, src_y, target_x, target_y, sams);

    let mut out = [0.0; 11];
    out[..8].copy_from_slice(&cp_render);
    out[8..].copy_from_slice(&th);
    out
}

/// `run_op(kind, args)` — capture harness entry (stateless). Kind table (see
/// `tools/gen_vectors.mjs`):
/// 0 samRange `[level]` -> `[r]`;
/// 1 clamp `[v, lo, hi]` -> `[r]` (module-private upstream — the capture
///   pins the NaN-passthrough through the control-point paths instead, but
///   the op is kept for the unit tests);
/// 2 computeNukeControlPoints `[srcX, srcY, dstX, dstY, mapH, dirUp]` -> `[8]`;
/// 3 computeTrajectoryThresholds `[8 cp..., srcX, srcY, dstX, dstY, n,
///   (x, y, r)*n]` -> `[3]`;
/// 4 buildNukeTrajectory `[srcX, srcY, dstX, dstY, mapH, dirUp, n, (x,y,r)*n]`
///   -> `[11]`;
/// 5 buildKeyOrder (same args as 4) -> `[len, (unit)*]` of the joined
///   `Object.keys` — pins the `{...cpRender, ...th}` 11-key order.
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut i = 0usize;
    // Past-the-end reads model the capture's `a[k]` on a short array: JS
    // yields `undefined`, which the SAM radius arithmetic turns into NaN.
    let num = |i: &mut usize| {
        let v = if *i < args.len() { args[*i] } else { f64::NAN };
        *i += 1;
        v
    };
    match kind {
        0 => vec![sam_range(num(&mut i))],
        1 => {
            let v = num(&mut i);
            let lo = num(&mut i);
            let hi = num(&mut i);
            vec![nt_clamp(v, lo, hi)]
        }
        2 => {
            let src_x = num(&mut i);
            let src_y = num(&mut i);
            let dst_x = num(&mut i);
            let dst_y = num(&mut i);
            let map_h = num(&mut i);
            let dir_up = num(&mut i) != 0.0;
            compute_nuke_control_points(src_x, src_y, dst_x, dst_y, map_h, dir_up).to_vec()
        }
        3 => {
            let mut cp = [0.0; 8];
            for c in cp.iter_mut() {
                *c = num(&mut i);
            }
            let src_x = num(&mut i);
            let src_y = num(&mut i);
            let dst_x = num(&mut i);
            let dst_y = num(&mut i);
            // The capture slices args at `12 + 1 + (a[12] | 0) * 3`, so a
            // scenario with no SAMs can end right after the 12th token and
            // `a[12]` reads undefined -> 0 there.
            let n = (if i < args.len() { args[i] } else { 0.0 }) as usize;
            i += 1;
            let sams: Vec<SamInfo> = (0..n).map(|_| (num(&mut i), num(&mut i), num(&mut i))).collect();
            compute_trajectory_thresholds(&cp, src_x, src_y, dst_x, dst_y, &sams).to_vec()
        }
        4 => {
            let src_x = num(&mut i);
            let src_y = num(&mut i);
            let dst_x = num(&mut i);
            let dst_y = num(&mut i);
            let map_h = num(&mut i);
            let dir_up = num(&mut i) != 0.0;
            let n = num(&mut i) as usize;
            let sams: Vec<SamInfo> = (0..n).map(|_| (num(&mut i), num(&mut i), num(&mut i))).collect();
            build_nuke_trajectory(src_x, src_y, dst_x, dst_y, map_h, dir_up, &sams).to_vec()
        }
        5 => {
            // The `{...cpRender, ...th}` key order, joined with commas and
            // crossed as [len, units...] (the TS capture runs Object.keys on
            // the real result object).
            let _ = args.len(); // args mirror kind 4; the keys are fixed
            let s = "p0x,p0y,p1x,p1y,p2x,p2y,p3x,p3y,tUntargetableStart,tUntargetableEnd,tSamIntercept";
            let units: Vec<f64> = s.encode_utf16().map(|u| u as f64).collect();
            let mut out = Vec::with_capacity(1 + units.len());
            out.push(units.len() as f64);
            out.extend(units);
            out
        }
        k => unreachable!("nuke_trajectory: unknown op kind {k}"),
    }
}
