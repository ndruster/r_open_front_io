// G6-0 (M1-0) — method-level coverage inventory for the M1 milestone.
//
//   node tools/m1_coverage.mjs [--out <path>]
//
// Diffs the TS method tables (GameImpl.ts / PlayerImpl.ts) against the Rust
// harness coverage:
//
//   * game_impl.rs — the G4a mid table + map pass-throughs + the G4b/G4c
//     additions prose (parse window: `## Mid table` .. `## Facade trace
//     event codes`, plus every `## G4x additions` section)
//   * player_impl.rs — the mid -> name dispatch arms (`N => "name"`)
//
// Emits a markdown checklist (method | TS line | Rust coverage) and a gap
// summary. Default output: <scratch>/m1_coverage_report.md (or --out).

import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const TS_ROOT = process.env.OPENFRONT_TS_ROOT ?? join(root, "..", "OpenFrontIO");

const outIdx = process.argv.indexOf("--out");
const outPath =
  outIdx >= 0 && process.argv[outIdx + 1]
    ? process.argv[outIdx + 1]
    : null;

// ------------------------------------------------------------- TS parsing

function tsMethods(relPath) {
  const src = readFileSync(join(TS_ROOT, relPath), "utf8");
  const lines = src.split(/\r?\n/);
  const methods = new Map(); // name -> first line
  const re =
    /^  (?:public |private |protected |readonly )*(?:async )?([a-zA-Z_][a-zA-Z0-9_]*)\s*[(<]/;
  for (let i = 0; i < lines.length; i++) {
    const m = lines[i].match(re);
    if (!m) continue;
    const name = m[1];
    if (name === "constructor") continue;
    if (!methods.has(name)) methods.set(name, i + 1);
  }
  return methods;
}

const gameImplTs = tsMethods("src/core/game/GameImpl.ts");
const playerImplTs = tsMethods("src/core/game/PlayerImpl.ts");

// ----------------------------------------------------------- Rust parsing

function readRel(relPath) {
  return readFileSync(join(root, "crates", "core", "src", relPath), "utf8");
}

function gameImplCovered() {
  const src = readRel("game_impl.rs");
  const lines = src.split(/\r?\n/);
  const covered = new Map(); // method -> mid label
  let zone = null; // "mid" | "additions" | null
  for (const line of lines) {
    if (!line.startsWith("//!")) {
      zone = null;
      continue;
    }
    if (line.includes("## Mid table")) {
      zone = "mid";
      continue;
    }
    if (line.includes("## Facade trace event codes")) {
      zone = null;
      continue;
    }
    if (/^\/\/! ## G4[a-z] additions/.test(line)) {
      zone = "additions";
      continue;
    }
    if (line.includes("## ") && zone === "additions") {
      zone = null;
      continue;
    }
    if (zone === null) continue;
    // Table rows: | 12 | `unit` | ... — pass-through rows are prose. The
    // mid table is two columns wide (mid/method twice per line), so scan
    // every `| N | \`name\`` pair, not just the first.
    for (const tableRow of line.matchAll(/\|\s*(\d+)\s*\|\s*`([A-Za-z_][A-Za-z0-9_]*)`/g)) {
      covered.set(tableRow[2], `game mid ${tableRow[1]}`);
    }
    if (line.match(/\|\s*(\d+)\s*\|\s*`/)) continue;
    // Prose: "300 `executeNextTick` (488-546)" / "72 `ref`, 73 `isValidRef`"
    const prose = line.matchAll(/(\d+)\s+`([A-Za-z_][A-Za-z0-9_]*)`/g);
    for (const m of prose) {
      if (!covered.has(m[2])) covered.set(m[2], `game mid ${m[1]}`);
    }
    // Ranged prose: "72..114 | `_map` pass-throughs" style headers name no
    // methods; ranges like "| 115..123 | stats / rail / water |" likewise.
  }
  return covered;
}

// TS methods that ride on another method's mid arm in player_impl.rs
// (toFullUpdate is materialized by the same mid-115 body as toUpdate —
// G3c4's `to_full_update_parts` drives both snapshots).
const PLAYER_ALIASES = {
  toFullUpdate: "toUpdate",
};

function playerImplCovered() {
  const src = readRel("player_impl.rs");
  const covered = new Map();
  const arms = src.matchAll(/(\d+)\s*=>\s*"([A-Za-z_][A-Za-z0-9_]*)"/g);
  for (const m of arms) {
    if (!covered.has(m[2])) covered.set(m[2], `player mid ${m[1]}`);
  }
  for (const [alias, target] of Object.entries(PLAYER_ALIASES)) {
    if (covered.has(target) && !covered.has(alias)) {
      covered.set(alias, covered.get(target));
    }
  }
  return covered;
}

const gameCov = gameImplCovered();
const playerCov = playerImplCovered();

// ------------------------------------------------------------- gap report

function report(title, tsMap, cov, integrationNote) {
  const rows = [];
  let missing = 0;
  for (const [name, line] of tsMap) {
    const c = cov.get(name);
    if (!c) missing++;
    rows.push(`| \`${name}\` | ${line} | ${c ?? "**MISSING**"} |`);
  }
  const md = [
    `## ${title}`,
    "",
    `TS methods: ${tsMap.size}; covered by Rust harness: ${tsMap.size - missing}; missing: ${missing}`,
    "",
    integrationNote,
    "",
    "| method | TS line | Rust coverage |",
    "|--------|---------|---------------|",
    ...rows,
    "",
  ].join("\n");
  return { md, missing, total: tsMap.size };
}

const gameNote = [
  "> Integration coverage (M1): the G4a/G4b harness runs these methods over",
  "> scripted facades. `game_loop` re-assembles them over REAL components —",
  "> see the module doc for the M1-0 pin list. Missing methods here are the",
  "> M1e/M1f method-level backlog.",
].join("\n");

const playerNote = [
  "> G3c1..G3c4 cover the player harness surface; M1a wires REAL PlayerImpl",
  "> instances into `game_loop`, at which point the covered set must survive",
  "> integration vectors as well as traces.",
].join("\n");

const gameR = report("GameImpl — method-level coverage", gameImplTs, gameCov, gameNote);
const playerR = report("PlayerImpl — method-level coverage", playerImplTs, playerCov, playerNote);

const md = [
  "# M1 coverage inventory (G6-0 tool output)",
  "",
  `TS_ROOT: ${TS_ROOT}`,
  "",
  gameR.md,
  playerR.md,
].join("\n");

if (outPath) {
  mkdirSync(dirname(outPath), { recursive: true });
  writeFileSync(outPath, md, "utf8");
  console.log(`written: ${outPath}`);
}
console.log(
  `GameImpl: ${gameR.total - gameR.missing}/${gameR.total} covered, ${gameR.missing} missing`,
);
console.log(
  `PlayerImpl: ${playerR.total - playerR.missing}/${playerR.total} covered, ${playerR.missing} missing`,
);
const gameMissing = [...gameImplTs].filter(([n]) => !gameCov.has(n)).map(([n]) => n);
const playerMissing = [...playerImplTs].filter(([n]) => !playerCov.has(n)).map(([n]) => n);
if (gameMissing.length) console.log(`GameImpl missing: ${gameMissing.join(", ")}`);
if (playerMissing.length) console.log(`PlayerImpl missing: ${playerMissing.join(", ")}`);
