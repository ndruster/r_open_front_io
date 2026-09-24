// Executes the Rust determinism layer compiled to wasm32 and diffs every
// golden vector from data/vectors.json against it.
//
//   cargo build -p openfront-core --target wasm32-unknown-unknown \
//       --release --features wasm-probe
//   node rust/tools/run_wasm_parity.mjs
//
// Exists because this machine has no Windows SDK import libs, so native test
// binaries cannot link; wasm runs fine under Node and exercises the exact
// same Rust code paths as tests/parity_golden.rs.
import { readFileSync } from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..", "..");
const wasmPath = join(
  root, "rust", "target", "wasm32-unknown-unknown", "release", "openfront_core.wasm",
);
const vectors = JSON.parse(
  readFileSync(join(root, "rust", "crates", "core", "tests", "data", "vectors.json"), "utf8"),
);

const { instance } = await WebAssembly.instantiate(
  await readFile(wasmPath),
  { env: { abort: () => { throw new Error("rust panic (wasm abi mismatch)"); } } },
);
const ex = instance.exports;
// Sanity: the probe must export every symbol we are about to call.
for (const name of [
  "probe_prng_seed", "probe_prng_next_u32", "probe_prng_next_int",
  "probe_prng_next_id_value", "probe_prng_chance", "probe_prng_shuffle_reset",
  "probe_prng_shuffle_at_last", "probe_exp", "probe_log", "probe_pow",
  "probe_atan2", "probe_pow2",
]) {
  if (typeof ex[name] !== "function") {
    console.error(`missing wasm export ${name} - rebuild with --features wasm-probe`);
    process.exit(1);
  }
}

async function readFile(p) {
  const { readFile } = await import("node:fs/promises");
  return readFile(p);
}

const dv = new DataView(new ArrayBuffer(8));
const toBits = (x) => (dv.setFloat64(0, x), dv.getBigUint64(0));
const fmt = (x) =>
  Number.isNaN(x) ? "NaN" : Object.is(x, -0) ? "-0" : String(x);

let checks = 0;
const failures = [];
function fail(what, idx, got, want) {
  if (failures.length < 20) failures.push(`${what}[${idx}]: got ${got}, want ${want}`);
}
function cmpU32(what, got, want, idx) {
  checks++;
  if (got !== want) fail(what, idx, got, want);
}
function cmpBits(what, got, want, idx) {
  checks++;
  if (toBits(got) !== BigInt(want)) {
    fail(what, idx, `${fmt(got)} (${toBits(got)})`, `${want} bits`);
  }
}

// wasm i32 returns are signed in JS; u32 exports need the >>> 0 coercion.
const u32 = (fn) => () => fn() >>> 0;
const nextU32 = u32(ex.probe_prng_next_u32);

const P = vectors.prng;

// --- next() stream, compared as exact u32 numerators ---
for (let si = 0; si < P.seeds.length; si++) {
  ex.probe_prng_seed(P.seeds[si]);
  for (let i = 0; i < P.streamLen; i++) {
    cmpU32(
      `streams seed=${P.seeds[si]}`,
      nextU32(),
      P.stream[si * P.streamLen + i],
      i,
    );
  }
}

// --- nextInt across ranges ---
const IV = P.intCount;
const NR = P.intRanges.length;
for (let si = 0; si < P.seeds.length; si++) {
  for (let ri = 0; ri < NR; ri++) {
    ex.probe_prng_seed(P.seeds[si]);
    const [lo, hi] = P.intRanges[ri];
    for (let i = 0; i < IV; i++) {
      checks++;
      const got = Number(ex.probe_prng_next_int(lo, hi));
      const want = P.intValues[(si * NR + ri) * IV + i];
      if (got !== want) fail(`nextInt(${lo},${hi}) seed=${P.seeds[si]}`, i, got, want);
    }
  }
}

// --- nextID strings ---
{
  ex.probe_prng_seed(P.idSeed);
  for (let i = 0; i < P.ids.length; i++) {
    checks++;
    const got = Math.floor(Number(ex.probe_prng_next_id_value()))
      .toString(36).padStart(8, "0");
    if (got !== P.ids[i]) fail("nextID", i, got, P.ids[i]);
  }
}

