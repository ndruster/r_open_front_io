// G6-0 (M1a) — integration-vector recorder for the `game_loop` harness.
//
// Drives the REAL TS classes (loaded through ts_load.mjs, same channel as
// gen_vectors.mjs) over synthetic terrain bytes and pins the whole per-op
// result stream, bit for bit:
//
//   node tools/gen_m1_vectors.mjs
//
// Writes:
//   crates/core/tests/data/m1_vectors.json   — for tools/m1_scan.mjs (wasm)
//   crates/core/tests/data/m1_vectors.rs     — #[path]-embedded for
//                                              crates/core/tests/parity_m1.rs
//
// Scenario ops mirror `crates/core/src/game_loop.rs`'s op table (kinds 0-8).
// M1-0 kept two pins: no Config (waterNukes pinned true) and no players.
// M1a lifts both for the construct_game flow (kind 7): with
// M1A_REAL_PLAYERS=1, ts_load redirects `new PlayerImpl` and the ctor's
// collaborators to the REAL ported classes, so the recorder drives the real
// `createGame` factory + ctor + populateTeams / addPlayers / addPlayer /
// maybeAssignTeam chain over the same synthetic terrain. The op-0 construct
// (kind 0) keeps the M1-0 pin behaviour for the pre-M1a scenarios unchanged.
process.env.M1A_REAL_PLAYERS = "1";

import { writeFileSync, mkdirSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { loadTs } from "./ts_load.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");

const { GameMapImpl } = await loadTs("src/core/game/GameMap.ts");
const { WaterManager } = await loadTs("src/core/game/WaterManager.ts");
const GI = await loadTs("src/core/game/GameImpl.ts");

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

// ------------------------------------------------------------ Config mirror
// The exact Config.ts surface GameImpl's construction chain reads, mirrored
// line-for-line (the real Config class is not headlessly loadable):
//   gameConfig 344 / playerTeams 386 / disableNavMesh 404 / waterNukes 415 /
//   infiniteTroops 421 / hasInfiniteTroopsForInfo 715-721 /
//   startingGold 436-441 + startingGoldFor 743-750 / startManpower 1000-1019.

class StubConfig {
  constructor(gameConfig) {
    this._gameConfig = gameConfig;
  }
  gameConfig() {
    return this._gameConfig;
  }
  playerTeams() {
    return this._gameConfig.playerTeams ?? 0;
  }
  disableNavMesh() {
    return this._gameConfig.disableNavMesh ?? false;
  }
  waterNukes() {
    return this._gameConfig.waterNukes ?? false;
  }
  infiniteTroops() {
    return this._gameConfig.infiniteTroops ?? false;
  }
  hasInfiniteTroopsForInfo(pi) {
    if (this.infiniteTroops()) return true;
    return (
      (this._gameConfig.hostCheats?.infiniteTroops ?? false) &&
      pi.isLobbyCreator
    );
  }
  startManpower(pi) {
    if (pi.playerType === "BOT") return 10_000;
    if (pi.playerType === "NATION") {
      switch (this._gameConfig.difficulty) {
        case "Easy":
          return 12_500;
        case "Medium":
          return 18_750;
        case "Hard":
          return 25_000;
        case "Impossible":
          return 31_250;
      }
    }
    return this.hasInfiniteTroopsForInfo(pi) ? 1_000_000 : 25_000;
  }
  startingGold(pi) {
    if (pi.playerType === "BOT") return 0n;
    const base = BigInt(this._gameConfig.startingGold ?? 0);
    const hc = this._gameConfig.hostCheats;
    if (hc?.startingGold && pi.isLobbyCreator) {
      return base + BigInt(hc.startingGold);
    }
    return base;
  }
}

