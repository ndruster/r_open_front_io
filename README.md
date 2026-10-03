# Rust port of `src/core` + `src/server`

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
│   │   ├── util.rs                port of Util.ts (deterministic core +
│   │   │                          clan-tag / lobby-label sanitizers +
│   │   │                          dist-sort comparators)
│   │   ├── team_assignment.rs     port of game/TeamAssignment.ts
│   │   ├── line.rs                port of utilities/Line.ts
│   │   │                          (DistanceBasedBezierCurve)
│   │   ├── veterancy.rs           port of game/Veterancy.ts
│   │   │                          (maxHealthWithVeterancy)
│   │   ├── motion_plans.rs        port of game/MotionPlans.ts
│   │   │                          (packMotionPlans / unpackMotionPlans)
│   │   ├── terrain_search_map.rs  port of game/TerrainSearchMap.ts
│   │   │                          (SearchMapTileType + node / neighbors)
│   │   ├── anon_names.rs          port of AnonNames.ts (ANON_WORDS +
│   │   │                          anonWordName)
│   │   ├── close_codes.rs         port of CloseCodes.ts (tables +
│   │   │                          isTerminalClose / isCloseReason)
│   │   ├── server_list.rs         port of ServerList.ts (commit/site
│   │   │                          predicates + version routing; zod
│   │   │                          schemas not ported)
│   │   ├── pattern_decoder.rs     port of PatternDecoder.ts (packed-pattern
│   │   │                          header decode + isPrimary bit lookup;
│   │   │                          base64url layer not ported)
│   │   ├── doomsday_clock.rs      port of game/DoomsdayClock.ts (wave
│   │   │                          schedule math + rot noises + drain curves)
│   │   ├── exec_util.rs           port of execution/Util.ts (pure-GameMap
│   │   │                          subset: nuke blast counts, spawn tiles,
│   │   │                          nearest-tile searches, closest-two sweep)
│   │   ├── water_manager.rs       port of game/WaterManager.ts (water-nuke
│   │   │                          terrain fixup + throttled minimap water-graph
│   │   │                          rebuild; graph/HPA bypass, persistent CC)
│   │   ├── game_update_utils.rs   port of game/GameUpdateUtils.ts (per-player
│   │   │                          PlayerUpdate diff / in-place state merge /
│   │   │                          attack troop-delta packing; refid models the
│   │   │                          `a === b` fast path)
│   │   ├── railroad.rs            port of game/Railroad.ts (closest-tile
│   │   │                          index geometry, oriented-railroad lookup
│   │   │                          + reversal, delete's update/call sequence;
│   │   │                          stations and rails cross by refid)
│   │   ├── railroad_spatial_grid.rs port of game/RailroadSpatialGrid.ts
│   │   │                          (cellSize buckets keyed by `${cx}:${cy}`;
│   │   │                          register/unregister/query + insertion-ordered
│   │   │                          Map/Set dumps, rails keyed by refid)
│   │   ├── tile_traversal_scratch.rs port of game/TileTraversalScratch.ts
│   │   │                          (per-game generation-stamped visited buffer
│   │   │                          + stack + cluster map, WeakMap-cached by
│   │   │                          game refid; ToIndex allocation, shrink-keeps
│   │   │                          reuse, 0xffffffff bump wrap)
│   │   ├── event_bus.rs           port of EventBus.ts (Map<ctor, callbacks[]>;
│   │   │                          on/off/emit + Map-order dump, ctors/cbs/events
│   │   │                          cross by refid, emit pinned as the callback
│   │   │                          call trace)
│   │   ├── asset_urls.rs          port of AssetUrls.ts (normalizeAssetPath /
│   │   │                          encodeAssetPath / buildAssetUrl; percent-decode
│   │   │                          + `.`/`..` guards, any-scheme absolute-URL
│   │   │                          regex, manifest/baseUrl join; throws as [1])
│   │   ├── maps_gen.rs            port of game/Maps.gen.ts (pure data tables:
│   │   │                          GameMapType 127 string-enum members,
│   │   │                          mapCategoryOrder, 127 MapInfo records; data
│   │   │                          region generated by tools/gen_maps_rs.mjs)
│   │   ├── tribe_names.rs         port of execution/utils/TribeNames.ts
│   │   │                          (resolveTribeNameData: theme merge, default
│   │   │                          fallback, console.warn edges; THEMES table
│   │   │                          generated by tools/gen_maps_rs.mjs)
│   │   ├── game_ts.rs             port of game/Game.ts (runtime-value subset:
│   │   │                          12 enums, unitTypeGroup tables, isEnumValue
│   │   │                          guards, message categories, Cell / PlayerInfo,
│   │   │                          bulk-cost math; interfaces are type-only)
│   │   ├── nation_creation.rs     port of game/NationCreation.ts (name
│   │   │                          templates + noun bank, pluralize, unique
│   │   │                          name generation, createRandomNations)
│   │   ├── game_updates.rs        port of game/GameUpdates.ts (GameUpdateType
│   │   │                          24-member numeric wire-tag enum + the
│   │   │                          GameImpl.ts createGameUpdatesMap traverse-
│   │   │                          filter-write over the reverse-mapped enum)
│   │   ├── nation_emoji.rs        port of Util.ts emojiTable /
│   │   │                          flattenedEmojiTable + NationEmojiBehavior.ts
│   │   │                          EMOJI_* constants
│   │   ├── water_path_memo.rs     port of pathfinding/PathFinder.ts
│   │   │                          (WaterPathMemo only: LRU byte-budget memo
│   │   │                          over a scripted inner pathfinder)
│   │   ├── cosmetic_schemas.rs    port of CosmeticSchemas.ts (runtime-value
│   │   │                          subset: EFFECT_TYPES / TRAIL_EFFECT_TYPES /
│   │   │                          NUKE_EXPLOSION_TYPES, the four effect/slot
│   │   │                          predicates, DefaultPattern; zod schemas
│   │   │                          inert, findEffect* not ported)
│   │   ├── stats_schemas.rs       port of StatsSchemas.ts (runtime-value
│   │   │                          subset: bombUnits / boatUnits / otherUnits,
│   │   │                          the two UnitType->short-name tables, the 34
│   │   │                          numeric index consts, toBigInt coercion;
│   │   │                          zod/zbin schemas inert, UnitType inlined)
│   │   ├── schemas.rs             port of Schemas.ts (runtime-value subset:
│   │   │                          the five z.enum option arrays, lobby
│   │   │                          constants, LogSeverity, QuickChat keys,
│   │   │                          GAME_ID / renderable-name predicates;
│   │   │                          zod schemas inert except z.enum options)
│   │   ├── api_schemas.rs         port of ApiSchemas.ts (runtime-value
│   │   │                          subset: ADMIN_ROLES / PlayerStatsGameModes
│   │   │                          / the two filter arrays, ten z.enum option
│   │   │                          arrays, isAdminRole / isTemporaryUsername /
│   │   │                          isVerifiedUsername / isGrantedSubscription;
│   │   │                          zod schemas inert, Base64 stubbed)
│   │   ├── terrain_map_loader.rs  port of TerrainMapLoader.ts (loadTerrainMap
│   │   │                          orchestration with loadImages=false: the
│   │   │                          loadedMaps cache, the dead mini-map ternary,
│   │   │                          Compact in-place nation / spawn-area scaling,
│   │   │                          layer placement / alpha validation,
│   │   │                          genTerrainFromBin buffer-size check)
│   │   ├── nation_utils.rs        port of execution/nation/NationUtils.ts
│   │   │                          (randTerritoryTileArray / randTerritoryTile
│   │   │                          sampling + findJuiciestTarget normalize /
│   │   │                          strict-gt best scan; Game / Player /
│   │   │                          PseudoRandom facades scripted-mocked with
│   │   │                          a pinned call trace)
│   │   ├── terra_nullius.rs       port of game/TerraNulliusImpl.ts (stateless
│   │   │                          neutral player: smallID=0 / clientID
│   │   │                          literal / id()=null / isPlayer()=false)
│   │   ├── stats_impl.rs          port of game/StatsImpl.ts (pure bigint
│   │   │                          stats accumulator; Player facade
│   │   │                          scripted-mocked with a pinned call trace)
│   │   ├── unit_grid.rs           port of game/UnitGrid.ts (100-cell 2-D
│   │   │                          spatial index over the real GameMap;
│   │   │                          Unit facade scripted-mocked with a pinned
│   │   │                          call trace)
│   │   ├── shared_water_cache.rs  port of execution/nation/SharedWaterCache
│   │   │                          .ts (nation-AI shared-water TTL cache;
│   │   │                          Game / Player facades scripted-mocked with
│   │   │                          a pinned call trace)
│   │   ├── execution_manager.rs   port of execution/ExecutionManager.ts
│   │   │                          (Executor intent dispatcher; the 24
│   │   │                          Execution classes stubbed as construction
│   │   │                          recorders over a pinned trace)
│   │   ├── station_manager.rs     port of game/RailNetworkImpl.ts
│   │   │                          (StationManagerImpl only; count()=nextId
│   │   │                          quirk, sparse stationsById, Set order)
│   │   ├── train_station.rs       port of game/TrainStation.ts
│   │   │                          (TrainStation graph node + Cluster;
│   │   │                          Unit/Game/Player facades scripted-mocked,
│   │   │                          stop-handler surface excluded)
│   │   ├── rail_network.rs        port of game/RailNetworkImpl.ts
│   │   │                          (RailNetworkImpl orchestration +
│   │   │                          createRailNetwork factory; Game/config/
│   │   │                          pathService/unit facades scripted-mocked,
│   │   │                          RailPathFinderServiceImpl excluded)
│   │   ├── js_json.rs             JS JSON.stringify fidelity helper
│   │   │                          (+ JsVal tri-state codec, shared by
│   │   │                          the server ports)
│   │   ├── vote_tally.rs          port of server/VoteTally.ts
│   │   │                          (VoteRound: IP-weighted majority)
│   │   ├── config_patch.rs        port of server/ConfigPatch.ts
│   │   │                          (applyGameConfigPatch + hostCheatsEnabled)
│   │   ├── intent_authorization.rs port of server/IntentAuthorization.ts
│   │   │                          (authorizeIntent guard table)
│   │   ├── consensus.rs           port of server/Consensus.ts
│   │   │                          (WinnerVote + LiveStatsVote on VoteRound)
│   │   ├── wasm_probe.rs          `extern "C"` surface, feature-gated
│   │   └── pathfinding/
│   │       ├── mod.rs
│   │       ├── priority_queue.rs  port of algorithms/PriorityQueue.ts
│   │       ├── flat_heap.rs       port of execution/utils/FlatBinaryHeap.ts
│   │       ├── bfs_grid.rs        port of algorithms/BFS.Grid.ts
│   │       ├── bfs.rs             port of algorithms/BFS.ts (generic BFS)
│   │       ├── air.rs             port of PathFinder.Air.ts (air walk)
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
│   │       ├── mini_map_transformer.rs
│   │       │                      port of transformers/MiniMapTransformer.ts
│   │       │                      (downscale/upscale decorator + PathFinder
│   │       │                       trait + PathStart union)
│   │       ├── stepper.rs
│   │       │                      port of PathFinderStepper.ts
│   │       │                      (path cache + stepping wrapper)
│   │       ├── component_check_transformer.rs
│   │       │                      port of transformers/ComponentCheckTransformer.ts
│   │       │                      (same-component filtering decorator)
│   │       ├── shore_coercing_transformer.rs
│   │       │                      port of transformers/ShoreCoercingTransformer.ts
│   │       │                      (shore->water coercion + endpoint restore)
│   │       ├── smoothing_water_transformer.rs
│   │       │                      port of transformers/SmoothingWaterTransformer.ts
│   │       │                      (LOS smoothing + local-A* endpoint refinement)
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
   (Array, Set, TileSet). Results compare by IEEE-754 bit pattern. The file's
   remaining pure functions (the clan-tag / lobby-label sanitizers and the
   dist-sort comparators, 61 more scenarios) are pinned in item 44.
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
18. **`MiniMapTransformer.ts`** (`pathfinding::mini_map_transformer`) is the
    minimap decorator: downscale `floor(map.x(f)/2)` to the mini map, delegate
    to `inner`, upscale by 2 with `Math.round` interpolation between
    consecutive mini tiles, then `fixExtremes` (unshift/slice the source,
    push/truncate the destination). The TS `TileRef | TileRef[]` union is
    modelled by `PathStart` because the distinction is *observable*: a
    single-element start array collapses to a scalar for `inner` yet still runs
    the multi-source closest-start branch (`Array.isArray`), and each engine
    normalises the union differently (`AStar`/`AStarWater`/`AStarWaterBounded`
    collapse `[s]` to `s`; `AStarWaterHierarchical` routes it to the
    multi-source path). The scripted `ScriptedFinder` stub records what the
    transformer passed `inner` so the collapse + downscale are pinned; the
    six scenarios cover null/empty/path results, all four `fixExtremes`
    branches, the Manhattan closest-start with strict `<` tie order, the empty
    array start (`srcRef` stays undefined), and both `ref` throw classes —
    downscale OOB (inner never called) and upscale OOB via an oversized mini
    map — pinned as panics natively and would-throw markers under wasm.
