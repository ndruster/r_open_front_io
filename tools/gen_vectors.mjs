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
L.push("/// list-taking kinds), `strs` the strings for simpleHash, `status`");
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

const dataDir = join(root, "crates", "core", "tests", "data");
mkdirSync(dataDir, { recursive: true });
writeFileSync(join(dataDir, "vectors.rs"), L.join("\n") + "\n", "utf8");
writeFileSync(join(dataDir, "vectors.json"), JSON.stringify(json) + "\n", "utf8");
console.log("wrote", dataDir);
