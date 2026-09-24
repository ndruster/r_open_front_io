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

import { loadTs } from "./ts_load.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..", "..");

const { PseudoRandom } = await import(
  pathToFileURL(join(root, "src/core/PseudoRandom.ts")).href
);
const DetMath = await import(pathToFileURL(join(root, "src/core/DetMath.ts")).href);

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
const { AStar } = await loadTs("src/core/pathfinding/algorithms/AStar.ts");
const { AStarRail } = await loadTs("src/core/pathfinding/algorithms/AStar.Rail.ts");
const { GameMapImpl } = await loadTs("src/core/game/GameMap.ts");

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

const structures = {
  minheap: mhScenarios,
  bucket: bqScenarios,
  flatheap: fbhScenarios,
  bfsgrid: bgScenarios,
  astar: asScenarios,
  rail: railScenarios,
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

const dataDir = join(root, "rust", "crates", "core", "tests", "data");
mkdirSync(dataDir, { recursive: true });
writeFileSync(join(dataDir, "vectors.rs"), L.join("\n") + "\n", "utf8");
writeFileSync(join(dataDir, "vectors.json"), JSON.stringify(json) + "\n", "utf8");
console.log("wrote", dataDir);
