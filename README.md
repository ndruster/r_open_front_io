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
│   │   ├── listing_state.rs       port of server/ListingState.ts
│   │   │                          (list/feature state; Date.now scripted)
│   │   ├── name_visibility.rs     port of server/NameVisibility.ts
│   │   │                          (anon-name / reveal rules; Client facade
│   │   │                          scripted-mocked, Client.ts ws class excluded)
│   │   ├── map_playlist.rs        port of server/MapPlaylist.ts (deterministic
│   │   │                          layer + Math.random orchestration: gameConfig
│   │   │                          / rollConfig ffa-team, ranked configs; Date.now
│   │   │                          / Math.random / getMapLandTiles scripted)
│   │   ├── desync_detector.rs     port of server/DesyncDetector.ts
│   │   │                          (hash tally + notify-once; Client facade)
│   │   ├── join_verify.rs         port of server/JoinVerify.ts (pure decision
│   │   │                          fns; verifyJoin fetch I/O excluded)
│   │   ├── censor.rs              port of server/Censor.ts (shadow/banned
│   │   │                          tables + censorPlayer orchestration; the
│   │   │                          obscenity matcher is a scripted facade)
│   │   ├── privilege.rs           port of server/Privilege.ts (decideClanTag /
│   │   │                          FailOpen / resolveVerifiedJoin / isAllowed
│   │   │                          orchestration; the six leaf validators are a
│   │   │                          scripted facade, trace 40-45)
│   │   ├── roster.rs              port of server/Roster.ts (full bookkeeping
│   │   │                          class; narrow Client stub + integer-id ws
│   │   │                          facades, close trace 50-52)
│   │   ├── match_telemetry.rs     port of server/MatchTelemetryRecorder.ts
│   │   │                          (recorder + identityFor; Date.now / emitter
│   │   │                          scripted, event trace 60)
│   │   ├── ranked_checkin_gate.rs port of server/RankedCheckin.ts (gate +
│   │   │                          buildVersionField/buildSiteField subset;
│   │   │                          isActive/log scripted, env trace 72)
│   │   ├── cluster_checkin.rs     port of server/ClusterCheckin.ts (pure
│   │   │                          subset; ServerEnv scripted facade, setActive
│   │   │                          trace 73)
│   │   ├── game_api_cors.rs       port of server/GameApiCors.ts +
│   │   │                          server/NoStoreHeaders.ts (setHeader facade
│   │   │                          trace 71, env trace 72)
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
59. **`server/ListingState.ts` + `server/NameVisibility.ts`**
    (`listing_state`, `name_visibility`) — the lobby listing/anonymity
    cluster. `Date.now()` is the only impure call in `ListingState`;
    `ts_load.mjs` rewrites it to `globalThis.__LISTING_NOW` and the capture
    injects the clock per op (Rust `set_listed(listed, now)` takes it
    explicitly). Quirks pinned: `setListed` is a **no-op when unchanged**
    (a duplicate `setListed(false)` never clears a stale `listedAt`, and a
    duplicate `setListed(true)` never resets the deadline); relisting after
    a delist stamps a fresh `listedAt`; `autoStartAt` returns `undefined`
    unless `listed && listedAt !== undefined`, and flips the constant on
    `featured` (`HOSTED` 300 000 vs `FEATURED` 600 000 ms, from `schemas`);
    `setFeatured` sets `featured = true` unconditionally, sanitises the
    label at the boundary (`sanitizeLobbyLabel`, already ported in `util`)
    with an empty-after-sanitise label collapsing to `undefined`, and
    assigns `accent` **even when `undefined`** (the key is created).
    `NameVisibility` runs over a scripted `view` facade (`config()` /
    `clients()` / `teamIndex()` thunks, every call traced to pin counts and
    short-circuit order); the `Client.ts` WebSocket class stays excluded —
    the capture passes plain client stubs. Quirks pinned: `anonName`'s slot
    is the target's **join-order index** in the insertion-ordered `clients`
    Map (a target absent from the map yields `slot === map.size`; late
    joiners append so existing slots never shift); `anonOffsetSeed` is
    `0` for an absent viewer, else `simpleHash(viewer)` when unteamed or
    ``simpleHash(`${gameID}:team:${team}`)`` when pinned to a matchmade team
    (the template interpolates the numeric team); `sameMatchmadeTeam` calls
    `teamIndex` on the target **only** when the viewer's team is defined
    (the `&&` short-circuit is traced); `seesRealBeyondTeam` short-circuits
    on `!anonymizeNames` (no `clients()` lookup) and on `target === viewer`
    (self-reveal without any grant); `viewerSeesAllNames` falls through to
    the `publicId` reveal only when the `nameReveals` array misses;
    `startInfoFor` reads `config()` once, returns the **same** `real`/`wire`
    object reference when names are not anonymised (gated on the admin-FFA
    clan-tag reveal), and otherwise rebuilds each player with `clanTag` read
    from `real.players[i]` at the **same index** as the `wire` player (an
    index-alignment quirk pinned with `real`/`wire` players in different
    order); `lobbyClients` computes `friendsLookup` once up front, and the
    anon branch emits a **narrower key set** (no `friends`/`verified`) than
    the real branch, with `spectator || undefined` (a `false` collapses to
    `undefined`, `||` not `??`) and the teammate-only reveal blanking
    `clanTag`/`friends` while keeping the real username; `friendsLookup`
    skips clients with a falsy `publicId` (empty string) or who are
    spectators, and returns `undefined` (not `[]`) when no friends are
    present — and it reads `client.friends` **unguarded**, so a stub without
    the field throws (the capture always supplies it). 26 scenarios
    (`ls_` 12, `nvs_` 14).
60. **`server/MapPlaylist.ts` (deterministic layer)** (`map_playlist`) — the
    public-lobby map rotation. `generateNewPlaylist`'s `Date.now()` seed is
    scripted through `globalThis.__MP_SEED` (precedent: `listing_state`), and
    the logger is rewritten to a message sink (`globalThis.__MP_LOG`) so the
    exact `Generated map playlist in N attempts` / fallback strings land in
    the golden `res` — the attempt count and fallback path are observable and
    the Rust replay must hit the same one for the same seed. Ported surface:
    the constant tables (`TEAM_WEIGHTS` 10 entries, `SPECIAL_MODIFIER_POOL`
    **38** tickets — the `Array(n).fill` spreads expand to 38, not 40 —
    `MUTUALLY_EXCLUSIVE_MODIFIERS` 6 pairs, `SPECIAL_TEAM_MAPS` 28 entries,
    `DOOMSDAY_ROTATION_SPEEDS`), `buildMapsList` with the `>= 0` (ffa/team)
    vs `> 0` (special per-mode fallback) asymmetry, `playlistKey`,
    `addNextMapNonConsecutive` (the `slice(-5)` window, splice-then-push on
    the first source entry outside it, `false` leaving both arrays untouched),
    `generateNewPlaylist` (re-shuffles `source` every attempt, so the retry
    count is a pure function of the seed — seed `1771000000307` lands on
    attempt 2), `getNextMap` (refill-when-empty then `shift`, consuming the
    current seed; the ffa queue is 604 entries), and the pure helpers
    `calculateMapPlayerCounts` (JS `Math.round` half-up), `playersPerTeam` /
    `numberOfTeams` / `adjustForTeams` / `adjustTeamCountForPlayerCapacity` /
    `getSpawnImmunityDuration`. Key quirk: `Duos` / `Trios` / `Quads` /
    `HumansVsNations` are **string** constants (`"Duos"`, … ,
    `"Humans Vs Nations"`), so the `typeof playerTeams !== "number"` capacity
    gate passes for all four presets and they are handed through untouched —
    the numeric branch only ever sees `2..7`. Excluded at this stage:
    `getSpecialConfig` / `getRandomSpecialGameModifiers`, whose
    `.sort(() => Math.random() - 0.5)` depends on V8's internal comparison
    sequence and is not bit-reproducible (permanently excluded); the
    `Math.random` orchestration layer (`rollConfig` / `gameConfig` /
    `get1v1Config` / `get2v2Config` / `lobbyMaxPlayers` /
    `supportsCompactMapForTeams` / `getCrowdedMaxPlayers`) is covered in
    entry 61. 21 scenarios (`mpl_`).
