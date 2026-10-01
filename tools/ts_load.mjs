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
