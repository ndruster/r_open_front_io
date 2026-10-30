// G6-0 (M1-0) — integration-vector recorder for the `game_loop` harness.
//
// Drives the REAL TS `GameMapImpl` + `WaterManager` (loaded through
// ts_load.mjs, same channel as gen_vectors.mjs) over synthetic terrain bytes
// and pins the whole per-op result stream, bit for bit:
//
//   node tools/gen_m1_vectors.mjs
//
// Writes:
//   crates/core/tests/data/m1_vectors.json   — for tools/m1_scan.mjs (wasm)
//   crates/core/tests/data/m1_vectors.rs     — #[path]-embedded for
//                                              crates/core/tests/parity_m1.rs
//
// Scenario ops mirror `crates/core/src/game_loop.rs`'s op table (kinds 0-6).
// The M1-0 pins documented there apply here too: no Config (waterNukes
// pinned true), no players (owner-throw arms unreachable).
import { writeFileSync, mkdirSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { loadTs } from "./ts_load.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");

const { GameMapImpl } = await loadTs("src/core/game/GameMap.ts");
const { WaterManager } = await loadTs("src/core/game/WaterManager.ts");

// ------------------------------------------------------------- encS mirror
// Bit-exact mirror of js_json.rs push_str: [len, ..utf16 units].

function pushStr(out, s) {
  const units = [];
  for (const ch of s) {
    const cp = ch.codePointAt(0);
    if (cp > 0xffff) {
      units.push(((cp - 0x10000) >> 10) + 0xd800, ((cp - 0x10000) & 0x3ff) + 0xdc00);
    } else {
      units.push(cp);
    }
  }
  out.push(units.length);
  for (const u of units) out.push(u);
}

// ---------------------------------------------------- synthetic terrain gen
// Pure arithmetic; bytes are pinned into the vectors, so only this script
// ever generates them (Rust replays the bytes verbatim).
//
// Island field: min radial distance d over three ellipses.
//   d >= 1        -> ocean water (OCEAN_BIT)
//   d < 0.055     -> impassable land (magnitude 31)
//   else          -> land (IS_LAND_BIT) with magnitude 1 + floor(d * 27)
// Land tiles with a 4-neighbour ocean tile take the SHORELINE bit.

const IS_LAND = 0x80;
const SHORELINE = 0x40;
const OCEAN = 0x20;

function genTerrain(w, h) {
  const islands = [
    [0.26, 0.42, 0.17, 0.34],
    [0.7, 0.58, 0.21, 0.36],
    [0.52, 0.16, 0.11, 0.2],
  ];
  const t = new Uint8Array(w * h);
  const isLand = new Array(w * h).fill(false);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      let d = Infinity;
      for (const [cx, cy, rx, ry] of islands) {
        const dx = (x + 0.5 - cx * w) / (rx * w);
        const dy = (y + 0.5 - cy * h) / (ry * h);
        d = Math.min(d, Math.sqrt(dx * dx + dy * dy));
      }
      const ref = y * w + x;
      if (d >= 1) {
        t[ref] = OCEAN;
      } else {
        isLand[ref] = true;
        const mag = d < 0.055 ? 31 : 1 + Math.floor(d * 27);
        t[ref] = IS_LAND | mag;
      }
    }
  }
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const ref = y * w + x;
      if (!isLand[ref]) continue;
      const nbOcean =
        (x > 0 && (t[ref - 1] & OCEAN) !== 0) ||
        (x < w - 1 && (t[ref + 1] & OCEAN) !== 0) ||
        (y > 0 && (t[ref - w] & OCEAN) !== 0) ||
        (y < h - 1 && (t[ref + w] & OCEAN) !== 0);
      if (nbOcean) t[ref] |= SHORELINE;
    }
  }
  return t;
}

function numLandOf(t) {
  let n = 0;
  for (const b of t) if ((b & IS_LAND) !== 0) n++;
  return n;
}

