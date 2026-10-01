# Rust port of `src/core`

Incremental rewrite of the OpenFront simulation core in Rust. This directory is
self-contained: it does not touch the TypeScript build, `package.json`, or CI.

## What is here

```
rust/
├── Cargo.toml                     workspace
├── cargo-vs.bat                   runs cargo with the MSVC link env set up
├── crates/core/
│   ├── Cargo.toml                 zero dependencies
│   ├── src/
│   │   ├── lib.rs                 crate entry + porting rules
│   │   ├── pseudo_random.rs       port of src/core/PseudoRandom.ts
│   │   ├── detmath.rs             port of src/core/DetMath.ts
│   │   ├── jsnum.rs               JS coercions: ToInt32/ToUint32/ToUint16/f32
│   │   ├── game_map.rs            port of game/GameMap.ts (GameMapImpl
│   │   │                          + the distFN factories)
│   │   ├── tile_set.rs            port of game/TileSet.ts
│   │   ├── util.rs                port of Util.ts (deterministic core only)
│   │   ├── team_assignment.rs     port of game/TeamAssignment.ts
│   │   ├── line.rs                port of utilities/Line.ts
│   │   │                          (DistanceBasedBezierCurve)
│   │   ├── veterancy.rs           port of game/Veterancy.ts
│   │   │                          (maxHealthWithVeterancy)
│   │   ├── motion_plans.rs        port of game/MotionPlans.ts
│   │   │                          (packMotionPlans / unpackMotionPlans)
│   │   ├── terrain_search_map.rs  port of game/TerrainSearchMap.ts
│   │   │                          (SearchMapTileType + node / neighbors)
│   │   ├── wasm_probe.rs          `extern "C"` surface, feature-gated
│   │   └── pathfinding/
│   │       ├── mod.rs
│   │       ├── priority_queue.rs  port of algorithms/PriorityQueue.ts
│   │       ├── flat_heap.rs       port of execution/utils/FlatBinaryHeap.ts
│   │       ├── bfs_grid.rs        port of algorithms/BFS.Grid.ts
│   │       ├── a_star.rs          port of algorithms/AStar.ts (+ GridAdapter)
│   │       ├── rail.rs            port of algorithms/AStar.Rail.ts
│   │       │                      (+ TerrainMap: GameMapImpl's packed bytes)
│   │       ├── water.rs           port of algorithms/AStar.Water.ts
│   │       ├── water_bounded.rs   port of algorithms/AStar.WaterBounded.ts
│   │       │                      (window-local indexing, clamped bounds)
│   │       ├── abstract_graph.rs  port of algorithms/AbstractGraph.ts
│   │       │                      (+ AbstractGraphBuilder: gateway nodes,
│   │       │                       bounded-BFS edges, partial rebuild)
│   │       ├── abstract_graph_astar.rs
│   │       │                      port of algorithms/AStar.AbstractGraph.ts
│   │       │                      (Float32Array gScore, startNode origins)
│   │       ├── water_hierarchical.rs
│   │       │                      port of algorithms/AStar.WaterHierarchical.ts
│   │       │                      (orchestrator: early exit / node lookup /
│   │       │                       abstract stitch / multi-source dispatch)
│   │       ├── parabola.rs
│   │       │                      port of PathFinder.Parabola.ts
│   │       │                      (control points + curve-cached stepping)
│   │       └── connected_components.rs
│   │                              port of algorithms/ConnectedComponents.ts
│   └── tests/
│       ├── data/vectors.rs        generated golden vectors (do not edit)
│       ├── data/vectors.json      same data, for the wasm runner
│       ├── parity_golden.rs       bit-for-bit parity vs the TS originals
│       └── parity_structures.rs   op-trace replay vs the TS classes
└── tools/
    ├── gen_vectors.mjs            regenerates both vector files from the TS
    ├── ts_load.mjs                shim: loads TS classes under strip-only node
    ├── run_wasm_parity.mjs        executes the wasm build vs vectors.json
    ├── smoke_ts_load.mjs          sanity check for the loading shim
    ├── probe_jsnum.mjs            documents V8 coercion behaviour by example
    └── find_sdk.ps1               diagnostic: locate Windows SDK import libs
```

