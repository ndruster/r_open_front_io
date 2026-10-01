//! Port of `src/core/pathfinding/transformers/MiniMapTransformer.ts`.
//!
//! A `PathFinder` decorator: downscales the start(s)/goal to a half-resolution
//! minimap, delegates to `inner`, upscales the returned path back to main-map
//! refs (inserting interpolated tiles so consecutive points stay adjacent),
//! then repairs the endpoints (`fixExtremes`).
//!
//! JS-isms pinned here:
//! * `Math.floor(map.x(f) / 2)` downscale, `mini.x(p) * 2` upscale.
//! * `Math.round` (half-up, [`crate::jsnum::js_round`]) for interpolated tiles.
//! * Multi-source closest start uses Manhattan distance with a strict `<`
//!   (ties keep the *first* source) and `Infinity` initial best.
//! * A single-element start array collapses to a scalar start for `inner`
//!   (`miniFromArray.length === 1 ? miniFromArray[0] : miniFromArray`) yet
//!   still runs the multi-source branch (`Array.isArray(from)`).
//! * `indexOf` over numeric tile refs -> `position()` with `==` (refs are
//!   finite integers, SameValueZero coincides).
//! * Out-of-range `main.ref` / `mini.ref` throws in TS -> panics here;
//!   [`MiniMapTransformer::debug_find_path`] observes the throw as
//!   [`Throw`] for the wasm probe (same convention as `parabola`).

use crate::game_map::{js_max, GameMap};
use crate::jsnum::js_round;

use super::{PathFinder, PathStart};

/// Marker returned by [`MiniMapTransformer::debug_find_path`] where the real
/// call would have panicked on an out-of-range `ref`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Throw;

/// `MiniMapTransformer` from the TS source.
pub struct MiniMapTransformer<'a, I: PathFinder> {
    inner: I,
    map: &'a GameMap,
    mini_map: &'a GameMap,
}

impl<'a, I: PathFinder> PathFinder for MiniMapTransformer<'a, I> {
    fn find_path(&mut self, starts: PathStart<'_>, goal: f64) -> Option<Vec<f64>> {
        MiniMapTransformer::find_path(self, starts, goal)
    }
}

impl<'a, I: PathFinder> MiniMapTransformer<'a, I> {
    /// `constructor(inner, map, miniMap)`.
    pub fn new(inner: I, map: &'a GameMap, mini_map: &'a GameMap) -> Self {
        Self {
            inner,
            map,
            mini_map,
        }
    }

    /// `findPath(from, to)`. `None` mirrors the TS `null` (inner returned
    /// nothing or an empty path). Panics where TS `ref` throws.
    pub fn find_path(&mut self, from: PathStart<'_>, to: f64) -> Option<Vec<f64>> {
        self.find_path_impl(from, to, false).ok().flatten()
    }

    /// Probe-only: [`find_path`](Self::find_path) with every `ref` throw
    /// observed as [`Err(Throw)`] instead of a panic. The panicking semantics
    /// themselves are pinned by the native replay test.
    pub fn debug_find_path(
        &mut self,
        from: PathStart<'_>,
        to: f64,
    ) -> Result<Option<Vec<f64>>, Throw> {
        self.find_path_impl(from, to, true)
    }

    fn find_path_impl(
        &mut self,
        from: PathStart<'_>,
        to: f64,
        debug: bool,
    ) -> Result<Option<Vec<f64>>, Throw> {
        // Convert game coords -> minimap coords (supports multi-source).
        let from_slice = from.as_slice();
        let mut mini_from = Vec::with_capacity(from_slice.len());
        for &f in from_slice {
            mini_from.push(self.downscale(f, debug)?);
        }
        let mini_start = if mini_from.len() == 1 {
            PathStart::Single(mini_from[0])
        } else {
            PathStart::Multi(&mini_from)
        };
        let mini_to = self.downscale(to, debug)?;

        // Search on minimap.
        let path = match self.inner.find_path(mini_start, mini_to) {
            None => return Ok(None),
            Some(p) if p.is_empty() => return Ok(None),
            Some(p) => p,
        };

        let upscaled = self.upscale_path(&path, debug)?;

        // For multi-source, find closest source to path start.
        let mut src_ref: Option<f64> = None;
        if from.is_multi() {
            if let Some(&first) = upscaled.first() {
                let start_x = self.map.x(first);
                let start_y = self.map.y(first);
                let mut min_dist = f64::INFINITY;
                for &f in from_slice {
                    let dist =
                        (self.map.x(f) - start_x).abs() + (self.map.y(f) - start_y).abs();
                    if dist < min_dist {
                        min_dist = dist;
                        src_ref = Some(f);
                    }
                }
            }
        } else {
            src_ref = Some(from_slice[0]);
        }
        Ok(Some(self.fix_extremes(upscaled, to, src_ref)))
    }

