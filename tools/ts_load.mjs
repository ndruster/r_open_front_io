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
    // The import (and the whole Game.ts graph behind it) is dropped; the
    // numeric enum is inlined so terrainType() still works. `Cell` is only
    // referenced by cell(), which the GameMap scenarios never call. The
    // ctor's parameter property is expanded, same as MinHeap's.
    out = must(
      out,
      'import { Cell, TerrainType } from "./Game";\n',
      "const TerrainType = { Plains: 0, Highland: 1, Mountain: 2, Ocean: 3, Impassable: 4 };\n",
      "Game import",
    );
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
  if (rel.endsWith("game/TileSet.ts")) {
    // TileRef is a type-only import; strip mode cannot tell and would pull
    // in the whole GameMap graph.
    out = must(out, 'import { TileRef } from "./GameMap";\n', "", "TileSet import");
  }
  if (rel.endsWith("core/Util.ts")) {
    // The UI/record surface (DOMPurify, nanoid, zod Schemas, Game.ts,
    // TribeNames) is only referenced from functions the parity capture never
    // calls, so those imports are dropped outright. `exp` is redirected to
    // the real DetMath module (sigmoid calls it), and `Cell` / `GameType` /
    // `PlayerType` are replaced by inert stubs with the same shape —
    // calculateBoundingBox only ever reads `.x` / `.y` off a Cell.
    out = must(out, 'import DOMPurify from "dompurify";\n', "", "Util DOMPurify import");
    out = must(out, 'import { customAlphabet } from "nanoid";\n', "", "Util nanoid import");
    out = must(
      out,
      'import { exp } from "./DetMath";\n',
      'import { exp } from "../../../src/core/DetMath.ts";\n',
      "Util DetMath import",
    );
    out = must(
      out,
      'import { Cell, GameType, PlayerType, Unit } from "./game/Game";\n',
      "const GameType = { Singleplayer: 4 };\n" +
        'const PlayerType = { Human: "player" };\n' +
        "class Cell {\n" +
        "  constructor(x, y) {\n" +
        "    this.x = x;\n" +
        "    this.y = y;\n" +
        "  }\n" +
        "}\n",
      "Util Game import",
    );
    out = must(
      out,
      'import { GameMap, TileRef } from "./game/GameMap";\n',
      "",
      "Util GameMap import",
    );
    // TileSet is a *value* use (`borderTiles instanceof TileSet`), so it
    // must resolve — redirect to the prepared copy.
    const tileSetRel = "src/core/game/TileSet.ts";
    if (!prepared.has(tileSetRel)) prepare(tileSetRel);
    out = must(
      out,
      'import { TileSet } from "./game/TileSet";\n',
      `import { TileSet } from "./${prepared.get(tileSetRel)}";\n`,
      "Util TileSet import",
    );
    out = must(
      out,
      'import {\n  GameConfig,\n  GameID,\n  GameRecord,\n  GameStartInfo,\n  PartialGameRecord,\n  PlayerRecord,\n  PlayerReport,\n  Tribe,\n  Turn,\n  Winner,\n} from "./Schemas";\n',
      "",
      "Util Schemas import",
    );
    out = must(
      out,
      'import { resolveTribeNameData } from "./execution/utils/TribeNames";\n',
      "",
      "Util TribeNames import",
    );
  }
  if (rel.endsWith("algorithms/AStar.Water.ts")) {
    // GameMap/TileRef/PathFinder are all type-only (interfaces / type
    // aliases); the MinHeap value import is redirected to the prepared
    // PriorityQueue copy (PriorityQueue itself is an interface -> dropped).
    out = must(out, 'import { GameMap, TileRef } from "../../game/GameMap";\n', "", "Water GameMap import");
    out = must(out, 'import { PathFinder } from "../types";\n', "", "Water types import");
    const pqRel = "src/core/pathfinding/algorithms/PriorityQueue.ts";
    if (!prepared.has(pqRel)) prepare(pqRel);
    out = must(
      out,
      'import { MinHeap, PriorityQueue } from "./PriorityQueue";',
      `import { MinHeap } from "./${prepared.get(pqRel)}";`,
      "Water PQ import",
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
