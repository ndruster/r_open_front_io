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
compares every value. Last run: **26,638 comparisons, all bit-identical**.

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