    /// `miniMap.ref(floor(map.x(f) / 2), floor(map.y(f) / 2))`.
    fn downscale(&self, f: f64, debug: bool) -> Result<f64, Throw> {
        let x = (self.map.x(f) / 2.0).floor();
        let y = (self.map.y(f) / 2.0).floor();
        self.ref_checked(self.mini_map, x, y, debug)
    }

    /// `main.ref(mini.x(p) * 2, mini.y(p) * 2)` plus per-segment interpolation
    /// (`scaleFactor = 2`).
    fn upscale_path(&self, path: &[f64], debug: bool) -> Result<Vec<f64>, Throw> {
        let mut smooth: Vec<f64> = Vec::new();

        for i in 0..path.len().saturating_sub(1) {
            let cur_x = self.mini_map.x(path[i]) * 2.0;
            let cur_y = self.mini_map.y(path[i]) * 2.0;
            let next_x = self.mini_map.x(path[i + 1]) * 2.0;
            let next_y = self.mini_map.y(path[i + 1]) * 2.0;

            smooth.push(self.ref_checked(self.map, cur_x, cur_y, debug)?);

            let dx = next_x - cur_x;
            let dy = next_y - cur_y;
            let steps = js_max(dx.abs(), dy.abs());

            let mut step = 1.0;
            while step < steps {
                smooth.push(self.ref_checked(
                    self.map,
                    js_round(cur_x + (dx * step) / steps),
                    js_round(cur_y + (dy * step) / steps),
                    debug,
                )?);
                step += 1.0;
            }
        }

        if let Some(&last) = path.last() {
            smooth.push(self.ref_checked(
                self.map,
                self.mini_map.x(last) * 2.0,
                self.mini_map.y(last) * 2.0,
                debug,
            )?);
        }

        Ok(smooth)
    }

    /// `fixExtremes(upscaled, dst, src)`.
    fn fix_extremes(&self, mut upscaled: Vec<f64>, dst: f64, src: Option<f64>) -> Vec<f64> {
        if let Some(src) = src {
            match upscaled.iter().position(|&t| t == src) {
                None => upscaled.insert(0, src),
                Some(idx) if idx != 0 => {
                    upscaled.drain(..idx);
                }
                Some(_) => {}
            }
        }

        match upscaled.iter().position(|&t| t == dst) {
            None => upscaled.push(dst),
            Some(idx) if idx != upscaled.len() - 1 => upscaled.truncate(idx + 1),
            Some(_) => {}
        }
        upscaled
    }

    /// `gm.ref(x, y)` with the throw observed (`debug`) or panicked (real).
    fn ref_checked(&self, gm: &GameMap, x: f64, y: f64, debug: bool) -> Result<f64, Throw> {
        if gm.is_valid_coord(x, y) {
            Ok(gm.tile_ref(x, y))
        } else if debug {
            Err(Throw)
        } else {
            // Invalid coord: tile_ref panics ("Invalid coordinates: x,y"),
            // mirroring the TS throw. The Ok is unreachable.
            Ok(gm.tile_ref(x, y))
        }
    }
}

/// Parity-harness inner finder: pops a scripted path per call and records what
/// the transformer passed it (`PathStart` kind + tiles + goal), so the
/// scalar/array collapse and downscale are observable. Mirrors the TS stub used
/// by `tools/gen_vectors.mjs`.
#[derive(Default)]
pub struct ScriptedFinder {
    /// Paths returned by successive `find_path` calls (`None` = TS `null`;
    /// exhausted queue behaves as `null`).
    pub paths: std::collections::VecDeque<Option<Vec<f64>>>,
    /// `Some((is_multi, tiles, goal))` from the most recent call, `None` if
    /// never called since the last reset.
    pub last_seen: Option<(bool, Vec<f64>, f64)>,
}

impl ScriptedFinder {
    /// Queue one scripted path (`None` = TS `null`, empty vec = TS `[]`).
    pub fn push_path(&mut self, path: Option<Vec<f64>>) {
        self.paths.push_back(path);
    }
    /// Clear `last_seen` before a query (mirrors the TS stub resetting to
    /// `null`).
    pub fn reset_seen(&mut self) {
        self.last_seen = None;
    }
}

impl PathFinder for ScriptedFinder {
    fn find_path(&mut self, starts: PathStart<'_>, goal: f64) -> Option<Vec<f64>> {
        self.last_seen = Some((starts.is_multi(), starts.as_slice().to_vec(), goal));
        self.paths.pop_front().flatten()
    }
}
