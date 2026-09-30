//! OpenFront deterministic simulation core, ported from `src/core`.
//!
//! Scope of this crate today: the determinism layer and the data structures
//! that decide ordering everywhere else:
//!
//! * [`pseudo_random`] — `src/core/PseudoRandom.ts` (sfc32 seeded by splitmix32)
//! * [`detmath`] — `src/core/DetMath.ts` (cross-platform `exp/log/pow/atan2`)
//! * [`game_map`] — `src/core/game/GameMap.ts` (`GameMapImpl`: packed
//!   terrain/state typed arrays, neighbour orders, stack-BFS, `updateTile`)
//! * [`jsnum`] — JS numeric coercions (`ToInt32`/`ToUint32`/`ToUint16`) that
//!   typed-array element writes in the ported structures rely on
//! * [`pathfinding::priority_queue`] — `MinHeap` / `BucketQueue`, whose
//!   tie-breaking order feeds directly into A*
//! * [`pathfinding::a_star`] — the generic `AStar` engine (with a reference
//!   `GridAdapter`), whose expansion order is observable through every
//!   army/ship path the simulation takes
//! * [`pathfinding::rail`] — the `AStar.Rail` adapter over a `GameMap`-shaped
//!   terrain surface (`TerrainMap` mirrors `GameMapImpl`'s packed bytes)
//! * [`pathfinding::water`] — the self-contained `AStarWater` engine (inlined
//!   expansion, `MinHeap` f32 priorities, cross-product tie-breaker)
//! * [`tile_set`] — `src/core/game/TileSet.ts` (insertion-ordered tile
//!   container: open-addressed table, tombstones, `iterDepth`-gated deferred
//!   compaction, `Uint32Array` storage quirks)
//! * [`util`] — the deterministic core of `src/core/Util.ts` (wrapped
//!   Manhattan distance, `simpleHash`, first-minimum searches, `getMode`,
//!   bigint clamps, `sigmoid`, bounding-box helpers over a `GameMap`)
//! * [`team_assignment`] — `src/core/game/TeamAssignment.ts` (lobby team
//!   balancing: pins, strict clans, soft friend preference, nation shuffle,
//!   and `resolveTeamsList`)
//! * [`line`] — `src/core/utilities/Line.ts` (`DistanceBasedBezierCurve`:
//!   fixed-point De Casteljau subdivision for MIRV trajectories)
//! * [`veterancy`] — `src/core/game/Veterancy.ts` (`maxHealthWithVeterancy`:
//!   shared warship-veterancy math; integer-percent health bonus floored)
//! * [`motion_plans`] — `src/core/game/MotionPlans.ts` (`packMotionPlans` /
//!   `unpackMotionPlans`: two-pass `Uint32Array` wire format with strict
//!   `wordCount` validation; every field clamped via `to_uint32`)
//!
//! Two rules govern every future port into this crate:
//!
//! 1. **Never use a platform transcendental in simulation logic.** Rust's
//!    `f64::exp` is free to differ from V8's in the last bit. Anything the
//!    simulation needs beyond [`detmath`] gets ported the same way.
//! 2. **Transcribe, do not simplify.** Association order, division vs.
//!    multiplication by a reciprocal, and rounding mode of each step all affect
//!    the result. A "cleaner" expression that changes a last bit is a desync
//!    waiting to happen.
//!
//! Parity is verified against vectors generated from the TS sources by
//! `rust/tools/gen_vectors.mjs`; rerun that script whenever a ported source
//! file changes.

pub mod detmath;
pub mod game_map;
pub mod jsnum;
pub mod line;
pub mod motion_plans;
pub mod pathfinding;
pub mod pseudo_random;
pub mod team_assignment;
pub mod tile_set;
pub mod util;
pub mod veterancy;

#[cfg(feature = "wasm-probe")]
mod wasm_probe;

pub use detmath::{atan2, exp, log, pow, pow2};
pub use pseudo_random::PseudoRandom;
