//! Pathfinding data structures ported from `src/core`.
//!
//! Everything here decides *ordering*, which is what the simulation actually
//! observes: which node A* pops when two share a priority, which tile a BFS
//! visits next, which frontier tile `AttackExecution` expands first. A port that
//! is "equally correct" in a textbook sense but orders ties differently is a
//! desync, so these are transcriptions, not reimplementations.

pub mod abstract_graph;
pub mod abstract_graph_astar;
pub mod a_star;
pub mod bfs_grid;
pub mod connected_components;
pub mod flat_heap;
pub mod mini_map_transformer;
pub mod parabola;
pub mod priority_queue;
pub mod rail;
pub mod stepper;
pub mod water;
pub mod water_bounded;
pub mod water_hierarchical;

/// Re-exported so both grid BFS and A* share one visitor contract.
pub use bfs_grid::Visit;

/// `PathFinder<T>` from `src/core/pathfinding/types.ts`, with `T = TileRef`
/// (`f64`). The `from: T | T[]` union is an enum because the distinction is
/// observable: `MiniMapTransformer` collapses a single-element start array to
/// a scalar start for the inner finder yet still runs its multi-source
/// closest-source logic (TS `Array.isArray`).
#[derive(Clone, Copy, Debug)]
pub enum PathStart<'a> {
    Single(f64),
    Multi(&'a [f64]),
}

impl<'a> PathStart<'a> {
    /// `Array.isArray(from)`.
    pub fn is_multi(&self) -> bool {
        matches!(self, PathStart::Multi(_))
    }
    /// The start tiles as a slice (single -> one-element view).
    pub fn as_slice(&self) -> &[f64] {
        match self {
            PathStart::Single(f) => std::slice::from_ref(f),
            PathStart::Multi(v) => v,
        }
    }
}

/// The `PathFinder<number>` interface: find a tile path from `starts` to
/// `goal`, or `None` when unreachable.
pub trait PathFinder {
    fn find_path(&mut self, starts: PathStart<'_>, goal: f64) -> Option<Vec<f64>>;
}

/// A `&mut P` is itself a `PathFinder` (mirrors TS handing an object reference
/// to a decorator), so `MiniMapTransformer::new(&mut inner, ..)` works.
impl<P: PathFinder + ?Sized> PathFinder for &mut P {
    fn find_path(&mut self, starts: PathStart<'_>, goal: f64) -> Option<Vec<f64>> {
        (**self).find_path(starts, goal)
    }
}

// ---- engine adapters -------------------------------------------------------
//
// Each TS engine normalises the `number | number[]` union differently, so the
// `PathStart` -> concrete-method bridge mirrors that:
//
// * `AStar`, `AStarWater`, `AStarWaterBounded` do `Array.isArray(start) ? start
//   : [start]` at the top of `findPath`, so a single-element array is
//   indistinguishable from a scalar start — collapse via `as_slice()`.
// * `AStarWaterHierarchical` branches on `Array.isArray(from)` *before*
//   collapsing, routing a single-element array to the multi-source path, so the
//   `Single`/`Multi` distinction is preserved.

use a_star::AStar;
use water::AStarWater;
use water_bounded::AStarWaterBounded;
use water_hierarchical::AStarWaterHierarchical;

impl<A: a_star::AStarAdapter> PathFinder for AStar<A> {
    fn find_path(&mut self, starts: PathStart<'_>, goal: f64) -> Option<Vec<f64>> {
        AStar::find_path(self, starts.as_slice(), goal)
    }
}

impl PathFinder for AStarWater {
    fn find_path(&mut self, starts: PathStart<'_>, goal: f64) -> Option<Vec<f64>> {
        AStarWater::find_path(self, starts.as_slice(), goal)
    }
}

impl PathFinder for AStarWaterBounded {
    fn find_path(&mut self, starts: PathStart<'_>, goal: f64) -> Option<Vec<f64>> {
        AStarWaterBounded::find_path(self, starts.as_slice(), goal)
    }
}

impl PathFinder for AStarWaterHierarchical {
    fn find_path(&mut self, starts: PathStart<'_>, goal: f64) -> Option<Vec<f64>> {
        match starts {
            PathStart::Single(f) => AStarWaterHierarchical::find_path_single(self, f, goal),
            PathStart::Multi(v) => AStarWaterHierarchical::find_path_multi(self, v, goal),
        }
    }
}