// Pick deterministic landmark tiles from a terrain array.
function pickTiles(t, w) {
  const land = [];
  const ocean = [];
  const impassable = [];
  for (let ref = 0; ref < t.length; ref++) {
    const b = t[ref];
    if ((b & IS_LAND) !== 0) {
      if ((b & 0x1f) === 31) impassable.push(ref);
      else land.push(ref);
    } else if ((b & OCEAN) !== 0) ocean.push(ref);
  }
  if (land.length < 4 || ocean.length < 2 || impassable.length < 1) {
    throw new Error("synthetic terrain lacks landmarks");
  }
  const mid = Math.floor(land.length / 2);
  return {
    landA: land[Math.floor(land.length * 0.25)],
    landB: land[mid],
    landC: land[Math.floor(land.length * 0.75)],
    oceanA: ocean[Math.floor(ocean.length * 0.5)],
    impassableA: impassable[0],
    // Land tile adjacent to ocean (for shoreline-adjacent conversions).
    landShore: land.find((r) => (t[r] & SHORELINE) !== 0) ?? land[0],
    w,
  };
}

// ------------------------------------------------------------ scenario rig
// Mirrors GameLoopHarness (game_loop.rs) op-for-op over the real TS classes.

class Rig {
  constructor(terrain, mini) {
    // Copy the terrain bytes: GameMapImpl stores the array by reference and
    // setWater mutates it in place. Without a copy, scenario N would run on
    // terrain polluted by earlier scenarios' setWater calls, diverging from
    // the unpolluted snapshot pinned into the construct args.
    this.map = new GameMapImpl(
      terrain.w,
      terrain.h,
      new Uint8Array(terrain.bytes),
      terrain.numLand,
    );
    this.mini = new GameMapImpl(
      mini.w,
      mini.h,
      new Uint8Array(mini.bytes),
      mini.numLand,
    );
    this.water = new WaterManager(this.map, this.mini, false);
    this.territoryVersion = 0;
    this.ticks = 0;
    this.tileUpdates = [];
  }

  recordTileUpdate(tile) {
    const packed =
      (this.map.tileState(tile) & 0xffff) + (this.map.terrainByte(tile) << 16);
    this.tileUpdates.push(tile, packed);
  }

  // GameImpl.setFallout (TS 230-240).
  setFallout(tile, value, out) {
    if (value && this.map.hasOwner(tile)) {
      out.push(1);
      pushStr(out, `cannot set fallout, tile ${tile} has owner`);
      return;
    }
    if (this.map.hasFallout(tile)) {
      out.push(0);
      return;
    }
    this.territoryVersion++;
    this.map.setFallout(tile, value);
    this.recordTileUpdate(tile);
    out.push(0);
  }

  // GameImpl.setWater (TS 242-254).
  setWater(tile, out) {
    if (!this.map.isLand(tile)) {
      out.push(0);
      return;
    }
    if (this.map.hasOwner(tile)) {
      out.push(1);
      pushStr(out, `cannot set water, tile ${tile} has owner`);
      return;
    }
    if (this.map.hasFallout(tile)) {
      this.map.setFallout(tile, false);
    }
    this.territoryVersion++;
    this.map.setWater(tile);
    this.recordTileUpdate(tile);
    out.push(0);
  }

  // GameImpl.queueWaterConversion (TS 256-266) — waterNukes() pinned true.
  queueWaterConversion(tile, out) {
    if (!this.map.isLand(tile)) {
      out.push(0);
      return;
    }
    if (this.map.hasOwner(tile)) {
      out.push(1);
      pushStr(out, `cannot queue water conversion, tile ${tile} has owner`);
      return;
    }
    this.water.queueTile(tile);
    out.push(0);
  }

  waterTick(ticks, out) {
    this.ticks = ticks;
    const changed = this.water.tick(ticks);
    for (const t of changed) this.recordTileUpdate(t);
    out.push(0, changed.length, ...changed);
  }

  drain(out) {
    out.push(0, this.tileUpdates.length, ...this.tileUpdates);
    this.tileUpdates = [];
  }

  digest(out) {
    const m = this.map;
    const n = m.width() * m.height();
    out.push(
      0,
      this.territoryVersion,
      this.ticks,
      m.numLandTiles(),
      m.waterVersion(),
      m.numTilesWithFallout(),
      this.water.waterGraphVersion(),
    );
    for (let t = 0; t < n; t++) {
      out.push((m.tileState(t) & 0xffff) + (m.terrainByte(t) << 16));
    }
  }
}

