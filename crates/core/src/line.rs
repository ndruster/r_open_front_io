//! Port of `src/core/utilities/Line.ts` — the `DistanceBasedBezierCurve`
//! fixed-point missile trajectory used by MIRV warheads.
//!
//! The whole class runs in 8-bit fixed point (`SUB_SCALE = 256`): control
//! points are `Math.round`ed to integers and scaled, every subdivision
//! midpoint is an arithmetic `>> 1` of an integer sum, and emitted points
//! come back through `(v + 128) >> 8`. Because all intermediates are exact
//! integers in `f64`, the only JS-isms to pin are:
//!
//! * [`crate::jsnum::js_round`] — JS `Math.round` is half-**up**, not
//!   half-away-from-zero like Rust's `f64::round`.
//! * `>>` — ToInt32 + arithmetic shift (see [`crate::jsnum::to_int32`]);
//!   `(v + 128) >> 8` and `(a + b) >> 1` are the exact JS results even if a
//!   pathological control point overflowed 32 bits.
//! * `Math.floor(Math.sqrt(...))` — IEEE-754 `sqrt` is correctly rounded, so
//!   Rust's `f64::sqrt` is bit-identical to V8's; `floor` of an exact
//!   non-negative integer is the identity.
//! * `Math.max` — the NaN-propagating variant (shared with `game_map`), so a
//!   `NaN` spacing/distance poisons the accumulator exactly like JS.
//!
//! The recursion is single-pass in-order De Casteljau subdivision (left
//! segment first), depth-capped at 10 and flat-capped at `dist <= 256`; the
//! emission order of `cached_points` is observable and replayed verbatim.

use crate::game_map::js_max;
use crate::jsnum::{js_round, to_int32};

/// The `Point` the class constructs and returns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

const SUB_SCALE: f64 = 256.0;

/// Shared mutable state of one `sharedSubdivide` walk. `step_threshold` and
/// `cached_points` are `None` for the length-only [`DistanceBasedBezierCurve::get_length`]
/// pass, mirroring the TS's optional fields.
struct SubdivState {
    last_x: f64,
    last_y: f64,
    accum_dist: f64,
    step_threshold: Option<f64>,
    cached_points: Option<Vec<Point>>,
}

/// `(v + 128) >> 8` in JS semantics.
#[inline]
fn shr8(v: f64) -> f64 {
    (to_int32(v + 128.0) >> 8) as f64
}

/// `(a + b) >> 1` in JS semantics.
#[inline]
fn shr1(a: f64, b: f64) -> f64 {
    (to_int32(a + b) >> 1) as f64
}

/// `Math.round(v) * scale` for a control-point coordinate.
#[inline]
fn scale_round(v: f64) -> f64 {
    js_round(v) * SUB_SCALE
}

/// `DistanceBasedBezierCurve`.
pub struct DistanceBasedBezierCurve {
    p0: Point,
    p1: Point,
    p2: Point,
    p3: Point,
    cached_points: Vec<Point>,
    current_index: usize,
    pixel_spacing_scaled: f64,
    accumulated_distance_scaled: f64,
}

impl DistanceBasedBezierCurve {
    /// `new DistanceBasedBezierCurve(p0, p1, p2, p3, distanceIncrement)`.
    pub fn new(p0: &Point, p1: &Point, p2: &Point, p3: &Point, distance_increment: f64) -> Self {
        let mut this = Self {
            p0: *p0,
            p1: *p1,
            p2: *p2,
            p3: *p3,
            cached_points: Vec::new(),
            current_index: 0,
            pixel_spacing_scaled: 1.0,
            accumulated_distance_scaled: 0.0,
        };
        this.compute_all_points(distance_increment);
        this
    }

    /// `DistanceBasedBezierCurve.getLength(p0, p1, p2, p3)` — the
    /// allocation-free length pass (no emission state).
    pub fn get_length(p0: &Point, p1: &Point, p2: &Point, p3: &Point) -> f64 {
        let scale = SUB_SCALE;
        let p0x = scale_round(p0.x);
        let p0y = scale_round(p0.y);
        let p3x = scale_round(p3.x);
        let p3y = scale_round(p3.y);

        let mut st = SubdivState {
            last_x: p0x,
            last_y: p0y,
            accum_dist: 0.0,
            step_threshold: None,
            cached_points: None,
        };

        Self::shared_subdivide(
            p0x,
            p0y,
            scale_round(p1.x),
            scale_round(p1.y),
            scale_round(p2.x),
            scale_round(p2.y),
            p3x,
            p3y,
            0,
            &mut st,
        );

        let edx = p3x - st.last_x;
        let edy = p3y - st.last_y;
        st.accum_dist += (edx * edx + edy * edy).sqrt().floor();

        st.accum_dist / scale
    }

    /// `getAllPoints()`.
    pub fn all_points(&self) -> &[Point] {
        &self.cached_points
    }