## The parity contract

The game is deterministic and every client hashes its simulation state, so a
single-ULP difference between the Rust and TypeScript implementations is a
desync, not a rounding nit. Two things enforce that here:

1. **`tests/parity_golden.rs`** asserts on `f64::to_bits()`, never on a
   tolerance. It covers the PRNG u32 stream (29 seeds × 24 steps, including
   negative, fractional and >2^32 seeds to pin JS `| 0` truncation), `nextInt`
   over 6 ranges, 200 `nextID` strings, shuffle permutations, a 256-bit
   `chance` stream, and `exp`/`log`/`pow`/`atan2`/`pow2` sweeps.
2. **`tools/gen_vectors.mjs`** produces those vectors by *importing the real
   TypeScript files*, not by restating them. So the fixtures track the
   implementation. `DetMath` additionally cross-checks the eight values pinned
   in `tests/core/__snapshots__/DetMath.test.ts.snap`. The script emits both
   `data/vectors.rs` (for `cargo test`) and `data/vectors.json` (for the wasm
   runner below) from one set of computed data.
3. For the stateful structures (`MinHeap`, `BucketQueue`, `FlatBinaryHeap`,
   `BfsGrid`) one value is not enough: **`tests/parity_structures.rs`** replays
   whole operation traces (push/pop/clear/isEmpty scripts with NaN priorities,
   f32 ties, negative buckets, capacity underflow) that the TS classes actually
   executed, comparing every return value *and* the final internal arrays —
   including the deliberate quirks the TS source has (popping an empty typed
   array yields its stale `0`; a negative-bucket push is counted but
   unreachable; `dist` wraps at 65 536).
4. `AStar` is stateful across calls *and* depends on an adapter, so its traces
   record the scenario's `GridAdapter` parameters (size, blocked tiles, cost,
   turn penalty, heuristic kind/scale, iteration cap) alongside the returned
   path and the four stamp-tracked arrays after the last `findPath`. Ten
   scenarios cover straight lines, walls, start == goal, the iteration cap,
   multi-start, turn costs, weighted and zero heuristics, instance reuse, and
   a negative cost that wraps `gScore` through `Uint32Array` storage.
5. The **rail adapter** (`pathfinding::rail`) is pinned the same way, but over
   the *real* `GameMapImpl`: scenarios are ASCII terrain maps packed into
   `GameMapImpl`'s terrain bytes (bit 7 land, 6 shoreline, 5 ocean, 0-4
   magnitude), and the trace replays through `rail::TerrainMap`. Six scenarios
   cover the shoreline-crossing rule, the plain-ocean wall, the quirk that
   `isImpassable` gates *expansion* but not *entry*, the water/shoreline and
   direction-change cost penalties, and a lake ring where only shoreline tiles
   are walkable.
6. **`GameMapImpl`** itself (`game_map.rs`) is pinned with five op-trace
   scenarios (4×4 sweep, 3×3 counter bookkeeping, 2×2 invalid-ref reads, 3×1
   `updateTile` packing, 5×5 `bfs`/`circleSearch`). The traces record every
   observable: each op's return token (including the `undefined` a typed-array
   read yields out of bounds), the final `terrain`/`state` buffers, and the
   three counters. They pin the JS-isms the class inherits — `x()` keeps the
   sign of the dividend, `y()` is `| 0` not `Math.floor`, `bfs` is LIFO via
   `q.pop()`, `Set` order is insertion order with SameValueZero dedup, and
   invalid writes are dropped while the surrounding bookkeeping still runs.
   The `throw` paths (`ref`, `setOwnerID`) are replayed under `catch_unwind`.
