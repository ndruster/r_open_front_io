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
│   │   ├── wasm_probe.rs          `extern "C"` surface, feature-gated
│   │   └── pathfinding/
│   │       ├── mod.rs
│   │       ├── priority_queue.rs  port of algorithms/PriorityQueue.ts
│   │       ├── flat_heap.rs       port of execution/utils/FlatBinaryHeap.ts
│   │       ├── bfs_grid.rs        port of algorithms/BFS.Grid.ts
│   │       ├── a_star.rs          port of algorithms/AStar.ts (+ GridAdapter)
│   │       └── rail.rs            port of algorithms/AStar.Rail.ts
│   │                              (+ TerrainMap: GameMapImpl's packed bytes)
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
compares every value. Last run: **19,977 comparisons, all bit-identical**.

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
| `AStar.Water` | `src/core/pathfinding/algorithms/AStar.Water.ts` | The performance-critical inlined variant; `AStar.Rail` is ported, so the water adapters are the next layer. |
| `TileSet` / `GameMap` | `src/core/game/TileSet.ts`, `GameMap.ts` | `GameMapImpl`'s terrain-byte surface is already mirrored by `rail::TerrainMap`; porting the full class lets every adapter share one map type. |
| `Util.ts` | `src/core/Util.ts` | Shared helpers (`toInt`, geometry) that the `execution/**` ports will need. |

`execution/**` and `game/**` are the bulk (~500 files, heavy on `zod` schemas,
`ApiSchemas.ts`, and worker IPC) and will want a serde/zbin schema story before
they move.
