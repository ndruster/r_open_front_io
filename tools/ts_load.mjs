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

  if (rel.endsWith("game/UnitGrid.ts")) {
    // UnitView / PlayerID / Unit / UnitType / GameMap / TileRef are all
    // type-only here (the grid only calls methods on the injected GameMap and
    // stores units by reference; UnitType appears only in erased annotations
    // and type arguments). Dropping the imports keeps the Game / GameMap /
    // client-view value graphs out of strip mode. The ctor uses a parameter
    // property -> expand. The three private helpers the capture exercises
    // (isValidCell / getCellsInRange / squaredDistanceFromTile) are exported
    // for the parity harness (the Rust twin exposes them for the same reason).
    out = must(out, 'import { UnitView } from "../../client/view";\n', "", "UnitGrid UnitView import");
    out = must(out, 'import { PlayerID, Unit, UnitType } from "./Game";\n', "", "UnitGrid Game import");
    out = must(out, 'import { GameMap, TileRef } from "./GameMap";\n', "", "UnitGrid GameMap import");
    out = must(
      out,
      "  constructor(private gm: GameMap) {",
      "  private gm: GameMap;\n\n  constructor(gm: GameMap) {\n    this.gm = gm;",
      "UnitGrid ctor",
    );
    out = must(out, "  private isValidCell(", "  isValidCell(", "UnitGrid isValidCell export");
    out = must(out, "  private getCellsInRange(", "  getCellsInRange(", "UnitGrid getCellsInRange export");
    out = must(out, "  private squaredDistanceFromTile(", "  squaredDistanceFromTile(", "UnitGrid squaredDistanceFromTile export");
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

  if (rel.endsWith("game/StatsImpl.ts")) {
    // AllPlayersStats / ClientID (Schemas), Player / TerraNullius (Game) and
    // Stats are type-only uses (annotations / the implements clause) -> the
    // imports are dropped. The StatsSchemas block is a *value* import (the
    // index constants and the two lookup tables) but it also names four types
    // (BoatUnit / NukeType / OtherUnitType / PlayerStats) that appear only in
    // erased annotations; strip mode keeps bare named imports at runtime, so
    // the type lines are dropped and the specifier redirected to the prepared
    // copy (which exports every value, with inert zod/zb shims). PlayerType /
    // UnitType are *value* uses (the conquest_by_type computed keys, the MIRV
    // check and the recordKill filter) and ride on the prepared Game.ts copy
    // (enums inlined as plain objects). The `type BigIntLike` alias and every
    // annotation are erased by strip mode; the `satisfies PlayerStats` clause
    // is erasable syntax.
    out = must(
      out,
      'import { AllPlayersStats, ClientID } from "../Schemas";\n',
      "",
      "StatsImpl Schemas import",
    );
    for (const t of ["BoatUnit", "NukeType", "OtherUnitType", "PlayerStats"]) {
      out = must(out, `  ${t},\n`, "", `StatsImpl StatsSchemas type ${t}`);
    }
    const siStatsRel = "src/core/StatsSchemas.ts";
    if (!prepared.has(siStatsRel)) prepare(siStatsRel);
    out = must(
      out,
      '} from "../StatsSchemas";\n',
      `} from "./${prepared.get(siStatsRel)}";\n`,
      "StatsImpl StatsSchemas import",
    );
    if (!prepared.has("src/core/game/Game.ts")) prepare("src/core/game/Game.ts");
    out = must(
      out,
      'import { Player, PlayerType, TerraNullius, UnitType } from "./Game";\n' +
        'import { Stats } from "./Stats";\n',
      `import { PlayerType, UnitType } from "./${prepared.get("src/core/game/Game.ts")}";\n`,
      "StatsImpl Game/Stats imports",
    );
    out = must(
      out,
      "export class StatsImpl implements Stats {",
      "export class StatsImpl {",
      "StatsImpl implements clause",
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

  if (rel.endsWith("execution/nation/SharedWaterCache.ts")) {
    // PlayerType is a *value* use (`player.type() === PlayerType.Bot` — the
    // Game.ts string enum, Bot = "BOT") and rides on the prepared Game.ts
    // copy (enum inlined as a plain object); Game / Player are type-only
    // annotations erased by strip mode -> dropped outright (the capture feeds
    // scripted mocks for the facades, same pattern as NationUtils). The ctor's
    // `private game: Game` parameter property is expanded for strip mode.
    if (!prepared.has("src/core/game/Game.ts")) prepare("src/core/game/Game.ts");
    out = must(
      out,
      'import { Game, Player, PlayerType } from "../../game/Game";\n',
      `import { PlayerType } from "./${prepared.get("src/core/game/Game.ts")}";\n`,
      "SharedWaterCache imports",
    );
    out = must(
      out,
      "  constructor(private game: Game) {}",
      "  private game: Game;\n\n  constructor(game: Game) { this.game = game; }",
      "SharedWaterCache ctor",
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

  if (rel.endsWith("execution/ExecutionManager.ts")) {
    // The Executor dispatcher is captured against a scripted Game facade;
    // the 24 `XxxExecution` classes + TribeSpawner / PlayerSpawner are NOT
    // ported (existing exclusion decision) and are replaced by construction
    // recorders (precedent: nation_utils' facade mocks + trace pinning). The
    // whole import block is swapped for the recorder definitions plus the two
    // *value* imports the class body really executes — PseudoRandom (direct
    // real module, no imports) and simpleHash (redirected to the prepared
    // Util.ts copy, same pattern as TeamAssignment). The Execution / Game /
    // ClientID / GameID / StampedIntent / Turn / SpawnExecution names are
    // type-only annotations erased by strip mode. console.warn stays (the
    // capture hooks globalThis console.warn to pin the message).
    const emUtilRel = "src/core/Util.ts";
    if (!prepared.has(emUtilRel)) prepare(emUtilRel);
    out = must(
      out,
      'import { Execution, Game } from "../game/Game";\n' +
        'import { PseudoRandom } from "../PseudoRandom";\n' +
        'import { ClientID, GameID, StampedIntent, Turn } from "../Schemas";\n' +
        'import { simpleHash } from "../Util";\n' +
        'import { AllianceExtensionExecution } from "./alliance/AllianceExtensionExecution";\n' +
        'import { AllianceRejectExecution } from "./alliance/AllianceRejectExecution";\n' +
        'import { AllianceRequestExecution } from "./alliance/AllianceRequestExecution";\n' +
        'import { BreakAllianceExecution } from "./alliance/BreakAllianceExecution";\n' +
        'import { AttackExecution } from "./AttackExecution";\n' +
        'import { BoatRetreatExecution } from "./BoatRetreatExecution";\n' +
        'import { ConstructionExecution } from "./ConstructionExecution";\n' +
        'import { DeleteUnitExecution } from "./DeleteUnitExecution";\n' +
        'import { DonateGoldExecution } from "./DonateGoldExecution";\n' +
        'import { DonateTroopsExecution } from "./DonateTroopExecution";\n' +
        'import { EmbargoAllExecution } from "./EmbargoAllExecution";\n' +
        'import { EmbargoExecution } from "./EmbargoExecution";\n' +
        'import { EmojiExecution } from "./EmojiExecution";\n' +
        'import { MarkDisconnectedExecution } from "./MarkDisconnectedExecution";\n' +
        'import { MoveWarshipExecution } from "./MoveWarshipExecution";\n' +
        'import { NationExecution } from "./NationExecution";\n' +
        'import { NoOpExecution } from "./NoOpExecution";\n' +
        'import { PauseExecution } from "./PauseExecution";\n' +
        'import { QuickChatExecution } from "./QuickChatExecution";\n' +
        'import { RetreatExecution } from "./RetreatExecution";\n' +
        'import { SpawnExecution } from "./SpawnExecution";\n' +
        'import { TargetPlayerExecution } from "./TargetPlayerExecution";\n' +
        'import { TransportShipExecution } from "./TransportShipExecution";\n' +
        'import { TribeSpawner } from "./TribeSpawner";\n' +
        'import { UpgradeStructureExecution } from "./UpgradeStructureExecution";\n' +
        'import { PlayerSpawner } from "./utils/PlayerSpawner";\n',
      "import { PseudoRandom } from \"" + TS_URL + "src/core/PseudoRandom.ts\";\n" +
        `import { simpleHash } from "./${prepared.get(emUtilRel)}";\n` +
        "const __enc = (v) => {\n" +
        "  if (v === undefined) return [0];\n" +
        "  if (v === null) return [1];\n" +
        "  if (v === true) return [2];\n" +
        "  if (v === false) return [3];\n" +
        "  if (typeof v === \"number\") return [4, v];\n" +
        "  if (typeof v === \"string\")\n" +
        "    return [5, v.length, ...Array.from({ length: v.length }, (_, i) => v.charCodeAt(i))];\n" +
        "  if (Array.isArray(v)) return [6, v.length, ...v.flatMap(__enc)];\n" +
        "  if (v && v.__nation) return [12, v.__ref];\n" +
        "  if (v && v.__info !== undefined) return [8, v.__info];\n" +
        "  if (v && v.__ref !== undefined) return [7, v.__ref];\n" +
        "  throw new Error(\"em: unencodable \" + String(v));\n" +
        "};\n" +
        "const __ev = (a) => globalThis.__EMTRACE.push(...a.flat(Infinity));\n" +
        "const __stub = (tag) =>\n" +
        "  class {\n" +
        "    constructor(...a) {\n" +
        "      __ev([2, tag, ...a.flatMap(__enc)]);\n" +
        "      return { __ref: ++globalThis.__EMREF };\n" +
        "    }\n" +
        "  };\n" +
        "const NoOpExecution = __stub(0);\n" +
        "const AttackExecution = __stub(1);\n" +
        "const RetreatExecution = __stub(2);\n" +
        "const BoatRetreatExecution = __stub(3);\n" +
        "const MoveWarshipExecution = __stub(4);\n" +
        "const SpawnExecution = __stub(5);\n" +
        "const TransportShipExecution = __stub(6);\n" +
        "const AllianceRequestExecution = __stub(7);\n" +
        "const AllianceRejectExecution = __stub(8);\n" +
        "const BreakAllianceExecution = __stub(9);\n" +
        "const TargetPlayerExecution = __stub(10);\n" +
        "const EmojiExecution = __stub(11);\n" +
        "const DonateTroopsExecution = __stub(12);\n" +
        "const DonateGoldExecution = __stub(13);\n" +
        "const EmbargoExecution = __stub(14);\n" +
        "const EmbargoAllExecution = __stub(15);\n" +
        "const ConstructionExecution = __stub(16);\n" +
        "const AllianceExtensionExecution = __stub(17);\n" +
        "const UpgradeStructureExecution = __stub(18);\n" +
        "const DeleteUnitExecution = __stub(19);\n" +
        "const QuickChatExecution = __stub(20);\n" +
        "const MarkDisconnectedExecution = __stub(21);\n" +
        "const PauseExecution = __stub(22);\n" +
        "const NationExecution = __stub(23);\n" +
        "const TribeSpawner = class {\n" +
        "  constructor(mg, gameID, cells) {\n" +
        "    __ev([6, ...__enc(gameID), ...__enc(cells)]);\n" +
        "  }\n" +
        "  spawnTribes(n, names) {\n" +
        "    const ret = globalThis.__EMRET.shift();\n" +
        "    __ev([7, ...__enc(n), ...__enc(names), ret.length, ...ret]);\n" +
        "    return ret.map((r) => ({ __ref: r }));\n" +
        "  }\n" +
        "};\n" +
        "const PlayerSpawner = class {\n" +
        "  constructor(mg, gameID) {\n" +
        "    __ev([8, ...__enc(gameID)]);\n" +
        "  }\n" +
        "  spawnPlayers() {\n" +
        "    const ret = globalThis.__EMRET.shift();\n" +
        "    __ev([9, ret.length, ...ret]);\n" +
        "    return ret.map((r) => ({ __ref: r }));\n" +
        "  }\n" +
        "};\n",
      "ExecutionManager imports",
    );
    // Parameter properties (strip mode rejects them, same as MinHeap / BFS):
    // expand the four ctor fields, keeping the `= []` default on the last one.
    out = must(
      out,
      "  constructor(\n" +
        "    private mg: Game,\n" +
        "    private gameID: GameID,\n" +
        "    private clientID: ClientID | undefined,\n" +
        "    // Purchased bot tribe names drawn for this game (GameStartInfo.tribes).\n" +
        "    private purchasedTribeNames: string[] = [],\n" +
        "  ) {",
      "  private mg: any;\n" +
        "  private gameID: any;\n" +
        "  private clientID: any;\n" +
        "  private purchasedTribeNames: any;\n\n" +
        "  constructor(\n" +
        "    mg: any,\n" +
        "    gameID: any,\n" +
        "    clientID: any,\n" +
        "    // Purchased bot tribe names drawn for this game (GameStartInfo.tribes).\n" +
        "    purchasedTribeNames: any = [],\n" +
        "  ) {\n" +
        "    this.mg = mg;\n" +
        "    this.gameID = gameID;\n" +
        "    this.clientID = clientID;\n" +
        "    this.purchasedTribeNames = purchasedTribeNames;",
      "ExecutionManager ctor",
    );
    // The `(c): c is NonNullable<typeof c>` type-predicate arrow is beyond the
    // strip loader's erasure here; rewrite it to the plain boolean arrow (the
    // `c !== undefined` runtime test is untouched).
    out = must(
      out,
      "      .filter((c): c is NonNullable<typeof c> => c !== undefined);",
      "      .filter((c) => c !== undefined);",
      "ExecutionManager filter predicate",
    );
  }

  if (rel.endsWith("game/TrainStation.ts")) {
    // TrainStation + Cluster are captured against scripted Unit / Player /
    // Game mocks (precedent: nation_utils / shared_water_cache). GameUpdateType
    // (the RailroadDestructionEvent lookup in removeNeighboringRails) and
    // UnitType (the City / Port comparisons in Cluster.isTradeStation) are
    // *value* uses and redirect to the prepared GameUpdates.ts / Game.ts
    // copies (enums inlined as plain objects). TrainExecution / Game / Player /
    // Unit / TileRef / Railroad are type-only annotations erased by strip mode
    // -> dropped. PseudoRandom is dropped with the ctor's stopHandlers line:
    // the excluded `createTrainStopHandlers(new PseudoRandom(mg.ticks()))`
    // side effect is stripped (TrainStation::new must NOT replicate it); the
    // function declaration itself survives untouched (never called). The
    // `constructor(private mg: Game, public unit: Unit)` parameter properties
    // are expanded for strip mode.
    if (!prepared.has("src/core/game/Game.ts")) prepare("src/core/game/Game.ts");
    const tsGuRel = "src/core/game/GameUpdates.ts";
    if (!prepared.has(tsGuRel)) prepare(tsGuRel);
    out = must(
      out,
      'import { TrainExecution } from "../execution/TrainExecution";\n' +
        'import { PseudoRandom } from "../PseudoRandom";\n' +
        'import { Game, Player, Unit, UnitType } from "./Game";\n' +
        'import { TileRef } from "./GameMap";\n' +
        'import { GameUpdateType } from "./GameUpdates";\n' +
        'import { Railroad } from "./Railroad";\n',
      `import { UnitType } from "./${prepared.get("src/core/game/Game.ts")}";\n` +
        `import { GameUpdateType } from "./${prepared.get(tsGuRel)}";\n`,
      "TrainStation imports",
    );
    out = must(
      out,
      "  constructor(\n" +
        "    private mg: Game,\n" +
        "    public unit: Unit,\n" +
        "  ) {\n" +
        "    this.stopHandlers = createTrainStopHandlers(new PseudoRandom(mg.ticks()));\n" +
        "  }",
      "  mg: Game;\n" +
        "  unit: Unit;\n\n" +
        "  constructor(\n" +
        "    mg: Game,\n" +
        "    unit: Unit,\n" +
        "  ) {\n" +
        "    this.mg = mg;\n" +
        "    this.unit = unit;\n" +
        "  }",
      "TrainStation ctor",
    );
  }

  if (rel.endsWith("game/RailNetworkImpl.ts")) {
    // P55 captures the REAL RailNetworkImpl, so the identifiers its method
    // bodies use *as values* must resolve: UnitType (the City/Port/Factory
    // guard comparisons) -> prepared Game.ts, GameUpdateType (the
    // RailroadSnap/ConstructionEvent stamps) -> prepared GameUpdates.ts,
    // Railroad -> prepared Railroad.ts, RailSpatialGrid -> prepared
    // RailroadSpatialGrid.ts, Cluster + TrainStation -> prepared
    // TrainStation.ts. PathFinding stays dropped (RailPathFinderServiceImpl
    // is never instantiated; strip mode does no name resolution, so the
    // undefined identifier inside its body is harmless), as do RailNetwork /
    // Game / Unit / TileRef (interface / type-only annotations erased by
    // strip mode). The `implements` clauses and interface declarations are
    // type-only and erased.
    const rnGameRel = "src/core/game/Game.ts";
    if (!prepared.has(rnGameRel)) prepare(rnGameRel);
    const rnGuRel = "src/core/game/GameUpdates.ts";
    if (!prepared.has(rnGuRel)) prepare(rnGuRel);
    const rnRrRel = "src/core/game/Railroad.ts";
    if (!prepared.has(rnRrRel)) prepare(rnRrRel);
    const rnRsgRel = "src/core/game/RailroadSpatialGrid.ts";
    if (!prepared.has(rnRsgRel)) prepare(rnRsgRel);
    const rnTsRel = "src/core/game/TrainStation.ts";
    if (!prepared.has(rnTsRel)) prepare(rnTsRel);
    out = must(
      out,
      'import { PathFinding } from "../pathfinding/PathFinder";\n' +
        'import { Game, Unit, UnitType } from "./Game";\n' +
        'import { TileRef } from "./GameMap";\n' +
        'import { GameUpdateType } from "./GameUpdates";\n' +
        'import { RailNetwork } from "./RailNetwork";\n' +
        'import { Railroad } from "./Railroad";\n' +
        'import { RailSpatialGrid } from "./RailroadSpatialGrid";\n' +
        'import { Cluster, TrainStation } from "./TrainStation";\n',
      `import { UnitType } from "./${prepared.get(rnGameRel)}";\n` +
        `import { GameUpdateType } from "./${prepared.get(rnGuRel)}";\n` +
        `import { Railroad } from "./${prepared.get(rnRrRel)}";\n` +
        `import { RailSpatialGrid } from "./${prepared.get(rnRsgRel)}";\n` +
        `import { Cluster, TrainStation } from "./${prepared.get(rnTsRel)}";\n`,
      "RailNetworkImpl imports",
    );
    // Parameter properties (strip mode rejects them, same as ExecutionManager
    // / SharedWaterCache): expand both ctors. RailPathFinderServiceImpl is
    // still never instantiated (the capture injects a scripted pathService
    // mock), but the class *declarations* must parse and RailNetworkImpl is
    // constructed by the P55 capture.
    out = must(
      out,
      "class RailPathFinderServiceImpl implements RailPathFinderService {\n" +
        "  constructor(private game: Game) {}",
      "class RailPathFinderServiceImpl implements RailPathFinderService {\n" +
        "  game: Game;\n\n" +
        "  constructor(game: Game) { this.game = game; }",
      "RailPathFinderServiceImpl ctor",
    );
    out = must(
      out,
      "  constructor(\n" +
        "    private game: Game,\n" +
        "    private _stationManager: StationManager,\n" +
        "    private pathService: RailPathFinderService,\n" +
        "  ) {",
      "  game: Game;\n" +
        "  _stationManager: StationManager;\n" +
        "  pathService: RailPathFinderService;\n\n" +
        "  constructor(\n" +
        "    game: Game,\n" +
        "    _stationManager: StationManager,\n" +
        "    pathService: RailPathFinderService,\n" +
        "  ) {\n" +
        "    this.game = game;\n" +
        "    this._stationManager = _stationManager;\n" +
        "    this.pathService = pathService;",
      "RailNetworkImpl ctor",
    );
  }

  if (rel.endsWith("server/VoteTally.ts")) {
    // No imports at all; the class body is plain JS after strip mode
    // erases the generics and annotations. Nothing to rewrite.
  }
  if (rel.endsWith("server/ConfigPatch.ts")) {
    // GameConfig is a zod-schema-derived type (type-only) -> dropped; the
    // `as const satisfies readonly (keyof GameConfig)[]` clauses are erased
    // by strip mode, leaving the plain string arrays.
    out = must(out, 'import { GameConfig } from "../core/Schemas";\n', "", "ConfigPatch Schemas import");
  }
  if (rel.endsWith("server/IntentAuthorization.ts")) {
    // ClientID / Intent are branded types / interfaces (type-only) ->
    // dropped. GameType is a *value* (the string enum) and hostCheatsEnabled
    // a *value* import, both redirected to prepared copies.
    const iaGameRel = "src/core/game/Game.ts";
    if (!prepared.has(iaGameRel)) prepare(iaGameRel);
    const iaCpRel = "src/server/ConfigPatch.ts";
    if (!prepared.has(iaCpRel)) prepare(iaCpRel);
    out = must(
      out,
      'import { GameType } from "../core/game/Game";\n' +
        'import { ClientID, Intent } from "../core/Schemas";\n' +
        'import { hostCheatsEnabled } from "./ConfigPatch";\n',
      `import { GameType } from "./${prepared.get(iaGameRel)}";\n` +
        `import { hostCheatsEnabled } from "./${prepared.get(iaCpRel)}";\n`,
      "IntentAuthorization imports",
    );
  }
  if (rel.endsWith("server/Consensus.ts")) {
    // ClientID / ClientSendWinnerMessage / LiveStats are branded types /
    // interfaces (type-only) -> dropped. VoteRound is a *value* import,
    // redirected to the prepared VoteTally copy.
    const cvSchemasRel = 'import { ClientID, ClientSendWinnerMessage, LiveStats } from "../core/Schemas";\n';
    out = must(out, cvSchemasRel, "", "Consensus Schemas import");
    const cvVtRel = "src/server/VoteTally.ts";
    if (!prepared.has(cvVtRel)) prepare(cvVtRel);
    out = must(
      out,
      'import { VoteRound } from "./VoteTally";',
      `import { VoteRound } from "./${prepared.get(cvVtRel)}";`,
      "Consensus VoteTally import",
    );
  }

  if (rel.endsWith("server/ListingState.ts")) {
    // LobbyAccent is a zod-inferred type (type-only) -> dropped; the two
    // numeric constants are *value* imports redirected to the prepared
    // Schemas copy, sanitizeLobbyLabel to the prepared Util copy. Date.now()
    // is the module's only impure call: the capture scripts it through
    // globalThis.__LISTING_NOW (set per setListed op).
    const lsSchemasRel = "src/core/Schemas.ts";
    if (!prepared.has(lsSchemasRel)) prepare(lsSchemasRel);
    const lsUtilRel = "src/core/Util.ts";
    if (!prepared.has(lsUtilRel)) prepare(lsUtilRel);
    out = must(
      out,
      'import {\n' +
        "  FEATURED_LOBBY_AUTO_START_MS,\n" +
        "  HOSTED_LOBBY_AUTO_START_MS,\n" +
        "  LobbyAccent,\n" +
        '} from "../core/Schemas";\n' +
        'import { sanitizeLobbyLabel } from "../core/Util";\n',
      "import {\n" +
        "  FEATURED_LOBBY_AUTO_START_MS,\n" +
        "  HOSTED_LOBBY_AUTO_START_MS,\n" +
        `} from "./${prepared.get(lsSchemasRel)}";\n` +
        `import { sanitizeLobbyLabel } from "./${prepared.get(lsUtilRel)}";\n`,
      "ListingState imports",
    );
    out = must(
      out,
      "this.listedAt = listed ? Date.now() : undefined;",
      "this.listedAt = listed ? globalThis.__LISTING_NOW : undefined;",
      "ListingState Date.now",
    );
  }

  if (rel.endsWith("server/MapPlaylist.ts")) {
    // S3: the deterministic layer. SAM_CONSTRUCTION_TICKS is inlined (the
    // heavy Config.ts graph stays unprepared; the value `30 * 10` is pinned
    // by the upstream sync check). The Game.ts import keeps only the *value*
    // members the executed code paths need (allMaps / GameMapType / GameMode
    // / the four TeamCountConfig presets) and rides on the prepared copy
    // (whose enums are inlined plain objects and which re-exports `maps`
    // from the prepared Maps.gen); Difficulty / GameMapSize / GameType /
    // RankedType / UnitType are only referenced inside the S4-excluded
    // methods, whose unresolved identifiers never execute (strip mode is
    // syntax-only). GameConfig / ScheduledPublicGameType / TeamCountConfig
    // are zod-inferred types -> dropped. The ./Logger import is dropped and
    // `log` becomes a sink that records the info/warn MESSAGES into
    // globalThis.__MP_LOG, so the generateNewPlaylist attempt count (carried
    // by the log text) is pinned as an observable. The ./MapLandTiles import
    // is dropped (only the S4 methods call getMapLandTiles). Date.now() is
    // scripted through globalThis.__MP_SEED (precedent: ListingState). The
    // module-private tables are exported so the capture can dump them
    // verbatim.
    const mpGameRel = "src/core/game/Game.ts";
    if (!prepared.has(mpGameRel)) prepare(mpGameRel);
    out = must(
      out,
      'import { SAM_CONSTRUCTION_TICKS } from "../core/configuration/Config";\n' +
        "import {\n" +
        "  maps as allMaps,\n" +
        "  Difficulty,\n" +
        "  Duos,\n" +
        "  GameMapSize,\n" +
        "  GameMapType,\n" +
        "  GameMode,\n" +
        "  GameType,\n" +
        "  HumansVsNations,\n" +
        "  PublicGameModifiers,\n" +
        "  Quads,\n" +
        "  RankedType,\n" +
        "  Trios,\n" +
        "  UnitType,\n" +
        '} from "../core/game/Game";\n' +
        'import { PseudoRandom } from "../core/PseudoRandom";\n' +
        "import {\n" +
        "  GameConfig,\n" +
        "  ScheduledPublicGameType,\n" +
        "  TeamCountConfig,\n" +
        '} from "../core/Schemas";\n' +
        'import { logger } from "./Logger";\n' +
        'import { getMapLandTiles } from "./MapLandTiles";\n' +
        "\n" +
        "const log = logger.child({});\n",
      "const SAM_CONSTRUCTION_TICKS = 30 * 10;\n" +
        "import {\n" +
        "  maps as allMaps,\n" +
        "  Difficulty,\n" +
        "  Duos,\n" +
        "  GameMapSize,\n" +
        "  GameMapType,\n" +
        "  GameMode,\n" +
        "  GameType,\n" +
        "  HumansVsNations,\n" +
        "  Quads,\n" +
        "  RankedType,\n" +
        "  Trios,\n" +
        `} from "./${prepared.get(mpGameRel)}";\n` +
        `import { PseudoRandom } from "${TS_URL}src/core/PseudoRandom.ts";\n` +
        "\n" +
        "const log = {\n" +
        "  info: (m) => ((globalThis.__MP_LOG ||= []).push(m), m),\n" +
        "  warn: (m) => ((globalThis.__MP_LOG ||= []).push(m), m),\n" +
        "};\n" +
        "const getMapLandTiles = (map) => globalThis.__MP_LAND_FACADE(map);\n",
      "MapPlaylist imports",
    );
    out = must(
      out,
      "const rand = new PseudoRandom(Date.now());",
      "const rand = new PseudoRandom(globalThis.__MP_SEED);",
      "MapPlaylist Date.now",
    );
    // S4: Math.random is scripted through globalThis.__MP_RAND (a capture-
    // provided queue popper that logs each consumed value, precedent:
    // NationUtils). Every call site (getSpecialConfig /
    // getRandomSpecialGameModifiers included) is rewritten; the excluded
    // methods never execute, so their rewritten sites are inert.
    out = must(
      out,
      "Math.random()",
      "globalThis.__MP_RAND()",
      "MapPlaylist Math.random",
    );
    for (const c of [
      "const CROWDED_COMPACT_PLAYER_COUNT = 60;",
      "const CROWDED_PLAYER_COUNT = 125;",
      "const TRUSTED_PUBLIC_EVERY = 7;",
      "const TRUSTED_MAX_PLAYER_COUNT = 25;",
      "const TEAM_WEIGHTS:",
      "const SPECIAL_TEAM_FORCE_CHANCE = 0.75;",
      "const SPECIAL_TEAM_MAPS:",
      "const SPECIAL_MODIFIER_POOL:",
      "const DOOMSDAY_ROTATION_SPEEDS =",
      "const MUTUALLY_EXCLUSIVE_MODIFIERS:",
    ]) {
      out = must(out, c, `export ${c}`, `MapPlaylist export ${c.slice(6, 26)}`);
    }
  }

  if (rel.endsWith("server/NameVisibility.ts")) {
    // ClientID / GameConfig / GameInfo / GameStartInfo are branded types /
    // zod-inferred types (type-only) -> dropped, as is the Client class
    // import (the capture feeds plain JS stub objects; NameVisibility only
    // reads fields). anonWordName / GameMode / simpleHash are *value*
    // imports redirected to prepared copies (AnonNames has no imports and
    // loads unchanged). The parameter-property ctor is expanded (strip mode
    // rejects it, precedent: ExecutionManager / RailNetworkImpl).
    const nvAnonRel = "src/core/AnonNames.ts";
    if (!prepared.has(nvAnonRel)) prepare(nvAnonRel);
    const nvGameRel = "src/core/game/Game.ts";
    if (!prepared.has(nvGameRel)) prepare(nvGameRel);
    const nvUtilRel = "src/core/Util.ts";
    if (!prepared.has(nvUtilRel)) prepare(nvUtilRel);
    out = must(
      out,
      'import { anonWordName } from "../core/AnonNames";\n' +
        'import { GameMode } from "../core/game/Game";\n' +
        'import { ClientID, GameConfig, GameInfo, GameStartInfo } from "../core/Schemas";\n' +
        'import { simpleHash } from "../core/Util";\n' +
        'import { Client } from "./Client";\n',
      `import { anonWordName } from "./${prepared.get(nvAnonRel)}";\n` +
        `import { GameMode } from "./${prepared.get(nvGameRel)}";\n` +
        `import { simpleHash } from "./${prepared.get(nvUtilRel)}";\n`,
      "NameVisibility imports",
    );
    out = must(
      out,
      "export class NameVisibility {\n" +
        "  constructor(private readonly view: NameVisibilityView) {}",
      "export class NameVisibility {\n" +
        "  view;\n\n" +
        "  constructor(view) {\n" +
        "    this.view = view;\n" +
        "  }",
      "NameVisibility ctor",
    );
  }

  if (rel.endsWith("server/DesyncDetector.ts")) {
    // ClientID is a branded type and Client is only referenced in type
    // annotations (`readonly Client[]`, the Set<ClientID> generics) — both
    // type-only -> dropped (the capture feeds plain client stubs; strip mode
    // erases the annotations, so the bodies never name the imports).
    out = must(
      out,
      'import { ClientID } from "../core/Schemas";\n' +
        'import { Client } from "./Client";\n',
      "",
      "DesyncDetector imports",
    );
  }

  if (rel.endsWith("server/JoinVerify.ts")) {
    // The zod verdict schema is wire-validation for the EXCLUDED fetch-I/O
    // verifyJoin; the inert self-returning Proxy (precedent: ServerList)
    // keeps the top-level discriminatedUnion declaration evaluating. The
    // TokenPayload import is `import type` (erased by strip mode) and the
    // ServerEnv import is only used inside verifyJoin's body, which never
    // executes -> drop the import (unresolved identifiers inside an
    // uncalled body are harmless; strip mode is syntax-only).
    out = must(
      out,
      'import { z } from "zod";\n',
      "const z = new Proxy(function () {}, { get: () => z, apply: () => z });\n",
      "JoinVerify zod import",
    );
    out = must(
      out,
      'import { ServerEnv } from "./ServerEnv";\n',
      "",
      "JoinVerify ServerEnv import",
    );
  }

  if (rel.endsWith("server/Censor.ts")) {
    // The obscenity library is unresolvable in the port repo, so the whole
    // import block is dropped and the module-level `profanityMatcher`
    // becomes the scripted facade globalThis.__CN_MATCHER (set by the
    // capture BEFORE loading: the const initializer runs at module load).
    // createMatcher / buildDataset stay in the file but are never called
    // (their unresolved obscenity identifiers sit in dead bodies; strip
    // mode is syntax-only). simpleHash is a *value* import redirected to
    // the prepared Util copy. bannedWords is module-private -> exported so
    // the capture can dump the table verbatim.
    const cnUtilRel = "src/core/Util.ts";
    if (!prepared.has(cnUtilRel)) prepare(cnUtilRel);
    out = must(
      out,
      'import {\n' +
        "  DataSet,\n" +
        "  RegExpMatcher,\n" +
        "  collapseDuplicatesTransformer,\n" +
        "  englishDataset,\n" +
        "  pattern,\n" +
        "  resolveConfusablesTransformer,\n" +
        "  resolveLeetSpeakTransformer,\n" +
        "  skipNonAlphabeticTransformer,\n" +
        "  toAsciiLowerCaseTransformer,\n" +
        '} from "obscenity";\n' +
        'import { simpleHash } from "../core/Util";\n',
      `import { simpleHash } from "./${prepared.get(cnUtilRel)}";\n`,
      "Censor imports",
    );
    out = must(
      out,
      "export const profanityMatcher = createMatcher();",
      "export const profanityMatcher = globalThis.__CN_MATCHER;",
      "Censor profanityMatcher facade",
    );
    out = must(out, "const bannedWords = [", "export const bannedWords = [", "Censor bannedWords export");
  }

  if (rel.endsWith("server/Privilege.ts")) {
    // S6: the countries.json bare-specifier import is unresolvable; the
    // module-level `countryCodes` becomes [] (isFlagAllowed is monkey-
    // patched by the capture, so the table is never read). The zod-inferred
    // type bindings from CosmeticSchemas / Schemas are erased (strip mode
    // erases their annotations; the value imports would pull the catalog
    // graph). findEffectForSlot / decodePatternData are *value* imports used
    // only inside the leaf methods the capture patches -> drop the imports
    // (unresolved identifiers sit in dead bodies; strip mode is syntax-only).
    // isTemporaryUsername is a real *value* import -> redirect to the
    // prepared ApiSchemas copy (its zod shim already loads). The ctor's
    // parameter properties are expanded (strip mode rejects them).
    const pvApiRel = "src/core/ApiSchemas.ts";
    if (!prepared.has(pvApiRel)) prepare(pvApiRel);
    out = must(
      out,
      'import countries from "resources/countries.json";\n',
      "const countries = [];\n",
      "Privilege countries import",
    );
    out = must(
      out,
      'import { isTemporaryUsername } from "../core/ApiSchemas";\n' +
        'import { Cosmetics, findEffectForSlot } from "../core/CosmeticSchemas";\n' +
        'import { decodePatternData } from "../core/PatternDecoder";\n' +
        "import {\n" +
        "  PlayerColor,\n" +
        "  PlayerCosmeticRefs,\n" +
        "  PlayerCosmetics,\n" +
        "  PlayerCrown,\n" +
        "  PlayerEffect,\n" +
        "  PlayerPattern,\n" +
        "  PlayerSkin,\n" +
        '} from "../core/Schemas";\n',
      `import { isTemporaryUsername } from "./${prepared.get(pvApiRel)}";\n`,
      "Privilege imports",
    );
    out = must(
      out,
      "export class PrivilegeCheckerImpl implements PrivilegeChecker {\n" +
        "  constructor(\n" +
        "    private cosmetics: Cosmetics,\n" +
        "    private b64urlDecode: (base64: string) => Uint8Array,\n" +
        "    // Every registered clan tag (uppercase). Polled by PrivilegeRefresher so\n" +
        "    // ownership is resolved in memory — no per-join existence probe.\n" +
        "    private reservedClanTags: Set<string> = new Set(),\n" +
        "  ) {}",
      "export class PrivilegeCheckerImpl {\n" +
        "  cosmetics;\n" +
        "  b64urlDecode;\n" +
        "  reservedClanTags;\n" +
        "  constructor(\n" +
        "    cosmetics,\n" +
        "    b64urlDecode,\n" +
        "    reservedClanTags = new Set(),\n" +
        "  ) {\n" +
        "    this.cosmetics = cosmetics;\n" +
        "    this.b64urlDecode = b64urlDecode;\n" +
        "    this.reservedClanTags = reservedClanTags;\n" +
        "  }",
      "Privilege ctor",
    );
  }

  if (rel.endsWith("server/Roster.ts")) {
    // S6: the `ws` package is unresolvable; the module-level constants
    // WebSocket.CONNECTING/OPEN/CLOSING/CLOSED (the real npm values) are
    // inlined so closeAll's `readyState === WebSocket.OPEN` gate still
    // evaluates against the capture's scripted ws stubs. CloseCodes is a
    // pure const-object module (no imports) -> load it for real so
    // CloseCode.Normal (1000) rides verbatim. Client / ClientID are type-only
    // (the capture feeds plain stubs; strip mode erases the annotations).
    const rsCloseRel = "src/core/CloseCodes.ts";
    if (!prepared.has(rsCloseRel)) prepare(rsCloseRel);
    out = must(
      out,
      'import WebSocket from "ws";\n' +
        'import { CloseCode, CloseReason } from "../core/CloseCodes";\n' +
        'import { ClientID } from "../core/Schemas";\n' +
        'import { Client } from "./Client";\n',
      "const WebSocket = { CONNECTING: 0, OPEN: 1, CLOSING: 2, CLOSED: 3 };\n" +
        `import { CloseCode, CloseReason } from "./${prepared.get(rsCloseRel)}";\n`,
      "Roster imports",
    );
  }

  if (rel.endsWith("server/MatchTelemetryRecorder.ts")) {
    // S6: Client is type-only (the capture feeds plain stubs); the telemetry
    // types are `import type` (erased by strip mode). Date.now() is scripted
    // through globalThis.__MT_NOW (precedent: ListingState / MapPlaylist).
    out = must(
      out,
      'import { Client } from "./Client";\n' +
        "import {\n" +
        "  type MatchTelemetryEmitter,\n" +
        "  type MatchTelemetryEvent,\n" +
        "  type MatchTelemetryPayloads,\n" +
        "  type MatchTelemetryType,\n" +
        "  type TelemetryPlayerIdentity,\n" +
        "} from \"./telemetry/MatchTelemetry\";\n",
      "",
      "MatchTelemetryRecorder imports",
    );
    out = must(out, "Date.now()", "globalThis.__MT_NOW()", "MatchTelemetryRecorder Date.now");
    out = must(
      out,
      "export class MatchTelemetryRecorder {\n" +
        "  private sequence = 0;\n" +
        "  private tickCounts = new Map<number, TickCounts>();\n" +
        "  private replayArchiveAttempted = false;\n" +
        "  private finished = false;\n\n" +
        "  constructor(\n" +
        "    private readonly emitter: MatchTelemetryEmitter,\n" +
        "    private readonly matchId: string,\n" +
        "    private readonly buildHash: string,\n" +
        "  ) {}",
      "export class MatchTelemetryRecorder {\n" +
        "  private sequence = 0;\n" +
        "  private tickCounts = new Map();\n" +
        "  private replayArchiveAttempted = false;\n" +
        "  private finished = false;\n" +
        "  emitter;\n" +
        "  matchId;\n" +
        "  buildHash;\n" +
        "  constructor(emitter, matchId, buildHash) {\n" +
        "    this.emitter = emitter;\n" +
        "    this.matchId = matchId;\n" +
        "    this.buildHash = buildHash;\n" +
        "  }",
      "MatchTelemetryRecorder ctor",
    );
  }

  if (rel.endsWith("server/ClusterCheckin.ts")) {
    // S7: zod keeps the functional-enum branch (precedent: core/Schemas.ts)
    // so `ServerStateSchema.options` — the real z.enum literal array the
    // capture dumps — still evaluates; every other chain (z.object for the
    // two wire schemas) falls through to the inert Proxy. The ServerEnv
    // import becomes the shared __CK_ENV facade global (set by the capture
    // BEFORE loading; every read is a [72, method, ...codec] trace event).
    // sendCheckin (fetch / AbortSignal.timeout / zod safeParse) stays in the
    // file but is never called (its unresolved identifiers sit in a dead
    // body; strip mode is syntax-only).
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
      "ClusterCheckin zod import",
    );
    out = must(
      out,
      'import { ServerEnv } from "./ServerEnv";\n',
      "const ServerEnv = globalThis.__CK_ENV;\n",
      "ClusterCheckin ServerEnv import",
    );
  }

  if (rel.endsWith("server/RankedCheckin.ts")) {
    // S7: the pure-decision subset. `import type winston` / the GameManager /
    // MapPlaylist type imports are erased by strip mode. zod takes the same
    // functional-enum shim as ClusterCheckin so MatchmakingAssignmentSchema
    // (a module-level z.object) evaluates inertly. The ServerList /
    // ClusterCheckin value imports ride on the prepared copies (registeredSite
    // is the real TS call edge the Rust port mirrors). startPolling is only
    // referenced inside the excluded startRankedCheckinLoops body -> drop the
    // import (dead body, strip mode is syntax-only). The ctor's parameter
    // properties are expanded (strip mode rejects them, precedent: Privilege).
    // buildVersionField / buildSiteField are module-private -> exported so
    // the capture can exercise them (precedent: ServerList pathNamesGame).
    const rcSlRel = "src/core/ServerList.ts";
    if (!prepared.has(rcSlRel)) prepare(rcSlRel);
    const rcCcRel = "src/server/ClusterCheckin.ts";
    if (!prepared.has(rcCcRel)) prepare(rcCcRel);
    out = must(
      out,
      'import { z } from "zod";\n' +
        'import { isCommitLike, isSiteLike } from "../core/ServerList";\n' +
        'import { registeredSite } from "./ClusterCheckin";\n' +
        'import type { GameManager } from "./GameManager";\n' +
        'import type { MapPlaylist } from "./MapPlaylist";\n' +
        'import { startPolling } from "./PollingLoop";\n' +
        'import { ServerEnv } from "./ServerEnv";\n',
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
        "});\n" +
        `import { isCommitLike, isSiteLike } from "./${prepared.get(rcSlRel)}";\n` +
        `import { registeredSite } from "./${prepared.get(rcCcRel)}";\n` +
        "const ServerEnv = globalThis.__CK_ENV;\n",
      "RankedCheckin imports",
    );
    out = must(
      out,
      "  constructor(\n" +
        "    private readonly isActive: () => boolean,\n" +
        "    private readonly log: Pick<winston.Logger, \"info\">,\n" +
        "  ) {}",
      "  isActive;\n" +
        "  log;\n" +
        "  constructor(isActive, log) {\n" +
        "    this.isActive = isActive;\n" +
        "    this.log = log;\n" +
        "  }",
      "RankedCheckinGate ctor",
    );
    out = must(
      out,
      "function buildVersionField(): { version?: string } {",
      "export function buildVersionField(): { version?: string } {",
      "RankedCheckin buildVersionField export",
    );
    out = must(
      out,
      "function buildSiteField(): { site?: string } {",
      "export function buildSiteField(): { site?: string } {",
      "RankedCheckin buildSiteField export",
    );
  }

  if (rel.endsWith("server/GameApiCors.ts")) {
    // S7: the `import type { ... } from "express"` line is erased by strip
    // mode (the express package is unresolvable here, but the import never
    // survives type stripping); drop it explicitly so the load does not
    // depend on that erasure. ServerEnv becomes the shared __CK_ENV facade.
    // isAllowedOrigin is module-private -> exported for the capture. The
    // gameApiCors middleware (express req/res/next) stays but is never
    // called (dead body).
    out = must(
      out,
      'import type { NextFunction, Request, Response } from "express";\n' +
        'import { ServerEnv } from "./ServerEnv";\n',
      "const ServerEnv = globalThis.__CK_ENV;\n",
      "GameApiCors imports",
    );
    out = must(
      out,
      "function isAllowedOrigin(origin: string): boolean {",
      "export function isAllowedOrigin(origin: string): boolean {",
      "GameApiCors isAllowedOrigin export",
    );
  }

  if (rel.endsWith("server/NoStoreHeaders.ts")) {
    // S7: the `import type { Response } from "express"` line is erased by
    // strip mode; drop it explicitly (the package is unresolvable here).
    out = must(
      out,
      'import type { Response } from "express";\n',
      "",
      "NoStoreHeaders express import",
    );
  }

  if (rel.endsWith("render/types/Renderer.ts")) {
    // S8: the `import type { TileRef }` line is erased by strip mode; drop it
    // explicitly (the GameMap graph is unresolvable here). Node's strip-only
    // TS loader rejects `export enum`; the two numeric enums are inlined with
    // the EXACT TypeScript emit shape (forward name->value plus the integer
    // reverse mapping, inserted member-by-member) so `Object.keys` still
    // lists "0","1","2" before the declaration-order names — the rnc_ key
    // dump pins that order. The interfaces / type aliases strip clean.
    out = must(
      out,
      'import type { TileRef } from "../../../core/game/GameMap";\n\n',
      "",
      "Renderer TileRef import",
    );
    out = must(
      out,
      "export enum TrainType {\n  Engine = 0,\n  TailEngine = 1,\n  Carriage = 2,\n}",
      "export const TrainType = (() => {\n" +
        "  const t: any = {};\n" +
        '  t[t["Engine"] = 0] = "Engine";\n' +
        '  t[t["TailEngine"] = 1] = "TailEngine";\n' +
        '  t[t["Carriage"] = 2] = "Carriage";\n' +
        "  return t;\n" +
        "})();",
      "Renderer TrainType enum",
    );
    out = must(
      out,
      "export enum PlayerTypeEnum {\n  Human = 0,\n  Bot = 1,\n  Nation = 2,\n}",
      "export const PlayerTypeEnum = (() => {\n" +
        "  const t: any = {};\n" +
        '  t[t["Human"] = 0] = "Human";\n' +
        '  t[t["Bot"] = 1] = "Bot";\n' +
        '  t[t["Nation"] = 2] = "Nation";\n' +
        "  return t;\n" +
        "})();",
      "Renderer PlayerTypeEnum enum",
    );
  }

  if (rel.endsWith("utilities/ReplaySpeedMultiplier.ts")) {
    // S8: no imports; the only strip-mode problem is `export enum`. Values
    // are 2 / 1 / 0.5 / 0 — the rps_ dump only reads the forward members, so
    // the plain-object inline (no reverse mapping) is sufficient here.
    out = must(
      out,
      "export enum ReplaySpeedMultiplier {\n  slow = 2,\n  normal = 1,\n  fast = 0.5,\n  fastest = 0,\n}",
      "export const ReplaySpeedMultiplier = { slow: 2, normal: 1, fast: 0.5, fastest: 0 };",
      "ReplaySpeedMultiplier enum",
    );
  }

  if (rel.endsWith("layers/lib/GoldRateTracker.ts")) {
    // S8: `history` is a TS-PRIVATE readonly field — runtime-accessible, and
    // the grt_ dump needs it. Strip mode erases the modifier.
    out = must(
      out,
      "  private readonly history = new Map<number, Sample[]>();",
      "  readonly history = new Map<number, Sample[]>();",
      "GoldRateTracker history visibility",
    );
  }

  // S8: the three derive modules that VALUE-import from the render types
  // barrel (../../types) — the barrel re-exports Renderer.ts (interfaces +
  // enums) and would pull the whole client graph in. Redirect the value
  // imports to the prepared UnitType.ts copy (precedent: Privilege's
  // isTemporaryUsername redirect); the `import type` lines are dropped.
  const utRel = "src/client/render/types/UnitType.ts";
  if (rel !== utRel && !prepared.has(utRel)) prepare(utRel);
  const utImport = `./${prepared.get(utRel)}`;

  if (rel.endsWith("derive/AttackRings.ts")) {
    out = must(
      out,
      'import type { AttackRingInput, UnitState } from "../../types";\n' +
        'import { UT_TRANSPORT } from "../../types";\n',
      `import { UT_TRANSPORT } from "${utImport}";\n`,
      "AttackRings types imports",
    );
  }

  if (rel.endsWith("derive/NukeTelegraphs.ts")) {
    out = must(
      out,
      'import type { NukeTelegraphData, UnitState } from "../../types";\n' +
        'import { NUKE_MAGNITUDES } from "../../types";\n',
      `import { NUKE_MAGNITUDES } from "${utImport}";\n`,
      "NukeTelegraphs types imports",
    );
    // classifyOwner is module-private -> exported for the capture (precedent:
    // GameApiCors isAllowedOrigin).
    out = must(
      out,
      "function classifyOwner(",
      "export function classifyOwner(",
      "NukeTelegraphs classifyOwner export",
    );
  }

  if (rel.endsWith("derive/PlayerStatus.ts")) {
    out = must(
      out,
      'import type { PlayerState, PlayerStatusData, UnitState } from "../../types";\n' +
        'import { NUKE_TYPES, UT_MIRV_WARHEAD } from "../../types";\n',
      `import { NUKE_TYPES, UT_MIRV_WARHEAD } from "${utImport}";\n`,
      "PlayerStatus types imports",
    );
    // The two module-private constants the pst_ kind-1 dump reads (precedent:
    // classifyOwner / isAllowedOrigin).
    out = must(
      out,
      "const NUKE_ACTIVE_TYPES: ReadonlySet<string> = new Set([",
      "export const NUKE_ACTIVE_TYPES: ReadonlySet<string> = new Set([",
      "PlayerStatus NUKE_ACTIVE_TYPES export",
    );
    out = must(
      out,
      "const OWNER_MASK = 0xfff;",
      "export const OWNER_MASK = 0xfff;",
      "PlayerStatus OWNER_MASK export",
    );
  }

  if (rel.endsWith("derive/AllianceClusters.ts")) {
    // Type-only barrel import; drop it explicitly (strip-mode erasure of
    // `import type` is not something the load should depend on).
    out = must(
      out,
      'import type { PlayerState } from "../../types";\n',
      "",
      "AllianceClusters types import",
    );
  }

  if (rel.endsWith("derive/RelationMatrix.ts")) {
    out = must(
      out,
      'import type { PlayerState, PlayerStatic } from "../../types";\n',
      "",
      "RelationMatrix types import",
    );
  }

  if (rel.endsWith("derive/TerrainRowSpans.ts")) {
    out = must(
      out,
      'import type { TerrainRect } from "../../types";\n',
      "",
      "TerrainRowSpans types import",
    );
    // The two module-private merge-gate constants the trs_ kind-1 dump reads.
    out = must(
      out,
      "const MAX_MERGE_OVERDRAW_RATIO = 1.5;\nconst MAX_MERGE_EXTRA_TEXELS = 4096;",
      "export const MAX_MERGE_OVERDRAW_RATIO = 1.5;\nexport const MAX_MERGE_EXTRA_TEXELS = 4096;",
      "TerrainRowSpans merge constants export",
    );
  }

  // ================= S9: render/frame stateful classes + client facades ======

  if (rel.endsWith("render/frame/SpiralTrails.ts")) {
    // S9: the `import type { UnitState }` line is dropped explicitly; the
    // value import (SMOOTHED_NUKE_TYPES / UT_MIRV_WARHEAD) redirects to the
    // prepared UnitType.ts copy (S8 precedent). TS-PRIVATE fields are
    // runtime-visible after strip (the stp_ dumps read params / ribbonsById /
    // ribbonList directly), no visibility rewrite needed.
    out = must(
      out,
      'import type { UnitState } from "../types";\n' +
        'import { SMOOTHED_NUKE_TYPES, UT_MIRV_WARHEAD } from "../types";\n',
      `import { SMOOTHED_NUKE_TYPES, UT_MIRV_WARHEAD } from "${utImport}";\n`,
      "SpiralTrails types imports",
    );
  }

  if (rel.endsWith("render/frame/TrailManager.ts")) {
    // S9: same barrel redirect as SpiralTrails; the private trailState /
    // trailCounts / unitTrails / _dirtyRowMin/_dirtyRowMax fields are read
    // by the tlm_ dumps at runtime (strip erases the modifiers).
    out = must(
      out,
      'import type { UnitState } from "../types";\n' +
        "import { SMOOTHED_NUKE_TYPES } from \"../types\";\n",
      `import { SMOOTHED_NUKE_TYPES } from "${utImport}";\n`,
      "TrailManager types imports",
    );
  }

  if (rel.endsWith("render/frame/RailroadCache.ts")) {
    // S9: Node's strip-only loader rejects `export enum`; RailType is a
    // regular enum whose forward members are the only thing the rlc_ ops
    // read (no key-order dump), so the plain-object inline suffices. The
    // GameUpdates import keeps the VALUE (GameUpdateType, prepared copy)
    // and drops the four type-only names.
    out = must(
      out,
      "export enum RailType {\n" +
        "  VERTICAL,\n" +
        "  HORIZONTAL,\n" +
        "  TOP_LEFT,\n" +
        "  TOP_RIGHT,\n" +
        "  BOTTOM_LEFT,\n" +
        "  BOTTOM_RIGHT,\n" +
        "}",
      "export const RailType = {\n" +
        "  VERTICAL: 0,\n" +
        "  HORIZONTAL: 1,\n" +
        "  TOP_LEFT: 2,\n" +
        "  TOP_RIGHT: 3,\n" +
        "  BOTTOM_LEFT: 4,\n" +
        "  BOTTOM_RIGHT: 5,\n" +
        "};",
      "RailroadCache RailType enum",
    );
    const rcGuRel = "src/core/game/GameUpdates.ts";
    if (!prepared.has(rcGuRel)) prepare(rcGuRel);
    out = must(
      out,
      "import {\n" +
        "  GameUpdateType,\n" +
        "  GameUpdateViewData,\n" +
        "  RailroadConstructionUpdate,\n" +
        "  RailroadDestructionUpdate,\n" +
        "  RailroadSnapUpdate,\n" +
        '} from "../../../core/game/GameUpdates";\n',
      `import { GameUpdateType } from "./${prepared.get(rcGuRel)}";\n`,
      "RailroadCache GameUpdates import",
    );
  }

  if (rel.endsWith("utilities/PlayerProfileUrl.ts")) {
    // S9: ClientEnv.shareBase() reads window.location on the real host; the
    // capture scripts the base through globalThis.__PPU_BASE (a string set
    // per ppu_ op — precedent: __CK_ENV).
    out = must(
      out,
      'import { ClientEnv } from "../ClientEnv";\n',
      "const ClientEnv = { shareBase: () => globalThis.__PPU_BASE };\n",
      "PlayerProfileUrl ClientEnv import",
    );
  }

  if (rel.endsWith("client/PagePin.ts")) {
    // S9: stripVersionPrefix redirects to the prepared ServerList.ts copy
    // (precedent: RankedCheckinGate); window.location.pathname becomes
    // globalThis.__PPN_PATH() — a scripted function that RETURNS the path or
    // THROWS (the non-browser host), and counts its own invocations so the
    // ppn_ facadeCalls op pins the lazy-latch read count.
    const ppnSlRel = "src/core/ServerList.ts";
    if (!prepared.has(ppnSlRel)) prepare(ppnSlRel);
    out = must(
      out,
      'import { stripVersionPrefix } from "../core/ServerList";\n',
      `import { stripVersionPrefix } from "./${prepared.get(ppnSlRel)}";\n`,
      "PagePin ServerList import",
    );
    out = must(
      out,
      "captured = stripVersionPrefix(window.location.pathname).commit;",
      "captured = stripVersionPrefix(globalThis.__PPN_PATH()).commit;",
      "PagePin location read",
    );
  }

  if (rel.endsWith("client/CreatorCode.ts")) {
    // S9: no imports; every host touch becomes a traced facade global.
    // localStorage -> __CCC_LS (getItem/setItem/removeItem push the 74/75/76
    // events), Date.now() -> __CCC_NOW() (FIFO + 81 event), window.location
    // -> __CCC_LOC (pathname/search/hash getters push 78/79/80), history ->
    // __CCC_HISTORY (replaceState pushes 77). The capture reads the trace
    // buffer alongside the return value, mirroring the Rust res prefix.
    out = must(
      out,
      "const PENDING_CREATOR_CODE_TTL_MS = 7 * 24 * 60 * 60 * 1000;",
      "export const PENDING_CREATOR_CODE_TTL_MS = 7 * 24 * 60 * 60 * 1000;",
      "CreatorCode TTL export",
    );
    out = must(
      out,
      "localStorage.setItem(",
      "globalThis.__CCC_LS.setItem(",
      "CCC setItem",
    );
    out = must(
      out,
      "localStorage.getItem(",
      "globalThis.__CCC_LS.getItem(",
      "CCC getItem",
    );
    out = must(
      out,
      "localStorage.removeItem(",
      "globalThis.__CCC_LS.removeItem(",
      "CCC removeItem",
    );
    out = must(out, "Date.now()", "globalThis.__CCC_NOW()", "CCC Date.now");
    out = must(
      out,
      "parseCreatorCodePath(window.location.pathname)",
      "parseCreatorCodePath(globalThis.__CCC_LOC.pathname)",
      "CCC pathname",
    );
    out = must(
      out,
      "history.replaceState(\n    null,\n    \"\",\n    \"/\" + window.location.search + window.location.hash,\n  );",
      "globalThis.__CCC_HISTORY.replaceState(\n    null,\n    \"\",\n    \"/\" + globalThis.__CCC_LOC.search + globalThis.__CCC_LOC.hash,\n  );",
      "CCC replaceState",
    );
  }

  // ================== S10: client pure-math / host-adjacent modules ==========

  if (rel.endsWith("render/gl/utils/NukeTrajectory.ts")) {
    // S10: the `import type { NukeTrajectoryData }` barrel line is dropped
    // explicitly (S8 precedent — strip-mode erasure must not be depended on);
    // the module body is pure math with no other host touch.
    out = must(
      out,
      'import type { NukeTrajectoryData } from "../../types";\n',
      "",
      "NukeTrajectory types import",
    );
  }

  if (rel.endsWith("client/PresenceGroup.ts")) {
    // S10: the Schemas / DesktopPresence imports are type-only -> dropped;
    // the GameMode / GameType VALUE import rides on the prepared Game.ts
    // copy (precedent L1376).
    out = must(
      out,
      'import type {\n  GameConfig,\n  ServerMessage,\n  ServerStartGameMessage,\n} from "../core/Schemas";\n',
      "",
      "PresenceGroup Schemas import",
    );
    out = must(
      out,
      'import type { PresencePayload } from "./DesktopPresence";\n',
      "",
      "PresenceGroup DesktopPresence import",
    );
    if (!prepared.has("src/core/game/Game.ts")) prepare("src/core/game/Game.ts");
    out = must(
      out,
      'import { GameMode, GameType } from "../core/game/Game";\n',
      `import { GameMode, GameType } from "./${prepared.get("src/core/game/Game.ts")}";\n`,
      "PresenceGroup Game import",
    );
  }

  if (rel.endsWith("client/GraphicsPresets.ts")) {
    // S10: only stableStringify is ported. The zod / JSON-import / Utils /
    // UserSettings imports, the top-level BUILTIN_PRESETS evaluation and the
    // two host-bound functions (parseGraphicsOverridesJson,
    // migrateLegacyGraphicsSettings) are all deleted — the preset map would
    // throw at module load without the schema graph.
    out = must(
      out,
      'import { UserSettings } from "../core/game/UserSettings";\n' +
        'import { GraphicsOverridesSchema, type GraphicsOverrides } from "./render/gl";\n' +
        'import builtinPresets from "./render/gl/graphics-presets.json";\n' +
        'import { translateText } from "./Utils";\n',
      "",
      "GraphicsPresets imports",
    );
    out = must(
      out,
      "// Built-in presets, defined in graphics-presets.json \u2014 each entry's overrides\n" +
        "// are schema-parsed at load (JSON imports can't carry the palette enum's\n" +
        "// literal types). Overrides are applied wholesale. Night's ambient 0.36 is\n" +
        "// the graphics modal slider's level 8.\n" +
        "export const BUILTIN_PRESETS: ReadonlyArray<{\n" +
        "  nameKey: string;\n" +
        "  descKey: string;\n" +
        "  overrides: GraphicsOverrides;\n" +
        "}> = builtinPresets.map((preset) => ({\n" +
        "  nameKey: preset.nameKey,\n" +
        "  descKey: preset.descKey,\n" +
        "  overrides: GraphicsOverridesSchema.parse(preset.overrides),\n" +
        "}));\n",
      "",
      "GraphicsPresets BUILTIN_PRESETS",
    );
    out = must(
      out,
      "/**\n" +
        " * Parse player-pasted settings JSON. Returns null unless the text is valid\n" +
        " * JSON the schema recognizes in full. The schema strips unknown keys (needed\n" +
        " * to read legacy stored data), which would let a mistyped paste apply as an\n" +
        " * empty or partial config \u2014 so anything the parse dropped rejects the import\n" +
        " * instead.\n" +
        " */\n" +
        "export function parseGraphicsOverridesJson(\n" +
        "  text: string,\n" +
        "): GraphicsOverrides | null {\n" +
        "  let raw: unknown;\n" +
        "  try {\n" +
        "    raw = JSON.parse(text);\n" +
        "  } catch {\n" +
        "    return null;\n" +
        "  }\n" +
        "  const parsed = GraphicsOverridesSchema.safeParse(raw);\n" +
        "  if (!parsed.success) return null;\n" +
        "  if (stableStringify(parsed.data) !== stableStringify(raw)) return null;\n" +
        "  return parsed.data;\n" +
        "}\n",
      "",
      "GraphicsPresets parseGraphicsOverridesJson",
    );
    const migIdx = out.indexOf("/**\n * One-time migration");
    if (migIdx === -1) {
      throw new Error("ts_load: GraphicsPresets migration block not found");
    }
    out = out.slice(0, migIdx);
  }

  if (rel.endsWith("render/gl/Camera.ts")) {
    // S11: renderDpr reads window.devicePixelRatio; the capture scripts the
    // RAW dpr per op through globalThis.__CAM_DPR and the rewrite keeps the
    // `|| 2` falsy gate and the cap of 2 (the Rust twin re-applies the same
    // two steps from the op's first argument).
    out = must(
      out,
      'import { renderDpr } from "./utils/Dpr";\n',
      "const renderDpr = () => Math.min(globalThis.__CAM_DPR || 2, 2);\n",
      "Camera Dpr import",
    );
  }

  if (rel.endsWith("name-pass/TextLayout.ts")) {
    // S11: the GlyphTables import is type-only -> dropped; the Types value
    // import is inlined (the two constants are plain numbers, the rest of
    // Types.ts is interfaces).
    out = must(
      out,
      'import type { GlyphTables } from "./AtlasData";\n',
      "",
      "TextLayout AtlasData import",
    );
    out = must(
      out,
      'import { CHAR_RANGE, MAX_CHARS } from "./Types";\n',
      "const CHAR_RANGE = 384;\nconst MAX_CHARS = 32;\n",
      "TextLayout Types import",
    );
  }

  if (rel.endsWith("render/gl/utils/ColorUtils.ts")) {
    // S11: the render-settings.json default import needs an absolute URL +
    // the JSON module attribute (QuickChat / TribeNames precedent).
    out = must(
      out,
      'import renderDefaults from "../render-settings.json";\n',
      `import renderDefaults from "${TS_URL}src/client/render/gl/render-settings.json" with { type: "json" };\n`,
      "ColorUtils JSON import",
    );
  }

  if (rel.endsWith("view/CosmeticVisibility.ts")) {
    // S11: effectTypeForSlot is a *value* import -> redirect to the prepared
    // CosmeticSchemas copy (its zod / jose graph is already shimmed away).
    // The PlayerCosmetics / GraphicsOverrides imports are `import type` and
    // ride strip mode untouched.
    const cvsCosRel = "src/core/CosmeticSchemas.ts";
    if (!prepared.has(cvsCosRel)) prepare(cvsCosRel);
    out = must(
      out,
      'import { effectTypeForSlot } from "../../core/CosmeticSchemas";\n',
      `import { effectTypeForSlot } from "./${prepared.get(cvsCosRel)}";\n`,
      "CosmeticVisibility CosmeticSchemas import",
    );
  }

  if (rel.endsWith("render/gl/utils/Affiliation.ts")) {
    // S11: getPaletteSize is a *value* import -> redirect to the prepared
    // ColorUtils copy (its JSON import is already rewritten). createTexture2D
    // is pure GL plumbing -> stub (the capture never observes the texture).
    // The RenderSettings import is `import type` (erased). The ctor's
    // `private settings` parameter property is expanded (strip mode).
    const afpCuRel = "src/client/render/gl/utils/ColorUtils.ts";
    if (!prepared.has(afpCuRel)) prepare(afpCuRel);
    out = must(
      out,
      'import { getPaletteSize } from "./ColorUtils";\n',
      `import { getPaletteSize } from "./${prepared.get(afpCuRel)}";\n`,
      "Affiliation ColorUtils import",
    );
    out = must(
      out,
      'import { createTexture2D } from "./GlUtils";\n',
      "const createTexture2D = () => ({});\n",
      "Affiliation GlUtils import",
    );
    out = must(
      out,
      "  constructor(\n" +
        "    gl: WebGL2RenderingContext,\n" +
        "    private settings: RenderSettings,\n" +
        "  ) {\n" +
        "    this.gl = gl;",
      "  private settings: RenderSettings;\n\n" +
        "  constructor(\n" +
        "    gl: WebGL2RenderingContext,\n" +
        "    settings: RenderSettings,\n" +
        "  ) {\n" +
        "    this.settings = settings;\n" +
        "    this.gl = gl;",
      "Affiliation ctor",
    );
  }

  if (rel.endsWith("hud/NameBoxCalculator.ts")) {
    // S10: the pure subset keeps only Cell (used at runtime by createGrid);
    // Game / NameViewData / Player are type-only names and ride on the
    // prepared Game.ts copy anyway (Cell is a class there). The Util import
    // (calculateBoundingBox) is only used by the out-of-scope placeName.
    if (!prepared.has("src/core/game/Game.ts")) prepare("src/core/game/Game.ts");
    out = must(
      out,
      'import { Cell, Game, NameViewData, Player } from "../../core/game/Game";\n',
      `import { Cell } from "./${prepared.get("src/core/game/Game.ts")}";\n`,
      "NameBoxCalculator Game import",
    );
    out = must(
      out,
      'import { calculateBoundingBox } from "../../core/Util";\n',
      "",
      "NameBoxCalculator Util import",
    );
  }

  if (rel.endsWith("utilities/GameConfigHelpers.ts")) {
    // S10: GameMapType is a runtime value (Object.values in getRandomMapType)
    // and UnitType a type-only name; both ride on the prepared Game.ts copy.
    // The Schemas import is type-only -> dropped. Math.random() becomes the
    // scripted globalThis.__GCH_RAND() facade (precedent __MP_RAND).
    if (!prepared.has("src/core/game/Game.ts")) prepare("src/core/game/Game.ts");
    out = must(
      out,
      'import { GameMapType, UnitType } from "../../core/game/Game";\n',
      `import { GameMapType, UnitType } from "./${prepared.get("src/core/game/Game.ts")}";\n`,
      "GameConfigHelpers Game import",
    );
    out = must(
      out,
      'import { GameConfig } from "../../core/Schemas";\n',
      "",
      "GameConfigHelpers Schemas import",
    );
    out = must(
      out,
      "const randIdx = Math.floor(Math.random() * maps.length);",
      "const randIdx = Math.floor(globalThis.__GCH_RAND() * maps.length);",
      "GameConfigHelpers Math.random",
    );
  }

  if (rel.endsWith("client/Utils.ts")) {
    // S11: the pure formatting subset (renderNumber / renderTroops /
    // formatPercentage / normaliseMapKey / presenceMapKey /
    // formatKeyForDisplay / formatDebugTranslation). The whole import block
    // is rewritten: intl-messageformat (not installed for the loader)
    // becomes a throw stub (translateText is never invoked by the capture);
    // the *value* names the captured functions actually touch (maps) plus
    // the dead-body value names (Duos / GameMode / HumansVsNations /
    // MessageType / Quads / Trios) ride the prepared Game.ts copy;
    // stripVersionPrefix rides the prepared ServerList.ts copy. The
    // type-only names (DoomsdayClockSpeed / GameConfig / PublicGameModifiers
    // / Team / LangSelector) are erased by strip mode, and the host-bound
    // value imports (ClientEnv / Platform) are only referenced inside dead
    // bodies - unresolved identifiers there are harmless. S11b: pagePin is
    // LIVE (currentPagePath) and rides the prepared PagePin.ts copy; the
    // three Date.now() default parameters become globalThis.__UN_NOW()
    // (FIFO facade, precedent __CCC_NOW / __GCH_RAND).
    if (!prepared.has("src/core/game/Game.ts")) prepare("src/core/game/Game.ts");
    const ufSlRel = "src/core/ServerList.ts";
    if (!prepared.has(ufSlRel)) prepare(ufSlRel);
    const ufPpnRel = "src/client/PagePin.ts";
    if (!prepared.has(ufPpnRel)) prepare(ufPpnRel);
    out = must(
      out,
      'import IntlMessageFormat from "intl-messageformat";\n' +
        'import { DoomsdayClockSpeed } from "../core/game/DoomsdayClock";\n' +
        "import {\n" +
        "  Duos,\n" +
        "  GameMode,\n" +
        "  HumansVsNations,\n" +
        "  maps,\n" +
        "  MessageType,\n" +
        "  PublicGameModifiers,\n" +
        "  Quads,\n" +
        "  Team,\n" +
        '  Trios,\n' +
        '} from "../core/game/Game";\n' +
        'import { GameConfig } from "../core/Schemas";\n' +
        'import { stripVersionPrefix } from "../core/ServerList";\n' +
        'import { ClientEnv } from "./ClientEnv";\n' +
        'import type { LangSelector } from "./LangSelector";\n' +
        'import { pagePin } from "./PagePin";\n' +
        'import { Platform } from "./Platform";\n',
      "const IntlMessageFormat = class {\n" +
        "  constructor() {\n" +
        '    throw new Error("intl-messageformat stub");\n' +
        "  }\n" +
        "};\n" +
        "import {\n" +
        "  Duos,\n" +
        "  GameMode,\n" +
        "  HumansVsNations,\n" +
        "  maps,\n" +
        "  MessageType,\n" +
        "  Quads,\n" +
        "  Trios,\n" +
        `} from "./${prepared.get("src/core/game/Game.ts")}";\n` +
        `import { stripVersionPrefix } from "./${prepared.get(ufSlRel)}";\n` +
        `import { pagePin } from "./${prepared.get(ufPpnRel)}";\n`,
      "Utils imports",
    );
    out = must(
      out,
      "localNowMs: number = Date.now(),",
      "localNowMs: number = globalThis.__UN_NOW(),",
      "Utils Date.now defaults",
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
