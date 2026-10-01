//! Port of `src/core/pathfinding/PathFinderStepper.ts`.
//!
//! A generic stepping wrapper around a [`PathFinder`]: caches the current
//! route, advances one node per `next()` call, and invalidates when the goal
//! changes or the unit strays off the cached route.
//!
//! The TS class is generic over `T` with an injected `StepperConfig`
//! (`equals`/`distance`/`preCheck`). This port pins `T = TileRef` (`f64`) and
//! the two production config shapes:
//! * `game_map: Some(gm)` — the `tileStepperConfig` shape: `equals` is `===`,
//!   `distance` is `manhattanDist`, and `preCheck` returns `NOT_FOUND` for
//!   invalid refs (the `typeof !== "number"` guard is vacuous in Rust).
//! * `game_map: None` — the default config `{ equals: (a, b) => a === b }`:
//!   no `distance`, no `preCheck`.
//!
//! JS-isms pinned here:
//! * Numeric paths are stored as `Vec<u32>` (`new Uint32Array(path)`), so
//!   every element passes through `to_uint32` (wrapping negatives/NaN to 0).
//! * `pathIndex > 0` short-circuits the stale-position check, so the TS
//!   `path[-1]` → `undefined` comparison never runs.
//! * The strayed-off-route case recurses after `invalidate()` + `lastTo = to`
//!   (modelled as a loop here).
//! * `pathAfterNext()` returns `null` while no traversal is active
//!   (`path === null || pathIndex === 0`).
//! * `findPath` with an empty start array: `every` over `[]` is `true`, so
//!   the production pre-check short-circuits to `null` without calling the
//!   inner finder.

use crate::game_map::GameMap;
use crate::jsnum::to_uint32;

use super::parabola::{PathResult, PathStatus};
use super::{PathFinder, PathStart};

/// `PathFinderStepper<TileRef>` from the TS source.
pub struct PathFinderStepper<'a, I: PathFinder> {
    finder: I,
    game_map: Option<&'a GameMap>,
    /// `path: T[] | Uint32Array | null` — for `T = TileRef` the numeric
    /// branch always applies (non-empty paths) or stores the empty array.
    path: Option<Vec<u32>>,
    path_index: usize,
    last_to: Option<f64>,
}

impl<'a, I: PathFinder> PathFinderStepper<'a, I> {
    /// `constructor(finder, config)`. `game_map` selects the production
    /// `tileStepperConfig` shape; `None` is the default bare config.
    pub fn new(finder: I, game_map: Option<&'a GameMap>) -> Self {
        Self {
            finder,
            game_map,
            path: None,
            path_index: 0,
            last_to: None,
        }
    }

    /// `next(from, to, dist)`.
    pub fn next(&mut self, from: f64, to: f64, dist: Option<f64>) -> PathResult {
        // Domain-specific pre-check (validation).
        if let Some(gm) = self.game_map {
            if !gm.is_valid_ref(from) || !gm.is_valid_ref(to) {
                return PathResult::NotFound {
                    status: PathStatus::NotFound,
                };
            }
        }

        if from == to {
            return PathResult::Complete {
                status: PathStatus::Complete,
                node: to,
            };
        }

        // Distance-based early exit.
        if let (Some(d), Some(gm)) = (dist, self.game_map) {
            if d > 0.0 && gm.manhattan_dist(from, to) <= d {
                return PathResult::Complete {
                    status: PathStatus::Complete,
                    node: from,
                };
            }
        }

        loop {
            // Invalidate cache if destination changed.
            if self.last_to != Some(to) {
                self.path = None;
                self.path_index = 0;
                self.last_to = Some(to);
            }

            // Compute path if not cached.
            if self.path.is_none() {
                let p = match self.finder.find_path(PathStart::Single(from), to) {
                    None => {
                        return PathResult::NotFound {
                            status: PathStatus::NotFound,
                        }
                    }
                    Some(p) => p,
                };
                if !p.is_empty() && p[0] == from {
                    self.path_index = 1;
                } else {
                    self.path_index = 0;
                }
                self.path = Some(p.iter().map(|v| to_uint32(*v)).collect());
            }

            let path = self.path.as_ref().unwrap();

            // Strayed off the cached route: invalidate and restart.
            if self.path_index > 0 {
                let expected = tile_at(path, self.path_index - 1);
                if from != expected {
                    self.invalidate();
                    self.last_to = Some(to);
                    continue;
                }
            }

            if self.path_index >= path.len() {
                return PathResult::Complete {
                    status: PathStatus::Complete,
                    node: to,
                };
            }

            let node = tile_at(path, self.path_index);
            self.path_index += 1;
            return PathResult::Next {
                status: PathStatus::Next,
                node,
            };
        }
    }

