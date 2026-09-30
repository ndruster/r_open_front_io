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
const root = join(here, "..");
const wasmPath = join(
  root, "target", "wasm32-unknown-unknown", "release", "openfront_core.wasm",
);
const vectors = JSON.parse(
  readFileSync(join(root, "crates", "core", "tests", "data", "vectors.json"), "utf8"),
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
  "probe_veterancy_arg", "probe_veterancy_op",
  "probe_mp_arg", "probe_mp_op", "probe_mp_out_len", "probe_mp_out_at",
  "probe_cc_terrain_byte", "probe_cc_new", "probe_cc_op", "probe_cc_field",
  "probe_cc_arr_len", "probe_cc_arr_get",
  "probe_tsm_buffer_byte", "probe_tsm_new", "probe_tsm_op",
  "probe_tsm_out_len", "probe_tsm_out_at",
  "probe_wb_terrain_byte", "probe_wb_new", "probe_wb_start", "probe_wb_run",
  "probe_wb_path_len", "probe_wb_path_at", "probe_wb_stamp",
  "probe_wb_arr_len", "probe_wb_arr_get",
  "probe_ag_terrain_byte", "probe_ag_dirty_byte", "probe_ag_new", "probe_ag_op",
  "probe_ag_out_len", "probe_ag_out_at", "probe_ag_field",
  "probe_ag_arr_len", "probe_ag_arr_get",
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

// --- AStarWater ---
for (const s of S.water) {
  for (const b of s.terrain) ex.probe_water_terrain_byte(b);
  ex.probe_water_new(s.w, s.h, s.weight, s.maxIter);
  for (const st of s.starts) ex.probe_water_start(st);
  const gotPath = ex.probe_water_run(s.goal) === 1;
  checks++;
  if (gotPath !== (s.path !== "u")) {
    fail(`${s.name} path presence`, 0, gotPath, s.path !== "u");
  }
  if (s.path !== "u") {
    checks++;
    if (ex.probe_water_path_len() !== s.path.length) {
      fail(`${s.name} path length`, ex.probe_water_path_len(), s.path.length, 0);
    }
    for (let i = 0; i < s.path.length; i++)
      cmpBits(`${s.name} path`, ex.probe_water_path_at(i), toBits(s.path[i]), i);
  }
  cmpU32(`${s.name} stamp`, Number(ex.probe_water_stamp()), s.stampAfter, 0);
  const wfields = [
    [0, s.closed, "closedStamp"],
    [1, s.gsStamp, "gScoreStamp"],
    [2, s.gScore, "gScore"],
    [3, s.cameFrom, "cameFrom"],
  ];
  for (const [f, want, label] of wfields) {
    checks++;
    if (ex.probe_water_arr_len(f) !== want.length)
      fail(`${s.name} ${label} length`, ex.probe_water_arr_len(f), want.length, 0);
    for (let i = 0; i < want.length; i++)
      cmpU32(`${s.name} ${label}`, Number(ex.probe_water_arr_get(f, i)), want[i], i);
  }
}

// --- AStarWaterBounded --- (same shape as the water probe; mode 1 runs
// searchBounded with the recorded explicit bounds)
for (const s of S.waterbounded) {
  for (const b of s.terrain) ex.probe_wb_terrain_byte(b);
  ex.probe_wb_new(s.w, s.maxArea, s.weight, s.maxIter);
  for (const st of s.starts) ex.probe_wb_start(st);
  const bd = s.bounds ?? [0, 0, 0, 0];
  const gotPath = ex.probe_wb_run(s.goal, s.mode, bd[0], bd[1], bd[2], bd[3]) === 1;
  checks++;
  if (gotPath !== (s.path !== "u")) {
    fail(`${s.name} path presence`, 0, gotPath, s.path !== "u");
  }
  if (s.path !== "u") {
    checks++;
    if (ex.probe_wb_path_len() !== s.path.length) {
      fail(`${s.name} path length`, ex.probe_wb_path_len(), s.path.length, 0);
    }
    for (let i = 0; i < s.path.length; i++)
      cmpBits(`${s.name} path`, ex.probe_wb_path_at(i), toBits(s.path[i]), i);
  }
  cmpU32(`${s.name} stamp`, Number(ex.probe_wb_stamp()), s.stampAfter, 0);
  const bfields = [
    [0, s.closed, "closedStamp"],
    [1, s.gsStamp, "gScoreStamp"],
    [2, s.gScore, "gScore"],
    [3, s.cameFrom, "cameFrom"],
  ];
  for (const [f, want, label] of bfields) {
    checks++;
    if (ex.probe_wb_arr_len(f) !== want.length)
      fail(`${s.name} ${label} length`, ex.probe_wb_arr_len(f), want.length, 0);
    for (let i = 0; i < want.length; i++)
      cmpU32(`${s.name} ${label}`, Number(ex.probe_wb_arr_get(f, i)), want[i], i);
  }
}

// --- GameMap --- (op-stream replay; scalar-only boundary, arrays via out buf)
const numTok = (v) => (v === "n" ? NaN : v === "-0" ? -0 : v === "i" ? Infinity : v === "-i" ? -Infinity : v);
for (const s of S.gamemap) {
  for (const b of s.terrain) ex.probe_gm_terrain_byte(b);
  ex.probe_gm_new(s.w, s.h, s.numLand);
  s.ops.forEach(([k, a, b, res], i) => {
    const A = numTok(a);
    const B = numTok(b);
    if (k === 5) {
      // setOwnerID: only the throw path is observable without trapping.
      const throws = ex.probe_gm_set_owner_throws(B) === 1;
      checks++;
      if (res === "t") {
        if (!throws) fail(`${s.name} setOwnerID throw`, i, "no throw", "throw");
      } else {
        if (throws) fail(`${s.name} setOwnerID unexpected throw`, i, "throw", "value");
        else ex.probe_gm_op(k, A, B);
      }
      return;
    }
    if (k === 31) {
      const throws = ex.probe_gm_ref_throws(A, B) === 1;
      checks++;
      if (res === "t") {
        if (!throws) fail(`${s.name} ref throw`, i, "no throw", "throw");
      } else {
        if (throws) fail(`${s.name} ref unexpected throw`, i, "throw", "value");
        else cmpScalar(`${s.name} ref`, ex.probe_gm_op(k, A, B), res, i);
      }
      return;
    }
    if (k === 9 || k === 10 || k === 11 || k === 12 || k === 34 || k === 35) {
      ex.probe_gm_op(k, A, B);
      const want = res;
      checks++;
      if (ex.probe_gm_out_len() !== want.length) {
        fail(`${s.name} arr len`, i, ex.probe_gm_out_len(), want.length);
        return;
      }
      for (let j = 0; j < want.length; j++) {
        checks++;
        const got = ex.probe_gm_out_at(j);
        if (!Object.is(got, numTok(want[j])))
          fail(`${s.name} arr[${j}]`, i, fmt(got), String(want[j]));
      }
      return;
    }
    const got = ex.probe_gm_op(k, A, B);
    cmpScalar(`${s.name} op${k}`, got, res, i);
  });
  // Final buffers + counters.
  for (let i = 0; i < s.terrainAfter.length; i++)
    cmpU32(`${s.name} terrainAfter`, ex.probe_gm_arr_get(0, i), s.terrainAfter[i], i);
  for (let i = 0; i < s.stateAfter.length; i++)
    cmpU32(`${s.name} stateAfter`, ex.probe_gm_arr_get(1, i), s.stateAfter[i], i);
  cmpBits(`${s.name} numLandAfter`, ex.probe_gm_field(0), toBits(s.numLandAfter), 0);
  cmpBits(`${s.name} waterVersionAfter`, ex.probe_gm_field(1), toBits(s.waterVersionAfter), 1);
  cmpBits(`${s.name} falloutAfter`, ex.probe_gm_field(2), toBits(s.falloutAfter), 2);
}

function cmpScalar(what, got, res, idx) {
  checks++;
  if (res === "v") return; // void mutation: probe returns NaN, nothing to compare
  const want = res === "u" ? NaN : numTok(res);
  if (!Object.is(got, want)) fail(what, idx, fmt(got), String(res));
}

// --- TileSet --- (op-stream replay; kinds match runTs in gen_vectors.mjs)
for (const s of S.tileset) {
  for (const v of s.initial) ex.probe_ts_initial_value(v);
  ex.probe_ts_new();
  s.ops.forEach(([k, a, , res], i) => {
    if (k === 4 || k === 6 || k === 7 || k === 8) {
      ex.probe_ts_op(k, numTok(a));
      checks++;
      if (ex.probe_ts_out_len() !== res.length) {
        fail(`${s.name} arr len`, i, ex.probe_ts_out_len(), res.length);
        return;
      }
      for (let j = 0; j < res.length; j++) {
        checks++;
        const got = ex.probe_ts_out_at(j);
        if (!Object.is(got, numTok(res[j])))
          fail(`${s.name} arr[${j}]`, i, fmt(got), String(res[j]));
      }
      return;
    }
    const got = ex.probe_ts_op(k, numTok(a));
    cmpScalar(`${s.name} op${k}`, got, res, i);
  });
  // Final internal state.
  checks++;
  if (ex.probe_ts_arr_len(0) !== s.dense.length)
    fail(`${s.name} dense length`, ex.probe_ts_arr_len(0), s.dense.length, 0);
  for (let i = 0; i < s.dense.length; i++)
    cmpU32(`${s.name} dense`, Number(ex.probe_ts_arr_get(0, i)), s.dense[i], i);
  checks++;
  if (ex.probe_ts_arr_len(1) !== s.table.length)
    fail(`${s.name} table length`, ex.probe_ts_arr_len(1), s.table.length, 0);
  for (let i = 0; i < s.table.length; i++)
    cmpU32(`${s.name} table`, Number(ex.probe_ts_arr_get(1, i)), s.table[i], i);
  cmpBits(`${s.name} denseLen`, ex.probe_ts_field(0), toBits(s.denseLen), 0);
  cmpBits(`${s.name} tableUsed`, ex.probe_ts_field(1), toBits(s.tableUsed), 1);
  cmpBits(`${s.name} iterDepth`, ex.probe_ts_field(2), toBits(s.iterDepth), 2);
}

// --- Util --- (single-call replay; kinds match the util runner in
// gen_vectors.mjs). Kinds 12/13/14 need a GameMap built first; the map is
// keyed by (w,h) so it is only rebuilt when the dimensions change.
let utilMapKey = "";
for (const s of S.util) {
  const args = s.args.map(numTok);
  if (s.kind === 12 || s.kind === 13 || s.kind === 14) {
    const key = `${args[0]}x${args[1]}`;
    if (key !== utilMapKey) {
      const w = args[0];
      const h = args[1];
      for (let i = 0; i < w * h; i++) ex.probe_gm_terrain_byte(0x85);
      ex.probe_gm_new(w, h, w * h);
      utilMapKey = key;
    }
  }
  if (s.kind === 2) {
    // charCodeAt per *code unit* (not for..of, which walks code points and
    // would collapse a surrogate pair into its high surrogate only).
    for (let i = 0; i < s.strs[0].length; i++) ex.probe_util_str_unit(s.strs[0].charCodeAt(i));
  }
  for (const v of args) ex.probe_util_arg(v);
  ex.probe_util_op(s.kind);
  checks++;
  if (ex.probe_util_out_len() !== s.res.length) {
    fail(`${s.name} res len`, 0, ex.probe_util_out_len(), s.res.length);
    continue;
  }
  for (let i = 0; i < s.res.length; i++) {
    checks++;
    const got = ex.probe_util_out_at(i);
    if (!Object.is(got, numTok(s.res[i])))
      fail(`${s.name} res[${i}]`, i, fmt(got), s.res[i]);
  }
}

// --- TeamAssignment --- (flat f64 token stream; the Rust `Cur` in
// wasm_probe.rs decodes exactly this layout: a string is [len, ...units],
// an optional string [present, string], teamIndex [flag] with 0=null /
// 1=number follows / 2=NaN, config [kind] (+num if 0, +string if 5)).
const tStr = (push, s) => {
  push(s.length);
  for (let i = 0; i < s.length; i++) push(s.charCodeAt(i));
};
const tOptStr = (push, s) => {
  if (s === null || s === undefined) push(0);
  else {
    push(1);
    tStr(push, s);
  }
};
const tTeamIndex = (push, tok) => {
  if (tok === null || tok === undefined) push(0);
  else if (tok === "n") push(2);
  else {
    push(1);
    push(numTok(tok));
  }
};
const tConfig = (push, c) => {
  push(c.kind);
  if (c.kind === 0) push(numTok(c.num));
  else if (c.kind === 5) tStr(push, c.str);
};
for (const s of S.team) {
  const push = (v) => ex.probe_team_arg(v);
  if (s.kind === 0 || s.kind === 1) {
    push(s.players.length);
    for (const p of s.players) {
      push(p.playerType === "BOT" ? 0 : p.playerType === "HUMAN" ? 1 : 2);
      tTeamIndex(push, p.teamIndex);
      tStr(push, p.id);
      tOptStr(push, p.clientID);
      tOptStr(push, p.clanTag);
      push(p.friends.length);
      for (const f of p.friends) tStr(push, f);
    }
    push(s.teams.length);
    for (const t of s.teams) tStr(push, t);
    push(s.isDuo);
    if (s.kind === 0) {
      push(s.hasMax);
      push(numTok(s.maxTeamSize));
    } else {
      push(s.nationCount);
      tConfig(push, s.config);
    }
  } else if (s.kind === 2) {
    push(numTok(s.config.num));
    push(numTok(s.totalPlayers));
  } else {
    tConfig(push, s.config);
    push(numTok(s.totalPlayers));
  }
  ex.probe_team_op(s.kind);
  checks++;
  const n = ex.probe_team_out_len();
  if (s.kind === 0 || s.kind === 1) {
    if (n !== s.res.length * 2) {
      fail(`${s.name} res len`, 0, n, s.res.length * 2);
      continue;
    }
    for (let i = 0; i < s.res.length; i++) {
      for (let j = 0; j < 2; j++) {
        checks++;
        const got = ex.probe_team_out_at(i * 2 + j);
        if (!Object.is(got, s.res[i][j]))
          fail(`${s.name} res[${i}][${j}]`, i, fmt(got), s.res[i][j]);
      }
    }
  } else if (s.kind === 2) {
    if (n !== 1) fail(`${s.name} res len`, 0, n, 1);
    else {
      checks++;
      const got = ex.probe_team_out_at(0);
      if (!Object.is(got, numTok(s.resNums[0])))
        fail(`${s.name} max`, 0, fmt(got), s.resNums[0]);
    }
  } else {
    checks++;
    const status = n > 0 ? ex.probe_team_out_at(0) : -1;
    if (!Object.is(status, s.status)) {
      fail(`${s.name} status`, 0, fmt(status), s.status);
      continue;
    }
    if (s.status === 0) {
      checks++;
      const len = ex.probe_team_out_at(1);
      if (!Object.is(len, s.resTeams.length)) {
        fail(`${s.name} teams len`, 0, len, s.resTeams.length);
        continue;
      }
      let at = 2;
      for (const want of s.resTeams) {
        const slen = ex.probe_team_out_at(at++);
        checks++;
        let got = "";
        for (let i = 0; i < slen; i++) got += String.fromCharCode(ex.probe_team_out_at(at++));
        checks++;
        if (got !== want) fail(`${s.name} team`, 0, got, want);
      }
    }
  }
}

// --- Bezier --- (out layout mirrors the Rust probe: kind 0 -> [len];
// kind 1 -> [np, points x/y, nw, walk index/x/y triples, final_index]).
for (const s of S.bezier) {
  const push = (v) => ex.probe_bezier_arg(v);
  for (const v of s.cp) push(numTok(v));
  if (s.kind === 1) {
    push(numTok(s.spacing));
    for (const v of s.incs) push(numTok(v));
  }
  ex.probe_bezier_op(s.kind);
  checks++;
  const n = ex.probe_bezier_out_len();
  if (s.kind === 0) {
    if (n !== 1) fail(`${s.name} len shape`, 0, n, 1);
    else {
      checks++;
      const got = ex.probe_bezier_out_at(0);
      if (!Object.is(got, numTok(s.len))) fail(`${s.name} len`, 0, fmt(got), s.len);
    }
    continue;
  }
  // out = [np, ...points, nw, ...walk, final_index]
  const np = n > 0 ? ex.probe_bezier_out_at(0) : -1;
  checks++;
  if (!Object.is(np, s.points.length / 2)) {
    fail(`${s.name} np`, 0, np, s.points.length / 2);
    continue;
  }
  for (let i = 0; i < s.points.length; i++) {
    checks++;
    const got = ex.probe_bezier_out_at(1 + i);
    if (!Object.is(got, numTok(s.points[i])))
      fail(`${s.name} point[${i}]`, i, fmt(got), s.points[i]);
  }
  const nwAt = 1 + s.points.length;
  const nw = ex.probe_bezier_out_at(nwAt);
  checks++;
  const walkFlat = s.walk.flat();
  if (!Object.is(nw, s.walk.length)) {
    fail(`${s.name} nw`, 0, nw, s.walk.length);
    continue;
  }
  for (let i = 0; i < walkFlat.length; i++) {
    checks++;
    const got = ex.probe_bezier_out_at(nwAt + 1 + i);
    if (!Object.is(got, numTok(walkFlat[i])))
      fail(`${s.name} walk[${i}]`, i, fmt(got), walkFlat[i]);
  }
  checks++;
  const fi = ex.probe_bezier_out_at(nwAt + 1 + walkFlat.length);
  if (!Object.is(fi, s.finalIndex)) fail(`${s.name} finalIndex`, 0, fi, s.finalIndex);
}

// --- Veterancy --- (pure scalar: three f64 args in, one f64 result out).
for (const s of S.veterancy) {
  ex.probe_veterancy_arg(numTok(s.base));
  ex.probe_veterancy_arg(numTok(s.vet));
  ex.probe_veterancy_arg(numTok(s.pct));
  checks++;
  const got = ex.probe_veterancy_op();
  if (!Object.is(got, numTok(s.res))) fail(`${s.name}`, 0, fmt(got), s.res);
}

// --- MotionPlans --- (records cross as the flat token stream; op 0 packs the
// input stream to [wlen, ...words], op 1 unpacks [wlen, ...words] back to a
// record stream. A roundtrip scenario checks both legs; unpack-only checks 1).
for (const s of S.motionplans) {
  if (s.kind === 0) {
    for (const v of s.input) ex.probe_mp_arg(numTok(v));
    ex.probe_mp_op(0);
    const n = ex.probe_mp_out_len();
    checks++;
    if (n !== s.words.length) {
      fail(`${s.name} pack len`, 0, n, s.words.length);
    } else {
      for (let i = 0; i < s.words.length; i++) {
        checks++;
        const got = ex.probe_mp_out_at(i);
        if (!Object.is(got, numTok(s.words[i])))
          fail(`${s.name} pack word[${i}]`, i, fmt(got), s.words[i]);
      }
    }
  }
  for (const v of s.words) ex.probe_mp_arg(numTok(v));
  ex.probe_mp_op(1);
  const n = ex.probe_mp_out_len();
  checks++;
  if (n !== s.out.length) {
    fail(`${s.name} unpack len`, 0, n, s.out.length);
    continue;
  }
  for (let i = 0; i < s.out.length; i++) {
    checks++;
    const got = ex.probe_mp_out_at(i);
    if (!Object.is(got, numTok(s.out[i])))
      fail(`${s.name} unpack tok[${i}]`, i, fmt(got), s.out[i]);
  }
}

// --- ConnectedComponents --- (op-stream replay; void ops 0/1 return NaN,
// queries 2/3 compare scalars; final buffers read back element-wise, JS
// holes in sizes cross as NaN and compare via Object.is).
for (const s of S.connectedcomponents) {
  for (const b of s.terrain) ex.probe_cc_terrain_byte(b);
  ex.probe_cc_new(s.w, s.h, s.direct);
  s.ops.forEach(([k, a, , res], i) => {
    const got = ex.probe_cc_op(k, numTok(a));
    cmpScalar(`${s.name} op${k}`, got, res, i);
  });
  cmpBits(`${s.name} bits`, ex.probe_cc_field(0), toBits(s.bits), 0);
  cmpBits(`${s.name} landMarker`, ex.probe_cc_field(1), toBits(s.landMarker), 1);
  cmpBits(`${s.name} maxId`, ex.probe_cc_field(2), toBits(s.maxId), 2);
  for (const [field, arr] of [[0, s.ids], [1, s.sizes], [2, s.parents]]) {
    checks++;
    if (ex.probe_cc_arr_len(field) !== arr.length) {
      fail(`${s.name} field${field} len`, 0, ex.probe_cc_arr_len(field), arr.length);
      continue;
    }
    for (let j = 0; j < arr.length; j++) {
      checks++;
      const got = ex.probe_cc_arr_get(field, j);
      const tok = arr[j];
      const want = tok === "u" ? NaN : numTok(tok);
      if (!Object.is(got, want))
        fail(`${s.name} field${field}[${j}]`, j, fmt(got), String(tok));
    }
  }
}

// --- TerrainSearchMap --- (buffer replay; kinds 0/1/2 compare scalars,
// kind 3 (neighbors) reads the flattened [x0,y0,...] from the out buffer;
// NaN / infinite / fractional coordinates cross the boundary as f64).
for (const s of S.terrainsearchmap) {
  for (const b of s.buffer) ex.probe_tsm_buffer_byte(b);
  ex.probe_tsm_new();
  s.ops.forEach(([k, a, b, res], i) => {
    if (k === 3) {
      ex.probe_tsm_op(k, numTok(a), numTok(b));
      const want = res;
      checks++;
      if (ex.probe_tsm_out_len() !== want.length) {
        fail(`${s.name} neighbors len`, i, ex.probe_tsm_out_len(), want.length);
        return;
      }
      for (let j = 0; j < want.length; j++) {
        checks++;
        const got = ex.probe_tsm_out_at(j);
        if (!Object.is(got, numTok(want[j])))
          fail(`${s.name} neighbors[${j}]`, i, fmt(got), String(want[j]));
      }
      return;
    }
    const got = ex.probe_tsm_op(k, numTok(a), numTok(b));
    cmpScalar(`${s.name} op${k}`, got, res, i);
  });
}

// --- AbstractGraph --- (terrain bytes + optional dirty tiles queued before
// probe_ag_new; scalar kinds 0/1/6/7/11/12 compare directly — kind 6 returns
// NaN for a missing edge, matching the TS throw guard captured as "t"; array
// kinds 2/3/4/5/8/9/10/13 land in the out buffer, out_len == -1 encodes
// undefined/null; final internal arrays read back element-wise).
const AG_ARR_KINDS = new Set([2, 3, 4, 5, 8, 9, 10, 13]);
for (const s of S.abstractgraph) {
  for (const b of s.terrain) ex.probe_ag_terrain_byte(b);
  for (const d of s.dirty) ex.probe_ag_dirty_byte(numTok(d));
  ex.probe_ag_new(s.w, s.h, s.clusterSize, s.oldIdx);
  s.ops.forEach(([k, a, b, res], i) => {
    const A = numTok(a);
    const B = numTok(b);
    if (k === 14) {
      ex.probe_ag_op(k, A, B); // void mutation, nothing observable
      return;
    }
    if (k === 6) {
      const got = ex.probe_ag_op(k, A, B);
      if (res === "t") {
        checks++;
        if (!Number.isNaN(got)) fail(`${s.name} getOtherNode throw`, i, got, "NaN");
      } else {
        cmpScalar(`${s.name} op6`, got, res, i);
      }
      return;
    }
    if (AG_ARR_KINDS.has(k)) {
      ex.probe_ag_op(k, A, B);
      const gotLen = ex.probe_ag_out_len();
      if (res === "u") {
        checks++;
        if (gotLen !== -1) fail(`${s.name} op${k} undef`, i, gotLen, -1);
        return;
      }
      checks++;
      if (gotLen !== res.length) {
        fail(`${s.name} op${k} len`, i, gotLen, res.length);
        return;
      }
      for (let j = 0; j < res.length; j++) {
        checks++;
        const got = ex.probe_ag_out_at(j);
        const tok = res[j];
        const want = tok === "u" ? NaN : numTok(tok);
        if (!Object.is(got, want))
          fail(`${s.name} op${k}[${j}]`, j, fmt(got), String(tok));
      }
      return;
    }
    const got = ex.probe_ag_op(k, A, B);
    cmpScalar(`${s.name} op${k}`, got, res, i);
  });
  cmpBits(`${s.name} nodeCount`, ex.probe_ag_field(0), toBits(s.nodeCount), 0);
  cmpBits(`${s.name} edgeCount`, ex.probe_ag_field(1), toBits(s.edgeCount), 1);
  cmpBits(`${s.name} pathCacheLen`, ex.probe_ag_field(2), toBits(s.pathCacheLen), 2);
  for (const [field, arr] of [
    [0, s.nodes],
    [1, s.edges],
    [2, s.clusters],
    [3, s.nodeEdgeIds],
  ]) {
    checks++;
    if (ex.probe_ag_arr_len(field) !== arr.length) {
      fail(`${s.name} field${field} len`, 0, ex.probe_ag_arr_len(field), arr.length);
      continue;
    }
    for (let j = 0; j < arr.length; j++) {
      checks++;
      const got = ex.probe_ag_arr_get(field, j);
      const tok = arr[j];
      const want = tok === "u" ? NaN : numTok(tok);
      if (!Object.is(got, want))
        fail(`${s.name} field${field}[${j}]`, j, fmt(got), String(tok));
    }
  }
}

console.log(`${checks} vector comparisons executed against wasm build`);
if (failures.length) {
  console.error(`FAIL (${failures.length}+ mismatches, first 20):`);
  for (const f of failures) console.error("  " + f);
  process.exit(1);
}
console.log("OK: Rust wasm layer is bit-identical to the TypeScript implementation");