// --- shuffleArray permutations ---
for (let si = 0; si < P.seeds.length; si++) {
  ex.probe_prng_seed(P.seeds[si]);
  ex.probe_prng_shuffle_reset(P.shuffleLen);
  for (let i = 0; i < P.shuffleLen; i++) {
    cmpU32(
      `shuffle seed=${P.seeds[si]}`,
      ex.probe_prng_shuffle_at_last(i),
      P.shuffles[si * P.shuffleLen + i],
      i,
    );
  }
}

// --- chance() bit-exact 256-draw streak ---
{
  ex.probe_prng_seed(P.chanceSeed);
  for (let byte = 0; byte < P.chanceBytes.length; byte++) {
    let acc = 0;
    for (let bit = 0; bit < 8; bit++)
      acc = (acc << 1) | (ex.probe_prng_chance(P.chanceOdds) & 1);
    cmpU32("chance", acc >>> 0, P.chanceBytes[byte], byte);
  }
}

// --- DetMath, compared as raw IEEE-754 bits ---
const D = vectors.detmath;
for (let i = 0; i < D.expX.length; i++) cmpBits("exp", ex.probe_exp(D.expX[i]), D.expBits[i], i);
for (let i = 0; i < D.logX.length; i++) cmpBits("log", ex.probe_log(D.logX[i]), D.logBits[i], i);
for (let i = 0; i < D.powPairs.length; i++)
  cmpBits("pow", ex.probe_pow(...D.powPairs[i]), D.powBits[i], i);
for (let i = 0; i < D.atanPairs.length; i++)
  cmpBits("atan2", ex.probe_atan2(...D.atanPairs[i]), D.atanBits[i], i);
for (let i = 0; i < D.pow2N.length; i++) cmpBits("pow2", ex.probe_pow2(D.pow2N[i]), D.pow2Bits[i], i);

// --- published snapshot: bit-exact AND decimal-exact (rounds through String) ---
{
  const rustSnapshot = [
    () => ex.probe_exp(1),
    () => ex.probe_exp(-12.5),
    () => ex.probe_log(3),
    () => ex.probe_log(150000),
    () => ex.probe_pow(50000, 0.6),
    () => ex.probe_pow(1234567, 0.73),
    () => ex.probe_atan2(3, 4),
    () => ex.probe_atan2(-7, -2),
  ];
  for (let i = 0; i < D.snapshotBits.length; i++) {
    const got = rustSnapshot[i]();
    cmpBits("snapshot", got, D.snapshotBits[i], i);
    checks++;
    const dec = String(got);
    if (dec !== D.snapshotDecimals[i])
      fail("snapshot-decimal", i, dec, D.snapshotDecimals[i]);
  }
}

// ---------------------------------------------------------------- structures
// Replays the queue/heap/grid operation traces the TS classes recorded.

const S = vectors.structures;
const resNum = (r) =>
  r === "n" ? NaN : r === "-0" ? -0 : typeof r === "number" ? r : null;
const argNum = (v) => (v === "n" ? NaN : v === "-0" ? -0 : v); // string forms JSON can't carry
const eqRes = (got, want) =>
  want === null ? Number.isNaN(got) : got === want;

// --- MinHeap ---
for (const s of S.minheap) {
  ex.probe_st_new(0, s.cap);
  s.ops.forEach(([k, a, b, res], i) => {
    if (k === 0) ex.probe_st_push(argNum(a), argNum(b));
    else if (k === 1) {
      const got = ex.probe_st_pop();
      checks++;
      if (!eqRes(got, resNum(res))) fail(`${s.name} pop`, i, got, resNum(res));
    } else if (k === 2) ex.probe_st_clear();
    else if (k === 3) {
      checks++;
      if ((ex.probe_st_is_empty() === 1) !== (res === 1))
        fail(`${s.name} isEmpty`, i, ex.probe_st_is_empty(), res);
    }
  });
  for (let i = 0; i < s.heap.length; i++) cmpU32(`${s.name} heap`, ex.probe_st_a_get(i), s.heap[i], i);
  for (let i = 0; i < s.priBits.length; i++)
    cmpU32(`${s.name} priBits`, ex.probe_st_b_get(i), s.priBits[i], i);
  cmpU32(`${s.name} size`, ex.probe_st_field(0), s.size, 0);
  cmpU32(`${s.name} capacity`, ex.probe_st_field(1), s.capacity, 1);
}

