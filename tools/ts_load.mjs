// Loads the TS sources under test despite Node's strip-only TypeScript mode.
//
// Strip mode erases types but rejects *parameter properties*
// (`constructor(private x: T)`), which three of the ported classes use, and it
// cannot see that `import { TileRef }` is type-only, so it would try to load
// the whole GameMap module graph. Both are pure syntax problems around the
// edges of the files; the algorithm bodies load untouched.
//
// Every rewrite is asserted: if a pattern stops matching (because the TS
// source changed), this throws instead of silently benchmarking something else.
import { readFileSync, writeFileSync, mkdirSync, rmSync } from "node:fs";
import { join, dirname, basename } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { createHash } from "node:crypto";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..", "..");
const cacheDir = join(here, ".ts-load-cache");

function must(src, from, to, label) {
  if (!src.includes(from)) {
    throw new Error(`ts_load: expected pattern not found (${label}) - the TS source changed, update this shim`);
  }
  return src.split(from).join(to);
}

/** rel -> basename of the prepared copy, so dependents can import it. */
const prepared = new Map();

/** Rewrites one file and returns the path of the loadable .ts copy. */
function prepare(rel) {
  const src = readFileSync(join(root, rel), "utf8");
  let out = src;

  if (rel.endsWith("FlatBinaryHeap.ts")) {
    // Type-only import; strip mode cannot tell and would pull in GameMap.
    out = must(out, 'import { TileRef } from "../../game/GameMap";\n', "", "TileRef import");
  }
  if (rel.endsWith("PriorityQueue.ts")) {
    out = must(
      out,
      "  private size = 0;\n\n  constructor(private capacity: number) {",
      "  private size = 0;\n  private capacity: number;\n\n  constructor(capacity: number) {\n    this.capacity = capacity;",
      "MinHeap ctor",
    );
  }
  if (rel.endsWith("BFS.ts")) {
    out = must(
      out,
      "export class BFS<T> {\n  constructor(private adapter: BFSAdapter<T>) {}",
      "export class BFS<T> {\n  private adapter: BFSAdapter<T>;\n  constructor(adapter: BFSAdapter<T>) { this.adapter = adapter; }",
      "BFS ctor",
    );
  }
  if (rel.endsWith("game/GameMap.ts")) {
    // `Cell`/`TerrainType` are only referenced by cell()/terrainType(), which
    // the Rail adapter never calls; strip mode cannot know that, so the
    // import (and the whole Game.ts graph behind it) is dropped. The ctor's
    // parameter property is expanded, same as MinHeap's.
    out = must(out, 'import { Cell, TerrainType } from "./Game";\n', "", "Game import");
    out = must(
      out,
      "  constructor(\n    width: number,\n    height: number,\n    terrainData: Uint8Array,\n    private numLandTiles_: number,\n  ) {",
      "  private numLandTiles_: number;\n\n  constructor(\n    width: number,\n    height: number,\n    terrainData: Uint8Array,\n    numLandTiles_: number,\n  ) {\n    this.numLandTiles_ = numLandTiles_;",
      "GameMap ctor",
    );
  }
  if (rel.endsWith("algorithms/AStar.Rail.ts")) {
    // PathFinder (interface) and GameMap (interface, type-only here) imports
    // are erased; DebugSpan and AStar are values redirected to prepared
    // copies. AStarAdapter is an interface -> dropped from the named import.
    out = must(out, 'import { PathFinder } from "../types";\n', "", "Rail types import");
    out = must(out, 'import { GameMap } from "../../game/GameMap";\n', "", "Rail GameMap import");
    const dbgRel = "src/core/utilities/DebugSpan.ts";
    if (!prepared.has(dbgRel)) prepare(dbgRel);
    const asRel = "src/core/pathfinding/algorithms/AStar.ts";
    if (!prepared.has(asRel)) prepare(asRel);
    out = must(
      out,
      'import { DebugSpan } from "../../utilities/DebugSpan";',
      `import { DebugSpan } from "./${prepared.get(dbgRel)}";`,
      "Rail DebugSpan import",
    );
    out = must(
      out,
      "import { AStar, AStarAdapter } from \"./AStar\";",
      `import { AStar } from "./${prepared.get(asRel)}";`,
      "Rail AStar import",
    );
  }
  if (rel.endsWith("algorithms/AStar.ts")) {
    // `PathFinder` is an interface (erased at runtime) and the extensionless
    // "../types" specifier is not loadable; the PriorityQueue value import is
    // redirected to its prepared copy (same cache dir).
    out = must(out, 'import { PathFinder } from "../types";\n', "", "AStar types import");
    const pqRel = "src/core/pathfinding/algorithms/PriorityQueue.ts";
    if (!prepared.has(pqRel)) prepare(pqRel);
    out = must(
      out,
      'import { BucketQueue, PriorityQueue } from "./PriorityQueue";',
      `import { BucketQueue } from "./${prepared.get(pqRel)}";`,
      "AStar PQ import",
    );
  }

  mkdirSync(cacheDir, { recursive: true });
  const hash = createHash("sha1").update(out).digest("hex").slice(0, 10);
  const base = `${basename(rel, ".ts")}-${hash}.ts`;
  const outPath = join(cacheDir, base);
  writeFileSync(outPath, out, "utf8");
  prepared.set(rel, base);
  return outPath;
}

/** Import a ported TS module by its repo-relative path. */
export async function loadTs(rel) {
  const mod = await import(pathToFileURL(prepare(rel)).href);
  return mod;
}

/** Remove the rewrite cache (rewrite files are disposable). */
export function clearTsLoadCache() {
  rmSync(cacheDir, { recursive: true, force: true });
}
