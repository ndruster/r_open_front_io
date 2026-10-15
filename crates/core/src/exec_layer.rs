//! Bit-exact port of the G5a execution layer part 1: the four alliance
//! executions plus eight trivial leaf executions, and the `Game`-facade helper
//! `wouldNukeBreakAlliance` (`src/core/execution/Util.ts`), which is needed by
//! `AllianceRequestExecution.cancelNukesBetweenAlliedPlayers`.
//!
//! Ported TS files (READ-ONLY source `d:\work_space\OpenFrontIO`):
//!
//! * `src/core/execution/NoOpExecution.ts`
//! * `src/core/execution/PauseExecution.ts`
//! * `src/core/execution/SpawnTimerExecution.ts`
//! * `src/core/execution/RecomputeRailClusterExecution.ts`
//! * `src/core/execution/MarkDisconnectedExecution.ts`
//! * `src/core/execution/EmbargoExecution.ts`
//! * `src/core/execution/EmbargoAllExecution.ts`
//! * `src/core/execution/TargetPlayerExecution.ts`
//! * `src/core/execution/alliance/AllianceRequestExecution.ts`
//! * `src/core/execution/alliance/AllianceExtensionExecution.ts`
//! * `src/core/execution/alliance/BreakAllianceExecution.ts`
//! * `src/core/execution/alliance/AllianceRejectExecution.ts`
//! * `src/core/execution/Util.ts` (`wouldNukeBreakAlliance` only; the pure
//!   `GameMap` subset — `computeNukeBlastCounts` / `getSpawnTiles` /
//!   `closestTile` / `nearestTileDist` / `nearestTileDistCapped` /
//!   `closestTwoTiles` — already lives in [`crate::exec_util`]).
//!
//! # Harness shape
//!
//! One [`ExecHarness`] carries the scenario state across ops (like a scenario's
//! class instance plus its scripted facade mocks). Each call to
//! [`ExecHarness::run_op`] is one `kind`:
//!
//! * `kind 0` construct — `args` is ONE `js_json`-encoded object
//!   `{c, a, mg, players, alliances, requests, units, neighbors}` (see the
//!   script format below). Returns `[0, 0]` (trace length 0, then the `0`
//!   marker): no ctor trace is kept. Class `31` (`TribeSpawner`) is the sole
//!   ctor that reads the facade — its `gameMap` / per-cell `isValidCoord` /
//!   `ref` events are replayed into a scratch buffer so the script cursors
//!   advance exactly as in the capture.
//! * `kind 1` `init(ticks)` — `args = [ticks]`.
//! * `kind 2` `tick(ticks)` — `args = [ticks]`.
//! * `kind 3` `isActive()` — `args = []`.
//! * `kind 4` `activeDuringSpawnPhase()` — `args = []`.
//! * `kind 5` `TribeSpawner.spawnTribes(numTribes, purchasedNames)` —
//!   `args = [numTribes, nameCount, (len, c0, c1, ...)*nameCount]` (a flat
//!   list; class `31` only).
//!
//! Method ops return `[traceLen, (trace)*, payload]` where `payload` is
//! `[0, ...encVal(ret)]` on success or `[1, ...encS(errMessage)]` for a thrown
//! JS `Error` / `TypeError` (the `[1, …]` arm mirrors the capture's
//! `[1, ...encS(msg)]`).
//!
//! # Class id table (mirrored by `tools/gen_vectors.mjs`)
//!
//! `0` NoOp, `1` Pause, `2` SpawnTimer, `3` RecomputeRailCluster,
//! `4` MarkDisconnected, `5` Embargo, `6` EmbargoAll, `7` TargetPlayer,
//! `8` AllianceRequest, `9` AllianceExtension, `10` BreakAlliance,
//! `11` AllianceReject.
//!
//! G5b appends `12` Retreat, `13` BoatRetreat, `14` MoveWarship, `15`
//! DeleteUnit, `16` QuickChat, `17` MissileSilo, `18` UpgradeStructure,
//! `19` City, `20` Factory, `21` DefensePost.
//!
//! G5c appends `22` Emoji, `23` DonateTroop, `24` DonateGold
//! (`DonateTroopsExecution` / `DonateGoldExecution` also exercise the
//! module-level `respondToEmoji` from `nation/NationEmojiBehavior.ts`).
//!
//! G5d appends `25` WinCheck, `26` Shell, `27` SAMMissile, `28` Port,
//! `29` Construction (`WinCheckExecution`, `ShellExecution`,
//! `SAMMissileExecution`, `PortExecution`, `ConstructionExecution`).
//!
//! G5e appends `30` DoomsdayClock, `31` TribeSpawner, `32` Spawn
//! (`DoomsdayClockExecution` + its module-local `LowestN` helper, `TribeSpawner`,
//! `SpawnExecution`). Class `31` is NOT an `Execution`: only op kinds `0`
//! (construct) and `5` (`spawnTribes`) apply to it (kinds 1..4 unreachable).
//!
//! G5f appends `33` Nuke, `34` MIRV, `35` SAMLauncher (`NukeExecution`,
//! `MirvExecution`, `SAMLauncherExecution`).
//!
//! # G5b facade additions (codes ≥ 300)
//!
//! `mg`: `300` isValidRef, `301` getWaterComponent, `302` hasWaterComponent,
//! `303` unit, `304` isLand, `305` hasUnitNearby, `306` nearbyUnits,
//! `307` addExecution (construction record), `308` displayChat,
//! `309` config().trainStationMaxRange(), `310` config().SiloCooldown(),
//! `311` config().defensePostShellAttackRate() (only reachable from the
//! unreachable `DefensePost.shoot`).
//! `player`: `320` orderRetreat, `321` executeRetreat, `322` canSendQuickChat,
//! `323` recordQuickChat, `324` canDeleteUnit, `325` recordDeleteUnit,
//! `326` canUpgradeUnit, `327` upgradeUnit.
//! `unit`: `340` id, `341` updateTransportShipState, `342` updateWarshipState,
//! `343` setTargetTile, `344` tile, `345` isUnderConstruction,
//! `346` missileTimerQueue, `347` reloadMissile, `348` markForDeletion,
//! `349` isOverdueDeletion, `350` hasTrainStation, `351` setTrainStation,
//! `352` hasHealth (only reachable from the unreachable `DefensePost.shoot`).
//!
//! # G5c facade additions (codes ≥ 400)
//!
//! `mg`: `400` config().defaultDonationAmount(player),
//! `401` config().maxTroops(player), `402` config().gameConfig().difficulty
//! (the `gameConfig()` object is already reached for `214` gameType).
//! `player`: `410` canSendEmoji(recipient), `411` sendEmoji(recipient, str),
//! `412` relation(other), `413` canDonateTroops(other),
//! `414` donateTroops(other, troops), `415` canDonateGold(other),
//! `416` donateGold(other, gold), `417` gold(), `418` troops(), `419` name().
//! A facade argument that may hold the `AllPlayers` sentinel (the plain string
//! `"AllPlayers"`) or a Player token is encoded as `[5, …]` / `[3, tok]`
//! (`undefined` `[1]`, `null` `[2]`); JS `===` on `AllPlayers` is a value
//! comparison. `new EmojiExecution(...)` (from `DonateTroopsExecution` /
//! `DonateGoldExecution` / `respondToEmoji`) is an `addExecution` construction
//! record on code `307` with tag `2` (ctor args: requestor token, recipientID
//! string, emoji id).
//!
//! # G5d facade additions (codes ≥ 500)
//!
//! `mg`: `500` allPlayers(), `501` numLandTiles(), `502` numTilesWithFallout(),
//! `503` elapsedGameSeconds(), `504` stats().stats(), `505` setWinner(winner,
//! stats), `506` unitCount(type), `507` neighbors(tile), `508` isWater(tile),
//! `509` manhattanDist(a, b), `510` config().percentageTilesOwnedToWin(t),
//! `511` config().isUnitDisabled(type), `512` unitInfo(type),
//! `513` config().unitInfo(type), `514` config().shellLifetime(),
//! `515` config().warshipVeterancyShellDamageBonus(),
//! `516` config().defaultSamMissileSpeed(),
//! `517` config().tradeShipSpawnRate(rejections, n),
//! `518` config().tradeShipShortRangeDebuff(),
//! `519` config().proximityBonusPortsNb(n),
//! `520` config().gameConfig().gameMode, `521` config().gameConfig().rankedType,
//! `522` config().gameConfig().maxPlayers,
//! `523` config().gameConfig().maxTimerValue,
//! `524` stats().bombIntercept(player, type, n),
//! `525` `PathFinding.Air(mg).next(from, to)` (the Shell / SAM stepper).
//! `mg.setWinner`'s `winner` is a Player token (`[3, tok]`), a Team string
//! (`[5, …]`) or `null` (`[2]`). The pathfinder is a scripted facade object
//! replayed by `__stub/pathfinding.ts` (`PathFinding.Air(mg)` reads
//! `game.__pathFinder`); `PseudoRandom` is the real ported sfc32.
//! `player`: `530` hasSpawned(), `531` isDisconnected(), `532` numTilesOwned(),
//! `533` canTrade(other), `534` canBuild(type, tile),
//! `535` buildUnit(type, tile, params) (returns a Unit token).
//! `unit`: `540` level(), `541` veterancy(), `542` recordKill(type),
//! `543` modifyHealth(delta, owner), `544` setReachedTarget(), `545` move(node),
//! `546` setUnderConstruction(flag), `547` setTargetedBySAM(flag).
//!
//! # Constructing un-ported downstream executions
//!
//! `TrainStationExecution` (and `ShellExecution`) are intentionally NOT ported.
//! `mg.addExecution(new TrainStationExecution(unit, spawnTrains?))` is modelled
//! as an `addExecution` facade event (code 307) whose prefix is the construct
//! tag plus the encoded ctor args — exactly how `execution_manager.rs` records
//! `new XxxExecution(...)`. On the capture side we import the REAL
//! `TrainStationExecution` class (option (a)): its ctor runs against the
//! scripted unit mock and therefore emits `unit.setTrainStation(true)` FIRST
//! (code 351), then the `addExecution` record — this keeps the trace faithful
//! to the actual TS runtime and is mirrored verbatim on the Rust side
//! (`mg_add_train_station` calls `un_set_train_station` then pushes the
//! record). `ShellExecution` is likewise un-ported; its only construction site
//! is the unreachable `DefensePost.shoot`, so it is modelled by
//! `mg_add_execution_shell` (tag 1) but is never exercised by any scenario.
//!
//! G5d extends the construction-record tag space (allocated from `3` upward):
//! `3` Nuke `(nukeType, player, dst, src, speed, waitTicks, rocketDirectionUp)`,
//! `4` MIRV `(player, tile)`, `5` Warship `({owner, patrolTile})`,
//! `6` Port `(port)`, `7` MissileSilo `(silo)`, `8` DefensePost `(post)`,
//! `9` SAMLauncher `(player, tileArg, sam)`, `10` City `(city)`,
//! `11` Factory `(factory)`, `12` TradeShip `(origOwner, srcPort, dstPort)`.
//! These downstream classes are stubs (`__stub/downstream.ts`); their
//! `addExecution` recorder records the ctor args, and the Rust side emits the
//! same record (see `mg_add_nuke` / `mg_add_mirv` / `mg_add_warship` /
//! `mg_add_unit_exec` / `mg_add_sam_launcher` / `mg_add_trade_ship`). The one
//! exception is `SAMLauncherExecution`, whose ctor side effect `sam.tile()`
//! (code 344) fires before the record when the `sam` argument is non-null.
//!
//! # Facade (mock) surface — the capture scripts these, every call traced
//!
//! Players / alliances / alliance-requests / units / neighbors cross as TOKENS
//! (index into the harness tables); the TS `===` reference-identity gates are
//! modelled as token equality (the `ui_` / `alliance_impl` token convention).
//! Scripted returns are consumed FIFO per scenario; an exhausted list REPEATS
//! its last value on both sides, and an empty list throws on the first call
//! (so every used method must be scripted with at least one value).
//!
//! ## `mg` (Game) facade — slots 0..18
//!
//! | slot | code | method |
//! |------|------|--------|
//! | 0  | 210 | `hasPlayer(id)` |
//! | 1  | 211 | `player(id)` |
//! | 2  | 212 | `ticks()` |
//! | 3  | 213 | `config().allianceRequestDuration()` |
//! | 4  | 214 | `config().gameConfig().gameType` (read) |
//! | 5  | 215 | `config().numSpawnPhaseTurns()` |
//! | 6  | 216 | `inSpawnPhase()` |
//! | 7  | 217 | `endSpawnPhase()` |
//! | 8  | 218 | `setPaused(paused)` |
//! | 9  | 219 | `teamTilesOwned(team)` |
//! | 10 | 220 | `totalLandTiles()` |
//! | 11 | 221 | `players()` |
//! | 12 | 222 | `owner(tile)` |
//! | 13 | 223 | `ownerID(tile)` |
//! | 14 | 224 | `config().nukeMagnitudes(type)` |
//! | 15 | 225 | `config().nukeAllianceBreakThreshold()` |
//! | 16 | 226 | `displayMessage(...)` (7 args) |
//! | 17 | 227 | `anyUnitNearby(tile, outer, types, cb)` |
//! | 18 | 228 | `circleSearch(tile, outer, cb)` |
//! | 19 | 300 | `isValidRef(tile)` |
//! | 20 | 301 | `getWaterComponent(tile)` |
//! | 21 | 302 | `hasWaterComponent(tile, comp)` |
//! | 22 | 303 | `unit(id)` |
//! | 23 | 304 | `isLand(tile)` |
//! | 24 | 305 | `hasUnitNearby(tile, outer, type)` |
//! | 25 | 306 | `nearbyUnits(tile, outer, types)` |
//! | 28 | 309 | `config().trainStationMaxRange()` |
//! | 29 | 310 | `config().SiloCooldown()` |
//! | 30 | 311 | `config().defensePostShellAttackRate()` (unreachable) |
//!
//! G5d mg slots (codes ≥ 500): `34` 500 `allPlayers()`, `35` 501
//! `numLandTiles()`, `36` 502 `numTilesWithFallout()`, `37` 503
//! `elapsedGameSeconds()`, `38` 504 `stats().stats()`, `39` 505
//! `setWinner(winner, stats)`, `40` 506 `unitCount(type)`, `41` 507
//! `neighbors(tile)`, `42` 508 `isWater(tile)`, `43` 509 `manhattanDist(a, b)`,
//! `44` 510 `config().percentageTilesOwnedToWin(t)`, `45` 511
//! `config().isUnitDisabled(type)`, `46` 512 `unitInfo(type)`, `47` 513
//! `config().unitInfo(type)`, `48` 514 `config().shellLifetime()`, `49` 515
//! `config().warshipVeterancyShellDamageBonus()`, `50` 516
//! `config().defaultSamMissileSpeed()`, `51` 517
//! `config().tradeShipSpawnRate(rejections, n)`, `52` 518
//! `config().tradeShipShortRangeDebuff()`, `53` 519
//! `config().proximityBonusPortsNb(n)`, `54` 520
//! `config().gameConfig().gameMode`, `55` 521 `config().gameConfig().rankedType`,
//! `56` 522 `config().gameConfig().maxPlayers`, `57` 523
//! `config().gameConfig().maxTimerValue`, `59` 525 `next(from, to)` (the
//! pathfinder object; code 524 `stats().bombIntercept(...)` is void).
//!
//! `307` `addExecution(...)` (construction record) and `308` `displayChat(...)`
//! are synthetic events: they consume no script slot on either side.
//!
//! `config()` / `gameConfig()` are live-object accessors; only the leaf
//! accessor is traced (one event per full chain read).
//!
//! ## `player` facade — slots 0..33
//!
//! `0` 230 `smallID()`, `1` 231 `id()`, `2` 232 `displayName()`,
//! `3` 233 `isAlive()`, `4` 234 `isLobbyCreator()`,
//! `5` 235 `canSendAllianceRequest(other)` (BOOL),
//! `6` 236 `outgoingAllianceRequests()`, `7` 237 `updateRelation(other, amt)`,
//! `8` 238 `hasEmbargoAgainst(other)` (BOOL),
//! `9` 239 `endTemporaryEmbargo(other)`, `10` 240 `createAllianceRequest(other)`,
//! `11` 241 `units(types)`, `12` 242 `isFriendly(other)` (BOOL),
//! `13` 243 `allianceWith(other)`, `14` 244 `breakAlliance(alliance)`,
//! `15` 245 `nearby()`, `16` 246 `canTarget(other)` (BOOL), `17` 247 `target(other)`,
//! `18` 248 `team()`, `19` 249 `markDisconnected(flag, obj|none)`,
//! `20` 250 `isOnSameTeam(other)` (BOOL), `21` 251 `type()`,
//! `22` 252 `canEmbargoAll()` (BOOL), `23` 253 `addEmbargo(other, flag)`,
//! `24` 254 `stopEmbargo(other)`, `25` 255 `recordEmbargoAll()`,
//! `26` 259 `isPlayer()` (BOOL — the `anyUnitNearby` callback reads the unit's
//! owner through it), `29` 322 `canSendQuickChat(other)` (BOOL),
//! `31` 324 `canDeleteUnit()` (BOOL), `33` 326 `canUpgradeUnit(unit)` (BOOL).
//! The void G5b additions (`320` orderRetreat, `321` executeRetreat,
//! `323` recordQuickChat, `325` recordDeleteUnit, `327` upgradeUnit) consume
//! no slot.
//!
//! G5d player slots (codes ≥ 530): `43` 530 `hasSpawned()`, `44` 531
//! `isDisconnected()`, `45` 532 `numTilesOwned()`, `46` 533 `canTrade(other)`,
//! `47` 534 `canBuild(type, tile)`, `48` 535 `buildUnit(type, tile, params)`.
//!
//! ## `alliance` facade — slots 0..3
//!
//! `0` 260 `onlyOneAgreedToExtend()`, `1` 261 `addExtensionRequest(player)`,
//! `2` 262 `bothAgreedToExtend()`, `3` 263 `extend()`.
//!
//! ## `allianceRequest` facade — slots 0..4
//!
//! `0` 270 `recipient()`, `1` 271 `accept()`, `2` 272 `reject()`,
//! `3` 273 `status()`, `4` 274 `createdAt()`.
//!
//! ## `unit` facade — slots 0..19
//!
//! `0` 280 `isActive()`, `1` 281 `reachedTarget()`, `2` 282 `targetTile()`,
//! `3` 283 `type()`, `4` 284 `targetPlayer()`, `5` 285 `delete(flag)`,
//! `6` 286 `owner()`, `7` 340 `id()`, `11` 344 `tile()`,
//! `12` 345 `isUnderConstruction()`, `13` 346 `missileTimerQueue()`,
//! `16` 349 `isOverdueDeletion()`, `17` 350 `hasTrainStation()`,
//! `19` 352 `hasHealth()` (unreachable). The void G5b additions
//! (`341` updateTransportShipState, `342` updateWarshipState,
//! `343` setTargetTile, `347` reloadMissile, `348` markForDeletion,
//! `351` setTrainStation) consume no slot.
//!
//! G5d unit slots (codes ≥ 540): `20` 540 `level()`, `21` 541 `veterancy()`,
//! `22` 542 `recordKill(type)`, `23` 543 `modifyHealth(delta, owner)`,
//! `24` 544 `setReachedTarget()`, `25` 545 `move(node)`, `26` 546
//! `setUnderConstruction(flag)`, `27` 547 `setTargetedBySAM(flag)`.
//!
//! ## G5e facade additions
//!
//! `mg` slots 60..78: `60` 600 `getWinner()`, `61` 601
//! `config().doomsdayClockConfig()` (the 15-key cfg object), `62` 602
//! `setFallout(tile, fallout)`, `63` 603 `x(tile)`, `64` 604 `y(tile)`,
//! `65` 605 `addPlayer(playerInfo)`, `66` 606 `config().isRandomSpawn()` (BOOL),
//! `67` 607 `hasOwner(tile)` (BOOL), `68` 608 `isBorder(tile)` (BOOL),
//! `69` 609 `ref(x, y)`, `70` 610 `width()`, `71` 611 `height()`, `72` 612
//! `config().minDistanceBetweenPlayers()`, `73` 613 `teamSpawnArea(team)`,
//! `74` 614 `config().gameConfig().gameMap`, `75` 615
//! `config().gameConfig().gameMapSize`, `76` 616 `isValidCoord(x, y)` (BOOL),
//! `77` 617 `isImpassable(tile)` (BOOL), `78` 618 `getSpawnTiles(tile, needFull)`.
//!
//! `player` slots 49..52 (codes ≥ 630): `49` 634 `tiles()`, `50` 635
//! `borderTiles()`, `51` 632 `doomsdayClockTicks()`, `52` 639 `spawnTile()`.
//! The void G5e additions (`630` clearDoomsdayClock, `631` enterDoomsdayClock,
//! `633` removeTroops, `636` relinquish, `637` conquer, `638` setSpawnTile,
//! `640` markRotted) consume no slot.
//!
//! `unit` slots 28..29: `28` 620 `maxHealth()`, `29` 621 `health()`.
//!
//! New construction-record tags (code 307): `13` `PlayerExecution(player)`
//! (the ctor has no side effect), `14` `TribeExecution(player)` (its ctor
//! reads `player.id()` first, then the record).
//!
//! ## G5f facade additions (codes ≥ 700)
//!
//! `mg`: `700` config().nukeSpeed(type), `701` config().waterNukes() (BOOL),
//! `702` config().defaultNukeTargetableRange(), `703` playerBySmallID(id),
//! `704` config().nukeDeathFactor(type, troops, tilesLeft, maxTroops),
//! `705` config().mirvNormalizeTargetTicks(), `706`
//! config().dynamicSamRange(sam, tick), `707` config().maxSamRange(),
//! `708` config().SAMCooldown(), `709` units(), `710` euclideanDistSquared(a, b),
//! `715` nearbyUnits(tile, outer, types, cb) (the callback form of `306`),
//! `716` bfs(start, cb). Void (`mg`): `711` recordMotionPlan(plan), `712`
//! queueNukeImpact(tile), `713` queueWaterConversion(tile), `714`
//! displayIncomingUnit(id, msg, type, targetId), `717` stats().bombLaunch(p, t,
//! type), `718` stats().bombLand(p, t, type).
//! `player`: `720` incomingAllianceRequests(), `721` outgoingAttacks() (both
//! return facade-token lists). `request`: `744` requestor().
//! `unit`: `730` troops(), `731` setTroops(n), `732` updateNukeState(waitTicks),
//! `733` setTrajectoryIndex(i), `734` setTargetable(flag), `735` targetUnit(),
//! `736` touch(), `737` isUnit() (BOOL), `738` trajectoryIndex(), `739`
//! nukeState() (`{waitTicks}`), `740` trajectory() (`TrajectoryTile[]`), `741`
//! isInCooldown() (BOOL), `742` launch(), `743` targetedBySAM() (BOOL).
//!
//! The two callback facades push their code and args BEFORE replaying their
//! scripted list through the callback, then the filtered result: `bfs` (`716`,
//! a `TileRef[]`) and the callback `nearbyUnits` (`715`, a unit-token list) — the
//! `bfs`/`nearbyUnits` callbacks drive the nested `mg`/`unit` reads. The parabola
//! pathfinder is a scripted `__parabola` object (`750` `next(from, to, speed)`,
//! `751` `findPath(from, to)`, `752` `currentIndex()`), replayed by the
//! `__stub/pathfinding.ts` stub. The G5f construction tag is `15`
//! (`SAMMissileExecution`: `spawn`, `_owner`, `ownerUnit`, `target`,
//! `targetTile`).
//!
//! ## `neighbor` facade — slots 0..2
//!
//! `0` 257 `isPlayer()`, `1` 258 `isOnSameTeam(other)`,
//! `2` 256 `updateRelation(other, amt)`.
//!
//! ## `railNetwork` facade
//!
//! `290` `recomputeClusters()`.
//!
//! # Event framing
//!
//! Every non-callback facade event is `[code, (prefix)*, ...encVal(ret)]`. The
//! two callback-driven facades (`anyUnitNearby` 227 / `circleSearch` 228) push
//! their code and arguments BEFORE iterating their scripted replay list, then
//! the callback's own events, then the return — the `anyUnitNearby` script is a
//! list of unit tokens, the `circleSearch` script a list of `(tile, d2)` pairs.
//!
//! `Structures.types` (the `wouldNukeBreakAlliance` import) is
//! `["City", "Defense Post", "SAM Launcher", "Missile Silo", "Port", "Factory"]`
//! (Game.ts:233-240, declaration order of the `unitTypeGroup`).

use crate::doomsday_clock::{DrainConfig, FloorConfig, Profile};
use crate::js_json::{push_str, push_val, read_val, val_field, JsVal};
use crate::jsnum::{js_max, js_min, js_round};
use crate::pseudo_random::PseudoRandom;

/// `MessageType.ALLIANCE_ACCEPTED` (numeric enum member 13).
const MSG_ALLIANCE_ACCEPTED: f64 = 13.0;
/// `MessageType.RENEW_ALLIANCE` (numeric enum member 21).
const MSG_RENEW_ALLIANCE: f64 = 21.0;
/// `PlayerType.Bot`.
const PLAYER_TYPE_BOT: &str = "BOT";
/// `PlayerType.Nation`.
const PLAYER_TYPE_NATION: &str = "NATION";
/// `GameType.Singleplayer`.
const GAME_TYPE_SINGLEPLAYER: &str = "Singleplayer";
/// `UnitType.MIRV` / `UnitType.MIRVWarhead` string members.
const UNIT_MIRV: &str = "MIRV";
const UNIT_MIRV_WARHEAD: &str = "MIRV Warhead";
/// `Structures.types` (Game.ts:233-240).
const STRUCTURES_TYPES: [&str; 6] = [
    "City",
    "Defense Post",
    "SAM Launcher",
    "Missile Silo",
    "Port",
    "Factory",
];

/// `UnitType` string members reused by the G5b executions.
const UNIT_TRANSPORT: &str = "Transport";
const UNIT_WARSHIP: &str = "Warship";
const UNIT_CITY: &str = "City";
const UNIT_PORT: &str = "Port";
const UNIT_FACTORY: &str = "Factory";
/// `MessageType.UNIT_DESTROYED` (numeric enum member 12).
const MSG_UNIT_DESTROYED: f64 = 12.0;
/// `RetreatExecution.cancelDelay`.
const RETREAT_CANCEL_DELAY: f64 = 20.0;
/// Construction-record tags (the order the capture's recorder assigns).
const TAG_TRAIN_STATION: f64 = 0.0;
const TAG_SHELL: f64 = 1.0;
const TAG_EMOJI: f64 = 2.0;
// G5d downstream-execution tags (`mg.addExecution(new XxxExecution(...))`).
const TAG_NUKE: f64 = 3.0;
const TAG_MIRV: f64 = 4.0;
const TAG_WARSHIP: f64 = 5.0;
const TAG_PORT_EXEC: f64 = 6.0;
const TAG_MISSILE_SILO: f64 = 7.0;
const TAG_DEFENSE_POST: f64 = 8.0;
const TAG_SAM_LAUNCHER: f64 = 9.0;
const TAG_CITY: f64 = 10.0;
const TAG_FACTORY: f64 = 11.0;
const TAG_TRADE_SHIP: f64 = 12.0;
// G5e downstream-execution tags (`mg.addExecution(new XxxExecution(...))`).
const TAG_PLAYER_EXEC: f64 = 13.0;
const TAG_TRIBE_EXEC: f64 = 14.0;
// G5f downstream-execution tag (`mg.addExecution(new SAMMissileExecution(...))`).
const TAG_SAM_MISSILE: f64 = 15.0;

/// G5d `UnitType` string members (`Game.ts`).
const UNIT_SHELL: &str = "Shell";
const UNIT_SAM_MISSILE: &str = "SAMMissile";
const UNIT_TRADE_SHIP: &str = "Trade Ship";
const UNIT_ATOM_BOMB: &str = "Atom Bomb";
const UNIT_HYDROGEN_BOMB: &str = "Hydrogen Bomb";
/// G5f `UnitType` string members.
const UNIT_MISSILE_SILO: &str = "Missile Silo";
const UNIT_SAM_LAUNCHER: &str = "SAM Launcher";
/// G5f `MessageType` numeric enum members.
const MSG_MIRV_INBOUND: f64 = 4.0;
const MSG_NUKE_INBOUND: f64 = 5.0;
const MSG_NUKE_DETONATED: f64 = 6.0;
const MSG_HYDROGEN_BOMB_INBOUND: f64 = 7.0;
/// `NukeExecution.SPRITE_RADIUS`.
const SPRITE_RADIUS: f64 = 16.0;
/// `MIRVExecution` static/instance constants.
const MIRV_MATH_SCALE: f64 = 100.0;
const MIRV_LONG_FLIGHT_MULT: f64 = 14.0;
const MIRV_LONG_FLIGHT_LINEAR_PERCENT: f64 = 10.0;
const MIRV_SHORT_FLIGHT_MULT: f64 = 10.0;
/// `PathStatus` members (`src/core/pathfinding/types.ts`).
const PATH_STATUS_NEXT: f64 = 0.0;
const PATH_STATUS_COMPLETE: f64 = 2.0;
/// `PathFinder.Parabola.ts` `PARABOLA_MIN_HEIGHT`.
const PARABOLA_MIN_HEIGHT: f64 = 50.0;
/// `MirvExecution` instance constants (`range` / `minimumSpread` / `warheadCount`).
const MIRV_RANGE: f64 = 1500.0;
const MIRV_MINIMUM_SPREAD: f64 = 55.0;
const MIRV_WARHEAD_COUNT: f64 = 350.0;
/// `GameMode.FFA`.
const GAME_MODE_FFA: &str = "Free For All";
/// `GameMapSize.Compact`.
const GAME_MAP_SIZE_COMPACT: &str = "Compact";
/// `RankedType` members.
const RANKED_ONE_V_ONE: &str = "1v1";
const RANKED_TWO_V_TWO: &str = "2v2";
/// `PlayerType.Human`.
const PLAYER_TYPE_HUMAN: &str = "HUMAN";
/// `ColoredTeams.Bot`.
const COLORED_TEAMS_BOT: &str = "Bot";
/// `MessageType.SAM_HIT` (numeric enum member 10).
const MSG_SAM_HIT: f64 = 10.0;
/// `WinCheckExecution.HARD_TIME_LIMIT_SECONDS`.
const WIN_HARD_TIME_LIMIT_SECONDS: f64 = 170.0 * 60.0;
/// `SAMMissileExecution` nukes whitelist.
const NUKES_WHITELIST: [&str; 3] = [UNIT_ATOM_BOMB, UNIT_HYDROGEN_BOMB, UNIT_MIRV_WARHEAD];

/// The first argument of `Game.setWinner` / `WinCheckExecution`'s winner.
#[derive(Clone, Debug)]
enum WinArg {
    Null,
    Team(String),
    Player(usize),
}

fn ev_win(a: &WinArg) -> Vec<f64> {
    match a {
        WinArg::Null => vec![2.0],
        WinArg::Team(s) => evs(s),
        WinArg::Player(t) => vec![3.0, *t as f64],
    }
}

/// `SAMMissileExecution`'s `INTERCEPTED_UNIT_TRANSLATION_KEYS` lookup with the
/// `?? this.target.type()` fallback.
fn intercepted_key(ty: &str) -> String {
    match ty {
        "Atom Bomb" => "unit_type.atom_bomb".to_string(),
        "Hydrogen Bomb" => "unit_type.hydrogen_bomb".to_string(),
        "MIRV Warhead" => "unit_type.mirv".to_string(),
        _ => ty.to_string(),
    }
}

/// `ConstructionExecution.isStructure`.
fn is_structure_type(ty: &str) -> bool {
    matches!(
        ty,
        "Port" | "Missile Silo" | "Defense Post" | "SAM Launcher" | "City" | "Factory"
    )
}

/// `DoomsdayClockSpeed` string -> [`ClockSpeed`]. An unknown string models the
/// `SCHEDULES[speed] ?? SCHEDULES.normal` fallback.
fn speed_from_str(s: &str) -> crate::doomsday_clock::ClockSpeed {
    use crate::doomsday_clock::ClockSpeed;
    match s {
        "slow" => ClockSpeed::Slow,
        "fast" => ClockSpeed::Fast,
        "veryfast" => ClockSpeed::VeryFast,
        _ => ClockSpeed::Normal,
    }
}

/// Per-player `DoomsdayClockExecution.rotState` entry.
#[derive(Clone, Debug)]
struct RotState {
    id: f64,
    since: f64,
    held: f64,
    front: Vec<(f64, f64)>,
}

/// `DoomsdayClockExecution`'s module-local `LowestN`: keeps the `n` smallest
/// `(tile, key)` pairs by `key`, in ascending `key` order (stable for ties, as
/// the TS `splice` inserts before equal keys).
struct LowestN {
    n: f64,
    items: Vec<(f64, f64)>,
}

impl LowestN {
    fn new(n: f64) -> Self {
        LowestN {
            n,
            items: Vec::new(),
        }
    }

    fn size(&self) -> f64 {
        self.items.len() as f64
    }

    /// `offer(tile, key)`: drop when full and `key >= keys[^1]`; otherwise
    /// insert after the last entry whose key is `<= key`, then pop the
    /// overflowing tail.
    fn offer(&mut self, tile: f64, key: f64) {
        let len = self.items.len();
        if len as f64 == self.n {
            let last = match self.items.last() {
                Some(&(_, k)) => k,
                None => f64::NAN,
            };
            if key >= last {
                return;
            }
        }
        let mut i = len;
        while i > 0 && self.items[i - 1].1 > key {
            i -= 1;
        }
        self.items.insert(i, (tile, key));
        if self.items.len() as f64 > self.n {
            self.items.pop();
        }
    }

    fn take(self) -> Vec<f64> {
        self.items.into_iter().map(|(t, _)| t).collect()
    }
}

// ---- codec / script helpers ------------------------------------------------

/// FIFO script consumption with the capture's exhausted-repeat rule.
#[derive(Clone, Debug, Default)]
struct Script {
    vals: Vec<JsVal>,
    i: usize,
}

impl Script {
    fn take(&mut self, tr: &mut Vec<f64>, code: f64, prefix: &[f64]) -> JsVal {
        assert!(
            !self.vals.is_empty(),
            "exec harness: unscripted facade method (code {code})"
        );
        let idx = self.i.min(self.vals.len() - 1);
        self.i += 1;
        let v = self.vals[idx].clone();
        tr.push(code);
        tr.extend_from_slice(prefix);
        push_val(tr, &v);
        v
    }
}

/// One mock object: a fixed-order array of per-method scripts.
#[derive(Clone, Debug, Default)]
struct Mock {
    slots: Vec<Script>,
}

fn build_script(v: &JsVal) -> Script {
    match v {
        JsVal::Arr(items) => Script {
            vals: items.clone(),
            i: 0,
        },
        _ => Script::default(),
    }
}

fn build_mock(v: &JsVal) -> Mock {
    match v {
        JsVal::Arr(items) => Mock {
            slots: items.iter().map(build_script).collect(),
        },
        _ => Mock::default(),
    }
}

fn as_arr(v: &JsVal) -> Vec<JsVal> {
    if let JsVal::Arr(a) = v {
        a.clone()
    } else {
        Vec::new()
    }
}

fn as_num(v: &JsVal) -> f64 {
    if let JsVal::Num(n) = v {
        *n
    } else {
        f64::NAN
    }
}

fn as_bool(v: &JsVal) -> bool {
    matches!(v, JsVal::Bool(true))
}

fn as_str(v: &JsVal) -> String {
    if let JsVal::Str(s) = v {
        s.clone()
    } else {
        String::new()
    }
}

fn as_str_opt(v: &JsVal) -> Option<String> {
    if let JsVal::Str(s) = v {
        Some(s.clone())
    } else {
        None
    }
}