19. **`PathFinderStepper.ts`** (`pathfinding::stepper`) is the generic
    stepping wrapper: path cache + `pathIndex` advance, `lastTo` invalidation
    on goal change, stray-from-route recompute, and the `path[0] === from`
    start-skip. The port pins `T = TileRef` and the two production config
    shapes (`tileStepperConfig` vs the default bare `{equals: ===}`) via an
    `Option<&GameMap>` selector. JS-isms pinned: numeric paths stored through
    `to_uint32` (`new Uint32Array`), the `pathIndex > 0` short-circuit that
    hides the `path[-1]` → `undefined` comparison, the recursive `next`
    re-entry after `invalidate()` (modelled as a loop), `pathAfterNext()`
    null-while-idle, and the `findPath` vacuous-`every` short-circuit (empty
    start array → `null`, inner never called). The `SharedStub` inner finder
    counts calls so "inner untouched" (pre-check hit, cache hit, vacuous
    `every`) is observable; the two scenarios cover the pre-check NOT_FOUND,
    `from === to`, distance early exit, the full drain to COMPLETE, the stray
    recompute to NOT_FOUND, multi/scalar passthrough and the three allFailed
    short-circuits.
20. **`ComponentCheckTransformer.ts`** (`pathfinding::component_check_transformer`)
    is the fail-fast decorator: it keeps only the starts whose component id
    matches the goal's and delegates those (single survivor collapses to a
    scalar start), returning `null` when none match — including the vacuous
    empty-array case — without ever calling `inner`. The injected
    `(t) => number` getter is a trait so the production
    `graph.getComponentId` shape and a table stub both fit; the pass-through of
    `inner`'s result is pinned via the recorded `PathStart` (kind + filtered
    tiles + goal) and "inner never called" via the seen flag.
21. **`ShoreCoercingTransformer.ts`** (`pathfinding::shore_coercing_transformer`)
    is the water decorator: shore starts/goals are coerced to their best
    adjacent water tile (scored by water-4-neighbour count, strict `>` so ties
    keep the first in `neighbors4` order), the inner finder runs on water
    tiles, then the original shore start is unshifted back and the original
    goal appended (unless already the last tile). JS-isms pinned: the
    `waterToOriginal` `Map` semantics — a raw water source `delete`s an earlier
    coercion to the same tile (no restore) and two shores coercing to the same
    water tile overwrite (last write wins) — both leaving duplicate starts
    visible to `inner`; the single-water-source scalar collapse; and the
    `!path || path.length === 0` → `null` short-circuit.
22. **`SmoothingWaterTransformer.ts`** (`pathfinding::smoothing_water_transformer`)
    is the water-route smoother decorator: `inner`'s path goes through three
    passes — LOS binary-search smoothing (min magnitude 2), endpoint
    refinement via a local `AStarWaterBounded` over the first/last ~50
    manhattan tiles (padded 10-tile window), and LOS smoothing again (min
    magnitude 3). JS-isms pinned: the empty-inner result is truthy so `[]`
    flows through `smooth` (length `<= 2` early return) instead of becoming
    `null`; `terrain[tile] & 0x1f` reads the packed buffer with `Uint8Array`
    OOB semantics (undefined → magnitude 0, failing every gate); the
    diagonal Bresenham step validates the intermediate tile and rolls back to
    the alternative on failure — `canSee` gates both traversable *and*
    magnitude, `tracePath` only traversable (pushing the detour tile it
    validated); `refineSegment` clamps its window with NaN-propagating
    `js_max`/`js_min`; the injected `(tile) => boolean` predicate is a trait
    (`WaterTraversable` mirrors the `map.isWater` default). The six scenarios
    pin the LOS collapse + trace splice, the refinement hit (a rerouted
    zig-zag) and miss (isolated tiles keep the inner path verbatim), the
    >50-tile partial splice, and the pass-3 subtlety where a failing
    `canSee` still leaves `farthest` at its `lo` initialisation — so a
    shallow band blocks smoothing without shortening the path.
23. **`BFS.ts`** (`pathfinding::bfs`) is the generic graph BFS with a visitor
    callback (`T = f64`, adapter trait `BfsAdapter::neighbors`). JS-isms
    pinned: the starts loop does `visited.add(s); queue.push(s)` unconditionally,
    so a **duplicate start is a Set no-op yet still enqueued** — the visitor
    sees that node once per occurrence; the tri-state return treats any
    non-null/non-undefined value (including falsy `0`) as found; the depth
    clip is a JS `>` comparison, **false against `NaN`**, so a NaN bound never
    clips (a `!(<=)` rewrite would diverge); `visited` is a `Set` with
    SameValueZero (`NaN` dedupes against `NaN`, `+0`/`-0` are one key); and
    neighbours are marked visited at *enqueue* time, collapsing self-loops and
    repeated neighbours. The twelve scenarios pin the duplicate-start double
    visit, the falsy-found short-circuit, the reject-without-expansion dist
    shift, the NaN bound, NaN-node dedup, and the empty-start no-op.