// Plain-object PlayerInfo mirror: PlayerInfo.ts's ctor only assigns readonly
// fields (+ displayName formatting, not part of the M1a observable surface),
// and every consumer (PlayerImpl ctor field reads, assignTeams) reads the
// same property names off the object.
let nextInfoNum = 1;
function makeInfo(name, type, extra = {}) {
  const clanTag = extra.clanTag ?? null;
  return {
    name,
    playerType: type,
    // formatPlayerDisplayName (PlayerInfo.ts): a truthy clanTag prepends the
    // bracketed tag. M1b's toFullUpdate reads displayName, so the plain
    // object must carry the real ctor's computation.
    displayName: clanTag ? `[${clanTag}] ${name}` : name,
    clientID: extra.clientID ?? null,
    id: extra.id ?? `P${nextInfoNum++}`,
    isLobbyCreator: extra.isLobbyCreator ?? false,
    clanTag,
    friends: extra.friends ?? [],
    teamIndex: extra.teamIndex ?? null,
    nationFlag: extra.nationFlag ?? null,
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

  // GameImpl.setFallout (TS 230-240). Once a game exists the REAL method
  // runs (it owns _territoryVersion and tileUpdatePairs from here on); the
  // pre-game scenarios keep the mirror with the M1-0 hasOwner pin.
  setFallout(tile, value, out) {
    if (this.game) {
      try {
        this.game.setFallout(tile, value);
        out.push(0);
      } catch (e) {
        out.push(1);
        pushStr(out, String(e.message));
      }
      return;
    }
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
    if (this.game) {
      try {
        this.game.setWater(tile);
        out.push(0);
      } catch (e) {
        out.push(1);
        pushStr(out, String(e.message));
      }
      return;
    }
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

  // GameImpl.queueWaterConversion (TS 256-266) — with a game the real
  // method runs (real waterNukes gate into the game's own water manager);
  // the water-only M1-0 ops have no Config installed, pinning
  // waterNukes() true.
  queueWaterConversion(tile, out) {
    if (this.game) {
      try {
        this.game.queueWaterConversion(tile);
        out.push(0);
      } catch (e) {
        out.push(1);
        pushStr(out, String(e.message));
      }
      return;
    }
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
    for (const t of changed) {
      if (this.game) {
        // The game owns tileUpdatePairs once constructed (its private
        // recordTileUpdate is the exact packer the tick loop uses).
        this.game.recordTileUpdate(t);
      } else {
        this.recordTileUpdate(t);
      }
    }
    out.push(0, changed.length, ...changed);
  }

  drain(out) {
    if (this.game) {
      const r = this.game.drainPackedTileUpdates();
      out.push(0, r.length, ...r);
      return;
    }
    out.push(0, this.tileUpdates.length, ...this.tileUpdates);
    this.tileUpdates = [];
  }

  digest(out) {
    const m = this.map;
    const n = m.width() * m.height();
    out.push(
      0,
      this.game ? this.game._territoryVersion : this.territoryVersion,
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

  // The REAL createGame factory + GameImpl ctor + populateTeams /
  // addPlayers / addPlayer / maybeAssignTeam chain (GameImpl.ts 55-74,
  // 118-151, 155-198, 680-703). The ctor logs timing (and addPlayers warns
  // on kicked clans) — silenced so the recorder stdout stays parseable.
  constructGame(cfgObj, humans, nations, out) {
    const log = console.log;
    const warn = console.warn;
    console.log = () => {};
    console.warn = () => {};
    try {
      this.game = GI.createGame(
        humans,
        nations,
        this.map,
        this.mini,
        new StubConfig(cfgObj),
        undefined,
      );
      // The ctor builds its own WaterManager; from here on every water op
      // must flow through the game's instance (the pre-game one would keep
      // its own queue/graph and diverge from executeNextTick's internal
      // `this._waterManager.tick`).
      this.water = this.game._waterManager;
      out.push(0);
    } catch (e) {
      // stderr only — stdout must stay parseable for the vector emitter.
      process.stderr.write(`[m1a] construct_game error: ${e.stack}\n`);
      out.push(1);
      pushStr(out, String(e.message));
    } finally {
      console.log = log;
      console.warn = warn;
    }
  }

  // Post-construction internal state snapshot: nextPlayerID, botTeam, the
  // resolved team list, then every player in smallID order (identity, team,
  // troops/gold as the ctor computed them, and one PseudoRandom draw to pin
  // the per-player seed state). Mirrors game_loop.rs op 8 token-for-token.
  snapshot(out) {
    const g = this.game;
    out.push(0);
    out.push(g.nextPlayerID);
    pushStr(out, g.botTeam);
    out.push(g.playerTeams.length);
    for (const t of g.playerTeams) pushStr(out, t);
    const players = g._playersBySmallID;
    out.push(players.length);
    for (const p of players) {
      out.push(p.smallID());
      pushStr(out, p.id());
      pushStr(out, p.name());
      out.push(p.type() === "HUMAN" ? 0 : p.type() === "BOT" ? 1 : 2);
      const t = p.team();
      if (t === null) {
        out.push(-1);
      } else {
        out.push(-2);
        pushStr(out, t);
      }
      out.push(Number(p.troops()));
      out.push(Number(p.gold()));
      out.push(p._pseudo_random.next());
    }
  }

  // ---- M1b: the real executeNextTick loop + its setup/observation ops -----

  // GameImpl.executeNextTick (TS 488-546) — full real path: fresh
  // createGameUpdatesMap, tileUpdatePairs reset, empty execs/unInitExecs,
  // per-player recordTickSample + toUpdate, %10 Hash update, real
  // waterManager.tick flush, _ticks++. The updates map is pinned as JSON —
  // the same serialization the worker channel would carry (bigint fields
  // replaced via Number(), because JSON.stringify throws on bigint; every
  // other value is already plain JSON material). Payload:
  // [0, ticksAfter, gameHash, updatesJSON(encS)] — gameHash recomputes
  // 1 + Σ p.hash() post-tick as a cross-check of the Hash update inside the
  // JSON (identical state: nothing mutates between the update and the recompute).
  executeNextTick(out) {
    const g = this.game;
    const updates = g.executeNextTick();
    this.ticks = g._ticks;
    let h = 1;
    for (const p of g._players.values()) h += p.hash();
    out.push(0, g._ticks, h);
    pushStr(
      out,
      JSON.stringify(updates, (k, v) =>
        typeof v === "bigint" ? Number(v) : v,
      ),
    );
  }

  // StatsImpl.data snapshot (op 10): the clientID-keyed insertion-ordered
  // object, with optional-tag encoding for the three fields recordTickSample
  // touches (tiles triple, peakTroops, alliances array). recordTickSample
  // skips clientID-less players (bots/nations) and unspawned/dead players,
  // so entries exist only for sampled humans.
  statsSnapshot(out) {
    const data = this.game._stats.data;
    out.push(0);
    out.push(Object.keys(data).length);
    for (const cid of Object.keys(data)) {
      const ps = data[cid];
      pushStr(out, cid);
      if (ps.tiles === undefined) out.push(-1);
      else {
        out.push(-2, ps.tiles.length);
        for (const t of ps.tiles) out.push(Number(t));
      }
      if (ps.peakTroops === undefined) out.push(-1);
      else out.push(-2, Number(ps.peakTroops));
      if (ps.alliances === undefined) out.push(-1);
      else {
        out.push(-2, ps.alliances.length);
        for (const a of ps.alliances) out.push(Number(a));
      }
    }
  }

  // PlayerImpl.setSpawnTile (TS 726-728): flips hasSpawned and makes the
  // next toUpdate diff carry spawnTile. pidx indexes _playersBySmallID.
  setSpawnTile(pidx, tile, out) {
    this.game._playersBySmallID[pidx].setSpawnTile(tile);
    out.push(0);
  }

  // PlayerImpl.addTroops (TS 1345-1351): fractional amounts floor via
  // toInt into the bigint troops.
  addTroops(pidx, amount, out) {
    this.game._playersBySmallID[pidx].addTroops(amount);
    out.push(0);
  }

  // GameImpl.conquer (TS 761-783) — water / impassable throw paths included.
  conquer(pidx, tile, out) {
    try {
      this.game.conquer(this.game._playersBySmallID[pidx], tile);
      out.push(0);
    } catch (e) {
      out.push(1);
      pushStr(out, String(e.message));
    }
  }

  // GameImpl.drainPackedPlayerUpdates / drainPackedAttackUpdates
  // (TS 567-581): null when nothing accumulated (encoded [0, -1]), else
  // [0, len, ..values]. Successive drains must see null (the drain clears).
  drainPQ(out) {
    const r = this.game.drainPackedPlayerUpdates();
    if (r === null) {
      out.push(0, -1);
    } else {
      out.push(0, r.length, ...r);
    }
  }

  drainAQ(out) {
    const r = this.game.drainPackedAttackUpdates();
    if (r === null) {
      out.push(0, -1);
    } else {
      out.push(0, r.length, ...r);
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
const op = (kind, args = [], extra = {}) => ({ kind, args, ...extra });
const K = {
  CONSTRUCT: 0,
  SET_FALLOUT: 1,
  SET_WATER: 2,
  QUEUE_WATER: 3,
  TICK: 4,
  DRAIN: 5,
  DIGEST: 6,
  CONSTRUCT_GAME: 7,
  SNAPSHOT: 8,
  // M1b: the real executeNextTick loop + its setup/observation ops.
  EXECUTE_NEXT_TICK: 9,
  STATS_SNAPSHOT: 10,
  SET_SPAWN_TILE: 11,
  ADD_TROOPS: 12,
  CONQUER: 13,
  DRAIN_PQ: 14,
  DRAIN_AQ: 15,
};

// --------------------------------------------------- construct_game encoding
// Wire codes mirrored by game_loop.rs (op 7 parse + op 8 snapshot).

const MODE = { FFA: 0, TEAM: 1 };
const PT_TAG = { NUM: 0, DUOS: 1, TRIOS: 2, QUADS: 3, HVN: 4, OTHER: 5 };
const PTYPE = { HUMAN: 0, BOT: 1, NATION: 2 };
const DIFF = { EASY: 0, MEDIUM: 1, HARD: 2, IMPOSSIBLE: 3 };

function encPlayer(p, out) {
  pushStr(out, p.id);
  pushStr(out, p.name);
  out.push(PTYPE[p.playerType]);
  if (p.clientID === null) out.push(-1);
  else {
    out.push(-2);
    pushStr(out, p.clientID);
  }
  out.push(p.isLobbyCreator ? 1 : 0);
  if (p.clanTag === null) out.push(-1);
  else {
    out.push(-2);
    pushStr(out, p.clanTag);
  }
  out.push(p.friends.length);
  for (const f of p.friends) pushStr(out, f);
  if (p.teamIndex === null) out.push(-1);
  else out.push(p.teamIndex);
  if (p.nationFlag === null) out.push(-1);
  else {
    out.push(-2);
    pushStr(out, p.nationFlag);
  }
}

// construct_game args (op 7): the op-0 terrain prefix, then the Config
// surface (gameMode, playerTeams, disableNavMesh, waterNukes, difficulty,
// startingGold, hostCheats), then humans/nations as PlayerInfo runs.
function constructGameArgs({
  mode,
  pt,
  disableNavMesh = 0,
  waterNukes = 0,
  difficulty = DIFF.HARD,
  startingGold = -1,
  hcStartingGold = -1,
  hcInfiniteTroops = 0,
  infiniteTroops = 0,
  humans = [],
  nations = [],
}) {
  const a = [...constructArgs(main, mini)];
  a.push(mode);
  a.push(pt.tag);
  if (pt.tag === PT_TAG.NUM) a.push(pt.v);
  else if (pt.tag === PT_TAG.OTHER) pushStr(a, pt.v);
  a.push(disableNavMesh, waterNukes, difficulty);
  a.push(startingGold, hcStartingGold, hcInfiniteTroops, infiniteTroops);
  a.push(humans.length);
  for (const p of humans) encPlayer(p, a);
  a.push(nations.length);
  for (const p of nations) encPlayer(p, a);
  return a;
}

// The op-7 record carries both the flat wire args (Rust replay) and the live
// descriptor objects the real TS ctor consumes (recorder execution).
const PT_VAL = {
  [PT_TAG.NUM]: (v) => v,
  [PT_TAG.DUOS]: () => "Duos",
  [PT_TAG.TRIOS]: () => "Trios",
  [PT_TAG.QUADS]: () => "Quads",
  [PT_TAG.HVN]: () => "Humans Vs Nations",
  [PT_TAG.OTHER]: (v) => v,
};
const DIFF_VAL = ["Easy", "Medium", "Hard", "Impossible"];
function gameOp(desc) {
  return op(K.CONSTRUCT_GAME, constructGameArgs(desc), {
    cfg: {
      gameMode: desc.mode === MODE.TEAM ? "Team" : "Free For All",
      playerTeams: PT_VAL[desc.pt.tag](desc.pt.v),
      disableNavMesh: desc.disableNavMesh === 1,
      waterNukes: desc.waterNukes === 1,
      difficulty: DIFF_VAL[desc.difficulty],
      startingGold: desc.startingGold === -1 ? undefined : desc.startingGold,
      hostCheats:
        desc.hcStartingGold === -1 && desc.hcInfiniteTroops !== 1
          ? undefined
          : {
              startingGold:
                desc.hcStartingGold === -1 ? undefined : desc.hcStartingGold,
              infiniteTroops: desc.hcInfiniteTroops === 1 || undefined,
            },
      infiniteTroops: desc.infiniteTroops === 1,
    },
    humans: desc.humans ?? [],
    // Live ctor call: GameImpl reads `n.playerInfo` on nation entries —
    // Game.ts `Nation` is a `{spawnCell, playerInfo}` holder (spawnCell
    // stays undefined until the spawn phase, M1c). The wire side keeps
    // encoding the bare PlayerInfos.
    nations: (desc.nations ?? []).map((pi) => ({
      spawnCell: undefined,
      playerInfo: pi,
    })),
  });
}

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

// ---- M1a: createGame factory + ctor assembly (real Config + players) -----

// 6. FFA, single human: the FFA addPlayer path (maybeAssignTeam -> null
//    team), Human startManpower 25000 / startingGold 0, plus the
//    waterNukes=false fallback arm of queueWaterConversion (setFallout).
{
  const ops = [
    gameOp({
      mode: MODE.FFA,
      pt: { tag: PT_TAG.NUM, v: 0 },
      waterNukes: 0,
      humans: [makeInfo("Alice", "HUMAN", { clientID: "c1" })],
    }),
    op(K.SNAPSHOT),
    op(K.QUEUE_WATER, [P.landA]),
    op(K.DIGEST),
    op(K.DRAIN),
  ];
  scenarios.push({ name: "m1a_ffa_single_human", ops });
}

// 7. FFA, humans + bots + nations: humans-then-nations addPlayer order,
//    Nation startManpower under Medium difficulty, absent startingGold.
{
  const ops = [
    gameOp({
      mode: MODE.FFA,
      pt: { tag: PT_TAG.NUM, v: 0 },
      difficulty: DIFF.MEDIUM,
      humans: [
        makeInfo("Alice", "HUMAN", {
          clientID: "c1",
          isLobbyCreator: true,
          clanTag: "AAA",
        }),
        makeInfo("Bob", "HUMAN", { clientID: "c2", friends: ["c9"] }),
        makeInfo("Bot 1", "BOT"),
      ],
      nations: [
        makeInfo("Nation A", "NATION", { id: "nation-a" }),
        makeInfo("Nation B", "NATION", { id: "nation-b" }),
      ],
    }),
    op(K.SNAPSHOT),
    op(K.DIGEST),
  ];
  scenarios.push({ name: "m1a_ffa_humans_nations", ops });
}

// 8. Team mode, numeric 2 teams, 8 mixed players: clan grouping, a friend
//    edge, a teamIndex pin, bots through assignTeams's otherPlayers pass.
{
  const ops = [
    gameOp({
      mode: MODE.TEAM,
      pt: { tag: PT_TAG.NUM, v: 2 },
      humans: [
        makeInfo("P1", "HUMAN", { clientID: "c1", clanTag: "AAA" }),
        makeInfo("P2", "HUMAN", { clientID: "c2", clanTag: "AAA" }),
        makeInfo("P3", "HUMAN", { clientID: "c3", friends: ["c4"] }),
        makeInfo("P4", "HUMAN", { clientID: "c4", friends: ["c3"] }),
        makeInfo("P5", "HUMAN", { clientID: "c5", teamIndex: 1 }),
        makeInfo("P6", "HUMAN", { clientID: "c6" }),
        makeInfo("Bot 1", "BOT"),
        makeInfo("Bot 2", "BOT"),
      ],
    }),
    op(K.SNAPSHOT),
  ];
  scenarios.push({ name: "m1a_team_numeric8", ops });
}

// 9. Team mode Duos: clan overflow kicks (a 3-member clan over
//    maxTeamSize=2 leaves one player un-added; nextPlayerID counts only the
//    added ones).
{
  const ops = [
    gameOp({
      mode: MODE.TEAM,
      pt: { tag: PT_TAG.DUOS },
      humans: [
        makeInfo("C1", "HUMAN", { clientID: "c1", clanTag: "BIG" }),
        makeInfo("C2", "HUMAN", { clientID: "c2", clanTag: "BIG" }),
        makeInfo("C3", "HUMAN", { clientID: "c3", clanTag: "BIG" }),
        makeInfo("Solo", "HUMAN", { clientID: "c4" }),
      ],
    }),
    op(K.SNAPSHOT),
    op(K.DIGEST),
  ];
  scenarios.push({ name: "m1a_team_duos_kick", ops });
}

// 10. HumansVsNations: fixed Humans/Nations teams over both populations.
{
  const ops = [
    gameOp({
      mode: MODE.TEAM,
      pt: { tag: PT_TAG.HVN },
      humans: [
        makeInfo("H1", "HUMAN", { clientID: "c1" }),
        makeInfo("H2", "HUMAN", { clientID: "c2" }),
      ],
      nations: [
        makeInfo("Nation A", "NATION", { id: "nation-a" }),
        makeInfo("Nation B", "NATION", { id: "nation-b" }),
      ],
    }),
    op(K.SNAPSHOT),
  ];
  scenarios.push({ name: "m1a_team_hvn", ops });
}

// 11. Server-pinned team slots honored unconditionally (before clan /
//     friend grouping, seeding the counts the balancing sees).
{
  const ops = [
    gameOp({
      mode: MODE.TEAM,
      pt: { tag: PT_TAG.NUM, v: 3 },
      humans: [
        makeInfo("P1", "HUMAN", { clientID: "c1", teamIndex: 0 }),
        makeInfo("P2", "HUMAN", { clientID: "c2", teamIndex: 1 }),
        makeInfo("P3", "HUMAN", { clientID: "c3", teamIndex: 1 }),
        makeInfo("P4", "HUMAN", { clientID: "c4", teamIndex: 2 }),
        makeInfo("P5", "HUMAN", { clientID: "c5" }),
      ],
    }),
    op(K.SNAPSHOT),
  ];
  scenarios.push({ name: "m1a_team_pinned", ops });
}

// 12. resolveTeamsList throw: numeric 1 team is below the 2-team minimum.
{
  const ops = [
    gameOp({
      mode: MODE.TEAM,
      pt: { tag: PT_TAG.NUM, v: 1 },
      humans: [makeInfo("Alice", "HUMAN", { clientID: "c1" })],
    }),
  ];
  scenarios.push({ name: "m1a_too_few_teams", ops });
}

// 13. resolveTeamsList throw: unknown TeamCountConfig string.
{
  const ops = [
    gameOp({
      mode: MODE.TEAM,
      pt: { tag: PT_TAG.OTHER, v: "Bogus" },
      humans: [makeInfo("Alice", "HUMAN", { clientID: "c1" })],
    }),
  ];
  scenarios.push({ name: "m1a_unknown_team_config", ops });
}

// 14. Host cheats: the lobby creator gets the bonus gold, the other human
//     does not; nations ride the base startingGold.
{
  const ops = [
    gameOp({
      mode: MODE.TEAM,
      pt: { tag: PT_TAG.NUM, v: 2 },
      startingGold: 100,
      hcStartingGold: 1000,
      humans: [
        makeInfo("Host", "HUMAN", { clientID: "c1", isLobbyCreator: true }),
        makeInfo("Guest", "HUMAN", { clientID: "c2" }),
      ],
      nations: [makeInfo("Nation A", "NATION", { id: "nation-a" })],
    }),
    op(K.SNAPSHOT),
  ];
  scenarios.push({ name: "m1a_host_cheats", ops });
}

// 15. Game + water pipeline coexistence: real queueWaterConversion with
//     waterNukes=true flows into the real WaterManager flush across ticks,
//     with players present (the hasOwner throw arms stay unreachable —
//     nobody owns tiles until the spawn phase, M1c).
{
  const ops = [
    gameOp({
      mode: MODE.TEAM,
      pt: { tag: PT_TAG.NUM, v: 2 },
      waterNukes: 1,
      humans: [
        makeInfo("H1", "HUMAN", { clientID: "c1" }),
        makeInfo("H2", "HUMAN", { clientID: "c2" }),
      ],
    }),
    op(K.QUEUE_WATER, [P.landA]),
    op(K.QUEUE_WATER, [P.landB]),
    op(K.TICK, [5]),
    op(K.TICK, [26]),
    op(K.SNAPSHOT),
    op(K.DIGEST),
    op(K.DRAIN),
  ];
  scenarios.push({ name: "m1a_game_water_interleave", ops });
}

// ---- M1b: the real executeNextTick loop + player updates + stats ---------

// 16. Unspawned FFA solo over the %10 hash boundary: every tick emits a full
//     PlayerUpdate on the first call and null diffs after (nothing changes
//     for a troopless, tileless human), Hash updates at ticks 0 and 10, the
//     recordTickSample gate stays closed (unspawned), and all three drains
//     come back empty/null.
{
  const ops = [
    gameOp({
      mode: MODE.FFA,
      pt: { tag: PT_TAG.NUM, v: 0 },
      humans: [makeInfo("Alice", "HUMAN", { clientID: "c1" })],
    }),
    ...Array.from({ length: 11 }, () => op(K.EXECUTE_NEXT_TICK)),
    op(K.DRAIN),
    op(K.DRAIN_PQ),
    op(K.DRAIN_AQ),
    op(K.STATS_SNAPSHOT),
    op(K.DIGEST),
  ];
  scenarios.push({ name: "m1b_single_unspawned_11t", ops });
}

// 17. FFA five-way (humans + bot + nations) over 35 ticks: per-player full
//     → null-diff cadence across four hash boundaries (0/10/20/30), with
//     startManpower differences (HUMAN 25000, BOT 10000, NATION 18750 under
//     Medium) visible in the full updates and the game hash.
{
  const ops = [
    gameOp({
      mode: MODE.FFA,
      pt: { tag: PT_TAG.NUM, v: 0 },
      difficulty: DIFF.MEDIUM,
      humans: [
        makeInfo("Alice", "HUMAN", {
          clientID: "c1",
          isLobbyCreator: true,
          clanTag: "AAA",
        }),
        makeInfo("Bob", "HUMAN", { clientID: "c2", friends: ["c9"] }),
        makeInfo("Bot 1", "BOT"),
      ],
      nations: [
        makeInfo("Nation A", "NATION", { id: "nation-a" }),
        makeInfo("Nation B", "NATION", { id: "nation-b" }),
      ],
    }),
    ...Array.from({ length: 35 }, () => op(K.EXECUTE_NEXT_TICK)),
    op(K.DRAIN),
    op(K.DRAIN_PQ),
    op(K.DRAIN_AQ),
    op(K.STATS_SNAPSHOT),
    op(K.DIGEST),
  ];
  scenarios.push({ name: "m1b_mixed_ffa_35t", ops });
}

// 18. Spawn midstream: two pre-spawn ticks, then setSpawnTile + three
//     conquers (two tiles for Alice, one for Bob), a mid-scenario tile
//     drain (executeNextTick resets tileUpdatePairs at tick start, so the
//     conquer pairs must be drained before the next tick), the tick that
//     diffs isAlive/hasSpawned/spawnTile and flushes tiles quints, a
//     fractional addTroops whose quint-only tick follows, then a null
//     diff tick, the stats snapshot (peakTroops moved) and final drains.
{
  const ops = [
    gameOp({
      mode: MODE.FFA,
      pt: { tag: PT_TAG.NUM, v: 0 },
      humans: [
        makeInfo("Alice", "HUMAN", { clientID: "c1" }),
        makeInfo("Bob", "HUMAN", { clientID: "c2" }),
        makeInfo("Bot 1", "BOT"),
      ],
    }),
    op(K.EXECUTE_NEXT_TICK),
    op(K.EXECUTE_NEXT_TICK),
    op(K.SET_SPAWN_TILE, [0, P.landA]),
    op(K.CONQUER, [0, P.landA]),
    op(K.CONQUER, [0, P.landB]),
    op(K.CONQUER, [1, P.landC]),
    op(K.DRAIN),
    op(K.EXECUTE_NEXT_TICK),
    op(K.DRAIN_PQ),
    op(K.DRAIN_PQ),
    op(K.ADD_TROOPS, [0, 1234.7]),
    op(K.EXECUTE_NEXT_TICK),
    op(K.DRAIN_PQ),
    op(K.EXECUTE_NEXT_TICK),
    op(K.STATS_SNAPSHOT),
    op(K.DRAIN),
    op(K.DRAIN_AQ),
    op(K.DIGEST),
  ];
  scenarios.push({ name: "m1b_spawn_midstream", ops });
}

// 19. Real water flush inside executeNextTick: queued conversions with
//     waterNukes=true are flushed by the game's own waterManager.tick and
//     land in the tick's tileUpdatePairs (recordTileUpdate at TS 541-543).
//     Seven ticks straddle the queue-flush boundary; the drain after the
//     last tick must be empty (tick-start reset + in-tick consumption).
{
  const ops = [
    gameOp({
      mode: MODE.FFA,
      pt: { tag: PT_TAG.NUM, v: 0 },
      waterNukes: 1,
      humans: [
        makeInfo("Alice", "HUMAN", { clientID: "c1" }),
        makeInfo("Bob", "HUMAN", { clientID: "c2" }),
      ],
    }),
    op(K.QUEUE_WATER, [P.landA]),
    op(K.QUEUE_WATER, [P.landB]),
    ...Array.from({ length: 7 }, () => op(K.EXECUTE_NEXT_TICK)),
    op(K.DRAIN),
    op(K.DRAIN_PQ),
    op(K.DIGEST),
  ];
  scenarios.push({ name: "m1b_water_flush_game", ops });
}

// 20. conquer throw paths: water and impassable terrain reject with the
//     exact error strings, the land conquest succeeds; the player is alive
//     but hasSpawned stays false (no setSpawnTile), so recordTickSample
//     never fires and the stats snapshot stays empty while the full update
//     already carries tilesOwned=1.
{
  const ops = [
    gameOp({
      mode: MODE.FFA,
      pt: { tag: PT_TAG.NUM, v: 0 },
      humans: [makeInfo("Alice", "HUMAN", { clientID: "c1" })],
    }),
    op(K.CONQUER, [0, P.oceanA]),
    op(K.CONQUER, [0, P.impassableA]),
    op(K.CONQUER, [0, P.landA]),
    op(K.DRAIN),
    op(K.EXECUTE_NEXT_TICK),
    op(K.EXECUTE_NEXT_TICK),
    op(K.DRAIN_PQ),
    op(K.STATS_SNAPSHOT),
    op(K.DIGEST),
  ];
  scenarios.push({ name: "m1b_conquer_throws", ops });
}

// 21. Team-mode JSON shape: eight players across two numeric teams exercise
//     non-null `team` in the full updates (declaration-order key set with
//     team present, unlike the FFA scenarios), clan-tagged displayName,
//     pinned/friend/clan team assignment riding into the player loop, plus
//     spawn + conquer-driven diffs and the sampled-stats snapshot.
{
  const ops = [
    gameOp({
      mode: MODE.TEAM,
      pt: { tag: PT_TAG.NUM, v: 2 },
      humans: [
        makeInfo("P1", "HUMAN", { clientID: "c1", clanTag: "AAA" }),
        makeInfo("P2", "HUMAN", { clientID: "c2", clanTag: "AAA" }),
        makeInfo("P3", "HUMAN", { clientID: "c3", friends: ["c4"] }),
        makeInfo("P4", "HUMAN", { clientID: "c4", friends: ["c3"] }),
        makeInfo("P5", "HUMAN", { clientID: "c5", teamIndex: 1 }),
        makeInfo("P6", "HUMAN", { clientID: "c6" }),
        makeInfo("Bot 1", "BOT"),
        makeInfo("Bot 2", "BOT"),
      ],
    }),
    op(K.SET_SPAWN_TILE, [0, P.landA]),
    op(K.CONQUER, [0, P.landA]),
    op(K.CONQUER, [1, P.landC]),
    ...Array.from({ length: 3 }, () => op(K.EXECUTE_NEXT_TICK)),
    op(K.DRAIN),
    op(K.DRAIN_PQ),
    op(K.STATS_SNAPSHOT),
    op(K.DIGEST),
  ];
  scenarios.push({ name: "m1b_team_spawn_json", ops });
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
      case K.CONSTRUCT_GAME:
        rig.constructGame(step.cfg, step.humans, step.nations, out);
        break;
      case K.SNAPSHOT:
        rig.snapshot(out);
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
      case K.EXECUTE_NEXT_TICK:
        rig.executeNextTick(out);
        break;
      case K.STATS_SNAPSHOT:
        rig.statsSnapshot(out);
        break;
      case K.SET_SPAWN_TILE:
        rig.setSpawnTile(step.args[0], step.args[1], out);
        break;
      case K.ADD_TROOPS:
        rig.addTroops(step.args[0], step.args[1], out);
        break;
      case K.CONQUER:
        rig.conquer(step.args[0], step.args[1], out);
        break;
      case K.DRAIN_PQ:
        rig.drainPQ(out);
        break;
      case K.DRAIN_AQ:
        rig.drainAQ(out);
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