fn as_opt_tok(v: &JsVal) -> Option<usize> {
    if let JsVal::Num(n) = v {
        Some(*n as usize)
    } else {
        None
    }
}

/// A ctor arg that is either a number or `null` / `undefined`.
fn as_opt_num(v: Option<&JsVal>) -> Option<f64> {
    match v {
        Some(JsVal::Num(n)) => Some(*n),
        _ => None,
    }
}

fn as_tok_list(v: &JsVal) -> Vec<usize> {
    if let JsVal::Arr(a) = v {
        a.iter().filter_map(as_opt_tok).collect()
    } else {
        Vec::new()
    }
}

fn as_num_list(v: &JsVal) -> Vec<f64> {
    if let JsVal::Arr(a) = v {
        a.iter().map(as_num).collect()
    } else {
        Vec::new()
    }
}

/// JS `SameValueZero` (the key relation used by `Set` / `Map`).
fn svz(a: f64, b: f64) -> bool {
    a == b || (a.is_nan() && b.is_nan())
}

/// JS `String.prototype.split(".")` for the `QuickChatExecution` chat key.
fn split_dot(s: &str) -> Vec<String> {
    s.split('.').map(|x| x.to_string()).collect()
}

fn as_pairs(v: &JsVal) -> Vec<(f64, f64)> {
    if let JsVal::Arr(a) = v {
        a.iter()
            .filter_map(|p| {
                if let JsVal::Arr(q) = p {
                    if q.len() >= 2 {
                        return Some((as_num(&q[0]), as_num(&q[1])));
                    }
                }
                None
            })
            .collect()
    } else {
        Vec::new()
    }
}

/// JS truthiness of a `TileRef` (tile `0` and `NaN` are falsy).
fn truthy_tile(t: Option<f64>) -> bool {
    match t {
        Some(x) => x != 0.0 && !x.is_nan(),
        None => false,
    }
}

// ---- G5e live-config field readers -----------------------------------------

/// Reads a numeric key out of a `js_json`-encoded live config object. The
/// capture scripts the WHOLE `doomsdayClockConfig()` object; the Rust side reads
/// its fields by name (`val_field`), so key order is irrelevant.
fn cfg_num(cfg: &JsVal, key: &str) -> f64 {
    val_field(cfg, key).map(as_num).unwrap_or(f64::NAN)
}

fn cfg_bool(cfg: &JsVal, key: &str) -> bool {
    val_field(cfg, key).map(as_bool).unwrap_or(false)
}

fn cfg_str(cfg: &JsVal, key: &str) -> String {
    val_field(cfg, key).map(as_str).unwrap_or_default()
}

// ---- encoded-argument helpers ---------------------------------------------

fn ev1(v: f64) -> Vec<f64> {
    vec![3.0, v]
}

fn evb(b: bool) -> Vec<f64> {
    vec![4.0, if b { 1.0 } else { 0.0 }]
}

fn evs(s: &str) -> Vec<f64> {
    let mut o = vec![5.0];
    push_str(&mut o, s);
    o
}

fn evtok(t: Option<usize>) -> Vec<f64> {
    match t {
        Some(i) => vec![3.0, i as f64],
        None => vec![1.0],
    }
}

fn evstr_opt(s: Option<&str>) -> Vec<f64> {
    match s {
        Some(x) => evs(x),
        None => vec![2.0],
    }
}

fn ev_enc(v: &JsVal) -> Vec<f64> {
    let mut o = Vec::new();
    push_val(&mut o, v);
    o
}

fn ev_strs(items: &[String]) -> Vec<f64> {
    let mut o = vec![7.0, items.len() as f64];
    for it in items {
        o.extend_from_slice(&evs(it));
    }
    o
}

fn ev_tok_list(items: &[usize]) -> Vec<f64> {
    let mut o = vec![7.0, items.len() as f64];
    for it in items {
        o.extend_from_slice(&ev1(*it as f64));
    }
    o
}

/// `elEvTokList` over a list of plain tile numbers (the `bfs` callback result).
fn ev_num_list(items: &[f64]) -> Vec<f64> {
    let mut o = vec![7.0, items.len() as f64];
    for it in items {
        o.extend_from_slice(&ev1(*it));
    }
    o
}

/// `ParabolaUniversalPathFinder.next(...).status`.
fn parabola_status(v: &JsVal) -> f64 {
    val_field(v, "status").map(as_num).unwrap_or(f64::NAN)
}

/// `ParabolaUniversalPathFinder.next(...).node`, a tile number.
fn parabola_node_num(v: &JsVal) -> f64 {
    val_field(v, "node").map(as_num).unwrap_or(f64::NAN)
}

