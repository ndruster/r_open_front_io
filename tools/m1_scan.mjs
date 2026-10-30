// G6-0 (M1-0) — wasm parity scan for the `game_loop` integration vectors.
//
//   cargo build -p openfront-core --target wasm32-unknown-unknown \
//       --release --features wasm-probe
//   node tools/m1_scan.mjs
//
// Replays every m1_vectors.json scenario through the probe_gl_* exports and
// compares every f64 bit-for-bit against the TS-recorded expectation.

import { readFileSync } from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const wasmPath = join(
  root, "target", "wasm32-unknown-unknown", "release", "openfront_core.wasm",
);

const data = JSON.parse(
  readFileSync(join(root, "crates", "core", "tests", "data", "m1_vectors.json"), "utf8"),
);

const { instance } = await WebAssembly.instantiate(
  await readFileSync(wasmPath),
  { env: { abort: () => { throw new Error("rust panic (wasm abi mismatch)"); } } },
);
const ex = instance.exports;
for (const name of ["probe_gl_reset", "probe_gl_arg", "probe_gl_op", "probe_gl_out_at"]) {
  if (typeof ex[name] !== "function") {
    throw new Error(`wasm probe missing export ${name}`);
  }
}

const f64Bits = (v) => {
  const f = new Float64Array(1);
  f[0] = v;
  return new BigUint64Array(f.buffer)[0];
};

let scenarios = 0;
let ops = 0;
let compared = 0;
const failures = [];

for (const sc of data.scenarios) {
  scenarios++;
  ex.probe_gl_reset();
  if (sc.ops.length !== sc.res.length) {
    failures.push(`${sc.name}: op/res count mismatch`);
    continue;
  }
  for (let i = 0; i < sc.ops.length; i++) {
    ops++;
    const step = sc.ops[i];
    const want = sc.res[i];
    for (const a of step.args) ex.probe_gl_arg(a);
    const len = ex.probe_gl_op(step.kind);
    if (len !== want.length) {
      failures.push(
        `${sc.name} op #${i} (kind ${step.kind}): length ${len} vs TS ${want.length}`,
      );
      continue;
    }
    for (let k = 0; k < len; k++) {
      compared++;
      const g = ex.probe_gl_out_at(k);
      if (f64Bits(g) !== f64Bits(want[k])) {
        failures.push(
          `${sc.name} op #${i} (kind ${step.kind}) token ${k}: got ${g} ` +
            `(bits ${f64Bits(g)}), TS ${want[k]} (bits ${f64Bits(want[k])})`,
        );
      }
    }
  }
}

if (failures.length > 0) {
  console.error(`m1_scan: ${failures.length} FAILURES`);
  for (const f of failures.slice(0, 50)) console.error(`  - ${f}`);
  process.exit(1);
}
console.log(
  `m1_scan: ALL CLEAN — ${scenarios} scenarios / ${ops} ops / ${compared} f64 comparisons bit-exact`,
);
