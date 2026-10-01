//! Port of `src/core/pathfinding/PathFinder.Parabola.ts` — the ballistic
//! (parabola-arc) pathfinder for shells/missiles.
//!
//! [`get_parabola_control_points`] is a pure function: four cubic-Bezier
//! control points whose middle two are lifted by `heightMult * maxHeight`
//! (negative = arc upward) and, unless `ignore_map_bounds`, clamped into the
//! map with the NaN-propagating `within`. The only JS-isms are IEEE `sqrt`
//! (bit-identical to V8) and the default-`??` option resolution, which in Rust
//! is an `Option<ParabolaOptions>` with per-field `unwrap_or` defaults.
//!
//! [`ParabolaUniversalPathFinder`] caches one [`DistanceBasedBezierCurve`]
//! keyed by `last_to`: `next` rebuilds only when the goal changes, so the
//! observable state is `(last_to, curve.current_index)`. Curve points are
//! emitted through `gameMap.ref(floor(x), floor(y))`, which panics on
//! out-of-range coordinates exactly like the TS `throw` (the parity replay
//! catches those panics). The TS array-`from` `findPath` guard is a
//! signature-level check the Rust single-ref API cannot express, so it is
//! not ported.

use crate::game_map::{js_max, GameMap};
use crate::line::{DistanceBasedBezierCurve, Point};
use crate::util::within;

/// `PARABOLA_MIN_HEIGHT`.
const PARABOLA_MIN_HEIGHT: f64 = 50.0;

/// `PathStatus` from `src/core/pathfinding/types.ts` (NEXT=0, COMPLETE=2,
/// NOT_FOUND=3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathStatus {
    Next = 0,
    Complete = 2,
    NotFound = 3,
}

/// `PathResult<TileRef>` — the tagged union of `types.ts`. `NotFound` carries
/// no node (the TS variant has no `node` field).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathResult {
    Next { status: PathStatus, node: f64 },
    Complete { status: PathStatus, node: f64 },
    NotFound { status: PathStatus },
}

/// `ParabolaOptions` — all four fields optional; `None` means the TS
/// `options === undefined` (every default applies).
#[derive(Clone, Copy, Debug, Default)]
pub struct ParabolaOptions {
    pub increment: Option<f64>,
    pub distance_based_height: Option<bool>,
    pub direction_up: Option<bool>,
    pub ignore_map_bounds: Option<bool>,
}

impl ParabolaOptions {
    /// `options?.distanceBasedHeight ?? true`.
    fn distance_based_height(&self) -> bool {
        self.distance_based_height.unwrap_or(true)
    }
    /// `options?.directionUp ?? true`.
    fn direction_up(&self) -> bool {
        self.direction_up.unwrap_or(true)
    }
    /// `options?.ignoreMapBounds ?? false`.
    fn ignore_map_bounds(&self) -> bool {
        self.ignore_map_bounds.unwrap_or(false)
    }
    /// `options?.increment ?? 3` (used by `createCurve`).
    fn increment(&self) -> f64 {
        self.increment.unwrap_or(3.0)
    }
}

/// `getParabolaControlPoints(gameMap, from, to, options?)`.
pub fn get_parabola_control_points(
    game_map: &GameMap,
    from: f64,
    to: f64,
    options: Option<&ParabolaOptions>,
) -> [Point; 4] {
    let (distance_based_height, direction_up, ignore_map_bounds) = match options {
        Some(o) => (
            o.distance_based_height(),
            o.direction_up(),
            o.ignore_map_bounds(),
        ),
        None => (true, true, false),
    };

    let p0 = Point {
        x: game_map.x(from),
        y: game_map.y(from),
    };
    let p3 = Point {
        x: game_map.x(to),
        y: game_map.y(to),
    };
    let dx = p3.x - p0.x;
    let dy = p3.y - p0.y;
    let distance = (dx * dx + dy * dy).sqrt();
    let max_height = if distance_based_height {
        js_max(distance / 3.0, PARABOLA_MIN_HEIGHT)
    } else {
        0.0
    };
    let height_mult = if direction_up { -1.0 } else { 1.0 };
    let map_height = game_map.height();

    let p1y = p0.y + dy / 4.0 + height_mult * max_height;
    let p2y = p0.y + (dy * 3.0) / 4.0 + height_mult * max_height;

    let p1 = Point {
        x: p0.x + dx / 4.0,
        y: if ignore_map_bounds {
            p1y
        } else {
            within(p1y, 0.0, map_height - 1.0)
        },
    };
    let p2 = Point {
        x: p0.x + (dx * 3.0) / 4.0,
        y: if ignore_map_bounds {
            p2y
        } else {
            within(p2y, 0.0, map_height - 1.0)
        },
    };

    [p0, p1, p2, p3]
}