    /// `invalidate()`.
    pub fn invalidate(&mut self) {
        self.path = None;
        self.path_index = 0;
        self.last_to = None;
    }

    /// `pathAfterNext()` — the active route from the most recently returned
    /// node, or `None` when no traversal is active.
    pub fn path_after_next(&self) -> Option<Vec<f64>> {
        let path = self.path.as_ref()?;
        if self.path_index == 0 {
            return None;
        }
        Some(path[self.path_index - 1..].iter().map(|v| *v as f64).collect())
    }

    /// `findPath(from, to)` — one-shot query; the production pre-check
    /// short-circuits when *every* start fails (empty array: `every` is
    /// vacuously true).
    pub fn find_path(&mut self, from: PathStart<'_>, to: f64) -> Option<Vec<f64>> {
        if let Some(gm) = self.game_map {
            let all_failed = from
                .as_slice()
                .iter()
                .all(|f| !gm.is_valid_ref(*f) || !gm.is_valid_ref(to));
            if all_failed {
                return None;
            }
        }
        self.finder.find_path(from, to)
    }

    /// Debug accessor: `pathIndex`.
    pub fn debug_path_index(&self) -> usize {
        self.path_index
    }

    /// Debug accessor: whether a path is cached (`path !== null`).
    pub fn debug_has_path(&self) -> bool {
        self.path.is_some()
    }

    /// Debug accessor: `lastTo`.
    pub fn debug_last_to(&self) -> Option<f64> {
        self.last_to
    }
}

/// Typed-array read semantics: OOB yields `undefined` -> `NaN` once compared
/// against a number.
#[inline]
fn tile_at(path: &[u32], i: usize) -> f64 {
    path.get(i).map_or(f64::NAN, |v| *v as f64)
}

/// Shared inner-finder stub for the parity harness: queue-backed, cheaply
/// `Clone` (the stepper owns a handle, the test reads the observation through
/// another), and counts calls so "inner not called" (cache hit, pre-check hit,
/// vacuous `every`) is observable. Mirrors the TS stub in
/// `tools/gen_vectors.mjs`.
#[derive(Clone, Default)]
pub struct SharedStub(std::rc::Rc<std::cell::RefCell<StubState>>);

#[derive(Default)]
struct StubState {
    queue: std::collections::VecDeque<Option<Vec<f64>>>,
    last_seen: Option<(bool, Vec<f64>, f64)>,
    calls: u64,
}

impl SharedStub {
    /// Queue a `null` result.
    pub fn push_null(&self) {
        self.0.borrow_mut().queue.push_back(None);
    }
    /// Queue a tile-list result.
    pub fn push_list(&self, tiles: Vec<f64>) {
        self.0.borrow_mut().queue.push_back(Some(tiles));
    }
    /// Clear the queue + observation (call count persists).
    pub fn reset(&self) {
        let mut s = self.0.borrow_mut();
        s.queue.clear();
        s.last_seen = None;
    }
    /// Cumulative `findPath` calls.
    pub fn calls(&self) -> u64 {
        self.0.borrow().calls
    }
    /// `(is_multi, tiles, goal)` from the most recent call.
    pub fn last_seen(&self) -> Option<(bool, Vec<f64>, f64)> {
        self.0.borrow().last_seen.clone()
    }
}

impl PathFinder for SharedStub {
    fn find_path(&mut self, starts: PathStart<'_>, goal: f64) -> Option<Vec<f64>> {
        let mut s = self.0.borrow_mut();
        s.calls += 1;
        s.last_seen = Some((starts.is_multi(), starts.as_slice().to_vec(), goal));
        s.queue.pop_front().flatten()
    }
}
