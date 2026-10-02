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
const root = join(here, "..");
// This repo holds only the Rust port; the authoritative TypeScript sources
// live in the upstream OpenFrontIO checkout. Override with OPENFRONT_TS_ROOT
// when the checkout sits elsewhere (the sync workflow points it at a
// worktree of the commit recorded in UPSTREAM_COMMIT).
export const TS_ROOT = process.env.OPENFRONT_TS_ROOT
  ? join(process.env.OPENFRONT_TS_ROOT)
  : join(root, "..", "OpenFrontIO");
const TS_URL = pathToFileURL(TS_ROOT).href.replace(/\/?$/, "/");
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
  const src = readFileSync(join(TS_ROOT, rel), "utf8");
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
  if (rel.endsWith("PathFinder.Air.ts")) {
    // Game / TileRef / PathFinder are type-only (interface + branded type);
    // strip mode would still execute their imports and pull in the whole
    // Game.ts graph. PseudoRandom is a *value* use (`new PseudoRandom`), so it
    // resolves to the real module. The ctor's `private game` parameter
    // property is expanded, and the `implements` clause is dropped.
    out = must(
      out,
      'import { Game } from "../game/Game";\n' +
        'import { TileRef } from "../game/GameMap";\n' +
        'import { PseudoRandom } from "../PseudoRandom";\n' +
        'import { PathFinder } from "./types";\n',
      `import { PseudoRandom } from "${TS_URL}src/core/PseudoRandom.ts";\n`,
      "Air imports",
    );
    out = must(
      out,
      "export class AirPathFinder implements PathFinder<TileRef> {\n" +
        "  private seed: number;\n\n" +
        "  constructor(private game: Game) {\n" +
        "    this.seed = game.ticks();\n" +
        "  }",
      "export class AirPathFinder {\n" +
        "  private seed: number;\n" +
        "  private game: any;\n\n" +
        "  constructor(game: any) {\n" +
        "    this.game = game;\n" +
        "    this.seed = game.ticks();\n" +
        "  }",
      "Air ctor",
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
  if (rel.endsWith("game/MotionPlans.ts")) {
    // Same type-only TileRef import as TileSet; the enum and the two pack /
    // unpack functions carry no other runtime dependency.
    out = must(out, 'import { TileRef } from "./GameMap";\n', "", "MotionPlans import");
    // Node's strip-only TS loader rejects `export enum`; inline the numeric
    // enum as a frozen object so the switch comparisons still resolve.
    out = must(
      out,
      "export enum PackedMotionPlanKind {\n  GridPathSet = 1,\n  TrainRailPathSet = 2,\n}",
      "export const PackedMotionPlanKind = { GridPathSet: 1, TrainRailPathSet: 2 };",
      "MotionPlans enum",
    );
  }
  if (rel.endsWith("game/Veterancy.ts")) {
    // No imports at all; nothing to rewrite.
  }
  if (rel.endsWith("game/TerrainSearchMap.ts")) {
    // No imports; the only strip-mode problem is `export enum`, inlined as a
    // plain object with the same numeric values (Land=0, Shore=1, Water=2).
    out = must(
      out,
      "export enum SearchMapTileType {\n  Land,\n  Shore,\n  Water,\n}",
      "export const SearchMapTileType = { Land: 0, Shore: 1, Water: 2 };",
      "TerrainSearchMap enum",
    );
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
      `import { exp } from "${TS_URL}src/core/DetMath.ts";\n`,
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
  if (rel.endsWith("utilities/Line.ts")) {
    // The ctor's four control points are parameter properties, which strip
    // mode rejects; expand them to plain fields + assignments (same rewrite
    // MinHeap's ctor gets).
    out = must(
      out,
      "  constructor(\n" +
        "    private p0: Point,\n" +
        "    private p1: Point,\n" +
        "    private p2: Point,\n" +
        "    private p3: Point,\n" +
        "    distanceIncrement: number,\n" +
        "  ) {",
      "  private p0: Point;\n" +
        "  private p1: Point;\n" +
        "  private p2: Point;\n" +
        "  private p3: Point;\n\n" +
        "  constructor(\n" +
        "    p0: Point,\n" +
        "    p1: Point,\n" +
        "    p2: Point,\n" +
        "    p3: Point,\n" +
        "    distanceIncrement: number,\n" +
        "  ) {\n" +
        "    this.p0 = p0;\n" +
        "    this.p1 = p1;\n" +
        "    this.p2 = p2;\n" +
        "    this.p3 = p3;",
      "Line ctor",
    );
  }
  if (rel.endsWith("game/TeamAssignment.ts")) {
    // ClientID / TeamCountConfig / PlayerInfo / Team are type-only (branded
    // types + interfaces) -> dropped. PseudoRandom and simpleHash are *value*
    // uses (the nation shuffle), so they resolve: PseudoRandom has no imports
    // and loads directly; simpleHash redirects to the prepared Util copy. The
    // Game import is a mix of runtime constants (ColoredTeams, the
    // TeamCountConfig string literals, PlayerType) and types (PlayerInfo,
    // Team) — replace it with inert stubs carrying the same values.
    out = must(
      out,
      'import { PseudoRandom } from "../PseudoRandom";\n',
      `import { PseudoRandom } from "${TS_URL}src/core/PseudoRandom.ts";\n`,
      "Team PseudoRandom import",
    );
    out = must(
      out,
      'import { ClientID, TeamCountConfig } from "../Schemas";\n',
      "",
      "Team Schemas import",
    );
    const utilRel = "src/core/Util.ts";
    if (!prepared.has(utilRel)) prepare(utilRel);
    out = must(
      out,
      'import { simpleHash } from "../Util";\n',
      `import { simpleHash } from "./${prepared.get(utilRel)}";\n`,
      "Team Util import",
    );
    out = must(
      out,
      "import {\n" +
        "  ColoredTeams,\n" +
        "  Duos,\n" +
        "  HumansVsNations,\n" +
        "  PlayerInfo,\n" +
        "  PlayerType,\n" +
        "  Quads,\n" +
        "  Team,\n" +
        "  Trios,\n" +
        '} from "./Game";\n',
      "const ColoredTeams = { Red: \"Red\", Blue: \"Blue\", Teal: \"Teal\", " +
        "Purple: \"Purple\", Yellow: \"Yellow\", Orange: \"Orange\", Green: \"Green\", " +
        "Bot: \"Bot\", Humans: \"Humans\", Nations: \"Nations\" };\n" +
        "const Duos = \"Duos\";\n" +
        "const Trios = \"Trios\";\n" +
        "const Quads = \"Quads\";\n" +
        "const HumansVsNations = \"Humans Vs Nations\";\n" +
        "const PlayerType = { Bot: \"BOT\", Human: \"HUMAN\", Nation: \"NATION\" };\n",
      "Team Game import",
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
  if (rel.endsWith("algorithms/AStar.WaterBounded.ts")) {
    // Same shape as AStar.Water.ts: GameMap/TileRef/PathFinder are type-only
    // (the `implements PathFinder<number>` clause is erased by strip mode),
    // the two exported interfaces are type declarations (erased), and the
    // MinHeap value import is redirected to the prepared PriorityQueue copy.
    // The constructor uses plain params + field assignments, no parameter
    // properties.
    out = must(out, 'import { GameMap, TileRef } from "../../game/GameMap";\n', "", "WaterBounded GameMap import");
    out = must(out, 'import { PathFinder } from "../types";\n', "", "WaterBounded types import");
    const pqRel = "src/core/pathfinding/algorithms/PriorityQueue.ts";
    if (!prepared.has(pqRel)) prepare(pqRel);
    out = must(
      out,
      'import { MinHeap } from "./PriorityQueue";',
      `import { MinHeap } from "./${prepared.get(pqRel)}";`,
      "WaterBounded PQ import",
    );
  }
  if (rel.endsWith("algorithms/AStar.WaterHierarchical.ts")) {
    // Orchestrator class: GameMap/TileRef/PathFinder/AbstractGraph/AbstractNode
    // are type-only (the graph and map are passed in, never news-ed here) ->
    // dropped. DebugSpan / BFSGrid / AbstractGraphAStar / AStarWaterBounded are
    // value imports redirected to prepared copies (all cached flat). Both ctors
    // use parameter properties (strip-mode rejects them) -> expanded.
    out = must(out, 'import { GameMap, TileRef } from "../../game/GameMap";\n', "", "WH GameMap import");
    out = must(out, 'import { PathFinder } from "../types";\n', "", "WH types import");
    out = must(out, 'import { AbstractGraph, AbstractNode } from "./AbstractGraph";\n', "", "WH AbstractGraph import");
    const dbgRel = "src/core/utilities/DebugSpan.ts";
    if (!prepared.has(dbgRel)) prepare(dbgRel);
    out = must(
      out,
      'import { DebugSpan } from "../../utilities/DebugSpan";',
      `import { DebugSpan } from "./${prepared.get(dbgRel)}";`,
      "WH DebugSpan import",
    );
    const bfsRel = "src/core/pathfinding/algorithms/BFS.Grid.ts";
    if (!prepared.has(bfsRel)) prepare(bfsRel);
    out = must(
      out,
      'import { BFSGrid } from "./BFS.Grid";',
      `import { BFSGrid } from "./${prepared.get(bfsRel)}";`,
      "WH BFSGrid import",
    );
    const agaRel = "src/core/pathfinding/algorithms/AStar.AbstractGraph.ts";
    if (!prepared.has(agaRel)) prepare(agaRel);
    out = must(
      out,
      'import { AbstractGraphAStar } from "./AStar.AbstractGraph";',
      `import { AbstractGraphAStar } from "./${prepared.get(agaRel)}";`,
      "WH AGA import",
    );
    const wbRel = "src/core/pathfinding/algorithms/AStar.WaterBounded.ts";
    if (!prepared.has(wbRel)) prepare(wbRel);
    out = must(
      out,
      'import { AStarWaterBounded } from "./AStar.WaterBounded";',
      `import { AStarWaterBounded } from "./${prepared.get(wbRel)}";`,
      "WH WB import",
    );
    out = must(
      out,
      "  constructor(\n" +
        "    private map: GameMap,\n" +
        "    private graph: AbstractGraph,\n" +
        "    private options: {\n" +
        "      cachePaths?: boolean;\n" +
        "    } = {},\n" +
        "  ) {",
      "  private map: GameMap;\n" +
        "  private graph: AbstractGraph;\n" +
        "  private options: {\n" +
        "    cachePaths?: boolean;\n" +
        "  };\n\n" +
        "  constructor(\n" +
        "    map: GameMap,\n" +
        "    graph: AbstractGraph,\n" +
        "    options: {\n" +
        "      cachePaths?: boolean;\n" +
        "    } = {},\n" +
        "  ) {\n" +
        "    this.map = map;\n" +
        "    this.graph = graph;\n" +
        "    this.options = options;",
      "WH ctor",
    );
    out = must(
      out,
      "  constructor(\n" +
        "    private map: GameMap,\n" +
        "    private graph: AbstractGraph,\n" +
        "  ) {}",
      "  private map: GameMap;\n" +
        "  private graph: AbstractGraph;\n\n" +
        "  constructor(\n" +
        "    map: GameMap,\n" +
        "    graph: AbstractGraph,\n" +
        "  ) {\n" +
        "    this.map = map;\n" +
        "    this.graph = graph;\n" +
        "  }",
      "SourceResolver ctor",
    );
  }
  if (rel.endsWith("algorithms/AStar.AbstractGraph.ts")) {
    // PathFinder (interface) and AbstractGraph (used only as a *type* here —
    // the class never news it) are type-only -> dropped. MinHeap is the only
    // value import; PriorityQueue is an interface -> dropped from the named
    // import, the specifier redirected to the prepared copy.
    out = must(out, 'import { PathFinder } from "../types";\n', "", "AGA types import");
    out = must(out, 'import { AbstractGraph } from "./AbstractGraph";\n', "", "AGA graph import");
    const pqRel = "src/core/pathfinding/algorithms/PriorityQueue.ts";
    if (!prepared.has(pqRel)) prepare(pqRel);
    out = must(
      out,
      'import { MinHeap, PriorityQueue } from "./PriorityQueue";',
      `import { MinHeap } from "./${prepared.get(pqRel)}";`,
      "AGA PQ import",
    );
  }
  if (rel.endsWith("execution/Util.ts")) {
    // Only the pure-GameMap functions are exercised. GameView / Game /
    // Player / Structures / NukeMagnitude are type-only or used exclusively
    // by the excluded Game-facade functions -> dropped. ReadonlyTileSet is an
    // interface -> dropped. euclDistFN is a *value* import and GameMap is a
    // type-only import in this file, so the pair is split: GameMap erased,
    // euclDistFN redirected to the real (prepared) GameMap.ts module.
    out = must(
      out,
      'import { GameView } from "../../client/view";\n',
      "",
      "EU view import",
    );
    out = must(
      out,
      'import { NukeMagnitude } from "../configuration/Config";\n',
      "",
      "EU Config import",
    );
    out = must(
      out,
      'import { Game, Player, Structures } from "../game/Game";\n',
      "",
      "EU Game import",
    );
    const gmRel = "src/core/game/GameMap.ts";
    if (!prepared.has(gmRel)) prepare(gmRel);
    out = must(
      out,
      'import { euclDistFN, GameMap, TileRef } from "../game/GameMap";\n',
      `import { euclDistFN } from "./${prepared.get(gmRel)}";\n`,
      "EU GameMap import",
    );
    out = must(
      out,
      'import { ReadonlyTileSet } from "../game/TileSet";\n',
      "",
      "EU TileSet import",
    );
  }
  if (rel.endsWith("algorithms/ConnectedComponents.ts")) {
    // GameMap/TileRef are type-only (interface + branded type) -> dropped; the
    // DebugSpan value import is redirected to its prepared copy. The ctor's two
    // parameter properties are expanded (strip mode rejects them).
    out = must(out, 'import { GameMap, TileRef } from "../../game/GameMap";\n', "", "CC GameMap import");
    const dbgRel = "src/core/utilities/DebugSpan.ts";
    if (!prepared.has(dbgRel)) prepare(dbgRel);
    out = must(
      out,
      'import { DebugSpan } from "../../utilities/DebugSpan";',
      `import { DebugSpan } from "./${prepared.get(dbgRel)}";`,
      "CC DebugSpan import",
    );
    out = must(
      out,
      "  constructor(\n" +
        "    private readonly map: GameMap,\n" +
        "    private readonly accessTerrainDirectly: boolean = true,\n" +
        "  ) {\n" +
        "    this.width = map.width();",
      "  private readonly map: GameMap;\n" +
        "  private readonly accessTerrainDirectly: boolean;\n\n" +
        "  constructor(\n" +
        "    map: GameMap,\n" +
        "    accessTerrainDirectly: boolean = true,\n" +
        "  ) {\n" +
        "    this.map = map;\n" +
        "    this.accessTerrainDirectly = accessTerrainDirectly;\n" +
        "    this.width = map.width();",
      "CC ctor",
    );
  }
  if (rel.endsWith("algorithms/AbstractGraph.ts")) {
    // GameMap/TileRef and the BFSGrid/ConnectedComponents imports are
    // type-vs-value mixed: BFSGrid and ConnectedComponents are *value* uses
    // (the builder news them), redirected to their prepared copies; DebugSpan
    // is a value use, redirected too. The two interfaces (AbstractNode, etc.)
    // and TileRef are type-only. The AbstractGraph ctor's three parameter
    // properties and the builder ctor's six are expanded (strip mode rejects
    // them).
    out = must(out, 'import { GameMap, TileRef } from "../../game/GameMap";\n', "", "AG GameMap import");
    const dbgRel = "src/core/utilities/DebugSpan.ts";
    if (!prepared.has(dbgRel)) prepare(dbgRel);
    const bfsRel = "src/core/pathfinding/algorithms/BFS.Grid.ts";
    if (!prepared.has(bfsRel)) prepare(bfsRel);
    const ccRel = "src/core/pathfinding/algorithms/ConnectedComponents.ts";
    if (!prepared.has(ccRel)) prepare(ccRel);
    out = must(
      out,
      'import { DebugSpan } from "../../utilities/DebugSpan";',
      `import { DebugSpan } from "./${prepared.get(dbgRel)}";`,
      "AG DebugSpan import",
    );
    out = must(
      out,
      'import { BFSGrid } from "./BFS.Grid";',
      `import { BFSGrid } from "./${prepared.get(bfsRel)}";`,
      "AG BFSGrid import",
    );
    out = must(
      out,
      'import { ConnectedComponents } from "./ConnectedComponents";',
      `import { ConnectedComponents } from "./${prepared.get(ccRel)}";`,
      "AG ConnectedComponents import",
    );
    out = must(
      out,
      "  constructor(\n" +
        "    readonly clusterSize: number,\n" +
        "    readonly clustersX: number,\n" +
        "    readonly clustersY: number,\n" +
        "  ) {}",
      "  readonly clusterSize: number;\n" +
        "  readonly clustersX: number;\n" +
        "  readonly clustersY: number;\n\n" +
        "  constructor(\n" +
        "    clusterSize: number,\n" +
        "    clustersX: number,\n" +
        "    clustersY: number,\n" +
        "  ) {\n" +
        "    this.clusterSize = clusterSize;\n" +
        "    this.clustersX = clustersX;\n" +
        "    this.clustersY = clustersY;\n" +
        "  }",
      "AG graph ctor",
    );
    out = must(
      out,
      "  constructor(\n" +
        "    private readonly map: GameMap,\n" +
        "    private readonly clusterSize: number = AbstractGraphBuilder.CLUSTER_SIZE,\n" +
        "    private readonly oldGraph?: AbstractGraph,\n" +
        "    private readonly dirtyMiniTiles?: Set<TileRef>,\n" +
        "    // An already-initialized ConnectedComponents kept up to date by the\n" +
        "    // caller (see WaterManager). Skips the full-map flood fill per build.\n" +
        "    private readonly sharedWaterComponents?: ConnectedComponents,\n" +
        "    // Reusable map-sized BFS scratch (stateless between searches).  Avoids\n" +
        "    // reallocating ~20MB of typed arrays on every water-graph rebuild.\n" +
        "    sharedTileBFS?: BFSGrid,\n" +
        "  ) {\n" +
        "    this.width = map.width();",
      "  private readonly map: GameMap;\n" +
        "  private readonly clusterSize: number;\n" +
        "  private readonly oldGraph?: AbstractGraph;\n" +
        "  private readonly dirtyMiniTiles?: Set<TileRef>;\n" +
        "  private readonly sharedWaterComponents?: ConnectedComponents;\n\n" +
        "  constructor(\n" +
        "    map: GameMap,\n" +
        "    clusterSize: number = AbstractGraphBuilder.CLUSTER_SIZE,\n" +
        "    oldGraph?: AbstractGraph,\n" +
        "    dirtyMiniTiles?: Set<TileRef>,\n" +
        "    sharedWaterComponents?: ConnectedComponents,\n" +
        "    sharedTileBFS?: BFSGrid,\n" +
        "  ) {\n" +
        "    this.map = map;\n" +
        "    this.clusterSize = clusterSize;\n" +
        "    this.oldGraph = oldGraph;\n" +
        "    this.dirtyMiniTiles = dirtyMiniTiles;\n" +
        "    this.sharedWaterComponents = sharedWaterComponents;\n" +
        "    this.width = map.width();",
      "AG builder ctor",
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
  if (rel.endsWith("pathfinding/PathFinder.Parabola.ts")) {
    // GameMap/TileRef are type-only (interface + branded type) -> dropped.
    // `within` (Util) and `DistanceBasedBezierCurve` (Line) are *value* uses,
    // redirected to their prepared copies. The `./types` import mixes the
    // PathStatus enum (a runtime value) with PathResult / SteppingPathFinder
    // (types, erased); Node's strip loader rejects `export enum` in the real
    // types.ts and the extensionless specifier is unloadable, so inline the
    // numeric enum as a plain object and drop the type names. The ctor's two
    // parameter properties are expanded.
    out = must(out, 'import { GameMap, TileRef } from "../game/GameMap";\n', "", "Parabola GameMap import");
    const utilRel = "src/core/Util.ts";
    if (!prepared.has(utilRel)) prepare(utilRel);
    out = must(
      out,
      'import { within } from "../Util";',
      `import { within } from "./${prepared.get(utilRel)}";`,
      "Parabola Util import",
    );
    const lineRel = "src/core/utilities/Line.ts";
    if (!prepared.has(lineRel)) prepare(lineRel);
    out = must(
      out,
      'import { DistanceBasedBezierCurve } from "../utilities/Line";',
      `import { DistanceBasedBezierCurve } from "./${prepared.get(lineRel)}";`,
      "Parabola Line import",
    );
    out = must(
      out,
      'import { PathResult, PathStatus, SteppingPathFinder } from "./types";\n',
      "const PathStatus = { NEXT: 0, COMPLETE: 2, NOT_FOUND: 3 };\n",
      "Parabola types import",
    );
    out = must(
      out,
      "  constructor(\n" +
        "    private gameMap: GameMap,\n" +
        "    private options?: ParabolaOptions,\n" +
        "  ) {}",
      "  private gameMap: GameMap;\n" +
        "  private options?: ParabolaOptions;\n\n" +
        "  constructor(\n" +
        "    gameMap: GameMap,\n" +
        "    options?: ParabolaOptions,\n" +
        "  ) {\n" +
        "    this.gameMap = gameMap;\n" +
        "    this.options = options;\n" +
        "  }",
      "Parabola ctor",
    );
  }
  if (rel.endsWith("pathfinding/transformers/MiniMapTransformer.ts")) {
    // GameMap/TileRef/PathFinder are all type-only (interface + branded type +
    // interface) and the extensionless specifiers are unloadable -> dropped.
    // The ctor's three parameter properties are expanded.
    out = must(
      out,
      'import { GameMap, TileRef } from "../../game/GameMap";\n' +
        'import { PathFinder } from "../types";\n',
      "",
      "MMT imports",
    );
    out = must(
      out,
      "  constructor(\n" +
        "    private inner: PathFinder<number>,\n" +
        "    private map: GameMap,\n" +
        "    private miniMap: GameMap,\n" +
        "  ) {}",
      "  private inner: PathFinder<number>;\n" +
        "  private map: GameMap;\n" +
        "  private miniMap: GameMap;\n\n" +
        "  constructor(\n" +
        "    inner: PathFinder<number>,\n" +
        "    map: GameMap,\n" +
        "    miniMap: GameMap,\n" +
        "  ) {\n" +
        "    this.inner = inner;\n" +
        "    this.map = map;\n" +
        "    this.miniMap = miniMap;\n" +
        "  }",
      "MMT ctor",
    );
  }
  if (rel.endsWith("pathfinding/PathFinderStepper.ts")) {
    // The `./types` import mixes the PathStatus enum (a runtime value) with
    // PathFinder / PathResult / SteppingPathFinder (types, erased); Node's
    // strip loader rejects `export enum` and the extensionless specifier is
    // unloadable, so inline the numeric enum as a plain object. The ctor's two
    // parameter properties (finder/config) are expanded.
    out = must(
      out,
      "import {\n" +
        "  PathFinder,\n" +
        "  PathResult,\n" +
        "  PathStatus,\n" +
        "  SteppingPathFinder,\n" +
        "} from \"./types\";\n",
      "const PathStatus = { NEXT: 0, COMPLETE: 2, NOT_FOUND: 3 };\n",
      "Stepper types import",
    );
    out = must(
      out,
      "  constructor(\n" +
        "    private finder: PathFinder<T>,\n" +
        "    private config: StepperConfig<T> = { equals: (a, b) => a === b },\n" +
        "  ) {}",
      "  private finder: PathFinder<T>;\n" +
        "  private config: StepperConfig<T>;\n\n" +
        "  constructor(\n" +
        "    finder: PathFinder<T>,\n" +
        "    config: StepperConfig<T> = { equals: (a, b) => a === b },\n" +
        "  ) {\n" +
        "    this.finder = finder;\n" +
        "    this.config = config;\n" +
        "  }",
      "Stepper ctor",
    );
  }
  if (rel.endsWith("pathfinding/transformers/ComponentCheckTransformer.ts")) {
    // PathFinder is type-only (interface) and the extensionless specifier is
    // unloadable -> dropped. The ctor's two parameter properties are expanded.
    out = must(
      out,
      'import { PathFinder } from "../types";\n',
      "",
      "CCT import",
    );
    out = must(
      out,
      "  constructor(\n" +
        "    private inner: PathFinder<T>,\n" +
        "    private getComponent: (t: T) => number,\n" +
        "  ) {}",
      "  private inner: PathFinder<T>;\n" +
        "  private getComponent: (t: T) => number;\n\n" +
        "  constructor(\n" +
        "    inner: PathFinder<T>,\n" +
        "    getComponent: (t: T) => number,\n" +
        "  ) {\n" +
        "    this.inner = inner;\n" +
        "    this.getComponent = getComponent;\n" +
        "  }",
      "CCT ctor",
    );
  }
  if (rel.endsWith("pathfinding/transformers/ShoreCoercingTransformer.ts")) {
    // GameMap/TileRef/PathFinder are all type-only and the extensionless
    // specifiers are unloadable -> dropped. The ctor's two parameter
    // properties are expanded.
    out = must(
      out,
      'import { GameMap, TileRef } from "../../game/GameMap";\n' +
        'import { PathFinder } from "../types";\n',
      "",
      "SCT imports",
    );
    out = must(
      out,
      "  constructor(\n" +
        "    private inner: PathFinder<number>,\n" +
        "    private map: GameMap,\n" +
        "  ) {}",
      "  private inner: PathFinder<number>;\n" +
        "  private map: GameMap;\n\n" +
        "  constructor(\n" +
        "    inner: PathFinder<number>,\n" +
        "    map: GameMap,\n" +
        "  ) {\n" +
        "    this.inner = inner;\n" +
        "    this.map = map;\n" +
        "  }",
      "SCT ctor",
    );
  }
  if (rel.endsWith("pathfinding/transformers/SmoothingWaterTransformer.ts")) {
    // GameMap/TileRef/PathFinder are type-only and the extensionless
    // specifiers are unloadable -> dropped. DebugSpan is a value import
    // redirected to the prepared copy; AStarWaterBounded is redirected too
    // and SearchBounds (an interface) is erased from the named import. The
    // ctor's two parameter properties are expanded; the default predicate
    // `(t) => map.isWater(t)` captures the *parameter* `map`, which the
    // expansion keeps in scope.
    out = must(
      out,
      'import { GameMap, TileRef } from "../../game/GameMap";\n',
      "",
      "SWT GameMap import",
    );
    out = must(out, 'import { PathFinder } from "../types";\n', "", "SWT types import");
    const dbgRel = "src/core/utilities/DebugSpan.ts";
    if (!prepared.has(dbgRel)) prepare(dbgRel);
    out = must(
      out,
      'import { DebugSpan } from "../../utilities/DebugSpan";',
      `import { DebugSpan } from "./${prepared.get(dbgRel)}";`,
      "SWT DebugSpan import",
    );
    const wbRel = "src/core/pathfinding/algorithms/AStar.WaterBounded.ts";
    if (!prepared.has(wbRel)) prepare(wbRel);
    out = must(
      out,
      "import {\n  AStarWaterBounded,\n  SearchBounds,\n} from \"../algorithms/AStar.WaterBounded\";",
      `import { AStarWaterBounded } from "./${prepared.get(wbRel)}";`,
      "SWT WB import",
    );
    out = must(
      out,
      "  constructor(\n" +
        "    private inner: PathFinder<TileRef>,\n" +
        "    private map: GameMap,\n" +
        "    isTraversable: (tile: TileRef) => boolean = (t) => map.isWater(t),\n" +
        "  ) {",
      "  private inner: PathFinder<TileRef>;\n" +
        "  private map: GameMap;\n\n" +
        "  constructor(\n" +
        "    inner: PathFinder<TileRef>,\n" +
        "    map: GameMap,\n" +
        "    isTraversable: (tile: TileRef) => boolean = (t) => map.isWater(t),\n" +
        "  ) {\n" +
        "    this.inner = inner;\n" +
        "    this.map = map;",
      "SWT ctor",
    );
  }

  if (rel.endsWith("game/WaterManager.ts")) {
    // Orchestrator class: GameMap/TileRef (interface + branded type) and
    // PathFinder (interface) are type-only -> dropped. AbstractGraphBuilder,
    // AStarWaterHierarchical, BFSGrid, ConnectedComponents and DebugSpan are
    // *value* uses (the ctor/news them), redirected to their prepared copies.
    // `AbstractGraph` rides along in the builder import as a type. The ctor's
    // three parameter properties are expanded (strip mode rejects them).
    // `(map as unknown as { terrain }).terrain` is a plain type assertion —
    // strip mode erases it and the live private field reads through.
    out = must(out, 'import { PathFinder } from "../pathfinding/types";\n', "", "WM types import");
    out = must(out, 'import { GameMap, TileRef } from "./GameMap";\n', "", "WM GameMap import");
    const agRel = "src/core/pathfinding/algorithms/AbstractGraph.ts";
    if (!prepared.has(agRel)) prepare(agRel);
    out = must(
      out,
      'import {\n  AbstractGraph,\n  AbstractGraphBuilder,\n} from "../pathfinding/algorithms/AbstractGraph";',
      `import {\n  AbstractGraph,\n  AbstractGraphBuilder,\n} from "./${prepared.get(agRel)}";`,
      "WM AbstractGraph import",
    );
    const whRel = "src/core/pathfinding/algorithms/AStar.WaterHierarchical.ts";
    if (!prepared.has(whRel)) prepare(whRel);
    out = must(
      out,
      'import { AStarWaterHierarchical } from "../pathfinding/algorithms/AStar.WaterHierarchical";',
      `import { AStarWaterHierarchical } from "./${prepared.get(whRel)}";`,
      "WM WH import",
    );
    const bfsRel = "src/core/pathfinding/algorithms/BFS.Grid.ts";
    if (!prepared.has(bfsRel)) prepare(bfsRel);
    out = must(
      out,
      'import { BFSGrid } from "../pathfinding/algorithms/BFS.Grid";',
      `import { BFSGrid } from "./${prepared.get(bfsRel)}";`,
      "WM BFSGrid import",
    );
    const ccRel = "src/core/pathfinding/algorithms/ConnectedComponents.ts";
    if (!prepared.has(ccRel)) prepare(ccRel);
    out = must(
      out,
      'import { ConnectedComponents } from "../pathfinding/algorithms/ConnectedComponents";',
      `import { ConnectedComponents } from "./${prepared.get(ccRel)}";`,
      "WM ConnectedComponents import",
    );
    const dbgRel = "src/core/utilities/DebugSpan.ts";
    if (!prepared.has(dbgRel)) prepare(dbgRel);
    out = must(
      out,
      'import { DebugSpan } from "../utilities/DebugSpan";',
      `import { DebugSpan } from "./${prepared.get(dbgRel)}";`,
      "WM DebugSpan import",
    );
    out = must(
      out,
      "  constructor(\n" +
        "    private map: GameMap,\n" +
        "    private miniMap: GameMap,\n" +
        "    private disableNavMesh: boolean,\n" +
        "  ) {",
      "  private map: GameMap;\n" +
        "  private miniMap: GameMap;\n" +
        "  private disableNavMesh: boolean;\n\n" +
        "  constructor(\n" +
        "    map: GameMap,\n" +
        "    miniMap: GameMap,\n" +
        "    disableNavMesh: boolean,\n" +
        "  ) {\n" +
        "    this.map = map;\n" +
        "    this.miniMap = miniMap;\n" +
        "    this.disableNavMesh = disableNavMesh;",
      "WM ctor",
    );
  }

  if (rel.endsWith("game/GameUpdates.ts")) {
    // Wire-shape module: every import is type-only (zod schema types, the
    // Game.ts type surface, the branded TileRef) -> dropped; strip mode
    // erases the annotations that referenced them. `export enum` is inlined
    // as a plain object with the same numeric values (Tile=0 .. DonateEvent=23)
    // PLUS the reverse mapping the V8 numeric-enum runtime object carries
    // (0:"Tile" .. 23:"DonateEvent") - GameImpl.ts's createGameUpdatesMap
    // filters `Object.values(GameUpdateType)` through `!isNaN(Number(key))`,
    // which only has reverse entries to filter out when they exist. Object
    // property order follows the ECMAScript own-key rule: integer-index keys
    // ascending first, then string keys in insertion order, so the forward /
    // reverse split here reproduces the compiled enum's iteration semantics.
    out = must(
      out,
      'import { AllPlayersStats, ClientID, Winner } from "../Schemas";\n',
      "",
      "GameUpdates Schemas import",
    );
    out = must(
      out,
      "import {\n" +
        "  EmojiMessage,\n" +
        "  GameUpdates,\n" +
        "  Gold,\n" +
        "  MessageType,\n" +
        "  NameViewData,\n" +
        "  NukeState,\n" +
        "  PlayerID,\n" +
        "  PlayerType,\n" +
        "  SamLauncherState,\n" +
        "  Team,\n" +
        "  Tick,\n" +
        "  TrainType,\n" +
        "  TransportShipState,\n" +
        "  UnitType,\n" +
        "  WarshipState,\n" +
        '} from "./Game";\n',
      "",
      "GameUpdates Game import",
    );
    out = must(out, 'import { TileRef } from "./GameMap";\n', "", "GameUpdates GameMap import");
    out = must(
      out,
      "export enum GameUpdateType {\n" +
        "  // Tile updates are delivered via `packedTileUpdates` on the outer GameUpdateViewData.\n" +
        "  Tile,\n" +
        "  Unit,\n" +
        "  Player,\n" +
        "  DisplayEvent,\n" +
        "  DisplayChatEvent,\n" +
        "  AllianceRequest,\n" +
        "  AllianceRequestReply,\n" +
        "  BrokeAlliance,\n" +
        "  AllianceExpired,\n" +
        "  AllianceExtension,\n" +
        "  TargetPlayer,\n" +
        "  Emoji,\n" +
        "  Win,\n" +
        "  Hash,\n" +
        "  UnitIncoming,\n" +
        "  BonusEvent,\n" +
        "  RailroadDestructionEvent,\n" +
        "  RailroadConstructionEvent,\n" +
        "  RailroadSnapEvent,\n" +
        "  ConquestEvent,\n" +
        "  EmbargoEvent,\n" +
        "  SpawnPhaseEnd,\n" +
        "  GamePaused,\n" +
        "  DonateEvent,\n" +
        "}",
      "export const GameUpdateType = {\n" +
        "  Tile: 0,\n" +
        "  Unit: 1,\n" +
        "  Player: 2,\n" +
        "  DisplayEvent: 3,\n" +
        "  DisplayChatEvent: 4,\n" +
        "  AllianceRequest: 5,\n" +
        "  AllianceRequestReply: 6,\n" +
        "  BrokeAlliance: 7,\n" +
        "  AllianceExpired: 8,\n" +
        "  AllianceExtension: 9,\n" +
        "  TargetPlayer: 10,\n" +
        "  Emoji: 11,\n" +
        "  Win: 12,\n" +
        "  Hash: 13,\n" +
        "  UnitIncoming: 14,\n" +
        "  BonusEvent: 15,\n" +
        "  RailroadDestructionEvent: 16,\n" +
        "  RailroadConstructionEvent: 17,\n" +
        "  RailroadSnapEvent: 18,\n" +
        "  ConquestEvent: 19,\n" +
        "  EmbargoEvent: 20,\n" +
        "  SpawnPhaseEnd: 21,\n" +
        "  GamePaused: 22,\n" +
        "  DonateEvent: 23,\n" +
        "  0: \"Tile\",\n" +
        "  1: \"Unit\",\n" +
        "  2: \"Player\",\n" +
        "  3: \"DisplayEvent\",\n" +
        "  4: \"DisplayChatEvent\",\n" +
        "  5: \"AllianceRequest\",\n" +
        "  6: \"AllianceRequestReply\",\n" +
        "  7: \"BrokeAlliance\",\n" +
        "  8: \"AllianceExpired\",\n" +
        "  9: \"AllianceExtension\",\n" +
        "  10: \"TargetPlayer\",\n" +
        "  11: \"Emoji\",\n" +
        "  12: \"Win\",\n" +
        "  13: \"Hash\",\n" +
        "  14: \"UnitIncoming\",\n" +
        "  15: \"BonusEvent\",\n" +
        "  16: \"RailroadDestructionEvent\",\n" +
        "  17: \"RailroadConstructionEvent\",\n" +
        "  18: \"RailroadSnapEvent\",\n" +
        "  19: \"ConquestEvent\",\n" +
        "  20: \"EmbargoEvent\",\n" +
        "  21: \"SpawnPhaseEnd\",\n" +
        "  22: \"GamePaused\",\n" +
        "  23: \"DonateEvent\",\n" +
        "};",
      "GameUpdates enum",
    );
  }

  if (rel.endsWith("game/GameImpl.ts")) {
    // Only the module-tail `createGameUpdatesMap` is ported; the GameImpl
    // class body and the createGame factory ride on the Config / PlayerImpl /
    // StatsImpl facades (out of scope). Truncate the file to the tail
    // function, export it for the capture, and import the (reverse-mapped)
    // GameUpdateType object from the prepared GameUpdates copy. The whole
    // import header is dropped with the head slice; strip mode erases the
    // `GameUpdates` type annotations syntactically, so no import is needed
    // for them.
    const marker = "// Or a more dynamic approach";
    const idx = out.indexOf(marker);
    if (idx < 0) {
      throw new Error(
        "ts_load: expected pattern not found (GameImpl tail marker) - the TS source changed, update this shim",
      );
    }
    const guRel = "src/core/game/GameUpdates.ts";
    if (!prepared.has(guRel)) prepare(guRel);
    out = must(
      `import { GameUpdateType } from "./${prepared.get(guRel)}";\n` + out.slice(idx),
      "const createGameUpdatesMap",
      "export const createGameUpdatesMap",
      "GameImpl createGameUpdatesMap export",
    );
  }

  if (rel.endsWith("game/TerraNulliusImpl.ts")) {
    // ClientID / TerraNullius are type-only (the branded id type + the
    // interface); drop both imports and the `implements` clause so the class
    // loads standalone. The method bodies are four constant returns.
    out = must(out, 'import { ClientID } from "../Schemas";\n', "", "TNImpl Schemas import");
    out = must(out, 'import { TerraNullius } from "./Game";\n', "", "TNImpl Game import");
    out = must(
      out,
      "export class TerraNulliusImpl implements TerraNullius {",
      "export class TerraNulliusImpl {",
      "TNImpl implements clause",
    );
  }

  if (rel.endsWith("game/GameUpdateUtils.ts")) {
    // PlayerState / EmojiMessage are `import type` (erased anyway; dropped
    // explicitly so the shim asserts the shape). The GameUpdates import also
    // names three interfaces — strip mode would keep them in the value import
    // and the prepared copy no longer exports them, so only the
    // GameUpdateType *value* is imported, redirected to the prepared copy.
    out = must(
      out,
      'import type { PlayerState } from "../../client/render/types";\n',
      "",
      "GUU PlayerState import",
    );
    out = must(out, 'import type { EmojiMessage } from "./Game";\n', "", "GUU EmojiMessage import");
    const guRel = "src/core/game/GameUpdates.ts";
    if (!prepared.has(guRel)) prepare(guRel);
    out = must(
      out,
      "import {\n" +
        "  AllianceView,\n" +
        "  AttackUpdate,\n" +
        "  GameUpdateType,\n" +
        "  PlayerUpdate,\n" +
        '} from "./GameUpdates";',
      `import { GameUpdateType } from "./${prepared.get(guRel)}";`,
      "GUU GameUpdates import",
    );
  }

  if (rel.endsWith("game/Railroad.ts")) {
    // Game / TileRef / TrainStation are type-only (interface + branded type +
    // the stations are only ever *called on*, never constructed here) ->
    // dropped. GameUpdateType is a value use (delete() stamps it), redirected
    // to the prepared GameUpdates copy. Both ctors use parameter properties,
    // which strip mode rejects; expand them.
    out = must(out, 'import { Game } from "./Game";\n', "", "Railroad Game import");
    out = must(out, 'import { TileRef } from "./GameMap";\n', "", "Railroad GameMap import");
    out = must(out, 'import { TrainStation } from "./TrainStation";\n', "", "Railroad TrainStation import");
    const guRel = "src/core/game/GameUpdates.ts";
    if (!prepared.has(guRel)) prepare(guRel);
    out = must(
      out,
      'import { GameUpdateType } from "./GameUpdates";',
      `import { GameUpdateType } from "./${prepared.get(guRel)}";`,
      "Railroad GameUpdates import",
    );
    out = must(
      out,
      "  constructor(\n" +
        "    public from: TrainStation,\n" +
        "    public to: TrainStation,\n" +
        "    public tiles: TileRef[],\n" +
        "    public id: number,\n" +
        "  ) {}",
      "  from: TrainStation;\n" +
        "  to: TrainStation;\n" +
        "  tiles: TileRef[];\n" +
        "  id: number;\n\n" +
        "  constructor(from: TrainStation, to: TrainStation, tiles: TileRef[], id: number) {\n" +
        "    this.from = from;\n" +
        "    this.to = to;\n" +
        "    this.tiles = tiles;\n" +
        "    this.id = id;\n" +
        "  }",
      "Railroad ctor",
    );
    out = must(
      out,
      "  constructor(\n" +
        "    private railroad: Railroad,\n" +
        "    private forward: boolean,\n" +
        "  ) {",
      "  private railroad: Railroad;\n" +
        "  private forward: boolean;\n\n" +
        "  constructor(railroad: Railroad, forward: boolean) {\n" +
        "    this.railroad = railroad;\n" +
        "    this.forward = forward;",
      "OrientedRailroad ctor",
    );
  }

  if (rel.endsWith("game/RailroadSpatialGrid.ts")) {
    // GameMap / TileRef / Railroad are type-only here (the grid only calls
    // game.x/y and stores rails by reference; strip mode drops the type
    // annotations anyway, but the imports would pull the GameMap value graph
    // -> drop them). The ctor uses parameter properties -> expand.
    out = must(
      out,
      'import { GameMap, TileRef } from "./GameMap";\n',
      "",
      "RailSpatialGrid GameMap import",
    );
    out = must(
      out,
      'import { Railroad } from "./Railroad";\n',
      "",
      "RailSpatialGrid Railroad import",
    );
    out = must(
      out,
      "  constructor(\n" +
        "    private game: GameMap,\n" +
        "    private cellSize: number,\n" +
        "  ) {",
      "  private game: GameMap;\n" +
        "  private cellSize: number;\n\n" +
        "  constructor(game: GameMap, cellSize: number) {\n" +
        "    this.game = game;\n" +
        "    this.cellSize = cellSize;",
      "RailSpatialGrid ctor",
    );
  }

  if (rel.endsWith("game/TileTraversalScratch.ts")) {
    // Game / TileRef are type-only here (the scratch only calls game.width()/
    // height() and stores TileRef numbers in a plain array). Dropping the
    // imports avoids pulling the Game value graph into strip mode.
    out = must(
      out,
      'import { Game } from "./Game";\n',
      "",
      "TileTraversalScratch Game import",
    );
    out = must(
      out,
      'import { TileRef } from "./GameMap";\n',
      "",
      "TileTraversalScratch TileRef import",
    );
  }

  if (rel.endsWith("PatternDecoder.ts")) {
    // PlayerPattern is a type-only import (a z.infer type from Schemas); strip
    // mode cannot tell and would pull in the whole zod schema graph -> drop.
    out = must(out, 'import { PlayerPattern } from "./Schemas";\n', "", "PatternDecoder Schemas import");
  }

  if (rel.endsWith("ServerList.ts")) {
    // zod is only used by the wire-validation schemas (not ported — the pure
    // functions never touch them at runtime). Replace the import with an
    // inert self-returning Proxy so the schema declarations still evaluate.
    out = must(
      out,
      'import { z } from "zod";\n',
      "const z = new Proxy(function () {}, { get: () => z, apply: () => z });\n",
      "ServerList zod import",
    );
    // pathNamesGame is module-private in TS; export it so the capture can
    // exercise its decodeURIComponent edges directly (the Rust twin exposes
    // it for the same reason).
    out = must(
      out,
      "function pathNamesGame(versionFreePath: string, gameID: string): boolean {",
      "export function pathNamesGame(versionFreePath: string, gameID: string): boolean {",
      "ServerList pathNamesGame export",
    );
  }

  if (rel.endsWith("CosmeticSchemas.ts")) {
    // zod is only used by the cosmetic schema declarations (not ported — the
    // pure effect/slot functions never touch them at runtime). Replace the
    // import with an inert self-returning Proxy so the schema declarations
    // still evaluate (same shim as ServerList.ts). The other three imports
    // are type-only or schema-only: base64url and decodePatternData are used
    // exclusively inside PatternDataSchema's refine callback (never invoked
    // by the capture), and PlayerPattern is a z.infer type -> drop.
    out = must(out, 'import { base64url } from "jose";\n', "", "CosmeticSchemas jose import");
    out = must(
      out,
      'import { z } from "zod/v4";\n',
      "const z = new Proxy(function () {}, { get: () => z, apply: () => z });\n",
      "CosmeticSchemas zod import",
    );
    out = must(
      out,
      'import { decodePatternData } from "./PatternDecoder";\n',
      "",
      "CosmeticSchemas PatternDecoder import",
    );
    out = must(out, 'import { PlayerPattern } from "./Schemas";\n', "", "CosmeticSchemas Schemas import");
  }

  if (rel.endsWith("StatsSchemas.ts")) {
    // zod and zb are only used by the wire-validation schema declarations
    // (not ported — the runtime values never touch them). Replace both
    // imports with inert self-returning Proxies so the schema declarations
    // still evaluate (same shim as ServerList.ts). ZbEncodeError must be a
    // real class: the capture exercises toBigInt's throw branch.
    out = must(
      out,
      'import { z } from "zod";\n',
      "const z = new Proxy(function () {}, { get: () => z, apply: () => z });\n",
      "StatsSchemas zod import",
    );
    out = must(
      out,
      'import { zb, ZbEncodeError } from "../../zbin";\n',
      "const zb = new Proxy(function () {}, { get: () => zb, apply: () => zb });\n" +
        "const ZbEncodeError = class extends Error {};\n",
      "StatsSchemas zbin import",
    );
    // UnitType is a string enum in game/Game.ts; Node's strip-only loader
    // rejects `export enum` and pulling the whole Game graph in for one
    // lookup table is overkill. Inline all 16 members as a plain object with
    // the exact upstream values (precedent: Maps.gen GameMapType).
    out = must(
      out,
      'import { UnitType } from "./game/Game";\n',
      "const UnitType = {\n" +
        '  TransportShip: "Transport",\n' +
        '  Warship: "Warship",\n' +
        '  Shell: "Shell",\n' +
        '  SAMMissile: "SAMMissile",\n' +
        '  Port: "Port",\n' +
        '  AtomBomb: "Atom Bomb",\n' +
        '  HydrogenBomb: "Hydrogen Bomb",\n' +
        '  TradeShip: "Trade Ship",\n' +
        '  MissileSilo: "Missile Silo",\n' +
        '  DefensePost: "Defense Post",\n' +
        '  SAMLauncher: "SAM Launcher",\n' +
        '  City: "City",\n' +
        '  MIRV: "MIRV",\n' +
        '  MIRVWarhead: "MIRV Warhead",\n' +
        '  Train: "Train",\n' +
        '  Factory: "Factory",\n' +
        "};\n",
      "StatsSchemas UnitType import",
    );
    // toBigInt is module-private in TS; export it so the capture can exercise
    // its coercion branches directly (the Rust twin exposes it for the same
    // reason).
    out = must(
      out,
      "function toBigInt(v: unknown): bigint {",
      "export function toBigInt(v: unknown): bigint {",
      "StatsSchemas toBigInt export",
    );
  }

  if (rel.endsWith("core/Schemas.ts")) {
    // The zod / zb schema declarations are wire-validation and are not
    // ported, but unlike ServerList / StatsSchemas the capture must *read*
    // z.enum results (.options), so the zod shim keeps a functional `enum`
    // branch (options / exclude, every other chain falls through to the
    // inert Proxy) and routes the rest lazily. zb stays fully inert. The
    // JSON import needs an absolute URL + import attribute (precedent:
    // TribeNames); the Game / StatsSchemas / Util / CosmeticSchemas value
    // imports redirect to prepared copies. `import type { GameEvent }` is
    // erased by strip mode untouched.
    out = must(
      out,
      'import quickChatData from "resources/QuickChat.json";',
      `import quickChatData from "${TS_URL}resources/QuickChat.json" with { type: "json" };`,
      "Schemas QuickChat JSON import",
    );
    out = must(
      out,
      'import { z } from "zod";\n',
      "const z = new Proxy(function () {}, {\n" +
        "  get: (_t, k) => {\n" +
        "    if (k === \"enum\") {\n" +
        "      return (a) => {\n" +
        "        const base = {\n" +
        "          options: a,\n" +
        "          exclude: (b) => ({ options: a.filter((o) => !b.includes(o)) }),\n" +
        "        };\n" +
        "        return new Proxy(base, { get: (t, kk) => (kk in t ? t[kk] : z), apply: () => z });\n" +
        "      };\n" +
        "    }\n" +
        "    return z;\n" +
        "  },\n" +
        "  apply: () => z,\n" +
        "});\n",
      "Schemas zod import",
    );
    out = must(
      out,
      'import { zb } from "../../zbin";\n',
      "const zb = new Proxy(function () {}, { get: () => zb, apply: () => zb });\n",
      "Schemas zbin import",
    );
    const cosRel = "src/core/CosmeticSchemas.ts";
    if (!prepared.has(cosRel)) prepare(cosRel);
    out = must(
      out,
      "import {\n" +
        "  ColorPaletteSchema,\n" +
        "  CosmeticNameSchema,\n" +
        "  EffectTypeSchema,\n" +
        "  PatternDataSchema,\n" +
        '} from "./CosmeticSchemas";\n',
      "import {\n" +
        "  ColorPaletteSchema,\n" +
        "  CosmeticNameSchema,\n" +
        "  EffectTypeSchema,\n" +
        "  PatternDataSchema,\n" +
        `} from "./${prepared.get(cosRel)}";\n`,
      "Schemas CosmeticSchemas import",
    );
    if (!prepared.has("src/core/game/Game.ts")) prepare("src/core/game/Game.ts");
    out = must(
      out,
      "import {\n" +
        "  AllPlayers,\n" +
        "  Difficulty,\n" +
        "  Duos,\n" +
        "  GameMapSize,\n" +
        "  GameMapType,\n" +
        "  GameMode,\n" +
        "  GameType,\n" +
        "  HumansVsNations,\n" +
        "  MAX_UPGRADE_AMOUNT,\n" +
        "  Quads,\n" +
        "  RankedType,\n" +
        "  Trios,\n" +
        "  UnitType,\n" +
        '} from "./game/Game";\n',
      "import {\n" +
        "  AllPlayers,\n" +
        "  Difficulty,\n" +
        "  Duos,\n" +
        "  GameMapSize,\n" +
        "  GameMapType,\n" +
        "  GameMode,\n" +
        "  GameType,\n" +
        "  HumansVsNations,\n" +
        "  MAX_UPGRADE_AMOUNT,\n" +
        "  Quads,\n" +
        "  RankedType,\n" +
        "  Trios,\n" +
        "  UnitType,\n" +
        `} from "./${prepared.get("src/core/game/Game.ts")}";\n`,
      "Schemas Game import",
    );
    const statsRel = "src/core/StatsSchemas.ts";
    if (!prepared.has(statsRel)) prepare(statsRel);
    out = must(
      out,
      'import { ArchivedPlayerStatsSchema, PlayerStatsSchema } from "./StatsSchemas";\n',
      `import { ArchivedPlayerStatsSchema, PlayerStatsSchema } from "./${prepared.get(statsRel)}";\n`,
      "Schemas StatsSchemas import",
    );
    const utilRel = "src/core/Util.ts";
    if (!prepared.has(utilRel)) prepare(utilRel);
    out = must(
      out,
      'import { flattenedEmojiTable, LOBBY_LABEL_MAX } from "./Util";\n',
      `import { flattenedEmojiTable, LOBBY_LABEL_MAX } from "./${prepared.get(utilRel)}";\n`,
      "Schemas Util import",
    );
    // The four ported z.enum declarations become explicit option objects (the
    // capture reads .options; the schema fields chain .optional(), which the
    // shim would also provide - spelled out here for determinism).
    out = must(
      out,
      "export const PublicGameTypeSchema = z.enum([\n" +
        '  "ffa",\n' +
        '  "team",\n' +
        '  "special",\n' +
        '  "hosted",\n' +
        "]);",
      "export const PublicGameTypeSchema = {\n" +
        '  options: ["ffa", "team", "special", "hosted"],\n' +
        "  exclude: (b) => ({\n" +
        "    options: PublicGameTypeSchema.options.filter((o) => !b.includes(o)),\n" +
        "    optional: () => z,\n" +
        "  }),\n" +
        "  optional: () => z,\n" +
        "};",
      "Schemas PublicGameTypeSchema",
    );
    out = must(
      out,
      'export const LobbyAccentSchema = z.enum(["gold", "blue", "green", "red"]);',
      'export const LobbyAccentSchema = {\n' +
        '  options: ["gold", "blue", "green", "red"],\n' +
        "  optional: () => z,\n" +
        "};",
      "Schemas LobbyAccentSchema",
    );
    out = must(
      out,
      "export const ReportReasonSchema = z.enum([\n" +
        '  "botting",\n' +
        '  "teaming",\n' +
        '  "inappropriate_username",\n' +
        '  "griefing",\n' +
        "]);",
      "export const ReportReasonSchema = {\n" +
        '  options: ["botting", "teaming", "inappropriate_username", "griefing"],\n' +
        "  optional: () => z,\n" +
        "};",
      "Schemas ReportReasonSchema",
    );
    out = must(
      out,
      'export const ClientPlatformSchema = z.enum(["web", "steam", "crazygames"]);',
      'export const ClientPlatformSchema = {\n' +
        '  options: ["web", "steam", "crazygames"],\n' +
        "  optional: () => z,\n" +
        "};",
      "Schemas ClientPlatformSchema",
    );
    // QuickChatKeySchema derives its options from the JSON at module scope;
    // drop the z.enum wrapper and the type assertion so the export is the
    // plain derived array the capture dumps.
    out = must(
      out,
      "export const QuickChatKeySchema = z.enum(\n" +
        "  Object.entries(quickChatData).flatMap(([category, entries]) =>\n" +
        "    entries.map((entry) => `${category}.${entry.key}`),\n" +
        "  ) as [string, ...string[]],\n" +
        ");",
      "export const QuickChatKeySchema = Object.entries(quickChatData).flatMap(\n" +
        "  ([category, entries]) => entries.map((entry) => `${category}.${entry.key}`),\n" +
        ");",
      "Schemas QuickChatKeySchema",
    );
    // Node's strip-only TS loader rejects `export enum`; LogSeverity is a
    // string enum -> plain object (precedent: MotionPlans / TerrainSearchMap).
    out = must(
      out,
      "export enum LogSeverity {\n" +
        '  Debug = "DEBUG",\n' +
        '  Info = "INFO",\n' +
        '  Warn = "WARN",\n' +
        '  Error = "ERROR",\n' +
        '  Fatal = "FATAL",\n' +
        "}",
      "export const LogSeverity = {\n" +
        '  Debug: "DEBUG",\n' +
        '  Info: "INFO",\n' +
        '  Warn: "WARN",\n' +
        '  Error: "ERROR",\n' +
        '  Fatal: "FATAL",\n' +
        "};",
      "Schemas LogSeverity enum",
    );
    // The two event classes use ctor parameter properties, which strip mode
    // rejects; expand them (precedent: MinHeap / BFS / Air). The `implements
    // GameEvent` clause rides on the erased type-only import -> dropped.
    out = must(
      out,
      "export class LobbyInfoEvent implements GameEvent {\n" +
        "  constructor(\n" +
        "    public lobby: GameInfo,\n" +
        "    public myClientID: ClientID,\n" +
        "  ) {}\n" +
        "}",
      "export class LobbyInfoEvent {\n" +
        "  public lobby: GameInfo;\n" +
        "  public myClientID: ClientID;\n" +
        "  constructor(\n" +
        "    lobby: GameInfo,\n" +
        "    myClientID: ClientID,\n" +
        "  ) {\n" +
        "    this.lobby = lobby;\n" +
        "    this.myClientID = myClientID;\n" +
        "  }\n" +
        "}",
      "Schemas LobbyInfoEvent ctor",
    );
    out = must(
      out,
      "export class GroupTokenEvent implements GameEvent {\n" +
        "  constructor(public groupToken: string) {}\n" +
        "}",
      "export class GroupTokenEvent {\n" +
        "  public groupToken: string;\n" +
        "  constructor(groupToken: string) {\n" +
        "    this.groupToken = groupToken;\n" +
        "  }\n" +
        "}",
      "Schemas GroupTokenEvent ctor",
    );
  }

  if (rel.endsWith("core/ApiSchemas.ts")) {
    // The runtime-value subset (data constants, z.enum .options, the four
    // pure predicates) rides on the same functional zod shim as
    // core/Schemas: enum keeps real options / exclude, everything else falls
    // through to the inert Proxy - which also swallows the .unwrap() /
    // .pick() / .extend() / .refine() / .transform() / .or() / .default() /
    // .catch() / .iso.datetime() / .partialRecord() chains this file builds
    // its (unported) schema declarations with. The Base64 import pulls in
    // `jose`, but base64urlToUuid is only referenced inside the
    // TokenPayloadSchema refine / transform callbacks the capture never
    // invokes -> replace the import with a stub. The Schemas / StatsSchemas
    // / Game value imports redirect to prepared copies.
    out = must(
      out,
      'import { z } from "zod";\n',
      "const z = new Proxy(function () {}, {\n" +
        "  get: (_t, k) => {\n" +
        "    if (k === \"enum\") {\n" +
        "      return (a) => {\n" +
        "        const base = {\n" +
        "          options: a,\n" +
        "          exclude: (b) => ({ options: a.filter((o) => !b.includes(o)) }),\n" +
        "        };\n" +
        "        return new Proxy(base, { get: (t, kk) => (kk in t ? t[kk] : z), apply: () => z });\n" +
        "      };\n" +
        "    }\n" +
        "    return z;\n" +
        "  },\n" +
        "  apply: () => z,\n" +
        "});\n",
      "ApiSchemas zod import",
    );
    out = must(
      out,
      'import { base64urlToUuid } from "./Base64";\n',
      "function base64urlToUuid(_encoded: string): string {\n" +
        "  return null as unknown as string;\n" +
        "}\n",
      "ApiSchemas Base64 import",
    );
    const apiSchemasRel = "src/core/Schemas.ts";
    if (!prepared.has(apiSchemasRel)) prepare(apiSchemasRel);
    out = must(
      out,
      'import { ClanTagSchema } from "./Schemas";\n',
      `import { ClanTagSchema } from "./${prepared.get(apiSchemasRel)}";\n`,
      "ApiSchemas Schemas import",
    );
    const apiStatsRel = "src/core/StatsSchemas.ts";
    if (!prepared.has(apiStatsRel)) prepare(apiStatsRel);
    out = must(
      out,
      'import { BigIntStringSchema, PlayerStatsSchema } from "./StatsSchemas";\n',
      `import { BigIntStringSchema, PlayerStatsSchema } from "./${prepared.get(apiStatsRel)}";\n`,
      "ApiSchemas StatsSchemas import",
    );
    if (!prepared.has("src/core/game/Game.ts")) prepare("src/core/game/Game.ts");
    out = must(
      out,
      "import {\n" +
        "  Difficulty,\n" +
        "  GameMode,\n"
        +
        "  GameType,\n" +
        "  HumansVsNations,\n" +
        "  RankedType,\n" +
        '} from "./game/Game";\n',
      "import {\n" +
        "  Difficulty,\n" +
        "  GameMode,\n" +
        "  GameType,\n" +
        "  HumansVsNations,\n" +
        "  RankedType,\n" +
        `} from "./${prepared.get("src/core/game/Game.ts")}";\n`,
      "ApiSchemas Game import",
    );
  }

  if (rel.endsWith("game/TerrainMapLoader.ts")) {
    // The runtime surface (loadTerrainMap + genTerrainFromBin + the module
    // loadedMaps cache) needs two value imports: GameMapSize (the enum the
    // mapSize branches compare against) and GameMapImpl (new'd by
    // genTerrainFromBin). TeamGameSpawnAreas / GameMap / GameMapLoader are
    // type-only (annotations erased by strip mode, but the *named imports*
    // survive and would link-error against erased exports), so they are
    // dropped from the import lists; both value imports redirect to the
    // prepared copies (Game.ts inlines GameMapSize / re-exports GameMapType
    // as plain objects, GameMap.ts keeps the expanded GameMapImpl ctor).
    if (!prepared.has("src/core/game/Game.ts")) prepare("src/core/game/Game.ts");
    if (!prepared.has("src/core/game/GameMap.ts")) prepare("src/core/game/GameMap.ts");
    out = must(
      out,
      'import { GameMapSize, GameMapType, TeamGameSpawnAreas } from "./Game";\n' +
        'import { GameMap, GameMapImpl } from "./GameMap";\n' +
        'import { GameMapLoader } from "./GameMapLoader";\n',
      `import { GameMapSize, GameMapType } from "./${prepared.get("src/core/game/Game.ts")}";\n` +
        `import { GameMapImpl } from "./${prepared.get("src/core/game/GameMap.ts")}";\n`,
      "TerrainMapLoader imports",
    );
  }

  if (rel.endsWith("execution/nation/NationUtils.ts")) {
    // Cell / Structures / UnitType are *value* uses (`new Cell`, the
    // `Structures.has` membership and the DefensePost / MissileSilo string
    // comparisons) and ride on the prepared Game.ts copy (UnitType inlined as
    // a plain object); Game / Player / TileRef / PseudoRandom are type-only
    // annotations erased by strip mode -> dropped outright (the capture feeds
    // scripted mocks for the facades). calculateBoundingBox is a *value* use
    // redirected to the prepared Util.ts copy (its own enum-inlined
    // calculateBoundingBox + TileSet instanceof path).
    if (!prepared.has("src/core/game/Game.ts")) prepare("src/core/game/Game.ts");
    const nuUtilRel = "src/core/Util.ts";
    if (!prepared.has(nuUtilRel)) prepare(nuUtilRel);
    out = must(
      out,
      'import { Cell, Game, Player, Structures, UnitType } from "../../game/Game";\n' +
        'import { TileRef } from "../../game/GameMap";\n' +
        'import { PseudoRandom } from "../../PseudoRandom";\n' +
        'import { calculateBoundingBox } from "../../Util";\n',
      `import { Cell, Structures, UnitType } from "./${prepared.get("src/core/game/Game.ts")}";\n` +
        `import { calculateBoundingBox } from "./${prepared.get(nuUtilRel)}";\n`,
      "NationUtils imports",
    );
  }

  if (rel.endsWith("game/Maps.gen.ts")) {
    // Node's strip-only TS loader rejects `export enum`. GameMapType is a
    // string enum (member name = the folder id, value = the canonical wire
    // name); rewrite the whole declaration into a plain object literal so the
    // `type: GameMapType.X` references in `maps` still resolve. The types
    // (GameMapName, MapCategory, SpecialModifierKey, MapInfo, CustomTribe,
    // MapLayer, LayerPlacement) are erased by strip mode, and `maps` /
    // `mapCategoryOrder` are plain const arrays that load untouched.
    const before = out;
    out = out.replace(
      /export enum GameMapType \{([\s\S]*?)\n\}/,
      (_m, body) => {
        const entries = [
          ...body.matchAll(
            /^\s*([A-Za-z0-9_$]+)\s*=\s*"((?:[^"\\]|\\.)*)"/gm,
          ),
        ]
          .map((mm) => `  ${mm[1]}: "${mm[2]}",`)
          .join("\n");
        return `export const GameMapType = {\n${entries}\n};`;
      },
    );
    if (out === before) {
      throw new Error(
        "ts_load: Maps.gen.ts GameMapType enum not found - the TS source changed, update this shim",
      );
    }
  }

  if (rel.endsWith("game/Game.ts")) {
    // All 13 imports are type-only (interfaces / branded types / type
    // aliases - grep-verified: none is used in a value position), so the
    // block is dropped outright. The one runtime dependency the ported
    // surface actually needs is `formatPlayerDisplayName`, inlined verbatim
    // from Util.ts (PlayerInfo's ctor calls it).
    out = must(
      out,
      'import { Config } from "../configuration/Config";\n' +
        'import { AbstractGraph } from "../pathfinding/algorithms/AbstractGraph";\n' +
        'import { PathFinder } from "../pathfinding/types";\n' +
        'import { AllPlayersStats, ClientID } from "../Schemas";\n' +
        'import { formatPlayerDisplayName } from "../Util";\n' +
        'import { GameMap, TileRef } from "./GameMap";\n' +
        "import {\n" +
        "  GameUpdate,\n" +
        "  GameUpdateType,\n" +
        "  PlayerUpdate,\n" +
        "  UnitUpdate,\n" +
        '} from "./GameUpdates";\n' +
        'import { MotionPlanRecord } from "./MotionPlans";\n' +
        'import { RailNetwork } from "./RailNetwork";\n' +
        'import { Stats } from "./Stats";\n' +
        'import { ReadonlyTileSet } from "./TileSet";\n' +
        'import { UnitPredicate } from "./UnitGrid";\n',
      "function formatPlayerDisplayName(username, clanTag) {\n" +
        "  return clanTag ? `[${clanTag}] ${username}` : username;\n" +
        "}\n",
      "Game imports",
    );
    // The Maps.gen re-export: strip mode erases the `type` members, but the
    // value exports (GameMapType / mapCategoryOrder / maps) must resolve to
    // the prepared copy (whose enum is inlined as a plain object).
    const gameMapsRel = "src/core/game/Maps.gen.ts";
    if (!prepared.has(gameMapsRel)) prepare(gameMapsRel);
    out = must(
      out,
      '} from "./Maps.gen";',
      `} from "./${prepared.get(gameMapsRel)}";`,
      "Game Maps.gen re-export",
    );
    // Node's strip-only TS loader rejects `export enum`; the 12 enums are
    // inlined as plain objects with the same member order and values.
    out = must(
      out,
      'export enum Difficulty {\n' +
        '  Easy = "Easy",\n' +
        '  Medium = "Medium",\n' +
        '  Hard = "Hard",\n' +
        '  Impossible = "Impossible",\n' +
        "}",
      'export const Difficulty = {\n' +
        '  Easy: "Easy",\n' +
        '  Medium: "Medium",\n' +
        '  Hard: "Hard",\n' +
        '  Impossible: "Impossible",\n' +
        "};",
      "Game Difficulty enum",
    );
    out = must(
      out,
      'export enum GameType {\n' +
        '  Singleplayer = "Singleplayer",\n' +
        '  Public = "Public",\n' +
        '  Private = "Private",\n' +
        "}",
      'export const GameType = {\n' +
        '  Singleplayer: "Singleplayer",\n' +
        '  Public: "Public",\n' +
        '  Private: "Private",\n' +
        "};",
      "Game GameType enum",
    );
    out = must(
      out,
      'export enum GameMode {\n  FFA = "Free For All",\n  Team = "Team",\n}',
      'export const GameMode = {\n  FFA: "Free For All",\n  Team: "Team",\n};',
      "Game GameMode enum",
    );
    out = must(
      out,
      'export enum RankedType {\n  OneVOne = "1v1",\n  TwoVTwo = "2v2",\n}',
      'export const RankedType = {\n  OneVOne: "1v1",\n  TwoVTwo: "2v2",\n};',
      "Game RankedType enum",
    );
    out = must(
      out,
      'export enum GameMapSize {\n  Compact = "Compact",\n  Normal = "Normal",\n}',
      'export const GameMapSize = {\n  Compact: "Compact",\n  Normal: "Normal",\n};',
      "Game GameMapSize enum",
    );
    out = must(
      out,
      'export enum UnitType {\n' +
        '  TransportShip = "Transport",\n' +
        '  Warship = "Warship",\n' +
        '  Shell = "Shell",\n' +
        '  SAMMissile = "SAMMissile",\n' +
        '  Port = "Port",\n' +
        '  AtomBomb = "Atom Bomb",\n' +
        '  HydrogenBomb = "Hydrogen Bomb",\n' +
        '  TradeShip = "Trade Ship",\n' +
        '  MissileSilo = "Missile Silo",\n' +
        '  DefensePost = "Defense Post",\n' +
        '  SAMLauncher = "SAM Launcher",\n' +
        '  City = "City",\n' +
        '  MIRV = "MIRV",\n' +
        '  MIRVWarhead = "MIRV Warhead",\n' +
        '  Train = "Train",\n' +
        '  Factory = "Factory",\n' +
        "}",
      'export const UnitType = {\n' +
        '  TransportShip: "Transport",\n' +
        '  Warship: "Warship",\n' +
        '  Shell: "Shell",\n' +
        '  SAMMissile: "SAMMissile",\n' +
        '  Port: "Port",\n' +
        '  AtomBomb: "Atom Bomb",\n' +
        '  HydrogenBomb: "Hydrogen Bomb",\n' +
        '  TradeShip: "Trade Ship",\n' +
        '  MissileSilo: "Missile Silo",\n' +
        '  DefensePost: "Defense Post",\n' +
        '  SAMLauncher: "SAM Launcher",\n' +
        '  City: "City",\n' +
        '  MIRV: "MIRV",\n' +
        '  MIRVWarhead: "MIRV Warhead",\n' +
        '  Train: "Train",\n' +
        '  Factory: "Factory",\n' +
        "};",
      "Game UnitType enum",
    );
    out = must(
      out,
      'export enum TrainType {\n' +
        '  Engine = "Engine",\n' +
        '  TailEngine = "TailEngine",\n' +
        '  Carriage = "Carriage",\n' +
        "}",
      'export const TrainType = {\n' +
        '  Engine: "Engine",\n' +
        '  TailEngine: "TailEngine",\n' +
        '  Carriage: "Carriage",\n' +
        "};",
      "Game TrainType enum",
    );
    out = must(
      out,
      "export enum Relation {\n  Hostile = 0,\n  Distrustful = 1,\n  Neutral = 2,\n  Friendly = 3,\n}",
      "export const Relation = {\n  Hostile: 0,\n  Distrustful: 1,\n  Neutral: 2,\n  Friendly: 3,\n};",
      "Game Relation enum",
    );
    out = must(
      out,
      "export enum TerrainType {\n  Plains,\n  Highland,\n  Mountain,\n  Ocean,\n  Impassable,\n}",
      "export const TerrainType = {\n  Plains: 0,\n  Highland: 1,\n  Mountain: 2,\n  Ocean: 3,\n  Impassable: 4,\n};",
      "Game TerrainType enum",
    );
    out = must(
      out,
      'export enum PlayerType {\n  Bot = "BOT",\n  Human = "HUMAN",\n  Nation = "NATION",\n}',
      'export const PlayerType = {\n  Bot: "BOT",\n  Human: "HUMAN",\n  Nation: "NATION",\n};',
      "Game PlayerType enum",
    );
    out = must(
      out,
      "export enum MessageType {\n" +
        "  ATTACK_FAILED,\n" +
        "  ATTACK_CANCELLED,\n" +
        "  ATTACK_REQUEST,\n" +
        "  CONQUERED_PLAYER,\n" +
        "  MIRV_INBOUND,\n" +
        "  NUKE_INBOUND,\n" +
        "  NUKE_DETONATED,\n" +
        "  HYDROGEN_BOMB_INBOUND,\n" +
        "  NAVAL_INVASION_INBOUND,\n" +
        "  SAM_MISS,\n" +
        "  SAM_HIT,\n" +
        "  CAPTURED_ENEMY_UNIT,\n" +
        "  UNIT_DESTROYED,\n" +
        "  ALLIANCE_ACCEPTED,\n" +
        "  ALLIANCE_REJECTED,\n" +
        "  ALLIANCE_REQUEST,\n" +
        "  ALLIANCE_BROKEN,\n" +
        "  ALLIANCE_EXPIRED,\n" +
        "  DONATION_SENT,\n" +
        "  DONATION_RECEIVED,\n" +
        "  CHAT,\n" +
        "  RENEW_ALLIANCE,\n" +
        "}",
      "export const MessageType = {\n" +
        "  ATTACK_FAILED: 0,\n" +
        "  ATTACK_CANCELLED: 1,\n" +
        "  ATTACK_REQUEST: 2,\n" +
        "  CONQUERED_PLAYER: 3,\n" +
        "  MIRV_INBOUND: 4,\n" +
        "  NUKE_INBOUND: 5,\n" +
        "  NUKE_DETONATED: 6,\n" +
        "  HYDROGEN_BOMB_INBOUND: 7,\n" +
        "  NAVAL_INVASION_INBOUND: 8,\n" +
        "  SAM_MISS: 9,\n" +
        "  SAM_HIT: 10,\n" +
        "  CAPTURED_ENEMY_UNIT: 11,\n" +
        "  UNIT_DESTROYED: 12,\n" +
        "  ALLIANCE_ACCEPTED: 13,\n" +
        "  ALLIANCE_REJECTED: 14,\n" +
        "  ALLIANCE_REQUEST: 15,\n" +
        "  ALLIANCE_BROKEN: 16,\n" +
        "  ALLIANCE_EXPIRED: 17,\n" +
        "  DONATION_SENT: 18,\n" +
        "  DONATION_RECEIVED: 19,\n" +
        "  CHAT: 20,\n" +
        "  RENEW_ALLIANCE: 21,\n" +
        "};",
      "Game MessageType enum",
    );
    out = must(
      out,
      'export enum MessageCategory {\n' +
        '  ATTACK = "ATTACK",\n' +
        '  NUKE = "NUKE",\n' +
        '  ALLIANCE = "ALLIANCE",\n' +
        '  TRADE = "TRADE",\n' +
        '  CHAT = "CHAT",\n' +
        "}",
      'export const MessageCategory = {\n' +
        '  ATTACK: "ATTACK",\n' +
        '  NUKE: "NUKE",\n' +
        '  ALLIANCE: "ALLIANCE",\n' +
        '  TRADE: "TRADE",\n' +
        '  CHAT: "CHAT",\n' +
        "};",
      "Game MessageCategory enum",
    );
    // The three classes use ctor parameter properties, which strip mode
    // rejects; expand them, same as MinHeap / GameMap.
    out = must(
      out,
      "export class Nation {\n" +
        "  constructor(\n" +
        "    public readonly spawnCell: Cell | undefined,\n" +
        "    public readonly playerInfo: PlayerInfo,\n" +
        "  ) {}\n" +
        "}",
      "export class Nation {\n" +
        "  public readonly spawnCell: Cell | undefined;\n" +
        "  public readonly playerInfo: PlayerInfo;\n" +
        "  constructor(\n" +
        "    spawnCell: Cell | undefined,\n" +
        "    playerInfo: PlayerInfo,\n" +
        "  ) {\n" +
        "    this.spawnCell = spawnCell;\n" +
        "    this.playerInfo = playerInfo;\n" +
        "  }\n" +
        "}",
      "Game Nation ctor",
    );
    out = must(
      out,
      "  constructor(\n" +
        "    public readonly x: number,\n" +
        "    public readonly y: number,\n" +
        "  ) {\n" +
        "    this.strRepr = `Cell[${this.x},${this.y}]`;",
      "  public readonly x: number;\n" +
        "  public readonly y: number;\n\n" +
        "  constructor(\n" +
        "    x: number,\n" +
        "    y: number,\n" +
        "  ) {\n" +
        "    this.x = x;\n" +
        "    this.y = y;\n" +
        "    this.strRepr = `Cell[${this.x},${this.y}]`;",
      "Game Cell ctor",
    );
    out = must(
      out,
      "export class PlayerInfo {\n" +
        "  public readonly displayName: string;\n\n" +
        "  constructor(\n" +
        "    public readonly name: string,\n" +
        "    public readonly playerType: PlayerType,\n" +
        "    // null if tribe.\n" +
        "    public readonly clientID: ClientID | null,\n" +
        "    // TODO: make player id the small id\n" +
        "    public readonly id: PlayerID,\n" +
        "    public readonly isLobbyCreator: boolean = false,\n" +
        "    public readonly clanTag: string | null = null,\n" +
        "    public readonly friends: ClientID[] = [],\n" +
        "    // Server-pinned team slot (index into the game's team list) for\n" +
        "    // matchmade team games; null = assign normally.\n" +
        "    public readonly teamIndex: number | null = null,\n" +
        "    // Manifest flag code (e.g. \"in\", \"pk\") for PlayerType.Nation players.\n" +
        "    // Carried from the map manifest through to the client so it can render\n" +
        "    // the correct flag even when multiple nations on a map share a display\n" +
        "    // name (e.g. India's and Pakistan's \"Punjab\").\n" +
        "    public readonly nationFlag: string | null = null,\n" +
        "  ) {\n" +
        "    this.displayName = formatPlayerDisplayName(this.name, this.clanTag);\n" +
        "  }\n" +
        "}",
      "export class PlayerInfo {\n" +
        "  public readonly displayName: string;\n" +
        "  public readonly name: string;\n" +
        "  public readonly playerType: PlayerType;\n" +
        "  public readonly clientID: ClientID | null;\n" +
        "  public readonly id: PlayerID;\n" +
        "  public readonly isLobbyCreator: boolean;\n" +
        "  public readonly clanTag: string | null;\n" +
        "  public readonly friends: ClientID[];\n" +
        "  public readonly teamIndex: number | null;\n" +
        "  public readonly nationFlag: string | null;\n\n" +
        "  constructor(\n" +
        "    name: string,\n" +
        "    playerType: PlayerType,\n" +
        "    clientID: ClientID | null,\n" +
        "    id: PlayerID,\n" +
        "    isLobbyCreator: boolean = false,\n" +
        "    clanTag: string | null = null,\n" +
        "    friends: ClientID[] = [],\n" +
        "    teamIndex: number | null = null,\n" +
        "    nationFlag: string | null = null,\n" +
        "  ) {\n" +
        "    this.name = name;\n" +
        "    this.playerType = playerType;\n" +
        "    this.clientID = clientID;\n" +
        "    this.id = id;\n" +
        "    this.isLobbyCreator = isLobbyCreator;\n" +
        "    this.clanTag = clanTag;\n" +
        "    this.friends = friends;\n" +
        "    this.teamIndex = teamIndex;\n" +
        "    this.nationFlag = nationFlag;\n" +
        "    this.displayName = formatPlayerDisplayName(this.name, this.clanTag);\n" +
        "  }\n" +
        "}",
      "Game PlayerInfo ctor",
    );
  }

  if (rel.endsWith("execution/utils/TribeNames.ts")) {
    // The JSON import needs a `with { type: "json" }` attribute (Node's
    // strip-only loader does not apply the package's import attributes for a
    // bare "resources/..." specifier), so redirect it to an absolute file
    // URL with the attribute inline. Maps.gen imports: the types are erased
    // by strip mode, but the *value* import of GameMapType + maps must
    // resolve to the prepared copy (whose enum is inlined).
    out = must(
      out,
      'import tribeNameThemesData from "resources/tribeNameThemes.json";',
      `import tribeNameThemesData from "${TS_URL}resources/tribeNameThemes.json" with { type: "json" };`,
      "TribeNames JSON import",
    );
    const mapsRel = "src/core/game/Maps.gen.ts";
    if (!prepared.has(mapsRel)) prepare(mapsRel);
    out = must(
      out,
      'import {\n' +
        '  type CustomTribe,\n' +
        '  GameMapType,\n' +
        '  type MapInfo,\n' +
        '  maps,\n' +
        '} from "../../game/Maps.gen";',
      `import { GameMapType, maps } from "./${prepared.get(mapsRel)}";`,
      "TribeNames Maps.gen import",
    );
  }

  if (rel.endsWith("game/NationCreation.ts")) {
    // The Schemas / TerrainMapLoader imports are type-only (GameStartInfo,
    // ManifestNation, AdditionalNation) and the PseudoRandom import is only
    // used as a type annotation - all erased by strip mode. The Game.ts
    // import is a *value* use (Cell / PlayerInfo / PlayerType / the enums
    // behind createNationsForGame's config comparisons), so it redirects to
    // the prepared Game.ts copy (whose enums are inlined plain objects and
    // whose Nation ctor is expanded). The module's own tables and helpers
    // (PLURAL_NOUN / NOUN / NAME_TEMPLATES / NOUNS / O_TO_OES /
    // SPECIAL_PLURALS / pluralize / generateNationName /
    // generateUniqueNationName / createRandomNations) are module-private;
    // they are re-exported for the capture.
    if (!prepared.has("src/core/game/Game.ts")) prepare("src/core/game/Game.ts");
    out = must(
      out,
      'import { PseudoRandom } from "../PseudoRandom";\n' +
        'import { GameStartInfo } from "../Schemas";\n' +
        "import {\n" +
        "  Cell,\n" +
        "  GameMapSize,\n" +
        "  GameMode,\n" +
        "  GameType,\n" +
        "  HumansVsNations,\n" +
        "  Nation,\n" +
        "  PlayerInfo,\n" +
        "  PlayerType,\n" +
        "} from \"./Game\";\n" +
        'import { AdditionalNation, Nation as ManifestNation } from "./TerrainMapLoader";\n',
      "import {\n" +
        "  Cell,\n" +
        "  GameMapSize,\n" +
        "  GameMode,\n" +
        "  GameType,\n" +
        "  HumansVsNations,\n" +
        "  Nation,\n" +
        "  PlayerInfo,\n" +
        "  PlayerType,\n" +
      `} from "./${prepared.get("src/core/game/Game.ts")}";\n`,
      "NationCreation imports",
    );
    out =
      out +
      "\nexport { PLURAL_NOUN, NOUN, NAME_TEMPLATES, NOUNS, O_TO_OES, SPECIAL_PLURALS, pluralize, generateNationName, generateUniqueNationName, createRandomNations };\n";
  }

  if (rel.endsWith("execution/nation/NationEmojiBehavior.ts")) {
    // The Game.ts import names only type-position uses (AllPlayers /
    // Difficulty / Game / GameMode / Player / PlayerType / Relation / Tick):
    // strip mode would still *execute* the import and pull the whole Game.ts
    // graph, but the capture only reads the module-top-level EMOJI_* consts,
    // so the class body and the respondTo* functions never run. Drop it, and
    // likewise PseudoRandom / EmojiExecution (only referenced inside method
    // bodies / erased annotations). flattenedEmojiTable is a *value* use (the
    // 23 consts map over it at module scope), so it redirects to the prepared
    // Util copy. The ctor is parameter properties, which strip mode rejects;
    // expand them, same as MinHeap / BFS.
    out = must(
      out,
      'import {\n' +
        '  AllPlayers,\n' +
        '  Difficulty,\n' +
        '  Game,\n' +
        '  GameMode,\n' +
        '  Player,\n' +
        '  PlayerType,\n' +
        '  Relation,\n' +
        '  Tick,\n' +
        '} from "../../game/Game";\n',
      "",
      "NationEmoji Game import",
    );
    out = must(
      out,
      'import { PseudoRandom } from "../../PseudoRandom";\n',
      "",
      "NationEmoji PseudoRandom import",
    );
    out = must(
      out,
      'import { EmojiExecution } from "../EmojiExecution";\n',
      "",
      "NationEmoji EmojiExecution import",
    );
    const utilRel = "src/core/Util.ts";
    if (!prepared.has(utilRel)) prepare(utilRel);
    out = must(
      out,
      'import { flattenedEmojiTable } from "../../Util";',
      `import { flattenedEmojiTable } from "./${prepared.get(utilRel)}";`,
      "NationEmoji Util import",
    );
    out = must(
      out,
      "  constructor(\n" +
        "    private random: PseudoRandom,\n" +
        "    private game: Game,\n" +
        "    private player: Player,\n" +
        "  ) {}",
      "  private random: PseudoRandom;\n" +
        "  private game: Game;\n" +
        "  private player: Player;\n\n" +
        "  constructor(\n" +
        "    random: PseudoRandom,\n" +
        "    game: Game,\n" +
        "    player: Player,\n" +
        "  ) {\n" +
        "    this.random = random;\n" +
        "    this.game = game;\n" +
        "    this.player = player;\n" +
        "  }",
      "NationEmoji ctor",
    );
  }

  if (rel.endsWith("pathfinding/PathFinder.ts")) {
    // Only WaterPathMemo is captured; everything else in the file rides on the
    // Game facade. Every import here is either type-only (Game, GameMap/TileRef,
    // TrainStation, the ./types names — PathStatus is an enum the strip loader
    // rejects, but it is only referenced inside tileStepperConfig, which never
    // runs) or a *value* import of the pathfinding graph (AStar.Rail,
    // AStar.Water, PathFinder.Air/Parabola/Station, PathFinderBuilder,
    // PathFinderStepper, the four transformers) that strip mode would really
    // execute and pull in whole. WaterPathMemo's class body references none of
    // them, so the entire import block is dropped. The module-level
    // `_waterChainCache = new WeakMap(...)` still executes (global WeakMap,
    // type args erased) and is harmless. The ctor is parameter properties,
    // which strip mode rejects; expand them, same as MinHeap / BFS — the
    // maxBytes default keeps referencing WaterPathMemo.DEFAULT_MAX_BYTES.
    out = must(
      out,
      'import { Game } from "../game/Game";\n' +
        'import { GameMap, TileRef } from "../game/GameMap";\n' +
        'import { TrainStation } from "../game/TrainStation";\n' +
        'import { AStarRail } from "./algorithms/AStar.Rail";\n' +
        'import { AStarWater } from "./algorithms/AStar.Water";\n' +
        'import { AirPathFinder } from "./PathFinder.Air";\n' +
        "import {\n" +
        "  ParabolaOptions,\n" +
        "  ParabolaUniversalPathFinder,\n" +
        '} from "./PathFinder.Parabola";\n' +
        'import { StationPathFinder } from "./PathFinder.Station";\n' +
        'import { PathFinderBuilder } from "./PathFinderBuilder";\n' +
        'import { PathFinderStepper, StepperConfig } from "./PathFinderStepper";\n' +
        'import { ComponentCheckTransformer } from "./transformers/ComponentCheckTransformer";\n' +
        'import { MiniMapTransformer } from "./transformers/MiniMapTransformer";\n' +
        'import { ShoreCoercingTransformer } from "./transformers/ShoreCoercingTransformer";\n' +
        'import { SmoothingWaterTransformer } from "./transformers/SmoothingWaterTransformer";\n' +
        "import {\n" +
        "  PathFinder,\n" +
        "  PathResult,\n" +
        "  PathStatus,\n" +
        "  SteppingPathFinder,\n" +
        '} from "./types";\n',
      "",
      "PathFinder imports",
    );
    out = must(
      out,
      "  constructor(\n" +
        "    private readonly inner: PathFinder<TileRef>,\n" +
        "    private readonly numTiles: number,\n" +
        "    /** The map's waterVersion() — every live water conversion advances it. */\n" +
        "    private readonly currentWaterVersion: () => number,\n" +
        "    /** Live cache budget; the default fits the client worker. Tests shrink it to reach eviction. */\n" +
        "    private readonly maxBytes: number = WaterPathMemo.DEFAULT_MAX_BYTES,\n" +
        "  ) {\n" +
        "    this.waterVersion = currentWaterVersion();\n" +
        "  }",
      "  private readonly inner: PathFinder<TileRef>;\n" +
        "  private readonly numTiles: number;\n" +
        "  private readonly currentWaterVersion: () => number;\n" +
        "  private readonly maxBytes: number;\n" +
        "  constructor(\n" +
        "    inner: PathFinder<TileRef>,\n" +
        "    numTiles: number,\n" +
        "    currentWaterVersion: () => number,\n" +
        "    maxBytes: number = WaterPathMemo.DEFAULT_MAX_BYTES,\n" +
        "  ) {\n" +
        "    this.inner = inner;\n" +
        "    this.numTiles = numTiles;\n" +
        "    this.currentWaterVersion = currentWaterVersion;\n" +
        "    this.maxBytes = maxBytes;\n" +
        "    this.waterVersion = currentWaterVersion();\n" +
        "  }",
      "WaterPathMemo ctor",
    );
    // WaterPathFinder never runs, but its ctor is parameter properties too and
    // strip mode parses the whole file — expand it the same way.
    out = must(
      out,
      "  constructor(\n" +
        "    private game: Game,\n" +
        "    private _stagger: number = 0,\n" +
        "    private readonly _memoized: boolean = false,\n" +
        "  ) {\n" +
        "    this.stepper = new PathFinderStepper(\n" +
        "      sharedWaterChain(game, _memoized),\n" +
        "      tileStepperConfig(game),\n" +
        "    );",
      "  private game: Game;\n" +
        "  private _stagger: number;\n" +
        "  private readonly _memoized: boolean;\n" +
        "  constructor(\n" +
        "    game: Game,\n" +
        "    _stagger: number = 0,\n" +
        "    _memoized: boolean = false,\n" +
        "  ) {\n" +
        "    this.game = game;\n" +
        "    this._stagger = _stagger;\n" +
        "    this._memoized = _memoized;\n" +
        "    this.stepper = new PathFinderStepper(\n" +
        "      sharedWaterChain(game, _memoized),\n" +
        "      tileStepperConfig(game),\n" +
        "    );",
      "WaterPathFinder ctor",
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