// Construct op args: [w, h, numLand, miniW, miniH, miniNumLand, len, ..bytes, len, ..mini]
function constructArgs(main, mini) {
  return [
    main.w, main.h, main.numLand,
    mini.w, mini.h, mini.numLand,
    main.bytes.length, ...main.bytes,
    mini.bytes.length, ...mini.bytes,
  ];
}

// --------------------------------------------------------------- scenarios

const W = 48, H = 32;
const MW = 24, MH = 16;

const mainT = genTerrain(W, H);
const miniT = genTerrain(MW, MH);
const main = { w: W, h: H, bytes: mainT, numLand: numLandOf(mainT) };
const mini = { w: MW, h: MH, bytes: miniT, numLand: numLandOf(miniT) };
const P = pickTiles(mainT, W);

// op(kind, args) helpers
const op = (kind, args = []) => ({ kind, args });
const K = { CONSTRUCT: 0, SET_FALLOUT: 1, SET_WATER: 2, QUEUE_WATER: 3, TICK: 4, DRAIN: 5, DIGEST: 6 };

const scenarios = [];

// 1. Fallout: set, duplicate no-op, clear attempt no-op (already-fallout
//    early return), digest + drain.
{
  const ops = [
    op(K.CONSTRUCT, constructArgs(main, mini)),
    op(K.SET_FALLOUT, [P.landA, 1]),
    op(K.SET_FALLOUT, [P.landA, 1]),
    op(K.SET_FALLOUT, [P.landB, 1]),
    op(K.SET_FALLOUT, [P.landA, 0]),
    op(K.DIGEST),
    op(K.DRAIN),
  ];
  scenarios.push({ name: "m1_fallout_basic", ops });
}

// 2. setWater: two land conversions, water no-op, impassable no-op, flush
//    tick, digest + drain.
{
  const ops = [
    op(K.CONSTRUCT, constructArgs(main, mini)),
    op(K.SET_WATER, [P.landA]),
    op(K.SET_WATER, [P.landC]),
    op(K.SET_WATER, [P.oceanA]),
    op(K.SET_WATER, [P.impassableA]),
    op(K.TICK, [0]),
    op(K.DIGEST),
    op(K.DRAIN),
  ];
  scenarios.push({ name: "m1_set_water_lake", ops });
}

// 3. Queued conversions flushed by the water manager, then idle ticks and
//    the 20-tick throttled graph rebuild boundary.
{
  const ops = [
    op(K.CONSTRUCT, constructArgs(main, mini)),
    op(K.QUEUE_WATER, [P.landA]),
    op(K.QUEUE_WATER, [P.landB]),
    op(K.QUEUE_WATER, [P.landShore]),
    op(K.QUEUE_WATER, [P.oceanA]),
    op(K.TICK, [5]),
    op(K.TICK, [6]),
    op(K.TICK, [26]),
    op(K.DIGEST),
    op(K.DRAIN),
  ];
  scenarios.push({ name: "m1_water_queue_rebuild", ops });
}

// 4. Mixed statefulness: fallout then water over the fallout tile (clears),
//    queues crossing ticks, a later fallout, digest + drain at the end.
{
  const ops = [
    op(K.CONSTRUCT, constructArgs(main, mini)),
    op(K.SET_FALLOUT, [P.landA, 1]),
    op(K.SET_FALLOUT, [P.landB, 1]),
    op(K.SET_WATER, [P.landA]),
    op(K.QUEUE_WATER, [P.landC]),
    op(K.TICK, [10]),
    op(K.SET_FALLOUT, [P.landShore, 1]),
    op(K.QUEUE_WATER, [P.landB]),
    op(K.TICK, [11]),
    op(K.TICK, [31]),
    op(K.DIGEST),
    op(K.DRAIN),
  ];
  scenarios.push({ name: "m1_mixed_statefulness", ops });
}

// 5. Fallout + queue interleave with drain mid-scenario (drain clears, so a
//    second drain must be empty) and a final digest.
{
  const ops = [
    op(K.CONSTRUCT, constructArgs(main, mini)),
    op(K.SET_FALLOUT, [P.landA, 1]),
    op(K.QUEUE_WATER, [P.landB]),
    op(K.TICK, [2]),
    op(K.DRAIN),
    op(K.DRAIN),
    op(K.SET_FALLOUT, [P.landC, 1]),
    op(K.TICK, [40]),
    op(K.DIGEST),
    op(K.DRAIN),
  ];
  scenarios.push({ name: "m1_drain_reset", ops });
}