7. **`AStarWater`** (`pathfinding::water`) is pinned with seven scenarios over
   real `GameMapImpl` terrain bytes: a straight deep-water shot, a shallow
   magnitude band the search detours around (+1000/tile), a land wall with one
   water gap, a land *goal* (enterable even though land is a wall), a
   multi-start lake ring where the cross-product tie-breaker decides the side,
   a weight-1 (near-Dijkstra) field where every f32 priority ties and the
   MinHeap's insertion-order pop decides everything, and an iteration cap that
   forces `null`. Each records the path plus all four stamp-tracked arrays.
8. **`TileSet`** (`tile_set.rs`) is pinned with nine op-trace scenarios over
   the real class: insertion order, duplicate-add, delete-then-re-add (moves
   the value to the end), growth past the dense(16)/table(32) defaults, a
   60-delete sweep that triggers the `iterDepth`-gated deferred compaction,
   `forEach` that *adds* mid-iteration (the appended value must still be
   visited — `denseLen` is re-read each step) and one that *deletes* a
   not-yet-visited entry (it must be skipped), and `clear()`. The Uint32Array
   storage quirk is its own scenario: `add(-1)` stores `0xffffffff`, so
   `has(-1)` is false while `has(4294967295)` is true, yet `values()` skips
   the tombstone-equal slot. Each trace pins the final `dense`/`table`
   buffers, `denseLen`, `size_`, `tableUsed` and `iterDepth`.
9. **`Util.ts`** (`util.rs`) is pinned with 347 single-call scenarios over the
   real TS functions: wrapped Manhattan distance on a width-100 torus, the
   `Math.min/max` clamp edges (including `NaN` and the `-0`/`+0` sign rules
   that `js_min`/`js_max` now share), `simpleHash` over ASCII, BMP and
   surrogate-pair strings (the UTF-16 `charCodeAt` walk), `findMinimumBy` /
   `findClosestBy` across five score and four candidate closures × nine value
   sets (first-minimum ties, `NaN` scores that never win, empty inputs),
   `getMode` with insertion-order tie-breaking, the `toInt` ±Infinity clamps
   and `NaN` throw, `maxInt`/`minInt`/`withinInt`, `sigmoid` over the
   deterministic exp stream, `boundingBoxCenter`/`inscribed`, and
   `calculateBoundingBox` / `calculateBoundingBoxCenter` / `boundingBoxTiles`
   over real `GameMapImpl` maps through all three TS container branches
   (Array, Set, TileSet). Results compare by IEEE-754 bit pattern.
10. **`TeamAssignment.ts`** (`team_assignment.rs`) is pinned with 66 lobby
    scenarios over the real TS functions: server-pinned team slots (including
    the out-of-range / fractional / `NaN` pins that JS `teams[i]` leaves
    unpinned), strict clans with overflow kicks and stable equal-size
    ordering, the falsy empty `clanTag`, soft friend preference (absent IDs,
    null clientIDs, bidirectional edges, spill-when-full), the Duos/Trios/
    Quads largest-team mode, the nation shuffle seeded by `simpleHash` of the
    first nation's id (surrogate-pair ids included), duplicate team names
    merging by string equality, `getMaxTeamSize` `±Infinity`/`NaN`/`-0`
    edges, and every `resolveTeamsList` branch plus both `throw` paths. The
    result map's **insertion order** is compared entry by entry.
11. **`ConnectedComponents.ts`** (`pathfinding::connected_components`) is
    pinned with eleven op-trace scenarios over real `GameMapImpl` terrain
    bytes: two-blob labelling, the `accessTerrainDirectly` vs `isWater`
    premark paths (must agree byte-for-byte), queries before `initialize()`,
    bridging two components through one added land tile (the union-find alias,
    the moved size, the zeroed old size), an isolated crater (fresh-id
    alloc), a four-neighbour multi-merge onto the canonical root, edge
    guards on all four borders, a double-add no-op, out-of-range / fractional
    / negative refs, a path-compression chain, and the `Uint8Array`→
    `Uint16Array` upgrade at 253 components (with the land marker moving
    `0xFF`→`0xFFFF`). Each trace pins the final `componentIds` buffer, the
    sparse `_componentSizes` (JS holes → `NaN`), the union-find `parents`,
    `maxId` and `landMarker`.