61. **`server/MapPlaylist.ts` (Math.random orchestration)** (`map_playlist`,
    S4) — the scheduled-lobby config layer on top of the S3 queues. Both
    remaining impurities are scripted: `Math.random()` is rewritten to
    `globalThis.__MP_RAND()` (a capture-side FIFO popper; the Rust harness
    mirrors the queue and pins, per op, the exact **values consumed in call
    order** as a `[n_rand, (v)*, n_land, (map-str)*, payload...]` prefix —
    short-circuit paths that consume nothing are pinned too), and
    `getMapLandTiles` becomes a table facade (`__MP_LAND`, missing map falls
    back to `1_000_000` — the real TS catch branch). Ported: `getTeamCount`
    (force gate consumes a rand **only** when the map declares a
    `specialTeamCount`, else short-circuits; weighted roll over
    `TEAM_WEIGHTS`), `lobbyMaxPlayers` (tier roll `r<0.3?l:r<0.6?m:s`, Team
    `ceil(base*1.5)` capped at `l`, compact `max(3, floor(p*0.25))`),
    `supportsCompactMapForTeams`, `getCrowdedMaxPlayers` (`<=60` → 60/125,
    else `undefined`), `rollConfig` **ffa/team only** (special branch asserts
    — it reaches the excluded `getSpecialConfig`), `gameConfig` (`scheduled`
    counter, `trusted` every 7th), `get1v1Config` / `get2v2Config` (fixed
    maps list, `Europe Classic` is the enum's string value). Quirks pinned:
    `isCompact = playlists[type].length % 3 === 0 || undefined` is
    `true|undefined` (never `false`) and reads the queue length **after**
    `getNextMap`'s `shift()` (the first ffa roll sees 603 → compact, the
    first team roll sees 760 → not); the `||` comment's "75% reduction" is
    really `floor(p*0.25)`; `trusted: true` is **appended** at the end of
    the config object (not insertion-ordered mid-object); the FFA path's
    `playerTeams` key is present-`undefined` (`JsVal::Undef`, not absent);
    the weighted-roll tail is reachable only with a scripted rand of exactly
    `1.0`. The `GameConfig` object crosses the harness through the `js_json`
    `JsVal` codec with the TS literal's key order. 12 scenarios
    (`mpl_tc_*`, `mpl_lmp_*`, `mpl_sc_*`, `mpl_cm_*`, `mpl_rc_*`,
    `mpl_gc_*`, `mpl_1v1_*`, `mpl_2v2_*`).
62. **`server/DesyncDetector.ts` + `server/JoinVerify.ts` + `server/Censor.ts`**
    (`desync_detector`, `join_verify`, `censor`) — the sync / admission /
    moderation cluster. `DesyncDetector` runs over the narrow `Client` facade
    (plain stubs `{clientID, hashes}`; `outOfSyncClients` crosses as the
    clientID list); quirks pinned: `counts` is a JS `Map<number, number>`
    (SameValueZero keys, first-seen insertion order — `NumMap`),
    `mostCommonHash` uses strict `>` so a tie keeps the **first-inserted**
    hash, the strict-majority swap `outOfSync.length > floor(active.length /
    2)` replaces the list with **all** of `active` — including clients that
    never reported a hash — and `>` not `>=` means exactly half does NOT
    trigger it; `check` gates `active.length <= 1` and
    `turnsCommitted % 10 !== 0 || turnsCommitted < 10` (the `%` short-circuits
    first, so `check(0)` hits the `< 10` gate); `record` adds every id to
    `desynced` but `notified` latches — each client is returned at most once.
    `JoinVerify` ports the two **pure decision functions** only
    (`isSteamAuthenticated`, `planJoinVerify`); `verifyJoin` is fetch I/O and
    stays excluded. Quirks pinned: `!args.turnstileToken` is a FALSY test —
    the empty string `""` rejects exactly like `null` (never forward a first
    join without a token); the returned plans are TS object literals, so
    `{action:"reject"}` / `{action:"skip"}` have **no** `token` key at all
    (Absent, not present-`undefined`) while `{action:"verify", token}` always
    carries it (the value may be JS `null`). `Censor` ports the tables
    (`shadowNames` 21 entries, `bannedWords` 13) and `censorPlayer`'s
    orchestration; the obscenity `profanityMatcher` is unresolvable in the
    port repo (no `node_modules/obscenity`), so it rides as a scripted
    black-box facade (`globalThis.__CN_MATCHER` set before the import;
    `hasMatch(input) -> bool`, `getAllMatches(input) -> [{startIndex,
    endIndex}]`, every call a trace event pinning counts, inputs and the
    `||` / `.some()` short-circuit orders) — the library internals
    (transformer chains, the `kkk` includes check) are NOT replicated.
    Quirks pinned: `clanTag ?` is a truthy gate — `null` AND the empty string
    take the false branch with **zero** facade calls; the boundary slur
    concatenates `clanTag + username` (tag first) and the predicate compares
    against the tag's **UTF-16 unit length**; `simpleHash` is computed only
    when the shadow branch is taken; a surviving tag is `toUpperCase()`d, a
    dropped one is JS `null` (the `clanTag` key is always present). 21
    scenarios (`dd_` 7, `jv_` 3, `cn_` 11).
63. **`server/Privilege.ts` + `server/Roster.ts` +
    `server/MatchTelemetryRecorder.ts`** (`privilege`, `roster`,
    `match_telemetry`) — the S6 server cluster. `Privilege` ports the pure
    decisions (`decideClanTag` via `resolveClanTag`, `FailOpenPrivilegeChecker`
    both methods, `resolveVerifiedJoin`, `isTemporaryUsername` reusing
    `api_schemas`) plus the `isAllowed` ORCHESTRATION layer; the six leaf
    validators (cosmetics catalog graph) ride as a scripted black-box facade —
    the capture monkey-patches the prototype methods to consult
    `globalThis.__PV_LEAF`, every leaf call a trace event (40-45) pinning the
    gate order and the first-throw short-circuit. Quirks pinned: the KEPT clan
    tag is the ORIGINAL `censoredTag` (not the uppercased compare key);
    `FailOpen.isAllowed` gates STRICT `verified === true` (`1` / `"true"` fail
    it); `resolveVerifiedJoin`'s fall-through runs `delete cosmetics.verified`
    — the res dumps the POST-MUTATION object (key REMOVED, not set undefined);
    `\d` without the `u` flag is ASCII-only (`TEMPORARY١٢٣٤` is NOT
    temporary); `cosmetics.effects ??= {}` is lazy — an EMPTY `refs.effects`
    passes the truthy gate but the result carries NO `effects` key; the result
    key order is pattern,color,flag,skin,crown,effects,verified. `Roster`
    ports the whole bookkeeping class over the narrow `Client` stub and
    integer-id ws facades (same id == same socket object): `add`'s five
    container orders, `reconnect`'s REFERENCE-identity gate (same ws → zero
    close trace; new ws → 51 removeAllListeners + 52 `close()` no-args on the
    OLD socket, then move-to-tail), `markLeft` keeps the `everyone` record and
    the reconnect mapping, `forgetReconnect`'s `get(pid) === clientID` guard
    (a b-steals-pid seat → forget(a) deletes nothing), `kick` adds to `kicked`
    BEFORE the `some` scan (so the answer is wasConnected, and `wasAdmitted`
    consults `kicked` first), `pruneStale` strict `>` (silence == max survives),
    `closeAll` iterates the sockets Set in insertion order and closes ONLY
    `readyState === OPEN` with `(1000, reason)`, `isDisconnected` is `?? true`
    for unknown ids, `votingUniqueIPs` sizes a `Set` over `players()` (the
    spectator-filtered list). `MatchTelemetryRecorder` ports `identityFor` +
    the recorder; `Date.now()` is scripted (`globalThis.__MT_NOW` FIFO,
    precedent: `__LISTING_NOW`) and the emitter is a construction-injected
    black box (outcome queue 0 enqueued / 1 dropped-return / 2 throw; every
    emission traces `[60, ...codec(event), outcome]` BEFORE the return/throw,
    pinning the event byte-for-byte). Quirks: the event key order is
    schemaVersion,type,matchId,sequence,observedAt,serverTick,payload;
    `sequence` POST-increments even when the emitter throws (the gap is
    observable); the intent payload's SIX keys are always present (omitted
    reason args cross as present-`undefined`, not absent); `takeTickCounts`
    is get-then-`delete` (a second read returns the `{0,0,0}` default);
    `matchFinished` latches (second call emits nothing) and passes `totalTurns`
    as BOTH `serverTick` and `payload.totalTurns`; `observedAt` is read AFTER
    the sequence increment; tick counts key through `NumMap` (+0/-0 collapse,
    NaN by bits). 36 scenarios (`pv_` 16, `rs_` 12, `mt_` 8).