// -------------------------------------------------------------- TS runner

function runScenario(sc) {
  const rig = new Rig(main, mini);
  const perOpRes = [];
  for (const step of sc.ops) {
    const out = [];
    switch (step.kind) {
      case K.CONSTRUCT:
        out.push(0);
        break;
      case K.SET_FALLOUT:
        rig.setFallout(step.args[0], step.args[1] !== 0, out);
        break;
      case K.SET_WATER:
        rig.setWater(step.args[0], out);
        break;
      case K.QUEUE_WATER:
        rig.queueWaterConversion(step.args[0], out);
        break;
      case K.TICK:
        rig.waterTick(step.args[0], out);
        break;
      case K.DRAIN:
        rig.drain(out);
        break;
      case K.DIGEST:
        rig.digest(out);
        break;
      default:
        throw new Error(`unknown kind ${step.kind}`);
    }
    perOpRes.push(out);
  }
  return perOpRes;
}

const results = scenarios.map((sc) => ({
  name: sc.name,
  ops: sc.ops,
  res: runScenario(sc),
}));

// --------------------------------------------------------------- emitters

const rsStr = (s) => {
  let out = "";
  for (const ch of s) {
    if (ch === "\\") out += "\\\\";
    else if (ch === '"') out += '\\"';
    else if (ch === "\n") out += "\\n";
    else if (ch === "\r") out += "\\r";
    else if (ch === "\t") out += "\\t";
    else out += ch;
  }
  return `"${out}"`;
};

const num = (v) => {
  if (Number.isInteger(v) && Math.abs(v) < 1e15) return `${v}.0`;
  if (Object.is(v, -0)) return "-0.0";
  if (!Number.isFinite(v)) throw new Error(`non-finite ${v}`);
  // Non-integer f64: exact hex bits round-trip.
  const f = new Float64Array(1);
  f[0] = v;
  const bits = new BigUint64Array(f.buffer)[0];
  return `f64::from_bits(${bits}u64)`;
};

const nums = (arr) => `&[${arr.map(num).join(", ")}]`;

const L = [];
L.push("// BEGIN GENERATED — rewritten by tools/gen_m1_vectors.mjs, do not edit by hand.");
L.push("// M1 integration vectors: real TS GameMapImpl + WaterManager over");
L.push("// synthetic terrain; replayed by tests/parity_m1.rs and tools/m1_scan.mjs.");
L.push("pub struct M1Op {");
L.push("    pub kind: u8,");
L.push("    pub args: &'static [f64],");
L.push("}");
L.push("pub struct M1Scenario {");
L.push("    pub name: &'static str,");
L.push("    pub ops: &'static [M1Op],");
L.push("    pub res: &'static [&'static [f64]],");
L.push("}");
L.push("pub static M1_SCENARIOS: &[M1Scenario] = &[");
for (const sc of results) {
  L.push("    M1Scenario {");
  L.push(`        name: ${rsStr(sc.name)},`);
  L.push("        ops: &[");
  for (const st of sc.ops) {
    L.push(`            M1Op { kind: ${st.kind}, args: ${nums(st.args)} },`);
  }
  L.push("        ],");
  L.push("        res: &[");
  for (const r of sc.res) {
    L.push(`            ${nums(r)},`);
  }
  L.push("        ],");
  L.push("    },");
}
L.push("];");
L.push("// END GENERATED");

const dataDir = join(root, "crates", "core", "tests", "data");
mkdirSync(dataDir, { recursive: true });
writeFileSync(join(dataDir, "m1_vectors.rs"), L.join("\n") + "\n", "utf8");
writeFileSync(
  join(dataDir, "m1_vectors.json"),
  JSON.stringify({ note: "G6-0 (M1-0) game_loop integration vectors", scenarios: results }),
);

const totalOps = results.reduce((a, s) => a + s.ops.length, 0);
const totalNums = results.reduce(
  (a, s) => a + s.res.reduce((b, r) => b + r.length, 0),
  0,
);
console.log(
  `m1 vectors: ${results.length} scenarios / ${totalOps} ops / ${totalNums} expected f64s`,
);