12. **`TerrainSearchMap.ts`** (`terrain_search_map`) is pinned with six
    buffer-replay scenarios. The class decodes `width`/`height` from the
    buffer's first four bytes (`(d[1] << 8) | d[0]`, `(d[3] << 8) | d[2]`) and
    classifies the packed byte at `4 + y*width + x` (bit 7 → `Land`,
    magnitude `< 10` → `Shore`, else `Water`). The traces pin the JS-isms: a
    short buffer decodes the missing header bytes as `0` (so `height` is `0`,
    not a throw); out-of-range, negative, fractional and `NaN` `node()` reads
    hit `undefined`, whose `& 0x80` / `& 0x1f` are both `0` → `Shore`; a
    negative coordinate can land the index inside the header, where the
    header byte is read like any tile; the magnitude-10 boundary and bit-7
    dominance over a magnitude of 31; and `neighbors()` keeps fractional
    coordinates (the bounds test is purely relational) while `NaN` / `±Infinity`
    fail every comparison and yield the empty list, in the TS `dirs` order.
13. **`AbstractGraph.ts`** (`pathfinding::abstract_graph`) is pinned with six
    build-and-query scenarios (single cluster, all-water 8×8, a cross-shaped
    channel, a checkerboard of isolated tiles, all-water 12×12, and a partial
    rebuild reusing the 12×12 graph with four dirty minimap tiles). Every
    ordering decision of the builder is observable through the recorded
    access-op stream (15 kinds: node/edge/cluster lookups, component queries,
    path-cache get/set) plus the final flattened `_nodes`/`_edges`/`_clusters`/
    `_nodeEdgeIds` arrays: gateway nodes sit at `spanStart + floor(spanLength/2)`
    of each contiguous boundary water span, right edge before bottom edge in
    row-major cluster order; `tileToNode` dedupes corner gateways; edge ids
    follow the bounded BFS *find* order (JS `Map` insertion order, modelled as
    a `Vec` where a repeated set updates in place); `addOrUpdateEdge`
    canonicalises `(lo, hi)` and rewrites the cluster attribution only on a
    strict cost improvement; and the partial-rebuild path recreates clean
    clusters' edges from the old graph keyed by `(minTile, maxTile)`, keeping
    the original cluster. `getOtherNode` on a missing edge mirrors the TS
    throw (captured as `"t"`, the wasm probe returns `NaN`).
14. **`AStar.WaterBounded.ts`** (`pathfinding::water_bounded`) is pinned with
    eleven scenarios (nine `findPath`, two direct `searchBounded` with
    explicit bounds). The class indexes all four stamp-tracked arrays by a
    *window-local* id inside the start/goal hull, so the port pins the mapping
    itself: a same-row start/goal restricts the window to one row — the
    shallow-band case that `AStarWater` detours around here goes straight
    through (`wb_shallow_detour`), and a routable land-wall case becomes
    `null` (`wb_unreachable`); `maxSearchArea` below `numLocalNodes`
    short-circuits before searching (`wb_too_small`); an explicit window that
    excludes the goal *clamps* it to the window corner and reaches the
    clamped tile instead (`wb_clamped_goal`); inverted bounds (`min > max`)
    make `boundsWidth` negative, `numLocalNodes` 1, and the degenerate window
    returns the single clamped tile (`wb_inverted_bounds`). The magnitude
    curve also differs from `AStarWater` (`< 3` costs 300, not 1000), and the
    defaults are `heuristicWeight ?? 3` / `maxIterations ?? 100_000`.
