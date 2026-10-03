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
  "probe_aga_node", "probe_aga_edge", "probe_aga_new", "probe_aga_start",
  "probe_aga_run", "probe_aga_path_len", "probe_aga_path_at", "probe_aga_stamp",
  "probe_aga_qfield", "probe_aga_arr_len", "probe_aga_arr_get",
  "probe_wh_terrain_byte", "probe_wh_new", "probe_wh_start", "probe_wh_run",
  "probe_wh_path_len", "probe_wh_path_at", "probe_wh_stamp",
  "probe_wh_cache_len", "probe_wh_cache_at",
  "probe_pb_new", "probe_pb_cp", "probe_pb_cp_at", "probe_pb_find",
  "probe_pb_path_len", "probe_pb_path_at", "probe_pb_next", "probe_pb_node",
  "probe_pb_invalidate", "probe_pb_index",
  "probe_mmt_new", "probe_mmt_reset", "probe_mmt_from", "probe_mmt_inner",
  "probe_mmt_run", "probe_mmt_out_len", "probe_mmt_out_at",
  "probe_mmt_seen_flag", "probe_mmt_seen_multi", "probe_mmt_seen_len",
  "probe_mmt_seen_at", "probe_mmt_seen_goal",
  "probe_sp_new", "probe_sp_reset", "probe_sp_inv", "probe_sp_push_null",
  "probe_sp_push_start", "probe_sp_push_tile", "probe_sp_push_end",
  "probe_sp_next", "probe_sp_node", "probe_sp_pan_null", "probe_sp_pan_len",
  "probe_sp_pan_at", "probe_sp_idx", "probe_sp_has_path", "probe_sp_calls",
  "probe_sp_from_reset", "probe_sp_from_push", "probe_sp_find_path",
  "probe_sp_out_len", "probe_sp_out_at", "probe_sp_seen_flag",
  "probe_sp_seen_multi", "probe_sp_seen_len", "probe_sp_seen_at",
  "probe_sp_seen_goal",
  "probe_cct_new", "probe_cct_pair", "probe_cct_reset", "probe_cct_from",
  "probe_cct_inner", "probe_cct_run", "probe_cct_out_len", "probe_cct_out_at",
  "probe_cct_seen_flag", "probe_cct_seen_multi", "probe_cct_seen_len",
  "probe_cct_seen_at", "probe_cct_seen_goal",
  "probe_sct_water_reset", "probe_sct_water", "probe_sct_new",
  "probe_sct_reset", "probe_sct_from", "probe_sct_inner", "probe_sct_run",
  "probe_sct_out_len", "probe_sct_out_at", "probe_sct_seen_flag",
  "probe_sct_seen_multi", "probe_sct_seen_len", "probe_sct_seen_at",
  "probe_sct_seen_goal",
  "probe_swt_cell_reset", "probe_swt_cell", "probe_swt_new",
  "probe_swt_reset", "probe_swt_from", "probe_swt_inner", "probe_swt_run",
  "probe_swt_out_len", "probe_swt_out_at", "probe_swt_seen_flag",
  "probe_swt_seen_multi", "probe_swt_seen_len", "probe_swt_seen_at",
  "probe_swt_seen_goal",
  "probe_bfs_edge_reset", "probe_bfs_edge_key", "probe_bfs_edge_nb",
  "probe_bfs_edge_end", "probe_bfs_reset", "probe_bfs_start", "probe_bfs_run",
  "probe_bfs_visits_len", "probe_bfs_visit_node", "probe_bfs_visit_dist",
  "probe_bfs_result_flag", "probe_bfs_result",
  "probe_air_new", "probe_air_run", "probe_air_path_len", "probe_air_path_at",
  "probe_anon_run", "probe_anon_out_at",
  "probe_close_arg", "probe_close_op",
  "probe_sl_arg", "probe_sl_op", "probe_sl_out_at",
  "probe_au_arg", "probe_au_op", "probe_au_out_at",
  "probe_mg_arg", "probe_mg_op", "probe_mg_out_at",
  "probe_tn_arg", "probe_tn_op", "probe_tn_out_at",
  "probe_pd_arg", "probe_pd_op", "probe_pd_out_at",
  "probe_dc_arg", "probe_dc_op", "probe_dc_out_at",
  "probe_eu_terrain_byte", "probe_eu_owner", "probe_eu_new",
  "probe_eu_arg", "probe_eu_op", "probe_eu_out_at",
  "probe_wm_map_terrain_byte", "probe_wm_mini_terrain_byte", "probe_wm_new",
  "probe_wm_arg", "probe_wm_op", "probe_wm_out_at", "probe_wm_version",
  "probe_wm_map_terrain_at", "probe_wm_map_state_at", "probe_wm_mini_terrain_at",
  "probe_gu_arg", "probe_gu_op", "probe_gu_out_at",
  "probe_gupd_arg", "probe_gupd_op", "probe_gupd_out_at",
  "probe_ne_arg", "probe_ne_op", "probe_ne_out_at",
  "probe_cs_arg", "probe_cs_op", "probe_cs_out_at",
  "probe_ss_arg", "probe_ss_op", "probe_ss_out_at",
  "probe_sc_arg", "probe_sc_op", "probe_sc_out_at",
  "probe_as_arg", "probe_as_op", "probe_as_out_at",
  "probe_tml_arg", "probe_tml_op", "probe_tml_out_at",
  "probe_nu_arg", "probe_nu_op", "probe_nu_out_at",
  "probe_em_arg", "probe_em_op", "probe_em_out_at",
  "probe_tni_arg", "probe_tni_op", "probe_tni_out_at",
  "probe_si_arg", "probe_si_op", "probe_si_out_at",
  "probe_swc_arg", "probe_swc_op", "probe_swc_out_at",
  "probe_wpm_arg", "probe_wpm_op", "probe_wpm_out_at",
  "probe_rr_arg", "probe_rr_op", "probe_rr_out_at",
  "probe_rsg_reset", "probe_rsg_arg", "probe_rsg_op", "probe_rsg_out_at",
  "probe_ug_reset", "probe_ug_arg", "probe_ug_op", "probe_ug_out_at",
  "probe_stm_reset", "probe_stm_arg", "probe_stm_op", "probe_stm_out_at",
  "probe_tsn_reset", "probe_tsn_arg", "probe_tsn_op", "probe_tsn_out_at",
  "probe_rn_reset", "probe_rn_arg", "probe_rn_op", "probe_rn_out_at",
  "probe_tts_reset", "probe_tts_arg", "probe_tts_op", "probe_tts_out_at",
  "probe_eb_reset", "probe_eb_arg", "probe_eb_op", "probe_eb_out_at",
  "probe_vt_reset", "probe_vt_arg", "probe_vt_op", "probe_vt_out_at",
  "probe_cp_reset", "probe_cp_arg", "probe_cp_op", "probe_cp_out_at",
  "probe_ia_reset", "probe_ia_arg", "probe_ia_op", "probe_ia_out_at",
  "probe_cv_reset", "probe_cv_arg", "probe_cv_op", "probe_cv_out_at",
  "probe_ls_reset", "probe_ls_arg", "probe_ls_op", "probe_ls_out_at",
  "probe_nvs_reset", "probe_nvs_arg", "probe_nvs_op", "probe_nvs_out_at",
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
// gen_vectors.mjs). Kinds 12/13/14/17/18 need a GameMap built first; the map
// is keyed by (w,h) so it is only rebuilt when the dimensions change.
let utilMapKey = "";
for (const s of S.util) {
  const args = s.args.map(numTok);
  if (s.kind === 12 || s.kind === 13 || s.kind === 14 || s.kind === 17 || s.kind === 18) {
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

// --- AbstractGraphAStar --- (hand-built graph; per-query full state snapshot,
// including the live MinHeap arrays — gScore/queue priorities as raw f32 bits)
for (const s of S.abstractgraphastar) {
  for (const v of s.nodes) ex.probe_aga_node(v);
  for (const v of s.edges) ex.probe_aga_edge(v);
  ex.probe_aga_new(s.numNodes, s.edgeCount, s.weight, s.maxIter);
  for (const q of s.queries) {
    for (const st of q.starts) ex.probe_aga_start(st);
    const gotPath = ex.probe_aga_run(q.goal, q.isMulti) === 1;
    checks++;
    if (gotPath !== (q.path !== "u")) {
      fail(`${s.name} q path presence`, 0, gotPath, q.path !== "u");
    }
    if (q.path !== "u") {
      checks++;
      if (ex.probe_aga_path_len() !== q.path.length) {
        fail(`${s.name} q path length`, ex.probe_aga_path_len(), q.path.length, 0);
      }
      for (let i = 0; i < q.path.length; i++)
        cmpBits(`${s.name} q path`, ex.probe_aga_path_at(i), toBits(q.path[i]), i);
    }
    cmpU32(`${s.name} q stamp`, Number(ex.probe_aga_stamp()), q.stampAfter, 0);
    cmpU32(`${s.name} q size`, Number(ex.probe_aga_qfield(0)), q.qSize, 0);
    cmpU32(`${s.name} q cap`, Number(ex.probe_aga_qfield(1)), q.qCap, 0);
    const afields = [
      [0, q.closed, "closedStamp"],
      [1, q.gsStamp, "gScoreStamp"],
      [2, q.gScoreBits, "gScoreBits"],
      [3, q.cameFrom, "cameFrom"],
      [4, q.startNode, "startNode"],
      [5, q.qHeap, "queueHeap"],
      [6, q.qPriBits, "queuePriBits"],
    ];
    for (const [f, want, label] of afields) {
      checks++;
      if (ex.probe_aga_arr_len(f) !== want.length)
        fail(`${s.name} ${label} length`, ex.probe_aga_arr_len(f), want.length, 0);
      for (let i = 0; i < want.length; i++)
        cmpU32(`${s.name} ${label}`, Number(ex.probe_aga_arr_get(f, i)), want[i], i);
    }
  }
}

// --- AStarWaterHierarchical ---------------------------------------------------
// Rebuilds each scenario's map + graph + orchestrator from the raw terrain,
// replays the query script and compares the returned path, the five engine
// stamps (the dispatch witness) and the graph path cache after every query.
for (const s of S.waterhierarchical) {
  for (const b of s.terrain) ex.probe_wh_terrain_byte(b);
  ex.probe_wh_new(s.w, s.h, s.clusterSize, s.cachePaths);
  for (const q of s.queries) {
    for (const st of q.starts) ex.probe_wh_start(st);
    const gotPath = ex.probe_wh_run(q.goal, q.isMulti, q.rebuildBefore) === 1;
    checks++;
    if (gotPath !== (q.path !== "u")) {
      fail(`${s.name} q path presence`, 0, gotPath, q.path !== "u");
    }
    if (q.path !== "u") {
      checks++;
      if (ex.probe_wh_path_len() !== q.path.length) {
        fail(`${s.name} q path length`, ex.probe_wh_path_len(), q.path.length, 0);
      }
      for (let i = 0; i < q.path.length; i++)
        cmpBits(`${s.name} q path`, ex.probe_wh_path_at(i), toBits(q.path[i]), i);
    }
    cmpU32(`${s.name} q bfs`, Number(ex.probe_wh_stamp(0)), q.bfs, 0);
    cmpU32(`${s.name} q local`, Number(ex.probe_wh_stamp(1)), q.local, 0);
    cmpU32(`${s.name} q multi`, Number(ex.probe_wh_stamp(2)), q.multi, 0);
    cmpU32(`${s.name} q short`, Number(ex.probe_wh_stamp(3)), q.short, 0);
    cmpU32(`${s.name} q aga`, Number(ex.probe_wh_stamp(4)), q.aga, 0);
    if (s.cachePaths) {
      checks++;
      if (ex.probe_wh_cache_len() !== q.cache.length)
        fail(`${s.name} q cache length`, ex.probe_wh_cache_len(), q.cache.length, 0);
      for (let i = 0; i < q.cache.length; i++)
        cmpBits(`${s.name} q cache`, ex.probe_wh_cache_at(i), toBits(q.cache[i]), i);
    }
  }
}

// --- Parabola (PathFinder.Parabola.ts) ----------------------------------------
// One all-land map + finder per scenario. Replays the control-point reads,
// the findPath script and the single-instance next/invalidate/currentIndex
// walk; the out-of-bounds throw is compared via the would-throw flag.
for (const s of S.parabola) {
  const o = s.opt ?? {};
  const tri = (v) => (v === undefined || v === null ? 0 : v ? 2 : 1);
  ex.probe_pb_new(
    s.w,
    s.h,
    o.increment === undefined ? -1 : o.increment,
    tri(o.distanceBasedHeight),
    tri(o.directionUp),
    tri(o.ignoreMapBounds),
  );
  for (const g of s.cps) {
    ex.probe_pb_cp(g[0], g[1]);
    for (let i = 0; i < 8; i++) cmpBits(`${s.name} cp`, ex.probe_pb_cp_at(i), toBits(g[2 + i]), i);
  }
  for (const g of s.finds) {
    const from = g[0];
    const to = g[1];
    const len = g[2];
    const got = ex.probe_pb_find(from, to) === 1;
    checks++;
    if (got !== (len >= 0)) fail(`${s.name} find presence`, 0, got, len >= 0);
    if (len >= 0) {
      const want = g.slice(3);
      checks++;
      if (ex.probe_pb_path_len() !== want.length)
        fail(`${s.name} find length`, ex.probe_pb_path_len(), want.length, 0);
      for (let i = 0; i < want.length; i++)
        cmpBits(`${s.name} find path`, ex.probe_pb_path_at(i), toBits(want[i]), i);
    }
  }
  for (const g of s.walk) {
    const [kind, from, to, hasSpeed, speed, status, node, index] = g;
    if (kind === 0 || kind === 1) {
      const got = ex.probe_pb_next(from, to, hasSpeed ? speed : -1);
      checks++;
      const want = kind === 1 ? 1 : status;
      if (got !== want) fail(`${s.name} next status`, 0, got, want);
      if (kind === 0) cmpBits(`${s.name} next node`, ex.probe_pb_node(), toBits(node), 0);
      cmpU32(`${s.name} next index`, Number(ex.probe_pb_index()), index, 0);
    } else if (kind === 2) {
      ex.probe_pb_invalidate();
    } else {
      cmpU32(`${s.name} idx read`, Number(ex.probe_pb_index()), index, 0);
    }
  }
}

// --- MiniMapTransformer (transformers/MiniMapTransformer.ts) -----------------
// Replays each query: queue the start refs + inner tiles, run, and compare the
// inner stub's observation (scalar/array + downscale tiles + goal) and the
// upscaled result path bit-exactly. Throw modes (downscale/upscale OOB) are
// compared as the would-throw marker.
for (const s of S.minimaptransformer) {
  ex.probe_mmt_new(s.mw, s.mh, s.miniW, s.miniH);
  for (let qi = 0; qi < s.groups.length; qi++) {
    const g = s.groups[qi];
    let j = 0;
    const fromIsArray = g[j++];
    const fromLen = g[j++];
    const fromTiles = g.slice(j, j + fromLen);
    j += fromLen;
    const to = g[j++];
    const innerMode = g[j++];
    const innerLen = g[j++];
    const innerTiles = g.slice(j, j + innerLen);
    j += innerLen;
    const seenFlag = g[j++];
    let seen = null;
    if (seenFlag === 1) {
      const m = g[j++];
      const l = g[j++];
      seen = { m, tiles: g.slice(j, j + l), goal: g[j + l] };
      j += l + 1;
    }
    const outMode = g[j++];
    const outTiles = outMode === 2 ? g.slice(j + 1, j + 1 + g[j]) : [];

    ex.probe_mmt_reset();
    for (const t of fromTiles) ex.probe_mmt_from(t);
    for (const t of innerTiles) ex.probe_mmt_inner(t);
    const got = ex.probe_mmt_run(to, fromIsArray, innerMode);
    cmpU32(`${s.name} q${qi} out mode`, got, outMode, qi);
    if (outMode === 2) {
      checks++;
      if (Number(ex.probe_mmt_out_len()) !== outTiles.length)
        fail(`${s.name} q${qi} out length`, Number(ex.probe_mmt_out_len()), outTiles.length, qi);
      for (let i = 0; i < outTiles.length; i++)
        cmpBits(`${s.name} q${qi} out`, ex.probe_mmt_out_at(i), toBits(outTiles[i]), i);
    }
    cmpU32(`${s.name} q${qi} seen flag`, ex.probe_mmt_seen_flag(), seen ? 1 : 0, qi);
    if (seen) {
      cmpU32(`${s.name} q${qi} seen multi`, ex.probe_mmt_seen_multi(), seen.m, qi);
      checks++;
      if (Number(ex.probe_mmt_seen_len()) !== seen.tiles.length)
        fail(`${s.name} q${qi} seen length`, Number(ex.probe_mmt_seen_len()), seen.tiles.length, qi);
      for (let i = 0; i < seen.tiles.length; i++)
        cmpBits(`${s.name} q${qi} seen`, ex.probe_mmt_seen_at(i), toBits(seen.tiles[i]), i);
      cmpBits(`${s.name} q${qi} seen goal`, ex.probe_mmt_seen_goal(), toBits(seen.goal), qi);
    }
  }
}

// --- PathFinderStepper (PathFinderStepper.ts) --------------------------------
// Replays the flat op trace against the wasm stepper + shared stub, comparing
// every observable of the native replay: next() status/node/pathAfterNext/
// pathIndex/hasPath/calls and findPath() out/seen/calls.
for (const s of S.stepper) {
  ex.probe_sp_new(s.prod ? 1 : 0);
  const o = s.ops;
  let i = 0;
  let oi = 0;
  while (i < o.length) {
    const kind = o[i];
    if (kind === 2) {
      ex.probe_sp_reset();
      i += 1;
    } else if (kind === 1) {
      ex.probe_sp_inv();
      i += 1;
    } else if (kind === 4) {
      const count = o[i + 1];
      let j = i + 2;
      for (let q = 0; q < count; q++) {
        if (o[j] === 0) {
          ex.probe_sp_push_null();
          j += 1;
        } else {
          const l = o[j + 1];
          ex.probe_sp_push_start();
          for (let t = 0; t < l; t++) ex.probe_sp_push_tile(o[j + 2 + t]);
          ex.probe_sp_push_end();
          j += 2 + l;
        }
      }
      i = j;
    } else if (kind === 0) {
      const from = o[i + 1];
      const to = o[i + 2];
      const dist = o[i + 3];
      const status = o[i + 4];
      const node = o[i + 5];
      let j = i + 6;
      const panLen = o[j];
      let pan = null;
      if (panLen < 0) {
        j += 1;
      } else {
        pan = o.slice(j + 1, j + 1 + panLen);
        j += 1 + panLen;
      }
      const idx = o[j];
      const hasPath = o[j + 1];
      const calls = o[j + 2];
      j += 3;

      const got = Number(ex.probe_sp_next(from, to, dist));
      cmpU32(`${s.name} o${oi} status`, got, status, oi);
      if (status === 0 || status === 2)
        cmpBits(`${s.name} o${oi} node`, ex.probe_sp_node(), toBits(node), oi);
      cmpU32(`${s.name} o${oi} pan null`, ex.probe_sp_pan_null(), pan === null ? 1 : 0, oi);
      if (pan !== null) {
        checks++;
        if (Number(ex.probe_sp_pan_len()) !== pan.length)
          fail(`${s.name} o${oi} pan length`, Number(ex.probe_sp_pan_len()), pan.length, oi);
        for (let t = 0; t < pan.length; t++)
          cmpBits(`${s.name} o${oi} pan`, ex.probe_sp_pan_at(t), toBits(pan[t]), t);
      }
      cmpU32(`${s.name} o${oi} pathIndex`, Number(ex.probe_sp_idx()), idx, oi);
      cmpU32(`${s.name} o${oi} hasPath`, ex.probe_sp_has_path(), hasPath, oi);
      cmpBits(`${s.name} o${oi} calls`, ex.probe_sp_calls(), toBits(calls), oi);
      i = j;
    } else {
      const isMulti = o[i + 1];
      const fl = o[i + 2];
      const fs = o.slice(i + 3, i + 3 + fl);
      let j = i + 3 + fl;
      const to = o[j];
      j += 1;
      const outMode = o[j];
      j += 1;
      const outLen = o[j];
      const out = outMode === 2 ? o.slice(j + 1, j + 1 + outLen) : null;
      j += 1 + outLen;
      const seenFlag = o[j];
      j += 1;
      let seen = null;
      if (seenFlag === 1) {
        const m = o[j];
        const l = o[j + 1];
        seen = { m, tiles: o.slice(j + 2, j + 2 + l), goal: o[j + 2 + l] };
        j += 2 + l + 1;
      }
      const calls = o[j];
      j += 1;

      ex.probe_sp_from_reset();
      for (const t of fs) ex.probe_sp_from_push(t);
      const got = Number(ex.probe_sp_find_path(to, isMulti));
      cmpU32(`${s.name} o${oi} fp out mode`, got, outMode, oi);
      if (outMode === 2) {
        checks++;
        if (Number(ex.probe_sp_out_len()) !== out.length)
          fail(`${s.name} o${oi} fp out length`, Number(ex.probe_sp_out_len()), out.length, oi);
        for (let t = 0; t < out.length; t++)
          cmpBits(`${s.name} o${oi} fp out`, ex.probe_sp_out_at(t), toBits(out[t]), t);
      }
      cmpU32(`${s.name} o${oi} fp seen flag`, ex.probe_sp_seen_flag(), seen ? 1 : 0, oi);
      if (seen) {
        cmpU32(`${s.name} o${oi} fp seen multi`, ex.probe_sp_seen_multi(), seen.m, oi);
        checks++;
        if (Number(ex.probe_sp_seen_len()) !== seen.tiles.length)
          fail(`${s.name} o${oi} fp seen length`, Number(ex.probe_sp_seen_len()), seen.tiles.length, oi);
        for (let t = 0; t < seen.tiles.length; t++)
          cmpBits(`${s.name} o${oi} fp seen`, ex.probe_sp_seen_at(t), toBits(seen.tiles[t]), t);
        cmpBits(`${s.name} o${oi} fp seen goal`, ex.probe_sp_seen_goal(), toBits(seen.goal), oi);
      }
      cmpBits(`${s.name} o${oi} fp calls`, ex.probe_sp_calls(), toBits(calls), oi);
      i = j;
    }
    oi += 1;
  }
}

// --- ComponentCheckTransformer (ComponentCheckTransformer.ts) ----------------
// Replays each query: set the component table, queue the start refs + inner
// tiles, run, and compare the inner stub's observation (the filtered PathStart
// + goal) and the pass-through output bit-exactly.
for (const s of S.componentcheck) {
  ex.probe_cct_new(s.default);
  for (const [k, v] of Object.entries(s.table)) ex.probe_cct_pair(Number(k), v);
  for (let qi = 0; qi < s.groups.length; qi++) {
    const g = s.groups[qi];
    let j = 0;
    const fromIsArray = g[j++];
    const fromLen = g[j++];
    const fromTiles = g.slice(j, j + fromLen);
    j += fromLen;
    const to = g[j++];
    const innerMode = g[j++];
    const innerLen = g[j++];
    const innerTiles = g.slice(j, j + innerLen);
    j += innerLen;
    const seenFlag = g[j++];
    let seen = null;
    if (seenFlag === 1) {
      const m = g[j++];
      const l = g[j++];
      seen = { m, tiles: g.slice(j, j + l), goal: g[j + l] };
      j += l + 1;
    }
    // Output is derived: null when inner never ran or returned null.
    const outMode = seenFlag === 0 || innerMode === 0 ? 0 : 2;

    ex.probe_cct_reset();
    for (const t of fromTiles) ex.probe_cct_from(t);
    for (const t of innerTiles) ex.probe_cct_inner(t);
    const got = ex.probe_cct_run(to, fromIsArray, innerMode);
    cmpU32(`${s.name} q${qi} out mode`, got, outMode, qi);
    if (outMode === 2) {
      checks++;
      if (Number(ex.probe_cct_out_len()) !== innerTiles.length)
        fail(`${s.name} q${qi} out length`, Number(ex.probe_cct_out_len()), innerTiles.length, qi);
      for (let i = 0; i < innerTiles.length; i++)
        cmpBits(`${s.name} q${qi} out`, ex.probe_cct_out_at(i), toBits(innerTiles[i]), i);
    }
    cmpU32(`${s.name} q${qi} seen flag`, ex.probe_cct_seen_flag(), seen ? 1 : 0, qi);
    if (seen) {
      cmpU32(`${s.name} q${qi} seen multi`, ex.probe_cct_seen_multi(), seen.m, qi);
      checks++;
      if (Number(ex.probe_cct_seen_len()) !== seen.tiles.length)
        fail(`${s.name} q${qi} seen length`, Number(ex.probe_cct_seen_len()), seen.tiles.length, qi);
      for (let i = 0; i < seen.tiles.length; i++)
        cmpBits(`${s.name} q${qi} seen`, ex.probe_cct_seen_at(i), toBits(seen.tiles[i]), i);
      cmpBits(`${s.name} q${qi} seen goal`, ex.probe_cct_seen_goal(), toBits(seen.goal), qi);
    }
  }
}

// --- ShoreCoercingTransformer (ShoreCoercingTransformer.ts) ------------------
// Replays each query: build the land/water map, queue the start refs + inner
// tiles, run, and compare the coerced PathStart the inner stub received and the
// restored/extended output path bit-exactly.
for (const s of S.shorecoercing) {
  ex.probe_sct_water_reset();
  for (const [x, y] of s.water) ex.probe_sct_water(x, y);
  ex.probe_sct_new(s.w, s.h);
  for (let qi = 0; qi < s.groups.length; qi++) {
    const g = s.groups[qi];
    let j = 0;
    const fromIsArray = g[j++];
    const fromLen = g[j++];
    const fromTiles = g.slice(j, j + fromLen);
    j += fromLen;
    const to = g[j++];
    const innerMode = g[j++];
    const innerLen = g[j++];
    const innerTiles = g.slice(j, j + innerLen);
    j += innerLen;
    const seenFlag = g[j++];
    let seen = null;
    if (seenFlag === 1) {
      const m = g[j++];
      const l = g[j++];
      seen = { m, tiles: g.slice(j, j + l), goal: g[j + l] };
      j += l + 1;
    }
    const outMode = g[j++];
    const outTiles = outMode === 2 ? g.slice(j + 1, j + 1 + g[j]) : [];

    ex.probe_sct_reset();
    for (const t of fromTiles) ex.probe_sct_from(t);
    for (const t of innerTiles) ex.probe_sct_inner(t);
    const got = ex.probe_sct_run(to, fromIsArray, innerMode);
    cmpU32(`${s.name} q${qi} out mode`, got, outMode, qi);
    if (outMode === 2) {
      checks++;
      if (Number(ex.probe_sct_out_len()) !== outTiles.length)
        fail(`${s.name} q${qi} out length`, Number(ex.probe_sct_out_len()), outTiles.length, qi);
      for (let i = 0; i < outTiles.length; i++)
        cmpBits(`${s.name} q${qi} out`, ex.probe_sct_out_at(i), toBits(outTiles[i]), i);
    }
    cmpU32(`${s.name} q${qi} seen flag`, ex.probe_sct_seen_flag(), seen ? 1 : 0, qi);
    if (seen) {
      cmpU32(`${s.name} q${qi} seen multi`, ex.probe_sct_seen_multi(), seen.m, qi);
      checks++;
      if (Number(ex.probe_sct_seen_len()) !== seen.tiles.length)
        fail(`${s.name} q${qi} seen length`, Number(ex.probe_sct_seen_len()), seen.tiles.length, qi);
      for (let i = 0; i < seen.tiles.length; i++)
        cmpBits(`${s.name} q${qi} seen`, ex.probe_sct_seen_at(i), toBits(seen.tiles[i]), i);
      cmpBits(`${s.name} q${qi} seen goal`, ex.probe_sct_seen_goal(), toBits(seen.goal), qi);
    }
  }
}

// --- SmoothingWaterTransformer (SmoothingWaterTransformer.ts) ----------------
// Replays each query: build the water map from the [x, y, byte] cell list,
// queue the start refs + inner tiles, run, and compare the PathStart the
// inner stub received and the smoothed output path bit-exactly.
for (const s of S.smoothingwater) {
  ex.probe_swt_cell_reset();
  for (const [x, y, b] of s.cells) ex.probe_swt_cell(x, y, b);
  ex.probe_swt_new(s.w, s.h);
  for (let qi = 0; qi < s.groups.length; qi++) {
    const g = s.groups[qi];
    let j = 0;
    const fromIsArray = g[j++];
    const fromLen = g[j++];
    const fromTiles = g.slice(j, j + fromLen);
    j += fromLen;
    const to = g[j++];
    const innerMode = g[j++];
    const innerLen = g[j++];
    const innerTiles = g.slice(j, j + innerLen);
    j += innerLen;
    const seenFlag = g[j++];
    let seen = null;
    if (seenFlag === 1) {
      const m = g[j++];
      const l = g[j++];
      seen = { m, tiles: g.slice(j, j + l), goal: g[j + l] };
      j += l + 1;
    }
    const outMode = g[j++];
    const outTiles = outMode === 2 ? g.slice(j + 1, j + 1 + g[j]) : [];

    ex.probe_swt_reset();
    for (const t of fromTiles) ex.probe_swt_from(t);
    for (const t of innerTiles) ex.probe_swt_inner(t);
    const got = ex.probe_swt_run(to, fromIsArray, innerMode);
    cmpU32(`${s.name} q${qi} out mode`, got, outMode, qi);
    if (outMode === 2) {
      checks++;
      if (Number(ex.probe_swt_out_len()) !== outTiles.length)
        fail(`${s.name} q${qi} out length`, Number(ex.probe_swt_out_len()), outTiles.length, qi);
      for (let i = 0; i < outTiles.length; i++)
        cmpBits(`${s.name} q${qi} out`, ex.probe_swt_out_at(i), toBits(outTiles[i]), i);
    }
    cmpU32(`${s.name} q${qi} seen flag`, ex.probe_swt_seen_flag(), seen ? 1 : 0, qi);
    if (seen) {
      cmpU32(`${s.name} q${qi} seen multi`, ex.probe_swt_seen_multi(), seen.m, qi);
      checks++;
      if (Number(ex.probe_swt_seen_len()) !== seen.tiles.length)
        fail(`${s.name} q${qi} seen length`, Number(ex.probe_swt_seen_len()), seen.tiles.length, qi);
      for (let i = 0; i < seen.tiles.length; i++)
        cmpBits(`${s.name} q${qi} seen`, ex.probe_swt_seen_at(i), toBits(seen.tiles[i]), i);
      cmpBits(`${s.name} q${qi} seen goal`, ex.probe_swt_seen_goal(), toBits(seen.goal), qi);
    }
  }
}

// --- Generic BFS (BFS.ts) -----------------------------------------------------
// Rebuild the edge table through the scalar probe API, replay the search, and
// compare the full (node, dist) visitor stream and the return bit-exactly.
for (const s of S.bfs) {
  ex.probe_bfs_edge_reset();
  for (const [k, nb] of s.edges) {
    ex.probe_bfs_edge_key(numTok(k));
    for (const n of nb) ex.probe_bfs_edge_nb(numTok(n));
    ex.probe_bfs_edge_end();
  }
  ex.probe_bfs_reset();
  for (const st of s.starts) ex.probe_bfs_start(numTok(st));
  const found = Number(ex.probe_bfs_run(numTok(s.maxd), s.mode, numTok(s.blocker), numTok(s.foundval)));
  cmpU32(`${s.name} result flag`, found, s.result === "u" ? 0 : 1, 0);
  const nvisits = s.visits.length;
  checks++;
  if (Number(ex.probe_bfs_visits_len()) !== nvisits * 2)
    fail(`${s.name} visits length`, 0, Number(ex.probe_bfs_visits_len()), nvisits * 2);
  for (let i = 0; i < nvisits; i++) {
    cmpBits(`${s.name} visit[${i}] node`, ex.probe_bfs_visit_node(i), toBits(numTok(s.visits[i][0])), i);
    cmpBits(`${s.name} visit[${i}] dist`, ex.probe_bfs_visit_dist(i), toBits(numTok(s.visits[i][1])), i);
  }
  if (s.result !== "u")
    cmpBits(`${s.name} result`, ex.probe_bfs_result(), toBits(numTok(s.result)), 0);
}

// --- AirPathFinder (PathFinder.Air.ts) ----------------------------------------
// Replays the walk against the wasm finder and compares the (x, y) coordinate
// stream bit-exactly; the multi-start / OOB-ref throws are pinned via the
// would-throw flag.
for (const s of S.air) {
  ex.probe_air_new(s.w, s.h);
  const got = Number(ex.probe_air_run(numTok(s.ticks), numTok(s.from), numTok(s.to), s.multi ? 1 : 0));
  cmpU32(`${s.name} threw flag`, got, s.threw ? 0 : 1, 0);
  if (!s.threw) {
    checks++;
    if (Number(ex.probe_air_path_len()) !== s.path.length * 2)
      fail(`${s.name} path length`, 0, Number(ex.probe_air_path_len()), s.path.length * 2);
    for (let i = 0; i < s.path.length; i++) {
      cmpBits(`${s.name} path[${i}] x`, ex.probe_air_path_at(2 * i), toBits(s.path[i][0]), i);
      cmpBits(`${s.name} path[${i}] y`, ex.probe_air_path_at(2 * i + 1), toBits(s.path[i][1]), i);
    }
  }
}

// --- AnonNames (AnonNames.ts) -------------------------------------------------
// Replays anonWordName and compares the handle's UTF-16 code-unit stream
// (length + each unit); a -1 length is the JS `undefined` a missed word lookup
// returns when round === 0.
for (const s of S.anon) {
  const len = Number(ex.probe_anon_run(numTok(s.slot), numTok(s.offset), s.has));
  checks++;
  if (s.res === "u") {
    if (len !== -1) fail(`${s.name} undefined`, 0, len, -1);
    continue;
  }
  if (len !== s.res.length) {
    fail(`${s.name} len`, 0, len, s.res.length);
    continue;
  }
  let got = "";
  for (let i = 0; i < len; i++) {
    checks++;
    got += String.fromCharCode(ex.probe_anon_out_at(i));
  }
  if (got !== s.res) fail(`${s.name} handle`, 0, got, s.res);
}

// --- CloseCodes (CloseCodes.ts) -----------------------------------------------
// Replays both predicates; kind 0 pushes the code, kind 1 the UTF-16 string.
for (const s of S.close) {
  if (s.kind === 0) {
    ex.probe_close_arg(numTok(s.code));
  } else {
    ex.probe_close_arg(s.val.length);
    for (let i = 0; i < s.val.length; i++) ex.probe_close_arg(s.val.charCodeAt(i));
  }
  checks++;
  const got = Number(ex.probe_close_op(s.kind));
  if (got !== (s.res ? 1 : 0))
    fail(`${s.name} verdict`, 0, got, s.res ? 1 : 0);
}

// --- ServerList (ServerList.ts) -----------------------------------------------
// Replays every pure function through the shared run_op runner; args and res
// are flat f64 token streams, compared element-by-element with Object.is.
for (const s of S.serverlist) {
  for (const a of s.args) ex.probe_sl_arg(numTok(a));
  const len = Number(ex.probe_sl_op(s.kind));
  if (len !== s.res.length) {
    fail(`${s.name} res len`, 0, len, s.res.length);
    continue;
  }
  for (let i = 0; i < len; i++) {
    checks++;
    const g = ex.probe_sl_out_at(i);
    const w = numTok(s.res[i]);
    if (!Object.is(g, w)) fail(`${s.name} res[${i}]`, 0, g, w);
  }
}

// --- AssetUrls (AssetUrls.ts) -------------------------------------------------
// Replays normalize / encode / buildAssetUrl through the shared run_op runner;
// args and res are flat f64 token streams, compared element-by-element with
// Object.is.
for (const s of S.asseturls) {
  for (const a of s.args) ex.probe_au_arg(numTok(a));
  const len = Number(ex.probe_au_op(s.kind));
  if (len !== s.res.length) {
    fail(`${s.name} res len`, 0, len, s.res.length);
    continue;
  }
  for (let i = 0; i < len; i++) {
    checks++;
    const g = ex.probe_au_out_at(i);
    const w = numTok(s.res[i]);
    if (!Object.is(g, w)) fail(`${s.name} res[${i}]`, 0, g, w);
  }
}

// --- Maps.gen (Maps.gen.ts) ---------------------------------------------------
// Replays the data-table dumps / id lookup through the shared run_op runner;
// args and res are flat f64 token streams, compared element-by-element with
// Object.is.
for (const s of S.maps) {
  for (const a of s.args) ex.probe_mg_arg(numTok(a));
  const len = Number(ex.probe_mg_op(s.kind));
  if (len !== s.res.length) {
    fail(`${s.name} res len`, 0, len, s.res.length);
    continue;
  }
  for (let i = 0; i < len; i++) {
    checks++;
    const g = ex.probe_mg_out_at(i);
    const w = numTok(s.res[i]);
    if (!Object.is(g, w)) fail(`${s.name} res[${i}]`, 0, g, w);
  }
}

// --- TribeNames (TribeNames.ts) -----------------------------------------------
// Replays resolveTribeNameData through the shared run_op runner; the
// capture's theme-record mutations ride in as removed/blanked name lists.
// Args and res are flat f64 token streams, compared element-by-element with
// Object.is.
for (const s of S.tribenames) {
  for (const a of s.args) ex.probe_tn_arg(numTok(a));
  const len = Number(ex.probe_tn_op(s.kind));
  if (len !== s.res.length) {
    fail(`${s.name} res len`, 0, len, s.res.length);
    continue;
  }
  for (let i = 0; i < len; i++) {
    checks++;
    const g = ex.probe_tn_out_at(i);
    const w = numTok(s.res[i]);
    if (!Object.is(g, w)) fail(`${s.name} res[${i}]`, 0, g, w);
  }
}

// --- game/Game.ts (game_ts) --------------------------------------------------
// Replays the runtime-value subset through the shared run_op runner; args and
// res are flat f64 token streams, compared element-by-element with Object.is.
for (const s of S.game) {
  for (const a of s.args) ex.probe_game_arg(numTok(a));
  const len = Number(ex.probe_game_op(s.kind));
  if (len !== s.res.length) {
    fail(`${s.name} res len`, 0, len, s.res.length);
    continue;
  }
  for (let i = 0; i < len; i++) {
    checks++;
    const g = ex.probe_game_out_at(i);
    const w = numTok(s.res[i]);
    if (!Object.is(g, w)) fail(`${s.name} res[${i}]`, 0, g, w);
  }
}

// --- game/NationCreation.ts (nation_creation) --------------------------------
// Replays the tables dump / pluralize / name generation / createRandomNations
// through the shared run_op runner; args and res are flat f64 token streams,
// compared element-by-element with Object.is.
for (const s of S.nationcreation) {
  for (const a of s.args) ex.probe_nc_arg(numTok(a));
  const len = Number(ex.probe_nc_op(s.kind));
  if (len !== s.res.length) {
    fail(`${s.name} res len`, 0, len, s.res.length);
    continue;
  }
  for (let i = 0; i < len; i++) {
    checks++;
    const g = ex.probe_nc_out_at(i);
    const w = numTok(s.res[i]);
    if (!Object.is(g, w)) fail(`${s.name} res[${i}]`, 0, g, w);
  }
}

// --- game/GameUpdates.ts (game_updates) --------------------------------------
// Replays the GameUpdateType dump / name lookup through the shared run_op
// runner; args and res are flat f64 token streams, compared element-by-element
// with Object.is.
for (const s of S.gameupdates) {
  for (const a of s.args) ex.probe_gupd_arg(numTok(a));
  const len = Number(ex.probe_gupd_op(s.kind));
  if (len !== s.res.length) {
    fail(`${s.name} res len`, 0, len, s.res.length);
    continue;
  }
  for (let i = 0; i < len; i++) {
    checks++;
    const g = ex.probe_gupd_out_at(i);
    const w = numTok(s.res[i]);
    if (!Object.is(g, w)) fail(`${s.name} res[${i}]`, 0, g, w);
  }
}

// --- Util.ts emojiTable + NationEmojiBehavior.ts (nation_emoji) --------------
// Replays the table dumps / EMOJI_* id arrays / emoji_id batches through the
// shared run_op runner; args and res are flat f64 token streams, compared
// element-by-element with Object.is.
for (const s of S.nationemoji) {
  for (const a of s.args) ex.probe_ne_arg(numTok(a));
  const len = Number(ex.probe_ne_op(s.kind));
  if (len !== s.res.length) {
    fail(`${s.name} res len`, 0, len, s.res.length);
    continue;
  }
  for (let i = 0; i < len; i++) {
    checks++;
    const g = ex.probe_ne_out_at(i);
    const w = numTok(s.res[i]);
    if (!Object.is(g, w)) fail(`${s.name} res[${i}]`, 0, g, w);
  }
}

// --- CosmeticSchemas.ts (cosmetic_schemas) -----------------------------------
// Replays the effect-type array dumps / DefaultPattern / the pure effect-slot
// function batches through the shared run_op runner; args and res are flat
// f64 token streams, compared element-by-element with Object.is.
for (const s of S.cosmeticschemas) {
  for (const a of s.args) ex.probe_cs_arg(numTok(a));
  const len = Number(ex.probe_cs_op(s.kind));
  if (len !== s.res.length) {
    fail(`${s.name} res len`, 0, len, s.res.length);
    continue;
  }
  for (let i = 0; i < len; i++) {
    checks++;
    const g = ex.probe_cs_out_at(i);
    const w = numTok(s.res[i]);
    if (!Object.is(g, w)) fail(`${s.name} res[${i}]`, 0, g, w);
  }
}

// --- StatsSchemas.ts (stats_schemas) ------------------------------------------
// Replays the unit-name array dumps / lookup-table dumps / the index-constant
// dump / the toBigInt coercion batches through the shared run_op runner; args
// and res are flat f64 token streams, compared element-by-element with
// Object.is.
for (const s of S.statschemas) {
  for (const a of s.args) ex.probe_ss_arg(numTok(a));
  const len = Number(ex.probe_ss_op(s.kind));
  if (len !== s.res.length) {
    fail(`${s.name} res len`, 0, len, s.res.length);
    continue;
  }
  for (let i = 0; i < len; i++) {
    checks++;
    const g = ex.probe_ss_out_at(i);
    const w = numTok(s.res[i]);
    if (!Object.is(g, w)) fail(`${s.name} res[${i}]`, 0, g, w);
  }
}

// --- Schemas.ts (schemas) ------------------------------------------------------
// Replays the enum option-array dumps / the lobby constants / the LogSeverity
// table / the QuickChat key dump / the isValidGameID and renderable-name regex
// batches through the shared run_op runner; args and res are flat f64 token
// streams, compared element-by-element with Object.is.
for (const s of S.schemas) {
  for (const a of s.args) ex.probe_sc_arg(numTok(a));
  const len = Number(ex.probe_sc_op(s.kind));
  if (len !== s.res.length) {
    fail(`${s.name} res len`, 0, len, s.res.length);
    continue;
  }
  for (let i = 0; i < len; i++) {
    checks++;
    const g = ex.probe_sc_out_at(i);
    const w = numTok(s.res[i]);
    if (!Object.is(g, w)) fail(`${s.name} res[${i}]`, 0, g, w);
  }
}

// --- ApiSchemas.ts (api_schemas) ---------------------------------------------
// Replays the data-constant / z.enum option-array dumps and the isAdminRole /
// isTemporaryUsername / isVerifiedUsername / isGrantedSubscription predicate
// batches through the shared run_op runner; args and res are flat f64 token
// streams, compared element-by-element with Object.is.
for (const s of S.apischemas) {
  for (const a of s.args) ex.probe_as_arg(numTok(a));
  const len = Number(ex.probe_as_op(s.kind));
  if (len !== s.res.length) {
    fail(`${s.name} res len`, 0, len, s.res.length);
    continue;
  }
  for (let i = 0; i < len; i++) {
    checks++;
    const g = ex.probe_as_out_at(i);
    const w = numTok(s.res[i]);
    if (!Object.is(g, w)) fail(`${s.name} res[${i}]`, 0, g, w);
  }
}

// --- game/TerrainMapLoader.ts (terrain_map_loader) ----------------------------
// Replays the scripted loadTerrainMap scenarios (cache hit / throw-never-
// caches / in-place Compact scaling / spawn-area scaling / placement + alpha
// validation) through the shared run_op runner; args and res are flat f64
// token streams, compared element-by-element with Object.is.
for (const s of S.terrainmaploader) {
  for (const a of s.args) ex.probe_tml_arg(numTok(a));
  const len = Number(ex.probe_tml_op(s.kind));
  if (len !== s.res.length) {
    fail(`${s.name} res len`, 0, len, s.res.length);
    continue;
  }
  for (let i = 0; i < len; i++) {
    checks++;
    const g = ex.probe_tml_out_at(i);
    const w = numTok(s.res[i]);
    if (!Object.is(g, w)) fail(`${s.name} res[${i}]`, 0, g, w);
  }
}

// --- execution/nation/NationUtils.ts (nation_utils) ---------------------------
// Replays the scripted-mock randTerritoryTileArray / findJuiciestTarget
// scenarios (sampling loop / fallback / normalize + strict-`>` best scan)
// through the shared run_op runner; args and res are flat f64 token streams,
// compared element-by-element with Object.is.
for (const s of S.nationutils) {
  for (const a of s.args) ex.probe_nu_arg(numTok(a));
  const len = Number(ex.probe_nu_op(s.kind));
  if (len !== s.res.length) {
    fail(`${s.name} res len`, 0, len, s.res.length);
    continue;
  }
  for (let i = 0; i < len; i++) {
    checks++;
    const g = ex.probe_nu_out_at(i);
    const w = numTok(s.res[i]);
    if (!Object.is(g, w)) fail(`${s.name} res[${i}]`, 0, g, w);
  }
}

// --- execution/ExecutionManager.ts (execution_manager) ------------------------
// Replays the scripted-mock Executor scenarios (ctor seed pin / switch-case
// construction traces / !player warn branch / default throw / spawner stubs)
// through the shared run_op runner; args and res are flat f64 token streams,
// compared element-by-element with Object.is.
for (const s of S.executionmanager) {
  for (const a of s.args) ex.probe_em_arg(numTok(a));
  const len = Number(ex.probe_em_op(s.kind));
  if (len !== s.res.length) {
    fail(`${s.name} res len`, 0, len, s.res.length);
    continue;
  }
  for (let i = 0; i < len; i++) {
    checks++;
    const g = ex.probe_em_out_at(i);
    const w = numTok(s.res[i]);
    if (!Object.is(g, w)) fail(`${s.name} res[${i}]`, 0, g, w);
  }
}

// --- game/GameImpl.ts createGameUpdatesMap (game_updates kinds 2/3) ----------
// Replays the map() dump and the Object.values + filter pin through the same
// game_updates run_op runner as the gupd scenarios.
for (const s of S.gameimpl) {
  for (const a of s.args) ex.probe_gupd_arg(numTok(a));
  const len = Number(ex.probe_gupd_op(s.kind));
  if (len !== s.res.length) {
    fail(`${s.name} res len`, 0, len, s.res.length);
    continue;
  }
  for (let i = 0; i < len; i++) {
    checks++;
    const g = ex.probe_gupd_out_at(i);
    const w = numTok(s.res[i]);
    if (!Object.is(g, w)) fail(`${s.name} res[${i}]`, 0, g, w);
  }
}

// --- game/TerraNulliusImpl.ts (terra_nullius) --------------------------------
// Replays the four constant-return methods through the shared run_op runner.
for (const s of S.terranulliusimpl) {
  for (const a of s.args) ex.probe_tni_arg(numTok(a));
  const len = Number(ex.probe_tni_op(s.kind));
  if (len !== s.res.length) {
    fail(`${s.name} res len`, 0, len, s.res.length);
    continue;
  }
  for (let i = 0; i < len; i++) {
    checks++;
    const g = ex.probe_tni_out_at(i);
    const w = numTok(s.res[i]);
    if (!Object.is(g, w)) fail(`${s.name} res[${i}]`, 0, g, w);
  }
}

// --- game/StatsImpl.ts (stats_impl) -------------------------------------------
// Replays the scripted-mock bigint accumulator scenarios (facade trace + throw
// points + stats() dump) through the shared run_op runner; args and res are
// flat f64 token streams, compared element-by-element with Object.is.
for (const s of S.statsimpl) {
  for (const a of s.args) ex.probe_si_arg(numTok(a));
  const len = Number(ex.probe_si_op(s.kind));
  if (len !== s.res.length) {
    fail(`${s.name} res len`, 0, len, s.res.length);
    continue;
  }
  for (let i = 0; i < len; i++) {
    checks++;
    const g = ex.probe_si_out_at(i);
    const w = numTok(s.res[i]);
    if (!Object.is(g, w)) fail(`${s.name} res[${i}]`, 0, g, w);
  }
}

// --- execution/nation/SharedWaterCache.ts (shared_water_cache) ----------------
// Replays the scripted-mock shared-water cache scenarios (TTL rebuild /
// waterFor rescan traces + per-get dumps + final internal state) through the
// shared run_op runner; args and res are flat f64 token streams, compared
// element-by-element with Object.is.
for (const s of S.sharedwatercache) {
  for (const a of s.args) ex.probe_swc_arg(numTok(a));
  const len = Number(ex.probe_swc_op(s.kind));
  if (len !== s.res.length) {
    fail(`${s.name} res len`, 0, len, s.res.length);
    continue;
  }
  for (let i = 0; i < len; i++) {
    checks++;
    const g = ex.probe_swc_out_at(i);
    const w = numTok(s.res[i]);
    if (!Object.is(g, w)) fail(`${s.name} res[${i}]`, 0, g, w);
  }
}

// --- pathfinding/PathFinder.ts WaterPathMemo (water_path_memo) ---------------
// Replays the scripted-inner memo scenarios through the shared run_op runner;
// args and res are flat f64 token streams, compared element-by-element with
// Object.is.
for (const s of S.waterpathmemo) {
  for (const a of s.args) ex.probe_wpm_arg(numTok(a));
  const len = Number(ex.probe_wpm_op(s.kind));
  if (len !== s.res.length) {
    fail(`${s.name} res len`, 0, len, s.res.length);
    continue;
  }
  for (let i = 0; i < len; i++) {
    checks++;
    const g = ex.probe_wpm_out_at(i);
    const w = numTok(s.res[i]);
    if (!Object.is(g, w)) fail(`${s.name} res[${i}]`, 0, g, w);
  }
}

// --- PatternDecoder (PatternDecoder.ts) ----------------------------------------
// Replays decode + isPrimary through the shared run_op runner; args and res
// are flat f64 token streams, compared element-by-element with Object.is.
for (const s of S.patterndecoder) {
  for (const a of s.args) ex.probe_pd_arg(numTok(a));
  const len = Number(ex.probe_pd_op(s.kind));
  if (len !== s.res.length) {
    fail(`${s.name} res len`, 0, len, s.res.length);
    continue;
  }
  for (let i = 0; i < len; i++) {
    checks++;
    const g = ex.probe_pd_out_at(i);
    const w = numTok(s.res[i]);
    if (!Object.is(g, w)) fail(`${s.name} res[${i}]`, 0, g, w);
  }
}

// --- DoomsdayClock (DoomsdayClock.ts) ------------------------------------------
// Replays the wave math through the shared run_op runner.
for (const s of S.doomsdayclock) {
  for (const a of s.args) ex.probe_dc_arg(numTok(a));
  const len = Number(ex.probe_dc_op(s.kind));
  if (len !== s.res.length) {
    fail(`${s.name} res len`, 0, len, s.res.length);
    continue;
  }
  for (let i = 0; i < len; i++) {
    checks++;
    const g = ex.probe_dc_out_at(i);
    const w = numTok(s.res[i]);
    if (!Object.is(g, w)) fail(`${s.name} res[${i}]`, 0, g, w);
  }
}

// --- execution/Util.ts (exec_util) ---------------------------------------------
// Rebuild the packed-terrain map + owner writes, then replay one run_op.
for (const s of S.executil) {
  for (const b of s.terrain) ex.probe_eu_terrain_byte(b);
  for (const [t, id] of s.owners) ex.probe_eu_owner(numTok(t), numTok(id));
  ex.probe_eu_new(s.w, s.h);
  for (const a of s.args) ex.probe_eu_arg(numTok(a));
  const len = Number(ex.probe_eu_op(s.kind));
  if (len !== s.res.length) {
    fail(`${s.name} res len`, 0, len, s.res.length);
    continue;
  }
  for (let i = 0; i < len; i++) {
    checks++;
    const g = ex.probe_eu_out_at(i);
    const w = numTok(s.res[i]);
    if (!Object.is(g, w)) fail(`${s.name} res[${i}]`, 0, g, w);
  }
}

// --- game/WaterManager.ts (water_manager) ------------------------------------
// Queue both packed terrains (initial state is all-zero), build the manager,
// replay the op stream, then diff the version counter and the final buffers.
for (const s of S.watermanager) {
  for (const b of s.mapTerrain) ex.probe_wm_map_terrain_byte(b);
  for (const b of s.miniTerrain) ex.probe_wm_mini_terrain_byte(b);
  ex.probe_wm_new(s.mw, s.mh, s.nw, s.nh, s.disable ? 1 : 0);
  for (const [k, a, b, res] of s.ops) {
    ex.probe_wm_arg(numTok(a));
    ex.probe_wm_arg(numTok(b));
    const len = Number(ex.probe_wm_op(k));
    if (len !== res.length) {
      fail(`${s.name} op${k} res len`, 0, len, res.length);
      continue;
    }
    for (let i = 0; i < len; i++) {
      checks++;
      const g = ex.probe_wm_out_at(i);
      const w = numTok(res[i]);
      if (!Object.is(g, w)) fail(`${s.name} op${k} res[${i}]`, 0, g, w);
    }
  }
  checks++;
  {
    const g = ex.probe_wm_version();
    const w = numTok(s.versionAfter);
    if (!Object.is(g, w)) fail(`${s.name} version`, 0, g, w);
  }
  for (let i = 0; i < s.mapTerrainAfter.length; i++)
    cmpU32(`${s.name} mapTerrain`, ex.probe_wm_map_terrain_at(i), s.mapTerrainAfter[i], i);
  for (let i = 0; i < s.mapStateAfter.length; i++)
    cmpU32(`${s.name} mapState`, ex.probe_wm_map_state_at(i), s.mapStateAfter[i], i);
  for (let i = 0; i < s.miniTerrainAfter.length; i++)
    cmpU32(`${s.name} miniTerrain`, ex.probe_wm_mini_terrain_at(i), s.miniTerrainAfter[i], i);
}

// --- game/GameUpdateUtils.ts (game_update_utils) ------------------------------
// Replays diff / apply / pack through the shared run_op runner; args and res
// are flat f64 token streams, compared element-by-element with Object.is.
for (const s of S.gameupdateutils) {
  for (const a of s.args) ex.probe_gu_arg(numTok(a));
  const len = Number(ex.probe_gu_op(s.kind));
  if (len !== s.res.length) {
    fail(`${s.name} res len`, 0, len, s.res.length);
    continue;
  }
  for (let i = 0; i < len; i++) {
    checks++;
    const g = ex.probe_gu_out_at(i);
    const w = numTok(s.res[i]);
    if (!Object.is(g, w)) fail(`${s.name} res[${i}]`, 0, g, w);
  }
}

// --- game/Railroad.ts (railroad) ----------------------------------------------
// Replays closest-tile-index / oriented-railroad / delete through the shared
// run_op runner; stations and rails cross by capture-assigned refids.
for (const s of S.railroad) {
  for (const a of s.args) ex.probe_rr_arg(numTok(a));
  const len = Number(ex.probe_rr_op(s.kind));
  if (len !== s.res.length) {
    fail(`${s.name} res len`, 0, len, s.res.length);
    continue;
  }
  for (let i = 0; i < len; i++) {
    checks++;
    const g = ex.probe_rr_out_at(i);
    const w = numTok(s.res[i]);
    if (!Object.is(g, w)) fail(`${s.name} res[${i}]`, 0, g, w);
  }
}

// --- game/TileTraversalScratch.ts (tile_traversal_scratch) --------------------
// Replays the stateful op stream (allocate / bump / typed-array writes /
// stack push) through the wasm RigHarness. Each scenario resets the harness
// first (a fresh WeakMap per capture scenario).
for (const s of S.tiletravscratch) {
  ex.probe_tts_reset();
  for (const op of s.ops) {
    for (const a of op.args) ex.probe_tts_arg(numTok(a));
    const len = Number(ex.probe_tts_op(op.kind));
    if (len !== op.res.length) {
      fail(`${s.name} op${op.kind} res len`, 0, len, op.res.length);
      continue;
    }
    for (let i = 0; i < len; i++) {
      checks++;
      const g = ex.probe_tts_out_at(i);
      const w = numTok(op.res[i]);
      if (!Object.is(g, w)) fail(`${s.name} op${op.kind} res[${i}]`, 0, g, w);
    }
  }
}

// --- EventBus.ts (event_bus) --------------------------------------------------
// Replays the stateful op stream (on / off / emit call trace / Map-order dump)
// through the wasm RigHarness. Each scenario resets the harness first.
for (const s of S.eventbus) {
  ex.probe_eb_reset();
  for (const op of s.ops) {
    for (const a of op.args) ex.probe_eb_arg(numTok(a));
    const len = Number(ex.probe_eb_op(op.kind));
    if (len !== op.res.length) {
      fail(`${s.name} op${op.kind} res len`, 0, len, op.res.length);
      continue;
    }
    for (let i = 0; i < len; i++) {
      checks++;
      const g = ex.probe_eb_out_at(i);
      const w = numTok(op.res[i]);
      if (!Object.is(g, w)) fail(`${s.name} op${op.kind} res[${i}]`, 0, g, w);
    }
  }
}

// --- game/UnitGrid.ts (unit_grid) ---------------------------------------------
// Replays the stateful op stream (construct / add / remove / update / probes /
// nearby / has / any / dump) through the wasm RigHarness. Each scenario resets
// the harness first.
for (const s of S.unitgrid) {
  ex.probe_ug_reset();
  for (const op of s.ops) {
    for (const a of op.args) ex.probe_ug_arg(numTok(a));
    const len = Number(ex.probe_ug_op(op.kind));
    if (len !== op.res.length) {
      fail(`${s.name} op${op.kind} res len`, 0, len, op.res.length);
      continue;
    }
    for (let i = 0; i < len; i++) {
      checks++;
      const g = ex.probe_ug_out_at(i);
      const w = numTok(op.res[i]);
      if (!Object.is(g, w)) fail(`${s.name} op${op.kind} res[${i}]`, 0, g, w);
    }
  }
}

// --- game/RailroadSpatialGrid.ts (railroad_spatial_grid) ----------------------
// Replays the stateful op stream (construct / register / unregister / query /
// dumps) through the wasm RigHarness. Each scenario resets the harness first.
for (const s of S.railgrid) {
  ex.probe_rsg_reset();
  for (const op of s.ops) {
    for (const a of op.args) ex.probe_rsg_arg(numTok(a));
    const len = Number(ex.probe_rsg_op(op.kind));
    if (len !== op.res.length) {
      fail(`${s.name} op${op.kind} res len`, 0, len, op.res.length);
      continue;
    }
    for (let i = 0; i < len; i++) {
      checks++;
      const g = ex.probe_rsg_out_at(i);
      const w = numTok(op.res[i]);
      if (!Object.is(g, w)) fail(`${s.name} op${op.kind} res[${i}]`, 0, g, w);
    }
  }
}

// --- game/RailNetworkImpl.ts StationManagerImpl (station_manager) -------------
// Replays the stateful op stream (construct / add / remove / find / getById /
// count / dumps) through the wasm RigHarness. Each scenario resets first.
for (const s of S.stationmanager) {
  ex.probe_stm_reset();
  for (const op of s.ops) {
    for (const a of op.args) ex.probe_stm_arg(numTok(a));
    const len = Number(ex.probe_stm_op(op.kind));
    if (len !== op.res.length) {
      fail(`${s.name} op${op.kind} res len`, 0, len, op.res.length);
      continue;
    }
    for (let i = 0; i < len; i++) {
      checks++;
      const g = ex.probe_stm_out_at(i);
      const w = numTok(op.res[i]);
      if (!Object.is(g, w)) fail(`${s.name} op${op.kind} res[${i}]`, 0, g, w);
    }
  }
}

// --- game/TrainStation.ts (train_station) -------------------------------------
// Replays the stateful op stream (stations / rails / clusters / trade probes /
// dumps) through the wasm RigHarness. Each scenario resets first.
for (const s of S.trainstation) {
  ex.probe_tsn_reset();
  for (const op of s.ops) {
    for (const a of op.args) ex.probe_tsn_arg(numTok(a));
    const len = Number(ex.probe_tsn_op(op.kind));
    if (len !== op.res.length) {
      fail(`${s.name} op${op.kind} res len`, 0, len, op.res.length);
      continue;
    }
    for (let i = 0; i < len; i++) {
      checks++;
      const g = ex.probe_tsn_out_at(i);
      const w = numTok(op.res[i]);
      if (!Object.is(g, w)) fail(`${s.name} op${op.kind} res[${i}]`, 0, g, w);
    }
  }
}

// --- game/RailNetworkImpl.ts (rail_network) -----------------------------------
// Replays the stateful op stream (construct / station / connect / recompute /
// remove / overlapping / ghost / path / mgr / dump / unit / factory) through
// the wasm RigHarness. Each scenario resets first.
for (const s of S.railnetwork) {
  ex.probe_rn_reset();
  for (const op of s.ops) {
    for (const a of op.args) ex.probe_rn_arg(numTok(a));
    const len = Number(ex.probe_rn_op(op.kind));
    if (len !== op.res.length) {
      fail(`${s.name} op${op.kind} res len`, 0, len, op.res.length);
      continue;
    }
    for (let i = 0; i < len; i++) {
      checks++;
      const g = ex.probe_rn_out_at(i);
      const w = numTok(op.res[i]);
      if (!Object.is(g, w)) fail(`${s.name} op${op.kind} res[${i}]`, 0, g, w);
    }
  }
}

// --- server/VoteTally.ts (vote_tally) -----------------------------------------
for (const s of S.votetally) {
  ex.probe_vt_reset();
  for (const op of s.ops) {
    for (const a of op.args) ex.probe_vt_arg(numTok(a));
    const len = Number(ex.probe_vt_op(op.kind));
    if (len !== op.res.length) {
      fail(`${s.name} op${op.kind} res len`, 0, len, op.res.length);
      continue;
    }
    for (let i = 0; i < len; i++) {
      checks++;
      const g = ex.probe_vt_out_at(i);
      const w = numTok(op.res[i]);
      if (!Object.is(g, w)) fail(`${s.name} op${op.kind} res[${i}]`, 0, g, w);
    }
  }
}

// --- server/ConfigPatch.ts (config_patch) --------------------------------------
for (const s of S.configpatch) {
  ex.probe_cp_reset();
  for (const op of s.ops) {
    for (const a of op.args) ex.probe_cp_arg(numTok(a));
    const len = Number(ex.probe_cp_op(op.kind));
    if (len !== op.res.length) {
      fail(`${s.name} op${op.kind} res len`, 0, len, op.res.length);
      continue;
    }
    for (let i = 0; i < len; i++) {
      checks++;
      const g = ex.probe_cp_out_at(i);
      const w = numTok(op.res[i]);
      if (!Object.is(g, w)) fail(`${s.name} op${op.kind} res[${i}]`, 0, g, w);
    }
  }
}

// --- server/IntentAuthorization.ts (intent_authorization) ----------------------
for (const s of S.intentauth) {
  ex.probe_ia_reset();
  for (const op of s.ops) {
    for (const a of op.args) ex.probe_ia_arg(numTok(a));
    const len = Number(ex.probe_ia_op(op.kind));
    if (len !== op.res.length) {
      fail(`${s.name} op${op.kind} res len`, 0, len, op.res.length);
      continue;
    }
    for (let i = 0; i < len; i++) {
      checks++;
      const g = ex.probe_ia_out_at(i);
      const w = numTok(op.res[i]);
      if (!Object.is(g, w)) fail(`${s.name} op${op.kind} res[${i}]`, 0, g, w);
    }
  }
}

// --- server/Consensus.ts (consensus) --------------------------------------------
for (const s of S.consensus) {
  ex.probe_cv_reset();
  for (const op of s.ops) {
    for (const a of op.args) ex.probe_cv_arg(numTok(a));
    const len = Number(ex.probe_cv_op(op.kind));
    if (len !== op.res.length) {
      fail(`${s.name} op${op.kind} res len`, 0, len, op.res.length);
      continue;
    }
    for (let i = 0; i < len; i++) {
      checks++;
      const g = ex.probe_cv_out_at(i);
      const w = numTok(op.res[i]);
      if (!Object.is(g, w)) fail(`${s.name} op${op.kind} res[${i}]`, 0, g, w);
    }
  }
}

// --- server/ListingState.ts (listing_state) --------------------------------------
for (const s of S.listingstate) {
  ex.probe_ls_reset();
  for (const op of s.ops) {
    for (const a of op.args) ex.probe_ls_arg(numTok(a));
    const len = Number(ex.probe_ls_op(op.kind));
    if (len !== op.res.length) {
      fail(`${s.name} op${op.kind} res len`, 0, len, op.res.length);
      continue;
    }
    for (let i = 0; i < len; i++) {
      checks++;
      const g = ex.probe_ls_out_at(i);
      const w = numTok(op.res[i]);
      if (!Object.is(g, w)) fail(`${s.name} op${op.kind} res[${i}]`, 0, g, w);
    }
  }
}

// --- server/NameVisibility.ts (name_visibility) ----------------------------------
for (const s of S.namevisibility) {
  ex.probe_nvs_reset();
  for (const op of s.ops) {
    for (const a of op.args) ex.probe_nvs_arg(numTok(a));
    const len = Number(ex.probe_nvs_op(op.kind));
    if (len !== op.res.length) {
      fail(`${s.name} op${op.kind} res len`, 0, len, op.res.length);
      continue;
    }
    for (let i = 0; i < len; i++) {
      checks++;
      const g = ex.probe_nvs_out_at(i);
      const w = numTok(op.res[i]);
      if (!Object.is(g, w)) fail(`${s.name} op${op.kind} res[${i}]`, 0, g, w);
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
