//! Pathfinding data structures ported from `src/core`.
//!
//! Everything here decides *ordering*, which is what the simulation actually
//! observes: which node A* pops when two share a priority, which tile a BFS
//! visits next, which frontier tile `AttackExecution` expands first. A port that
//! is "equally correct" in a textbook sense but orders ties differently is a
//! desync, so these are transcriptions, not reimplementations.

pub mod a_star;
pub mod bfs_grid;
pub mod flat_heap;
pub mod priority_queue;
pub mod rail;
pub mod water;

/// Re-exported so both grid BFS and A* share one visitor contract.
pub use bfs_grid::Visit;
