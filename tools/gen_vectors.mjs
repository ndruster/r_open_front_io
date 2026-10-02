// Generates golden vectors from the authoritative TypeScript implementation so
// the Rust port can be verified bit-for-bit against it.
//
//   node rust/tools/gen_vectors.mjs
//
// Outputs, both from the same computed data:
//   rust/crates/core/tests/data/vectors.rs    consumed by tests/parity_golden.rs
//   rust/crates/core/tests/data/vectors.json  consumed by run_wasm_parity.mjs
//
// Every double is emitted either as its shortest round-tripping decimal (exact
// under IEEE-754 parsing in both languages) or as raw IEEE-754 bits, so the
// Rust assertions are exact-equality checks rather than tolerance checks.
import { writeFileSync, mkdirSync } from "node:fs";
import { pathToFileURL, fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

import { loadTs, TS_ROOT } from "./ts_load.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");

const { PseudoRandom } = await import(
  pathToFileURL(join(TS_ROOT, "src/core/PseudoRandom.ts")).href
);
const DetMath = await import(pathToFileURL(join(TS_ROOT, "src/core/DetMath.ts")).href);

const dv = new DataView(new ArrayBuffer(8));
function bits(x) {
  dv.setFloat64(0, x);
  return dv.getBigUint64(0);
}
// String(v) is the shortest round-tripping decimal, so the Rust literal
// parses back to the identical double.
const f64 = (v) => `${String(v)}f64`;
const bitsArr = (xs) => xs.map((x) => bits(x).toString());

// ================================================================ compute
const seeds = [
  0, 1, 2, 7, 42, 55, 77, 99, 123, 1000, 1073, 987654, 1234, 11, 31, 5, 78, -1,
  -12345, 0.75, 42.9, 2 ** 31, 2 ** 32 - 1, 2 ** 32, 2 ** 32 + 5,
  -(2 ** 31) - 3, 1e10, 1e15, Number.MAX_SAFE_INTEGER,
];

// next() is (t >>> 0) / 2^32; the u32 numerator is an exact proxy for the
// double stream.
const STREAM = 24;
const streams = [];
for (const s of seeds) {
  const r = new PseudoRandom(s);
  for (let i = 0; i < STREAM; i++) streams.push(Math.floor(r.next() * 2 ** 32) >>> 0);
}

const ranges = [[0, 1000], [3, 8], [1, 12], [-5, 5], [1.9, 4.7], [0, 1]];
const INT_N = 40;
const intValues = [];
for (const s of seeds) {
  for (const [lo, hi] of ranges) {
    const r = new PseudoRandom(s);
    for (let i = 0; i < INT_N; i++) intValues.push(r.nextInt(lo, hi));
  }
}

const ID_SEED = 42;
const ids = [];
{
  const r = new PseudoRandom(ID_SEED);
  for (let i = 0; i < 200; i++) ids.push(r.nextID());
}

const SHUF = 12;
const shuffles = [];
for (const s of seeds) {
  const r = new PseudoRandom(s);
  shuffles.push(...r.shuffleArray(Array.from({ length: SHUF }, (_, i) => i)));
}

const CHANCE_SEED = 42;
const CHANCE_ODDS = 100;
const chanceBytes = [];
{
  const r = new PseudoRandom(CHANCE_SEED);
  let acc = 0;
  let n = 0;
  for (let i = 0; i < 256; i++) {
    acc = (acc << 1) | (r.chance(CHANCE_ODDS) ? 1 : 0);
    if (++n === 8) {
      chanceBytes.push(acc >>> 0);
      acc = 0;
      n = 0;
    }
  }
}

function sweep(from, to, step) {
  // Reproduces a JS `for (x = from; x <= to; x += step)` accumulation exactly.
  const out = [];
  for (let x = from; x <= to; x += step) out.push(x);
  return out;
}

const expX = [
  ...sweep(-700, 700, 0.37), 0, 1, -1, 0.5, 709, 710, -708, -800, 1e-9, 709.78,
  -745,
];
const logX = [
  ...sweep(-300, 300, 0.61).map((p) => 10 ** p),
  ...sweep(0.5, 4, 0.013),
  1, Math.E, 2, 5e-324, 1e-320, 2.2250738585072014e-308, 1e308, 150000, 3,
];
const powPairs = [];
{
  const bases = [0.5, 1, 2, 7.3, 100, 5000, 123456, 1e6, 4e6, 1e9, 0, 3, 0.0001];
  const exps = [0, 0.15, 0.35, 0.5, 0.6, 0.73, 1, 2, 2.5, 10];
  for (const b of bases) for (const e of exps) powPairs.push([b, e]);
}
const atanPairs = [];
for (let y = -20; y <= 20; y++) for (let x = -20; x <= 20; x++) atanPairs.push([y, x]);
for (let t = -Math.PI + 1e-6; t < Math.PI; t += 0.0137)
  atanPairs.push([1000 * Math.sin(t), 1000 * Math.cos(t)]);
atanPairs.push(
  [0, 1], [0, -1], [1, 0], [-1, 0], [0, 0],
  [1e-300, 1e-300], [1e300, 1e300], [3, 4], [-7, -2],
);
const pow2Ns = [];
for (let n = -1074; n <= 1100; n++) pow2Ns.push(n);

const snapshotCalls = [
  () => DetMath.exp(1),
  () => DetMath.exp(-12.5),
  () => DetMath.log(3),
  () => DetMath.log(150_000),
  () => DetMath.pow(50_000, 0.6),
  () => DetMath.pow(1_234_567, 0.73),
  () => DetMath.atan2(3, 4),
  () => DetMath.atan2(-7, -2),
];
const snapshotVals = snapshotCalls.map((f) => f());

// ================================================================ structures
// Op-stream traces replayed against the real TS classes. Kind numbering:
//   0 push/enqueue(node, priority)   1 pop/dequeue -> result
//   2 clear                          3 isEmpty -> result(1/0)
//   4 dequeue-expects-throw (FBH)    5 size -> result (FBH)
// Results are captured verbatim from TS so the Rust port replays the exact
// same script and must reproduce every return value and the final internal
// state. enc() maps JS-only values that JSON cannot carry:
//   undefined -> "u", NaN -> "n", true/false -> 1/0.
const { MinHeap, BucketQueue } = await loadTs(
  "src/core/pathfinding/algorithms/PriorityQueue.ts",
);
const { FlatBinaryHeap } = await loadTs("src/core/execution/utils/FlatBinaryHeap.ts");
const { BFSGrid } = await loadTs("src/core/pathfinding/algorithms/BFS.Grid.ts");
const { BFS } = await loadTs("src/core/pathfinding/algorithms/BFS.ts");
const { AirPathFinder } = await loadTs("src/core/pathfinding/PathFinder.Air.ts");
const { anonWordName } = await loadTs("src/core/AnonNames.ts");
const CloseCodes = await loadTs("src/core/CloseCodes.ts");
const SL = await loadTs("src/core/ServerList.ts");
const PD = await loadTs("src/core/PatternDecoder.ts");
const DC = await loadTs("src/core/game/DoomsdayClock.ts");
const EU = await loadTs("src/core/execution/Util.ts");
const WM = await loadTs("src/core/game/WaterManager.ts");
const GUU = await loadTs("src/core/game/GameUpdateUtils.ts");
const RR = await loadTs("src/core/game/Railroad.ts");
const RSG = await loadTs("src/core/game/RailroadSpatialGrid.ts");
const TTS = await loadTs("src/core/game/TileTraversalScratch.ts");
const EB = await loadTs("src/core/EventBus.ts");
const AU = await loadTs("src/core/AssetUrls.ts");
const MG = await loadTs("src/core/game/Maps.gen.ts");
const TN = await loadTs("src/core/execution/utils/TribeNames.ts");
const GAME = await loadTs("src/core/game/Game.ts");
const NC = await loadTs("src/core/game/NationCreation.ts");
const GUPD = await loadTs("src/core/game/GameUpdates.ts");
const NE = await loadTs("src/core/execution/nation/NationEmojiBehavior.ts");
const Cosmetic = await loadTs("src/core/CosmeticSchemas.ts");
const St = await loadTs("src/core/StatsSchemas.ts");
const Sc = await loadTs("src/core/Schemas.ts");
const Api = await loadTs("src/core/ApiSchemas.ts");
const Tml = await loadTs("src/core/game/TerrainMapLoader.ts");
const Nu = await loadTs("src/core/execution/nation/NationUtils.ts");
const GI = await loadTs("src/core/game/GameImpl.ts");
const TNI = await loadTs("src/core/game/TerraNulliusImpl.ts");
const PF = await loadTs("src/core/pathfinding/PathFinder.ts");
const { AStar } = await loadTs("src/core/pathfinding/algorithms/AStar.ts");
const { AStarRail } = await loadTs("src/core/pathfinding/algorithms/AStar.Rail.ts");
const { AStarWater } = await loadTs("src/core/pathfinding/algorithms/AStar.Water.ts");
const { AStarWaterBounded } = await loadTs(
  "src/core/pathfinding/algorithms/AStar.WaterBounded.ts",
);
const { GameMapImpl } = await loadTs("src/core/game/GameMap.ts");
const { TileSet } = await loadTs("src/core/game/TileSet.ts");
const Util = await loadTs("src/core/Util.ts");
const TeamAssignment = await loadTs("src/core/game/TeamAssignment.ts");
const { DistanceBasedBezierCurve } = await loadTs("src/core/utilities/Line.ts");
const { maxHealthWithVeterancy } = await loadTs("src/core/game/Veterancy.ts");
const { packMotionPlans, unpackMotionPlans } = await loadTs(
  "src/core/game/MotionPlans.ts",
);
const { ConnectedComponents } = await loadTs(
  "src/core/pathfinding/algorithms/ConnectedComponents.ts",
);
const { TerrainSearchMap } = await loadTs("src/core/game/TerrainSearchMap.ts");
const { AbstractGraph, AbstractGraphBuilder } = await loadTs(
  "src/core/pathfinding/algorithms/AbstractGraph.ts",
);
const { AbstractGraphAStar } = await loadTs(
  "src/core/pathfinding/algorithms/AStar.AbstractGraph.ts",
);
const { AStarWaterHierarchical } = await loadTs(
  "src/core/pathfinding/algorithms/AStar.WaterHierarchical.ts",
);

// enc() maps JS-only values JSON cannot carry: undefined -> "u", NaN -> "n",
// booleans -> 1/0, and -0 -> "-0" (JSON collapses -0 to 0, yet the f32 bit
// pattern of -0.0 is real compared state; priMix can produce it via -0).
const enc = (v) =>
  v === undefined
    ? "u"
    : v === true
      ? 1
      : v === false
        ? 0
        : Number.isNaN(v)
          ? "n"
          : Object.is(v, -0)
            ? "-0"
            : v;

// Like enc(), but also maps the infinities to tokens (JSON.stringify would
// otherwise turn them into null and the host could not tell them apart).
const uenc = (v) =>
  v === Number.POSITIVE_INFINITY
    ? "i"
    : v === Number.NEGATIVE_INFINITY
      ? "-i"
      : enc(v);

function play(inst, ops) {
  return ops.map(([k, a, b]) => {
    if (k === 0) {
      inst.push(a, b);
      return [k, enc(a), enc(b), "u"];
    }
    if (k === 1) return [k, 0, 0, enc(inst.pop())];
    if (k === 2) {
      inst.clear();
      return [k, 0, 0, "u"];
    }
    if (k === 3) return [k, 0, 0, enc(inst.isEmpty())];
    throw new Error("bad op kind " + k);
  });
}

function playFbh(inst, ops) {
  const out = [];
  for (const [k, a, b] of ops) {
    if (k === 0) {
      inst.enqueue(a, b);
      out.push([k, enc(a), enc(b), "u"]);
    } else if (k === 1) {
      // A random pop against an empty heap *throws* in TS. The scenario
      // generator must not rely on luck: silently drop pops that would hit an
      // empty heap so the recorded script is exactly replayable. (Kind 4
      // below records an intentional throw.)
      if (inst.len === 0) continue;
      out.push([k, 0, 0, enc(inst.dequeue())]);
    } else if (k === 2) {
      inst.clear();
      out.push([k, 0, 0, "u"]);
    } else if (k === 5) out.push([k, 0, 0, inst.size()]);
    else if (k === 4) {
      let threw = false;
      try {
        inst.dequeue();
      } catch (e) {
        threw = /heap empty/.test(String(e.message));
      }
      if (!threw) throw new Error("expected dequeue to throw");
      out.push([k, 0, 0, "t"]);
    } else throw new Error("bad op kind " + k);
  }
  return out;
}

const priMix = (rng) => {
  // The f32-tie and NaN paths are what make heap order observable; they must
  // appear in the trace, not just in hand-picked unit tests.
  const r = rng.next();
  if (r < 0.06) return NaN;
  if (r < 0.12) return 1.0 + 2 ** -30;
  if (r < 0.18) return -rng.nextInt(0, 5);
  return rng.nextInt(0, 100);
};

function randomOps(rng, count, { popRate, clearEvery, maxNode, pri }) {
  const ops = [];
  for (let i = 0; i < count; i++) {
    if (clearEvery && (i + 1) % clearEvery === 0) ops.push([2, 0, 0]);
    else if (rng.next() < popRate) ops.push([1, 0, 0]);
    else ops.push([0, rng.nextInt(0, maxNode), pri(rng)]);
  }
  return ops;
}

const structRng = () => new PseudoRandom(20260922);

const mhScenarios = [];
function captureMinHeap(name, cap, ops) {
  const inst = new MinHeap(cap);
  const played = play(inst, ops);
  mhScenarios.push({
    name,
    cap,
    ops: played,
    heap: Array.from(inst.heap),
    priBits: Array.from(inst.priorities, (p) => {
      dv.setFloat32(0, p);
      return dv.getUint32(0);
    }),
    size: inst.size,
    capacity: inst.capacity,
  });
}
captureMinHeap(
  "mh_random",
  8,
  randomOps(structRng(), 220, { popRate: 0.4, clearEvery: 45, maxNode: 999, pri: priMix }),
);
captureMinHeap(
  "mh_grow",
  2,
  randomOps(structRng(), 160, { popRate: 0.35, clearEvery: 0, maxNode: 500, pri: priMix }),
);
captureMinHeap("mh_underflow", 2, [
  [1], [1], [0, 5, 1], [1], [1], [0, 9, 2], [0, 3, 1], [1], [1], [1], [1],
]);

const bqScenarios = [];
function captureBucketQueue(name, maxP, ops) {
  const inst = new BucketQueue(maxP);
  const played = play(inst, ops);
  bqScenarios.push({
    name,
    maxP,
    ops: played,
    sizes: Array.from(inst.bucketSizes),
    stamps: Array.from(inst.bucketStamp),
    keys: Object.keys(inst.buckets).map(Number).sort((a, b) => a - b),
    minBucket: inst.minBucket,
    size: inst.size,
    stamp: inst.stamp,
  });
}
captureBucketQueue(
  "bq_random",
  50,
  randomOps(structRng(), 220, {
    popRate: 0.45,
    clearEvery: 55,
    maxNode: 999,
    pri: (rng) => rng.nextInt(-3, 56),
  }),
);
captureBucketQueue("bq_clamp_frac", 3, [
  [0, 1, 2.9], [0, 2, 2.1], [0, 3, 99], [0, 4, -5],
  [1], [1], [1], [1], [1], [1], [2, 0, 0], [0, 7, 0], [1], [1],
]);
captureBucketQueue("bq_negative_drift", 2, [
  [0, 7, -5], [0, 8, -3], [1], [1], [1], [2, 0, 0], [0, 1, 1], [1], [1],
]);

const fbhScenarios = [];
function captureFlatHeap(name, cap, ops) {
  const inst = new FlatBinaryHeap(cap);
  const played = playFbh(inst, ops);
  fbhScenarios.push({
    name,
    cap,
    ops: played,
    priBits: Array.from(inst.pri, (p) => {
      dv.setFloat32(0, p);
      return dv.getUint32(0);
    }),
    tiles: Array.from(inst.tiles, (t) => enc(t)),
    len: inst.len,
  });
}
captureFlatHeap(
  "fbh_random",
  4,
  randomOps(structRng(), 200, { popRate: 0.45, clearEvery: 50, maxNode: 999, pri: priMix }),
);
captureFlatHeap("fbh_throw_and_grow", 2, [
  [4], [0, 5, 1], [1], [5], [0, 6, 2], [0, 7, 3], [0, 8, 1], [1], [1], [1], [1], [4], [5],
]);

const CAP = 200; // scenarios with more visits than this are sampled, not inlined

/** (index, node, dist) triples: every visit when small, otherwise the first
 * and last 20 plus the Uint16 wrap boundary. Sampling instead of a shared
 * hash keeps one canonical implementation of "the stream" and avoids a
 * cross-language hash that must agree to the last bit. */
function pickSamples(order, dists) {
  if (order.length <= CAP) return order.map((n, i) => [i, n, dists[i]]);
  const set = new Set();
  for (let i = 0; i < 20 && i < order.length; i++) set.add(i);
  for (const i of [65_534, 65_535, 65_536, 65_537, 65_538]) if (i < order.length) set.add(i);
  for (let i = Math.max(0, order.length - 20); i < order.length; i++) set.add(i);
  return [...set].sort((x, y) => x - y).map((i) => [i, order[i], dists[i]]);
}

// --- BFSGrid scenario runner -------------------------------------------------
const bgScenarios = [];
function captureGrid(name, w, h, s0, s1, maxd, mode, blocker) {
  const g = new BFSGrid(w * h);
  const order = [];
  const dists = [];
  let found = -1;
  const valid = (n) => !(mode === 1 && n === blocker);
  const visitor = (n, d) => {
    order.push(n);
    dists.push(d);
    if (mode === 2 && n === blocker) return null;
    if (mode === 3 && n === blocker) { found = 42; return 42; }
    return undefined;
  };
  const start = s1 >= 0 ? [s0, s1] : s0;
  const r = g.search(w, h, start, maxd, valid, visitor);
  if (r !== null && r !== undefined) found = r;
  const big = order.length > CAP;
  bgScenarios.push({
    name, w, h, s0, s1,
    maxd: maxd === Infinity ? "Infinity" : String(maxd),
    mode, blocker,
    nvisits: order.length,
    // (visitIndex, node, dist) triples; full stream when small enough,
    // boundary+head+tail samples otherwise.
    samples: pickSamples(order, dists),
    order: big ? [] : order,
    dists: big ? [] : dists,
    found,
    stampAfter: g.stamp,
  });
}
captureGrid("bg_spiral3x3", 3, 3, 4, -1, Infinity, 0, -1);
captureGrid("bg_corner0", 3, 3, 0, -1, Infinity, 0, -1);
captureGrid("bg_starts2", 3, 3, 0, 8, Infinity, 0, -1);
captureGrid("bg_block_north", 3, 3, 4, -1, Infinity, 1, 1);
captureGrid("bg_reject7", 3, 3, 4, -1, Infinity, 2, 7);
captureGrid("bg_stop4", 5, 5, 12, -1, Infinity, 3, 22);
captureGrid("bg_dist1", 3, 3, 4, -1, 1, 0, -1);
// 1x70000 corridor: exercises the Uint16Array distance wrap (depth 69999 ->
// 4463). Too long to inline; verified via its visit-stream hash.
captureGrid("bg_corridor_wrap", 70_000, 1, 0, -1, Infinity, 0, -1);

// --- BFS scenario runner (generic BFS.ts) ------------------------------------
// The real TS `BFS` over a Map-keyed edge table (Map uses SameValueZero, so a
// NaN key is reachable — the Rust replay mirrors that with an is_nan match).
// The visitor records the full (node, dist) stream and implements the tri-state
// by `mode`: 0 explore-all, 1 reject `blocker` (return null), 2 found `blocker`
// (return `foundval`). `foundval` may be 0 to pin that a falsy non-null return
// still short-circuits. Starts are always passed as an array (the scalar/array
// split is not observable in BFS.ts — Array.isArray just normalises).
const bfsScenarios = [];
function captureBFS(name, edges, starts, maxd, mode, blocker, foundval) {
  const map = new Map();
  for (const [k, nb] of edges) map.set(k, nb);
  const adapter = { neighbors: (node) => (map.has(node) ? map.get(node).slice() : []) };
  const bfs = new BFS(adapter);
  const visits = [];
  const visitor = (node, dist) => {
    visits.push([node, dist]);
    if (mode === 1 && node === blocker) return null;
    if (mode === 2 && node === blocker) return foundval;
    return undefined;
  };
  const start = starts.length === 1 ? starts[0] : starts;
  const r = bfs.search(start, maxd, visitor);
  bfsScenarios.push({
    name,
    edges: edges.map(([k, nb]) => [uenc(k), nb.map(uenc)]),
    starts: starts.map(uenc),
    maxd: uenc(maxd),
    mode,
    blocker: uenc(blocker),
    foundval: uenc(foundval),
    visits: visits.map(([n, d]) => [uenc(n), uenc(d)]),
    result: r === null || r === undefined ? "u" : uenc(r),
  });
}
// 3x3 grid, N/S/W/E order (twin of the BFSGrid adapter).
const BFS_GRID3 = (() => {
  const e = [];
  for (let n = 0; n < 9; n++) {
    const nb = [];
    if (n >= 3) nb.push(n - 3);
    if (n < 6) nb.push(n + 3);
    if (n % 3 !== 0) nb.push(n - 1);
    if (n % 3 !== 2) nb.push(n + 1);
    e.push([n, nb]);
  }
  return e;
})();
captureBFS("bfs_grid3x3", BFS_GRID3, [4], Infinity, 0, -1, 0);
// Duplicate start: Set dedups, queue pushes twice -> visitor sees 0 at dist 0
// twice (the core JS-ism of the starts loop).
captureBFS("bfs_dup_start", BFS_GRID3, [0, 0], Infinity, 0, -1, 0);
captureBFS("bfs_found42", BFS_GRID3, [4], Infinity, 2, 7, 42);
// Falsy found value still short-circuits (0 !== null && 0 !== undefined).
captureBFS("bfs_found_zero", BFS_GRID3, [4], Infinity, 2, 7, 0);
// Reject 1: its children 0/2 arrive only later via 3/5 at dist 2.
captureBFS("bfs_reject1", BFS_GRID3, [4], Infinity, 1, 1, 0);
// Depth clip: dist-1 nodes still visited, their nextDist=2 > 1 clips.
captureBFS("bfs_dist1", BFS_GRID3, [4], 1, 0, -1, 0);
// NaN bound: `nextDist > NaN` is false -> never clips -> full 9 visits.
captureBFS("bfs_nan_maxd", BFS_GRID3, [4], NaN, 0, -1, 0);
// Self-loop + duplicate neighbours collapse at enqueue-time marking.
captureBFS("bfs_selfloop", [[0, [0, 1, 1, 2]]], [0], Infinity, 0, -1, 0);
// Empty start array -> queue empty -> no visits, null.
captureBFS("bfs_empty_starts", BFS_GRID3, [], Infinity, 0, -1, 0);
// Fractional / negative nodes flow through unchanged.
captureBFS("bfs_nonint", [[-1.5, [0.25]], [0.25, [-1.5, 7.75]], [7.75, []]], [-1.5], Infinity, 0, -1, 0);
// NaN nodes: two NaN starts are one Set key but two queue entries; the NaN
// neighbour of 1 is already visited, so it is not re-enqueued.
captureBFS("bfs_nan_nodes", [[NaN, [1]], [1, [NaN]]], [NaN, NaN], Infinity, 0, -1, 0);
// Two disconnected components seeded by a multi-start.
captureBFS("bfs_multi_comp", [[10, [11]], [11, []], [20, [21]], [21, []]], [10, 20], Infinity, 0, -1, 0);

// --- AirPathFinder scenario runner (PathFinder.Air.ts) -----------------------
// The real TS AirPathFinder over a GameMap-backed `game` stub (ticks/x/y/ref).
// Records the walked path as (x, y) coordinate pairs (tile refs are map-width
// dependent; coords are the stable cross-language observable). `multi` drives
// the Array.isArray throw branch; an out-of-range goal makes the final
// `game.ref` throw, recorded as threw=true.
const airScenarios = [];
const gm_ref = (w, _h, x, y) => y * w + x; // GameMap.ref for valid coords
function captureAir(name, w, h, ticks, fromTile, toTile, multi) {
  const gm = new GameMapImpl(w, h, new Uint8Array(w * h).fill(0x83), w * h);
  const game = {
    ticks: () => ticks,
    x: (t) => gm.x(t),
    y: (t) => gm.y(t),
    ref: (x, y) => gm.ref(x, y),
  };
  const pf = new AirPathFinder(game);
  let threw = false;
  let path = [];
  try {
    const from = multi ? [fromTile] : fromTile;
    const r = pf.findPath(from, toTile);
    path = r.map((t) => [gm.x(t), gm.y(t)]);
  } catch {
    threw = true;
  }
  airScenarios.push({
    name,
    w,
    h,
    ticks: uenc(ticks),
    from: uenc(fromTile),
    to: uenc(toTile),
    multi,
    threw,
    path,
  });
}
captureAir("air_same", 10, 10, 0, gm_ref(10, 10, 3, 4), gm_ref(10, 10, 3, 4), false);
captureAir("air_vertical", 10, 10, 0, gm_ref(10, 10, 5, 1), gm_ref(10, 10, 5, 8), false);
captureAir("air_horizontal", 10, 10, 123, gm_ref(10, 10, 1, 6), gm_ref(10, 10, 7, 6), false);
captureAir("air_diag_42", 16, 16, 42, gm_ref(16, 16, 2, 2), gm_ref(16, 16, 9, 12), false);
captureAir("air_diag_0", 16, 16, 0, gm_ref(16, 16, 2, 2), gm_ref(16, 16, 9, 12), false);
captureAir("air_diag_neg", 16, 16, -1, gm_ref(16, 16, 2, 2), gm_ref(16, 16, 9, 12), false);
captureAir("air_diag_frac", 16, 16, 0.75, gm_ref(16, 16, 2, 2), gm_ref(16, 16, 9, 12), false);
captureAir("air_diag_777", 20, 20, 777, gm_ref(20, 20, 1, 1), gm_ref(20, 20, 15, 10), false);
// Array.isArray(from) throws before any walk.
captureAir("air_multi", 10, 10, 0, gm_ref(10, 10, 1, 1), gm_ref(10, 10, 5, 5), true);
// Goal with an out-of-range y (tile 100 on a 10x10 map -> y=10): the vertical
// walk steps into y=10 and game.ref throws.
captureAir("air_oob", 10, 10, 0, gm_ref(10, 10, 0, 0), 100, false);

// --- AnonNames scenario runner ----------------------------------------------
// Exercises the real AnonNames.ts anonWordName(slot, offset) and records the
// returned handle. `has`: 0 = offset defaulted (TS default param), 1 =
// explicit. slot/offset carry the uenc tokens (n/-0/i/-i) so NaN/-0/±Inf
// survive JSON. A word-lookup miss with round === 0 returns the JS `undefined`
// *value*, recorded as "u". The Rust twin is `anon_names::anon_word_name`.
const anonScenarios = [];
function captureAnon(name, slot, offset) {
  const has = offset === undefined ? 0 : 1;
  const r = anonWordName(slot, offset);
  anonScenarios.push({
    name,
    slot: uenc(slot),
    has,
    offset: has ? uenc(offset) : 0,
    res: r === undefined ? "u" : r,
  });
}
// Full first-round sweep: every bare word, in bank order.
for (let i = 0; i < 125; i++) captureAnon(`anon_w${i}`, i, undefined);
// Round suffixes past the bank.
captureAnon("anon_r125", 125, undefined);
captureAnon("anon_r250", 250, undefined);
captureAnon("anon_r251", 251, undefined);
captureAnon("anon_r374", 374, undefined);
captureAnon("anon_r375", 375, undefined);
captureAnon("anon_r500", 500, undefined);
captureAnon("anon_r1000", 1000, undefined);
// Negatives (abs first) and -0.
captureAnon("anon_neg1", -1, undefined);
captureAnon("anon_neg125", -125, undefined);
captureAnon("anon_neg126", -126, undefined);
captureAnon("anon_neg0", -0, undefined);
// Offsets: rotate, wrap, negative, huge, explicit zero/undefined.
captureAnon("anon_off1", 0, 1);
captureAnon("anon_off_wrap", 124, 1);
captureAnon("anon_off_neg", 10, -3);
captureAnon("anon_off_big", 120, 120);
captureAnon("anon_off_round", 200, 100);
captureAnon("anon_off_undef", 5, undefined);
captureAnon("anon_off_zero", 5, 0);
// Fractional inputs truncate toward zero.
captureAnon("anon_frac_09", 0.9, undefined);
captureAnon("anon_frac_neg05", -0.5, undefined);
captureAnon("anon_frac_1257", 125.7, undefined);
captureAnon("anon_frac_both", 2.5, 3.5);
captureAnon("anon_frac_round", 125.9, 0.1);
// Non-finite: the word lookup misses ("undefined") and the round suffix
// stringifies NaN/Infinity.
captureAnon("anon_nan", NaN, undefined);
captureAnon("anon_inf", Infinity, undefined);
captureAnon("anon_ninf", -Infinity, undefined);
captureAnon("anon_off_nan", 0, NaN);
captureAnon("anon_off_inf", 0, Infinity);
captureAnon("anon_off_ninf", 0, -Infinity);
captureAnon("anon_both_nan", NaN, NaN);
captureAnon("anon_nan_off5", NaN, 5);
captureAnon("anon_inf_off1", Infinity, 1);
// Huge slots: exact-integer range up to 2^53, then beyond (still exact
// integers for these magnitudes; the suffix must print plain digits).
captureAnon("anon_1e16", 1e16, undefined);
captureAnon("anon_1e17", 1e17, undefined);
captureAnon("anon_1e18", 1e18, undefined);
captureAnon("anon_1e18_128", 1e18 + 128, undefined);
captureAnon("anon_1e20", 1e20, undefined);
captureAnon("anon_5_1e18", 5, 1e18);
captureAnon("anon_2p53m1", 9007199254740991, undefined);
captureAnon("anon_2p53", 9007199254740992, undefined);

// --- CloseCodes scenario runner -----------------------------------------------
// Exercises the real CloseCodes.ts predicates. kind 0 = isTerminalClose(code)
// with `code` as a uenc token; kind 1 = isCloseReason(value) with `val`.
// The Rust twin is `close_codes::{is_terminal_close, is_close_reason}`.
const closeScenarios = [];
function captureCloseCode(name, code) {
  closeScenarios.push({
    name,
    kind: 0,
    code: uenc(code),
    val: "",
    res: CloseCodes.isTerminalClose(code),
  });
}
function captureCloseReason(name, val) {
  closeScenarios.push({
    name,
    kind: 1,
    code: 0,
    val,
    res: CloseCodes.isCloseReason(val),
  });
}
// Every declared code, plus the range boundaries and the JS-ism edges.
for (const [k, v] of Object.entries(CloseCodes.CloseCode)) captureCloseCode(`cc_${k}`, v);
captureCloseCode("cc_below_min", 3999);
captureCloseCode("cc_range_min", 4000);
captureCloseCode("cc_range_max", 4999);
captureCloseCode("cc_above_max", 5000);
captureCloseCode("cc_frac_min", 4000.5);
captureCloseCode("cc_frac_max", 4998.75);
captureCloseCode("cc_frac_1000", 1000.5);
captureCloseCode("cc_zero", 0);
captureCloseCode("cc_neg_zero", -0);
captureCloseCode("cc_neg", -1000);
captureCloseCode("cc_nan", NaN);
captureCloseCode("cc_inf", Infinity);
captureCloseCode("cc_ninf", -Infinity);
// Every declared reason, plus membership edges. (Scenario names double as the
// Rust const ids, so the dots become underscores.)
for (const r of Object.values(CloseCodes.CloseReason))
  captureCloseReason(`cr_${r.replace(/\./g, "_")}`, r);
captureCloseReason("cr_empty", "");
captureCloseReason("cr_case", "Close_Reason.Unknown");
captureCloseReason("cr_prefix", "close_reason");
captureCloseReason("cr_suffix", "close_reason.unknown ");
captureCloseReason("cr_unknown_word", "close_reason.does_not_exist");

// --- ServerList scenario runner ----------------------------------------------
// Exercises the real ServerList.ts pure functions (the zod schemas are not
// ported). Each scenario is one `run_op(kind, args)` call: args and res are
// flat f64 token streams — a string is [len, u0, ..], an input optional string
// is [0] / [1, string], an output string|null is [-1] / [len, u0, ..], a list
// is [n, (letter, host, numWorkers, version, state)*n] with state
// 0=open/1=draining/2=fenced. Numeric args that can be JS-only values go
// through uenc. The Rust twin is `server_list::*`.
const slScenarios = [];
const encS = (s) => [
  s.length,
  ...Array.from({ length: s.length }, (_, i) => s.charCodeAt(i)),
];
const encIn = (v) => (v === undefined || v === null ? [0] : [1, ...encS(v)]);
const encOut = (v) => (v === null || v === undefined ? [-1] : encS(v));
const encList = (entries) => [
  entries.length,
  ...entries.flatMap(([l, e]) => [
    ...encS(l),
    ...encS(e.host),
    e.numWorkers,
    ...encS(e.version),
    e.state,
  ]),
];
function captureSL(name, kind, args, res) {
  slScenarios.push({ name, kind, args: args.flat().map(uenc), res: res.flat().map(uenc) });
}
const SL_C1 = "a1b2c3d4e5f60718293a4b5c6d7e8f9012345678";
const SL_C7 = "a1b2c3d";
const SL_C2 = "deadbeef1234567890abcdef1234567890abcd";
const SL_LIST = [
  ["a", { host: "a.example.com", numWorkers: 1, version: SL_C1, state: 0 }],
  ["b", { host: "B.EXAMPLE.COM", numWorkers: 2, version: SL_C2, state: 1 }],
  ["c", { host: "c.example.com", numWorkers: 1, version: "00112233445566778899aabbccddeeff00112233", state: 2 }],
];
// The same list with the zod enum's *string* states, for calling the TS
// functions (the token stream above keeps the numeric encoding).
const SL_STATE_NAMES = ["open", "draining", "fenced"];
const slTsList = () => ({
  servers: Object.fromEntries(
    SL_LIST.map(([l, e]) => [l, { ...e, state: SL_STATE_NAMES[e.state] }]),
  ),
});
// kind 0/1: the two shape predicates.
{
  let i = 0;
  for (const v of [SL_C7, SL_C1, SL_C1 + "0", "abcdef", "ABCDEF0", "abcdefg",
    "0x12345", "", "a1b2c3é", "é1b2c3d4", "a1b2c3d", "A1B2C3D", "ffffff",
    "0000000", "1234567890123456789012345678901234567890"])
    captureSL(`sl_commit_${i++}`, 0, [encS(v)], [SL.isCommitLike(v)]);
}
// kind 1: site shapes.
{
  let i = 0;
  for (const v of ["a", "a.b", "a..b", "a-", "-a", "A", "a_b", ".", "..",
    "x".repeat(253), "x".repeat(254), "a-b-c", "1.2.3", "-.", ".-", "a.b.c.d",
    "abcÉ", "a b"])
    captureSL(`sl_site_${i++}`, 1, [encS(v)], [SL.isSiteLike(v)]);
}
// kind 2: commitsMatch prefix / identity edges.
{
  let i = 0;
  for (const [a, b] of [[SL_C7, SL_C1], [SL_C1, SL_C7], ["A1B2C3D", SL_C1],
    ["a1b2c3e", SL_C1], ["DEV", "DEV"], ["DEV", "dev"], [SL_C7, "a1b2c3"],
    ["", ""], [SL_C1, SL_C1], ["deadbee", SL_C2], ["DEADBEE", SL_C2],
    ["a1b2c3d4", "a1b2c3d"]])
    captureSL(`sl_cm_${i++}`, 2, [...encS(a), ...encS(b)], [SL.commitsMatch(a, b)]);
}
// kind 3: versionMatches (unlabeled builds match anything).
{
  let i = 0;
  for (const [own, ver] of [["DEV", SL_C1], ["desktop", SL_C1], [SL_C7, SL_C1],
    [SL_C1, SL_C2], [SL_C1, SL_C1], ["dev", SL_C1], [SL_C2, SL_C2],
    ["abcdef", SL_C1], ["", ""]])
    captureSL(`sl_vm_${i++}`, 3, [...encS(own), ...encS(ver)], [SL.versionMatches(own, ver)]);
}
// kind 4: servesBuild per letter (present / absent / fenced / draining).
{
  let i = 0;
  for (const [letter, own] of [["a", SL_C1], ["b", SL_C2], ["c", "00112233445566778899aabbccddeeff00112233"],
    ["z", SL_C1], ["a", SL_C2], ["b", "DEV"], ["c", "DEV"], ["toString", SL_C1]])
    captureSL(`sl_serves_${i++}`, 4, [...encList(SL_LIST), ...encS(letter), ...encS(own)],
      [SL.servesBuild(slTsList(), letter, own)]);
}
// kind 5: pickServerForBuild. pick kind 0 const arg, 1 n-1, 2 n, 3 -1.
{
  const slPick = (k, a) => (n) => (k === 0 ? a : k === 1 ? n - 1 : k === 2 ? n : -1);
  let i = 0;
  for (const [own, pk, pa] of [
    [SL_C1, 0, 0], [SL_C1, 0, 5], [SL_C1, 0, NaN], [SL_C1, 0, 0.5],
    [SL_C1, 0, -3], [SL_C1, 0, -0], [SL_C1, 0, Infinity], [SL_C1, 1, 0],
    [SL_C1, 2, 0], [SL_C1, 3, 0], [SL_C2, 0, 0], ["DEV", 0, 1],
    [SL_C7, 0, 0], ["00112233445566778899aabbccddeeff00112233", 0, 0],
  ]) {
    const list = slTsList();
    const got = SL.pickServerForBuild(list, own, slPick(pk, pa));
    captureSL(`sl_pick_${i++}`, 5, [...encList(SL_LIST), ...encS(own), pk, uenc(pa)],
      [encOut(got)]);
  }
}
// kind 6: ownLetterIn host / letter precedence.
{
  let i = 0;
  for (const [host, letter] of [["A.EXAMPLE.COM", "b"], ["nope", "b"], ["", "b"],
    [undefined, "b"], [undefined, "z"], [undefined, undefined], ["b.example.com", "a"],
    ["a.example.com", undefined], ["", undefined], ["", "z"]])
    captureSL(`sl_own_${i++}`, 6, [...encList(SL_LIST), ...encIn(host), ...encIn(letter)],
      [encOut(SL.ownLetterIn(slTsList(), host, letter))]);
}
// kind 7: stripVersionPrefix.
{
  let i = 0;
  for (const p of [`/v/${SL_C1}/game/5`, `/v/${SL_C7}`, "/v//x", "/v/", "/game/5",
    "/v/abc", "/V/x", `/v/${SL_C1}`, "/v/a/b/c", "/v/%20/x", "/v/x/"]) {
    const r = SL.stripVersionPrefix(p);
    captureSL(`sl_strip_${i++}`, 7, [...encS(p)], [encOut(r.commit), encS(r.path)]);
  }
}
// kind 8: shortCommit.
{
  let i = 0;
  for (const v of [SL_C1, SL_C1.toUpperCase(), SL_C7, "DEV", "abcdefg", "abcdef",
    "A1B2C3D4E5F6", "0000000"])
    captureSL(`sl_short_${i++}`, 8, [...encS(v)], [encS(SL.shortCommit(v))]);
}
// kind 9: versionedPath (loop guard + worker strip + search).
{
  let i = 0;
  for (const [commit, pathname, search] of [
    [SL_C1, `/v/${SL_C7}/game/5`, ""], [SL_C1, "/w12/game/5", "?lobby"],
    ["DEV", "/game/5", ""], [SL_C1, `/v/${SL_C1}/game/5`, ""],
    [SL_C2, "/w1/game/9", "?spectate"], ["abcdef", "/v/zzz/x", ""],
    [SL_C1, "/w/game/5", ""], [SL_C1, "/w12x/game/5", ""],
  ])
    captureSL(`sl_vp_${i++}`, 9, [...encS(commit), ...encS(pathname), ...encS(search)],
      [encOut(SL.versionedPath(commit, pathname, search))]);
}
// kind 10: pathNamesGame (decode edges).
{
  let i = 0;
  for (const [p, id] of [["/game/abc", "abc"], ["/w12/game/abc", "abc"],
    ["/game/abc?x", "abc"], ["/game/abc/def", "abc"], ["/game/", "abc"],
    ["/GAME/abc", "abc"], ["/w/game/abc", "abc"], ["/game/%41", "A"],
    ["/game/%41", "%41"], ["/game/%zz", "%zz"], ["/game/%", "%"],
    ["/game/%C0%80", "%C0%80"], ["/game/%E4%B8%AD", "中"],
    ["/game/%F0%9F%98%80", "😀"], ["/game/%2F", "/"], ["/game/%D8%80", "%D8%80"],
    ["/game/%E0%80%80", "%E0%80%80"], ["/game/%ED%A0%80", "%ED%A0%80"],
    ["/game/%FF", "%FF"], ["/game/%25", "%"], ["/game/a%2Fb", "a/b"],
    ["/game/x", "x"], ["/w7/game/%41", "A"], ["/w12", "abc"],
    ["/game/abc#frag", "abc"], ["/game/", ""], ["/game/%E4%B8", "%E4%B8"]])
    captureSL(`sl_png_${i++}`, 10, [...encS(p), ...encS(id)], [SL.pathNamesGame(p, id)]);
}
// kind 11: versionedPathForGame.
{
  let i = 0;
  for (const [own, gv, gid, vfp, pathname, search, spec] of [
    ["DEV", SL_C2, "5", "/game/5", "/game/5", "", false],
    [SL_C1, SL_C2, "5", "/game/5", "/game/5", "", false],
    [SL_C1, SL_C2, "5", "/game/5", "/v/deadbee/game/5", "", false],
    [SL_C1, SL_C2, "5", "/game/5", "/game/OTHER", "", true],
    [SL_C1, undefined, "5", "/game/5", "/game/5", "", false],
    [SL_C1, SL_C1, "5", "/game/5", "/game/5", "", false],
    [SL_C1, SL_C2, "5", "/game/5", "/w12/game/5", "?lobby", false],
    [SL_C1, SL_C2, "5", "/game/5", "/v/zzz/game/5", "", true],
    ["abcdef", SL_C2, "9", "/game/9", "/home", "?x", false],
    [SL_C1, "DEV", "5", "/game/5", "/game/5", "", false],
  ])
    captureSL(`sl_vpf_${i++}`, 11, [...encS(own), ...encIn(gv), ...encS(gid), ...encS(vfp),
      ...encS(pathname), ...encS(search), spec ? 1 : 0],
      [encOut(SL.versionedPathForGame(own, gv, gid, vfp, pathname, search, spec))]);
}

// --- GameUpdateUtils scenario runner ------------------------------------------
// Exercises the real GameUpdateUtils.ts diff / apply / pack through the shared
// run_op runner. A PlayerUpdate crosses as its interface-order token stream:
// the required id string, then per field a three-state primitive `[0]`
// (undefined) / `[1]` (null) / `[2, v]` (bool 0/1, string `[len,u..]`), or an
// array `[0]` / `[2, refid, len, elems…]`. `refid` models JS reference
// identity — arrays sharing one object share one refid, which is the
// comparators' `a === b` fast path (NaN inside a shared array stays "equal").
// Attack `[attackerID, targetID, troops, id, retreating]`, alliance
// `[id, other, createdAt, expiresAt, hasExtensionRequest]`, emoji
// `[message, senderID, recipient, createdAt]` with recipient `[0]`
// ("AllPlayers") / `[1, n]`. kind 0 = diff(prev,next) → `[0]` (null) or
// `[1, id, (fieldIdx, valueEnc)…]` in setIfDifferent order; kind 1 =
// applyStateUpdate(target,pu) → post-merge PlayerState stream (arrays without
// refid); kind 2 = packAttackTroopDeltas(prev,next,owner,dir) → `[len, …]`.
const guScenarios = [];
function captureGU(name, kind, args, res) {
  guScenarios.push({ name, kind, args: args.flat().map(uenc), res: res.flat().map(uenc) });
}
const GUA = (ref, items) => ({ __ref: ref, items });

const GU_PU_FIELDS = [
  ["id", "sid"],
  ["clientID", "s"], ["name", "s"], ["displayName", "s"], ["clanTag", "s"],
  ["nationFlag", "s"], ["team", "s"], ["smallID", "n"], ["playerType", "s"],
  ["isAlive", "b"], ["isDisconnected", "b"], ["killedBy", "s"],
  ["deathPosition", "n"], ["tilesOwned", "n"], ["gold", "n"], ["tradeGold", "n"],
  ["trainGold", "n"], ["piracyGold", "n"], ["goldEarned", "n"], ["troops", "n"],
  ["allies", "numarr"], ["embargoes", "setarr"], ["isTraitor", "b"],
  ["traitorRemainingTicks", "n"], ["inDoomsdayClock", "b"], ["isDecaying", "b"],
  ["markedDoomsdayClockTick", "n"], ["targets", "numarr"],
  ["outgoingEmojis", "emojarr"], ["outgoingAttacks", "atkarr"],
  ["incomingAttacks", "atkarr"], ["outgoingAllianceRequests", "strarr"],
  ["alliances", "allarr"], ["hasSpawned", "b"], ["spawnTile", "n"],
  ["betrayals", "n"], ["lastDeleteUnitTick", "n"], ["isLobbyCreator", "b"],
];
const GU_DIFF_ORDER = [
  ["clientID", 0, "s"], ["name", 1, "s"], ["displayName", 2, "s"],
  ["clanTag", 3, "s"], ["nationFlag", 4, "s"], ["team", 5, "s"],
  ["smallID", 6, "n"], ["playerType", 7, "s"], ["isAlive", 8, "b"],
  ["isDisconnected", 9, "b"], ["killedBy", 10, "s"], ["deathPosition", 11, "n"],
  ["tradeGold", 12, "n"], ["trainGold", 13, "n"], ["piracyGold", 14, "n"],
  ["isTraitor", 15, "b"], ["traitorRemainingTicks", 16, "n"],
  ["inDoomsdayClock", 17, "b"], ["markedDoomsdayClockTick", 18, "n"],
  ["isDecaying", 19, "b"], ["hasSpawned", 20, "b"], ["spawnTile", 21, "n"],
  ["betrayals", 22, "n"], ["lastDeleteUnitTick", 23, "n"],
  ["isLobbyCreator", 24, "b"], ["allies", 25, "numarr"], ["targets", 26, "numarr"],
  ["outgoingAllianceRequests", 27, "strarr"], ["embargoes", 28, "setarr"],
  ["outgoingEmojis", 29, "emojarr"], ["outgoingAttacks", 30, "atkarr"],
  ["incomingAttacks", 31, "atkarr"], ["alliances", 32, "allarr"],
];
const GU_STATE_ORDER = [
  ["isAlive", "b"], ["isDisconnected", "b"], ["killedBy", "s"],
  ["deathPosition", "n"], ["tilesOwned", "n"], ["gold", "n"], ["tradeGold", "n"],
  ["trainGold", "n"], ["piracyGold", "n"], ["goldEarned", "n"], ["troops", "n"],
  ["isTraitor", "b"], ["traitorRemainingTicks", "n"], ["inDoomsdayClock", "b"],
  ["markedDoomsdayClockTick", "n"], ["isDecaying", "b"], ["betrayals", "n"],
  ["hasSpawned", "b"], ["spawnTile", "n"], ["lastDeleteUnitTick", "n"],
  ["allies", "numarr"], ["targets", "numarr"],
  ["outgoingAllianceRequests", "strarr"], ["outgoingAttacks", "atkarr"],
  ["incomingAttacks", "atkarr"], ["alliances", "allarr"], ["outgoingEmojis", "emojarr"],
];

const guTokPrim = (kind, v) =>
  v === undefined ? [0] : v === null ? [1]
    : kind === "b" ? [2, v ? 1 : 0]
    : kind === "s" ? [2, ...encS(v)]
    : [2, v];

const guElKey = (kind) => (kind === "setarr" ? "str" : kind.slice(0, -3));

const guTokEl = (key, e) =>
  key === "num" ? [e]
    : key === "str" ? encS(e)
    : key === "atk" ? [e.attackerID, e.targetID, e.troops, ...encS(e.id), e.retreating ? 1 : 0]
    : key === "all" ? [e.id, ...encS(e.other), e.createdAt, e.expiresAt, e.hasExtensionRequest ? 1 : 0]
    : [...encS(e.message), e.senderID, ...(e.recipientID === "AllPlayers" ? [0] : [1, e.recipientID]), e.createdAt];

const guItems = (kind, v) => (v instanceof Set ? [...v] : v);

function guBuildPU(spec, shared) {
  const js = { type: 2, id: spec.id };
  for (const [k, kind] of GU_PU_FIELDS) {
    if (k === "id" || !(k in spec)) continue;
    const v = spec[k];
    if (!kind.endsWith("arr")) {
      js[k] = v;
    } else if (v !== undefined) {
      if (!shared.has(v.__ref)) {
        shared.set(v.__ref, kind === "setarr" ? new Set(v.items) : v.items);
      }
      js[k] = shared.get(v.__ref);
    }
  }
  return js;
}

function guTokPU(spec) {
  const t = [...encS(spec.id)];
  for (const [k, kind] of GU_PU_FIELDS) {
    if (k === "id") continue;
    const v = spec[k];
    if (!kind.endsWith("arr")) {
      t.push(...guTokPrim(kind, v));
    } else if (v === undefined) {
      t.push(0);
    } else {
      const items = kind === "setarr" ? [...new Set(v.items)] : v.items;
      const key = guElKey(kind);
      t.push(2, v.__ref, items.length, ...items.flatMap((e) => guTokEl(key, e)));
    }
  }
  return t;
}

function guRefOf(shared, arr) {
  for (const [r, a] of shared) if (a === arr) return r;
  throw new Error("gu: unknown array ref");
}

function guTokDiff(d, shared) {
  if (d === null) return [0];
  const t = [1, ...encS(d.id)];
  for (const [k, idx, kind] of GU_DIFF_ORDER) {
    if (!(k in d)) continue;
    const v = d[k];
    if (!kind.endsWith("arr")) t.push(idx, ...guTokPrim(kind, v));
    else if (v === undefined) t.push(idx, 0);
    else {
      const items = guItems(kind, v);
      const key = guElKey(kind);
      t.push(idx, 2, guRefOf(shared, v), items.length, ...items.flatMap((e) => guTokEl(key, e)));
    }
  }
  return t;
}

function guTokState(st) {
  const t = [];
  for (const [k, kind] of GU_STATE_ORDER) {
    const v = st[k];
    if (!kind.endsWith("arr")) t.push(...guTokPrim(kind, v));
    else if (v === undefined) t.push(0);
    else {
      const items = guItems(kind, v);
      const key = guElKey(kind);
      t.push(1, items.length, ...items.flatMap((e) => guTokEl(key, e)));
    }
  }
  return t;
}

function guDiff(name, prevSpec, nextSpec) {
  const shared = new Map();
  const prev = guBuildPU(prevSpec, shared);
  const next = guBuildPU(nextSpec, shared);
  const d = GUU.diffPlayerUpdate(prev, next);
  captureGU(name, 0, [...guTokPU(prevSpec), ...guTokPU(nextSpec)], guTokDiff(d, shared));
}

function guApply(name, stateSpec, puSpec) {
  const shared = new Map();
  const target = { ...stateSpec };
  const pu = guBuildPU(puSpec, shared);
  GUU.applyStateUpdate(target, pu);
  captureGU(name, 1, [...guTokState(stateSpec), ...guTokPU(puSpec)], guTokState(target));
}

function guPack(name, prevA, nextA, owner, dir) {
  const shared = new Map();
  const mk = (a) => {
    if (a === undefined) return undefined;
    if (!shared.has(a.__ref)) shared.set(a.__ref, a.items);
    return shared.get(a.__ref);
  };
  const prev = mk(prevA);
  const next = mk(nextA);
  const out = [];
  GUU.packAttackTroopDeltas(prev, next, owner, dir, out);
  const tokArr = (a) =>
    a === undefined ? [0] : [2, a.__ref, a.items.length, ...a.items.flatMap((e) => guTokEl("atk", e))];
  captureGU(name, 2, [...tokArr(prevA), ...tokArr(nextA), owner, dir], [out.length, ...out]);
}

const guAtk = (troops, id = "a", ret = false, at = 1, tg = 2) => ({
  attackerID: at, targetID: tg, troops, id, retreating: ret,
});
const guAll = (id = 1, other = "p2", ca = 10, ex = 20, ext = false) => ({
  id, other, createdAt: ca, expiresAt: ex, hasExtensionRequest: ext,
});
const guEmo = (msg = "hi", snd = 1, rcv = 2, ca = 5) => ({
  message: msg, senderID: snd, recipientID: rcv, createdAt: ca,
});

// A fully-populated spec. `off` shifts every array refid so two builds with
// different offsets hold structurally-equal but *distinct* JS arrays; equal
// offsets share one object per refid (the `a === b` fast path).
const GU_FULL = (over = {}, off = 0) => ({
  id: "p1", clientID: "c1", name: "Alice", displayName: "ali", clanTag: "CT",
  nationFlag: "en", team: "Red", smallID: 7, playerType: "HUMAN", isAlive: true,
  isDisconnected: false, killedBy: null, deathPosition: null, tilesOwned: 100,
  gold: 50, tradeGold: 3, trainGold: 2, piracyGold: 1, goldEarned: 60, troops: 25,
  allies: GUA(1 + off, [2, 3]), embargoes: GUA(2 + off, ["e1"]), isTraitor: false,
  traitorRemainingTicks: 0, inDoomsdayClock: false, isDecaying: false,
  markedDoomsdayClockTick: 0, targets: GUA(3 + off, [9]),
  outgoingEmojis: GUA(4 + off, [guEmo()]),
  outgoingAttacks: GUA(5 + off, [guAtk(10, "a1")]),
  incomingAttacks: GUA(6 + off, [guAtk(7, "a2", true, 3, 1)]),
  outgoingAllianceRequests: GUA(7 + off, ["r1"]),
  alliances: GUA(8 + off, [guAll()]),
  hasSpawned: true, spawnTile: 42, betrayals: 0, lastDeleteUnitTick: 1,
  isLobbyCreator: true,
  ...over,
});

// --- diff (kind 0) ---
guDiff("gu_diff_identical", GU_FULL(), GU_FULL());
guDiff("gu_diff_struct_equal", GU_FULL(), GU_FULL({}, 100));
guDiff("gu_diff_nan_shared", GU_FULL({ allies: GUA(1, [NaN]) }), GU_FULL({ allies: GUA(1, [NaN]) }));
guDiff("gu_diff_nan_refs", GU_FULL({ allies: GUA(1, [NaN]) }), GU_FULL({ allies: GUA(101, [NaN]) }));
guDiff("gu_diff_id_only", GU_FULL(), GU_FULL({ id: "p2" }));
guDiff("gu_diff_multi", GU_FULL(), GU_FULL({ name: "Bob", tradeGold: 4, isAlive: false }, 100));
guDiff("gu_diff_embargoes_order", GU_FULL({ embargoes: GUA(2, ["e1", "e2"]) }), GU_FULL({ embargoes: GUA(102, ["e2", "e1"]) }, 100));
guDiff("gu_diff_embargoes_add", GU_FULL({ embargoes: GUA(2, ["e1"]) }), GU_FULL({ embargoes: GUA(102, ["e1", "e2"]) }, 100));
guDiff("gu_diff_troops_only", GU_FULL(), GU_FULL({ outgoingAttacks: GUA(105, [guAtk(11, "a1")]) }, 100));
guDiff("gu_diff_attack_id", GU_FULL(), GU_FULL({ outgoingAttacks: GUA(105, [guAtk(10, "a1x")]) }, 100));
guDiff("gu_diff_incoming_retreat", GU_FULL(), GU_FULL({ incomingAttacks: GUA(106, [guAtk(7, "a2", false, 3, 1)]) }, 100));
guDiff("gu_diff_alliance_expiry", GU_FULL(), GU_FULL({ alliances: GUA(108, [guAll(1, "p2", 10, 21)]) }, 100));
guDiff("gu_diff_emoji_recipient", GU_FULL(), GU_FULL({ outgoingEmojis: GUA(104, [guEmo("hi", 1, "AllPlayers")]) }, 100));
guDiff("gu_diff_emoji_nan", GU_FULL({ outgoingEmojis: GUA(4, [guEmo("hi", 1, NaN)]) }), GU_FULL({ outgoingEmojis: GUA(104, [guEmo("hi", 1, NaN)]) }, 100));
guDiff("gu_diff_smallid_nan", GU_FULL({ smallID: NaN }), GU_FULL({ smallID: NaN }));
guDiff("gu_diff_negzero", GU_FULL({ smallID: -0 }), GU_FULL({ smallID: 0 }));
guDiff("gu_diff_killed_null_undef", GU_FULL({ killedBy: null }), { ...GU_FULL({}, 100), killedBy: undefined });
for (const [k, v, v2] of [
  ["clientID", "c1", "c2"], ["name", "Alice", "Bob"], ["displayName", "ali", "bob"],
  ["clanTag", "CT", null], ["nationFlag", "en", "fr"], ["team", "Red", "Blue"],
  ["smallID", 7, 8], ["playerType", "HUMAN", "BOT"], ["isAlive", true, false],
  ["isDisconnected", false, true], ["killedBy", null, "c9"],
  ["deathPosition", null, 12.5], ["tradeGold", 3, 4], ["trainGold", 2, 3],
  ["piracyGold", 1, 2], ["isTraitor", false, true], ["traitorRemainingTicks", 0, 5],
  ["inDoomsdayClock", false, true], ["markedDoomsdayClockTick", 0, 77],
  ["isDecaying", false, true], ["hasSpawned", true, false], ["spawnTile", 42, 43],
  ["betrayals", 0, 1], ["lastDeleteUnitTick", 1, 2], ["isLobbyCreator", true, false],
]) guDiff(`gu_diff_one_${k}`, GU_FULL({ [k]: v }), GU_FULL({ [k]: v2 }));
guDiff("gu_diff_allies_undef", GU_FULL(), { ...GU_FULL({}, 100), allies: undefined });
guDiff("gu_diff_targets_len", GU_FULL({ targets: GUA(3, [9]) }), GU_FULL({ targets: GUA(103, [9, 10]) }, 100));
guDiff("gu_diff_oar", GU_FULL({ outgoingAllianceRequests: GUA(7, ["r1"]) }), GU_FULL({ outgoingAllianceRequests: GUA(107, ["r2"]) }, 100));
guDiff("gu_diff_empty", { id: "p" }, { id: "p" });

// --- apply (kind 1) ---
guApply("gu_apply_full", {}, GU_FULL());
guApply("gu_apply_partial", { isAlive: true, gold: 9 }, { id: "p", gold: NaN, traitorRemainingTicks: -3 });
guApply("gu_apply_nulls", {}, { id: "p", killedBy: null, deathPosition: null, gold: null, traitorRemainingTicks: null });
guApply("gu_apply_undef", { isAlive: true }, { id: "p", isAlive: undefined });
guApply("gu_apply_trt_nan", {}, { id: "p", traitorRemainingTicks: NaN });
guApply("gu_apply_trt_negzero", {}, { id: "p", traitorRemainingTicks: -0 });
guApply("gu_apply_trt_inf", {}, { id: "p", traitorRemainingTicks: Infinity });
guApply("gu_apply_arrays", {}, { id: "p", allies: GUA(1, [1, NaN, -0]), targets: GUA(2, []), outgoingAllianceRequests: GUA(3, ["x"]) });
guApply("gu_apply_arrays_over", { allies: [9], targets: [8], outgoingAllianceRequests: ["z"] }, { id: "p", allies: GUA(1, [1]), targets: GUA(2, []), outgoingAllianceRequests: GUA(3, []) });
guApply("gu_apply_attacks", {}, { id: "p", outgoingAttacks: GUA(1, [guAtk(10, "a1")]), incomingAttacks: GUA(2, [guAtk(7, "a2", true, 3, 1)]), alliances: GUA(3, [guAll()]), outgoingEmojis: GUA(4, [guEmo("hi", 1, "AllPlayers")]) });
guApply("gu_apply_noop", { isAlive: true, troops: 5 }, { id: "p" });
guApply("gu_apply_tiles", { tilesOwned: 1 }, { id: "p", tilesOwned: 2, troops: 3, goldEarned: 7 });
guApply("gu_apply_embargoes_absent", { allies: [1] }, { id: "p", embargoes: GUA(1, ["e1"]) });

// --- pack (kind 2) ---
guPack("gu_pack_same_ref", GUA(1, [guAtk(10)]), GUA(1, [guAtk(10)]), 5, 0);
guPack("gu_pack_undef_prev", undefined, GUA(1, [guAtk(10)]), 5, 0);
guPack("gu_pack_undef_next", GUA(1, [guAtk(10)]), undefined, 5, 0);
guPack("gu_pack_changes", GUA(1, [guAtk(10, "a"), guAtk(20, "b"), guAtk(30, "c")]), GUA(2, [guAtk(11, "a"), guAtk(20, "b"), guAtk(31, "c")]), 5, 0);
guPack("gu_pack_membership", GUA(1, [guAtk(10, "a")]), GUA(2, [guAtk(10, "b")]), 5, 0);
guPack("gu_pack_nan_troops", GUA(1, [guAtk(NaN, "a")]), GUA(2, [guAtk(NaN, "a")]), 5, 0);
guPack("gu_pack_negzero", GUA(1, [guAtk(-0, "a")]), GUA(2, [guAtk(0, "a")]), 5, 0);
guPack("gu_pack_incoming", GUA(1, [guAtk(1, "a")]), GUA(2, [guAtk(2, "a")]), 9, 1);
guPack("gu_pack_retreat", GUA(1, [guAtk(1, "a", false)]), GUA(2, [guAtk(1, "a", true)]), 5, 0);
guPack("gu_pack_len", GUA(1, [guAtk(1, "a")]), GUA(2, [guAtk(1, "a"), guAtk(2, "b")]), 5, 0);
guPack("gu_pack_empty", GUA(1, []), GUA(2, []), 5, 0);
guPack("gu_pack_owner_nan", GUA(1, [guAtk(1, "a")]), GUA(2, [guAtk(2, "a")]), NaN, 0);

// --- Railroad scenario runner --------------------------------------------------
// Exercises the real Railroad.ts through the shared run_op runner. Stations are
// duck-typed stubs carrying a numeric `__refid` (identity), rails are real
// Railroad instances carrying `__refid`. `game` is a width-parameterised duck
// (`x = t % w`, `y = (t / w) | 0`); `delete` records the emitted update and the
// two `removeRailroad` calls. kind 0 = getClosestTileIndex `[width, to, n,
// tiles…]` → `[index]`; kind 1 = getOrientedRailroad `[to, k, (nbr,rr)*k, m,
// (railroad)*m]` (railroad = `[refid, from, to, id, n, tiles…]`) → `[0]` or
// `[1, forward, n, tiles…, start, end]`; kind 2 = delete `[railroad]` →
// `[type, id, caller_from, rr, caller_to, rr]`.
const rrScenarios = [];
function captureRR(name, kind, args, res) {
  rrScenarios.push({ name, kind, args: args.flat().map(uenc), res: res.flat().map(uenc) });
}
let rrRefSeq = 1;
const rrStation = (refid) => ({
  __refid: refid,
  railroadByNeighbor: new Map(),
  removeRailroadCalls: [],
  getRailroadTo(o) {
    return this.railroadByNeighbor.get(o) ?? null;
  },
  removeRailroad(r) {
    this.removeRailroadCalls.push(r);
  },
});
const rrRail = (from, to, tiles, id) => {
  const r = new RR.Railroad(from, to, tiles, id);
  r.__refid = rrRefSeq++;
  return r;
};
const rrGame = (w) => ({ x: (t) => t % w, y: (t) => (t / w) | 0 });
const rrTokRail = (r) => [r.__refid, r.from.__refid, r.to.__refid, r.id, r.tiles.length, ...r.tiles];

// kind 0: closest-tile-index geometry.
function rrClosest(name, width, tiles, to) {
  const s = rrStation(1);
  const rail = rrRail(s, s, tiles, 0);
  const idx = rail.getClosestTileIndex(rrGame(width), to);
  captureRR(name, 0, [width, to, tiles.length, ...tiles], [idx]);
}
rrClosest("rr_close_empty", 10, [], 20);
rrClosest("rr_close_basic", 10, [23, 41, 2], 20);
rrClosest("rr_close_tie", 10, [10, 30], 20);
rrClosest("rr_close_single", 10, [7], 7);
rrClosest("rr_close_nan_to", 10, [23, 41], NaN);
rrClosest("rr_close_nan_tile", 10, [NaN, 41], 20);
rrClosest("rr_close_width1", 1, [0, 5, 9], 3);
rrClosest("rr_close_neg_to", 10, [23, 41], -1);
rrClosest("rr_close_big", 1000, [123456, 234567, 345678], 234000);
rrClosest("rr_close_inf_to", 10, [23, 41], Infinity);
rrClosest("rr_close_dup", 10, [50, 50, 50], 55);

// kind 1: oriented-railroad lookup + reversal.
function rrOriented(name, fromSt, toSt, rails) {
  const o = RR.getOrientedRailroad(fromSt, toSt);
  const by = [...fromSt.railroadByNeighbor].map(([s, r]) => [s.__refid, r.__refid]);
  const args = [toSt.__refid, by.length, ...by.flat(), rails.length, ...rails.flatMap(rrTokRail)];
  let res;
  if (o === null) {
    res = [0];
  } else {
    const rail = fromSt.railroadByNeighbor.get(toSt);
    const forward = rail.to === toSt ? 1 : 0;
    const tiles = o.getTiles();
    res = [1, forward, tiles.length, ...tiles, o.getStart().__refid, o.getEnd().__refid];
  }
  captureRR(name, 1, args, res);
}
{
  const A = rrStation(10), B = rrStation(20);
  const r = rrRail(A, B, [1, 2, 3], 7);
  A.railroadByNeighbor.set(B, r);
  B.railroadByNeighbor.set(A, r);
  rrOriented("rr_or_forward", A, B, [r]);
  rrOriented("rr_or_backward", B, A, [r]);
  const C = rrStation(30);
  rrOriented("rr_or_missing", A, C, [r]);
}
{
  const A = rrStation(10), B = rrStation(20);
  const r = rrRail(A, B, [], 8);
  A.railroadByNeighbor.set(B, r);
  rrOriented("rr_or_empty_tiles", A, B, [r]);
}
{
  const A = rrStation(10), B = rrStation(20), C = rrStation(30);
  const r1 = rrRail(A, B, [1, 2], 1);
  const r2 = rrRail(A, C, [3, 4, 5], 2);
  A.railroadByNeighbor.set(B, r1);
  A.railroadByNeighbor.set(C, r2);
  rrOriented("rr_or_parallel", A, C, [r1, r2]);
  rrOriented("rr_or_parallel2", A, B, [r1, r2]);
}

// kind 2: delete's observable update + removeRailroad call sequence.
function rrDelete(name, rail) {
  const game = { updates: [], addUpdate(u) { this.updates.push(u); } };
  rail.from.removeRailroadCalls = [];
  rail.to.removeRailroadCalls = [];
  rail.delete(game);
  const u = game.updates[0];
  const res = [u.type, u.id, rail.from.__refid, rail.__refid, rail.to.__refid, rail.__refid];
  captureRR(name, 2, rrTokRail(rail), res);
}
{
  const A = rrStation(10), B = rrStation(20);
  rrDelete("rr_del_basic", rrRail(A, B, [1, 2, 3], 7));
}
{
  const A = rrStation(10);
  rrDelete("rr_del_selfloop", rrRail(A, A, [], 9));
}
{
  const A = rrStation(10), B = rrStation(20);
  rrDelete("rr_del_negid", rrRail(A, B, [5], -3));
}

// --- RailSpatialGrid scenario runner -----------------------------------------
// Exercises the real RailroadSpatialGrid.ts through a stateful op stream. Rails
// are duck-typed `{ tiles, __refid }` (the grid keys them by object identity;
// refids cross in the token stream). `game` is a real GameMapImpl (all-land).
// kind 0 = construct `[cellSize]` -> `[0]` ok / `[1]` throw; 1 = register
// `[refid, n, tiles…]` -> `[]`; 2 = unregister `[refid]` -> `[]`; 3 = query
// `[tile, radius]` -> `[len, refids…]`; 4 = dump cells -> `[ncells, (klen,
// bytes…, m, refids…)*]`; 5 = dump railToCells -> `[nrails, (refid, m, (klen,
// bytes…)*m)*]`.
const rsgScenarios = [];
let rsgIdx = 0;
function runRSG(name, width, height, ops) {
  const game = new GameMapImpl(width, height, new Uint8Array(width * height), width * height);
  const played = [];
  let grid = null;
  // JS Map/Set key on object identity: one persistent rail object per refid
  // (register with the same refid reuses the object, exactly like re-registering
  // the same Railroad instance; unregister must pass the same object).
  const rails = new Map();
  const railOf = (refid, tiles) => {
    let r = rails.get(refid);
    if (!r) {
      r = { tiles, __refid: refid };
      rails.set(refid, r);
    } else {
      r.tiles = tiles;
    }
    return r;
  };
  for (const [k, ...a] of ops) {
    let args, res;
    if (k === 0) {
      args = [width, height, a[0]];
      try {
        grid = new RSG.RailSpatialGrid(game, a[0]);
        res = [0];
      } catch {
        grid = null;
        res = [1];
      }
    } else if (k === 1) {
      const [refid, tiles] = a;
      args = [refid, tiles.length, ...tiles];
      grid.register(railOf(refid, tiles));
      res = [];
    } else if (k === 2) {
      args = [a[0]];
      grid.unregister(railOf(a[0], []));
      res = [];
    } else if (k === 3) {
      args = [a[0], a[1]];
      const r = [...grid.query(a[0], a[1])].map((x) => x.__refid);
      res = [r.length, ...r];
    } else if (k === 4) {
      args = [];
      res = [grid.cells.size];
      for (const [key, set] of grid.cells) {
        res.push(key.length, ...[...key].map((c) => c.charCodeAt(0)));
        res.push(set.size, ...[...set].map((x) => x.__refid));
      }
    } else {
      args = [];
      res = [grid.railToCells.size];
      for (const [rail, keys] of grid.railToCells) {
        res.push(rail.__refid, keys.size);
        for (const key of keys) res.push(key.length, ...[...key].map((c) => c.charCodeAt(0)));
      }
    }
    played.push({ kind: k, args: args.flat().map(uenc), res: res.flat().map(uenc) });
  }
  rsgScenarios.push({ name: `${name}_${rsgIdx++}`, width, height, ops: played });
}

// 1. Basic register + query + dumps (cellSize 5 over a 10x10 map).
runRSG("rsg_basic", 10, 10, [
  [0, 5],
  [1, 1, [22, 77]], // rail 1: (2,2) -> "0:0", (7,7) -> "1:1"
  [1, 2, [16]],     // rail 2: (6,1) -> "1:0"
  [4],
  [5],
  [3, 22, 0],       // exact cell "0:0" -> [1]
  [3, 16, 0],       // "1:0" -> [2]
  [3, 77, 0],       // "1:1" -> [1]
  [3, 5, 6],        // wide box -> [1, 2] in scan order
  [2, 1],           // unregister rail 1 -> cells "1:1" pruned
  [4],
  [3, 77, 0],       // now empty
]);

// 2. Double-register replaces (defensive unregister first).
runRSG("rsg_re_register", 10, 10, [
  [0, 5],
  [1, 1, [22]],
  [1, 1, [77]],
  [4],
  [5],
]);

// 3. Empty-tiles rail is not tracked in railToCells.
runRSG("rsg_empty_tiles", 10, 10, [
  [0, 5],
  [1, 1, []],
  [4],
  [5],
]);

// 4. Shared cell: two rails in one cell keep insertion order.
runRSG("rsg_shared_cell", 10, 10, [
  [0, 5],
  [1, 1, [0]],
  [1, 2, [1]],
  [1, 3, [2]],
  [4],
  [3, 0, 0],
  [2, 2],
  [4],
  [3, 0, 0],
]);

// 5. cellSize 0 / negative throw at construct; NaN collapses to "NaN:NaN".
runRSG("rsg_ctor_throw0", 10, 10, [[0, 0]]);
runRSG("rsg_ctor_throwneg", 10, 10, [[0, -5]]);
runRSG("rsg_ctor_nan", 10, 10, [
  [0, NaN],
  [1, 1, [22]],
  [4],
  [5],
]);

// 6. cellSize 1: every tile is its own cell.
runRSG("rsg_cell1", 10, 10, [
  [0, 1],
  [1, 1, [23, 41]],
  [4],
  [3, 23, 0],
  [3, 23, 1],
]);

// 7. cellSize larger than the map: single cell.
runRSG("rsg_cellbig", 10, 10, [
  [0, 100],
  [1, 1, [0, 99]],
  [4],
  [3, 50, 0],
]);

// 8. Negative-radius query: box inverts -> empty.
runRSG("rsg_neg_radius", 10, 10, [
  [0, 5],
  [1, 1, [22]],
  [3, 22, -1],
]);

// 9. Fractional cellSize: floor division.
runRSG("rsg_frac_cell", 10, 10, [
  [0, 2.5],
  [1, 1, [22, 77]], // (2,2)/2.5=0.8->0, (7,7)/2.5=2.8->2
  [4],
  [3, 22, 0],
]);

// --- TileTraversalScratch scenario runner -------------------------------------
// Exercises the real TileTraversalScratch.ts through a stateful op stream. The
// module caches a per-game scratch in a WeakMap keyed by the Game object, so
// each scenario uses a fresh game stub `{ width, height }` (the Rust harness
// reset() clears its cache between scenarios, matching a fresh WeakMap). A
// persistent game object per refid keeps identity stable across ops (the
// WeakMap keys on object identity, exactly like the railOf table in RSG).
// kind 0 = tileTraversalScratch(refid, width, height) -> `[1]` (ToIndex throw)
// or `[0, visited_len, stack_len, cluster_len, gen]`; 1 = bump(refid) -> `[gen]`;
// 2 = set_gen(refid, g) -> `[gen]`; 3 = write_visited(refid, i, v) -> `[val]`;
// 4 = push_stack(refid, tile) -> `[stack_len]`; 5 = read_visited(refid, i) ->
// `[val]` (NaN for OOB); 6 = write_cluster(refid, i, v) -> `[val]`.
const ttsScenarios = [];
let ttsIdx = 0;
function runTTS(name, games, ops) {
  // games: { refid: [width, height] }. A persistent stub object per refid
  // keeps WeakMap identity stable; kind 0 mutates its accessors to the recorded
  // dims before calling tileTraversalScratch. The scratch returned by kind 0 is
  // cached per refid and reused by kinds 1-6 (matching the Rust harness, whose
  // non-alloc ops touch the existing scratch directly — they never re-run the
  // grow-realloc check).
  const gameObjs = new Map();
  for (const [refid, [w, h]] of Object.entries(games)) {
    gameObjs.set(Number(refid), { width: () => w, height: () => h });
  }
  const scratchByRefid = new Map();
  const played = [];
  for (const [k, ...a] of ops) {
    let args, res;
    const refid = a[0];
    if (k === 0) {
      // a = [refid, w, h]; set the game's dims before the call.
      const [, w, h] = a;
      const g = gameObjs.get(refid);
      g.width = () => w;
      g.height = () => h;
      args = [refid, w, h];
      try {
        const s = TTS.tileTraversalScratch(g);
        scratchByRefid.set(refid, s);
        res = [0, s.visited.length, s.stack.length, s.clusterIndexMap.length, s.gen];
      } catch {
        res = [1];
      }
    } else {
      const s = scratchByRefid.get(refid);
      if (k === 1) {
        args = [refid];
        res = [TTS.bumpTraversalGeneration(s)];
      } else if (k === 2) {
        // set_gen: pre-arm the wrap by writing scratch.gen directly.
        args = [refid, a[1]];
        s.gen = a[1];
        res = [s.gen];
      } else if (k === 3) {
        args = [refid, a[1], a[2]];
        s.visited[a[1]] = a[2];
        const v = s.visited[a[1]];
        res = [v === undefined ? NaN : v];
      } else if (k === 4) {
        args = [refid, a[1]];
        s.stack.push(a[1]);
        res = [s.stack.length];
      } else if (k === 5) {
        args = [refid, a[1]];
        const v = s.visited[a[1]];
        res = [v === undefined ? NaN : v];
      } else {
        // kind 6: write_cluster
        args = [refid, a[1], a[2]];
        s.clusterIndexMap[a[1]] = a[2];
        const v = s.clusterIndexMap[a[1]];
        res = [v === undefined ? NaN : v];
      }
    }
    played.push({ kind: k, args: args.flat().map(uenc), res: res.flat().map(uenc) });
  }
  ttsScenarios.push({ name: `${name}_${ttsIdx++}`, ops: played });
}

// 1. Allocate, reuse (same size), shrink keeps buffers, grow reallocates.
runTTS(
  "tts_reuse",
  { 1: [2, 2] },
  [
    [0, 1, 2, 2], // -> [0,4,0,4,0]
    [0, 1, 2, 2], // same size reuses
    [0, 1, 1, 2], // shrink: keeps 4-slot buffers
    [0, 1, 3, 3], // grow: reallocates, gen resets to 0
  ],
);

// 2. Bump + wrap: arm gen just below the wrap, write visited, bump fills.
runTTS(
  "tts_bump_wrap",
  { 1: [4, 4] },
  [
    [0, 1, 4, 4], // [0,16,0,16,0]
    [1, 1],       // bump -> 1
    [1, 1],       // bump -> 2
    [3, 1, 0, 4_294_967_295], // visited[0] = 0xffffffff (ToUint32 -> 4294967295)
    [5, 1, 0],    // read back -> 4294967295
    [2, 1, 4_294_967_294], // arm wrap: gen = 0xfffffffe
    [1, 1],       // bump -> 0xffffffff -> fill(0), gen=1
    [5, 1, 0],    // visited[0] now 0
  ],
);

// 3. Distinct games get distinct scratches.
runTTS(
  "tts_distinct_games",
  { 1: [2, 2], 2: [4, 4] },
  [
    [0, 1, 2, 2], // game1 -> len 4
    [0, 2, 4, 4], // game2 -> len 16
    [1, 1],       // bump game1 -> 1
    [1, 2],       // bump game2 -> 1 (independent gen)
    [0, 1, 2, 2], // game1 still len 4, gen 1
    [0, 2, 4, 4], // game2 still len 16, gen 1
  ],
);

// 4. ToIndex throw on negative total.
runTTS("tts_throw_neg", { 1: [-1, 5] }, [[0, 1, -1, 5]]);

// 5. Fractional total truncates (no throw): 1.5*1 -> length 1.
runTTS("tts_frac_total", { 1: [1.5, 1] }, [[0, 1, 1.5, 1]]);

// 6. NaN total allocates length 0.
runTTS("tts_nan_total", { 1: [NaN, 5] }, [[0, 1, NaN, 5]]);

// 7. Typed-array OOB: in-range write/read, OOB write dropped, OOB read NaN.
runTTS(
  "tts_typed_oob",
  { 1: [2, 2] },
  [
    [0, 1, 2, 2], // len 4
    [3, 1, 1, 4_294_967_301], // ToUint32(4294967301) = 5
    [5, 1, 1],    // read -> 5
    [3, 1, 99, 7], // OOB write dropped
    [5, 1, 99],   // OOB read -> NaN
    [6, 1, 0, -1], // cluster[0] = -1 (ToInt32)
    [6, 1, 1, 4_294_967_295], // ToInt32(0xffffffff) = -1
  ],
);

// 8. Stack push observable (length only).
runTTS(
  "tts_stack",
  { 1: [2, 2] },
  [
    [0, 1, 2, 2],
    [4, 1, 10], // push -> len 1
    [4, 1, 20], // push -> len 2
  ],
);

// 9. Infinity total throws.
runTTS("tts_inf_total", { 1: [Infinity, 1] }, [[0, 1, Infinity, 1]]);

// --- EventBus scenario runner -------------------------------------------------
// Exercises the real EventBus.ts through a stateful op stream. The Map keys on
// the event *constructor* object and the array holds *callback* function
// objects, so each rides in as a persistent capture refid (one object per
// refid, exactly like railOf in RSG): `off`'s `indexOf` and `emit`'s `get` see
// the same identities the Rust harness keys by. `emit` takes an event
// instance; its constructor is resolved from `event.constructor.__refid` so the
// recorded ctor always matches the object JS looks up. kind 0 = on(ctor, cb)
// -> []; 1 = off(ctor, cb) -> []; 2 = emit(ctor, event) -> `[n, (cb, event)*n]`
// call trace; 3 = dump -> `[nentries, (ctor, m, cbs…)*]` in Map order.
const ebScenarios = [];
let ebIdx = 0;
function runEB(name, ops) {
  const ctors = new Map();
  const ctorOf = (refid) => {
    let c = ctors.get(refid);
    if (!c) {
      c = class {};
      c.__refid = refid;
      ctors.set(refid, c);
    }
    return c;
  };
  // The per-emit call trace. Each callback pushes `(own refid, event refid)`
  // when the *real* `bus.emit` invokes it, so the recorded trace is the actual
  // invocation sequence (registration order, first-match `off`, truthy-empty
  // array, etc.) rather than a re-implementation.
  let trace = [];
  const cbs = new Map();
  const cbOf = (refid) => {
    let f = cbs.get(refid);
    if (!f) {
      f = (e) => trace.push(f.__refid, e.__refid);
      f.__refid = refid;
      cbs.set(refid, f);
    }
    return f;
  };
  // Each event refid is a fresh instance of its ctor; the ctor refid is given
  // in the op so the instance's `constructor` identity matches the Rust key.
  const events = new Map();
  const eventOf = (refid, ctorRefid) => {
    let e = events.get(refid);
    if (!e) {
      e = new (ctorOf(ctorRefid))();
      e.__refid = refid;
      events.set(refid, e);
    }
    return e;
  };
  const bus = new EB.EventBus();
  const played = [];
  for (const [k, ...a] of ops) {
    let args, res;
    if (k === 0) {
      args = [a[0], a[1]];
      bus.on(ctorOf(a[0]), cbOf(a[1]));
      res = [];
    } else if (k === 1) {
      args = [a[0], a[1]];
      bus.off(ctorOf(a[0]), cbOf(a[1]));
      res = [];
    } else if (k === 2) {
      // a = [ctorRefid, eventRefid]; run the real emit.
      const evt = eventOf(a[1], a[0]);
      args = [a[0], a[1]];
      trace = [];
      bus.emit(evt);
      res = [trace.length / 2, ...trace];
    } else {
      args = [];
      res = [bus.listeners.size];
      for (const [ctor, list] of bus.listeners) {
        res.push(ctor.__refid, list.length, ...list.map((cb) => cb.__refid));
      }
    }
    played.push({ kind: k, args: args.flat().map(uenc), res: res.flat().map(uenc) });
  }
  ebScenarios.push({ name: `${name}_${ebIdx++}`, ops: played });
}

// 1. Registration order, emit fan-out, off removes first duplicate.
runEB("eb_basic", [
  [0, 1, 10], // on(EvA, c1)
  [0, 1, 20], // on(EvA, c2)
  [2, 1, 100], // emit EvA -> [2, (10,100),(20,100)]
  [0, 1, 30], // on(EvA, c3)
  [2, 1, 101], // emit -> [3, ...]
  [1, 1, 20], // off c2
  [2, 1, 102], // emit -> [2, (10,102),(30,102)]
]);

// 2. Duplicate callback: one off removes only the first.
runEB("eb_dup_cb", [
  [0, 1, 10],
  [0, 1, 10], // same cb twice
  [1, 1, 10], // off removes first
  [2, 1, 55], // emit -> [1, (10,55)]
]);

// 3. Unknown ctor emit/off are no-ops; empty array is truthy (0 calls).
runEB("eb_unknown", [
  [2, 2, 200], // emit unknown ctor -> [0]
  [1, 2, 10], // off unknown -> no-op
  [0, 2, 10],
  [1, 2, 10], // entry exists but empty
  [2, 2, 201], // emit -> [0]
]);

// 4. Map insertion order across ctors; re-on does not reorder.
runEB("eb_order", [
  [0, 2, 20], // ctor2 first
  [0, 1, 10], // ctor1 second
  [0, 2, 21], // append to ctor2
  [3], // dump -> [2, (2,[20,21]), (1,[10])]
]);

// 5. Two ctors, interleaved emits.
runEB("eb_multi_ctor", [
  [0, 1, 10],
  [0, 2, 20],
  [2, 1, 100], // emit ctor1 -> [1,(10,100)]
  [2, 2, 200], // emit ctor2 -> [1,(20,200)]
  [3], // dump -> [2, (1,[10]), (2,[20])]
]);

// --- AssetUrls scenario runner ------------------------------------------------
// Exercises the real AssetUrls.ts pure path helpers through the shared
// run_op runner. Strings cross as `[len, u0, ..]` (UTF-16 code units); the
// manifest is `[n, (key, value) * n]`; a result is `[0, len, u0, ..]` or
// `[1]` when the TS function throws. kind 0 = normalizeAssetPath,
// 1 = encodeAssetPath, 2 = buildAssetUrl(path, manifest, baseUrl).
// The Rust twin is `asset_urls::*`.
const auScenarios = [];
function captureAU(name, kind, args, res) {
  auScenarios.push({ name, kind, args: args.flat().map(uenc), res: res.flat().map(uenc) });
}
const auCall = (fn, ...a) => {
  try {
    return [0, ...encS(fn(...a))];
  } catch {
    return [1];
  }
};

// kind 0: normalizeAssetPath — leading-slash strip, empty-segment drop,
// percent-decode fallbacks, and the . / .. guards (raw and encoded forms).
{
  const cases = [
    "/flags/US.svg",
    "///a//b/",
    "",
    "/",
    "a",
    "a/../b",
    ".",
    "..",
    "%2e%2e/x",
    "%2E/x",
    "%2e/x",
    "a/%2Fb",
    "a/%2e%2fb",
    "a/%2E%2Fb",
    "caf%C3%A9",
    "a%20b",
    "%zz",
    "%",
    "%c",
    "%c3%28",
    "%ed%a0%80",
    "%f0%9f%98%80",
    "app://openfront/x",
    "a%26b",
    "a+b",
    "a~b!c(d)e*f'g",
    "/%2d",
    "%E2%84%AA",
    "a/%2E%2E/b",
    "a/%2f%2e/b",
  ];
  for (const [i, p] of cases.entries()) {
    captureAU(`au_norm_${i}`, 0, [encS(p)], auCall(AU.normalizeAssetPath, p));
  }
}

// kind 1: encodeAssetPath — re-split of the normalised path (decoded slashes
// become empty segments and vanish), unreserved passthrough, UTF-8 escapes.
{
  const cases = [
    "/a b/c",
    "a/%2Fb",
    "café",
    "a%20b",
    "😀x",
    "a/..",
    "a/%2E%2Fb",
    "a!'()*~-_.b ",
    "a%26b",
    "",
    "app://openfront/x",
    "a/%2Fb/%2Fc",
    "%",
    "%zz",
    "a\u00e9\u4e2d\ud83d\ude00",
  ];
  for (const [i, p] of cases.entries()) {
    captureAU(`au_enc_${i}`, 1, [encS(p)], auCall(AU.encodeAssetPath, p));
  }
}

// kind 2: buildAssetUrl — the absolute-URL fast path (any scheme, the i-flag
// edges), the manifest hit / miss / empty-value branches, the baseUrl
// trailing-slash strip, and the encode fallback (which can throw where
// normalizeAssetPath alone did not).
{
  const M1 = [["a/b", "https://cdn/z.svg"]];
  const M2 = [["a/b", "z.svg"]];
  const M3 = [["a/b", ""]];
  const M4 = [["a/./b", "hit"]];
  const M5 = [["a//b", "dbl"]];
  const tok = (m) => [
    m.length,
    ...m.flatMap(([k, v]) => [...encS(k), ...encS(v)]),
  ];
  const cases = [
    ["app://openfront/_assets/flags/US.svg", [], "https://cdn/"],
    ["HTTP://x/y", [], ""],
    ["a+b://x", [], ""],
    ["a.-+://x", [], ""],
    ["1a://x", [], ""],
    ["a:x//y", [], ""],
    ["://x", [], ""],
    ["", [], ""],
    ["a/b", M1, "https://cdn/"],
    ["a/b", M1, "https://cdn///"],
    ["a/b", M1, ""],
    ["a/b", M2, ""],
    ["a/b", M2, "x"],
    ["a/b", M3, ""],
    ["a/b", [], ""],
    ["a/b", [], "https://cdn/"],
    ["/a/b", M2, ""],
    ["a/%2E%2Fb", [], ""],
    ["a/%2E%2Fb", M4, ""],
    ["a/%2Fb", [], ""],
    ["a/%2Fb", M5, ""],
    ["a b/c", [], ""],
    ["café", [], ""],
    ["a\u00e9\u4e2d\ud83d\ude00", [], ""],
    ["a%20b", [], ""],
    ["%2e/x", [], ""],
    ["a/./b", M4, "base/"],
  ];
  for (const [i, [p, m, base]] of cases.entries()) {
    captureAU(
      `au_build_${i}`,
      2,
      [encS(p), tok(m), encS(base)],
      auCall(AU.buildAssetUrl, p, Object.fromEntries(m), base),
    );
  }
}

// --- Maps.gen scenario runner --------------------------------------------------
// Exercises the real Maps.gen.ts data tables through the shared run_op
// runner. kind 0 = dump maps, 1 = dump GameMapType, 2 = dump
// mapCategoryOrder, 3 = find a map by id. The record serialisation (strings
// as [len, u0, ..], optional fields behind an explicit presence flag) is
// re-implemented here in JS exactly as maps_gen.rs does it in Rust, so the
// golden stream is the TS data and the replay proves the Rust table matches.
// The Rust twin is `maps_gen::*`.
const mgScenarios = [];
function captureMG(name, kind, args, res) {
  mgScenarios.push({ name, kind, args: args.flat().map(uenc), res: res.flat().map(uenc) });
}

const mgStr = (s) => encS(s);
const mgOptNum = (v) => (v === undefined ? [0] : [1, v]);
const mgOptStrs = (v) =>
  v === undefined ? [0] : [1, v.length, ...v.flatMap(mgStr)];
const mgRecord = (m) => [
  ...mgStr(m.id),
  ...mgStr(m.type),
  ...mgStr(m.translationKey),
  m.categories.length,
  ...m.categories.flatMap(mgStr),
  m.multiplayerFrequency,
  m.ffaFrequency,
  m.teamFrequency,
  m.specialFrequency,
  m.defaultNationCount,
  ...mgOptNum(m.featuredRank),
  ...mgOptNum(m.specialTeamCount),
  ...mgOptStrs(m.disabledModifiers),
  ...mgOptStrs(m.forcedModifiers),
  ...mgOptStrs(m.themes),
  ...(m.customTribes === undefined
    ? [0]
    : [
        1,
        m.customTribes.length,
        ...m.customTribes.flatMap((t) => [
          ...mgStr(t.name),
          ...(t.coordinates === undefined
            ? [0]
            : [1, t.coordinates[0], t.coordinates[1]]),
        ]),
      ]),
  ...(m.layers === undefined
    ? [0]
    : [
        1,
        m.layers.length,
        ...m.layers.flatMap((l) => [
          ...mgStr(l.id),
          ...mgStr(l.placement),
          ...(l.nukeable === undefined ? [0] : [1, l.nukeable ? 1 : 0]),
        ]),
      ]),
];

captureMG("mg_types_dump", 1, [], [
  Object.keys(MG.GameMapType).length,
  ...Object.entries(MG.GameMapType).flatMap(([k, v]) => [...mgStr(k), ...mgStr(v)]),
]);
captureMG("mg_categories_dump", 2, [], [
  MG.mapCategoryOrder.length,
  ...MG.mapCategoryOrder.flatMap(mgStr),
]);
captureMG("mg_maps_dump", 0, [], [
  MG.maps.length,
  ...MG.maps.flatMap(mgRecord),
]);
// kind 3: a hit for the first / last map and a miss for an unknown id.
captureMG("mg_find_achiran", 3, [mgStr("Achiran")], [0, ...mgRecord(MG.maps[0])]);
captureMG("mg_find_milkyway", 3, [mgStr("MilkyWay")], [
  0,
  ...mgRecord(MG.maps.find((m) => m.id === "MilkyWay")),
]);
captureMG("mg_find_miss", 3, [mgStr("NoSuchMap")], [1]);

// --- TribeNames scenario runner ------------------------------------------------
// Exercises the real TribeNames.ts resolveTribeNameData through the shared
// run_op runner. The theme record is the JSON module object the prepared
// module imported (same ESM instance, mutable), so the capture can delete /
// blank entries to reach the unknown-theme and fallback edges, restoring them
// after each call. The console.warn trace is captured as the warn list.
// args = [mapPresent, mapType?, removedN, (removed)*, blankedN, (blanked)*];
// res = [0, warnN, (warn)*, prefixN, (prefix)*, suffixN, (suffix)*,
//        tribesPresent, tribeN, (name, coordPresent, x, y)*] or [1] (throw).
// The Rust twin is `tribe_names::*`.
const tnScenarios = [];
function captureTN(name, args, res) {
  tnScenarios.push({ name, kind: 0, args: args.flat(2).map(uenc), res: res.flat().map(uenc) });
}

const TN_THEMES = (
  await import(
    new URL(
      "resources/tribeNameThemes.json",
      pathToFileURL(TS_ROOT).href.replace(/\/?$/, "/"),
    ).href,
    { with: { type: "json" } }
  )
).default;

const tnOk = (r, warns) => [
  0,
  warns.length,
  ...warns.flatMap(encS),
  r.prefixes.length,
  ...r.prefixes.flatMap(encS),
  r.suffixes.length,
  ...r.suffixes.flatMap(encS),
  ...(r.customTribes === undefined
    ? [0]
    : [
        1,
        r.customTribes.length,
        ...r.customTribes.flatMap((t) => [
          ...encS(t.name),
          ...(t.coordinates === undefined
            ? [0]
            : [1, t.coordinates[0], t.coordinates[1]]),
        ]),
      ]),
];

function tnRun(mapType, removed = [], blanked = []) {
  const saved = new Map();
  for (const name of removed) {
    saved.set(name, TN_THEMES[name]);
    delete TN_THEMES[name];
  }
  for (const name of blanked) {
    saved.set(name, TN_THEMES[name]);
    TN_THEMES[name] = { prefixes: [], suffixes: [] };
  }
  const warns = [];
  const origWarn = console.warn;
  console.warn = (m) => warns.push(m);
  let res;
  try {
    res = tnOk(TN.resolveTribeNameData(mapType), warns);
  } catch {
    res = [1];
  } finally {
    console.warn = origWarn;
    for (const [k, v] of saved) TN_THEMES[k] = v;
  }
  const args = [
    mapType === undefined ? [0] : [1, encS(mapType)],
    [removed.length, ...removed.flatMap(encS)],
    [blanked.length, ...blanked.flatMap(encS)],
  ];
  return { args, res };
}

const tnCases = [
  ["tn_none", undefined, [], []],
  ["tn_unknown_map", "NoSuchMap", [], []],
  ["tn_achiran", "Achiran", [], []],
  ["tn_antarctica", "Antarctica", [], []],
  ["tn_germany", "Germany", [], []],
  ["tn_sol", "Sol", [], []],
  ["tn_aegean_rm_europe", "Aegean", ["europe"], []],
  ["tn_aegean_rm_both", "Aegean", ["europe", "asia"], []],
  ["tn_alps_blank_europe", "Alps", [], ["europe"]],
  ["tn_throw_none", undefined, ["default"], []],
  ["tn_throw_aegean", "Aegean", ["europe", "asia", "default"], []],
  ["tn_alps_rm_europe_blank_default", "Alps", ["europe"], ["default"]],
  ["tn_milkyway", "MilkyWay", [], []],
  // getMapInfo matches the enum *value*, not the id: the wire name finds the
  // map, the folder id does not.
  ["tn_guanabara_value", "Rio de Janeiro", [], []],
  ["tn_guanabara_id", "Guanabara", [], []],
];
for (const [name, mt, removed, blanked] of tnCases) {
  const { args, res } = tnRun(mt, removed, blanked);
  captureTN(name, args, res);
}

// --- game/Game.ts scenario runner ---------------------------------------------
// Exercises the runtime-value subset of Game.ts (the interface part has no
// runtime) through the shared run_op runner. Strings cross as `[len, u0, ..]`
// (UTF-16 code units); numbers through uenc. The Rust twin is `game_ts::*`.
//
// kind table (matches the Rust dispatch):
//   0 string-enum dump  args [enumIdx]                res [n, (name,val)*]
//   1 numeric-enum dump args [enumIdx]                res [n, (name,val)*]
//   2 group has()       args [groupIdx]               res [tLen,(typeVal)*,16,(has)*]
//   3 isEnumValue guard args [guardIdx, valueStr]     res [0/1]
//   4 msg categories    args [0] dump / [1, mt] lookup res dump [22,(cat)*] / [p,(cat)?]
//   5 ColoredTeams dump args [0]                       res [n, (name,val)*]
//   6 Cell              args [x, y]                    res [strRepr, x, y]
//   7 PlayerInfo        args [name,pt,id,clanP,(clan)?] res [disp,pt,id,lobby,fLen,tiP,nfP,ciP]
//   8 bulk math         args [cost,ucP,(n,(uc)*)?,amount,gold] res [bulkCost,maxAmount]
//   9 const dump        args [0]                       res [AP,D,T,Q,HVN,MAXU,(2,5),(5,10)]
const gameScenarios = [];
// Scenario names become Rust const identifiers (uppercased), so non-alphanumerics are folded to `_`.
const gname = (s) => s.replace(/[^A-Za-z0-9]+/g, "_");
function captureGAME(name, kind, args, res) {
  gameScenarios.push({
    name: gname(name),
    kind,
    args: args.flat(Infinity).map(uenc),
    res: res.flat(Infinity).map(uenc),
  });
}

// kind 0: the nine string enums, in the order the Rust table indexes them.
const GAME_STR_ENUMS = [
  "Difficulty",
  "GameType",
  "GameMode",
  "RankedType",
  "GameMapSize",
  "UnitType",
  "TrainType",
  "PlayerType",
  "MessageCategory",
];
for (let i = 0; i < GAME_STR_ENUMS.length; i++) {
  const obj = GAME[GAME_STR_ENUMS[i]];
  const entries = Object.entries(obj);
  captureGAME(
    `g_str_${i}_${GAME_STR_ENUMS[i]}`,
    0,
    [i],
    [entries.length, ...entries.flatMap(([k, v]) => [encS(k), encS(v)])],
  );
}

// kind 1: the three numeric enums.
const GAME_NUM_ENUMS = ["Relation", "TerrainType", "MessageType"];
for (let i = 0; i < GAME_NUM_ENUMS.length; i++) {
  const obj = GAME[GAME_NUM_ENUMS[i]];
  const entries = Object.entries(obj);
  captureGAME(
    `g_num_${i}_${GAME_NUM_ENUMS[i]}`,
    1,
    [i],
    [entries.length, ...entries.flatMap(([k, v]) => [encS(k), v])],
  );
}

// kind 2: the five unitTypeGroups — dump the `types` array (in order) then the
// has() matrix over all 16 UnitType values.
const GAME_GROUPS = ["Nukes", "BuildableAttacks", "Structures", "BuildMenus", "PlayerBuildable"];
const GAME_UNIT_VALUES = Object.values(GAME.UnitType);
for (let g = 0; g < GAME_GROUPS.length; g++) {
  const grp = GAME[GAME_GROUPS[g]];
  const types = grp.types;
  const has = GAME_UNIT_VALUES.map((u) => (grp.has(u) ? 1 : 0));
  captureGAME(`g_group_${g}_${GAME_GROUPS[g]}`, 2, [g], [
    types.length,
    ...types.flatMap((t) => encS(t)),
    GAME_UNIT_VALUES.length,
    ...has,
  ]);
}

// kind 3: the three isEnumValue guards (0 Difficulty, 1 GameType, 2 GameMode).
const GAME_GUARDS = [
  ["isDifficulty", GAME.Difficulty],
  ["isGameType", GAME.GameType],
  ["isGameMode", GAME.GameMode],
];
const guardCases = [
  [0, "Easy", 1],
  [0, "easy", 0],
  [0, "Impossible", 1],
  [0, "", 0],
  [1, "Public", 1],
  [1, "Singleplayer", 1],
  [1, "public", 0],
  [2, "Free For All", 1],
  [2, "Team", 1],
  [2, "FFA", 0],
  [2, "Nope", 0],
];
for (const [ci, [gi, val, want]] of guardCases.entries()) {
  const actual = GAME[GAME_GUARDS[gi][0]](val) ? 1 : 0;
  captureGAME(`g_guard_${ci}`, 3, [gi, encS(val)], [actual]);
  if (actual !== want) {
    throw new Error(`guard ${gi} ${val}: got ${actual} want ${want}`);
  }
}

// kind 4: MESSAGE_TYPE_CATEGORIES dump + getMessageCategory lookups.
{
  const cat = GAME.MESSAGE_TYPE_CATEGORIES;
  const keys = Object.keys(cat).map(Number).sort((a, b) => a - b);
  captureGAME("g_msgcat_dump", 4, [0], [
    keys.length,
    ...keys.flatMap((k) => encS(cat[k])),
  ]);
  const lookups = [0, 4, 8, 13, 18, 20, 21, 22, -1, 999];
  for (const mt of lookups) {
    const got = GAME.getMessageCategory(mt);
    captureGAME(`g_msgcat_get_${mt}`, 4, [1, mt], got === undefined ? [0] : [1, encS(got)]);
  }
}

// kind 5: ColoredTeams dump.
{
  const entries = Object.entries(GAME.ColoredTeams);
  captureGAME("g_colored_teams", 5, [0], [
    entries.length,
    ...entries.flatMap(([k, v]) => [encS(k), encS(v)]),
  ]);
}

// kind 6: Cell — strRepr, pos().x, pos().y.
const cellCases = [
  [3, -4],
  [0, 0],
  [-0, 1.5],
  [100, 250],
  [7, -13],
  [1.5, 2.25],
];
for (const [x, y] of cellCases) {
  const c = new GAME.Cell(x, y);
  captureGAME(`g_cell_${x}_${y}`, 6, [x, y], [encS(c.toString()), c.x, c.y]);
}

// kind 7: PlayerInfo — displayName + the ctor defaults.
// args [nameStr, ptStr, idStr, clanPresent, (clanStr)?]; clientID always null.
const piCases = [
  { name: "Bob", pt: "HUMAN", id: "p1", clan: null },
  { name: "Bob", pt: "BOT", id: "p2", clan: "xyz" },
  { name: "Alice", pt: "NATION", id: "p3", clan: "" },
  { name: "", pt: "HUMAN", id: "", clan: "tag" },
  { name: "Ünïcode", pt: "HUMAN", id: "ü", clan: "ключ" },
];
for (const [i, c] of piCases.entries()) {
  const clanPresent = c.clan === null ? 0 : 1;
  const pi = new GAME.PlayerInfo(c.name, c.pt, null, c.id, false, c.clan);
  const res = [
    encS(pi.displayName),
    encS(pi.playerType),
    encS(pi.id),
    pi.isLobbyCreator ? 1 : 0,
    pi.friends.length,
    pi.teamIndex === null ? 0 : 1,
    pi.nationFlag === null ? 0 : 1,
    pi.clientID === null ? 0 : 1,
  ];
  captureGAME(`g_pinfo_${i}`, 7, [encS(c.name), encS(c.pt), encS(c.id), clanPresent, ...(clanPresent ? encS(c.clan) : [])], res);
}

// kind 8: bulkCost / maxBulkAmount. cost/gold are bigint; keep them under 2^53
// so they cross as f64. upgradeCosts is a bigint[] or absent.
const bulkCases = [
  { cost: 100, uc: [10, 30, 60], amount: 2, gold: 35 },
  { cost: 100, uc: [10, 30, 60], amount: 5, gold: 1000 },
  { cost: 100, uc: [10, 30, 60], amount: 0, gold: 500 },
  { cost: 100, uc: [10, 30, 60], amount: 3, gold: 35 },
  { cost: 1, uc: null, amount: 3, gold: 100 },
  { cost: 7, uc: null, amount: 1, gold: 0 },
  { cost: 2, uc: [1, 3, 6, 10], amount: 4, gold: 10 },
  { cost: 5, uc: [5, 10, 15], amount: 2, gold: 12 },
];
for (const [i, b] of bulkCases.entries()) {
  const bu = {
    cost: BigInt(b.cost),
    upgradeCosts: b.uc === null ? undefined : b.uc.map((x) => BigInt(x)),
  };
  const bulk = Number(GAME.bulkCost(bu, b.amount));
  const maxAmt = GAME.maxBulkAmount(bu, BigInt(b.gold));
  const ucPresent = b.uc === null ? 0 : 1;
  const args = [b.cost, ucPresent, ...(ucPresent ? [b.uc.length, ...b.uc] : []), b.amount, b.gold];
  captureGAME(`g_bulk_${i}`, 8, args, [bulk, maxAmt]);
}

// kind 9: the module consts.
captureGAME("g_consts", 9, [0], [
  encS(GAME.AllPlayers),
  encS(GAME.Duos),
  encS(GAME.Trios),
  encS(GAME.Quads),
  encS(GAME.HumansVsNations),
  GAME.MAX_UPGRADE_AMOUNT,
  GAME.NUKE_BULK_STEPS.length,
  ...GAME.NUKE_BULK_STEPS,
  GAME.STRUCTURE_BULK_STEPS.length,
  ...GAME.STRUCTURE_BULK_STEPS,
]);

// --- NationCreation scenario runner ------------------------------------------
// Exercises the real NationCreation.ts tables + name machinery through the
// shared run_op runner. Strings cross as `[len, u0, ..]` (UTF-16 code units);
// the Rust twin is `nation_creation::*`.
//
// kind table (matches the Rust dispatch):
//   0 templates dump  args [0]   res [tN,(pN,(tag,lit?)*)*]  tag 0=lit 1=noun 2=plural
//   1 nouns dump      args [0]   res [nN,(noun)*]
//   2 O_TO_OES dump   args [0]   res [kN,(word)*]
//   3 SPECIAL dump    args [0]   res [kN,(key,val)*]
//   4 pluralize       args [noun] res [plural]
//   5 generateName    args [seed] res [name]
//   6 uniqueName      args [seed,uN,(used)*] res [name]
//   7 compactCount    args [n,isCompact] res [count]
//   8 createRandomNations args [seed,target,mN,(nat)*,eN,(nat)*] res [kN,(out)*]
//     nat = [name,coordP,x,y,flagP,(flag)?]   out = nat + [id]
const ncScenarios = [];
const ncname = (s) => s.replace(/[^A-Za-z0-9]+/g, "_");
function captureNC(name, kind, args, res) {
  ncScenarios.push({
    name: ncname(name),
    kind,
    args: args.flat(Infinity).map(uenc),
    res: res.flat(Infinity).map(uenc),
  });
}

// kind 0: NAME_TEMPLATES dump — parts tagged 0=lit / 1=NOUN / 2=PLURAL_NOUN.
captureNC("nc_templates", 0, [0], [
  NC.NAME_TEMPLATES.length,
  ...NC.NAME_TEMPLATES.flatMap((t) => [
    t.length,
    ...t.flatMap((p) =>
      p === NC.PLURAL_NOUN ? [2] : p === NC.NOUN ? [1] : [0, ...encS(p)],
    ),
  ]),
]);

// kind 1-3: the word bank, the -oes set and the irregular map (insertion order).
captureNC("nc_nouns", 1, [0], [NC.NOUNS.length, ...NC.NOUNS.map(encS)]);
captureNC("nc_otooes", 2, [0], [[...NC.O_TO_OES].length, [...NC.O_TO_OES].map(encS)]);
captureNC("nc_special", 3, [0], [
  NC.SPECIAL_PLURALS.size,
  [...NC.SPECIAL_PLURALS.entries()].flatMap(([k, v]) => [encS(k), encS(v)]),
]);

// kind 4: pluralize over every bank word plus the UTF-16 / boundary cases.
const ncPluralCases = [
  ...NC.NOUNS,
  "y",
  "Key",
  "Bus",
  "Peach",
  "Sphinx",
  "Quiz",
  "Kazoo",
  "Kiwi",
  "Ibis",
  "",
  "ss",
  "ch",
  "a",
  "é",
  "yo",
];
for (const [i, w] of ncPluralCases.entries()) {
  captureNC(`nc_plural_${i}`, 4, [encS(w)], [encS(NC.pluralize(w))]);
}

// kind 5: generateNationName over a seed matrix (template draw, then noun).
const ncSeeds = [0, 1, 7, 42, 20260922, -5, 1.9, 123456789, 2 ** 31, -(2 ** 31), 987654321, 555, 36 ** 8, 1e6, 2 ** 53 - 1];
for (const [i, s] of ncSeeds.entries()) {
  const r = new PseudoRandom(s);
  captureNC(`nc_gen_${i}`, 5, [s], [encS(NC.generateNationName(r))]);
}

// kind 6: generateUniqueNationName — empty used, one collision, and the
// fallback path (all 1000 retry draws pre-marked used -> `base 1`).
{
  const s = 20260922;
  captureNC("nc_uniq_empty", 6, [s, 0], [encS(NC.generateUniqueNationName(new PseudoRandom(s), new Set()))]);

  const r1 = new PseudoRandom(s);
  const first = NC.generateNationName(r1);
  captureNC("nc_uniq_one", 6, [s, 1, encS(first)], [
    encS(NC.generateUniqueNationName(new PseudoRandom(s), new Set([first]))),
  ]);

  const rF = new PseudoRandom(777);
  const used = new Set();
  for (let i = 0; i < 1000; i++) used.add(NC.generateNationName(rF));
  captureNC("nc_uniq_fallback", 6, [777, used.size, [...used].map(encS)], [
    encS(NC.generateUniqueNationName(new PseudoRandom(777), used)),
  ]);
}

// kind 7: getCompactMapNationCount matrix.
for (const n of [0, 1, 2, 3, 4, 5, 8, 9, 13, 100]) {
  for (const c of [0, 1]) {
    captureNC(`nc_compact_${n}_${c}`, 7, [n, c], [NC.getCompactMapNationCount(n, c === 1)]);
  }
}

// kind 8: createRandomNations through the real toNation callback (Cell +
// PlayerInfo + shared-RNG nextID). nat = [name,coordP,x,y,flagP,(flag)?].
const ncNat = (name, coord, flag) => [
  encS(name),
  coord ? 1 : 0,
  ...(coord ? coord : [0, 0]),
  flag ? 1 : 0,
  ...(flag ? encS(flag) : []),
];
const ncRunRes = (seed, target, manifest, extras) => {
  const r = new PseudoRandom(seed);
  // Nation / Cell / PlayerInfo / PlayerType are Game.ts bindings (imported
  // by NationCreation but not re-exported), so they come from GAME.
  const toNation = (n) =>
    new GAME.Nation(
      n.coordinates !== undefined
        ? new GAME.Cell(n.coordinates[0], n.coordinates[1])
        : undefined,
      new GAME.PlayerInfo(
        n.name,
        GAME.PlayerType.Nation,
        null,
        r.nextID(),
        false,
        null,
        [],
        null,
        n.flag ?? null,
      ),
    );
  const mk = (list) => list.map(([name, coord, flag]) => ({
    name,
    ...(coord ? { coordinates: coord } : {}),
    ...(flag ? { flag } : {}),
  }));
  const nations = NC.createRandomNations(target, mk(manifest), mk(extras), toNation, r);
  return [
    nations.length,
    ...nations.flatMap((n) => {
      const pi = n.playerInfo;
      const sc = n.spawnCell;
      return [
        encS(pi.name),
        sc ? 1 : 0,
        sc ? sc.x : 0,
        sc ? sc.y : 0,
        pi.nationFlag ? 1 : 0,
        ...(pi.nationFlag ? encS(pi.nationFlag) : []),
        encS(pi.id),
      ];
    }),
  ];
};
const ncCases = [
  // [seed, target, manifest, extras]
  [11, 2, [["Alpha", [1, 2], "us"], ["Beta", null, null], ["Gamma", [7, 8], null]], []],
  [11, 0, [["Alpha", [1, 2], "us"]], []],
  [11, 5, [["Alpha", [1, 2], "us"], ["Beta", null, null]], []],
  [42, 4, [["Alpha", null, null], ["Beta", [3, 4], "in"], ["Gamma", null, null]], [["Extra1", [5, 6], "pk"], ["Alpha", null, null], ["Extra3", null, null]]],
  [42, 3, [], [["E1", null, null], ["E2", [9, 9], null]]],
  [7, 2, [], []],
  [20260922, 6, [["A", [0, 0], null], ["B", null, "us"], ["C", null, null]], [["D", [1, 1], null], ["E", null, null], ["F", null, null], ["G", null, null]]],
];
for (const [i, [seed, target, man, ext]] of ncCases.entries()) {
  const args = [
    seed,
    target,
    man.length,
    ...man.flatMap(([n, c, f]) => ncNat(n, c, f)),
    ext.length,
    ...ext.flatMap(([n, c, f]) => ncNat(n, c, f)),
  ];
  captureNC(`nc_crn_${i}`, 8, args, ncRunRes(seed, target, man, ext));
}

// --- GameUpdateType scenario runner -------------------------------------------
// Exercises the GameUpdates.ts GameUpdateType enum through the shared run_op
// runner (the prepared copy inlines the enum as a plain object with the same
// numeric values PLUS the V8 reverse mapping 0:"Tile" .. 23:"DonateEvent", so
// GameImpl's createGameUpdatesMap capture runs against real enum semantics).
// The dump / name-lookup scenarios pin the FORWARD mapping only, so the key
// list drops the integer-index (reverse) keys - `Object.keys` of the
// reverse-mapped object starts with "0".."23" (integer-index keys sort first),
// and filtering them back out keeps these token streams identical to the
// forward-only object the earlier captures saw. Strings cross as
// `[len, u0, ..]` (UTF-16 code units); the Rust twin is `game_updates::*`.
//
// kind table (matches the Rust dispatch):
//   0 dump all 24 (name, value) pairs   args [0]  res [24,(name,val)*24]
//   1 name lookup                        args [name]  res [value | -1]
const gupdScenarios = [];
const gupdname = (s) => s.replace(/[^A-Za-z0-9]+/g, "_");
function captureGUPD(name, kind, args, res) {
  gupdScenarios.push({
    name: gupdname(name),
    kind,
    args: args.flat(Infinity).map(uenc),
    res: res.flat(Infinity).map(uenc),
  });
}
const GUT = GUPD.GameUpdateType;
const GUT_NAMES = Object.keys(GUT).filter((k) => isNaN(Number(k)));

// kind 0: the full enum dump (declaration order = numeric order).
captureGUPD("gupd_dump", 0, [0], [
  GUT_NAMES.length,
  ...GUT_NAMES.flatMap((n) => [encS(n), GUT[n]]),
]);

// kind 1: every member name plus miss cases (wrong case, empty, prefix).
// The index keeps the generated Rust const ids unique (Tile vs tile collide
// once uppercased).
for (const [i, n] of [...GUT_NAMES, "tile", "Tile2", "", "Donat", "DonateEven", "Alliance"].entries()) {
  captureGUPD(`gupd_lookup_${i}`, 1, [encS(n)], [GUT[n] ?? -1]);
}

// --- NationEmojiBehavior scenario runner --------------------------------------
// Exercises the Util.ts emojiTable / flattenedEmojiTable and the 23 EMOJI_*
// id arrays from NationEmojiBehavior.ts through the shared run_op runner.
// Strings cross as `[len, u0, ..]` (UTF-16 code units); the Rust twin is
// `nation_emoji::*`.
//
// kind table (matches the Rust dispatch):
//   0 emojiTable dump   args [0]  res [12,(5,(str)*5)*12]
//   1 flattened dump    args [0]  res [60,(str)*60]
//   2 EMOJI_* dump      args [0]  res [23,(name,len,(id)*len)*23]
//   3 emoji_id batch    args [n,(str)*n]  res [n,(id)*n]
const neScenarios = [];
const nename = (s) => s.replace(/[^A-Za-z0-9]+/g, "_");
function captureNE(name, kind, args, res) {
  neScenarios.push({
    name: nename(name),
    kind,
    args: args.flat(Infinity).map(uenc),
    res: res.flat(Infinity).map(uenc),
  });
}
const NE_NAMES = [
  "EMOJI_ASSIST_ACCEPT",
  "EMOJI_ASSIST_RELATION_TOO_LOW",
  "EMOJI_ASSIST_TARGET_ME",
  "EMOJI_ASSIST_TARGET_ALLY",
  "EMOJI_AGGRESSIVE_ATTACK",
  "EMOJI_ATTACK",
  "EMOJI_WARSHIP_RETALIATION",
  "EMOJI_NUKE",
  "EMOJI_GOT_INSULTED",
  "EMOJI_LOVE",
  "EMOJI_CONFUSED",
  "EMOJI_BRAG",
  "EMOJI_CHARM_ALLIES",
  "EMOJI_CLOWN",
  "EMOJI_RAT",
  "EMOJI_OVERWHELMED",
  "EMOJI_CONGRATULATE",
  "EMOJI_SCARED_OF_THREAT",
  "EMOJI_BORED",
  "EMOJI_HANDSHAKE",
  "EMOJI_DONATION_OK",
  "EMOJI_DONATION_TOO_SMALL",
  "EMOJI_GREET",
];

// kind 0: the 12x5 grid, row-major, each row prefixed with its width.
captureNE("ne_table", 0, [0], [
  Util.emojiTable.length,
  ...Util.emojiTable.flatMap((row) => [row.length, ...row.map(encS)]),
]);

// kind 1: the flattened 60-entry table.
captureNE("ne_flat", 1, [0], [
  Util.flattenedEmojiTable.length,
  ...Util.flattenedEmojiTable.map(encS),
]);

// kind 2: the 23 id arrays in declaration order (names ride along for
// locating a mismatch).
captureNE("ne_consts", 2, [0], [
  NE_NAMES.length,
  ...NE_NAMES.flatMap((n) => {
    const arr = NE[n];
    return [encS(n), arr.length, ...arr];
  }),
]);

// kind 3: emoji_id over every table entry, the emojis the consts reference,
// and the -1 branches (not-in-table emoji, multi-char, empty, plain ASCII).
const neIdCases = [
  ...Util.flattenedEmojiTable,
  "🐔🐔",
  "😀😀",
  "🤦",
  "❤",
  "",
  "a",
  "👍 ",
];
for (const [i, e] of neIdCases.entries()) {
  captureNE(`ne_id_${i}`, 3, [1, encS(e)], [1, Util.flattenedEmojiTable.indexOf(e)]);
}
// one batch call with the whole case list (pins the count framing both ways)
captureNE("ne_id_batch", 3, [neIdCases.length, neIdCases.map(encS)], [
  neIdCases.length,
  ...neIdCases.map((e) => Util.flattenedEmojiTable.indexOf(e)),
]);

// --- CosmeticSchemas scenario runner ------------------------------------------
// Exercises the CosmeticSchemas.ts runtime values (EFFECT_TYPES /
// TRAIL_EFFECT_TYPES / NUKE_EXPLOSION_TYPES, the four pure effect/slot
// functions, and DefaultPattern) through the shared run_op runner. Strings
// cross as `[len, u0, ..]` (UTF-16 code units); the Rust twin is
// `cosmetic_schemas::*`.
//
// kind table (matches the Rust dispatch):
//   0 EFFECT_TYPES dump       args [0]  res [7,(str)*7]
//   1 TRAIL dump              args [0]  res [2,(str)*2]
//   2 NUKE dump               args [0]  res [3,(str)*3]
//   3 DefaultPattern          args [0]  res [(name),(data)]
//   4 isTrailEffect batch     args [n,(str)*n]  res [n,(0/1)*n]
//   5 isNukeExplosion batch   args [n,(str)*n]  res [n,(0/1)*n]
//   6 effectTypeForSlot batch args [n,(str)*n]  res [n,([1,str]|[0])*n]
//   7 effectMatchesSlot batch args [n,(et,np,ns?,slot)*n]  res [n,(0/1)*n]
const csScenarios = [];
const csname = (s) => s.replace(/[^A-Za-z0-9]+/g, "_");
function captureCS(name, kind, args, res) {
  csScenarios.push({
    name: csname(name),
    kind,
    args: args.flat(Infinity).map(uenc),
    res: res.flat(Infinity).map(uenc),
  });
}

// kinds 0-2: the three `as const` arrays.
captureCS("cs_effect_types", 0, [0], [
  Cosmetic.EFFECT_TYPES.length,
  ...Cosmetic.EFFECT_TYPES.map(encS),
]);
captureCS("cs_trail_types", 1, [0], [
  Cosmetic.TRAIL_EFFECT_TYPES.length,
  ...Cosmetic.TRAIL_EFFECT_TYPES.map(encS),
]);
captureCS("cs_nuke_types", 2, [0], [
  Cosmetic.NUKE_EXPLOSION_TYPES.length,
  ...Cosmetic.NUKE_EXPLOSION_TYPES.map(encS),
]);

// kind 3: DefaultPattern (colorPalette is undefined in the TS; the Rust twin
// carries the two string fields only, and the capture never reads it).
captureCS("cs_default_pattern", 3, [0], [
  encS(Cosmetic.DefaultPattern.name),
  encS(Cosmetic.DefaultPattern.patternData),
]);

// kinds 4-6: probe strings - every catalog member plus the boundaries
// (empty, wrong case, trailing space; the bare "nukeExplosion" key rides in
// via EFFECT_TYPES and must resolve to undefined in forSlot).
const csCases = [
  ...Cosmetic.EFFECT_TYPES,
  ...Cosmetic.NUKE_EXPLOSION_TYPES,
  "",
  "a",
  "TransportShip",
  "atom ",
];
for (const [i, s] of csCases.entries()) {
  captureCS(`cs_trail_${i}`, 4, [1, encS(s)], [
    1,
    enc(Cosmetic.isTrailEffect({ effectType: s })),
  ]);
  captureCS(`cs_nukefx_${i}`, 5, [1, encS(s)], [
    1,
    enc(Cosmetic.isNukeExplosionEffect({ effectType: s })),
  ]);
  const t = Cosmetic.effectTypeForSlot(s);
  captureCS(`cs_forslot_${i}`, 6, [1, encS(s)], [
    1,
    t === undefined ? [0] : [1, encS(t)],
  ]);
}
captureCS("cs_trail_batch", 4, [csCases.length, csCases.map(encS)], [
  csCases.length,
  ...csCases.map((s) => enc(Cosmetic.isTrailEffect({ effectType: s }))),
]);
captureCS("cs_nukefx_batch", 5, [csCases.length, csCases.map(encS)], [
  csCases.length,
  ...csCases.map((s) => enc(Cosmetic.isNukeExplosionEffect({ effectType: s }))),
]);
captureCS("cs_forslot_batch", 6, [csCases.length, csCases.map(encS)], [
  csCases.length,
  ...csCases.flatMap((s) => {
    const t = Cosmetic.effectTypeForSlot(s);
    return t === undefined ? [0] : [1, encS(t)];
  }),
]);

// kind 7: effectMatchesSlot over (effect, slot) pairs; the nukeType token
// rides along only for nukeExplosion effects (np=1). The last case pins the
// quirk that an effect whose effectType is a nuke *type name* never matches
// (effectTypeForSlot maps the slot to "nukeExplosion", not "atom").
const csMatchCases = [
  [{ effectType: "nukeTrail" }, "nukeTrail"],
  [{ effectType: "nukeExplosion", attributes: { nukeType: "atom" } }, "atom"],
  [{ effectType: "nukeExplosion", attributes: { nukeType: "atom" } }, "hydro"],
  [
    { effectType: "nukeExplosion", attributes: { nukeType: "atom" } },
    "nukeExplosion",
  ],
  [{ effectType: "transportShipTrail" }, "nukeTrail"],
  [{ effectType: "structures" }, "structures"],
  [{ effectType: "train" }, "bogus"],
  [{ effectType: "atom" }, "atom"],
];
const csMatchArgs = ([e, slot]) => {
  const nuke = e.effectType === "nukeExplosion";
  return [
    encS(e.effectType),
    nuke ? 1 : 0,
    nuke ? encS(e.attributes.nukeType) : [],
    encS(slot),
  ];
};
for (const [i, c] of csMatchCases.entries()) {
  captureCS(`cs_match_${i}`, 7, [1, csMatchArgs(c)], [
    1,
    enc(Cosmetic.effectMatchesSlot(c[0], c[1])),
  ]);
}
captureCS("cs_match_batch", 7, [csMatchCases.length, csMatchCases.map(csMatchArgs)], [
  csMatchCases.length,
  ...csMatchCases.map(([e, slot]) => enc(Cosmetic.effectMatchesSlot(e, slot))),
]);

// --- StatsSchemas scenario runner ---------------------------------------------
// Exercises the StatsSchemas.ts runtime values (the three `as const` unit-name
// arrays, the two UnitType->short-name lookup objects, the 34 numeric index
// constants, and the module-private toBigInt coercion) through the shared
// run_op runner. Strings cross as `[len, u0, ..]` (UTF-16 code units); the
// Rust twin is `stats_schemas::*`.
//
// kind table (matches the Rust dispatch):
//   0 bombUnits dump          args [0]  res [4,(str)*4]
//   1 boatUnits dump          args [0]  res [2,(str)*2]
//   2 otherUnits dump         args [0]  res [7,(str)*7]
//   3 unitTypeToBombUnit dump args [0]  res [4,(key,val)*4]  (declaration
//     order = the TS computed-key insertion order)
//   4 unitTypeToOtherUnit dump args [0] res [7,(key,val)*7]
//   5 index consts dump       args [0]  res [34,(name,value)*34] (TS order)
//   6 toBigInt batch          args [n,(tag,str?)*n]  res [n,(per-item:
//     [0,value] ok | [1] threw)*n]
//     tag 0=null, 1=undefined, 2=string (followed by the string), 3=bigint
//     (followed by its decimal string). Throw cases all carry values that
//     stay within |v| <= 2^53 so the [0,value] token is exact; the arbitrary-
//     precision side of TS BigInt is out of scope (doc-comment in the Rust
//     twin).
const stScenarios = [];
const stname = (s) => s.replace(/[^A-Za-z0-9]+/g, "_");
function captureST(name, kind, args, res) {
  stScenarios.push({
    name: stname(name),
    kind,
    args: args.flat(Infinity).map(uenc),
    res: res.flat(Infinity).map(uenc),
  });
}

// kinds 0-2: the three `as const` arrays.
captureST("st_bomb_units", 0, [0], [
  St.bombUnits.length,
  ...St.bombUnits.map(encS),
]);
captureST("st_boat_units", 1, [0], [
  St.boatUnits.length,
  ...St.boatUnits.map(encS),
]);
captureST("st_other_units", 2, [0], [
  St.otherUnits.length,
  ...St.otherUnits.map(encS),
]);

// kinds 3-4: the lookup objects; Object.entries preserves the TS computed-key
// insertion order (all keys are non-integer strings).
captureST("st_bomb_map", 3, [0], [
  Object.keys(St.unitTypeToBombUnit).length,
  ...Object.entries(St.unitTypeToBombUnit).flatMap(([k, v]) => [encS(k), encS(v)]),
]);
captureST("st_other_map", 4, [0], [
  Object.keys(St.unitTypeToOtherUnit).length,
  ...Object.entries(St.unitTypeToOtherUnit).flatMap(([k, v]) => [encS(k), encS(v)]),
]);

// kind 5: the 34 numeric index constants in TS declaration order.
const ST_INDEX_CONSTS = [
  ["ATTACK_INDEX_SENT", St.ATTACK_INDEX_SENT],
  ["ATTACK_INDEX_RECV", St.ATTACK_INDEX_RECV],
  ["ATTACK_INDEX_CANCEL", St.ATTACK_INDEX_CANCEL],
  ["ATTACK_INDEX_MAX_RECV", St.ATTACK_INDEX_MAX_RECV],
  ["TILE_INDEX_PEAK", St.TILE_INDEX_PEAK],
  ["TILE_INDEX_DRAWDOWN_PEAK", St.TILE_INDEX_DRAWDOWN_PEAK],
  ["TILE_INDEX_DRAWDOWN_TROUGH", St.TILE_INDEX_DRAWDOWN_TROUGH],
  ["ALLIANCE_INDEX_FORMED", St.ALLIANCE_INDEX_FORMED],
  ["ALLIANCE_INDEX_BROKEN_BY_OTHER", St.ALLIANCE_INDEX_BROKEN_BY_OTHER],
  ["ALLIANCE_INDEX_EXPIRED", St.ALLIANCE_INDEX_EXPIRED],
  ["ALLIANCE_INDEX_HELD_TO_END", St.ALLIANCE_INDEX_HELD_TO_END],
  ["ALLIANCE_INDEX_PEAK_CONCURRENT", St.ALLIANCE_INDEX_PEAK_CONCURRENT],
  ["ALLIANCE_INDEX_LONGEST_HELD", St.ALLIANCE_INDEX_LONGEST_HELD],
  ["PLAYER_INDEX_HUMAN", St.PLAYER_INDEX_HUMAN],
  ["PLAYER_INDEX_NATION", St.PLAYER_INDEX_NATION],
  ["PLAYER_INDEX_BOT", St.PLAYER_INDEX_BOT],
  ["BOAT_INDEX_SENT", St.BOAT_INDEX_SENT],
  ["BOAT_INDEX_ARRIVE", St.BOAT_INDEX_ARRIVE],
  ["BOAT_INDEX_CAPTURE", St.BOAT_INDEX_CAPTURE],
  ["BOAT_INDEX_DESTROY", St.BOAT_INDEX_DESTROY],
  ["BOMB_INDEX_LAUNCH", St.BOMB_INDEX_LAUNCH],
  ["BOMB_INDEX_LAND", St.BOMB_INDEX_LAND],
  ["BOMB_INDEX_INTERCEPT", St.BOMB_INDEX_INTERCEPT],
  ["GOLD_INDEX_WORK", St.GOLD_INDEX_WORK],
  ["GOLD_INDEX_WAR", St.GOLD_INDEX_WAR],
  ["GOLD_INDEX_TRADE", St.GOLD_INDEX_TRADE],
  ["GOLD_INDEX_STEAL", St.GOLD_INDEX_STEAL],
  ["GOLD_INDEX_TRAIN_SELF", St.GOLD_INDEX_TRAIN_SELF],
  ["GOLD_INDEX_TRAIN_OTHER", St.GOLD_INDEX_TRAIN_OTHER],
  ["OTHER_INDEX_BUILT", St.OTHER_INDEX_BUILT],
  ["OTHER_INDEX_DESTROY", St.OTHER_INDEX_DESTROY],
  ["OTHER_INDEX_CAPTURE", St.OTHER_INDEX_CAPTURE],
  ["OTHER_INDEX_LOST", St.OTHER_INDEX_LOST],
  ["OTHER_INDEX_UPGRADE", St.OTHER_INDEX_UPGRADE],
];
captureST("st_consts", 5, [0], [
  ST_INDEX_CONSTS.length,
  ...ST_INDEX_CONSTS.flatMap(([n, v]) => [encS(n), v]),
]);

// kind 6: toBigInt over every branch (bigint passthrough, null/undefined ->
// 0n, decimal-string regex hit/miss, leading zeros, negative zero) plus the
// throw path. Single-item and one-batch scenarios.
const stBigIntCases = [
  ["bigint", 0n],
  ["bigint", -1n],
  ["bigint", 2n ** 53n],
  ["str", "123"],
  ["str", "-456"],
  ["str", "0"],
  ["str", "007"],
  ["str", ""],
  ["str", "1.5"],
  ["str", "1e3"],
  ["str", "-0"],
  ["null", null],
  ["undef", undefined],
  ["str", " 1"],
  ["str", "+1"],
  ["bigint", 9007199254740991n],
];
const stBigIntArg = ([tag, v]) =>
  tag === "null"
    ? [0]
    : tag === "undef"
      ? [1]
      : tag === "str"
        ? [2, encS(v)]
        : [3, encS(v.toString())];
const stBigIntRes = ([, v]) => {
  let r;
  try {
    r = St.toBigInt(v);
  } catch {
    return [1];
  }
  if (typeof r !== "bigint") return [1];
  // Every scenario value stays within |v| <= 2^53 so the f64 token is exact;
  // if a future case breaks that, fail loudly instead of losing precision.
  if (r > 2n ** 53n || r < -(2n ** 53n))
    throw new Error(`st: toBigInt result out of f64-exact range: ${r}`);
  return [0, Number(r)];
};
for (const [i, c] of stBigIntCases.entries()) {
  captureST(`st_bigint_${i}`, 6, [1, stBigIntArg(c)], [1, stBigIntRes(c)]);
}
captureST("st_bigint_batch", 6, [stBigIntCases.length, stBigIntCases.map(stBigIntArg)], [
  stBigIntCases.length,
  ...stBigIntCases.flatMap(stBigIntRes),
]);

// --- Schemas scenario runner ---------------------------------------------------
// Exercises the Schemas.ts runtime values (the enum option arrays, the lobby
// numeric/string constants, the LogSeverity table, the JSON-derived QuickChat
// key list, and the three regex-backed predicates) through the shared run_op
// runner. Strings cross as `[len, u0, ..]` (UTF-16 code units); the Rust twin
// is `schemas::*`.
//
// kind table (matches the Rust dispatch):
//   0 PublicGameTypeSchema.options dump  args [0]  res [4,(str)*4]
//   1 SCHEDULED_PUBLIC_GAME_TYPES dump   args [0]  res [3,(str)*3]
//   2 LobbyAccentSchema.options dump     args [0]  res [4,(str)*4]
//   3 ClientPlatformSchema.options dump  args [0]  res [3,(str)*3]
//   4 ReportReasonSchema.options dump    args [0]  res [4,(str)*4]
//   5 numeric lobby consts               args [0]  res [3,(val)*3]
//     (MAX_HOSTED_LOBBIES, HOSTED_LOBBY_AUTO_START_MS, FEATURED_LOBBY_AUTO_START_MS)
//   6 string consts                      args [0]  res [5,(str)*5]
//     (CLIENT_ID_MAPPING, ADMIN_BOT_CLIENT_ID, GAME_ID_REGEX.source,
//      RENDERABLE_NAME_ALNUM, RENDERABLE_NAME_CHARS)
//   7 LogSeverity dump                   args [0]  res [5,(name,val)*5]
//   8 QuickChatKeySchema dump            args [0]  res [n,(str)*n]
//   9 isValidGameID batch                args [n,(str)*n]  res [n,(0/1)*n]
//  10 RENDERABLE_NAME_CHAR_RE.test batch args [n,(str)*n]  res [n,(0/1)*n]
//  11 RENDERABLE_NAME_HAS_ALNUM_RE.test batch, same shape
const scScenarios = [];
const scname = (s) => s.replace(/[^A-Za-z0-9]+/g, "_");
function captureSC(name, kind, args, res) {
  scScenarios.push({
    name: scname(name),
    kind,
    args: args.flat(Infinity).map(uenc),
    res: res.flat(Infinity).map(uenc),
  });
}

// kinds 0-4: the enum option arrays (declaration order).
captureSC("sc_public_game_types", 0, [0], [
  Sc.PublicGameTypeSchema.options.length,
  ...Sc.PublicGameTypeSchema.options.map(encS),
]);
captureSC("sc_scheduled_types", 1, [0], [
  Sc.SCHEDULED_PUBLIC_GAME_TYPES.length,
  ...Sc.SCHEDULED_PUBLIC_GAME_TYPES.map(encS),
]);
captureSC("sc_lobby_accents", 2, [0], [
  Sc.LobbyAccentSchema.options.length,
  ...Sc.LobbyAccentSchema.options.map(encS),
]);
captureSC("sc_client_platforms", 3, [0], [
  Sc.ClientPlatformSchema.options.length,
  ...Sc.ClientPlatformSchema.options.map(encS),
]);
captureSC("sc_report_reasons", 4, [0], [
  Sc.ReportReasonSchema.options.length,
  ...Sc.ReportReasonSchema.options.map(encS),
]);

// kind 5: the three numeric lobby constants (TS declaration order).
captureSC("sc_num_consts", 5, [0], [
  3,
  Sc.MAX_HOSTED_LOBBIES,
  Sc.HOSTED_LOBBY_AUTO_START_MS,
  Sc.FEATURED_LOBBY_AUTO_START_MS,
]);

// kind 6: the five string constants. The ALNUM/CHARS sources carry literal
// `\u00C0`-style text (backslash-u-4hex), not real code points - the dump
// pins the exact bytes.
captureSC("sc_str_consts", 6, [0], [
  5,
  ...encS(Sc.CLIENT_ID_MAPPING),
  ...encS(Sc.ADMIN_BOT_CLIENT_ID),
  ...encS(Sc.GAME_ID_REGEX.source),
  ...encS(Sc.RENDERABLE_NAME_ALNUM),
  ...encS(Sc.RENDERABLE_NAME_CHARS),
]);

// kind 7: LogSeverity (name,value) pairs in declaration order.
captureSC("sc_log_severities", 7, [0], [
  Object.keys(Sc.LogSeverity).length,
  ...Object.entries(Sc.LogSeverity).flatMap(([k, v]) => [encS(k), encS(v)]),
]);

// kind 8: the full JSON-derived QuickChat key list (Object.entries insertion
// order over the six categories).
captureSC("sc_quick_chat_keys", 8, [0], [
  Sc.QuickChatKeySchema.length,
  ...Sc.QuickChatKeySchema.map(encS),
]);

// kind 9: isValidGameID over the length boundaries (7/8/9/10/11) plus the
// character-class edges (no u flag: JS tests UTF-16 code units, so a
// surrogate pair or a Latin-1 letter fails).
const scGameIds = [
  "",
  "a",
  "abcdefgh",
  "abcdefghi",
  "ABCDEFGHIJ",
  "abcdefg",
  "abcdefghijk",
  "abcd1234",
  "12345678",
  "0aZ9zA0a",
  "abcdefg_",
  "abcd 123",
  "abcd-123",
  "Àbcdefgh",
  "\u{1F600}abcdefg",
  "\uD83D\uDE00abcdef",
  "AbCdEfGhIj",
  "ADMINBOT",
];
captureSC("sc_game_ids_batch", 9, [scGameIds.length, scGameIds.map(encS)], [
  scGameIds.length,
  ...scGameIds.map((s) => (Sc.isValidGameID(s) ? 1 : 0)),
]);

// kind 10: RENDERABLE_NAME_CHAR_RE over every printable ASCII code point
// (the `\\-` in the class source resolves to an escaped literal `-`, NOT a
// U+005C-U+0061 range: `[` `]` `^` backtick `\` all test false, `-` tests
// true) plus the Latin-1 range boundaries and the multi-unit cases (the u
// flag anchors one code point, so empty / 2-code-point / lone-surrogate
// strings test false).
const scCharCases = [];
for (let cp = 0x20; cp <= 0x7e; cp++) scCharCases.push(String.fromCodePoint(cp));
for (const cp of [
  0x00, 0x09, 0x1f, 0xa0, 0xbf, 0xc0, 0xd6, 0xd7, 0xd8, 0xf6, 0xf7, 0xf8,
  0xff, 0x100, 0x178, 0x250, 0x1f600,
]) {
  scCharCases.push(String.fromCodePoint(cp));
}
for (const s of ["", "ab", "a ", "  ", "\u00C0\u00D6", "\uD83D", "\uD83DA"]) {
  scCharCases.push(s);
}
captureSC("sc_char_re_batch", 10, [scCharCases.length, scCharCases.map(encS)], [
  scCharCases.length,
  ...scCharCases.map((s) => (Sc.RENDERABLE_NAME_CHAR_RE.test(s) ? 1 : 0)),
]);

// kind 11: RENDERABLE_NAME_HAS_ALNUM_RE (unanchored, u flag: any-code-point
// search; lone surrogates are each their own code point and never match).
const scHasCases = [
  "",
  " ",
  "_.",
  "-",
  "\\",
  "a",
  "Z",
  "9",
  "  a",
  "\u{1F600}",
  "\uD83D",
  "ab",
  "~~~",
  "\u00D7",
  "\u00F7",
  "\u20AC",
  "\u00C0",
  "\u00DF",
  "\u00FF",
  "\u0100",
  "\u0178",
  "a\u{1F600}",
  "\uD83DA",
  "\u00D6\u00D8",
];
captureSC("sc_has_alnum_batch", 11, [scHasCases.length, scHasCases.map(encS)], [
  scHasCases.length,
  ...scHasCases.map((s) => (Sc.RENDERABLE_NAME_HAS_ALNUM_RE.test(s) ? 1 : 0)),
]);

// --- ApiSchemas scenario runner ----------------------------------------------
// Exercises the runtime-value subset of src/core/ApiSchemas.ts: the data
// constants, the z.enum option arrays (read through the functional zod shim),
// and the four pure predicates. The Rust twin is `api_schemas::run_op`.
//
// kind table:
//   0  [0] -> [2,(str)*2]            ADMIN_ROLES dump
//   1  [0] -> [3,(str)*3]            PlayerStatsGameModes dump (string values)
//   2  [0] -> [4,(str)*4]            PlayerGameModeFilters dump
//   3  [0] -> [3,(str)*3]            PlayerGameTypeFilters dump
//   4  [0] -> [4,(str)*4]            UsernameStatusSchema.options dump
//   5  [0] -> [3,(str)*3]            BareClaimSchema.options dump
//   6  [0] -> [4,(str)*4]            TribeNameStatusSchema.options dump
//   7  [0] -> [3,(str)*3]            PlayerGameResultSchema.options dump
//   8  [0] -> [2,(str)*2]            PaymentsProviderSchema.options dump
//   9  [0] -> [3,(str)*3]            PaymentsKindSchema.options dump
//   10 [0] -> [3,(str)*3]            PaymentsHandoffSchema.options dump
//   11 [0] -> [4,(str)*4]            SteamOrderResolutionSchema.options dump
//   12 [n,(str)*n] -> [n,(0/1)*n]    isAdminRole batch
//   13 [n,(str)*n] -> [n,(0/1)*n]    isTemporaryUsername batch
//   14 [n,(str)*n] -> [n,(0/1)*n]    isVerifiedUsername batch
//   15 [n,(enc sub)*n] -> [n,(0/1)*n] isGrantedSubscription batch
//        enc sub: [0]=undefined, [1,(str)provider]=provider string value,
//        [2]=provider is null
const asSchemasScenarios = [];
function captureAS(name, kind, args, res) {
  asSchemasScenarios.push({
    name,
    kind,
    args: args.flat(Infinity).map(uenc),
    res: res.flat(Infinity).map(uenc),
  });
}

// kinds 0-3: the data constants (declaration order; the GameModes dump
// captures the actual string values, not the enum member names).
captureAS("as_admin_roles", 0, [0], [
  Api.ADMIN_ROLES.length,
  ...Api.ADMIN_ROLES.map(encS),
]);
captureAS("as_player_stats_game_modes", 1, [0], [
  Api.PlayerStatsGameModes.length,
  ...Api.PlayerStatsGameModes.map(encS),
]);
captureAS("as_player_game_mode_filters", 2, [0], [
  Api.PlayerGameModeFilters.length,
  ...Api.PlayerGameModeFilters.map(encS),
]);
captureAS("as_player_game_type_filters", 3, [0], [
  Api.PlayerGameTypeFilters.length,
  ...Api.PlayerGameTypeFilters.map(encS),
]);

// kinds 4-11: the z.enum option arrays.
captureAS("as_username_status_options", 4, [0], [
  Api.UsernameStatusSchema.options.length,
  ...Api.UsernameStatusSchema.options.map(encS),
]);
captureAS("as_bare_claim_options", 5, [0], [
  Api.BareClaimSchema.options.length,
  ...Api.BareClaimSchema.options.map(encS),
]);
captureAS("as_tribe_name_status_options", 6, [0], [
  Api.TribeNameStatusSchema.options.length,
  ...Api.TribeNameStatusSchema.options.map(encS),
]);
captureAS("as_player_game_result_options", 7, [0], [
  Api.PlayerGameResultSchema.options.length,
  ...Api.PlayerGameResultSchema.options.map(encS),
]);
captureAS("as_payments_provider_options", 8, [0], [
  Api.PaymentsProviderSchema.options.length,
  ...Api.PaymentsProviderSchema.options.map(encS),
]);
captureAS("as_payments_kind_options", 9, [0], [
  Api.PaymentsKindSchema.options.length,
  ...Api.PaymentsKindSchema.options.map(encS),
]);
captureAS("as_payments_handoff_options", 10, [0], [
  Api.PaymentsHandoffSchema.options.length,
  ...Api.PaymentsHandoffSchema.options.map(encS),
]);
captureAS("as_steam_order_resolution_options", 11, [0], [
  Api.SteamOrderResolutionSchema.options.length,
  ...Api.SteamOrderResolutionSchema.options.map(encS),
]);

// kind 12: isAdminRole - the two admin roles plus every other wire role,
// case-sensitivity negatives, and the empty string.
const asAdminRoles = [
  "admin",
  "root",
  "mod",
  "flagged",
  "banned",
  "",
  "Admin",
  "ADMIN",
];
captureAS("as_is_admin_role_batch", 12, [
  asAdminRoles.length,
  asAdminRoles.map(encS),
], [
  asAdminRoles.length,
  ...asAdminRoles.map((s) => (Api.isAdminRole(s) ? 1 : 0)),
]);

// kind 13: isTemporaryUsername - the exact TEMPORARY#### shape plus the
// boundary cases (3/5 digits, lowercase, non-ASCII digits that JS \d without
// the u flag does not match, interior space, prefix/suffix, empty).
const asTempNames = [
  "TEMPORARY1234",
  "TEMPORARY123",
  "TEMPORARY12345",
  "temporary1234",
  "TEMPORARY١٢٣٤",
  "TEMPORARY12 4",
  "XTEMPORARY1234",
  "TEMPORARY1234X",
  "",
];
captureAS("as_is_temporary_username_batch", 13, [
  asTempNames.length,
  asTempNames.map(encS),
], [
  asTempNames.length,
  ...asTempNames.map((s) => (Api.isTemporaryUsername(s) ? 1 : 0)),
]);

// kind 14: isVerifiedUsername - bare names true, dotted / TEMPORARY####
// renames false, the dot-only and empty edges.
const asVerifiedNames = [
  "Ninja",
  "Ninja.4471",
  "TEMPORARY1234",
  "TEMPORARY1234.5",
  "a.b.",
  ".",
  "",
];
captureAS("as_is_verified_username_batch", 14, [
  asVerifiedNames.length,
  asVerifiedNames.map(encS),
], [
  asVerifiedNames.length,
  ...asVerifiedNames.map((s) => (Api.isVerifiedUsername(s) ? 1 : 0)),
]);

// kind 15: isGrantedSubscription - the three-state provider rule: null =
// granted (true), a string rail = paid (false), a missing provider field =
// the pre-feature server (false), and no subscription at all (false).
// enc sub: [0]=undefined, [1,(str)provider]=string, [2]=null.
const asSubs = [
  [0],
  [2],
  [1, ...encS("steam")],
  [1, ...encS("stripe")],
  [0],
  [2],
  [2],
  [1, ...encS("future_rail")],
];
captureAS("as_is_granted_subscription_batch", 15, [
  asSubs.length,
  asSubs,
], [
  asSubs.length,
  ...asSubs.map((e) => {
    const sub =
      e[0] === 0 ? undefined : e[0] === 2 ? { provider: null } : { provider: String.fromCharCode(...e.slice(2)) };
    return Api.isGrantedSubscription(sub) ? 1 : 0;
  }),
]);


// --- TerrainMapLoader scenario runner ----------------------------------------
// Exercises the real game/TerrainMapLoader.ts loadTerrainMap (loadImages fixed
// false) against a scripted GameMapLoader mock: getMapData returns a *stable*
// manifest object (so the Compact in-place nation scaling is observable across
// ops through the shared array reference) and fresh Uint8Array bins per call;
// the getMapData call counter pins the module loadedMaps cache (a hit skips
// the loader, every throw path leaves the key uncached). binKind 1 hands back
// a zero-length buffer so genTerrainFromBin throws its real message. The Rust
// twin is `terrain_map_loader::run_op`.
//
// args layout (flat f64; strings [len, u0, ..]):
//   [map.w, map.h, map.nlt, map4x.w, .., map16x.w, ..,
//    n, (hasCoord, x?, y?, flag, name)*n,
//    addPresent, m, (...)*m,
//    tgsaPresent, k, (key, areasLen, (x,y,w,h)*areasLen)*k,
//    layersPresent, l, (id, placement, hasAlpha, alpha?)*l,
//    (binKind, len, (byte)*len)*3,      // mapBin, map4xBin, map16xBin
//    opsLen, (mapName, size)*opsLen]    // size 0=Normal, 1=Compact
//
// res: [opsLen, (per call)*, getMapDataCalls]
//   success: [0, gameMap, miniMap, nations, addNations, tgsa, layers]
//     gameMap / miniMap: [w, h, nlt, terrainLen, (byte)*terrainLen]
//     nations: [n, (hasCoord, x?, y?, flag, name)*n]
//     addNations: [present, m, (...)*m]   // present = manifest field defined
//     tgsa: [present, k, (key, areasLen, (x,y,w,h)*areasLen)*k]
//     layers: [present, l, (id, placement, hasAlpha, alpha?)*l]
//   throws: [1|2|3, (str)message]         // bin / placement / alpha
const tmlScenarios = [];
const tmlname = (s) => s.replace(/[^A-Za-z0-9]+/g, "_");

async function tmlRun(args) {
  let p = 0;
  const num = () => args[p++];
  const str = () => {
    const len = args[p++];
    let s = "";
    for (let i = 0; i < len; i++) s += String.fromCharCode(args[p++]);
    return s;
  };
  const meta = () => ({ width: num(), height: num(), num_land_tiles: num() });
  const nation = () => {
    const has = args[p++];
    const n = {};
    if (has === 1) n.coordinates = [num(), num()];
    n.flag = str();
    n.name = str();
    return n;
  };
  const map = meta();
  const map4x = meta();
  const map16x = meta();
  const nations = [];
  for (let i = 0, n = args[p++]; i < n; i++) nations.push(nation());
  const addPresent = args[p++];
  const addNations = [];
  for (let i = 0, m = args[p++]; i < m; i++) addNations.push(nation());
  const tgsaPresent = args[p++];
  const tgsa = {};
  for (let i = 0, k = args[p++]; i < k; i++) {
    const key = str();
    const areas = [];
    for (let j = 0, a = args[p++]; j < a; j++) {
      areas.push({ x: num(), y: num(), width: num(), height: num() });
    }
    tgsa[key] = areas;
  }
  const layersPresent = args[p++];
  const layers = [];
  for (let i = 0, l = args[p++]; i < l; i++) {
    const layer = { id: str(), placement: str() };
    if (args[p++] === 1) layer.alpha = num();
    layers.push(layer);
  }
  const bins = [];
  for (let b = 0; b < 3; b++) {
    const kind = args[p++];
    const len = args[p++];
    const bytes = args.slice(p, p + len);
    p += len;
    bins.push(kind === 1 ? [] : bytes);
  }
  const manifest = {
    name: "cap",
    map,
    map4x,
    map16x,
    nations,
    ...(addPresent === 1 ? { additionalNations: addNations } : {}),
    ...(tgsaPresent === 1 ? { teamGameSpawnAreas: tgsa } : {}),
    ...(layersPresent === 1 ? { layers } : {}),
  };
  let calls = 0;
  const loader = {
    getMapData(_map) {
      calls++;
      return {
        manifest: () => Promise.resolve(manifest),
        mapBin: () => Promise.resolve(new Uint8Array(bins[0])),
        map4xBin: () => Promise.resolve(new Uint8Array(bins[1])),
        map16xBin: () => Promise.resolve(new Uint8Array(bins[2])),
      };
    },
  };
  const pushMap = (gm) => {
    const w = gm.width();
    const h = gm.height();
    res.push(w, h, gm.numLandTiles(), w * h);
    for (let i = 0; i < w * h; i++) res.push(gm.terrainByte(i));
  };
  const pushNation = (n) => {
    if (n.coordinates !== undefined) res.push(1, ...n.coordinates);
    else res.push(0);
    res.push(...encS(n.flag ?? ""));
    res.push(...encS(n.name));
  };
  const opsLen = args[p++];
  const res = [opsLen];
  for (let o = 0; o < opsLen; o++) {
    const mapName = str();
    const size = args[p++];
    try {
      const r = await Tml.loadTerrainMap(
        mapName,
        size === 0 ? GAME.GameMapSize.Normal : GAME.GameMapSize.Compact,
        loader,
        false,
      );
      res.push(0);
      pushMap(r.gameMap);
      pushMap(r.miniGameMap);
      res.push(r.nations.length);
      for (const n of r.nations) pushNation(n);
      res.push(manifest.additionalNations !== undefined ? 1 : 0);
      res.push(r.additionalNations.length);
      for (const n of r.additionalNations) pushNation(n);
      if (r.teamGameSpawnAreas === undefined) {
        res.push(0);
      } else {
        const entries = Object.entries(r.teamGameSpawnAreas);
        res.push(1, entries.length);
        for (const [key, areas] of entries) {
          res.push(...encS(key));
          res.push(areas.length);
          for (const a of areas) res.push(a.x, a.y, a.width, a.height);
        }
      }
      if (r.layers === undefined) {
        res.push(0);
      } else {
        res.push(1, r.layers.length);
        for (const l of r.layers) {
          res.push(...encS(l.id));
          res.push(...encS(l.placement));
          if (l.alpha !== undefined) res.push(1, l.alpha);
          else res.push(0);
        }
      }
    } catch (e) {
      const kind = e.message.startsWith("Invalid data:")
        ? 1
        : e.message.includes("invalid placement")
          ? 2
          : e.message.includes("invalid alpha")
            ? 3
            : -1;
      if (kind === -1) throw e;
      res.push(kind, ...encS(e.message));
    }
  }
  res.push(calls);
  return res;
}

const tmlMeta = (w, h, nlt) => [w, h, nlt];
const tmlNation = (has, x, y, flag, name) => [
  has,
  ...(has ? [x, y] : []),
  ...encS(flag),
  ...encS(name),
];
const tmlTgsa = (key, areas) => [
  ...encS(key),
  areas.length,
  ...areas.flat(),
];
const tmlLayer = (id, placement, hasAlpha, alpha) => [
  ...encS(id),
  ...encS(placement),
  hasAlpha,
  ...(hasAlpha ? [alpha] : []),
];
const tmlBin = (kind, bytes) => [kind, kind === 1 ? 0 : bytes.length, ...bytes];
const tmlOp = (name, size) => [...encS(name), size];

function captureTML(name, spec) {
  const args = [
    ...spec.map,
    ...spec.map4x,
    ...spec.map16x,
    spec.nations.length,
    spec.nations,
    spec.addPresent,
    spec.addNations.length,
    spec.addNations,
    spec.tgsaPresent,
    spec.tgsa.length,
    spec.tgsa,
    spec.layersPresent,
    spec.layers.length,
    spec.layers,
    spec.bins,
    spec.ops.length,
    spec.ops,
  ].flat(Infinity);
  return tmlRun(args).then((res) => {
    tmlScenarios.push({
      name: tmlname(name),
      kind: 0,
      args: args.map(uenc),
      res: res.map(uenc),
    });
  });
}

const TML_META = {
  map: tmlMeta(2, 2, 7),
  map4x: tmlMeta(2, 2, 5),
  map16x: tmlMeta(2, 2, 3),
};
const TML_BINS = [tmlBin(0, [1, 2, 3, 4]), tmlBin(0, [5, 6, 7, 8]), tmlBin(0, [9, 10, 11, 12])];
const TML_LAND = [tmlLayer("L", "land", 0)];
const TML_FULL = {
  ...TML_META,
  nations: [tmlNation(1, 5, 6, "f1", "Alpha"), tmlNation(0, 0, 0, "", "Beta")],
  addPresent: 1,
  addNations: [tmlNation(1, 3, 1, "", "Gamma")],
  tgsaPresent: 1,
  tgsa: [tmlTgsa("duo", [[3, 4, 5, 6], [1, 1, 1, 1]])],
  layersPresent: 1,
  layers: TML_LAND,
  bins: TML_BINS,
};

// 1. Normal basic: gameMap = map + mapBin, mini = map4x + map4xBin (dead
//    ternary pinned by the distinct nlt values 7 / 5); nations unscaled.
await captureTML("tml_normal_basic", { ...TML_FULL, ops: [tmlOp("nb1", 0)] });

// 2. Compact basic: gameMap = map4x + map4xBin, mini = map16x + map16xBin;
//    nations / additionalNations scaled in place, spawn areas scaled with the
//    max(1, floor(1/2)) = 1 edge (second area is all-ones).
await captureTML("tml_compact_basic", {
  ...TML_FULL,
  ops: [tmlOp("cb1", 1)],
});

// 3. Cache hit: same (map, size) twice - the second op skips getMapData
//    (calls = 1) and dumps the identical cached object.
await captureTML("tml_cache_hit", {
  ...TML_FULL,
  ops: [tmlOp("ch1", 0), tmlOp("ch1", 0)],
});

// 4. Cache isolation + in-place scaling pollution: k1 / k2 are distinct keys
//    (calls = 2) over the SAME manifest object, so k2's Compact load re-scales
//    the nations k1 already scaled (5,6 -> 2,3 -> 1,1). The result objects
//    alias the shared array, so the third k1 op hits the cache and dumps the
//    DOUBLE-scaled (1,1) nations - not the (2,3) its own load produced.
await captureTML("tml_pollution_isolation", {
  ...TML_FULL,
  ops: [tmlOp("pi1", 1), tmlOp("pi2", 1), tmlOp("pi1", 1)],
});

// 5. genTerrainFromBin throw: mapBin comes back zero-length against the 2x2
//    map; the same key retried re-throws (calls = 2, throw never caches).
await captureTML("tml_bin_throw", {
  ...TML_FULL,
  bins: [tmlBin(1, []), ...TML_BINS.slice(1)],
  ops: [tmlOp("bt1", 0), tmlOp("bt1", 0)],
});

// 6. Mini-map throw: the game map builds, then map4xBin (zero-length) throws
//    for the mini map - the whole result is discarded and uncached.
await captureTML("tml_mini_throw", {
  ...TML_FULL,
  bins: [TML_BINS[0], tmlBin(1, []), TML_BINS[2]],
  ops: [tmlOp("mt1", 0)],
});

// 7. Placement throw: layer.placement = "sky".
await captureTML("tml_placement_throw", {
  ...TML_FULL,
  layers: [tmlLayer("L", "sky", 0)],
  ops: [tmlOp("pt1", 0)],
});

// 8. Alpha throw NaN: the message interpolates JS Number->string "NaN".
await captureTML("tml_alpha_nan", {
  ...TML_FULL,
  layers: [tmlLayer("L", "land", 1, NaN)],
  ops: [tmlOp("an1", 0)],
});

// 9. Alpha throw -0.5: message slot "-0.5".
await captureTML("tml_alpha_neg", {
  ...TML_FULL,
  layers: [tmlLayer("L", "land", 1, -0.5)],
  ops: [tmlOp("ag1", 0)],
});

// 10. Alpha throw 1.5: message slot "1.5".
await captureTML("tml_alpha_over", {
  ...TML_FULL,
  layers: [tmlLayer("L", "land", 1, 1.5)],
  ops: [tmlOp("ao1", 0)],
});

// 11. Alpha -0 boundary: Number.isFinite(-0) is true and -0 < 0 is false, so
//     the layer passes and the dump round-trips the -0 (Object.is-pinned).
await captureTML("tml_alpha_negzero", {
  ...TML_FULL,
  layers: [tmlLayer("L", "land", 1, -0)],
  ops: [tmlOp("az1", 0)],
});

// 12. additionalNations absent: ?? [] yields a fresh empty array (present 0).
await captureTML("tml_addnations_absent", {
  ...TML_FULL,
  addPresent: 0,
  addNations: [],
  ops: [tmlOp("aa1", 0)],
});

// 13. teamGameSpawnAreas absent: result field undefined (present 0).
await captureTML("tml_tgsa_absent", {
  ...TML_FULL,
  tgsaPresent: 0,
  tgsa: [],
  ops: [tmlOp("ta1", 0)] },
);

// 14. layers absent: result field undefined (present 0), no validation runs.
await captureTML("tml_layers_absent", {
  ...TML_FULL,
  layersPresent: 0,
  layers: [],
  ops: [tmlOp("la1", 0)],
});


// --- NationUtils scenario runner ----------------------------------------------
// Exercises the real execution/nation/NationUtils.ts orchestration against
// scripted PseudoRandom / Game / Player mocks: every facade call consumes the
// next script entry and records its arguments + return into a flat trace, so
// the loop order, the short-circuits (`&&`, the ternary, the `??=` which the
// public API never triggers), the strict `===` owner identity (modeled as a
// player id), the Array.from(tiles()) iteration order and the reduce filter
// are all pinned in the res stream. The facade *internals* (nextInt math,
// randElement's throw-on-empty, GameMap geometry beyond x/y) are mock domain
// and out of the ported surface; randElement's script value is always one of
// the tiles, so the `undefined !== null` push quirk (numTilesOwned > 0 with an
// empty tiles()) stays outside the capture domain. Structures.has / the
// DefensePost / MissileSilo exclusions run on the REAL prepared Game.ts
// values (string enum: "Defense Post" / "Missile Silo"). The Rust twin is
// `nation_utils::run_op`; the bounding box is computed by the real
// calculateBoundingBox over the mock's GameMap-shaped x/y (tile % width,
// (tile / width) | 0), which the port replays through
// `crate::util::calculate_bounding_box`.
//
// args (flat f64, strings [len, u0, ..]):
//   kind 0 randTerritoryTileArray:
//     [0, width, height, numTiles,
//      nBorder, (tile)*, nNext, (ret)*, nOn, (0|1)*, nRef, (ret)*, nOwn,
//      (playerId)*, randElemRet, numTilesOwned, nTiles, (tile)*, playerId]
//   kind 1 findJuiciestTarget:
//     [1, nCand, (id, troops, numTilesOwned, maxTroops, nUnits,
//        (typeStr, level)*nUnits)*nCand]
//
// res: [traceLen, (trace)*, payload]
//   trace events: 0 nextInt [0,min,max,ret] | 1 randElement [1,len,(tile)*,ret]
//     2 isOnMap [2,x,y,0|1] | 3 ref [3,x,y,ret] | 4 owner [4,tile,id]
//     5 borderTiles [5,len,(tile)*] | 6 numTilesOwned [6,(id,)ret]
//     7 tiles [7,len,(tile)*] | 8 config [8] | 9 maxTroops [9,id,ret]
//     10 units [10,id,len] | 11 troops [11,id,ret]
//     12 unit [12,id,(str)type,level,0|1 counted]
//   kind 0 payload: [outLen, (tile)*]
//   kind 1 payload: [nCand, (juiciness)*nCand, present, winnerId?]
const nuScenarios = [];
const numame = (s) => s.replace(/[^A-Za-z0-9]+/g, "_");

function nuRun(args) {
  let p = 0;
  const num = () => args[p++];
  const str = () => {
    const len = args[p++];
    let s = "";
    for (let i = 0; i < len; i++) s += String.fromCharCode(args[p++]);
    return s;
  };
  const kind = args[p++];
  const trace = [];
  const ev = (...t) => trace.push(...t);
  const tail = [];
  if (kind === 0) {
    const width = num();
    const height = num();
    const numTiles = num();
    const borderLen = num();
    const border = args.slice(p, p + borderLen);
    p += borderLen;
    const nextLen = num();
    const nextScript = args.slice(p, p + nextLen);
    p += nextLen;
    const onLen = num();
    const onScript = args.slice(p, p + onLen);
    p += onLen;
    const refLen = num();
    const refScript = args.slice(p, p + refLen);
    p += refLen;
    const ownLen = num();
    const ownScript = args.slice(p, p + ownLen);
    p += ownLen;
    const randRet = num();
    const nOwned = num();
    const tilesLen = num();
    const tilesArr = args.slice(p, p + tilesLen);
    p += tilesLen;
    const playerId = num();
    let ni = 0;
    let oi = 0;
    let ri = 0;
    let wi = 0;
    const player = {
      borderTiles() {
        ev(5, border.length, ...border);
        return border;
      },
      numTilesOwned() {
        ev(6, nOwned);
        return nOwned;
      },
      tiles() {
        ev(7, tilesArr.length, ...tilesArr);
        return tilesArr;
      },
    };
    const other = {};
    const random = {
      nextInt(min, max) {
        const ret = nextScript[ni++];
        ev(0, min, max, ret);
        return ret;
      },
      randElement(arr) {
        ev(1, arr.length, ...arr, randRet);
        return randRet;
      },
    };
    const mg = {
      x: (t) => t % width,
      y: (t) => (t / width) | 0,
      isOnMap(cell) {
        const ret = onScript[oi++];
        ev(2, cell.x, cell.y, ret);
        return ret === 1;
      },
      ref(x, y) {
        const ret = refScript[ri++];
        ev(3, x, y, ret);
        return ret;
      },
      owner(tile) {
        const id = ownScript[wi++];
        ev(4, tile, id);
        return id === playerId ? player : other;
      },
    };
    const out = Nu.randTerritoryTileArray(random, mg, player, numTiles);
    tail.push(out.length, ...out);
  } else {
    const nc = num();
    const cands = [];
    for (let i = 0; i < nc; i++) {
      const id = num();
      const troops = num();
      const nOwned = num();
      const maxTroops = num();
      const nu = num();
      const units = [];
      for (let j = 0; j < nu; j++) {
        const type = str();
        const level = num();
        units.push({ type: () => type, level: () => level });
      }
      cands.push({ id, troops, nOwned, maxTroops, units });
    }
    const game = {
      config() {
        ev(8);
        return {
          maxTroops(pl) {
            ev(9, pl.__c.id, pl.__c.maxTroops);
            return pl.__c.maxTroops;
          },
        };
      },
    };
    const objs = cands.map((c) => {
      const o = {
        __c: c,
        units() {
          ev(10, c.id, c.units.length);
          for (const u of c.units) {
            const t = u.type();
            const counted =
              GAME.Structures.has(t) &&
              t !== GAME.UnitType.DefensePost &&
              t !== GAME.UnitType.MissileSilo
                ? 1
                : 0;
            ev(12, c.id, ...encS(t), u.level(), counted);
          }
          return c.units;
        },
        troops() {
          ev(11, c.id, c.troops);
          return c.troops;
        },
        numTilesOwned() {
          ev(6, c.id, c.nOwned);
          return c.nOwned;
        },
      };
      return o;
    });
    const winner = Nu.findJuiciestTarget(game, objs);
    // Recompute the per-candidate juiciness for the payload (the same
    // normalize math the ported Rust runs; the capture's ground truth for the
    // trace is the real function's return, the payload pins the intermediate).
    const scs = cands.map((c) =>
      c.units.reduce(
        (sum, u) =>
          GAME.Structures.has(u.type()) &&
          u.type() !== GAME.UnitType.DefensePost &&
          u.type() !== GAME.UnitType.MissileSilo
            ? sum + u.level()
            : sum,
        0,
      ),
    );
    const grs = cands.map((c) =>
      c.maxTroops > 0 ? 1 - c.troops / c.maxTroops : 0,
    );
    const tls = cands.map((c) => c.nOwned);
    const nrm = (value, values) => {
      const mn = Math.min(...values);
      const mx = Math.max(...values);
      return mx > mn ? (value - mn) / (mx - mn) : 0;
    };
    tail.push(nc);
    for (let i = 0; i < nc; i++) {
      tail.push(
        nrm(scs[i], scs) + nrm(grs[i], grs) + nrm(tls[i], tls),
      );
    }
    if (winner === null) tail.push(0);
    else tail.push(1, winner.__c.id);
  }
  return [trace.length, ...trace, ...tail];
}

const nuArgs0 = (
  width,
  height,
  numTiles,
  border,
  nextInt,
  onMap,
  refs,
  owners,
  randRet,
  nOwned,
  tiles,
  playerId,
) => [
  0,
  width,
  height,
  numTiles,
  border.length,
  ...border,
  nextInt.length,
  ...nextInt,
  onMap.length,
  ...onMap,
  refs.length,
  ...refs,
  owners.length,
  ...owners,
  randRet,
  nOwned,
  tiles.length,
  ...tiles,
  playerId,
];
const nuCand = (id, troops, nOwned, maxTroops, units) => [
  id,
  troops,
  nOwned,
  maxTroops,
  units.length,
  ...units.flatMap(([t, l]) => [...encS(t), l]),
];
const nuArgs1 = (cands) => [1, cands.length, ...cands.flat(Infinity)];

function captureNU(name, args) {
  nuScenarios.push({
    name: numame(name),
    kind: args[0],
    args: args.flat(Infinity).map(uenc),
    res: nuRun(args.flat(Infinity)).map(uenc),
  });
}

// 1. First sample hits: bb from border tiles (12 -> (2,1), 34 -> (4,3)),
//    nextInt(2,4)=5 / nextInt(1,3)=6, on-map, ref 56, owner === p -> [56].
captureNU("nu_array_hit_first_try", nuArgs0(10, 10, 1, [12, 34], [5, 6], [1], [56], [7], 0, 0, [], 7));

// 2. 100 off-map misses then the numTilesOwned 1..100 fallback: randElement
//    over Array.from(tiles()) returns the scripted tile 20.
captureNU("nu_array_miss_then_fallback", nuArgs0(10, 10, 1, [12], Array(200).fill(5), Array(100).fill(0), [], [], 20, 3, [10, 20, 30], 7));

// 3. Off-map continue: iteration 1 isOnMap false (no ref/owner), iteration 2
//    hits owner -> tile 44.
captureNU("nu_array_offmap_continues", nuArgs0(10, 10, 1, [12, 34], [5, 6, 2, 3], [0, 1], [44], [7], 0, 0, [], 7));

// 4. >100 tiles -> null (not pushed): 100 off-map misses, numTilesOwned 150
//    called twice (the second `<= 100` test fails, tiles() never runs).
captureNU("nu_array_null", nuArgs0(10, 10, 1, [12], Array(200).fill(5), Array(100).fill(0), [], [], 0, 150, [], 7));

// 5. numTiles=2 mixed: call 1 hits 56, call 2 runs 100 misses then falls back
//    to randElement 44.
captureNU("nu_array_multi_tiles", nuArgs0(10, 10, 2, [12, 34], [5, 6, ...Array(200).fill(5)], [1, ...Array(100).fill(0)], [56], [7], 44, 2, [33, 44], 7));

// 6. Empty borderTiles: calculateBoundingBox returns min=(Inf,Inf)
//    max=(-Inf,-Inf) (never null, so the ??= default path stays dead through
//    the public API); the Infinity bounds ride into the nextInt trace.
captureNU("nu_bb_empty_border", nuArgs0(10, 10, 1, [], [5, 6], [1], [56], [7], 0, 0, [], 7));

// 7. numTiles=0: the loop never runs, only borderTiles is consumed.
captureNU("nu_array_zero_tiles", nuArgs0(10, 10, 0, [12], [], [], [], [], 0, 0, [], 7));

// 8. numTilesOwned 0: the `> 0` short-circuit skips the second call and the
//    tiles()/randElement fallback entirely -> null.
captureNU("nu_array_owned0_null", nuArgs0(10, 10, 1, [12], Array(200).fill(5), Array(100).fill(0), [], [], 0, 0, [], 7));

// 9. Empty candidates: length 0 -> null, no facade calls.
captureNU("nu_juice_empty", nuArgs1([]));

// 10. Single candidate: every normalize sees a one-value array (max > min
//     false -> 0), juiciness 0 beats -Infinity -> the candidate wins.
captureNU("nu_juice_single", nuArgs1([nuCand(1, 5, 10, 20, [["City", 3], ["Defense Post", 2]])]));

// 11. Tie: identical stats -> juiciness 0 vs 0, the strict `>` keeps the
//     first candidate.
captureNU("nu_juice_tie", nuArgs1([
  nuCand(1, 5, 10, 20, [["City", 3]]),
  nuCand(2, 5, 10, 20, [["City", 3]]),
]));

// 12. Structure filter: City/Factory/SAM Launcher/Port counted by level,
//     Defense Post / Missile Silo excluded, non-structures (Warship/Shell)
//     excluded; candidate A sweeps all three normalizations.
captureNU("nu_juice_structures", nuArgs1([
  nuCand(1, 0, 100, 10, [
    ["City", 2], ["Factory", 1], ["SAM Launcher", 3], ["Port", 1],
    ["Missile Silo", 2], ["Defense Post", 5], ["Warship", 9], ["Shell", 4],
  ]),
  nuCand(2, 10, 1, 10, []),
]));

// 13. maxTroops 0: the ternary false branch -> ratio 0 and troops() is NEVER
//     called (no ev 11 for candidate 1); candidate 2's normal ratio wins.
captureNU("nu_juice_troopgap", nuArgs1([
  nuCand(1, 9, 5, 0, [["City", 1]]),
  nuCand(2, 1, 5, 4, [["City", 1]]),
]));

// 14. Winner in the middle of three candidates.
captureNU("nu_juice_winner_middle", nuArgs1([
  nuCand(1, 10, 0, 10, []),
  nuCand(2, 0, 10, 10, [["City", 5]]),
  nuCand(3, 5, 4, 10, [["City", 2]]),
]));

// 15. NaN troops with maxTroops > 0: the ratio is NaN, Math.min/max over the
//     ratios go NaN, `max > min` is false -> normalize 0 (NaN never wins);
//     the tie keeps the first candidate.
captureNU("nu_juice_nan_troops", nuArgs1([
  nuCand(1, NaN, 5, 10, []),
  nuCand(2, 0, 5, 10, []),
]));

// 16. Negative gap ratio (troops > maxTroops -> 1 - 2 = -1): normalize spans
//     the [-1, 1] range.
captureNU("nu_juice_neg_ratio", nuArgs1([
  nuCand(1, 20, 3, 10, [["City", 0]]),
  nuCand(2, 0, 7, 10, []),
]));

// 17. -0 level: `0 + -0` folds to +0 in the reduce sum, and the -0 rides
//     through the ev-12 level slot (Object.is-pinned).
captureNU("nu_juice_negzero", nuArgs1([
  nuCand(1, 0, 2, 5, [["City", -0]]),
  nuCand(2, 0, 2, 5, [["Port", 0]]),
]));


// --- GameImpl.ts createGameUpdatesMap scenario runner -------------------------
// Exercises the module-tail `createGameUpdatesMap` of game/GameImpl.ts against
// the prepared (reverse-mapped) GameUpdateType object. kind 2 calls the real
// function and dumps `Object.keys` order/count plus, per entry, Array.isArray
// and length. kind 3 pins the intermediate `Object.values` + `filter(!isNaN
// (Number(key)))` the function runs: the full value table in enumeration order
// (integer-index keys sort first, so the reverse-mapping strings come before
// the forward numeric values) with a 0=number / 1=string tag, then the kept
// numbers. The Rust twin is `game_updates::run_op` kinds 2 / 3.
const giScenarios = [];
const giname = (s) => s.replace(/[^A-Za-z0-9]+/g, "_");
function captureGI(name, kind, args, res) {
  giScenarios.push({
    name: giname(name),
    kind,
    args: args.flat(Infinity).map(uenc),
    res: res.flat(Infinity).map(uenc),
  });
}

// kind 3 first: the raw Object.values + filter over the enum object (this is
// what pins the reverse mapping's presence and the filter semantics).
{
  const vals = Object.values(GUT);
  const filtered = vals.filter((key) => !isNaN(Number(key)));
  captureGI("gi_values", 3, [3], [
    vals.length,
    ...vals.map((v) => (typeof v === "number" ? [0, v] : [1, ...encS(String(v))])),
    filtered.length,
    ...filtered,
  ]);
}

// kind 2: the whole createGameUpdatesMap() result object.
{
  const map = GI.createGameUpdatesMap();
  const keys = Object.keys(map);
  captureGI("gi_map", 2, [2], [
    keys.length,
    ...keys.flatMap((k) => [
      ...encS(k),
      Array.isArray(map[k]) ? 1 : 0,
      map[k].length,
    ]),
  ]);
}

// --- TerraNulliusImpl.ts scenario runner --------------------------------------
// Exercises the four constant-return methods of game/TerraNulliusImpl.ts on a
// real instance. kind 0 smallID -> [0]; kind 1 clientID -> [encS]; kind 2 id
// -> [-1] for JS null (the server_list encOut precedent); kind 3 isPlayer ->
// [0|1]. The Rust twin is `terra_nullius::run_op`.
const tniScenarios = [];
const tniname = (s) => s.replace(/[^A-Za-z0-9]+/g, "_");
function captureTNI(name, kind, args, res) {
  tniScenarios.push({
    name: tniname(name),
    kind,
    args: args.flat(Infinity).map(uenc),
    res: res.flat(Infinity).map(uenc),
  });
}
{
  const tn = new TNI.TerraNulliusImpl();
  captureTNI("tn_small_id", 0, [0], [tn.smallID()]);
  captureTNI("tn_client_id", 1, [1], [encS(tn.clientID())]);
  captureTNI("tn_id", 2, [2], [tn.id() === null ? -1 : 0]);
  captureTNI("tn_is_player", 3, [3], [tn.isPlayer() ? 1 : 0]);
}

// --- WaterPathMemo scenario runner -------------------------------------------
// Exercises the real PathFinder.ts WaterPathMemo against a scripted inner
// PathFinder mock: every inner.findPath call consumes the next script entry
// (an array or null) and records its arguments into a trace, so cache hits
// (inner untouched) and array-from passthrough are observable in the res
// stream. `currentWaterVersion` is a mutable closure variable op kind 2 can
// bump. The Rust twin is `water_path_memo::run_op`.
//
// args layout (all flat f64, no strings):
//   [numTiles, maxBytes, wv0, scriptLen,
//     (retKind, len, (elem)*len)*scriptLen,        retKind 0=null (len 0), 1=array
//     opsLen, (op)*)*opsLen]
//   op kinds: 0 findPath(number from, to) -> [0, from, to]
//             1 findPath(array from, to)  -> [1, fromLen, to, (elem)*fromLen]
//             2 setWaterVersion(v)        -> [2, v]
//             3 read entryCount           -> [3]
//             4 read byteCount            -> [4]
//
// res per op:
//   findPath: [retKind, len, (elem)*len, entryCount, byteCount,
//              nInner, (innerCall)*)       innerCall: number from [0,from,to],
//                                          array from [1,len,to,(elem)*len]
//   setWaterVersion: nothing
//   entryCount / byteCount read: [value]
const wpmScenarios = [];
const wpmname = (s) => s.replace(/[^A-Za-z0-9]+/g, "_");

function wpmRun(args) {
  let p = 0;
  const numTiles = args[p++];
  const maxBytes = args[p++];
  let wv = args[p++];
  const scriptLen = args[p++];
  const script = [];
  for (let i = 0; i < scriptLen; i++) {
    const retKind = args[p++];
    const len = args[p++];
    const elems = args.slice(p, p + len);
    p += len;
    script.push(retKind === 0 ? null : elems);
  }
  const opsLen = args[p++];
  let scriptIdx = 0;
  const inner = {
    calls: [],
    findPath(from, to) {
      if (typeof from === "number") inner.calls.push([0, from, to]);
      else inner.calls.push([1, from.length, to, ...from]);
      if (scriptIdx >= script.length) throw new Error("wpm: inner script exhausted");
      return script[scriptIdx++];
    },
  };
  const memo = new PF.WaterPathMemo(inner, numTiles, () => wv, maxBytes);
  const res = [];
  const pushFind = (path) => {
    if (path === null) res.push(0, 0);
    else res.push(1, path.length, ...path);
    res.push(memo.entryCount, memo.byteCount);
    const calls = inner.calls.splice(0);
    res.push(calls.length, ...calls.flat());
  };
  for (let oi = 0; oi < opsLen; oi++) {
    const opKind = args[p++];
    if (opKind === 0) {
      const from = args[p++];
      const to = args[p++];
      pushFind(memo.findPath(from, to));
    } else if (opKind === 1) {
      const fromLen = args[p++];
      const to = args[p++];
      const from = args.slice(p, p + fromLen);
      p += fromLen;
      pushFind(memo.findPath(from, to));
    } else if (opKind === 2) {
      wv = args[p++];
    } else if (opKind === 3) {
      res.push(memo.entryCount);
    } else if (opKind === 4) {
      res.push(memo.byteCount);
    } else {
      throw new Error(`wpm: unknown op kind ${opKind}`);
    }
  }
  return res;
}

const wpmArgs = (numTiles, maxBytes, wv0, script, ops) => [
  numTiles,
  maxBytes,
  wv0,
  script.length,
  ...script.flatMap((s) => (s === null ? [0, 0] : [1, s.length, ...s])),
  ops.length,
  ...ops.flat(),
];

function captureWPM(name, args) {
  wpmScenarios.push({
    name: wpmname(name),
    kind: 0,
    args: args.flat(Infinity).map(uenc),
    res: wpmRun(args.flat(Infinity)).map(uenc),
  });
}

// 1. pure miss: two distinct pairs, inner called once per query.
captureWPM("wpm_miss", wpmArgs(100, 100000, 1, [[1, 2, 3], [7]], [
  [0, 1, 2],
  [0, 4, 5],
]));

// 2. hit: the second identical query never reaches inner, same values back.
captureWPM("wpm_hit", wpmArgs(100, 100000, 1, [[1, 2, 3]], [
  [0, 1, 2],
  [0, 1, 2],
]));

// 3. null accounting: a null miss costs 16 bytes; the cached null hit returns
//    null without touching inner and the byte count stays 16.
captureWPM("wpm_null", wpmArgs(100, 100000, 1, [null], [
  [0, 1, 2],
  [4],
  [0, 1, 2],
  [3],
  [4],
]));

// 4. LRU re-insertion: A/B/C fill the 100-byte budget exactly, querying A
//    again moves it to the tail, and D then evicts B (not A) - proven by B
//    missing afterwards while A still hits.
captureWPM("wpm_lru", wpmArgs(100, 100, 1, [
  [1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
  [2, 2, 2, 2, 2, 2, 2, 2, 2, 2],
  [3, 3, 3, 3, 3],
  [4, 4, 4, 4, 4, 4, 4, 4, 4, 4],
  [5, 5, 5, 5, 5, 5, 5, 5, 5, 5],
], [
  [0, 1, 2],
  [0, 3, 4],
  [0, 5, 6],
  [0, 1, 2],
  [0, 7, 8],
  [0, 1, 2],
  [0, 3, 4],
]));

// 5. waterVersion change: the entry check clears everything (even on an
//    array-from passthrough), and the next numeric query misses again.
captureWPM("wpm_version", wpmArgs(100, 100000, 1, [[1, 2, 3], [1, 2, 3], [9]], [
  [0, 1, 2],
  [2, 2],
  [1, 2, 3, 1, 2],
  [3],
  [4],
  [0, 1, 2],
]));

// 6. array-from passthrough: inner receives the array arguments, nothing is
//    cached (entryCount unchanged), and the raw path comes back.
captureWPM("wpm_array_from", wpmArgs(100, 100000, 1, [[4, 5], null], [
  [1, 2, 7, 10, 20],
  [3],
  [4],
  [1, 1, 7, 30],
  [3],
]));

// 7. over-budget single entry: one 40-byte path under a 10-byte budget is
//    evicted immediately (the eviction loop takes the just-inserted entry),
//    so the same query misses again.
captureWPM("wpm_over_budget", wpmArgs(100, 10, 1, [
  [1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
  [1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
], [
  [0, 1, 2],
  [3],
  [4],
  [0, 1, 2],
]));

// 8. numeric-key collision: numTiles=10 makes (1,2) and (0,12) share key 12,
//    so the second query hits the first one's entry without inner seeing it.
captureWPM("wpm_key_collision", wpmArgs(10, 100000, 1, [[1, 2, 3]], [
  [0, 1, 2],
  [0, 0, 12],
  [0, 1, 2],
]));

// 9. Uint32Array coercion on hit: the miss returns the raw script values, the
//    hit returns the stored-to-Uint32 copies (-1 -> 4294967295, 2^32+1 -> 1);
//    the byte charge is still len*4 either way.
captureWPM("wpm_uint32", wpmArgs(100, 100000, 1, [[-1, 4294967297, 2.9]], [
  [0, 1, 2],
  [4],
  [0, 1, 2],
]));



// --- PatternDecoder scenario runner -------------------------------------------
// Exercises the real PatternDecoder.ts decode + isPrimary through the shared
// run_op runner. The base64url layer is not ported: bytes cross as
// `[len, b0, ..]` and the capture encodes them to base64url so the actual TS
// decodePatternData runs. kind 0 = construct (res `[0,h,w,scale,sh,sw]` or
// `[code]`), kind 1 = isPrimary(bytes,x,y) (res `[0]`/`[1]`, or `[code]` for a
// decode throw / `[4]` for the bounds throw). Codes: 1 too short, 2 bad
// version, 3 too short for dimensions, 4 invalid pattern.
const pdScenarios = [];
function capturePD(name, kind, args, res) {
  pdScenarios.push({ name, kind, args: args.flat().map(uenc), res: res.flat().map(uenc) });
}
const pdHeader = (scale, width, height) => {
  const w = width - 2;
  const h = height - 2;
  return [0, (scale & 0x07) | ((w & 0x1f) << 3), ((h & 0x3f) << 2) | ((w >> 5) & 0x03)];
};
const pdBytesTok = (bytes) => [bytes.length, ...bytes];
const pdDecode = (bytes) => {
  const b64 = Buffer.from(bytes).toString("base64url");
  return PD.decodePatternData(b64, (s) => new Uint8Array(Buffer.from(s, "base64url")));
};
// Map the real TS throw to the port's numeric code (message prefix match).
const pdThrowCode = (e) => {
  const m = String(e && e.message);
  if (m.startsWith("Pattern data is too short to contain")) return 1;
  if (m.startsWith("Unrecognized pattern version")) return 2;
  if (m.startsWith("Pattern data is too short for the")) return 3;
  if (m === "Invalid pattern") return 4;
  throw e;
};
const pdRes0 = (bytes) => {
  let d;
  try {
    d = pdDecode(bytes);
  } catch (e) {
    return [pdThrowCode(e)];
  }
  return [0, d.height, d.width, d.scale, d.height << d.scale, d.width << d.scale];
};
const pdRes1 = (bytes, x, y) => {
  let dec;
  try {
    const b64 = Buffer.from(bytes).toString("base64url");
    dec = new PD.PatternDecoder(
      { patternData: b64 },
      (s) => new Uint8Array(Buffer.from(s, "base64url")),
    );
  } catch (e) {
    return [pdThrowCode(e)];
  }
  try {
    return [dec.isPrimary(x, y) ? 1 : 0];
  } catch (e) {
    return [pdThrowCode(e)];
  }
};
// kind 0: decode guards, header round-trips, scaled dims.
{
  let i = 0;
  const k0cases = [
    [], [0], [0, 1], [1, 0, 0], [2, 255, 255],
    pdHeader(0, 2, 2),
    [...pdHeader(0, 2, 2), 0],
    [...pdHeader(3, 5, 4), 0, 0, 0],
    [...pdHeader(7, 129, 65), ...new Array(1049).fill(0)],
    [...pdHeader(0, 129, 65), ...new Array(1049).fill(0)],
    [...pdHeader(1, 3, 3), 0, 0],
    [...pdHeader(7, 2, 2), 0],
    [...pdHeader(0, 2, 2), 0, 0, 0],
    [...pdHeader(2, 5, 4), 0, 0],
    [...pdHeader(0, 33, 33), ...new Array(137).fill(0)],
    [...pdHeader(0, 34, 2), 0, 0, 0, 0, 0, 0, 0, 0, 0],
  ];
  for (const s of [0, 1, 2, 3, 4, 5, 6, 7])
    k0cases.push([...pdHeader(s, 2, 2), 0]);
  for (const bytes of k0cases) capturePD(`pd_dec_${i++}`, 0, [pdBytesTok(bytes)], pdRes0(bytes));
}
// kind 1: bit lookup, negative-index metadata reads, bounds throws, scale.
{
  let i = 0;
  const D2 = [...pdHeader(0, 2, 2), 0b0000_0010];
  const D34 = [...pdHeader(0, 2, 34), 0, 0, 0, 0, 0, 0, 0, 0, 0]; // byte2 = 128
  const D33 = [...pdHeader(0, 33, 33), ...new Array(137).fill(0)];
  const D3 = [...pdHeader(3, 4, 4), 0b0000_0010];
  const k1cases = [
    [D2, 0, 0], [D2, 1, 0], [D2, 0, 1], [D2, 1, 1],
    [D2, -1, -1], [D2, -1, 0], [D2, 0, -1],
    [D34, -1, 0], [D34, -2, 0],
    [D33, -32, -32], [D33, 1e9, 1e9], [D33, 1e15, -1e15],
    [D33, 16, -1], [D33, -17, 0],
    [D2, 1.7, 0.2], [D2, -1.9, 0.5], [D2, NaN, 0], [D2, 0, NaN],
    [D2, Infinity, 0], [D2, -Infinity, 1],
    [D3, 8, 0], [D3, 0, 0], [D3, 15, 15], [D3, 31, 31],
    [D3, -8, 0], [D2, -2147483648, 0], [D2, 2147483648, 0],
    [D33, 1088, 0], [D33, 0, 1088], [D33, 1087, 0],
  ];
  for (const [bytes, x, y] of k1cases)
    capturePD(`pd_ip_${i++}`, 1, [...pdBytesTok(bytes), x, y], pdRes1(bytes, x, y));
}

// --- DoomsdayClock scenario runner --------------------------------------------
// Exercises the real DoomsdayClock.ts wave math. kind 0 requiredTiles
// [speed,team,land,elapsed] -> [tiles]; kind 1 waveState [speed,team,elapsed]
// -> [currentPct, targetPct, growing, secToNext, secToTarget, flash, done];
// kind 2 rotSpeckleNoise [x,y,salt]; kind 3 rotFrontNoise [tile,salt];
// kind 4 troopFloor [max,spw,drainFloor,start,decay]; kind 5 drain
// [max,spw,start,max,ramp,exponent]; kind 6 rotQuota [left,under,death].
// Speed codes: 0 slow / 1 normal / 2 fast / 3 veryfast / 4 unknown (the ??
// fallback); team code 1 = teamGame === true.
const dcScenarios = [];
function captureDC(name, kind, args, res) {
  dcScenarios.push({ name, kind, args: args.flat().map(uenc), res: res.flat().map(uenc) });
}
const DC_SPEEDS = ["slow", "normal", "fast", "veryfast", "weird"];
const dcProfile = (sc, team) => ({
  speed: DC_SPEEDS[sc],
  ...(team === 1 ? { teamGame: true } : team === 2 ? { teamGame: undefined } : {}),
});
// kind 0: required tiles across the wave boundaries of every speed.
{
  let i = 0;
  const cases = [];
  for (let sc = 0; sc < 5; sc++)
    for (const team of [0, 1, 2])
      for (const el of [
        -1e9, -1, 0, 0.5, 595, 596, 600, 600.5, 601, 618, 636, 640, 644, 768,
        822, 858, 864, 896, 900, 1176, 1246, 1316, 1386, 1456, 1526, 1596,
        1666, 1736, 2100, 2310, 2520, 2730, 2940, 3150, 3360, 3570, 4500,
        1e9, NaN, -Infinity, Infinity, 600.9999999,
      ])
        cases.push([sc, team, el]);
  for (const [sc, team, el] of cases)
    captureDC(`dc_rt_${i++}`, 0, [sc, team, 10000, el],
      [DC.doomsdayClockRequiredTiles(dcProfile(sc, team), 10000, el)]);
  for (const land of [0, -5, 1, 0.5, NaN, Infinity, -Infinity, 1e12])
    captureDC(`dc_land_${i++}`, 0, [1, 0, land, 1e9],
      [DC.doomsdayClockRequiredTiles(dcProfile(1, 0), land, 1e9)]);
}
// kind 1: wave state — every segment boundary of normal + veryfast, both ladders.
{
  let i = 0;
  for (const [sc, team] of [[1, 0], [1, 1], [3, 0], [0, 1], [2, 1], [4, 0]])
    for (const el of [
      NaN, -Infinity, 0, 594, 595, 596, 600, 600.5, 601, 605, 606, 618, 635,
      636, 640, 644, 645, 767, 768, 772, 822, 858, 863, 864, 868, 895, 896,
      900, 1067, 1068, 1175, 1176, 1245, 1246, 1314, 1315, 1316, 1385, 1386,
      1455, 1456, 1525, 1526, 1595, 1596, 1665, 1666, 1735, 1736, 2309, 2310,
      2939, 2940, 3569, 3570, 4499, 4500, 5000, 1e9, Infinity,
    ])
      captureDC(`dc_ws_${i++}`, 1, [sc, team, el], (() => {
        const w = DC.doomsdayClockWaveState(dcProfile(sc, team), el);
        return [w.currentPercent, w.targetPercent, w.growing ? 1 : 0,
          w.secondsToNextGrowth, w.secondsToTarget, w.waveFlash ? 1 : 0, w.done ? 1 : 0];
      })());
}
// kind 2/3: the two rot noises over the int32 coercion edges.
{
  let i = 0;
  const vals = [0, 1, -1, 2, 3, 0.5, -0.5, NaN, Infinity, -Infinity, 2 ** 31,
    -(2 ** 31), 2 ** 32, 2 ** 53, 12345.678, -98765.4321, 65535, 65536, 4294967295];
  for (const x of vals)
    for (const y of [0, 1, -1, 60, 2 ** 31, NaN])
      captureDC(`dc_spk_${i++}`, 2, [x, y, 7],
        [DC.rotSpeckleNoise(x, y, 7)]);
  for (const t of vals)
    for (const s of [0, 1, -1, 2 ** 32, NaN])
      captureDC(`dc_frt_${i++}`, 3, [t, s], [DC.rotFrontNoise(t, s)]);
}
// kind 4: troop floor.
{
  let i = 0;
  for (const [max, spw, end, start, decay] of [
    [1000, 0, 10, 50, 100], [1000, 50, 10, 50, 100], [1000, 99, 10, 50, 100],
    [1000, 100, 10, 50, 100], [1000, 101, 10, 50, 100], [1000, -5, 10, 50, 100],
    [1000, NaN, 10, 50, 100], [1000, Infinity, 10, 50, 100],
    [1000, 50.7, 10, 50, 100], [999, 33, 10, 50, 100], [1, 50, 10, 50, 100],
    [0, 50, 10, 50, 100], [1000, 50, 50, 50, 100], [1000, 50, 60, 50, 100],
    [1000, 50, 10, 50, 0], [1000, 50, 10, 50, -10], [1000, 50, 10, 50, NaN],
    [1e9, 1, 10, 90, 1e9], [1000, 1e9, 10, 50, 100],
  ])
    captureDC(`dc_tf_${i++}`, 4, [max, spw, end, start, decay],
      [DC.doomsdayClockTroopFloor(max, spw, {
        drainFloorPercent: end, floorStartPercent: start, floorDecaySeconds: decay,
      })]);
}
// kind 5: drain, linear vs convex vs fractional/NaN exponents.
{
  let i = 0;
  for (const [max, spw, start, mx, ramp, exp] of [
    [1000, 0, 1, 11, 100, 1], [1000, 1, 1, 11, 100, 1], [1000, 50, 1, 11, 100, 1],
    [1000, 99, 1, 11, 100, 1], [1000, 100, 1, 11, 100, 1], [1000, 1e9, 1, 11, 100, 1],
    [1000, -5, 1, 11, 100, 1], [1000, NaN, 1, 11, 100, 1],
    [1000, 50, 1, 11, 100, 2], [1000, 50, 1, 11, 100, 3], [1000, 50, 1, 11, 100, 10],
    [1000, 50, 1, 11, 100, 2.5], [1000, 50, 1, 11, 100, 0.5], [1000, 50, 1, 11, 100, 0],
    [1000, 50, 1, 11, 100, -1], [1000, 50, 1, 11, 100, NaN],
    [1000, 99, 1, 11, 100, 3], [1000, 1, 1, 11, 100, 3],
    [1000, 50, 1, 11, 0, 1], [1000, 50, 1, 11, -10, 1], [1000, 50, 1, 11, NaN, 1],
    [0, 1e9, 1, 11, 100, 1], [1000, 1e9, 11, 11, 100, 1], [1000, 1e9, 11, 1, 100, 1],
    [1e9, 50, 1, 11, 100, 2], [999, 37, 1, 11, 100, 2],
  ])
    captureDC(`dc_dr_${i++}`, 5, [max, spw, start, mx, ramp, exp],
      [DC.doomsdayClockDrain(max, spw, {
        drainStartPercent: start, drainMaxPercent: mx, drainRampSeconds: ramp,
      }, exp)]);
}
// kind 6: rot quota.
{
  let i = 0;
  for (const [left, under, death] of [
    [100, 5, 10], [100, 0, 10], [100, 9, 10], [100, 10, 10], [100, 1e9, 10],
    [0, 0, 10], [-5, 0, 10], [100, 0, 0], [100, 0, -10], [NaN, 0, 10],
    [100, NaN, 10], [100, 0, NaN], [100.5, 5, 10], [1, 0, 1e9], [1e9, 1e9, 1e9],
    [100, -5, 10], [100, 5, Infinity], [Infinity, 0, 10],
  ])
    captureDC(`dc_rq_${i++}`, 6, [left, under, death],
      [DC.doomsdayClockRotQuota(left, under, { rotDeathSeconds: death })]);
}

// --- execution/Util.ts scenario runner ----------------------------------------
// Exercises the pure-GameMap subset of `execution/Util.ts` against the real TS
// functions over a `GameMapImpl` built from packed terrain bytes plus a list of
// `setOwnerID` writes. The Rust twin is `exec_util::run_op(gm, kind, args)`;
// the map is reconstructed identically on both sides. Result tokens are flat
// numbers (uenc: NaN->"n", -0->"-0", +Inf->"i", -Inf->"-i").
//
// kind / arg / result table (mirrors exec_util.rs):
//   0 computeNukeBlastCounts [target,inner,outer] -> [owner,weight]*n
//   1 getSpawnTiles          [tile,requireAll]     -> [1] | [0,len,tiles...]
//   2 closestTile            [tile,refs...]        -> [refOrNaN,dist]
//   3 nearestTileDist        [tile,tiles...]       -> [best]
//   4 nearestTileDistCapped  [tile,cap,mode,tiles] -> [dist] (mode 1 = TileSet)
//   5 closestTwoTiles        [nx,x...,y...]        -> [1] | [0,x,y]
const euScenarios = [];
const EU_LAND = 0x85;
const EU_OCEAN = 0x20;
const EU_IMPASS = 0x9f;
function euMap(w, h, cells, owners) {
  // cells: flat array of glyphs (numbers) length w*h, or a single glyph.
  const terrain = Uint8Array.from(
    typeof cells === "number" ? new Array(w * h).fill(cells) : cells,
  );
  const gm = new GameMapImpl(w, h, terrain, w * h);
  for (const [t, id] of owners || []) gm.setOwnerID(t, id);
  return gm;
}
let euIdx = 0;
function captureEU(name, w, h, cells, owners, kind, args, res) {
  euScenarios.push({
    name: `${name}_${euIdx++}`,
    w,
    h,
    terrain: (typeof cells === "number" ? new Array(w * h).fill(cells) : cells).slice(),
    owners: (owners || []).map(([t, id]) => [uenc(t), uenc(id)]),
    kind,
    args: args.map(uenc),
    res: res.map(uenc),
  });
}
{
  // kind 0: nuke blast counts. 5x5 all-land, owner 1 across the map.
  const m0 = euMap(5, 5, EU_LAND, Array.from({ length: 25 }, (_, i) => [i, 1]));
  for (const [target, inner, outer] of [
    [12, 0, 1], [12, 1, 1], [12, 2, 2], [0, 1, 1], [12, 0, 0],
    [12, NaN, 1], [12, 1, NaN], [NaN, 1, 1], [12, 0.5, 1.5], [12, 3, 3],
  ]) {
    const r = EU.computeNukeBlastCounts({
      gm: m0,
      targetTile: target,
      magnitude: { inner, outer },
    });
    captureEU("eu_nb", 5, 5, EU_LAND,
      Array.from({ length: 25 }, (_, i) => [i, 1]),
      0, [target, inner, outer], [...r.entries()].flat());
  }
  // Mixed owners + ownerless tiles: insertion order of first-seen owners.
  const m0b = euMap(5, 5, EU_LAND, [[12, 3], [11, 7], [13, 3], [17, 5]]);
  for (const [target, inner, outer] of [[12, 0, 2], [12, 1, 2], [11, 0, 3]]) {
    const r = EU.computeNukeBlastCounts({
      gm: m0b,
      targetTile: target,
      magnitude: { inner, outer },
    });
    captureEU("eu_nb_mixed", 5, 5, EU_LAND,
      [[12, 3], [11, 7], [13, 3], [17, 5]],
      0, [target, inner, outer], [...r.entries()].flat());
  }
  // Ownerless map -> empty.
  {
    const r = EU.computeNukeBlastCounts({
      gm: euMap(3, 3, EU_LAND, []),
      targetTile: 4,
      magnitude: { inner: 1, outer: 1 },
    });
    captureEU("eu_nb_empty", 3, 3, EU_LAND, [], 0, [4, 1, 1], []);
  }

  // kind 1: getSpawnTiles over the centred radius-4 Euclidean stack-BFS.
  // All-land unowned: strict and loose both return the full bfs list.
  {
    const gm = euMap(5, 5, EU_LAND, []);
    const loose = EU.getSpawnTiles(gm, 12, false);
    const strict = EU.getSpawnTiles(gm, 12, true);
    captureEU("eu_sp_loose", 5, 5, EU_LAND, [], 1, [12, 0], [0, loose.length, ...loose]);
    captureEU("eu_sp_strict", 5, 5, EU_LAND, [], 1, [12, 1], [0, strict.length, ...strict]);
  }
  // Centre owned -> strict null, loose filters it.
  {
    const gm = euMap(5, 5, EU_LAND, [[12, 1]]);
    const loose = EU.getSpawnTiles(gm, 12, false);
    const strict = EU.getSpawnTiles(gm, 12, true);
    captureEU("eu_sp_owned_loose", 5, 5, EU_LAND, [[12, 1]], 1, [12, 0], [0, loose.length, ...loose]);
    captureEU("eu_sp_owned_strict", 5, 5, EU_LAND, [[12, 1]], 1, [12, 1], strict === null ? [1] : [0, strict.length, ...strict]);
  }
  // Impassable tile inside the ball -> strict null.
  {
    const cells = new Array(25).fill(EU_LAND);
    cells[7] = EU_IMPASS; // (x2,y1) neighbour of centre 12
    const gm = euMap(5, 5, cells, []);
    const strict = EU.getSpawnTiles(gm, 12, true);
    const loose = EU.getSpawnTiles(gm, 12, false);
    captureEU("eu_sp_impass_strict", 5, 5, cells, [], 1, [12, 1], strict === null ? [1] : [0, strict.length, ...strict]);
    captureEU("eu_sp_impass_loose", 5, 5, cells, [], 1, [12, 0], [0, loose.length, ...loose]);
  }
  // Water tile inside the ball -> strict null (not land).
  {
    const cells = new Array(25).fill(EU_LAND);
    cells[11] = EU_OCEAN;
    const gm = euMap(5, 5, cells, []);
    const strict = EU.getSpawnTiles(gm, 12, true);
    captureEU("eu_sp_water_strict", 5, 5, cells, [], 1, [12, 1], strict === null ? [1] : [0, strict.length, ...strict]);
  }
  // Corner / edge / NaN targets.
  for (const [tile, req, gm] of [
    [0, 0, euMap(5, 5, EU_LAND, [])],
    [0, 1, euMap(5, 5, EU_LAND, [])],
    [24, 1, euMap(5, 5, EU_LAND, [])],
    [NaN, 0, euMap(5, 5, EU_LAND, [])],
    [-1, 0, euMap(5, 5, EU_LAND, [])],
  ]) {
    const r = EU.getSpawnTiles(gm, tile, !!req);
    captureEU(`eu_sp_edge`, 5, 5, EU_LAND, [], 1, [tile, req],
      r === null ? [1] : [0, r.length, ...r]);
  }

  // kind 2: closestTile.
  const gm2 = euMap(10, 1, EU_LAND, []);
  for (const [tile, refs] of [
    [3, [2, 5]], [2, [1, 3]], [3, []], [3, [NaN, 5]], [3, [5, NaN]],
    [NaN, [1, 2]], [3, [3]], [0, [9]], [9, [0]], [3, [-1, 7]],
    [3, [1.5, 4]], [5, [5, 5, 5]],
  ]) {
    const [r, d] = EU.closestTile(gm2, refs, tile);
    captureEU("eu_ct", 10, 1, EU_LAND, [], 2, [tile, ...refs], [r === null ? NaN : r, d]);
  }

  // kind 3: nearestTileDist.
  for (const [tile, tiles] of [
    [3, [2, 5]], [3, []], [3, [NaN, 5]], [NaN, [1, 2]], [3, [3]], [0, [9]],
  ]) {
    captureEU("eu_nd", 10, 1, EU_LAND, [], 3, [tile, ...tiles], [EU.nearestTileDist(gm2, tiles, tile)]);
  }

  // kind 4: nearestTileDistCapped.
  const gm4 = euMap(5, 5, EU_LAND, []);
  // Linear (array) branch: mode 0.
  for (const [tile, tiles, cap] of [
    [12, [14], 1], [12, [14], 2], [12, [14], 3], [12, [], 5], [12, [12], 0],
    [12, [NaN], 5], [NaN, [14], 5], [12, [0, 24], 4], [12, [14], Infinity],
    [12, [14], NaN], [12, [14], -1],
  ]) {
    captureEU("eu_nc_lin", 5, 5, EU_LAND, [], 4, [tile, cap, 0, ...tiles],
      [EU.nearestTileDistCapped(gm4, tiles, tile, cap)]);
  }
  // TileSet branch: mode 1. An `Infinity` cap would spin the ring loop
  // forever (`d <= Infinity` never fails) — it hangs the TS side and is
  // not captured.
  for (const [tile, tiles, cap] of [
    [12, [14], 2], [12, [14], 1], [12, [12], 0], [12, [], 5], [12, [0], 4],
    [12, [24], 4], [12, [14, 0], 2], [NaN, [14], 5],
    [12, [14], NaN], [12, [14], -1], [2, [22], 4], [6, [8], 4], [12, [14], 0],
  ]) {
    const ts = new TileSet(tiles);
    captureEU("eu_nc_set", 5, 5, EU_LAND, [], 4, [tile, cap, 1, ...tiles],
      [EU.nearestTileDistCapped(gm4, ts, tile, cap)]);
  }

  // kind 5: closestTwoTiles.
  const gm5 = euMap(10, 2, EU_LAND, []);
  for (const [xs, ys] of [
    [[2], [5]], [[], [5]], [[2], []], [[1, 8], [3]], [[8, 1], [3]],
    [[0, 9], [4, 5]], [[3, 3], [3]], [[NaN], [5]], [[2], [NaN]],
    [[1, 1, 1], [1, 1]], [[5, 15], [9, 19]], [[12], [17]],
  ]) {
    const r = EU.closestTwoTiles(gm5, xs, ys);
    captureEU("eu_c2", 10, 2, EU_LAND, [], 5, [xs.length, ...xs, ...ys],
      r === null ? [1] : [0, r.x, r.y]);
  }
}

// --- WaterManager scenario runner -------------------------------------------
// Op-stream replay against the real `WaterManager` over two real
// `GameMapImpl`s (full + minimap). `runWM` kind table mirrors
// `water_manager::WaterManager::run_op`:
//   0 queueTile(a)          1 tick(a) -> [len, changed...]
//   2 waterGraphVersion()   3 getWaterComponent(a) -> [1|null] | [0,id]
//   4 hasWaterComponent(a,b) 5 getWaterComponentSize(a) -> [1|null] | [0,size]
//   6 map.setOwnerID(a,b)   7 map.setFallout(a,b)
// The scenario records the per-op result streams plus the final terrain/state
// buffers of both maps and the version counter, so any divergence in the
// five-phase finalize (ocean/magnitude/shoreline/minimap/CC) or the throttled
// rebuild shows up even when the returned changed-tile lists happen to match.
const wmScenarios = [];
let wmIdx = 0;
function runWM(name, mw, mh, mcells, nw, nh, ncells, disable, ops) {
  // cells: flat glyph arrays or a single glyph (numbers).
  const mk = (w, h, cells) =>
    new GameMapImpl(
      w,
      h,
      Uint8Array.from(typeof cells === "number" ? new Array(w * h).fill(cells) : cells),
      w * h,
    );
  const map = mk(mw, mh, mcells);
  const mini = mk(nw, nh, ncells);
  // Snapshot the *initial* buffers before any op mutates them.
  const mapTerrain0 = Array.from(map.terrain);
  const mapState0 = Array.from(map.state);
  const miniTerrain0 = Array.from(mini.terrain);
  const wm = new WM.WaterManager(map, mini, disable);
  const played = ops.map(([k, a, b]) => {
    let res;
    switch (k) {
      case 0: wm.queueTile(a); res = []; break;
      case 1: { const c = wm.tick(a); res = [c.length, ...c]; break; }
      case 2: res = [wm.waterGraphVersion()]; break;
      case 3: { const c = wm.getWaterComponent(a); res = c === null ? [1] : [0, c]; break; }
      case 4: res = [wm.hasWaterComponent(a, b) ? 1 : 0]; break;
      case 5: { const s = wm.getWaterComponentSize(a); res = s === null ? [1] : [0, s]; break; }
      case 6: map.setOwnerID(a, b); res = []; break;
      case 7: map.setFallout(a, !!b); res = []; break;
      default: throw new Error("bad wm op kind " + k);
    }
    return [k, uenc(a === undefined ? 0 : a), uenc(b === undefined ? 0 : b), res.map(uenc)];
  });
  wmScenarios.push({
    name: `${name}_${wmIdx++}`,
    mw,
    mh,
    mapTerrain: mapTerrain0,
    mapState: mapState0,
    nw,
    nh,
    miniTerrain: miniTerrain0,
    disable,
    ops: played,
    mapTerrainAfter: Array.from(map.terrain),
    mapStateAfter: Array.from(map.state),
    miniTerrainAfter: Array.from(mini.terrain),
    versionAfter: wm.waterGraphVersion(),
  });
}
{
  const L = 0x85, OC = 0x20, IP = 0x9f;
  // 1. Basic crater: 4x4 all-land full map over an all-land 2x2 minimap.
  //    Converting three of the four tiles under mini tile 0 folds the minimap
  //    (>= min(3,4)), dirties the graph, and the throttled rebuild lands on
  //    tick 20 (never the conversion tick).
  runWM("wm_basic", 4, 4, L, 2, 2, L, false, [
    [0, 0], [0, 1], [0, 4],
    [1, 0],
    [2],
    [1, 19],
    [2],
    [1, 20],
    [2],
    [3, 0],
    [5, 0],
    [4, 0, 1],
  ]);
  // 2. Single isolated crater, ocean minimap: the converted tile joins the
  //    ocean component; queries hit the persistent CC.
  runWM("wm_ocean_mini", 8, 8, L, 4, 4, OC, false, [
    [0, 18],
    [1, 0],
    [3, 18],
    [5, 18],
    [4, 18, 1],
    [1, 20],
    [2],
    [3, 18],
  ]);
  // 3. disableNavMesh: permissive fallbacks (component 0 / has true / size 0),
  //    version never advances.
  runWM("wm_nonav", 4, 4, L, 2, 2, L, true, [
    [0, 0], [0, 1], [0, 4],
    [1, 0],
    [3, 0],
    [4, 0, 7],
    [5, 0],
    [1, 100],
    [2],
  ]);
  // 4. Conquered tile is skipped at flush (owner set between queue and tick).
  runWM("wm_owned", 4, 4, L, 2, 2, L, false, [
    [0, 0], [0, 1], [0, 4],
    [6, 0, 3],
    [1, 0],
    [3, 0],
  ]);
  // 5. Fallout tile converts and clears its fallout bit.
  runWM("wm_fallout", 4, 4, L, 2, 2, L, false, [
    [7, 0, 1],
    [0, 0], [0, 1], [0, 4],
    [1, 0],
  ]);
  // 6. Impassable tile is never converted.
  {
    const cells = new Array(16).fill(L);
    cells[0] = IP;
    runWM("wm_impass", 4, 4, cells, 2, 2, L, false, [
      [0, 0], [0, 1], [0, 4],
      [1, 0],
      [3, 0],
    ]);
  }
  // 7. Two distant craters -> separate crater groups (barrage case). 12x12
  //    full map, 6x6 ocean minimap; convert corners far apart.
  runWM("wm_two_craters", 12, 12, L, 6, 6, OC, false, [
    [0, 0], [0, 1], [0, 12], [0, 13],
    [0, 130], [0, 131], [0, 142], [0, 143],
    [1, 0],
    [3, 0],
    [3, 130],
    [5, 0],
    [1, 20],
    [2],
  ]);
  // 8. Dense cluster collapses to one crater group; shoreline ring observed.
  runWM("wm_dense", 6, 6, L, 3, 3, L, false, [
    [0, 14], [0, 15], [0, 16], [0, 17], [0, 20], [0, 21],
    [1, 0],
    [3, 14],
    [5, 14],
    [1, 20],
    [2],
  ]);
  // 9. Shoreline tile with no water component within 2 hops -> null.
  //    Full map 6x6 all-land; minimap 3x3 all-land; convert a 2x2 block so
  //    mini folds, then query a land tile far from any water.
  runWM("wm_null_comp", 6, 6, L, 3, 3, L, false, [
    [0, 14], [0, 15], [0, 20], [0, 21],
    [1, 0],
    [3, 0],
    [5, 0],
  ]);
  // 10. Repeat conversions across ticks: incremental CC merge (a second
  //     crater bridging two components).
  runWM("wm_merge", 8, 8, L, 4, 4, OC, false, [
    [0, 10], [0, 18],
    [1, 0],
    [3, 10],
    [0, 26], [0, 34], [0, 11], [0, 19],
    [1, 20],
    [3, 26],
    [5, 10],
    [1, 40],
    [2],
  ]);
}

// --- AStar scenario runner ---------------------------------------------------
// The grid adapter below is the *twin* of `pathfinding::a_star::GridAdapter`
// in the Rust crate (same N/S/W/E order, same literal cost predicate, same
// heuristic kinds). Each scenario runs the real TS AStar and records the
// returned path plus the full stamp-tracked arrays afterwards: any divergence
// in expansion order, tie-breaking or Uint32Array g-score wrapping shows up
// in those arrays even when the path itself happens to match.
const asScenarios = [];
function makeGridAdapter(w, h, blocked, constCost, turnPenalty, heurKind, heurScale) {
  const block = new Set(blocked);
  const hit = (v) => block.has(v | 0);
  return {
    neighbors(node, buffer) {
      let n = 0;
      const x = node % w;
      if (node >= w && !hit(node - w)) buffer[n++] = node - w;
      if (node < (h - 1) * w && !hit(node + w)) buffer[n++] = node + w;
      if (x !== 0 && !hit(node - 1)) buffer[n++] = node - 1;
      if (x !== w - 1 && !hit(node + 1)) buffer[n++] = node + 1;
      return n;
    },
    cost(from, to, prev) {
      let c = constCost;
      if (prev !== undefined && (prev - from) !== (to - from)) c += turnPenalty;
      return c;
    },
    heuristic(node, goal) {
      const d =
        Math.abs((node % w) - (goal % w)) +
        Math.abs(Math.floor(node / w) - Math.floor(goal / w));
      if (heurKind === 1) return heurScale * d;
      if (heurKind === 2) return 0;
      return d;
    },
    numNodes: () => w * h,
    maxPriority: () => (w + h) * (constCost + turnPenalty) * 4 + 8,
    maxNeighbors: () => 4,
  };
}
function captureAStar(name, cfg, starts, goal, runs = 1) {
  const { w, h, blocked, cc, tp, hk, hs, maxIter } = cfg;
  const adapter = makeGridAdapter(w, h, blocked, cc, tp, hk, hs);
  const astar =
    maxIter === null
      ? new AStar({ adapter })
      : new AStar({ adapter, maxIterations: maxIter });
  let path = null;
  for (let i = 0; i < runs; i++) path = astar.findPath(starts, goal);
  asScenarios.push({
    name,
    w,
    h,
    blocked,
    cc,
    tp,
    hk,
    hs,
    maxIter,
    starts,
    goal,
    runs,
    path: path === null ? "u" : Array.from(path),
    stampAfter: astar.stamp,
    closed: Array.from(astar.closedStamp),
    gsStamp: Array.from(astar.gScoreStamp),
    gScore: Array.from(astar.gScore),
    cameFrom: Array.from(astar.cameFrom),
  });
}
const asCfg = (w, h, blocked = [], cc = 1, tp = 0, hk = 0, hs = 0, maxIter = null) => ({
  w, h, blocked, cc, tp, hk, hs, maxIter,
});
captureAStar("as_line", asCfg(5, 1), [0], 4);
captureAStar("as_blocked", asCfg(3, 3, [1, 4, 7]), [0], 2);
captureAStar("as_start_goal", asCfg(3, 3), [4], 4);
captureAStar("as_capped", asCfg(5, 5, [], 1, 0, 0, 0, 3), [0], 24);
captureAStar("as_multistart", asCfg(5, 5), [3, 21], 12);
captureAStar("as_turns", asCfg(5, 5, [], 1, 3), [0], 24);
captureAStar("as_weighted", asCfg(6, 6, [7, 8, 9, 14, 20, 21, 22], 1, 0, 1, 0.5), [0], 35);
captureAStar("as_dijkstra", asCfg(5, 5, [6, 11, 12, 13], 1, 2, 2, 0), [0], 24);
// Reused instance: two full searches on one AStar; the recorded arrays are
// from *after* the second, pinning stamp advancement and stale-state hygiene.
captureAStar("as_reuse", asCfg(5, 5), [0], 24, 2);
// Negative const cost: g-scores wrap through the Uint32Array store and the
// search exhausts the bucket queue (pop sentinel -1) before finding a path.
captureAStar("as_negcost", asCfg(3, 3, [], -1, 2, 0, 0, 40), [0], 8);

// --- AStarRail scenario runner -----------------------------------------------
// Runs the real `AStarRail` (its private RailAdapter over a real GameMapImpl)
// and records the path plus the engine's stamp-tracked arrays. Terrain bytes
// follow GameMapImpl's packing: bit7 land, bit6 shoreline, bit5 ocean,
// bits0-4 magnitude (31 = impassable). The Rust twin is `rail::RailAdapter`
// over `rail::TerrainMap`.
const railScenarios = [];
function captureRail(name, rows, starts, goal) {
  const h = rows.length;
  const w = rows[0].length;
  const glyph = { L: 0x85, s: 0x40, o: 0x20, M: 0x9f };
  const terrain = Uint8Array.from(rows.join("").split(""), (c) => glyph[c]);
  const gm = new GameMapImpl(w, h, terrain, w * h);
  const rail = new AStarRail(gm);
  // AStarRail wraps AStar privately; reach the engine for its arrays.
  const astar = rail.aStar;
  // One findPath with the whole array: TS treats it as a multi-start search.
  const path = rail.findPath(starts, goal);
  railScenarios.push({
    name,
    w,
    h,
    terrain: Array.from(terrain),
    starts,
    goal,
    path: path === null ? "u" : Array.from(path),
    stampAfter: astar.stamp,
    closed: Array.from(astar.closedStamp),
    gsStamp: Array.from(astar.gScoreStamp),
    gScore: Array.from(astar.gScore),
    cameFrom: Array.from(astar.cameFrom),
  });
}
// Shoreline water column crossable, plain ocean wall is not.
captureRail("rail_shore_cross", ["LLsLL", "LLsLL", "LLsLL"], [0], 4);
captureRail("rail_ocean_wall", ["LLoLL", "LLoLL", "LLoLL"], [0], 2);
// Impassable land: enterable (it is land) but never expanded from.
captureRail("rail_impassable_fwd", ["LLM"], [0], 2);
captureRail("rail_impassable_rev", ["LLM"], [2], 0);
// A land corridor between two water bodies: the +5 water/shoreline and +3
// direction-change penalties decide the route, and the shoreline rule gates
// which water tiles are enterable at all.
captureRail(
  "rail_penalty_route",
  ["soooL", "LLLLL", "soooL"],
  [0, 10],
  4,
);
// A lake ring: the only land is the shoreline tiles, so the path must hug the
// shore (every step is a shoreline-water or land tile the rule permits).
captureRail(
  "rail_shore_ring",
  ["osso", "sLLs", "sLLs", "osso"],
  [6],
  9,
);

// --- AStarWater scenario runner ----------------------------------------------
// Runs the real `AStarWater` over a real GameMapImpl (it reaches the map's
// private `terrain` Uint8Array directly, exactly like the class does) and
// records the path plus the engine's stamp-tracked arrays. Water cost depends
// on the magnitude bits (3-10 sweet spot, <3 = +1000, >10 = +100), land is a
// wall unless it is the goal, and the f32 MinHeap priorities plus the
// cross-product tie-breaker decide every pop order. The Rust twin is
// `water::AStarWater`.
const waterScenarios = [];
// L land mag5, w deep water mag8 (sweet spot), x shallow water mag1 (+1000).
const WGL = { L: 0x85, w: 0x08, x: 0x01 };
function captureWater(name, rows, starts, goal, weight, maxIter) {
  const h = rows.length;
  const w = rows[0].length;
  const terrain = Uint8Array.from(rows.join("").split(""), (c) => WGL[c]);
  const gm = new GameMapImpl(w, h, terrain, w * h);
  const cfg = {};
  if (weight !== null && weight !== undefined) cfg.heuristicWeight = weight;
  if (maxIter !== null && maxIter !== undefined) cfg.maxIterations = maxIter;
  const water = new AStarWater(gm, Object.keys(cfg).length ? cfg : undefined);
  const path = water.findPath(starts, goal);
  waterScenarios.push({
    name,
    w,
    h,
    terrain: Array.from(terrain),
    weight: weight ?? 5,
    maxIter: maxIter ?? 1000000,
    starts,
    goal,
    path: path === null ? "u" : Array.from(path),
    stampAfter: water.stamp,
    closed: Array.from(water.closedStamp),
    gsStamp: Array.from(water.gScoreStamp),
    gScore: Array.from(water.gScore),
    cameFrom: Array.from(water.cameFrom),
  });
}
// Straight shot across a deep-water channel (magnitude 8 = sweet spot).
captureWater("w_line", ["wwwwwwwww"], [0], 8, null, null);
// Shallow band (magnitude 1 -> +1000/tile): the search detours around it.
captureWater(
  "w_shallow_detour",
  ["xxxxxxx", "wwwwwww", "wwwwwww"],
  [0],
  6,
  null,
  null,
);
// Land wall down the middle with a single water gap in row 2: the only route
// from the left shore to the right must pass through the gap tile.
captureWater(
  "w_land_gap",
  ["wwLww", "wwLww", "wwwww", "wwLww", "wwLww"],
  [0],
  4,
  null,
  null,
);
// The goal itself is land: entering it is legal even though land is a wall.
captureWater("w_goal_land", ["wwL"], [0], 2, null, null);
// Multi-start on a lake ring: starts hug opposite shores of the impassable
// centre, so the cross-product tie-breaker (which side of the start->goal
// line a candidate sits on) decides the route.
captureWater(
  "w_multistart_ring",
  ["wwwww", "wLLLw", "wLLLw", "wLLLw", "wwwww"],
  [0, 24],
  2,
  null,
  null,
);
// Weight-1 heuristic: nearly Dijkstra, so the f32 priority ties (all costs
// are multiples of 100) exercise the MinHeap's insertion-order pop order.
captureWater("w_weight1_ties", ["wwwwwww", "wwwwwww"], [0], 13, 1, null);
// Iteration cap: the maze route needs 17 pops, so a budget of 10 makes the
// search give up and return null (the cap branch is exercised).
captureWater(
  "w_capped",
  ["wwwww", "LLLLw", "wwwww", "wLLLL", "wwwww"],
  [0],
  24,
  null,
  10,
);

// --- AStarWaterBounded scenario runner ---------------------------------------
// Runs the real `AStarWaterBounded` over a real GameMapImpl (private terrain
// bytes again) and records the path plus the engine's four stamp-tracked
// LOCAL arrays. mode 0 = findPath (bounds derived from the start/goal hull);
// mode 1 = searchBounded with explicit bounds (pins the clamp, the
// numLocalNodes guard and the inverted-bounds degeneracies). The Rust twin is
// `water_bounded::AStarWaterBounded`.
const wbScenarios = [];
function captureWB(name, rows, starts, goal, opts) {
  const h = rows.length;
  const w = rows[0].length;
  const terrain = Uint8Array.from(rows.join("").split(""), (c) => WGL[c]);
  const gm = new GameMapImpl(w, h, terrain, w * h);
  const { weight = null, maxIter = null, maxArea = w * h, bounds = null } = opts ?? {};
  const cfg = {};
  if (weight !== null) cfg.heuristicWeight = weight;
  if (maxIter !== null) cfg.maxIterations = maxIter;
  const wb = new AStarWaterBounded(gm, maxArea, Object.keys(cfg).length ? cfg : undefined);
  let path;
  let mode;
  if (bounds) {
    mode = 1;
    path = wb.searchBounded(starts, goal, {
      minX: bounds[0],
      maxX: bounds[1],
      minY: bounds[2],
      maxY: bounds[3],
    });
  } else {
    mode = 0;
    path = wb.findPath(starts, goal);
  }
  wbScenarios.push({
    name,
    mode,
    w,
    h,
    terrain: Array.from(terrain),
    maxArea,
    weight: weight ?? 3,
    maxIter: maxIter ?? 100000,
    starts,
    goal,
    bounds: bounds ?? null,
    path: path === null ? "u" : Array.from(path),
    stampAfter: wb.stamp,
    closed: Array.from(wb.closedStamp),
    gsStamp: Array.from(wb.gScoreStamp),
    gScore: Array.from(wb.gScore),
    cameFrom: Array.from(wb.cameFrom),
  });
}
// Straight shot across a deep-water channel (window = the row itself).
captureWB("wb_line", ["wwwwwwwww"], [0], 8);
// Shallow band (magnitude 1 -> +300/tile here, not +1000): detour decision
// depends on this class's penalty curve.
captureWB("wb_shallow_detour", ["xxxxxxx", "wwwwwww", "wwwwwww"], [0], 6);
// Land wall with a single gap in row 2.
captureWB("wb_land_gap", ["wwLww", "wwLww", "wwwww", "wwLww", "wwLww"], [0], 4);
// The goal itself is land: entering it is legal even though land is a wall.
captureWB("wb_goal_land", ["wwL"], [0], 2);
// Multi-start on a lake ring: tie-breaker decides which side of the line.
captureWB("wb_multistart_ring", ["wwwww", "wLLLw", "wLLLw", "wLLLw", "wwwww"], [0, 24], 2);
// Weight-1 heuristic: f32 priority ties exercise the MinHeap pop order.
captureWB("wb_weight1_ties", ["wwwwwww", "wwwwwww"], [0], 13, { weight: 1 });
// Iteration cap: budget 10 makes the search give up (null).
captureWB("wb_capped", ["wwwww", "LLLLw", "wwwww", "wLLLL", "wwwww"], [0], 24, { maxIter: 10 });
// Same-row start/goal: the derived window is one row tall, so the land column
// cannot be detoured around -> null (the window restricts the search).
captureWB("wb_unreachable", ["wwLww", "wwwww", "wwwww", "wwwww", "wwwww"], [0], 4);
// maxSearchArea 4 < numLocalNodes 9: the guard returns null before searching.
captureWB("wb_too_small", ["www", "www", "www"], [0], 8, { maxArea: 4 });
// Explicit window excluding the goal corner: toLocal clamps goal (24) to the
// window corner tile 18, so the search targets the CLAMPED tile.
captureWB("wb_clamped_goal", ["wwwww", "wwwww", "wwwww", "wwwww", "wwwww"], [0], 24, {
  bounds: [1, 3, 1, 3],
});
// Inverted bounds (minX > maxX, minY > maxY): boundsWidth/Height are -1,
// numLocalNodes = 1, and the degenerate window maps everything to local 0 —
// the search "succeeds" with a single clamped tile.
captureWB("wb_inverted_bounds", ["www", "www", "www"], [0], 4, {
  bounds: [2, 0, 2, 0],
});

// --- AbstractGraphAStar scenario runner --------------------------------------
// Runs the real `AbstractGraphAStar` over a hand-built `AbstractGraph` (via the
// underscore-prefixed `_addNode`/`_addEdge`) and records, after EVERY query,
// the returned path plus the engine's full stamp-tracked state and the live
// MinHeap. Multi-query scenarios pin the stamp-reuse and the "queue is not
// cleared before the missing-node early return" observations. The Rust twin is
// `abstract_graph_astar::AbstractGraphAStar`.
const agaScenarios = [];
const f32bits = (a) =>
  Array.from(a, (v) => {
    dv.setFloat32(0, v);
    return dv.getUint32(0);
  });
function mkAgaGraph(nodes, edges) {
  const g = new AbstractGraph(1, 1, 1);
  for (const n of nodes) g._addNode({ id: n.id, x: n.x, y: n.y, tile: 0, componentId: 0 });
  for (const e of edges)
    g._addEdge({ id: e.id, nodeA: e.a, nodeB: e.b, cost: e.cost, clusterX: 0, clusterY: 0 });
  return g;
}
function captureAGA(name, spec, queries) {
  const { weight = null, maxIter = null } = spec;
  const graph = mkAgaGraph(spec.nodes, spec.edges);
  const cfg = {};
  if (weight !== null) cfg.heuristicWeight = weight;
  if (maxIter !== null) cfg.maxIterations = maxIter;
  const eng = new AbstractGraphAStar(graph, Object.keys(cfg).length ? cfg : undefined);
  const snap = () => ({
    stampAfter: eng.stamp,
    closed: Array.from(eng.closedStamp),
    gsStamp: Array.from(eng.gScoreStamp),
    gScoreBits: f32bits(eng.gScore),
    cameFrom: Array.from(eng.cameFrom),
    startNode: Array.from(eng.startNode),
    qHeap: Array.from(eng.queue.heap),
    qPriBits: f32bits(eng.queue.priorities),
    qSize: eng.queue.size,
    qCap: eng.queue.capacity,
  });
  const qs = queries.map((q) => {
    const isMulti = Array.isArray(q.start);
    const path = eng.findPath(q.start, q.goal);
    return {
      isMulti: isMulti ? 1 : 0,
      starts: isMulti ? q.start.slice() : [q.start],
      goal: q.goal,
      path: path === null ? "u" : Array.from(path),
      ...snap(),
    };
  });
  agaScenarios.push({
    name,
    numNodes: graph.nodeCount,
    edgeCount: graph.edgeCount,
    weight: weight ?? 1,
    maxIter: maxIter ?? 100000,
    nodes: spec.nodes.flatMap((n) => [n.id, n.x, n.y]),
    edges: spec.edges.flatMap((e) => [e.id, e.a, e.b, e.cost]),
    queries: qs,
  });
}

// Straight 3-node chain: the canonical happy path.
captureAGA(
  "aga_line",
  { nodes: [{ id: 0, x: 0, y: 0 }, { id: 1, x: 1, y: 0 }, { id: 2, x: 2, y: 0 }],
    edges: [{ id: 0, a: 0, b: 1, cost: 1 }, { id: 1, a: 1, b: 2, cost: 1 }] },
  [{ start: 0, goal: 2 }],
);
// Weight-10 heuristic rescales every priority: exercises the heap ordering and
// a reverse query on the same engine (stamp reuse).
captureAGA(
  "aga_weight",
  { nodes: [{ id: 0, x: 0, y: 0 }, { id: 1, x: 1, y: 0 }, { id: 2, x: 2, y: 0 }],
    edges: [{ id: 0, a: 0, b: 1, cost: 1 }, { id: 1, a: 1, b: 2, cost: 1 }],
    weight: 10 },
  [{ start: 0, goal: 2 }, { start: 2, goal: 0 }],
);
// Two routes to the goal that differ only in Float32Array gScore rounding:
// 0.1+0.2 vs 0.2+0.1 accumulate to different f32 values, so the relaxation
// comparison `tentativeG < gScore[n]` (reading the f32-rounded store) picks the
// path the f64 sum alone would not.
captureAGA(
  "aga_f32_round",
  { nodes: [{ id: 0, x: 0, y: 0 }, { id: 1, x: 1, y: 0 }, { id: 2, x: 1, y: 1 }, { id: 3, x: 2, y: 0 }],
    edges: [
      { id: 0, a: 0, b: 1, cost: 0.1 },
      { id: 1, a: 1, b: 3, cost: 0.2 },
      { id: 2, a: 0, b: 2, cost: 0.2 },
      { id: 3, a: 2, b: 3, cost: 0.1 },
    ] },
  // Query 1 reaches the goal while node 2 is still in the heap (qsize 1).
  // Query 2 has a missing goal: the early return happens BEFORE queue.clear(),
  // so the heap must still hold node 2 — pinning the check ordering.
  [{ start: 0, goal: 3 }, { start: 0, goal: 99 }],
);
// Missing goal AFTER a successful search: the goal-node check runs before
// `queue.clear()`, so the second query's null return leaves the first query's
// queue state observable.
captureAGA(
  "aga_missing_goal",
  { nodes: [{ id: 0, x: 0, y: 0 }, { id: 1, x: 1, y: 0 }, { id: 2, x: 2, y: 0 }],
    edges: [{ id: 0, a: 0, b: 1, cost: 1 }, { id: 1, a: 1, b: 2, cost: 1 }] },
  [{ start: 0, goal: 2 }, { start: 0, goal: 99 }],
);
// Missing start: the stamp is consumed (bump happens first), then the start
// lookup returns null before any queue mutation.
captureAGA(
  "aga_missing_start",
  { nodes: [{ id: 0, x: 0, y: 0 }, { id: 1, x: 1, y: 0 }, { id: 2, x: 2, y: 0 }],
    edges: [{ id: 0, a: 0, b: 1, cost: 1 }, { id: 1, a: 1, b: 2, cost: 1 }] },
  [{ start: 99, goal: 2 }],
);
// Multi-source from both ends of a 5-line toward the middle: the two equal
// routes tie-break through the heap, and `startNode` records the origin.
captureAGA(
  "aga_multi_ring",
  { nodes: [{ id: 0, x: 0, y: 0 }, { id: 1, x: 1, y: 0 }, { id: 2, x: 2, y: 0 }, { id: 3, x: 3, y: 0 }, { id: 4, x: 4, y: 0 }],
    edges: [
      { id: 0, a: 0, b: 1, cost: 1 },
      { id: 1, a: 1, b: 2, cost: 1 },
      { id: 2, a: 2, b: 3, cost: 1 },
      { id: 3, a: 3, b: 4, cost: 1 },
    ] },
  [{ start: [0, 4], goal: 2 }],
);
// A single-element start array delegates to findPathSingle: it consumes a
// stamp but never writes `startNode` (stays all-zero).
captureAGA(
  "aga_multi_single",
  { nodes: [{ id: 0, x: 0, y: 0 }, { id: 1, x: 1, y: 0 }, { id: 2, x: 2, y: 0 }],
    edges: [{ id: 0, a: 0, b: 1, cost: 1 }, { id: 1, a: 1, b: 2, cost: 1 }] },
  [{ start: [0], goal: 2 }],
);
// Empty start array: null returned BEFORE the stamp bump (stamp stays at 1).
captureAGA(
  "aga_multi_empty",
  { nodes: [{ id: 0, x: 0, y: 0 }, { id: 1, x: 1, y: 0 }, { id: 2, x: 2, y: 0 }],
    edges: [{ id: 0, a: 0, b: 1, cost: 1 }, { id: 1, a: 1, b: 2, cost: 1 }] },
  [{ start: [], goal: 2 }],
);
// Iteration cap of 2 on a 6-node chain: the search gives up before reaching
// the far goal.
captureAGA(
  "aga_capped",
  { nodes: [0, 1, 2, 3, 4, 5].map((i) => ({ id: i, x: i, y: 0 })),
    edges: [0, 1, 2, 3, 4].map((i) => ({ id: i, a: i, b: i + 1, cost: 1 })),
    maxIter: 2 },
  [{ start: 0, goal: 5 }],
);
// Disconnected goal: the queue drains without ever popping the goal -> null.
captureAGA(
  "aga_unreachable",
  { nodes: [{ id: 0, x: 0, y: 0 }, { id: 1, x: 1, y: 0 }, { id: 2, x: 5, y: 5 }, { id: 3, x: 6, y: 5 }],
    edges: [{ id: 0, a: 0, b: 1, cost: 1 }, { id: 1, a: 2, b: 3, cost: 1 }] },
  [{ start: 0, goal: 3 }],
);

// --- GameMap scenario runner -------------------------------------------------
// Replays a scripted op stream against the real `GameMapImpl` and records every
// return value plus the final typed-array state and counters. The Rust twin is
// `game_map::GameMap`; a divergence in neighbour order, the stack-BFS traversal,
// typed-array write dropping, or counter bookkeeping shows up in the trace.
//
// Op result tokens: "v" void mutation, "u" undefined scalar (invalid-ref
// getter), "t" threw, 1/0 boolean, number (enc maps NaN->"n", -0->"-0"), or an
// array of numbers (neighbour / search results).
const gmScenarios = [];
function runGm(name, w, h, terrain, numLand, ops) {
  const gm = new GameMapImpl(w, h, Uint8Array.from(terrain), numLand);
  const played = ops.map(([k, a, b]) => {
    let res;
    switch (k) {
      case 0: gm.setWater(a); res = "v"; break;
      case 1: gm.setShorelineBit(a); res = "v"; break;
      case 2: gm.clearShorelineBit(a); res = "v"; break;
      case 3: gm.setOcean(a); res = "v"; break;
      case 4: gm.setMagnitude(a, b); res = "v"; break;
      case 5:
        try { gm.setOwnerID(a, b); res = "v"; }
        catch (e) { if (!/exceeds maximum/.test(String(e.message))) throw e; res = "t"; }
        break;
      case 6: gm.setFallout(a, !!b); res = "v"; break;
      case 7: gm.setDefenseBonus(a, !!b); res = "v"; break;
      case 8: res = enc(gm.updateTile(a, b)); break;
      case 9: res = gm.neighbors(a).map(enc); break;
      case 10: { const o = []; const n = gm.neighbors4(a, o); res = o.slice(0, n).map(enc); break; }
      case 11: { const o = []; const n = gm.neighbors8(a, o); res = o.slice(0, n).map(enc); break; }
      case 12: { const o = []; gm.forEachNeighborWithDiag(a, (t) => o.push(t)); res = o.map(enc); break; }
      case 13: res = enc(gm.isLand(a)); break;
      case 14: res = enc(gm.isImpassable(a)); break;
      case 15: res = enc(gm.isOceanShore(a)); break;
      case 16: res = enc(gm.isShore(a)); break;
      case 17: res = enc(gm.isWater(a)); break;
      case 18: res = enc(gm.cost(a)); break;
      case 19: res = enc(gm.terrainType(a)); break;
      case 20: res = enc(gm.magnitude(a)); break;
      case 21: res = gm.terrainByte(a) === undefined ? "u" : gm.terrainByte(a); break;
      case 22: res = enc(gm.ownerID(a)); break;
      case 23: res = gm.tileState(a) === undefined ? "u" : gm.tileState(a); break;
      case 24: res = enc(gm.hasFallout(a)); break;
      case 25: res = enc(gm.hasDefenseBonus(a)); break;
      case 26: res = enc(gm.hasOwner(a)); break;
      case 27: res = enc(gm.isBorder(a)); break;
      case 28: res = enc(gm.isOnEdgeOfMap(a)); break;
      case 29: res = enc(gm.x(a)); break;
      case 30: res = enc(gm.y(a)); break;
      case 31:
        try { res = enc(gm.ref(a, b)); }
        catch (e) { if (!/Invalid coordinates/.test(String(e.message))) throw e; res = "t"; }
        break;
      case 32: res = enc(gm.manhattanDist(a, b)); break;
      case 33: res = enc(gm.euclideanDistSquared(a, b)); break;
      case 34: {
        const f = b === 1 ? (t) => gm.isLand(t) : b === 2 ? (t) => t % 2 === 0 : () => true;
        res = Array.from(gm.bfs(a, (_gm, t) => f(t))).map(enc);
        break;
      }
      case 35: {
        const radius = b;
        const f = (t, d2) => (radius === 0 ? true : d2 % 2 === 0);
        res = Array.from(gm.circleSearch(a, Math.abs(radius) || 1, f)).map(enc);
        break;
      }
      default: throw new Error("bad gm op kind " + k);
    }
    return [k, enc(a), enc(b === undefined ? 0 : b), res];
  });
  gmScenarios.push({
    name,
    w,
    h,
    terrain: Array.from(terrain),
    numLand,
    ops: played,
    terrainAfter: Array.from(gm.terrain),
    stateAfter: Array.from(gm.state),
    numLandAfter: gm.numLandTiles(),
    waterVersionAfter: gm.waterVersion(),
    falloutAfter: gm.numTilesWithFallout(),
  });
}

// Glyphs: L land mag5, o ocean, M impassable land, s shoreline water,
// H highland mag15, R mountain mag25. w/x are pure water (no land bit) with
// magnitudes chosen for AStarWater's cost curve: w = mag 8 (sweet spot),
// x = mag 1 (too close to shore, +1000).
const GL = { L: 0x85, o: 0x20, M: 0x9f, s: 0x40, H: 0x8f, R: 0x99, w: 0x08, x: 0x01 };
function rowsTerrain(rows) {
  return rows.join("").split("").map((c) => GL[c]);
}

// 4x4 mixed map: exercise every getter/setter and a full query sweep.
runGm(
  "gm_sweep",
  4,
  4,
  rowsTerrain(["LLoM", "LHos", "RRoo", "LsoL"]),
  8,
  [
    [13, 0], [14, 5], [15, 3], [16, 6], [17, 2], [18, 0], [19, 5], [20, 5],
    [21, 0], [22, 0], [23, 0], [29, 5], [30, 5], [31, 2, 1],
    [9, 0], [10, 5], [11, 5], [12, 5], [9, 15],
    [0, 0], [13, 0], [18, 0], [21, 0],
    [3, 2], [19, 2],
    [4, 1, 22], [19, 1], [20, 1],
    [5, 6, 100], [22, 6], [26, 6], [27, 6],
    [5, 7, 5000], // throws: playerId > 0xfff
    [6, 6, 1], [24, 6], [8, 6, (1 << 13) | (100 << 16)], // updateTile keeps fallout, terrain 100
    [7, 6, 1], [25, 6],
    [2, 1], [16, 1],
    [34, 0, 1], // bfs over land only
    [34, 0, 0], // bfs all
    [35, 5, 2], // circleSearch radius 2, d2-even filter
  ],
);

// Counter focus: setWater / updateTile land flips, fallout add/remove.
runGm(
  "gm_counters",
  3,
  3,
  rowsTerrain(["LLL", "LML", "ooo"]),
  5,
  [
    [0, 0], [0, 4], [0, 1], // water two land tiles, impassable guarded
    [6, 0, 1], [6, 1, 1], [6, 0, 1], // fallout set twice on tile 0
    [6, 0, 0], [6, 0, 0], // clear twice
    [8, 3, (1 << 13) | (0x85 << 16)], // land + fallout on water tile
    [8, 3, (1 << 13) | (0x20 << 16)], // ocean + fallout, terrain changed
    [8, 8, 0x85 << 16], // flip ocean corner to land
  ],
);

// Invalid / fractional / negative refs: writes drop, counters still move.
// (No bfs from an invalid ref: the traversal never bounds-checks and would
// walk the infinite integer lattice.)
runGm(
  "gm_invalid",
  2,
  2,
  rowsTerrain(["Lo", "oL"]),
  2,
  [
    [13, -1], [13, 1.5], [13, 99], [20, -1], [21, 1.5], [23, 99], [22, -1],
    [0, 99], [4, 1.5, 7], [5, -1, 3], [6, 99, 1], [7, 1.5, 1],
    [8, 99, 0x85 << 16], [8, -1, 0x20], [9, 1.5], [9, -1],
    [29, -1], [30, -1], [31, 1.5, 0], [31, 0, 1.5],
    [35, -1, 2],
  ],
);

// updateTile packing: state/terrain split, fallout diff, land-flip version.
runGm(
  "gm_update",
  3,
  1,
  rowsTerrain(["LoM"]),
  2,
  [
    [8, 0, 0x0000 | (0x20 << 16)], // same terrain (ocean stays? no, tile0 is L)
    [8, 0, 0x0005 | (0x85 << 16)], // land, owner 5, terrain unchanged
    [8, 1, 0x2000 | (0x85 << 16)], // ocean->land, defense bit
    [8, 2, (1 << 13) | (0x9f << 16)], // impassable + fallout
    [8, 2, 0x0000], // clear fallout + terrain -> water
    [23, 0], [22, 0], [25, 1], [24, 2],
  ],
);

// Larger search surface: neighbour + traversal ordering on a 5x5.
runGm(
  "gm_search",
  5,
  5,
  rowsTerrain(["LLLLL", "LoooL", "LoMLo", "LoooL", "LLLLL"]),
  16,
  [
    [9, 12], [11, 12], [12, 12], [11, 0], [12, 0], [11, 4], [9, 6],
    [34, 0, 1], [34, 12, 0], [34, 12, 2],
    [35, 12, 2], [35, 0, 3],
    [27, 5], [28, 12], [15, 6],
  ],
);

// --- TileSet scenario runner -------------------------------------------------
// Replays a scripted op stream against the real `TileSet` and records every
// return value plus the final internal buffers. The Rust twin is
// `tile_set::TileSet`; a divergence in hash probing, tombstone skipping,
// deferred compaction, or the Uint32Array storage-vs-arg comparison shows up
// in the trace.
//
// Op result tokens reuse the GameMap table: "v" void mutation, 1/0 boolean,
// number (enc maps NaN->"n", -0->"-0"), or an array (values()/forEach).
//
// kind: 0=add(v) "v", 1=delete(v) bool, 2=has(v) bool, 3=size->Val,
//       4=values()->Arr, 5=clear() "v", 6=forEach collect->Arr,
//       7=add-during-forEach: a=extra value to append inside the callback,
//         result is the visited array (pins denseLen re-read + growth),
//       8=delete-during-forEach: a=value to delete inside the callback,
//         result is the visited array (pins tombstone skip of not-yet-visited).
const tsScenarios = [];
function runTs(name, initial, ops) {
  const ts = new TileSet(initial.length ? initial : undefined);
  const played = ops.map(([k, a]) => {
    let res;
    switch (k) {
      case 0: ts.add(a); res = "v"; break;
      case 1: res = enc(ts.delete(a)); break;
      case 2: res = enc(ts.has(a)); break;
      case 3: res = enc(ts.size); break;
      case 4: res = Array.from(ts.values()).map(enc); break;
      case 5: ts.clear(); res = "v"; break;
      case 6: { const o = []; ts.forEach((t) => o.push(t)); res = o.map(enc); break; }
      case 7: {
        const o = [];
        ts.forEach((t) => { o.push(t); if (o.length === 1) ts.add(a); });
        res = o.map(enc);
        break;
      }
      case 8: {
        const o = [];
        ts.forEach((t) => { o.push(t); if (o.length === 1) ts.delete(a); });
        res = o.map(enc);
        break;
      }
      default: throw new Error("bad ts op kind " + k);
    }
    return [k, enc(a === undefined ? 0 : a), 0, res];
  });
  tsScenarios.push({
    name,
    initial,
    ops: played,
    // Final internal state (reached via the same private fields the class uses).
    dense: Array.from(ts.dense),
    denseLen: ts.denseLen,
    size: ts.size_,
    table: Array.from(ts.table),
    tableUsed: ts.tableUsed,
    iterDepth: ts.iterDepth,
  });
}

// Basic insertion order, membership, and size bookkeeping.
runTs("ts_basic", [], [
  [0, 5], [0, 1], [0, 9], [0, 3], [2, 5], [2, 7], [3, 0], [4, 0],
]);
// Duplicate add is a no-op; delete of a missing value is false.
runTs("ts_dup", [1, 2, 3], [
  [0, 2], [3, 0], [1, 99], [1, 2], [2, 2], [4, 0],
]);
// Delete + re-add moves the value to the end (tombstone then fresh slot).
runTs("ts_readd", [1, 2, 3], [
  [1, 2], [0, 2], [4, 0], [3, 0],
]);
// The Uint32Array storage quirk: -1 stores 0xffffffff, so has(-1) is false
// but has(0xffffffff) is true, and iteration skips the tombstone-equal slot.
runTs("ts_uint32_quirk", [], [
  [0, -1], [2, -1], [2, 4294967295], [4, 0], [3, 0], [1, 4294967295], [3, 0],
]);
// Growth past the initial dense(16)/table(32): 40 adds force a rehash and
// dense doubling; membership must survive.
runTs("ts_growth", [], [
  ...Array.from({ length: 40 }, (_, i) => [0, i * 7 + 1]),
  [3, 0], [2, 1], [2, 274], [2, 5],
]);
// Tombstone compaction on delete: fill past 64 dense slots, delete most, then
// a delete triggers compact (iterDepth 0). Check the surviving order.
runTs("ts_compact", Array.from({ length: 70 }, (_, i) => i), [
  ...Array.from({ length: 60 }, (_, i) => [1, i]),
  [3, 0], [4, 0],
]);
// forEach that appends during iteration: the appended value must be visited
// (denseLen is re-read each step) and a growth buffer swap must not lose it.
runTs("ts_iter_add", [10, 20, 30], [
  [7, 999], [4, 0],
]);
// forEach that deletes a not-yet-visited entry: it must be skipped.
runTs("ts_iter_delete", [10, 20, 30, 40], [
  [8, 30], [4, 0],
]);
// clear() resets both buffers to constructor defaults.
runTs("ts_clear", [1, 2, 3], [
  [5, 0], [3, 0], [4, 0],
]);

// --- Util scenario runner --------------------------------------------------
// Exercises the deterministic core of `src/core/Util.ts` against the real TS
// functions and records every result. The Rust twin is `util::*`; a divergence
// in the UTF-16 hash walk, the first-minimum tie rule, the Map-insertion-order
// mode, the ±Infinity bigint clamps, the sigmoid exp stream, or the bounding
// box scans shows up in `res`. `status` is 0 ok, 1 null, 2 threw.
//
// kind table (matches the Rust dispatch in parity_structures.rs):
//   0 manhattanDistWrapped [x1,y1,x2,y2,width] -> [d]
//   1 within [value,min,max] -> [out]
//   2 simpleHash strs=[s] -> [hash]
//   3 findMinimumBy [scoreKind,candKind,v0,v1,...] -> [winner] (1=null)
//   4 getMode [k0,c0,k1,c1,...] -> [mode] (1=null)
//   5 toInt [num] -> [bigint as f64] (2=threw on NaN)
//   6 maxInt [a,b] -> [out]   7 minInt [a,b] -> [out]
//   8 withinInt [num,min,max] -> [out]
//   9 sigmoid [value,decayRate,midpoint] -> [out]
//  10 boundingBoxCenter [minx,miny,maxx,maxy] -> [cx,cy]
//  11 inscribed [ominx,ominy,omaxx,omaxy,iminx,iminy,imaxx,imaxy] -> [0/1]
//  12 calculateBoundingBox [w,h,containerKind,tile...] -> [minx,miny,maxx,maxy]
//  13 boundingBoxTiles [w,h,center,radius] -> [tiles...]
//  14 calculateBoundingBoxCenter [w,h,tile...] -> [cx,cy]
//  15 sanitizeClanTag [len,u0..] -> [outlen,u0..]  (UTF-16 code units)
//  16 sanitizeLobbyLabel [len,u0..] -> [outlen,u0..]
//  17 distSort [w,h,target,tile...] -> [sorted tiles...]
//  18 distSortUnit [w,h,target,unitTile...] -> [sorted tiles...]
//     (the unit variant's target/unit `tile()` resolution happens in the
//     capture; the numeric comparator core is identical to kind 17)
const utilScenarios = [];
function pushUtil(name, kind, args, res, strs = [], status = 0) {
  utilScenarios.push({ name, kind, args: args.map(uenc), strs, status, res: res.map(uenc) });
}

// manhattanDistWrapped: horizontal wrap on a width-100 torus, vertical never
// wraps, plus same-cell and half-width boundary cases.
{
  const cases = [
    [0, 0, 99, 0, 100],
    [0, 0, 60, 0, 100],
    [10, 20, 90, 25, 100],
    [50, 0, 50, 0, 100],
    [0, 0, 50, 0, 100],
    [30, 70, 70, 30, 100],
    [5, 5, 5, 5, 7],
  ];
  for (const [i, a] of cases.entries()) {
    const [x1, y1, x2, y2, width] = a;
    const d = Util.manhattanDistWrapped({ x: x1, y: y1 }, { x: x2, y: y2 }, width);
    pushUtil(`u_mdw_${i}`, 0, a, [enc(d)]);
  }
}

// within: clamp below, above, inside, and the min>max / NaN edges.
{
  const cases = [
    [5, 0, 10], [-3, 0, 10], [42, 0, 10], [7, 7, 7],
    [1.5, 0, 2], [-0.0, -1, 1], [Number.NaN, 0, 10], [5, 10, 0],
  ];
  for (const [i, [v, lo, hi]] of cases.entries()) {
    pushUtil(`u_within_${i}`, 1, [v, lo, hi], [enc(Util.within(v, lo, hi))]);
  }
}

// simpleHash: ASCII, empty, a long run, BMP non-ASCII, and a surrogate-pair
// emoji (two UTF-16 code units) to pin the charCodeAt iteration.
{
  const strs = ["", "a", "abc", "OpenFront", "hello world", "café", "😀x", "1234567890", "A".repeat(40)];
  for (const [i, s] of strs.entries()) {
    pushUtil(`u_hash_${i}`, 2, [], [enc(Util.simpleHash(s))], [s]);
  }
}

// findMinimumBy closures indexed by the Rust score/candidate kinds.
{
  const scores = [
    (v) => v,
    (v) => Math.abs(v),
    (v) => v % 3,
    (v) => (v === -999 ? Number.NaN : v),
    (v) => -v,
  ];
  const cands = [undefined, (v) => v >= 0, (v) => v !== -999, () => false];
  const valueSets = [
    [4, 1, 3, 2],
    [-5, 3, -1, 7, -2],
    [2, 2, 1, 1],
    [],
    [-999, 5, -3],
    [Number.NaN, 3, 1],
    [7],
    [3, 3, 3],
    [1.5, -2.5, 0.5],
  ];
  let n = 0;
  for (const vs of valueSets) {
    for (let sk = 0; sk < scores.length; sk++) {
      for (let ck = 0; ck < cands.length; ck++) {
        const got = Util.findMinimumBy(vs, scores[sk], cands[ck]);
        const status = got === null ? 1 : 0;
        pushUtil(`u_fmb_${n}`, 3, [sk, ck, ...vs], got === null ? [] : [enc(got)], [], status);
        n++;
      }
    }
  }
}

// getMode: ties keep the earliest-inserted key; zero/negative counts, empty.
{
  const sets = [
    [[1, 3], [2, 5], [3, 5]],
    [[7, 1], [8, 1], [9, 1]],
    [[1, 2], [2, 0]],
    [],
    [[-1, 4], [5, 4], [6, 9]],
    [[2, 1], [1, 2], [3, 2]],
  ];
  for (const [i, pairs] of sets.entries()) {
    const m = new Map(pairs);
    const args = pairs.flat();
    const got = Util.getMode(m);
    pushUtil(`u_mode_${i}`, 4, args, got === null ? [] : [enc(got)], [], got === null ? 1 : 0);
  }
}

// toInt: ±Infinity clamps, NaN throws, floor of fractions and negatives.
{
  const nums = [3.7, -3.2, 0, -0.0, 1 / 3, 2 ** 53, -(2 ** 53), Number.POSITIVE_INFINITY, Number.NEGATIVE_INFINITY, Number.NaN, 42.9, -0.5, 5, 1e15];
  for (const [i, num] of nums.entries()) {
    let status = 0;
    let out;
    try {
      out = Number(Util.toInt(num));
    } catch {
      status = 2;
    }
    pushUtil(`u_toint_${i}`, 5, [num], status === 2 ? [] : [enc(out)], [], status);
  }
}

// maxInt / minInt / withinInt over the safe-integer lattice.
{
  const ab = [[1, 2], [2, 1], [5, 5], [-3, -7], [0, -0.0], [2 ** 53, -(2 ** 53)]];
  for (const [i, [a, b]] of ab.entries()) {
    pushUtil(`u_maxint_${i}`, 6, [a, b], [enc(Number(Util.maxInt(BigInt(Math.floor(a)), BigInt(Math.floor(b)))))]);
    pushUtil(`u_minint_${i}`, 7, [a, b], [enc(Number(Util.minInt(BigInt(Math.floor(a)), BigInt(Math.floor(b)))))]);
  }
  const triples = [[5, 0, 10], [-5, 0, 10], [42, 0, 10], [3, 3, 3], [7, 10, 0]];
  for (const [i, [num, lo, hi]] of triples.entries()) {
    const got = Util.withinInt(BigInt(num), BigInt(lo), BigInt(hi));
    pushUtil(`u_withinint_${i}`, 8, [num, lo, hi], [enc(Number(got))]);
  }
}

// sigmoid: logistic curve, saturation both ways, zero decay, NaN midpoint.
{
  const cases = [
    [0, 1, 0], [5, 1, 0], [-5, 1, 0], [10, 1, 10], [0, 0, 0],
    [100, 0.1, 0], [-100, 0.1, 0], [3, 2, 3], [1, 10, 0], [0, 1, 50],
    [2, -1, 0], [0.5, 0.5, 0.5],
  ];
  for (const [i, [v, k, m]] of cases.entries()) {
    pushUtil(`u_sig_${i}`, 9, [v, k, m], [enc(Util.sigmoid(v, k, m))]);
  }
}

// boundingBoxCenter: even and odd spans (floor toward the min corner).
{
  const boxes = [
    [0, 0, 10, 10], [0, 0, 9, 9], [1, 2, 4, 8], [-3, -3, 3, 3], [0, 0, 0, 0], [5, 5, 6, 7],
  ];
  for (const [i, [mnx, mny, mxx, mxy]] of boxes.entries()) {
    const c = Util.boundingBoxCenter({ min: { x: mnx, y: mny }, max: { x: mxx, y: mxy } });
    pushUtil(`u_bbc_${i}`, 10, [mnx, mny, mxx, mxy], [enc(c.x), enc(c.y)]);
  }
}

// inscribed: the four <=/>= comparisons, exact-fit, and each single violation.
{
  const cases = [
    [[0, 0, 10, 10], [2, 2, 8, 8], true],
    [[0, 0, 10, 10], [0, 0, 10, 10], true],
    [[0, 0, 10, 10], [-1, 0, 8, 8], false],
    [[0, 0, 10, 10], [0, 1, 8, 8], false],
    [[0, 0, 10, 10], [2, 2, 11, 8], false],
    [[0, 0, 10, 10], [2, 2, 8, 11], false],
    [[-5, -5, 5, 5], [-2, -2, 2, 2], true],
  ];
  for (const [i, [o, inr]] of cases.entries()) {
    const got = Util.inscribed({ min: { x: o[0], y: o[1] }, max: { x: o[2], y: o[3] } }, { min: { x: inr[0], y: inr[1] }, max: { x: inr[2], y: inr[3] } });
    pushUtil(`u_insc_${i}`, 11, [...o, ...inr], [enc(got)]);
  }
}

// calculateBoundingBox / calculateBoundingBoxCenter / boundingBoxTiles over a
// real GameMapImpl. Terrain is all-land (magnitude 5); the box/tile math only
// reads x()/y()/ref()/isValidCoord(), which are terrain-independent. container
// kind: 0 Array, 1 Set, 2 TileSet — all three TS branches must agree.
{
  const maps = [
    { w: 5, h: 5 },
    { w: 4, h: 3 },
    { w: 8, h: 2 },
  ];
  const tileSets = [
    [0, 6, 12, 24],
    [1, 2, 3],
    [0, 4, 5, 9, 10, 14, 15, 19, 20, 21, 22, 23],
    [7],
    [2, 8, 13, 3, 17],
  ];
  let n = 0;
  for (const { w, h } of maps) {
    const gm = new GameMapImpl(w, h, new Uint8Array(w * h).fill(0x85), w * h);
    for (const tiles of tileSets) {
      for (const kind of [0, 1, 2]) {
        const container =
          kind === 0 ? tiles.slice() : kind === 1 ? new Set(tiles) : new TileSet(tiles);
        const bb = Util.calculateBoundingBox(gm, container);
        pushUtil(`u_cbb_${n}`, 12, [w, h, kind, ...tiles], [enc(bb.min.x), enc(bb.min.y), enc(bb.max.x), enc(bb.max.y)]);
        n++;
      }
      // Center variant (Array container): boundingBoxCenter(calculateBoundingBox).
      const c = Util.calculateBoundingBoxCenter(gm, tiles.slice());
      pushUtil(`u_cbbc_${n}`, 14, [w, h, ...tiles], [enc(c.x), enc(c.y)]);
      n++;
    }
    // boundingBoxTiles: perimeter square around a center, clipped to the map.
    for (const [center, radius] of [[12, 1], [0, 0], [0, 2], [12, 3], [24, 2], [6, 1], [2, 5]]) {
      const tiles = Util.boundingBoxTiles(gm, center, radius);
      pushUtil(`u_bbt_${n}`, 13, [w, h, center, radius], tiles.map(enc));
      n++;
    }
  }
}

// --- Util.ts remaining pure functions (P42) --------------------------------
// sanitizeClanTag / sanitizeLobbyLabel cross the boundary as UTF-16 code-unit
// token streams `[len, u0, .. ]` (JS charCodeAt units, so lone surrogates and
// surrogate pairs survive the JSON round trip verbatim). distSort /
// distSortUnit replay against a real prepared GameMap (all-land 0x85 —
// manhattanDist is terrain-independent).

// kind 15: sanitizeClanTag.
{
  const u16tok = (s) => {
    const out = [s.length];
    for (let i = 0; i < s.length; i++) out.push(s.charCodeAt(i));
    return out;
  };
  const cases = [
    "", "abc", "hello world!", "ABCDE", "ABCDEF", "a1B2c3D4",
    "\u{1F600}abc", "Ⅷ5x", "abß", "  abc  def  ",
    "\ud800abc", "z".repeat(10), "a-b-c-d-e-f", "9", "OpenFront",
  ];
  for (const [i, s] of cases.entries()) {
    const got = Util.sanitizeClanTag(s);
    pushUtil(`u_sct_${i}`, 15, u16tok(s), u16tok(got));
  }
}

// kind 16: sanitizeLobbyLabel — one case per filter branch, the JS \s
// collapse/trim edges (NBSP / U+2028 / FEFF in, ZWSP out), and the 48-code-
// point cap around surrogate pairs.
{
  const u16tok = (s) => {
    const out = [s.length];
    for (let i = 0; i < s.length; i++) out.push(s.charCodeAt(i));
    return out;
  };
  const cases = [
    "",
    "Europe — Official OpenFront Masters Scrims",
    "a\tb",
    "a\r\nb",
    "a\u000bb",
    "a\u000cb",
    "abc\u001bdef",
    "abc\u007fdef",
    "abc\u0085def",
    "abc\u009fdef",
    "abc\u202adef",
    "abc\u202edef",
    "abc\u2066def",
    "abc\u2069def",
    "abc\u200edef",
    "abc\u200fdef",
    "abc\u061cdef",
    "\u{1F468}\u200D\u{1F469}\u200D\u{1F467} family",
    "a\u00a0\u00a0b",
    "a\u2028b",
    "a\ufeffb",
    "a\u200bb",
    "  hello  ",
    "\u00a0hello\u3000",
    "a\u0000b",
    "\u0000\u0001",
    "x".repeat(60),
    "a".repeat(47) + "\u{1F600}" + "b",
    "a".repeat(48) + "\u{1F600}",
    "a".repeat(46) + "\u{1F600}" + "bc",
    "\t\u000b \u00a0",
    "  a  \n  b  ",
    "  Europe\u2028\u00a0Scrims  ",
  ];
  for (const [i, s] of cases.entries()) {
    const got = Util.sanitizeLobbyLabel(s);
    pushUtil(`u_sll_${i}`, 16, u16tok(s), u16tok(got));
  }
}

// kinds 17/18: distSort / distSortUnit over real GameMapImpls. NaN cases are
// shaped so every comparator result is either 0 or NaN (never a strict sign):
// V8 SortCompare reads NaN as +0 and Rust's Less/Greater/else-Equal chain
// reads it as Equal, so both stable sorts must return the input order —
// pinning the "NaN distance propagates to equal" rule without relying on a
// non-transitive comparator (where V8's binary insertion and Rust's linear
// insertion could legitimately diverge; see exec_util's closestTwoTiles).
{
  const sortCases = [
    // [w, h, target, tiles]
    [6, 5, 14, [0, 5, 29, 12, 17, 3, 26, 14]],
    [6, 5, 0, [0, 1, 5, 6, 25, 29, 14]],
    [6, 5, NaN, [0, 1, 2, 3]],
    [6, 5, 14, [13, 15, 8, 20, 14, 2]],
    [6, 5, 14, [13, NaN, 15, NaN, 8]],
    [10, 10, 55, [0, 9, 90, 99, 55, 1, 45, 5, 50, 60, 49, 61]],
    [6, 5, 14, []],
    [6, 5, 14, [7]],
    [10, 10, 0, [-1, 1, -10, 10, 20]],
  ];
  for (const [i, [w, h, target, tiles]] of sortCases.entries()) {
    const gm = new GameMapImpl(w, h, new Uint8Array(w * h).fill(0x85), w * h);
    const cmp = Util.distSort(gm, target);
    const sorted = tiles.slice().sort(cmp);
    pushUtil(`u_ds_${i}`, 17, [w, h, target, ...tiles], sorted);
  }
  // distSortUnit: target as a plain number or a {tile()} object; units are
  // {tile()} wrappers. The capture resolves everything to refs — the Rust
  // twin replays the identical numeric comparator.
  const unitCases = [
    [6, 5, 14, false, [0, 5, 29, 12]],
    [6, 5, 14, true, [0, 25, 13, 15, 14]],
    [6, 5, NaN, true, [3, NaN, 7]],
    [10, 10, 55, false, [0, 99, 45, 5, 60]],
  ];
  for (const [i, [w, h, target, targetIsUnit, tiles]] of unitCases.entries()) {
    const gm = new GameMapImpl(w, h, new Uint8Array(w * h).fill(0x85), w * h);
    const targetArg = targetIsUnit ? { tile: () => target } : target;
    const units = tiles.map((t) => ({ tile: () => t }));
    const cmp = Util.distSortUnit(gm, targetArg);
    const sorted = units.slice().sort(cmp).map((u) => u.tile());
    pushUtil(`u_dsu_${i}`, 18, [w, h, target, ...tiles], sorted);
  }
}

// --- TeamAssignment scenario runner ------------------------------------------
// Exercises the real TeamAssignment.ts lobby-balancing functions and records
// every observable: the result map's insertion order (player index -> team
// index or -1 for "kicked"), the getMaxTeamSize edges, and resolveTeamsList's
// team lists / throw kinds. The Rust twin is `team_assignment::*`.
//
// kind table (matches the Rust dispatch in parity_structures.rs):
//   0 assignTeams(players, teams, isDuo, maxTeamSize)
//   1 assignTeamsLobbyPreview(players, teams, config, nationCount)
//   2 getMaxTeamSize(numPlayers, numTeams) -> resNums=[max]
//   3 resolveTeamsList(config, totalPlayers) -> resTeams | status 1/2/3
// status: 0 ok; 1 Unknown TeamCountConfig; 2 Too few teams; 3 RangeError.
// player: {id, playerType: "BOT"|"HUMAN"|"NATION", clientID: null|str,
//          clanTag: null|str, friends: [str], teamIndex: null|uenc token}
const teamScenarios = [];
const PT = ["BOT", "HUMAN", "NATION"];
const mkP = (id, t, c = null, k = null, f = [], i = null) => ({
  id,
  playerType: PT[t],
  clientID: c,
  clanTag: k,
  friends: f,
  teamIndex: i,
});
// Encode a player for the vector file: teamIndex keeps the null-vs-NaN
// distinction (JSON would collapse NaN to null).
const encPlayer = (p) => ({
  id: p.id,
  playerType: p.playerType,
  clientID: p.clientID,
  clanTag: p.clanTag,
  friends: p.friends,
  teamIndex: p.teamIndex === null ? null : uenc(p.teamIndex),
});
const configOf = (c) =>
  typeof c === "number"
    ? { kind: 0, num: uenc(c), str: "" }
    : c === "Duos"
      ? { kind: 1, num: 0, str: "" }
      : c === "Trios"
        ? { kind: 2, num: 0, str: "" }
        : c === "Quads"
          ? { kind: 3, num: 0, str: "" }
          : c === "Humans Vs Nations"
            ? { kind: 4, num: 0, str: "" }
            : { kind: 5, num: 0, str: String(c) };

function pushTeamAssign(name, players, teams, isDuo, maxTeamSize) {
  const hasMax = maxTeamSize !== undefined;
  const res = hasMax
    ? TeamAssignment.assignTeams(players, teams, isDuo, maxTeamSize)
    : TeamAssignment.assignTeams(players, teams, isDuo);
  const max = hasMax ? maxTeamSize : TeamAssignment.getMaxTeamSize(players.length, teams.length);
  const pairs = [];
  for (const [p, v] of res.entries()) {
    pairs.push([players.indexOf(p), v === "kicked" ? -1 : teams.indexOf(v)]);
  }
  teamScenarios.push({
    name, kind: 0, players: players.map(encPlayer), teams,
    isDuo: isDuo ? 1 : 0, hasMax: hasMax ? 1 : 0, maxTeamSize: uenc(max),
    nationCount: 0, config: { kind: 0, num: 0, str: "" }, totalPlayers: 0,
    status: 0, res: pairs, resNums: [], resTeams: [],
  });
}

function pushTeamPreview(name, players, teams, config, nationCount) {
  const res = TeamAssignment.assignTeamsLobbyPreview(players, teams, config, nationCount);
  const pairs = [];
  for (const [p, v] of res.entries()) {
    pairs.push([players.indexOf(p), v === "kicked" ? -1 : teams.indexOf(v)]);
  }
  teamScenarios.push({
    name, kind: 1, players: players.map(encPlayer), teams,
    isDuo: 0, hasMax: 0, maxTeamSize: 0, nationCount,
    config: configOf(config), totalPlayers: 0,
    status: 0, res: pairs, resNums: [], resTeams: [],
  });
}

function pushMaxTeamSize(name, n, t) {
  teamScenarios.push({
    name, kind: 2, players: [], teams: [], isDuo: 0, hasMax: 0, maxTeamSize: 0,
    nationCount: 0, config: { kind: 0, num: uenc(n), str: "" }, totalPlayers: uenc(t),
    status: 0, res: [], resNums: [uenc(TeamAssignment.getMaxTeamSize(n, t))], resTeams: [],
  });
}

function pushResolveTeams(name, config, totalPlayers) {
  let status = 0;
  let resTeams = [];
  try {
    resTeams = TeamAssignment.resolveTeamsList(config, totalPlayers);
  } catch (e) {
    const m = String(e && e.message);
    status = /Unknown TeamCountConfig/.test(m) ? 1 : /Too few teams/.test(m) ? 2 : 3;
  }
  teamScenarios.push({
    name, kind: 3, players: [], teams: [], isDuo: 0, hasMax: 0, maxTeamSize: 0,
    nationCount: 0, config: configOf(config), totalPlayers: uenc(totalPlayers),
    status, res: [], resNums: [], resTeams,
  });
}

{
  const T2 = ["Red", "Blue"];
  const T3 = ["Red", "Blue", "Yellow"];
  const T4 = ["Red", "Blue", "Yellow", "Green"];
  const H = (id, ...rest) => mkP(id, 1, ...rest);
  const N = (id, ...rest) => mkP(id, 2, ...rest);
  const B = (id, ...rest) => mkP(id, 0, ...rest);

  // Empty lobby.
  pushTeamAssign("t_empty", [], T2, false, undefined);

  // Plain humans, default max: even split, odd player lands on the first
  // minimum team.
  pushTeamAssign("t_even4", [H("a"), H("b"), H("c"), H("d")], T2, false, undefined);
  pushTeamAssign("t_odd5", [H("a"), H("b"), H("c"), H("d"), H("e")], T2, false, undefined);
  pushTeamAssign("t_4x3", [H("a"), H("b"), H("c"), H("d")], T3, false, undefined);

  // Server-pinned slots seed the counts the balancer sees.
  pushTeamAssign("t_pins", [
    H("a", null, null, [], 1), H("b", null, null, [], 1),
    H("c"), H("d"),
  ], T2, false, undefined);
  // Out-of-range / negative / fractional / NaN pins leave the player unpinned
  // (JS `teams[i]` is undefined for all of them).
  pushTeamAssign("t_pin_oob", [H("a", null, null, [], 5), H("b")], T2, false, undefined);
  pushTeamAssign("t_pin_neg", [H("a", null, null, [], -1), H("b")], T2, false, undefined);
  pushTeamAssign("t_pin_frac", [H("a", null, null, [], 1.5), H("b")], T2, false, undefined);
  pushTeamAssign("t_pin_nan", [H("a", null, null, [], Number.NaN), H("b")], T2, false, undefined);
  pushTeamAssign("t_pin_zero", [H("a", null, null, [], 0), H("b")], T2, false, undefined);

  // Clans: all-or-nothing with overflow kicks; equal-size clans keep
  // first-seen order (stable sort); a clan goes to the emptiest team.
  pushTeamAssign("t_clan_overflow", [
    H("a", "ca", "X"), H("b", "cb", "X"), H("c", "cc", "X"), H("d", "cd", "X"), H("e"),
  ], T2, false, undefined);
  pushTeamAssign("t_clan_ties", [
    H("a", "ca", "X"), H("b", "cb", "X"),
    H("c", "cc", "Y"), H("d", "cd", "Y"),
  ], T2, false, undefined);
  pushTeamAssign("t_clan_seed", [
    H("a", null, null, [], 1),
    H("b", "cb", "X"), H("c", "cc", "X"),
  ], T2, false, undefined);
  // Empty clanTag is falsy: the player is treated as clanless.
  pushTeamAssign("t_clan_empty_tag", [H("a", "ca", ""), H("b", "cb", "")], T2, false, undefined);
  // Clan when every team is already at max: the *first* team is still
  // selected (teamSize stays >= max) and the whole clan is kicked.
  pushTeamAssign("t_clan_allfull", [
    H("a", null, null, [], 0), H("b", null, null, [], 1),
    H("c", "cc", "X"), H("d", "cd", "X"),
  ], T2, false, 1);

  // Friends: soft preference, spill when full, absent IDs ignored, null
  // clientIDs never form edges, edges are bidirectional.
  pushTeamAssign("t_friends_attract", [
    H("a", "ca", null, ["cb"]), H("b", "cb", null, [], 2), H("c"),
  ], T3, false, undefined);
  pushTeamAssign("t_friends_full_spill", [
    H("a", "ca", null, ["cb"]), H("b", "cb", null, [], 1), H("c"),
  ], T2, false, 1);
  pushTeamAssign("t_friends_absent", [
    H("a", "ca", null, ["ghost"]), H("b"),
  ], T2, false, undefined);
  pushTeamAssign("t_friends_nullclient", [
    H("a", null, null, ["cb", "ca"]), H("b", "cb"),
  ], T2, false, undefined);
  pushTeamAssign("t_friends_bidir", [
    H("a", "ca", null, [], 1), H("b", "cb", null, ["ca"]), H("c"),
  ], T2, false, undefined);

  // Duos/Trios/Quads mode: bestSize starts at -1 and prefers the *largest*
  // non-full team, so pairs form before singles spread.
  pushTeamAssign("t_duos", [H("a"), H("b"), H("c"), H("d"), H("e")], T2, true, undefined);
  pushTeamAssign("t_duos_pins", [
    H("a", null, null, [], 0), H("b", null, null, [], 0),
    H("c"), H("d"), H("e"),
  ], T2, true, undefined);

  // Nations: shuffled once (seeded by simpleHash of the first nation's id)
  // and placed *after* every other player.
  pushTeamAssign("t_nations", [
    N("n1", "cn1"), N("n2", "cn2"), N("n3", "cn3"), H("h1"),
  ], T2, false, undefined);
  pushTeamAssign("t_nations_nullclient", [N("n1"), N("n2"), H("h1")], T2, false, undefined);
  // Non-ASCII ids exercise the UTF-16 simpleHash seed of the shuffle.
  pushTeamAssign("t_nations_unicode", [
    N("π-ν1"), N("🚀n2"), N("n3"), N("n4"),
  ], T3, false, undefined);
  pushTeamAssign("t_bots", [B("b1"), B("b2"), H("h1")], T2, false, undefined);

  // Everything at once: pins + clan + friends + nations.
  pushTeamAssign("t_mixed", [
    H("a", "ca", "X"), H("b", "cb", "X"),
    H("c", "cc", null, ["cd"]), H("d", "cd", null, [], 0),
    N("n1", "cn1"), N("n2", "cn2"), H("e"),
  ], T4, false, undefined);

  // Duplicate team names: the Map keys merge by string equality, so both
  // slots share one count.
  pushTeamAssign("t_dupnames", [H("a"), H("b"), H("c")], ["Red", "Blue", "Red"], false, undefined);

  // No teams at all: nothing to pin onto, clans are skipped, everyone is
  // kicked (placePlayer finds no candidate).
  pushTeamAssign("t_no_teams", [H("a"), H("b", "cb", "X")], [], false, undefined);

  // maxTeamSize edges.
  pushTeamAssign("t_kick_all", [H("a"), H("b")], T2, false, 0);
  pushTeamAssign("t_max_inf", [H("a"), H("b"), H("c")], T2, false, Number.POSITIVE_INFINITY);

  // Lobby preview: maxTeamSize counts the incoming nations, and only
  // Duos/Trios/Quads flip the fill-preference mode.
  pushTeamPreview("t_prev_duos", [H("a"), H("b"), H("c"), H("d"), H("e")], T2, "Duos", 3);
  pushTeamPreview("t_prev_hvn", [H("a"), H("b"), H("c")], ["Humans", "Nations"], "Humans Vs Nations", 5);
  pushTeamPreview("t_prev_num", [H("a"), H("b"), H("c"), H("d")], T4, 4, 0);
  pushTeamPreview("t_prev_other", [H("a"), H("b"), H("c")], T2, "Fives", 1);
  pushTeamPreview("t_prev_zero_nations", [H("a"), H("b")], T2, "Quads", 0);

  // getMaxTeamSize edges: exact, rounding up, zero divisor (Infinity), 0/0
  // (NaN), and the -0 result of ceil(-0.5).
  pushMaxTeamSize("t_max_10_2", 10, 2);
  pushMaxTeamSize("t_max_5_2", 5, 2);
  pushMaxTeamSize("t_max_0_0", 0, 0);
  pushMaxTeamSize("t_max_1_0", 1, 0);
  pushMaxTeamSize("t_max_neg1_2", -1, 2);
  pushMaxTeamSize("t_max_10_3", 10, 3);

  // resolveTeamsList: every branch, both throws, the fractional / NaN /
  // Infinity length edges.
  pushResolveTeams("t_res_hvn", "Humans Vs Nations", 10);
  for (const n of [2, 3, 4, 5, 6, 7, 8, 9, 20]) pushResolveTeams(`t_res_num${n}`, n, 0);
  pushResolveTeams("t_res_num0", 0, 10);
  pushResolveTeams("t_res_num1", 1, 10);
  pushResolveTeams("t_res_numneg", -5, 10);
  pushResolveTeams("t_res_numnan", Number.NaN, 10);
  pushResolveTeams("t_res_numinf", Number.POSITIVE_INFINITY, 10);
  pushResolveTeams("t_res_num8_5", 8.5, 0);
  pushResolveTeams("t_res_duos0", "Duos", 0);
  pushResolveTeams("t_res_duos5", "Duos", 5);
  pushResolveTeams("t_res_trios7", "Trios", 7);
  pushResolveTeams("t_res_quads9", "Quads", 9);
  pushResolveTeams("t_res_quads0", "Quads", 0);
  pushResolveTeams("t_res_duosnan", "Duos", Number.NaN);
  pushResolveTeams("t_res_other", "Fives", 10);
  pushResolveTeams("t_res_emptystr", "", 10);
}

// --- Bezier (Line.ts) scenario runner ----------------------------------------
// Exercises the real DistanceBasedBezierCurve: getLength (pure static), and
// the constructor's computeAllPoints followed by a scripted increment walk.
// Every observable is recorded: the full cached-point list (order included),
// each increment's returned point (or -1 sentinel for null), and the final
// currentIndex. The Rust twin is `line::DistanceBasedBezierCurve`.
const bezierScenarios = [];
const P = (x, y) => ({ x, y });

function pushBezierLength(name, cp) {
  const [a, b, c, d] = cp;
  const len = DistanceBasedBezierCurve.getLength(a, b, c, d);
  bezierScenarios.push({
    name, kind: 0, cp: cp.map((p) => [uenc(p.x), uenc(p.y)]).flat(),
    spacing: 0, incs: [], len: uenc(len), points: [], walk: [], finalIndex: 0,
  });
}

function pushBezierWalk(name, cp, spacing, incs) {
  const [a, b, c, d] = cp;
  const curve = new DistanceBasedBezierCurve(a, b, c, d, spacing);
  const points = curve.getAllPoints().map((p) => [p.x, p.y]).flat();
  const walk = [];
  for (const dist of incs) {
    const p = curve.increment(dist);
    walk.push(p === null ? [-1, 0, 0] : [curve.getCurrentIndex(), p.x, p.y]);
  }
  bezierScenarios.push({
    name, kind: 1, cp: cp.map((p) => [uenc(p.x), uenc(p.y)]).flat(),
    spacing: uenc(spacing), incs: incs.map(uenc), len: 0,
    points: points.map(uenc), walk: walk.map((w) => w.map(uenc)),
    finalIndex: curve.getCurrentIndex(),
  });
}

{
  // Straight degenerate curves and collinear shots.
  pushBezierLength("b_len_point", [P(0, 0), P(0, 0), P(0, 0), P(0, 0)]);
  pushBezierLength("b_len_straight", [P(0, 0), P(33, 0), P(66, 0), P(100, 0)]);
  pushBezierLength("b_len_diag", [P(0, 0), P(20, 20), P(60, 60), P(100, 100)]);
  pushBezierLength("b_len_curve", [P(0, 0), P(100, 0), P(100, 100), P(0, 100)]);
  pushBezierLength("b_len_neg", [P(-50.5, -20.5), P(-10.5, -40.25), P(5.5, -5.5), P(30.5, 15.75)]);
  // Half-up rounding edges: .5 rounds toward +Infinity, so -0.5 -> -0 and
  // 0.5 -> 1 (the -0 survives the * 256 scale as -0).
  pushBezierLength("b_len_halfs", [P(-0.5, 0.5), P(1.5, -1.5), P(2.5, -2.5), P(3.5, 4.5)]);
  // 32-bit overflow edges: 8388608 * 256 = 2^31 wraps ToInt32 to -2^31, so
  // the >> 8 emission lands on negative pixel coordinates.
  pushBezierLength("b_len_wrap", [P(8388608, 0), P(8388608, 1), P(8388607, 0), P(0, 0)]);
  // NaN / Infinity control points: dist comparisons go false, the recursion
  // runs to the depth cap, and the accumulator poisons exactly like JS.
  pushBezierLength("b_len_nan", [P(Number.NaN, 0), P(50, 50), P(50, 0), P(100, 0)]);
  pushBezierLength("b_len_inf", [P(0, 0), P(Number.POSITIVE_INFINITY, 50), P(50, 50), P(100, 100)]);

  // Walk scenarios: the cached-point list plus an increment script.
  pushBezierWalk("b_walk_straight", [P(0, 0), P(33, 0), P(66, 0), P(100, 0)], 1, [1, 1, 2, 5, 100, 1]);
  pushBezierWalk("b_walk_curve", [P(0, 0), P(100, 0), P(100, 100), P(0, 100)], 2, [1, 1, 1, 3, 3, 10, 10, 1000]);
  pushBezierWalk("b_walk_spacing0", [P(0, 0), P(20, 40), P(80, 60), P(100, 100)], 0, [1, 1, 1]);
  pushBezierWalk("b_walk_spacing_nan", [P(0, 0), P(20, 40), P(80, 60), P(100, 100)], Number.NaN, [1, 1]);
  pushBezierWalk("b_walk_inc_nan", [P(0, 0), P(30, 0), P(60, 0), P(90, 0)], 1, [Number.NaN, 1, 1]);
  pushBezierWalk("b_walk_inc_neg", [P(0, 0), P(30, 0), P(60, 0), P(90, 0)], 1, [-5, 0.2, 0.6, 0.5, -0.5]);
  pushBezierWalk("b_walk_fractional", [P(0, 0), P(10.5, 20.25), P(70.75, 30.5), P(90, 60)], 0.5, [0.001, 0.004, 1.5, 2.5, -1.5, 0.49999999999999994]);
  pushBezierWalk("b_walk_point", [P(5, 5), P(5, 5), P(5, 5), P(5, 5)], 1, [1, 1]);
  pushBezierWalk("b_walk_neg_coords", [P(-40.5, -30.5), P(-20.5, -10.25), P(0.5, 10.5), P(20.5, 30.75)], 1, [1, 2, 4, 8]);
  // Negative spacing clamps to SUB_SCALE via Math.max; the 2^31 ToInt32 wrap
  // of the midpoint shifts is pinned by b_len_wrap (getLength only — a walk
  // over wrapped midpoints accumulates ~2^31 distance per leaf and emits
  // millions of points).
  pushBezierWalk("b_walk_wrap", [P(10, 0), P(10, 20), P(20, 20), P(20, 0)], -5, [1, 2, 3]);
  pushBezierWalk("b_walk_big", [P(0, 0), P(500, 0), P(500, 500), P(0, 500)], 16, [16, 16, 16, 48, 16, 1e9]);
}

// --- Veterancy (Veterancy.ts) ------------------------------------------------
// maxHealthWithVeterancy is pure; record the (base, veterancy, percent) triple
// and the returned number. NaN / -0 / Infinity inputs pin the branch that the
// `veterancy <= 0` guard selects and the floor of a poisoned product.
const veterancyScenarios = [];
function pushVeterancy(name, base, vet, pct) {
  const res = maxHealthWithVeterancy(base, vet, pct);
  veterancyScenarios.push({
    name, base: uenc(base), vet: uenc(vet), pct: uenc(pct), res: uenc(res),
  });
}
{
  pushVeterancy("v_basic", 100, 2, 10);
  pushVeterancy("v_floor", 33, 1, 10);
  pushVeterancy("v_zero", 50, 0, 10);
  pushVeterancy("v_neg", 50, -1, 10);
  pushVeterancy("v_negzero", 50, -0, 10);
  pushVeterancy("v_nan_vet", 100, Number.NaN, 10);
  pushVeterancy("v_nan_base", Number.NaN, 2, 10);
  pushVeterancy("v_nan_pct", 100, 2, Number.NaN);
  pushVeterancy("v_frac", 10, 1, 33.3);
  pushVeterancy("v_half", 100, 1, 0.5);
  pushVeterancy("v_pct0", 100, 5, 0);
  pushVeterancy("v_negpct", 100, 2, -10);
  pushVeterancy("v_inf_vet", 100, Number.POSITIVE_INFINITY, 10);
  pushVeterancy("v_inf_base", Number.POSITIVE_INFINITY, 2, 10);
  pushVeterancy("v_ninf_base", Number.NEGATIVE_INFINITY, 2, 10);
  pushVeterancy("v_big", 1e15, 3, 7);
}

// --- MotionPlans (MotionPlans.ts) -------------------------------------------
// Record encoding shared with the wasm probe:
//   [count, per record: 1,unitId,planId,startTick,ticksPerStep,pathLen,path...
//                      | 2,engineUnitId,planId,startTick,speed,spacing,
//                        carCount,pathLen,cars...,path...]
// kind 0 = roundtrip: pack `input` -> `words` ([len, ...packed]) -> unpack ->
// `out`. kind 1 = unpack-only: feed `words` ([len, ...raw]) -> `out`.
const mpScenarios = [];
const G = (unitId, planId, startTick, ticksPerStep, path) =>
  ({ kind: "grid", unitId, planId, startTick, ticksPerStep, path });
const T = (engineUnitId, carUnitIds, planId, startTick, speed, spacing, path) =>
  ({ kind: "train", engineUnitId, carUnitIds, planId, startTick, speed, spacing, path });
const encRec = (r) =>
  r.kind === "grid"
    ? [1, r.unitId, r.planId, r.startTick, r.ticksPerStep, r.path.length, ...r.path]
    : [2, r.engineUnitId, r.planId, r.startTick, r.speed, r.spacing,
       r.carUnitIds.length, r.path.length, ...r.carUnitIds, ...r.path];
const encRecs = (rs) => [rs.length, ...rs.map(encRec).flat()];

function pushMpRoundtrip(name, records) {
  const packed = Array.from(packMotionPlans(records));
  const unpacked = unpackMotionPlans(new Uint32Array(packed));
  mpScenarios.push({
    name, kind: 0,
    input: encRecs(records).map(uenc),
    words: [packed.length, ...packed].map(uenc),
    out: encRecs(unpacked).map(uenc),
  });
}
function pushMpUnpack(name, rawWords) {
  const unpacked = unpackMotionPlans(new Uint32Array(rawWords));
  mpScenarios.push({
    name, kind: 1,
    input: [],
    words: [rawWords.length, ...rawWords].map(uenc),
    out: encRecs(unpacked).map(uenc),
  });
}
{
  pushMpRoundtrip("mp_rt_empty", []);
  pushMpRoundtrip("mp_rt_grid", [G(10, 20, 30, 40, [100, 101, 102])]);
  pushMpRoundtrip("mp_rt_grid_clamp", [G(-1, 4294967296, 2.7, -0.5, [5, 6, 7, 8])]);
  pushMpRoundtrip("mp_rt_train", [T(1, [7, 8, 9], 2, 3, 4, 5, [50, 51, 52])]);
  pushMpRoundtrip("mp_rt_train_nocars", [T(1, [], 2, 3, 4, 5, [9])]);
  pushMpRoundtrip("mp_rt_mixed", [
    G(1, 2, 3, 4, [10, 11]),
    T(5, [6, 7], 8, 9, 10, 11, [20, 21, 22]),
    G(30, 40, 50, 60, []),
  ]);
  pushMpRoundtrip("mp_rt_big", [
    T(1, Array.from({ length: 8 }, (_, i) => i + 100), 2, 3, 4, 5,
      Array.from({ length: 12 }, (_, i) => 200 + i)),
  ]);
  pushMpUnpack("mp_up_empty", []);
  pushMpUnpack("mp_up_trunc", [1, 1, 7]);
  pushMpUnpack("mp_up_wc_low", [1, 1, 1]);
  pushMpUnpack("mp_up_wc_over", [1, 1, 99]);
  pushMpUnpack("mp_up_unknown", [1, 99, 2, 0, 0]);
  pushMpUnpack("mp_up_grid_mismatch", [1, 1, 9, 1, 2, 3, 4, 7, 0, 0]);
  pushMpUnpack("mp_up_train_mismatch", [1, 2, 9, 1, 2, 3, 4, 5, 6, 7, 0, 0]);
  pushMpUnpack("mp_up_two_ok", [2, 1, 7, 5, 6, 7, 8, 0, 1, 7, 9, 10, 11, 12, 0]);
}

// --- ConnectedComponents (ConnectedComponents.ts) ---------------------------
// Op-stream replay over the real class, driven by a real GameMapImpl (packed
// terrain bytes; bit 7 = land). The class exposes initialize(), the
// incremental addWaterTiles(), the two queries (getComponentId /
// getComponentSize), and the internal buffers the trace pins: componentIds
// (Uint8Array or Uint16Array after the 253-component upgrade), the sparse
// _componentSizes (JS holes -> "u"), parents (union-find, mutated by path
// compression), maxId, landMarker.
//
// kind: 0=initialize "v", 1=addWaterTiles(a) "v", 2=getComponentId(a) -> Val,
//       3=getComponentSize(a) -> Val
const ccScenarios = [];
function runCc(name, w, h, terrain, direct, ops) {
  const gm = new GameMapImpl(w, h, Uint8Array.from(terrain), 0);
  const cc = new ConnectedComponents(gm, direct);
  const played = ops.map(([k, a]) => {
    let res;
    switch (k) {
      case 0: cc.initialize(); res = "v"; break;
      case 1: cc.addWaterTiles([a]); res = "v"; break;
      case 2: res = enc(cc.getComponentId(a)); break;
      case 3: res = enc(cc.getComponentSize(a)); break;
      default: throw new Error("bad cc op kind " + k);
    }
    return [k, enc(a === undefined ? 0 : a), 0, res];
  });
  const ids = cc.componentIds;
  ccScenarios.push({
    name,
    w,
    h,
    terrain: Array.from(terrain),
    direct: direct ? 1 : 0,
    ops: played,
    // 0 = not yet created (null), 8 = Uint8Array, 16 = Uint16Array.
    bits: ids === null ? 0 : ids instanceof Uint16Array ? 16 : 8,
    ids: ids === null ? [] : Array.from(ids),
    // _componentSizes is a sparse JS array (holes for unused ids); Array.from
    // iterates by index so each hole becomes enc(undefined) -> "u", unlike
    // .map() which skips holes and leaves the result sparse.
    sizes: Array.from(cc._componentSizes, (v) => enc(v)),
    parents: Array.from(cc.parents, (v) => enc(v)),
    maxId: enc(cc.maxId),
    landMarker: enc(cc.landMarker),
  });
}

const CW = 0x00; // water
const CL = 0x85; // land, magnitude 5

// Two blobs separated by a land wall: labels, sizes, and the land marker.
runCc("cc_two_blobs", 3, 2, [CW, CW, CL, CW, CW, CL], true, [
  [0], [2, 0], [2, 1], [2, 2], [2, 3], [3, 1], [3, 2],
]);
// The premark paths must agree byte-for-byte: same map, direct vs isWater.
runCc("cc_iter_premark", 3, 2, [CW, CW, CL, CW, CW, CL], false, [
  [0], [2, 0], [2, 1], [2, 2], [2, 3], [3, 1], [3, 2],
]);
// Queries before initialize(): getComponentId is 0, addWaterTiles no-ops.
runCc("cc_pre_init", 2, 2, [CL, CW, CW, CW], true, [
  [2, 0], [1, 0], [2, 0], [0], [2, 0], [3, 1],
]);
// Bridging: two components merge through one added land tile; the union-find
// alias, the moved size, and the zeroed old size are all pinned.
runCc("cc_bridge", 3, 2, [CW, CL, CW, CW, CL, CW], false, [
  [0], [1, 1], [2, 1], [2, 2], [3, 1], [3, 2],
]);
// Isolated crater: an all-land map, then one interior tile converts -> a
// fresh component id (allocComponentId path).
runCc("cc_crater", 3, 3, [CL, CL, CL, CL, CL, CL, CL, CL, CL], true, [
  [0], [1, 4], [2, 4], [3, 2], [2, 0],
]);
// A vertical wall bridges the top and bottom water: one added tile joins the
// four cardinal neighbours' roots (two distinct components) into the
// canonical (smallest) root.
runCc("cc_multi_merge", 3, 3, [CW, CL, CW, CW, CL, CW, CW, CL, CW], true, [
  [0], [1, 1], [2, 0], [2, 2], [2, 4], [3, 1], [3, 2],
]);
// Edge guards: adding a tile in the first/last row and first/last column
// exercises the four boundary conditions of the neighbour collection.
runCc("cc_edges", 4, 2, [CL, CL, CL, CL, CL, CL, CL, CL], true, [
  [0], [1, 0], [1, 3], [1, 4], [1, 7], [1, 1], [2, 0], [2, 1], [3, 1],
]);
// Double add of the same tile: the second is a no-op (already water).
runCc("cc_double_add", 2, 1, [CL, CW], true, [
  [0], [1, 0], [1, 0], [2, 0], [3, 1],
]);
// Invalid refs: out-of-range / fractional / negative tiles are skipped by
// the land-marker test (undefined !== marker) and read as 0.
runCc("cc_invalid_refs", 2, 2, [CW, CW, CW, CW], true, [
  [0], [1, 9], [1, 1.5], [1, -1], [2, 9], [2, 1.5], [2, -1], [3, 0], [3, -1],
  [3, 99999], [3, 0.5],
]);
// Path compression: a merge chain (parents[r] = canon) makes find() walk and
// rewrite parents; the final parents buffer is the observable.
runCc("cc_chain", 5, 1, [CW, CL, CW, CL, CW], false, [
  [0], [1, 1], [1, 3], [2, 0], [2, 2], [2, 4], [3, 3], [3, 1],
]);
// The Uint8Array -> Uint16Array upgrade: 253 isolated single-tile water
// pockets on a 13x40 map (row 0 all water, rows 1-2 land, then alternating
// water columns). Simpler: a 1x254 map with land at every odd index gives
// 127 components... instead use a 254x2 map whose first row is 254 isolated
// water tiles separated by single land tiles: 127 blobs, not enough. Use
// 2x128: no. The direct construction: 128x2, row0 = W,L,W,L,... row1 all L
// -> 64 components. To exceed 253 cheaply: 16x32 with a checkerboard of
// water/land -> 256 water tiles, each isolated -> 256 components, forcing
// the upgrade at 253 and the 0xFFFF break at 65535.
{
  const w = 16;
  const h = 32;
  const terrain = [];
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++)
      terrain.push((x + y) % 2 === 0 ? CW : CL);
  runCc("cc_upgrade_16", w, h, terrain, true, [
    [0], [2, 0], [2, 1], [2, 16], [3, 1], [3, 253], [3, 254], [3, 255],
    [3, 256], [1, 1], [2, 1], [3, 1],
  ]);
}

// --- TerrainSearchMap (TerrainSearchMap.ts) ---------------------------------
// The class reads width/height from the buffer's first 4 bytes
// (little-endian pairs) and classifies the packed byte at 4 + y*width + x:
// bit 7 -> Land, magnitude < 10 -> Shore, else Water. Out-of-range reads hit
// `undefined`, whose `& 0x80` / `& 0x1f` are 0 -> Shore; header bytes at
// indices < 4 are read like any tile. neighbors() keeps fractional
// coordinates (the bounds test is relational) and drops NaN/Infinity.
//
// kind: 0=getWidth, 1=getHeight, 2=node(x,y) -> Val,
//       3=neighbors(x,y) -> [x0,y0,x1,y1,...]
const tsmScenarios = [];
function runTsm(name, w, h, tiles, ops) {
  const buf = new Uint8Array(4 + tiles.length);
  buf[0] = w & 0xff;
  buf[1] = (w >> 8) & 0xff;
  buf[2] = h & 0xff;
  buf[3] = (h >> 8) & 0xff;
  buf.set(tiles, 4);
  const tsm = new TerrainSearchMap(buf.buffer);
  const played = ops.map(([k, a, b]) => {
    if (k === 0) return [k, 0, 0, enc(tsm.getWidth())];
    if (k === 1) return [k, 0, 0, enc(tsm.getHeight())];
    if (k === 2) return [k, uenc(a), uenc(b), enc(tsm.node(a, b))];
    if (k === 3) {
      const ns = tsm.neighbors(a, b);
      const flat = [];
      for (const n of ns) {
        flat.push(uenc(n.x), uenc(n.y));
      }
      return [k, uenc(a), uenc(b), flat];
    }
    throw new Error("bad tsm op kind " + k);
  });
  tsmScenarios.push({
    name,
    buffer: Array.from(buf),
    ops: played,
  });
}

// 3x2 map covering every classification branch plus the out-of-range and
// header-byte reads. Bytes: land(0x85), shore(0x09), water(0x0a),
// water-mag31(0x1f), land-mag31(0x9f), shore-mag0(0x00).
runTsm("tsm_basic", 3, 2, [0x85, 0x09, 0x0a, 0x1f, 0x9f, 0x00], [
  [0], [1],
  [2, 0, 0], [2, 1, 0], [2, 2, 0], [2, 0, 1], [2, 1, 1], [2, 2, 1],
  // out of range -> undefined byte -> Shore
  [2, 3, 0], [2, -1, 0], [2, 0, 2], [2, 0, -1], [2, 99, 99],
  // fractional coords: index 4+0*3+1.5 = 5.5 -> undefined -> Shore;
  // index lands inside the header: node(-4+?,..) style below
  [2, 1.5, 0], [2, 0.5, 0.5],
  // header bytes read as tiles: idx 0..3 -> buffer[0..3] = 3,0,2,0
  [2, -4, 0], [2, -3, 0], [2, -2, 0], [2, -1.5, 0],
]);
// The magnitude-10 boundary and bit-7 dominance: 0x09 Shore vs 0x0a Water,
// 0x80 Land with magnitude 0, 0x9f Land with magnitude 31 (bit 7 wins).
runTsm("tsm_magnitude", 4, 1, [0x09, 0x0a, 0x80, 0x9f], [
  [2, 0, 0], [2, 1, 0], [2, 2, 0], [2, 3, 0],
]);
// neighbors(): interior, all four corners, and edge centers pin the dirs
// order and the bounds test.
runTsm("tsm_neighbors", 3, 3, new Array(9).fill(0x00), [
  [3, 1, 1], [3, 0, 0], [3, 2, 0], [3, 0, 2], [3, 2, 2],
  [3, 1, 0], [3, 0, 1], [3, 2, 1], [3, 1, 2],
]);
// Fractional coordinates pass the relational bounds test; NaN and the
// infinities fail every comparison -> empty list.
runTsm("tsm_frac_neighbors", 3, 3, new Array(9).fill(0x00), [
  [3, 1.5, 1.5], [3, 0.5, 0.5], [3, 2.5, 2.5],
  [3, NaN, 1], [3, 1, NaN], [3, Infinity, Infinity], [3, -Infinity, 0],
  [2, NaN, NaN], [2, Infinity, 0], [2, -0, 0],
]);
// A 2-byte buffer: width decodes from the present bytes, height from
// `undefined` reads -> 0; every node/neighbor query then hits undefined.
{
  const buf = new Uint8Array([3, 0]);
  const tsm = new TerrainSearchMap(buf.buffer);
  const ops = [
    [0], [1], [2, 0, 0], [2, -4, 0], [2, -2, 0], [3, 0, 0],
  ].map(([k, a, b]) => {
    if (k === 0) return [k, 0, 0, enc(tsm.getWidth())];
    if (k === 1) return [k, 0, 0, enc(tsm.getHeight())];
    if (k === 2) return [k, uenc(a), uenc(b), enc(tsm.node(a, b))];
    const ns = tsm.neighbors(a, b);
    return [k, uenc(a), uenc(b), ns.flatMap((n) => [uenc(n.x), uenc(n.y)])];
  });
  tsmScenarios.push({ name: "tsm_short_header", buffer: Array.from(buf), ops });
}
// Width/height above one byte exercise the (d[1]<<8)|d[0] decode; node reads
// at negative coords land on header bytes (0x34 -> Water, 0x12 -> Water).
runTsm("tsm_wide_header", 0x1234, 0x00ff, [0x85], [
  [0], [1], [2, 0, 0], [2, -4, 0], [2, -3, 0],
]);

// --- AbstractGraph (AbstractGraph.ts) ---------------------------------------
// Build the coarse navigation graph over a real GameMapImpl with the real
// AbstractGraphBuilder, then replay the container's accessors against the
// built graph and pin the final internal arrays. The builder's ordering
// decisions (cluster scan order, span mid-points, BFS find order -> edge ids,
// canonical (lo,hi) dedup, clean-cluster cache) are the parity target.
//
// op kinds: 0=nodeCount, 1=edgeCount, 2=getNode(a), 3=getEdge(a),
//   4=getNodeEdges(a), 5=getEdgeBetween(a,b), 6=getOtherNode(edgeId=a,node=b),
//   7=getClusterKey(a,b), 8=getCluster(a=cx,b=cy), 9=getClusterNodes,
//   10=getNearbyClusterNodes, 11=getComponentId(a), 12=getComponentSize(a),
//   13=getCachedPath(a=edgeId,b=fromNodeId), 14=setCachedPath(a,b) -> "v".
// Node results flatten to [id,x,y,tile,componentId]; edge results to
// [id,nodeA,nodeB,cost,clusterX,clusterY]; cluster to [x,y,count,ids...].
const agScenarios = [];
const agGraphs = new Map(); // name -> built graph, for partial-rebuild oldGraph
const agNode = (n) =>
  n === undefined ? "u" : [enc(n.id), enc(n.x), enc(n.y), enc(n.tile), enc(n.componentId)];
const agEdge = (e) =>
  e === undefined
    ? "u"
    : [enc(e.id), enc(e.nodeA), enc(e.nodeB), enc(e.cost), enc(e.clusterX), enc(e.clusterY)];
const agCluster = (c) =>
  c === undefined ? "u" : [enc(c.x), enc(c.y), enc(c.nodeIds.length), ...c.nodeIds.map(enc)];
const agFlat = (xs) => xs.map((x) => enc(x));

function runAg(name, w, h, clusterSize, terrain, ops, oldName, dirtyTiles) {
  const gm = new GameMapImpl(w, h, Uint8Array.from(terrain), 0);
  let oldGraph = null;
  if (oldName) {
    oldGraph = agGraphs.get(oldName);
    if (!oldGraph) throw new Error("ag: unknown old graph " + oldName);
  }
  let builder;
  if (oldGraph) {
    builder = new AbstractGraphBuilder(gm, clusterSize, oldGraph, new Set(dirtyTiles));
  } else {
    builder = new AbstractGraphBuilder(gm, clusterSize);
  }
  const graph = builder.build();

  const played = ops.map(([k, a, b]) => {
    switch (k) {
      case 0: return [k, 0, 0, enc(graph.nodeCount)];
      case 1: return [k, 0, 0, enc(graph.edgeCount)];
      case 2: return [k, enc(a), 0, agNode(graph.getNode(a))];
      case 3: return [k, enc(a), 0, agEdge(graph.getEdge(a))];
      case 4: return [k, enc(a), 0, graph.getNodeEdges(a).flatMap((e) => agEdge(e))];
      case 5: return [k, enc(a), enc(b), agEdge(graph.getEdgeBetween(a, b))];
      case 6: {
        const e = graph.getEdge(a);
        if (!e) return [k, enc(a), enc(b), "t"];
        return [k, enc(a), enc(b), enc(graph.getOtherNode(e, b))];
      }
      case 7: return [k, enc(a), enc(b), enc(graph.getClusterKey(a, b))];
      case 8: return [k, enc(a), enc(b), agCluster(graph.getCluster(a, b))];
      case 9: return [k, enc(a), enc(b), graph.getClusterNodes(a, b).flatMap((n) => agNode(n))];
      case 10:
        return [k, enc(a), enc(b), graph.getNearbyClusterNodes(a, b).flatMap((n) => agNode(n))];
      case 11: return [k, enc(a), 0, enc(graph.getComponentId(a))];
      case 12: return [k, enc(a), 0, enc(graph.getComponentSize(a))];
      case 13: {
        const p = graph.getCachedPath(a, b);
        return [k, enc(a), enc(b), p === null ? "u" : agFlat(p)];
      }
      case 14: {
        const e = graph.getEdge(a);
        if (!e) return [k, enc(a), enc(b), "v"];
        const dir = b === e.nodeA ? 0 : 1;
        graph.setCachedPath(a, b, [a, b, a * 2 + dir]);
        return [k, enc(a), enc(b), "v"];
      }
      default: throw new Error("bad ag op kind " + k);
    }
  });

  // Final internal arrays (all dense — ids/keys are sequential).
  const nodes = [];
  for (const n of graph._nodes) nodes.push(...agNode(n));
  const edges = [];
  for (const e of graph._edges) edges.push(...agEdge(e));
  const clusters = [];
  for (const c of graph._clusters) clusters.push(...agCluster(c));
  const nodeEdgeIds = [];
  for (const list of graph._nodeEdgeIds) nodeEdgeIds.push(enc(list.length), ...list.map(enc));

  agScenarios.push({
    name,
    w,
    h,
    clusterSize,
    terrain: Array.from(terrain),
    ops: played,
    nodeCount: enc(graph.nodeCount),
    edgeCount: enc(graph.edgeCount),
    pathCacheLen: enc(graph._pathCache.length),
    nodes,
    edges,
    clusters,
    nodeEdgeIds,
    oldIdx: oldName ? agScenarios.findIndex((s) => s.name === oldName) : -1,
    dirty: oldName ? Array.from(dirtyTiles) : [],
  });
  agGraphs.set(name, graph);
}

const AW = 0x00; // water
const AL = 0x85; // land

// 1x1 cluster grid: no internal boundaries -> zero gateway nodes, zero edges.
runAg("ag_single_cluster", 4, 4, 4, new Array(16).fill(AW), [
  [0], [1], [2, 0], [3, 0], [4, 0], [5, 0, 1], [7, 0, 0], [8, 0, 0], [9, 0, 0], [10, 0, 0],
]);
// 8x8 all water, clusterSize 4 -> 2x2 clusters. Every cluster boundary is a
// contiguous water span, so gateways appear at each span mid-point and edges
// connect them within each cluster (BFS find order -> edge ids).
runAg("ag_all_water", 8, 8, 4, new Array(64).fill(AW), [
  [0], [1],
  [2, 0], [2, 1], [2, 2], [2, 3], [2, 4], [2, 5], [2, 999],
  [3, 0], [3, 1], [3, 999],
  [4, 0], [4, 1], [4, 999],
  [5, 0, 1], [5, 0, 2], [5, 1, 0], [5, 0, 999],
  [6, 0, 0], [6, 1, 0], [6, 999, 0],
  [7, 0, 0], [7, 1, 0], [7, 0, 1], [7, 1, 1],
  [8, 0, 0], [8, 1, 0], [8, 0, 1], [8, 1, 1], [8, 2, 2],
  [9, 0, 0], [9, 1, 0], [9, 0, 1],
  [10, 0, 0], [10, 1, 1],
  [11, 0], [11, 4], [11, 36], [11, 999],
  [12, 1], [12, 0], [12, 999],
  [13, 0, 0], [14, 0, 0], [13, 0, 0], [13, 0, 1], [13, 999, 0],
]);
// Cross-shaped water on a land map: only the channel tiles are water, so the
// vertical/horizontal boundary scans find short entrance spans.
{
  const t = new Array(64).fill(AL);
  for (let i = 0; i < 8; i++) {
    t[3 * 8 + i] = AW; // row 3 water
    t[i * 8 + 3] = AW; // col 3 water
  }
  runAg("ag_cross", 8, 8, 4, t, [
    [0], [1], [2, 0], [3, 0], [4, 0], [5, 0, 1], [8, 0, 0], [9, 0, 0], [10, 0, 0], [11, 27], [12, 1],
  ]);
}
// Checkerboard: isolated single water tiles, no adjacent water -> no entrance
// spans -> no gateways.
{
  const t = [];
  for (let y = 0; y < 8; y++) for (let x = 0; x < 8; x++) t.push((x + y) % 2 === 0 ? AW : AL);
  runAg("ag_checker", 8, 8, 4, t, [
    [0], [1], [2, 0], [3, 0], [8, 0, 0], [10, 0, 0],
  ]);
}
// Partial rebuild: reuse the all-water graph as the old graph and dirty only
// cluster (0,0)'s tiles. Clusters outside the 1-ring are "clean" and recreate
// their edges from the old-graph cost cache instead of BFS.
{
  const t = new Array(144).fill(AW);
  runAg("ag_all_water_12", 12, 12, 4, t, [
    [0], [1], [2, 0], [3, 0], [4, 0], [5, 0, 1], [6, 0, 0], [8, 0, 0], [9, 0, 0], [10, 0, 0], [11, 0], [12, 1],
  ]);
  // dirty tiles in cluster (0,0) -> its 1-ring (4 clusters) rebuilt by BFS,
  // the remaining 5 clusters (of 3x3) recreated from cache.
  runAg("ag_partial_rebuild", 12, 12, 4, t, [
    [0], [1], [2, 0], [2, 1], [3, 0], [3, 1], [4, 0], [5, 0, 1], [6, 0, 0], [7, 0, 0], [7, 1, 1], [8, 0, 0], [8, 2, 2], [9, 0, 0], [10, 0, 0], [10, 2, 2], [11, 0], [12, 1], [13, 0, 0], [14, 0, 0], [13, 0, 0],
  ], "ag_all_water_12", [0, 1, 12, 13]);
}

// --- AStarWaterHierarchical scenario runner ----------------------------------
// Runs the real orchestrator over a real GameMapImpl + AbstractGraphBuilder
// graph and records, after EVERY query, the returned path plus the five engine
// stamps (BFS / local / multi-cluster / short-path / abstract A*). The stamps
// pin the dispatch: which bounded engine a query consumed and how often, so a
// Rust port that takes a different branch (early exit vs node lookup vs
// abstract stitch, short-path vs abstract winner, cache hit vs recompute)
// diverges even when the final path happens to match. `rebuildBefore` replays
// `setGraph` (recreates the abstract engine — its stamp resets — while the
// map-sized engines keep counting). For cachePaths scenarios each query also
// snapshots the graph's direction-aware path cache, flattened per slot as
// `[-1]` (null) or `[len, tiles...]`. The Rust twin is
// `water_hierarchical::AStarWaterHierarchical`.
const whScenarios = [];
function captureWH(name, w, h, clusterSize, rows, cachePaths, queries) {
  const terrain = Uint8Array.from(rows.join("").split(""), (c) => (c === "w" ? AW : AL));
  const gm = new GameMapImpl(w, h, terrain, w * h);
  const graph = new AbstractGraphBuilder(gm, clusterSize).build();
  const wh = new AStarWaterHierarchical(gm, graph, cachePaths ? { cachePaths: true } : {});
  const snapCache = () =>
    graph._pathCache.flatMap((p) => (p === null || p === undefined ? [-1] : [p.length, ...Array.from(p)]));
  const qs = queries.map((q) => {
    if (q.rebuildBefore) {
      wh.setGraph(new AbstractGraphBuilder(gm, clusterSize).build());
    }
    const isMulti = Array.isArray(q.starts);
    const path = wh.findPath(isMulti ? q.starts : q.starts[0], q.goal);
    return {
      rebuildBefore: q.rebuildBefore ? 1 : 0,
      isMulti: isMulti ? 1 : 0,
      starts: q.starts.slice(),
      goal: q.goal,
      path: path === null ? "u" : Array.from(path),
      bfs: wh.tileBFS.stamp,
      local: wh.localAStar.stamp,
      multi: wh.localAStarMultiCluster.stamp,
      short: wh.localAStarShortPath.stamp,
      aga: wh.abstractAStar.stamp,
      cache: cachePaths ? snapCache() : [],
    };
  });
  whScenarios.push({
    name,
    w,
    h,
    clusterSize,
    terrain: Array.from(terrain),
    cachePaths: cachePaths ? 1 : 0,
    queries: qs,
  });
}

const WHW = "wwwwwwwwwwww"; // 12-wide all-water row (AW byte 0x00)
// 12x12 all water, clusterSize 4. Pins: early-exit local (27->38), the
// same-node fallback with dist > clusterSize (0->62 both resolve to node 1),
// the abstract stitch forward + reverse, and multi-source short-path success
// (both winner choices the short engine can make).
captureWH("wh_all_water", 12, 12, 4, Array(12).fill(WHW), false, [
  { starts: [27], goal: 38 },
  { starts: [0], goal: 15 },
  { starts: [0], goal: 62 },
  { starts: [0], goal: 143 },
  { starts: [143], goal: 0 },
  { starts: [27, 38], goal: 90 },
  { starts: [0, 143], goal: 70 },
]);
// cachePaths on the same map: q1 computes + caches the three middle segments,
// q2 runs the REVERSE node path (different cache direction -> recompute),
// q3 repeats q1 and must hit every cached segment (stamps prove the local
// engines stay untouched while the BFS still runs).
captureWH("wh_cache", 12, 12, 4, Array(12).fill(WHW), true, [
  { starts: [0], goal: 143 },
  { starts: [143], goal: 0 },
  { starts: [0], goal: 143 },
]);
// Two disconnected water strips (cols 0-3 and 8-11). Pins: abstract null
// across components (0->143), early exit inside a strip (12->15), a
// same-component stitch through the strip's two gateways (2->50), and a
// multi-source short-path whose winner is the far strip's source.
{
  const rows = Array(12).fill("wwwwLLLLwwww");
  captureWH("wh_disconnected", 12, 12, 4, rows, false, [
    { starts: [0], goal: 143 },
    { starts: [12], goal: 15 },
    { starts: [2], goal: 50 },
    { starts: [0, 143], goal: 70 },
  ]);
}
// Land map with a horizontal channel (rows 5-6) and a right block (x8-11,
// y4-11): clusters (0,0)/(1,0) have NO gateway nodes. Pins: startNode null
// (0->60), endNode null (5->0), and a multi-source query whose sources all
// resolve to nothing -> null after the short-path miss.
{
  const rows = [
    "LLLLLLLLLLLL",
    "LLLLLLLLLLLL",
    "LLLLLLLLLLLL",
    "LLLLLLLLLLLL",
    "LLLLLLLLwwww",
    "wwwwwwwwwwww",
    "wwwwwwwwwwww",
    "LLLLLLLLwwww",
    "LLLLLLLLwwww",
    "LLLLLLLLwwww",
    "LLLLLLLLwwww",
    "LLLLLLLLwwww",
  ];
  captureWH("wh_no_nodes", 12, 12, 4, rows, false, [
    { starts: [0], goal: 60 },
    { starts: [5], goal: 0 },
    { starts: [0, 5], goal: 70 },
  ]);
}
// 40x40 all water with a land wall down column 20 (rows 0-24, gap below):
// (18,2)->(22,2) is only 4 apart so the early-exit 3x3 window cannot cross
// the wall -> the query falls through to the node lookup + abstract stitch,
// exercising the endpoint-fix unshift/push on real gateway tiles and the
// multi-source ABSTRACT winner path (short fails through the wall, the
// closest-per-node map picks source 418 regardless of insertion order).
{
  const rows = [];
  for (let y = 0; y < 40; y++) {
    rows.push(y <= 24 ? "w".repeat(20) + "L" + "w".repeat(19) : "w".repeat(40));
  }
  captureWH("wh_gap", 40, 40, 4, rows, false, [
    { starts: [738], goal: 882 },
    { starts: [882], goal: 738 },
    { starts: [738, 418], goal: 882 },
    { starts: [418, 738], goal: 882 },
  ]);
}
// setGraph between queries: q2 rebuilds the graph first, so the abstract
// engine's stamp RESETS to 1 while bfs/local/multi/short keep counting.
captureWH("wh_setgraph", 12, 12, 4, Array(12).fill(WHW), false, [
  { starts: [0], goal: 143 },
  { starts: [143], goal: 0, rebuildBefore: 1 },
]);

// --- Parabola (PathFinder.Parabola.ts) scenario runner ------------------------
// Exercises the real getParabolaControlPoints and ParabolaUniversalPathFinder
// over a real GameMapImpl. Control points and findPath tile lists are
// recorded as raw doubles; the walk is one flat 8-slot-per-step script over a
// single finder instance, so the curve/lastTo cache state evolves exactly as
// in production. The wasm probe compares everything bit-exactly. The Rust
// twin is `pathfinding::parabola`.
//
// Walk step kinds: [0,from,to,hasSpeed,speed,status,node,index] next ok;
// [1,from,to,hasSpeed,speed,_,_,index] next threw (out-of-bounds floor ->
// TS `Invalid coordinates`; the script ends there); [2,0...] invalidate;
// [3,index,...] currentIndex read.
const parabolaScenarios = [];
const {
  getParabolaControlPoints,
  ParabolaUniversalPathFinder,
} = await loadTs("src/core/pathfinding/PathFinder.Parabola.ts");

function pbLandMap(w, h) {
  const data = new Uint8Array(w * h).fill(0x03);
  return new GameMapImpl(w, h, data, w * h);
}

function capturePB(name, w, h, opt, cpPairs, findPairs, walkScript) {
  const gm = pbLandMap(w, h);
  const o = opt === null ? undefined : opt;
  const R = ([x, y]) => gm.ref(x, y);
  const cps = cpPairs.map(([f, t]) =>
    [R(f), R(t)].concat(
      getParabolaControlPoints(gm, R(f), R(t), o).flatMap((p) => [p.x, p.y]),
    ),
  );
  const pf = new ParabolaUniversalPathFinder(gm, o);
  const finds = findPairs.map(([f, t]) => {
    try {
      const p = Array.from(pf.findPath(R(f), R(t)));
      return [R(f), R(t), p.length, ...p];
    } catch {
      return [R(f), R(t), -1];
    }
  });
  const walk = [];
  for (const step of walkScript) {
    if (step[0] === "next") {
      const [, f, t, sp] = step;
      const hasSpeed = sp === undefined ? 0 : 1;
      try {
        const r = pf.next(R(f), R(t), sp);
        walk.push([0, R(f), R(t), hasSpeed, sp ?? 0, r.status, r.node === undefined ? 0 : r.node, pf.currentIndex()]);
      } catch {
        walk.push([1, R(f), R(t), hasSpeed, sp ?? 0, 0, 0, pf.currentIndex()]);
        break;
      }
    } else if (step[0] === "inv") {
      pf.invalidate();
      walk.push([2, 0, 0, 0, 0, 0, 0, 0]);
    } else {
      walk.push([3, 0, 0, 0, 0, 0, 0, pf.currentIndex()]);
    }
  }
  parabolaScenarios.push({ name, w, h, opt, cps, finds, walk });
}

// 40x30 all-land map, default options (increment 3, distanceBasedHeight,
// directionUp, bounds clamp). Pins: clamped control points (p1/p2 y hit 0),
// the 15-tile findPath, and a 45-step drain to COMPLETE + plateau.
capturePB(
  "pb_default",
  40,
  30,
  null,
  [
    [[3, 5], [30, 25]],
    [[0, 0], [39, 29]],
  ],
  [
    [[3, 5], [30, 25]],
    [[0, 0], [39, 29]],
  ],
  Array.from({ length: 45 }, () => ["next", [3, 5], [30, 25]]),
);

// Explicit options: increment 1, flat arc (no distanceBasedHeight), arcing
// downward. The speed script pins the fixed-point increment accumulation
// (0.5 rounds up to a full step, 25 jumps several indices at once).
capturePB(
  "pb_options",
  40,
  30,
  { increment: 1, distanceBasedHeight: false, directionUp: false },
  [[[3, 5], [30, 25]]],
  [[[3, 5], [30, 25]]],
  [10, 0.5, 3, 1, 1, 1, 25, 1, 1, 1].map((sp) => ["next", [3, 5], [30, 25], sp]),
);

// ignoreMapBounds on a 20x10 map: the upward arc escapes the map (p1y=-47),
// so findPath throws on the first floored point and next throws at step 2 —
// the throw marker pins the partial walk state (index 1).
capturePB(
  "pb_ignore_oob",
  20,
  10,
  { ignoreMapBounds: true },
  [[[2, 1], [18, 9]]],
  [[[2, 1], [18, 9]]],
  [
    ["next", [2, 1], [18, 9]],
    ["next", [2, 1], [18, 9]],
    ["next", [2, 1], [18, 9]],
    ["idx"],
  ],
);

// Degenerate from === to: the curve still has 4 points, and next() plateaus
// at index 0 forever (accumulated distance never reaches the spacing).
capturePB(
  "pb_same_point",
  40,
  30,
  null,
  [[[3, 5], [3, 5]]],
  [[[3, 5], [3, 5]]],
  Array.from({ length: 3 }, () => ["next", [3, 5], [3, 5]]),
);

// Goal switching + invalidate on one finder instance: three steps toward
// (30,25), a currentIndex read, then switching the goal rebuilds the curve
// (index resets), switching back rebuilds again — the lastTo cache never
// resumes a stale curve. invalidate() drops the curve (currentIndex -> 0)
// and the next call rebuilds.
capturePB(
  "pb_rebuild",
  40,
  30,
  null,
  [],
  [],
  [
    ["next", [3, 5], [30, 25]],
    ["next", [3, 5], [30, 25]],
    ["next", [3, 5], [30, 25]],
    ["idx"],
    ["next", [3, 5], [1, 1]],
    ["next", [3, 5], [1, 1]],
    ["next", [3, 5], [30, 25]],
    ["inv"],
    ["idx"],
    ["next", [3, 5], [30, 25]],
  ],
);

// --- MiniMapTransformer (transformers/MiniMapTransformer.ts) runner ----------
// Runs the real transformer over a real main + mini GameMapImpl pair with a
// scripted inner PathFinder stub. The stub records what the transformer passed
// it (scalar vs array + the minimap downscale), which is what makes the TS
// `TileRef | TileRef[]` union semantics observable (single-element array
// collapses to a scalar start yet still runs the multi-source branch).
// Queries replay linearly over one transformer + stub.
//
// Query group (variable length):
//   [from_is_array, from_len, from_tiles..., to,
//    inner_mode, inner_len, inner_tiles...,
//    seen_flag, [is_multi, seen_len, seen_tiles..., seen_goal],
//    out_mode, [out_len, out_tiles...]]
// from_tiles are main-map refs. inner_mode 0 = null, 1 = [], 2 = tile list
// (inner tiles are mini refs). seen_flag 0 = stub never called (a downscale
// `ref` threw). out_mode 0 = null, 1 = threw (upscale `ref` out of bounds),
// 2 = path (main refs).
const mmtScenarios = [];
const { MiniMapTransformer } = await loadTs(
  "src/core/pathfinding/transformers/MiniMapTransformer.ts",
);

function mmtLandMap(w, h) {
  return new GameMapImpl(w, h, new Uint8Array(w * h).fill(0x03), w * h);
}

function captureMMT(name, mw, mh, miniW, miniH, queries) {
  const main = mmtLandMap(mw, mh);
  const mini = mmtLandMap(miniW, miniH);
  const stub = {
    next: null,
    last: null,
    findPath(from, to) {
      this.last = { from, to };
      return this.next;
    },
  };
  const tr = new MiniMapTransformer(stub, main, mini);
  const groups = [];
  for (const q of queries) {
    const innerMode = q.inner === null ? 0 : q.inner.length === 0 ? 1 : 2;
    const innerTiles = (q.inner ?? []).map(([x, y]) => mini.ref(x, y));
    const fromArg = q.scalar ? main.ref(q.from[0][0], q.from[0][1]) : q.from.map(([x, y]) => main.ref(x, y));
    const toRef = main.ref(q.to[0], q.to[1]);
    stub.next = innerMode === 0 ? null : innerMode === 1 ? [] : innerTiles;
    stub.last = null;
    let outMode;
    let outTiles = [];
    try {
      const p = tr.findPath(fromArg, toRef);
      if (p === null) outMode = 0;
      else {
        outMode = 2;
        outTiles = p;
      }
    } catch {
      outMode = 1;
    }
    const fs = Array.isArray(fromArg) ? fromArg : [fromArg];
    const g = [Array.isArray(fromArg) ? 1 : 0, fs.length, ...fs, toRef];
    g.push(innerMode, innerTiles.length, ...innerTiles);
    if (stub.last === null) {
      g.push(0);
    } else {
      const f = stub.last.from;
      const isMulti = Array.isArray(f) ? 1 : 0;
      const seenTiles = Array.isArray(f) ? f : [f];
      g.push(1, isMulti, seenTiles.length, ...seenTiles, stub.last.to);
    }
    g.push(outMode);
    if (outMode === 2) g.push(outTiles.length, ...outTiles);
    groups.push(g);
  }
  mmtScenarios.push({ name, mw, mh, miniW, miniH, groups });
}

// 20x20 main / 10x10 mini: null / empty / path results, fixExtremes unshift,
// and the single-element-array collapse (seen scalar, multi branch still run).
captureMMT("mmt_basic", 20, 20, 10, 10, [
  { from: [[2, 4]], scalar: true, to: [16, 10], inner: null },
  { from: [[2, 4]], scalar: true, to: [16, 10], inner: [] },
  { from: [[2, 4]], scalar: true, to: [16, 10], inner: [[1, 2], [2, 3], [3, 4], [8, 5]] },
  { from: [[4, 4]], scalar: true, to: [16, 10], inner: [[1, 2], [2, 3], [3, 4], [8, 5]] },
  { from: [[2, 4]], scalar: false, to: [16, 10], inner: [[1, 2], [2, 3], [3, 4], [8, 5]] },
]);

// Destination / source landing mid-path: dstIndex truncation, srcIndex slice,
// and dst appended when absent.
captureMMT("mmt_dst", 20, 20, 10, 10, [
  { from: [[2, 4]], scalar: true, to: [4, 4], inner: [[1, 2], [2, 2], [3, 3]] },
  { from: [[6, 6]], scalar: true, to: [4, 4], inner: [[1, 2], [2, 2], [3, 3]] },
  { from: [[2, 4]], scalar: true, to: [16, 10], inner: [[1, 2], [2, 2], [3, 3]] },
]);

// Multi-source: closest start by Manhattan distance (strict < keeps the first
// on ties), null/empty propagation, and the empty-array start where srcRef
// stays undefined (no source to fix).
captureMMT("mmt_multi", 20, 20, 10, 10, [
  { from: [[2, 4], [10, 10]], to: [16, 10], inner: [[5, 5], [6, 5], [8, 5]] },
  { from: [[2, 4], [10, 10]], to: [16, 10], inner: [[1, 2], [2, 2], [3, 3]] },
  { from: [[2, 4], [14, 4]], to: [16, 10], inner: [[4, 4], [5, 4]] },
  { from: [[2, 4], [10, 10]], to: [16, 10], inner: null },
  { from: [], to: [16, 10], inner: null },
  { from: [], to: [16, 10], inner: [[1, 2]] },
]);

// 10x10 main / 5x5 mini: the interpolated diagonal pins Math.round half-up at
// every .5 step; the single-point inner path exercises the last-point push and
// dst append.
captureMMT("mmt_interp", 10, 10, 5, 5, [
  { from: [[0, 0]], scalar: true, to: [4, 8], inner: [[0, 0], [2, 4]] },
  { from: [[0, 0]], scalar: true, to: [4, 8], inner: [[0, 0]] },
]);

// 30x30 main / 10x10 mini: downscale `ref` throws (floor(coord/2) past the
// mini edge) before the stub is ever called - for scalar, goal, and the second
// element of a multi start.
captureMMT("mmt_oob", 30, 30, 10, 10, [
  { from: [[28, 28]], scalar: true, to: [2, 2], inner: null },
  { from: [[2, 2]], scalar: true, to: [28, 28], inner: null },
  { from: [[2, 2], [28, 28]], to: [2, 2], inner: null },
]);

// 20x20 main / 15x15 mini (mini larger than half): upscale `ref` throws on the
// last point and mid-interpolation, after the stub ran; plus a valid upscale
// through the oversized mini.
captureMMT("mmt_bigmini", 20, 20, 15, 15, [
  { from: [[2, 2]], scalar: true, to: [19, 19], inner: [[12, 12]] },
  { from: [[2, 2]], scalar: true, to: [19, 19], inner: [[9, 9], [12, 12]] },
  { from: [[10, 10]], scalar: true, to: [12, 12], inner: [[5, 5], [6, 6]] },
]);

// --- PathFinderStepper (PathFinderStepper.ts) runner --------------------------
// Runs the real stepper over a real 10x10 all-land GameMap with a queue-backed
// inner stub (records calls + last seen start/goal). Two config shapes:
// `prod` = tileStepperConfig (equals ===, distance = manhattanDist, preCheck
// invalid-ref -> NOT_FOUND); bare = default config (equals only, no distance,
// no preCheck). Ops replay linearly over ONE stepper instance so the path /
// pathIndex / lastTo cache evolves exactly as in production.
//
// Flat op encodings:
//   [0, from, to, dist, status, node, panLen, tiles..., idx, hasPath, calls]
//     next (dist -1 = undefined; node -1 = NotFound; panLen -1 = null
//     pathAfterNext; status 1 = threw; hasPath = path !== null)
//   [1] invalidate
//   [2] reset (stub queue + observation only; call count persists)
//   [3, isMulti, fromLen, fromTiles..., to, outMode, outLen, outTiles...,
//       seenFlag, [seenMulti, seenLen, seenTiles..., seenGoal], calls] findPath
//     (outMode 0 = null, 1 = threw, 2 = list; seenFlag 0 = inner not called)
//   [4, count, (nullFlag | 1, len, tiles...)...] queue inner results
const stepperScenarios = [];
const { PathFinderStepper } = await loadTs("src/core/pathfinding/PathFinderStepper.ts");
const PATH_STATUS = { NEXT: 0, COMPLETE: 2, NOT_FOUND: 3 };

function captureStepper(name, prod, ops) {
  const gm = pbLandMap(10, 10);
  const refOf = (v) => (Array.isArray(v) ? gm.ref(v[0], v[1]) : v);
  const stub = {
    queue: [],
    last: null,
    calls: 0,
    findPath(from, to) {
      this.calls++;
      this.last = { from, to };
      return this.queue.length ? this.queue.shift() : null;
    },
  };
  const config = prod
    ? {
        equals: (a, b) => a === b,
        distance: (a, b) => gm.manhattanDist(a, b),
        preCheck: (from, to) =>
          typeof from !== "number" ||
          typeof to !== "number" ||
          !gm.isValidRef(from) ||
          !gm.isValidRef(to)
            ? { status: PATH_STATUS.NOT_FOUND }
            : null,
      }
    : undefined;
  const st = new PathFinderStepper(stub, config);
  const flat = [];
  for (const op of ops) {
    if (op[0] === "reset") {
      stub.queue = [];
      stub.last = null;
      flat.push(2);
    } else if (op[0] === "inv") {
      st.invalidate();
      flat.push(1);
    } else if (op[0] === "queue") {
      const g = [4, op[1].length];
      for (const p of op[1]) {
        if (p === null) {
          g.push(0);
          stub.queue.push(null);
        } else {
          const tiles = p.map(refOf);
          g.push(1, tiles.length, ...tiles);
          stub.queue.push(tiles);
        }
      }
      flat.push(...g);
    } else if (op[0] === "next") {
      const [, f, t, d] = op;
      const from = refOf(f);
      const to = refOf(t);
      const dist = d === undefined ? -1 : d;
      let status;
      let node;
      try {
        const r = st.next(from, to, dist === -1 ? undefined : dist);
        status = r.status;
        node = r.node === undefined ? -1 : r.node;
      } catch {
        status = 1;
        node = -1;
      }
      const pan = st.pathAfterNext();
      const g = [0, from, to, dist, status, node];
      if (pan === null) g.push(-1);
      else g.push(pan.length, ...pan);
      g.push(st.pathIndex, st.path !== null ? 1 : 0, stub.calls);
      flat.push(...g);
    } else {
      const [, fs, t, isMulti] = op;
      const fromTiles = fs.map(refOf);
      const fromArg = isMulti ? fromTiles : fromTiles[0];
      const to = refOf(t);
      let outMode;
      let out = [];
      try {
        const p = st.findPath(fromArg, to);
        outMode = p === null ? 0 : 2;
        out = p ?? [];
      } catch {
        outMode = 1;
      }
      const g = [3, isMulti, fromTiles.length, ...fromTiles, to, outMode, out.length, ...out];
      if (stub.last === null) {
        g.push(0);
      } else {
        const f = stub.last.from;
        const m = Array.isArray(f) ? 1 : 0;
        const tiles = Array.isArray(f) ? f : [f];
        g.push(1, m, tiles.length, ...tiles, stub.last.to);
      }
      g.push(stub.calls);
      flat.push(...g);
    }
  }
  stepperScenarios.push({ name, prod, ops: flat });
}

// Production config: pre-check NOT_FOUND on invalid refs (inner untouched),
// from === to COMPLETE, distance-based early exit, path[0] === from skipping
// the start node, the stray-from-cached-route invalidate + recompute (queue
// exhausted -> NOT_FOUND), a 3-step drain to COMPLETE, multi/scalar findPath
// passthrough, and the three allFailed short-circuits (invalid starts, empty
// array, invalid goal).
captureStepper("sp_prod", true, [
  ["reset"],
  ["next", 105, [0, 5]],
  ["reset"],
  ["next", [0, 0], [0, 0]],
  ["reset"],
  ["next", [0, 0], [9, 9], 5],
  ["reset"],
  ["queue", [[[0, 0], [1, 1], [2, 2], [3, 3]]]],
  ["next", [0, 0], [3, 3]],
  ["next", [1, 1], [3, 3]],
  ["reset"],
  ["queue", [[[0, 0], [1, 1], [2, 2]]]],
  ["next", [0, 0], [2, 2]],
  ["next", [1, 1], [2, 2]],
  ["next", [2, 2], [2, 2]],
  ["next", [0, 0], [9, 9]],
  ["reset"],
  ["queue", [[[0, 0], [1, 1]]]],
  ["next", [0, 0], [1, 1], 18],
  ["reset"],
  ["queue", [[[0, 0], [5, 5]]]],
  ["fp", [[0, 0], [9, 9]], [5, 5], 1],
  ["reset"],
  ["fp", [105, 106], [0, 5], 1],
  ["reset"],
  ["fp", [], [0, 5], 1],
  ["reset"],
  ["fp", [[0, 0]], 105, 0],
]);

// Bare config: no pre-check (invalid refs pass through), no distance early
// exit; two cache-hit steps with `from` frozen at the start node, then the
// stray recompute, and a multi findPath passthrough.
captureStepper("sp_bare", false, [
  ["reset"],
  ["queue", [[[0, 0], [1, 1], [2, 2], [3, 3]]]],
  ["next", [0, 0], [3, 3]],
  ["next", [0, 0], [3, 3]],
  ["next", [0, 0], [3, 3]],
  ["reset"],
  ["queue", [[[0, 0], [1, 1], [2, 2]]]],
  ["fp", [[0, 0], [1, 1]], [2, 2], 1],
  ["reset"],
  ["queue", [[[0, 0]]]],
  ["fp", [105], 105, 0],
]);

// --- ComponentCheckTransformer (ComponentCheckTransformer.ts) runner ---------
// Runs the real transformer over a queue-backed inner stub (records the
// PathStart kind/tiles/goal) with a table component getter. The transformer
// passes inner's result through unchanged, so the output is fully derived
// from (seen_flag, inner_mode) and need not be recorded separately. Group
// encoding:
// [is_multi, from_len, from_refs..., to, inner_mode, inner_len, inner_refs...,
//  seen_flag, [seen_multi, seen_len, seen_refs..., seen_goal]]
// inner_mode 0 = null, 2 = list; seen_flag 0 = inner never called (=> output
// null), 1 = inner called (=> output = inner result).
const ccTScenarios = [];
const { ComponentCheckTransformer } = await loadTs(
  "src/core/pathfinding/transformers/ComponentCheckTransformer.ts",
);

function captureCCT(name, table, def, queries) {
  const gm = pbLandMap(10, 10);
  const R = (xy) => gm.ref(xy[0], xy[1]);
  const comp = (t) => (table[t] === undefined ? def : table[t]);
  const stub = {
    next: null,
    last: null,
    findPath(from, to) {
      this.last = { from, to };
      return this.next;
    },
  };
  const tr = new ComponentCheckTransformer(stub, comp);
  const groups = [];
  for (const q of queries) {
    const fromRefs = q.from.map(R);
    const fromArg = q.scalar ? fromRefs[0] : fromRefs;
    const toRef = R(q.to);
    const innerMode = q.inner === null ? 0 : 2;
    const innerRefs = (q.inner ?? []).map(R);
    stub.next = q.inner === null ? null : innerRefs;
    stub.last = null;
    const p = tr.findPath(fromArg, toRef);
    const g = [q.scalar ? 0 : 1, fromRefs.length, ...fromRefs, toRef];
    g.push(innerMode, innerRefs.length, ...innerRefs);
    if (stub.last === null) {
      g.push(0);
    } else {
      const f = stub.last.from;
      const m = Array.isArray(f) ? 1 : 0;
      const tiles = Array.isArray(f) ? f : [f];
      g.push(1, m, tiles.length, ...tiles, stub.last.to);
    }
    // Cross-check: the pass-through invariant must hold in TS too.
    const expectNull = stub.last === null || innerMode === 0;
    if (expectNull ? p !== null : !(p && sameList(p, innerRefs))) {
      throw new Error(`${name} q${groups.length}: pass-through violated`);
    }
    groups.push(g);
  }
  ccTScenarios.push({ name, table, default: def, groups });
}

const sameList = (a, b) => a.length === b.length && a.every((v, i) => v === b[i]);

// 10x10 all-water map (refs = y*10+x). Table pins a few component ids,
// everything else falls to the default. Covers: same-component scalar
// passthrough, mismatch -> null (inner untouched), multi filtering with the
// single-survivor collapse, order preservation with multiple survivors, the
// vacuous empty-array null, null propagation from inner, and all-default
// equality (scalar and multi).
captureCCT("cct_basic", { 0: 10, 11: 10, 22: 20, 33: 30 }, 0, [
  { from: [[0, 0]], scalar: true, to: [1, 1], inner: [[0, 0], [1, 1]] },
  { from: [[0, 0]], scalar: true, to: [2, 2], inner: [[0, 0]] },
  { from: [[0, 0], [2, 2], [3, 3]], to: [1, 1], inner: [[0, 0], [1, 1]] },
  { from: [[0, 0], [2, 2], [3, 3]], to: [3, 3], inner: [[3, 3]] },
  { from: [[0, 0], [2, 2], [1, 1]], to: [1, 1], inner: [[0, 0], [1, 1]] },
  { from: [], to: [1, 1], inner: [[0, 0]] },
  { from: [[0, 0]], scalar: true, to: [1, 1], inner: null },
  { from: [[0, 0], [2, 2]], to: [4, 4], inner: [[0, 0]] },
]);

captureCCT("cct_default", {}, 7, [
  { from: [[5, 5]], scalar: true, to: [6, 6], inner: [[5, 5], [6, 6]] },
  { from: [[5, 5], [6, 6], [7, 7]], to: [5, 5], inner: [[7, 7]] },
]);

// --- ShoreCoercingTransformer (ShoreCoercingTransformer.ts) runner -----------
// Runs the real transformer over a hand-built land/water map with a
// queue-backed inner stub. Map is `w x h`, default land (0x83), `water`
// coords flipped to 0x03. Group encoding:
// [is_multi, from_len, from_refs..., to, inner_mode, inner_len, inner_refs...,
//  seen_flag, [seen_multi, seen_len, seen_refs..., seen_goal], out_mode,
//  [out_len, out_refs...]]
// inner_mode 0 = null, 1 = [], 2 = list; seen_flag 0 = inner never called;
// out_mode 0 = null, 2 = path.
const sctScenarios = [];
const { ShoreCoercingTransformer } = await loadTs(
  "src/core/pathfinding/transformers/ShoreCoercingTransformer.ts",
);

function captureSCT(name, w, h, water, queries) {
  const data = new Uint8Array(w * h).fill(0x83);
  for (const [x, y] of water) data[y * w + x] = 0x03;
  const gm = new GameMapImpl(w, h, data, w * h - water.length);
  const R = (xy) => gm.ref(xy[0], xy[1]);
  const stub = {
    next: null,
    last: null,
    findPath(from, to) {
      this.last = { from, to };
      return this.next;
    },
  };
  const tr = new ShoreCoercingTransformer(stub, gm);
  const groups = [];
  for (const q of queries) {
    const fromRefs = q.from.map(R);
    const fromArg = q.scalar ? fromRefs[0] : fromRefs;
    const toRef = R(q.to);
    const innerMode = q.inner === null ? 0 : q.inner.length === 0 ? 1 : 2;
    const innerRefs = (q.inner ?? []).map(R);
    stub.next = innerMode === 0 ? null : innerMode === 1 ? [] : innerRefs;
    stub.last = null;
    const p = tr.findPath(fromArg, toRef);
    const g = [q.scalar ? 0 : 1, fromRefs.length, ...fromRefs, toRef];
    g.push(innerMode, innerRefs.length, ...innerRefs);
    if (stub.last === null) {
      g.push(0);
    } else {
      const f = stub.last.from;
      const m = Array.isArray(f) ? 1 : 0;
      const tiles = Array.isArray(f) ? f : [f];
      g.push(1, m, tiles.length, ...tiles, stub.last.to);
    }
    if (p === null) g.push(0);
    else g.push(2, p.length, ...p);
    groups.push(g);
  }
  sctScenarios.push({ name, w, h, water, groups });
}

// Blob of water (2,2)-(4,3) plus (3,4); isolated (8,8); a (6,2)/(7,2) and
// (6,4)/(7,4) pair creating a score tie for (6,3). Covers: water passthrough
// scalar, shore coercion + start restore, multi shore + goal coercion + end
// append, deep-land start / goal nulls, null / empty inner, the raw-water
// delete overriding an earlier coercion (duplicate starts observable), last-
// write-wins on two shores coercing to the same water, and the strict->
// first-wins tie in bestWaterNeighbor.
const SCT_WATER = [
  [2, 2], [3, 2], [4, 2], [2, 3], [3, 3], [4, 3], [3, 4],
  [8, 8], [6, 2], [7, 2], [6, 4], [7, 4],
];
captureSCT("sct_blob", 10, 10, SCT_WATER, [
  { from: [[3, 3]], scalar: true, to: [4, 2], inner: [[3, 3], [4, 3], [4, 2]] },
  { from: [[1, 2]], scalar: true, to: [4, 2], inner: [[2, 2], [3, 2], [4, 2]] },
  { from: [[1, 2], [5, 2]], to: [3, 4], inner: [[2, 2], [3, 2], [3, 3]] },
  { from: [[0, 0]], scalar: true, to: [4, 2], inner: [[4, 2]] },
  { from: [[3, 3]], scalar: true, to: [0, 0], inner: [[3, 3]] },
  { from: [[1, 2], [5, 2]], to: [4, 2], inner: null },
  { from: [[1, 2], [5, 2]], to: [4, 2], inner: [] },
  { from: [[1, 2], [2, 2]], to: [4, 2], inner: [[2, 2], [4, 2]] },
  { from: [[1, 2], [2, 1]], to: [4, 2], inner: [[2, 2], [4, 2]] },
  { from: [[6, 3]], scalar: true, to: [6, 2], inner: [[6, 2]] },
]);

// Goal already equals the coerced original's path end: the `path[last] !==
// originalTo` guard skips the duplicate append. Single-water-source collapse
// to scalar start.
captureSCT("sct_goal", 10, 10, SCT_WATER, [
  { from: [[8, 8]], scalar: true, to: [8, 8], inner: [[8, 8]] },
  { from: [[7, 3]], scalar: true, to: [7, 2], inner: [[7, 2]] },
]);

// --- SmoothingWaterTransformer (SmoothingWaterTransformer.ts) runner ---------
// Runs the real transformer over a hand-built water map with a queue-backed
// inner stub. Map is `w x h`, default land (0x83); `cells` is a list of
// [x, y, byte] setting explicit terrain magnitudes (deep 0x0B = mag 11,
// shallow 0x02 = mag 2, very-shallow 0x03 = mag 3). Group encoding matches
// the SCT runner:
// [is_multi, from_len, from_refs..., to, inner_mode, inner_len, inner_refs...,
//  seen_flag, [seen_multi, seen_len, seen_refs..., seen_goal], out_mode,
//  [out_len, out_refs...]]
// inner_mode 0 = null, 1 = [], 2 = list; seen_flag 0 = inner never called;
// out_mode 0 = null, 2 = path.
const swtScenarios = [];
const { SmoothingWaterTransformer } = await loadTs(
  "src/core/pathfinding/transformers/SmoothingWaterTransformer.ts",
);

function captureSWT(name, w, h, cells, queries) {
  const data = new Uint8Array(w * h).fill(0x83);
  for (const [x, y, b] of cells) data[y * w + x] = b;
  const landCount = data.reduce((n, byte) => n + (byte & 0x80 ? 1 : 0), 0);
  const gm = new GameMapImpl(w, h, data, landCount);
  const R = (xy) => gm.ref(xy[0], xy[1]);
  const stub = {
    next: null,
    last: null,
    findPath(from, to) {
      this.last = { from, to };
      return this.next;
    },
  };
  const tr = new SmoothingWaterTransformer(stub, gm);
  const groups = [];
  for (const q of queries) {
    const fromRefs = q.from.map(R);
    const fromArg = q.scalar ? fromRefs[0] : fromRefs;
    const toRef = R(q.to);
    const innerMode = q.inner === null ? 0 : q.inner.length === 0 ? 1 : 2;
    const innerRefs = (q.inner ?? []).map(R);
    stub.next = innerMode === 0 ? null : innerMode === 1 ? [] : innerRefs;
    stub.last = null;
    const p = tr.findPath(fromArg, toRef);
    const g = [q.scalar ? 0 : 1, fromRefs.length, ...fromRefs, toRef];
    g.push(innerMode, innerRefs.length, ...innerRefs);
    if (stub.last === null) {
      g.push(0);
    } else {
      const f = stub.last.from;
      const m = Array.isArray(f) ? 1 : 0;
      const tiles = Array.isArray(f) ? f : [f];
      g.push(1, m, tiles.length, ...tiles, stub.last.to);
    }
    if (p === null) g.push(0);
    else g.push(2, p.length, ...p);
    groups.push(g);
  }
  swtScenarios.push({ name, w, h, cells, groups });
}

// Deep-water row y=5 x=2..9 (mag 11). Covers: inner null / empty / length-2
// passthrough, the straight-line LOS collapse + endpoint refinement that
// re-runs the whole path through the local bounded A*, and the multi-start
// union forwarded untouched to `inner`.
const SWT_DEEP_ROW = [];
for (let x = 2; x <= 9; x++) SWT_DEEP_ROW.push([x, 5, 0x0b]);
captureSWT("swt_straight", 12, 12, SWT_DEEP_ROW, [
  { from: [[2, 5]], scalar: true, to: [9, 5], inner: null },
  { from: [[2, 5]], scalar: true, to: [9, 5], inner: [] },
  { from: [[2, 5]], scalar: true, to: [9, 5], inner: [[2, 5], [9, 5]] },
  {
    from: [[2, 5]],
    scalar: true,
    to: [9, 5],
    inner: [[2, 5], [3, 5], [4, 5], [5, 5], [6, 5], [7, 5], [8, 5], [9, 5]],
  },
  {
    from: [[2, 5], [3, 5]],
    scalar: false,
    to: [9, 5],
    inner: [[2, 5], [3, 5], [4, 5], [5, 5], [6, 5], [7, 5], [8, 5], [9, 5]],
  },
]);

// Same row but x=5,6 are shallow (mag 2): pass-1 LOS (min 2) sees straight
// through, pass-3 LOS (min 3) refuses the shallow band, so the endpoint
// refinement (bounded A* traverses any water) is what stitches the result.
const SWT_SHALLOW_ROW = SWT_DEEP_ROW.map(([x, y, b]) =>
  y === 5 && (x === 5 || x === 6) ? [x, y, 0x02] : [x, y, b],
);
captureSWT("swt_shallow", 12, 12, SWT_SHALLOW_ROW, [
  {
    from: [[2, 5]],
    scalar: true,
    to: [9, 5],
    inner: [[2, 5], [3, 5], [4, 5], [5, 5], [6, 5], [7, 5], [8, 5], [9, 5]],
  },
]);

// Three isolated deep tiles (no connecting water): LOS cannot collapse any
// pair, and the endpoint refinement's bounded A* finds no route, so both
// refine passes return null and the inner path survives verbatim.
captureSWT("swt_isolated", 12, 12, [[2, 2, 0x0b], [5, 5, 0x0b], [9, 9, 0x0b]], [
  { from: [[2, 2]], scalar: true, to: [9, 9], inner: [[2, 2], [5, 5], [9, 9]] },
]);

// A staircase of deep tiles joined by a shallow (mag 2) neck: pass-1 LOS
// threads the neck, the refinement reroutes, pass-3 LOS (min 3) splits at the
// neck again — exercising tracePath's diagonal detour and the splice slice.
const SWT_STAIR = [
  [2, 2, 0x0b], [3, 3, 0x0b], [4, 4, 0x0b], [5, 4, 0x02],
  [6, 4, 0x0b], [7, 5, 0x0b], [8, 6, 0x0b], [9, 7, 0x0b],
];
captureSWT("swt_stair", 12, 12, SWT_STAIR, [
  {
    from: [[2, 2]],
    scalar: true,
    to: [9, 7],
    inner: [[2, 2], [3, 3], [4, 4], [5, 4], [6, 4], [7, 5], [8, 6], [9, 7]],
  },
]);

// Open deep-water pool (2..10 x 2..6) with a zig-zagging inner path: LOS
// pass 1 collapses the staircase onto a straight Bresenham trace, the local
// A* refinement returns its own tie-broken route, and pass 3 smooths again —
// the output differs from the input.
const SWT_POOL = [];
for (let x = 2; x <= 10; x++) for (let y = 2; y <= 6; y++) SWT_POOL.push([x, y, 0x0b]);
captureSWT("swt_zigzag", 12, 12, SWT_POOL, [
  {
    from: [[2, 2]],
    scalar: true,
    to: [10, 6],
    inner: [
      [2, 2], [2, 3], [3, 3], [3, 4], [4, 4], [4, 5], [5, 5], [5, 6],
      [6, 6], [7, 6], [8, 6], [9, 6], [10, 6],
    ],
  },
]);

// Long corridor (> 50 manhattan tiles): the endpoint refinement only covers
// the first/last ~50 tiles, so the middle of the LOS-collapsed path survives
// the splice unchanged — pins findTileAtDistance's early stop and the
// slice/re-join arithmetic.
const SWT_LONG = [];
for (let x = 2; x <= 58; x++) for (let y = 4; y <= 6; y++) SWT_LONG.push([x, y, 0x0b]);
const SWT_LONG_PATH = [];
for (let x = 2; x <= 58; x++) SWT_LONG_PATH.push([x, x % 2 === 0 ? 4 : 6]);
captureSWT("swt_long", 64, 12, SWT_LONG, [
  { from: [[2, 4]], scalar: true, to: [58, 6], inner: SWT_LONG_PATH },
]);

const structures = {
  minheap: mhScenarios,
  bucket: bqScenarios,
  flatheap: fbhScenarios,
  bfsgrid: bgScenarios,
  bfs: bfsScenarios,
  air: airScenarios,
  anon: anonScenarios,
  close: closeScenarios,
  serverlist: slScenarios,
  patterndecoder: pdScenarios,
  doomsdayclock: dcScenarios,
  executil: euScenarios,
  watermanager: wmScenarios,
  gameupdateutils: guScenarios,
  railroad: rrScenarios,
  railgrid: rsgScenarios,
  tiletravscratch: ttsScenarios,
  eventbus: ebScenarios,
  asseturls: auScenarios,
  maps: mgScenarios,
  tribenames: tnScenarios,
  game: gameScenarios,
  nationcreation: ncScenarios,
  gameupdates: gupdScenarios,
  gameimpl: giScenarios,
  terranulliusimpl: tniScenarios,
  nationemoji: neScenarios,
  cosmeticschemas: csScenarios,
  statschemas: stScenarios,
  schemas: scScenarios,
  apischemas: asSchemasScenarios,
  terrainmaploader: tmlScenarios,
  nationutils: nuScenarios,
  waterpathmemo: wpmScenarios,
  astar: asScenarios,
  rail: railScenarios,
  water: waterScenarios,
  waterbounded: wbScenarios,
  gamemap: gmScenarios,
  tileset: tsScenarios,
  util: utilScenarios,
  team: teamScenarios,
  bezier: bezierScenarios,
  veterancy: veterancyScenarios,
  motionplans: mpScenarios,
  connectedcomponents: ccScenarios,
  terrainsearchmap: tsmScenarios,
  abstractgraph: agScenarios,
  abstractgraphastar: agaScenarios,
  waterhierarchical: whScenarios,
  parabola: parabolaScenarios,
  minimaptransformer: mmtScenarios,
  stepper: stepperScenarios,
  componentcheck: ccTScenarios,
  shorecoercing: sctScenarios,
  smoothingwater: swtScenarios,
};

// ================================================================ JSON
const json = {
  note: "generated by rust/tools/gen_vectors.mjs from src/core - do not edit",
  prng: {
    seeds,
    stream: streams,
    streamLen: STREAM,
    intRanges: ranges,
    intValues,
    intCount: INT_N,
    idSeed: ID_SEED,
    ids,
    shuffleLen: SHUF,
    shuffles,
    chanceSeed: CHANCE_SEED,
    chanceOdds: CHANCE_ODDS,
    chanceBytes,
  },
  detmath: {
    expX, expBits: bitsArr(expX.map(DetMath.exp)),
    logX, logBits: bitsArr(logX.map(DetMath.log)),
    powPairs, powBits: bitsArr(powPairs.map(([a, b]) => DetMath.pow(a, b))),
    atanPairs, atanBits: bitsArr(atanPairs.map(([a, b]) => DetMath.atan2(a, b))),
    pow2N: pow2Ns, pow2Bits: bitsArr(pow2Ns.map(DetMath.pow2)),
    snapshotDecimals: snapshotVals.map((v) => String(v)),
    snapshotBits: bitsArr(snapshotVals),
  },
  structures,
};

// ================================================================ Rust
const L = [];
L.push("// @generated by rust/tools/gen_vectors.mjs -- DO NOT EDIT.");
L.push("// Golden vectors produced by the TypeScript implementation in src/core.");
L.push("// Values are JS literals (e.g. Math.E) and must stay byte-identical to the");
L.push("// source, so the approx-constant lint does not apply here.");
L.push("// Each integration test includes only the subset of vectors it replays,");
L.push("// so unused-item warnings are expected and suppressed.");
L.push("#![allow(clippy::approx_constant, dead_code)]");
L.push("");
L.push("/// Seeds fed to the PRNG constructor, as JS numbers (f64 here).");
L.push("/// Includes negatives, fractions and magnitudes beyond 2^31/2^32 to");
L.push("/// exercise JS `| 0` (ToInt32) truncation.");
L.push("pub const PRNG_SEEDS: &[f64] = &[");
for (const s of seeds) L.push(`    ${f64(s)},`);
L.push("];");
L.push("");
L.push("/// Raw u32 output of `next_u32()` for each seed, `PRNG_STREAM_LEN` per seed.");
L.push("pub const PRNG_STREAMS: &[u32] = &[");
for (let si = 0; si < seeds.length; si++) {
  L.push(`    // seed ${seeds[si]}`);
  const slice = streams.slice(si * STREAM, si * STREAM + STREAM);
  for (let i = 0; i < slice.length; i += 8)
    L.push("    " + slice.slice(i, i + 8).map((v) => `${v}u32`).join(", ") + ",");
}
L.push("];");
L.push(`pub const PRNG_STREAM_LEN: usize = ${STREAM};`);
L.push("");
L.push("pub const PRNG_INT_RANGES: &[(f64, f64)] = &[");
for (const [lo, hi] of ranges) L.push(`    (${f64(lo)}, ${f64(hi)}),`);
L.push("];");
L.push("/// Flattened nextInt sequences, ordered seed-major then range-major.");
L.push("pub const PRNG_INT_VALUES: &[i64] = &[");
const groupCount = seeds.length * ranges.length;
for (let b = 0; b < groupCount; b++) {
  const s = seeds[Math.floor(b / ranges.length)];
  const [lo, hi] = ranges[b % ranges.length];
  L.push(`    // seed ${s} range [${lo}, ${hi})`);
  const slice = intValues.slice(b * INT_N, b * INT_N + INT_N);
  for (let i = 0; i < slice.length; i += 10)
    L.push("    " + slice.slice(i, i + 10).map((v) => `${v}i64`).join(", ") + ",");
}
L.push("];");
L.push(`pub const PRNG_INT_COUNT: usize = ${INT_N};`);
L.push("");
L.push("/// 200 consecutive nextID() strings from a PRNG seeded with `PRNG_ID_SEED`.");
L.push(`pub const PRNG_ID_SEED: f64 = ${f64(ID_SEED)};`);
L.push("pub const PRNG_IDS: &[&str] = &[");
for (const id of ids) L.push(`    "${id}",`);
L.push("];");
L.push("");
L.push(`/// shuffleArray over 0..${SHUF} for each seed, as byte permutations.`);
L.push("pub const PRNG_SHUFFLES: &[u8] = &[");
for (let si = 0; si < seeds.length; si++) {
  L.push(`    // seed ${seeds[si]}`);
  const slice = shuffles.slice(si * SHUF, si * SHUF + SHUF);
  L.push("    " + slice.map((v) => `${v}u8`).join(", ") + ",");
}
L.push("];");
L.push(`pub const PRNG_SHUFFLE_LEN: usize = ${SHUF};`);
L.push("");
L.push("/// chance(PRNG_CHANCE_ODDS) for a PRNG seeded with 42, bits MSB-first per byte.");
L.push(`pub const PRNG_CHANCE_SEED: f64 = ${f64(CHANCE_SEED)};`);
L.push(`pub const PRNG_CHANCE_ODDS: f64 = ${f64(CHANCE_ODDS)};`);
L.push("pub const PRNG_CHANCE_BITS: &[u8] = &[");
for (let i = 0; i < chanceBytes.length; i += 16)
  L.push("    " + chanceBytes.slice(i, i + 16).map((v) => `${v}u8`).join(", ") + ",");
L.push("];");
L.push("");
L.push("/// detmath::exp inputs; *_BITS holds f64::to_bits() of the TS result.");
L.push("pub const DET_EXP_X: &[f64] = &[");
for (let i = 0; i < expX.length; i += 6)
  L.push("    " + expX.slice(i, i + 6).map(f64).join(", ") + ",");
L.push("];");
L.push("pub const DET_EXP_BITS: &[u64] = &[");
for (let i = 0; i < expX.length; i += 6)
  L.push("    " + expX.slice(i, i + 6).map((v) => `${bits(DetMath.exp(v))}u64`).join(", ") + ",");
L.push("];");
L.push("");
L.push("pub const DET_LOG_X: &[f64] = &[");
for (let i = 0; i < logX.length; i += 6)
  L.push("    " + logX.slice(i, i + 6).map(f64).join(", ") + ",");
L.push("];");
L.push("pub const DET_LOG_BITS: &[u64] = &[");
for (let i = 0; i < logX.length; i += 6)
  L.push("    " + logX.slice(i, i + 6).map((v) => `${bits(DetMath.log(v))}u64`).join(", ") + ",");
L.push("];");
L.push("");
L.push("pub const DET_POW_ARGS: &[(f64, f64)] = &[");
for (let i = 0; i < powPairs.length; i += 6)
  L.push("    " + powPairs.slice(i, i + 6).map(([b, e]) => `(${f64(b)}, ${f64(e)})`).join(", ") + ",");
L.push("];");
L.push("pub const DET_POW_BITS: &[u64] = &[");
for (let i = 0; i < powPairs.length; i += 6)
  L.push("    " + powPairs.slice(i, i + 6).map(([b, e]) => `${bits(DetMath.pow(b, e))}u64`).join(", ") + ",");
L.push("];");
L.push("");
L.push("pub const DET_ATAN2_ARGS: &[(f64, f64)] = &[");
for (let i = 0; i < atanPairs.length; i += 5)
  L.push("    " + atanPairs.slice(i, i + 5).map(([y, x]) => `(${f64(y)}, ${f64(x)})`).join(", ") + ",");
L.push("];");
L.push("pub const DET_ATAN2_BITS: &[u64] = &[");
for (let i = 0; i < atanPairs.length; i += 5)
  L.push("    " + atanPairs.slice(i, i + 5).map(([y, x]) => `${bits(DetMath.atan2(y, x))}u64`).join(", ") + ",");
L.push("];");
L.push("");
L.push("pub const DET_POW2_N: &[i32] = &[");
for (let i = 0; i < pow2Ns.length; i += 12)
  L.push("    " + pow2Ns.slice(i, i + 12).map((v) => `${v}i32`).join(", ") + ",");
L.push("];");
L.push("pub const DET_POW2_BITS: &[u64] = &[");
for (let i = 0; i < pow2Ns.length; i += 12)
  L.push("    " + pow2Ns.slice(i, i + 12).map((v) => `${bits(DetMath.pow2(v))}u64`).join(", ") + ",");
L.push("];");
L.push("");
L.push("/// Cross-check of tests/core/__snapshots__/DetMath.test.ts.snap.");
L.push("pub const DET_SNAPSHOT_DECIMALS: &[f64] = &[");
for (const v of snapshotVals) L.push(`    ${f64(v)},`);
L.push("];");
L.push("pub const DET_SNAPSHOT_BITS: &[u64] = &[");
for (const v of snapshotVals) L.push(`    ${bits(v)}u64,`);
L.push("];");
L.push("");

// ---- structures: typed scenario constants for trace replay --------------
const resLit = (r) =>
  r === "u" ? "Res::Undef" : r === "n" ? "Res::Nan" : r === "t" ? "Res::Threw"
  : r === "-0" ? "Res::Val(-0.0f64)" : `Res::Val(${f64(r)})`;
const argLit = (v) => (v === "n" ? "f64::NAN" : v === "-0" ? "-0.0f64" : f64(v));
const opsLit = (ops) =>
  ops.map(([k, a, b, r]) => `Op { kind: ${k}, a: ${argLit(a)}, b: ${argLit(b)}, res: ${resLit(r)} }`);
const numArr = (xs, suffix, per = 12) => {
  const out = [];
  for (let i = 0; i < xs.length; i += per)
    out.push("    " + xs.slice(i, i + per).map((v) => `${v}${suffix}`).join(", ") + ",");
  return out;
};
const optArr = (xs) =>
  xs.map((v) => (v === "u" ? "None" : v === "n" ? "Some(f64::NAN)" : `Some(${f64(v)})`));

L.push("#[derive(Clone, Copy, Debug, PartialEq)]");
L.push("pub enum Res { Void, Val(f64), Undef, Nan, Threw }");
L.push("");
L.push("#[derive(Clone, Copy, Debug)]");
L.push("pub struct Op { pub kind: u8, pub a: f64, pub b: f64, pub res: Res }");
L.push("");
L.push("pub struct MinHeapScenario {");
L.push("    pub name: &'static str,");
L.push("    pub cap: f64,");
L.push("    pub ops: &'static [Op],");
L.push("    pub heap: &'static [i32],");
L.push("    pub pri_bits: &'static [u32],");
L.push("    pub size: i64,");
L.push("    pub capacity: usize,");
L.push("}");
L.push("");
L.push("pub struct BucketScenario {");
L.push("    pub name: &'static str,");
L.push("    pub max_p: f64,");
L.push("    pub ops: &'static [Op],");
L.push("    pub sizes: &'static [i32],");
L.push("    pub stamps: &'static [u32],");
L.push("    pub keys: &'static [i64],");
L.push("    pub min_bucket: f64,");
L.push("    pub size: f64,");
L.push("    pub stamp: u64,");
L.push("}");
L.push("");
L.push("pub struct FlatHeapScenario {");
L.push("    pub name: &'static str,");
L.push("    pub cap: usize,");
L.push("    pub ops: &'static [Op],");
L.push("    pub pri_bits: &'static [u32],");
L.push("    pub tiles: &'static [Option<f64>],");
L.push("    pub len: usize,");
L.push("}");
L.push("");
L.push("/// Grid-BFS scenario: mode 0 explore-all, 1 blocker invalid, 2");
L.push("/// blocker rejected (null), 3 blocker found (returns 42). When the");
L.push("/// visit stream is longer than the inline cap, `order`/`dists` are");
L.push("/// empty and the stream is pinned by sampled (index, node, dist)");
L.push("/// triples covering the Uint16 wrap boundary and both ends.");
L.push("pub struct BfsScenario {");
L.push("    pub name: &'static str,");
L.push("    pub w: i64,");
L.push("    pub h: i64,");
L.push("    pub s0: i64,");
L.push("    pub s1: i64,");
L.push("    pub max_d: f64,");
L.push("    pub mode: u8,");
L.push("    pub blocker: i64,");
L.push("    pub nvisits: usize,");
L.push("    pub samples: &'static [(i64, i64, i64)],");
L.push("    pub order: &'static [i64],");
L.push("    pub dists: &'static [i64],");
L.push("    pub found: i64,");
L.push("    pub stamp_after: u64,");
L.push("}");
L.push("");

for (const s of structures.minheap) {
  const id = s.name.toUpperCase();
  L.push(`const ${id}_OPS: &[Op] = &[`);
  for (const o of opsLit(s.ops)) L.push(`    ${o},`);
  L.push("];");
  L.push(`pub const ${id}: MinHeapScenario = MinHeapScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    cap: ${f64(s.cap)},`);
  L.push(`    ops: ${id}_OPS,`);
  L.push(`    heap: &[`);
  L.push(...numArr(s.heap, "i32", 16));
  L.push("],");
  L.push(`    pri_bits: &[`);
  L.push(...numArr(s.priBits, "u32", 12));
  L.push("],");
  L.push(`    size: ${s.size}i64,`);
  L.push(`    capacity: ${s.capacity},`);
  L.push("};");
  L.push("");
}
L.push("pub const MINHEAP_SCENARIOS: &[MinHeapScenario] = &[");
for (const s of structures.minheap) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

for (const s of structures.bucket) {
  const id = s.name.toUpperCase();
  L.push(`const ${id}_OPS: &[Op] = &[`);
  for (const o of opsLit(s.ops)) L.push(`    ${o},`);
  L.push("];");
  L.push(`pub const ${id}: BucketScenario = BucketScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    max_p: ${f64(s.maxP)},`);
  L.push(`    ops: ${id}_OPS,`);
  L.push(`    sizes: &[`);
  L.push(...numArr(s.sizes, "i32", 16));
  L.push("],");
  L.push(`    stamps: &[`);
  L.push(...numArr(s.stamps, "u32", 16));
  L.push("],");
  L.push(`    keys: &[`);
  L.push(...numArr(s.keys, "i64", 16));
  L.push("],");
  L.push(`    min_bucket: ${f64(s.minBucket)},`);
  L.push(`    size: ${f64(s.size)},`);
  L.push(`    stamp: ${s.stamp}u64,`);
  L.push("};");
  L.push("");
}
L.push("pub const BUCKET_SCENARIOS: &[BucketScenario] = &[");
for (const s of structures.bucket) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

for (const s of structures.flatheap) {
  const id = s.name.toUpperCase();
  L.push(`const ${id}_OPS: &[Op] = &[`);
  for (const o of opsLit(s.ops)) L.push(`    ${o},`);
  L.push("];");
  L.push(`pub const ${id}: FlatHeapScenario = FlatHeapScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    cap: ${s.cap},`);
  L.push(`    ops: ${id}_OPS,`);
  L.push(`    pri_bits: &[`);
  L.push(...numArr(s.priBits, "u32", 12));
  L.push("],");
  L.push(`    tiles: &[`);
  for (let i = 0; i < s.tiles.length; i += 8)
    L.push("    " + optArr(s.tiles.slice(i, i + 8)).join(", ") + ",");
  L.push("],");
  L.push(`    len: ${s.len},`);
  L.push("};");
  L.push("");
}
L.push("pub const FLATHEAP_SCENARIOS: &[FlatHeapScenario] = &[");
for (const s of structures.flatheap) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

for (const s of structures.bfsgrid) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: BfsScenario = BfsScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    w: ${s.w}i64,`);
  L.push(`    h: ${s.h}i64,`);
  L.push(`    s0: ${s.s0}i64,`);
  L.push(`    s1: ${s.s1}i64,`);
  L.push(`    max_d: ${s.maxd === "Infinity" ? "f64::INFINITY" : f64(Number(s.maxd))},`);
  L.push(`    mode: ${s.mode}u8,`);
  L.push(`    blocker: ${s.blocker}i64,`);
  L.push(`    nvisits: ${s.nvisits},`);
  L.push(`    samples: &[`);
  for (let i = 0; i < s.samples.length; i += 6)
    L.push(
      "    " +
        s.samples
          .slice(i, i + 6)
          .map(([idx, n, d]) => `(${idx}i64, ${n}i64, ${d}i64)`)
          .join(", ") +
        ", ",
    );
  L.push("],");
  L.push(`    order: &[`);
  L.push(...numArr(s.order, "i64", 16));
  L.push("],");
  L.push(`    dists: &[`);
  L.push(...numArr(s.dists, "i64", 16));
  L.push("],");
  L.push(`    found: ${s.found}i64,`);
  L.push(`    stamp_after: ${s.stampAfter}u64,`);
  L.push("};");
  L.push("");
}
L.push("pub const BFS_SCENARIOS: &[BfsScenario] = &[");
for (const s of structures.bfsgrid) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// A* scenario: GridAdapter parameters, one findPath call (or `runs`");
L.push("/// calls for reuse scenarios), the returned path (`None` = TS null),");
L.push("/// and the full stamp-tracked arrays after the last call.");
L.push("pub struct AStarScenario {");
L.push("    pub name: &'static str,");
L.push("    pub w: f64,");
L.push("    pub h: f64,");
L.push("    pub blocked: &'static [f64],");
L.push("    pub cc: f64,");
L.push("    pub tp: f64,");
L.push("    pub hk: u8,");
L.push("    pub hs: f64,");
L.push("    pub max_iter: Option<f64>,");
L.push("    pub starts: &'static [f64],");
L.push("    pub goal: f64,");
L.push("    pub runs: usize,");
L.push("    pub path: Option<&'static [f64]>,");
L.push("    pub stamp_after: u64,");
L.push("    pub closed: &'static [u32],");
L.push("    pub gs_stamp: &'static [u32],");
L.push("    pub g_score: &'static [u32],");
L.push("    pub came_from: &'static [i32],");
L.push("}");
L.push("");
for (const s of structures.astar) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: AStarScenario = AStarScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    w: ${f64(s.w)},`);
  L.push(`    h: ${f64(s.h)},`);
  L.push(`    blocked: &[${s.blocked.map(f64).join(", ")}],`);
  L.push(`    cc: ${f64(s.cc)},`);
  L.push(`    tp: ${f64(s.tp)},`);
  L.push(`    hk: ${s.hk}u8,`);
  L.push(`    hs: ${f64(s.hs)},`);
  L.push(`    max_iter: ${s.maxIter === null ? "None" : `Some(${f64(s.maxIter)})`},`);
  L.push(`    starts: &[${s.starts.map(f64).join(", ")}],`);
  L.push(`    goal: ${f64(s.goal)},`);
  L.push(`    runs: ${s.name === "as_reuse" ? 2 : 1},`);
  L.push(
    `    path: ${s.path === "u" ? "None" : `Some(&[${s.path.map(f64).join(", ")}] as &[f64])`},`,
  );
  L.push(`    stamp_after: ${s.stampAfter}u64,`);
  L.push(`    closed: &[`);
  L.push(...numArr(s.closed, "u32", 16));
  L.push("],");
  L.push(`    gs_stamp: &[`);
  L.push(...numArr(s.gsStamp, "u32", 16));
  L.push("],");
  L.push(`    g_score: &[`);
  L.push(...numArr(s.gScore, "u32", 16));
  L.push("],");
  L.push(`    came_from: &[`);
  L.push(...numArr(s.cameFrom, "i32", 16));
  L.push("],");
  L.push("};");
  L.push("");
}
L.push("pub const ASTAR_SCENARIOS: &[AStarScenario] = &[");
for (const s of structures.astar) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// A* rail scenario: packed terrain bytes (GameMapImpl layout) fed to");
L.push("/// rail::TerrainMap + rail::RailAdapter, one multi-start findPath, the");
L.push("/// returned path (`None` = TS null) and the engine arrays after it.");
L.push("pub struct RailScenario {");
L.push("    pub name: &'static str,");
L.push("    pub w: f64,");
L.push("    pub h: f64,");
L.push("    pub terrain: &'static [u8],");
L.push("    pub starts: &'static [f64],");
L.push("    pub goal: f64,");
L.push("    pub path: Option<&'static [f64]>,");
L.push("    pub stamp_after: u64,");
L.push("    pub closed: &'static [u32],");
L.push("    pub gs_stamp: &'static [u32],");
L.push("    pub g_score: &'static [u32],");
L.push("    pub came_from: &'static [i32],");
L.push("}");
L.push("");
for (const s of structures.rail) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: RailScenario = RailScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    w: ${f64(s.w)},`);
  L.push(`    h: ${f64(s.h)},`);
  L.push(`    terrain: &[`);
  L.push(...numArr(s.terrain, "u8", 16));
  L.push("],");
  L.push(`    starts: &[${s.starts.map(f64).join(", ")}],`);
  L.push(`    goal: ${f64(s.goal)},`);
  L.push(
    `    path: ${s.path === "u" ? "None" : `Some(&[${s.path.map(f64).join(", ")}] as &[f64])`},`,
  );
  L.push(`    stamp_after: ${s.stampAfter}u64,`);
  L.push(`    closed: &[`);
  L.push(...numArr(s.closed, "u32", 16));
  L.push("],");
  L.push(`    gs_stamp: &[`);
  L.push(...numArr(s.gsStamp, "u32", 16));
  L.push("],");
  L.push(`    g_score: &[`);
  L.push(...numArr(s.gScore, "u32", 16));
  L.push("],");
  L.push(`    came_from: &[`);
  L.push(...numArr(s.cameFrom, "i32", 16));
  L.push("],");
  L.push("};");
  L.push("");
}
L.push("pub const RAIL_SCENARIOS: &[RailScenario] = &[");
for (const s of structures.rail) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// A* water scenario: packed terrain bytes (GameMapImpl layout) fed to");
L.push("/// water::AStarWater (config weight/iterations included), one multi-start");
L.push("/// findPath, the returned path (`None` = TS null) and the engine arrays.");
L.push("pub struct WaterScenario {");
L.push("    pub name: &'static str,");
L.push("    pub w: f64,");
L.push("    pub h: f64,");
L.push("    pub terrain: &'static [u8],");
L.push("    pub weight: f64,");
L.push("    pub max_iter: f64,");
L.push("    pub starts: &'static [f64],");
L.push("    pub goal: f64,");
L.push("    pub path: Option<&'static [f64]>,");
L.push("    pub stamp_after: u64,");
L.push("    pub closed: &'static [u32],");
L.push("    pub gs_stamp: &'static [u32],");
L.push("    pub g_score: &'static [u32],");
L.push("    pub came_from: &'static [i32],");
L.push("}");
L.push("");
for (const s of structures.water) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: WaterScenario = WaterScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    w: ${f64(s.w)},`);
  L.push(`    h: ${f64(s.h)},`);
  L.push(`    terrain: &[`);
  L.push(...numArr(s.terrain, "u8", 16));
  L.push("],");
  L.push(`    weight: ${f64(s.weight)},`);
  L.push(`    max_iter: ${f64(s.maxIter)},`);
  L.push(`    starts: &[${s.starts.map(f64).join(", ")}],`);
  L.push(`    goal: ${f64(s.goal)},`);
  L.push(
    `    path: ${s.path === "u" ? "None" : `Some(&[${s.path.map(f64).join(", ")}] as &[f64])`},`,
  );
  L.push(`    stamp_after: ${s.stampAfter}u64,`);
  L.push(`    closed: &[`);
  L.push(...numArr(s.closed, "u32", 16));
  L.push("],");
  L.push(`    gs_stamp: &[`);
  L.push(...numArr(s.gsStamp, "u32", 16));
  L.push("],");
  L.push(`    g_score: &[`);
  L.push(...numArr(s.gScore, "u32", 16));
  L.push("],");
  L.push(`    came_from: &[`);
  L.push(...numArr(s.cameFrom, "i32", 16));
  L.push("],");
  L.push("};");
  L.push("");
}
L.push("pub const WATER_SCENARIOS: &[WaterScenario] = &[");
for (const s of structures.water) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// A* bounded-water scenario: packed terrain bytes, the");
L.push("/// `maxSearchArea` constructor arg, config weight/iterations, one");
L.push("/// findPath (mode 0, bounds derived) or searchBounded (mode 1,");
L.push("/// explicit `bounds` = [minX,maxX,minY,maxY]; empty when mode 0),");
L.push("/// the returned path (`None` = TS null) and the engine's LOCAL");
L.push("/// stamp-tracked arrays. Twin: `water_bounded::AStarWaterBounded`.");
L.push("pub struct WaterBoundedScenario {");
L.push("    pub name: &'static str,");
L.push("    pub mode: u8,");
L.push("    pub w: f64,");
L.push("    pub h: f64,");
L.push("    pub terrain: &'static [u8],");
L.push("    pub max_area: f64,");
L.push("    pub weight: f64,");
L.push("    pub max_iter: f64,");
L.push("    pub starts: &'static [f64],");
L.push("    pub goal: f64,");
L.push("    pub bounds: &'static [f64],");
L.push("    pub path: Option<&'static [f64]>,");
L.push("    pub stamp_after: u64,");
L.push("    pub closed: &'static [u32],");
L.push("    pub gs_stamp: &'static [u32],");
L.push("    pub g_score: &'static [u32],");
L.push("    pub came_from: &'static [i32],");
L.push("}");
L.push("");
for (const s of structures.waterbounded) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: WaterBoundedScenario = WaterBoundedScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    mode: ${s.mode}u8,`);
  L.push(`    w: ${f64(s.w)},`);
  L.push(`    h: ${f64(s.h)},`);
  L.push("    terrain: &[");
  L.push(...numArr(s.terrain, "u8", 16));
  L.push("],");
  L.push(`    max_area: ${f64(s.maxArea)},`);
  L.push(`    weight: ${f64(s.weight)},`);
  L.push(`    max_iter: ${f64(s.maxIter)},`);
  L.push(`    starts: &[${s.starts.map(f64).join(", ")}],`);
  L.push(`    goal: ${f64(s.goal)},`);
  L.push(
    `    bounds: &[${s.bounds ? s.bounds.map(f64).join(", ") : ""}],`,
  );
  L.push(
    `    path: ${s.path === "u" ? "None" : `Some(&[${s.path.map(f64).join(", ")}] as &[f64])`},`,
  );
  L.push(`    stamp_after: ${s.stampAfter}u64,`);
  L.push(`    closed: &[`);
  L.push(...numArr(s.closed, "u32", 16));
  L.push("],");
  L.push(`    gs_stamp: &[`);
  L.push(...numArr(s.gsStamp, "u32", 16));
  L.push("],");
  L.push(`    g_score: &[`);
  L.push(...numArr(s.gScore, "u32", 16));
  L.push("],");
  L.push(`    came_from: &[`);
  L.push(...numArr(s.cameFrom, "i32", 16));
  L.push("],");
  L.push("};");
  L.push("");
}
L.push("pub const WATERBOUNDED_SCENARIOS: &[WaterBoundedScenario] = &[");
for (const s of structures.waterbounded) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// GameMap op result: void mutation, TS throw, `undefined` scalar,");
L.push("/// a number/boolean value, or a tile array (neighbours / searches).");
L.push("#[derive(Clone, Copy, Debug)]");
L.push("pub enum GmRes {");
L.push("    Void,");
L.push("    Threw,");
L.push("    Undef,");
L.push("    Val(f64),");
L.push("    Arr(&'static [f64]),");
L.push("}");
L.push("");
L.push("#[derive(Clone, Copy, Debug)]");
L.push("pub struct GmOp { pub kind: u8, pub a: f64, pub b: f64, pub res: GmRes }");
L.push("");
L.push("/// GameMapImpl scenario: initial packed terrain + land count, an op");
L.push("/// stream replayed against the real TS class (see gen_vectors.mjs for");
L.push("/// the kind table), and the final terrain/state buffers + counters.");
L.push("pub struct GameMapScenario {");
L.push("    pub name: &'static str,");
L.push("    pub w: f64,");
L.push("    pub h: f64,");
L.push("    pub terrain: &'static [u8],");
L.push("    pub num_land: f64,");
L.push("    pub ops: &'static [GmOp],");
L.push("    pub terrain_after: &'static [u8],");
L.push("    pub state_after: &'static [u16],");
L.push("    pub num_land_after: f64,");
L.push("    pub water_version_after: f64,");
L.push("    pub fallout_after: f64,");
L.push("}");
L.push("");
const gmResLit = (r) => {
  if (r === "v") return "GmRes::Void";
  if (r === "t") return "GmRes::Threw";
  if (r === "u") return "GmRes::Undef";
  if (Array.isArray(r)) {
    const items = r.map((v) => (v === "n" ? "f64::NAN" : v === "-0" ? "-0.0f64" : f64(v)));
    return `GmRes::Arr(&[${items.join(", ")}])`;
  }
  if (r === "n") return "GmRes::Val(f64::NAN)";
  if (r === "-0") return "GmRes::Val(-0.0f64)";
  return `GmRes::Val(${f64(r)})`;
};
for (const s of structures.gamemap) {
  const id = s.name.toUpperCase();
  L.push(`const ${id}_OPS: &[GmOp] = &[`);
  for (const [k, a, b, r] of s.ops)
    L.push(`    GmOp { kind: ${k}, a: ${argLit(a)}, b: ${argLit(b)}, res: ${gmResLit(r)} },`);
  L.push("];");
  L.push(`pub const ${id}: GameMapScenario = GameMapScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    w: ${f64(s.w)},`);
  L.push(`    h: ${f64(s.h)},`);
  L.push(`    terrain: &[`);
  L.push(...numArr(s.terrain, "u8", 16));
  L.push("],");
  L.push(`    num_land: ${f64(s.numLand)},`);
  L.push(`    ops: ${id}_OPS,`);
  L.push(`    terrain_after: &[`);
  L.push(...numArr(s.terrainAfter, "u8", 16));
  L.push("],");
  L.push(`    state_after: &[`);
  L.push(...numArr(s.stateAfter, "u16", 16));
  L.push("],");
  L.push(`    num_land_after: ${f64(s.numLandAfter)},`);
  L.push(`    water_version_after: ${f64(s.waterVersionAfter)},`);
  L.push(`    fallout_after: ${f64(s.falloutAfter)},`);
  L.push("};");
  L.push("");
}
L.push("pub const GAMEMAP_SCENARIOS: &[GameMapScenario] = &[");
for (const s of structures.gamemap) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// TileSet scenario: initial values, an op stream replayed against the");
L.push("/// real TS class (kind table in gen_vectors.mjs; GmOp/GmRes reused),");
L.push("/// and the final dense/table buffers + bookkeeping counters.");
L.push("pub struct TileSetScenario {");
L.push("    pub name: &'static str,");
L.push("    pub initial: &'static [f64],");
L.push("    pub ops: &'static [GmOp],");
L.push("    pub dense: &'static [u32],");
L.push("    pub dense_len: u64,");
L.push("    pub size: f64,");
L.push("    pub table: &'static [i32],");
L.push("    pub table_used: f64,");
L.push("    pub iter_depth: f64,");
L.push("}");
L.push("");
for (const s of structures.tileset) {
  const id = s.name.toUpperCase();
  L.push(`const ${id}_OPS: &[GmOp] = &[`);
  for (const [k, a, b, r] of s.ops)
    L.push(`    GmOp { kind: ${k}, a: ${argLit(a)}, b: ${argLit(b)}, res: ${gmResLit(r)} },`);
  L.push("];");
  L.push(`pub const ${id}: TileSetScenario = TileSetScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    initial: &[${s.initial.map(f64).join(", ")}],`);
  L.push(`    ops: ${id}_OPS,`);
  L.push(`    dense: &[`);
  L.push(...numArr(s.dense, "u32", 16));
  L.push("],");
  L.push(`    dense_len: ${s.denseLen}u64,`);
  L.push(`    size: ${f64(s.size)},`);
  L.push(`    table: &[`);
  L.push(...numArr(s.table, "i32", 16));
  L.push("],");
  L.push(`    table_used: ${f64(s.tableUsed)},`);
  L.push(`    iter_depth: ${f64(s.iterDepth)},`);
  L.push("};");
  L.push("");
}
L.push("pub const TILESET_SCENARIOS: &[TileSetScenario] = &[");
for (const s of structures.tileset) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// Util scenario: one call of a ported `Util.ts` function. `kind`");
L.push("/// selects the function (table in the capture section above), `args`");
L.push("/// carries its scalar inputs (variable-length payloads for the");
L.push("/// list-taking kinds; kinds 15/16 carry `[len, u0, ..]` UTF-16 code");
L.push("/// units), `strs` the strings for simpleHash, `status`");
L.push("/// 0 ok / 1 TS null / 2 TS threw, and `res` every f64 of the result");
L.push("/// (NaN and -0 are pinned by bit pattern, not by `==`).");
L.push("pub struct UtilScenario {");
L.push("    pub name: &'static str,");
L.push("    pub kind: u8,");
L.push("    pub args: &'static [f64],");
L.push("    pub strs: &'static [&'static str],");
L.push("    pub status: u8,");
L.push("    pub res: &'static [f64],");
L.push("}");
L.push("");
const utilResLit = (r) => {
  if (r === "n") return "f64::NAN";
  if (r === "-0") return "-0.0f64";
  if (r === "i") return "f64::INFINITY";
  if (r === "-i") return "f64::NEG_INFINITY";
  return f64(r);
};
for (const s of structures.util) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: UtilScenario = UtilScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    kind: ${s.kind}u8,`);
  L.push(`    args: &[${s.args.map((v) => utilResLit(v === undefined ? 0 : v)).join(", ")}],`);
  L.push(`    strs: &[${s.strs.map((x) => JSON.stringify(x)).join(", ")}],`);
  L.push(`    status: ${s.status}u8,`);
  L.push(`    res: &[${s.res.map(utilResLit).join(", ")}],`);
  L.push("};");
  L.push("");
}
L.push("pub const UTIL_SCENARIOS: &[UtilScenario] = &[");
for (const s of structures.util) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// One `PlayerInfo` as `assignTeams` sees it. `player_type`: 0 BOT,");
L.push("/// 1 HUMAN, 2 NATION. Empty `client_id` / `clan_tag` mean `null`");
L.push("/// (an empty clanTag is falsy anyway, so the two collapse).");
L.push("/// `team_index` is the raw `number | null` pin (NaN is a real");
L.push("/// token: JS `teams[NaN]` is undefined, i.e. unpinned).");
L.push("pub struct TeamPlayer {");
L.push("    pub id: &'static str,");
L.push("    pub player_type: u8,");
L.push("    pub client_id: &'static str,");
L.push("    pub clan_tag: &'static str,");
L.push("    pub friends: &'static [&'static str],");
L.push("    pub team_index: Option<f64>,");
L.push("}");
L.push("");
L.push("/// `TeamCountConfig`: kind 0 Num(num), 1 Duos, 2 Trios, 3 Quads,");
L.push("/// 4 HumansVsNations, 5 Other(str).");
L.push("pub struct TeamConfig { pub kind: u8, pub num: f64, pub s: &'static str }");
L.push("");
L.push("/// TeamAssignment scenario. `kind`: 0 assignTeams, 1");
L.push("/// assignTeamsLobbyPreview, 2 getMaxTeamSize, 3 resolveTeamsList.");
L.push("/// `status`: 0 ok, 1 Unknown config, 2 Too few teams, 3 RangeError.");
L.push("/// Kinds 0/1 fill `res` with the result map's insertion-ordered");
L.push("/// (player index, team index or -1) pairs; kind 2 fills `res_nums`");
L.push("/// with [max]; kind 3 fills `res_teams` with the resolved list.");
L.push("pub struct TeamScenario {");
L.push("    pub name: &'static str,");
L.push("    pub kind: u8,");
L.push("    pub players: &'static [TeamPlayer],");
L.push("    pub teams: &'static [&'static str],");
L.push("    pub is_duo: u8,");
L.push("    pub has_max: u8,");
L.push("    pub max_team_size: f64,");
L.push("    pub nation_count: u64,");
L.push("    pub config: TeamConfig,");
L.push("    pub total_players: f64,");
L.push("    pub status: u8,");
L.push("    pub res: &'static [(i64, i64)],");
L.push("    pub res_nums: &'static [f64],");
L.push("    pub res_teams: &'static [&'static str],");
L.push("}");
L.push("");
const ptCode = (t) => (t === "BOT" ? 0 : t === "HUMAN" ? 1 : 2);
for (const s of structures.team) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: TeamScenario = TeamScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    kind: ${s.kind}u8,`);
  L.push("    players: &[");
  for (const p of s.players) {
    const ti =
      p.teamIndex === null ? "None" : `Some(${utilResLit(p.teamIndex)})`;
    L.push(
      `        TeamPlayer { id: ${JSON.stringify(p.id)}, player_type: ${ptCode(p.playerType)}u8, ` +
        `client_id: ${p.clientID === null ? '""' : JSON.stringify(p.clientID)}, ` +
        `clan_tag: ${p.clanTag === null ? '""' : JSON.stringify(p.clanTag)}, ` +
        `friends: &[${p.friends.map((f) => JSON.stringify(f)).join(", ")}], ` +
        `team_index: ${ti} },`,
    );
  }
  L.push("    ],");
  L.push(`    teams: &[${s.teams.map((t) => JSON.stringify(t)).join(", ")}],`);
  L.push(`    is_duo: ${s.isDuo}u8,`);
  L.push(`    has_max: ${s.hasMax}u8,`);
  L.push(`    max_team_size: ${utilResLit(s.maxTeamSize)},`);
  L.push(`    nation_count: ${s.nationCount}u64,`);
  L.push(
    `    config: TeamConfig { kind: ${s.config.kind}u8, num: ${utilResLit(s.config.num)}, s: ${JSON.stringify(s.config.str)} },`,
  );
  L.push(`    total_players: ${utilResLit(s.totalPlayers)},`);
  L.push(`    status: ${s.status}u8,`);
  L.push(`    res: &[${s.res.map(([a, b]) => `(${a}i64, ${b}i64)`).join(", ")}],`);
  L.push(`    res_nums: &[${s.resNums.map(utilResLit).join(", ")}],`);
  L.push(`    res_teams: &[${s.resTeams.map((t) => JSON.stringify(t)).join(", ")}],`);
  L.push("};");
  L.push("");
}
L.push("pub const TEAM_SCENARIOS: &[TeamScenario] = &[");
for (const s of structures.team) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// Bezier scenario over `Line.ts`'s DistanceBasedBezierCurve. `kind`");
L.push("/// 0 = getLength (result in `len`); 1 = construct with `spacing`,");
L.push("/// capture the cached-point list (`points`, flat x/y) and replay the");
L.push("/// `incs` increment script (`walk`, flat (index, x, y) triples where");
L.push("/// index -1 marks the null end-of-curve), then `final_index`.");
L.push("pub struct BezierScenario {");
L.push("    pub name: &'static str,");
L.push("    pub kind: u8,");
L.push("    pub cp: &'static [f64],");
L.push("    pub spacing: f64,");
L.push("    pub incs: &'static [f64],");
L.push("    pub len: f64,");
L.push("    pub points: &'static [f64],");
L.push("    pub walk: &'static [f64],");
L.push("    pub final_index: u64,");
L.push("}");
L.push("");
for (const s of structures.bezier) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: BezierScenario = BezierScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    kind: ${s.kind}u8,`);
  L.push(`    cp: &[${s.cp.map(utilResLit).join(", ")}],`);
  L.push(`    spacing: ${utilResLit(s.spacing)},`);
  L.push(`    incs: &[${s.incs.map(utilResLit).join(", ")}],`);
  L.push(`    len: ${utilResLit(s.len)},`);
  L.push(`    points: &[${s.points.map(utilResLit).join(", ")}],`);
  L.push(`    walk: &[${s.walk.flat().map(utilResLit).join(", ")}],`);
  L.push(`    final_index: ${s.finalIndex}u64,`);
  L.push("};");
  L.push("");
}
L.push("pub const BEZIER_SCENARIOS: &[BezierScenario] = &[");
for (const s of structures.bezier) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// Veterancy scenario: one `maxHealthWithVeterancy(base, vet, pct)");
L.push("/// call with the recorded result. NaN / -0 / Infinity are pinned by");
L.push("/// literal, not by `==`.");
L.push("pub struct VeterancyScenario {");
L.push("    pub name: &'static str,");
L.push("    pub base: f64,");
L.push("    pub vet: f64,");
L.push("    pub pct: f64,");
L.push("    pub res: f64,");
L.push("}");
L.push("");
for (const s of structures.veterancy) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: VeterancyScenario = VeterancyScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    base: ${utilResLit(s.base)},`);
  L.push(`    vet: ${utilResLit(s.vet)},`);
  L.push(`    pct: ${utilResLit(s.pct)},`);
  L.push(`    res: ${utilResLit(s.res)},`);
  L.push("};");
  L.push("");
}
L.push("pub const VETERANCY_SCENARIOS: &[VeterancyScenario] = &[");
for (const s of structures.veterancy) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// MotionPlans scenario. `kind`: 0 roundtrip (pack `input` -> `words`");
L.push("/// -> unpack -> `out`), 1 unpack-only (feed `words` -> `out`).");
L.push("/// Record token stream: [count, per record");
L.push("/// 1,unitId,planId,startTick,ticksPerStep,pathLen,path... |");
L.push("/// 2,engineId,planId,startTick,speed,spacing,carCount,pathLen,");
L.push("/// cars...,path...]. All tokens f64; u32 wire values fit exactly.");
L.push("/// `input` is empty for kind 1. `words` is [len, ...buffer].");
L.push("pub struct MpScenario {");
L.push("    pub name: &'static str,");
L.push("    pub kind: u8,");
L.push("    pub input: &'static [f64],");
L.push("    pub words: &'static [f64],");
L.push("    pub out: &'static [f64],");
L.push("}");
L.push("");
const mpLit = (a) => `&[${a.map(utilResLit).join(", ")}]`;
for (const s of structures.motionplans) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: MpScenario = MpScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    kind: ${s.kind}u8,`);
  L.push(`    input: ${mpLit(s.input)},`);
  L.push(`    words: ${mpLit(s.words)},`);
  L.push(`    out: ${mpLit(s.out)},`);
  L.push("};");
  L.push("");
}
L.push("pub const MP_SCENARIOS: &[MpScenario] = &[");
for (const s of structures.motionplans) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// ConnectedComponents scenario: a packed-terrain map, an op stream");
L.push("/// replayed against the real TS class (kind table in gen_vectors.mjs;");
L.push("/// GmOp/GmRes reused), and the final internal buffers. `bits` is 0");
L.push("/// (componentIds still null), 8 (Uint8Array) or 16 (Uint16Array after");
L.push("/// the 253-component upgrade). `ids` is the componentIds buffer as");
L.push("/// numbers; `sizes` and `parents` render JS holes as NaN (`u`).");
L.push("pub struct CcScenario {");
L.push("    pub name: &'static str,");
L.push("    pub w: f64,");
L.push("    pub h: f64,");
L.push("    pub terrain: &'static [u8],");
L.push("    pub direct: u8,");
L.push("    pub ops: &'static [GmOp],");
L.push("    pub bits: u8,");
L.push("    pub ids: &'static [f64],");
L.push("    pub sizes: &'static [f64],");
L.push("    pub parents: &'static [f64],");
L.push("    pub max_id: f64,");
L.push("    pub land_marker: f64,");
L.push("}");
L.push("");
// Render a cc scalar token: "u" (a JS `undefined` hole) becomes NaN so the
// Rust side can compare it with Object.is semantics.
const ccLit = (r) => (r === "u" ? "f64::NAN" : utilResLit(r));
// Render a cc buffer as Rust f64 literals, 16 per line. Plain numbers get the
// `f64` suffix; "u" (undefined hole) becomes NaN.
const ccBuf = (xs) => {
  const lits = xs.map((v) => (v === "u" ? "f64::NAN" : `${v}f64`));
  const out = [];
  for (let i = 0; i < lits.length; i += 16)
    out.push("    " + lits.slice(i, i + 16).join(", ") + ",");
  return out;
};
for (const s of structures.connectedcomponents) {
  const id = s.name.toUpperCase();
  L.push(`const ${id}_OPS: &[GmOp] = &[`);
  for (const [k, a, b, r] of s.ops)
    L.push(`    GmOp { kind: ${k}, a: ${argLit(a)}, b: ${argLit(b)}, res: ${gmResLit(r)} },`);
  L.push("];");
  L.push(`pub const ${id}: CcScenario = CcScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    w: ${f64(s.w)},`);
  L.push(`    h: ${f64(s.h)},`);
  L.push(`    terrain: &[`);
  L.push(...numArr(s.terrain, "u8", 16));
  L.push("],");
  L.push(`    direct: ${s.direct}u8,`);
  L.push(`    ops: ${id}_OPS,`);
  L.push(`    bits: ${s.bits}u8,`);
  L.push("    ids: &[");
  L.push(...ccBuf(s.ids));
  L.push("],");
  L.push(`    sizes: &[`);
  L.push(...ccBuf(s.sizes));
  L.push("],");
  L.push(`    parents: &[`);
  L.push(...ccBuf(s.parents));
  L.push("],");
  L.push(`    max_id: ${ccLit(s.maxId)},`);
  L.push(`    land_marker: ${ccLit(s.landMarker)},`);
  L.push("};");
  L.push("");
}
L.push("pub const CC_SCENARIOS: &[CcScenario] = &[");
for (const s of structures.connectedcomponents) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// TerrainSearchMap scenario: the raw buffer bytes (4-byte header +");
L.push("/// packed tiles) and an op stream replayed against the real TS class");
L.push("/// (GmOp/GmRes reused; kind 0=getWidth, 1=getHeight, 2=node(x,y),");
L.push("/// 3=neighbors(x,y) -> flattened [x0,y0,...]; node args and neighbor");
L.push("/// coordinates may be NaN / infinite / fractional tokens).");
L.push("pub struct TsmScenario {");
L.push("    pub name: &'static str,");
L.push("    pub buffer: &'static [u8],");
L.push("    pub ops: &'static [GmOp],");
L.push("}");
L.push("");
for (const s of structures.terrainsearchmap) {
  const id = s.name.toUpperCase();
  L.push(`const ${id}_OPS: &[GmOp] = &[`);
  for (const [k, a, b, r] of s.ops)
    L.push(`    GmOp { kind: ${k}, a: ${utilResLit(a)}, b: ${utilResLit(b)}, res: ${gmResLit(r)} },`);
  L.push("];");
  L.push(`pub const ${id}: TsmScenario = TsmScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push("    buffer: &[");
  L.push(...numArr(s.buffer, "u8", 16));
  L.push("],");
  L.push(`    ops: ${id}_OPS,`);
  L.push("};");
  L.push("");
}
L.push("pub const TSM_SCENARIOS: &[TsmScenario] = &[");
for (const s of structures.terrainsearchmap) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// AbstractGraph scenario: a packed-terrain map + clusterSize, an op");
L.push("/// stream replayed against the real TS container after the builder");
L.push("/// runs (GmOp/GmRes reused; kind table in gen_vectors.mjs), and the");
L.push("/// final internal arrays. `nodes` is flattened 5-per-entry");
L.push("/// [id,x,y,tile,componentId]; `edges` 6-per-entry");
L.push("/// [id,nodeA,nodeB,cost,clusterX,clusterY]; `clusters`");
L.push("/// [x,y,count,ids...]; `nodeEdgeIds` [count,ids...]. `oldIdx` is the");
L.push("/// prior scenario whose built graph seeds a partial rebuild (-1 =");
L.push("/// fresh build); `dirty` are the dirty minimap tiles.");
L.push("pub struct AgScenario {");
L.push("    pub name: &'static str,");
L.push("    pub w: f64,");
L.push("    pub h: f64,");
L.push("    pub cluster_size: f64,");
L.push("    pub terrain: &'static [u8],");
L.push("    pub ops: &'static [GmOp],");
L.push("    pub node_count: f64,");
L.push("    pub edge_count: f64,");
L.push("    pub path_cache_len: f64,");
L.push("    pub nodes: &'static [f64],");
L.push("    pub edges: &'static [f64],");
L.push("    pub clusters: &'static [f64],");
L.push("    pub node_edge_ids: &'static [f64],");
L.push("    pub old_idx: i32,");
L.push("    pub dirty: &'static [f64],");
L.push("}");
L.push("");
// Render an ag flat token array: "u" (undefined) -> NaN, "n" -> NaN, "-0" ->
// -0.0, otherwise the f64 literal.
const agBuf = (xs) => {
  const lits = xs.map((v) => {
    if (v === "u" || v === "n") return "f64::NAN";
    if (v === "-0") return "-0.0f64";
    return `${v}f64`;
  });
  const out = [];
  for (let i = 0; i < lits.length; i += 16)
    out.push("    " + lits.slice(i, i + 16).join(", ") + ",");
  return out;
};
const agScalar = (v) => {
  if (v === "u" || v === "n") return "f64::NAN";
  if (v === "-0") return "-0.0f64";
  return `${v}f64`;
};
for (const s of structures.abstractgraph) {
  const id = s.name.toUpperCase();
  L.push(`const ${id}_OPS: &[GmOp] = &[`);
  for (const [k, a, b, r] of s.ops)
    L.push(`    GmOp { kind: ${k}, a: ${argLit(a)}, b: ${argLit(b)}, res: ${gmResLit(r)} },`);
  L.push("];");
  L.push(`pub const ${id}: AgScenario = AgScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    w: ${f64(s.w)},`);
  L.push(`    h: ${f64(s.h)},`);
  L.push(`    cluster_size: ${f64(s.clusterSize)},`);
  L.push("    terrain: &[");
  L.push(...numArr(s.terrain, "u8", 16));
  L.push("],");
  L.push(`    ops: ${id}_OPS,`);
  L.push(`    node_count: ${agScalar(s.nodeCount)},`);
  L.push(`    edge_count: ${agScalar(s.edgeCount)},`);
  L.push(`    path_cache_len: ${agScalar(s.pathCacheLen)},`);
  L.push("    nodes: &[");
  L.push(...agBuf(s.nodes));
  L.push("],");
  L.push("    edges: &[");
  L.push(...agBuf(s.edges));
  L.push("],");
  L.push("    clusters: &[");
  L.push(...agBuf(s.clusters));
  L.push("],");
  L.push("    node_edge_ids: &[");
  L.push(...agBuf(s.nodeEdgeIds));
  L.push("],");
  L.push(`    old_idx: ${s.oldIdx}i32,`);
  L.push(`    dirty: &[${s.dirty.map((d) => `${d}f64`).join(", ")}],`);
  L.push("};");
  L.push("");
}
L.push("pub const AG_SCENARIOS: &[AgScenario] = &[");
for (const s of structures.abstractgraph) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// One `findPath` query on the engine: the start (scalar ->");
L.push("/// `is_multi = 0`, array -> 1; a single-element array is recorded as");
L.push("/// the multi form and the Rust replays `find_path_multi`, matching");
L.push("/// the TS `Array.isArray` dispatch), the goal, the returned path");
L.push("/// (`None` = TS null) and the FULL engine state snapshotted after");
L.push("/// this query (stamp, the five node arrays with `gScore` as raw f32");
L.push("/// bits, and the live MinHeap arrays).");
L.push("pub struct AgaQuery {");
L.push("    pub is_multi: u8,");
L.push("    pub starts: &'static [f64],");
L.push("    pub goal: f64,");
L.push("    pub path: Option<&'static [f64]>,");
L.push("    pub stamp_after: u64,");
L.push("    pub closed: &'static [u32],");
L.push("    pub gs_stamp: &'static [u32],");
L.push("    pub g_score_bits: &'static [u32],");
L.push("    pub came_from: &'static [i32],");
L.push("    pub start_node: &'static [i32],");
L.push("    pub q_heap: &'static [i32],");
L.push("    pub q_pri_bits: &'static [u32],");
L.push("    pub q_size: i64,");
L.push("    pub q_cap: usize,");
L.push("}");
L.push("");
L.push("/// Hand-built abstract graph (node triples `id,x,y`; edge");
L.push("/// quadruples `id,nodeA,nodeB,cost`), the engine config, and the");
L.push("/// query script. Twin: `abstract_graph_astar::AbstractGraphAStar`.");
L.push("pub struct AbstractGraphAStarScenario {");
L.push("    pub name: &'static str,");
L.push("    pub num_nodes: f64,");
L.push("    pub edge_count: f64,");
L.push("    pub weight: f64,");
L.push("    pub max_iter: f64,");
L.push("    pub nodes: &'static [f64],");
L.push("    pub edges: &'static [f64],");
L.push("    pub queries: &'static [AgaQuery],");
L.push("}");
L.push("");
for (const s of structures.abstractgraphastar) {
  const id = s.name.toUpperCase();
  const qNames = [];
  s.queries.forEach((q, qi) => {
    const qn = `${id}_Q${qi}`;
    qNames.push(qn);
    L.push(`const ${qn}: AgaQuery = AgaQuery {`);
    L.push(`    is_multi: ${q.isMulti}u8,`);
    L.push(`    starts: &[${q.starts.map(f64).join(", ")}],`);
    L.push(`    goal: ${f64(q.goal)},`);
    L.push(
      `    path: ${q.path === "u" ? "None" : `Some(&[${q.path.map(f64).join(", ")}] as &[f64])`},`,
    );
    L.push(`    stamp_after: ${q.stampAfter}u64,`);
    L.push("    closed: &[");
    L.push(...numArr(q.closed, "u32", 16));
    L.push("],");
    L.push("    gs_stamp: &[");
    L.push(...numArr(q.gsStamp, "u32", 16));
    L.push("],");
    L.push("    g_score_bits: &[");
    L.push(...numArr(q.gScoreBits, "u32", 12));
    L.push("],");
    L.push("    came_from: &[");
    L.push(...numArr(q.cameFrom, "i32", 16));
    L.push("],");
    L.push("    start_node: &[");
    L.push(...numArr(q.startNode, "i32", 16));
    L.push("],");
    L.push("    q_heap: &[");
    L.push(...numArr(q.qHeap, "i32", 16));
    L.push("],");
    L.push("    q_pri_bits: &[");
    L.push(...numArr(q.qPriBits, "u32", 12));
    L.push("],");
    L.push(`    q_size: ${q.qSize}i64,`);
    L.push(`    q_cap: ${q.qCap},`);
    L.push("};");
    L.push("");
  });
  L.push(`pub const ${id}: AbstractGraphAStarScenario = AbstractGraphAStarScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    num_nodes: ${f64(s.numNodes)},`);
  L.push(`    edge_count: ${f64(s.edgeCount)},`);
  L.push(`    weight: ${f64(s.weight)},`);
  L.push(`    max_iter: ${f64(s.maxIter)},`);
  L.push("    nodes: &[");
  L.push(...numArr(s.nodes, "f64", 12));
  L.push("],");
  L.push("    edges: &[");
  L.push(...numArr(s.edges, "f64", 12));
  L.push("],");
  L.push(`    queries: &[${qNames.join(", ")}],`);
  L.push("};");
  L.push("");
}
L.push("pub const ABSTRACT_GRAPH_ASTAR_SCENARIOS: &[AbstractGraphAStarScenario] = &[");
for (const s of structures.abstractgraphastar) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// One hierarchical query: the dispatch flag (1 = array start ->");
L.push("/// multi-source), the start(s), the goal, the returned path (`None` =");
L.push("/// TS null), the five engine stamps after the query (BFS / local /");
L.push("/// multi-cluster / short-path / abstract A* — the dispatch witness),");
L.push("/// and, for cachePaths scenarios, the graph path cache flattened per");
L.push("/// slot as [-1] (null) or [len, tiles...]. `rebuild_before` = 1 means");
L.push("/// setGraph(fresh rebuild) runs before this query.");
L.push("#[derive(Clone, Copy, Debug)]");
L.push("pub struct WhQuery {");
L.push("    pub rebuild_before: u8,");
L.push("    pub is_multi: u8,");
L.push("    pub starts: &'static [f64],");
L.push("    pub goal: f64,");
L.push("    pub path: Option<&'static [f64]>,");
L.push("    pub bfs: u64,");
L.push("    pub local: u64,");
L.push("    pub multi: u64,");
L.push("    pub short: u64,");
L.push("    pub aga: u64,");
L.push("    pub cache: &'static [f64],");
L.push("}");
L.push("");
L.push("/// A real GameMapImpl + AbstractGraphBuilder graph, the cachePaths");
L.push("/// flag, and a query script. Twin:");
L.push("/// `water_hierarchical::AStarWaterHierarchical`.");
L.push("pub struct WaterHierarchicalScenario {");
L.push("    pub name: &'static str,");
L.push("    pub w: f64,");
L.push("    pub h: f64,");
L.push("    pub cluster_size: f64,");
L.push("    pub terrain: &'static [u8],");
L.push("    pub cache_paths: u8,");
L.push("    pub queries: &'static [WhQuery],");
L.push("}");
L.push("");
for (const s of structures.waterhierarchical) {
  const id = s.name.toUpperCase();
  const qNames = [];
  s.queries.forEach((q, qi) => {
    const qn = `${id}_Q${qi}`;
    qNames.push(qn);
    L.push(`const ${qn}: WhQuery = WhQuery {`);
    L.push(`    rebuild_before: ${q.rebuildBefore}u8,`);
    L.push(`    is_multi: ${q.isMulti}u8,`);
    L.push(`    starts: &[${q.starts.map(f64).join(", ")}],`);
    L.push(`    goal: ${f64(q.goal)},`);
    L.push(
      `    path: ${q.path === "u" ? "None" : `Some(&[${q.path.map(f64).join(", ")}] as &[f64])`},`,
    );
    L.push(`    bfs: ${q.bfs}u64,`);
    L.push(`    local: ${q.local}u64,`);
    L.push(`    multi: ${q.multi}u64,`);
    L.push(`    short: ${q.short}u64,`);
    L.push(`    aga: ${q.aga}u64,`);
    L.push("    cache: &[");
    L.push(...numArr(q.cache, "f64", 12));
    L.push("],");
    L.push("};");
    L.push("");
  });
  L.push(`pub const ${id}: WaterHierarchicalScenario = WaterHierarchicalScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    w: ${f64(s.w)},`);
  L.push(`    h: ${f64(s.h)},`);
  L.push(`    cluster_size: ${f64(s.clusterSize)},`);
  L.push("    terrain: &[");
  L.push(...numArr(s.terrain, "u8", 16));
  L.push("],");
  L.push(`    cache_paths: ${s.cachePaths}u8,`);
  L.push(`    queries: &[${qNames.join(", ")}],`);
  L.push("};");
  L.push("");
}
L.push("pub const WATER_HIERARCHICAL_SCENARIOS: &[WaterHierarchicalScenario] = &[");
for (const s of structures.waterhierarchical) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// Parabola scenario (PathFinder.Parabola.ts). Option fields are");
L.push("/// tri-state u8: 0 = absent (undefined), 1 = false, 2 = true;");
L.push("/// `increment` -1 = absent. `cps` is groups of 10: [from, to,");
L.push("/// p0x,p0y,p1x,p1y,p2x,p2y,p3x,p3y]. `finds` is variable groups");
L.push("/// [from, to, len, tiles...] (len -1 = threw out of bounds). `walk`");
L.push("/// is 8-slot groups: [kind, from, to, has_speed, speed, status,");
L.push("/// node, index]; kind 0 = next ok, 1 = next threw (script ends),");
L.push("/// 2 = invalidate, 3 = currentIndex read.");
L.push("pub struct ParabolaScenario {");
L.push("    pub name: &'static str,");
L.push("    pub w: f64,");
L.push("    pub h: f64,");
L.push("    pub increment: f64,");
L.push("    pub distance_based_height: u8,");
L.push("    pub direction_up: u8,");
L.push("    pub ignore_map_bounds: u8,");
L.push("    pub cps: &'static [f64],");
L.push("    pub finds: &'static [f64],");
L.push("    pub walk: &'static [f64],");
L.push("}");
L.push("");
const tri = (v) => (v === undefined || v === null ? 0 : v ? 2 : 1);
for (const s of structures.parabola) {
  const id = s.name.toUpperCase();
  const o = s.opt ?? {};
  L.push(`pub const ${id}: ParabolaScenario = ParabolaScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    w: ${f64(s.w)},`);
  L.push(`    h: ${f64(s.h)},`);
  L.push(`    increment: ${o.increment === undefined ? "-1.0f64" : f64(o.increment)},`);
  L.push(`    distance_based_height: ${tri(o.distanceBasedHeight)}u8,`);
  L.push(`    direction_up: ${tri(o.directionUp)}u8,`);
  L.push(`    ignore_map_bounds: ${tri(o.ignoreMapBounds)}u8,`);
  L.push("    cps: &[");
  L.push(...numArr(s.cps.flat(), "f64", 10));
  L.push("],");
  L.push("    finds: &[");
  L.push(...numArr(s.finds.flat(), "f64", 10));
  L.push("],");
  L.push("    walk: &[");
  L.push(...numArr(s.walk.flat(), "f64", 8));
  L.push("],");
  L.push("};");
  L.push("");
}
L.push("pub const PARABOLA_SCENARIOS: &[ParabolaScenario] = &[");
for (const s of structures.parabola) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// MiniMapTransformer scenario (transformers/MiniMapTransformer.ts).");
L.push("/// `groups` is variable-length query groups: [from_is_array, from_len,");
L.push("/// from_tiles..., to, inner_mode, inner_len, inner_tiles...,");
L.push("/// seen_flag, [is_multi, seen_len, seen_tiles..., seen_goal],");
L.push("/// out_mode, [out_len, out_tiles...]]. from/out tiles are main refs,");
L.push("/// inner/seen tiles mini refs. inner_mode 0 = null, 1 = empty, 2 =");
L.push("/// list. seen_flag 0 = inner never called (downscale threw). out_mode");
L.push("/// 0 = null, 1 = threw (upscale out of bounds), 2 = path.");
L.push("pub struct MiniMapTransformerScenario {");
L.push("    pub name: &'static str,");
L.push("    pub mw: f64,");
L.push("    pub mh: f64,");
L.push("    pub mini_w: f64,");
L.push("    pub mini_h: f64,");
L.push("    pub groups: &'static [f64],");
L.push("}");
L.push("");
for (const s of structures.minimaptransformer) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: MiniMapTransformerScenario = MiniMapTransformerScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    mw: ${f64(s.mw)},`);
  L.push(`    mh: ${f64(s.mh)},`);
  L.push(`    mini_w: ${f64(s.miniW)},`);
  L.push(`    mini_h: ${f64(s.miniH)},`);
  L.push("    groups: &[");
  L.push(...numArr(s.groups.flat(), "f64", 10));
  L.push("],");
  L.push("};");
  L.push("");
}
L.push("pub const MMT_SCENARIOS: &[MiniMapTransformerScenario] = &[");
for (const s of structures.minimaptransformer) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// PathFinderStepper scenario (PathFinderStepper.ts). `prod` selects");
L.push("/// the tileStepperConfig shape (preCheck + manhattan distance) vs the");
L.push("/// default bare config. `ops` is a flat script:");
L.push("/// [0, from, to, dist, status, node, pan_len, tiles..., idx,");
L.push("/// has_path, calls] = next (dist -1 = undefined, node -1 = NotFound,");
L.push("/// pan_len -1 = null pathAfterNext, status 1 = threw); [1] =");
L.push("/// invalidate; [2] = stub reset (queue + observation, call count");
L.push("/// persists); [3, is_multi, from_len, from_tiles..., to, out_mode,");
L.push("/// out_len, out_tiles..., seen_flag, [seen_multi, seen_len,");
L.push("/// seen_tiles..., seen_goal], calls] = findPath (out_mode 0 = null,");
L.push("/// 1 = threw, 2 = list; seen_flag 0 = inner not called); [4, count,");
L.push("/// (null_flag | 1, len, tiles...)...] = queue inner results.");
L.push("pub struct StepperScenario {");
L.push("    pub name: &'static str,");
L.push("    pub prod: bool,");
L.push("    pub ops: &'static [f64],");
L.push("}");
L.push("");
for (const s of structures.stepper) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: StepperScenario = StepperScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    prod: ${s.prod},`);
  L.push("    ops: &[");
  L.push(...numArr(s.ops, "f64", 10));
  L.push("],");
  L.push("};");
  L.push("");
}
L.push("pub const STEPPER_SCENARIOS: &[StepperScenario] = &[");
for (const s of structures.stepper) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// ComponentCheckTransformer scenario (ComponentCheckTransformer.ts).");
L.push("/// `table` is a flat [ref, component_id] pair list; `default` is the");
L.push("/// component id for unlisted tiles. The transformer passes inner's");
L.push("/// result through unchanged, so output is derived from (seen_flag,");
L.push("/// inner_mode). `groups` is a flat script: [is_multi, from_len,");
L.push("/// from_refs..., to, inner_mode, inner_len, inner_refs..., seen_flag,");
L.push("/// [seen_multi, seen_len, seen_refs..., seen_goal]] (inner_mode 0 =");
L.push("/// null, 2 = list; seen_flag 0 = inner never called => output null,");
L.push("/// 1 = inner called => output = inner result).");
L.push("pub struct ComponentCheckScenario {");
L.push("    pub name: &'static str,");
L.push("    pub table: &'static [f64],");
L.push("    pub default: f64,");
L.push("    pub groups: &'static [f64],");
L.push("}");
L.push("");
for (const s of structures.componentcheck) {
  const id = s.name.toUpperCase();
  const pairs = Object.entries(s.table).flatMap(([k, v]) => [Number(k), v]);
  L.push(`pub const ${id}: ComponentCheckScenario = ComponentCheckScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push("    table: &[");
  L.push(...numArr(pairs, "f64", 10));
  L.push("],");
  L.push(`    default: ${s.default}f64,`);
  L.push("    groups: &[");
  L.push(...numArr(s.groups.flat(), "f64", 10));
  L.push("],");
  L.push("};");
  L.push("");
}
L.push("pub const COMPONENT_CHECK_SCENARIOS: &[ComponentCheckScenario] = &[");
for (const s of structures.componentcheck) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// ShoreCoercingTransformer scenario (ShoreCoercingTransformer.ts).");
L.push("/// Map is `w x h`, default land (0x83), `water` is a flat [x, y] list");
L.push("/// flipped to 0x03. `groups` is a flat script: [is_multi, from_len,");
L.push("/// from_refs..., to, inner_mode, inner_len, inner_refs...,");
L.push("/// seen_flag, [seen_multi, seen_len, seen_refs..., seen_goal],");
L.push("/// out_mode, [out_len, out_refs...]] (inner_mode 0 = null, 1 = [],");
L.push("/// 2 = list; seen_flag 0 = inner never called; out_mode 0 = null,");
L.push("/// 2 = path).");
L.push("pub struct ShoreCoercingScenario {");
L.push("    pub name: &'static str,");
L.push("    pub w: f64,");
L.push("    pub h: f64,");
L.push("    pub water: &'static [f64],");
L.push("    pub groups: &'static [f64],");
L.push("}");
L.push("");
for (const s of structures.shorecoercing) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: ShoreCoercingScenario = ShoreCoercingScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    w: ${s.w}f64,`);
  L.push(`    h: ${s.h}f64,`);
  L.push("    water: &[");
  L.push(...numArr(s.water.flat(), "f64", 10));
  L.push("],");
  L.push("    groups: &[");
  L.push(...numArr(s.groups.flat(), "f64", 10));
  L.push("],");
  L.push("};");
  L.push("");
}
L.push("pub const SHORE_COERCING_SCENARIOS: &[ShoreCoercingScenario] = &[");
for (const s of structures.shorecoercing) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// SmoothingWaterTransformer scenario (SmoothingWaterTransformer.ts).");
L.push("/// Map is `w x h`, default land (0x83); `cells` is a flat [x, y, byte]");
L.push("/// list setting explicit terrain magnitudes (deep 0x0B = mag 11, shallow");
L.push("/// 0x02 = mag 2). `groups` uses the same flat script as ShoreCoercing:");
L.push("/// [is_multi, from_len, from_refs..., to, inner_mode, inner_len,");
L.push("/// inner_refs..., seen_flag, [seen_multi, seen_len, seen_refs...,");
L.push("/// seen_goal], out_mode, [out_len, out_refs...]].");
L.push("pub struct SmoothingWaterScenario {");
L.push("    pub name: &'static str,");
L.push("    pub w: f64,");
L.push("    pub h: f64,");
L.push("    pub cells: &'static [f64],");
L.push("    pub groups: &'static [f64],");
L.push("}");
L.push("");
for (const s of structures.smoothingwater) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: SmoothingWaterScenario = SmoothingWaterScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    w: ${s.w}f64,`);
  L.push(`    h: ${s.h}f64,`);
  L.push("    cells: &[");
  L.push(...numArr(s.cells.flat(), "f64", 10));
  L.push("],");
  L.push("    groups: &[");
  L.push(...numArr(s.groups.flat(), "f64", 10));
  L.push("],");
  L.push("};");
  L.push("");
}
L.push("pub const SMOOTHING_WATER_SCENARIOS: &[SmoothingWaterScenario] = &[");
for (const s of structures.smoothingwater) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// Generic BFS scenario (BFS.ts). `edges` is a flat SameValueZero-keyed");
L.push("/// table: entry i is [key, nb_0, .., nb_{deg_i-1}] with the key count");
L.push("/// carried separately in `edge_degrees`. Tokens n/-0/i/-i decode to");
L.push("/// NaN/-0/+Inf/-Inf. `visits` is the flat [node, dist] visitor stream;");
L.push("/// `has_result`/`result` carry the search return (null -> false).");
L.push("pub struct BfsTsScenario {");
L.push("    pub name: &'static str,");
L.push("    pub edges: &'static [f64],");
L.push("    pub edge_degrees: &'static [usize],");
L.push("    pub starts: &'static [f64],");
L.push("    pub max_d: f64,");
L.push("    pub mode: u8,");
L.push("    pub blocker: f64,");
L.push("    pub foundval: f64,");
L.push("    pub visits: &'static [f64],");
L.push("    pub has_result: bool,");
L.push("    pub result: f64,");
L.push("}");
L.push("");
const bfsTok = (v) => {
  if (v === "n") return "f64::NAN";
  if (v === "-0") return "-0.0f64";
  if (v === "i") return "f64::INFINITY";
  if (v === "-i") return "f64::NEG_INFINITY";
  return f64(v);
};
for (const s of structures.bfs) {
  const id = s.name.toUpperCase();
  const flat = [];
  const degrees = [];
  for (const [k, nb] of s.edges) {
    flat.push(bfsTok(k));
    degrees.push(nb.length);
    for (const n of nb) flat.push(bfsTok(n));
  }
  L.push(`pub const ${id}: BfsTsScenario = BfsTsScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    edges: &[${flat.join(", ")}],`);
  L.push(`    edge_degrees: &[${degrees.map((d) => `${d}usize`).join(", ")}],`);
  L.push(`    starts: &[${s.starts.map(bfsTok).join(", ")}],`);
  L.push(`    max_d: ${bfsTok(s.maxd)},`);
  L.push(`    mode: ${s.mode}u8,`);
  L.push(`    blocker: ${bfsTok(s.blocker)},`);
  L.push(`    foundval: ${bfsTok(s.foundval)},`);
  L.push(`    visits: &[${s.visits.flat().map(bfsTok).join(", ")}],`);
  L.push(`    has_result: ${s.result !== "u"},`);
  L.push(`    result: ${s.result === "u" ? "0.0f64" : bfsTok(s.result)},`);
  L.push("};");
  L.push("");
}
L.push("pub const BFS_TS_SCENARIOS: &[BfsTsScenario] = &[");
for (const s of structures.bfs) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// AirPathFinder scenario (PathFinder.Air.ts). Walks from `from` to");
L.push("/// `to` on a `w x h` map seeded with `ticks`. `path` is the recorded");
L.push("/// (x, y) coordinate stream (refs are width-dependent); `threw` marks");
L.push("/// the Array.isArray multi-start or an out-of-range `game.ref`.");
L.push("pub struct AirScenario {");
L.push("    pub name: &'static str,");
L.push("    pub w: f64,");
L.push("    pub h: f64,");
L.push("    pub ticks: f64,");
L.push("    pub from: f64,");
L.push("    pub to: f64,");
L.push("    pub multi: bool,");
L.push("    pub threw: bool,");
L.push("    pub path: &'static [f64],");
L.push("}");
L.push("");
const airTok = (v) => {
  if (v === "n") return "f64::NAN";
  if (v === "-0") return "-0.0f64";
  if (v === "i") return "f64::INFINITY";
  if (v === "-i") return "f64::NEG_INFINITY";
  return f64(v);
};
for (const s of structures.air) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: AirScenario = AirScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    w: ${f64(s.w)},`);
  L.push(`    h: ${f64(s.h)},`);
  L.push(`    ticks: ${airTok(s.ticks)},`);
  L.push(`    from: ${airTok(s.from)},`);
  L.push(`    to: ${airTok(s.to)},`);
  L.push(`    multi: ${s.multi},`);
  L.push(`    threw: ${s.threw},`);
  L.push(`    path: &[${s.path.flat().map((v) => `${v}f64`).join(", ")}],`);
  L.push("};");
  L.push("");
}
L.push("pub const AIR_SCENARIOS: &[AirScenario] = &[");
for (const s of structures.air) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// anonWordName scenario (AnonNames.ts). `slot`/`offset` decode the");
L.push("/// uenc tokens; `has` 0 = offset defaulted (TS default param). `res`");
L.push("/// is the returned handle, or `None` for the JS `undefined` a missed");
L.push("/// word lookup returns when round === 0.");
L.push("pub struct AnonScenario {");
L.push("    pub name: &'static str,");
L.push("    pub slot: f64,");
L.push("    pub has: u8,");
L.push("    pub offset: f64,");
L.push("    pub res: Option<&'static str>,");
L.push("}");
L.push("");
const anonTok = (v) => {
  if (v === "n") return "f64::NAN";
  if (v === "-0") return "-0.0f64";
  if (v === "i") return "f64::INFINITY";
  if (v === "-i") return "f64::NEG_INFINITY";
  return f64(v);
};
for (const s of structures.anon) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: AnonScenario = AnonScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    slot: ${anonTok(s.slot)},`);
  L.push(`    has: ${s.has}u8,`);
  L.push(`    offset: ${s.has ? anonTok(s.offset) : "0.0f64"},`);
  L.push(
    `    res: ${s.res === "u" ? "None" : `Some(${JSON.stringify(s.res)})`},`,
  );
  L.push("};");
  L.push("");
}
L.push("pub const ANON_SCENARIOS: &[AnonScenario] = &[");
for (const s of structures.anon) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// CloseCodes scenario. `kind`: 0 isTerminalClose(`code`),");
L.push("/// 1 isCloseReason(`val`). `res` is the boolean verdict.");
L.push("pub struct CloseScenario {");
L.push("    pub name: &'static str,");
L.push("    pub kind: u8,");
L.push("    pub code: f64,");
L.push("    pub val: &'static str,");
L.push("    pub res: bool,");
L.push("}");
L.push("");
const closeTok = (v) => {
  if (v === "n") return "f64::NAN";
  if (v === "-0") return "-0.0f64";
  if (v === "i") return "f64::INFINITY";
  if (v === "-i") return "f64::NEG_INFINITY";
  return f64(v);
};
for (const s of structures.close) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: CloseScenario = CloseScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    kind: ${s.kind}u8,`);
  L.push(`    code: ${s.kind === 0 ? closeTok(s.code) : "0.0f64"},`);
  L.push(`    val: ${JSON.stringify(s.val)},`);
  L.push(`    res: ${s.res},`);
  L.push("};");
  L.push("");
}
L.push("pub const CLOSE_SCENARIOS: &[CloseScenario] = &[");
for (const s of structures.close) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// ServerList scenario: one `server_list::run_op(kind, args)` call.");
L.push("/// `args` / `res` are flat f64 token streams (strings as");
L.push("/// `[len, u0, ..]`, output `string|null` as `[-1]` or the string).");
L.push("pub struct SlScenario {");
L.push("    pub name: &'static str,");
L.push("    pub kind: u8,");
L.push("    pub args: &'static [f64],");
L.push("    pub res: &'static [f64],");
L.push("}");
L.push("");
for (const s of structures.serverlist) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: SlScenario = SlScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    kind: ${s.kind}u8,`);
  L.push(`    args: &[${s.args.map(utilResLit).join(", ")}],`);
  L.push(`    res: &[${s.res.map(utilResLit).join(", ")}],`);
  L.push("};");
  L.push("");
}
L.push("pub const SL_SCENARIOS: &[SlScenario] = &[");
for (const s of structures.serverlist) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// PatternDecoder scenario: one `pattern_decoder::run_op(kind, args)` call.");
L.push("/// `args` is `[len, b0, .., x, y]` (kind 1) or `[len, b0, ..]` (kind 0);");
L.push("/// `res` is the flat result token stream (see `pattern_decoder::run_op`).");
L.push("pub struct PdScenario {");
L.push("    pub name: &'static str,");
L.push("    pub kind: u8,");
L.push("    pub args: &'static [f64],");
L.push("    pub res: &'static [f64],");
L.push("}");
L.push("");
for (const s of structures.patterndecoder) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: PdScenario = PdScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    kind: ${s.kind}u8,`);
  L.push(`    args: &[${s.args.map(utilResLit).join(", ")}],`);
  L.push(`    res: &[${s.res.map(utilResLit).join(", ")}],`);
  L.push("};");
  L.push("");
}
L.push("pub const PD_SCENARIOS: &[PdScenario] = &[");
for (const s of structures.patterndecoder) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// DoomsdayClock scenario: one `doomsday_clock::run_op(kind, args)` call.");
L.push("pub struct DcScenario {");
L.push("    pub name: &'static str,");
L.push("    pub kind: u8,");
L.push("    pub args: &'static [f64],");
L.push("    pub res: &'static [f64],");
L.push("}");
L.push("");
for (const s of structures.doomsdayclock) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: DcScenario = DcScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    kind: ${s.kind}u8,`);
  L.push(`    args: &[${s.args.map(utilResLit).join(", ")}],`);
  L.push(`    res: &[${s.res.map(utilResLit).join(", ")}],`);
  L.push("};");
  L.push("");
}
L.push("pub const DC_SCENARIOS: &[DcScenario] = &[");
for (const s of structures.doomsdayclock) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// execution/Util.ts scenario: a packed-terrain GameMap, owner writes,");
L.push("/// then one `exec_util::run_op(gm, kind, args)` call. `owners` are");
L.push("/// `(tile, playerId)` pairs applied via `set_owner_id` before the op.");
L.push("pub struct EuScenario {");
L.push("    pub name: &'static str,");
L.push("    pub w: f64,");
L.push("    pub h: f64,");
L.push("    pub terrain: &'static [u8],");
L.push("    pub owners: &'static [(f64, f64)],");
L.push("    pub kind: u8,");
L.push("    pub args: &'static [f64],");
L.push("    pub res: &'static [f64],");
L.push("}");
L.push("");
for (const s of structures.executil) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: EuScenario = EuScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    w: ${f64(s.w)},`);
  L.push(`    h: ${f64(s.h)},`);
  L.push(`    terrain: &[`);
  L.push(...numArr(s.terrain, "u8", 16));
  L.push("],");
  L.push(`    owners: &[${s.owners.map(([t, i]) => `(${utilResLit(t)}, ${utilResLit(i)})`).join(", ")}],`);
  L.push(`    kind: ${s.kind}u8,`);
  L.push(`    args: &[${s.args.map(utilResLit).join(", ")}],`);
  L.push(`    res: &[${s.res.map(utilResLit).join(", ")}],`);
  L.push("};");
  L.push("");
}
L.push("pub const EU_SCENARIOS: &[EuScenario] = &[");
for (const s of structures.executil) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// WaterManager scenario: two packed GameMaps (full + minimap), an");
L.push("/// op stream replayed against the real TS class (kind table in");
L.push("/// gen_vectors.mjs; per-op result streams recorded), and the final");
L.push("/// terrain/state buffers of both maps plus the graph version.");
L.push("pub struct WmOp { pub kind: u8, pub a: f64, pub b: f64, pub res: &'static [f64] }");
L.push("pub struct WmScenario {");
L.push("    pub name: &'static str,");
L.push("    pub mw: f64,");
L.push("    pub mh: f64,");
L.push("    pub map_terrain: &'static [u8],");
L.push("    pub map_state: &'static [u16],");
L.push("    pub nw: f64,");
L.push("    pub nh: f64,");
L.push("    pub mini_terrain: &'static [u8],");
L.push("    pub disable: bool,");
L.push("    pub ops: &'static [WmOp],");
L.push("    pub map_terrain_after: &'static [u8],");
L.push("    pub map_state_after: &'static [u16],");
L.push("    pub mini_terrain_after: &'static [u8],");
L.push("    pub version_after: f64,");
L.push("}");
L.push("");
for (const s of structures.watermanager) {
  const id = s.name.toUpperCase();
  L.push(`const ${id}_OPS: &[WmOp] = &[`);
  for (const [k, a, b, r] of s.ops)
    L.push(
      `    WmOp { kind: ${k}, a: ${argLit(a)}, b: ${argLit(b)}, res: &[${r.map(utilResLit).join(", ")}] },`,
    );
  L.push("];");
  L.push(`pub const ${id}: WmScenario = WmScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    mw: ${f64(s.mw)},`);
  L.push(`    mh: ${f64(s.mh)},`);
  L.push(`    map_terrain: &[`);
  L.push(...numArr(s.mapTerrain, "u8", 16));
  L.push("],");
  L.push(`    map_state: &[`);
  L.push(...numArr(s.mapState, "u16", 16));
  L.push("],");
  L.push(`    nw: ${f64(s.nw)},`);
  L.push(`    nh: ${f64(s.nh)},`);
  L.push(`    mini_terrain: &[`);
  L.push(...numArr(s.miniTerrain, "u8", 16));
  L.push("],");
  L.push(`    disable: ${s.disable},`);
  L.push(`    ops: ${id}_OPS,`);
  L.push(`    map_terrain_after: &[`);
  L.push(...numArr(s.mapTerrainAfter, "u8", 16));
  L.push("],");
  L.push(`    map_state_after: &[`);
  L.push(...numArr(s.mapStateAfter, "u16", 16));
  L.push("],");
  L.push(`    mini_terrain_after: &[`);
  L.push(...numArr(s.miniTerrainAfter, "u8", 16));
  L.push("],");
  L.push(`    version_after: ${f64(s.versionAfter)},`);
  L.push("};");
  L.push("");
}
L.push("pub const WM_SCENARIOS: &[WmScenario] = &[");
for (const s of structures.watermanager) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// GameUpdateUtils scenario: one `game_update_utils::run_op(kind, args)` call.");
L.push("/// `args` / `res` are flat f64 token streams (see the Rust module docs).");
L.push("pub struct GuScenario {");
L.push("    pub name: &'static str,");
L.push("    pub kind: u8,");
L.push("    pub args: &'static [f64],");
L.push("    pub res: &'static [f64],");
L.push("}");
L.push("");
for (const s of structures.gameupdateutils) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: GuScenario = GuScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    kind: ${s.kind}u8,`);
  L.push(`    args: &[${s.args.map(utilResLit).join(", ")}],`);
  L.push(`    res: &[${s.res.map(utilResLit).join(", ")}],`);
  L.push("};");
  L.push("");
}
L.push("pub const GU_SCENARIOS: &[GuScenario] = &[");
for (const s of structures.gameupdateutils) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// One `Railroad.ts` scenario through the shared `run_op` runner.");
L.push("/// `kind` 0 = getClosestTileIndex `[width, to, n, tiles…]` → `[index]`;");
L.push("/// 1 = getOrientedRailroad `[to, k, (nbr,rr)*k, m, (railroad)*m]` → `[0]`");
L.push("/// or `[1, forward, n, tiles…, start, end]`; 2 = delete `[railroad]` →");
L.push("/// `[type, id, caller_from, rr, caller_to, rr]`. A `railroad` token is");
L.push("/// `[refid, from, to, id, n, tiles…]`; stations cross by numeric refid.");
L.push("pub struct RrScenario {");
L.push("    pub name: &'static str,");
L.push("    pub kind: u8,");
L.push("    pub args: &'static [f64],");
L.push("    pub res: &'static [f64],");
L.push("}");
L.push("");
for (const s of structures.railroad) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: RrScenario = RrScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    kind: ${s.kind}u8,`);
  L.push(`    args: &[${s.args.map(utilResLit).join(", ")}],`);
  L.push(`    res: &[${s.res.map(utilResLit).join(", ")}],`);
  L.push("};");
  L.push("");
}
L.push("pub const RR_SCENARIOS: &[RrScenario] = &[");
for (const s of structures.railroad) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// One `RailroadSpatialGrid.ts` op: `kind` + flat `args` / `res` token");
L.push("/// streams (see the Rust `RigHarness::run_op` docs).");
L.push("pub struct RsgOp {");
L.push("    pub kind: u8,");
L.push("    pub args: &'static [f64],");
L.push("    pub res: &'static [f64],");
L.push("}");
L.push("/// One grid scenario: an all-land `width x height` GameMap and the op");
L.push("/// stream replayed against it (kind 0 constructs the grid).");
L.push("pub struct RsgScenario {");
L.push("    pub name: &'static str,");
L.push("    pub width: f64,");
L.push("    pub height: f64,");
L.push("    pub ops: &'static [RsgOp],");
L.push("}");
L.push("");
for (const s of structures.railgrid) {
  const id = s.name.toUpperCase();
  L.push(`const ${id}_OPS: &[RsgOp] = &[`);
  for (const o of s.ops)
    L.push(
      `    RsgOp { kind: ${o.kind}, args: &[${o.args.map(utilResLit).join(", ")}], res: &[${o.res.map(utilResLit).join(", ")}] },`,
    );
  L.push("];");
  L.push(`pub const ${id}: RsgScenario = RsgScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    width: ${f64(s.width)},`);
  L.push(`    height: ${f64(s.height)},`);
  L.push(`    ops: ${id}_OPS,`);
  L.push("};");
  L.push("");
}
L.push("pub const RSG_SCENARIOS: &[RsgScenario] = &[");
for (const s of structures.railgrid) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// One `TileTraversalScratch.ts` op: `kind` + flat `args` / `res` token");
L.push("/// streams (see the Rust `RigHarness::run_op` docs).");
L.push("pub struct TtsOp {");
L.push("    pub kind: u8,");
L.push("    pub args: &'static [f64],");
L.push("    pub res: &'static [f64],");
L.push("}");
L.push("/// One scratch scenario: the op stream replayed against a fresh harness");
L.push("/// (kind 0 allocates / reuses a per-game scratch).");
L.push("pub struct TtsScenario {");
L.push("    pub name: &'static str,");
L.push("    pub ops: &'static [TtsOp],");
L.push("}");
L.push("");
for (const s of structures.tiletravscratch) {
  const id = s.name.toUpperCase();
  L.push(`const ${id}_OPS: &[TtsOp] = &[`);
  for (const o of s.ops)
    L.push(
      `    TtsOp { kind: ${o.kind}, args: &[${o.args.map(utilResLit).join(", ")}], res: &[${o.res.map(utilResLit).join(", ")}] },`,
    );
  L.push("];");
  L.push(`pub const ${id}: TtsScenario = TtsScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    ops: ${id}_OPS,`);
  L.push("};");
  L.push("");
}
L.push("pub const TTS_SCENARIOS: &[TtsScenario] = &[");
for (const s of structures.tiletravscratch) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// One `EventBus.ts` op: `kind` + flat `args` / `res` token streams");
L.push("/// (see the Rust `RigHarness::run_op` docs).");
L.push("pub struct EbOp {");
L.push("    pub kind: u8,");
L.push("    pub args: &'static [f64],");
L.push("    pub res: &'static [f64],");
L.push("}");
L.push("/// One bus scenario: the op stream replayed against a fresh harness.");
L.push("pub struct EbScenario {");
L.push("    pub name: &'static str,");
L.push("    pub ops: &'static [EbOp],");
L.push("}");
L.push("");
for (const s of structures.eventbus) {
  const id = s.name.toUpperCase();
  L.push(`const ${id}_OPS: &[EbOp] = &[`);
  for (const o of s.ops)
    L.push(
      `    EbOp { kind: ${o.kind}, args: &[${o.args.map(utilResLit).join(", ")}], res: &[${o.res.map(utilResLit).join(", ")}] },`,
    );
  L.push("];");
  L.push(`pub const ${id}: EbScenario = EbScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    ops: ${id}_OPS,`);
  L.push("};");
  L.push("");
}
L.push("pub const EB_SCENARIOS: &[EbScenario] = &[");
for (const s of structures.eventbus) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// AssetUrls.ts scenario: one `asset_urls::run_op(kind, args)` call.");
L.push("/// `args` is `[len, u0, ..]` (kind 0/1) or `[path, n, (key, value)*n,");
L.push("/// baseUrl]` (kind 2); `res` is `[0, len, u0, ..]` on success or `[1]`");
L.push("/// when the TS function throws.");
L.push("pub struct AuScenario {");
L.push("    pub name: &'static str,");
L.push("    pub kind: u8,");
L.push("    pub args: &'static [f64],");
L.push("    pub res: &'static [f64],");
L.push("}");
L.push("");
for (const s of structures.asseturls) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: AuScenario = AuScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    kind: ${s.kind}u8,`);
  L.push(`    args: &[${s.args.map(utilResLit).join(", ")}],`);
  L.push(`    res: &[${s.res.map(utilResLit).join(", ")}],`);
  L.push("};");
  L.push("");
}
L.push("pub const AU_SCENARIOS: &[AuScenario] = &[");
for (const s of structures.asseturls) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// Maps.gen.ts scenario: one `maps_gen::run_op(kind, args)` call.");
L.push("/// kind 0 dumps `maps`, 1 dumps `GameMapType`, 2 dumps");
L.push("/// `mapCategoryOrder`, 3 finds a map by id. `args` is `[]` for the");
L.push("/// dumps or `[id]` (kind 3); `res` is the flat token stream the Rust");
L.push("/// serialiser must reproduce byte-for-byte.");
L.push("pub struct MgScenario {");
L.push("    pub name: &'static str,");
L.push("    pub kind: u8,");
L.push("    pub args: &'static [f64],");
L.push("    pub res: &'static [f64],");
L.push("}");
L.push("");
for (const s of structures.maps) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: MgScenario = MgScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    kind: ${s.kind}u8,`);
  L.push(`    args: &[${s.args.map(utilResLit).join(", ")}],`);
  L.push(`    res: &[${s.res.map(utilResLit).join(", ")}],`);
  L.push("};");
  L.push("");
}
L.push("pub const MG_SCENARIOS: &[MgScenario] = &[");
for (const s of structures.maps) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// TribeNames.ts scenario: one `tribe_names::run_op(0, args)` call.");
L.push("/// `args` is `[mapPresent, mapType?, removedN, (removed)*, blankedN,");
L.push("/// (blanked)*]`; `res` is `[0, warnN, (warn)*, prefixN, (prefix)*,");
L.push("/// suffixN, (suffix)*, tribesPresent, tribeN, (name, coordPresent,");
L.push("/// x, y)*]` on success or `[1]` when the TS function throws.");
L.push("pub struct TnScenario {");
L.push("    pub name: &'static str,");
L.push("    pub kind: u8,");
L.push("    pub args: &'static [f64],");
L.push("    pub res: &'static [f64],");
L.push("}");
L.push("");
for (const s of structures.tribenames) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: TnScenario = TnScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    kind: ${s.kind}u8,`);
  L.push(`    args: &[${s.args.map(utilResLit).join(", ")}],`);
  L.push(`    res: &[${s.res.map(utilResLit).join(", ")}],`);
  L.push("};");
  L.push("");
}
L.push("pub const TN_SCENARIOS: &[TnScenario] = &[");
for (const s of structures.tribenames) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// game/Game.ts scenario: one `game_ts::run_op(kind, args)` call.");
L.push("/// kind 0/1 dump a string/numeric enum, 2 a unitTypeGroup, 3 an");
L.push("/// isEnumValue guard, 4 the message-category table / a lookup, 5");
L.push("/// ColoredTeams, 6 a Cell, 7 a PlayerInfo, 8 the bulk-cost math, 9");
L.push("/// the module consts. Strings cross as `[len, u0, ..]` UTF-16 units.");
L.push("pub struct GameScenario {");
L.push("    pub name: &'static str,");
L.push("    pub kind: u8,");
L.push("    pub args: &'static [f64],");
L.push("    pub res: &'static [f64],");
L.push("}");
L.push("");
for (const s of structures.game) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: GameScenario = GameScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    kind: ${s.kind}u8,`);
  L.push(`    args: &[${s.args.map(utilResLit).join(", ")}],`);
  L.push(`    res: &[${s.res.map(utilResLit).join(", ")}],`);
  L.push("};");
  L.push("");
}
L.push("pub const GAME_SCENARIOS: &[GameScenario] = &[");
for (const s of structures.game) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// game/NationCreation.ts scenario: one `nation_creation::run_op(kind,");
L.push("/// args)` call. kind 0-3 dump the template / noun / O_TO_OES /");
L.push("/// SPECIAL_PLURALS tables, 4 pluralize, 5 generateNationName, 6");
L.push("/// generateUniqueNationName, 7 getCompactMapNationCount, 8");
L.push("/// createRandomNations. Strings cross as `[len, u0, ..]` UTF-16 units.");
L.push("pub struct NcScenario {");
L.push("    pub name: &'static str,");
L.push("    pub kind: u8,");
L.push("    pub args: &'static [f64],");
L.push("    pub res: &'static [f64],");
L.push("}");
L.push("");
for (const s of structures.nationcreation) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: NcScenario = NcScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    kind: ${s.kind}u8,`);
  L.push(`    args: &[${s.args.map(utilResLit).join(", ")}],`);
  L.push(`    res: &[${s.res.map(utilResLit).join(", ")}],`);
  L.push("};");
  L.push("");
}
L.push("pub const NC_SCENARIOS: &[NcScenario] = &[");
for (const s of structures.nationcreation) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// game/GameUpdates.ts scenario: one `game_updates::run_op(kind, args)`");
L.push("/// call. kind 0 dumps the 24 GameUpdateType (name, value) pairs, 1");
L.push("/// looks a name up (-1 when absent). Strings cross as `[len, u0, ..]`");
L.push("/// UTF-16 units.");
L.push("pub struct GupdScenario {");
L.push("    pub name: &'static str,");
L.push("    pub kind: u8,");
L.push("    pub args: &'static [f64],");
L.push("    pub res: &'static [f64],");
L.push("}");
L.push("");
for (const s of structures.gameupdates) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: GupdScenario = GupdScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    kind: ${s.kind}u8,`);
  L.push(`    args: &[${s.args.map(utilResLit).join(", ")}],`);
  L.push(`    res: &[${s.res.map(utilResLit).join(", ")}],`);
  L.push("};");
  L.push("");
}
L.push("pub const GUPD_SCENARIOS: &[GupdScenario] = &[");
for (const s of structures.gameupdates) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// Util.ts emojiTable + NationEmojiBehavior.ts EMOJI_* scenario: one");
L.push("/// `nation_emoji::run_op(kind, args)` call. kind 0 dumps the 12x5");
L.push("/// emojiTable, 1 the flattened 60-entry table, 2 the 23 EMOJI_* id");
L.push("/// arrays (names included), 3 maps an input string batch through");
L.push("/// emoji_id (-1 for absent). Strings cross as `[len, u0, ..]` UTF-16");
L.push("/// units.");
L.push("pub struct NeScenario {");
L.push("    pub name: &'static str,");
L.push("    pub kind: u8,");
L.push("    pub args: &'static [f64],");
L.push("    pub res: &'static [f64],");
L.push("}");
L.push("");
for (const s of structures.nationemoji) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: NeScenario = NeScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    kind: ${s.kind}u8,`);
  L.push(`    args: &[${s.args.map(utilResLit).join(", ")}],`);
  L.push(`    res: &[${s.res.map(utilResLit).join(", ")}],`);
  L.push("};");
  L.push("");
}
L.push("pub const NE_SCENARIOS: &[NeScenario] = &[");
for (const s of structures.nationemoji) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// CosmeticSchemas.ts scenario: one `cosmetic_schemas::run_op(kind,");
L.push("/// args)` call. kind 0 dumps EFFECT_TYPES, 1 TRAIL_EFFECT_TYPES, 2");
L.push("/// NUKE_EXPLOSION_TYPES, 3 the DefaultPattern (name, patternData), 4");
L.push("/// maps a string batch through isTrailEffect, 5 through");
L.push("/// isNukeExplosionEffect, 6 through effectTypeForSlot ([1,str] when");
L.push("/// resolved, [0] for undefined), 7 through effectMatchesSlot ((et,");
L.push("/// np, ns?, slot) per case). Strings cross as `[len, u0, ..]` UTF-16");
L.push("/// units.");
L.push("pub struct CsScenario {");
L.push("    pub name: &'static str,");
L.push("    pub kind: u8,");
L.push("    pub args: &'static [f64],");
L.push("    pub res: &'static [f64],");
L.push("}");
L.push("");
for (const s of structures.cosmeticschemas) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: CsScenario = CsScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    kind: ${s.kind}u8,`);
  L.push(`    args: &[${s.args.map(utilResLit).join(", ")}],`);
  L.push(`    res: &[${s.res.map(utilResLit).join(", ")}],`);
  L.push("};");
  L.push("");
}
L.push("pub const CS_SCENARIOS: &[CsScenario] = &[");
for (const s of structures.cosmeticschemas) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// StatsSchemas.ts scenario: one `stats_schemas::run_op(kind, args)`");
L.push("/// call. kind 0 dumps bombUnits, 1 boatUnits, 2 otherUnits, 3 the");
L.push("/// unitTypeToBombUnit (key,val) pairs, 4 the unitTypeToOtherUnit");
L.push("/// pairs (both in TS declaration order), 5 dumps the 40 numeric");
L.push("/// index constants (name,value) in TS order, 6 maps a toBigInt input");
L.push("/// batch ([0]=null, [1]=undefined, [2,str]=string, [3,str]=bigint");
L.push("/// decimal) to per-item results ([0,value] ok, [1] threw). Strings");
L.push("/// cross as `[len, u0, ..]` UTF-16 units.");
L.push("pub struct SsScenario {");
L.push("    pub name: &'static str,");
L.push("    pub kind: u8,");
L.push("    pub args: &'static [f64],");
L.push("    pub res: &'static [f64],");
L.push("}");
L.push("");
for (const s of structures.statschemas) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: SsScenario = SsScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    kind: ${s.kind}u8,`);
  L.push(`    args: &[${s.args.map(utilResLit).join(", ")}],`);
  L.push(`    res: &[${s.res.map(utilResLit).join(", ")}],`);
  L.push("};");
  L.push("");
}
L.push("pub const SS_SCENARIOS: &[SsScenario] = &[");
for (const s of structures.statschemas) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// Schemas.ts scenario: one `schemas::run_op(kind, args)` call. kind 0");
L.push("/// dumps PublicGameTypeSchema.options, 1 SCHEDULED_PUBLIC_GAME_TYPES,");
L.push("/// 2 LobbyAccentSchema.options, 3 ClientPlatformSchema.options, 4");
L.push("/// ReportReasonSchema.options, 5 the three numeric lobby constants, 6");
L.push("/// the five string constants (CLIENT_ID_MAPPING, ADMIN_BOT_CLIENT_ID,");
L.push("/// GAME_ID_REGEX.source, RENDERABLE_NAME_ALNUM, RENDERABLE_NAME_CHARS),");
L.push("/// 7 the LogSeverity (name,value) pairs, 8 the full JSON-derived");
L.push("/// QuickChat key list, 9 maps a string batch through isValidGameID, 10");
L.push("/// through RENDERABLE_NAME_CHAR_RE.test, 11 through");
L.push("/// RENDERABLE_NAME_HAS_ALNUM_RE.test. Strings cross as `[len, u0, ..]`");
L.push("/// UTF-16 units; the boolean batches emit [n,(0/1)*n].");
L.push("pub struct ScScenario {");
L.push("    pub name: &'static str,");
L.push("    pub kind: u8,");
L.push("    pub args: &'static [f64],");
L.push("    pub res: &'static [f64],");
L.push("}");
L.push("");
for (const s of structures.schemas) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: ScScenario = ScScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    kind: ${s.kind}u8,`);
  L.push(`    args: &[${s.args.map(utilResLit).join(", ")}],`);
  L.push(`    res: &[${s.res.map(utilResLit).join(", ")}],`);
  L.push("};");
  L.push("");
}
L.push("pub const SC_SCENARIOS: &[ScScenario] = &[");
for (const s of structures.schemas) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// ApiSchemas.ts scenario: one `api_schemas::run_op(kind, args)` call.");
L.push("/// kind 0 dumps ADMIN_ROLES, 1 PlayerStatsGameModes (string values), 2");
L.push("/// PlayerGameModeFilters, 3 PlayerGameTypeFilters, 4-11 the z.enum");
L.push("/// option arrays (UsernameStatus, BareClaim, TribeNameStatus,");
L.push("/// PlayerGameResult, PaymentsProvider, PaymentsKind, PaymentsHandoff,");
L.push("/// SteamOrderResolution), 12 maps a string batch through isAdminRole,");
L.push("/// 13 through isTemporaryUsername, 14 through isVerifiedUsername, 15");
L.push("/// through isGrantedSubscription (sub encoding [0]=undefined,");
L.push("/// [1,(str)provider]=string, [2]=provider null). Strings cross as");
L.push("/// `[len, u0, ..]` UTF-16 units; the boolean batches emit [n,(0/1)*n].");
L.push("pub struct AsScenario {");
L.push("    pub name: &'static str,");
L.push("    pub kind: u8,");
L.push("    pub args: &'static [f64],");
L.push("    pub res: &'static [f64],");
L.push("}");
L.push("");
for (const s of structures.apischemas) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: AsScenario = AsScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    kind: ${s.kind}u8,`);
  L.push(`    args: &[${s.args.map(utilResLit).join(", ")}],`);
  L.push(`    res: &[${s.res.map(utilResLit).join(", ")}],`);
  L.push("};");
  L.push("");
}
L.push("pub const AS_SCENARIOS: &[AsScenario] = &[");
for (const s of structures.apischemas) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// pathfinding/PathFinder.ts WaterPathMemo scenario: one");
L.push("/// `water_path_memo::run_op(kind, args)` call. kind 0 replays the");
L.push("/// whole scenario (scripted inner mock + op sequence) and emits the");
L.push("/// per-op token stream; args/res layouts are documented in");
L.push("/// water_path_memo.rs. All tokens are plain numbers.");
L.push("pub struct WpmScenario {");
L.push("    pub name: &'static str,");
L.push("    pub kind: u8,");
L.push("    pub args: &'static [f64],");
L.push("    pub res: &'static [f64],");
L.push("}");
L.push("");
for (const s of structures.waterpathmemo) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: WpmScenario = WpmScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    kind: ${s.kind}u8,`);
  L.push(`    args: &[${s.args.map(utilResLit).join(", ")}],`);
  L.push(`    res: &[${s.res.map(utilResLit).join(", ")}],`);
  L.push("};");
  L.push("");
}
L.push("pub const WPM_SCENARIOS: &[WpmScenario] = &[");
for (const s of structures.waterpathmemo) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// game/TerrainMapLoader.ts scenario: one `terrain_map_loader::run_op(0,");
L.push("/// args)` call replaying a whole scripted loadTerrainMap sequence");
L.push("/// (loadImages=false) against the stable-manifest / fresh-bin mock");
L.push("/// loader. args layout: map/map4x/map16x metadata triples, nations");
L.push("/// `[n,(hasCoord,x?,y?,flag,name)*]`, additionalNations");
L.push("/// `[present,m,(...)*]`, teamGameSpawnAreas `[present,k,(key,areasLen,");
L.push("/// (x,y,w,h)*areasLen)*k]`, layers `[present,l,(id,placement,hasAlpha,");
L.push("/// alpha?)*l]`, three bins `[kind,len,(byte)*len]` (kind 1 = throw with a");
L.push("/// zero-length buffer), then ops `[opsLen,(mapName,size)*]` (size 0=");
L.push("/// Normal, 1=Compact). res: `[opsLen, (per call)*, getMapDataCalls]` -");
L.push("/// success `[0, gameMap, miniMap, nations, addNations, tgsa, layers]`");
L.push("/// (maps as `[w,h,nlt,terrainLen,(byte)*]`), throws `[1|2|3,(str)msg]`");
L.push("/// (bin length / placement / alpha). Strings cross as `[len, u0, ..]`");
L.push("/// UTF-16 units.");
L.push("pub struct TmlScenario {");
L.push("    pub name: &'static str,");
L.push("    pub kind: u8,");
L.push("    pub args: &'static [f64],");
L.push("    pub res: &'static [f64],");
L.push("}");
L.push("");
for (const s of structures.terrainmaploader) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: TmlScenario = TmlScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    kind: ${s.kind}u8,`);
  L.push(`    args: &[${s.args.map(utilResLit).join(", ")}],`);
  L.push(`    res: &[${s.res.map(utilResLit).join(", ")}],`);
  L.push("};");
  L.push("");
}
L.push("pub const TML_SCENARIOS: &[TmlScenario] = &[");
for (const s of structures.terrainmaploader) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// execution/nation/NationUtils.ts scenario: one `nation_utils::run_op(kind,");
L.push("/// args)` call replaying a whole scripted randTerritoryTileArray (kind 0)");
L.push("/// or findJuiciestTarget (kind 1) run against the PseudoRandom / Game /");
L.push("/// Player mocks. kind 0 args: `[0,width,height,numTiles,nBorder,(tile)*,");
L.push("/// nNext,(ret)*,nOn,(0|1)*,nRef,(ret)*,nOwn,(playerId)*,randElemRet,");
L.push("/// numTilesOwned,nTiles,(tile)*,playerId]`; kind 1 args: `[1,nCand,(id,");
L.push("/// troops,numTilesOwned,maxTroops,nUnits,(typeStr,level)*nUnits)*]`. res:");
L.push("/// `[traceLen,(trace)*,payload]` - trace events 0 nextInt [0,min,max,ret],");
L.push("/// 1 randElement [1,len,(tile)*,ret], 2 isOnMap [2,x,y,0|1], 3 ref [3,x,y,");
L.push("/// ret], 4 owner [4,tile,id], 5 borderTiles [5,len,(tile)*], 6 numTilesOwned");
L.push("/// [6,(id,)ret], 7 tiles [7,len,(tile)*], 8 config [8], 9 maxTroops [9,id,");
L.push("/// ret], 10 units [10,id,len], 11 troops [11,id,ret], 12 unit [12,id,");
L.push("/// (str)type,level,0|1]; payload kind 0 `[outLen,(tile)*]`, kind 1 `[nCand,");
L.push("/// (juiciness)*,present,winnerId?]`. Strings cross as `[len,u0,..]` UTF-16.");
L.push("pub struct NuScenario {");
L.push("    pub name: &'static str,");
L.push("    pub kind: u8,");
L.push("    pub args: &'static [f64],");
L.push("    pub res: &'static [f64],");
L.push("}");
L.push("");
for (const s of structures.nationutils) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: NuScenario = NuScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    kind: ${s.kind}u8,`);
  L.push(`    args: &[${s.args.map(utilResLit).join(", ")}],`);
  L.push(`    res: &[${s.res.map(utilResLit).join(", ")}],`);
  L.push("};");
  L.push("");
}
L.push("pub const NU_SCENARIOS: &[NuScenario] = &[");
for (const s of structures.nationutils) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// game/GameImpl.ts createGameUpdatesMap scenario: one `game_updates::");
L.push("/// run_op(kind, args)` call. kind 2 dumps the map() result object");
L.push("/// `[n,(key,isArray,len)*n]`, kind 3 pins the `Object.values` +");
L.push("/// `!isNaN(Number(key))` filter `[n,(0,value|1,str)*n,m,(kept)*m]`.");
L.push("/// Strings cross as `[len,u0,..]` UTF-16.");
L.push("pub struct GiScenario {");
L.push("    pub name: &'static str,");
L.push("    pub kind: u8,");
L.push("    pub args: &'static [f64],");
L.push("    pub res: &'static [f64],");
L.push("}");
L.push("");
for (const s of structures.gameimpl) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: GiScenario = GiScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    kind: ${s.kind}u8,`);
  L.push(`    args: &[${s.args.map(utilResLit).join(", ")}],`);
  L.push(`    res: &[${s.res.map(utilResLit).join(", ")}],`);
  L.push("};");
  L.push("");
}
L.push("pub const GI_SCENARIOS: &[GiScenario] = &[");
for (const s of structures.gameimpl) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

L.push("/// game/TerraNulliusImpl.ts scenario: one `terra_nullius::run_op(kind,");
L.push("/// args)` call over the four constant-return methods. kind 0 smallID");
L.push("/// `[0]`, 1 clientID `[len,u0,..]`, 2 id `[-1]` (JS null sentinel), 3");
L.push("/// isPlayer `[0|1]`. Strings cross as `[len,u0,..]` UTF-16.");
L.push("pub struct TniScenario {");
L.push("    pub name: &'static str,");
L.push("    pub kind: u8,");
L.push("    pub args: &'static [f64],");
L.push("    pub res: &'static [f64],");
L.push("}");
L.push("");
for (const s of structures.terranulliusimpl) {
  const id = s.name.toUpperCase();
  L.push(`pub const ${id}: TniScenario = TniScenario {`);
  L.push(`    name: "${s.name}",`);
  L.push(`    kind: ${s.kind}u8,`);
  L.push(`    args: &[${s.args.map(utilResLit).join(", ")}],`);
  L.push(`    res: &[${s.res.map(utilResLit).join(", ")}],`);
  L.push("};");
  L.push("");
}
L.push("pub const TNI_SCENARIOS: &[TniScenario] = &[");
for (const s of structures.terranulliusimpl) L.push(`    ${s.name.toUpperCase()},`);
L.push("];");
L.push("");

const dataDir = join(root, "crates", "core", "tests", "data");
mkdirSync(dataDir, { recursive: true });
writeFileSync(join(dataDir, "vectors.rs"), L.join("\n") + "\n", "utf8");
writeFileSync(join(dataDir, "vectors.json"), JSON.stringify(json) + "\n", "utf8");
console.log("wrote", dataDir);