64. **`server/RankedCheckin.ts` + `server/ClusterCheckin.ts` +
    `server/GameApiCors.ts` + `server/NoStoreHeaders.ts`**
    (`ranked_checkin_gate`, `cluster_checkin`, `game_api_cors`) — the S7
    small-decision cluster. `RankedCheckinGate` ports the flip latch
    (`lastActive` SEEDED `true` — the first active pass logs nothing; a flip
    traces the verbatim PAUSED/RESUMED line as `[70, ...codec(msg)]` and
    `shouldCheckIn` returns the FRESH active, never latched) plus
    `buildVersionField` (`isCommitLike ? {version: commit.toLowerCase()} : {}`
    — the `{}` branch is the ABSENT-key literal) and `buildSiteField`
    (`site !== undefined && isSiteLike(site) ? {site} : {}`).
    `ClusterCheckin` ports `isRefusal` (`typeof result === "object" &&
    result !== null` — strings/null false, objects/arrays true),
    `registeredSite` (`siteHost() ?? publicHost()` — the `??` fires ONLY for
    null/undefined, a siteHost hit reads publicHost ZERO times), `checkinBody`
    (evaluation order publicHost → machine → site (registeredSite: siteHost,
    then a SECOND publicHost read when siteHost is undefined) → letter →
    version → numWorkers; key insertion order
    site,letter,host,version,numWorkers,liveGames,(machine?) with the
    conditional spread appending `machine` LAST only when `machine !==
    undefined` STRICT; `host === undefined` STRICT → `null`) and
    `applyCheckinState` (`result === null` STRICT return — `undefined` falls
    THROUGH to `setActive(false)`; only the exact `"open"` string is true).
    `GameApiCors` ports `isAllowedOrigin` (five-gate chain: desktop hit =
    ZERO env reads; the strict `!== undefined` gates with the
    `https://${host}` template compares; `own === undefined` ends the chain
    false BEFORE pageHostFor) and `applyGameApiCorsHeaders` (`Vary` ALWAYS
    first; `requestOrigin === undefined` short-circuits before the gate
    chain, the empty string walks it; a grant adds the four verbatim
    Access-Control-* headers, deliberately NO Allow-Credentials).
    `NoStoreHeaders.setNoStoreHeaders` is the three verbatim cache-killer
    headers. `ServerEnv` rides as the shared scripted facade
    (`globalThis.__CK_ENV`, every read `[72, method, ...codec]`, pageHostFor
    carrying the host arg); `setHeader` traces `[71, ...codec(name),
    ...codec(value)]`, `setActive` `[73, bool]`, `log.info` `[70,
    ...codec(msg)]`. zod is erased (the functional-enum shim keeps
    `ServerStateSchema.options` dumpable); `sendCheckin` / `rankedCheckinPass`
    / `startRankedCheckinLoops` / the express middleware are EXCLUDED
    (`fetch / GameManager / Math.random / req-res-next). 32 scenarios
    (`ck_` 12, `rg_` 8, `hd_` 12).
65. **`client/render/gl/utils/TileCodec.ts` +
    `client/render/types/UnitType.ts` + `client/render/types/Renderer.ts` +
    `client/SubscriptionPolicy.ts` + `client/StatsConstants.ts` +
    `client/utilities/ReplaySpeedMultiplier.ts` +
    `client/hud/layers/lib/GoldRateTracker.ts` +
    `client/render/frame/derive/{AllianceClusters,AttackRings,NukeTelegraphs,PlayerStatus,RelationMatrix,TerrainRowSpans}.ts`**
    (`tile_codec`, `unit_types`, `renderer_consts`, `subscription_policy`,
    `stats_constants`, `replay_speed`, `gold_rate_tracker`,
    `alliance_clusters`, `attack_rings`, `nuke_telegraphs`, `player_status`,
    `relation_matrix`, `terrain_row_spans`) — the S8 client cluster: six pure
    constant/data modules, the GoldRateTracker bookkeeping class, and the
    six-module frame-derive pipeline.
    `TileCodec`
    dumps the mask forms (`0xfff`, `1<<13`, `1<<14`) AND the `TILE_DEFINES`
    table whose values are the BIT INDICES (13/14), the shader's injection
    form. `UnitType` dumps the 16 atlas-ordered strings, the three derived
    `ReadonlySet`s probed by membership (STRUCTURE 6 / NUKE 3 / SMOOTHED 4 —
    insertion order), and the `NUKE_MAGNITUDES` radii with property-read
    probes. `Renderer` ports the numeric enums WITH their reverse mappings
    (the capture emits the exact `t[t["Engine"]=0]="Engine"` IIFE so
    `Object.keys` order matches the real enum: "0","1","2" first) plus
    `MAX_NUKE_EXPLOSION_COLORS` and the `[0.6, 0.1, 1]` fallback triple; the
    PlayerState/UnitState/PlayerStatic interfaces become the shared wire
    codecs (`push_*`/`read_*`) the derive ports ride. `SubscriptionPolicy` is
    the one-boolean launch switch (`STEAM_TIER_CHANGE_IN_APP = false`);
    `StatsConstants` the 21 `COLUMN_IDS` plus the two-key `DEFAULT_STATS_COLUMNS`
    membership dump; `ReplaySpeedMultiplier` the speed table. `GoldRateTracker`
    ports the whole bookkeeping class over game TICKS: `record`'s eviction is
    a `while (samples[0].tick < cutoff) shift()` with STRICT `<` — a sample
    exactly AT the cutoff survives (`grt_window`); the hard cap is STRICT `>`
    — 240 samples never splice, the 241st drains the head (`grt_cap`);
    `rate` is the two-point slope with the `dtMin <= 0 → 0` gate (frozen clock
    yields 0, never a divide); `history` is insertion-ordered per smallID
    (observable through the dump). `AllianceClusters` ports the union-find
    with path HALVING — each pass mutates `parent[x] = parent[parent[x]]` then
    advances on the OLD parent, and the step-by-step mutation is pinned through
    the parent-map dump after a scripted find/union session (`ac_session_halving`);
    the seeding gate is `smallID > 0`, the union gate requires
    `parent.has(allyID)` (a foreign ally is never seeded, never unioned), and
    the result iterates `parent.keys()` in seed order. `AttackRings` pins the
    five-gate ORDER with the owner filter LAST — a foreign transport still
    walks the first four gates — and the STRICT `targetTile === null` check
    (`undefined` would pass; the codec keeps null as the only absent form).
    `NukeTelegraphs` ports both extract variants plus `classifyOwner`: the
    `startTick > currentTick` gate is STRICT `>` (a plan starting exactly now
    STILL telegraphs), `NUKE_MAGNITUDES[unitType]` is a truthy gate — a MIRV
    has NO mag entry and is SKIPPED even though it is a NUKE_TYPE — and the
    relation matrix rides SPARSE (nonzero cells only; out-of-range cell writes
    are silently dropped, matching `Uint8Array` semantics). `PlayerStatus`
    pins the crown scan's STRICT `>` (a tilesOwned tie keeps the FIRST alive
    player), the `localPlayerSmallID > 0` truthy gate, the nuke-targets-me
    chain `lpsid > 0` → `tileState !== undefined` → `targetTile !== null` →
    `(tileState[targetTile] & 0xfff) === lpsid` where an OOB / negative /
    fractional `Uint16Array` read is `undefined` and `undefined & 0xfff` → 0
    (`pst_nuke_targets_me` feeds tile 99 into an 8-slot array), the alliance
    fraction gate's RAW-`localPlayerID` TRUTHINESS (`""` falsy disables the
    progress bar even though the `?? ""` copy still feeds `allianceReq` —
    `pst_empty_lpid`), and the eleven-flag OR gate that admits an entry to the
    result map (draining/decaying/warnProgress alone do NOT qualify —
    `pst_replay_crown` players 5/6 get no entry). `RelationMatrix` holds the
    ONE reusable `Uint8Array(1024*1024)` in the harness; the alliance upgrade
    is `if (matrix[ab] < 1) matrix[ab] = 1` — an EMBARGO (2) is NEVER
    downgraded by a later alliance — while embargo writes are UNCONDITIONAL
    overwrites both directions (a same-team friendly written earlier for the
    same pair LOSES; `rmx_embargo_override` pins both directions), the sid
    gate is `sid <= 0 || sid >= 1024`, and `buildTeamMap` filters
    `p.team !== null` STRICT. `TerrainRowSpans` ports the ref→row→merge
    pipeline: the merge gate is adjacent rows AND (`mergedArea <=
    sourceArea * 1.5` OR `extraTexels <= 4096`) — `trs_merge_ratio` passes on
    the ratio arm, `trs_merge_extra` on the texel arm, and a wide-map pair
    fails both; the pending rect mutates in place (`x = minX`, `w`, `h++`,
    `sourceArea += rowWidth`); `bytes` fills in RECT order then dy then dx
    through the deterministic `(ref * 7 + 3) & 0xff` closed form both sides
    compute. Excluded: the GL upload calls, the React/HUD layers, and
    everything host-bound (the TS `terrainByteAt` callback is modelled as the
    same closed form on both sides). 44 scenarios (`tc_` 1, `ut_` 5, `rnc_` 3,
    `spp_` 1, `stc_` 1, `rps_` 1, `grt_` 6, `ac_` 3, `arr_` 1, `nkt_` 4,
    `pst_` 6, `rmx_` 6, `trs_` 6).

66. **`client/render/frame/SpiralTrails.ts` + `client/render/frame/TrailManager.ts`
    + `client/render/frame/RailroadCache.ts` + `client/utilities/PlayerProfileUrl.ts`
    + `client/PagePin.ts` + `client/CreatorCode.ts`**
    (`spiral_trails`, `trail_manager`, `railroad_cache`, `player_profile_url`,
    `page_pin`, `creator_code`) — the S9 client cluster: three stateful
    render/frame classes and three host-bound pure-logic modules.
    `SpiralTrails` ports the ribbon/strand geometry: `setParams` clamps strands
    through `js_min(js_max(js_round(n), 1), 8)` — the JS `Math.max/min` NaN
    propagation (Rust `f64::max` would SWALLOW a NaN strand count; `js_max`/
    `js_min` in `jsnum` match V8, pinned by `stp_clamp` feeding NaN/0.4/2.5/
    7.5/9/-3/Infinity) — `update` deletes dead ribbons WHILE iterating the JS
    `Map` key snapshot and rebuilds `ribbonList` ONLY when something changed,
    and `advance` is all-f64 with `js_mod` phase and `js_hypot` radius. The
    `Math.hypot` parity is the V8 SCALED form `m*(1+t*t).sqrt()` (a naive
    `sqrt(a*a+b*b)` differs in the last bit on a large fraction of the domain,
    e.g. `hypot(7,33)`); verified over 5.59M points, 0 mismatches. `pushSample`
    grows the `Float32Array` by DOUBLING (`new Float32Array(old.length*2)` +
    copy) — `stp_growth` walks ref 0→13000 on a 100-wide map to force 261
    samples and several regrowths. `TrailManager` is the per-tile 16-bit owner
    stamp: `trailState`/`trailCounts` are `Uint16Array` so every write wraps
    through `to_uint16` (NaN/±Infinity→0) and an out-of-range or fractional
    index write is silently DROPPED while a read is `undefined`→NaN (`tlm_oob_refs`
    pins Infinity/0.5 head writes and a -1 sentinel re-entry). The killer quirk
    is `--trailCounts[ref] === 0`: the prefix-decrement EXPRESSION value is the
    ARITHMETIC `old-1` (so `0-1 === -1`, never `65535`), only the STORE wraps —
    exactly a count of `1` reaches `=== 0` and calls `stamp(ref, 0)`
    (`tlm_overlap_keeps_value` proves a shared tile survives at count 2).
    `bresenham` is pure integer f64 with the double-`if` (NOT else-if) on
    `e2 >= dy`/`e2 <= dx`, and `NUKE_TRAIL_BIT = 1<<12` ORs into the ToInt32
    owner value only for `SMOOTHED_NUKE_TYPES`. `RailroadCache` ports the
    6-variant `RailType` (inlined as a plain object; the capture never dumps
    key order) and `computeRailTiles` orientation via `railExtremity`; `apply`
    runs the GameUpdateType 16/17/18 event order Construction→Snap→Destruction
    then `tickAnimations` (two-sided head/tail advance, `RAIL_INCREMENT = 3`,
    `railroadState` is a `Uint8Array` so writes wrap, `tileRefCount` is a
    NumMap with the ASYMMETRIC `??0` increment / `??1` release (a missing
    count releases to `0` and CLEARS the tile rather than wrapping to -1).
    `removeRailroad` sets `railroadDirty` whenever it actually removes an anim
    — even when every tile survives on a shared reference and nothing visible
    changes — but an UNKNOWN id early-returns WITHOUT touching the flag;
    `rlc_snap_and_shared` clears the flag first to prove the unknown-destruct
    does not re-dirty it. `PlayerProfileUrl`
    is stateless: `ClientEnv.shareBase()` behind the `__PPU_BASE` facade +
    `encodeURIComponent` (reused from `asset_urls`), `ppu_reserved`/`ppu_unicode`
    pin the percent-encoding. `PagePin` latches the commit slug ONCE into a
    three-state `Option<Option<String>>` (unset / pinned / pinned-None); the
    `__PPN_PATH()` facade counts calls so `ppn_pinned_lazy` proves the host is
    read exactly once across repeated `pagePin()` calls, and a THROWING
    host latches `null` through the catch (the same one-shot latch —
    `ppn_throwing_host`). `CreatorCode` is
    the localStorage/Date.now/history-bound stash: `normalizeCreatorCodeInput`
    is trim→toUpperCase→`/^[A-Z0-9_-]{3,22}$/` (length-changing uppercases run
    on the ALREADY-uppercased candidate: `"ß"` → `"SS"` FAILS the 3-char floor,
    `"ﬅx"` → `"STX"` passes), `parseCreatorCodePath` decode-URI-
    component with a try/catch fallback to the raw segment, and `stash`/`take`/
    `consume`/`resume` thread a traced host-call log (event codes 74 getItem /
    75 setItem / 76 removeItem / 77 replaceState / 78 pathname / 79 search /
    80 hash / 81 Date.now / 82 open-callback) that the Rust replay re-emits
    token-for-token. `take` removes BEFORE parsing (consume-style), a non-
    numeric `stashedAt` short-circuits the `||` gate BEFORE any Date.now (no
    81 event — `ccc_take_nonnumeric_stashedat`), and the TTL is STRICT `>` so
    a sample exactly at `PENDING_CREATOR_CODE_TTL_MS` (604800000) survives
    (`ccc_take_expired_and_exact_ttl`). Excluded: the WebGL/GL upload calls, the React HUD
    layers, and everything else host-bound (all host touches ride the traced
    facades). 46 scenarios (`stp_` 7, `tlm_` 6, `rlc_` 5, `ppu_` 4, `ppn_` 6,
    `ccc_` 18).
67. **`client/render/gl/utils/NukeTrajectory.ts` + `client/PresenceGroup.ts`
    + `client/GraphicsPresets.ts` (stableStringify subset)
    + `client/hud/NameBoxCalculator.ts` (pure-geometry subset)
    + `client/utilities/GameConfigHelpers.ts` (non-DOM subset)**
    (`nuke_trajectory`, `presence_group`, `stable_stringify`,
    `name_box_calculator`, `game_config_helpers`) — the S10 client cluster:
    pure trajectory math, presence-token rules, the preset serializer subset,
    the name-box geometry and the config helpers. `NukeTrajectory` is all-f64:
    `samRange(-5)` divides by zero → `-Infinity` (JS never throws), the `clamp`
    ternary chain passes NaN through, `computeNukeControlPoints` uses a plain
    `sqrt` (NOT `Math.hypot` — the last bits differ) with `js_max(dist/3, 50)`
    and the `js_max/js_min` NaN propagation, `refineCrossing` runs the Horner
    `(((A*t+B)*t+C)*t+D+0.5)|0` evaluation (ToInt32 truncation toward zero)
    through 10 bisection steps with the false-alarm `return 1.0` fallback,
    `hasUntargetable` gates on the STRICT `distSq > 4*RANGE_SQ` (the exact
    boundary is targetable — `nt_thresholds_plain` pins 0→300), the SAM block
    walks `l2/invL2` with `maxDist = sqrt(l2)+150+0.75`, the three-way `dot`
    branch, the `(r+0.75)²` candidate gate and the `intercept < 1.0` adoption,
    and `buildNukeTrajectory` seeds `prevX/Y` from `(p0x+0.5)|0`, rounds the
    target with `js_round` and pins the `{...cpRender, ...th}` 11-key order
    through an `Object.keys` join capture (kind 5). The capture's arg slice
    `12 + 1 + (a[12]|0)*3` truncates the 15-token zero-radius SAM scenario so
    TS reads `a[15]` as `undefined`→NaN — the Rust `run_op` models
    past-the-end reads as NaN to match bit-for-bit. `PresenceGroup` ports the
    token rules: `groupTokenOf` gates STRICT on `"lobby_info"`/`"start"`,
    `loggableStartMessage` is a spread copy with `groupToken` deleted IN PLACE,
    `accept` compares with STRICT `===` (undefined onto fresh → `false`),
    `presenceLobbyId` gates `config === undefined` STRICT so a present-but-NULL
    config THROWS a TypeError upstream reading `.gameType` — the capture
    records the throw as the `[99]` sentinel and the port models it with
    `Option::None` (a non-object config boxes and reads `undefined`, passing
    through to the gameID), and `withGroupToken` returns the SAME payload
    reference for an undefined token (pinned by a `sameRef` flag; the spread
    overwrite of an existing key keeps its FIRST position). `stableStringify`
    is the preset-equality serializer: non-objects go through `JSON.stringify`
    (a top-level `undefined` returns `undefined`, not a string), arrays go
    through `map().join(",")` where an `undefined` ELEMENT renders as the
    EMPTY string (`[1,undefined]` → `"[1,]"`, unlike `JSON.stringify`'s
    `"[1,null]"`), object entries filter STRICT `v !== undefined` and sort
    with `a < b ? -1 : 1` — a UTF-16 code-unit compare, NOT `localeCompare`
    (`"Z" < "a"`) — while V8's integer-key-first own-key order rides in
    through the capture codec (`{10:_,2:_}` iterates `2,10`).
    `NameBoxCalculator` runs over a closed-form terrain facade (`ref =
    x*1000+y`, `cat = (ref*31+7)%11`) with SIX predicate call counters that
    pin the `||` short-circuit gate order of `createGrid` token-for-token;
    the grid is column-major, `findLargestInscribedRectangle` transposes
    (`rows = grid[0].length; cols = grid.length`), keeps the FIRST rectangle
    on an area tie (STRICT `>`) and emits `y: row - height + 1`;
    `largestRectangleInHistogram` is the monotone stack with the `h=0`
    sentinel column and the STRICT `<` that does NOT pop equal heights;
    `calculateFontSize` divides by the UTF-16 CODE-UNIT length (astral
    characters count 2) and clamps through `js_min`. `GameConfigHelpers`
    covers the slider tri-state (`-0 === 0` → `"disabled"`, NaN passes
    through), `toOptionalNumber`'s full JS `Number()` coercion table (`" 12abc"`
    → NaN → undefined, `"0x10"` → 16, `"0b101"` → 5, `"Infinity"` → undefined,
    NBSP/BOM trimmed but U+0085 NOT), the compact adjusters' `Math.max(0,
    Math.floor(default*0.25))` four-gate chain (a NaN default yields a NaN
    `compactCount` that compares `=== false` against everything, so only the
    nations value ever passes through), `getRandomMapType` over the 127
    `GameMapType` declaration-order names with `Math.random()` scripted through
    the `__GCH_RAND` FIFO facade (the draw echoes first in the res stream,
    out-of-domain indices read `undefined` like a JS array hole) and
    `getUpdatedDisabledUnits` which ALWAYS builds a new array. Excluded:
    `parseGraphicsOverridesJson`/`BUILTIN_PRESETS`/the migration (zod and
    `UserSettings` never enter the graph), `placeSpawnName`/`placeName` (the
    `NameMap` HUD), and the three DOM input helpers. 26 scenarios (`nt_` 5,
    `pg_` 5, `sst_` 6, `nb_` 4, `gch_` 6).

68. **`client/render/gl/SettingsUtils.ts` + `client/render/gl/Camera.ts`
    + `client/render/gl/passes/name-pass/TextLayout.ts`
    + `client/render/gl/utils/ColorUtils.ts`
    + `client/view/CosmeticVisibility.ts`
    + `client/render/gl/utils/Affiliation.ts` (CPU half)
    + `client/Utils.ts` (pure formatting + nav/time/avatar subsets)**
    (`settings_utils`, `camera`, `text_layout`, `color_utils`,
    `cosmetic_visibility`, `affiliation_palette`, `utils_format`,
    `utils_nav`) — the S11 client cluster: the whole remaining pure/stubbable
    client surface.
    `SettingsUtils` ports `deepAssign`/`deepDiff` over the codec: every
    `deepAssign` write gates on `key in target` (the prototype chain — a
    `valueOf` source key onto an empty target DOES land as a new own
    property, pinned), arrays clone wholesale (`structuredClone`), the
    recursion needs BOTH sides `typeof object && !== null`, `deepDiff` is
    driven by `Object.keys(defaults)` (arrays recurse with index-string keys:
    `deepDiff([1,2],[1,3])` → `{"1":3}`), `dv !== cv` is STRICT (`NaN !==
    NaN` records a diff, `-0 !== 0` does not) and a no-difference result is
    the JS `undefined` return. `Camera` is the stateful pan/zoom mat3 rig:
    `renderDpr()` is scripted through `__CAM_DPR` (the RAW dpr is the first
    arg of every dpr-consuming op; `|| 2` falsy gate then `Math.min(…, 2)`),
    `resize` fits ONLY while `needsInitialFit` survives (`setCameraState`
    clears it), `getMatrix` leads the PRE-CALL `dirty` flag and stores every
    entry through `Float32Array` (`m[6] = -offsetX * sx` keeps `-0`,
    Object.is-compared), NaN zoom poisons BOTH offsets through the
    NaN-propagating `js_max`/`js_min` in `clampOffset`, and a zero canvas
    width makes `sx` 0 / `tx` `-0`. `TextLayout` shapes over scripted glyph
    tables: `charCodes[i] = charCodeAt(i)` writes through a `Uint8Array`
    (surrogates truncate — `Ā` → 0, `😀`'s high unit → 61), `cursors` is a
    `Float32Array` (every write `to_float32`, the centring subtraction can
    produce `-0`), an out-of-range `Int8Array` kern read is `undefined` →
    NaN poisons every later cursor, and the empty string still runs the
    visual-bounds block reading `charCodes[-1]` → NaN. `ColorUtils` is the
    terrain RGBA encoder: `hexToRgb` trims with the JS set (U+0085 NOT
    trimmed, U+FEFF IS) before the single-`#` anchored gate, `encodeTerrainTile`
    coerces `tb` through ToInt32 (NaN → deep-water base, `-1` → peak gate
    wins), the plains branch has NO clamp (the `Uint8Array` write wraps mod
    256), overrides ride `??` (an EMPTY array does NOT fall back — missing
    channels index to NaN → 0) and `buildTerrainRGBA` allocates
    `new Uint8Array(w*h*4)` (ToIndex truncates) but loops `i < w*h` (rounds
    UP) — a fractional `w` leaves the last pixel's tail bytes AND its alpha
    unwritten. `CosmeticVisibility.visibleCosmetics` short-circuits on
    `owner === "self"` BEFORE reading `visibility`, gates `showFrom ??
    "everyone"` STRICT, returns the ONE-key `{ verified }` object (present-
    even-undefined) when hidden, deletes categories on STRICT `=== false`,
    and re-filters `effects` through the ported `cosmetic_schemas::
    effect_type_for_slot` (stale bare `"nukeExplosion"` kept) with the
    Arr/Str → `Object.fromEntries` → Obj transition and the in-place key
    position surviving. `Affiliation` ports the CPU palette half (GL stubbed:
    `createTexture2D` becomes `() => ({})`): `Math.round(v*255)` channel
    expansion with the Uint8Array wrap/zero specials, the
    `rel && lp > 0 && owner > 0 && owner < rs && lp < rs` gate (an EMPTY
    `Uint8Array` is truthy — every read OOB), fractional indices read
    `undefined` → neutral, the STRICT-`===` `setLocalPlayer` early return
    leaves `dirty` untouched, and the two-row owner loop (row 0 four-state,
    row 1 folds neutral into enemy). `Utils` contributes the pure formatting
    subset (`renderNumber`/`renderTroops`/`formatPercentage`/`normaliseMapKey`/
    `presenceMapKey`/`formatKeyForDisplay`/`formatDebugTranslation`): the
    NaN-propagating `Math.max(num, 0)` clamp lets `"NaN"` through the else
    branch while `Infinity` renders `"InfinityB"`, `fixedPoints ?? d` is a
    NULLISH gate (explicit `0` honoured half-up, `NaN` → digits 0, fractions
    truncate; digits outside `0..=100` throw RangeError — out of domain),
    the `>= 1e5` branch has NO `toFixed` (explicit fp IGNORED there),
    `formatPercentage` gates STRICT `Number.isNaN` (`-0` → unsigned
    `"0.0%"`), `normaliseMapKey` looks the display name up in the ported
    `maps_gen::MAPS` table BEFORE lowercasing (tourney ids win) and strips
    `/[\s.]+/g` (final-sigma included, U+0085 kept, U+FEFF stripped),
    `formatKeyForDisplay` recurses on `Shift+` and its fallback grows `"ß"`
    to `"SS"`, and `formatDebugTranslation` serialises `Object.entries` in
    V8 own-key order with `String(value)` over `Number::toString`. The
    follow-up `utils_nav` subset adds `apexPathFor` (the `stripVersionPrefix`
    path then the LEADING-only `/^\/w\d+\//` replace — `/v/c1/w2/w3/x` →
    `/w3/x`, ASCII `\d`, no Perl trailing-newline leniency), `currentPagePath`
    (the lazy `pagePin()` latch through the ported `PagePinState` — the
    facade-read counter pins one read per scenario, the `commit === null`
    gate is STRICT and an empty path still yields the bare `/v/<commit>`),
    the three `Date.now()` default-parameter time functions
    (`calculateServerTimeOffset`/`getServerNow`/
    `getSecondsUntilServerTimestamp` — the scripted `__UN_NOW` FIFO with the
    cumulative consumption counter in every res: an omitted argument and an
    explicit `undefined` BOTH consume, an explicit number NEVER does, and the
    seconds chain passes its evaluated `localNowMs` into `getServerNow` so
    the whole chain consumes exactly once; `Math.max(0, floor(NaN))`
    penetrates NaN through `js_max`), and `getDiscordAvatarUrl` (TRUTHY
    `avatar` gate, the ASCII `/^\d+$/` + lowercase-hex + `a_`-prefix regexes,
    `a_` → `gif`, `encodeURIComponent` reusing `asset_urls`, the STRICT
    `!== undefined` discriminator gate — `null` PASSES (`Number(null)` → 0)
    — and `Number(discriminator) % 5` over the ported `js_number` coercion
    table: `"0x10"` → 1, `"-7"` → −2 keeping the JS remainder sign, `NaN % 5`
    → `"embed/avatars/NaN.png"`). Excluded, with reasons: the
    `translateText`/`intl-messageformat` cluster (`translateText` itself,
    `getMapName`, `getGameModeLabel`, `getActiveModifiers`,
    `getModifierLabels`, `renderDuration`, `getTranslatedPlayerTeamLabel`,
    `isRTL`, `textDirection` — all bottom out in `translateText`, which needs
    the un-installed `intl-messageformat`, the DOM `lang-selector` and the
    language JSON files); `resolveTeamClanTag` + its private `getTopClans`
    (a `clanTag` may be a CALLABLE read through `typeof p.clanTag ===
    "function" ? p.clanTag() : p.clanTag`, and `getTopClans` ranks over a
    JS `Map` — a callable/black-box harness for one HUD helper, not a pure
    port); `getMessageTypeClasses` + `severityColors` (the `MessageType`
    string enum from the Game graph plus a `console.warn` side effect on the
    default branch); and every host-bound function (`copyToClipboard`,
    `createCanvas`, `generateCryptoRandomUUID`, `getSvgAspectRatio`,
    `showToast`/`showToastAfterReload`/`flushReloadToast`, `reloadForUpdate`,
    `homeHref`, `isInIframe`, `getGamesPlayed`/`incrementGamesPlayed`,
    `getModifierKey`/`getAltKey` — DOM/window/`Platform`/`sessionStorage`
    touches with no pure residue). 50 scenarios (`su_` 3, `cam_` 9, `txl_` 4,
    `cu_` 6, `cvs_` 4, `afp_` 6, `uf_` 7, `un_` 11). (The "exhausted" claim
    made here was premature — the S12 survey below found ten further pure
    client modules.)

69. **`client/AccountIdentity.ts` + `client/VersionedReplay.ts` +
    `client/GameVersion.ts` + `client/BootInterrupts.ts` +
    `client/MapLayerSettings.ts` +
    `client/render/gl/passes/fx-pass/FxSettings.ts` +
    `client/render/gl/passes/name-pass/AtlasData.ts` +
    `client/render/gl/debug/EffectEditorState.ts` + `client/PlayerName.ts` +
    the pure gates of `client/GameModeSelector.ts` /
    `client/DesktopShell.ts`**
    (`account_identity`, `versioned_replay`, `game_version`,
    `boot_interrupts`, `map_layer_settings`, `fx_settings`, `atlas_data`,
    `effect_editor_state`, `player_name`, `game_mode_gate`) — the S12 client
    batch: ten more pure modules, all stateless `run_op` runners.
    `AccountIdentity` — `isSteamPrimaryUser` guards `user?.steam` then TRUTHY
    on the rest, while `hasLinkedIdentity` uses `!== undefined` PRESENCE
    checks (a present `null` discord counts as linked!) and
    `(email ?? "") !== ""`; `responseHasLinkedIdentity` gates `!== false`
    STRICTLY. `VersionedReplay` — strict `audience === "" || === "localhost"`
    dev gate then template-concat, case-sensitive `startsWith("replay.")`.
    `GameVersion` — the UNANCHORED `/^v?\d+\.\d+\.\d+/` tail and the
    lowercase-only `startsWith("v")` prefix gate (`"V1.2.3"` fails the regex
    and gets a `v` PREPENDED → `"vV1.2.3"`); the env/DOM readers
    (`currentGameVersion`/`renderNavVersion`/`currentGitCommit`) stay in TS.
    `BootInterrupts` — the `nextBootInterrupt` gate-order contract (clean
    homepage, entitled+TEMPORARY base, entitled+`!username` TRUTHY + due +
    strings-ready, lapse, `rewardCount > 0`), the STRICT `lobbyHandle ===
    null` gate (`undefined` does NOT pass), `joinOwnsInFlightFlag` `===` on
    numbers (`NaN === NaN` false), and the claim-prompt decay store: the
    strict-JSON parser models the `JSON.parse` throw, entries rebuild through
    `map_set_v8` (canonical decimal integer keys sort NUMERICALLY FIRST), and
    `claimPromptShown`'s subtraction-comparator sort + `slice(0, 7)` prune +
    rebuild order is PINNED by the golden (scripted ties, a future-timestamp
    pin, an absent-field NaN comparator — Rust's stable `sort_by` matches
    V8's stable `Array#sort`). `MapLayerSettings` — the `?.`/`??` chains
    return the RAW codec value (a non-boolean passes `??` untouched; an
    omitted `manifestDefault` argument rides `[1]` undefined). `FxSettings` —
    the `===` unit-type switch (`Atom Bomb`/`Hydrogen Bomb`/`MIRV Warhead`),
    anything else `undefined`; a null `fx` throws in TS (outside the typed
    domain, not captured). `AtlasData` — the two CPU table builders: the
    `ch.id < CHAR_RANGE` gate lets negatives through (the typed-array element
    write at a negative/fractional index is a silently DROPPED non-index key),
    Float32 writes through `to_float32`, `Int8Array` amount writes through the
    new `to_int8` (trunc-then-mod-256, signed reinterpret), the kern dump
    SPARSE (nonzero cells only) plus the table length. `EffectEditorState` —
    strict-`===` `maxColorsFor`, the 7-key `EFFECT_EDITOR_TYPES` insertion-
    order dump, `defaultSlotState` crossing as a BARE map (the `push_map`
    convention: the capture records `encMap`, not `encVal`), `fieldsForType`
    as an insertion-order Set (re-add keeps the FIRST position); a bogus slot
    throws in TS → `[1]` undefined is the runner's `undefined` return.
    `PlayerName` — UTF-16-unit `clampUsername`/`truncateToCap` (a cut can
    SPLIT a surrogate pair), `Array.from` CODE-POINT iteration in
    `sanitizePersona` (one space per pair, never two), the JS `\s` set (U+0085
    NOT whitespace, U+FEFF IS), `looksGenerated`'s greedy-`[A-Za-z]+`
    backtracking (`AnonCat12` no, `AnonCat1` yes); the two `Date`-dependent
    functions (`verifiedClaimGrace`, `lapseNoticeDue`) take the `now`
    EXPLICITLY in the runner (JS default arguments fire only on `undefined`)
    and `new Date(iso).getTime()` is modelled by `iso_to_epoch_ms` over the
    V8 `Date.parse` domain — pinned by a 23-string kind-9 golden (date-only
    and `…Z`/`…z` are UTC, `±hhmm` without the colon is ACCEPTED, `Zjunk` is
    NaN, `+5:30` is NaN, day overflow is tolerated `2026-02-30` → Mar 2,
    fractional seconds keep the first three digits); the crypto
    (`genAnonUsername`/`fallbackPlayerName`) and NFKD
    (`sanitizeAccountPersona`) functions stay in TS; a `{}` `userMe` throws a
    TypeError in TS (outside the `UserMeResponse` domain, not captured).
    `GameModeGate` — the status/session/outage allow-list tables
    (`failedAllowsMultiplayer`'s `default` → false), `joinIsGateable`'s
    optional chain where an absent `gameStartInfo` makes `undefined !==
    "Singleplayer"` TRUE (the gate PASSES), and the ordered short-circuit
    block helpers; the lit component, the refusal feedback and the effectful
    ServerList reads stay in TS, and a `{ gameStartInfo: {} }` lobby throws
    (`?.` guards only `gameStartInfo`, not `config.gameType`) — not captured.
    31 scenarios (`ai_` 2, `vr_` 2, `gv_` 2, `bi_` 7, `mls_` 2, `fxs_` 1,
    `atd_` 3, `ees_` 3, `pn_` 6, `gms_` 3).

70. **`client/render/gl/RenderSettings.ts` +
    `client/render/gl/RenderOverrides.ts`**
    (`render_settings`, `render_overrides`) — the S13 settings-factory pair.
    The three JSON modules (`render-settings.json`, `default-theme.json`,
    `colorblind-theme.json`) are copied byte-for-byte into
    `crates/core/data/` and embedded through `include_str!` (a wasm build
    cannot read the upstream checkout), parsed once into the `JsVal` codec
    domain by the new `js_json::json_parse` (strict JSON grammar, V8
    integer-key ordering, UTF-16 surrogate-pair escapes; the embedded data
    itself has no integer keys, so the `JSON.parse(JSON.stringify(x))` deep
    copy is an identity on the codec value and a plain `clone()` is
    equivalent — pinned by a mutate-then-reread independence scenario).
    `RenderSettings` — `createThemeSettings(name = "default")`: the default
    parameter fires ONLY on `undefined`; `null` / `""` / out-of-domain strings
    read `THEMES[name]` → `undefined` → `JSON.parse(undefined)` throws
    `SyntaxError` (modelled as the `[1]` status token); `createRenderSettings`
    spreads the 24 render-settings.json keys in own-key order and APPENDS
    `theme` last (25 keys). `dumpSettings` stays in TS (Blob /
    `URL.createObjectURL` / `document` — host-bound). `GraphicsOverrides.ts`
    contributes only the `PALETTE_NAMES` constant (inlined in the loader; the
    zod schema stays in TS per the schema-exclusion precedent).
    `RenderOverrides` — `applyGraphicsOverrides` in full: every `?.Y !==
    undefined` gate is STRICT (a present `null` PASSES and lands raw),
    `classicIcons ?? true` falls back ONLY on nullish (`false`/`0`/`""` skip —
    no else branch), `showDots === false` is the strict-literal gate,
    `classicNumbers !== undefined` lets `null` through (the `??`/`!==`
    split pinned side by side), the hex gates divide `hexToRgb` channels by 255
    (reusing `color_utils`), an unparseable hex writes NOTHING and the
    function CONTINUES (no early return — pinned by a scenario where the
    later tint and palette swap both apply), a NON-string hex throws a
    TypeError inside `hexToRgb`'s `.trim()` (`[1]` with the partial mutation
    dumped), `ambient < 1` uses JS relational ToNumber (`NaN`/`Infinity`/`1`
    false, `-0`/`null`/`""`/`"0.5"` true, `true` false), `darkNames` assigns
    `!dark` as a BOOLEAN but `outlineUsePlayerColor` the RAW value, `fallout`
    is one toggle driving BOTH passes, and the `palette` swap evaluates
    `createThemeSettings` BEFORE assigning — an out-of-domain palette throws
    `SyntaxError` (`[2]`) with the earlier gates already applied and `theme`
    left intact; a valid palette replaces `theme` IN PLACE (key position
    survives). A nullish `overrides` throws on the first `?.` read before any
    mutation. 19 scenarios (`rs_` 4, `ro_` 15).

71. **`client/components/baseComponents/ranking/GameInfoRanking.ts` +
    `client/hud/Tutorial.ts` + `client/render/preview/PreviewMap.ts`**
    (`game_info_ranking`, `tutorial`, `preview_map`) — the S14 batch-1 trio.
    `GameInfoRanking` — the `Ranking` over an `AnalyticsRecord`: `hasPlayed`
    (stats gate AND the units/killedAt/conquests three-way OR, a
    present-`undefined` arm failing), the `Record` summariser keyed by
    `clientID` in **V8 own-key order** (canonical integer keys ascending
    ahead of insertion-ordered string keys — pinned by a `"5"`-before-`"a"`
    scenario; a repeat id overwrites in place), `BigInt(v ?? 0)` element
    coercion (nullish → `0n`, a non-integral finite NUMBER throws
    `RangeError` → status 1, bigints ride as `i64` in the ≤ 2^53 wire
    domain), the `killedAt === undefined || === null` double gate,
    `Number(bombs?.X?.[0]) || 0` (a non-array `bombs.abomb` reads `undefined`
    → NaN → 0), the winner-block pass (`"player"` marks index 1 through the
    truthy `players[id]` gate with `ToPropertyKey` on non-string ids,
    `"team"` marks 2.., anything else inert), the twelve-branch `getScore`
    with NO `default` (out-of-domain → `undefined`, `None` here) and the
    subtraction-comparator `sortedBy` (descending, stable — a NaN comparator
    result is V8 `+0`, pinned by the out-of-domain and NaN-killedAt
    scenarios; winner bonus `+0.1`). `RankType` is a string enum inlined as
    a plain object in the loader; `RANK_TYPE_LABEL_KEYS` dumps with
    `Lifetime` first (computed-key insertion order).
    `Tutorial` — the pure cursor: the 22-step `TUTORIAL_STEPS` table with
    every `applies`/`isDone` closure transcribed verbatim (including the
    `cityCost !== null && gold >= cityCost` bigint gate), and
    `TutorialProgress` — the `countCtx` latch on the FIRST `hasSpawned`
    context (never re-snapshotted; `position`/`total` count over
    `countCtx ?? ctx`), `doneTicks++` BEFORE the `< 15` linger return, the
    skip-`while` + `step?.isDone?.(ctx)` re-arm order inside `update`, the
    `step?.manual && doneTicks === null` acknowledge gate and the
    finished-guarded `skip`. The capture reads only the public surface
    (current id / finished / stepDone / position / total) — the scripted op
    chain pins the hidden trajectory.
    `PreviewMap` — `buildPreviewMap` (length `!== mapW * mapH` throw with
    the exact template message, undefined-only default params, `& 0x80`
    land bit, `Uint16Array` `tileState`), `previewTileRef`, and the
    module-singleton `getPreviewRailLoop`: the closed four-edge 180-tile
    rectangle fed through the now-`pub(crate)` `computeRailTiles`
    (`[path[n-1], ...path, path[0]]` wrap + `.slice(1, n+1)`),
    `railroadState[ref] = type + 1` dumped sparsely; the TS-side capture
    shim `__pmResetRailLoop` mirrors the Rust `thread_local` latch reset so
    the built-vs-cached identity is pinned. 22 scenarios (`gir_` 10, `tp_`
    7, `pm_` 5).
72. **`server/StaticAssetCache.ts` + `client/render/frame/Upload.ts` +
    `client/components/LobbyCard.ts` + `client/sound/Sounds.ts` +
    `client/components/baseComponents/stats/GameTypeLabels.ts` +
    `client/components/InputCardStyles.ts`** (`static_asset_cache`,
    `frame_upload`, `lobby_card`, `sounds`, `misc_pure`) — the S14 batch-2
    pure islands.
    `StaticAssetCache` — `stripQueryString` (first `?` truncation, `""` /
    `?` / `a?b?c` → `a`), the falsy `!urlPath` gate (`""` / `undefined` →
    no header), the `/assets/` and `/_assets/` prefixes (one letter off, no
    trailing slash, case and `assets/x` all miss) and the `setHeader` facade
    trace; `IMMUTABLE_CACHE_CONTROL` dumped verbatim.
    `Upload` — `uploadFrameData` as a `(methodId, params…)` trace over the
    16-method `FrameUploadTarget` declaration order: `changedTiles` truthy
    gate (the empty array `[]` IS truthy — enters the delta branch, then the
    `length > 0` gate skips the upload — tagged `0` falsy / `1` array),
    `trailDirtyRowMax >= 0` INSIDE that branch (NaN false, `-0` true),
    `railroadDirty` / `structuresDirty` / `relationsDirty` as truthy-num
    gates, the three event lists on independent `length > 0` gates,
    `updateNames` snap literally `false`, and the unconditional methods
    (spiral / units / rings / telegraphs / clusters) pinned.
    `LobbyCard` — `viewerIsTrusted` (the strict `!== false` first gate;
    `undefined` / `null` / a player-less object throw a `TypeError` → status
    1; a primitive `player` boxes → `Ok(false)`; only
    `trustTier === "trusted"` passes), `canJoinTrustedLobby` (the `?.`
    protects only `gameConfig`; ONLY a literal `true` defers to
    `viewerTrusted`) and `viewerIsSignedIn` delegating to the ported
    `account_identity::response_has_linked_identity`.
    `Sounds` — the 31-key `CUE_CATEGORY` table in declaration order
    (`message` → `"alerts"`), the four-key `ambienceUrls` set and
    `categoryOf` (the ambience `has` gate wins first; out-of-domain reads
    `undefined` → status 1). The `assetUrl(...)` calls become their literal
    argument in the loader (URL values irrelevant to `categoryOf`).
    `misc_pure` — `isFfa` (the `"Free For All"` strict `===` short-circuit
    vs the `mode === undefined` AND nullish-`playerTeams` fallback — a null
    WITH a mode is still a Team game) and `cardClass` (default parameter
    fires only on `undefined`; the template's double space survives).
    21 scenarios (`sac_` 4, `ufr_` 8, `lg_` 3, `snd_` 3, `mpp_` 3).

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
compares every value. Last run: **800,103 comparisons, all bit-identical**.

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