    /// `increment(distance = 1)`: the next cached point, or `None` at the end.
    pub fn increment(&mut self, distance: f64) -> Option<Point> {
        self.accumulated_distance_scaled += js_max(1.0, js_round(distance * SUB_SCALE));

        // `cached_points` always holds at least the start point after
        // `compute_all_points`, but saturate to mirror JS's `length - 1`
        // (which would be -1 on an empty array and skip the loop).
        let last = self.cached_points.len().saturating_sub(1);
        while self.current_index < last && self.accumulated_distance_scaled >= self.pixel_spacing_scaled
        {
            self.current_index += 1;
            self.accumulated_distance_scaled -= self.pixel_spacing_scaled;
        }

        if self.current_index >= last {
            return None;
        }
        Some(self.cached_points[self.current_index])
    }

    /// `getCurrentIndex()`.
    pub fn current_index(&self) -> usize {
        self.current_index
    }

    /// `computeAllPoints(pixelSpacing)` — also the constructor body.
    pub fn compute_all_points(&mut self, pixel_spacing: f64) {
        self.cached_points = Vec::new();
        self.current_index = 0;
        self.accumulated_distance_scaled = 0.0;
        self.pixel_spacing_scaled = js_max(SUB_SCALE, js_round(pixel_spacing * SUB_SCALE));

        let step_threshold = self.pixel_spacing_scaled;

        let p0x = scale_round(self.p0.x);
        let p0y = scale_round(self.p0.y);
        let p3x = scale_round(self.p3.x);
        let p3y = scale_round(self.p3.y);

        let mut st = SubdivState {
            last_x: p0x,
            last_y: p0y,
            accum_dist: 0.0,
            step_threshold: Some(step_threshold),
            cached_points: Some(Vec::new()),
        };
        st.cached_points
            .as_mut()
            .unwrap()
            .push(Point { x: shr8(p0x), y: shr8(p0y) });

        // Single-pass recursive midpoint subdivision and inline spatial
        // filtering
        Self::shared_subdivide(
            p0x,
            p0y,
            scale_round(self.p1.x),
            scale_round(self.p1.y),
            scale_round(self.p2.x),
            scale_round(self.p2.y),
            p3x,
            p3y,
            0,
            &mut st,
        );

        let mut points = st.cached_points.take().unwrap();

        // Ensure endpoint is included if not already P3
        let last_pt = Point { x: shr8(p3x), y: shr8(p3y) };
        let last_index = points.len(); // JS `length - 1` guarded by >= 0
        if last_index > 0 {
            let end_cached = points[last_index - 1];
            if end_cached.x != last_pt.x || end_cached.y != last_pt.y {
                points.push(last_pt);
            }
        } else {
            points.push(last_pt);
        }

        self.cached_points = points;
    }

    /// In-order single-pass De Casteljau subdivision with inline spatial
    /// filtering (`sharedSubdivide`). The 10-argument signature mirrors the
    /// TS recursion verbatim — bundling the control points into a struct
    /// would restate nothing but risks reordering the arithmetic.
    #[allow(clippy::too_many_arguments)]
    fn shared_subdivide(
        ax: f64,
        ay: f64,
        bx: f64,
        by: f64,
        cx: f64,
        cy: f64,
        dx: f64,
        dy: f64,
        depth: u32,
        st: &mut SubdivState,
    ) {
        let dist = (bx - ax).abs()
            + (by - ay).abs()
            + (cx - bx).abs()
            + (cy - by).abs()
            + (dx - cx).abs()
            + (dy - cy).abs();
        if dist <= 256.0 || depth >= 10 {
            let edx = ax - st.last_x;
            let edy = ay - st.last_y;

            st.accum_dist += (edx * edx + edy * edy).sqrt().floor();
            st.last_x = ax;
            st.last_y = ay;

            if let (Some(threshold), Some(points)) = (st.step_threshold, &mut st.cached_points) {
                while st.accum_dist >= threshold {
                    points.push(Point {
                        x: shr8(ax),
                        y: shr8(ay),
                    });
                    st.accum_dist -= threshold;
                }
            }
            return;
        }

        // De Casteljau midpoints via bitwise right-shift >> 1
        let m01_x = shr1(ax, bx);
        let m01_y = shr1(ay, by);
        let m12_x = shr1(bx, cx);
        let m12_y = shr1(by, cy);
        let m23_x = shr1(cx, dx);
        let m23_y = shr1(cy, dy);

        let m012_x = shr1(m01_x, m12_x);
        let m012_y = shr1(m01_y, m12_y);
        let m123_x = shr1(m12_x, m23_x);
        let m123_y = shr1(m12_y, m23_y);

        let mx = shr1(m012_x, m123_x);
        let my = shr1(m012_y, m123_y);

        // IN-ORDER RECURSION: Left segment first, then Right segment
        Self::shared_subdivide(ax, ay, m01_x, m01_y, m012_x, m012_y, mx, my, depth + 1, st);
        Self::shared_subdivide(mx, my, m123_x, m123_y, m23_x, m23_y, dx, dy, depth + 1, st);
    }
}