// --- BucketQueue ---
for (const s of S.bucket) {
  ex.probe_st_new(1, s.maxP);
  s.ops.forEach(([k, a, b, res], i) => {
    if (k === 0) ex.probe_st_push(argNum(a), argNum(b));
    else if (k === 1) {
      const got = ex.probe_st_pop();
      checks++;
      if (!eqRes(got, resNum(res))) fail(`${s.name} pop`, i, got, resNum(res));
    } else if (k === 2) ex.probe_st_clear();
    else if (k === 3) {
      checks++;
      if ((ex.probe_st_is_empty() === 1) !== (res === 1))
        fail(`${s.name} isEmpty`, i, ex.probe_st_is_empty(), res);
    }
  });
  for (let i = 0; i < s.sizes.length; i++) cmpU32(`${s.name} sizes`, ex.probe_st_a_get(i), s.sizes[i], i);
  for (let i = 0; i < s.stamps.length; i++)
    cmpU32(`${s.name} stamps`, ex.probe_st_b_get(i), s.stamps[i], i);
  for (let i = 0; i < s.keys.length; i++)
    cmpU32(`${s.name} keys`, ex.probe_st_c_get(i), s.keys[i], i);
  cmpU32(`${s.name} minBucket`, Number(ex.probe_st_field(0)), s.minBucket, 0);
  cmpU32(`${s.name} size`, Number(ex.probe_st_field(1)), s.size, 1);
  cmpU32(`${s.name} stamp`, Number(ex.probe_st_field(2)), s.stamp, 2);
}

// --- FlatBinaryHeap ---
for (const s of S.flatheap) {
  ex.probe_st_new(2, s.cap);
  s.ops.forEach(([k, a, b, res], i) => {
    if (k === 0) ex.probe_st_push(argNum(a), argNum(b));
    else if (k === 1) {
      const got = ex.probe_st_pop();
      checks++;
      if (!eqRes(got, resNum(res))) fail(`${s.name} dequeue`, i, got, resNum(res));
    } else if (k === 2) ex.probe_st_clear();
    else if (k === 4) {
      checks++;
      if (ex.probe_st_can_throw() !== 1)
        fail(`${s.name} expected empty throw`, i, "no throw", "throw");
    } else if (k === 5) cmpU32(`${s.name} size`, Number(ex.probe_st_field(0)), res, 0);
  });
  for (let i = 0; i < s.priBits.length; i++)
    cmpU32(`${s.name} priBits`, ex.probe_st_a_get(i), s.priBits[i], i);
  for (let i = 0; i < s.tiles.length; i++) {
    const got = ex.probe_st_b_get(i);
    const want = s.tiles[i];
    checks++;
    // "u" is a JS array hole: the probe encodes it as NaN, and a real tile
    // (including 0) must never be accepted in its place.
    const ok = want === "u" ? Number.isNaN(got) : got === want;
    if (!ok) fail(`${s.name} tile`, i, got, want);
  }
  cmpU32(`${s.name} len`, Number(ex.probe_st_field(0)), s.len, 0);
}

// --- BFSGrid --- (i64 params must cross as BigInt; mode is u8/i32)
for (const s of S.bfsgrid) {
  ex.probe_grid_new(s.w * s.h);
  const found = ex.probe_grid_search(
    BigInt(s.w), BigInt(s.h), BigInt(s.s0), BigInt(s.s1),
    s.maxd === "Infinity" ? Infinity : Number(s.maxd),
    s.mode, BigInt(s.blocker),
  );
  cmpU32(`${s.name} found`, found, s.found, 0);
  cmpU32(`${s.name} nvisits`, ex.probe_grid_visit_count(), s.nvisits, 0);
  cmpU32(`${s.name} stamp`, Number(ex.probe_grid_stamp()), s.stampAfter, 0);
  for (const [idx, node, dist] of s.samples) {
    cmpU32(`${s.name} node@${idx}`, ex.probe_grid_visit_node(idx), node, idx);
    cmpU32(`${s.name} dist@${idx}`, ex.probe_grid_visit_dist(idx), dist, idx);
  }
}