/// `ParabolaUniversalPathFinder implements SteppingPathFinder<TileRef>`.
pub struct ParabolaUniversalPathFinder<'a> {
    game_map: &'a GameMap,
    options: Option<ParabolaOptions>,
    curve: Option<DistanceBasedBezierCurve>,
    last_to: Option<f64>,
}

impl<'a> ParabolaUniversalPathFinder<'a> {
    /// `new ParabolaUniversalPathFinder(gameMap, options?)`.
    pub fn new(game_map: &'a GameMap, options: Option<ParabolaOptions>) -> Self {
        Self {
            game_map,
            options,
            curve: None,
            last_to: None,
        }
    }

    /// `createCurve(from, to)`.
    fn create_curve(&self, from: f64, to: f64) -> DistanceBasedBezierCurve {
        let increment = self.options.map(|o| o.increment()).unwrap_or(3.0);
        let [p0, p1, p2, p3] =
            get_parabola_control_points(self.game_map, from, to, self.options.as_ref());
        DistanceBasedBezierCurve::new(&p0, &p1, &p2, &p3, increment)
    }

    /// `findPath(from, to)`. The TS throws on an array `from`; Rust has no
    /// array overload, so callers pass a single tile ref.
    pub fn find_path(&self, from: f64, to: f64) -> Vec<f64> {
        let curve = self.create_curve(from, to);
        curve
            .all_points()
            .iter()
            .map(|p| self.game_map.tile_ref(p.x.floor(), p.y.floor()))
            .collect()
    }

    /// `next(from, to, speed?)`.
    pub fn next(&mut self, from: f64, to: f64, speed: Option<f64>) -> PathResult {
        if self.last_to != Some(to) {
            self.curve = Some(self.create_curve(from, to));
            self.last_to = Some(to);
        }

        let curve = self.curve.as_mut().unwrap();
        match curve.increment(speed.unwrap_or(1.0)) {
            None => PathResult::Complete {
                status: PathStatus::Complete,
                node: to,
            },
            Some(point) => PathResult::Next {
                status: PathStatus::Next,
                node: self.game_map.tile_ref(point.x.floor(), point.y.floor()),
            },
        }
    }

    /// `invalidate()`.
    pub fn invalidate(&mut self) {
        self.curve = None;
        self.last_to = None;
    }

    /// `currentIndex()` — `curve?.getCurrentIndex() ?? 0`.
    pub fn current_index(&self) -> usize {
        self.curve.as_ref().map(|c| c.current_index()).unwrap_or(0)
    }

    /// Probe-only: [`find_path`](Self::find_path) but returns `None` when a
    /// floored curve point leaves the map (where TS `ref` throws) instead of
    /// panicking — the wasm probe cannot survive an abort, so the throw is
    /// observed as a would-throw flag (same convention as
    /// `probe_gm_ref_throws`). The panicking semantics themselves are pinned
    /// by the native replay test.
    pub fn debug_find_path(&self, from: f64, to: f64) -> Option<Vec<f64>> {
        let curve = self.create_curve(from, to);
        curve
            .all_points()
            .iter()
            .map(|p| {
                let (x, y) = (p.x.floor(), p.y.floor());
                if self.game_map.is_valid_coord(x, y) {
                    Some(self.game_map.tile_ref(x, y))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Probe-only: [`next`](Self::next) with the throw observed as
    /// [`DebugNext::Threw`]. State mutation up to the throw (curve rebuild on
    /// goal change, `increment`) is identical to the real `next`.
    pub fn debug_next(&mut self, from: f64, to: f64, speed: Option<f64>) -> DebugNext {
        if self.last_to != Some(to) {
            self.curve = Some(self.create_curve(from, to));
            self.last_to = Some(to);
        }
        let point = self.curve.as_mut().unwrap().increment(speed.unwrap_or(1.0));
        match point {
            None => DebugNext::Complete(to),
            Some(p) => {
                let (x, y) = (p.x.floor(), p.y.floor());
                if self.game_map.is_valid_coord(x, y) {
                    DebugNext::Next(self.game_map.tile_ref(x, y))
                } else {
                    DebugNext::Threw
                }
            }
        }
    }
}

/// Outcome of [`ParabolaUniversalPathFinder::debug_next`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DebugNext {
    Next(f64),
    Complete(f64),
    Threw,
}
