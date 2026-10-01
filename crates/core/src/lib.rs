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
//! * [`close_codes`] — `src/core/CloseCodes.ts` (`CloseCode` / `CloseReason`
//!   tables, `isTerminalClose` range test and `isCloseReason` set membership)
//! * [`anon_names`] — `src/core/AnonNames.ts` (`ANON_WORDS` bank +
//!   `anonWordName`: trunc/abs/floor JS-isms, property-access word lookup
//!   that spells out-of-range indices as `"undefined"`, and `String(round)`
//!   for the suffix)
//! * [`server_list`] — `src/core/ServerList.ts` (commit/site predicates,
//!   `commitsMatch`/`versionMatches` prefix rules, build-aware letter
//!   filtering and `pickServerForBuild` clamping, `ownLetterIn` host match,
//!   and the `/v/<commit>/` URL rewrites with hand-transcribed regexes and a
//!   JS-accurate `decodeURIComponent`; the zod schemas are not ported)
//! * [`pattern_decoder`] — `src/core/PatternDecoder.ts` (packed player-pattern
//!   header decode and `isPrimary` bit lookup; the injected `base64urlDecode`
//!   is not ported — the module takes the decoded bytes directly)
//! * [`doomsday_clock`] — `src/core/game/DoomsdayClock.ts` (wave-schedule
//!   threshold math: required share / HUD wave state / troop floor / convex
//!   drain curve / rot lattice + hashed noises / rot quota; integer-only,
//!   `Math.imul`→`wrapping_mul`, `>>>0`→`to_uint32`)
//! * [`exec_util`] — the pure-`GameMap` subset of `src/core/execution/Util.ts`
//!   (`computeNukeBlastCounts` insertion-ordered owner weights, `getSpawnTiles`
//!   over the centred radius-4 Euclidean stack-BFS, `closestTile` /
//!   `nearestTileDist` first-minimum strict `<`, `nearestTileDistCapped` with
//!   the `isTileSetLike` duck-test branch (Manhattan ring walk vs. clamped
//!   linear scan) and `closestTwoTiles` stable-by-column sort + two-pointer
//!   sweep; the three `Game`-facade functions are out of scope)
//! * [`water_manager`] — `src/core/game/WaterManager.ts` (water-nuke terrain
//!   fixup: pending-tile flush, ocean BFS propagation, crater-grouped
//!   magnitude BFS over direct terrain bytes, shoreline 2-ring recompute,
//!   minimap 2×2 downsample + mini ocean/magnitude BFSes, persistent shared
//!   `ConnectedComponents` incremental labeling, 20-tick throttled graph
//!   rebuild as a version bump — the graph/HPA machinery never crosses the
//!   WaterManager API surface)
//! * [`game_update_utils`] — `src/core/game/GameUpdateUtils.ts`
//!   (`diffPlayerUpdate` field-by-field diff with the `a === b` reference
//!   fast path modelled as a capture `refid`, `applyStateUpdate` in-place
//!   merge with the `Number()` / `Math.max(0,·)` / `.slice()` quirks, and
//!   `packAttackTroopDeltas` membership-gated troop quads)
//! * [`railroad`] — `src/core/game/Railroad.ts` (`getClosestTileIndex`
//!   squared-distance first-minimum over `game.x/y`-decoded tiles,
//!   `getOrientedRailroad` neighbor lookup + tile reversal, and `delete`'s
//!   observable destruction-update / `removeRailroad` call sequence; station
//!   identity rides in as capture refids — the `TrainStation` internals are a
//!   later port)
//! * [`railroad_spatial_grid`] — `src/core/game/RailroadSpatialGrid.ts`
//!   (`RailSpatialGrid`: `cellSize x cellSize` spatial buckets keyed by the
//!   `` `${cx}:${cy}` `` template string; register / unregister / query over
//!   `game.x/y`-decoded rail tiles, rails keyed by capture refid, insertion-
//!   ordered `Map`/`Set` semantics preserved)
//! * [`line`] — `src/core/utilities/Line.ts` (`DistanceBasedBezierCurve`:
//!   fixed-point De Casteljau subdivision for MIRV trajectories)
//! * [`veterancy`] — `src/core/game/Veterancy.ts` (`maxHealthWithVeterancy`:
//!   shared warship-veterancy math; integer-percent health bonus floored)
//! * [`motion_plans`] — `src/core/game/MotionPlans.ts` (`packMotionPlans` /
//!   `unpackMotionPlans`: two-pass `Uint32Array` wire format with strict
//!   `wordCount` validation; every field clamped via `to_uint32`)
//! * [`pathfinding::connected_components`] — `src/core/pathfinding/algorithms/ConnectedComponents.ts`
//!   (scan-line flood-fill component labelling over packed terrain bytes,
//!   `Uint8Array`→`Uint16Array` upgrade at 253 components, union-find alias
//!   table for incremental `addWaterTiles` merges, path-compressed `find`)
//! * [`terrain_search_map`] — `src/core/game/TerrainSearchMap.ts` (read-only
//!   search view over packed terrain bytes: header-decoded width/height,
//!   `Land`/`Shore`/`Water` classification, 8-way `neighbors` with JS
//!   out-of-range and fractional-index semantics)
//! * [`pathfinding::abstract_graph`] — `src/core/pathfinding/algorithms/AbstractGraph.ts`
//!   (`AbstractGraph` container + `AbstractGraphBuilder`: cluster-boundary
//!   gateway nodes from contiguous water "entrance" spans, intra-cluster edges
//!   from bounded grid-BFS distances (edge ids follow BFS find order),
//!   canonical `(lo,hi)` edge dedup keeping the cheaper cost, and the
//!   clean-cluster partial-rebuild cache keyed by `(minTile,maxTile)`)
//! * [`pathfinding::water_bounded`] — `src/core/pathfinding/algorithms/AStar.WaterBounded.ts`
//!   (`AStarWaterBounded`: the bounded-window sibling of `AStarWater` — all
//!   four stamp arrays indexed by a clamped window-local id, the
//!   `numLocalNodes` guard, the 3× shore penalty curve, and defaults
//!   `heuristicWeight ?? 3` / `maxIterations ?? 100_000`)
//! * [`pathfinding::abstract_graph_astar`] — `src/core/pathfinding/algorithms/AStar.AbstractGraph.ts`
//!   (`AbstractGraphAStar`: A* over the abstract graph — `Float32Array`
//!   g-scores (f32-rounded comparisons), multi-source origin tracking via
//!   `startNode`, goal/start checks *before* `queue.clear()`, single-start
//!   multi delegation, and the `buildPathFromGoal` out-of-range/over-long
//!   null guards; defaults `heuristicWeight ?? 1` / `maxIterations ?? 100_000`)
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

pub mod anon_names;
pub mod close_codes;
pub mod detmath;
pub mod doomsday_clock;
pub mod exec_util;
pub mod game_map;
pub mod game_update_utils;
pub mod jsnum;
pub mod line;
pub mod motion_plans;
pub mod pathfinding;
pub mod pattern_decoder;
pub mod pseudo_random;
pub mod railroad;
pub mod railroad_spatial_grid;
pub mod server_list;
pub mod team_assignment;
pub mod terrain_search_map;
pub mod tile_set;
pub mod util;
pub mod veterancy;
pub mod water_manager;

#[cfg(feature = "wasm-probe")]
mod wasm_probe;

pub use detmath::{atan2, exp, log, pow, pow2};
pub use pseudo_random::PseudoRandom;