24. **`PathFinder.Air.ts`** (`pathfinding::air`) is the deterministic aircraft
    walk: each step nudges one tile toward the goal, picking the X or Y axis by
    a seeded `chance(ratio)` coin whose bias is the destination slope
    (`floor(1 + |dy| / (|dx| + 1))`). JS-isms pinned: `game.ticks()` seeds the
    generator **once at construction**, and every `findPath` builds a *fresh*
    `PseudoRandom(this.seed)`, so repeated queries replay the identical stream;
    the `Array.isArray(from)` guard **throws** (the union is not collapsed,
    unlike the grid engines) — modelled as a panic pinned by `catch_unwind`;
    `game.ref(nextX, nextY)` **throws** on an out-of-range step; and the loop
    guard `next === current` is strict `f64` equality. The `y === dstY` and
    `chance` X-move branches share a body, merged with `||` short-circuit so
    `chance` is drawn only when `y !== dstY` (the TS order). The ten scenarios
    pin the same-tile single path, the pure-axis walks (no random draw), four
    seed-varied diagonal interleavings, the multi-start throw, and the
    out-of-range `game.ref` throw.
25. **`AnonNames.ts`** (`anon_names`) is the anonymous-handle generator:
    `anonWordName(slot, offset = 0)` truncates both inputs toward zero
    (`Math.trunc`), takes their absolute values, indexes the 125-word bank by
    `(s + o) % 125` (JS sign-of-dividend remainder; IEEE `fmod` twin) and
    suffixes `floor(s / 125)` once the bank wraps. JS-isms pinned: the bank
    lookup is a **property access**, so `NaN` / `±Infinity` indices read
    `undefined` — with `round === 0` the TS returns that `undefined` *value*
    (modelled as `Option::None`, the `: string` annotation is a runtime lie),
    while `round !== 0` the template literal spells `"undefined"` into the
    handle; `-0` hits index 0 (key stringifies to `"0"`) and `floor(-0 / 125)`
    is `-0`, which strict-equals `0` so the bare-name branch wins; `${round}`
    prints plain decimal digits below `1e21`, `NaN` / `Infinity` otherwise.
    The 165 scenarios sweep the full bank, round suffixes, negative/fractional
    slots and offsets, every non-finite combination, and huge slots up to
    `2^53`.
26. **`CloseCodes.ts`** (`close_codes`) is the WebSocket close-code /
    close-reason vocabulary: the 16-entry `CloseCode` table and 23-entry
    `CloseReason` table (declaration order pinned) plus the two predicates.
    JS-isms pinned: `isTerminalClose` is raw IEEE `f64` comparison — `NaN`
    and `±Infinity` are never terminal, `-0` is `==` to `0` but matches no
    listed code, and a **fractional** code inside `[4000, 4999]` (e.g.
    `4000.5`) *is* terminal because the range test never casts to integer;
    `isCloseReason` is `Set#has` string equality, so case and whitespace
    near-misses are `false`. The 57 scenarios sweep every declared value,
    the range boundaries, the fractional / zero / negative / non-finite code
    edges, and the reason membership edges.
27. **`ServerList.ts`** (`server_list`) is the server-discovery helper set:
    commit/site shape predicates, `commitsMatch` / `versionMatches` prefix
    rules, build-aware letter filtering (`servesBuild`,
    `pickServerForBuild`), `ownLetterIn` host precedence, and the
    `/v/<commit>/` URL rewrites (`stripVersionPrefix`, `shortCommit`,
    `versionedPath`, `versionedPathForGame`). The zod schemas are wire
    validation at the API boundary and are **not** ported. JS-isms pinned:
    the regexes are hand-transcribed (`{7,40}` counts UTF-16 units, `\d` is
    ASCII-only, `[^/]+` / `[^/?#]+` stop at the first delimiter);
    `pickServerForBuild` sends `NaN` / `±Infinity` / fractional picks to
    index `0` and clamps integers into range; `ownLetterIn` lowercases hosts
    with Unicode-aware `toLowerCase`; `pathNamesGame` decodes the id with a
    JS-accurate `decodeURIComponent` whose malformed escapes (`%`, `%zz`,
    overlongs, encoded surrogates) fall back to the raw segment. The 150
    scenarios sweep the predicate edges, the prefix/identity matrix, the
    open/draining/fenced pick table with every clamp kind, the host/letter
    precedence, and the version-URL loop guards.
28. **`PatternDecoder.ts`** (`pattern_decoder`) decodes the packed
    player-cosmetic pattern header (version / scale / width / height guards)
    and answers `isPrimary(x, y)` from the bitmap. The injected
    `base64urlDecode` is external codec, **not** ported — the Rust twin takes
    the decoded bytes directly and the capture encodes them to base64url so
    the real TS runs. JS-isms pinned: `x >> scale` goes through `ToInt32`
    both sides (huge / fractional / `NaN` / `±Infinity` coords wrap), `%`
    keeps the sign of the dividend so negative cells read *backward* into the
    3-byte header (e.g. `bytes[2]`), and only indices fully outside
    `[0, len)` throw (`bytes[3 + byteIndex] === undefined`). The three decode
    throws and the bounds throw are pinned as distinct codes 1–4. The 54
    scenarios sweep the guard order, every `scale` value, the width/height
    bit packing (incl. the 7-bit width high field), metadata-region negative
    reads, and the `ToInt32` coordinate wrap edges.
29. **`DoomsdayClock.ts`** (`doomsday_clock`) is the wave-schedule threshold
    math: `requiredBasisPoints` / `doomsdayClockRequiredTiles` (grace, linear
    ramps, flat pauses, the team ladder swap), the HUD `waveState`, the troop
    floor / convex drain curves, the R2-lattice `rotSpeckleNoise` and hashed
    `rotFrontNoise`, and `rotQuota`. JS-isms pinned: `Math.imul` →
    `i32::wrapping_mul` with `ToInt32` on the constants (`R2_X` / `R2_Y`
    exceed `INT32_MAX` and wrap negative), the three int32 products sum in
    `f64` before `>>> 0`, `Math.max` is the NaN-propagating [`js_max`], the
    convex curve's loop counter stays `f64` so fractional exponents iterate
    like the JS `for` loop (`NaN` exponent runs zero times, `Infinity` hangs
    the TS side and is not captured), and unknown speed codes fall back to
    `normal` via `??`. The 1,270 scenarios sweep every ramp/pause boundary
    of all four speeds, both ladders, the int32 coercion edges of the
    noises, and the floor/drain clamp matrix.
30. **`execution/Util.ts`** (`exec_util`) is the pure-`GameMap` subset:
    `computeNukeBlastCounts` (insertion-ordered owner weights over
    `circleSearch`, `d2 <= inner² ? 1 : 0.5`), `getSpawnTiles` (the centred
    radius-4 Euclidean stack-BFS, strict vs. filtered overloads), `closestTile`
    / `nearestTileDist` (first-minimum strict `<`, `Infinity` on empty),
    `nearestTileDistCapped` (the `isTileSetLike` duck test picks the Manhattan
    ring walk over a `TileSet` probe vs. the clamped linear scan) and
    `closestTwoTiles` (stable sort by `a % w` — a `NaN` comparator counts as
    equal, per V8 `SortCompare` — then the two-pointer sweep). The three
    `Game`-facade functions in the same file (`wouldNukeBreakAlliance`,
    `listNukeBreakAlliance`, `calculateTerritoryCenter`) are out of scope until
    `Game.ts` lands. 80 scenarios pin the owner-weight insertion order, the
    strict/loose spawn split over owned / impassable / water tiles, the NaN
    distance edges, both capped branches (an `Infinity` cap would spin the ring
    loop forever and hangs the TS side — not captured), and the tie / empty /
    NaN-column sort cases of the sweep.