/// `unit.trajectory()` (`TrajectoryTile[]`) as `(tile, targetable)` pairs.
fn as_traj(v: &JsVal) -> Vec<(f64, bool)> {
    match v {
        JsVal::Arr(items) => items
            .iter()
            .map(|it| {
                (
                    val_field(it, "tile").map(as_num).unwrap_or(f64::NAN),
                    val_field(it, "targetable").map(as_bool).unwrap_or(false),
                )
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// `unit.nukeState().waitTicks ?? 0`.
fn nuke_state_wait_ticks(v: &JsVal) -> f64 {
    match val_field(v, "waitTicks") {
        Some(JsVal::Num(n)) => *n,
        _ => 0.0,
    }
}

/// `JsVal::Arr` as a number list (the `findPath` / `missileTimerQueue` scripts).
fn as_arr_nums(v: &JsVal) -> Vec<f64> {
    match v {
        JsVal::Arr(items) => items.iter().map(as_num).collect(),
        _ => Vec::new(),
    }
}

fn mk_take(m: &mut Mock, slot: usize, tr: &mut Vec<f64>, code: f64, prefix: &[f64]) -> JsVal {
    m.slots[slot].take(tr, code, prefix)
}

fn mk_void(tr: &mut Vec<f64>, code: f64, prefix: &[f64]) {
    tr.push(code);
    tr.extend_from_slice(prefix);
    push_val(tr, &JsVal::Undef);
}

/// `AllPlayers` (`Game.ts`) is a plain string, so JS `===` against it is a
/// value comparison — the sentinel needs no object identity.
const ALL_PLAYERS: &str = "AllPlayers";

/// The `recipient` slot of `EmojiExecution` / `respondToEmoji`: a Player token,
/// the `AllPlayers` sentinel, or a not-yet-resolved `undefined` / `null`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Rec {
    #[default]
    Unset,
    All,
    Null,
    P(usize),
}

/// Encodes a [`Rec`] exactly like the capture's `elEvRecipient`.
fn ev_rec(r: Rec) -> Vec<f64> {
    match r {
        Rec::Unset => vec![1.0],
        Rec::Null => vec![2.0],
        Rec::All => evs(ALL_PLAYERS),
        Rec::P(t) => vec![3.0, t as f64],
    }
}

/// `EMOJI_*` id arrays: `lits.map(emojiId)` for one of the 23 constants.
fn emoji_ids(name: &str) -> Vec<f64> {
    for (n, lits) in crate::nation_emoji::EMOJI_CONSTS {
        if n == name {
            return lits
                .iter()
                .map(|e| crate::nation_emoji::emoji_id(e) as f64)
                .collect();
        }
    }
    Vec::new()
}

/// JS string comparison gate shared by `respondToEmoji`.
fn is_one_of(s: &str, set: &[&str]) -> bool {
    set.contains(&s)
}

/// `flattenedEmojiTable[this.emoji]` — a JS array index must be a
/// non-negative integer; anything else yields `undefined`.
fn emoji_at(idx: f64) -> Option<&'static str> {
    if idx.is_nan() || idx.fract() != 0.0 || idx < 0.0 {
        return None;
    }
    crate::nation_emoji::FLATTENED_EMOJI_TABLE.get(idx as usize).copied()
}

// ---- the harness -----------------------------------------------------------

/// `SAMTargetingSystem.CachedInterception`.
#[derive(Clone, Copy, Debug, Default)]
struct CachedInterception {
    tick: f64,
    tile: f64,
    min_dist_sq: f64,
    last_seen_tick: f64,
}

/// `SAMLauncherExecution.SAMTargetingSystem`. `precomputed_nukes` reproduces the
/// insertion-ordered `Map<number, CachedInterception>` keyed by unit id.
#[derive(Clone, Debug, Default)]
struct SamTargeting {
    precomputed_nukes: Vec<(f64, CachedInterception)>,
    missile_speed: f64,
}

/// The mutable class state shared by the 12 executions. The names are the
/// reused slots: `a_tok` is requestor / player / from / launcher, `b_tok` the
/// AllianceReject recipient, `id_a` the ctor's string id (recipientID /
/// targetID / toID / requestorID), `flag` is `active` / `paused` /
/// `isDisconnected`, `target` / `req` are the init-resolved tokens.
#[derive(Clone, Debug, Default)]
pub struct ExecHarness {
    class_id: u8,
    a_tok: usize,
    b_tok: usize,
    id_a: String,
    action: String,
    flag: bool,
    target: Option<usize>,
    req: Option<usize>,
    mg_ready: bool,

    // G5b state.
    unit_id: f64,
    unit_ids: Vec<f64>,
    position: f64,
    amount: f64,
    quick_key: String,
    chat_target: Option<String>,
    start_tick: f64,
    retreat_ordered: bool,
    station_created: bool,
    unit: Option<usize>,
    recipient: Option<usize>,
    structure: Option<usize>,
    #[allow(dead_code)]
    last_shell_attack: f64,
    #[allow(dead_code)]
    already_sent_shell: Vec<usize>,

    // G5c state.
    emoji: f64,
    rec: Rec,
    troops: Option<f64>,
    gold: Option<i64>,
    random: Option<PseudoRandom>,

    // G5d state.
    spawn: f64,
    owner_unit: Option<usize>,
    shell_tok: Option<usize>,
    destroy_at_tick: f64,
    sam_tok: Option<usize>,
    sam_speed: f64,
    target_tile: f64,
    check_offset: f64,
    trade_ship_rejections: f64,
    ticks_until_complete: f64,
    construction_type: String,
    rocket_direction_up: Option<bool>,
    el_amount: Option<f64>,
    #[allow(dead_code)]
    checked_ranked_spawns: bool,

    // G5e state.
    el_game_id: String,
    el_tribe_data: Option<crate::tribe_names::TribeNameData>,
    el_nation_cells: Vec<(f64, f64)>,
    el_nation_tiles: Vec<f64>,
    el_used_custom_tribes: Vec<String>,
    el_player_id: String,
    el_player_type: String,
    el_from_intent: bool,
    el_queued_spawn: bool,
    el_spawn_tile: Option<f64>,
    el_rot_state: Vec<RotState>,

    // G5f state (`NukeExecution`, class 33).
    nuke_type: String,
    nuke_dst: f64,
    nuke_src: Option<f64>,
    nuke_speed: f64,
    nuke_wait_ticks: f64,
    nuke_rocket_up: bool,
    nuke: Option<usize>,
    tiles_to_destroy_cache: Option<Vec<f64>>,

    // G5f state (`MirvExecution`, class 34).
    mirv_dst: f64,
    mirv_nuke: Option<usize>,
    mirv_spawn_tile: f64,
    mirv_range_squared: f64,
    mirv_base_x: f64,
    mirv_base_y: f64,
    mirv_target_player: Option<usize>,
    mirv_separate_dst: f64,
    mirv_speed: f64,
    mirv_full_path: Vec<f64>,
    mirv_path_index: usize,
    mirv_staged_targets: Vec<f64>,
    mirv_warheads_spawned: bool,
    mirv_warhead_count: usize,

    // G5f state (`SAMLauncherExecution`, class 35).
    sam_tile: Option<f64>,
    sam_unit: Option<usize>,
    sam_targeting: Option<SamTargeting>,
    sam_last_level: f64,

    mg: Mock,
    players: Vec<Mock>,
    alliances: Vec<Mock>,
    requests: Vec<Mock>,
    units: Vec<Mock>,
    neighbors: Vec<Mock>,
}

// mg slot indices.
const MG_HAS_PLAYER: usize = 0;
const MG_PLAYER: usize = 1;
const MG_TICKS: usize = 2;
const MG_ALLIANCE_REQUEST_DURATION: usize = 3;
const MG_GAME_TYPE: usize = 4;
const MG_NUM_SPAWN_PHASE_TURNS: usize = 5;
const MG_IN_SPAWN_PHASE: usize = 6;
const MG_TEAM_TILES_OWNED: usize = 9;
const MG_TOTAL_LAND_TILES: usize = 10;
const MG_PLAYERS: usize = 11;
const MG_OWNER: usize = 12;
const MG_OWNER_ID: usize = 13;
const MG_NUKE_MAGNITUDES: usize = 14;
const MG_NUKE_ALLIANCE_BREAK_THRESHOLD: usize = 15;
const MG_ANY_UNIT_NEARBY: usize = 17;
const MG_CIRCLE_SEARCH: usize = 18;
const MG_IS_VALID_REF: usize = 19;
const MG_GET_WATER_COMPONENT: usize = 20;
const MG_HAS_WATER_COMPONENT: usize = 21;
const MG_UNIT: usize = 22;
const MG_IS_LAND: usize = 23;
const MG_HAS_UNIT_NEARBY: usize = 24;
const MG_NEARBY_UNITS: usize = 25;
const MG_TRAIN_STATION_MAX_RANGE: usize = 28;
const MG_SILO_COOLDOWN: usize = 29;
const MG_DEFENSE_POST_SHELL_ATTACK_RATE: usize = 30;
const MG_DEFAULT_DONATION_AMOUNT: usize = 31;
const MG_MAX_TROOPS: usize = 32;
const MG_DIFFICULTY: usize = 33;
// G5d mg slots.
const MG_ALL_PLAYERS: usize = 34;
const MG_NUM_LAND_TILES: usize = 35;
const MG_NUM_TILES_WITH_FALLOUT: usize = 36;
const MG_ELAPSED_GAME_SECONDS: usize = 37;
const MG_STATS: usize = 38;
const MG_UNIT_COUNT: usize = 40;
const MG_NEIGHBORS: usize = 41;
const MG_IS_WATER: usize = 42;
const MG_MANHATTAN_DIST: usize = 43;
const MG_PCT_TILES_TO_WIN: usize = 44;
const MG_IS_UNIT_DISABLED: usize = 45;
const MG_UNIT_INFO: usize = 46;
const MG_CONFIG_UNIT_INFO: usize = 47;
const MG_SHELL_LIFETIME: usize = 48;
const MG_WARSHIP_VET_SHELL_DMG: usize = 49;
const MG_DEFAULT_SAM_SPEED: usize = 50;
const MG_TRADE_SHIP_SPAWN_RATE: usize = 51;
const MG_TRADE_SHIP_SHORT_RANGE_DEBUFF: usize = 52;
const MG_PROXIMITY_BONUS_PORTS_NB: usize = 53;
const MG_GAME_MODE: usize = 54;
const MG_RANKED_TYPE: usize = 55;
const MG_MAX_PLAYERS: usize = 56;
const MG_MAX_TIMER_VALUE: usize = 57;
const MG_PATH_NEXT: usize = 59;
// G5e mg slots.
const MG_GET_WINNER: usize = 60;
const MG_DOOMSDAY_CLOCK_CONFIG: usize = 61;
const MG_X: usize = 63;
const MG_Y: usize = 64;
const MG_ADD_PLAYER: usize = 65;
const MG_IS_RANDOM_SPAWN: usize = 66;
const MG_HAS_OWNER: usize = 67;
const MG_IS_BORDER: usize = 68;
const MG_REF: usize = 69;
const MG_WIDTH: usize = 70;
const MG_HEIGHT: usize = 71;
const MG_MIN_DISTANCE_BETWEEN_PLAYERS: usize = 72;
const MG_TEAM_SPAWN_AREA: usize = 73;
const MG_GAME_MAP: usize = 74;
const MG_GAME_MAP_SIZE: usize = 75;
const MG_IS_VALID_COORD: usize = 76;
const MG_IS_IMPASSABLE: usize = 77;
const MG_GET_SPAWN_TILES: usize = 78;
// G5f mg slots (NukeExecution / MirvExecution / SAMLauncherExecution).
const MG_NUKE_SPEED: usize = 79;
const MG_WATER_NUKES: usize = 80;
const MG_DEFAULT_NUKE_TARGETABLE_RANGE: usize = 81;
const MG_PLAYER_BY_SMALL_ID: usize = 82;
const MG_NUKE_DEATH_FACTOR: usize = 83;
const MG_MIRV_NORMALIZE_TARGET_TICKS: usize = 84;
const MG_DYNAMIC_SAM_RANGE: usize = 85;
const MG_MAX_SAM_RANGE: usize = 86;
const MG_SAM_COOLDOWN: usize = 87;
const MG_UNITS: usize = 88;
const MG_EUCLIDEAN_DIST_SQUARED: usize = 89;
const MG_NEARBY_UNITS_CB: usize = 94;
const MG_BFS: usize = 95;
const MG_PARABOLA_NEXT: usize = 98;
const MG_PARABOLA_FIND_PATH: usize = 99;
const MG_PARABOLA_CURRENT_INDEX: usize = 100;

// player slot indices.
const PL_SMALL_ID: usize = 0;
const PL_ID: usize = 1;
const PL_DISPLAY_NAME: usize = 2;
const PL_IS_ALIVE: usize = 3;
const PL_IS_LOBBY_CREATOR: usize = 4;
const PL_CAN_SEND_ALLIANCE_REQUEST: usize = 5;
const PL_OUTGOING_ALLIANCE_REQUESTS: usize = 6;
const PL_HAS_EMBARGO_AGAINST: usize = 8;
const PL_CREATE_ALLIANCE_REQUEST: usize = 10;
const PL_UNITS: usize = 11;
const PL_IS_FRIENDLY: usize = 12;
const PL_ALLIANCE_WITH: usize = 13;
const PL_NEARBY: usize = 15;
const PL_CAN_TARGET: usize = 16;
const PL_TEAM: usize = 18;
const PL_IS_ON_SAME_TEAM: usize = 20;
const PL_TYPE: usize = 21;
const PL_CAN_EMBARGO_ALL: usize = 22;
const PL_IS_PLAYER: usize = 26;
const PL_CAN_SEND_QUICK_CHAT: usize = 29;
const PL_CAN_DELETE_UNIT: usize = 31;
const PL_CAN_UPGRADE_UNIT: usize = 33;
const PL_CAN_SEND_EMOJI: usize = 34;
const PL_RELATION: usize = 35;
const PL_CAN_DONATE_TROOPS: usize = 36;
const PL_DONATE_TROOPS: usize = 37;
const PL_CAN_DONATE_GOLD: usize = 38;
const PL_DONATE_GOLD: usize = 39;
const PL_GOLD: usize = 40;
const PL_TROOPS: usize = 41;
const PL_NAME: usize = 42;
// G5d player slots.
const PL_HAS_SPAWNED: usize = 43;
const PL_IS_DISCONNECTED: usize = 44;
const PL_NUM_TILES_OWNED: usize = 45;
const PL_CAN_TRADE: usize = 46;
const PL_CAN_BUILD: usize = 47;
const PL_BUILD_UNIT: usize = 48;
// G5e player slots (void mock methods consume no slot, so only the
// value-returning ones below are referenced from the Rust side).
const PL_TILES: usize = 49;
const PL_BORDER_TILES: usize = 50;
const PL_DOOMSDAY_CLOCK_TICKS: usize = 51;
const PL_SPAWN_TILE: usize = 52;
// G5f player slots (`NukeExecution.maybeBreakAlliances`, `detonate`).
const PL_INCOMING_ALLIANCE_REQUESTS: usize = 53;
const PL_OUTGOING_ATTACKS: usize = 54;

// alliance slot indices.
const AL_ONLY_ONE_AGREED: usize = 0;
const AL_BOTH_AGREED: usize = 2;

// alliance-request slot indices.
const RQ_RECIPIENT: usize = 0;
const RQ_STATUS: usize = 3;
const RQ_CREATED_AT: usize = 4;
// G5f alliance-request slot.
const RQ_REQUESTOR: usize = 5;

// unit slot indices.
const UN_IS_ACTIVE: usize = 0;
const UN_REACHED_TARGET: usize = 1;
const UN_TARGET_TILE: usize = 2;
const UN_TYPE: usize = 3;
const UN_TARGET_PLAYER: usize = 4;
const UN_OWNER: usize = 6;
const UN_ID: usize = 7;
const UN_TILE: usize = 11;
const UN_IS_UNDER_CONSTRUCTION: usize = 12;
const UN_MISSILE_TIMER_QUEUE: usize = 13;
const UN_IS_OVERDUE_DELETION: usize = 16;
const UN_HAS_TRAIN_STATION: usize = 17;
const UN_HAS_HEALTH: usize = 19;
// G5d unit slots (void mock methods consume no slot, so only the value-returning
// ones below are referenced from the Rust side).
const UN_LEVEL: usize = 20;
const UN_VETERANCY: usize = 21;
// G5e unit slots.
const UN_MAX_HEALTH: usize = 28;
const UN_HEALTH: usize = 29;
// G5f unit slots.
const UN_TROOPS: usize = 30;
const UN_TARGET_UNIT: usize = 35;
const UN_IS_UNIT: usize = 37;
const UN_TRAJECTORY_INDEX: usize = 38;
const UN_NUKE_STATE: usize = 39;
const UN_TRAJECTORY: usize = 40;
const UN_IS_IN_COOLDOWN: usize = 41;
const UN_TARGETED_BY_SAM: usize = 43;

// neighbor slot indices.
const NB_IS_PLAYER: usize = 0;
const NB_IS_ON_SAME_TEAM: usize = 1;

impl ExecHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    // ---- facade accessors ----

    fn mg_has_player(&mut self, tr: &mut Vec<f64>, id: &str) -> bool {
        let v = mk_take(&mut self.mg, MG_HAS_PLAYER, tr, 210.0, &evs(id));
        as_bool(&v)
    }
    fn mg_player(&mut self, tr: &mut Vec<f64>, id: &str) -> Option<usize> {
        let v = mk_take(&mut self.mg, MG_PLAYER, tr, 211.0, &evs(id));
        as_opt_tok(&v)
    }
    fn mg_ticks(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(&mut self.mg, MG_TICKS, tr, 212.0, &[]);
        as_num(&v)
    }
    fn mg_alliance_request_duration(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(&mut self.mg, MG_ALLIANCE_REQUEST_DURATION, tr, 213.0, &[]);
        as_num(&v)
    }
    fn mg_game_type(&mut self, tr: &mut Vec<f64>) -> String {
        let v = mk_take(&mut self.mg, MG_GAME_TYPE, tr, 214.0, &[]);
        as_str(&v)
    }
    fn mg_num_spawn_phase_turns(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(&mut self.mg, MG_NUM_SPAWN_PHASE_TURNS, tr, 215.0, &[]);
        as_num(&v)
    }
    fn mg_in_spawn_phase(&mut self, tr: &mut Vec<f64>) -> bool {
        let v = mk_take(&mut self.mg, MG_IN_SPAWN_PHASE, tr, 216.0, &[]);
        as_bool(&v)
    }
    fn mg_end_spawn_phase(&mut self, tr: &mut Vec<f64>) {
        mk_void(tr, 217.0, &[]);
    }
    fn mg_set_paused(&mut self, tr: &mut Vec<f64>, paused: bool) {
        mk_void(tr, 218.0, &evb(paused));
    }
    fn mg_team_tiles_owned(&mut self, tr: &mut Vec<f64>, team: Option<&str>) -> f64 {
        let v = mk_take(&mut self.mg, MG_TEAM_TILES_OWNED, tr, 219.0, &evstr_opt(team));
        as_num(&v)
    }
    fn mg_total_land_tiles(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(&mut self.mg, MG_TOTAL_LAND_TILES, tr, 220.0, &[]);
        as_num(&v)
    }
    fn mg_players(&mut self, tr: &mut Vec<f64>) -> Vec<usize> {
        let v = mk_take(&mut self.mg, MG_PLAYERS, tr, 221.0, &[]);
        as_tok_list(&v)
    }
    fn mg_owner(&mut self, tr: &mut Vec<f64>, tile: f64) -> Option<usize> {
        let v = mk_take(&mut self.mg, MG_OWNER, tr, 222.0, &ev1(tile));
        as_opt_tok(&v)
    }
    fn mg_owner_id(&mut self, tr: &mut Vec<f64>, tile: f64) -> f64 {
        let v = mk_take(&mut self.mg, MG_OWNER_ID, tr, 223.0, &ev1(tile));
        as_num(&v)
    }
    fn mg_nuke_magnitudes(&mut self, tr: &mut Vec<f64>, ty: &str) -> (f64, f64) {
        let v = mk_take(&mut self.mg, MG_NUKE_MAGNITUDES, tr, 224.0, &evs(ty));
        let inner = val_field(&v, "inner").map(as_num).unwrap_or(f64::NAN);
        let outer = val_field(&v, "outer").map(as_num).unwrap_or(f64::NAN);
        (inner, outer)
    }
    fn mg_nuke_alliance_break_threshold(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(
            &mut self.mg,
            MG_NUKE_ALLIANCE_BREAK_THRESHOLD,
            tr,
            225.0,
            &[],
        );
        as_num(&v)
    }
    fn mg_display_message(&mut self, tr: &mut Vec<f64>, args: &[JsVal]) {
        tr.push(226.0);
        for a in args {
            push_val(tr, a);
        }
        push_val(tr, &JsVal::Undef);
    }
    fn mg_is_valid_ref(&mut self, tr: &mut Vec<f64>, tile: f64) -> bool {
        let v = mk_take(&mut self.mg, MG_IS_VALID_REF, tr, 300.0, &ev1(tile));
        as_bool(&v)
    }
    fn mg_get_water_component(&mut self, tr: &mut Vec<f64>, tile: f64) -> JsVal {
        mk_take(&mut self.mg, MG_GET_WATER_COMPONENT, tr, 301.0, &ev1(tile))
    }
    fn mg_has_water_component(&mut self, tr: &mut Vec<f64>, tile: f64, comp: &JsVal) -> bool {
        let mut prefix = ev1(tile);
        push_val(&mut prefix, comp);
        let v = mk_take(&mut self.mg, MG_HAS_WATER_COMPONENT, tr, 302.0, &prefix);
        as_bool(&v)
    }
    fn mg_unit(&mut self, tr: &mut Vec<f64>, id: f64) -> Option<usize> {
        let v = mk_take(&mut self.mg, MG_UNIT, tr, 303.0, &ev1(id));
        as_opt_tok(&v)
    }
    fn mg_is_land(&mut self, tr: &mut Vec<f64>, tile: f64) -> bool {
        let v = mk_take(&mut self.mg, MG_IS_LAND, tr, 304.0, &ev1(tile));
        as_bool(&v)
    }
    fn mg_has_unit_nearby(&mut self, tr: &mut Vec<f64>, tile: f64, outer: f64, ty: &str) -> bool {
        let mut prefix = ev1(tile);
        prefix.extend_from_slice(&ev1(outer));
        prefix.extend_from_slice(&evs(ty));
        let v = mk_take(&mut self.mg, MG_HAS_UNIT_NEARBY, tr, 305.0, &prefix);
        as_bool(&v)
    }
    fn mg_nearby_units(
        &mut self,
        tr: &mut Vec<f64>,
        tile: f64,
        outer: f64,
        types: &[String],
    ) -> Vec<usize> {
        let mut prefix = ev1(tile);
        prefix.extend_from_slice(&ev1(outer));
        prefix.extend_from_slice(&ev_strs(types));
        let v = mk_take(&mut self.mg, MG_NEARBY_UNITS, tr, 306.0, &prefix);
        as_tok_list(&v)
    }
    /// `mg.addExecution(new TrainStationExecution(unit, spawnTrains?))` — the
    /// ctor side effect (`unit.setTrainStation(true)`, code 351) fires first,
    /// then the construction record (code 307, tag 0, ctor args).
    fn mg_add_train_station(&mut self, tr: &mut Vec<f64>, unit_tok: usize, spawn: Option<bool>) {
        self.un_set_train_station(tr, unit_tok, true);
        tr.push(307.0);
        tr.push(TAG_TRAIN_STATION);
        push_val(tr, &JsVal::Num(unit_tok as f64));
        push_val(
            tr,
            &match spawn {
                Some(b) => JsVal::Bool(b),
                None => JsVal::Undef,
            },
        );
        push_val(tr, &JsVal::Undef);
    }
    /// `mg.addExecution(new ShellExecution(...))` — only reachable from the
    /// unreachable `DefensePost.shoot`.
    #[allow(dead_code)]
    fn mg_add_execution_shell(
        &mut self,
        tr: &mut Vec<f64>,
        spawn: f64,
        owner: Option<usize>,
        owner_unit: usize,
        target: usize,
    ) {
        tr.push(307.0);
        tr.push(TAG_SHELL);
        push_val(tr, &JsVal::Num(spawn));
        push_val(
            tr,
            &match owner {
                Some(o) => JsVal::Num(o as f64),
                None => JsVal::Undef,
            },
        );
        push_val(tr, &JsVal::Num(owner_unit as f64));
        push_val(tr, &JsVal::Num(target as f64));
        push_val(tr, &JsVal::Undef);
    }
    #[allow(clippy::too_many_arguments)]
    fn mg_display_chat(
        &mut self,
        tr: &mut Vec<f64>,
        m1: Option<&str>,
        m0: Option<&str>,
        target: Option<&str>,
        pid: Option<&str>,
        is_from: bool,
        rec: Option<&str>,
    ) {
        tr.push(308.0);
        tr.extend_from_slice(&evstr_opt(m1));
        tr.extend_from_slice(&evstr_opt(m0));
        tr.extend_from_slice(&evstr_opt(target));
        tr.extend_from_slice(&evstr_opt(pid));
        tr.extend_from_slice(&evb(is_from));
        tr.extend_from_slice(&evstr_opt(rec));
        push_val(tr, &JsVal::Undef);
    }
    fn mg_train_station_max_range(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(&mut self.mg, MG_TRAIN_STATION_MAX_RANGE, tr, 309.0, &[]);
        as_num(&v)
    }
    fn mg_silo_cooldown(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(&mut self.mg, MG_SILO_COOLDOWN, tr, 310.0, &[]);
        as_num(&v)
    }
    #[allow(dead_code)]
    fn mg_defense_post_shell_attack_rate(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(
            &mut self.mg,
            MG_DEFENSE_POST_SHELL_ATTACK_RATE,
            tr,
            311.0,
            &[],
        );
        as_num(&v)
    }
    fn mg_default_donation_amount(&mut self, tr: &mut Vec<f64>, player: Rec) -> f64 {
        let v = mk_take(
            &mut self.mg,
            MG_DEFAULT_DONATION_AMOUNT,
            tr,
            400.0,
            &ev_rec(player),
        );
        as_num(&v)
    }
    fn mg_max_troops(&mut self, tr: &mut Vec<f64>, player: Rec) -> f64 {
        let v = mk_take(&mut self.mg, MG_MAX_TROOPS, tr, 401.0, &ev_rec(player));
        as_num(&v)
    }
    fn mg_difficulty(&mut self, tr: &mut Vec<f64>) -> String {
        let v = mk_take(&mut self.mg, MG_DIFFICULTY, tr, 402.0, &[]);
        as_str(&v)
    }
    /// `mg.addExecution(new EmojiExecution(requestor, recipientID, emoji))` —
    /// construction record (code 307, tag 2).
    fn mg_add_execution_emoji(
        &mut self,
        tr: &mut Vec<f64>,
        requestor: usize,
        recipient_id: &str,
        emoji: f64,
    ) {
        tr.push(307.0);
        tr.push(TAG_EMOJI);
        push_val(tr, &JsVal::Num(requestor as f64));
        push_val(tr, &JsVal::Str(recipient_id.to_string()));
        push_val(tr, &JsVal::Num(emoji));
        push_val(tr, &JsVal::Undef);
    }

    // ---- G5d mg facade ----
    fn mg_all_players(&mut self, tr: &mut Vec<f64>) -> Vec<usize> {
        let v = mk_take(&mut self.mg, MG_ALL_PLAYERS, tr, 500.0, &[]);
        as_tok_list(&v)
    }
    fn mg_num_land_tiles(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(&mut self.mg, MG_NUM_LAND_TILES, tr, 501.0, &[]);
        as_num(&v)
    }
    fn mg_num_tiles_with_fallout(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(&mut self.mg, MG_NUM_TILES_WITH_FALLOUT, tr, 502.0, &[]);
        as_num(&v)
    }
    fn mg_elapsed_game_seconds(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(&mut self.mg, MG_ELAPSED_GAME_SECONDS, tr, 503.0, &[]);
        as_num(&v)
    }
    fn mg_stats_stats(&mut self, tr: &mut Vec<f64>) -> JsVal {
        mk_take(&mut self.mg, MG_STATS, tr, 504.0, &[])
    }
    fn mg_set_winner(&mut self, tr: &mut Vec<f64>, arg: WinArg, stats: &JsVal) {
        let mut prefix = ev_win(&arg);
        prefix.extend_from_slice(&ev_enc(stats));
        mk_void(tr, 505.0, &prefix);
    }
    fn mg_unit_count(&mut self, tr: &mut Vec<f64>, ty: &str) -> f64 {
        let v = mk_take(&mut self.mg, MG_UNIT_COUNT, tr, 506.0, &evs(ty));
        as_num(&v)
    }
    fn mg_neighbors(&mut self, tr: &mut Vec<f64>, tile: f64) -> Vec<f64> {
        let v = mk_take(&mut self.mg, MG_NEIGHBORS, tr, 507.0, &ev1(tile));
        as_num_list(&v)
    }
    fn mg_is_water(&mut self, tr: &mut Vec<f64>, tile: f64) -> bool {
        let v = mk_take(&mut self.mg, MG_IS_WATER, tr, 508.0, &ev1(tile));
        as_bool(&v)
    }
    fn mg_manhattan_dist(&mut self, tr: &mut Vec<f64>, a: f64, b: f64) -> f64 {
        let mut prefix = ev1(a);
        prefix.extend_from_slice(&ev1(b));
        let v = mk_take(&mut self.mg, MG_MANHATTAN_DIST, tr, 509.0, &prefix);
        as_num(&v)
    }
    fn mg_pct_tiles_to_win(&mut self, tr: &mut Vec<f64>, elapsed: f64) -> f64 {
        let v = mk_take(&mut self.mg, MG_PCT_TILES_TO_WIN, tr, 510.0, &ev1(elapsed));
        as_num(&v)
    }
    fn mg_is_unit_disabled(&mut self, tr: &mut Vec<f64>, ty: &str) -> bool {
        let v = mk_take(&mut self.mg, MG_IS_UNIT_DISABLED, tr, 511.0, &evs(ty));
        as_bool(&v)
    }
    fn mg_unit_info(&mut self, tr: &mut Vec<f64>, ty: &str) -> JsVal {
        mk_take(&mut self.mg, MG_UNIT_INFO, tr, 512.0, &evs(ty))
    }
    fn mg_config_unit_info(&mut self, tr: &mut Vec<f64>, ty: &str) -> JsVal {
        mk_take(&mut self.mg, MG_CONFIG_UNIT_INFO, tr, 513.0, &evs(ty))
    }
    fn mg_shell_lifetime(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(&mut self.mg, MG_SHELL_LIFETIME, tr, 514.0, &[]);
        as_num(&v)
    }
    fn mg_warship_vet_shell_dmg(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(&mut self.mg, MG_WARSHIP_VET_SHELL_DMG, tr, 515.0, &[]);
        as_num(&v)
    }
    fn mg_default_sam_speed(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(&mut self.mg, MG_DEFAULT_SAM_SPEED, tr, 516.0, &[]);
        as_num(&v)
    }
    fn mg_trade_ship_spawn_rate(&mut self, tr: &mut Vec<f64>, rejections: f64, num: f64) -> f64 {
        let mut prefix = ev1(rejections);
        prefix.extend_from_slice(&ev1(num));
        let v = mk_take(&mut self.mg, MG_TRADE_SHIP_SPAWN_RATE, tr, 517.0, &prefix);
        as_num(&v)
    }
    fn mg_trade_ship_short_range_debuff(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(
            &mut self.mg,
            MG_TRADE_SHIP_SHORT_RANGE_DEBUFF,
            tr,
            518.0,
            &[],
        );
        as_num(&v)
    }
    fn mg_proximity_bonus_ports_nb(&mut self, tr: &mut Vec<f64>, n: f64) -> f64 {
        let v = mk_take(&mut self.mg, MG_PROXIMITY_BONUS_PORTS_NB, tr, 519.0, &ev1(n));
        as_num(&v)
    }
    fn mg_game_mode(&mut self, tr: &mut Vec<f64>) -> String {
        let v = mk_take(&mut self.mg, MG_GAME_MODE, tr, 520.0, &[]);
        as_str(&v)
    }
    fn mg_ranked_type(&mut self, tr: &mut Vec<f64>) -> String {
        let v = mk_take(&mut self.mg, MG_RANKED_TYPE, tr, 521.0, &[]);
        as_str(&v)
    }
    fn mg_max_players(&mut self, tr: &mut Vec<f64>) -> Option<f64> {
        let v = mk_take(&mut self.mg, MG_MAX_PLAYERS, tr, 522.0, &[]);
        match v {
            JsVal::Num(n) => Some(n),
            _ => None,
        }
    }
    fn mg_max_timer_value(&mut self, tr: &mut Vec<f64>) -> JsVal {
        mk_take(&mut self.mg, MG_MAX_TIMER_VALUE, tr, 523.0, &[])
    }
    fn mg_bomb_intercept(&mut self, tr: &mut Vec<f64>, player: usize, ty: &str, n: f64) {
        let mut prefix = evtok(Some(player));
        prefix.extend_from_slice(&evs(ty));
        prefix.extend_from_slice(&ev1(n));
        mk_void(tr, 524.0, &prefix);
    }
    fn mg_path_next(&mut self, tr: &mut Vec<f64>, from: f64, to: f64) -> JsVal {
        let mut prefix = ev1(from);
        prefix.extend_from_slice(&ev1(to));
        mk_take(&mut self.mg, MG_PATH_NEXT, tr, 525.0, &prefix)
    }

    // ---- G5e mg facade (`Game` methods) ----
    fn mg_get_winner(&mut self, tr: &mut Vec<f64>) -> JsVal {
        mk_take(&mut self.mg, MG_GET_WINNER, tr, 600.0, &[])
    }
    fn mg_doomsday_clock_config(&mut self, tr: &mut Vec<f64>) -> JsVal {
        mk_take(&mut self.mg, MG_DOOMSDAY_CLOCK_CONFIG, tr, 601.0, &[])
    }
    fn mg_set_fallout(&mut self, tr: &mut Vec<f64>, tile: f64, fallout: bool) {
        let mut prefix = ev1(tile);
        prefix.extend_from_slice(&evb(fallout));
        mk_void(tr, 602.0, &prefix);
    }
    fn mg_x(&mut self, tr: &mut Vec<f64>, tile: f64) -> f64 {
        let v = mk_take(&mut self.mg, MG_X, tr, 603.0, &ev1(tile));
        as_num(&v)
    }
    fn mg_y(&mut self, tr: &mut Vec<f64>, tile: f64) -> f64 {
        let v = mk_take(&mut self.mg, MG_Y, tr, 604.0, &ev1(tile));
        as_num(&v)
    }
    fn mg_add_player(&mut self, tr: &mut Vec<f64>, player_info: &JsVal) -> Option<usize> {
        let prefix = ev_enc(player_info);
        let v = mk_take(&mut self.mg, MG_ADD_PLAYER, tr, 605.0, &prefix);
        as_opt_tok(&v)
    }
    fn mg_is_random_spawn(&mut self, tr: &mut Vec<f64>) -> bool {
        let v = mk_take(&mut self.mg, MG_IS_RANDOM_SPAWN, tr, 606.0, &[]);
        as_bool(&v)
    }
    fn mg_has_owner(&mut self, tr: &mut Vec<f64>, tile: f64) -> bool {
        let v = mk_take(&mut self.mg, MG_HAS_OWNER, tr, 607.0, &ev1(tile));
        as_bool(&v)
    }
    fn mg_is_border(&mut self, tr: &mut Vec<f64>, tile: f64) -> bool {
        let v = mk_take(&mut self.mg, MG_IS_BORDER, tr, 608.0, &ev1(tile));
        as_bool(&v)
    }
    fn mg_ref(&mut self, tr: &mut Vec<f64>, x: f64, y: f64) -> f64 {
        let mut prefix = ev1(x);
        prefix.extend_from_slice(&ev1(y));
        let v = mk_take(&mut self.mg, MG_REF, tr, 609.0, &prefix);
        as_num(&v)
    }
    fn mg_width(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(&mut self.mg, MG_WIDTH, tr, 610.0, &[]);
        as_num(&v)
    }
    fn mg_height(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(&mut self.mg, MG_HEIGHT, tr, 611.0, &[]);
        as_num(&v)
    }
    fn mg_min_distance_between_players(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(
            &mut self.mg,
            MG_MIN_DISTANCE_BETWEEN_PLAYERS,
            tr,
            612.0,
            &[],
        );
        as_num(&v)
    }
    fn mg_team_spawn_area(&mut self, tr: &mut Vec<f64>, team: &str) -> JsVal {
        mk_take(&mut self.mg, MG_TEAM_SPAWN_AREA, tr, 613.0, &evs(team))
    }
    fn mg_game_map(&mut self, tr: &mut Vec<f64>) -> String {
        let v = mk_take(&mut self.mg, MG_GAME_MAP, tr, 614.0, &[]);
        as_str(&v)
    }
    fn mg_game_map_size(&mut self, tr: &mut Vec<f64>) -> String {
        let v = mk_take(&mut self.mg, MG_GAME_MAP_SIZE, tr, 615.0, &[]);
        as_str(&v)
    }
    fn mg_is_valid_coord(&mut self, tr: &mut Vec<f64>, x: f64, y: f64) -> bool {
        let mut prefix = ev1(x);
        prefix.extend_from_slice(&ev1(y));
        let v = mk_take(&mut self.mg, MG_IS_VALID_COORD, tr, 616.0, &prefix);
        as_bool(&v)
    }
    fn mg_is_impassable(&mut self, tr: &mut Vec<f64>, tile: f64) -> bool {
        let v = mk_take(&mut self.mg, MG_IS_IMPASSABLE, tr, 617.0, &ev1(tile));
        as_bool(&v)
    }
    fn mg_get_spawn_tiles(&mut self, tr: &mut Vec<f64>, tile: Option<f64>, need_full: bool) -> JsVal {
        let mut prefix = ev_enc(&match tile {
            Some(t) => JsVal::Num(t),
            None => JsVal::Undef,
        });
        prefix.extend_from_slice(&evb(need_full));
        mk_take(&mut self.mg, MG_GET_SPAWN_TILES, tr, 618.0, &prefix)
    }

    // ---- G5d construction-record adders (`mg.addExecution`) ----
    #[allow(clippy::too_many_arguments)]
    fn mg_add_nuke(
        &mut self,
        tr: &mut Vec<f64>,
        nuke_type: &str,
        player: usize,
        dst: f64,
        src: Option<f64>,
        speed: f64,
        wait_ticks: f64,
        rocket_up: Option<bool>,
    ) {
        tr.push(307.0);
        tr.push(TAG_NUKE);
        push_val(tr, &JsVal::Str(nuke_type.to_string()));
        push_val(tr, &JsVal::Num(player as f64));
        push_val(tr, &JsVal::Num(dst));
        push_val(
            tr,
            &match src {
                Some(s) => JsVal::Num(s),
                None => JsVal::Null,
            },
        );
        push_val(tr, &JsVal::Num(speed));
        push_val(tr, &JsVal::Num(wait_ticks));
        push_val(
            tr,
            &match rocket_up {
                Some(b) => JsVal::Bool(b),
                None => JsVal::Undef,
            },
        );
        push_val(tr, &JsVal::Undef);
    }
    fn mg_add_mirv(&mut self, tr: &mut Vec<f64>, player: usize, tile: f64) {
        tr.push(307.0);
        tr.push(TAG_MIRV);
        push_val(tr, &JsVal::Num(player as f64));
        push_val(tr, &JsVal::Num(tile));
        push_val(tr, &JsVal::Undef);
    }
    fn mg_add_warship(&mut self, tr: &mut Vec<f64>, player: usize, patrol_tile: f64) {
        tr.push(307.0);
        tr.push(TAG_WARSHIP);
        push_val(tr, &JsVal::Num(player as f64));
        push_val(tr, &JsVal::Num(patrol_tile));
        push_val(tr, &JsVal::Undef);
    }
    fn mg_add_unit_exec(&mut self, tr: &mut Vec<f64>, tag: f64, unit_tok: usize) {
        tr.push(307.0);
        tr.push(tag);
        push_val(tr, &JsVal::Num(unit_tok as f64));
        push_val(tr, &JsVal::Undef);
    }
    fn mg_add_sam_launcher(
        &mut self,
        tr: &mut Vec<f64>,
        player: usize,
        tile_arg: JsVal,
        sam_tok: usize,
    ) {
        tr.push(307.0);
        tr.push(TAG_SAM_LAUNCHER);
        push_val(tr, &JsVal::Num(player as f64));
        push_val(tr, &tile_arg);
        push_val(tr, &JsVal::Num(sam_tok as f64));
        push_val(tr, &JsVal::Undef);
    }
    fn mg_add_trade_ship(
        &mut self,
        tr: &mut Vec<f64>,
        owner: Option<usize>,
        src_port: usize,
        dst_port: usize,
    ) {
        tr.push(307.0);
        tr.push(TAG_TRADE_SHIP);
        push_val(
            tr,
            &match owner {
                Some(o) => JsVal::Num(o as f64),
                None => JsVal::Undef,
            },
        );
        push_val(tr, &JsVal::Num(src_port as f64));
        push_val(tr, &JsVal::Num(dst_port as f64));
        push_val(tr, &JsVal::Undef);
    }
    /// `mg.addExecution(new PlayerExecution(player))` (tag 13): the ctor has no
    /// side effect, so only the construction record follows.
    fn mg_add_exec_player(&mut self, tr: &mut Vec<f64>, player: usize) {
        tr.push(307.0);
        tr.push(TAG_PLAYER_EXEC);
        push_val(tr, &JsVal::Num(player as f64));
        push_val(tr, &JsVal::Undef);
    }
    /// `mg.addExecution(new TribeExecution(player))` (tag 14): the real ctor
    /// reads `player.id()` (code 231) first, then the construction record.
    fn mg_add_exec_tribe(&mut self, tr: &mut Vec<f64>, player: usize) {
        let _ = self.pl_id(tr, player);
        tr.push(307.0);
        tr.push(TAG_TRIBE_EXEC);
        push_val(tr, &JsVal::Num(player as f64));
        push_val(tr, &JsVal::Undef);
    }

    fn mg_any_unit_nearby(
        &mut self,
        tr: &mut Vec<f64>,
        tile: f64,
        outer: f64,
        types: &[String],
        mut cb: impl FnMut(&mut Self, &mut Vec<f64>, usize) -> bool,
    ) -> bool {
        let v = {
            let s = &mut self.mg.slots[MG_ANY_UNIT_NEARBY];
            assert!(!s.vals.is_empty(), "exec harness: unscripted anyUnitNearby");
            let idx = s.i.min(s.vals.len() - 1);
            s.i += 1;
            s.vals[idx].clone()
        };
        let units = as_tok_list(&v);
        tr.push(227.0);
        tr.extend_from_slice(&ev1(tile));
        tr.extend_from_slice(&ev1(outer));
        tr.extend_from_slice(&ev_strs(types));
        tr.extend_from_slice(&ev_tok_list(&units));
        let mut result = false;
        for uid in units {
            if cb(self, tr, uid) {
                result = true;
                break;
            }
        }
        push_val(tr, &JsVal::Bool(result));
        result
    }

    fn mg_circle_search(
        &mut self,
        tr: &mut Vec<f64>,
        tile: f64,
        outer: f64,
        mut cb: impl FnMut(&mut Self, &mut Vec<f64>, f64, f64) -> bool,
    ) {
        let v = {
            let s = &mut self.mg.slots[MG_CIRCLE_SEARCH];
            assert!(!s.vals.is_empty(), "exec harness: unscripted circleSearch");
            let idx = s.i.min(s.vals.len() - 1);
            s.i += 1;
            s.vals[idx].clone()
        };
        let pairs = as_pairs(&v);
        tr.push(228.0);
        tr.extend_from_slice(&ev1(tile));
        tr.extend_from_slice(&ev1(outer));
        for (t, d2) in pairs {
            if !cb(self, tr, t, d2) {
                break;
            }
        }
        push_val(tr, &JsVal::Undef);
    }

    // player facade
    fn pl(&mut self, tok: usize, slot: usize, tr: &mut Vec<f64>, code: f64, extra: &[f64]) -> JsVal {
        let mut prefix = vec![tok as f64];
        prefix.extend_from_slice(extra);
        mk_take(&mut self.players[tok], slot, tr, code, &prefix)
    }
    fn pl_void(&mut self, tok: usize, tr: &mut Vec<f64>, code: f64, extra: &[f64]) {
        let mut prefix = vec![tok as f64];
        prefix.extend_from_slice(extra);
        mk_void(tr, code, &prefix);
    }

    fn pl_small_id(&mut self, tr: &mut Vec<f64>, tok: usize) -> f64 {
        let v = self.pl(tok, PL_SMALL_ID, tr, 230.0, &[]);
        as_num(&v)
    }
    fn pl_id(&mut self, tr: &mut Vec<f64>, tok: usize) -> String {
        let v = self.pl(tok, PL_ID, tr, 231.0, &[]);
        as_str(&v)
    }
    fn pl_display_name(&mut self, tr: &mut Vec<f64>, tok: usize) -> String {
        let v = self.pl(tok, PL_DISPLAY_NAME, tr, 232.0, &[]);
        as_str(&v)
    }
    fn pl_is_alive(&mut self, tr: &mut Vec<f64>, tok: usize) -> bool {
        let v = self.pl(tok, PL_IS_ALIVE, tr, 233.0, &[]);
        as_bool(&v)
    }
    fn pl_is_lobby_creator(&mut self, tr: &mut Vec<f64>, tok: usize) -> bool {
        let v = self.pl(tok, PL_IS_LOBBY_CREATOR, tr, 234.0, &[]);
        as_bool(&v)
    }
    fn pl_can_send_alliance_request(&mut self, tr: &mut Vec<f64>, tok: usize, other: Option<usize>) -> bool {
        let v = self.pl(tok, PL_CAN_SEND_ALLIANCE_REQUEST, tr, 235.0, &evtok(other));
        as_bool(&v)
    }
    fn pl_outgoing_alliance_requests(&mut self, tr: &mut Vec<f64>, tok: usize) -> Vec<usize> {
        let v = self.pl(tok, PL_OUTGOING_ALLIANCE_REQUESTS, tr, 236.0, &[]);
        as_tok_list(&v)
    }
    fn pl_has_embargo_against(&mut self, tr: &mut Vec<f64>, tok: usize, other: Option<usize>) -> bool {
        let v = self.pl(tok, PL_HAS_EMBARGO_AGAINST, tr, 238.0, &evtok(other));
        as_bool(&v)
    }
    fn pl_create_alliance_request(&mut self, tr: &mut Vec<f64>, tok: usize, other: Option<usize>) -> Option<usize> {
        let v = self.pl(tok, PL_CREATE_ALLIANCE_REQUEST, tr, 240.0, &evtok(other));
        as_opt_tok(&v)
    }
    fn pl_units(&mut self, tr: &mut Vec<f64>, tok: usize, types: &[String]) -> Vec<usize> {
        let v = self.pl(tok, PL_UNITS, tr, 241.0, &ev_strs(types));
        as_tok_list(&v)
    }
    fn pl_is_friendly(&mut self, tr: &mut Vec<f64>, tok: usize, other: Option<usize>) -> bool {
        let v = self.pl(tok, PL_IS_FRIENDLY, tr, 242.0, &evtok(other));
        as_bool(&v)
    }
    fn pl_alliance_with(&mut self, tr: &mut Vec<f64>, tok: usize, other: Option<usize>) -> Option<usize> {
        let v = self.pl(tok, PL_ALLIANCE_WITH, tr, 243.0, &evtok(other));
        as_opt_tok(&v)
    }
    fn pl_nearby(&mut self, tr: &mut Vec<f64>, tok: usize) -> Vec<usize> {
        let v = self.pl(tok, PL_NEARBY, tr, 245.0, &[]);
        as_tok_list(&v)
    }
    fn pl_can_target(&mut self, tr: &mut Vec<f64>, tok: usize, other: Option<usize>) -> bool {
        let v = self.pl(tok, PL_CAN_TARGET, tr, 246.0, &evtok(other));
        as_bool(&v)
    }
    fn pl_team(&mut self, tr: &mut Vec<f64>, tok: usize) -> JsVal {
        self.pl(tok, PL_TEAM, tr, 248.0, &[])
    }
    fn pl_is_on_same_team(&mut self, tr: &mut Vec<f64>, tok: usize, other: Option<usize>) -> bool {
        let v = self.pl(tok, PL_IS_ON_SAME_TEAM, tr, 250.0, &evtok(other));
        as_bool(&v)
    }
    fn pl_type(&mut self, tr: &mut Vec<f64>, tok: usize) -> String {
        let v = self.pl(tok, PL_TYPE, tr, 251.0, &[]);
        as_str(&v)
    }
    fn pl_can_embargo_all(&mut self, tr: &mut Vec<f64>, tok: usize) -> bool {
        let v = self.pl(tok, PL_CAN_EMBARGO_ALL, tr, 252.0, &[]);
        as_bool(&v)
    }
    fn pl_is_player(&mut self, tr: &mut Vec<f64>, tok: usize) -> bool {
        let v = self.pl(tok, PL_IS_PLAYER, tr, 259.0, &[]);
        as_bool(&v)
    }
    fn pl_update_relation(&mut self, tr: &mut Vec<f64>, tok: usize, other: Option<usize>, amt: f64) {
        let mut extra = evtok(other);
        extra.extend_from_slice(&ev1(amt));
        self.pl_void(tok, tr, 237.0, &extra);
    }
    fn pl_end_temporary_embargo(&mut self, tr: &mut Vec<f64>, tok: usize, other: Option<usize>) {
        self.pl_void(tok, tr, 239.0, &evtok(other));
    }
    fn pl_break_alliance(&mut self, tr: &mut Vec<f64>, tok: usize, alliance: usize) {
        self.pl_void(tok, tr, 244.0, &ev1(alliance as f64));
    }
    fn pl_target(&mut self, tr: &mut Vec<f64>, tok: usize, other: Option<usize>) {
        self.pl_void(tok, tr, 247.0, &evtok(other));
    }
    fn pl_add_embargo(&mut self, tr: &mut Vec<f64>, tok: usize, other: Option<usize>, flag: bool) {
        let mut extra = evtok(other);
        extra.extend_from_slice(&evb(flag));
        self.pl_void(tok, tr, 253.0, &extra);
    }
    fn pl_stop_embargo(&mut self, tr: &mut Vec<f64>, tok: usize, other: Option<usize>) {
        self.pl_void(tok, tr, 254.0, &evtok(other));
    }
    fn pl_record_embargo_all(&mut self, tr: &mut Vec<f64>, tok: usize) {
        self.pl_void(tok, tr, 255.0, &[]);
    }
    fn pl_mark_disconnected(&mut self, tr: &mut Vec<f64>, tok: usize, flag: bool, obj: JsVal) {
        let mut extra = evb(flag);
        extra.extend_from_slice(&ev_enc(&obj));
        self.pl_void(tok, tr, 249.0, &extra);
    }
    fn pl_order_retreat(&mut self, tr: &mut Vec<f64>, tok: usize, attack_id: &str) {
        self.pl_void(tok, tr, 320.0, &evs(attack_id));
    }
    fn pl_execute_retreat(&mut self, tr: &mut Vec<f64>, tok: usize, attack_id: &str) {
        self.pl_void(tok, tr, 321.0, &evs(attack_id));
    }
    fn pl_can_send_quick_chat(&mut self, tr: &mut Vec<f64>, tok: usize, other: Option<usize>) -> bool {
        let v = self.pl(tok, PL_CAN_SEND_QUICK_CHAT, tr, 322.0, &evtok(other));
        as_bool(&v)
    }
    fn pl_record_quick_chat(&mut self, tr: &mut Vec<f64>, tok: usize, other: Option<usize>) {
        self.pl_void(tok, tr, 323.0, &evtok(other));
    }
    fn pl_can_delete_unit(&mut self, tr: &mut Vec<f64>, tok: usize) -> bool {
        let v = self.pl(tok, PL_CAN_DELETE_UNIT, tr, 324.0, &[]);
        as_bool(&v)
    }
    fn pl_record_delete_unit(&mut self, tr: &mut Vec<f64>, tok: usize) {
        self.pl_void(tok, tr, 325.0, &[]);
    }
    fn pl_can_upgrade_unit(&mut self, tr: &mut Vec<f64>, tok: usize, unit: usize) -> bool {
        let v = self.pl(tok, PL_CAN_UPGRADE_UNIT, tr, 326.0, &ev1(unit as f64));
        as_bool(&v)
    }
    fn pl_upgrade_unit(&mut self, tr: &mut Vec<f64>, tok: usize, unit: usize) {
        self.pl_void(tok, tr, 327.0, &ev1(unit as f64));
    }
    fn pl_can_send_emoji(&mut self, tr: &mut Vec<f64>, tok: usize, other: Rec) -> bool {
        let v = self.pl(tok, PL_CAN_SEND_EMOJI, tr, 410.0, &ev_rec(other));
        as_bool(&v)
    }
    fn pl_send_emoji(&mut self, tr: &mut Vec<f64>, tok: usize, other: Rec, emoji: &str) {
        let mut extra = ev_rec(other);
        extra.extend_from_slice(&evs(emoji));
        self.pl_void(tok, tr, 411.0, &extra);
    }
    fn pl_relation(&mut self, tr: &mut Vec<f64>, tok: usize, other: Option<usize>) -> f64 {
        let v = self.pl(tok, PL_RELATION, tr, 412.0, &evtok(other));
        as_num(&v)
    }
    fn pl_can_donate_troops(&mut self, tr: &mut Vec<f64>, tok: usize, other: Rec) -> bool {
        let v = self.pl(tok, PL_CAN_DONATE_TROOPS, tr, 413.0, &ev_rec(other));
        as_bool(&v)
    }
    fn pl_donate_troops(
        &mut self,
        tr: &mut Vec<f64>,
        tok: usize,
        other: Rec,
        troops: f64,
    ) -> bool {
        let mut extra = ev_rec(other);
        extra.extend_from_slice(&ev1(troops));
        let v = self.pl(tok, PL_DONATE_TROOPS, tr, 414.0, &extra);
        as_bool(&v)
    }
    fn pl_can_donate_gold(&mut self, tr: &mut Vec<f64>, tok: usize, other: Rec) -> bool {
        let v = self.pl(tok, PL_CAN_DONATE_GOLD, tr, 415.0, &ev_rec(other));
        as_bool(&v)
    }
    fn pl_donate_gold(&mut self, tr: &mut Vec<f64>, tok: usize, other: Rec, gold: i64) -> bool {
        let mut extra = ev_rec(other);
        extra.extend_from_slice(&ev1(gold as f64));
        let v = self.pl(tok, PL_DONATE_GOLD, tr, 416.0, &extra);
        as_bool(&v)
    }
    fn pl_gold(&mut self, tr: &mut Vec<f64>, tok: usize) -> i64 {
        let v = self.pl(tok, PL_GOLD, tr, 417.0, &[]);
        as_num(&v) as i64
    }
    fn pl_troops(&mut self, tr: &mut Vec<f64>, tok: usize) -> f64 {
        let v = self.pl(tok, PL_TROOPS, tr, 418.0, &[]);
        as_num(&v)
    }
    fn pl_name(&mut self, tr: &mut Vec<f64>, tok: usize) -> String {
        let v = self.pl(tok, PL_NAME, tr, 419.0, &[]);
        as_str(&v)
    }
    // G5d player facade.
    fn pl_has_spawned(&mut self, tr: &mut Vec<f64>, tok: usize) -> bool {
        let v = self.pl(tok, PL_HAS_SPAWNED, tr, 530.0, &[]);
        as_bool(&v)
    }
    fn pl_is_disconnected(&mut self, tr: &mut Vec<f64>, tok: usize) -> bool {
        let v = self.pl(tok, PL_IS_DISCONNECTED, tr, 531.0, &[]);
        as_bool(&v)
    }
    fn pl_num_tiles_owned(&mut self, tr: &mut Vec<f64>, tok: usize) -> f64 {
        let v = self.pl(tok, PL_NUM_TILES_OWNED, tr, 532.0, &[]);
        as_num(&v)
    }
    fn pl_can_trade(&mut self, tr: &mut Vec<f64>, tok: usize, other: Option<usize>) -> bool {
        let v = self.pl(tok, PL_CAN_TRADE, tr, 533.0, &evtok(other));
        as_bool(&v)
    }
    fn pl_can_build(&mut self, tr: &mut Vec<f64>, tok: usize, ty: &str, tile: f64) -> JsVal {
        let mut extra = evs(ty);
        extra.extend_from_slice(&ev1(tile));
        self.pl(tok, PL_CAN_BUILD, tr, 534.0, &extra)
    }
    fn pl_build_unit(
        &mut self,
        tr: &mut Vec<f64>,
        tok: usize,
        ty: &str,
        tile: f64,
        params: &JsVal,
    ) -> Option<usize> {
        let mut extra = evs(ty);
        extra.extend_from_slice(&ev1(tile));
        extra.extend_from_slice(&ev_enc(params));
        let v = self.pl(tok, PL_BUILD_UNIT, tr, 535.0, &extra);
        as_opt_tok(&v)
    }
    // G5e player facade.
    fn pl_tiles(&mut self, tr: &mut Vec<f64>, tok: usize) -> Vec<f64> {
        let v = self.pl(tok, PL_TILES, tr, 634.0, &[]);
        as_num_list(&v)
    }
    fn pl_border_tiles(&mut self, tr: &mut Vec<f64>, tok: usize) -> Vec<f64> {
        let v = self.pl(tok, PL_BORDER_TILES, tr, 635.0, &[]);
        as_num_list(&v)
    }
    fn pl_doomsday_clock_ticks(&mut self, tr: &mut Vec<f64>, tok: usize) -> f64 {
        let v = self.pl(tok, PL_DOOMSDAY_CLOCK_TICKS, tr, 632.0, &[]);
        as_num(&v)
    }
    fn pl_spawn_tile(&mut self, tr: &mut Vec<f64>, tok: usize) -> Option<f64> {
        let v = self.pl(tok, PL_SPAWN_TILE, tr, 639.0, &[]);
        match v {
            JsVal::Num(n) => Some(n),
            _ => None,
        }
    }
    fn pl_clear_doomsday_clock(&mut self, tr: &mut Vec<f64>, tok: usize) {
        self.pl_void(tok, tr, 630.0, &[]);
    }
    fn pl_enter_doomsday_clock(&mut self, tr: &mut Vec<f64>, tok: usize) {
        self.pl_void(tok, tr, 631.0, &[]);
    }
    fn pl_remove_troops(&mut self, tr: &mut Vec<f64>, tok: usize, n: f64) {
        self.pl_void(tok, tr, 633.0, &ev1(n));
    }
    fn pl_relinquish(&mut self, tr: &mut Vec<f64>, tok: usize, tile: f64) {
        self.pl_void(tok, tr, 636.0, &ev1(tile));
    }
    fn pl_conquer(&mut self, tr: &mut Vec<f64>, tok: usize, tile: f64) {
        self.pl_void(tok, tr, 637.0, &ev1(tile));
    }
    fn pl_set_spawn_tile(&mut self, tr: &mut Vec<f64>, tok: usize, tile: f64) {
        self.pl_void(tok, tr, 638.0, &ev1(tile));
    }
    fn pl_mark_rotted(&mut self, tr: &mut Vec<f64>, tok: usize) {
        self.pl_void(tok, tr, 640.0, &[]);
    }

    // alliance facade
    fn al(&mut self, aid: usize, slot: usize, tr: &mut Vec<f64>, code: f64, extra: &[f64]) -> JsVal {
        let mut prefix = vec![aid as f64];
        prefix.extend_from_slice(extra);
        mk_take(&mut self.alliances[aid], slot, tr, code, &prefix)
    }
    fn al_void(&mut self, aid: usize, tr: &mut Vec<f64>, code: f64, extra: &[f64]) {
        let mut prefix = vec![aid as f64];
        prefix.extend_from_slice(extra);
        mk_void(tr, code, &prefix);
    }
    fn al_only_one(&mut self, tr: &mut Vec<f64>, aid: usize) -> bool {
        let v = self.al(aid, AL_ONLY_ONE_AGREED, tr, 260.0, &[]);
        as_bool(&v)
    }
    fn al_add_extension_request(&mut self, tr: &mut Vec<f64>, aid: usize, tok: usize) {
        self.al_void(aid, tr, 261.0, &ev1(tok as f64));
    }
    fn al_both_agreed(&mut self, tr: &mut Vec<f64>, aid: usize) -> bool {
        let v = self.al(aid, AL_BOTH_AGREED, tr, 262.0, &[]);
        as_bool(&v)
    }
    fn al_extend(&mut self, tr: &mut Vec<f64>, aid: usize) {
        self.al_void(aid, tr, 263.0, &[]);
    }

    // alliance-request facade
    fn rq(&mut self, rid: usize, slot: usize, tr: &mut Vec<f64>, code: f64, extra: &[f64]) -> JsVal {
        let mut prefix = vec![rid as f64];
        prefix.extend_from_slice(extra);
        mk_take(&mut self.requests[rid], slot, tr, code, &prefix)
    }
    fn rq_void(&mut self, rid: usize, tr: &mut Vec<f64>, code: f64) {
        mk_void(tr, code, &[rid as f64]);
    }
    fn rq_recipient(&mut self, tr: &mut Vec<f64>, rid: usize) -> Option<usize> {
        let v = self.rq(rid, RQ_RECIPIENT, tr, 270.0, &[]);
        as_opt_tok(&v)
    }
    fn rq_accept(&mut self, tr: &mut Vec<f64>, rid: usize) {
        self.rq_void(rid, tr, 271.0);
    }
    fn rq_reject(&mut self, tr: &mut Vec<f64>, rid: usize) {
        self.rq_void(rid, tr, 272.0);
    }
    fn rq_status(&mut self, tr: &mut Vec<f64>, rid: usize) -> String {
        let v = self.rq(rid, RQ_STATUS, tr, 273.0, &[]);
        as_str(&v)
    }
    fn rq_created_at(&mut self, tr: &mut Vec<f64>, rid: usize) -> f64 {
        let v = self.rq(rid, RQ_CREATED_AT, tr, 274.0, &[]);
        as_num(&v)
    }

    // unit facade
    fn un(&mut self, uid: usize, slot: usize, tr: &mut Vec<f64>, code: f64, extra: &[f64]) -> JsVal {
        let mut prefix = vec![uid as f64];
        prefix.extend_from_slice(extra);
        mk_take(&mut self.units[uid], slot, tr, code, &prefix)
    }
    fn un_is_active(&mut self, tr: &mut Vec<f64>, uid: usize) -> bool {
        let v = self.un(uid, UN_IS_ACTIVE, tr, 280.0, &[]);
        as_bool(&v)
    }
    fn un_reached_target(&mut self, tr: &mut Vec<f64>, uid: usize) -> bool {
        let v = self.un(uid, UN_REACHED_TARGET, tr, 281.0, &[]);
        as_bool(&v)
    }
    fn un_target_tile(&mut self, tr: &mut Vec<f64>, uid: usize) -> Option<f64> {
        let v = self.un(uid, UN_TARGET_TILE, tr, 282.0, &[]);
        if let JsVal::Num(n) = v {
            Some(n)
        } else {
            None
        }
    }
    fn un_type(&mut self, tr: &mut Vec<f64>, uid: usize) -> String {
        let v = self.un(uid, UN_TYPE, tr, 283.0, &[]);
        as_str(&v)
    }
    fn un_target_player(&mut self, tr: &mut Vec<f64>, uid: usize) -> Option<usize> {
        let v = self.un(uid, UN_TARGET_PLAYER, tr, 284.0, &[]);
        as_opt_tok(&v)
    }
    fn un_delete(&mut self, tr: &mut Vec<f64>, uid: usize, flag: bool) {
        let mut prefix = vec![uid as f64];
        prefix.extend_from_slice(&evb(flag));
        mk_void(tr, 285.0, &prefix);
    }
    fn un_owner(&mut self, tr: &mut Vec<f64>, uid: usize) -> Option<usize> {
        let v = self.un(uid, UN_OWNER, tr, 286.0, &[]);
        as_opt_tok(&v)
    }
    fn un_id(&mut self, tr: &mut Vec<f64>, uid: usize) -> f64 {
        let v = self.un(uid, UN_ID, tr, 340.0, &[]);
        as_num(&v)
    }
    fn un_update_transport_ship_state(&mut self, tr: &mut Vec<f64>, uid: usize) {
        let obj = JsVal::Obj(vec![("isRetreating".to_string(), JsVal::Bool(true))]);
        let mut prefix = vec![uid as f64];
        prefix.extend_from_slice(&ev_enc(&obj));
        mk_void(tr, 341.0, &prefix);
    }
    fn un_update_warship_state(&mut self, tr: &mut Vec<f64>, uid: usize, position: f64) {
        let obj = JsVal::Obj(vec![("patrolTile".to_string(), JsVal::Num(position))]);
        let mut prefix = vec![uid as f64];
        prefix.extend_from_slice(&ev_enc(&obj));
        mk_void(tr, 342.0, &prefix);
    }
    fn un_set_target_tile(&mut self, tr: &mut Vec<f64>, uid: usize) {
        let mut prefix = vec![uid as f64];
        prefix.extend_from_slice(&ev_enc(&JsVal::Undef));
        mk_void(tr, 343.0, &prefix);
    }
    fn un_tile(&mut self, tr: &mut Vec<f64>, uid: usize) -> f64 {
        let v = self.un(uid, UN_TILE, tr, 344.0, &[]);
        as_num(&v)
    }
    fn un_is_under_construction(&mut self, tr: &mut Vec<f64>, uid: usize) -> bool {
        let v = self.un(uid, UN_IS_UNDER_CONSTRUCTION, tr, 345.0, &[]);
        as_bool(&v)
    }
    fn un_missile_timer_queue(&mut self, tr: &mut Vec<f64>, uid: usize) -> JsVal {
        self.un(uid, UN_MISSILE_TIMER_QUEUE, tr, 346.0, &[])
    }
    fn un_reload_missile(&mut self, tr: &mut Vec<f64>, uid: usize) {
        let prefix = vec![uid as f64];
        mk_void(tr, 347.0, &prefix);
    }
    fn un_mark_for_deletion(&mut self, tr: &mut Vec<f64>, uid: usize) {
        let prefix = vec![uid as f64];
        mk_void(tr, 348.0, &prefix);
    }
    fn un_is_overdue_deletion(&mut self, tr: &mut Vec<f64>, uid: usize) -> bool {
        let v = self.un(uid, UN_IS_OVERDUE_DELETION, tr, 349.0, &[]);
        as_bool(&v)
    }
    fn un_has_train_station(&mut self, tr: &mut Vec<f64>, uid: usize) -> bool {
        let v = self.un(uid, UN_HAS_TRAIN_STATION, tr, 350.0, &[]);
        as_bool(&v)
    }
    fn un_set_train_station(&mut self, tr: &mut Vec<f64>, uid: usize, flag: bool) {
        let mut prefix = vec![uid as f64];
        prefix.extend_from_slice(&evb(flag));
        mk_void(tr, 351.0, &prefix);
    }
    #[allow(dead_code)]
    fn un_has_health(&mut self, tr: &mut Vec<f64>, uid: usize) -> bool {
        let v = self.un(uid, UN_HAS_HEALTH, tr, 352.0, &[]);
        as_bool(&v)
    }
    // G5d unit facade.
    fn un_level(&mut self, tr: &mut Vec<f64>, uid: usize) -> f64 {
        let v = self.un(uid, UN_LEVEL, tr, 540.0, &[]);
        as_num(&v)
    }
    fn un_veterancy(&mut self, tr: &mut Vec<f64>, uid: usize) -> f64 {
        let v = self.un(uid, UN_VETERANCY, tr, 541.0, &[]);
        as_num(&v)
    }
    fn un_record_kill(&mut self, tr: &mut Vec<f64>, uid: usize, ty: &str) {
        let mut prefix = vec![uid as f64];
        prefix.extend_from_slice(&evs(ty));
        mk_void(tr, 542.0, &prefix);
    }
    fn un_modify_health(&mut self, tr: &mut Vec<f64>, uid: usize, d: f64, owner: Option<usize>) {
        let mut prefix = vec![uid as f64];
        prefix.extend_from_slice(&ev1(d));
        prefix.extend_from_slice(&evtok(owner));
        mk_void(tr, 543.0, &prefix);
    }
    fn un_set_reached_target(&mut self, tr: &mut Vec<f64>, uid: usize) {
        mk_void(tr, 544.0, &[uid as f64]);
    }
    fn un_move(&mut self, tr: &mut Vec<f64>, uid: usize, node: &JsVal) {
        let mut prefix = vec![uid as f64];
        prefix.extend_from_slice(&ev_enc(node));
        mk_void(tr, 545.0, &prefix);
    }
    fn un_set_under_construction(&mut self, tr: &mut Vec<f64>, uid: usize, flag: bool) {
        let mut prefix = vec![uid as f64];
        prefix.extend_from_slice(&evb(flag));
        mk_void(tr, 546.0, &prefix);
    }
    fn un_set_targeted_by_sam(&mut self, tr: &mut Vec<f64>, uid: usize, flag: bool) {
        let mut prefix = vec![uid as f64];
        prefix.extend_from_slice(&evb(flag));
        mk_void(tr, 547.0, &prefix);
    }
    // G5e unit facade.
    fn un_max_health(&mut self, tr: &mut Vec<f64>, uid: usize) -> f64 {
        let v = self.un(uid, UN_MAX_HEALTH, tr, 620.0, &[]);
        as_num(&v)
    }
    fn un_health(&mut self, tr: &mut Vec<f64>, uid: usize) -> f64 {
        let v = self.un(uid, UN_HEALTH, tr, 621.0, &[]);
        as_num(&v)
    }

    // ---- G5f facade accessors (`NukeExecution` / `MirvExecution` /
    // `SAMLauncherExecution`) ----

    fn mg_nuke_speed(&mut self, tr: &mut Vec<f64>, ty: &str) -> f64 {
        let v = mk_take(&mut self.mg, MG_NUKE_SPEED, tr, 700.0, &evs(ty));
        as_num(&v)
    }
    fn mg_water_nukes(&mut self, tr: &mut Vec<f64>) -> bool {
        let v = mk_take(&mut self.mg, MG_WATER_NUKES, tr, 701.0, &[]);
        as_bool(&v)
    }
    fn mg_default_nuke_targetable_range(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(
            &mut self.mg,
            MG_DEFAULT_NUKE_TARGETABLE_RANGE,
            tr,
            702.0,
            &[],
        );
        as_num(&v)
    }
    /// `mg.playerBySmallID(id)` — returns the player facade or `null`.
    fn mg_player_by_small_id(&mut self, tr: &mut Vec<f64>, id: f64) -> JsVal {
        mk_take(&mut self.mg, MG_PLAYER_BY_SMALL_ID, tr, 703.0, &ev1(id))
    }
    fn mg_nuke_death_factor(
        &mut self,
        tr: &mut Vec<f64>,
        ty: &str,
        troops: f64,
        tiles_left: f64,
        max_troops: f64,
    ) -> f64 {
        let mut p = evs(ty);
        p.extend_from_slice(&ev1(troops));
        p.extend_from_slice(&ev1(tiles_left));
        p.extend_from_slice(&ev1(max_troops));
        let v = mk_take(&mut self.mg, MG_NUKE_DEATH_FACTOR, tr, 704.0, &p);
        as_num(&v)
    }
    fn mg_mirv_normalize_target_ticks(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(&mut self.mg, MG_MIRV_NORMALIZE_TARGET_TICKS, tr, 705.0, &[]);
        as_num(&v)
    }
    fn mg_dynamic_sam_range(&mut self, tr: &mut Vec<f64>, sam: usize, tick: f64) -> f64 {
        let mut p = evtok(Some(sam));
        p.extend_from_slice(&ev1(tick));
        let v = mk_take(&mut self.mg, MG_DYNAMIC_SAM_RANGE, tr, 706.0, &p);
        as_num(&v)
    }
    fn mg_max_sam_range(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(&mut self.mg, MG_MAX_SAM_RANGE, tr, 707.0, &[]);
        as_num(&v)
    }
    fn mg_sam_cooldown(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(&mut self.mg, MG_SAM_COOLDOWN, tr, 708.0, &[]);
        as_num(&v)
    }
    fn mg_units(&mut self, tr: &mut Vec<f64>) -> Vec<usize> {
        let v = mk_take(&mut self.mg, MG_UNITS, tr, 709.0, &[]);
        as_tok_list(&v)
    }
    fn mg_euclidean_dist_squared(&mut self, tr: &mut Vec<f64>, a: f64, b: f64) -> f64 {
        let mut p = ev1(a);
        p.extend_from_slice(&ev1(b));
        let v = mk_take(&mut self.mg, MG_EUCLIDEAN_DIST_SQUARED, tr, 710.0, &p);
        as_num(&v)
    }
    fn mg_record_motion_plan(&mut self, tr: &mut Vec<f64>, plan: &JsVal) {
        mk_void(tr, 711.0, &ev_enc(plan));
    }
    fn mg_queue_nuke_impact(&mut self, tr: &mut Vec<f64>, tile: f64) {
        mk_void(tr, 712.0, &ev1(tile));
    }
    fn mg_queue_water_conversion(&mut self, tr: &mut Vec<f64>, tile: f64) {
        mk_void(tr, 713.0, &ev1(tile));
    }
    fn mg_display_incoming_unit(
        &mut self,
        tr: &mut Vec<f64>,
        id: f64,
        msg: &str,
        ty: f64,
        player_id: &str,
    ) {
        let mut p = ev1(id);
        p.extend_from_slice(&evs(msg));
        p.extend_from_slice(&ev1(ty));
        p.extend_from_slice(&evs(player_id));
        mk_void(tr, 714.0, &p);
    }
    fn mg_stats_bomb_launch(&mut self, tr: &mut Vec<f64>, p: usize, t: Option<usize>, ty: &str) {
        let mut args = evtok(Some(p));
        args.extend_from_slice(&evtok(t));
        args.extend_from_slice(&evs(ty));
        mk_void(tr, 717.0, &args);
    }
    fn mg_stats_bomb_land(&mut self, tr: &mut Vec<f64>, p: usize, t: Option<usize>, ty: &str) {
        let mut args = evtok(Some(p));
        args.extend_from_slice(&evtok(t));
        args.extend_from_slice(&evs(ty));
        mk_void(tr, 718.0, &args);
    }
    /// `mg.nearbyUnits(tile, outer, types, cb)` — the callback form. The raw
    /// scripted list and the filtered result are both traced.
    fn mg_nearby_units_cb(
        &mut self,
        tr: &mut Vec<f64>,
        tile: f64,
        outer: f64,
        types: &[String],
        mut cb: impl FnMut(&mut Self, &mut Vec<f64>, usize) -> bool,
    ) -> Vec<usize> {
        let mut prefix = ev1(tile);
        prefix.extend_from_slice(&ev1(outer));
        prefix.extend_from_slice(&ev_strs(types));
        let v = mk_take(&mut self.mg, MG_NEARBY_UNITS_CB, tr, 715.0, &prefix);
        let units = as_tok_list(&v);
        let mut out = Vec::new();
        for t in &units {
            if cb(self, tr, *t) {
                out.push(*t);
            }
        }
        tr.extend_from_slice(&ev_tok_list(&out));
        out
    }
    /// `mg.bfs(start, cb)` — the callback form (tile list in, filtered out).
    fn mg_bfs(
        &mut self,
        tr: &mut Vec<f64>,
        start: f64,
        mut cb: impl FnMut(&mut Self, &mut Vec<f64>, f64) -> bool,
    ) -> Vec<f64> {
        let v = mk_take(&mut self.mg, MG_BFS, tr, 716.0, &ev1(start));
        let list = as_num_list(&v);
        let mut out = Vec::new();
        for n in &list {
            if cb(self, tr, *n) {
                out.push(*n);
            }
        }
        tr.extend_from_slice(&ev_num_list(&out));
        out
    }
    fn mg_parabola_next(&mut self, tr: &mut Vec<f64>, from: f64, to: f64, speed: f64) -> JsVal {
        let mut p = ev1(from);
        p.extend_from_slice(&ev1(to));
        p.extend_from_slice(&ev1(speed));
        mk_take(&mut self.mg, MG_PARABOLA_NEXT, tr, 750.0, &p)
    }
    fn mg_parabola_find_path(&mut self, tr: &mut Vec<f64>, from: f64, to: f64) -> Vec<f64> {
        let mut p = ev1(from);
        p.extend_from_slice(&ev1(to));
        let v = mk_take(&mut self.mg, MG_PARABOLA_FIND_PATH, tr, 751.0, &p);
        as_num_list(&v)
    }
    fn mg_parabola_current_index(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(&mut self.mg, MG_PARABOLA_CURRENT_INDEX, tr, 752.0, &[]);
        as_num(&v)
    }
    fn mg_add_sam_missile(
        &mut self,
        tr: &mut Vec<f64>,
        spawn: f64,
        owner: usize,
        owner_unit: usize,
        target: usize,
        target_tile: f64,
    ) {
        tr.push(307.0);
        tr.push(TAG_SAM_MISSILE);
        push_val(tr, &JsVal::Num(spawn));
        push_val(tr, &JsVal::Num(owner as f64));
        push_val(tr, &JsVal::Num(owner_unit as f64));
        push_val(tr, &JsVal::Num(target as f64));
        push_val(tr, &JsVal::Num(target_tile));
        push_val(tr, &JsVal::Undef);
    }
    fn pl_incoming_alliance_requests(&mut self, tr: &mut Vec<f64>, tok: usize) -> Vec<usize> {
        let v = self.pl(tok, PL_INCOMING_ALLIANCE_REQUESTS, tr, 720.0, &[]);
        as_tok_list(&v)
    }
    fn pl_outgoing_attacks(&mut self, tr: &mut Vec<f64>, tok: usize) -> Vec<usize> {
        let v = self.pl(tok, PL_OUTGOING_ATTACKS, tr, 721.0, &[]);
        as_tok_list(&v)
    }
    fn rq_requestor(&mut self, tr: &mut Vec<f64>, rid: usize) -> Option<usize> {
        let v = self.rq(rid, RQ_REQUESTOR, tr, 744.0, &[]);
        as_opt_tok(&v)
    }
    fn un_troops(&mut self, tr: &mut Vec<f64>, uid: usize) -> f64 {
        let v = self.un(uid, UN_TROOPS, tr, 730.0, &[]);
        as_num(&v)
    }
    fn un_set_troops(&mut self, tr: &mut Vec<f64>, uid: usize, n: f64) {
        let mut prefix = vec![uid as f64];
        prefix.extend_from_slice(&ev1(n));
        mk_void(tr, 731.0, &prefix);
    }
    fn un_update_nuke_state(&mut self, tr: &mut Vec<f64>, uid: usize, wait_ticks: f64) {
        let obj = JsVal::Obj(vec![("waitTicks".to_string(), JsVal::Num(wait_ticks))]);
        let mut prefix = vec![uid as f64];
        prefix.extend_from_slice(&ev_enc(&obj));
        mk_void(tr, 732.0, &prefix);
    }
    fn un_set_trajectory_index(&mut self, tr: &mut Vec<f64>, uid: usize, i: f64) {
        let mut prefix = vec![uid as f64];
        prefix.extend_from_slice(&ev1(i));
        mk_void(tr, 733.0, &prefix);
    }
    fn un_set_targetable(&mut self, tr: &mut Vec<f64>, uid: usize, flag: bool) {
        let mut prefix = vec![uid as f64];
        prefix.extend_from_slice(&evb(flag));
        mk_void(tr, 734.0, &prefix);
    }
    fn un_target_unit(&mut self, tr: &mut Vec<f64>, uid: usize) -> Option<usize> {
        let v = self.un(uid, UN_TARGET_UNIT, tr, 735.0, &[]);
        as_opt_tok(&v)
    }
    fn un_touch(&mut self, tr: &mut Vec<f64>, uid: usize) {
        mk_void(tr, 736.0, &[uid as f64]);
    }
    fn un_is_unit(&mut self, tr: &mut Vec<f64>, uid: usize) -> bool {
        let v = self.un(uid, UN_IS_UNIT, tr, 737.0, &[]);
        as_bool(&v)
    }
    fn un_trajectory_index(&mut self, tr: &mut Vec<f64>, uid: usize) -> f64 {
        let v = self.un(uid, UN_TRAJECTORY_INDEX, tr, 738.0, &[]);
        as_num(&v)
    }
    fn un_nuke_state(&mut self, tr: &mut Vec<f64>, uid: usize) -> JsVal {
        self.un(uid, UN_NUKE_STATE, tr, 739.0, &[])
    }
    fn un_trajectory(&mut self, tr: &mut Vec<f64>, uid: usize) -> JsVal {
        self.un(uid, UN_TRAJECTORY, tr, 740.0, &[])
    }
    fn un_is_in_cooldown(&mut self, tr: &mut Vec<f64>, uid: usize) -> bool {
        let v = self.un(uid, UN_IS_IN_COOLDOWN, tr, 741.0, &[]);
        as_bool(&v)
    }
    fn un_launch(&mut self, tr: &mut Vec<f64>, uid: usize) {
        mk_void(tr, 742.0, &[uid as f64]);
    }
    fn un_targeted_by_sam(&mut self, tr: &mut Vec<f64>, uid: usize) -> bool {
        let v = self.un(uid, UN_TARGETED_BY_SAM, tr, 743.0, &[]);
        as_bool(&v)
    }

    // neighbor facade
    fn nb(&mut self, nid: usize, slot: usize, tr: &mut Vec<f64>, code: f64, extra: &[f64]) -> JsVal {
        let mut prefix = vec![nid as f64];
        prefix.extend_from_slice(extra);
        mk_take(&mut self.neighbors[nid], slot, tr, code, &prefix)
    }
    fn nb_is_player(&mut self, tr: &mut Vec<f64>, nid: usize) -> bool {
        let v = self.nb(nid, NB_IS_PLAYER, tr, 257.0, &[]);
        as_bool(&v)
    }
    fn nb_is_on_same_team(&mut self, tr: &mut Vec<f64>, nid: usize, other: Option<usize>) -> bool {
        let v = self.nb(nid, NB_IS_ON_SAME_TEAM, tr, 258.0, &evtok(other));
        as_bool(&v)
    }
    fn nb_update_relation(&mut self, tr: &mut Vec<f64>, nid: usize, other: Option<usize>, amt: f64) {
        let mut prefix = vec![nid as f64];
        prefix.extend_from_slice(&evtok(other));
        prefix.extend_from_slice(&ev1(amt));
        mk_void(tr, 256.0, &prefix);
    }

    fn rail_recompute(&mut self, tr: &mut Vec<f64>) {
        mk_void(tr, 290.0, &[]);
    }

    // ---- wouldNukeBreakAlliance (Util.ts:51-95) ----

    fn would_nuke_break_alliance(
        &mut self,
        tr: &mut Vec<f64>,
        target_tile: f64,
        inner: f64,
        outer: f64,
        ally_small_ids: &[f64],
        threshold: f64,
    ) -> bool {
        if ally_small_ids.is_empty() {
            return false;
        }
        let types: Vec<String> = STRUCTURES_TYPES.iter().map(|s| s.to_string()).collect();
        let ids = ally_small_ids.to_vec();
        let hit = self.mg_any_unit_nearby(tr, target_tile, outer, &types, |h, tr, uid| {
            let is_player = match h.un_owner(tr, uid) {
                Some(o) => h.pl_is_player(tr, o),
                None => false,
            };
            if !is_player {
                return false;
            }
            let small = match h.un_owner(tr, uid) {
                Some(o) => h.pl_small_id(tr, o),
                None => f64::NAN,
            };
            ids.contains(&small)
        });
        if hit {
            return true;
        }

        let inner2 = inner * inner;
        let mut result = false;
        let mut counts: Vec<(f64, f64)> = Vec::new();
        self.mg_circle_search(tr, target_tile, outer, |h, tr, tile, d2| {
            let owner_small = h.mg_owner_id(tr, tile);
            if owner_small > 0.0 && ids.contains(&owner_small) {
                let weight = if d2 <= inner2 { 1.0 } else { 0.5 };
                let new_count = match counts.iter_mut().find(|(id, _)| *id == owner_small) {
                    Some(e) => {
                        e.1 += weight;
                        e.1
                    }
                    None => {
                        counts.push((owner_small, weight));
                        weight
                    }
                };
                if new_count > threshold {
                    result = true;
                    return false;
                }
            }
            true
        });
        result
    }

    // ---- cancelNukesBetweenAlliedPlayers (AllianceRequestExecution.ts:89-171) ----

    fn cancel_nukes(&mut self, tr: &mut Vec<f64>, recipient: usize) -> Result<(), String> {
        let requestor = self.a_tok;
        // insertion-ordered Map<Player-token, count> and Set<Player-token>.
        let mut neutralized: Vec<(usize, f64)> = Vec::new();
        let mut cancelled: Vec<usize> = Vec::new();

        let players = [requestor, recipient];
        let nuke_types: Vec<String> = ["Atom Bomb", "Hydrogen Bomb", "MIRV", "MIRV Warhead"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        for launcher in players {
            let other = if launcher == requestor {
                recipient
            } else {
                requestor
            };
            let units = self.pl_units(tr, launcher, &nuke_types);
            for uid in units {
                if !self.un_is_active(tr, uid) || self.un_reached_target(tr, uid) {
                    continue;
                }
                let target_tile = self.un_target_tile(tr, uid);

                if self.un_type(tr, uid) == UNIT_MIRV {
                    let tp = self.un_target_player(tr, uid);
                    let target = match tp {
                        Some(t) => Some(t),
                        None => {
                            if truthy_tile(target_tile) {
                                self.mg_owner(tr, target_tile.unwrap())
                            } else {
                                None
                            }
                        }
                    };
                    if target != Some(other) {
                        continue;
                    }
                } else if self.un_type(tr, uid) == UNIT_MIRV_WARHEAD {
                    if !truthy_tile(target_tile) {
                        continue;
                    }
                    if self.mg_owner(tr, target_tile.unwrap()) != Some(other) {
                        continue;
                    }
                } else {
                    if !truthy_tile(target_tile) {
                        continue;
                    }
                    let ty = self.un_type(tr, uid);
                    let (inner, outer) = self.mg_nuke_magnitudes(tr, &ty);
                    let small = self.pl_small_id(tr, other);
                    let threshold = self.mg_nuke_alliance_break_threshold(tr);
                    if !self.would_nuke_break_alliance(
                        tr,
                        target_tile.unwrap(),
                        inner,
                        outer,
                        &[small],
                        threshold,
                    ) {
                        continue;
                    }
                }

                self.un_delete(tr, uid, false);
                if self.un_type(tr, uid) == UNIT_MIRV_WARHEAD {
                    if !cancelled.contains(&launcher) {
                        cancelled.push(launcher);
                    }
                } else {
                    match neutralized.iter_mut().find(|(l, _)| *l == launcher) {
                        Some(e) => e.1 += 1.0,
                        None => neutralized.push((launcher, 1.0)),
                    }
                }
            }
        }

        for launcher in cancelled {
            match neutralized.iter_mut().find(|(l, _)| *l == launcher) {
                Some(e) => e.1 += 1.0,
                None => neutralized.push((launcher, 1.0)),
            }
        }

        for &(launcher, count) in &neutralized {
            let other = if launcher == requestor {
                recipient
            } else {
                requestor
            };
            let l_id = self.pl_id(tr, launcher);
            let o_name = self.pl_display_name(tr, other);
            let o_id = self.pl_id(tr, other);
            self.mg_display_message(
                tr,
                &[
                    JsVal::Str("events_display.alliance_nukes_destroyed_outgoing".into()),
                    JsVal::Num(MSG_ALLIANCE_ACCEPTED),
                    JsVal::Str(l_id),
                    JsVal::Undef,
                    JsVal::Obj(vec![
                        ("name".into(), JsVal::Str(o_name)),
                        ("count".into(), JsVal::Num(count)),
                    ]),
                    JsVal::Undef,
                    JsVal::Str(o_id),
                ],
            );

            let o_id2 = self.pl_id(tr, other);
            let l_name = self.pl_display_name(tr, launcher);
            let l_id2 = self.pl_id(tr, launcher);
            self.mg_display_message(
                tr,
                &[
                    JsVal::Str("events_display.alliance_nukes_destroyed_incoming".into()),
                    JsVal::Num(MSG_ALLIANCE_ACCEPTED),
                    JsVal::Str(o_id2),
                    JsVal::Undef,
                    JsVal::Obj(vec![
                        ("name".into(), JsVal::Str(l_name)),
                        ("count".into(), JsVal::Num(count)),
                    ]),
                    JsVal::Undef,
                    JsVal::Str(l_id2),
                ],
            );
        }
        Ok(())
    }

    // ---- construct ----

    fn construct(&mut self, args: &[f64]) {
        let mut i = 0;
        let v = read_val(args, &mut i);
        self.class_id = val_field(&v, "c").map(as_num).unwrap_or(0.0) as u8;
        let ctor = as_arr(val_field(&v, "a").unwrap_or(&JsVal::Absent));

        self.mg = val_field(&v, "mg").map(build_mock).unwrap_or_default();
        self.players = val_field(&v, "players")
            .map(as_arr)
            .unwrap_or_default()
            .iter()
            .map(build_mock)
            .collect();
        self.alliances = val_field(&v, "alliances")
            .map(as_arr)
            .unwrap_or_default()
            .iter()
            .map(build_mock)
            .collect();
        self.requests = val_field(&v, "requests")
            .map(as_arr)
            .unwrap_or_default()
            .iter()
            .map(build_mock)
            .collect();
        self.units = val_field(&v, "units")
            .map(as_arr)
            .unwrap_or_default()
            .iter()
            .map(build_mock)
            .collect();
        self.neighbors = val_field(&v, "neighbors")
            .map(as_arr)
            .unwrap_or_default()
            .iter()
            .map(build_mock)
            .collect();

        let n = |k: usize| -> f64 { ctor.get(k).map(as_num).unwrap_or(f64::NAN) };
        match self.class_id {
            1 | 4 => {
                self.a_tok = n(0) as usize;
                self.flag = ctor.get(1).map(as_bool).unwrap_or(false);
            }
            5 => {
                self.a_tok = n(0) as usize;
                self.id_a = ctor.get(1).map(as_str).unwrap_or_default();
                self.action = ctor.get(2).map(as_str).unwrap_or_default();
                // EmbargoExecution: `private active = true`.
                self.flag = true;
            }
            6 => {
                self.a_tok = n(0) as usize;
                self.action = ctor.get(1).map(as_str).unwrap_or_default();
            }
            7 | 8 | 10 => {
                self.a_tok = n(0) as usize;
                self.id_a = ctor.get(1).map(as_str).unwrap_or_default();
                // TargetPlayer / AllianceRequest / BreakAlliance: `active = true`.
                self.flag = true;
            }
            9 => {
                self.a_tok = n(0) as usize;
                self.id_a = ctor.get(1).map(as_str).unwrap_or_default();
            }
            11 => {
                self.id_a = ctor.first().map(as_str).unwrap_or_default();
                self.b_tok = n(1) as usize;
                // AllianceRejectExecution: `private active = true`.
                self.flag = true;
            }
            12 => {
                self.a_tok = n(0) as usize;
                self.id_a = ctor.get(1).map(as_str).unwrap_or_default();
                // RetreatExecution: `private active = true`.
                self.flag = true;
            }
            13 => {
                self.a_tok = n(0) as usize;
                self.unit_id = n(1);
                // BoatRetreatExecution: `private active = true`.
                self.flag = true;
            }
            14 => {
                self.a_tok = n(0) as usize;
                self.unit_ids = ctor.get(1).map(as_num_list).unwrap_or_default();
                self.position = n(2);
            }
            15 => {
                self.a_tok = n(0) as usize;
                self.unit_id = n(1);
                // DeleteUnitExecution: `private active = true`.
                self.flag = true;
            }
            16 => {
                self.a_tok = n(0) as usize;
                self.id_a = ctor.get(1).map(as_str).unwrap_or_default();
                self.quick_key = ctor.get(2).map(as_str).unwrap_or_default();
                self.chat_target = ctor.get(3).and_then(as_str_opt);
                // QuickChatExecution: `private active = true`.
                self.flag = true;
            }
            17 => {
                self.a_tok = n(0) as usize;
                // MissileSiloExecution: `private active = true`.
                self.flag = true;
            }
            18 => {
                self.a_tok = n(0) as usize;
                self.unit_id = n(1);
                self.amount = match ctor.get(2) {
                    Some(JsVal::Num(m)) => *m,
                    _ => 1.0,
                };
            }
            19..=21 => {
                self.a_tok = n(0) as usize;
                // City / Factory / DefensePost: `private active = true`.
                self.flag = true;
            }
            22 => {
                self.a_tok = n(0) as usize;
                self.id_a = ctor.get(1).map(as_str).unwrap_or_default();
                self.emoji = n(2);
                // EmojiExecution: `private active = true`.
                self.flag = true;
            }
            23 => {
                self.a_tok = n(0) as usize;
                self.id_a = ctor.get(1).map(as_str).unwrap_or_default();
                self.troops = as_opt_num(ctor.get(2));
                // DonateTroopsExecution: `private active = true`.
                self.flag = true;
            }
            24 => {
                self.a_tok = n(0) as usize;
                self.id_a = ctor.get(1).map(as_str).unwrap_or_default();
                // `this.gold = goldNum !== null ? toInt(goldNum) : null`.
                self.gold = match ctor.get(2) {
                    None | Some(JsVal::Null) | Some(JsVal::Undef) => None,
                    Some(v) => crate::util::to_int(as_num(v)),
                };
                // DonateGoldExecution: `private active = true`.
                self.flag = true;
            }
            25 => {
                // WinCheckExecution: `private active = true`.
                self.flag = true;
            }
            26 => {
                // ShellExecution(spawn, _owner, ownerUnit, target).
                self.spawn = n(0);
                self.a_tok = n(1) as usize;
                self.owner_unit = Some(n(2) as usize);
                self.target = Some(n(3) as usize);
                self.destroy_at_tick = -1.0;
                // `private active = true`.
                self.flag = true;
            }
            27 => {
                // SAMMissileExecution(spawn, _owner, ownerUnit, target, targetTile).
                self.spawn = n(0);
                self.a_tok = n(1) as usize;
                self.owner_unit = Some(n(2) as usize);
                self.target = Some(n(3) as usize);
                self.target_tile = n(4);
                // `private active = true`.
                self.flag = true;
            }
            28 => {
                // PortExecution(port): `private active = true`.
                self.a_tok = n(0) as usize;
                self.flag = true;
            }
            29 => {
                // ConstructionExecution(player, constructionType, tile,
                // rocketDirectionUp?, amount?).
                self.a_tok = n(0) as usize;
                self.construction_type = ctor.get(1).map(as_str).unwrap_or_default();
                self.spawn = n(2);
                self.rocket_direction_up = ctor.get(3).and_then(|v| match v {
                    JsVal::Bool(b) => Some(*b),
                    _ => None,
                });
                self.el_amount = as_opt_num(ctor.get(4));
                // `private active = true`.
                self.flag = true;
            }
            30 => {
                // DoomsdayClockExecution(): no ctor args; `private active = true`,
                // `private mg: Game | null = null`, empty `rotState`.
                self.flag = true;
                self.mg_ready = false;
                self.el_rot_state = Vec::new();
            }
            31 => {
                // TribeSpawner(gameID, nationCells = []).
                self.el_game_id = ctor.first().map(as_str).unwrap_or_default();
                self.el_nation_cells = ctor.get(1).map(as_pairs).unwrap_or_default();
                self.el_used_custom_tribes = Vec::new();
                // `new PseudoRandom(simpleHash(gameID) + 2)`.
                self.random = Some(PseudoRandom::new(
                    crate::util::simple_hash(&self.el_game_id) + 2.0,
                ));
                self.replay_tribe_spawner_ctor();
            }
            32 => {
                // SpawnExecution(gameID, playerInfo, tile?, fromIntent = false).
                self.el_game_id = ctor.first().map(as_str).unwrap_or_default();
                self.el_player_id = ctor.get(1).map(as_str).unwrap_or_default();
                self.el_player_type = ctor.get(2).map(as_str).unwrap_or_default();
                self.el_spawn_tile = as_opt_num(ctor.get(3));
                self.el_from_intent = ctor.get(4).map(as_bool).unwrap_or(false);
                self.el_queued_spawn = false;
                // `new PseudoRandom(simpleHash(playerInfo.id) + simpleHash(gameID))`.
                self.random = Some(PseudoRandom::new(
                    crate::util::simple_hash(&self.el_player_id)
                        + crate::util::simple_hash(&self.el_game_id),
                ));
                // `active: boolean = true`.
                self.flag = true;
                self.mg_ready = false;
            }
            33 => {
                // NukeExecution(nukeType, player, dst, src?, speed = -1,
                // waitTicks = 0, rocketDirectionUp = true). `active = true`.
                self.nuke_type = ctor.first().map(as_str).unwrap_or_default();
                self.a_tok = n(1) as usize;
                self.nuke_dst = n(2);
                self.nuke_src = as_opt_num(ctor.get(3));
                self.nuke_speed = ctor.get(4).map(as_num).unwrap_or(-1.0);
                self.nuke_wait_ticks = ctor.get(5).map(as_num).unwrap_or(0.0);
                self.nuke_rocket_up = ctor.get(6).map(as_bool).unwrap_or(true);
                self.nuke = None;
                self.tiles_to_destroy_cache = None;
                self.flag = true;
            }
            34 => {
                // MirvExecution(player, dst). `active = true`, `range = 1500`.
                self.a_tok = n(0) as usize;
                self.mirv_dst = n(1);
                self.mirv_range_squared = 1500.0 * 1500.0;
                self.mirv_nuke = None;
                self.mirv_full_path = Vec::new();
                self.mirv_path_index = 0;
                self.mirv_staged_targets = Vec::new();
                self.mirv_warheads_spawned = false;
                self.mirv_warhead_count = MIRV_WARHEAD_COUNT as usize;
                self.flag = true;
                self.mg_ready = false;
            }
            35 => {
                // SAMLauncherExecution(player, tileArg, sam = null). The ctor
                // reads `sam.tile()` when a unit is supplied; the capture drops
                // that read (kind 0) but the script cursor advances, so replay
                // it into a scratch buffer.
                self.a_tok = n(0) as usize;
                let sam_arg = as_opt_tok(ctor.get(2).unwrap_or(&JsVal::Absent));
                self.sam_unit = sam_arg;
                self.sam_targeting = None;
                self.sam_last_level = 1.0;
                self.flag = true;
                self.mg_ready = false;
                if let Some(sam) = sam_arg {
                    let mut scratch: Vec<f64> = Vec::new();
                    self.sam_tile = Some(self.un_tile(&mut scratch, sam));
                } else {
                    // The ctor `tile` argument is `TileRef | null`; `null` throws
                    // "tile is null" on the first tick.
                    self.sam_tile = as_opt_num(ctor.get(1));
                }
            }
            _ => {}
        }
    }

    /// The `TribeSpawner` ctor body. Its facade reads are discarded by the
    /// capture (kind `0` returns `[0, 0]`), but the script cursors they advance
    /// must be replayed so later ops line up. `nationCells.filter(...).map(...)`
    /// runs every `isValidCoord` before any `ref`.
    fn replay_tribe_spawner_ctor(&mut self) {
        let mut scratch: Vec<f64> = Vec::new();
        self.el_rot_state = Vec::new();
        let map = self.mg_game_map(&mut scratch);
        let (_, data) = crate::tribe_names::resolve_tribe_name_data(Some(&map), &[], &[]);
        self.el_tribe_data = data.ok();
        let cells = self.el_nation_cells.clone();
        let mut valid: Vec<(f64, f64)> = Vec::new();
        for (x, y) in cells {
            if self.mg_is_valid_coord(&mut scratch, x, y) {
                valid.push((x, y));
            }
        }
        let mut tiles: Vec<f64> = Vec::new();
        for (x, y) in valid {
            let t = self.mg_ref(&mut scratch, x, y);
            if !tiles.iter().any(|v| svz(*v, t)) {
                tiles.push(t);
            }
        }
        self.el_nation_tiles = tiles;
    }

    // ---- the op dispatcher ----

    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        if kind == 0 {
            self.reset();
            self.construct(args);
            return vec![0.0, 0.0];
        }
        let ticks = if args.is_empty() { 0.0 } else { args[0] };
        let mut tr: Vec<f64> = Vec::new();
        let res = self.dispatch(kind, ticks, args, &mut tr);
        let mut out: Vec<f64> = Vec::new();
        match res {
            Ok(v) => {
                out.push(0.0);
                push_val(&mut out, &v);
            }
            Err(msg) => {
                out.push(1.0);
                push_str(&mut out, &msg);
            }
        }
        let mut full = Vec::with_capacity(1 + tr.len() + out.len());
        full.push(tr.len() as f64);
        full.extend_from_slice(&tr);
        full.extend_from_slice(&out);
        full
    }

    fn dispatch(
        &mut self,
        kind: u8,
        ticks: f64,
        args: &[f64],
        tr: &mut Vec<f64>,
    ) -> Result<JsVal, String> {
        match self.class_id {
            0 => Ok(self.noop(kind)),
            1 => self.pause(kind, tr),
            2 => self.spawn_timer(kind, tr),
            3 => self.recompute_rail_cluster(kind, tr),
            4 => self.mark_disconnected(kind, ticks, tr),
            5 => self.embargo(kind, tr),
            6 => self.embargo_all(kind, tr),
            7 => self.target_player(kind, tr),
            8 => self.alliance_request(kind, tr),
            9 => self.alliance_extension(kind, tr),
            10 => self.break_alliance(kind, tr),
            11 => self.alliance_reject(kind, tr),
            12 => self.retreat(kind, tr),
            13 => self.boat_retreat(kind, tr),
            14 => self.move_warship(kind, tr),
            15 => self.delete_unit(kind, tr),
            16 => self.quick_chat(kind, tr),
            17 => self.missile_silo(kind, tr),
            18 => self.upgrade_structure(kind, tr),
            19 => self.city(kind, tr),
            20 => self.factory(kind, tr),
            21 => self.defense_post(kind, tr),
            22 => self.emoji(kind, tr),
            23 => self.donate_troops_exec(kind, tr),
            24 => self.donate_gold_exec(kind, ticks, tr),
            25 => self.win_check(kind, ticks, tr),
            26 => self.shell(kind, tr),
            27 => self.sam_missile(kind, tr),
            28 => self.port(kind, tr),
            29 => self.construction(kind, tr),
            30 => self.doomsday_clock_exec(kind, ticks, tr),
            31 => self.tribe_spawner(kind, args, tr),
            32 => self.spawn_execution(kind, ticks, tr),
            33 => self.nuke_execution(kind, ticks, tr),
            34 => self.mirv_execution(kind, ticks, tr),
            35 => self.sam_launcher_execution(kind, ticks, tr),
            k => unreachable!("exec harness: unknown class id {k}"),
        }
    }

    fn noop(&mut self, kind: u8) -> JsVal {
        match kind {
            3 | 4 => JsVal::Bool(false),
            _ => JsVal::Undef,
        }
    }

    // 1 Pause
    fn pause(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(false)),
            4 => Ok(JsVal::Bool(true)),
            1 => {
                let creator = self.pl_is_lobby_creator(tr, self.a_tok);
                if creator || self.mg_game_type(tr) == GAME_TYPE_SINGLEPLAYER {
                    self.mg_set_paused(tr, self.flag);
                }
                Ok(JsVal::Undef)
            }
            2 => Ok(JsVal::Undef),
            _ => unreachable!(),
        }
    }

    // 2 SpawnTimer
    fn spawn_timer(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => {
                if !self.mg_ready {
                    return Err("Cannot read properties of undefined (reading 'inSpawnPhase')".into());
                }
                Ok(JsVal::Bool(self.mg_in_spawn_phase(tr)))
            }
            4 => Ok(JsVal::Bool(true)),
            1 => {
                self.mg_ready = true;
                Ok(JsVal::Undef)
            }
            2 => {
                if !self.mg_ready {
                    return Err("Cannot read properties of undefined (reading 'ticks')".into());
                }
                let t = self.mg_ticks(tr);
                if t > self.mg_num_spawn_phase_turns(tr) {
                    self.mg_end_spawn_phase(tr);
                }
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    // 3 RecomputeRailCluster
    fn recompute_rail_cluster(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(true)),
            4 => Ok(JsVal::Bool(false)),
            2 => {
                self.rail_recompute(tr);
                Ok(JsVal::Undef)
            }
            _ => Ok(JsVal::Undef),
        }
    }

    // 4 MarkDisconnected
    fn mark_disconnected(&mut self, kind: u8, ticks: f64, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 | 4 => Ok(JsVal::Bool(false)),
            1 => {
                if self.flag {
                    let team_v = self.pl_team(tr, self.a_tok);
                    let team = as_str_opt(&team_v);
                    let team_tiles = match &team {
                        Some(s) if !s.is_empty() => self.mg_team_tiles_owned(tr, Some(s)),
                        _ => 0.0,
                    };
                    let total_land = self.mg_total_land_tiles(tr);
                    let alive = self.pl_is_alive(tr, self.a_tok);
                    let obj = JsVal::Obj(vec![
                        ("currentTick".into(), JsVal::Num(ticks)),
                        ("teamTiles".into(), JsVal::Num(team_tiles)),
                        ("totalLand".into(), JsVal::Num(total_land)),
                        ("wasAlive".into(), JsVal::Bool(alive)),
                    ]);
                    self.pl_mark_disconnected(tr, self.a_tok, true, obj);
                } else {
                    self.pl_mark_disconnected(tr, self.a_tok, false, JsVal::Undef);
                }
                Ok(JsVal::Undef)
            }
            _ => Ok(JsVal::Undef),
        }
    }

    // 5 Embargo
    fn embargo(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => {
                let id_a = self.id_a.clone();
                if !self.mg_has_player(tr, &id_a) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                self.target = self.mg_player(tr, &id_a);
                Ok(JsVal::Undef)
            }
            2 => {
                if self.action == "start" {
                    self.pl_add_embargo(tr, self.a_tok, self.target, false);
                } else {
                    self.pl_stop_embargo(tr, self.a_tok, self.target);
                }
                self.flag = false;
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    // 6 EmbargoAll
    fn embargo_all(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 | 4 => Ok(JsVal::Bool(false)),
            1 => {
                if !self.pl_can_embargo_all(tr, self.a_tok) {
                    return Ok(JsVal::Undef);
                }
                let players = self.mg_players(tr);
                for p in players {
                    let pid = self.pl_id(tr, p);
                    let me_id = self.pl_id(tr, self.a_tok);
                    if pid == me_id {
                        continue;
                    }
                    if self.pl_type(tr, p) == PLAYER_TYPE_BOT {
                        continue;
                    }
                    if self.pl_is_on_same_team(tr, self.a_tok, Some(p)) {
                        continue;
                    }
                    if self.action == "start" {
                        if !self.pl_has_embargo_against(tr, self.a_tok, Some(p)) {
                            self.pl_add_embargo(tr, self.a_tok, Some(p), false);
                        }
                    } else {
                        if self.pl_has_embargo_against(tr, self.a_tok, Some(p)) {
                            self.pl_stop_embargo(tr, self.a_tok, Some(p));
                        }
                    }
                }
                self.pl_record_embargo_all(tr, self.a_tok);
                Ok(JsVal::Undef)
            }
            2 => Ok(JsVal::Undef),
            _ => unreachable!(),
        }
    }

    // 7 TargetPlayer
    fn target_player(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => {
                let id_a = self.id_a.clone();
                if !self.mg_has_player(tr, &id_a) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                self.target = self.mg_player(tr, &id_a);
                Ok(JsVal::Undef)
            }
            2 => {
                if self.pl_can_target(tr, self.a_tok, self.target) {
                    self.pl_target(tr, self.a_tok, self.target);
                    if let Some(t) = self.target {
                        self.pl_update_relation(tr, t, Some(self.a_tok), -40.0);
                    }
                }
                self.flag = false;
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    // 8 AllianceRequest
    fn alliance_request(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => {
                self.mg_ready = true;
                let id_a = self.id_a.clone();
                if !self.mg_has_player(tr, &id_a) {
                    return Ok(JsVal::Undef);
                }
                let recipient = self.mg_player(tr, &id_a).unwrap_or(0);
                if !self.pl_can_send_alliance_request(tr, self.a_tok, Some(recipient)) {
                    self.flag = false;
                } else {
                    let reqs = self.pl_outgoing_alliance_requests(tr, recipient);
                    let mut incoming = None;
                    for rid in reqs {
                        if self.rq_recipient(tr, rid) == Some(self.a_tok) {
                            incoming = Some(rid);
                            break;
                        }
                    }
                    if let Some(rid) = incoming {
                        self.flag = false;
                        self.rq_accept(tr, rid);
                        self.pl_update_relation(tr, self.a_tok, Some(recipient), 100.0);
                        self.pl_update_relation(tr, recipient, Some(self.a_tok), 100.0);
                        if self.pl_has_embargo_against(tr, self.a_tok, Some(recipient)) {
                            self.pl_end_temporary_embargo(tr, self.a_tok, Some(recipient));
                        }
                        if self.pl_has_embargo_against(tr, recipient, Some(self.a_tok)) {
                            self.pl_end_temporary_embargo(tr, recipient, Some(self.a_tok));
                        }
                        self.cancel_nukes(tr, recipient)?;
                    } else {
                        self.req = self.pl_create_alliance_request(tr, self.a_tok, Some(recipient));
                    }
                }
                Ok(JsVal::Undef)
            }
            2 => {
                if !self.mg_ready {
                    return Err("Cannot read properties of undefined (reading 'ticks')".into());
                }
                let req = self.req;
                let st1 = req.map(|r| self.rq_status(tr, r));
                if st1.as_deref() == Some("accepted") {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                let st2 = req.map(|r| self.rq_status(tr, r));
                if st2.as_deref() == Some("rejected") {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                let ticks_now = self.mg_ticks(tr);
                let created = match self.req {
                    Some(r) => self.rq_created_at(tr, r),
                    None => 0.0,
                };
                if ticks_now - created > self.mg_alliance_request_duration(tr) {
                    if let Some(r) = self.req {
                        self.rq_reject(tr, r);
                    }
                    self.flag = false;
                }
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    // 9 AllianceExtension
    fn alliance_extension(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 | 4 => Ok(JsVal::Bool(false)),
            1 => {
                let from = self.a_tok;
                let id_a = self.id_a.clone();
                if !self.mg_has_player(tr, &id_a) {
                    return Ok(JsVal::Undef);
                }
                let to = self.mg_player(tr, &id_a).unwrap_or(0);
                if !self.pl_is_alive(tr, from) || !self.pl_is_alive(tr, to) {
                    // `console.info` reads `this.from.id()`.
                    let _ = self.pl_id(tr, from);
                    return Ok(JsVal::Undef);
                }
                let alliance = self.pl_alliance_with(tr, from, Some(to));
                let aid = match alliance {
                    Some(a) => a,
                    None => {
                        // `console.warn` reads `this.from.id()`.
                        let _ = self.pl_id(tr, from);
                        return Ok(JsVal::Undef);
                    }
                };
                let was_only_one = self.al_only_one(tr, aid);
                self.al_add_extension_request(tr, aid, from);
                if self.al_both_agreed(tr, aid) {
                    self.al_extend(tr, aid);
                    let f_id = self.pl_id(tr, from);
                    let t_name = self.pl_display_name(tr, to);
                    let t_id = self.pl_id(tr, to);
                    self.mg_display_message(
                        tr,
                        &[
                            JsVal::Str("events_display.alliance_renewed".into()),
                            JsVal::Num(MSG_ALLIANCE_ACCEPTED),
                            JsVal::Str(f_id),
                            JsVal::Undef,
                            JsVal::Obj(vec![("name".into(), JsVal::Str(t_name))]),
                            JsVal::Undef,
                            JsVal::Str(t_id),
                        ],
                    );
                    let f_name = self.pl_display_name(tr, from);
                    let f_id2 = self.pl_id(tr, from);
                    self.mg_display_message(
                        tr,
                        &[
                            JsVal::Str("events_display.alliance_renewed".into()),
                            JsVal::Num(MSG_ALLIANCE_ACCEPTED),
                            JsVal::Str(self.id_a.clone()),
                            JsVal::Undef,
                            JsVal::Obj(vec![("name".into(), JsVal::Str(f_name))]),
                            JsVal::Undef,
                            JsVal::Str(f_id2),
                        ],
                    );
                } else if self.al_only_one(tr, aid) && !was_only_one {
                    let f_name = self.pl_display_name(tr, from);
                    let f_id = self.pl_id(tr, from);
                    self.mg_display_message(
                        tr,
                        &[
                            JsVal::Str("events_display.wants_to_renew_alliance".into()),
                            JsVal::Num(MSG_RENEW_ALLIANCE),
                            JsVal::Str(self.id_a.clone()),
                            JsVal::Undef,
                            JsVal::Obj(vec![("name".into(), JsVal::Str(f_name))]),
                            JsVal::Undef,
                            JsVal::Str(f_id),
                        ],
                    );
                }
                Ok(JsVal::Undef)
            }
            2 => Ok(JsVal::Undef),
            _ => unreachable!(),
        }
    }

    // 10 BreakAlliance
    fn break_alliance(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => {
                let id_a = self.id_a.clone();
                if !self.mg_has_player(tr, &id_a) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                self.target = self.mg_player(tr, &id_a);
                self.mg_ready = true;
                Ok(JsVal::Undef)
            }
            2 => {
                if !self.mg_ready {
                    return Err("Not initialized".into());
                }
                let recipient = self.target.unwrap_or(0);
                let alliance = self.pl_alliance_with(tr, self.a_tok, Some(recipient));
                match alliance {
                    None => { /* warn: cant break alliance, not allied */ }
                    Some(al) => {
                        self.pl_break_alliance(tr, self.a_tok, al);
                        self.pl_update_relation(tr, recipient, Some(self.a_tok), -100.0);
                        let nbrs = self.pl_nearby(tr, self.a_tok);
                        let mut filtered = Vec::new();
                        for nid in nbrs {
                            if self.nb_is_player(tr, nid)
                                && !self.nb_is_on_same_team(tr, nid, Some(recipient))
                            {
                                filtered.push(nid);
                            }
                        }
                        for nid in filtered {
                            self.nb_update_relation(tr, nid, Some(self.a_tok), -40.0);
                        }
                    }
                }
                self.flag = false;
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    // 11 AllianceReject
    fn alliance_reject(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => {
                let id_a = self.id_a.clone();
                if !self.mg_has_player(tr, &id_a) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                let requestor = self.mg_player(tr, &id_a).unwrap_or(0);
                if self.pl_is_friendly(tr, requestor, Some(self.b_tok)) {
                    // warn: already allied (the TS `this.recipient.id` has no
                    // call parens, so it does not touch the facade).
                } else {
                    let reqs = self.pl_outgoing_alliance_requests(tr, requestor);
                    let mut found = None;
                    for rid in reqs {
                        if self.rq_recipient(tr, rid) == Some(self.b_tok) {
                            found = Some(rid);
                            break;
                        }
                    }
                    match found {
                        None => { /* warn: no alliance request found */ }
                        Some(rid) => self.rq_reject(tr, rid),
                    }
                }
                self.flag = false;
                Ok(JsVal::Undef)
            }
            2 => Ok(JsVal::Undef),
            _ => unreachable!(),
        }
    }

    // 12 Retreat
    fn retreat(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => {
                self.mg_ready = true;
                self.start_tick = self.mg_ticks(tr);
                Ok(JsVal::Undef)
            }
            2 => {
                if !self.retreat_ordered {
                    let id_a = self.id_a.clone();
                    self.pl_order_retreat(tr, self.a_tok, &id_a);
                    self.retreat_ordered = true;
                }
                if !self.mg_ready {
                    return Err("Cannot read properties of undefined (reading 'ticks')".into());
                }
                let t = self.mg_ticks(tr);
                if t >= self.start_tick + RETREAT_CANCEL_DELAY {
                    let id_a = self.id_a.clone();
                    self.pl_execute_retreat(tr, self.a_tok, &id_a);
                    self.flag = false;
                }
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    // 13 BoatRetreat
    fn boat_retreat(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => Ok(JsVal::Undef),
            2 => {
                let units = self.pl_units(tr, self.a_tok, &[]);
                let mut found = None;
                for uid in units {
                    let id = self.un_id(tr, uid);
                    if id == self.unit_id && self.un_type(tr, uid) == UNIT_TRANSPORT {
                        found = Some(uid);
                        break;
                    }
                }
                match found {
                    None => { /* warn: outgoing boat not found */ }
                    Some(uid) => self.un_update_transport_ship_state(tr, uid),
                }
                self.flag = false;
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    // 14 MoveWarship
    fn move_warship(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 | 4 => Ok(JsVal::Bool(false)),
            2 => Ok(JsVal::Undef),
            1 => {
                if !self.mg_is_valid_ref(tr, self.position) {
                    /* warn: position not valid */
                    return Ok(JsVal::Undef);
                }
                let comp = self.mg_get_water_component(tr, self.position);
                let warship_ty = vec![UNIT_WARSHIP.to_string()];
                let warships = self.pl_units(tr, self.a_tok, &warship_ty);
                // `new Map(units.map(u => [u.id(), u]))` — SameValueZero keys,
                // overwrite in place keeps the first insertion position.
                let mut map: Vec<(f64, usize)> = Vec::new();
                for uid in warships {
                    let id = self.un_id(tr, uid);
                    match map.iter_mut().find(|(k, _)| svz(*k, id)) {
                        Some(e) => e.1 = uid,
                        None => map.push((id, uid)),
                    }
                }
                // `for (const unitId of new Set(this.unitIds))`.
                let mut dedup: Vec<f64> = Vec::new();
                for id in self.unit_ids.clone() {
                    if !dedup.iter().any(|x| svz(*x, id)) {
                        dedup.push(id);
                    }
                }
                for id in dedup {
                    let uid = match map.iter().find(|(k, _)| svz(*k, id)) {
                        Some((_, t)) => *t,
                        None => continue,
                    };
                    if !self.un_is_active(tr, uid) {
                        continue;
                    }
                    let tile = self.un_tile(tr, uid);
                    if !self.mg_has_water_component(tr, tile, &comp) {
                        continue;
                    }
                    self.un_update_warship_state(tr, uid, self.position);
                    self.un_set_target_tile(tr, uid);
                }
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    // 15 DeleteUnit
    fn delete_unit(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            2 => {
                if !self.flag || self.unit.is_none() {
                    return Ok(JsVal::Undef);
                }
                let ui = self.unit.unwrap();
                if !self.un_is_active(tr, ui) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                if self.un_is_overdue_deletion(tr, ui) {
                    self.un_delete(tr, ui, false);
                    let pid = self.pl_id(tr, self.a_tok);
                    self.mg_display_message(
                        tr,
                        &[
                            JsVal::Str("events_display.unit_voluntarily_deleted".into()),
                            JsVal::Num(MSG_UNIT_DESTROYED),
                            JsVal::Str(pid),
                        ],
                    );
                    self.flag = false;
                }
                Ok(JsVal::Undef)
            }
            1 => {
                if !self.flag {
                    return Ok(JsVal::Undef);
                }
                self.mg_ready = true;
                let unit = self.mg_unit(tr, self.unit_id);
                let bad = match unit {
                    None => true,
                    Some(u) => self.un_owner(tr, u) != Some(self.a_tok),
                };
                if bad {
                    // warn reads `this.player.displayName()`.
                    let _ = self.pl_display_name(tr, self.a_tok);
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                let ui = unit.unwrap();
                if !self.un_is_active(tr, ui) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                self.unit = Some(ui);
                let tile = self.un_tile(tr, ui);
                let tile_owner = match self.mg_owner(tr, tile) {
                    Some(o) => o,
                    None => {
                        return Err("Cannot read properties of null (reading 'isPlayer')".into());
                    }
                };
                let bad2 = if !self.pl_is_player(tr, tile_owner) {
                    true
                } else {
                    self.pl_id(tr, tile_owner) != self.pl_id(tr, self.a_tok)
                };
                if bad2 {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                let tile2 = self.un_tile(tr, ui);
                if !self.mg_is_land(tr, tile2) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                if self.mg_in_spawn_phase(tr) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                if !self.pl_can_delete_unit(tr, self.a_tok) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                self.pl_record_delete_unit(tr, self.a_tok);
                self.un_mark_for_deletion(tr, ui);
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    // 16 QuickChat
    fn quick_chat(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => {
                self.mg_ready = true;
                let id_a = self.id_a.clone();
                if !self.mg_has_player(tr, &id_a) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                self.recipient = self.mg_player(tr, &id_a);
                Ok(JsVal::Undef)
            }
            2 => {
                if !self.pl_can_send_quick_chat(tr, self.a_tok, self.recipient) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                let parts = split_dot(&self.quick_key);
                let m0 = parts.first().cloned();
                let m1 = parts.get(1).cloned();
                self.pl_record_quick_chat(tr, self.a_tok, self.recipient);
                if !self.mg_ready {
                    return Err("Cannot read properties of undefined (reading 'displayChat')".into());
                }
                let target = self.chat_target.clone();
                let rid = self.recipient.map(|r| self.pl_id(tr, r));
                let sid = self.pl_id(tr, self.a_tok);
                self.mg_display_chat(
                    tr,
                    m1.as_deref(),
                    m0.as_deref(),
                    target.as_deref(),
                    rid.as_deref(),
                    true,
                    Some(&sid),
                );
                let sid2 = self.pl_id(tr, self.a_tok);
                let rid2 = self.recipient.map(|r| self.pl_id(tr, r));
                self.mg_display_chat(
                    tr,
                    m1.as_deref(),
                    m0.as_deref(),
                    target.as_deref(),
                    Some(&sid2),
                    false,
                    rid2.as_deref(),
                );
                self.flag = false;
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    // 17 MissileSilo
    fn missile_silo(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => {
                self.mg_ready = true;
                Ok(JsVal::Undef)
            }
            2 => {
                if self.un_is_under_construction(tr, self.a_tok) {
                    return Ok(JsVal::Undef);
                }
                if !self.un_is_active(tr, self.a_tok) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                let q = self.un_missile_timer_queue(tr, self.a_tok);
                let front = match as_num_list(&q).first() {
                    Some(f) => *f,
                    None => return Ok(JsVal::Undef),
                };
                if !self.mg_ready {
                    return Err("Cannot read properties of undefined (reading 'config')".into());
                }
                let cd = self.mg_silo_cooldown(tr);
                let ticks = self.mg_ticks(tr);
                if cd - (ticks - front) <= 0.0 {
                    self.un_reload_missile(tr, self.a_tok);
                }
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    // 18 UpgradeStructure
    fn upgrade_structure(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 | 4 => Ok(JsVal::Bool(false)),
            2 => Ok(JsVal::Undef),
            1 => {
                self.structure = self.mg_unit(tr, self.unit_id);
                if let Some(ui) = self.structure {
                    if self.un_owner(tr, ui) != Some(self.a_tok) {
                        /* warn: structure not owned by player */
                        self.structure = None;
                    }
                }
                let ui = match self.structure {
                    Some(u) => u,
                    None => {
                        /* warn: structure is undefined */
                        return Ok(JsVal::Undef);
                    }
                };
                let mut i = 0.0;
                while i < self.amount {
                    if !self.pl_can_upgrade_unit(tr, self.a_tok, ui) {
                        if i == 0.0 {
                            // warn reads `this.structure.type()`.
                            let _ = self.un_type(tr, ui);
                        }
                        break;
                    }
                    self.pl_upgrade_unit(tr, self.a_tok, ui);
                    i += 1.0;
                }
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    // 19 City
    fn city(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => {
                self.mg_ready = true;
                Ok(JsVal::Undef)
            }
            2 => {
                if !self.station_created {
                    if !self.mg_ready {
                        return Err(
                            "Cannot read properties of undefined (reading 'hasUnitNearby')".into(),
                        );
                    }
                    let tile = self.un_tile(tr, self.a_tok);
                    let range = self.mg_train_station_max_range(tr);
                    if self.mg_has_unit_nearby(tr, tile, range, UNIT_FACTORY) {
                        self.mg_add_train_station(tr, self.a_tok, None);
                    }
                    self.station_created = true;
                }
                if !self.un_is_active(tr, self.a_tok) {
                    self.flag = false;
                }
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    // 20 Factory
    fn factory(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => {
                self.mg_ready = true;
                Ok(JsVal::Undef)
            }
            2 => {
                if !self.station_created {
                    if !self.mg_ready {
                        return Err(
                            "Cannot read properties of undefined (reading 'nearbyUnits')".into(),
                        );
                    }
                    let tile = self.un_tile(tr, self.a_tok);
                    let range = self.mg_train_station_max_range(tr);
                    let types = vec![
                        UNIT_CITY.to_string(),
                        UNIT_PORT.to_string(),
                        UNIT_FACTORY.to_string(),
                    ];
                    let units = self.mg_nearby_units(tr, tile, range, &types);
                    self.mg_add_train_station(tr, self.a_tok, Some(true));
                    for uid in units {
                        if !self.un_has_train_station(tr, uid) {
                            self.mg_add_train_station(tr, uid, None);
                        }
                    }
                    self.station_created = true;
                }
                if !self.un_is_active(tr, self.a_tok) {
                    self.flag = false;
                }
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    // 21 DefensePost
    fn defense_post(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => {
                self.mg_ready = true;
                Ok(JsVal::Undef)
            }
            2 => {
                if !self.un_is_active(tr, self.a_tok) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                if self.un_is_under_construction(tr, self.a_tok) {
                    return Ok(JsVal::Undef);
                }
                // `this.target` is never set (the ship-targeting block below is
                // commented out in TS), so it stays null and `shoot()` — which
                // lives only in the commented branch — is never called.
                if self.target.is_some() {
                    let t = self.target.unwrap();
                    if !self.un_is_active(tr, t) {
                        self.target = None;
                    }
                }
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    /// `DefensePostExecution.shoot` (TS:19-39) — unreachable from `tick` (the
    /// ship-targeting block is commented out, so `target` stays null); ported
    /// for completeness. `new ShellExecution(...)` is modelled as an
    /// `addExecution` construction record (tag 1), exactly like the
    /// `TrainStationExecution` constructions.
    #[allow(dead_code)]
    fn defense_post_shoot(&mut self, tr: &mut Vec<f64>) {
        let target = match self.target {
            Some(t) => t,
            None => return,
        };
        let rate = self.mg_defense_post_shell_attack_rate(tr);
        let ticks = self.mg_ticks(tr);
        if ticks - self.last_shell_attack > rate {
            self.last_shell_attack = ticks;
            let spawn = self.un_tile(tr, self.a_tok);
            let owner = self.un_owner(tr, self.a_tok);
            self.mg_add_execution_shell(tr, spawn, owner, self.a_tok, target);
            if !self.un_has_health(tr, target) {
                self.already_sent_shell.push(target);
                self.target = None;
            }
        }
    }

    // 22 Emoji
    fn emoji(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => {
                self.mg_ready = true;
                let seed = self.mg_ticks(tr);
                self.random = Some(PseudoRandom::new(seed));
                let id = self.id_a.clone();
                // `this.recipientID !== AllPlayers && !mg.hasPlayer(...)`.
                if id != ALL_PLAYERS && !self.mg_has_player(tr, &id) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                self.rec = if id == ALL_PLAYERS {
                    Rec::All
                } else {
                    match self.mg_player(tr, &id) {
                        Some(t) => Rec::P(t),
                        None => Rec::Null,
                    }
                };
                Ok(JsVal::Undef)
            }
            2 => {
                if let Some(emoji_string) = emoji_at(self.emoji) {
                    if self.pl_can_send_emoji(tr, self.a_tok, self.rec) {
                        self.pl_send_emoji(tr, self.a_tok, self.rec, emoji_string);
                        let mut rng = self
                            .random
                            .take()
                            .unwrap_or_else(|| PseudoRandom::new(0.0));
                        let r = self.respond_to_emoji(
                            tr,
                            &mut rng,
                            self.a_tok,
                            self.rec,
                            emoji_string,
                        );
                        self.random = Some(rng);
                        r?;
                    }
                }
                self.flag = false;
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    // 23 DonateTroops
    fn donate_troops_exec(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => {
                self.mg_ready = true;
                let seed = self.mg_ticks(tr);
                self.random = Some(PseudoRandom::new(seed));
                let id = self.id_a.clone();
                if !self.mg_has_player(tr, &id) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                let rt = self.mg_player(tr, &id);
                self.recipient = rt;
                if self.troops.is_none() {
                    let d = self.mg_default_donation_amount(tr, Rec::P(self.a_tok));
                    self.troops = Some(d);
                }
                let max_troops = self
                    .mg_max_troops(tr, rt.map(Rec::P).unwrap_or(Rec::Null));
                let cur = match rt {
                    Some(t) => self.pl_troops(tr, t),
                    None => {
                        return Err("Cannot read properties of null (reading 'troops')".into())
                    }
                };
                let max_donation = max_troops - cur;
                let t = self.troops.unwrap();
                self.troops = Some(js_min(t, max_donation));
                if self.troops.unwrap() <= 0.0 {
                    self.flag = false;
                }
                Ok(JsVal::Undef)
            }
            2 => {
                if self.troops.is_none() {
                    return Err("not initialized".into());
                }
                let min_troops = self.get_min_troops(tr)?;
                let rt = match self.recipient {
                    Some(t) => t,
                    None => {
                        return Err(
                            "Cannot read properties of null (reading 'canDonateTroops')".into()
                        )
                    }
                };
                let troops = self.troops.unwrap();
                if self.pl_can_donate_troops(tr, self.a_tok, Rec::P(rt))
                    && self.pl_donate_troops(tr, self.a_tok, Rec::P(rt), troops)
                {
                    if troops >= min_troops {
                        self.pl_update_relation(tr, rt, Some(self.a_tok), 50.0);
                    }
                    if self.pl_type(tr, rt) == PLAYER_TYPE_NATION
                        && self.pl_can_send_emoji(tr, rt, Rec::P(self.a_tok))
                    {
                        let arr = if troops >= min_troops {
                            emoji_ids("EMOJI_LOVE")
                        } else {
                            emoji_ids("EMOJI_DONATION_TOO_SMALL")
                        };
                        let sid = self.pl_id(tr, self.a_tok);
                        let mut rng = self
                            .random
                            .take()
                            .unwrap_or_else(|| PseudoRandom::new(0.0));
                        let e = rng.rand_element(&arr).copied().unwrap_or(f64::NAN);
                        self.random = Some(rng);
                        self.mg_add_execution_emoji(tr, rt, &sid, e);
                    }
                }
                self.flag = false;
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    /// `DonateTroopsExecution.getMinTroopsForRelationUpdate` (TS:91-123).
    fn get_min_troops(&mut self, tr: &mut Vec<f64>) -> Result<f64, String> {
        let difficulty = self.mg_difficulty(tr);
        let max = match self.recipient {
            Some(t) => self.mg_max_troops(tr, Rec::P(t)),
            None => self.mg_max_troops(tr, Rec::Null),
        };
        let mut rng = self
            .random
            .take()
            .unwrap_or_else(|| PseudoRandom::new(0.0));
        let v = match difficulty.as_str() {
            "Easy" => rng.next_int(max / 13.0, max / 11.0) as f64,
            "Medium" => rng.next_int(max / 11.0, max / 9.0) as f64,
            "Hard" => rng.next_int(max / 9.0, max / 7.0) as f64,
            "Impossible" => rng.next_int(max / 7.0, max / 5.0) as f64,
            other => {
                self.random = Some(rng);
                // `assertNever(x)` -> `throw new Error("Unexpected value: " + x)`.
                return Err(format!("Unexpected value: {other}"));
            }
        };
        self.random = Some(rng);
        Ok(v)
    }

    // 24 DonateGold
    fn donate_gold_exec(
        &mut self,
        kind: u8,
        ticks: f64,
        tr: &mut Vec<f64>,
    ) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => {
                self.mg_ready = true;
                let seed = self.mg_ticks(tr);
                self.random = Some(PseudoRandom::new(seed));
                let id = self.id_a.clone();
                if !self.mg_has_player(tr, &id) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                let rt = self.mg_player(tr, &id);
                self.recipient = rt;
                if self.gold.is_none() {
                    // `this.gold ??= this.sender.gold() / 3n` (i64 truncation).
                    let g = self.pl_gold(tr, self.a_tok);
                    self.gold = Some(g / 3);
                }
                Ok(JsVal::Undef)
            }
            2 => {
                if self.gold.is_none() {
                    return Err("not initialized".into());
                }
                let g = self.gold.unwrap();
                let rt = match self.recipient {
                    Some(t) => t,
                    None => {
                        return Err(
                            "Cannot read properties of null (reading 'canDonateGold')".into()
                        )
                    }
                };
                if self.pl_can_donate_gold(tr, self.a_tok, Rec::P(rt))
                    && self.pl_donate_gold(tr, self.a_tok, Rec::P(rt), g)
                {
                    let relation_update = self.calculate_relation_update(tr, g, ticks)?;
                    if relation_update > 0.0 {
                        self.pl_update_relation(tr, rt, Some(self.a_tok), relation_update);
                    }
                    if self.pl_type(tr, rt) == PLAYER_TYPE_NATION
                        && self.pl_can_send_emoji(tr, rt, Rec::P(self.a_tok))
                    {
                        let name = if relation_update >= 50.0 {
                            "EMOJI_LOVE"
                        } else if relation_update > 0.0 {
                            "EMOJI_DONATION_OK"
                        } else {
                            "EMOJI_DONATION_TOO_SMALL"
                        };
                        let arr = emoji_ids(name);
                        let sid = self.pl_id(tr, self.a_tok);
                        let mut rng = self
                            .random
                            .take()
                            .unwrap_or_else(|| PseudoRandom::new(0.0));
                        let e = rng.rand_element(&arr).copied().unwrap_or(f64::NAN);
                        self.random = Some(rng);
                        self.mg_add_execution_emoji(tr, rt, &sid, e);
                    }
                } else {
                    let _ = self.pl_name(tr, self.a_tok);
                    let _ = self.pl_name(tr, rt);
                }
                self.flag = false;
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    /// `DonateGoldExecution.getGoldChunkSize` (TS:93-107).
    fn get_gold_chunk_size(&mut self, tr: &mut Vec<f64>) -> Result<f64, String> {
        let difficulty = self.mg_difficulty(tr);
        match difficulty.as_str() {
            "Easy" => Ok(2500.0),
            "Medium" => Ok(5000.0),
            "Hard" => Ok(12500.0),
            "Impossible" => Ok(25000.0),
            other => Err(format!("Unexpected value: {other}")),
        }
    }

    /// `DonateGoldExecution.calculateRelationUpdate` (TS:109-124) — the bigint
    /// division `goldSent / adjustedChunkSize` truncates toward zero, mirrored
    /// by `i64 / i64`.
    fn calculate_relation_update(
        &mut self,
        tr: &mut Vec<f64>,
        gold_sent: i64,
        ticks: f64,
    ) -> Result<f64, String> {
        let chunk_size = self.get_gold_chunk_size(tr)?;
        let nsp = self.mg_num_spawn_phase_turns(tr);
        let multiplier = ticks / (3000.0 + nsp);
        let adjusted = js_round(chunk_size + chunk_size * multiplier) as i64;
        if adjusted == 0 {
            // `BigInt(x) / 0n` throws a RangeError in JS (not exercised).
            return Err("RangeError: Division by zero".into());
        }
        let chunks = (gold_sent / adjusted) as f64;
        let relation_update = chunks * 5.0;
        if relation_update > 100.0 {
            Ok(100.0)
        } else {
            Ok(relation_update)
        }
    }

    /// `respondToEmoji` (`nation/NationEmojiBehavior.ts`:279-327) — the three
    /// blocks are independent `if`s; `sender` is the original requestor and
    /// `recipient` the player that received the emoji.
    fn respond_to_emoji(
        &mut self,
        tr: &mut Vec<f64>,
        rng: &mut PseudoRandom,
        sender_tok: usize,
        rec: Rec,
        emoji_string: &str,
    ) -> Result<(), String> {
        let rt = match rec {
            Rec::All => return Ok(()),
            Rec::Unset => {
                return Err("Cannot read properties of undefined (reading 'type')".into())
            }
            Rec::Null => return Err("Cannot read properties of null (reading 'type')".into()),
            Rec::P(t) => t,
        };
        if self.pl_type(tr, rt) != PLAYER_TYPE_NATION {
            return Ok(());
        }
        if !self.pl_can_send_emoji(tr, rt, Rec::P(sender_tok)) {
            return Ok(());
        }

        if emoji_string == "\u{1F595}" {
            self.pl_update_relation(tr, rt, Some(sender_tok), -100.0);
            let arr = emoji_ids("EMOJI_GOT_INSULTED");
            let e = rng.rand_element(&arr).copied().unwrap_or(f64::NAN);
            let sid = self.pl_id(tr, sender_tok);
            self.mg_add_execution_emoji(tr, rt, &sid, e);
        }

        if emoji_string == "\u{1F921}" {
            self.pl_update_relation(tr, rt, Some(sender_tok), -10.0);
            let arr = emoji_ids("EMOJI_CONFUSED");
            let e = rng.rand_element(&arr).copied().unwrap_or(f64::NAN);
            let sid = self.pl_id(tr, sender_tok);
            self.mg_add_execution_emoji(tr, rt, &sid, e);
        }

        const LOVE_LIST: [&str; 5] = [
            "\u{1F54A}\u{FE0F}",
            "\u{1F3F3}\u{FE0F}",
            "\u{2764}\u{FE0F}",
            "\u{1F970}",
            "\u{1F44F}",
        ];
        if is_one_of(emoji_string, &LOVE_LIST) {
            if self.mg_difficulty(tr) == "Easy" {
                self.pl_update_relation(tr, rt, Some(sender_tok), 15.0);
            }
            let sid = self.pl_id(tr, sender_tok);
            let rel = self.pl_relation(tr, sender_tok, Some(rt));
            let arr = if rel >= 2.0 {
                emoji_ids("EMOJI_LOVE")
            } else {
                emoji_ids("EMOJI_CONFUSED")
            };
            let e = rng.rand_element(&arr).copied().unwrap_or(f64::NAN);
            self.mg_add_execution_emoji(tr, rt, &sid, e);
        }
        Ok(())
    }

    // 25 WinCheck
    fn win_check(&mut self, kind: u8, ticks: f64, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => {
                self.mg_ready = true;
                Ok(JsVal::Undef)
            }
            2 => {
                if ticks % 10.0 != 0.0 {
                    return Ok(JsVal::Undef);
                }
                // TS: `if (this.mg === null) throw new Error("Not initialized")`.
                if !self.mg_ready {
                    return Err("Not initialized".into());
                }
                if self.check_ranked_2v2_cancelled(tr)? {
                    return Ok(JsVal::Undef);
                }
                if self.mg_game_mode(tr) == GAME_MODE_FFA {
                    self.check_winner_ffa(tr)?;
                } else {
                    self.check_winner_team(tr)?;
                }
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    /// `WinCheckExecution.checkRanked2v2Cancelled`.
    fn check_ranked_2v2_cancelled(&mut self, tr: &mut Vec<f64>) -> Result<bool, String> {
        if self.checked_ranked_spawns {
            return Ok(false);
        }
        self.checked_ranked_spawns = true;
        if self.mg_ranked_type(tr) != RANKED_TWO_V_TWO {
            return Ok(false);
        }
        let players = self.mg_all_players(tr);
        let mut spawned = 0.0;
        for p in players {
            if self.pl_type(tr, p) == PLAYER_TYPE_HUMAN && self.pl_has_spawned(tr, p) {
                spawned += 1.0;
            }
        }
        let expected = self.mg_max_players(tr).unwrap_or(0.0);
        if spawned >= expected {
            return Ok(false);
        }
        let s = self.mg_stats_stats(tr);
        self.mg_set_winner(tr, WinArg::Null, &s);
        self.flag = false;
        Ok(true)
    }

    /// `WinCheckExecution.checkWinnerFFA`.
    fn check_winner_ffa(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        let players = self.mg_players(tr);
        let sorted = self.sort_players_by_tiles(tr, players);
        if sorted.is_empty() {
            return Ok(());
        }
        if self.mg_ranked_type(tr) == RANKED_ONE_V_ONE {
            let mut humans = Vec::new();
            for p in &sorted {
                if self.pl_type(tr, *p) == PLAYER_TYPE_HUMAN && !self.pl_is_disconnected(tr, *p) {
                    humans.push(*p);
                }
            }
            if humans.len() == 1 {
                let s = self.mg_stats_stats(tr);
                self.mg_set_winner(tr, WinArg::Player(humans[0]), &s);
                let _ = self.pl_name(tr, humans[0]);
                self.flag = false;
                return Ok(());
            }
        }
        let max = sorted[0];
        let tiles = self.pl_num_tiles_owned(tr, max);
        if self.has_won(tr, tiles)? {
            let s = self.mg_stats_stats(tr);
            self.mg_set_winner(tr, WinArg::Player(max), &s);
            let _ = self.pl_name(tr, max);
            self.flag = false;
        }
        Ok(())
    }

    /// `WinCheckExecution.checkWinnerTeam`.
    fn check_winner_team(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        if self.mg_ranked_type(tr) == RANKED_TWO_V_TWO {
            let players = self.mg_players(tr);
            let mut teams: Vec<String> = Vec::new();
            for p in players {
                if self.pl_type(tr, p) != PLAYER_TYPE_HUMAN || self.pl_is_disconnected(tr, p) {
                    continue;
                }
                let team = self.pl_team(tr, p);
                if !matches!(team, JsVal::Null) {
                    let s = as_str(&team);
                    if !teams.contains(&s) {
                        teams.push(s);
                    }
                }
            }
            if teams.len() == 1 {
                let s = self.mg_stats_stats(tr);
                self.mg_set_winner(tr, WinArg::Team(teams[0].clone()), &s);
                self.flag = false;
                return Ok(());
            }
        }
        let players = self.mg_players(tr);
        let mut team_to_tiles: Vec<(String, f64)> = Vec::new();
        for p in players {
            let team = self.pl_team(tr, p);
            if matches!(team, JsVal::Null) {
                continue;
            }
            let key = as_str(&team);
            let tiles = self.pl_num_tiles_owned(tr, p);
            match team_to_tiles.iter_mut().find(|(k, _)| *k == key) {
                Some(e) => e.1 += tiles,
                None => team_to_tiles.push((key, tiles)),
            }
        }
        // `Array.from(teamToTiles.entries()).sort((a, b) => b[1] - a[1])` — the
        // comparator only reads plain numbers, so the trace is order-independent
        // and sorting by tile count descending picks the same maximum.
        team_to_tiles.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        if team_to_tiles.is_empty() {
            return Ok(());
        }
        let (team, tiles) = team_to_tiles[0].clone();
        if self.has_won(tr, tiles)? {
            // `ColoredTeams.Bot` never wins.
            if team == COLORED_TEAMS_BOT {
                return Ok(());
            }
            let s = self.mg_stats_stats(tr);
            self.mg_set_winner(tr, WinArg::Team(team), &s);
            self.flag = false;
        }
        Ok(())
    }

    /// `WinCheckExecution.hasWon`.
    fn has_won(&mut self, tr: &mut Vec<f64>, tiles_owned: f64) -> Result<bool, String> {
        let time_elapsed = self.mg_elapsed_game_seconds(tr);
        // `maxTimerValue !== undefined && maxTimerValue !== null`.
        let max_timer = self.mg_max_timer_value(tr);
        if !matches!(max_timer, JsVal::Undef | JsVal::Null | JsVal::Absent) {
            let mt = as_num(&max_timer);
            if time_elapsed >= mt * 60.0 {
                return Ok(true);
            }
        }
        if time_elapsed >= WIN_HARD_TIME_LIMIT_SECONDS {
            return Ok(true);
        }
        let num_tiles_without_fallout =
            self.mg_num_land_tiles(tr) - self.mg_num_tiles_with_fallout(tr);
        let pct = self.mg_pct_tiles_to_win(tr, time_elapsed);
        Ok(tiles_owned * 100.0 > num_tiles_without_fallout * pct)
    }

    /// V8's binary insertion sort (`Array.prototype.sort` for arrays up to
    /// length 7). The comparator `(a, b) => b.numTilesOwned() - a.numTilesOwned()`
    /// is invoked as `Compare(element, mid)`, so `mid`'s tile count is read
    /// before the element's. Scenario lists are kept to a single element, for
    /// which sort makes no comparator calls.
    fn sort_players_by_tiles(&mut self, tr: &mut Vec<f64>, mut arr: Vec<usize>) -> Vec<usize> {
        let n = arr.len();
        if n <= 1 {
            return arr;
        }
        for i in 1..n {
            let element = arr[i];
            let (mut lo, mut hi) = (0usize, i);
            while lo < hi {
                let mid = (lo + hi) / 2;
                let mt = self.pl_num_tiles_owned(tr, arr[mid]);
                let et = self.pl_num_tiles_owned(tr, element);
                if mt - et < 0.0 {
                    hi = mid;
                } else {
                    lo = mid + 1;
                }
            }
            if lo < i {
                arr.remove(i);
                arr.insert(lo, element);
            }
        }
        arr
    }

    // 26 Shell
    fn shell(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => {
                self.mg_ready = true;
                let seed = self.mg_ticks(tr);
                self.random = Some(PseudoRandom::new(seed));
                Ok(JsVal::Undef)
            }
            2 => {
                if self.shell_tok.is_none() {
                    let params = JsVal::Obj(vec![]);
                    self.shell_tok =
                        self.pl_build_unit(tr, self.a_tok, UNIT_SHELL, self.spawn, &params);
                }
                let sh = match self.shell_tok {
                    Some(t) => t,
                    None => {
                        return Err(
                            "Cannot read properties of undefined (reading 'isActive')".into()
                        )
                    }
                };
                if !self.un_is_active(tr, sh) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                let target = self.target.unwrap();
                let owner_unit = self.owner_unit.unwrap();
                // `!target.isActive() || target.owner() === shell.owner() ||
                //  (destroyAtTick !== -1 && mg.ticks() >= destroyAtTick)`.
                let mut bail = !self.un_is_active(tr, target);
                if !bail {
                    let a = self.un_owner(tr, target);
                    let b = self.un_owner(tr, sh);
                    bail = a == b;
                }
                if !bail && self.destroy_at_tick != -1.0 {
                    bail = self.mg_ticks(tr) >= self.destroy_at_tick;
                }
                if bail {
                    self.un_delete(tr, sh, false);
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                if self.destroy_at_tick == -1.0 && !self.un_is_active(tr, owner_unit) {
                    let t = self.mg_ticks(tr);
                    let life = self.mg_shell_lifetime(tr);
                    self.destroy_at_tick = t + life;
                }
                let mut i = 0.0;
                while i < 3.0 {
                    let from = self.un_tile(tr, sh);
                    let to = self.un_tile(tr, target);
                    let result = self.mg_path_next(tr, from, to);
                    let status = val_field(&result, "status").map(as_num).unwrap_or(f64::NAN);
                    if status == 2.0 {
                        self.flag = false;
                        let target_type = self.un_type(tr, target);
                        let target_was_active = self.un_is_active(tr, target);
                        let eff = self.effect_on_target(tr, owner_unit);
                        self.un_modify_health(tr, target, -eff, Some(self.a_tok));
                        let mut record = target_was_active && !self.un_is_active(tr, target);
                        if record {
                            record = self.un_is_active(tr, owner_unit);
                        }
                        if record {
                            record = self.un_type(tr, owner_unit) == UNIT_WARSHIP;
                        }
                        if record {
                            self.un_record_kill(tr, owner_unit, &target_type);
                        }
                        self.un_set_reached_target(tr, sh);
                        self.un_delete(tr, sh, false);
                        return Ok(JsVal::Undef);
                    } else if status == 0.0 {
                        let node = val_field(&result, "node").cloned().unwrap_or(JsVal::Undef);
                        self.un_move(tr, sh, &node);
                    }
                    i += 1.0;
                }
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    /// `ShellExecution.effectOnTarget`.
    fn effect_on_target(&mut self, tr: &mut Vec<f64>, owner_unit: usize) -> f64 {
        let info = self.mg_config_unit_info(tr, UNIT_SHELL);
        let base_damage = match val_field(&info, "damage") {
            Some(JsVal::Num(n)) => *n,
            _ => 250.0,
        };
        let roll = {
            let mut rng = self.random.take().unwrap_or_else(|| PseudoRandom::new(0.0));
            let r = rng.next_int(1.0, 6.0) as f64;
            self.random = Some(rng);
            r
        };
        let mut damage_multiplier = (roll - 1.0) * 25.0 + 200.0;
        let veterancy = self.un_veterancy(tr, owner_unit);
        if veterancy > 0.0 {
            let bonus = self.mg_warship_vet_shell_dmg(tr);
            damage_multiplier =
                ((damage_multiplier * (100.0 + veterancy * bonus)) / 100.0).floor();
        }
        js_round((base_damage / 250.0) * damage_multiplier)
    }

    // 27 SAMMissile
    fn sam_missile(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => {
                self.mg_ready = true;
                self.sam_speed = self.mg_default_sam_speed(tr);
                self.sam_missile_tick(tr)
            }
            2 => self.sam_missile_tick(tr),
            _ => unreachable!(),
        }
    }

    fn sam_missile_tick(&mut self, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        if self.sam_tok.is_none() {
            let tgt = self.target.unwrap();
            let params = JsVal::Obj(vec![("targetUnit".to_string(), JsVal::Num(tgt as f64))]);
            self.sam_tok =
                self.pl_build_unit(tr, self.a_tok, UNIT_SAM_MISSILE, self.spawn, &params);
        }
        let sm = match self.sam_tok {
            Some(t) => t,
            None => {
                return Err("Cannot read properties of undefined (reading 'isActive')".into())
            }
        };
        if !self.un_is_active(tr, sm) {
            self.flag = false;
            return Ok(JsVal::Undef);
        }
        let target = self.target.unwrap();
        let owner_unit = self.owner_unit.unwrap();
        // `!target.isActive() || !ownerUnit.isActive() ||
        //  target.owner() === SAMMissile.owner() ||
        //  !nukesWhitelist.includes(target.type())`.
        let mut not_in = !self.un_is_active(tr, target);
        if !not_in {
            not_in = !self.un_is_active(tr, owner_unit);
        }
        if !not_in {
            let a = self.un_owner(tr, target);
            let b = self.un_owner(tr, sm);
            not_in = a == b;
        }
        if !not_in {
            let ty = self.un_type(tr, target);
            not_in = !NUKES_WHITELIST.contains(&ty.as_str());
        }
        if not_in {
            if self.un_is_active(tr, target) {
                self.un_set_targeted_by_sam(tr, target, false);
            }
            self.un_delete(tr, sm, false);
            self.flag = false;
            return Ok(JsVal::Undef);
        }
        let speed = self.sam_speed;
        let mut i = 0.0;
        while i < speed {
            let from = self.un_tile(tr, sm);
            let result = self.mg_path_next(tr, from, self.target_tile);
            let status = val_field(&result, "status").map(as_num).unwrap_or(f64::NAN);
            if status == 2.0 {
                let oid = self.pl_id(tr, self.a_tok);
                let ttype = self.un_type(tr, target);
                let key = intercepted_key(&ttype);
                self.mg_display_message(
                    tr,
                    &[
                        JsVal::Str("events_display.missile_intercepted".into()),
                        JsVal::Num(MSG_SAM_HIT),
                        JsVal::Str(oid),
                        JsVal::Undef,
                        JsVal::Obj(vec![("unit".to_string(), JsVal::Str(key))]),
                    ],
                );
                self.flag = false;
                self.un_delete(tr, target, true);
                self.un_delete(tr, sm, false);
                // `this.mg.stats().bombIntercept(...)` — `stats()` itself is not
                // traced; only the `bombIntercept` call (code 524) is.
                let ttype2 = self.un_type(tr, target);
                self.mg_bomb_intercept(tr, self.a_tok, &ttype2, 1.0);
                return Ok(JsVal::Undef);
            } else if status == 0.0 {
                let node = val_field(&result, "node").cloned().unwrap_or(JsVal::Undef);
                self.un_move(tr, sm, &node);
            }
            i += 1.0;
        }
        Ok(JsVal::Undef)
    }

    // 28 Port
    fn port(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => {
                self.mg_ready = true;
                let t1 = self.mg_ticks(tr);
                self.random = Some(PseudoRandom::new(t1));
                let t2 = self.mg_ticks(tr);
                self.check_offset = t2 % 10.0;
                Ok(JsVal::Undef)
            }
            2 => {
                // TS: `if (this.mg === null || this.random === null ||
                // this.checkOffset === null) throw new Error("Not initialized")`.
                // `mg`/`random`/`checkOffset` are declared without an initialiser
                // (`private mg: Game;`), so before `init` they are `undefined`
                // — never `null` — and this guard is dead code.
                if !self.un_is_active(tr, self.a_tok) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                if self.un_is_under_construction(tr, self.a_tok) {
                    return Ok(JsVal::Undef);
                }
                if !self.un_has_train_station(tr, self.a_tok) {
                    self.create_station(tr);
                }
                let t = self.mg_ticks(tr);
                if (t + self.check_offset) % 10.0 != 0.0 {
                    return Ok(JsVal::Undef);
                }
                if !self.should_spawn_trade_ship(tr) {
                    return Ok(JsVal::Undef);
                }
                let ports = self.trading_ports(tr);
                if ports.is_empty() {
                    return Ok(JsVal::Undef);
                }
                let chosen = {
                    let mut rng = self.random.take().unwrap_or_else(|| PseudoRandom::new(0.0));
                    let c = rng.rand_element(&ports).copied().unwrap_or(usize::MAX);
                    self.random = Some(rng);
                    c
                };
                let owner = self.un_owner(tr, self.a_tok);
                self.mg_add_trade_ship(tr, owner, self.a_tok, chosen);
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    /// `PortExecution.createStation`.
    fn create_station(&mut self, tr: &mut Vec<f64>) {
        let tile = self.un_tile(tr, self.a_tok);
        let range = self.mg_train_station_max_range(tr);
        if self.mg_has_unit_nearby(tr, tile, range, UNIT_FACTORY) {
            self.mg_add_train_station(tr, self.a_tok, None);
        }
    }

    /// `PortExecution.shouldSpawnTradeShip`.
    fn should_spawn_trade_ship(&mut self, tr: &mut Vec<f64>) -> bool {
        let num = self.mg_unit_count(tr, UNIT_TRADE_SHIP);
        let mut i = 0.0;
        // TS: `for (let i = 0; i < this.port!.level(); i++)` — the loop
        // condition re-reads `level()` on every iteration (and once more when
        // it fails), so the facade call must happen inside the condition.
        while i < self.un_level(tr, self.a_tok) {
            let rate = self.mg_trade_ship_spawn_rate(tr, self.trade_ship_rejections, num);
            let chance = {
                let mut rng = self.random.take().unwrap_or_else(|| PseudoRandom::new(0.0));
                let c = rng.chance(rate);
                self.random = Some(rng);
                c
            };
            if chance {
                self.trade_ship_rejections = 0.0;
                return true;
            }
            self.trade_ship_rejections += 1.0;
            i += 1.0;
        }
        false
    }

    /// `PortExecution.tradingPorts`.
    fn trading_ports(&mut self, tr: &mut Vec<f64>) -> Vec<usize> {
        let mut comps: Vec<f64> = Vec::new();
        let port_tile = self.un_tile(tr, self.a_tok);
        let neighbors = self.mg_neighbors(tr, port_tile);
        for nb in neighbors {
            if !self.mg_is_water(tr, nb) {
                continue;
            }
            let comp = self.mg_get_water_component(tr, nb);
            // `comp !== null` (SameValueZero set dedup).
            if !matches!(comp, JsVal::Null) {
                let c = as_num(&comp);
                if !comps.iter().any(|x| svz(*x, c)) {
                    comps.push(c);
                }
            }
        }
        let players = self.mg_players(tr);
        // `.filter((p) => p !== port.owner() && p.canTrade(port.owner()))`.
        let mut passing: Vec<usize> = Vec::new();
        for p in players {
            let o1 = self.un_owner(tr, self.a_tok);
            if Some(p) == o1 {
                continue;
            }
            let o2 = self.un_owner(tr, self.a_tok);
            if self.pl_can_trade(tr, p, o2) {
                passing.push(p);
            }
        }
        // `.flatMap((p) => p.units(UnitType.Port))`.
        let types = vec![UNIT_PORT.to_string()];
        let mut ports: Vec<usize> = Vec::new();
        for p in passing {
            ports.extend(self.pl_units(tr, p, &types));
        }
        // `.filter((p) => sourceComponents.some(c => hasWaterComponent(p.tile(), c)))`.
        let mut filtered: Vec<usize> = Vec::new();
        for p in ports {
            let mut keep = false;
            for c in &comps {
                let tile = self.un_tile(tr, p);
                if self.mg_has_water_component(tr, tile, &JsVal::Num(*c)) {
                    keep = true;
                    break;
                }
            }
            if keep {
                filtered.push(p);
            }
        }
        let sorted = self.sort_ports_by_dist(tr, filtered);
        let n = sorted.len();
        let mut weighted: Vec<usize> = Vec::new();
        for (i, other) in sorted.iter().enumerate() {
            let level = self.un_level(tr, *other);
            // `new Array(otherPort.level()).fill(otherPort)`.
            let expanded: Vec<usize> = vec![*other; level as usize];
            weighted.extend_from_slice(&expanded);
            let too_close = {
                let t1 = self.un_tile(tr, self.a_tok);
                let t2 = self.un_tile(tr, *other);
                let d = self.mg_manhattan_dist(tr, t1, t2);
                d < self.mg_trade_ship_short_range_debuff(tr)
            };
            let close_bonus = (i as f64) < self.mg_proximity_bonus_ports_nb(tr, n as f64);
            if !too_close && close_bonus {
                weighted.extend_from_slice(&expanded);
            }
            if !too_close {
                let o1 = self.un_owner(tr, self.a_tok);
                let o2 = self.un_owner(tr, *other);
                if self.pl_is_friendly(tr, o1.unwrap_or(usize::MAX), o2) {
                    weighted.extend_from_slice(&expanded);
                }
            }
        }
        weighted
    }

    /// V8 binary insertion sort for
    /// `.sort((p1, p2) => manhattanDist(port.tile(), p1.tile()) - manhattanDist(port.tile(), p2.tile()))`,
    /// with `element` as the first comparator argument.
    fn sort_ports_by_dist(&mut self, tr: &mut Vec<f64>, mut arr: Vec<usize>) -> Vec<usize> {
        let n = arr.len();
        if n <= 1 {
            return arr;
        }
        for i in 1..n {
            let element = arr[i];
            let (mut lo, mut hi) = (0usize, i);
            while lo < hi {
                let mid = (lo + hi) / 2;
                if self.port_dist_cmp(tr, element, arr[mid]) < 0.0 {
                    hi = mid;
                } else {
                    lo = mid + 1;
                }
            }
            if lo < i {
                arr.remove(i);
                arr.insert(lo, element);
            }
        }
        arr
    }

    fn port_dist_cmp(&mut self, tr: &mut Vec<f64>, p1: usize, p2: usize) -> f64 {
        let a1 = self.un_tile(tr, self.a_tok);
        let b1 = self.un_tile(tr, p1);
        let d1 = self.mg_manhattan_dist(tr, a1, b1);
        let a2 = self.un_tile(tr, self.a_tok);
        let b2 = self.un_tile(tr, p2);
        let d2 = self.mg_manhattan_dist(tr, a2, b2);
        d1 - d2
    }

    // 29 Construction
    fn construction(&mut self, kind: u8, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => {
                self.mg_ready = true;
                let ty = self.construction_type.clone();
                if self.mg_is_unit_disabled(tr, &ty) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                if !self.mg_is_valid_ref(tr, self.spawn) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                Ok(JsVal::Undef)
            }
            2 => {
                if !self.mg_ready {
                    return Err("Cannot read properties of undefined (reading 'unitInfo')".into());
                }
                if self.structure.is_none() {
                    let ty = self.construction_type.clone();
                    let info = self.mg_unit_info(tr, &ty);
                    if !is_structure_type(&ty) {
                        // Defer to the specialized execution.
                        self.complete_construction(tr);
                        self.flag = false;
                        return Ok(JsVal::Undef);
                    }
                    let spawn = self.pl_can_build(tr, self.a_tok, &ty, self.spawn);
                    if matches!(spawn, JsVal::Bool(false)) {
                        self.flag = false;
                        return Ok(JsVal::Undef);
                    }
                    let spawn_tile = as_num(&spawn);
                    let params = JsVal::Obj(vec![]);
                    let built = self.pl_build_unit(tr, self.a_tok, &ty, spawn_tile, &params);
                    self.structure = built;
                    let duration = match val_field(&info, "constructionDuration") {
                        Some(JsVal::Num(n)) => *n,
                        _ => 0.0,
                    };
                    if duration > 0.0 {
                        self.un_set_under_construction(tr, built.unwrap_or(usize::MAX), true);
                        self.ticks_until_complete = duration;
                        return Ok(JsVal::Undef);
                    }
                    self.complete_construction(tr);
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                let st = self.structure.unwrap();
                if !self.un_is_active(tr, st) {
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                let o1 = self.un_owner(tr, st);
                if Some(self.a_tok) != o1 {
                    self.a_tok = self.un_owner(tr, st).unwrap_or(usize::MAX);
                }
                if self.ticks_until_complete == 0.0 {
                    self.a_tok = self.un_owner(tr, st).unwrap_or(usize::MAX);
                    self.complete_construction(tr);
                    self.flag = false;
                    return Ok(JsVal::Undef);
                }
                self.ticks_until_complete -= 1.0;
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    /// `ConstructionExecution.completeConstruction`.
    fn complete_construction(&mut self, tr: &mut Vec<f64>) {
        if let Some(st) = self.structure {
            self.un_set_under_construction(tr, st, false);
        }
        let player = self.a_tok;
        let ty = self.construction_type.clone();
        match ty.as_str() {
            "Atom Bomb" | "Hydrogen Bomb" => {
                let count = self.el_amount.unwrap_or(1.0);
                let mut i = 0.0;
                while i < count {
                    self.mg_add_nuke(
                        tr,
                        &ty,
                        player,
                        self.spawn,
                        None,
                        -1.0,
                        0.0,
                        self.rocket_direction_up,
                    );
                    i += 1.0;
                }
            }
            "MIRV" => self.mg_add_mirv(tr, player, self.spawn),
            "Warship" => self.mg_add_warship(tr, player, self.spawn),
            "Port" => self.mg_add_unit_exec(tr, TAG_PORT_EXEC, self.structure.unwrap()),
            "Missile Silo" => self.mg_add_unit_exec(tr, TAG_MISSILE_SILO, self.structure.unwrap()),
            "Defense Post" => {
                self.mg_add_unit_exec(tr, TAG_DEFENSE_POST, self.structure.unwrap())
            }
            "SAM Launcher" => {
                let st = self.structure.unwrap();
                // `new SAMLauncherExecution(player, null, structure)` runs the
                // ctor side effect `sam.tile()` (code 344) before the record.
                let _ = self.un_tile(tr, st);
                self.mg_add_sam_launcher(tr, player, JsVal::Null, st);
            }
            "City" => self.mg_add_unit_exec(tr, TAG_CITY, self.structure.unwrap()),
            "Factory" => self.mg_add_unit_exec(tr, TAG_FACTORY, self.structure.unwrap()),
            _ => {}
        }
    }

    // ---- G5e shared helpers -------------------------------------------------

    /// JS `Set.has` over a `TileRef[]`-like (SameValueZero on numbers).
    fn tile_has(tiles: &[f64], t: f64) -> bool {
        tiles.iter().any(|x| svz(*x, t))
    }

    /// JS truthiness of a `js_json` value (`0`, `NaN`, `""`, `null`,
    /// `undefined` are falsy; every array/object is truthy).
    fn js_truthy(v: &JsVal) -> bool {
        match v {
            JsVal::Undef | JsVal::Null | JsVal::Absent => false,
            JsVal::Bool(b) => *b,
            JsVal::Num(n) => *n != 0.0 && !n.is_nan(),
            JsVal::Str(s) => !s.is_empty(),
            _ => true,
        }
    }

    /// `SpawnArea` (`{x, y, width, height}`) out of a `js_json` value; `None`
    /// for anything the TS `if (area)` truthiness test would reject.
    fn area_from_val(v: &JsVal) -> Option<(f64, f64, f64, f64)> {
        if !matches!(v, JsVal::Obj(_)) {
            return None;
        }
        let x = val_field(v, "x").map(as_num)?;
        let y = val_field(v, "y").map(as_num)?;
        let w = val_field(v, "width").map(as_num)?;
        let h = val_field(v, "height").map(as_num)?;
        Some((x, y, w, h))
    }

    // ---- 30 DoomsdayClockExecution -----------------------------------------

    fn doomsday_clock_exec(
        &mut self,
        kind: u8,
        ticks: f64,
        tr: &mut Vec<f64>,
    ) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => {
                self.mg_ready = true;
                Ok(JsVal::Undef)
            }
            2 => self.doomsday_clock_body(ticks, tr),
            _ => unreachable!(),
        }
    }

    fn doomsday_clock_body(&mut self, ticks: f64, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        if ticks % 10.0 != 0.0 {
            return Ok(JsVal::Undef);
        }
        if !self.mg_ready {
            return Err("Not initialized".into());
        }
        let cfg = self.mg_doomsday_clock_config(tr);
        if !cfg_bool(&cfg, "enabled") {
            return Ok(JsVal::Undef);
        }

        let elapsed = self.mg_elapsed_game_seconds(tr);
        let players = self.mg_players(tr);
        let mut contenders: Vec<usize> = Vec::new();
        for p in players {
            if self.pl_type(tr, p) != PLAYER_TYPE_BOT {
                contenders.push(p);
            }
        }

        let ffa = self.mg_game_mode(tr) == GAME_MODE_FFA;
        let sides = self.ddc_sides(&contenders, ffa, tr);

        let winner = self.mg_get_winner(tr);
        if !matches!(winner, JsVal::Null) || sides.len() < 2 {
            for p in &contenders {
                self.pl_clear_doomsday_clock(tr, *p);
            }
            self.flag = false;
            return Ok(JsVal::Undef);
        }

        let land = self.mg_num_land_tiles(tr) - self.mg_num_tiles_with_fallout(tr);
        let speed = speed_from_str(&cfg_str(&cfg, "speed"));
        let profile = Profile {
            speed,
            team: !ffa,
        };
        let required = crate::doomsday_clock::required_tiles(&profile, land, elapsed);

        let mut side_tiles: Vec<f64> = Vec::new();
        for members in &sides {
            let mut sum = 0.0;
            for m in members {
                sum += self.pl_num_tiles_owned(tr, *m);
            }
            side_tiles.push(sum);
        }
        let mut leader_idx = 0usize;
        for i in 1..side_tiles.len() {
            if side_tiles[i] > side_tiles[leader_idx] {
                leader_idx = i;
            }
        }

        let warn_seconds = cfg_num(&cfg, "warnSeconds");
        for i in 0..sides.len() {
            if i != leader_idx && side_tiles[i] < required {
                for m in &sides[i] {
                    let m = *m;
                    self.pl_enter_doomsday_clock(tr, m);
                    let seconds_under =
                        (self.pl_doomsday_clock_ticks(tr, m) / 10.0).floor();
                    if seconds_under >= warn_seconds {
                        let seconds_past_warn = seconds_under - warn_seconds;
                        let max_troops = self.mg_max_troops(tr, Rec::P(m));
                        let floor_cfg = FloorConfig {
                            drain_floor_percent: cfg_num(&cfg, "drainFloorPercent"),
                            floor_start_percent: cfg_num(&cfg, "floorStartPercent"),
                            floor_decay_seconds: cfg_num(&cfg, "floorDecaySeconds"),
                        };
                        let troop_floor = crate::doomsday_clock::troop_floor(
                            max_troops,
                            seconds_past_warn,
                            &floor_cfg,
                        );
                        let drain_cfg = DrainConfig {
                            drain_start_percent: cfg_num(&cfg, "drainStartPercent"),
                            drain_max_percent: cfg_num(&cfg, "drainMaxPercent"),
                            drain_ramp_seconds: cfg_num(&cfg, "drainRampSeconds"),
                        };
                        let chunk = crate::doomsday_clock::drain(
                            max_troops,
                            seconds_past_warn,
                            &drain_cfg,
                            1.0,
                        );
                        let troops = self.pl_troops(tr, m);
                        self.pl_remove_troops(
                            tr,
                            m,
                            js_min(chunk, js_max(0.0, troops - troop_floor)),
                        );
                        if cfg_num(&cfg, "rotDeathSeconds") > 0.0
                            && seconds_past_warn >= cfg_num(&cfg, "floorDecaySeconds")
                            && self.pl_troops(tr, m) <= troop_floor
                        {
                            self.ddc_rot(tr, m, seconds_under, &cfg);
                        }

                        let warship_cfg = DrainConfig {
                            drain_start_percent: cfg_num(&cfg, "warshipDrainStartPercent"),
                            drain_max_percent: cfg_num(&cfg, "warshipDrainMaxPercent"),
                            drain_ramp_seconds: cfg_num(&cfg, "drainRampSeconds"),
                        };
                        let warships = self.pl_units(tr, m, &["Warship".to_string()]);
                        for uid in warships {
                            let mh = self.un_max_health(tr, uid);
                            let ship_floor =
                                ((mh * cfg_num(&cfg, "drainFloorPercent")) / 100.0).floor();
                            let removable =
                                js_max(0.0, self.un_health(tr, uid) - ship_floor);
                            if removable <= 0.0 {
                                continue;
                            }
                            let mh2 = self.un_max_health(tr, uid);
                            let dmg = crate::doomsday_clock::drain(
                                mh2,
                                seconds_past_warn,
                                &warship_cfg,
                                cfg_num(&cfg, "warshipDrainCurveExponent"),
                            );
                            self.un_modify_health(tr, uid, -js_min(dmg, removable), None);
                        }
                    }
                }
            } else {
                for m in &sides[i] {
                    let m = *m;
                    self.pl_clear_doomsday_clock(tr, m);
                    let sid = self.pl_small_id(tr, m);
                    self.el_rot_state.retain(|s| !svz(s.id, sid));
                }
            }
        }
        Ok(JsVal::Undef)
    }

    /// `DoomsdayClockExecution.sides`: singletons in FFA, grouped by team (in
    /// first-seen order) otherwise.
    fn ddc_sides(&mut self, contenders: &[usize], ffa: bool, tr: &mut Vec<f64>) -> Vec<Vec<usize>> {
        if ffa {
            return contenders.iter().map(|p| vec![*p]).collect();
        }
        let mut by_team: Vec<(String, Vec<usize>)> = Vec::new();
        for p in contenders {
            let team = self.pl_team(tr, *p);
            if matches!(team, JsVal::Null) {
                continue;
            }
            let key = as_str(&team);
            match by_team.iter_mut().find(|(k, _)| *k == key) {
                Some(e) => e.1.push(*p),
                None => by_team.push((key, vec![*p])),
            }
        }
        by_team.into_iter().map(|(_, v)| v).collect()
    }

    fn ddc_rot(&mut self, tr: &mut Vec<f64>, player: usize, seconds_under: f64, cfg: &JsVal) {
        let owned = self.pl_num_tiles_owned(tr, player);
        if owned <= 0.0 {
            return;
        }
        let id = self.pl_small_id(tr, player);
        let state = match self.el_rot_state.iter().find(|s| svz(s.id, id)) {
            Some(s) => s.clone(),
            None => RotState {
                id,
                since: self.mg_ticks(tr),
                held: owned,
                front: Vec::new(),
            },
        };

        let even_quota = crate::doomsday_clock::rot_quota(
            owned,
            seconds_under,
            cfg_num(cfg, "rotDeathSeconds"),
        );
        let grain_seconds = cfg_num(cfg, "rotGrainSeconds");
        let speckle_percent = cfg_num(cfg, "rotSpecklePercent");
        let grainy = self.mg_ticks(tr) - state.since < grain_seconds * 10.0;
        let specks = if grainy {
            js_max(
                1.0,
                ((state.held * speckle_percent) / 100.0 / grain_seconds).ceil(),
            )
        } else {
            0.0
        };
        let mut budget = js_min(owned, js_max(even_quota, specks));

        let tiles = self.pl_tiles(tr, player);
        let mut front: Vec<(f64, f64)> = state
            .front
            .iter()
            .filter(|(t, _)| Self::tile_has(&tiles, *t))
            .cloned()
            .collect();

        budget -= self.ddc_speckle(tr, player, js_min(specks, budget), &mut front);
        budget -= self.ddc_spread(tr, player, budget, &mut front);
        if budget > 0.0 {
            self.ddc_speckle(tr, player, budget, &mut front);
        }

        match self.el_rot_state.iter_mut().find(|s| svz(s.id, id)) {
            Some(s) => {
                s.since = state.since;
                s.held = state.held;
                s.front = front;
            }
            None => self.el_rot_state.push(RotState {
                id,
                since: state.since,
                held: state.held,
                front,
            }),
        }
    }

    fn ddc_speckle(
        &mut self,
        tr: &mut Vec<f64>,
        player: usize,
        count: f64,
        front: &mut Vec<(f64, f64)>,
    ) -> f64 {
        if count <= 0.0 {
            return 0.0;
        }
        let salt = self.pl_small_id(tr, player);
        let border = self.pl_border_tiles(tr, player);
        let mut interior = LowestN::new(count);
        let mut edge = LowestN::new(count);
        let tiles = self.pl_tiles(tr, player);
        for tile in &tiles {
            let t = *tile;
            let key =
                crate::doomsday_clock::rot_speckle_noise(self.mg_x(tr, t), self.mg_y(tr, t), salt);
            if Self::tile_has(&border, t) {
                edge.offer(t, key);
            } else {
                interior.offer(t, key);
            }
        }
        let enough = interior.size() >= count;
        let mut picked = interior.take();
        if !enough {
            picked.extend(edge.take());
        }
        let mut opened = 0.0;
        for tile in picked {
            if opened == count {
                break;
            }
            if self.ddc_consume(tr, player, tile, front) {
                opened += 1.0;
            }
        }
        opened
    }

    fn ddc_spread(
        &mut self,
        tr: &mut Vec<f64>,
        player: usize,
        count: f64,
        front: &mut Vec<(f64, f64)>,
    ) -> f64 {
        if count <= 0.0 || front.is_empty() {
            return 0.0;
        }
        let salt = self.pl_small_id(tr, player);
        let mut pick = LowestN::new(count);
        for (tile, rotted) in front.iter() {
            let key = *rotted * f64::from(crate::doomsday_clock::ROT_NOISE_SCALE)
                + crate::doomsday_clock::rot_front_noise(*tile, salt);
            pick.offer(*tile, key);
        }
        let mut taken = 0.0;
        for tile in pick.take() {
            if taken == count {
                break;
            }
            if self.ddc_consume(tr, player, tile, front) {
                taken += 1.0;
            }
        }
        taken
    }

    /// `DoomsdayClockExecution.consume`: rot one tile and tally its owned
    /// neighbours into `front`.
    fn ddc_consume(
        &mut self,
        tr: &mut Vec<f64>,
        player: usize,
        tile: f64,
        front: &mut Vec<(f64, f64)>,
    ) -> bool {
        let tiles = self.pl_tiles(tr, player);
        if !Self::tile_has(&tiles, tile) {
            return false;
        }
        self.pl_relinquish(tr, player, tile);
        self.mg_set_fallout(tr, tile, true);
        self.pl_mark_rotted(tr, player);
        front.retain(|(t, _)| !svz(*t, tile));
        let neighbours = self.mg_neighbors(tr, tile);
        for n in neighbours {
            if !Self::tile_has(&tiles, n) {
                continue;
            }
            match front.iter_mut().find(|(t, _)| svz(*t, n)) {
                Some(e) => e.1 += 1.0,
                None => front.push((n, 1.0)),
            }
        }
        true
    }

    // ---- 31 TribeSpawner ---------------------------------------------------

    fn tribe_spawner(
        &mut self,
        kind: u8,
        args: &[f64],
        tr: &mut Vec<f64>,
    ) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            5 => self.tribe_spawn_tribes(args, tr),
            _ => unreachable!(),
        }
    }

    /// `TribeSpawner.spawnTribes(numTribes, purchasedNames)`; `args` is the flat
    /// `[numTribes, nameCount, (len, c0, c1, …)*nameCount]` encoding.
    fn tribe_spawn_tribes(&mut self, args: &[f64], tr: &mut Vec<f64>) -> Result<JsVal, String> {
        let num_tribes = args[0];
        let name_count = args[1] as usize;
        let mut i = 2usize;
        let mut purchased: Vec<String> = Vec::new();
        for _ in 0..name_count {
            let len = args[i] as usize;
            i += 1;
            let mut units: Vec<u16> = Vec::with_capacity(len);
            for _ in 0..len {
                units.push(args[i] as u16);
                i += 1;
            }
            purchased.push(String::from_utf16_lossy(&units));
        }

        let mut tribes: Vec<(String, String, Option<f64>)> = Vec::new();

        let custom_tribes = self.el_tribe_data.as_ref().and_then(|d| d.custom_tribes);
        if let Some(cts) = custom_tribes {
            for ct in cts {
                if ct.coordinates.is_none() {
                    continue;
                }
                if tribes.len() as f64 >= num_tribes {
                    break;
                }
                if let Some(rec) = self.spawn_positioned_tribe(ct, tr) {
                    tribes.push(rec);
                    self.el_used_custom_tribes.push(ct.name.to_string());
                }
            }
        }

        let remaining = num_tribes - tribes.len() as f64;
        let mut purchased_by_slot: Vec<(usize, String)> = Vec::new();
        if !purchased.is_empty() && remaining > 0.0 {
            let rem = remaining as usize;
            let used: Vec<String> = purchased.iter().take(rem).cloned().collect();
            let range: Vec<usize> = (0..rem).collect();
            let shuffled = self
                .random
                .as_mut()
                .expect("tribe spawner random")
                .shuffle_array(&range);
            let slots: Vec<usize> = shuffled.into_iter().take(used.len()).collect();
            for (k, s) in slots.into_iter().enumerate() {
                purchased_by_slot.push((s, used[k].clone()));
            }
        }

        let mut slot = 0usize;
        while (tribes.len() as f64) < num_tribes {
            let name = purchased_by_slot
                .iter()
                .find(|(s, _)| *s == slot)
                .map(|(_, n)| n.clone());
            let name = match name {
                Some(n) => n,
                None => self.random_tribe_name(),
            };
            tribes.push(self.spawn_tribe(&name));
            slot += 1;
        }

        let arr: Vec<JsVal> = tribes
            .into_iter()
            .map(|(name, id, tile)| {
                JsVal::Arr(vec![
                    JsVal::Str(name),
                    JsVal::Str(id),
                    match tile {
                        Some(t) => JsVal::Num(t),
                        None => JsVal::Null,
                    },
                ])
            })
            .collect();
        Ok(JsVal::Arr(arr))
    }

    /// `TribeSpawner.spawnPositionedTribe`.
    fn spawn_positioned_tribe(
        &mut self,
        ct: &crate::maps_gen::Tribe,
        tr: &mut Vec<f64>,
    ) -> Option<(String, String, Option<f64>)> {
        let (cx, cy) = ct.coordinates?;
        let is_compact = self.mg_game_map_size(tr) == GAME_MAP_SIZE_COMPACT;
        let x = if is_compact { (cx / 2.0).floor() } else { cx };
        let y = if is_compact { (cy / 2.0).floor() } else { cy };
        if !self.mg_is_valid_coord(tr, x, y) {
            return None;
        }
        let tile = self.mg_ref(tr, x, y);
        if !self.mg_is_land(tr, tile)
            || self.mg_has_owner(tr, tile)
            || self.mg_is_impassable(tr, tile)
            || Self::tile_has(&self.el_nation_tiles, tile)
        {
            return None;
        }
        Some((
            ct.name.to_string(),
            self.random.as_mut().expect("tribe spawner random").next_id(),
            Some(tile),
        ))
    }

    /// `TribeSpawner.spawnTribe`.
    fn spawn_tribe(&mut self, name: &str) -> (String, String, Option<f64>) {
        (
            name.to_string(),
            self.random.as_mut().expect("tribe spawner random").next_id(),
            None,
        )
    }

    /// `TribeSpawner.randomTribeName`.
    fn random_tribe_name(&mut self) -> String {
        let custom_tribes = self.el_tribe_data.as_ref().and_then(|d| d.custom_tribes);
        if let Some(cts) = custom_tribes {
            let mut available: Vec<&crate::maps_gen::Tribe> = Vec::new();
            for ct in cts {
                if ct.coordinates.is_none()
                    && !self.el_used_custom_tribes.iter().any(|u| u == ct.name)
                {
                    available.push(ct);
                }
            }
            if !available.is_empty() {
                let index = self
                    .random
                    .as_mut()
                    .expect("tribe spawner random")
                    .next_int(0.0, available.len() as f64) as usize;
                let chosen = available[index];
                self.el_used_custom_tribes.push(chosen.name.to_string());
                return chosen.name.to_string();
            }
        }

        let plen = self
            .el_tribe_data
            .as_ref()
            .map(|d| d.prefixes.len())
            .unwrap_or(0) as f64;
        let slen = self
            .el_tribe_data
            .as_ref()
            .map(|d| d.suffixes.len())
            .unwrap_or(0) as f64;
        let prefix_index = self
            .random
            .as_mut()
            .expect("tribe spawner random")
            .next_int(0.0, plen) as usize;
        let suffix_index = self
            .random
            .as_mut()
            .expect("tribe spawner random")
            .next_int(0.0, slen) as usize;
        let prefix = self
            .el_tribe_data
            .as_ref()
            .map(|d| d.prefixes[prefix_index].clone())
            .unwrap_or_default();
        let suffix = self
            .el_tribe_data
            .as_ref()
            .map(|d| d.suffixes[suffix_index].clone())
            .unwrap_or_default();
        format!("{prefix} {suffix}")
    }

    // ---- 32 SpawnExecution -------------------------------------------------

    fn spawn_execution(
        &mut self,
        kind: u8,
        _ticks: f64,
        tr: &mut Vec<f64>,
    ) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(true)),
            1 => {
                self.mg_ready = true;
                self.el_queued_spawn = self.mg_in_spawn_phase(tr);
                Ok(JsVal::Undef)
            }
            2 => self.spawn_execution_tick(tr),
            _ => unreachable!(),
        }
    }

    fn spawn_execution_tick(&mut self, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        self.flag = false;

        if !self.mg_ready {
            return Err(if self.el_spawn_tile.is_some() {
                "Cannot read properties of undefined (reading 'isValidRef')".into()
            } else {
                "Cannot read properties of undefined (reading 'hasPlayer')".into()
            });
        }

        if let Some(tile) = self.el_spawn_tile {
            if !self.mg_is_valid_ref(tr, tile) {
                return Ok(JsVal::Undef);
            }
        }

        let pid = self.el_player_id.clone();
        let player = if self.mg_has_player(tr, &pid) {
            self.mg_player(tr, &pid)
        } else {
            let pi = self.player_info_val();
            self.mg_add_player(tr, &pi)
        };
        let player = match player {
            Some(p) => p,
            None => {
                return Err(
                    "Cannot read properties of null (reading 'hasSpawned')".into(),
                )
            }
        };

        if self.el_from_intent && !self.el_queued_spawn {
            return Ok(JsVal::Undef);
        }

        if self.mg_is_random_spawn(tr) && self.pl_has_spawned(tr, player) {
            return Ok(JsVal::Undef);
        }

        let prev_tiles = self.pl_tiles(tr, player);
        for t in &prev_tiles {
            self.pl_relinquish(tr, player, *t);
        }
        let center_arg = if self.mg_is_random_spawn(tr) {
            None
        } else {
            self.el_spawn_tile
        };
        let spawn = match self.spawn_exec_get_spawn(tr, center_arg) {
            Some(s) => s,
            None => {
                for t in &prev_tiles {
                    self.pl_conquer(tr, player, *t);
                }
                return Ok(JsVal::Undef);
            }
        };

        for t in &spawn.1 {
            self.pl_conquer(tr, player, *t);
        }

        if !self.pl_has_spawned(tr, player) {
            self.mg_add_exec_player(tr, player);
            if self.pl_type(tr, player) == PLAYER_TYPE_BOT {
                self.mg_add_exec_tribe(tr, player);
            }
        }

        self.pl_set_spawn_tile(tr, player, spawn.0);

        if self.mg_game_type(tr) == GAME_TYPE_SINGLEPLAYER
            && self.el_player_type == PLAYER_TYPE_HUMAN
        {
            self.mg_end_spawn_phase(tr);
        }

        Ok(JsVal::Undef)
    }

    /// `SpawnExecution.getSpawn`: `Some((center, tiles))` or `None`.
    fn spawn_exec_get_spawn(
        &mut self,
        tr: &mut Vec<f64>,
        center: Option<f64>,
    ) -> Option<(f64, Vec<f64>)> {
        const MAX_SPAWN_TRIES: f64 = 1_000.0;
        const RELAX_MIN_DIST_AT: f64 = 750.0;

        if let Some(c) = center {
            let tiles = as_num_list(&self.mg_get_spawn_tiles(tr, Some(c), false));
            if tiles.is_empty() {
                return None;
            }
            return Some((c, tiles));
        }

        let area = self.spawn_exec_team_area(tr);
        let mut tries = 0.0f64;
        while tries < MAX_SPAWN_TRIES {
            tries += 1.0;
            let center = self.spawn_exec_rand_tile(tr, area);
            if !self.mg_is_land(tr, center)
                || self.mg_has_owner(tr, center)
                || self.mg_is_border(tr, center)
            {
                continue;
            }
            if tries <= RELAX_MIN_DIST_AT {
                let all = self.mg_all_players(tr);
                let mut others: Vec<usize> = Vec::new();
                for p in all {
                    if self.pl_id(tr, p) != self.el_player_id {
                        others.push(p);
                    }
                }
                let mut nearby = false;
                for p in others {
                    if let Some(st) = self.pl_spawn_tile(tr, p) {
                        let d = self.mg_manhattan_dist(tr, st, center);
                        let md = self.mg_min_distance_between_players(tr);
                        if d < md {
                            nearby = true;
                            break;
                        }
                    }
                }
                if nearby {
                    continue;
                }
            }
            let tiles_v = self.mg_get_spawn_tiles(tr, Some(center), true);
            if !Self::js_truthy(&tiles_v) {
                continue;
            }
            return Some((center, as_num_list(&tiles_v)));
        }
        None
    }

    /// `SpawnExecution.getTeamSpawnArea`.
    fn spawn_exec_team_area(&mut self, tr: &mut Vec<f64>) -> Option<(f64, f64, f64, f64)> {
        let pid = self.el_player_id.clone();
        let p = self.mg_player(tr, &pid)?;
        let team = self.pl_team(tr, p);
        if matches!(team, JsVal::Null) {
            return None;
        }
        let key = as_str(&team);
        let area = self.mg_team_spawn_area(tr, &key);
        Self::area_from_val(&area)
    }

    /// `SpawnExecution.randTile`.
    fn spawn_exec_rand_tile(
        &mut self,
        tr: &mut Vec<f64>,
        area: Option<(f64, f64, f64, f64)>,
    ) -> f64 {
        if let Some((ax, ay, w, h)) = area {
            let x = self
                .random
                .as_mut()
                .expect("spawn exec random")
                .next_int(ax, ax + w) as f64;
            let y = self
                .random
                .as_mut()
                .expect("spawn exec random")
                .next_int(ay, ay + h) as f64;
            return self.mg_ref(tr, x, y);
        }
        let w = self.mg_width(tr);
        let x = self
            .random
            .as_mut()
            .expect("spawn exec random")
            .next_int(0.0, w) as f64;
        let h = self.mg_height(tr);
        let y = self
            .random
            .as_mut()
            .expect("spawn exec random")
            .next_int(0.0, h) as f64;
        self.mg_ref(tr, x, y)
    }

    /// The plain `PlayerInfo` object as it arrives on `mg.addPlayer`, in the
    /// scenario's key order (`id`, then `playerType`).
    fn player_info_val(&self) -> JsVal {
        JsVal::Obj(vec![
            ("id".into(), JsVal::Str(self.el_player_id.clone())),
            ("playerType".into(), JsVal::Str(self.el_player_type.clone())),
        ])
    }

    // ================== G5f: Nuke / MIRV / SAM launcher ==================

    /// `getParabolaControlPoints(gameMap, from, to, options)`
    /// (`PathFinder.Parabola.ts`). The only map reads are x/y/height.
    #[allow(clippy::too_many_arguments)]
    fn el_parabola_control_points(
        &mut self,
        tr: &mut Vec<f64>,
        from: f64,
        to: f64,
        distance_based_height: bool,
        direction_up: bool,
        ignore_map_bounds: bool,
    ) -> [(f64, f64); 4] {
        let p0x = self.mg_x(tr, from);
        let p0y = self.mg_y(tr, from);
        let p3x = self.mg_x(tr, to);
        let p3y = self.mg_y(tr, to);
        let dx = p3x - p0x;
        let dy = p3y - p0y;
        let distance = (dx * dx + dy * dy).sqrt();
        let max_height = if distance_based_height {
            js_max(distance / 3.0, PARABOLA_MIN_HEIGHT)
        } else {
            0.0
        };
        let height_mult = if direction_up { -1.0 } else { 1.0 };
        let map_height = self.mg_height(tr);
        let p1y = p0y + dy / 4.0 + height_mult * max_height;
        let p2y = p0y + (dy * 3.0) / 4.0 + height_mult * max_height;
        let p1y = if ignore_map_bounds {
            p1y
        } else {
            crate::util::within(p1y, 0.0, map_height - 1.0)
        };
        let p2y = if ignore_map_bounds {
            p2y
        } else {
            crate::util::within(p2y, 0.0, map_height - 1.0)
        };
        [
            (p0x, p0y),
            (p0x + dx / 4.0, p1y),
            (p0x + (dx * 3.0) / 4.0, p2y),
            (p3x, p3y),
        ]
    }

    /// `DistanceBasedBezierCurve.getLength` over the 4 control points.
    fn bezier_length(pts: &[(f64, f64); 4]) -> f64 {
        use crate::line::{DistanceBasedBezierCurve, Point};
        let p = |i: usize| Point {
            x: pts[i].0,
            y: pts[i].1,
        };
        DistanceBasedBezierCurve::get_length(&p(0), &p(1), &p(2), &p(3))
    }

    /// `MirvExecution.calculateDeterministicSpeed`.
    fn mirv_calc_speed(&mut self, tr: &mut Vec<f64>, spawn_tile: f64, separate_dst: f64) -> f64 {
        let ideal = self.el_parabola_control_points(tr, spawn_tile, separate_dst, true, true, true);
        let ideal_len = Self::bezier_length(&ideal);
        let base_speed = self.mg_nuke_speed(tr, UNIT_MIRV);
        let ideal_ticks = ideal_len / base_speed;
        let base_ticks = self.mg_mirv_normalize_target_ticks(tr);
        let base_ticks_scaled = base_ticks * MIRV_MATH_SCALE;
        let ideal_ticks_int = (ideal_ticks * MIRV_MATH_SCALE).floor();
        let mut target_int = base_ticks_scaled;
        if ideal_ticks_int > base_ticks_scaled {
            let diff = ideal_ticks_int - base_ticks_scaled;
            target_int = base_ticks_scaled
                + diff.sqrt().floor() * MIRV_LONG_FLIGHT_MULT
                + ((diff * MIRV_LONG_FLIGHT_LINEAR_PERCENT) / 100.0).floor();
        } else if ideal_ticks_int < base_ticks_scaled {
            let diff = base_ticks_scaled - ideal_ticks_int;
            target_int = base_ticks_scaled - diff.sqrt().floor() * MIRV_SHORT_FLIGHT_MULT;
        }
        target_int = js_max(MIRV_MATH_SCALE, target_int);
        let actual = self.el_parabola_control_points(tr, spawn_tile, separate_dst, true, true, false);
        let actual_len = Self::bezier_length(&actual);
        let actual_len_int = js_round(actual_len * 256.0);
        let num = actual_len_int * MIRV_MATH_SCALE;
        let den = target_int;
        let pure = ((num + (den / 2.0).floor()) / den).floor();
        js_max(1.0, pure / 256.0)
    }

    /// `MirvExecution.tryGenerateTarget` — returns the accepted tile.
    fn mirv_try_generate_target(&mut self, tr: &mut Vec<f64>) -> Option<f64> {
        let mut attempt = 0;
        while attempt < 100 {
            let (r1, r2) = {
                let rng = self.random.as_mut().expect("mirv rng initialized");
                let r1 = rng.next();
                let r2 = crate::jsnum::js_mod(r1 * 15485863.0, 1.0);
                (r1, r2)
            };
            let x = js_round(r1 * MIRV_RANGE * 2.0 - MIRV_RANGE + self.mirv_base_x);
            let y = js_round(r2 * MIRV_RANGE * 2.0 - MIRV_RANGE + self.mirv_base_y);
            attempt += 1;
            if !self.mg_is_valid_coord(tr, x, y) {
                continue;
            }
            let tile = self.mg_ref(tr, x, y);
            if !self.mg_is_land(tr, tile) {
                continue;
            }
            if (x - self.mirv_base_x).powi(2) + (y - self.mirv_base_y).powi(2) > self.mirv_range_squared
            {
                continue;
            }
            if self.mg_owner(tr, tile) != self.mirv_target_player {
                continue;
            }
            if self.mirv_is_overlapping(tr, x, y) {
                continue;
            }
            return Some(tile);
        }
        None
    }

    /// `MirvExecution.isOverlapping` over the staged targets.
    fn mirv_is_overlapping(&mut self, tr: &mut Vec<f64>, x: f64, y: f64) -> bool {
        let taken = self.mirv_staged_targets.clone();
        for existing in taken {
            let ex = self.mg_x(tr, existing);
            let ey = self.mg_y(tr, existing);
            let manhattan = (x - ex).abs() + (y - ey).abs();
            if manhattan < MIRV_MINIMUM_SPREAD {
                return true;
            }
        }
        false
    }

    /// `MirvExecution.finalizeDestinations` (`500 + extraAttempts` callers).
    fn mirv_finalize_destinations(&mut self, tr: &mut Vec<f64>, additional_attempts: f64) {
        let dst = self.mirv_dst;
        let tp = self.mirv_target_player;
        let mut kept: Vec<f64> = Vec::new();
        for tile in self.mirv_staged_targets.clone() {
            if tile == dst || self.mg_owner(tr, tile) == tp {
                kept.push(tile);
            }
        }
        self.mirv_staged_targets = kept;
        let mut attempt = 0.0;
        while attempt < additional_attempts {
            if self.mirv_staged_targets.len() as f64 >= self.mirv_warhead_count as f64 {
                break;
            }
            if let Some(t) = self.mirv_try_generate_target(tr) {
                self.mirv_staged_targets.push(t);
            }
            attempt += 1.0;
        }
        // V8 in-place sort, comparator `(a, b) => dist(b) - dist(a)`.
        let n = self.mirv_staged_targets.len();
        if n > 1 {
            for i in 1..n {
                let element = self.mirv_staged_targets[i];
                let (mut lo, mut hi) = (0usize, i);
                while lo < hi {
                    let mid = (lo + hi) / 2;
                    let m = self.mg_manhattan_dist(tr, self.mirv_staged_targets[mid], dst);
                    let e = self.mg_manhattan_dist(tr, element, dst);
                    if m - e < 0.0 {
                        hi = mid;
                    } else {
                        lo = mid + 1;
                    }
                }
                if lo < i {
                    self.mirv_staged_targets.remove(i);
                    self.mirv_staged_targets.insert(lo, element);
                }
            }
        }
    }

    /// `MirvExecution.spawnWarheadsWithWait` — a `NukeExecution` per staged tile.
    fn mirv_spawn_warheads_with_wait(&mut self, tr: &mut Vec<f64>, remaining_ticks: f64) {
        if self.mirv_nuke.is_none() {
            return;
        }
        let wait_base = js_max(0.0, remaining_ticks);
        let warhead_speed = self.mg_nuke_speed(tr, UNIT_MIRV_WARHEAD); // 700
        let staged = self.mirv_staged_targets.clone();
        let separate_dst = self.mirv_separate_dst;
        for (i, dst) in staged.into_iter().enumerate() {
            let speed_offset = if i < 70 {
                0.0
            } else if i < 140 {
                1.0
            } else if i < 210 {
                2.0
            } else if i < 280 {
                3.0
            } else {
                4.0
            };
            let wait =
                wait_base + self.random.as_mut().expect("mirv rng").next_int(0.0, 15.0) as f64;
            self.mg_add_nuke(
                tr,
                UNIT_MIRV_WARHEAD,
                self.a_tok,
                dst,
                Some(separate_dst),
                warhead_speed + speed_offset,
                wait,
                None,
            );
        }
    }

    fn mirv_execution(&mut self, kind: u8, _ticks: f64, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        match kind {
            3 => return Ok(JsVal::Bool(self.flag)),
            4 => return Ok(JsVal::Bool(false)),
            1 => {
                self.mg_ready = true;
                let t = self.mg_ticks(tr); // 212
                let pid = self.pl_id(tr, self.a_tok); // 231
                self.random = Some(PseudoRandom::new(t + crate::util::simple_hash(&pid)));
                self.mirv_target_player = self.mg_owner(tr, self.mirv_dst); // 222
                self.mirv_base_x = self.mg_x(tr, self.mirv_dst); // 603
                self.mirv_base_y = self.mg_y(tr, self.mirv_dst); // 604
                self.mirv_staged_targets = vec![self.mirv_dst];
                return Ok(JsVal::Undef);
            }
            2 => {}
            _ => return Ok(JsVal::Undef),
        }

        if self.mirv_nuke.is_none() {
            let spawn = self.pl_can_build(tr, self.a_tok, UNIT_MIRV, self.mirv_dst); // 534
            if matches!(spawn, JsVal::Bool(false)) {
                self.flag = false;
                return Ok(JsVal::Undef);
            }
            let spawn_tile = as_num(&spawn);
            self.mirv_spawn_tile = spawn_tile;
            let tp = self.mirv_target_player;
            let target_param = match tp {
                Some(t) => JsVal::Num(t as f64),
                None => JsVal::Undef,
            };
            let params = JsVal::Obj(vec![
                ("targetTile".to_string(), JsVal::Num(self.mirv_dst)),
                ("targetPlayer".to_string(), target_param),
            ]);
            self.mirv_nuke = self.pl_build_unit(tr, self.a_tok, UNIT_MIRV, spawn_tile, &params); // 535
            self.mg_stats_bomb_launch(tr, self.a_tok, tp, UNIT_MIRV); // 717
            let tp_tok = match tp {
                Some(t) => t,
                None => return Err("Cannot read properties of null (reading 'isPlayer')".into()),
            };
            if self.pl_is_player(tr, tp_tok) {
                // 259
                let alliance = self.pl_alliance_with(tr, self.a_tok, Some(tp_tok)); // 243
                if let Some(a) = alliance {
                    self.pl_break_alliance(tr, self.a_tok, a); // 244
                }
                if tp_tok != self.a_tok {
                    self.pl_update_relation(tr, tp_tok, Some(self.a_tok), -100.0); // 237
                    self.pl_update_relation(tr, self.a_tok, Some(tp_tok), -100.0); // 237
                }
            }
            let nu = self.mirv_nuke.expect("nuke built");
            let nuke_x = self.un_tile(tr, nu); // 344
            let x = ((self.mirv_base_x + self.mg_x(tr, nuke_x)) / 2.0).floor(); // 603
            let y = js_max(0.0, self.mirv_base_y - 500.0) + 50.0;
            self.mirv_separate_dst = self.mg_ref(tr, x, y); // 609
            let sep = self.mirv_separate_dst;
            self.mirv_speed = self.mirv_calc_speed(tr, spawn_tile, sep);
            let speed = self.mirv_speed;
            let mut full_path: Vec<f64> = Vec::new();
            let mut result = self.mg_parabola_next(tr, spawn_tile, sep, speed); // 750
            while parabola_status(&result) == PATH_STATUS_NEXT {
                full_path.push(parabola_node_num(&result));
                result = self.mg_parabola_next(tr, spawn_tile, sep, speed); // 750
            }
            self.mirv_full_path = full_path;
            self.mirv_path_index = 0;
            let nid = self.un_id(tr, nu); // 340
            let dn = self.pl_display_name(tr, self.a_tok); // 232
            let tid = match tp {
                Some(t) => self.pl_id(tr, t), // 231
                None => String::new(),
            };
            let msg = format!("⚠️⚠️⚠️ {dn} - MIRV INBOUND ⚠️⚠️⚠️");
            self.mg_display_incoming_unit(tr, nid, &msg, MSG_MIRV_INBOUND, &tid); // 714
            let silos = self.pl_units(tr, self.a_tok, &[UNIT_MISSILE_SILO.to_string()]); // 241
            let mut silo: Option<usize> = None;
            for s in silos {
                let st = self.un_tile(tr, s); // 344
                if svz(st, spawn_tile) {
                    silo = Some(s);
                    break;
                }
            }
            if let Some(s) = silo {
                self.un_launch(tr, s); // 742
            }
        }

        let nu = self.mirv_nuke.expect("nuke present");
        if !self.un_is_active(tr, nu) {
            // 280
            self.flag = false;
            return Ok(JsVal::Undef);
        }

        let remaining_ticks = self.mirv_full_path.len() as f64 - self.mirv_path_index as f64;
        if remaining_ticks <= 20.0 && remaining_ticks > 10.0 {
            let mut attempt = 0;
            while attempt < 100 {
                if self.mirv_staged_targets.len() as f64 >= self.mirv_warhead_count as f64 {
                    break;
                }
                if let Some(t) = self.mirv_try_generate_target(tr) {
                    self.mirv_staged_targets.push(t);
                }
                attempt += 1;
            }
        }
        if remaining_ticks <= 10.0 && !self.mirv_warheads_spawned {
            self.mirv_warheads_spawned = true;
            let extra = (10.0 - remaining_ticks) * 50.0;
            self.mirv_finalize_destinations(tr, 500.0 + extra);
            self.mirv_spawn_warheads_with_wait(tr, remaining_ticks);
        }
        if self.mirv_path_index < self.mirv_full_path.len() {
            let node = self.mirv_full_path[self.mirv_path_index];
            self.mirv_path_index += 1;
            self.un_move(tr, nu, &JsVal::Num(node)); // 545
        } else {
            self.un_delete(tr, nu, false); // 285
            self.flag = false;
            self.mg_stats_bomb_land(tr, self.a_tok, self.mirv_target_player, UNIT_MIRV); // 718
        }
        Ok(JsVal::Undef)
    }

    // ---------------------------- NukeExecution ----------------------------

    /// `NukeExecution.getTrajectory(target)`.
    fn nuke_get_trajectory(&mut self, tr: &mut Vec<f64>, target: f64) -> JsVal {
        let range = self.mg_default_nuke_targetable_range(tr); // 702
        let range_sq = range * range;
        let src = self.nuke_src.expect("src set");
        let all = self.mg_parabola_find_path(tr, src, target); // 751
        let mut out: Vec<JsVal> = Vec::new();
        for tile in all {
            let targetable = self.nuke_is_targetable(tr, target, tile, range_sq);
            out.push(JsVal::Obj(vec![
                ("tile".to_string(), JsVal::Num(tile)),
                ("targetable".to_string(), JsVal::Bool(targetable)),
            ]));
        }
        JsVal::Arr(out)
    }

    /// `NukeExecution.isTargetable(targetTile, nukeTile, targetRangeSquared)`.
    fn nuke_is_targetable(
        &mut self,
        tr: &mut Vec<f64>,
        target_tile: f64,
        nuke_tile: f64,
        range_sq: f64,
    ) -> bool {
        if self.mg_euclidean_dist_squared(tr, nuke_tile, target_tile) < range_sq {
            // 710
            return true;
        }
        if let Some(src) = self.nuke_src {
            if self.mg_euclidean_dist_squared(tr, src, nuke_tile) < range_sq {
                // 710
                return true;
            }
        }
        false
    }

    /// `NukeExecution.updateNukeTargetable`.
    fn nuke_update_targetable(&mut self, tr: &mut Vec<f64>) {
        let nu = match self.nuke {
            Some(n) => n,
            None => return,
        };
        if self.un_target_tile(tr, nu).is_none() {
            // 282
            return;
        }
        let range = self.mg_default_nuke_targetable_range(tr); // 702
        let range_sq = range * range;
        let target_tile = match self.un_target_tile(tr, nu) {
            // 282
            Some(t) => t,
            None => return,
        };
        let nuke_tile = self.un_tile(tr, nu); // 344
        let flag = self.nuke_is_targetable(tr, target_tile, nuke_tile, range_sq);
        self.un_set_targetable(tr, nu, flag); // 734
    }

    /// `NukeExecution.recordMotionPlan(ticks)`.
    fn nuke_record_motion_plan(&mut self, tr: &mut Vec<f64>, ticks: f64) {
        let nu = match self.nuke {
            Some(n) => n,
            None => return,
        };
        let src = match self.nuke_src {
            Some(s) => s,
            None => return,
        };
        let speed = self.nuke_speed;
        let dst = self.nuke_dst;
        let mut path: Vec<JsVal> = vec![JsVal::Num(src)];
        let mut result = self.mg_parabola_next(tr, src, dst, speed); // 750
        while parabola_status(&result) == PATH_STATUS_NEXT {
            path.push(JsVal::Num(parabola_node_num(&result)));
            result = self.mg_parabola_next(tr, src, dst, speed); // 750
        }
        let uid = self.un_id(tr, nu); // 340
        let plan = JsVal::Obj(vec![
            ("kind".to_string(), JsVal::Str("grid".to_string())),
            ("unitId".to_string(), JsVal::Num(uid)),
            ("planId".to_string(), JsVal::Num(1.0)),
            (
                "startTick".to_string(),
                JsVal::Num(ticks + self.nuke_wait_ticks + 1.0),
            ),
            ("ticksPerStep".to_string(), JsVal::Num(1.0)),
            ("path".to_string(), JsVal::Arr(path)),
        ]);
        self.mg_record_motion_plan(tr, &plan); // 711
    }

    /// `NukeExecution.tilesToDestroy` (memoised on the instance).
    fn nuke_tiles_to_destroy(&mut self, tr: &mut Vec<f64>) -> Vec<f64> {
        if let Some(c) = self.tiles_to_destroy_cache.as_ref() {
            return c.clone();
        }
        let nu = self.nuke.expect("nuke initialized");
        let ty = self.un_type(tr, nu); // 283
        let (inner, outer) = self.mg_nuke_magnitudes(tr, &ty); // 224
        let seed = self.mg_ticks(tr); // 212
        let mut rand = PseudoRandom::new(seed);
        let inner2 = inner * inner;
        let outer2 = outer * outer;
        let dst = self.nuke_dst;
        let result = if self.mg_water_nukes(tr) {
            // 701
            let num_samples = 16usize;
            let mut radii_sq = vec![0.0f64; num_samples];
            for r in radii_sq.iter_mut() {
                *r = rand.next_float(inner2, outer2);
            }
            let prev = radii_sq.clone();
            for i in 0..num_samples {
                let l = (i + num_samples - 1) % num_samples;
                let r = (i + 1) % num_samples;
                radii_sq[i] = prev[i] * 0.6 + prev[l] * 0.2 + prev[r] * 0.2;
            }
            let cx = self.mg_x(tr, dst); // 603
            let cy = self.mg_y(tr, dst); // 604
            let width = self.mg_width(tr); // 610
            let height = self.mg_height(tr); // 611
            let x0 = js_max(0.0, cx - outer);
            let y0 = js_max(0.0, cy - outer);
            let x1 = js_min(width - 1.0, cx + outer);
            let y1 = js_min(height - 1.0, cy + outer);
            let mut out: Vec<f64> = Vec::new();
            let mut py = y0;
            while py <= y1 {
                let mut px = x0;
                while px <= x1 {
                    let dx = px - cx;
                    let dy = py - cy;
                    let d2 = dx * dx + dy * dy;
                    px += 1.0;
                    if d2 > outer2 {
                        continue;
                    }
                    if d2 > inner2 {
                        let angle =
                            crate::detmath::atan2(dy, dx) + std::f64::consts::PI;
                        let t = (angle / (2.0 * std::f64::consts::PI)) * num_samples as f64;
                        let i0 = crate::jsnum::js_mod(t.floor(), num_samples as f64) as usize;
                        let i1 = (i0 + 1) % num_samples;
                        let frac = t - t.floor();
                        let threshold =
                            radii_sq[i0] * (1.0 - frac) + radii_sq[i1] * frac;
                        if d2 > threshold {
                            continue;
                        }
                    }
                    let tile = self.mg_ref(tr, px - 1.0, py); // 609
                    if self.mg_is_impassable(tr, tile) {
                        // 617
                        continue;
                    }
                    if !out.iter().any(|v| svz(*v, tile)) {
                        out.push(tile);
                    }
                }
                py += 1.0;
            }
            out
        } else {
            self.mg_bfs(tr, dst, |h, tr, n| {
                let d2 = h.mg_euclidean_dist_squared(tr, dst, n); // 710
                if !(d2 <= outer2 && (d2 <= inner2 || rand.chance(2.0))) {
                    return false;
                }
                !h.mg_is_impassable(tr, n) // 617
            })
        };
        self.tiles_to_destroy_cache = Some(result.clone());
        result
    }

    /// `listNukeBreakAlliance` (`Util.ts:100-129`) — returns the small IDs.
    fn list_nuke_break_alliance(
        &mut self,
        tr: &mut Vec<f64>,
        target_tile: f64,
        inner: f64,
        outer: f64,
        threshold: f64,
    ) -> Vec<f64> {
        let inner2 = inner * inner;
        let mut counts: Vec<(f64, f64)> = Vec::new();
        self.mg_circle_search(tr, target_tile, outer, |h, tr, tile, d2| {
            let sid = h.mg_owner_id(tr, tile); // 223
            if sid > 0.0 {
                let weight = if d2 <= inner2 { 1.0 } else { 0.5 };
                match counts.iter_mut().find(|(id, _)| *id == sid) {
                    Some(e) => e.1 += weight,
                    None => counts.push((sid, weight)),
                }
            }
            true
        });
        let mut players: Vec<f64> = Vec::new();
        for (sid, total) in &counts {
            if *total > threshold && !players.iter().any(|p| svz(*p, *sid)) {
                players.push(*sid);
            }
        }
        let types: Vec<String> = STRUCTURES_TYPES.iter().map(|s| s.to_string()).collect();
        for uid in self.mg_nearby_units(tr, target_tile, outer, &types) {
            // 306
            if let Some(o) = self.un_owner(tr, uid) {
                // 286
                let small = self.pl_small_id(tr, o); // 230
                if !players.iter().any(|p| svz(*p, small)) {
                    players.push(small);
                }
            }
        }
        players
    }

    /// `NukeExecution.maybeBreakAlliances`.
    fn nuke_maybe_break_alliances(&mut self, tr: &mut Vec<f64>) {
        let nu = match self.nuke {
            Some(n) => n,
            None => return,
        };
        let ty = self.un_type(tr, nu); // 283
        if ty == UNIT_MIRV_WARHEAD {
            return;
        }
        let ty2 = self.un_type(tr, nu); // 283
        let (inner, outer) = self.mg_nuke_magnitudes(tr, &ty2); // 224
        let threshold = self.mg_nuke_alliance_break_threshold(tr); // 225
        let to_break = self.list_nuke_break_alliance(tr, self.nuke_dst, inner, outer, threshold);
        let incoming = self.pl_incoming_alliance_requests(tr, self.a_tok); // 720
        for rid in incoming {
            let r = match self.rq_requestor(tr, rid) {
                Some(r) => r,
                None => continue,
            }; // 744
            let sid = self.pl_small_id(tr, r); // 230
            if to_break.iter().any(|p| svz(*p, sid)) {
                self.rq_reject(tr, rid); // 272
            }
        }
        for sid in to_break {
            let attacked = self.mg_player_by_small_id(tr, sid); // 703
            let tok = match as_opt_tok(&attacked) {
                Some(t) => t,
                None => continue,
            };
            if !self.pl_is_player(tr, tok) {
                // 259
                continue;
            }
            let ra = self.pl_incoming_alliance_requests(tr, tok); // 720
            let mut rejected = false;
            for rid in ra {
                let r = self.rq_requestor(tr, rid); // 744
                if r == Some(self.a_tok) {
                    self.rq_reject(tr, rid); // 272
                    rejected = true;
                    break;
                }
            }
            if rejected {
                continue;
            }
            let all = self.pl_alliance_with(tr, self.a_tok, Some(tok)); // 243
            if let Some(a) = all {
                self.pl_break_alliance(tr, self.a_tok, a); // 244
            }
            if tok != self.a_tok {
                self.pl_update_relation(tr, tok, Some(self.a_tok), -100.0); // 237
            }
        }
    }

    /// `NukeExecution.redrawBuildings(range)`.
    fn nuke_redraw_buildings(&mut self, tr: &mut Vec<f64>, range: f64) {
        let range_sq = range * range;
        let dst = self.nuke_dst;
        for unit in self.mg_units(tr) {
            // 709
            let ty = self.un_type(tr, unit); // 283
            if STRUCTURES_TYPES.iter().any(|s| *s == ty) {
                let ut = self.un_tile(tr, unit); // 344
                let d = self.mg_euclidean_dist_squared(tr, dst, ut); // 710
                if d < range_sq {
                    self.un_touch(tr, unit); // 736
                }
            }
        }
    }

    /// `NukeExecution.detonate`.
    fn nuke_detonate(&mut self, tr: &mut Vec<f64>) {
        let nu = self.nuke.expect("nuke initialized");
        let ty = self.un_type(tr, nu); // 283
        let (_inner, outer) = self.mg_nuke_magnitudes(tr, &ty); // 224
        let to_destroy = self.nuke_tiles_to_destroy(tr);
        // Cloned so the `mg_nuke_death_factor` calls below do not hold an
        // immutable borrow of `self` across the mutable one.
        let nuke_type = self.nuke_type.clone();
        let mut tiles_per_players: Vec<(usize, f64)> = Vec::new();
        for tile in to_destroy {
            let owner = self.mg_owner(tr, tile); // 222
            if let Some(o) = owner {
                if self.pl_is_player(tr, o) {
                    // 259
                    self.pl_relinquish(tr, o, tile); // 636
                    match tiles_per_players.iter_mut().find(|(p, _)| *p == o) {
                        Some(e) => e.1 += 1.0,
                        None => tiles_per_players.push((o, 1.0)),
                    }
                }
            }
            if self.mg_is_land(tr, tile) {
                // 304
                self.mg_queue_water_conversion(tr, tile); // 713
            }
            self.mg_queue_nuke_impact(tr, tile); // 712
        }
        for (player, num_impacted) in tiles_per_players.clone() {
            let tiles_before = self.pl_num_tiles_owned(tr, player) + num_impacted; // 532
            let transports = self.pl_units(tr, player, &[UNIT_TRANSPORT.to_string()]); // 241
            let mut transport_troops: Vec<(usize, f64)> = Vec::new();
            for u in &transports {
                let t = self.un_troops(tr, *u); // 730
                transport_troops.push((*u, t));
            }
            let outgoing = self.pl_outgoing_attacks(tr, player); // 721
            let max_troops = self.mg_max_troops(tr, Rec::P(player)); // 401
            let mut i = 0.0;
            while i < num_impacted {
                let tiles_left = tiles_before - i;
                let pt = self.pl_troops(tr, player); // 418
                let deaths =
                    self.mg_nuke_death_factor(tr, &nuke_type, pt, tiles_left, max_troops); // 704
                self.pl_remove_troops(tr, player, deaths); // 633
                for attack in &outgoing {
                    let at = self.un_troops(tr, *attack); // 730
                    let d = self.mg_nuke_death_factor(
                        tr,
                        &nuke_type,
                        at,
                        tiles_left,
                        max_troops,
                    ); // 704
                    self.un_set_troops(tr, *attack, at - d); // 731
                }
                for u in &transports {
                    let ut = transport_troops
                        .iter()
                        .find(|(x, _)| x == u)
                        .map(|(_, v)| *v)
                        .unwrap_or(0.0);
                    let d = self.mg_nuke_death_factor(
                        tr,
                        &nuke_type,
                        ut,
                        tiles_left,
                        max_troops,
                    ); // 704
                    let nv = js_max(0.0, ut - d);
                    if let Some(e) = transport_troops.iter_mut().find(|(x, _)| x == u) {
                        e.1 = nv;
                    }
                }
                i += 1.0;
            }
            for (u, troops) in &transport_troops {
                self.un_set_troops(tr, *u, *troops); // 731
            }
        }
        let outer2 = outer * outer;
        let dst = self.nuke_dst;
        for unit in self.mg_units(tr) {
            // 709
            let ty = self.un_type(tr, unit); // 283
            if ty == UNIT_ATOM_BOMB
                || ty == UNIT_HYDROGEN_BOMB
                || ty == UNIT_MIRV_WARHEAD
                || ty == UNIT_MIRV
                || ty == UNIT_SAM_MISSILE
            {
                continue;
            }
            let ut = self.un_tile(tr, unit); // 344
            let d = self.mg_euclidean_dist_squared(tr, dst, ut); // 710
            if d < outer2 {
                let o = self.un_owner(tr, unit); // 286
                let friendly = self.pl_is_friendly(tr, self.a_tok, o); // 242
                self.un_delete(tr, unit, true); // 285
                let _ = friendly;
            }
        }
        self.nuke_redraw_buildings(tr, outer + SPRITE_RADIUS);
        self.flag = false;
        self.un_set_reached_target(tr, nu); // 544
        self.un_delete(tr, nu, false); // 285
        if self.nuke_type == UNIT_ATOM_BOMB || self.nuke_type == UNIT_HYDROGEN_BOMB {
            let message_key = if self.nuke_type == UNIT_ATOM_BOMB {
                "events_display.atom_bomb_detonated"
            } else {
                "events_display.hydrogen_bomb_detonated"
            };
            for (impacted, _) in &tiles_per_players {
                let iid = self.pl_id(tr, *impacted); // 231
                let dn = self.pl_display_name(tr, self.a_tok); // 232
                let pid = self.pl_id(tr, self.a_tok); // 231
                let args = vec![
                    JsVal::Str(message_key.to_string()),
                    JsVal::Num(MSG_NUKE_DETONATED),
                    JsVal::Str(iid),
                    JsVal::Undef,
                    JsVal::Obj(vec![("name".to_string(), JsVal::Str(dn))]),
                    JsVal::Undef,
                    JsVal::Str(pid),
                ];
                self.mg_display_message(tr, &args); // 226
            }
        }
        let target = self.mg_owner(tr, dst); // 222
        let ty2 = self.un_type(tr, nu); // 283
        self.mg_stats_bomb_land(tr, self.a_tok, target, &ty2); // 718
    }

    fn nuke_execution(&mut self, kind: u8, ticks: f64, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        // Cloned so `nuke_type` can be passed alongside `&mut self` below.
        let nuke_type = self.nuke_type.clone();
        match kind {
            3 => return Ok(JsVal::Bool(self.flag)),
            4 => return Ok(JsVal::Bool(false)),
            1 => {
                self.mg_ready = true;
                if self.nuke_speed == -1.0 {
                    self.nuke_speed = self.mg_nuke_speed(tr, &nuke_type); // 700
                }
                return Ok(JsVal::Undef);
            }
            2 => {}
            _ => return Ok(JsVal::Undef),
        }

        if self.nuke.is_none() {
            let spawn = self.pl_can_build(tr, self.a_tok, &nuke_type, self.nuke_dst); // 534
            if matches!(spawn, JsVal::Bool(false)) {
                self.flag = false;
                return Ok(JsVal::Undef);
            }
            let spawn_tile = as_num(&spawn);
            let src = match self.nuke_src {
                Some(s) => s,
                None => {
                    self.nuke_src = Some(spawn_tile);
                    spawn_tile
                }
            };
            let silos = self.pl_units(tr, self.a_tok, &[UNIT_MISSILE_SILO.to_string()]); // 241
            let mut silo: Option<usize> = None;
            for s in silos {
                let st = self.un_tile(tr, s); // 344
                if svz(st, spawn_tile) {
                    silo = Some(s);
                    break;
                }
            }
            if let Some(s) = silo {
                let mut last_dep = 0.0;
                let q = self.un_missile_timer_queue(tr, s); // 346
                for launch_tick in as_arr_nums(&q) {
                    last_dep = js_max(launch_tick + 1.0, last_dep + 1.0);
                }
                let cur = self.mg_ticks(tr); // 212
                if last_dep > cur {
                    let cur2 = self.mg_ticks(tr); // 212
                    self.nuke_wait_ticks += last_dep - cur2;
                }
            }
            let trajectory = self.nuke_get_trajectory(tr, self.nuke_dst);
            let params = JsVal::Obj(vec![
                ("targetTile".to_string(), JsVal::Num(self.nuke_dst)),
                ("trajectory".to_string(), trajectory),
            ]);
            self.nuke = self.pl_build_unit(tr, self.a_tok, &nuke_type, src, &params); // 535
            let nu = match self.nuke {
                Some(n) => n,
                None => {
                    return Err(
                        "Cannot read properties of undefined (reading 'updateNukeState')".into(),
                    )
                }
            };
            self.un_update_nuke_state(tr, nu, self.nuke_wait_ticks); // 732
            self.nuke_record_motion_plan(tr, ticks);
            let ty = self.un_type(tr, nu); // 283
            if ty != UNIT_MIRV_WARHEAD {
                self.nuke_maybe_break_alliances(tr);
            }
            if self.mg_has_owner(tr, self.nuke_dst) {
                // 607
                let target = match self.mg_owner(tr, self.nuke_dst) {
                    // 222
                    Some(t) => t,
                    None => {
                        return Err(
                            "Cannot read properties of null (reading 'isPlayer')".into(),
                        )
                    }
                };
                if self.pl_is_player(tr, target) {
                    // 259
                    if self.nuke_type == UNIT_ATOM_BOMB {
                        let nid = self.un_id(tr, nu); // 340
                        let dn = self.pl_display_name(tr, self.a_tok); // 232
                        let tid = self.pl_id(tr, target); // 231
                        let msg = format!("{dn} - atom bomb inbound");
                        self.mg_display_incoming_unit(tr, nid, &msg, MSG_NUKE_INBOUND, &tid); // 714
                    } else if self.nuke_type == UNIT_HYDROGEN_BOMB {
                        let nid = self.un_id(tr, nu); // 340
                        let dn = self.pl_display_name(tr, self.a_tok); // 232
                        let tid = self.pl_id(tr, target); // 231
                        let msg = format!("{dn} - hydrogen bomb inbound");
                        self.mg_display_incoming_unit(
                            tr,
                            nid,
                            &msg,
                            MSG_HYDROGEN_BOMB_INBOUND,
                            &tid,
                        ); // 714
                    }
                }
                self.mg_stats_bomb_launch(tr, self.a_tok, Some(target), &nuke_type); // 717
            }
            if let Some(s) = silo {
                self.un_launch(tr, s); // 742
            }
            return Ok(JsVal::Undef);
        }

        let nu = self.nuke.unwrap();
        if !self.un_is_active(tr, nu) {
            // 280
            self.flag = false;
            return Ok(JsVal::Undef);
        }
        if self.nuke_wait_ticks > 0.0 {
            self.nuke_wait_ticks -= 1.0;
            self.un_update_nuke_state(tr, nu, self.nuke_wait_ticks); // 732
            return Ok(JsVal::Undef);
        }
        let src = self.nuke_src.unwrap_or(f64::NAN);
        let result = self.mg_parabola_next(tr, src, self.nuke_dst, self.nuke_speed); // 750
        let status = parabola_status(&result);
        if status == PATH_STATUS_COMPLETE {
            let node = val_field(&result, "node").cloned().unwrap_or(JsVal::Undef);
            self.un_move(tr, nu, &node); // 545
            let outer = self.mg_default_sam_speed(tr); // 516
            let hits = self.mg_nearby_units_cb(tr, self.nuke_dst, outer, &[UNIT_SAM_MISSILE.to_string()], |h, tr, uid| {
                if !h.un_is_unit(tr, uid) {
                    // 737
                    return false;
                }
                let cand_owner = h.un_owner(tr, uid); // 286
                let nuke_owner = h.un_owner(tr, nu); // 286
                if cand_owner == nuke_owner {
                    return false;
                }
                match h.un_target_unit(tr, uid) {
                    // 735
                    None => false,
                    Some(t) => h.un_id(tr, t) == h.un_id(tr, nu), // 340, 340
                }
            });
            if hits.is_empty() {
                self.nuke_detonate(tr);
            }
        } else if status == PATH_STATUS_NEXT {
            self.nuke_update_targetable(tr);
            let node = val_field(&result, "node").cloned().unwrap_or(JsVal::Undef);
            self.un_move(tr, nu, &node); // 545
            let idx = self.mg_parabola_current_index(tr); // 752
            self.un_set_trajectory_index(tr, nu, idx); // 733
        }
        Ok(JsVal::Undef)
    }

    // ------------------------ SAMLauncherExecution ------------------------

    fn sam_on_level_up(&mut self) {
        if let Some(t) = self.sam_targeting.as_mut() {
            t.precomputed_nukes.retain(|(_, c)| c.tick != -1.0);
        }
    }

    fn sam_update_unreachable_nukes(&mut self, tick: f64) {
        if let Some(t) = self.sam_targeting.as_mut() {
            t.precomputed_nukes.retain(|(_, c)| c.last_seen_tick == tick);
        }
    }

    fn sam_missile_speed(&self) -> f64 {
        self.sam_targeting
            .as_ref()
            .map(|t| t.missile_speed)
            .unwrap_or(f64::NAN)
    }

    /// `SAMTargetingSystem.tickToReach`.
    fn sam_tick_to_reach(&mut self, tr: &mut Vec<f64>, current: f64, tile: f64) -> f64 {
        let d = self.mg_manhattan_dist(tr, current, tile); // 509
        (d / self.sam_missile_speed()).ceil()
    }

    /// `SAMTargetingSystem.isValidNukeTarget`.
    fn sam_is_valid_nuke_target(&mut self, tr: &mut Vec<f64>, sam: usize, uid: usize) -> bool {
        if !self.un_is_unit(tr, uid) {
            // 737
            return false;
        }
        if self.un_targeted_by_sam(tr, uid) {
            // 743
            return false;
        }
        let uo = self.un_owner(tr, uid); // 286
        let sam_owner0 = self.un_owner(tr, sam); // 286
        if uo == sam_owner0 {
            return false;
        }
        let sam_owner = self.un_owner(tr, sam).unwrap_or(self.a_tok); // 286
        let nuke_owner = self.un_owner(tr, uid); // 286
        if self.pl_is_friendly(tr, sam_owner, nuke_owner) {
            // 242
            let winner = self.mg_get_winner(tr); // 600
            let game_over = if !matches!(winner, JsVal::Null) {
                let gt = self.mg_game_type(tr); // 214
                gt != GAME_TYPE_SINGLEPLAYER
            } else {
                false
            };
            if !game_over {
                return false;
            }
            return self.pl_is_on_same_team(tr, sam_owner, nuke_owner); // 250
        }
        true
    }

    /// `SAMTargetingSystem.checkDetonationInterception`.
    fn sam_check_detonation_interception(
        &mut self,
        tr: &mut Vec<f64>,
        sam: usize,
        uid: usize,
        sam_tile: f64,
        ticks: f64,
    ) -> Option<CachedInterception> {
        let traj = as_traj(&self.un_trajectory(tr, uid)); // 740
        let max_idx = traj.len() as f64 - 2.0;
        let final_tile = match traj.last() {
            Some(t) if t.1 => *t,
            _ => return None,
        };
        let cur_idx = self.un_trajectory_index(tr, uid); // 738
        let wait_ticks = nuke_state_wait_ticks(&self.un_nuke_state(tr, uid)); // 739
        let exp_ticks = traj.len() as f64 - 1.0 - cur_idx + wait_ticks;
        let range = self.mg_dynamic_sam_range(tr, sam, ticks + exp_ticks); // 706
        if self.mg_euclidean_dist_squared(tr, sam_tile, final_tile.0) > range * range {
            // 710
            return None;
        }
        let flight_tile = match traj.get(max_idx as usize) {
            Some(t) if max_idx >= 0.0 && t.1 => *t,
            _ => return None,
        };
        let nuke_ticks = max_idx - cur_idx + wait_ticks;
        let sam_ticks = self.sam_tick_to_reach(tr, sam_tile, flight_tile.0);
        let tick_before = nuke_ticks - sam_ticks;
        if tick_before >= 0.0 {
            Some(CachedInterception {
                tick: tick_before,
                tile: flight_tile.0,
                min_dist_sq: 0.0,
                last_seen_tick: ticks,
            })
        } else {
            None
        }
    }

    /// `SAMTargetingSystem.computeInterceptionTile`.
    fn sam_compute_interception_tile(
        &mut self,
        tr: &mut Vec<f64>,
        sam: usize,
        uid: usize,
        sam_tile: f64,
        ticks: f64,
    ) -> CachedInterception {
        let traj = as_traj(&self.un_trajectory(tr, uid)); // 740
        let cur_idx = self.un_trajectory_index(tr, uid); // 738
        let wait_ticks = nuke_state_wait_ticks(&self.un_nuke_state(tr, uid)); // 739
        let max_idx = traj.len() as f64 - 2.0;
        let max_range = self.mg_max_sam_range(tr); // 707
        let max_range_sq = max_range * max_range;
        let mut min_dist_sq = f64::INFINITY;
        let mut closest_tile = sam_tile;
        let mut inc_steps = 0.0;
        let mut last_dist_sq = -1.0;
        let mut i = cur_idx;
        while i <= max_idx {
            let (tile, targetable) = match traj.get(i as usize) {
                Some(v) => *v,
                None => (f64::NAN, false),
            };
            let dist_sq = self.mg_euclidean_dist_squared(tr, sam_tile, tile); // 710
            if dist_sq < min_dist_sq {
                min_dist_sq = dist_sq;
                closest_tile = tile;
            }
            if last_dist_sq != -1.0 && dist_sq > last_dist_sq {
                inc_steps += 1.0;
            } else {
                inc_steps = 0.0;
            }
            last_dist_sq = dist_sq;
            let nuke_ticks = i - cur_idx + wait_ticks;
            let sam_ticks = self.sam_tick_to_reach(tr, sam_tile, tile);
            let allowed = self.mg_dynamic_sam_range(tr, sam, ticks + nuke_ticks); // 706
            if targetable && dist_sq <= allowed * allowed && nuke_ticks >= sam_ticks {
                return CachedInterception {
                    tick: nuke_ticks - sam_ticks,
                    tile,
                    min_dist_sq,
                    last_seen_tick: ticks,
                };
            }
            if inc_steps > 3.0 && dist_sq > max_range_sq {
                break;
            }
            i += 1.0;
        }
        if let Some(d) = self.sam_check_detonation_interception(tr, sam, uid, sam_tile, ticks) {
            return CachedInterception {
                tick: d.tick,
                tile: d.tile,
                min_dist_sq,
                last_seen_tick: ticks,
            };
        }
        CachedInterception {
            tick: if min_dist_sq > max_range_sq { -2.0 } else { -1.0 },
            tile: closest_tile,
            min_dist_sq,
            last_seen_tick: ticks,
        }
    }

    /// `SAMTargetingSystem.computeTargetScore`.
    fn sam_compute_target_score(&mut self, tr: &mut Vec<f64>, uid: usize) -> f64 {
        let sam = self.sam_unit.expect("sam set");
        let sam_tile = self.un_tile(tr, sam); // 344
        let traj = as_traj(&self.un_trajectory(tr, uid)); // 740
        let current_index = self.un_trajectory_index(tr, uid); // 738
        let time_to_explode = js_max(1.0, traj.len() as f64 - current_index);
        let target_tile = match self.un_target_tile(tr, uid) {
            // 282
            Some(t) => t,
            None => {
                if !traj.is_empty() {
                    traj[traj.len() - 1].0
                } else {
                    sam_tile
                }
            }
        };
        let dist = self.mg_manhattan_dist(tr, sam_tile, target_tile); // 509
        let ty = self.un_type(tr, uid); // 283
        let type_bonus = if ty == UNIT_HYDROGEN_BOMB { 70001.0 } else { 0.0 };
        let distance_bonus = js_max(0.0, 200000.0 - dist * 1000.0);
        let urgency_bonus = js_max(0.0, 10000.0 - time_to_explode * 100.0);
        type_bonus + distance_bonus + urgency_bonus
    }

    /// `SAMTargetingSystem.sortTargets` — V8's binary-insertion sort on scores.
    fn sam_sort_targets(
        &mut self,
        tr: &mut Vec<f64>,
        targets: Vec<(usize, f64)>,
    ) -> Vec<(usize, f64)> {
        if targets.len() <= 1 {
            return targets;
        }
        let mut scored: Vec<(usize, f64, f64)> = Vec::new();
        for (u, t) in targets {
            let s = self.sam_compute_target_score(tr, u);
            scored.push((u, t, s));
        }
        let n = scored.len();
        for i in 1..n {
            let element = scored[i];
            let (mut lo, mut hi) = (0usize, i);
            while lo < hi {
                let mid = (lo + hi) / 2;
                if scored[mid].2 - element.2 < 0.0 {
                    hi = mid;
                } else {
                    lo = mid + 1;
                }
            }
            if lo < i {
                scored.remove(i);
                scored.insert(lo, element);
            }
        }
        scored.into_iter().map(|(u, t, _)| (u, t)).collect()
    }

    /// `SAMTargetingSystem.getValidTargets`.
    fn sam_get_valid_targets(&mut self, tr: &mut Vec<f64>, ticks: f64) -> Vec<(usize, f64)> {
        let sam = self.sam_unit.expect("sam set");
        let sam_tile = self.un_tile(tr, sam); // 344
        let detection_range = self.mg_max_sam_range(tr) * 4.0; // 707
        let types: Vec<String> = [UNIT_ATOM_BOMB, UNIT_HYDROGEN_BOMB, UNIT_MIRV_WARHEAD]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let candidates = self.mg_nearby_units_cb(tr, sam_tile, detection_range, &types, |h, tr, uid| {
            h.sam_is_valid_nuke_target(tr, sam, uid)
        }); // 715
        let mut targets: Vec<(usize, f64)> = Vec::new();
        for uid in candidates {
            let id = self.un_id(tr, uid); // 340
            let cached_idx = self
                .sam_targeting
                .as_ref()
                .and_then(|t| t.precomputed_nukes.iter().position(|(k, _)| svz(*k, id)));
            if let Some(ci) = cached_idx {
                let c = {
                    let t = self.sam_targeting.as_mut().unwrap();
                    t.precomputed_nukes[ci].1.last_seen_tick = ticks;
                    t.precomputed_nukes[ci].1
                };
                if c.tick == -2.0 || c.tick == -1.0 {
                    continue;
                }
                if c.tick == ticks || c.tick == ticks + 1.0 {
                    targets.push((uid, c.tile));
                    self.sam_targeting.as_mut().unwrap().precomputed_nukes.remove(ci);
                    continue;
                }
                if c.tick > ticks {
                    continue;
                }
                self.sam_targeting.as_mut().unwrap().precomputed_nukes.remove(ci);
            }
            let res = self.sam_compute_interception_tile(tr, sam, uid, sam_tile, ticks);
            if res.tick >= 0.0 && res.tick <= 1.0 {
                targets.push((uid, res.tile));
            } else {
                let tick = if res.tick >= 0.0 { res.tick + ticks } else { res.tick };
                self.sam_targeting.as_mut().unwrap().precomputed_nukes.push((
                    id,
                    CachedInterception {
                        tick,
                        tile: res.tile,
                        min_dist_sq: res.min_dist_sq,
                        last_seen_tick: ticks,
                    },
                ));
            }
        }
        self.sam_update_unreachable_nukes(ticks);
        self.sam_sort_targets(tr, targets)
    }

    fn sam_launcher_execution(
        &mut self,
        kind: u8,
        ticks: f64,
        tr: &mut Vec<f64>,
    ) -> Result<JsVal, String> {
        match kind {
            3 => return Ok(JsVal::Bool(self.flag)),
            4 => return Ok(JsVal::Bool(false)),
            1 => {
                self.mg_ready = true;
                return Ok(JsVal::Undef);
            }
            2 => {}
            _ => return Ok(JsVal::Undef),
        }
        if !self.mg_ready {
            return Err("Not initialized".into());
        }
        if self.sam_unit.is_none() {
            let tile = match self.sam_tile {
                Some(t) => t,
                None => return Err("tile is null".into()),
            };
            let spawn = self.pl_can_build(tr, self.a_tok, UNIT_SAM_LAUNCHER, tile); // 534
            if matches!(spawn, JsVal::Bool(false)) {
                self.flag = false;
                return Ok(JsVal::Undef);
            }
            let spawn_tile = as_num(&spawn);
            let params = JsVal::Obj(vec![]);
            self.sam_unit = self.pl_build_unit(tr, self.a_tok, UNIT_SAM_LAUNCHER, spawn_tile, &params); // 535
        }
        let sam = self.sam_unit.expect("sam set");
        if self.sam_targeting.is_none() {
            let ms = self.mg_default_sam_speed(tr); // 516
            self.sam_targeting = Some(SamTargeting {
                precomputed_nukes: Vec::new(),
                missile_speed: ms,
            });
        }
        let lvl = self.un_level(tr, sam); // 540
        if self.sam_last_level != lvl {
            self.sam_last_level = self.un_level(tr, sam); // 540
            self.sam_on_level_up();
        }
        if self.un_is_under_construction(tr, sam) {
            // 345
            return Ok(JsVal::Undef);
        }
        if !self.un_is_active(tr, sam) {
            // 280
            self.flag = false;
            return Ok(JsVal::Undef);
        }
        let owner = self.un_owner(tr, sam); // 286
        if owner != Some(self.a_tok) {
            if let Some(o) = owner {
                self.a_tok = o;
            }
        }
        loop {
            let q = self.un_missile_timer_queue(tr, sam); // 346
            if as_arr_nums(&q).is_empty() {
                break;
            }
            let q2 = self.un_missile_timer_queue(tr, sam); // 346
            let front = as_arr_nums(&q2).first().copied().unwrap_or(f64::NAN);
            let cd = self.mg_sam_cooldown(tr); // 708
            let tk = self.mg_ticks(tr); // 212
            if cd - (tk - front) > 0.0 {
                break;
            }
            self.un_reload_missile(tr, sam); // 347
        }
        if self.un_is_in_cooldown(tr, sam) {
            // 741
            return Ok(JsVal::Undef);
        }
        if self.random.is_none() {
            let id = self.un_id(tr, sam); // 340
            self.random = Some(PseudoRandom::new(id));
        }
        let a = self.mg_unit_count(tr, UNIT_ATOM_BOMB); // 506
        if a == 0.0 {
            let b = self.mg_unit_count(tr, UNIT_HYDROGEN_BOMB); // 506
            if b == 0.0 {
                let c = self.mg_unit_count(tr, UNIT_MIRV_WARHEAD); // 506
                if c == 0.0 {
                    return Ok(JsVal::Undef);
                }
            }
        }
        let targets = self.sam_get_valid_targets(tr, ticks);
        for (unit, tile) in targets {
            if self.un_is_in_cooldown(tr, sam) {
                // 741
                break;
            }
            self.un_launch(tr, sam); // 742
            self.un_set_targeted_by_sam(tr, unit, true); // 734
            let st = self.un_tile(tr, sam); // 344
            let ow = self.un_owner(tr, sam); // 286
            self.mg_add_sam_missile(tr, st, ow.unwrap_or(self.a_tok), sam, unit, tile);
        }
        Ok(JsVal::Undef)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_ally_small_ids_returns_false() {
        // wouldNukeBreakAlliance short-circuits on an empty ally set without
        // touching any facade script.
        let mut h = ExecHarness::new();
        let mut tr = Vec::new();
        assert!(!h.would_nuke_break_alliance(&mut tr, 1.0, 1.0, 2.0, &[], 5.0));
        assert!(tr.is_empty());
    }

    #[test]
    fn lowest_n_bounds_and_orders_by_key() {
        // `LowestN.offer` keeps the two smallest keys, ascending, and rejects a
        // key that is not strictly below the worst survivor.
        let mut ln = LowestN::new(2.0);
        ln.offer(10.0, 5.0);
        ln.offer(11.0, 1.0);
        ln.offer(12.0, 3.0);
        assert_eq!(ln.items, vec![(11.0, 1.0), (12.0, 3.0)]);
        ln.offer(13.0, 3.0);
        assert_eq!(ln.items, vec![(11.0, 1.0), (12.0, 3.0)]);
        ln.offer(14.0, 0.0);
        assert_eq!(ln.items, vec![(14.0, 0.0), (11.0, 1.0)]);
    }

    #[test]
    fn tile_has_uses_same_value_zero() {
        assert!(ExecHarness::tile_has(&[1.0, 2.0, 3.0], 2.0));
        assert!(!ExecHarness::tile_has(&[1.0, 2.0, 3.0], 4.0));
        // `-0.0` and `0.0` compare equal under SameValueZero (the JS `Set.has`).
        assert!(ExecHarness::tile_has(&[0.0], -0.0));
    }

    #[test]
    fn js_truthy_falsy_matrix() {
        assert!(!ExecHarness::js_truthy(&JsVal::Undef));
        assert!(!ExecHarness::js_truthy(&JsVal::Null));
        assert!(!ExecHarness::js_truthy(&JsVal::Num(0.0)));
        assert!(!ExecHarness::js_truthy(&JsVal::Num(f64::NAN)));
        assert!(!ExecHarness::js_truthy(&JsVal::Str(String::new())));
        assert!(ExecHarness::js_truthy(&JsVal::Bool(true)));
        assert!(!ExecHarness::js_truthy(&JsVal::Bool(false)));
        assert!(ExecHarness::js_truthy(&JsVal::Num(1.0)));
        assert!(ExecHarness::js_truthy(&JsVal::Obj(vec![])));
    }

    #[test]
    fn area_from_val_requires_the_four_fields() {
        let ok = JsVal::Obj(vec![
            ("x".into(), JsVal::Num(1.0)),
            ("y".into(), JsVal::Num(2.0)),
            ("width".into(), JsVal::Num(3.0)),
            ("height".into(), JsVal::Num(4.0)),
        ]);
        assert_eq!(ExecHarness::area_from_val(&ok), Some((1.0, 2.0, 3.0, 4.0)));
        assert_eq!(ExecHarness::area_from_val(&JsVal::Null), None);
        assert_eq!(ExecHarness::area_from_val(&JsVal::Undef), None);
        let missing = JsVal::Obj(vec![("x".into(), JsVal::Num(1.0))]);
        assert_eq!(ExecHarness::area_from_val(&missing), None);
    }

    #[test]
    fn player_info_val_keeps_declared_field_order() {
        let mut h = ExecHarness::new();
        h.el_player_id = "p1".into();
        h.el_player_type = "HUMAN".into();
        assert_eq!(
            h.player_info_val(),
            JsVal::Obj(vec![
                ("id".into(), JsVal::Str("p1".into())),
                ("playerType".into(), JsVal::Str("HUMAN".into())),
            ])
        );
    }

    #[test]
    fn nuke_state_wait_ticks_defaults_to_zero() {
        // `unit.nukeState().waitTicks ?? 0`: only a numeric field survives.
        assert_eq!(nuke_state_wait_ticks(&JsVal::Undef), 0.0);
        assert_eq!(nuke_state_wait_ticks(&JsVal::Null), 0.0);
        assert_eq!(
            nuke_state_wait_ticks(&JsVal::Obj(vec![("other".into(), JsVal::Num(3.0))])),
            0.0
        );
        assert_eq!(
            nuke_state_wait_ticks(&JsVal::Obj(vec![("waitTicks".into(), JsVal::Num(7.5))])),
            7.5
        );
    }

    #[test]
    fn parabola_status_and_node_read_the_object_fields() {
        let r = JsVal::Obj(vec![
            ("status".into(), JsVal::Num(2.0)),
            ("node".into(), JsVal::Num(9.0)),
        ]);
        assert_eq!(parabola_status(&r), 2.0);
        assert_eq!(parabola_node_num(&r), 9.0);
        // A missing field is NaN (the `PathStatus.NEXT`/`COMPLETE` comparison).
        assert!(parabola_status(&JsVal::Undef).is_nan());
        assert!(parabola_node_num(&JsVal::Null).is_nan());
    }

    #[test]
    fn as_traj_reads_tile_and_targetable() {
        let v = JsVal::Arr(vec![
            JsVal::Obj(vec![
                ("tile".into(), JsVal::Num(5.0)),
                ("targetable".into(), JsVal::Bool(true)),
            ]),
            JsVal::Obj(vec![("tile".into(), JsVal::Num(6.0))]),
        ]);
        assert_eq!(as_traj(&v), vec![(5.0, true), (6.0, false)]);
        assert!(as_traj(&JsVal::Null).is_empty());
    }

    #[test]
    fn as_arr_nums_reads_a_number_list() {
        assert_eq!(
            as_arr_nums(&JsVal::Arr(vec![JsVal::Num(3.0), JsVal::Num(5.0)])),
            vec![3.0, 5.0]
        );
        assert!(as_arr_nums(&JsVal::Undef).is_empty());
    }
}