// --- AStar --- (maxIter null crosses the boundary as NaN = "use default")
for (const s of S.astar) {
  for (const b of s.blocked) ex.probe_astar_block(b);
  ex.probe_astar_new(s.w, s.h, s.cc, s.tp, s.hk, s.hs, s.maxIter ?? NaN);
  for (const st of s.starts) ex.probe_astar_start(st);
  const gotPath = ex.probe_astar_run(s.goal, s.runs) === 1;
  checks++;
  if (gotPath !== (s.path !== "u")) {
    fail(`${s.name} path presence`, 0, gotPath, s.path !== "u");
  }
  if (s.path !== "u") {
    checks++;
    if (ex.probe_astar_path_len() !== s.path.length) {
      fail(`${s.name} path length`, ex.probe_astar_path_len(), s.path.length, 0);
    }
    for (let i = 0; i < s.path.length; i++)
      cmpBits(`${s.name} path`, ex.probe_astar_path_at(i), toBits(s.path[i]), i);
  }
  cmpU32(`${s.name} stamp`, Number(ex.probe_astar_stamp()), s.stampAfter, 0);
  const fields = [
    [0, s.closed, "closedStamp"],
    [1, s.gsStamp, "gScoreStamp"],
    [2, s.gScore, "gScore"],
    [3, s.cameFrom, "cameFrom"],
  ];
  for (const [f, want, label] of fields) {
    checks++;
    if (ex.probe_astar_arr_len(f) !== want.length)
      fail(`${s.name} ${label} length`, ex.probe_astar_arr_len(f), want.length, 0);
    for (let i = 0; i < want.length; i++)
      cmpU32(`${s.name} ${label}`, Number(ex.probe_astar_arr_get(f, i)), want[i], i);
  }
}

// --- AStarRail ---
for (const s of S.rail) {
  for (const b of s.terrain) ex.probe_rail_terrain_byte(b);
  ex.probe_rail_new(s.w, s.h);
  for (const st of s.starts) ex.probe_rail_start(st);
  const gotPath = ex.probe_rail_run(s.goal) === 1;
  checks++;
  if (gotPath !== (s.path !== "u")) {
    fail(`${s.name} path presence`, 0, gotPath, s.path !== "u");
  }
  if (s.path !== "u") {
    checks++;
    if (ex.probe_rail_path_len() !== s.path.length) {
      fail(`${s.name} path length`, ex.probe_rail_path_len(), s.path.length, 0);
    }
    for (let i = 0; i < s.path.length; i++)
      cmpBits(`${s.name} path`, ex.probe_rail_path_at(i), toBits(s.path[i]), i);
  }
  cmpU32(`${s.name} stamp`, Number(ex.probe_rail_stamp()), s.stampAfter, 0);
  const rfields = [
    [0, s.closed, "closedStamp"],
    [1, s.gsStamp, "gScoreStamp"],
    [2, s.gScore, "gScore"],
    [3, s.cameFrom, "cameFrom"],
  ];
  for (const [f, want, label] of rfields) {
    checks++;
    if (ex.probe_rail_arr_len(f) !== want.length)
      fail(`${s.name} ${label} length`, ex.probe_rail_arr_len(f), want.length, 0);
    for (let i = 0; i < want.length; i++)
      cmpU32(`${s.name} ${label}`, Number(ex.probe_rail_arr_get(f, i)), want[i], i);
  }
}

console.log(`${checks} vector comparisons executed against wasm build`);
if (failures.length) {
  console.error(`FAIL (${failures.length}+ mismatches, first 20):`);
  for (const f of failures) console.error("  " + f);
  process.exit(1);
}
console.log("OK: Rust wasm layer is bit-identical to the TypeScript implementation");