31. **`game/WaterManager.ts`** (`water_manager`) — pending water-tile queue,
    the five-phase `finalizeWaterChanges` (ocean BFS, crater-grouped magnitude
    BFS with stamp/distance scratch arrays, shoreline 2-ring recompute, minimap
    2×2 majority fold with its own ocean + magnitude passes, and persistent
    `ConnectedComponents` `addWaterTile` labeling), the 20-tick throttled graph
    rebuild, and the per-tile component queries. The TS graph/HPA/BFSGrid
    machinery only ever delegates to the *same* persistent CC object (the
    builder's `sharedWaterComponents` path) and a rebuild's sole observable is
    `waterGraphVersion++` + dirty-tile clear — so the port keeps the CC
    directly and models rebuilds as a version bump (the Rust
    `AbstractGraphBuilder` re-floods and renumbers per build, which would
    *break* parity). JS `Set`/`Map` insertion order is replicated by
    `OrderedSet`/`OrderedMap` (SameValueZero keys); the magnitude BFS reads
    packed terrain bytes and defers `setMagnitude` writes to the end of each
    update loop (each tile is read at most once inside the loop, so snapshot
    reads are identical). 10 scenarios cover the throttle boundary, owned /
    impassable / fallout skip-and-clear branches, single- and two-crater ocean
    folds, dense conversions, null-component queries, and a cross-tick CC
    merge; per-op result streams plus both maps' final terrain/state buffers
    and the version counter are compared.
32. **`game/GameUpdateUtils.ts`** (`game_update_utils`) — `diffPlayerUpdate`
    (field-by-field diff of the ~35 compared `PlayerUpdate` fields, emitted in
    `setIfDifferent` call order; the fast path shares the same comparators),
    `applyStateUpdate` (in-place merge where `undefined` = no-change, `null`
    is assigned — or coerced by `Number(null)=0` / `Math.max(0,·)` — and the
    three `.slice()`-detached arrays are copied), and `packAttackTroopDeltas`
    (membership-equality-gated `[owner, dir, index, troops]` quads). JS
    reference identity (`a === b`, true for a shared array even when it holds
    `NaN`) is modelled by a capture-assigned `refid` per distinct array
    object; three-state fields (`undefined` / `null` / value) are
    `Option<Option<T>>` so `===` keeps its JS distinctions (`null !==
    undefined`, `NaN !== NaN`, `-0 === 0`). 71 scenarios cover the shared-ref
    NaN fast path, set-semantic embargoes, troops-only attack changes,
    `null` vs `undefined` merges, `Math.max` NaN/negative/`-0`/Infinity edges,
    and the pack gate's reference / membership / length branches.
33. **`game/Railroad.ts`** (`railroad`) — `getClosestTileIndex` (squared
    Euclidean distance over the tile list with JS-accurate `x = t % width` /
    `y = (t / width) | 0` decoding, strict `<` keeps the first on ties, `-1`
    when empty), `getOrientedRailroad` + `OrientedRailroad` (neighbor→rail
    lookup, direction decided by `railroad.to === to` reference identity,
    backward rails copy-and-reverse their tiles), and `delete` (emits the
    `RailroadDestructionEvent` update and calls `removeRailroad(this)` on
    `from` then `to`). Stations and rails are duck-typed stubs carrying
    capture-assigned refids; the mutation lives in `TrainStation` (a later
    port), so the parity surface is the call sequence. 20 scenarios cover
    empty / tie / NaN / ±Inf geometry, forward / backward / missing /
    parallel-rail orientation, and self-loop / negative-id deletes.
34. **`game/RailroadSpatialGrid.ts`** (`railroad_spatial_grid`) —
    `RailSpatialGrid`: a `cellSize x cellSize` spatial index over rail tiles,
    cells keyed by the `` `${cx}:${cy}` `` template string (`js_int_str`
    reproduces `String(Number)` — `-0`→`"0"`, `NaN`/`±Infinity` print their
    JS spellings). `register` (defensive `unregister` first, per-rail cell
    dedup, empty-tile rails left untracked), `unregister` (emptied cells are
    pruned, survivor order preserved) and `query` (nested `cx`-then-`cy`
    scan over the `[x±radius, y±radius]` box, union in insertion order) all
    ride on JS `Map`/`Set` insertion-order + object-identity semantics,
    modelled with ordered Vec/HashMap pairs keyed by capture refids. The
    constructor's `cellSize <= 0` throw is recorded as a `[1]` construct
    result (NaN does *not* throw — every cell collapses to `"NaN:NaN"`). 11
    scenarios cover the dumps (`debug_cells` / `debug_rail_cells`), re-
    registration, shared-cell ordering, cell sizes 1 / 2.5 / 100, and
    negative-radius empty queries.
35. **`game/TileTraversalScratch.ts`** (`tile_traversal_scratch`) —
    `tileTraversalScratch` + `bumpTraversalGeneration`: the shared per-game
    traversal scratch (a generation-stamped `Uint32Array` visited buffer, a
    reusable `stack`, an `Int32Array` cluster map) cached in a `WeakMap` keyed
    by the `Game` object — modelled by a capture-assigned game refid. The
    allocation runs JS `ToIndex` (`NaN`/`±0` → length 0, fractional truncates,
    negative / `≥ 2^53` / `±Infinity` → `RangeError`, recorded as a `[1]` op
    result); the reuse test `visited.length < totalTiles` keeps oversized
    buffers on a shrinking game and reallocates (resetting `gen`) on a growing
    one; `bump` wraps at `0xffffffff` with `visited.fill(0)` and `gen = 1`.
    Element access follows typed-array semantics (`ToUint32`/`ToInt32` writes,
    OOB writes dropped, OOB reads `undefined` → `NaN`). 9 scenarios cover
    allocate/reuse/shrink/grow, the bump wrap, distinct games, the throw and
    truncation boundaries, and typed-array OOB.
36. **`EventBus.ts`** (`event_bus`) — `on` / `off` / `emit`: the typed pub/sub
    bus over a `Map<EventConstructor, Array<callback>>`. Constructors,
    callbacks and event instances all ride in as capture-assigned refids (the
    `Map` keys on ctor identity, `off`'s `indexOf` on callback identity, and
    `emit` passes the event object through), so the parity surface is the
    ordered `[(callback, event)]` call trace `emit` produces plus the
    `Map`-insertion-order dump. Faithful JS edges: re-`on`-ing a known ctor
    appends without reordering the `Map`; `off` splices only the *first*
    `===` match (a duplicate callback survives one `off`); an absent ctor key
    is `undefined` and an empty array is truthy — both emit zero calls;
    SameValueZero keys collapse `-0`/`+0` refids. 5 scenarios cover fan-out
    order, duplicate-callback `off`, unknown-ctor no-ops, cross-ctor insertion
    order, and interleaved multi-ctor emits.
37. **`AssetUrls.ts`** (`asset_urls`) — `normalizeAssetPath` /
    `encodeAssetPath` / `buildAssetUrl`: the pure asset-path surface (the
    `window`/`globalThis` manifest readers and the `index.html` CDN rewrite are
    out of scope). `normalizeAssetPath` strips a leading `/+` run, drops empty
    segments, percent-decodes each survivor (`decodeURIComponent` with the raw
    segment as the `URIError` fallback — the decoder is shared with
    `server_list`) and rejects a segment when the raw or decoded form is `.` /
    `..`; the joined result can still carry decoded `//` or `.` segments
    (`a/%2E%2Fb` → `a/./b`), which is why `buildAssetUrl`'s encode fallback
    re-normalises and throws where `normalizeAssetPath` alone succeeded.
    `isAbsoluteUrl` is `/^[a-z][a-z0-9+.-]*:\/\//i` — the `i` flag folds only
    ASCII in V8 (`K`/`ſ`/`ı` variants fail the class; probed), so an exact
    byte-level prefix scan is equivalent. The manifest lookup is exact-key
    (prototype-chain `toString` hits are outside the domain, same exclusion as
    `server_list`), and a hit counts only when the value is truthy. A
    hand-written `encodeURIComponent` (unreserved set passthrough, uppercase
    `%XX` per UTF-8 byte; lone surrogates are unrepresentable in a Rust
    `String`) backs the encode. 72 scenarios over the three `run_op` kinds pin
    the decode fallbacks (`%zz`, `%`, `%c`, `%c3%28`, `%ed%a0%80`), the
    raw-vs-decoded `.`/`..` guards, the scheme-regex edges, and every
    manifest/baseUrl branch; throws cross as `[1]`.
38. **`Maps.gen.ts`** (`maps_gen`) — the generated map-metadata table, a
    pure-data module with no imports: the 127-member `GameMapType` *string*
    enum (member name = the folder id, value = the canonical wire name — the
    two differ for e.g. `Guanabara` → `"Rio de Janeiro"`), the 16-entry
    `mapCategoryOrder`, and the 127 `MapInfo` records. The Rust data region
    is emitted verbatim from the TS source by `tools/gen_maps_rs.mjs` (via
    the strip-mode loader, which inlines the enum as a plain object), and the
    parity surface is the `run_op` dump serialiser: kind 0 = full `maps`
    table, 1 = enum name/value pairs, 2 = category order, 3 = id lookup
    (`[0, record]` / `[1]`). Every optional field crosses behind an explicit
    presence flag so a real `-1` frequency never reads as absent; tribe
    `coordinates` tuples and the lone `layers` record (with its optional
    `nukeable`) round-trip the same way. 6 scenarios pin the whole table
    byte-for-byte (~20k tokens per maps dump).
39. **`TribeNames.ts`** (`tribe_names`) — `resolveTribeNameData(mapType?)`:
    map lookup is by the enum *value* (`m.type === mapType`, so `Guanabara`
    resolves only as `"Rio de Janeiro"`), missing/empty `themes` degrades to
    `["default"]`, an unknown theme emits the exact
    `[TribeNames] Map "<type>" references unknown tribe name theme "<name>".
    Skipping.` console.warn (interpolating the *value*, not the member name)
    and is skipped; an empty merged prefix **or** suffix appends the default
    theme (suffix may then repeat); a missing default throws; non-empty
    `customTribes` passes through verbatim. The 17-theme `THEMES` table is
    generated from `resources/tribeNameThemes.json` by
    `tools/gen_maps_rs.mjs`. The loader's prepare block redirects the JSON
    import to an attribute-imported copy and re-points the `Maps.gen` value
    imports at the prepared module. 15 scenarios pin the warn/throw edges by
    deleting or blanking theme entries at runtime (restored in `finally`),
    the default-fallback duplication, and the value-vs-id lookups; throws
    cross as `[1]`.
40. **`game/Game.ts`** (`game_ts`) — the runtime-value subset (the interfaces
    have no runtime and are not ported): the 9 string + 3 numeric enums as
    name/value tables, the 5 `unitTypeGroup`s (`BuildMenus` / `PlayerBuildable`
    keep their *spread* order — Structures then BuildableAttacks then
    TransportShip — plus the 16-column `has()` matrix), `isEnumValue` and its
    three guards (strict-equality scan over the *values* only),
    `MESSAGE_TYPE_CATEGORIES` + `getMessageCategory` (out-of-domain keys read
    `undefined` → `[0]`), `ColoredTeams`, `Cell` (template-literal `strRepr`
    through a JS `Number`→string: `-0` → `"0"`, `1.5` → `"1.5"`; the never-
    assigned `index` is not modelled), `PlayerInfo` (empty clanTag is falsy →
    bare name; the ctor defaults ride back as fixed tokens), and the bigint
    bulk-cost math (`bulkCost`'s `?.[amount-1] ?? cost·amount` fallback —
    `amount = 0` indexes `-1` — and `maxBulkAmount`'s break *before* pricing
    past `upgradeCosts`; values stay < 2^53 so they cross as f64 and compute
    in i64). The loader drops Game.ts's 13 type-only imports, inlines
    `formatPlayerDisplayName`, re-points the `Maps.gen` re-export at the
    prepared copy, inlines all 12 `export enum`s as plain objects, and
    expands the `Nation` / `Cell` / `PlayerInfo` parameter properties. 60
    scenarios pin every table, the has() matrices, guard negatives, the
    category miss keys (22 / -1 / 999), `Cell[0,1.5]`, unicode display names,
    and the bulk edges (amount 0, no-upgradeCosts linear to 50, break-at-
    length).
41. **`game/NationCreation.ts`** (`nation_creation`) — the nation-name
    machinery: the 194-entry `NAME_TEMPLATES` (the `NOUN` / `PLURAL_NOUN`
    symbols become enum variants; a full dump pins every literal and marker
    in order), the 236-word `NOUNS` bank *with* its intentional duplicates
    (Fullsender ×3 / Mito ×3 / Mitochondria ×3) and the `é` tail of
    "Soufflé", `O_TO_OES` / `SPECIAL_PLURALS` (Set/Map insertion order),
    `pluralize` with its exact branch order (special → s/ch/sh/x/z+es →
    consonant-y → ies → oes → s) and the UTF-16 quirk that a one-character
    `"y"` indexes `noun[-1]` = `undefined`, which `includes` coerces to
    `"undefined"` and still takes the `-ies` path. `generateNationName`
    draws template *before* noun; `generateUniqueNationName` retries 1000×
    then falls back to `base + " " + counter`. `getCompactMapNationCount`
    pins the 0 / floor(·0.25) / max(1,·) matrix. `createRandomNations`
    replays the exact RNG draw order (shuffle → one `nextID()` per manifest
    nation in shuffled order → extras filter/shuffle/pick → unique-name +
    nextID per procedural nation) against the real `toNation` callback; the
    Nation/PlayerInfo construction crosses as name / spawnCell / flag / id
    tokens. `createNationsForGame` is out of scope (pure zod-config
    branching over already-ported `Game.ts` constants). The loader drops the
    type-only imports, re-points the Game.ts value import at the prepared
    copy, and re-exports the module-private bindings for capture. 300
    scenarios (7,837 result tokens) cover every table, pluralize over the
    whole bank + boundary words, 15 seeds, the collision/fallback paths, and
    7 createRandomNations cases (target ≤ / > manifest, extras dedupe,
    procedural fill).
42. **`game/GameUpdates.ts` + `execution/nation/NationEmojiBehavior.ts`**
    (`game_updates` / `nation_emoji`) — `GameUpdateType` is the 24-member
    implicit-numbering enum (`Tile = 0 .. DonateEvent = 23`); the port spells
    the discriminants out (`#[repr(i32)]` + `NAMES` + `from_i32`/`as_i32`/
    `name()`), and the rest of GameUpdates.ts is interfaces with no runtime.
    `nation_emoji` carries the Util.ts `emojiTable` (12×5 picker grid) and
    `flattenedEmojiTable` (60 distinct entries, row-major), the 23 `EMOJI_*`
    constants as literal arrays, and `emojiId` = `flattenedEmojiTable.indexOf`
    (-1 when absent). Faithfulness rides on the UTF-16 wire: the surrogate
    pairs, the VS16 tails (`❤️` = 2764 FE0F) and the ZWJ sequence
    (`🤦‍♂️` = 1F926 200D 2642 FE0F) are compared code-unit-exact, so a
    missing variation selector misses the table and returns -1 exactly like
    JS. The loader drops the type-only Game/PseudoRandom/EmojiExecution
    imports, re-points the `flattenedEmojiTable` value import at the prepared
    Util copy, and expands the parameter-property ctor. 31 gameupdates
    scenarios (372 result tokens: full enum dump + 30 name lookups incl.
    wrong-case/empty/prefix misses) and 71 nationemoji scenarios (1,064
    result tokens: grid dump, flattened dump, the 23 id arrays with names,
    and per-string + batched `emoji_id` over all 60 entries plus the -1
    branches).
43. **`pathfinding/PathFinder.ts`** (`water_path_memo`) — only `WaterPathMemo`
    is ported: the rest of the file (`UniversalPathFinding`,
    `sharedWaterChain` / `buildWaterChain`, `PathFinding`, `WaterPathFinder`,
    `tileStepperConfig`) rides on the `Game` facade. The inner `PathFinder` is
    a scripted mock (a queue of canned array/null answers that records its
    call arguments), so cache hits — where the chain is never touched — and
    array-`from` passthrough are observable in the token stream. Faithfulness
    points: a `null` answer is cached and costs a flat 16 bytes (a real path
    costs `len * 4`, the `Uint32Array.byteLength`); LRU order is JS `Map`
    insertion order with delete+re-insert on every hit; the waterVersion
    check runs at *every* `findPath` entry (even array-`from` queries) and a
    bump clears the whole cache; the numeric key `from * numTiles + to` is
    plain f64 arithmetic, so colliding pairs share an entry; a single
    over-budget insert evicts itself; and a cache *hit* returns the
    `Uint32Array` copy, whose `ToUint32` coercion can differ from the raw
    path the miss returned (-1 → 4294967295, 2^32+1 → 1). The loader drops
    the whole import block (type-only or Game-graph values the memo never
    touches) and expands both parameter-property ctors strip mode parses. 9
    scenarios (314 result tokens) cover miss, hit, null accounting, LRU
    re-insertion vs eviction order, waterVersion clears, array passthrough,
    over-budget self-eviction, key collision, and the Uint32 coercion.
44. **`Util.ts` remaining pure functions** (extends `util.rs`) — the two
    host-text sanitizers and the two distance comparator factories.
    `sanitizeClanTag` strips every non-`[a-zA-Z0-9]` **code unit** (the
    un-`u`-flagged regex), then `substring(0, 5)` + `toUpperCase()` — only
    ASCII survives the strip, so `ß → SS`-style Unicode expansions are
    unreachable; surrogate-pair emoji, `Ⅷ` and lone surrogates pin the
    per-unit stripping. `sanitizeLobbyLabel` filters by **code point**
    (`for...of`: tab/CR/LF/VT/FF → space; other C0, DEL, C1, the bidi
    overrides U+202A–2E / isolates U+2066–69 / marks U+200E/0F/061C dropped;
    U+200D ZWJ kept for emoji families), collapses `/\s+/g` and `trim`s with
    a hand-written **JS `\s` set** predicate (`char::is_whitespace` is a
    different set — NEL in, FEFF out), then caps at 48 **code points**
    without ever splitting a surrogate pair. `distSort` / `distSortUnit`
    share the numeric comparator `gm.manhattanDist(a, target) -`
    `gm.manhattanDist(b, target)` (the unit variant's `tile()` resolution is
    capture-side); the replay is a stable sort where a `NaN` distance
    difference is `+0` (equal), the V8 `SortCompare` rule from
    `closestTwoTiles` — NaN scenarios are shaped so every comparator result
    is 0-or-NaN, avoiding the non-transitive-insertion-order divergence.
    Strings cross the boundary as `[len, u0, ..]` UTF-16 code-unit token
    streams. 61 scenarios (536 result tokens) over kinds 15–18 of the util
    runner.
45. **`CosmeticSchemas.ts`** (`cosmetic_schemas`) — the runtime-value subset:
    the three `as const` effect-type arrays (`EFFECT_TYPES` 7,
    `TRAIL_EFFECT_TYPES` 2, `NUKE_EXPLOSION_TYPES` 3), the four pure
    effect/slot predicates (`isTrailEffect` / `isNukeExplosionEffect` /
    `effectTypeForSlot` / `effectMatchesSlot`) and the `DefaultPattern`
    literal. The `z.*` schema declarations are inert at capture time (the
    same zod-Proxy shim as `server_list`; `base64url` / `decodePatternData`
    only feed `PatternDataSchema`'s refine callback, never invoked);
    `findEffect` / `findEffectForSlot` ride on the nested `Cosmetics`
    catalog object and are not ported. `effectTypeForSlot`'s quirk — the
    bare `"nukeExplosion"` key resolves to `undefined` (the nuke-type
    branch matches only `atom`/`hydro`/`mirvWarhead`, the effect-type
    branch explicitly excludes `"nukeExplosion"`) — is pinned, as is
    `effectMatchesSlot`'s `effectType:"atom"` mismatch (forSlot resolves to
    `"nukeExplosion"` ≠ `"atom"`). 58 scenarios (511 result tokens) over
    kinds 0–7 of the cs runner; strings cross as `[len, u0, ..]` UTF-16
    code-unit streams.
46. **`StatsSchemas.ts`** (`stats_schemas`) — the runtime-value subset: the
    three `as const` unit-name arrays (`bombUnits` 4 / `boatUnits` 2 /
    `otherUnits` 7), the two `UnitType`-keyed lookup tables
    (`unitTypeToBombUnit` / `unitTypeToOtherUnit`; keys are the string-enum
    values, TS computed-key insertion order pinned), the 34 numeric
    `*_INDEX_*` constants in declaration order, and the module-private
    `toBigInt` coercion (bigint passthrough, `null`/`undefined` → `0n`,
    `/^-?\d+$/` decimal strings → parsed — `"007"` → 7, `"-0"` → 0 —
    anything else throws `ZbEncodeError`). The `z.*` / `zb.*` schema
    declarations are inert at capture time (zod/zb Proxy shims;
    `ZbEncodeError` is a real class so the throw branch is observable);
    `UnitType` is inlined as a plain object (strip-only loader cannot parse
    `export enum`, same precedent as `Maps.gen`), and
    `unitTypeToBoatUnit` is commented out in the TS. `toBigInt` returns
    `i64` here; every scenario stays within `|v| ≤ 2^53` (the capture fails
    loudly beyond that). 23 scenarios (1,042 result tokens) over kinds 0–6
    of the st runner.
47. **`Schemas.ts`** (`schemas`) — the runtime-value subset the zod wire
    layer is built from: the five closed `z.enum` option arrays
    (`PublicGameTypeSchema` 4 / `SCHEDULED_PUBLIC_GAME_TYPES` 3 /
    `LobbyAccentSchema` 4 / `ClientPlatformSchema` 3 / `ReportReasonSchema`
    4), the lobby constants (`MAX_HOSTED_LOBBIES` 10, the two auto-start
    windows 300000/600000 ms, `CLIENT_ID_MAPPING`, `ADMIN_BOT_CLIENT_ID`),
    the `LogSeverity` string-enum table (5 members), the 58-key
    `QuickChatKeySchema` list derived from `resources/QuickChat.json` in
    JSON insertion order, and the three regex-backed predicates:
    `isValidGameID` (`/^[A-Za-z0-9]{8,10}$/`, no `u` flag → UTF-16
    code-unit test), the renderable-name single-code-point test (the `u`-
    flag `^[ _.\-…]+$` class — node probing settled the `\\-` ambiguity:
    the backslash only escapes the hyphen, so `-` is a member but `\` is
    not, and there is no U+005C–U+0061 range) and the has-alnum search.
    The `z.*` / `zb.*` schema declarations are
    inert at capture time except `z.enum`, whose shim returns real
    `{options, exclude}` so the dumps read the actual arrays; the
    `RENDERABLE_NAME_ALNUM` / `_CHARS` regex-source strings (literal
    `\uXXXX` text) are ported verbatim. `LobbyInfoEvent` /
    `GroupTokenEvent` are pure field-storage classes and are not ported.
    12 scenarios (1,462 result tokens) over kinds 0–11 of the sc runner.
48. **`ApiSchemas.ts`** (`api_schemas`) — the runtime-value subset of the API
    wire layer: the four data constants (`ADMIN_ROLES`, `PlayerStatsGameModes`
    captured as its runtime *string values* — `"Free For All"` / `"Team"` /
    `"Humans Vs Nations"`, the bare `HumansVsNations` import included — and
    the two player-game filter arrays), the ten `z.enum` option arrays
    (`UsernameStatus` 4 / `BareClaim` 3 / `TribeNameStatus` 4 /
    `PlayerGameModeFilter` 4 / `PlayerGameTypeFilter` 3 / `PlayerGameResult`
    3 / `PaymentsProvider` 2 / `PaymentsKind` 3 / `PaymentsHandoff` 3 /
    `SteamOrderResolution` 4), and the four pure predicates: `isAdminRole`
    (strict `===` against the two literals), `isTemporaryUsername`
    (`/^TEMPORARY\d{4}$/`, no `u` flag → UTF-16 code units, `\d` ASCII-only
    so Arabic-Indic digits miss), `isVerifiedUsername` (no `.` code unit and
    not a TEMPORARY rename), and `isGrantedSubscription` (the three-state
    provider rule — `null` = granted true, a string rail false, a missing
    field false, no subscription false — modelled through the capture's sub
    encoding `[0]`=undefined / `[1,(str)provider]` / `[2]`=null). The
    `z.object(...)` declarations are inert at capture time except `z.enum`
    (the same functional shim as `schemas`; `.unwrap()` / `.pick()` /
    `.extend()` / `.refine()` / `.transform()` / `.or()` / `.default()` /
    `.partialRecord()` chains ride the Proxy), `TokenPayloadSchema`'s
    refine/transform callbacks reference `base64urlToUuid` (a `jose`
    dependency) and never run — the loader stubs the import. 16 scenarios
    (400 result tokens) over kinds 0–15 of the as runner.
49. **`game/TerrainMapLoader.ts`** (`terrain_map_loader`) — the `loadTerrainMap`
    orchestration with `loadImages` fixed `false` (the `createImageBitmap`
    layer-image branch is host-bound and not ported), captured against a
    scripted `GameMapLoader` mock whose `getMapData` hands back a *stable*
    manifest object so the JS reference semantics are observable: the
    module-level `loadedMaps` cache (a hit skips `getMapData`, every throw path
    leaves the key uncached — the call counter pins this), the dead mini-map
    ternary (`Normal` always takes `map4x` metadata with the `map4xBin` data;
    `Compact` takes `map16x` + `map16xBin`), the Compact **in-place** nation
    coordinate scaling (`Math.floor(x / 2)` rewriting the shared manifest
    arrays, so a second Compact load of the same manifest re-scales already
    scaled values and a later cache hit dumps the mutated array — modelled
    with `Rc<RefCell<Vec<_>>>`), the fresh-object `teamGameSpawnAreas` scaling
    (`Math.max(1, floor(w/2))` via `js_max`), the per-layer placement-then-
    alpha validation (`land`/`water` strict match; `!Number.isFinite(alpha) ||
    alpha < 0 || alpha > 1`, with `-0` passing), and `genTerrainFromBin`'s
    buffer-size check (`data.length !== width * height` before construction).
    The throw messages interpolate JS `Number`→string slots (`-0` → `"0"`,
    `NaN` → `"NaN"`) via `game_ts::js_num_str`. 14 scenarios (1,391 result
    tokens) of whole scripted load sequences over kind 0 of the tml runner.
50. **`execution/nation/NationUtils.ts`** (`nation_utils`) — `randTerritoryTileArray`
    / `randTerritoryTile` (bounding-box sampling: 100 tries, `isOnMap` continue,
    `ref`/`owner` identity via a player id, the `numTilesOwned() > 0 && <= 100`
    double-call `&&` short-circuit, the `randElement(Array.from(tiles()))`
    fallback) and `findJuiciestTarget` (the `Structures.has` / DefensePost /
    MissileSilo reduce filter over the real `Game.ts` **string** enum values,
    `troopGapRatio` — `troops()` only on the `maxTroops > 0` branch — the
    `Math.min/max(...values)` spread folded through NaN-propagating / ±0-correct
    `js_min`/`js_max`, normalize's `max > min ? (v-min)/(max-min) : 0`, and the
    strict-`>` best scan keeping the first tie). The `Game` / `Player` /
    `PseudoRandom` facades are scripted mocks; every facade call (arguments +
    return) is pinned in a flat trace in the res stream, so the loop order and
    short-circuits are bit-exact while the facade internals stay outside the
    ported surface. `calculateBoundingBox` reuses `crate::util` over the mock's
    `GameMap`-shaped `x`/`y`; the `??=` default path is dead through the public
    API (an empty `borderTiles` yields `(±Infinity)` bounds, never null —
    pinned). 17 scenarios (5,436 result tokens) over kinds 0–1 of the nu runner.
51. **`game/GameImpl.ts` tail + `game/TerraNulliusImpl.ts`** (`game_updates`
    kinds 2–3, `terra_nullius`) — `createGameUpdatesMap` (the `GameImpl.ts`
    module tail; the rest of the file — `createGame` and the `GameImpl` class —
    depends on the `Config` / `PlayerImpl` / `StatsImpl` facades and is out of
    scope): a faithful traverse-filter-write over the *reverse-mapped* numeric
    enum object (`Object.values` yields the 24 reverse names first, then the 24
    forward numbers, per the ECMAScript own-key rule — the `ts_load.mjs`
    inlined enum now carries the reverse mapping so the capture runs real
    numeric-enum semantics), keeping the values `!isNaN(Number(key))` passes
    (`Number(0)` is not NaN, so `0` survives; every name is NaN) and writing
    `map[key] = []` under the JS `Number`→string property name. The `gi_values`
    scenario pins the raw enumeration order and the filter's kept set; the
    `gi_map` scenario dumps the result object's key order / count and each
    value's `Array.isArray` + `length`. `TerraNulliusImpl` is the stateless
    neutral player: `smallID()` = `0`, `clientID()` = the literal
    `"TERRA_NULLIUS_CLIENT_ID"`, `id()` = JS `null` (encoded with the
    `[-1]` sentinel), `isPlayer()` = `false`. 6 scenarios (553 result tokens).
52. **`game/StatsImpl.ts`** (`stats_impl`) — the pure bigint stats
    accumulator: `conquest_by_type` over the `PlayerType` **string** enum
    (`"HUMAN"`/`"NATION"`/`"BOT"` → `PLAYER_INDEX_*`; an off-table mocked type
    reads `undefined` and skips the conquest), `_bigint` (bigint passthrough;
    number → `BigInt(Math.floor(v))` — `-0` → `0n`, fractional floors toward
    −∞, `NaN`/`±Infinity` **throw** at the exact TS landing point: after the
    `??=` init + `while` growth in `_addAttack`, after the `type()` facade
    call in `goldWar`, before any write in `_addPlayerKilled`), the shared
    `arr ??= [0n]` + `while (length <= index) push(0n)` + `+=`/max/set growth
    pattern (strict `>` — ties never rewrite), `attackCancel`'s `-troops`
    following the input shape, `bombLaunch`'s `MIRV` counter, the
    `recordTickSample` cross-multiplied drawdown comparison (exact integer
    arithmetic in `i128`, the `ddPeak === 0n` seed step), first-write-wins
    `killedBy` (`null` is a valid recorded value) / `deathPosition` `??=` /
    `kills` push, the `recordKill` Human + non-null-clientID filters, and the
    computed-key stringification (`{ [type]: [0n] }` and the
    `unitTypeToBombUnit`/`unitTypeToOtherUnit` lookups key `"undefined"` for
    an off-table type — captured). bigint crosses as `i64` (capture domain
    |v| ≤ 2^53, precedent `stats_schemas::toBigInt`); the `Player` facade is
    a scripted mock whose every call (`clientID()` / `type()` / `isPlayer()`)
    lands in the res trace, and the final `stats()` dump pins the `data` /
    `PlayerStats` / boats / bombs / units key insertion orders. The `in`
    prototype-chain quirk and the negative-index property write are out of
    domain (pinned in the module doc). 34 scenarios (2,137 result tokens).
53. **`game/UnitGrid.ts`** (`unit_grid`) — the 100-pixel-cell 2-D spatial
    index: a row-major `grid[ceil(h/cs)][ceil(w/cs)]` of insertion-ordered
    `Map<UnitType-string, Set<unit>>` cells over the **real** ported `GameMap`
    (so `gm.x/y/width/height` are already bit-exact), with the `Unit` facade a
    scripted mock (every `tile()` / `type()` / `isActive()` /
    `isUnderConstruction()` / `lastTile()` / `owner().id()` call recorded as a
    trace event, pinning call *counts* — `removeUnit` reads `tile()` once and
    `removeUnitByTile` never re-reads; the cross-cell `updateUnitCell` move
    re-reads it inside `addUnit`; `addUnit` reads `type()` **twice** on the
    missing-key branch, once otherwise). Faithful JS collection semantics:
    `Set.add` of a present member is a no-op that does *not* move it, while
    `delete` + re-`add` moves it to the tail; `Map.set` appends a new key and
    overwrites an existing one in place. `isValidCell` short-circuits
    left-to-right so a failing `gx >= 0` (NaN included) never touches
    `grid[0]`, but a passing one on a 0-row grid (0-height map) reads
    `.length` off `undefined` and throws a `TypeError` (pinned as op status 1
    with the partial trace). `getCellsInRange` rides JS `%` and the
    NaN-propagating / ±0-correct `js_min`/`js_max` (`Math.ceil(-0.05)` is `-0`,
    so a negative range can still leave a one-cell window since `0 <= -0`).
    `nearbyUnits`' array branch iterates `cy → cx → types → unitSet` while the
    scalar branch iterates `cy → cx → unitSet` — the same grid yields different
    result orders (pinned by `ug_nearby_order`); the distance filter is a
    strict `> rangeSquared` (equality survives) and `hasUnitNearby` /
    `anyUnitNearby` share `unitIsInRange`'s complementary `<=`, short-circuiting
    `isActive → under-construction → playerId (owner touched only when defined)
    → distance`. The `UnitPredicate` callbacks are scripted streams (a real JS
    closure runs at capture, every invocation traced). 20 scenarios (1,451
    result tokens).
54. **`execution/nation/SharedWaterCache.ts`** (`shared_water_cache`) — the
    nation-AI "which water bodies does each nation share with a valid trade
    partner" cache: `TTL_TICKS = 30` / `OCEAN_SENTINEL = -1`, the `get`
    rebuild gate (`tick - this.tick >= TTL_TICKS` starting from
    `tick = -Infinity` so the first `get` always rebuilds; after a rebuild
    `this.tick = tick` — *not* `++` — so a same-tick second `get` skips it,
    with a negative diff (tick regression) also skipping), the `waterFor`
    per-player rescan cache (strict `===` on both versions — a NaN
    `tileChangeVersion` never hits and rescans every rebuild), the
    border→shore→neighbor visit walk, the `comp !== null` lake add, and the
    two-pass `build` (pass 1 skips `PlayerType.Bot` — the string-enum value
    `"BOT"` — and collects `lakePartners` in `players()` order; pass 2
    iterates `playerToWater` insertion order, seeds the shared set with the
    `-1` ocean sentinel *first*, and per lake breaks on the first
    `other !== player && player.canTrade(other)` partner, pinning the
    `canTrade` call count; an empty shared set stores `null`, not a missing
    key). The `Game` / `Player` facades are scripted mocks with every call
    (tag + arguments + return) pinned in the res trace; `map().waterVersion()`
    collapses to one event (the intermediate `map()` is unobservable). 19
    scenarios (1,860 result tokens).
55. **`execution/ExecutionManager.ts`** (`execution_manager`) — the `Executor`
    client-intent dispatcher. The 24 `XxxExecution` classes and the
    `TribeSpawner` / `PlayerSpawner` helpers are *not* ported (existing
    exclusion) — they are stubbed as construction recorders, so every
    `new XxxExecution(args…)` lands in the trace as `[2, tag, args…]` and the
    orchestration replays token-for-token: the `playerByClientID` facade call,
    the `!player` warn branch (the warn message interpolates the clientID
    *field* — `undefined` / `null` spell themselves), the per-case argument
    extraction order (22 cases, e.g. `spawn` reads `gameID` / `player.info()` /
    `tile` / `true` — the `info()` facade call is pinned only where it
    happens), the `nations().map(n => n.spawnCell).filter(c => c !==
    undefined)` pipeline (null and `-0` survive), the `purchasedTribeNames =
    []` default parameter, and the default `throw` whose template literal
    stringifies the intent *object* — `"[object Object]"` for a plain literal
    (pinned). The ctor pins the real `simpleHash(gameID) + 1` seed feeding the
    (never-used) `PseudoRandom` through the ported `util::simple_hash_units` /
    `pseudo_random::PseudoRandom`; a throw inside `createExecs` aborts the
    `map` (already-built executions dropped, remaining intents skipped, never
    dispatched). The switch matches `intent.type` with `===` — a missing type
    falls through to the throw. 28 scenarios (1,359 result tokens).
56. **`game/RailNetworkImpl.ts` + `game/TrainStation.ts`** (`station_manager`,
    `train_station`) — the rail-network container trio. `RailNetworkImpl`,
    `RailPathFinderServiceImpl` and `createRailNetwork` are *not* ported (later
    phase — heavy pathService/config/nearbyUnits facade surface).
    `StationManagerImpl`: the `stations` `Set` (insertion order, delete+re-add
    moves to tail), the sparse `stationsById` array (`removeStation` writes
    `undefined` into the slot — length and holes survive), `nextId` starting at
    1, and the `count()` quirk — it returns `this.nextId`, **not**
    `stations.size` (three adds → 4; removals never decrease it). Re-adding the
    same station object re-assigns a fresh id and appends a second slot while
    the `Set` add is a no-op. `findStation` scans the `Set` in insertion order
    comparing `station.unit === unit` (JS `===`: `+0 === -0` true, `NaN` never
    matches) and returns the **first** hit or `null`.
    `TrainStation` + `Cluster` (the stop-handler surface — `stopHandlers`,
    `onTrainStop`, the ctor's `createTrainStopHandlers(new
    PseudoRandom(mg.ticks()))` side effect, `rel` — is excluded; the capture's
    `ts_load` block strips it so construction is observable-free):
    `railroads: Set` / `railroadByNeighbor: Map` insertion-order semantics with
    the `from === this ? to : from` neighbor rule; `removeNeighboringRails`
    removes only the **first** matching rail (`find`, not `filter`) with the
    `RailroadDestructionEvent` `addUpdate` emitted *before* the removal — and
    the parallel-rail quirk where `railroadByNeighbor.delete(neighbor)` drops
    the map key while a second rail survives in the set, so `getRailroadTo`
    returns `null` for a still-connected neighbor (pinned); `neighbors()`
    iteration order; `setCluster` disconnects the old cluster only when it is
    non-null *and* different (same-cluster re-set is a pure no-op) while
    `Cluster.removeStation` does **not** clear the station's cluster pointer;
    `Cluster.addStation` re-reads `unit.type()` on every call (even for an
    existing member — trace count pinned); `merge(other)` iterates
    `other.stations` while each `addStation → setCluster → removeStation`
    deletes the *current* element — JS `Set` iteration visits every element
    when only the current one is deleted (verified against real TS), replayed
    as snapshot + has-check; `tradeAvailable`'s `otherPlayer === player ||`
    short-circuit pins the `canTrade` call count (self → zero calls);
    `randomTradeDestination` reservoir sampling draws `nextInt(0,
    eligibleSeen)` once per **eligible** station only, over the real ported
    `PseudoRandom`; `isTradeStation` = unit type `===` the `"City"` / `"Port"`
    string-enum values (Factory excluded). 30 scenarios (1,875 total tokens).
57. **`game/RailNetworkImpl.ts`** (`rail_network`) — the `RailNetworkImpl`
    orchestration class + the `createRailNetwork` factory (the last big
    `src/core` surface). `RailPathFinderServiceImpl` is excluded — the capture
    injects a scripted `pathService` mock (`findTilePath` / `findStationsPath`
    tables; the station table is keyed by numbers while the impl passes
    objects, so `"[object Object]|…"` never hits and `findStationsPath`
    returns `[]` — pinned), and the `Game` facade (`x`/`y`/`addUpdate`/
    `config().trainStationMaxRange/MinRange/railroadMaxSize`/`hasUnitNearby`/
    `nearbyUnits`) and the unit's `setTrainStation` are scripted mocks whose
    every call rides the res trace, pinning counts and short-circuits.
    Quirks pinned against the real TS: `connectStation` adds to the manager
    **before** trying rails; `connectToExistingRails` — `closestRailIndex ===
    0 || >= tiles.length` continue (an empty-tiles rail returns `-1`, passes
    the guard, and splits into two empty rails), `nextId++` From→To, grid
    re-`register` To→From, only `from.getCluster()` is consulted, `edited.
    size > 1` merges, return `size !== 0`; `overlappingRailroads` /
    `computeGhostRailPaths` guard on the `"City"`/`"Port"`/`"Factory"` string
    enum, tiles Set → `sort((a,b)=>a-b)` (NaN comparator → V8 stable); ghost
    paths — `**2` via `x*x` (NaN minRange → `<=` never continues), `paths.
    length >= 5` break **before** the minRange continue, `connectedStations.
    some` short-circuits `distanceFrom` calls, a non-station City neighbor
    pathfinds in reverse (`targetTile → tile`), accept `0 < len < maxSize`;
    `connectToNearbyStations` — `distanceFrom` computed **before** the
    null-cluster continue, `connectionAvailable` short-circuits the minRange
    config read, `connect` success gates `addStation` (a station may switch
    clusters mid-loop), `size === 0` spawns a fresh `Cluster`; `removeStation`
    — `setTrainStation(false)` facade call, empty cluster → `deleteCluster` +
    `dirty.delete` else `dirty.add` (insertion order, add no-move);
    `disconnectFromNetwork` / `deleteCluster` / `merge` iterate a JS `Set`
    while deleting the **current** element — snapshot + has-check (P54
    precedent); `recomputeClusters` — `new Set(cluster.stations)` copy,
    `values().next().value` first, the **first** BFS group keeps the original
    cluster (only later groups get fresh clusters via `addStations`' auto-
    disconnect), trailing `dirtyClusters.clear()`; `distanceFrom` BFS — shift
    **then** visited-check, `distance >= max` continue, `neighbor === dest`
    → `distance + 1`, else `-1`; `mg.y = (t/w)|0` (ToInt32), `mg.x = t % w`.
    28 `rn_` scenarios (7,848 tokens).

## Server ports (`src/server`)

The `src/core` port is complete; the deterministic subset of `src/server`
is now ported the same way (host I/O — express routes, JWT, Redis, otel,
WebSockets — stays excluded, mirroring the `src/core` exclusions).

58. **`server/VoteTally.ts` + `server/ConfigPatch.ts` +
    `server/IntentAuthorization.ts` + `server/Consensus.ts`**
    (`vote_tally`, `config_patch`, `intent_authorization`, `consensus`,
    plus the shared `js_json` helper) — the server vote/authorization
    cluster. `js_json.rs` reproduces `JSON.stringify` byte-for-byte (ES2019
    lone-surrogate escapes, NaN/±Infinity → `null`, `-0` → `0`, object
    `undefined` values omit the key, array `undefined` → `null`, top-level
    `undefined` → no string, insertion-order keys) and carries a `JsVal`
    tri-state codec distinguishing absent-key / `undefined` / `null` —
    the ConfigPatch copy gates and the Consensus candidate keys depend on
    it. Quirks pinned: `VoteRound.result` returns the **first** candidate
    (Map insertion order) holding a *strict* majority (`votes * 2 > total`,
    a 1-of-2 tie is not a decision); `add` is idempotent per (candidate,
    IP) and returns the post-vote unique-IP count; `resultAmong` counts
    only `activeIPs` members and compares against `activeIPs.size`.
    `applyGameConfigPatch` — COPIED_KEYS write only when the patch value
    is `!== undefined` (absent and present-but-`undefined` both leave the
    target untouched), NULLABLE_KEYS additionally collapse `null` →
    `undefined`, and `hostCheats` is assigned **unconditionally** (an
    omitted patch key *clears* the target field); `hostCheatsEnabled`
    truth table (`{}` → false, `infiniteGold`/`infiniteTroops === true`,
    `typeof goldMultiplier/startingGold === "number"`). `authorizeIntent`
    — full guard-order table (adminBot+public pre-switch 403, `mark_
    _disconnected` 400, kick creator/admin + listed-host 403, update_
    game_config's five ordered guards incl. `allowedPublicIds?.length ??
    0`, start-timer, pause listed-host 403 / not-started 409, gameplay
    default adminBot 400). `WinnerVote.cast` keys by `JSON.stringify(
    msg.winner ?? null)` (a cancelled match keys `"null"`); `tally`/
    `tallyAmong` **overwrite** `decided` on every non-null result (no
    latch — the guard is the caller's job, `cv_redecide`); `LiveStatsVote
    .cast` ignores `turn <= settled.turn`, creates + prunes the round
    **before** the voter dedup, keys by `JSON.stringify(stats)`, and on
    settle deletes every round key `t <= turn`; `prune` drops the oldest
    key while `size > 20`. 39 scenarios (`vt_` 8, `cp_` 8, `ia_` 13,
    `cv_` 10).

Regenerate whenever a ported source changes:

```
node rust/tools/gen_vectors.mjs
```

Requires a working Node (v20+) in the repo; the script imports `src/core/*.ts`
and `src/server/*.ts` directly via Node's type stripping.

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
compares every value. Last run: **127,654 comparisons, all bit-identical**.

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