15. **`AStar.AbstractGraph.ts`** (`pathfinding::abstract_graph_astar`) is
    pinned with ten scenarios over hand-built abstract graphs, snapshotting
    the FULL engine state (stamp, five node arrays, live heap) after *every*
    query. `gScore` is a **`Float32Array`** — stores round through f32 and the
    relaxation comparison reads the rounded value, so `0.1+0.2` vs `0.2+0.1`
    pick different winners than an f64 engine would (`aga_f32_round`); the
    goal/start node checks run *before* `queue.clear()`, so a missing node
    leaves the previous search's heap observable (the second query of both
    `aga_f32_round` and `aga_missing_goal`); a single-element start array
    delegates to the single-source path — stamp consumed, `startNode` never
    written (`aga_multi_single`); an empty array returns `null` before the
    stamp bump (`aga_multi_empty`); multi-source propagates each node's origin
    start through `startNode` (`aga_multi_ring`). Defaults:
    `heuristicWeight ?? 1` / `maxIterations ?? 100_000`; the heap is sized
    `numNodes + edgeCount * 2`.
16. **`AStar.WaterHierarchical.ts`** (`pathfinding::water_hierarchical`) is an
    *orchestrator* — it composes `BfsGrid`, `AbstractGraphAStar` and three
    `AStarWaterBounded` engines over a shared map + `AbstractGraph`. The parity
    target is the **dispatch**, so every scenario snapshots all five engine
    stamps after each query: a port that takes a different branch (early exit
    vs node lookup vs abstract stitch, short-path vs abstract winner, cache hit
    vs recompute) diverges even when the final path happens to match. The
    capture builds the real graph with `AbstractGraphBuilder` (no hand-built
    graphs). Pinned behaviours: the `dist <= clusterSize` early exit runs a
    *3×3-cluster* local search and returns it only on success (`wh_all_water`
    q1); a same-node pair with `dist > clusterSize` falls through to the
    `findNearestNode` BFS + same-node local path (q3); the abstract stitch
    concatenates a start segment, one `slice(1)`-joined segment per edge, and
    an end segment, with `findLocalPath`'s endpoint fix (`unshift(from)` /
    `push(to)`) restoring gateway tiles that sit outside the clamped window
    (`wh_gap`, whose 40×40 wall forces the early exit to fail into a real
    multi-edge stitch); `options.cachePaths` gates a direction-aware edge cache
    stored *on the graph* — `wh_cache` runs forward, reverse (opposite
    direction → recompute), then forward again (every segment hits cache, so
    the local engines' stamps stay frozen while the BFS still advances);
    `findPathMultiSource` tries the short-path engine first (candidates within
    120 tiles, padded window) and, on failure, resolves each source to its
    cluster node (closest source per node, JS-`Map` insertion order), runs
    multi-source abstract A*, and re-runs single-source from the winning tile
    (`wh_gap` q3/q4 prove the winner is the *closest* source regardless of
    array order); `setGraph` recreates only the abstract engine — its stamp
    resets to 1 while the map-sized engines keep counting (`wh_setgraph` q2).
    The `DebugSpan` instrumentation is disabled in the capture (a transparent
    `wrap`), so it is omitted here.
17. **`PathFinder.Parabola.ts`** (`pathfinding::parabola`) is the ballistic
    arc pathfinder. `getParabolaControlPoints` lifts the two middle control
    points by `heightMult * max(distance/3, 50)` and clamps them into the map
    with the NaN-propagating `within` — unless `ignoreMapBounds` lets them
    escape, in which case the *emission* `gameMap.ref(floor(x), floor(y))`
    throws in TS / panics in Rust (pinned by `pb_ignore_oob`: findPath throws
    on the first point, `next` throws at step 2 with the partial curve index
    recorded). `ParabolaUniversalPathFinder` caches one curve keyed by
    `lastTo`: every scenario drives ONE finder instance, so goal switches
    rebuild (`pb_rebuild` pins the index reset), `invalidate` drops the curve
    (`currentIndex` → 0 until the next call rebuilds), and a drained walk
    plateaus at `COMPLETE` forever (`pb_default`'s 45-step tail). `speed`
    defaults to 1 and feeds `DistanceBasedBezierCurve.increment`'s fixed-point
    accumulator (`pb_options` pins 0.5 rounding up to a full step and 25
    jumping several indices). The array-`from` `findPath` throw is a
    signature-level guard the Rust API cannot express (single-ref parameter),
    so it is not captured.

Regenerate whenever a ported source changes:

```
node rust/tools/gen_vectors.mjs
```

Requires a working Node (v20+) in the repo; the script imports `src/core/*.ts`
directly via Node's type stripping.

## Building and testing

Rust 1.85+ (verified on 1.98).

```
cd rust
cargo test
```

### Linker-free execution (used on this machine)

This machine has the VS C++ tools but **no Windows SDK**, so native test
binaries cannot link. The same parity check still *executes* end-to-end by
compiling the crate to wasm and driving it from Node:

```
cargo build -p openfront-core --target wasm32-unknown-unknown \
    --release --features wasm-probe
node rust/tools/run_wasm_parity.mjs
```

`wasm-probe` exposes the ported functions through `extern "C"` scalar
entrypoints (`src/wasm_probe.rs`); the runner imports `data/vectors.json` and
compares every value. Last run: **30,494 comparisons, all bit-identical**.

### Windows: the linker environment

The `x86_64-pc-windows-msvc` target needs the MSVC linker **and** the Windows
SDK import libraries (`kernel32.lib` et al.). A plain shell usually has neither
on `PATH`, so `cargo-vs.bat` calls `vcvars64.bat` before invoking cargo — pass
through whatever you want:

```
rust\cargo-vs.bat test
```

If you see `LNK1181: cannot open input file 'kernel32.lib'`, the VS C++ tools
are present but the **Windows SDK is not installed**. Fix with either:

* `winget install Microsoft.WindowsSDK` (or add "Windows 11 SDK" through the
  Visual Studio Installer to your existing VS instance), or
* install `x86_64-pc-windows-gnu` with a MinGW-w64 toolchain instead.

`cargo check --all-targets` works without any of that, since it never links.

## Porting rules (for whoever adds the next module)

1. No platform transcendental in simulation logic. Use `detmath`, or port the
   function the same way; do not call `f64::exp` / `f64::ln` / `f64::atan2`.
2. Transcribe, don't simplify. Association order, `div` vs `mul` by a
   reciprocal, and each rounding step are all observable in the last bits.
3. JS numeric coercions need explicit ports: `| 0` is ToInt32 (see
   `js_to_int32_bits`), `>>>` is a *logical* shift on the 32-bit pattern, and
   `Math.floor` differs from Rust's `floor` only for `-0.0`/`NaN` edge cases.
4. `Set`/`Map` iteration order is insertion order in JS. If a module relies on
   `randFromSet`, mirror it with an insertion-ordered container and use
   `PseudoRandom::rand_index`, which reproduces the index selection exactly.
5. Integer overflow must not be silently different: the workspace profile sets
   `overflow-checks = true`, and JS-style wrapping needs `wrapping_*`.

## Suggested next modules

Roughly in order of leverage, all currently reachable from the ported layer:

| Module | TS source | Why next |
|---|---|---|
| `execution/**` scheduler layer | `src/core/execution/**` | The turn/intent pipeline the game logic runs on; leans on the now-ported `Util`, `GameMap` and pathfinding. |
| `game/Game.ts` types | `src/core/game/Game.ts` | `Cell`/`Unit`/enum surface most `game/**` modules import. |
| `game/TrainStation.ts` | `src/core/game/TrainStation.ts` | `Cluster`/reservoir-sampling logic, but the stop handlers need the whole `Game`/`Player`/`TrainExecution` graph. |
| `game/schemas/**` | `src/core/game/schemas/**` | The `zod` schema layer most `game/**` state types are defined against; needs a serde/zbin story. |

`execution/**` and `game/**` are the bulk (~500 files, heavy on `zod` schemas,
`ApiSchemas.ts`, and worker IPC) and will want a serde/zbin schema story before
they move.
