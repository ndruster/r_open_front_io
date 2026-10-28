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
//! G5g appends `36` Attack, `37` Transport (`AttackExecution`,
//! `TransportShipExecution`). Both reuse the already-ported `FlatBinaryHeap`
//! (`pathfinding/flat_heap.rs`) and `transport_ship_utils`; `Attack` is a
//! scripted facade mock (new `attacks` mock category), the water pathfinder a
//! scripted stub object.
//!
//! G5h appends `38` TradeShip (`TradeShipExecution`). It reuses the stub
//! `WaterPathFinder` (now with `pathForTraversal`), the real `findClosestBy`
//! (`src/core/Util.ts`), and `renderNumber` (`src/client/Utils.ts`) for its
//! captured-payout message; no new construction tag (it builds a *unit* via
//! `buildUnit`, and its own `addExecution` record tag `12` already existed).
//!
//! G5i appends `39` TrainStationExecution, `40` TrainExecution
//! (`TrainStationExecution.ts` / `TrainExecution.ts`). Stations are identified
//! by their unit token (the capture's `mg.__railStationFor(unit)` stub
//! canonicalizes the facade per unit, so `destination === station` compares
//! unit tokens). A new `stations` mock category (indexed by unit token) scripts
//! `getCluster` (920) / `getRailroadTo` (921) FIFO; the static `clusters` /
//! `railroads` tables drive the real `tradeAvailable` / reservoir-sampling
//! logic on both sides. `TrainExecution.loadCargo` is op kind `5`.
//!
//! G5j appends `41` WarshipExecution, `42` PlayerExecution
//! (`WarshipExecution.ts` / `PlayerExecution.ts`). Warships patrol, hunt trade
//! ships, and retreat to ports for repairs. `PlayerExecution` handles structure
//! capture, death bookkeeping, troop/gold income, alliance expiry, embargo
//! expiry, and periodic cluster removal (`removeClusters` / `calculateClusters`
//! / `checkAndAssignTerritory`).
//!
//! G5k appends `43` TribeExecution (`TribeExecution.ts` + its inlined
//! `AiAttackBehavior` collaborator, `execution/utils/AiAttackBehavior.ts`). The
//! tribe is a `PlayerType.Bot` (TribeSpawner), so the `shouldAttack` /
//! `troopSendCap` / `isAttackTooWeak` guards short-circuit on the tribe's own
//! `type()`. Targets reach `sendAttack` through three mock classes and the
//! `isPlayer()` trace follows the concrete object: `Player` (259), `Neighbor`
//! (257) or the untraced `TerraNullius` singleton (`TargetVia::Pl/Nb/Tn`).
//!
//! G5k Step 2 appends `44` NationExecution (`NationExecution.ts`) with its
//! behavior families ported inline as the `ne_*` functions: the tick ladder
//! (`initializeBehaviors` — only the `NationNukeBehavior` field initializers
//! trace, `cost(Atom/HydrogenBomb)` 512→969 plus `chance(3)`; `getAttackRate`,
//! `randomSpawnLand`, `updateRelationsFromEmbargos`,
//! `handleEmbargoesToHostileNations`), the social emoji behaviors, the
//! alliance request / decision / betray ladder, the MIRV ladder
//! (`considerMirv`, counter-MIRV / victory-denial / steamroll target
//! selection) and the warship behavior (spawn, transport / trade-ship
//! tracking + retaliation, infestation counter, team / FFA targeting,
//! `buildCounterWarship`). `NationStructureBehavior.handleStructures()` (G5l)
//! and `NationNukeBehavior.maybeSendNuke()` (G5m) are ported inline as well,
//! including the nuke target ladder (`findBestNukeTarget` — retaliation,
//! high-density, FFA-crown, ally-assist, most-hated, strongest-team), the
//! tile scoring / trajectory-interception helpers and the full
//! `maybeDestroyEnemySam` SAM-outing with the silo-upgrade fallback. The
//! nation player reuses the tribe token (`te_tribe`);
//! the eleven Step-2 scenarios (`el_nation_init_new_player` /
//! `_init_existing_player` / `_spawn_no_cell` / `_spawn_team_outside` /
//! `_spawn_random_land` / `_spawn_fail` / `_death` / `_gate_skip_easy` /
//! `_ship_transport_retaliate` / `_ship_trade_retaliate` /
//! `_ship_incoming_retaliate`) pin the init difficulty branches, the
//! `randomSpawnLand` paths, the death gate, the attack-rate gate and the
//! three warship tracking / retaliation chains. The alliance / betray / MIRV
//! ladders stay unvectorized for now; the `el_struct_*` scenarios (G5l) pin
//! the structure spawn-tile ladder at the one-third / two-thirds gates and
//! the `el_nuke_no_silo` / `el_nuke_retaliation_hydro` scenarios (G5m) pin
//! full attack-tick runs — the no-silo early exit and the retaliation
//! Hydrogen-Bomb launch through `findBestNukeTarget` → `sendNuke` — and the
//! four G5n scenarios pin the deep paths: `el_nuke_sam_outing` (Impossible
//! SAM-outing overwhelm: the covering-SAM scan, bombsNeeded/totalBombs math
//! and the staggered 0/16/32-waitTick AtomBomb launch), `el_nuke_sam_upgrade`
//! (silo too small → `maybeUpgradeHelpfulSilo` emitting an
//! `UpgradeStructureExecution`), `el_nuke_ffa_crown` (FFA crown-share target
//! selection through numLandTiles / numTilesWithFallout) and
//! `el_nuke_team_strongest` (strongest-team target selection — it caught the
//! cached `players()` reuse where the TS loop re-reads the facade).
//! G5o opens the maybeSendNuke tile scan itself: `el_nuke_tile_boxes` (Easy,
//! non-empty boundingBoxTiles rings with 300 target-owned tile reads, a
//! level-2 enemy City entering nukeTileScore's structure switch and the
//! closestTwoTiles distance penalty) and `el_nuke_sam_outrange_score`
//! (Impossible Hydrogen Bomb: empty-trajectory isTrajectoryInterceptableBySam
//! short-circuit and the +100k×level SAM-outrange bonus). Scenario C caught a
//! real parity bug: the post-closestTwoTiles distance read was recorded as
//! `(closest_x, closest_y)` while TS 729 reads
//! `euclideanDistSquared(tile, closestSilo)` — args swapped whenever the
//! silo and scored tiles differ. `el_nuke_team_spot_skip` walks the Team-mode
//! isTeammateAlreadyNukingThisSpot overlap check: a Hard three-player Team
//! retaliation leaves the getPerceivedNukeCost two-player shortcut (Team gold
//! gate + four cost reads through mg 121), the sole candidate is skipped by a
//! teammate Hydrogen Bomb whose targetTile collapses the distance read to
//! 0 <= (10+10)^2, and the tick ends with pure facade reads and no execution
//! records (Hard has no maybeDestroyEnemySam fallback). `el_nuke_recent_filter`
//! drives two attack ticks of one Easy nation: tick 108 launches the
//! retaliation HydrogenBomb and pushes the recent entry, tick 174 replays the
//! chain with removeOldNukeEvents keeping it (66 < 600 maxAge) and the
//! nukeTileScore recent filter subtracting 1_000_000 from the 0-valued tile,
//! so bestTile stays null and no second nuke is recorded.
//!
//! G5p opens the NationStructureBehavior deep paths: `el_struct_upgrade_city`
//! (Easy density-triggered upgrade — the 1/1000 density passes
//! UPGRADE_DENSITY_THRESHOLD, `config.unitInfo(City).upgradable` is read
//! through the mg-47 facade that zero-density scenarios short-circuit away,
//! and the Easy 70% random pick selects the only upgradable City, emitting a
//! 307/25 `UpgradeStructureExecution`) and `el_struct_upgrade_sam_score`
//! (Hard 25% roll misses into the SAM-protection scoring: a level-2 SAM
//! covers city 0 at dist² 100 <= 3600 for score 17.5 while city 1 sits at
//! 1_000_000, the descending sort keeps city 0 first, and the units slot FIFO
//! is arg-agnostic across units(City) / units(Structures.types) /
//! units(SAMLauncher)). Both scenarios also pin the NationExecution tick
//! side-gate: attackRate/attackTick derive from the scenario PRNG
//! (`simpleHash(id)+simpleHash(gameID)` — g1 Easy 66/42, g2 Hard 51/41), and
//! handleStructures runs at the one-third / two-thirds offsets (B1 tick 130,
//! B2 tick 126) while non-matching ticks only initialize the behaviors
//! (nuke-ctor cost cache reads + the 307/16 forceSendAttack record).
//! `el_struct_dp_front` (B3) completes the family with the Hard defense-post
//! front path: tick 109 (offset 7) walks the full City-build pipeline
//! (save-up target via MIRV+HydrogenBomb cost pairs, SAM-first dying on
//! startingGold < 3M, maybeSpawnStructure -> randTerritoryTileArray ->
//! cityValue connectivity setup -> 25-tile magnitude+border-dist scan, two
//! canBuild reads per placement — structureSpawnTile plus the
//! maybeSpawnStructure re-check) and bumps placementsCount to 1, so tick 126
//! (offset 24) enters tryBuildDefensePost: incomingAttacks (player 58, wire
//! 822) with a null sourceTile and a real attacker clears the boat filter,
//! troops 400/1000 pass the 0.35 threat ratio, both border tiles become front
//! tiles via neighbors4 (mg 39, wire 804) + owner (mg 12), and
//! sampleTilesNearFront's manhattan ring walk (width/height/x/y mg 70/71/63/
//! 64 over the border Set) lands 25 tiles inside the [0.75x, 1.5x] x
//! borderSpacing depth ring with no fallback. Both placements emit 307/17
//! `ConstructionExecution`. The scenario also pins the sticky FIFO tail
//! semantics (an exhausted slot repeats its last value) and the one long
//! owner FIFO: 25 owned reads for the territory sampler, two attacker-owned
//! front reads, then owned again for the post scan.
//! `el_struct_dp_blocked` (B4) records the deny branch: the gold FIFO drops
//! to 50 on the fourth gold() read so tryBuildDefensePost fails the
//! gold < cost check and defensePostNeeded (a second incomingAttacks pass,
//! attacker never resolved) blocks all other structures — a pure-read tick
//! with no ConstructionExecution. `el_struct_dp_multi` (B5) raises the
//! threat ratio to 0.8 (allowed = ceil(0.8/0.4) = 2) and scripts one Defense
//! Post unit (tile slot 11), exercising countDefensePostsNearFront's
//! dp.tile() + euclideanDistSquared (mg 89) cap-short-circuit loop,
//! sampleTilesNearFront's anchor-spread filter (whose sticky dist empties
//! the anchors into the full-front fallback), and 26 canBuild reads for the
//! second post. `el_struct_sam_first_high_gold` (B6) flips startingGold to
//! 5M: tick 109 takes the SAM-first branch (samLauncherValue with the
//! no-arg units() protectEntries scan, defaultSamRange mg 127, and a
//! coverage roll that re-reads units(SAMLauncher); all empty so the closure
//! is magnitude + border-dist), tick 126 builds City #2 (cooldown[1] = 0),
//! and tick 160 pins the cooldown block — placementsCount 2 indexes
//! HIGH_GOLD_STRUCTURE_COOLDOWN_TICKS to 250, so 34 ticks after the last
//! placement handleStructures returns false. mg 2 (ticks) is the only
//! tick-distinguishing FIFO: [109 x3, 126 x2, 160].
//! `el_struct_ratio_factory_silo` (B7) walks the ratio-driven build order
//! with a real city count: pl 72 (unitsOwned) is the tick-distinguishing
//! FIFO and exactly six values cover both ticks because the density/upgrade
//! checks read units(Structures.types) (pl 11, empty) rather than
//! unitsOwned — tick 109 reads City=3 (cityCount) then Factory=0 (target
//! floor(3 x 0.75) = 2) into the full factoryValue pipeline (perceived cost
//! via the FFA MIRV+Hydro save-up pair, connectivity roll + empty station
//! mocks, the 25-tile magnitude+border-dist tie landing tile 78), and tick
//! 126 re-reads City=3, skips Factory (owned 2, 2 < 2 false) and SAM
//! (target floor(3 x 0.25) = 0), then MissileSilo=0 takes
//! FIRST_MISSILE_SILO_RATIO 0.4 -> target 1 -> missileSiloValue
//! (borderTiles + units(MissileSilo) empty: magnitude + border dist only,
//! no connectivity). Both placements emit 307/24; mg 2 covers the
//! post-save-up set [109] and tick 126's sticky [126, 126] elapsed +
//! lastStructureTick reads. `el_struct_crowded_factory_first` (B8) drops
//! mg 35 to 50 land tiles so isHighNationDensity (1/50 > 1/7500) fires
//! after the SAM-first branch dies on startingGold: the crowded branch
//! picks Factory (sharedWater null short-circuits the Port preference) and
//! returns straight from the branch — no buildOrder entry evaluated —
//! pinning builtCrowdedMapFirstStructure on a single-structure tick.
//! `el_struct_team_saveup_block` (B9) switches mg 54 to "Team" so
//! getSaveUpTarget reads a single cost(HydrogenBomb) (no MIRV pair): tick
//! 109 builds City #1 and stamps _postSaveUpStartTick = 109, ticks 160/211
//! (elapsed 51/102) stay under the 150-tick off-phase and build Cities
//! #2/#3, and tick 262 (elapsed 153, 153 % 300 >= 150) hits the on-phase —
//! the deny chain (dp branch with empty incomingAttacks, cooldown[3] = 150
//! against 51 ticks since the last placement, phase check) is pure reads
//! with no gold read and no ConstructionExecution.
//! `el_struct_port_coastal_pipeline` (B10) opens the port coastal path:
//! mg 129 returns the shared water component set {7} so hasCoastalTiles
//! holds, and maybeSpawnStructure(Port) (cityCount 2 -> target 1 > owned 0)
//! runs randCoastalTileArray over the border set {55, 551555}: tile 55 is
//! shore with water neighbor 100 that is not ocean, sits in component 7
//! (shared) and passes the Hard-difficulty size gate (getWaterComponentSize
//! 3000 >= MIN_PORT_WATER_COMPONENT_SIZE), while 551555 fails isShore;
//! portValue scores the single candidate Infinity (no other ports, zero
//! manhattanDist reads) and the Port lands on tile 55 as 307/24 — no owner
//! reads anywhere on the tick.
//! `el_struct_factory_coastal_ratio` (B11) pins the coastal multiplier by
//! outcome: all nukes disabled (mg 45 MIRV/Hydro/Atom = true) route the
//! save-up ladder to cost(SAMLauncher) and skip SAMLauncher/MissileSilo in
//! buildOrder without an unitsOwned read, and cityCount 4 x (0.75 x 0.33) =
//! 0.99 floors to target 0 so Factory is suppressed — any weaker multiplier
//! would build it — falling through to the City fallback (307/24 "City").
//! Its mg 45 FIFO is the first non-uniform script and fixed the read-order
//! model: isInPostSaveUpBlockedPhase runs BEFORE doHandleStructures and
//! consumes MissileSilo twice plus the ladder triple, stamping
//! _postSaveUpStartTick when gold >= saveUpTarget even in FFA.
//! `el_struct_silo_cap_skip` (B12) pins the MAX_MISSILE_SILOS cap: with no
//! coastal tiles (mg 129 null) Port is skipped without an owned read,
//! Factory (owned 2 >= target 2) and SAM (target floor(3 x 0.25) = 0) skip
//! on ratio, MissileSilo returns false on unitsOwned 3 >= 3 before the
//! FIRST_MISSILE_SILO_RATIO override can apply, and the City fallback
//! builds City #4 through the full cityValue pipeline.
//! G5p-E completes the difficulty matrix and the upgrade family. Changing
//! difficulty re-derives the whole gate schedule because getAttackRate draws
//! from a difficulty-specific range — Medium nextInt(55,70) and Impossible
//! nextInt(30,50) on the seed-6702 stream yield rate 61 / attackTick 49 /
//! offsets 8+28 and rate 38 / attackTick 30 / offsets 4+17 respectively
//! (verified offline against the recorded Hard 51/41/7+24), so the scenario
//! ticks move with the matrix. `el_struct_dp_medium_chance` (B13) replays the
//! B3 defense-post shape under Medium: the `!random.chance(2)` entry roll
//! (nextInt(0,2) === 0) is the only new consumption, `allowed` is the flat 1
//! instead of ceil(ratio/0.4), the SAM-first branch is unreachable
//! (Hard/Impossible only), and the recorded stream hits the roll — tick 150
//! builds City #1, tick 211 builds the Defense Post as 307/24.
//! `el_struct_sam_ratio_impossible` (B14) pins SAM_RATIO_BY_DIFFICULTY by
//! outcome: cityCount 7 with SAM owned 1 would skip under Hard
//! (floor(7 x 0.25) = 1) but Impossible's 0.3 floors to 2, so the SAM builds
//! through the normal build order at tick 118; samLauncherValue arms
//! weightByLevel on Impossible and the coverage roll hits, adding the extra
//! units(SAMLauncher) existingSams pass. `el_struct_factory_upgrade_density`
//! (B15) takes a Factory through the density-triggered upgrade branch
//! (density 1/1000 > 1/1500): mg 47 upgradable is read, maybeUpgradeStructure
//! re-runs the density check, and with no SAMs the Hard 25% random-pick roll
//! misses into the tie-break scoring fallback — both converge on unit 9 and
//! emit 307/25 with no tile-scan consumption. `el_struct_upgrade_blocked_chain`
//! (B16) blocks the same branch via canUpgradeUnit false: the upgradable
//! filter empties before the samLaunchers read, the
//! structures.length > 0 guard refuses the new build, and the chain repeats
//! through the City fallback (density 3/1000, canUpgradeUnit per structure) —
//! a pure deny tick with no 307 event at all.
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
//! `ShellExecution` is intentionally NOT ported; its construction sites are
//! the unreachable `DefensePost.shoot` and `WarshipExecution.shootTarget` (G5j),
//! so it is modelled by `mg_add_execution_shell` (tag 1) — exercised by the
//! G5j warship scenarios.
//! `mg.addExecution(new TrainStationExecution(unit, spawnTrains?))` (issued by
//! the ported `City` / `Factory` executions) is modelled as an `addExecution`
//! facade event (code 307, tag 0) — exactly how `execution_manager.rs` records
//! `new XxxExecution(...)`. On the capture side we import the REAL
//! `TrainStationExecution` class: its ctor runs against the scripted unit mock
//! and therefore emits `unit.setTrainStation(true)` FIRST (code 351), then the
//! `addExecution` record — mirrored verbatim on the Rust side
//! (`mg_add_train_station` calls `un_set_train_station` then pushes the
//! record). Since G5i ports `TrainStationExecution` itself (class 39), the
//! class-39 construct op (kind 0) drops that ctor trace like every other class
//! (res `[0, 0]`).
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
//! ## G5h facade additions (codes ≥ 900)
//!
//! `mg`: `900` config().tradeShipGold(tilesTraveled, owner), `901` isShoreline(tile)
//! (BOOL), `905` `WaterPathFinder.pathForTraversal(from, to)` (`TileRef[]`, through
//! the stub's `__waterPathFinder`). Void (`mg.stats()`): `902` boatSendTrade(player,
//! target), `903` boatCapturedTrade(player, originalOwner, gold), `904`
//! boatArriveTrade(src, dst, gold). Void (`player`): `910` addGold(gold, tile),
//! `911` addPiracyGold(gold), `912` addTradeGold(gold) — each prefixed by the
//! receiver token like every player facade event. `unit`: `913`
//! isMarkedForDeletion() (BOOL, slot 49); void `914` setTargetUnit(unit), `915`
//! setSafeFromPirates(). `gold` is a plain number in the mocks. No new construction
//! tag: `TradeShipExecution` spawns a *unit* via the `buildUnit` facade (`535`),
//! and its own downstream record tag `12` already existed.
//!
//! ## G5i facade additions (codes ≥ 916)
//!
//! `mg`: void `916` `railNetwork().connectStation(station)` (prefix `[3, utok]`);
//! `917` `railNetwork().findStationsPath(from, to)` (mg slot 97, `TileRef[]` of
//! station unit tokens or null); `918` `config().trainSpawnRate(factories,
//! trains)` (mg slot 101). `station` (the new `stations` mock category, indexed
//! by unit token; the raw unit token is the event prefix like the `unit`
//! facade): slot 0 `920` `getCluster()` (a `clusters` table index or null),
//! slot 1 `921` `getRailroadTo(neighbor)` (a `railroads` table index or null —
//! FIFO scripted so `resolveSplitRailroad` can flip a pair to null), void `922`
//! `onTrainStop()`. `923` / `924` are the cluster wrappers
//! `hasAnyTradeDestination(owner)` / `randomTradeDestination(owner, random)`:
//! they push `[923|924, ctok, ...evtok(owner)]` BEFORE the embedded
//! `tradeAvailable` events (`286` + short-circuited `533`), then the encoded
//! result (bool / station unit token or null). `unit`: void `927`
//! `setLoaded(flag)`. The downstream `mg.addExecution(new TrainExecution(...))`
//! record is tag `17` `(player, sourceUtok, destUtok, numCars)`.
//!
//! ## G5j facade additions (codes ≥ 928)
//!
//! `unit`: `928` warshipState() (returns `{state, patrolTile, retreatPort?}`);
//! void `929` setPatrolTile(tile), `930` setWarshipState(state), `931`
//! setRetreatPort(tile), `932` setAlreadySentShell(target), `933`
//! setAlreadySentShellClear(), `935` setDocked(flag), `936` setHealth(health),
//! `937` setPatrolling(), `938` setLastShellAttack(tick), `939`
//! setAlreadySentShellAdd(target). `940` `owner().inDoomsdayClock()` (player
//! facade, slot 63). `player`: void `941` captureUnit(unit), `942`
//! removeTroops(n), `943` recordDeathPosition(place), `944` recordKill(),
//! `945` recordDeath(), `946` addTroops(n), `947` addGold(gold), `948`
//! stopEmbargo(target), `949` decayRelations(), `950` removeGold(gold), `951`
//! removeAllAlliances(). `952` lastTileChange(), `953` largestClusterBoundingBox
//! (setter, `null` or `{min, max}`), `954` alliances(), `955` getEmbargoes(),
//! `956` allianceExpiresAt(alliance), `957` allianceExpire(alliance).
//!
//! ## G5k facade additions (codes ≥ 958)
//!
//! `mg`: `958` config().disableAlliances() (BOOL, slot 115), `959`
//! config().donateTroops() (BOOL, slot 116), `960`
//! config().gameConfig().playerTeams (slot 117), `967` isShore(tile) (BOOL,
//! slot 118), `961` `__closestShoreByWater(player, tile)` (slot 119, the
//! `__stub/g5k.ts` canBuildTransportShip stub; returns a shore tile or null).
//! `player`: `962`
//! sharesBorderWith(other) (BOOL, slot 67), `963` targets() (slot 68, player
//! tokens), `964` allies() (slot 69, player tokens), `965`
//! allRelationsSorted() (slot 70, `{relation, player}` entries). `alliance`:
//! slot 5 `966` other(player) — the other member's player token (the event
//! prefix is `[al_tok, ...evtok(player)]` like every facade). `neighbor`: the
//! harness delegates the player accessors (`type` / `troops` / `isTraitor` /
//! `id` / `units` / `numTilesOwned`) to the underlying player mock, so those
//! traces land on the `player` codes (neighbor tokens ARE player tokens).
//! New construction-record tags (code 307): `18` `TransportShipExecution`
//! `(attacker, ref, troops)`, `19` `DonateTroopsExecution` `(sender,
//! recipientID, troops)`, `20` `AllianceExtensionExecution` `(from, toID)`,
//! `21` `DeleteUnitExecution` `(player, unitId)`. The `TerraNullius` singleton
//! only traces `id()` (`811`, always null); its `isPlayer()` / `smallID()` are
//! untraced constants (the Rust side branches on the TN token).
//!
//! ## G5k Step 2 facade additions (codes ≥ 968, class 44 `NationExecution`)
//!
//! `mg`: `968` `units(...types)` the typed form (slot 120, unit tokens), `969`
//! `unitInfo(ty).cost(game, player)` (slot 121, `[evs(ty), evtok(player)]`),
//! `970` `config().startingGold(info)` (slot 122, `[encVal(info)]`), `971`
//! `nations()` (slot 123, player tokens), `972` `isOnMap({x, y})` (slot 124,
//! `[encVal(cell)]`), `973` `config().trainStationMinRange()` (slot 125), `974`
//! `config().samRange(level)` (slot 126, `[ev1(level)]`), `975`
//! `config().defaultSamRange()` (slot 127), `976` `config().trainGold(kind,
//! troops, player)` (slot 128, `[evs(kind), ev1(troops), evtok(player)]`), `977`
//! `sharedWaterComponents(player)` (slot 129, a component-id list or null),
//! `978` `getWaterComponentSize(tile)` (slot 130), `979`
//! `railNetwork().stationManager().getAll()` (slot 131, unit tokens — station
//! identity is the unit token). Grid-delegated when `spec.map` exists (the
//! trace records the computed value): `980` `magnitude(tile)`, `981`
//! `isOcean(tile)`. `player`: `982` `unitsConstructed(ty)` (BOOL, slot 71),
//! `983` `unitsOwned(ty)` (slot 72), `984` `info()` (slot 73, a plain
//! PlayerInfo object). `unit`: `985` `wasDestroyedByEnemy()` (BOOL, slot 52),
//! `986` `destroyer()` (slot 53, a unit token or null). New construction-record
//! tags (code 307): `22` `SpawnExecution` `(gameID, playerInfo, tile?)`, `23`
//! `AllianceRequestExecution` `(requestor, recipientID)`, `24`
//! `ConstructionExecution` `(player, constructionType, tile,
//! rocketDirectionUp?, amount?)`, `25` `UpgradeStructureExecution` `(player,
//! unitId)`. `hasUnitNearby`'s optional 5th argument
//! (`includeUnderConstruction`) is NOT traced (the mock ignores it).
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
use crate::exec_util::closest_two_tiles_w;
use crate::js_json::{push_str, push_val, read_val, val_field, JsVal};
use crate::jsnum::{js_max, js_min, js_round};
use crate::pathfinding::flat_heap::FlatBinaryHeap;
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
const UNIT_TRAIN: &str = "Train";
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
// G5g downstream-execution tag (`mg.addExecution(new AttackExecution(...))`).
const TAG_ATTACK_EXEC: f64 = 16.0;
// G5i downstream-execution tag (`mg.addExecution(new TrainExecution(...))`).
const TAG_TRAIN_EXEC: f64 = 17.0;
// G5k downstream-execution tags (`mg.addExecution(new XxxExecution(...))`).
const TAG_TRANSPORT_SHIP_EXEC: f64 = 18.0;
const TAG_DONATE_TROOPS_EXEC: f64 = 19.0;
const TAG_ALLIANCE_EXTENSION_EXEC: f64 = 20.0;
const TAG_DELETE_UNIT_EXEC: f64 = 21.0;
// G5k Step 2 downstream-execution tags (`NationExecution` / nation behaviors).
const TAG_SPAWN_EXEC: f64 = 22.0;
const TAG_ALLIANCE_REQUEST_EXEC: f64 = 23.0;
const TAG_CONSTRUCTION_EXEC: f64 = 24.0;
#[allow(dead_code)]
const TAG_UPGRADE_STRUCTURE_EXEC: f64 = 25.0;

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
/// `GameMode.Team`.
const GAME_MODE_TEAM: &str = "Team";
/// `GameType.Public`.
const GAME_TYPE_PUBLIC: &str = "Public";
/// `HumansVsNations` sentinel (`Game.ts`).
const HUMANS_VS_NATIONS: &str = "Humans Vs Nations";
/// `Relation` numeric enum members (`Game.ts`).
const RELATION_HOSTILE: f64 = 0.0;
#[allow(dead_code)]
const RELATION_DISTRUSTFUL: f64 = 1.0;
const RELATION_NEUTRAL: f64 = 2.0;
const RELATION_FRIENDLY: f64 = 3.0;
/// `MAX_NATION_SILO_UPGRADE_LEVEL` (NationNukeBehavior.ts) — silo-level cap
/// for the `maybeDestroyEnemySam` upgrade fallback.
const NN_MAX_NATION_SILO_UPGRADE_LEVEL: f64 = 5.0;
/// `HIGH_DENSITY_NUKE_THRESHOLD` — level-weighted structure density above
/// which the richest impossible nation pre-emptively nukes.
const NN_HIGH_DENSITY_NUKE_THRESHOLD: f64 = 1.0 / 75.0;
/// `MIN_LEVEL_SUM_FOR_HIGH_DENSITY_NUKE`.
const NN_MIN_LEVEL_SUM_FOR_HIGH_DENSITY_NUKE: f64 = 5.0;
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

// G5g (`AttackExecution` class 36 / `TransportShipExecution` class 37).
/// `UnitType.DefensePost` (`attackLogicInput`'s defense-post probe).
const UNIT_DEFENSE_POST: &str = "Defense Post";
/// `UnitType.TransportShip` (`TransportShipExecution`).
const UNIT_TRANSPORT_SHIP: &str = "Transport";
/// G5g `MessageType` numeric enum members.
const MSG_ATTACK_FAILED: f64 = 0.0;
const MSG_ATTACK_CANCELLED: f64 = 1.0;
const MSG_NAVAL_INVASION_INBOUND: f64 = 8.0;
/// `TerrainType` numeric enum members (`Game.ts`).
const TERRAIN_PLAINS: f64 = 0.0;
const TERRAIN_HIGHLAND: f64 = 1.0;
const TERRAIN_MOUNTAIN: f64 = 2.0;
/// `Difficulty` string enum members (`Game.ts`).
const DIFFICULTY_EASY: &str = "Easy";
const DIFFICULTY_MEDIUM: &str = "Medium";
const DIFFICULTY_HARD: &str = "Hard";
const DIFFICULTY_IMPOSSIBLE: &str = "Impossible";
/// `NationMIRVBehavior.MIRV_COOLDOWN_TICKS` (30 seconds at 10 ticks/second).
const MIRV_COOLDOWN_TICKS: f64 = 300.0;
/// `AttackExecution.malusForRetreat` / `TransportShipExecution.malusForRetreat`.
const ATTACK_RETREAT_MALUS: f64 = 25.0;
/// `AttackExecution.handleDeadDefender` MAX_PASSES / tile threshold.
const AT_MAX_PASSES: f64 = 100.0;
const AT_DEAD_DEFENDER_TILES: f64 = 100.0;
/// `PathStatus.NOT_FOUND` (`src/core/pathfinding/types.ts`).
const PATH_STATUS_NOT_FOUND: f64 = 3.0;
/// The `TerraNullius` sentinel token. Its `.id()` is always `null`, so the Rust
/// side branches on this token instead of a `players` table entry.
const TN_TOK: usize = 1_000_000;
/// `TransportShipExecution.ticksPerMove` / `TransportShipExecution` motion-plan
/// `startTick` uses `ticks + ticksPerMove`.
const TS_TICKS_PER_MOVE: f64 = 1.0;
/// `MessageType.CAPTURED_ENEMY_UNIT` (numeric enum member 11) — G5h
/// (`TradeShipExecution.complete`).
const MSG_CAPTURED_ENEMY_UNIT: f64 = 11.0;

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

/// JS strict equality (`===`) for the scalar [`JsVal`] shapes the mock facades
/// produce (team ids are strings or `null`; numbers compare like JS, so
/// `NaN === NaN` is false).
fn js_val_eq(a: &JsVal, b: &JsVal) -> bool {
    match (a, b) {
        (JsVal::Str(x), JsVal::Str(y)) => x == y,
        (JsVal::Num(x), JsVal::Num(y)) => x == y,
        (JsVal::Bool(x), JsVal::Bool(y)) => x == y,
        (JsVal::Null, JsVal::Null) => true,
        (JsVal::Undef, JsVal::Undef) => true,
        _ => false,
    }
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

/// JS truthiness of a plain number (`0` and `NaN` are falsy).
fn js_truthy(n: f64) -> bool {
    n != 0.0 && !n.is_nan()
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

/// G5j: grid-backed `GameMap` mock (the `elMapGrid` TS equivalent). Built from
/// `spec.map = {w, h, terrain[], owner[]}` where `terrain[i]` is the GameMapImpl
/// terrain byte (bit 7 = land, bit 6 = shoreline, bit 5 = ocean, bits 0-4 =
/// magnitude) and `owner[i]` is the owner smallID (0 = unowned).
#[derive(Clone, Debug, Default)]
struct ElMap {
    w: f64,
    h: f64,
    terrain: Vec<f64>,
    owners: Vec<f64>,
}

const EL_MAP_IS_LAND: u8 = 1 << 7;
const EL_MAP_SHORELINE: u8 = 1 << 6;
const EL_MAP_OCEAN: u8 = 1 << 5;
const EL_MAP_MAG_MASK: u8 = 0x1f;
const EL_MAP_IMPASSABLE_MAG: u8 = 31;

impl ElMap {
    fn from_val(v: &JsVal) -> Self {
        let w = val_field(v, "w").map(as_num).unwrap_or(0.0);
        let h = val_field(v, "h").map(as_num).unwrap_or(0.0);
        let terrain = val_field(v, "terrain").map(as_arr_nums).unwrap_or_default();
        let owners = val_field(v, "owner").map(as_arr_nums).unwrap_or_default();
        Self { w, h, terrain, owners }
    }
    fn x(&self, r: f64) -> f64 { r % self.w }
    fn y(&self, r: f64) -> f64 { (r / self.w).floor() }
    fn is_land(&self, r: f64) -> bool { self.terrain[r as usize] as u8 & EL_MAP_IS_LAND != 0 }
    fn is_ocean(&self, r: f64) -> bool { self.terrain[r as usize] as u8 & EL_MAP_OCEAN != 0 }
    fn is_shoreline(&self, r: f64) -> bool { self.terrain[r as usize] as u8 & EL_MAP_SHORELINE != 0 }
    fn is_shore(&self, r: f64) -> bool { self.is_land(r) && self.is_shoreline(r) }
    fn is_ocean_shore(&self, r: f64) -> bool {
        if !self.is_land(r) { return false; }
        let x = self.x(r);
        let w = self.w;
        let h = self.h;
        if x != 0.0 && self.is_ocean(r - 1.0) { return true; }
        if x != w - 1.0 && self.is_ocean(r + 1.0) { return true; }
        if r >= w && self.is_ocean(r - w) { return true; }
        if r < (h - 1.0) * w && self.is_ocean(r + w) { return true; }
        false
    }
    fn is_on_edge_of_map(&self, r: f64) -> bool {
        let x = self.x(r);
        let w = self.w;
        let h = self.h;
        if x == 0.0 || x == w - 1.0 || r < w || r >= (h - 1.0) * w { return true; }
        self.is_impassable(r - 1.0) || self.is_impassable(r + 1.0)
            || self.is_impassable(r - w) || self.is_impassable(r + w)
    }
    fn is_impassable(&self, r: f64) -> bool {
        self.is_land(r) && (self.terrain[r as usize] as u8 & EL_MAP_MAG_MASK) == EL_MAP_IMPASSABLE_MAG
    }
    fn owner_id(&self, r: f64) -> f64 { self.owners[r as usize] }
    fn magnitude(&self, r: f64) -> f64 {
        (self.terrain[r as usize] as u8 & EL_MAP_MAG_MASK) as f64
    }
    fn set_owner_id(&mut self, r: f64, pid: f64) { self.owners[r as usize] = pid; }
    fn neighbors4(&self, r: f64, buf: &mut [f64; 4]) -> usize {
        let x = self.x(r);
        let w = self.w;
        let h = self.h;
        let mut n = 0;
        if r >= w { buf[n] = r - w; n += 1; }
        if r < (h - 1.0) * w { buf[n] = r + w; n += 1; }
        if x != 0.0 { buf[n] = r - 1.0; n += 1; }
        if x != w - 1.0 { buf[n] = r + 1.0; n += 1; }
        n
    }
    fn neighbors8(&self, r: f64, buf: &mut [f64; 8]) -> usize {
        let x = self.x(r);
        let w = self.w;
        let h = self.h;
        let has_n = r >= w;
        let has_s = r < (h - 1.0) * w;
        let mut n = 0;
        if x != 0.0 {
            if has_n { buf[n] = r - 1.0 - w; n += 1; }
            buf[n] = r - 1.0; n += 1;
            if has_s { buf[n] = r - 1.0 + w; n += 1; }
        }
        if has_n { buf[n] = r - w; n += 1; }
        if has_s { buf[n] = r + w; n += 1; }
        if x != w - 1.0 {
            if has_n { buf[n] = r + 1.0 - w; n += 1; }
            buf[n] = r + 1.0; n += 1;
            if has_s { buf[n] = r + 1.0 + w; n += 1; }
        }
        n
    }
}

/// G5j: `TileTraversalScratch` equivalent — generation-stamped visited array
/// plus a reusable stack, so BFS/DFS passes over the map allocate nothing per
/// query.
#[derive(Clone, Debug, Default)]
struct ElScratch {
    visited: Vec<u32>,
    stack: Vec<f64>,
    cluster_index_map: Vec<i32>,
    gen: u32,
}

impl ElScratch {
    fn ensure_capacity(&mut self, total_tiles: usize) {
        if self.visited.len() < total_tiles {
            self.visited = vec![0u32; total_tiles];
            self.stack = Vec::new();
            self.cluster_index_map = vec![0i32; total_tiles];
            self.gen = 0;
        }
    }
    fn bump_generation(&mut self) -> u32 {
        self.gen = self.gen.wrapping_add(1);
        if self.gen == u32::MAX {
            self.visited.fill(0);
            self.gen = 1;
        }
        self.gen
    }
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

    // G5g state (`AttackExecution`, class 36).
    at_start_troops: Option<f64>,
    at_target_id: Option<String>,
    at_source_tile: Option<f64>,
    at_remove_troops: bool,
    at_target: Option<usize>,
    at_attack: Option<usize>,
    at_owner_small_id: f64,
    at_target_small_id: f64,
    at_to_conquer: FlatBinaryHeap,
    at_active: bool,

    // G5g state (`TransportShipExecution`, class 37).
    ts_attacker: usize,
    ts_ref: f64,
    ts_troops: Option<f64>,
    ts_last_move: f64,
    ts_target: Option<usize>,
    ts_dst: Option<f64>,
    ts_src: Option<f64>,
    ts_retreat_dst: RetDst,
    ts_boat: Option<usize>,
    ts_motion_plan_id: f64,
    ts_motion_plan_dst: Option<f64>,
    ts_original_owner: usize,
    ts_active: bool,

    // G5h state (`TradeShipExecution`, class 38).
    tds_orig_owner: usize,
    tds_src_port: Option<usize>,
    tds_dst_port: Option<usize>,
    tds_trade_ship: Option<usize>,
    tds_was_captured: bool,
    tds_active: bool,
    tds_tiles_traveled: f64,
    tds_motion_plan_id: f64,
    tds_motion_plan_dst: Option<f64>,

    // G5i state (class 39 `TrainStationExecution`).
    tss_active: bool,
    tss_station: Option<usize>,
    tss_num_cars: f64,
    tss_last_spawn_tick: f64,
    tss_ticks_cooldown: f64,
    tss_spawn_trains: Option<bool>,
    tss_random: Option<PseudoRandom>,

    // G5i state (class 40 `TrainExecution`).
    tex_active: bool,
    tex_train: Option<usize>,
    tex_cars: Vec<usize>,
    tex_has_cargo: bool,
    tex_current_tile: f64,
    tex_used_tiles: Vec<f64>,
    tex_stations: Vec<usize>,
    tex_current_railroad: Option<(usize, Vec<f64>)>,
    tex_speed: f64,
    tex_spacing: f64,
    tex_trade_stops: f64,
    tex_path_tiles: Vec<f64>,
    tex_path_index: f64,
    tex_source: usize,
    tex_destination: usize,
    tex_num_cars: f64,

    // G5j state (class 41 `WarshipExecution`).
    ws_warship: Option<usize>,
    last_manual_move_tick_retreat_disabled: f64,
    last_observed_patrol_tile: Option<f64>,
    active_healing_remainder: f64,
    last_emitted_combat: bool,
    ws_random: Option<PseudoRandom>,
    last_shell_attack: f64,
    already_sent_shell: Vec<usize>,

    // G5j state (class 42 `PlayerExecution`).
    pe_player: usize,
    last_calc: f64,
    pe_active: bool,
    nbuf: [f64; 4],
    nbuf8: [f64; 8],
    el_map: Option<ElMap>,
    el_scratch: ElScratch,

    // G5k state (class 43 `TribeExecution` + `AiAttackBehavior`).
    te_tribe: usize,
    te_attack_rate: f64,
    te_attack_tick: f64,
    te_trigger_ratio: f64,
    te_reserve_ratio: f64,
    te_expand_ratio: f64,
    te_neighbors_tn: bool,
    te_attack_behavior: bool,
    te_bot_attack_troops_sent: f64,
    te_nbuf: [f64; 4],
    /// True when the `te_tribe` token is a `NationExecution` player: the
    /// `ai_*` (AiAttackBehavior) family routes the optional `allianceBehavior`
    /// / `emojiBehavior` hooks to the `ne_*` nation methods instead of throwing
    /// (the TS fields are `undefined` for tribes).
    te_is_nation: bool,

    // G5k Step 2 state (class 44 `NationExecution` + the seven nation
    // behaviors). `te_tribe` doubles as the nation player token so the whole
    // `ai_*` (AiAttackBehavior) family runs unchanged; `te_trigger_ratio` /
    // `te_reserve_ratio` / `te_expand_ratio` carry the ctor draws.
    ne_player: Option<usize>,
    ne_initialized: bool,
    ne_behaviors_initialized: bool,
    ne_spawn_exec_added: bool,
    ne_attack_rate: f64,
    ne_attack_tick: f64,
    ne_nation_id: String,
    ne_nation_name: String,
    ne_player_info: JsVal,
    ne_spawn_cell: Option<(f64, f64)>,
    ne_embargo_malus_applied: Vec<String>,
    // `NationEmojiBehavior` state.
    ne_last_emoji_sent: Vec<(usize, f64)>,
    ne_game_over: bool,
    // `NationWarshipBehavior` tracked unit sets (insertion order, tokens).
    ne_tracked_transport: Vec<usize>,
    ne_tracked_trade: Vec<usize>,
    ne_tracked_incoming: Vec<usize>,
    ne_dealt_with: Vec<usize>,
    // `NationMIRVBehavior` state (the static `recentMirvTargets` map keyed by
    // player id; one instance per scenario so a field is equivalent).
    ne_recent_mirv: Vec<(String, f64)>,
    // `NationNukeBehavior` state.
    ne_recently_sent_nukes: Vec<(f64, f64, String)>,
    ne_atom_bombs_launched: f64,
    ne_atom_perceived_cost: i64,
    ne_hydro_bombs_launched: f64,
    ne_hydro_perceived_cost: i64,
    ne_is_hydro_nation: bool,
    // `NationStructureBehavior` state.
    ne_placements_count: f64,
    ne_built_crowded: bool,
    ne_last_structure_tick: Option<f64>,
    ne_has_high_starting_gold: Option<bool>,
    ne_post_save_up_start_tick: Option<f64>,
    ne_shared_water_components: Option<Vec<f64>>,
    /// `NationStructureBehavior.reachableStationsCache` — trade-weighted
    /// station entries `(tile, cluster, weight)` for connectivity scoring;
    /// cleared at the top of every `doHandleStructures()`.
    ne_reachable_stations_cache: Option<Vec<(f64, Option<usize>, f64)>>,

    mg: Mock,
    players: Vec<Mock>,
    alliances: Vec<Mock>,
    requests: Vec<Mock>,
    units: Vec<Mock>,
    neighbors: Vec<Mock>,
    attacks: Vec<Mock>,
    stations: Vec<Mock>,
    el_clusters: Vec<Vec<usize>>,
    el_railroads: Vec<ElRailroad>,
}

/// G5i railroad table entry (`tools/gen_vectors.mjs` `railroads` table).
#[derive(Clone, Debug, Default)]
struct ElRailroad {
    #[allow(dead_code)]
    from_utok: usize,
    to_utok: usize,
    tiles: Vec<f64>,
}

/// G5l: precomputed state for one `NationStructureBehavior.structureSpawnTileValue()`
/// closure. The TS closures capture their outer function's locals once; the
/// per-tile body is `ne_value_at` reading this context.
#[derive(Clone, Debug, Default)]
struct NeSpawnValueCtx {
    /// 0 = city, 1 = missile silo, 2 = factory, 3 = port, 4 = SAM launcher.
    kind: u8,
    /// `player.borderTiles()` (635) — empty for port (its closure reads none).
    border_tiles: Vec<f64>,
    /// Same set as `border_tiles` keyed by integer ref for the ring search.
    border_set: std::collections::HashSet<i64>,
    /// `nukeMagnitudes(AtomBomb).outer` (224).
    border_spacing: f64,
    /// `border_spacing * 2`.
    structure_spacing: f64,
    /// Same-type `player.units(type)` tokens (241); `tile()` (344) is re-read
    /// per closure call like the TS `new Set(otherUnits.map(u => u.tile()))`.
    other_units: Vec<usize>,
    /// Factory/city: `trainStationMaxRange()` (309) and its square.
    station_range: f64,
    station_range_sq: f64,
    /// Factory/city: `shouldUseConnectivityScore()` draw result.
    use_conn: bool,
    /// Factory/city: cached reachable stations (976/979/920/241/344/245/…).
    reachable: Vec<(f64, Option<usize>, f64)>,
    /// Factory/city: `trainStationMinRange() ** 2` (973).
    min_range_sq: f64,
    /// Factory: city tiles; city: factory tiles (241 + 344 each, deduped).
    cross_tiles: Vec<f64>,
    /// SAM: protectable entries `(tile, weight)` (241 no-arg + 283/344/540).
    protect: Vec<(f64, f64)>,
    /// SAM: `defaultSamRange() ** 2` (975).
    range_sq: f64,
    /// SAM: `difficulty !== Easy && nextInt(0, 100) < 25`.
    use_coverage: bool,
    /// SAM: coverage score per protectable tile (540/974/344/710).
    coverage: Vec<(f64, f64)>,
    /// `difficulty !== Easy` (SAM closure gate).
    not_easy: bool,
}

/// `TransportShipExecution.retreatDst: TileRef | false | null`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum RetDst {
    #[default]
    Null,
    False,
    Tile(f64),
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
// G5g mg slots (`AttackExecution` / `TransportShipExecution`). `__waterPathFinder`
// and `__targetTransportTile` are nested facade objects that index into the same
// mg slot array (the capture's `elSlots` extends on out-of-range keys).
const MG_HAS_FALLOUT: usize = 8;
const MG_FOR_EACH_NEIGHBOR: usize = 16;
const MG_TERRAIN_TYPE: usize = 26;
const MG_ATTACK_LOGIC: usize = 27;
const MG_NEIGHBORS4: usize = 39;
const MG_DEFENSE_POST_RANGE: usize = 58;
const MG_ATTACK_AMOUNT: usize = 62;
const MG_BOAT_MAX_NUMBER: usize = 90;
const MG_BOAT_ATTACK_AMOUNT: usize = 91;
const MG_WATER_REBUILT: usize = 860;
const MG_WATER_FIND_PATH: usize = 861;
const MG_WATER_NEXT: usize = 862;
const MG_TARGET_TRANSPORT_TILE: usize = 880;
// G5h mg slots (`TradeShipExecution`). `config().tradeShipGold` shares the mg
// mock's slot table (as with `attackLogic`); `isShoreline` is `mg.isShoreline`
// and `pathForTraversal` reaches the stub `WaterPathFinder`.
const MG_TRADE_SHIP_GOLD: usize = 92;
const MG_IS_SHORELINE: usize = 93;
const MG_WATER_PATH_FOR_TRAVERSAL: usize = 96;
// G5i mg slots (`TrainStationExecution` / `TrainExecution`). `findStationsPath`
// reaches the scripted `RailNetwork` facade object; `trainSpawnRate` is
// `mg.config().trainSpawnRate(factoryCount, trainCount)`.
const MG_FIND_STATIONS_PATH: usize = 97;
const MG_TRAIN_SPAWN_RATE: usize = 101;
// G5j mg slots (`WarshipExecution` / `PlayerExecution`).
const MG_WARSHIP_PASSIVE_HEALING: usize = 102;
const MG_WARSHIP_PASSIVE_HEALING_RANGE: usize = 103;
const MG_WARSHIP_RETREAT_HEALTH_PERCENT: usize = 104;
const MG_WARSHIP_TARGETTING_RANGE: usize = 105;
const MG_WARSHIP_PATROL_RANGE: usize = 106;
const MG_WARSHIP_DOCKING_RANGE: usize = 107;
const MG_WARSHIP_PORT_HEALING_BONUS: usize = 108;
const MG_WARSHIP_PORT_SWITCH_THRESHOLD: usize = 109;
const MG_WARSHIP_SHELL_ATTACK_RATE: usize = 110;
const MG_PERF_NOW: usize = 111;
const MG_TROOP_INCREASE_RATE: usize = 112;
const MG_GOLD_ADDITION_RATE: usize = 113;
const MG_TEMPORARY_EMBARGO_DURATION: usize = 114;
// G5k mg slots (`TribeExecution` / `AiAttackBehavior`).
const MG_DISABLE_ALLIANCES: usize = 115;
const MG_DONATE_TROOPS_CFG: usize = 116;
const MG_PLAYER_TEAMS: usize = 117;
const MG_IS_SHORE: usize = 118;
const MG_CLOSEST_SHORE_BY_WATER: usize = 119;
// G5k Step 2 mg slots (class 44 `NationExecution` + nation behaviors).
const MG_UNITS_TYPED: usize = 120;
const MG_UNIT_INFO_COST: usize = 121;
const MG_STARTING_GOLD: usize = 122;
const MG_NATIONS: usize = 123;
const MG_IS_ON_MAP: usize = 124;
const MG_TRAIN_STATION_MIN_RANGE: usize = 125;
const MG_SAM_RANGE: usize = 126;
const MG_DEFAULT_SAM_RANGE: usize = 127;
const MG_TRAIN_GOLD: usize = 128;
const MG_SHARED_WATER_COMPONENTS: usize = 129;
const MG_GET_WATER_COMPONENT_SIZE: usize = 130;
const MG_STATION_MANAGER_GET_ALL: usize = 131;
const MG_MAGNITUDE: usize = 132;
const MG_IS_OCEAN: usize = 133;

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
// G5g player slots (`AttackExecution` / `TransportShipExecution`).
const PL_REMOVE_TROOPS: usize = 55;
const PL_CAN_ATTACK_PLAYER: usize = 56;
const PL_CREATE_ATTACK: usize = 57;
const PL_INCOMING_ATTACKS: usize = 58;
const PL_UNIT_COUNT: usize = 59;
const PL_BEST_TRANSPORT_SHIP_SPAWN: usize = 61;
const PL_IS_TRAITOR: usize = 62;
// G5j player slots (`WarshipExecution` / `PlayerExecution`).
const PL_IN_DOOMSDAY_CLOCK: usize = 63;
const PL_LAST_TILE_CHANGE: usize = 64;
const PL_ALLIANCES: usize = 65;
const PL_GET_EMBARGOES: usize = 66;
// G5k player slots (`AiAttackBehavior`).
const PL_SHARES_BORDER_WITH: usize = 67;
const PL_TARGETS: usize = 68;
const PL_ALLIES: usize = 69;
const PL_ALL_RELATIONS_SORTED: usize = 70;
// G5k Step 2 player slots (`NationExecution` / `NationStructureBehavior`).
const PL_UNITS_CONSTRUCTED: usize = 71;
const PL_UNITS_OWNED: usize = 72;
const PL_INFO: usize = 73;
const PL_IS_ALLIED_WITH: usize = 74;

// alliance slot indices.
const AL_ONLY_ONE_AGREED: usize = 0;
const AL_BOTH_AGREED: usize = 2;
// G5j alliance slot (`PlayerExecution.tick` alliance expiry).
const AL_EXPIRES_AT: usize = 4;
// G5k alliance slot (`TribeExecution.acceptAllAllianceRequests`).
const AL_OTHER: usize = 5;

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
// G5g unit slot (`TransportShipExecution.tick`).
const UN_TRANSPORT_SHIP_STATE: usize = 48;
// G5h unit slot (`TradeShipExecution.tick` — `isMarkedForDeletion`; the other
// two unit facades are void and consume no slot).
const UN_IS_MARKED_FOR_DELETION: usize = 49;
// G5j unit slots (`WarshipExecution`).
const UN_WARSHIP_STATE: usize = 50;
const UN_IS_SAFE_FROM_PIRATES: usize = 51;
// G5k Step 2 unit slots (`NationWarshipBehavior.trackTransport`).
const UN_WAS_DESTROYED_BY_ENEMY: usize = 52;
const UN_DESTROYER: usize = 53;

// attack slot indices (`AttackExecution` facade).
const AT_TROOPS: usize = 0;
const AT_BORDER_SIZE: usize = 6;
const AT_RETREATED: usize = 7;
const AT_RETREATING: usize = 8;
const AT_IS_ACTIVE: usize = 9;
const AT_TARGET: usize = 10;
const AT_SOURCE_TILE: usize = 11;
const AT_ATTACKER: usize = 12;

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
    /// `mg.ownerID(tile)` — G5j: grid-delegated when a `spec.map` grid exists
    /// (the trace records the computed value instead of a script value, so both
    /// sides stay in sync through `conquer` / `conquerPlayer` mutations).
    fn mg_owner_id(&mut self, tr: &mut Vec<f64>, tile: f64) -> f64 {
        if let Some(ref map) = self.el_map {
            let v = map.owner_id(tile);
            tr.push(223.0);
            tr.extend_from_slice(&ev1(tile));
            tr.extend_from_slice(&ev1(v));
            return v;
        }
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
    /// `mg.addExecution(new ShellExecution(...))`.
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
    /// `player.id()` as the raw JS value — `TransportShipExecution` compares it
    /// against `mg.terraNullius().id()` (both may be `null`).
    fn pl_id_val(&mut self, tr: &mut Vec<f64>, tok: usize) -> JsVal {
        self.pl(tok, PL_ID, tr, 231.0, &[])
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
        if tok == TN_TOK {
            // `TerraNullius.isPlayer()` is an untraced constant `false`.
            return false;
        }
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
    /// G5g: `removeTroops` now RETURNS the amount actually deducted (`elRet`).
    fn pl_remove_troops(&mut self, tr: &mut Vec<f64>, tok: usize, n: f64) -> f64 {
        let v = self.pl(tok, PL_REMOVE_TROOPS, tr, 633.0, &ev1(n));
        as_num(&v)
    }
    fn pl_relinquish(&mut self, tr: &mut Vec<f64>, tok: usize, tile: f64) {
        self.pl_void(tok, tr, 636.0, &ev1(tile));
    }
    fn pl_conquer(&mut self, tr: &mut Vec<f64>, tok: usize, tile: f64) {
        self.pl_void(tok, tr, 637.0, &ev1(tile));
        // G5j: mutate the grid when a map is present.
        if self.el_map.is_some() {
            let sid = self.pl_peek_small_id(tok);
            if let (Some(map), Some(sid)) = (self.el_map.as_mut(), sid) {
                map.set_owner_id(tile, sid);
            }
        }
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

    // ---- G5i facade helpers (`TrainStationExecution` / `TrainExecution`) ----

    fn st(&mut self, utok: usize, slot: usize, tr: &mut Vec<f64>, code: f64, extra: &[f64]) -> JsVal {
        let mut prefix = vec![utok as f64];
        prefix.extend_from_slice(extra);
        mk_take(&mut self.stations[utok], slot, tr, code, &prefix)
    }
    fn st_void(&mut self, utok: usize, tr: &mut Vec<f64>, code: f64, extra: &[f64]) {
        let mut prefix = vec![utok as f64];
        prefix.extend_from_slice(extra);
        mk_void(tr, code, &prefix);
    }
    fn st_get_cluster(&mut self, tr: &mut Vec<f64>, utok: usize) -> Option<usize> {
        let v = self.st(utok, 0, tr, 920.0, &[]);
        as_opt_tok(&v)
    }
    fn st_get_railroad_to(&mut self, tr: &mut Vec<f64>, utok: usize, nb_utok: usize) -> Option<usize> {
        let v = self.st(utok, 1, tr, 921.0, &ev1(nb_utok as f64));
        as_opt_tok(&v)
    }
    fn st_on_train_stop(&mut self, tr: &mut Vec<f64>, utok: usize) {
        self.st_void(utok, tr, 922.0, &[]);
    }
    fn mg_connect_station(&mut self, tr: &mut Vec<f64>, utok: usize) {
        mk_void(tr, 916.0, &ev1(utok as f64));
    }
    fn mg_find_stations_path(&mut self, tr: &mut Vec<f64>, from: usize, to: usize) -> Option<Vec<usize>> {
        let mut prefix = ev1(from as f64);
        prefix.extend_from_slice(&ev1(to as f64));
        let v = mk_take(&mut self.mg, MG_FIND_STATIONS_PATH, tr, 917.0, &prefix);
        match v {
            JsVal::Null | JsVal::Undef => None,
            JsVal::Arr(items) => Some(items.iter().map(|x| as_num(x) as usize).collect()),
            _ => None,
        }
    }
    fn mg_train_spawn_rate(&mut self, tr: &mut Vec<f64>, factory_count: f64, train_count: f64) -> f64 {
        let mut prefix = ev1(factory_count);
        prefix.extend_from_slice(&ev1(train_count));
        let v = mk_take(&mut self.mg, MG_TRAIN_SPAWN_RATE, tr, 918.0, &prefix);
        as_num(&v)
    }
    fn un_set_loaded(&mut self, tr: &mut Vec<f64>, utok: usize, flag: bool) {
        let mut prefix = vec![utok as f64];
        prefix.extend_from_slice(&evb(flag));
        mk_void(tr, 927.0, &prefix);
    }
    fn station_trade_available(&mut self, tr: &mut Vec<f64>, station_utok: usize, other: usize) -> bool {
        let owner = self.un_owner(tr, station_utok).unwrap();
        if other == owner {
            true
        } else {
            self.pl_can_trade(tr, owner, Some(other))
        }
    }
    fn cluster_has_any_trade_dest(&mut self, tr: &mut Vec<f64>, ctok: usize, owner: usize) -> bool {
        tr.push(923.0);
        tr.push(ctok as f64);
        tr.extend_from_slice(&evtok(Some(owner)));
        let trade = self.el_clusters[ctok].clone();
        let mut result = false;
        for utok in trade {
            if self.station_trade_available(tr, utok, owner) {
                result = true;
                break;
            }
        }
        push_val(tr, &JsVal::Bool(result));
        result
    }
    fn cluster_random_trade_dest(&mut self, tr: &mut Vec<f64>, ctok: usize, owner: usize, random: &mut PseudoRandom) -> Option<usize> {
        tr.push(924.0);
        tr.push(ctok as f64);
        tr.extend_from_slice(&evtok(Some(owner)));
        let trade = self.el_clusters[ctok].clone();
        let mut selected: Option<usize> = None;
        let mut eligible_seen: i64 = 0;
        for utok in trade {
            if !self.station_trade_available(tr, utok, owner) {
                continue;
            }
            eligible_seen += 1;
            if random.next_int(0.0, eligible_seen as f64) == 0 {
                selected = Some(utok);
            }
        }
        push_val(tr, &match selected {
            Some(u) => JsVal::Num(u as f64),
            None => JsVal::Null,
        });
        selected
    }
    fn mg_add_train_execution(
        &mut self,
        tr: &mut Vec<f64>,
        player: usize,
        source_utok: usize,
        dest_utok: usize,
        num_cars: f64,
    ) {
        tr.push(307.0);
        tr.push(TAG_TRAIN_EXEC);
        tr.extend_from_slice(&evtok(Some(player)));
        tr.extend_from_slice(&ev1(source_utok as f64));
        tr.extend_from_slice(&ev1(dest_utok as f64));
        tr.extend_from_slice(&ev1(num_cars));
        push_val(tr, &JsVal::Undef);
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
    fn un_transport_ship_state(&mut self, tr: &mut Vec<f64>, uid: usize) -> JsVal {
        self.un(uid, UN_TRANSPORT_SHIP_STATE, tr, 850.0, &[])
    }
    /// `unit.setTargetTile(tile)` with a numeric tile (the G5b variant passes
    /// `undefined`).
    fn un_set_target_tile_num(&mut self, tr: &mut Vec<f64>, uid: usize, tile: f64) {
        let mut prefix = vec![uid as f64];
        prefix.extend_from_slice(&ev1(tile));
        mk_void(tr, 343.0, &prefix);
    }

    // ---- G5g facade accessors (codes >= 800) ----

    /// `mg.terraNullius().id()` — the ONE traced `TerraNullius` accessor (code
    /// 811, always `null`); `isPlayer()` / `smallID()` are untraced constants.
    fn tn_id(&mut self, tr: &mut Vec<f64>) -> Option<usize> {
        tr.push(811.0);
        push_val(tr, &JsVal::Null);
        None
    }

    // mg (`AttackExecution` / `TransportShipExecution`).
    fn mg_attack_amount(&mut self, tr: &mut Vec<f64>, owner: usize, target: usize) -> f64 {
        let mut p = evtok(Some(owner));
        p.extend_from_slice(&evtok(Some(target)));
        as_num(&mk_take(&mut self.mg, MG_ATTACK_AMOUNT, tr, 800.0, &p))
    }
    fn mg_boat_max_number(&mut self, tr: &mut Vec<f64>) -> f64 {
        as_num(&mk_take(&mut self.mg, MG_BOAT_MAX_NUMBER, tr, 801.0, &[]))
    }
    fn mg_boat_attack_amount(&mut self, tr: &mut Vec<f64>, attacker: usize, target: usize) -> f64 {
        let mut p = evtok(Some(attacker));
        p.extend_from_slice(&evtok(Some(target)));
        as_num(&mk_take(&mut self.mg, MG_BOAT_ATTACK_AMOUNT, tr, 802.0, &p))
    }
    fn mg_defense_post_range(&mut self, tr: &mut Vec<f64>) -> f64 {
        as_num(&mk_take(&mut self.mg, MG_DEFENSE_POST_RANGE, tr, 803.0, &[]))
    }
    /// `mg.config().attackLogic(input)` — `input` is the exact object the TS side
    /// built (key order preserved).
    fn mg_attack_logic(&mut self, tr: &mut Vec<f64>, input: &JsVal) -> JsVal {
        mk_take(&mut self.mg, MG_ATTACK_LOGIC, tr, 809.0, &ev_enc(input))
    }
    /// `mg.hasUnitNearby(tile, range, ty, playerID)` (the 4-arg form).
    fn mg_has_unit_nearby_pid(
        &mut self,
        tr: &mut Vec<f64>,
        tile: f64,
        outer: f64,
        ty: &str,
        pid: &str,
    ) -> bool {
        let mut p = ev1(tile);
        p.extend_from_slice(&ev1(outer));
        p.extend_from_slice(&evs(ty));
        p.extend_from_slice(&evstr_opt(Some(pid)));
        as_bool(&mk_take(&mut self.mg, MG_HAS_UNIT_NEARBY, tr, 305.0, &p))
    }
    /// `map.neighbors4(tile, buf)` — returns the traced neighbour list.
    fn mg_neighbors4(&mut self, tr: &mut Vec<f64>, tile: f64) -> Vec<f64> {
        let v = mk_take(&mut self.mg, MG_NEIGHBORS4, tr, 804.0, &ev1(tile));
        as_num_list(&v)
    }
    /// `map.terrainType(tile)` — the numeric `TerrainType` member.
    fn mg_terrain_type(&mut self, tr: &mut Vec<f64>, tile: f64) -> f64 {
        as_num(&mk_take(&mut self.mg, MG_TERRAIN_TYPE, tr, 805.0, &ev1(tile)))
    }
    fn mg_has_fallout(&mut self, tr: &mut Vec<f64>, tile: f64) -> bool {
        as_bool(&mk_take(&mut self.mg, MG_HAS_FALLOUT, tr, 806.0, &ev1(tile)))
    }
    fn mg_conquer_player(&mut self, tr: &mut Vec<f64>, owner: usize, target: usize) {
        let mut p = evtok(Some(owner));
        p.extend_from_slice(&evtok(Some(target)));
        mk_void(tr, 808.0, &p);
        // G5j: mutate the grid — reassign every tile owned by the target player.
        if self.el_map.is_some() {
            let o_sid = self.pl_peek_small_id(owner).unwrap_or(f64::NAN);
            let t_sid = self.pl_peek_small_id(target).unwrap_or(f64::NAN);
            if let Some(map) = self.el_map.as_mut() {
                for v in map.owners.iter_mut() {
                    if *v == t_sid {
                        *v = o_sid;
                    }
                }
            }
        }
    }
    /// `mg.forEachNeighbor(tile, cb)` — callback form, no return value.
    fn mg_for_each_neighbor(
        &mut self,
        tr: &mut Vec<f64>,
        tile: f64,
        mut cb: impl FnMut(&mut Self, &mut Vec<f64>, f64),
    ) {
        let v = mk_take(&mut self.mg, MG_FOR_EACH_NEIGHBOR, tr, 807.0, &ev1(tile));
        let list = as_num_list(&v);
        for n in &list {
            cb(self, tr, *n);
        }
        push_val(tr, &JsVal::Undef);
    }
    /// `targetTransportTile(mg, attacker, ref)` → `TileRef | null`.
    fn mg_target_transport_tile(
        &mut self,
        tr: &mut Vec<f64>,
        attacker: usize,
        r: f64,
    ) -> Option<f64> {
        let mut p = evtok(Some(attacker));
        p.extend_from_slice(&ev1(r));
        let v = mk_take(&mut self.mg, MG_TARGET_TRANSPORT_TILE, tr, 880.0, &p);
        as_opt_num(Some(&v))
    }
    /// `WaterPathFinder.rebuilt`.
    fn mg_water_rebuilt(&mut self, tr: &mut Vec<f64>) -> bool {
        as_bool(&mk_take(&mut self.mg, MG_WATER_REBUILT, tr, 860.0, &[]))
    }
    /// `WaterPathFinder.findPath(from, to)` — `TileRef[] | null`.
    fn mg_water_find_path(&mut self, tr: &mut Vec<f64>, from: f64, to: f64) -> Option<Vec<f64>> {
        let mut p = ev1(from);
        p.extend_from_slice(&ev1(to));
        let v = mk_take(&mut self.mg, MG_WATER_FIND_PATH, tr, 861.0, &p);
        match v {
            JsVal::Null | JsVal::Undef => None,
            other => Some(as_num_list(&other)),
        }
    }
    /// `WaterPathFinder.next(from, to)` → `{status, node}`.
    fn mg_water_next(&mut self, tr: &mut Vec<f64>, from: f64, to: f64) -> JsVal {
        let mut p = ev1(from);
        p.extend_from_slice(&ev1(to));
        mk_take(&mut self.mg, MG_WATER_NEXT, tr, 862.0, &p)
    }
    fn mg_stats_attack(&mut self, tr: &mut Vec<f64>, p: usize, t: usize, troops: f64) {
        let mut e = evtok(Some(p));
        e.extend_from_slice(&evtok(Some(t)));
        e.extend_from_slice(&ev1(troops));
        mk_void(tr, 870.0, &e);
    }
    fn mg_stats_attack_max_incoming(&mut self, tr: &mut Vec<f64>, p: usize, troops: f64) {
        let mut e = evtok(Some(p));
        e.extend_from_slice(&ev1(troops));
        mk_void(tr, 871.0, &e);
    }
    fn mg_stats_attack_cancel(&mut self, tr: &mut Vec<f64>, p: usize, t: usize, troops: f64) {
        let mut e = evtok(Some(p));
        e.extend_from_slice(&evtok(Some(t)));
        e.extend_from_slice(&ev1(troops));
        mk_void(tr, 872.0, &e);
    }
    fn mg_stats_boat_send_troops(&mut self, tr: &mut Vec<f64>, p: usize, t: usize, troops: f64) {
        let mut e = evtok(Some(p));
        e.extend_from_slice(&evtok(Some(t)));
        e.extend_from_slice(&ev1(troops));
        mk_void(tr, 873.0, &e);
    }
    fn mg_stats_boat_arrive_troops(&mut self, tr: &mut Vec<f64>, p: usize, t: usize, troops: f64) {
        let mut e = evtok(Some(p));
        e.extend_from_slice(&evtok(Some(t)));
        e.extend_from_slice(&ev1(troops));
        mk_void(tr, 874.0, &e);
    }

    // G5h trade-ship facade (`TradeShipExecution`).
    /// `mg.config().tradeShipGold(tilesTraveled, owner)`.
    fn mg_config_trade_ship_gold(&mut self, tr: &mut Vec<f64>, tiles: f64, owner: usize) -> f64 {
        let mut p = ev1(tiles);
        p.extend_from_slice(&evtok(Some(owner)));
        as_num(&mk_take(&mut self.mg, MG_TRADE_SHIP_GOLD, tr, 900.0, &p))
    }
    /// `mg.isShoreline(tile)`.
    fn mg_is_shoreline(&mut self, tr: &mut Vec<f64>, tile: f64) -> bool {
        as_bool(&mk_take(
            &mut self.mg,
            MG_IS_SHORELINE,
            tr,
            901.0,
            &ev1(tile),
        ))
    }
    /// `WaterPathFinder.pathForTraversal(from, to)` — `TileRef[]`.
    fn mg_water_path_for_traversal(
        &mut self,
        tr: &mut Vec<f64>,
        from: f64,
        to: f64,
    ) -> Option<Vec<f64>> {
        let mut p = ev1(from);
        p.extend_from_slice(&ev1(to));
        let v = mk_take(&mut self.mg, MG_WATER_PATH_FOR_TRAVERSAL, tr, 905.0, &p);
        match v {
            JsVal::Null | JsVal::Undef => None,
            other => Some(as_num_list(&other)),
        }
    }
    /// `stats().boatSendTrade(player, target)`.
    fn mg_stats_boat_send_trade(&mut self, tr: &mut Vec<f64>, p: usize, t: usize) {
        let mut e = evtok(Some(p));
        e.extend_from_slice(&evtok(Some(t)));
        mk_void(tr, 902.0, &e);
    }
    /// `stats().boatCapturedTrade(player, originalOwner, gold)`.
    fn mg_stats_boat_captured_trade(&mut self, tr: &mut Vec<f64>, p: usize, o: usize, gold: f64) {
        let mut e = evtok(Some(p));
        e.extend_from_slice(&evtok(Some(o)));
        e.extend_from_slice(&ev1(gold));
        mk_void(tr, 903.0, &e);
    }
    /// `stats().boatArriveTrade(src, dst, gold)`.
    fn mg_stats_boat_arrive_trade(&mut self, tr: &mut Vec<f64>, p: usize, t: usize, gold: f64) {
        let mut e = evtok(Some(p));
        e.extend_from_slice(&evtok(Some(t)));
        e.extend_from_slice(&ev1(gold));
        mk_void(tr, 904.0, &e);
    }
    fn un_is_marked_for_deletion(&mut self, tr: &mut Vec<f64>, uid: usize) -> bool {
        let v = self.un(uid, UN_IS_MARKED_FOR_DELETION, tr, 913.0, &[]);
        as_bool(&v)
    }
    fn un_set_target_unit(&mut self, tr: &mut Vec<f64>, uid: usize, target: Option<usize>) {
        let mut p = vec![uid as f64];
        p.extend_from_slice(&evtok(target));
        mk_void(tr, 914.0, &p);
    }
    fn un_set_safe_from_pirates(&mut self, tr: &mut Vec<f64>, uid: usize) {
        mk_void(tr, 915.0, &[uid as f64]);
    }
    // The `addGold` / `addPiracyGold` / `addTradeGold` voids encode the receiver
    // token like every other player facade method (precedent: `pl_add_troops`).
    fn pl_add_gold(&mut self, tr: &mut Vec<f64>, tok: usize, gold: f64, tile: f64) {
        let mut e = ev1(gold);
        e.extend_from_slice(&ev1(tile));
        self.pl_void(tok, tr, 910.0, &e);
    }
    fn pl_add_piracy_gold(&mut self, tr: &mut Vec<f64>, tok: usize, gold: f64) {
        self.pl_void(tok, tr, 911.0, &ev1(gold));
    }
    fn pl_add_trade_gold(&mut self, tr: &mut Vec<f64>, tok: usize, gold: f64) {
        self.pl_void(tok, tr, 912.0, &ev1(gold));
    }

    // ---- G5j facade helpers (`WarshipExecution` / `PlayerExecution`) ----

    // `mg.config().warship*()` — plain numeric getters, no argument prefix.
    fn mg_warship_passive_healing(&mut self, tr: &mut Vec<f64>) -> f64 {
        as_num(&mk_take(&mut self.mg, MG_WARSHIP_PASSIVE_HEALING, tr, 930.0, &[]))
    }
    fn mg_warship_passive_healing_range(&mut self, tr: &mut Vec<f64>) -> f64 {
        as_num(&mk_take(&mut self.mg, MG_WARSHIP_PASSIVE_HEALING_RANGE, tr, 931.0, &[]))
    }
    fn mg_warship_retreat_health_percent(&mut self, tr: &mut Vec<f64>) -> f64 {
        as_num(&mk_take(&mut self.mg, MG_WARSHIP_RETREAT_HEALTH_PERCENT, tr, 932.0, &[]))
    }
    fn mg_warship_targetting_range(&mut self, tr: &mut Vec<f64>) -> f64 {
        as_num(&mk_take(&mut self.mg, MG_WARSHIP_TARGETTING_RANGE, tr, 933.0, &[]))
    }
    fn mg_warship_patrol_range(&mut self, tr: &mut Vec<f64>) -> f64 {
        as_num(&mk_take(&mut self.mg, MG_WARSHIP_PATROL_RANGE, tr, 934.0, &[]))
    }
    fn mg_warship_docking_range(&mut self, tr: &mut Vec<f64>) -> f64 {
        as_num(&mk_take(&mut self.mg, MG_WARSHIP_DOCKING_RANGE, tr, 935.0, &[]))
    }
    fn mg_warship_port_healing_bonus(&mut self, tr: &mut Vec<f64>) -> f64 {
        as_num(&mk_take(&mut self.mg, MG_WARSHIP_PORT_HEALING_BONUS, tr, 936.0, &[]))
    }
    fn mg_warship_port_switch_threshold(&mut self, tr: &mut Vec<f64>) -> f64 {
        as_num(&mk_take(&mut self.mg, MG_WARSHIP_PORT_SWITCH_THRESHOLD, tr, 937.0, &[]))
    }
    fn mg_warship_shell_attack_rate(&mut self, tr: &mut Vec<f64>) -> f64 {
        as_num(&mk_take(&mut self.mg, MG_WARSHIP_SHELL_ATTACK_RATE, tr, 938.0, &[]))
    }
    /// `performance.now()` — patched to the scripted mg slot 111 for class 42.
    fn mg_perf_now(&mut self, tr: &mut Vec<f64>) -> f64 {
        as_num(&mk_take(&mut self.mg, MG_PERF_NOW, tr, 942.0, &[]))
    }
    /// `mg.config().troopIncreaseRate(player)`.
    fn mg_troop_increase_rate(&mut self, tr: &mut Vec<f64>, p: usize) -> f64 {
        as_num(&mk_take(&mut self.mg, MG_TROOP_INCREASE_RATE, tr, 946.0, &evtok(Some(p))))
    }
    /// `mg.config().goldAdditionRate(player)`.
    fn mg_gold_addition_rate(&mut self, tr: &mut Vec<f64>, p: usize) -> f64 {
        as_num(&mk_take(&mut self.mg, MG_GOLD_ADDITION_RATE, tr, 947.0, &evtok(Some(p))))
    }
    /// `mg.config().temporaryEmbargoDuration()`.
    fn mg_temporary_embargo_duration(&mut self, tr: &mut Vec<f64>) -> f64 {
        as_num(&mk_take(&mut self.mg, MG_TEMPORARY_EMBARGO_DURATION, tr, 948.0, &[]))
    }
    /// `mg.stats().recordDeathPosition(player, pos)`.
    fn mg_stats_record_death_position(&mut self, tr: &mut Vec<f64>, p: usize, pos: f64) {
        let mut e = evtok(Some(p));
        e.extend_from_slice(&ev1(pos));
        mk_void(tr, 943.0, &e);
    }
    /// `mg.stats().playerKilled(player, ticks)`.
    fn mg_stats_player_killed(&mut self, tr: &mut Vec<f64>, p: usize, ticks: f64) {
        let mut e = evtok(Some(p));
        e.extend_from_slice(&ev1(ticks));
        mk_void(tr, 944.0, &e);
    }
    /// `mg.stats().goldWork(player, gold)`.
    fn mg_stats_gold_work(&mut self, tr: &mut Vec<f64>, p: usize, gold: f64) {
        let mut e = evtok(Some(p));
        e.extend_from_slice(&ev1(gold));
        mk_void(tr, 945.0, &e);
    }
    /// The scripted `smallID` (player slot 0, first value) WITHOUT consuming it —
    /// the G5j grid mutation (`conquer` / `conquerPlayer`) needs the raw smallID.
    fn pl_peek_small_id(&self, tok: usize) -> Option<f64> {
        self.players
            .get(tok)
            .and_then(|m| m.slots.get(PL_SMALL_ID))
            .and_then(|s| s.vals.first())
            .map(as_num)
    }
    /// `player.inDoomsdayClock()`.
    fn pl_in_doomsday_clock(&mut self, tr: &mut Vec<f64>, tok: usize) -> bool {
        as_bool(&self.pl(tok, PL_IN_DOOMSDAY_CLOCK, tr, 940.0, &[]))
    }
    /// `player.captureUnit(unit)` (void).
    fn pl_capture_unit(&mut self, tr: &mut Vec<f64>, tok: usize, u: usize) {
        self.pl_void(tok, tr, 941.0, &evtok(Some(u)));
    }
    /// `player.decayRelations()` (void).
    fn pl_decay_relations(&mut self, tr: &mut Vec<f64>, tok: usize) {
        self.pl_void(tok, tr, 949.0, &[]);
    }
    /// `player.removeGold(gold)` (void).
    fn pl_remove_gold(&mut self, tr: &mut Vec<f64>, tok: usize, gold: f64) {
        self.pl_void(tok, tr, 950.0, &ev1(gold));
    }
    /// `player.removeAllAlliances()` (void).
    fn pl_remove_all_alliances(&mut self, tr: &mut Vec<f64>, tok: usize) {
        self.pl_void(tok, tr, 951.0, &[]);
    }
    /// `player.lastTileChange()`.
    fn pl_last_tile_change(&mut self, tr: &mut Vec<f64>, tok: usize) -> f64 {
        as_num(&self.pl(tok, PL_LAST_TILE_CHANGE, tr, 952.0, &[]))
    }
    /// `player.largestClusterBoundingBox = v` — traced as a void with the value
    /// (`null` clears, otherwise four numbers minx/miny/maxx/maxy).
    fn pl_set_largest_cluster_bbox(
        &mut self,
        tr: &mut Vec<f64>,
        tok: usize,
        v: Option<(f64, f64, f64, f64)>,
    ) {
        let mut prefix = vec![tok as f64];
        match v {
            None => prefix.push(2.0), // null
            Some((minx, miny, maxx, maxy)) => {
                prefix.extend_from_slice(&ev1(minx));
                prefix.extend_from_slice(&ev1(miny));
                prefix.extend_from_slice(&ev1(maxx));
                prefix.extend_from_slice(&ev1(maxy));
            }
        }
        mk_void(tr, 953.0, &prefix);
    }
    /// `player.alliances()` → alliance token list.
    fn pl_alliances(&mut self, tr: &mut Vec<f64>, tok: usize) -> Vec<usize> {
        let v = self.pl(tok, PL_ALLIANCES, tr, 954.0, &[]);
        as_tok_list(&v)
    }
    /// `player.getEmbargoes()` → `[{isTemporary, createdAt, target}]` (target is
    /// a player token).
    fn pl_get_embargoes(&mut self, tr: &mut Vec<f64>, tok: usize) -> Vec<(bool, f64, usize)> {
        let v = self.pl(tok, PL_GET_EMBARGOES, tr, 955.0, &[]);
        match v {
            JsVal::Arr(items) => items
                .iter()
                .map(|it| {
                    (
                        val_field(it, "isTemporary").map(as_bool).unwrap_or(false),
                        val_field(it, "createdAt").map(as_num).unwrap_or(f64::NAN),
                        val_field(it, "target").map(as_num).unwrap_or(f64::NAN) as usize,
                    )
                })
                .collect(),
            _ => Vec::new(),
        }
    }
    /// `alliance.expiresAt()`.
    fn al_expires_at(&mut self, tr: &mut Vec<f64>, aid: usize) -> f64 {
        as_num(&self.al(aid, AL_EXPIRES_AT, tr, 956.0, &[]))
    }
    /// `alliance.expire()` (void).
    fn al_expire(&mut self, tr: &mut Vec<f64>, aid: usize) {
        self.al_void(aid, tr, 957.0, &[]);
    }
    /// `unit.warshipState()` — the scripted `{state, patrolTile?, retreatPort?, isInCombat?}`.
    fn un_warship_state(&mut self, tr: &mut Vec<f64>, uid: usize) -> JsVal {
        self.un(uid, UN_WARSHIP_STATE, tr, 928.0, &[])
    }
    /// `unit.recordTradeCapture()` (void).
    fn un_record_trade_capture(&mut self, tr: &mut Vec<f64>, uid: usize) {
        mk_void(tr, 929.0, &[uid as f64]);
    }
    /// `unit.isSafeFromPirates()`.
    fn un_is_safe_from_pirates(&mut self, tr: &mut Vec<f64>, uid: usize) -> bool {
        as_bool(&self.un(uid, UN_IS_SAFE_FROM_PIRATES, tr, 939.0, &[]))
    }
    /// `unit.updateWarshipState(state)` with a generic state object.
    fn un_update_warship_state_obj(&mut self, tr: &mut Vec<f64>, uid: usize, state: &JsVal) {
        let mut prefix = vec![uid as f64];
        prefix.extend_from_slice(&ev_enc(state));
        mk_void(tr, 342.0, &prefix);
    }
    /// `unit.delete(flag, captor)` with the optional captor player token.
    fn un_delete_captor(&mut self, tr: &mut Vec<f64>, uid: usize, flag: bool, captor: Option<usize>) {
        let mut prefix = vec![uid as f64];
        prefix.extend_from_slice(&evb(flag));
        if let Some(c) = captor {
            prefix.extend_from_slice(&evtok(Some(c)));
        }
        mk_void(tr, 285.0, &prefix);
    }
    /// `mg.nearbyUnits(tile, outer, types)` — non-callback form whose script
    /// entries may be plain tokens (legacy) or `{u, d2}` objects
    /// (`{unit, distSquared}` — `WarshipExecution.findBestTarget` reads both).
    fn mg_nearby_units_d2(
        &mut self,
        tr: &mut Vec<f64>,
        tile: f64,
        outer: f64,
        types: &[String],
    ) -> Vec<(usize, f64)> {
        let mut prefix = ev1(tile);
        prefix.extend_from_slice(&ev1(outer));
        prefix.extend_from_slice(&ev_strs(types));
        let v = mk_take(&mut self.mg, MG_NEARBY_UNITS, tr, 306.0, &prefix);
        match v {
            JsVal::Arr(items) => items
                .iter()
                .filter_map(|it| {
                    if let JsVal::Obj(_) = it {
                        let u = val_field(it, "u").map(as_num).unwrap_or(f64::NAN);
                        let d2 = val_field(it, "d2").map(as_num).unwrap_or(f64::NAN);
                        if u.is_nan() { None } else { Some((u as usize, d2)) }
                    } else {
                        let t = as_opt_tok(it)?;
                        Some((t, f64::NAN))
                    }
                })
                .collect(),
            _ => Vec::new(),
        }
    }
    /// `mg.addExecution(new AttackExecution(...))` (tag 16).
    #[allow(clippy::too_many_arguments)]
    fn mg_add_attack_exec(
        &mut self,
        tr: &mut Vec<f64>,
        start_troops: f64,
        owner: usize,
        target_id: Option<&str>,
        source_tile: Option<f64>,
        remove_troops: bool,
    ) {
        tr.push(307.0);
        tr.push(TAG_ATTACK_EXEC);
        push_val(tr, &JsVal::Num(start_troops));
        push_val(tr, &JsVal::Num(owner as f64));
        push_val(
            tr,
            &match target_id {
                Some(s) => JsVal::Str(s.to_string()),
                None => JsVal::Null,
            },
        );
        push_val(
            tr,
            &match source_tile {
                Some(t) => JsVal::Num(t),
                None => JsVal::Null,
            },
        );
        push_val(tr, &JsVal::Bool(remove_troops));
        push_val(tr, &JsVal::Undef);
    }

    // G5g player accessors.
    fn pl_can_attack_player(&mut self, tr: &mut Vec<f64>, tok: usize, other: usize) -> bool {
        let v = self.pl(tok, PL_CAN_ATTACK_PLAYER, tr, 820.0, &evtok(Some(other)));
        as_bool(&v)
    }
    fn pl_create_attack(
        &mut self,
        tr: &mut Vec<f64>,
        tok: usize,
        target: usize,
        troops: f64,
        source_tile: Option<f64>,
        tiles: &JsVal,
    ) -> Option<usize> {
        let mut extra = evtok(Some(target));
        extra.extend_from_slice(&ev1(troops));
        extra.extend_from_slice(&ev_enc(&match source_tile {
            Some(t) => JsVal::Num(t),
            None => JsVal::Null,
        }));
        extra.extend_from_slice(&ev_enc(tiles));
        let v = self.pl(tok, PL_CREATE_ATTACK, tr, 821.0, &extra);
        as_opt_tok(&v)
    }
    fn pl_incoming_attacks(&mut self, tr: &mut Vec<f64>, tok: usize) -> Vec<usize> {
        let v = self.pl(tok, PL_INCOMING_ATTACKS, tr, 822.0, &[]);
        as_tok_list(&v)
    }
    fn pl_unit_count(&mut self, tr: &mut Vec<f64>, tok: usize, ty: &str) -> f64 {
        as_num(&self.pl(tok, PL_UNIT_COUNT, tr, 823.0, &evs(ty)))
    }
    fn pl_add_troops(&mut self, tr: &mut Vec<f64>, tok: usize, n: f64) {
        self.pl_void(tok, tr, 824.0, &ev1(n));
    }
    /// `player.bestTransportShipSpawn(tile)` → `TileRef | false`.
    fn pl_best_transport_ship_spawn(&mut self, tr: &mut Vec<f64>, tok: usize, tile: f64) -> JsVal {
        self.pl(tok, PL_BEST_TRANSPORT_SHIP_SPAWN, tr, 825.0, &ev1(tile))
    }
    fn pl_is_traitor(&mut self, tr: &mut Vec<f64>, tok: usize) -> bool {
        let v = self.pl(tok, PL_IS_TRAITOR, tr, 826.0, &[]);
        as_bool(&v)
    }

    // G5g attack facade.
    fn at(&mut self, aid: usize, slot: usize, tr: &mut Vec<f64>, code: f64, extra: &[f64]) -> JsVal {
        let mut prefix = vec![aid as f64];
        prefix.extend_from_slice(extra);
        mk_take(&mut self.attacks[aid], slot, tr, code, &prefix)
    }
    fn at_void(&mut self, aid: usize, tr: &mut Vec<f64>, code: f64, extra: &[f64]) {
        let mut prefix = vec![aid as f64];
        prefix.extend_from_slice(extra);
        mk_void(tr, code, &prefix);
    }
    fn at_troops(&mut self, tr: &mut Vec<f64>, aid: usize) -> f64 {
        as_num(&self.at(aid, AT_TROOPS, tr, 830.0, &[]))
    }
    fn at_set_troops(&mut self, tr: &mut Vec<f64>, aid: usize, n: f64) {
        self.at_void(aid, tr, 831.0, &ev1(n));
    }
    fn at_delete(&mut self, tr: &mut Vec<f64>, aid: usize) {
        self.at_void(aid, tr, 832.0, &[]);
    }
    fn at_clear_border(&mut self, tr: &mut Vec<f64>, aid: usize) {
        self.at_void(aid, tr, 833.0, &[]);
    }
    fn at_add_border_tile(&mut self, tr: &mut Vec<f64>, aid: usize, tile: f64) {
        self.at_void(aid, tr, 834.0, &ev1(tile));
    }
    fn at_remove_border_tile(&mut self, tr: &mut Vec<f64>, aid: usize, tile: f64) {
        self.at_void(aid, tr, 835.0, &ev1(tile));
    }
    fn at_border_size(&mut self, tr: &mut Vec<f64>, aid: usize) -> f64 {
        as_num(&self.at(aid, AT_BORDER_SIZE, tr, 836.0, &[]))
    }
    fn at_retreated(&mut self, tr: &mut Vec<f64>, aid: usize) -> bool {
        as_bool(&self.at(aid, AT_RETREATED, tr, 837.0, &[]))
    }
    fn at_retreating(&mut self, tr: &mut Vec<f64>, aid: usize) -> bool {
        as_bool(&self.at(aid, AT_RETREATING, tr, 838.0, &[]))
    }
    fn at_is_active(&mut self, tr: &mut Vec<f64>, aid: usize) -> bool {
        as_bool(&self.at(aid, AT_IS_ACTIVE, tr, 839.0, &[]))
    }
    fn at_target(&mut self, tr: &mut Vec<f64>, aid: usize) -> Option<usize> {
        as_opt_tok(&self.at(aid, AT_TARGET, tr, 840.0, &[]))
    }
    fn at_source_tile(&mut self, tr: &mut Vec<f64>, aid: usize) -> Option<f64> {
        as_opt_num(Some(&self.at(aid, AT_SOURCE_TILE, tr, 841.0, &[])))
    }
    fn at_attacker(&mut self, tr: &mut Vec<f64>, aid: usize) -> Option<usize> {
        as_opt_tok(&self.at(aid, AT_ATTACKER, tr, 842.0, &[]))
    }

    // neighbor facade
    fn nb(&mut self, nid: usize, slot: usize, tr: &mut Vec<f64>, code: f64, extra: &[f64]) -> JsVal {
        let mut prefix = vec![nid as f64];
        prefix.extend_from_slice(extra);
        mk_take(&mut self.neighbors[nid], slot, tr, code, &prefix)
    }
    fn nb_is_player(&mut self, tr: &mut Vec<f64>, nid: usize) -> bool {
        if nid == TN_TOK {
            // `TerraNullius.isPlayer()` is an untraced constant `false`.
            return false;
        }
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
        self.attacks = val_field(&v, "attacks")
            .map(as_arr)
            .unwrap_or_default()
            .iter()
            .map(build_mock)
            .collect();
        // G5i: station mocks (indexed by unit token) and the static rail data
        // tables are appended only when the scenario scripts them.
        self.stations = val_field(&v, "stations")
            .map(as_arr)
            .unwrap_or_default()
            .iter()
            .map(build_mock)
            .collect();
        self.el_clusters = val_field(&v, "clusters")
            .map(as_arr)
            .unwrap_or_default()
            .iter()
            .map(|c| {
                val_field(c, "trade")
                    .map(as_arr)
                    .unwrap_or_default()
                    .iter()
                    .map(|x| as_num(x) as usize)
                    .collect()
            })
            .collect();
        self.el_railroads = val_field(&v, "railroads")
            .map(as_arr)
            .unwrap_or_default()
            .iter()
            .map(|r| ElRailroad {
                from_utok: val_field(r, "from").map(as_num).unwrap_or(0.0) as usize,
                to_utok: val_field(r, "to").map(as_num).unwrap_or(0.0) as usize,
                tiles: val_field(r, "tiles").map(as_arr_nums).unwrap_or_default(),
            })
            .collect();
        // G5j: the map grid data is passed through verbatim.
        self.el_map = val_field(&v, "map").map(ElMap::from_val);
        if let Some(ref map) = self.el_map {
            self.el_scratch.ensure_capacity((map.w * map.h) as usize);
        }

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
            36 => {
                // AttackExecution(startTroops = null, _owner, _targetID = null,
                // sourceTile = null, removeTroops = true).
                self.at_start_troops = as_opt_num(ctor.first());
                self.a_tok = n(1) as usize;
                self.at_target_id = ctor.get(2).and_then(as_str_opt);
                self.at_source_tile = as_opt_num(ctor.get(3));
                self.at_remove_troops = ctor.get(4).map(as_bool).unwrap_or(false);
                self.at_attack = None;
                self.at_active = true;
                self.at_to_conquer.clear();
                // `private random = new PseudoRandom(123)`.
                self.random = Some(PseudoRandom::new(123.0));
            }
            37 => {
                // TransportShipExecution(attacker, ref, troops).
                self.ts_attacker = n(0) as usize;
                self.ts_original_owner = self.ts_attacker;
                self.ts_ref = n(1);
                self.ts_troops = as_opt_num(ctor.get(2));
                self.ts_active = true;
                self.ts_last_move = f64::NAN;
                self.ts_dst = None;
                self.ts_src = None;
                self.ts_retreat_dst = RetDst::Null;
                self.ts_boat = None;
                self.ts_motion_plan_id = 1.0;
                self.ts_motion_plan_dst = None;
                self.ts_target = None;
            }
            38 => {
                // TradeShipExecution(origOwner, srcPort, _dstPort).
                self.tds_orig_owner = n(0) as usize;
                self.tds_src_port = as_opt_num(ctor.get(1)).map(|x| x as usize);
                self.tds_dst_port = as_opt_num(ctor.get(2)).map(|x| x as usize);
                self.tds_trade_ship = None;
                self.tds_was_captured = false;
                self.tds_active = true;
                self.tds_tiles_traveled = 0.0;
                self.tds_motion_plan_id = 1.0;
                self.tds_motion_plan_dst = None;
            }
            39 => {
                // TrainStationExecution(unit, spawnTrains?). `active = true`.
                self.a_tok = n(0) as usize;
                self.tss_spawn_trains = ctor.get(1).and_then(|v| match v {
                    JsVal::Bool(b) => Some(*b),
                    _ => None,
                });
                self.tss_active = true;
                self.tss_station = None;
                self.tss_num_cars = 5.0;
                self.tss_last_spawn_tick = 0.0;
                self.tss_ticks_cooldown = 10.0;
                self.tss_random = None;
                self.mg_ready = false;
            }
            40 => {
                // TrainExecution(railNetwork, player, source, destination, numCars).
                // The railNetwork facade arg is not stored (the scripted mg slot
                // 97 is reached directly); `active = true`.
                self.a_tok = n(0) as usize;
                self.tex_source = n(1) as usize;
                self.tex_destination = n(2) as usize;
                self.tex_num_cars = n(3);
                self.tex_active = true;
                self.tex_train = None;
                self.tex_cars = Vec::new();
                self.tex_has_cargo = false;
                self.tex_current_tile = 0.0;
                self.tex_used_tiles = Vec::new();
                self.tex_stations = Vec::new();
                self.tex_current_railroad = None;
                self.tex_speed = 2.0;
                self.tex_spacing = 2.0;
                self.tex_trade_stops = 0.0;
                self.tex_path_tiles = Vec::new();
                self.tex_path_index = 0.0;
                self.mg_ready = false;
            }
            41 => {
                // WarshipExecution(input: (UnitParams & OwnerComp) | Unit).
                // Scenarios pass `elUnitRef(0)` → the warship unit token.
                self.a_tok = n(0) as usize;
                self.ws_warship = None;
                self.last_manual_move_tick_retreat_disabled = 0.0;
                self.last_observed_patrol_tile = None;
                self.active_healing_remainder = 0.0;
                self.last_emitted_combat = false;
                self.ws_random = None;
                self.last_shell_attack = 0.0;
                self.already_sent_shell = Vec::new();
                self.flag = true;
                self.mg_ready = false;
            }
            42 => {
                // PlayerExecution(player). NOTE: the optional `spec.map` grid was
                // parsed during harness setup (before `construct`), so the arm
                // must NOT reset `el_map` / `el_scratch` here.
                self.pe_player = n(0) as usize;
                self.pe_active = true;
                self.last_calc = 0.0;
                self.nbuf = [0.0; 4];
                self.nbuf8 = [0.0; 8];
            }
            43 => {
                // TribeExecution(tribe). The ctor reads `tribe.id()` (231) into a
                // scratch buffer (the trace is discarded for kind 0), then draws
                // 5 values from PseudoRandom(simpleHash(id)).
                self.te_tribe = n(0) as usize;
                self.flag = true; // `private active = true`.
                self.te_neighbors_tn = true;
                self.te_attack_behavior = false;
                self.te_bot_attack_troops_sent = 0.0;
                self.te_nbuf = [0.0; 4];
                let mut scratch = Vec::new();
                let id = self.pl_id(&mut scratch, self.te_tribe);
                let mut rng = PseudoRandom::new(crate::util::simple_hash(&id));
                self.te_attack_rate = rng.next_int(40.0, 80.0) as f64;
                self.te_attack_tick = rng.next_int(0.0, self.te_attack_rate) as f64;
                self.te_trigger_ratio = rng.next_int(50.0, 60.0) as f64 / 100.0;
                self.te_reserve_ratio = rng.next_int(30.0, 40.0) as f64 / 100.0;
                self.te_expand_ratio = rng.next_int(10.0, 20.0) as f64 / 100.0;
                self.random = Some(rng);
            }
            44 => {
                // NationExecution(gameID, nation). The ctor traces nothing: it
                // only reads `nation.playerInfo.id` (plain data) and seeds the
                // PRNG with `simpleHash(id) + simpleHash(gameID)`, then draws
                // the three ratios (NO attackRate draw — that happens in init).
                self.el_game_id = ctor.first().map(as_str).unwrap_or_default();
                let nation = ctor.get(1).cloned().unwrap_or(JsVal::Absent);
                self.ne_player_info = val_field(&nation, "playerInfo")
                    .cloned()
                    .unwrap_or(JsVal::Absent);
                self.ne_nation_id = val_field(&self.ne_player_info, "id")
                    .map(as_str)
                    .unwrap_or_default();
                self.ne_nation_name = val_field(&self.ne_player_info, "name")
                    .map(as_str)
                    .unwrap_or_default();
                self.ne_spawn_cell = val_field(&nation, "spawnCell").and_then(|c| {
                    let x = val_field(c, "x").map(as_num)?;
                    let y = val_field(c, "y").map(as_num)?;
                    Some((x, y))
                });
                self.flag = true; // `private active = true`.
                let mut rng = PseudoRandom::new(
                    crate::util::simple_hash(&self.ne_nation_id)
                        + crate::util::simple_hash(&self.el_game_id),
                );
                self.te_trigger_ratio = rng.next_int(50.0, 60.0) as f64 / 100.0;
                self.te_reserve_ratio = rng.next_int(30.0, 40.0) as f64 / 100.0;
                self.te_expand_ratio = rng.next_int(10.0, 20.0) as f64 / 100.0;
                self.random = Some(rng);
                self.te_is_nation = true;
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
            36 => self.attack_execution(kind, ticks, tr),
            37 => self.transport_ship_execution(kind, ticks, tr),
            38 => self.trade_ship_execution(kind, ticks, tr),
            39 => self.train_station_execution(kind, ticks, tr),
            40 => self.train_execution(kind, ticks, tr),
            41 => self.warship_execution(kind, ticks, tr),
            42 => self.player_execution(kind, ticks, tr),
            43 => self.tribe_execution(kind, ticks, tr),
            44 => self.nation_execution(kind, ticks, tr),
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
                // `target.delete(true, this._owner)` — captor = the SAM owner.
                self.un_delete_captor(tr, target, true, Some(self.a_tok));
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
                    let at = self.at_troops(tr, *attack); // 830
                    let d = self.mg_nuke_death_factor(
                        tr,
                        &nuke_type,
                        at,
                        tiles_left,
                        max_troops,
                    ); // 704
                    self.at_set_troops(tr, *attack, at - d); // 831
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

    // ---- 36 `AttackExecution` (G5g) -----------------------------------------

    fn attack_execution(
        &mut self,
        kind: u8,
        _ticks: f64,
        tr: &mut Vec<f64>,
    ) -> Result<JsVal, String> {
        match kind {
            3 => return Ok(JsVal::Bool(self.at_active)),
            4 => return Ok(JsVal::Bool(false)),
            1 => {
                self.attack_init(tr)?;
                return Ok(JsVal::Undef);
            }
            2 => {}
            _ => return Ok(JsVal::Undef),
        }
        self.attack_tick(tr)?;
        Ok(JsVal::Undef)
    }

    /// `this.random.nextInt(min, max)` for the attack tile-priority draws.
    fn at_random_int(&mut self, min: f64, max: f64) -> f64 {
        self.random
            .as_mut()
            .expect("attack random")
            .next_int(min, max) as f64
    }

    fn at_attack_id(&self) -> Result<usize, String> {
        self.at_attack
            .ok_or_else(|| "Attack not initialized".to_string())
    }

    /// `this.target.isPlayer()` — the `TerraNullius` mock answers untraced
    /// `false`, so a sentinel target never reads code `259`.
    fn attack_target_is_player(&mut self, tr: &mut Vec<f64>) -> bool {
        match self.at_target {
            Some(p) if p != TN_TOK => self.pl_is_player(tr, p), // 259
            _ => false,
        }
    }

    fn attack_reject_incoming_alliance(&mut self, tr: &mut Vec<f64>) {
        let target = match self.at_target {
            Some(t) => t,
            None => return,
        };
        let incoming = self.pl_incoming_alliance_requests(tr, self.a_tok); // 720
        for rid in incoming {
            let r = self.rq_requestor(tr, rid); // 744
            if r == Some(target) {
                self.rq_reject(tr, rid); // 272
                break;
            }
        }
    }

    fn attack_init(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        if !self.at_active {
            return Ok(());
        }
        // `this.map = mg.map()` — untraced.
        if let Some(id) = self.at_target_id.clone() {
            if !self.mg_has_player(tr, &id) {
                // 210
                self.at_active = false;
                return Ok(());
            }
        }
        // `this.target = _targetID === mg.terraNullius().id() ? mg.terraNullius()
        //  : mg.player(_targetID)`; the `terraNullius().id()` read always fires.
        let tn_id = self.tn_id(tr); // 811
        let is_tn = self.at_target_id.is_none() && tn_id.is_none();
        self.at_target = if is_tn {
            Some(TN_TOK)
        } else {
            let id = self.at_target_id.clone().unwrap_or_default();
            self.mg_player(tr, &id) // 211
        };
        self.at_owner_small_id = self.pl_small_id(tr, self.a_tok); // 230
        if self.at_target == Some(TN_TOK) {
            self.at_target_small_id = TN_TOK as f64;
        } else {
            match self.at_target {
                Some(p) => self.at_target_small_id = self.pl_small_id(tr, p), // 230
                None => {
                    return Err(
                        "Cannot read properties of null (reading 'smallID')".to_string()
                    )
                }
            }
        }

        // `if (this._owner === this.target)`.
        if self.at_target == Some(self.a_tok) {
            self.at_active = false;
            return Ok(());
        }

        // ALLIANCE CHECK — block attacks on friendly (ally or same team).
        if self.attack_target_is_player(tr)
            && self.pl_is_friendly(tr, self.a_tok, self.at_target)
        {
            // 242
            let _ = self.pl_display_name(tr, self.a_tok); // 232
            let _ = self.pl_display_name(tr, self.at_target.unwrap()); // 232
            self.at_active = false;
            return Ok(());
        }

        // `if (this.target && this.target.isPlayer())`.
        if self.attack_target_is_player(tr) {
            let target_tok = self.at_target.unwrap();
            let tty = self.pl_type(tr, target_tok); // 251
            if tty != PLAYER_TYPE_BOT {
                let oty = self.pl_type(tr, self.a_tok); // 251
                if oty != PLAYER_TYPE_BOT {
                    self.pl_add_embargo(tr, target_tok, Some(self.a_tok), true); // 253
                    self.attack_reject_incoming_alliance(tr);
                }
            }
        }

        // `if (this.target.isPlayer() && !this._owner.canAttackPlayer(this.target))`.
        if self.attack_target_is_player(tr) {
            let ok = self.pl_can_attack_player(tr, self.a_tok, self.at_target.unwrap()); // 820
            if !ok {
                self.at_active = false;
                return Ok(());
            }
        }

        let target_tok = self.at_target.unwrap();
        if self.at_start_troops.is_none() {
            let amt = self.mg_attack_amount(tr, self.a_tok, target_tok); // 800
            self.at_start_troops = Some(amt);
        }
        if self.at_remove_troops {
            let owned = self.pl_troops(tr, self.a_tok); // 418
            let st = js_min(self.at_start_troops.unwrap(), owned);
            self.at_start_troops = Some(self.pl_remove_troops(tr, self.a_tok, st)); // 633
        }
        let tiles = JsVal::Obj(Vec::new());
        self.at_attack = self.pl_create_attack(
            tr,
            self.a_tok,
            target_tok,
            self.at_start_troops.unwrap(),
            self.at_source_tile,
            &tiles,
        ); // 821

        match self.at_source_tile {
            Some(t) => self.attack_add_neighbors(tr, t)?,
            None => self.attack_refresh_to_conquer(tr)?,
        }

        self.mg_stats_attack(tr, self.a_tok, target_tok, self.at_start_troops.unwrap()); // 870

        let atk = self.at_attack_id()?;
        let incoming = self.pl_incoming_attacks(tr, self.a_tok); // 822
        for inc in incoming {
            let att = self.at_attacker(tr, inc); // 842
            if att != self.at_target {
                continue;
            }
            let a = self.at_troops(tr, inc); // 830
            let b = self.at_troops(tr, atk); // 830
            if a > b {
                let x = self.at_troops(tr, inc); // 830
                let y = self.at_troops(tr, atk); // 830
                self.at_set_troops(tr, inc, x - y); // 831
                self.at_delete(tr, atk); // 832
                self.at_active = false;
                return Ok(());
            }
            let x = self.at_troops(tr, atk); // 830
            let y = self.at_troops(tr, inc); // 830
            self.at_set_troops(tr, atk, x - y); // 831
            self.at_delete(tr, inc); // 832
        }

        let outgoing = self.pl_outgoing_attacks(tr, self.a_tok); // 721
        for out in outgoing {
            if out == atk {
                continue;
            }
            let ot = self.at_target(tr, out); // 840
            let mt = self.at_target(tr, atk); // 840
            if ot != mt {
                continue;
            }
            let st = self.at_source_tile(tr, atk); // 841
            if st.is_none() {
                let x = self.at_troops(tr, atk); // 830
                let y = self.at_troops(tr, out); // 830
                self.at_set_troops(tr, atk, x + y); // 831
                self.at_delete(tr, out); // 832
            }
        }

        let max_in = self.at_troops(tr, atk); // 830
        self.mg_stats_attack_max_incoming(tr, target_tok, max_in); // 871

        if self.attack_target_is_player(tr) {
            let difficulty = self.mg_difficulty(tr); // 402
            let rel = if difficulty == DIFFICULTY_EASY {
                -60.0
            } else if difficulty == DIFFICULTY_MEDIUM {
                -70.0
            } else if difficulty == DIFFICULTY_HARD {
                -80.0
            } else if difficulty == DIFFICULTY_IMPOSSIBLE {
                -100.0
            } else {
                return Err(format!("Unexpected value: {difficulty}"));
            };
            self.pl_update_relation(tr, self.at_target.unwrap(), Some(self.a_tok), rel); // 237
        }
        Ok(())
    }

    fn attack_refresh_to_conquer(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        let atk = self.at_attack_id()?;
        self.at_to_conquer.clear();
        self.at_clear_border(tr, atk); // 833
        let tiles = self.pl_border_tiles(tr, self.a_tok); // 635
        for t in tiles {
            self.attack_add_neighbors(tr, t)?;
        }
        Ok(())
    }

    fn attack_add_neighbors(&mut self, tr: &mut Vec<f64>, tile: f64) -> Result<(), String> {
        let atk = self.at_attack_id()?;
        let tick_now = self.mg_ticks(tr); // 212
        let target_small = self.at_target_small_id;
        let owner_small = self.at_owner_small_id;
        let neighbors = self.mg_neighbors4(tr, tile); // 804
        for n in neighbors {
            if self.mg_is_water(tr, n) {
                // 508
                continue;
            }
            if self.mg_is_impassable(tr, n) {
                // 617
                continue;
            }
            if self.mg_owner_id(tr, n) != target_small {
                // 223
                continue;
            }
            self.at_add_border_tile(tr, atk, n); // 834
            let inner = self.mg_neighbors4(tr, n); // 804
            let mut num_owned_by_me = 0.0;
            for n2 in inner {
                if self.mg_owner_id(tr, n2) == owner_small {
                    // 223
                    num_owned_by_me += 1.0;
                }
            }
            let terrain = self.mg_terrain_type(tr, n); // 805
            let mag = if terrain == TERRAIN_PLAINS {
                1.0
            } else if terrain == TERRAIN_HIGHLAND {
                1.5
            } else if terrain == TERRAIN_MOUNTAIN {
                2.0
            } else {
                0.0
            };
            let r = self.at_random_int(0.0, 7.0);
            let priority =
                (r + 10.0) * (1.0 - num_owned_by_me * 0.5 + mag / 2.0) + tick_now;
            self.at_to_conquer.enqueue(n, priority);
        }
        Ok(())
    }

    fn attack_retreat(&mut self, tr: &mut Vec<f64>, malus_percent: f64) -> Result<(), String> {
        let atk = self.at_attack_id()?;
        let deaths = self.at_troops(tr, atk) * (malus_percent / 100.0); // 830
        if js_truthy(deaths) {
            let oid = self.pl_id(tr, self.a_tok); // 231
            let rendered = crate::utils_format::render_troops(deaths);
            self.mg_display_message(
                tr,
                &[
                    JsVal::Str("events_display.attack_cancelled_retreat".to_string()),
                    JsVal::Num(MSG_ATTACK_CANCELLED),
                    JsVal::Str(oid),
                    JsVal::Undef,
                    JsVal::Obj(vec![("troops".to_string(), JsVal::Str(rendered))]),
                ],
            ); // 226
        }
        if !self.at_remove_troops && self.at_source_tile.is_none() {
            let a = self.at_troops(tr, atk); // 830
            let b = self.at_start_troops.unwrap_or(0.0);
            self.at_set_troops(tr, atk, a - b); // 831
        }
        let survivors = self.at_troops(tr, atk) - deaths; // 830
        self.pl_add_troops(tr, self.a_tok, survivors); // 824
        self.at_delete(tr, atk); // 832
        self.at_active = false;
        if self.at_retreated(tr, atk) {
            // 837
            let t = self.at_target.unwrap_or(TN_TOK);
            self.mg_stats_attack_cancel(tr, self.a_tok, t, survivors); // 872
        }
        Ok(())
    }

    fn attack_tick(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        let atk = self.at_attack_id()?;
        let mut troop_count = self.at_troops(tr, atk); // 830
        let target_is_player = self.attack_target_is_player(tr); // 259

        if self.at_retreated(tr, atk) {
            // 837
            if target_is_player {
                self.attack_retreat(tr, ATTACK_RETREAT_MALUS)?;
            } else {
                self.attack_retreat(tr, 0.0)?;
            }
            self.at_active = false;
            return Ok(());
        }
        if self.at_retreating(tr, atk) {
            // 838
            return Ok(());
        }
        if !self.at_is_active(tr, atk) {
            // 839
            self.at_active = false;
            return Ok(());
        }
        if target_is_player && self.pl_is_friendly(tr, self.a_tok, self.at_target) {
            // 242
            self.attack_retreat(tr, 0.0)?;
            return Ok(());
        }

        let bs = self.at_border_size(tr, atk); // 836
        let border_size = bs + self.at_random_int(0.0, 5.0);
        let mut tick_budget = 1.0;

        while tick_budget > 0.0 {
            if troop_count < 1.0 {
                self.at_delete(tr, atk); // 832
                self.at_active = false;
                return Ok(());
            }
            if self.at_to_conquer.size() == 0 {
                self.attack_refresh_to_conquer(tr)?;
                self.attack_retreat(tr, 0.0)?;
                return Ok(());
            }
            let tile = self.at_to_conquer.dequeue().unwrap_or(f64::NAN);
            self.at_remove_border_tile(tr, atk, tile); // 835

            let neighbors = self.mg_neighbors4(tr, tile); // 804
            let mut on_border = false;
            for n in &neighbors {
                if self.mg_owner_id(tr, *n) == self.at_owner_small_id {
                    // 223
                    on_border = true;
                    break;
                }
            }
            if self.mg_owner_id(tr, tile) != self.at_target_small_id || !on_border {
                // 223
                continue;
            }
            if !self.mg_is_land(tr, tile) {
                // 304
                continue;
            }
            if self.mg_is_impassable(tr, tile) {
                // 617
                continue;
            }
            self.attack_add_neighbors(tr, tile)?;

            let input = self.attack_logic_input(tr, troop_count, tile, border_size);
            let logic = self.mg_attack_logic(tr, &input); // 809
            let tick_fraction = val_field(&logic, "tickFraction")
                .map(as_num)
                .unwrap_or(f64::NAN);
            let attacker_loss = val_field(&logic, "attackerTroopLoss")
                .map(as_num)
                .unwrap_or(f64::NAN);
            let defender_loss = val_field(&logic, "defenderTroopLoss")
                .map(as_num)
                .unwrap_or(f64::NAN);
            tick_budget -= tick_fraction;
            troop_count -= attacker_loss;
            self.at_set_troops(tr, atk, troop_count); // 831
            if target_is_player {
                let dp = self.at_target.unwrap();
                self.pl_remove_troops(tr, dp, defender_loss); // 633
            }
            self.pl_conquer(tr, self.a_tok, tile); // 637
            self.attack_handle_dead_defender(tr);
        }
        Ok(())
    }

    /// `AttackExecution.attackLogicInput` — the object passed to
    /// `config().attackLogic`. Property order mirrors the TS literal.
    fn attack_logic_input(
        &mut self,
        tr: &mut Vec<f64>,
        attack_troops: f64,
        tile: f64,
        border_size: f64,
    ) -> JsVal {
        let defender = if self.attack_target_is_player(tr) {
            // 259
            self.at_target
        } else {
            None
        };
        let mut defender_has_post = false;
        if let Some(d) = defender {
            let range = self.mg_defense_post_range(tr); // 803
            let did = self.pl_id(tr, d); // 231
            defender_has_post =
                self.mg_has_unit_nearby_pid(tr, tile, range, UNIT_DEFENSE_POST, &did); // 305
        }

        let terrain = self.mg_terrain_type(tr, tile); // 805
        let att_type = self.pl_type(tr, self.a_tok); // 251
        let att_tiles = self.pl_num_tiles_owned(tr, self.a_tok); // 532
        let attacker = JsVal::Obj(vec![
            ("type".to_string(), JsVal::Str(att_type)),
            ("numTiles".to_string(), JsVal::Num(att_tiles)),
        ]);

        let defender_val = match defender {
            None => JsVal::Null,
            Some(d) => {
                let dty = self.pl_type(tr, d); // 251
                let dtiles = self.pl_num_tiles_owned(tr, d); // 532
                let dtroops = self.pl_troops(tr, d); // 418
                let dtraitor = self.pl_is_traitor(tr, d); // 826
                let ddisc = self.pl_is_disconnected(tr, d); // 531
                let teammate = if ddisc {
                    self.pl_is_on_same_team(tr, self.a_tok, Some(d)) // 250
                } else {
                    false
                };
                JsVal::Obj(vec![
                    ("type".to_string(), JsVal::Str(dty)),
                    ("numTiles".to_string(), JsVal::Num(dtiles)),
                    ("troops".to_string(), JsVal::Num(dtroops)),
                    ("isTraitor".to_string(), JsVal::Bool(dtraitor)),
                    ("isDisconnectedTeammate".to_string(), JsVal::Bool(teammate)),
                ])
            }
        };

        let fallout = if self.mg_has_fallout(tr, tile) {
            // 806
            let with = self.mg_num_tiles_with_fallout(tr); // 502
            let land = self.mg_num_land_tiles(tr); // 501
            JsVal::Num(with / land)
        } else {
            JsVal::Null
        };

        JsVal::Obj(vec![
            ("terrain".to_string(), JsVal::Num(terrain)),
            ("attackTroops".to_string(), JsVal::Num(attack_troops)),
            ("attacker".to_string(), attacker),
            ("defender".to_string(), defender_val),
            ("defenderHasDefensePost".to_string(), JsVal::Bool(defender_has_post)),
            ("falloutRatio".to_string(), fallout),
            ("borderSize".to_string(), JsVal::Num(border_size)),
        ])
    }

    fn attack_handle_dead_defender(&mut self, tr: &mut Vec<f64>) {
        if !self.attack_target_is_player(tr) {
            // 259
            return;
        }
        let target = match self.at_target {
            Some(t) => t,
            None => return,
        };
        let owned = self.pl_num_tiles_owned(tr, target); // 532
        // `!(owned < 100)` — NaN must also bail out, hence the explicit check.
        if owned.is_nan() || owned >= AT_DEAD_DEFENDER_TILES {
            return;
        }
        self.mg_conquer_player(tr, self.a_tok, target); // 808

        let a_tok = self.a_tok;
        let mut pass = 0.0;
        while pass < AT_MAX_PASSES {
            let mut progressed = false;
            let tiles = self.pl_tiles(tr, target); // 634
            for tile in &tiles {
                let tile = *tile;
                let mut borders = false;
                self.mg_for_each_neighbor(tr, tile, |h, tr, t| {
                    let owner = h.mg_owner(tr, t); // 222
                    if !borders && owner == Some(a_tok) {
                        borders = true;
                    }
                }); // 807
                if borders {
                    self.pl_conquer(tr, self.a_tok, tile); // 637
                    progressed = true;
                } else {
                    let mut captured = false;
                    self.mg_for_each_neighbor(tr, tile, |h, tr, nb| {
                        if captured {
                            return;
                        }
                        let no = h.mg_owner(tr, nb); // 222
                        let np = match no {
                            Some(x) => x,
                            None => return,
                        };
                        if !h.pl_is_player(tr, np) {
                            // 259
                            return;
                        }
                        if np == target {
                            return;
                        }
                        if h.pl_is_friendly(tr, np, Some(target)) {
                            // 242
                            return;
                        }
                        let pid = h.pl_id(tr, np); // 231
                        let ptok = h.mg_player(tr, &pid).unwrap_or(np); // 211
                        h.pl_conquer(tr, ptok, tile); // 637
                        captured = true;
                    }); // 807
                    if captured {
                        progressed = true;
                    }
                }
            }
            if !progressed {
                break;
            }
            pass += 1.0;
        }
    }

    // ---- 37 `TransportShipExecution` (G5g) ---------------------------------

    fn transport_ship_execution(
        &mut self,
        kind: u8,
        ticks: f64,
        tr: &mut Vec<f64>,
    ) -> Result<JsVal, String> {
        match kind {
            3 => return Ok(JsVal::Bool(self.ts_active)),
            4 => return Ok(JsVal::Bool(false)),
            1 => {
                self.transport_init(ticks, tr)?;
                return Ok(JsVal::Undef);
            }
            2 => {}
            _ => return Ok(JsVal::Undef),
        }
        self.transport_tick(ticks, tr)?;
        Ok(JsVal::Undef)
    }

    fn ts_target_is_player(&mut self, tr: &mut Vec<f64>) -> Result<bool, String> {
        match self.ts_target {
            Some(p) => Ok(self.pl_is_player(tr, p)), // 259
            None => Err(
                "Cannot read properties of null (reading 'isPlayer')".to_string()
            ),
        }
    }

    fn ts_reject_incoming_alliance(&mut self, tr: &mut Vec<f64>, target: usize) {
        let incoming = self.pl_incoming_alliance_requests(tr, self.ts_attacker); // 720
        for rid in incoming {
            let r = self.rq_requestor(tr, rid); // 744
            if r == Some(target) {
                self.rq_reject(tr, rid); // 272
                break;
            }
        }
    }

    fn transport_init(&mut self, ticks: f64, tr: &mut Vec<f64>) -> Result<(), String> {
        if !self.mg_is_valid_ref(tr, self.ts_ref) {
            // 300
            self.ts_active = false;
            return Ok(());
        }
        self.ts_last_move = ticks;
        self.ts_target = self.mg_owner(tr, self.ts_ref); // 222
        // `new WaterPathFinder(mg, stagger)` — untraced.

        let cnt = self.pl_unit_count(tr, self.ts_attacker, UNIT_TRANSPORT_SHIP); // 823
        let bmax = self.mg_boat_max_number(tr); // 801
        if cnt >= bmax {
            let aid = self.pl_id(tr, self.ts_attacker); // 231
            let bmax2 = self.mg_boat_max_number(tr); // 801
            self.mg_display_message(
                tr,
                &[
                    JsVal::Str("events_display.no_boats_available".to_string()),
                    JsVal::Num(MSG_ATTACK_FAILED),
                    JsVal::Str(aid),
                    JsVal::Undef,
                    JsVal::Obj(vec![("max".to_string(), JsVal::Num(bmax2))]),
                ],
            ); // 226
            self.ts_active = false;
            return Ok(());
        }

        if self.ts_target_is_player(tr)? {
            let t = self.ts_target.unwrap();
            let tty = self.pl_type(tr, t); // 251
            if tty != PLAYER_TYPE_BOT {
                let aty = self.pl_type(tr, self.ts_attacker); // 251
                if aty != PLAYER_TYPE_BOT {
                    self.ts_reject_incoming_alliance(tr, t);
                }
            }
        }

        if self.ts_target == Some(self.ts_attacker) {
            self.ts_active = false;
            return Ok(());
        }

        if self.ts_target_is_player(tr)? {
            let t = self.ts_target.unwrap();
            let ok = self.pl_can_attack_player(tr, self.ts_attacker, t); // 820
            if !ok {
                self.ts_active = false;
                return Ok(());
            }
        }

        let t = self.ts_target.unwrap();
        if self.ts_troops.is_none() {
            let amt = self.mg_boat_attack_amount(tr, self.ts_attacker, t); // 802
            self.ts_troops = Some(amt);
        }
        let owned = self.pl_troops(tr, self.ts_attacker); // 418
        self.ts_troops = Some(js_min(self.ts_troops.unwrap(), owned));

        self.ts_dst = self.mg_target_transport_tile(tr, self.ts_attacker, self.ts_ref); // 880
        if self.ts_dst.is_none() {
            self.ts_active = false;
            return Ok(());
        }
        let dst = self.ts_dst.unwrap();

        let src = self.pl_can_build(tr, self.ts_attacker, UNIT_TRANSPORT_SHIP, dst); // 534
        if matches!(src, JsVal::Bool(false)) {
            self.ts_active = false;
            return Ok(());
        }
        let src_t = as_num(&src);
        self.ts_src = Some(src_t);

        let params = JsVal::Obj(vec![
            ("troops".to_string(), JsVal::Num(self.ts_troops.unwrap())),
            ("targetTile".to_string(), JsVal::Num(dst)),
        ]);
        self.ts_boat = self.pl_build_unit(tr, self.ts_attacker, UNIT_TRANSPORT_SHIP, src_t, &params); // 535
        let boat = match self.ts_boat {
            Some(b) => b,
            None => {
                return Err(
                    "Cannot read properties of null (reading 'isActive')".to_string()
                )
            }
        };

        let found = self.mg_water_find_path(tr, src_t, dst); // 861
        let mut full_path = match found {
            Some(p) => p,
            None => vec![src_t],
        };
        if full_path.is_empty() || full_path[0] != src_t {
            full_path.insert(0, src_t);
        }

        let uid = self.un_id(tr, boat); // 340
        let plan = JsVal::Obj(vec![
            ("kind".to_string(), JsVal::Str("grid".to_string())),
            ("unitId".to_string(), JsVal::Num(uid)),
            ("planId".to_string(), JsVal::Num(self.ts_motion_plan_id)),
            ("startTick".to_string(), JsVal::Num(ticks + TS_TICKS_PER_MOVE)),
            ("ticksPerStep".to_string(), JsVal::Num(TS_TICKS_PER_MOVE)),
            (
                "path".to_string(),
                JsVal::Arr(full_path.iter().map(|x| JsVal::Num(*x)).collect()),
            ),
        ]);
        self.mg_record_motion_plan(tr, &plan); // 711
        self.ts_motion_plan_dst = self.ts_dst;

        let tid_null = matches!(self.pl_id_val(tr, t), JsVal::Null); // 231
        let tn = self.tn_id(tr); // 811
        // `this.target.id() !== mg.terraNullius().id()`.
        if !(tid_null && tn.is_none()) {
            let bid = self.un_id(tr, boat); // 340
            let name = self.pl_display_name(tr, self.ts_attacker); // 232
            let btr = self.un_troops(tr, boat); // 730
            let rendered = crate::utils_format::render_troops(btr);
            let msg = format!("Naval invasion incoming from {name} ({rendered})");
            let pid = self.pl_id(tr, t); // 231
            self.mg_display_incoming_unit(tr, bid, &msg, MSG_NAVAL_INVASION_INBOUND, &pid); // 714
        }

        let sent = self.un_troops(tr, boat); // 730
        self.mg_stats_boat_send_troops(tr, self.ts_attacker, t, sent); // 873
        Ok(())
    }

    fn transport_tick(&mut self, ticks: f64, tr: &mut Vec<f64>) -> Result<(), String> {
        if self.ts_dst.is_none() {
            self.ts_active = false;
            return Ok(());
        }
        if !self.ts_active {
            return Ok(());
        }
        let boat = match self.ts_boat {
            Some(b) => b,
            None => {
                return Err(
                    "Cannot read properties of null (reading 'isActive')".to_string()
                )
            }
        };
        if !self.un_is_active(tr, boat) {
            // 280
            self.ts_active = false;
            return Ok(());
        }
        if ticks - self.ts_last_move < TS_TICKS_PER_MOVE {
            return Ok(());
        }
        self.ts_last_move = ticks;

        // Team mate can conquer disconnected player and get their ships.
        let boat_owner = self.un_owner(tr, boat); // 286
        if self.pl_is_disconnected(tr, self.ts_original_owner) {
            // 531
            if boat_owner != Some(self.ts_original_owner) {
                if let Some(bo) = boat_owner {
                    if self.pl_is_on_same_team(tr, bo, Some(self.ts_original_owner)) {
                        // 250
                        self.ts_attacker = bo;
                        self.ts_original_owner = bo;
                    }
                }
            }
        }

        if self.mg_water_rebuilt(tr) {
            // 860
            self.ts_motion_plan_dst = None;
        }

        let dst = self.ts_dst.unwrap();
        if self.mg_is_water(tr, dst) {
            // 508
            let st = self.un_transport_ship_state(tr, boat); // 850
            let retreating = val_field(&st, "isRetreating").map(as_bool).unwrap_or(false);
            if !retreating {
                self.un_update_transport_ship_state(tr, boat); // 341
            }
            self.ts_retreat_dst = RetDst::Null;
        }

        let st2 = self.un_transport_ship_state(tr, boat); // 850
        if val_field(&st2, "isRetreating").map(as_bool).unwrap_or(false) {
            if matches!(self.ts_retreat_dst, RetDst::Null) {
                let bt = self.un_tile(tr, boat); // 344
                let spawn = self.pl_best_transport_ship_spawn(tr, self.ts_attacker, bt); // 825
                self.ts_retreat_dst = match spawn {
                    JsVal::Bool(false) => RetDst::False,
                    v => RetDst::Tile(as_num(&v)),
                };
            }
            if matches!(self.ts_retreat_dst, RetDst::False) {
                let trp = self.un_troops(tr, boat); // 730
                self.pl_add_troops(tr, self.ts_attacker, trp); // 824
                self.un_delete(tr, boat, false); // 285
                self.ts_active = false;
                return Ok(());
            }
            let rd = match self.ts_retreat_dst {
                RetDst::Tile(x) => x,
                _ => f64::NAN,
            };
            self.ts_dst = Some(rd);
            let cur = self.un_target_tile(tr, boat); // 282
            if cur != Some(rd) {
                self.un_set_target_tile_num(tr, boat, rd); // 343
            }
        }

        let dst2 = self.ts_dst.unwrap();
        let bt2 = self.un_tile(tr, boat); // 344
        let result = self.mg_water_next(tr, bt2, dst2); // 862
        let status = val_field(&result, "status").map(as_num).unwrap_or(f64::NAN);
        let node = val_field(&result, "node").cloned().unwrap_or(JsVal::Undef);

        let t = match self.ts_target {
            Some(x) => x,
            None => {
                return Err(
                    "Cannot read properties of null (reading 'id')".to_string()
                )
            }
        };

        if status == 2.0 {
            // `PathStatus.COMPLETE`.
            let owner = self.mg_owner(tr, dst2); // 222
            if owner == Some(self.ts_attacker) {
                let t1 = self.un_troops(tr, boat); // 730
                let deaths = t1 * (ATTACK_RETREAT_MALUS / 100.0);
                let t2 = self.un_troops(tr, boat); // 730
                let survivors = t2 - deaths;
                self.pl_add_troops(tr, self.ts_attacker, survivors); // 824
                self.un_delete(tr, boat, false); // 285
                self.ts_active = false;
                self.mg_stats_boat_arrive_troops(tr, self.ts_attacker, t, survivors); // 874
                if js_truthy(deaths) {
                    let aid = self.pl_id(tr, self.ts_attacker); // 231
                    let rendered = crate::utils_format::render_troops(deaths);
                    self.mg_display_message(
                        tr,
                        &[
                            JsVal::Str(
                                "events_display.attack_cancelled_retreat".to_string(),
                            ),
                            JsVal::Num(MSG_ATTACK_CANCELLED),
                            JsVal::Str(aid),
                            JsVal::Undef,
                            JsVal::Obj(vec![("troops".to_string(), JsVal::Str(rendered))]),
                        ],
                    ); // 226
                }
                return Ok(());
            }
            self.pl_conquer(tr, self.ts_attacker, dst2); // 637
            let tip = self.ts_target_is_player(tr)?; // 259
            let fr = if tip {
                self.pl_is_friendly(tr, self.ts_attacker, self.ts_target) // 242
            } else {
                false
            };
            if tip && fr {
                let trp = self.un_troops(tr, boat); // 730
                self.pl_add_troops(tr, self.ts_attacker, trp); // 824
            } else {
                let trp = self.un_troops(tr, boat); // 730
                let tid = self.pl_id(tr, t); // 231
                self.mg_add_attack_exec(tr, trp, self.ts_attacker, Some(&tid), Some(dst2), false);
            }
            self.un_delete(tr, boat, false); // 285
            self.ts_active = false;
            let arrived = self.un_troops(tr, boat); // 730
            self.mg_stats_boat_arrive_troops(tr, self.ts_attacker, t, arrived); // 874
            return Ok(());
        }

        if status == PATH_STATUS_NOT_FOUND {
            let bt3 = self.un_tile(tr, boat); // 344
            let _ = self.mg_x(tr, bt3); // 603
            let _ = self.mg_y(tr, bt3); // 604
            let _ = self.mg_x(tr, dst2); // 603
            let _ = self.mg_y(tr, dst2); // 604
            let _ = self.pl_id(tr, self.ts_attacker); // 231
            let _ = self.pl_id(tr, t); // 231
            let trp = self.un_troops(tr, boat); // 730
            self.pl_add_troops(tr, self.ts_attacker, trp); // 824
            self.un_delete(tr, boat, false); // 285
            self.ts_active = false;
            return Ok(());
        }

        if status == 0.0 {
            // `PathStatus.NEXT`.
            self.un_move(tr, boat, &node); // 545
        }

        // Force a motion-plan re-record when the destination moved.
        if self.ts_dst.is_some() && self.ts_dst != self.ts_motion_plan_dst {
            self.ts_motion_plan_id += 1.0;
            let d = self.ts_dst.unwrap();
            let bt = self.un_tile(tr, boat); // 344
            let found = self.mg_water_find_path(tr, bt, d); // 861
            let mut full_path = match found {
                Some(p) => p,
                None => vec![self.un_tile(tr, boat)], // 344
            };
            let empty = full_path.is_empty();
            let mut mismatch = false;
            if !empty {
                let btc = self.un_tile(tr, boat); // 344
                mismatch = full_path[0] != btc;
            }
            if empty || mismatch {
                let btu = self.un_tile(tr, boat); // 344
                full_path.insert(0, btu);
            }
            let uid = self.un_id(tr, boat); // 340
            let plan = JsVal::Obj(vec![
                ("kind".to_string(), JsVal::Str("grid".to_string())),
                ("unitId".to_string(), JsVal::Num(uid)),
                ("planId".to_string(), JsVal::Num(self.ts_motion_plan_id)),
                ("startTick".to_string(), JsVal::Num(ticks + TS_TICKS_PER_MOVE)),
                ("ticksPerStep".to_string(), JsVal::Num(TS_TICKS_PER_MOVE)),
                (
                    "path".to_string(),
                    JsVal::Arr(full_path.iter().map(|x| JsVal::Num(*x)).collect()),
                ),
            ]);
            self.mg_record_motion_plan(tr, &plan); // 711
            self.ts_motion_plan_dst = self.ts_dst;
        }
        Ok(())
    }

    // G5h — `TradeShipExecution` (class 38).
    fn trade_ship_execution(
        &mut self,
        kind: u8,
        ticks: f64,
        tr: &mut Vec<f64>,
    ) -> Result<JsVal, String> {
        match kind {
            3 => return Ok(JsVal::Bool(self.tds_active)),
            // `activeDuringSpawnPhase()` -> false.
            4 => return Ok(JsVal::Bool(false)),
            1 => {
                // `init`: stores `mg`, advances the (unobservable) stagger counter
                // and builds the stub `WaterPathFinder`. No facade reads.
                return Ok(JsVal::Undef);
            }
            2 => {}
            _ => return Ok(JsVal::Undef),
        }
        self.trade_ship_tick(ticks, tr)?;
        Ok(JsVal::Undef)
    }

    fn trade_ship_tick(&mut self, ticks: f64, tr: &mut Vec<f64>) -> Result<(), String> {
        if self.mg_water_rebuilt(tr) {
            // 860 — "Force motion plan re-recording".
            self.tds_motion_plan_dst = None;
        }

        if self.tds_trade_ship.is_none() {
            let src_port = self.tds_src_port.unwrap();
            let src_tile = self.un_tile(tr, src_port); // 344
            let spawn = self.pl_can_build(tr, self.tds_orig_owner, UNIT_TRADE_SHIP, src_tile); // 534
            if matches!(spawn, JsVal::Bool(false)) {
                // `console.warn("cannot build trade ship")` is untraced.
                self.tds_active = false;
                return Ok(());
            }
            let spawn_tile = as_num(&spawn);
            let dst_port = self.tds_dst_port.unwrap();
            let params = JsVal::Obj(vec![
                ("targetUnit".to_string(), JsVal::Num(dst_port as f64)),
                ("lastSetSafeFromPirates".to_string(), JsVal::Num(ticks)),
            ]);
            self.tds_trade_ship = self.pl_build_unit(
                tr,
                self.tds_orig_owner,
                UNIT_TRADE_SHIP,
                spawn_tile,
                &params,
            ); // 535
            let dst_owner = self.un_owner(tr, dst_port).unwrap(); // 286
            self.mg_stats_boat_send_trade(tr, self.tds_orig_owner, dst_owner); // 902
        }

        let ship = self.tds_trade_ship.unwrap();
        if !self.un_is_active(tr, ship) {
            // 280
            self.tds_active = false;
            return Ok(());
        }

        let ship_owner = self.un_owner(tr, ship).unwrap(); // 286
        let dst_port = self.tds_dst_port.unwrap();
        let dst_owner = self.un_owner(tr, dst_port).unwrap(); // 286
        if !self.tds_was_captured && self.tds_orig_owner != ship_owner {
            // Store as variable in case ship is recaptured by previous owner.
            self.tds_was_captured = true;
            let orig_id = self.pl_id(tr, self.tds_orig_owner); // 231
            let name = self.pl_display_name(tr, ship_owner); // 232
            let ship_id = self.un_id(tr, ship); // 340
            let ship_owner_id = self.pl_id(tr, ship_owner); // 231
            self.mg_display_message(
                tr,
                &[
                    JsVal::Str("events_display.trade_ship_captured".to_string()),
                    JsVal::Num(MSG_UNIT_DESTROYED),
                    JsVal::Str(orig_id),
                    JsVal::Undef,
                    JsVal::Obj(vec![("name".to_string(), JsVal::Str(name))]),
                    JsVal::Num(ship_id),
                    JsVal::Str(ship_owner_id),
                ],
            ); // 226
        }

        // If a player captures another player's port while trading we should
        // delete the ship.
        let dst_owner_id = self.pl_id(tr, dst_owner); // 231
        let src_owner = self.un_owner(tr, self.tds_src_port.unwrap()).unwrap(); // 286
        let src_owner_id = self.pl_id(tr, src_owner); // 231
        if dst_owner_id == src_owner_id {
            self.un_delete(tr, ship, false); // 285
            self.tds_active = false;
            return Ok(());
        }

        if !self.tds_was_captured {
            let dst_active = self.un_is_active(tr, dst_port); // 280
            if !dst_active || !self.pl_can_trade(tr, ship_owner, Some(dst_owner)) {
                // 533
                self.un_delete(tr, ship, false); // 285
                self.tds_active = false;
                return Ok(());
            }
        }

        let cur_tile = self.un_tile(tr, ship); // 344

        if self.tds_was_captured && (ship_owner != dst_owner || !self.un_is_active(tr, dst_port)) {
            // 280
            let my_component = self.mg_get_water_component(tr, cur_tile); // 301
            let ports = self.pl_units(tr, ship_owner, &[UNIT_PORT.to_string()]); // 241
            match self.trade_ship_nearest_port(tr, &ports, cur_tile, &my_component) {
                None => {
                    self.un_delete(tr, ship, false); // 285
                    self.tds_active = false;
                    return Ok(());
                }
                Some(port) => {
                    self.tds_dst_port = Some(port);
                    self.un_set_target_unit(tr, ship, Some(port)); // 914
                    // Plan-driven units don't emit per-tick updates, force a sync.
                    self.un_touch(tr, ship); // 736
                }
            }
        }

        if cur_tile == self.un_tile(tr, self.tds_dst_port.unwrap()) {
            // 344 — `this.dstPort()`
            self.trade_ship_complete(tr)?;
            return Ok(());
        }

        let dst = self.un_tile(tr, self.tds_dst_port.unwrap()); // 344
        let result = self.mg_water_next(tr, cur_tile, dst); // 862
        let status = val_field(&result, "status").map(as_num).unwrap_or(f64::NAN);
        let node = val_field(&result, "node").map(as_num).unwrap_or(f64::NAN);
        match status as i32 {
            0 => {
                // PathStatus.NEXT
                if self.tds_motion_plan_dst != Some(dst) {
                    self.tds_motion_plan_id += 1.0;
                    let from = node;
                    let path = self.mg_water_path_for_traversal(tr, from, dst); // 905
                    let uid = self.un_id(tr, ship); // 340
                    let path_vals: Vec<JsVal> = path
                        .unwrap_or_default()
                        .iter()
                        .map(|x| JsVal::Num(*x))
                        .collect();
                    let plan = JsVal::Obj(vec![
                        ("kind".to_string(), JsVal::Str("grid".to_string())),
                        ("unitId".to_string(), JsVal::Num(uid)),
                        ("planId".to_string(), JsVal::Num(self.tds_motion_plan_id)),
                        ("startTick".to_string(), JsVal::Num(ticks + TS_TICKS_PER_MOVE)),
                        ("ticksPerStep".to_string(), JsVal::Num(TS_TICKS_PER_MOVE)),
                        ("path".to_string(), JsVal::Arr(path_vals)),
                    ]);
                    self.mg_record_motion_plan(tr, &plan); // 711
                    self.tds_motion_plan_dst = Some(dst);
                }
                // Update safeFromPirates status.
                if self.mg_is_water(tr, node) && self.mg_is_shoreline(tr, node) {
                    // 508, 901
                    self.un_set_safe_from_pirates(tr, ship); // 915
                }
                self.un_move(tr, ship, &JsVal::Num(node)); // 545
                self.tds_tiles_traveled += 1.0;
            }
            2 => {
                // PathStatus.COMPLETE
                self.trade_ship_complete(tr)?;
                return Ok(());
            }
            3 => {
                // PathStatus.NOT_FOUND — `console.warn` is untraced.
                if self.un_is_active(tr, ship) {
                    // 280
                    self.un_delete(tr, ship, false); // 285
                }
                self.tds_active = false;
                return Ok(());
            }
            _ => {}
        }
        Ok(())
    }

    /// `findClosestBy(ports, score, isCandidate)` — the candidate predicate runs
    /// first (matching `findMinimumBy`), then the distance score.
    fn trade_ship_nearest_port(
        &mut self,
        tr: &mut Vec<f64>,
        ports: &[usize],
        cur_tile: f64,
        my_component: &JsVal,
    ) -> Option<usize> {
        let mut best: Option<usize> = None;
        let mut best_score = f64::INFINITY;
        // `findMinimumBy` snapshots `values.length` once; iterating the slice
        // is equivalent.
        for &port in ports {
            let mut cand = self.un_is_active(tr, port); // 280
            if cand {
                cand = !self.un_is_marked_for_deletion(tr, port); // 913
            }
            if cand {
                cand = !self.un_is_under_construction(tr, port); // 345
            }
            if cand {
                cand = !matches!(my_component, JsVal::Null);
            }
            if cand {
                let pt = self.un_tile(tr, port); // 344
                cand = self.mg_has_water_component(tr, pt, my_component); // 302
            }
            if cand {
                let pt = self.un_tile(tr, port); // 344
                let score = self.mg_manhattan_dist(tr, pt, cur_tile); // 509
                if score < best_score {
                    best_score = score;
                    best = Some(port);
                }
            }
        }
        best
    }

    fn trade_ship_complete(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        self.tds_active = false;
        let ship = self.tds_trade_ship.unwrap();
        self.un_delete(tr, ship, false); // 285
        let owner = self.un_owner(tr, ship).unwrap(); // 286
        let gold = self.mg_config_trade_ship_gold(tr, self.tds_tiles_traveled, owner); // 900

        if self.tds_was_captured && self.un_owner(tr, ship) == Some(self.tds_orig_owner) {
            // 286 — retaken by its original owner: the payout stands, nobody pirated.
            let dst_tile = self.un_tile(tr, self.tds_dst_port.unwrap()); // 344
            self.pl_add_gold(tr, self.tds_orig_owner, gold, dst_tile); // 910
        } else if self.tds_was_captured {
            let cap_owner = self.un_owner(tr, ship).unwrap(); // 286
            let dst_tile = self.un_tile(tr, self.tds_dst_port.unwrap()); // 344
            self.pl_add_gold(tr, cap_owner, gold, dst_tile); // 910
            let piracy_owner = self.un_owner(tr, ship).unwrap(); // 286
            self.pl_add_piracy_gold(tr, piracy_owner, gold); // 911
            let disp_owner = self.un_owner(tr, ship).unwrap(); // 286
            let id = self.pl_id(tr, disp_owner); // 231
            let name = self.pl_display_name(tr, self.tds_orig_owner); // 232
            let rendered = crate::utils_format::render_number(gold, 0.0, 0.0);
            let orig_id = self.pl_id(tr, self.tds_orig_owner); // 231
            self.mg_display_message(
                tr,
                &[
                    JsVal::Str("events_display.received_gold_from_captured_ship".to_string()),
                    JsVal::Num(MSG_CAPTURED_ENEMY_UNIT),
                    JsVal::Str(id),
                    JsVal::Num(gold),
                    JsVal::Obj(vec![
                        ("gold".to_string(), JsVal::Str(rendered)),
                        ("name".to_string(), JsVal::Str(name)),
                    ]),
                    JsVal::Undef,
                    JsVal::Str(orig_id),
                ],
            ); // 226
            let stats_owner = self.un_owner(tr, ship).unwrap(); // 286
            self.mg_stats_boat_captured_trade(tr, stats_owner, self.tds_orig_owner, gold); // 903
        } else {
            let src_owner = self.un_owner(tr, self.tds_src_port.unwrap()).unwrap(); // 286
            let src_tile = self.un_tile(tr, self.tds_src_port.unwrap()); // 344
            self.pl_add_gold(tr, src_owner, gold, src_tile); // 910
            let dst_owner = self.un_owner(tr, self.tds_dst_port.unwrap()).unwrap(); // 286
            let dst_tile = self.un_tile(tr, self.tds_dst_port.unwrap()); // 344
            self.pl_add_gold(tr, dst_owner, gold, dst_tile); // 910
            let src_owner2 = self.un_owner(tr, self.tds_src_port.unwrap()).unwrap(); // 286
            self.pl_add_trade_gold(tr, src_owner2, gold); // 912
            let dst_owner2 = self.un_owner(tr, self.tds_dst_port.unwrap()).unwrap(); // 286
            self.pl_add_trade_gold(tr, dst_owner2, gold); // 912
            let s_owner = self.un_owner(tr, self.tds_src_port.unwrap()).unwrap(); // 286
            let d_owner = self.un_owner(tr, self.tds_dst_port.unwrap()).unwrap(); // 286
            self.mg_stats_boat_arrive_trade(tr, s_owner, d_owner, gold); // 904
        }
        Ok(())
    }

    // ---- G5i: `TrainStationExecution` (class 39) ----------------------------

    fn train_station_execution(
        &mut self,
        kind: u8,
        ticks: f64,
        tr: &mut Vec<f64>,
    ) -> Result<JsVal, String> {
        match kind {
            3 => return Ok(JsVal::Bool(self.tss_active)),
            // `activeDuringSpawnPhase()` -> false.
            4 => return Ok(JsVal::Bool(false)),
            1 => {
                // `init`: stores `mg`; seeds the spawn random when the station
                // spawns trains (`new PseudoRandom(mg.ticks())`).
                self.mg_ready = true;
                if self.tss_spawn_trains == Some(true) {
                    let seed = self.mg_ticks(tr); // 212
                    self.tss_random = Some(PseudoRandom::new(seed));
                }
                return Ok(JsVal::Undef);
            }
            2 => {}
            _ => return Ok(JsVal::Undef),
        }
        self.train_station_tick(ticks, tr)?;
        Ok(JsVal::Undef)
    }

    fn train_station_tick(&mut self, ticks: f64, tr: &mut Vec<f64>) -> Result<(), String> {
        if !self.mg_ready {
            return Err("Not initialized".to_string());
        }
        if !self.tss_active {
            return Ok(());
        }
        if self.tss_station.is_none() {
            // Can't create new executions on init, so it has to be done in the tick.
            let utok = self.a_tok;
            self.mg_connect_station(tr, utok); // 916
            self.tss_station = Some(utok);
        }
        let station = self.tss_station.unwrap();
        if !self.un_is_active(tr, station) {
            // 280 — `station.isActive()` delegates to `unit.isActive()`.
            self.tss_active = false;
            return Ok(());
        }
        if self.tss_spawn_trains == Some(true) {
            self.tss_spawn_train(tr, station, ticks)?;
        }
        Ok(())
    }

    fn tss_spawn_train(
        &mut self,
        tr: &mut Vec<f64>,
        station: usize,
        current_tick: f64,
    ) -> Result<(), String> {
        if !self.mg_ready || self.tss_random.is_none() {
            return Err("Not initialized".to_string());
        }
        if current_tick < self.tss_last_spawn_tick + self.tss_ticks_cooldown {
            return Ok(());
        }
        let cluster = self.st_get_cluster(tr, station); // 920
        let Some(ctok) = cluster else { return Ok(()) };
        let owner = self.un_owner(tr, self.a_tok).unwrap(); // 286
        if !self.cluster_has_any_trade_dest(tr, ctok, owner) {
            // 923
            return Ok(());
        }
        if !self.tss_should_spawn_train(tr) {
            return Ok(());
        }
        let mut random = self.tss_random.take().unwrap();
        let destination = self.cluster_random_trade_dest(tr, ctok, owner, &mut random); // 924
        self.tss_random = Some(random);
        let Some(dest) = destination else { return Ok(()) };
        if dest == station {
            return Ok(());
        }
        self.mg_add_train_execution(tr, owner, station, dest, self.tss_num_cars); // 307 tag 17
        self.tss_last_spawn_tick = current_tick;
        Ok(())
    }

    fn tss_should_spawn_train(&mut self, tr: &mut Vec<f64>) -> bool {
        let owner = self.un_owner(tr, self.a_tok).unwrap(); // 286
        let factory_count = self.pl_unit_count(tr, owner, UNIT_FACTORY); // 823
        let train_count = self.mg_unit_count(tr, UNIT_TRAIN); // 506
        let spawn_rate = self.mg_train_spawn_rate(tr, factory_count, train_count); // 918
        // `for (let i = 0; i < this.unit!.level(); i++)` re-reads `level()` (540)
        // on EVERY condition check, including the final failing one.
        let mut random = self.tss_random.take().unwrap();
        let mut i = 0.0;
        let result = loop {
            let level = self.un_level(tr, self.a_tok); // 540
            // JS `i < level()` is false for NaN, so the negation must stay a
            // bool negation (not `>=`).
            let below = i < level;
            if !below {
                break false;
            }
            if random.chance(spawn_rate) {
                break true;
            }
            i += 1.0;
        };
        self.tss_random = Some(random);
        result
    }

    // ---- G5i: `TrainExecution` (class 40) -----------------------------------

    fn train_execution(
        &mut self,
        kind: u8,
        ticks: f64,
        tr: &mut Vec<f64>,
    ) -> Result<JsVal, String> {
        match kind {
            3 => return Ok(JsVal::Bool(self.tex_active)),
            // `activeDuringSpawnPhase()` -> false.
            4 => return Ok(JsVal::Bool(false)),
            1 => {
                self.train_exec_init(ticks, tr)?;
                return Ok(JsVal::Undef);
            }
            5 => {
                self.train_load_cargo(tr);
                return Ok(JsVal::Undef);
            }
            2 => {}
            _ => return Ok(JsVal::Undef),
        }
        self.train_tick(tr)?;
        Ok(JsVal::Undef)
    }

    /// `getOrientedRailroad(from, to)`: station `from`'s scripted
    /// `getRailroadTo(to)` (921) resolved against the railroads table; the
    /// `forward` flag flips the stored tile list when the train runs the
    /// segment against its stored direction.
    fn get_oriented_railroad(
        &mut self,
        tr: &mut Vec<f64>,
        from_utok: usize,
        to_utok: usize,
    ) -> Option<(usize, Vec<f64>)> {
        let rail_tok = self.st_get_railroad_to(tr, from_utok, to_utok)?; // 921
        let r = &self.el_railroads[rail_tok];
        let forward = r.to_utok == to_utok;
        let tiles = if forward {
            r.tiles.clone()
        } else {
            r.tiles.iter().rev().cloned().collect()
        };
        Some((rail_tok, tiles))
    }

    fn train_exec_init(&mut self, ticks: f64, tr: &mut Vec<f64>) -> Result<(), String> {
        self.mg_ready = true;
        let stations = self.mg_find_stations_path(tr, self.tex_source, self.tex_destination); // 917
        let Some(stations) = stations.filter(|s| s.len() > 1) else {
            self.tex_active = false;
            return Ok(());
        };
        self.tex_stations = stations;
        let railroad = self.get_oriented_railroad(tr, self.tex_stations[0], self.tex_stations[1]);
        match railroad {
            Some(r) => self.tex_current_railroad = Some(r),
            None => {
                self.tex_active = false;
                return Ok(());
            }
        }
        let station_tile = self.un_tile(tr, self.tex_stations[0]); // 344
        let spawn = self.pl_can_build(tr, self.a_tok, UNIT_TRAIN, station_tile); // 534
        if matches!(spawn, JsVal::Bool(false)) {
            // `console.warn("cannot build train")` is untraced.
            self.tex_active = false;
            return Ok(());
        }
        let spawn_tile = as_num(&spawn);
        self.train_create_units(tr, spawn_tile);
        let car_unit_ids: Vec<JsVal> = self
            .tex_cars
            .clone()
            .iter()
            .map(|&c| JsVal::Num(self.un_id(tr, c))) // 340
            .collect();
        let mut path_tiles: Vec<f64> = Vec::new();
        for i in 0..self.tex_stations.len().saturating_sub(1) {
            let from = self.tex_stations[i];
            let to = self.tex_stations[i + 1];
            let segment = self.get_oriented_railroad(tr, from, to);
            match segment {
                Some((_, tiles)) => path_tiles.extend_from_slice(&tiles),
                None => {
                    self.tex_active = false;
                    return Ok(());
                }
            }
        }
        let start_tile = self.un_tile(tr, self.tex_train.unwrap()); // 344
        if path_tiles.is_empty() || path_tiles[0] != start_tile {
            path_tiles.insert(0, start_tile);
            self.tex_path_index = 1.0;
        }
        self.tex_path_tiles = path_tiles.clone();
        let plan = JsVal::Obj(vec![
            ("kind".to_string(), JsVal::Str("train".to_string())),
            ("engineUnitId".to_string(), JsVal::Num(self.un_id(tr, self.tex_train.unwrap()))), // 340
            ("carUnitIds".to_string(), JsVal::Arr(car_unit_ids)),
            ("planId".to_string(), JsVal::Num(1.0)),
            ("startTick".to_string(), JsVal::Num(ticks + 1.0)),
            ("speed".to_string(), JsVal::Num(self.tex_speed)),
            ("spacing".to_string(), JsVal::Num(self.tex_spacing)),
            ("path".to_string(), JsVal::Arr(path_tiles.iter().map(|t| JsVal::Num(*t)).collect())),
        ]);
        self.mg_record_motion_plan(tr, &plan); // 711
        Ok(())
    }

    /// `createTrainUnits(tile)`: engine, tail engine, then `numCars` carriages.
    fn train_create_units(&mut self, tr: &mut Vec<f64>, tile: f64) {
        let engine_params = JsVal::Obj(vec![
            ("targetUnit".to_string(), JsVal::Num(self.tex_destination as f64)),
            ("trainType".to_string(), JsVal::Str("Engine".to_string())),
        ]);
        self.tex_train = self.pl_build_unit(tr, self.a_tok, UNIT_TRAIN, tile, &engine_params); // 535
        let tail_params = JsVal::Obj(vec![
            ("targetUnit".to_string(), JsVal::Num(self.tex_destination as f64)),
            ("trainType".to_string(), JsVal::Str("TailEngine".to_string())),
        ]);
        if let Some(t) = self.pl_build_unit(tr, self.a_tok, UNIT_TRAIN, tile, &tail_params) {
            // 535
            self.tex_cars.push(t);
        }
        let mut i = 0.0;
        while i < self.tex_num_cars {
            let car_params = JsVal::Obj(vec![
                ("trainType".to_string(), JsVal::Str("Carriage".to_string())),
                ("loaded".to_string(), JsVal::Bool(self.tex_has_cargo)),
            ]);
            if let Some(c) = self.pl_build_unit(tr, self.a_tok, UNIT_TRAIN, tile, &car_params) {
                // 535
                self.tex_cars.push(c);
            }
            i += 1.0;
        }
    }

    fn train_tick(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        if self.tex_train.is_none() {
            return Err("Not initialized".to_string());
        }
        let train = self.tex_train.unwrap();
        if !self.un_is_active(tr, train) || !self.train_active_source_or_destination(tr) {
            // 280 (+ station 280s)
            self.train_delete_train(tr);
            return Ok(());
        }
        match self.train_get_next_tile(tr) {
            Some(tile) => self.train_update_cars_positions(tr, tile),
            None => {
                self.train_target_reached(tr);
                self.train_delete_train(tr);
            }
        }
        Ok(())
    }

    fn train_load_cargo(&mut self, tr: &mut Vec<f64>) {
        if self.tex_has_cargo || self.tex_train.is_none() {
            return;
        }
        self.tex_has_cargo = true;
        // Starts at 1: don't load the tail engine.
        let cars = self.tex_cars.clone();
        for &car in cars.iter().skip(1) {
            self.un_set_loaded(tr, car, true); // 927
        }
    }

    fn train_target_reached(&mut self, tr: &mut Vec<f64>) {
        let Some(train) = self.tex_train else { return };
        self.un_set_reached_target(tr, train); // 544
        let cars = self.tex_cars.clone();
        for car in cars {
            self.un_set_reached_target(tr, car); // 544
        }
    }

    fn train_delete_train(&mut self, tr: &mut Vec<f64>) {
        self.tex_active = false;
        if let Some(train) = self.tex_train {
            if self.un_is_active(tr, train) {
                // 280
                self.un_delete(tr, train, false); // 285
            }
        }
        let cars = self.tex_cars.clone();
        for car in cars {
            if self.un_is_active(tr, car) {
                // 280
                self.un_delete(tr, car, false); // 285
            }
        }
    }

    fn train_active_source_or_destination(&mut self, tr: &mut Vec<f64>) -> bool {
        self.tex_stations.len() > 1
            && self.un_is_active(tr, self.tex_stations[1]) // 280
            && self.un_is_active(tr, self.tex_stations[0]) // 280
    }

    fn train_save_traversed_tiles(&mut self, tr: &mut Vec<f64>, from: f64, speed: f64) {
        let Some((_, tiles)) = &self.tex_current_railroad else {
            return;
        };
        let tiles = tiles.clone();
        let mut tile_to_save = from;
        let mut i = 0.0;
        while i < speed && tile_to_save < tiles.len() as f64 {
            self.train_save_tile(tiles[tile_to_save as usize]);
            tile_to_save += 1.0;
            i += 1.0;
        }
        let _ = tr;
    }

    fn train_save_tile(&mut self, tile: f64) {
        self.tex_used_tiles.push(tile);
        if self.tex_used_tiles.len() as f64 > self.tex_cars.len() as f64 * self.tex_spacing + 3.0 {
            self.tex_used_tiles.remove(0);
        }
    }

    fn train_update_cars_positions(&mut self, tr: &mut Vec<f64>, new_tile: f64) {
        if !self.tex_cars.is_empty() {
            for i in (0..self.tex_cars.len()).rev() {
                let car_tile_index = (i + 1) as f64 * self.tex_spacing + 2.0;
                if self.tex_used_tiles.len() as f64 > car_tile_index {
                    let t = self.tex_used_tiles[car_tile_index as usize];
                    self.un_move(tr, self.tex_cars[i], &JsVal::Num(t)); // 545
                }
            }
        }
        if let Some(train) = self.tex_train {
            self.un_move(tr, train, &JsVal::Num(new_tile)); // 545
        }
    }

    fn train_next_station(&mut self, tr: &mut Vec<f64>) -> bool {
        if self.tex_stations.len() > 2 {
            let cur_len = self
                .tex_current_railroad
                .as_ref()
                .map(|(_, t)| t.len() as f64)
                .unwrap_or(0.0);
            self.tex_path_index += cur_len;
            self.tex_stations.remove(0);
            let railroad = self
                .get_oriented_railroad(tr, self.tex_stations[0], self.tex_stations[1])
                .or_else(|| self.train_resolve_split_railroad(tr));
            if let Some(r) = railroad {
                self.tex_current_railroad = Some(r);
                return true;
            }
        }
        false
    }

    fn train_resolve_split_railroad(&mut self, tr: &mut Vec<f64>) -> Option<(usize, Vec<f64>)> {
        let station0 = self.tex_stations[0];
        let station1 = self.tex_stations[1];
        let path = self.mg_find_stations_path(tr, station0, station1)?; // 917
        if path.len() <= 2 {
            return None;
        }
        let mut cursor = self.tex_path_index as usize;
        for i in 0..path.len() - 1 {
            let segment = self.get_oriented_railroad(tr, path[i], path[i + 1])?;
            for tile in &segment.1 {
                if self.tex_path_tiles.get(cursor).copied() != Some(*tile) {
                    return None;
                }
                cursor += 1;
            }
        }
        // `this.stations.splice(0, 2, ...path)`.
        self.tex_stations.splice(0..2, path.iter().copied());
        self.get_oriented_railroad(tr, self.tex_stations[0], self.tex_stations[1])
    }

    fn train_can_trade_with_destination(&mut self, tr: &mut Vec<f64>) -> bool {
        self.tex_stations.len() > 1
            && self.station_trade_available(tr, self.tex_stations[1], self.a_tok) // 286 (+ 533)
    }

    fn train_get_next_tile(&mut self, tr: &mut Vec<f64>) -> Option<f64> {
        if self.tex_current_railroad.is_none() || !self.train_can_trade_with_destination(tr) {
            return None;
        }
        self.train_save_traversed_tiles(tr, self.tex_current_tile, self.tex_speed);
        self.tex_current_tile += self.tex_speed;
        let tiles_len = self.tex_current_railroad.as_ref().unwrap().1.len() as f64;
        let left_over = self.tex_current_tile - tiles_len;
        if left_over >= 0.0 {
            // Station reached, pick the next station.
            self.train_station_reached(tr);
            if !self.train_next_station(tr) {
                return None; // Destination reached (or no valid connection).
            }
            self.tex_current_tile = left_over;
            self.train_save_traversed_tiles(tr, 0.0, left_over);
        }
        let cur = self.tex_current_tile as usize;
        self.tex_current_railroad
            .as_ref()
            .map(|(_, tiles)| tiles[cur])
    }

    fn train_station_reached(&mut self, tr: &mut Vec<f64>) {
        if !self.mg_ready {
            return;
        }
        let station = self.tex_stations[1];
        self.st_on_train_stop(tr, station); // 922
        let station_type = self.un_type(tr, station); // 283
        if station_type == UNIT_CITY || station_type == UNIT_PORT {
            self.tex_trade_stops += 1.0;
        }
    }

    // ---- G5k facade accessors (codes ≥ 958) --------------------------------

    /// `mg.config().disableAlliances()` (958).
    fn mg_disable_alliances(&mut self, tr: &mut Vec<f64>) -> bool {
        let v = mk_take(&mut self.mg, MG_DISABLE_ALLIANCES, tr, 958.0, &[]);
        as_bool(&v)
    }
    /// `mg.config().donateTroops()` (959).
    fn mg_donate_troops_cfg(&mut self, tr: &mut Vec<f64>) -> bool {
        let v = mk_take(&mut self.mg, MG_DONATE_TROOPS_CFG, tr, 959.0, &[]);
        as_bool(&v)
    }
    /// `mg.config().gameConfig().playerTeams` (960).
    fn mg_player_teams(&mut self, tr: &mut Vec<f64>) -> JsVal {
        mk_take(&mut self.mg, MG_PLAYER_TEAMS, tr, 960.0, &[])
    }
    /// `mg.isShore(tile)` (967) — the `Game`-facade shore test used by
    /// `AiAttackBehavior.shoreTiles`.
    fn mg_is_shore(&mut self, tr: &mut Vec<f64>, tile: f64) -> bool {
        let v = mk_take(&mut self.mg, MG_IS_SHORE, tr, 967.0, &ev1(tile));
        as_bool(&v)
    }
    /// `mg.__closestShoreByWater(player, tile)` (961) — the scripted
    /// `SpatialQuery.closestShoreByWater` facade.
    fn mg_closest_shore_by_water(
        &mut self,
        tr: &mut Vec<f64>,
        player: usize,
        tile: f64,
    ) -> Option<f64> {
        let mut p = evtok(Some(player));
        p.extend_from_slice(&ev1(tile));
        let v = mk_take(&mut self.mg, MG_CLOSEST_SHORE_BY_WATER, tr, 961.0, &p);
        as_opt_num(Some(&v))
    }
    /// `player.sharesBorderWith(other)` (962).
    fn pl_shares_border_with(
        &mut self,
        tr: &mut Vec<f64>,
        tok: usize,
        other: usize,
    ) -> bool {
        let v = self.pl(tok, PL_SHARES_BORDER_WITH, tr, 962.0, &evtok(Some(other)));
        as_bool(&v)
    }
    /// `player.targets()` (963) — a player-token list. Used only by the
    /// `assistAllies` strategy arm, which is unreachable for TribeExecution.
    #[allow(dead_code)]
    fn pl_targets(&mut self, tr: &mut Vec<f64>, tok: usize) -> Vec<usize> {
        let v = self.pl(tok, PL_TARGETS, tr, 963.0, &[]);
        as_tok_list(&v)
    }
    /// `player.allies()` (964) — a player-token list.
    fn pl_allies(&mut self, tr: &mut Vec<f64>, tok: usize) -> Vec<usize> {
        let v = self.pl(tok, PL_ALLIES, tr, 964.0, &[]);
        as_tok_list(&v)
    }
    /// `player.allRelationsSorted()` (965) — `[{relation, player}, ...]`.
    fn pl_all_relations_sorted(
        &mut self,
        tr: &mut Vec<f64>,
        tok: usize,
    ) -> Vec<(f64, usize)> {
        let v = self.pl(tok, PL_ALL_RELATIONS_SORTED, tr, 965.0, &[]);
        match &v {
            JsVal::Arr(items) => items
                .iter()
                .filter_map(|it| {
                    let rel = val_field(it, "relation").map(as_num)?;
                    let p = val_field(it, "player").and_then(as_opt_tok)?;
                    Some((rel, p))
                })
                .collect(),
            _ => Vec::new(),
        }
    }
    /// `alliance.other(player)` (966) — the other alliance member's player token.
    fn al_other(&mut self, tr: &mut Vec<f64>, al_tok: usize, player: usize) -> usize {
        let mut prefix = vec![al_tok as f64];
        prefix.extend_from_slice(&evtok(Some(player)));
        let v = mk_take(&mut self.alliances[al_tok], AL_OTHER, tr, 966.0, &prefix);
        as_num(&v) as usize
    }

    // ---- G5k Step 2 facade accessors (codes ≥ 968) ---------------------------

    /// `mg.units(...types)` — the typed form (968), a unit-token list.
    fn mg_units_typed(&mut self, tr: &mut Vec<f64>, types: &[String]) -> Vec<usize> {
        let v = mk_take(&mut self.mg, MG_UNITS_TYPED, tr, 968.0, &ev_strs(types));
        as_tok_list(&v)
    }
    /// `game.unitInfo(ty).cost(game, player)` (969) — gold as the f64 side.
    fn mg_unit_info_cost(&mut self, tr: &mut Vec<f64>, ty: &str, player: usize) -> i64 {
        let mut prefix = evs(ty);
        prefix.extend_from_slice(&evtok(Some(player)));
        let v = mk_take(&mut self.mg, MG_UNIT_INFO_COST, tr, 969.0, &prefix);
        as_num(&v) as i64
    }
    /// `mg.config().startingGold(info)` (970) — the PlayerInfo object is
    /// encoded verbatim (the mock returns the scripted number).
    fn mg_starting_gold(&mut self, tr: &mut Vec<f64>, info: &JsVal) -> i64 {
        let prefix = ev_enc(info);
        let v = mk_take(&mut self.mg, MG_STARTING_GOLD, tr, 970.0, &prefix);
        as_num(&v) as i64
    }
    /// `mg.nations()` (971) — a player-token list.
    fn mg_nations(&mut self, tr: &mut Vec<f64>) -> Vec<usize> {
        let v = mk_take(&mut self.mg, MG_NATIONS, tr, 971.0, &[]);
        as_tok_list(&v)
    }
    /// `mg.isOnMap({x, y})` (972) — the Cell object encodes as `{x, y}`.
    fn mg_is_on_map(&mut self, tr: &mut Vec<f64>, x: f64, y: f64) -> bool {
        let cell = JsVal::Obj(vec![
            ("x".to_string(), JsVal::Num(x)),
            ("y".to_string(), JsVal::Num(y)),
        ]);
        let prefix = ev_enc(&cell);
        let v = mk_take(&mut self.mg, MG_IS_ON_MAP, tr, 972.0, &prefix);
        as_bool(&v)
    }
    /// `mg.config().trainStationMinRange()` (973).
    fn mg_train_station_min_range(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(&mut self.mg, MG_TRAIN_STATION_MIN_RANGE, tr, 973.0, &[]);
        as_num(&v)
    }
    /// `mg.config().samRange(level)` (974).
    fn mg_sam_range(&mut self, tr: &mut Vec<f64>, level: f64) -> f64 {
        let v = mk_take(&mut self.mg, MG_SAM_RANGE, tr, 974.0, &ev1(level));
        as_num(&v)
    }
    /// `mg.config().defaultSamRange()` (975).
    fn mg_default_sam_range(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = mk_take(&mut self.mg, MG_DEFAULT_SAM_RANGE, tr, 975.0, &[]);
        as_num(&v)
    }
    /// `mg.config().trainGold(kind, troops, player)` (976) — a Gold number.
    fn mg_train_gold(&mut self, tr: &mut Vec<f64>, kind: &str, troops: f64, player: usize) -> f64 {
        let mut prefix = evs(kind);
        prefix.extend_from_slice(&ev1(troops));
        prefix.extend_from_slice(&evtok(Some(player)));
        let v = mk_take(&mut self.mg, MG_TRAIN_GOLD, tr, 976.0, &prefix);
        as_num(&v)
    }
    /// `mg.sharedWaterComponents(player)` (977) — a component-id list or null.
    fn mg_shared_water_components(&mut self, tr: &mut Vec<f64>, player: usize) -> Option<Vec<f64>> {
        let v = mk_take(&mut self.mg, MG_SHARED_WATER_COMPONENTS, tr, 977.0, &evtok(Some(player)));
        match v {
            JsVal::Null | JsVal::Undef | JsVal::Absent => None,
            JsVal::Arr(items) => Some(items.iter().map(as_num).collect()),
            _ => None,
        }
    }
    /// `mg.getWaterComponentSize(tile)` (978) — a number or null.
    fn mg_get_water_component_size(&mut self, tr: &mut Vec<f64>, tile: f64) -> Option<f64> {
        let v = mk_take(&mut self.mg, MG_GET_WATER_COMPONENT_SIZE, tr, 978.0, &ev1(tile));
        match v {
            JsVal::Null | JsVal::Undef | JsVal::Absent => None,
            JsVal::Num(n) => Some(n),
            _ => None,
        }
    }
    /// `mg.railNetwork().stationManager().getAll()` (979) — station identity is
    /// the unit token, so this yields a unit-token list.
    fn mg_station_manager_get_all(&mut self, tr: &mut Vec<f64>) -> Vec<usize> {
        let v = mk_take(&mut self.mg, MG_STATION_MANAGER_GET_ALL, tr, 979.0, &[]);
        as_tok_list(&v)
    }
    /// `mg.magnitude(tile)` (980) — grid-delegated when `spec.map` exists.
    fn mg_magnitude(&mut self, tr: &mut Vec<f64>, tile: f64) -> f64 {
        if let Some(ref map) = self.el_map {
            let v = map.magnitude(tile);
            tr.push(980.0);
            tr.extend_from_slice(&ev1(tile));
            tr.extend_from_slice(&ev1(v));
            return v;
        }
        let v = mk_take(&mut self.mg, MG_MAGNITUDE, tr, 980.0, &ev1(tile));
        as_num(&v)
    }
    /// `mg.isOcean(tile)` (981) — grid-delegated when `spec.map` exists.
    fn mg_is_ocean(&mut self, tr: &mut Vec<f64>, tile: f64) -> bool {
        if let Some(ref map) = self.el_map {
            let v = map.is_ocean(tile);
            tr.push(981.0);
            tr.extend_from_slice(&ev1(tile));
            tr.extend_from_slice(&evb(v));
            return v;
        }
        let v = mk_take(&mut self.mg, MG_IS_OCEAN, tr, 981.0, &ev1(tile));
        as_bool(&v)
    }
    /// `player.unitsConstructed(ty)` (982) — a truthy number in TS.
    fn pl_units_constructed(&mut self, tr: &mut Vec<f64>, tok: usize, ty: &str) -> f64 {
        let v = self.pl(tok, PL_UNITS_CONSTRUCTED, tr, 982.0, &evs(ty));
        as_num(&v)
    }
    /// `player.unitsOwned(ty)` (983).
    fn pl_units_owned(&mut self, tr: &mut Vec<f64>, tok: usize, ty: &str) -> f64 {
        let v = self.pl(tok, PL_UNITS_OWNED, tr, 983.0, &evs(ty));
        as_num(&v)
    }
    /// `player.info()` (984) — the plain PlayerInfo object.
    fn pl_info(&mut self, tr: &mut Vec<f64>, tok: usize) -> JsVal {
        self.pl(tok, PL_INFO, tr, 984.0, &[])
    }
    /// `unit.wasDestroyedByEnemy()` (985).
    fn un_was_destroyed_by_enemy(&mut self, tr: &mut Vec<f64>, uid: usize) -> bool {
        let v = self.un(uid, UN_WAS_DESTROYED_BY_ENEMY, tr, 985.0, &[]);
        as_bool(&v)
    }
    /// `unit.destroyer()` (986) — a unit token or null.
    fn un_destroyer(&mut self, tr: &mut Vec<f64>, uid: usize) -> Option<usize> {
        let v = self.un(uid, UN_DESTROYER, tr, 986.0, &[]);
        as_opt_tok(&v)
    }
    /// `player.isAlliedWith(other)` (987).
    fn pl_is_allied_with(&mut self, tr: &mut Vec<f64>, tok: usize, other: usize) -> bool {
        let v = self.pl(tok, PL_IS_ALLIED_WITH, tr, 987.0, &evtok(Some(other)));
        as_bool(&v)
    }

    // ---- G5k construction records (code 307) --------------------------------

    /// `mg.addExecution(new TransportShipExecution(player, dst, troops))` (tag 18).
    fn mg_add_transport_ship_exec(
        &mut self,
        tr: &mut Vec<f64>,
        player: usize,
        dst: f64,
        troops: f64,
    ) {
        tr.push(307.0);
        tr.push(TAG_TRANSPORT_SHIP_EXEC);
        push_val(tr, &JsVal::Num(player as f64));
        push_val(tr, &JsVal::Num(dst));
        push_val(tr, &JsVal::Num(troops));
        push_val(tr, &JsVal::Undef);
    }
    /// `mg.addExecution(new DonateTroopsExecution(sender, recipientID, troops))` (tag 19).
    fn mg_add_donate_troops_exec(
        &mut self,
        tr: &mut Vec<f64>,
        sender: usize,
        recipient_id: &str,
        troops: Option<f64>,
    ) {
        tr.push(307.0);
        tr.push(TAG_DONATE_TROOPS_EXEC);
        push_val(tr, &JsVal::Num(sender as f64));
        push_val(tr, &JsVal::Str(recipient_id.to_string()));
        push_val(
            tr,
            &match troops {
                Some(t) => JsVal::Num(t),
                None => JsVal::Null,
            },
        );
        push_val(tr, &JsVal::Undef);
    }
    /// `mg.addExecution(new AllianceExtensionExecution(from, toID))` (tag 20).
    fn mg_add_alliance_extension_exec(
        &mut self,
        tr: &mut Vec<f64>,
        from: usize,
        to_id: &str,
    ) {
        tr.push(307.0);
        tr.push(TAG_ALLIANCE_EXTENSION_EXEC);
        push_val(tr, &JsVal::Num(from as f64));
        push_val(tr, &JsVal::Str(to_id.to_string()));
        push_val(tr, &JsVal::Undef);
    }
    /// `mg.addExecution(new DeleteUnitExecution(player, unitId))` (tag 21).
    fn mg_add_delete_unit_exec(&mut self, tr: &mut Vec<f64>, player: usize, unit_id: f64) {
        tr.push(307.0);
        tr.push(TAG_DELETE_UNIT_EXEC);
        push_val(tr, &JsVal::Num(player as f64));
        push_val(tr, &JsVal::Num(unit_id));
        push_val(tr, &JsVal::Undef);
    }

    // ---- G5k Step 2 construction records (code 307, tags 22-25) --------------

    /// `mg.addExecution(new SpawnExecution(gameID, playerInfo, tile?))` (tag 22).
    fn mg_add_spawn_exec(
        &mut self,
        tr: &mut Vec<f64>,
        game_id: &str,
        player_info: &JsVal,
        tile: Option<f64>,
    ) {
        tr.push(307.0);
        tr.push(TAG_SPAWN_EXEC);
        push_val(tr, &JsVal::Str(game_id.to_string()));
        push_val(tr, player_info);
        push_val(
            tr,
            &match tile {
                Some(t) => JsVal::Num(t),
                None => JsVal::Undef,
            },
        );
        push_val(tr, &JsVal::Undef);
    }
    /// `mg.addExecution(new AllianceRequestExecution(requestor, recipientID))` (tag 23).
    fn mg_add_alliance_request_exec(
        &mut self,
        tr: &mut Vec<f64>,
        requestor: usize,
        recipient_id: &str,
    ) {
        tr.push(307.0);
        tr.push(TAG_ALLIANCE_REQUEST_EXEC);
        push_val(tr, &JsVal::Num(requestor as f64));
        push_val(tr, &JsVal::Str(recipient_id.to_string()));
        push_val(tr, &JsVal::Undef);
    }
    /// `mg.addExecution(new ConstructionExecution(player, type, tile,
    /// rocketDirectionUp?, amount?))` (tag 24).
    fn mg_add_construction_exec(
        &mut self,
        tr: &mut Vec<f64>,
        player: usize,
        construction_type: &str,
        tile: f64,
    ) {
        tr.push(307.0);
        tr.push(TAG_CONSTRUCTION_EXEC);
        push_val(tr, &JsVal::Num(player as f64));
        push_val(tr, &JsVal::Str(construction_type.to_string()));
        push_val(tr, &JsVal::Num(tile));
        push_val(tr, &JsVal::Undef);
        push_val(tr, &JsVal::Undef);
        push_val(tr, &JsVal::Undef);
    }
    /// `mg.addExecution(new UpgradeStructureExecution(player, unitId))` (tag 25).
    fn mg_add_upgrade_structure_exec(
        &mut self,
        tr: &mut Vec<f64>,
        player: usize,
        unit_id: f64,
    ) {
        tr.push(307.0);
        tr.push(TAG_UPGRADE_STRUCTURE_EXEC);
        push_val(tr, &JsVal::Num(player as f64));
        push_val(tr, &JsVal::Num(unit_id));
        push_val(tr, &JsVal::Undef);
    }

    // ---- G5j: `WarshipExecution` (class 41) ---------------------------------

    fn warship_execution(
        &mut self,
        kind: u8,
        _ticks: f64,
        tr: &mut Vec<f64>,
    ) -> Result<JsVal, String> {
        match kind {
            // `isActive()` — `this.warship?.isActive()` (undefined before init).
            3 => {
                return Ok(match self.ws_warship {
                    Some(w) => JsVal::Bool(self.un_is_active(tr, w)), // 280
                    None => JsVal::Undef,
                });
            }
            // `activeDuringSpawnPhase()` -> false.
            4 => return Ok(JsVal::Bool(false)),
            1 => {
                self.ws_init(tr);
                return Ok(JsVal::Undef);
            }
            2 => {}
            _ => return Ok(JsVal::Undef),
        }
        self.ws_tick(tr);
        Ok(JsVal::Undef)
    }

    /// `init(mg, ticks)`: seed the PRNG from `mg.ticks()` (212), then the
    /// `isUnit(input)` branch (737). The params-ctor path is dead in the
    /// capture (every scenario passes `elUnitRef(0)`), so a non-unit input
    /// leaves `ws_warship` unset and every later op is a no-op.
    fn ws_init(&mut self, tr: &mut Vec<f64>) {
        let seed = self.mg_ticks(tr); // 212
        self.ws_random = Some(PseudoRandom::new(seed));
        if self.un_is_unit(tr, self.a_tok) {
            // 737
            self.ws_warship = Some(self.a_tok);
        }
        if let Some(w) = self.ws_warship {
            let state = self.un_warship_state(tr, w); // 928
            self.last_observed_patrol_tile = as_opt_num(val_field(&state, "patrolTile"));
        }
    }

    /// `tick(ticks)`.
    fn ws_tick(&mut self, tr: &mut Vec<f64>) {
        let w = match self.ws_warship {
            Some(w) => w,
            None => return, // dead params-ctor path
        };
        if self.un_health(tr, w) <= 0.0 {
            // 621
            self.un_delete(tr, w, false); // 285 — `warship.delete()`
            return;
        }
        let state = self.un_warship_state(tr, w); // 928
        let is_in_combat = val_field(&state, "isInCombat").map(as_bool).unwrap_or(false);
        if self.last_emitted_combat && !is_in_combat {
            self.un_touch(tr, w); // 736
        }
        self.last_emitted_combat = is_in_combat;
        let health_before_healing = self.un_health(tr, w); // 621

        self.ws_heal_warship(tr);
        self.ws_handle_manual_patrol_override(tr);

        // A doomed side cannot repair its navy: undock / abort any retreat.
        let owner = self.un_owner(tr, w); // 286
        let doomed = match owner {
            Some(o) => self.pl_in_doomsday_clock(tr, o), // 940
            None => false,
        };
        if doomed {
            let state = self.un_warship_state(tr, w); // 928
            if g5j_ws_state(&state) != "patrolling" {
                self.ws_cancel_repair_retreat(tr, true);
            }
        }

        let state = self.un_warship_state(tr, w); // 928
        if g5j_ws_state(&state) == "docked" {
            if self.ws_current_retreat_port(tr).is_none() {
                self.ws_cancel_repair_retreat(tr, true);
            }
            if self.ws_is_fully_healed(tr) {
                self.ws_cancel_repair_retreat(tr, true);
            }
            let state = self.un_warship_state(tr, w); // 928
            if g5j_ws_state(&state) == "docked" {
                return;
            }
        }

        if self.ws_handle_repair_retreat(tr) {
            return;
        }

        // Priority 1: heal before doing anything else.
        if self.ws_should_start_repair_retreat(tr, health_before_healing) {
            self.ws_start_repair_retreat(tr);
            if self.ws_handle_repair_retreat(tr) {
                return;
            }
        }

        let target = self.ws_find_best_target(
            tr,
            &[UNIT_TRANSPORT_SHIP, UNIT_WARSHIP, UNIT_TRADE_SHIP],
            true,
        );
        self.un_set_target_unit(tr, w, target); // 914

        // Priorities 1/2/3 — each guard re-reads `targetUnit()?.type()`.
        let ty = match self.un_target_unit(tr, w) {
            // 735
            Some(t) => self.un_type(tr, t), // 283
            None => String::new(),
        };
        if ty == UNIT_TRANSPORT_SHIP {
            self.ws_shoot_target(tr);
            self.ws_patrol(tr);
            return;
        }
        let ty = match self.un_target_unit(tr, w) {
            Some(t) => self.un_type(tr, t),
            None => String::new(),
        };
        if ty == UNIT_WARSHIP {
            self.ws_shoot_target(tr);
            self.ws_patrol(tr);
            return;
        }
        let ty = match self.un_target_unit(tr, w) {
            Some(t) => self.un_type(tr, t),
            None => String::new(),
        };
        if ty == UNIT_TRADE_SHIP {
            self.ws_hunt_down_trade_ship(tr);
            return;
        }
        self.ws_patrol(tr);
    }

    /// `healWarship()`.
    fn ws_heal_warship(&mut self, tr: &mut Vec<f64>) {
        let w = self.ws_warship.unwrap();
        let owner = match self.un_owner(tr, w) {
            // 286
            Some(o) => o,
            None => return,
        };
        if self.pl_in_doomsday_clock(tr, owner) {
            // 940
            return;
        }
        let passive_healing = self.mg_warship_passive_healing(tr); // 930
        let range = self.mg_warship_passive_healing_range(tr); // 931
        let range_sq = range * range;
        let warship_tile = self.un_tile(tr, w); // 344
        let ports = self.pl_units(tr, owner, &[UNIT_PORT.to_string()]); // 241
        let mut is_near_port = false;
        for port in ports {
            let port_tile = self.un_tile(tr, port); // 344
            let d2 = self.mg_euclidean_dist_squared(tr, warship_tile, port_tile); // 710
            if d2 <= range_sq {
                is_near_port = true;
                break;
            }
        }
        if is_near_port {
            self.un_modify_health(tr, w, passive_healing, None); // 543
        }
        let state = self.un_warship_state(tr, w); // 928
        if g5j_ws_state(&state) == "docked" {
            self.ws_apply_active_docked_healing(tr);
        }
    }

    /// `isFullyHealed()`.
    fn ws_is_fully_healed(&mut self, tr: &mut Vec<f64>) -> bool {
        let w = self.ws_warship.unwrap();
        if !self.un_has_health(tr, w) {
            // 352
            return true;
        }
        self.un_health(tr, w) >= self.un_max_health(tr, w) // 621, 620
    }

    /// `handleManualPatrolOverride()`.
    fn ws_handle_manual_patrol_override(&mut self, tr: &mut Vec<f64>) {
        let w = self.ws_warship.unwrap();
        let state = self.un_warship_state(tr, w); // 928
        let patrol_tile = as_opt_num(val_field(&state, "patrolTile"));
        let changed = match self.last_observed_patrol_tile {
            Some(last) => patrol_tile != Some(last), // JS `!==` (undefined ≠ number)
            None => false,
        };
        if changed {
            self.last_manual_move_tick_retreat_disabled = self.mg_ticks(tr); // 212
            let state = self.un_warship_state(tr, w); // 928
            if g5j_ws_state(&state) != "patrolling" {
                self.ws_cancel_repair_retreat(tr, false);
            }
        }
        self.last_observed_patrol_tile = patrol_tile;
    }

    /// `cancelRepairRetreat(clearTargetTile)` — the 342 object keeps the TS key
    /// order `state` then `retreatPort`.
    fn ws_cancel_repair_retreat(&mut self, tr: &mut Vec<f64>, clear_target_tile: bool) {
        let w = self.ws_warship.unwrap();
        self.active_healing_remainder = 0.0;
        self.un_update_warship_state_obj(
            tr,
            w,
            &JsVal::Obj(vec![
                ("state".to_string(), JsVal::Str("patrolling".to_string())),
                ("retreatPort".to_string(), JsVal::Undef),
            ]),
        ); // 342
        if clear_target_tile {
            self.un_set_target_tile(tr, w); // 343 — undefined
        }
    }

    /// `shouldStartRepairRetreat(healthBeforeHealing)`.
    fn ws_should_start_repair_retreat(&mut self, tr: &mut Vec<f64>, health_before: f64) -> bool {
        let w = self.ws_warship.unwrap();
        let state = self.un_warship_state(tr, w); // 928
        if g5j_ws_state(&state) != "patrolling" {
            return false;
        }
        let owner = match self.un_owner(tr, w) {
            // 286
            Some(o) => o,
            None => return false,
        };
        if self.pl_in_doomsday_clock(tr, owner) {
            // 940
            return false;
        }
        let ticks = self.mg_ticks(tr); // 212
        if ticks - self.last_manual_move_tick_retreat_disabled < 50.0 {
            return false;
        }
        let max_health = self.un_max_health(tr, w); // 620
        let pct = self.mg_warship_retreat_health_percent(tr); // 932
        let retreat_threshold = (max_health * pct / 100.0).floor();
        if health_before >= retreat_threshold {
            return false;
        }
        let owner = match self.un_owner(tr, w) {
            // 286 — `this.warship.owner()` is read again for `units`.
            Some(o) => o,
            None => return false,
        };
        !self.pl_units(tr, owner, &[UNIT_PORT.to_string()]).is_empty() // 241
    }

    /// `findNearestPort()` — `findMinimumBy` checks the candidate predicate
    /// first, then the score, keeping the strict-`<` first-minimum.
    fn ws_find_nearest_port(&mut self, tr: &mut Vec<f64>) -> Option<f64> {
        let w = self.ws_warship.unwrap();
        let owner = self.un_owner(tr, w)?; // 286
        let ports = self.pl_units(tr, owner, &[UNIT_PORT.to_string()]); // 241
        if ports.is_empty() {
            return None;
        }
        let warship_tile = self.un_tile(tr, w); // 344
        let warship_comp = self.mg_get_water_component(tr, warship_tile); // 301
        // TS throws when the warship has no water component; unreached here.
        let mut best: Option<(usize, f64)> = None;
        for port in ports {
            let port_tile = self.un_tile(tr, port); // 344 (candidate closure)
            let port_comp = self.mg_get_water_component(tr, port_tile); // 301
            if !g5j_js_eq(&port_comp, &warship_comp) {
                continue;
            }
            let port_tile = self.un_tile(tr, port); // 344 (score closure)
            let score = self.mg_euclidean_dist_squared(tr, warship_tile, port_tile); // 710
            if best.is_none_or(|(_, bs)| score < bs) {
                best = Some((port, score));
            }
        }
        best.map(|(p, _)| self.un_tile(tr, p)) // 344 — `nearest?.tile()`
    }

    /// `startRepairRetreat()` — the 342 object keeps the TS key order
    /// `retreatPort` then `state`.
    fn ws_start_repair_retreat(&mut self, tr: &mut Vec<f64>) {
        let w = self.ws_warship.unwrap();
        let port_tile = match self.ws_find_nearest_port(tr) {
            Some(t) => t,
            None => return,
        };
        self.un_update_warship_state_obj(
            tr,
            w,
            &JsVal::Obj(vec![
                ("retreatPort".to_string(), JsVal::Num(port_tile)),
                ("state".to_string(), JsVal::Str("retreating".to_string())),
            ]),
        ); // 342
        self.active_healing_remainder = 0.0;
        self.un_set_target_unit(tr, w, None); // 914
    }

    /// `handleRepairRetreat()`.
    fn ws_handle_repair_retreat(&mut self, tr: &mut Vec<f64>) -> bool {
        let w = self.ws_warship.unwrap();
        let state = self.un_warship_state(tr, w); // 928
        if g5j_ws_state(&state) == "patrolling" {
            return false;
        }

        // Fire back at a transport / warship while retreating.
        let aggro = self.ws_find_best_target(tr, &[UNIT_TRANSPORT_SHIP, UNIT_WARSHIP], false);
        if aggro.is_some() {
            self.un_set_target_unit(tr, w, aggro); // 914
            self.ws_shoot_target(tr);
        }

        if !self.ws_refresh_retreat_port_tile(tr) {
            self.ws_cancel_repair_retreat(tr, true);
            return false;
        }

        // Only clear the target when there's no active aggro target this tick.
        if aggro.is_none() {
            self.un_set_target_unit(tr, w, None); // 914
        }

        let state = self.un_warship_state(tr, w); // 928
        let retreat_port = match as_opt_num(val_field(&state, "retreatPort")) {
            Some(t) => t,
            None => return false,
        };

        let docking_radius = self.mg_warship_docking_range(tr); // 935
        let docking_radius_sq = docking_radius * docking_radius;
        let warship_tile = self.un_tile(tr, w); // 344
        let dist_to_port = self.mg_euclidean_dist_squared(tr, warship_tile, retreat_port); // 710

        if dist_to_port <= docking_radius_sq {
            // In range — dock if the port still has healing capacity.
            let owner = self.un_owner(tr, w); // 286
            let ports = match owner {
                Some(o) => self.pl_units(tr, o, &[UNIT_PORT.to_string()]), // 241
                None => Vec::new(),
            };
            let mut port_at: Option<usize> = None;
            for p in ports {
                let pt = self.un_tile(tr, p); // 344 — `ports.find(...)`
                if pt == retreat_port {
                    port_at = Some(p);
                    break;
                }
            }
            let mut docked = false;
            if let Some(port) = port_at {
                if !self.ws_is_port_full_of_healing(tr, port, Some(w)) {
                    self.un_set_target_tile(tr, w); // 343 — undefined
                    self.un_update_warship_state_obj(
                        tr,
                        w,
                        &JsVal::Obj(vec![(
                            "state".to_string(),
                            JsVal::Str("docked".to_string()),
                        )]),
                    ); // 342
                    docked = true;
                }
            }
            if docked {
                return true;
            }
            // Port full — wait near the port, but leave once fully healed.
            if self.ws_is_fully_healed(tr) {
                self.ws_cancel_repair_retreat(tr, true);
                return false;
            }
            return true;
        }

        self.un_set_target_tile_num(tr, w, retreat_port); // 343 — number
        let warship_tile = self.un_tile(tr, w); // 344
        let result = self.mg_water_next(tr, warship_tile, retreat_port); // 862
        let status = val_field(&result, "status").map(as_num).unwrap_or(f64::NAN);
        let node = val_field(&result, "node").cloned().unwrap_or(JsVal::Null);
        match status as i32 {
            2 => {
                // PathStatus.COMPLETE
                self.un_move(tr, w, &node); // 545
                if let JsVal::Num(n) = node {
                    if n == retreat_port {
                        self.un_set_target_tile(tr, w); // 343 — undefined
                    }
                }
            }
            0 => {
                // PathStatus.NEXT
                self.un_move(tr, w, &node); // 545
            }
            3 => {
                // PathStatus.NOT_FOUND
                let new_port = self.ws_find_nearest_available_port_tile(tr);
                self.un_update_warship_state_obj(
                    tr,
                    w,
                    &JsVal::Obj(vec![match new_port {
                        Some(t) => ("retreatPort".to_string(), JsVal::Num(t)),
                        None => ("retreatPort".to_string(), JsVal::Undef),
                    }]),
                ); // 342
                if new_port.is_none() {
                    self.ws_cancel_repair_retreat(tr, true);
                }
            }
            _ => {}
        }
        true
    }

    /// `refreshRetreatPortTile()`.
    fn ws_refresh_retreat_port_tile(&mut self, tr: &mut Vec<f64>) -> bool {
        let w = self.ws_warship.unwrap();
        let owner = match self.un_owner(tr, w) {
            // 286
            Some(o) => o,
            None => return false,
        };
        let ports = self.pl_units(tr, owner, &[UNIT_PORT.to_string()]); // 241
        if ports.is_empty() {
            return false;
        }
        let state = self.un_warship_state(tr, w); // 928
        let current = as_opt_num(val_field(&state, "retreatPort"));
        let mut current_exists = false;
        if let Some(cur) = current {
            for &p in &ports {
                let pt = self.un_tile(tr, p); // 344 — `ports.some(...)`
                if pt == cur {
                    current_exists = true;
                    break;
                }
            }
        }
        if !current_exists {
            let new_port = self.ws_find_nearest_available_port_tile(tr);
            self.un_update_warship_state_obj(
                tr,
                w,
                &JsVal::Obj(vec![match new_port {
                    Some(t) => ("retreatPort".to_string(), JsVal::Num(t)),
                    None => ("retreatPort".to_string(), JsVal::Undef),
                }]),
            ); // 342
            return new_port.is_some();
        }
        let cur = current.unwrap();
        let mut current_port: Option<usize> = None;
        for &p in &ports {
            let pt = self.un_tile(tr, p); // 344 — `ports.find(...)`
            if pt == cur {
                current_port = Some(p);
                break;
            }
        }
        if let Some(port) = current_port {
            if self.ws_is_port_full_of_healing(tr, port, None) {
                // At healing capacity — look for an alternative.
                if let Some(alt) = self.ws_find_nearest_available_port(tr) {
                    self.un_update_warship_state_obj(
                        tr,
                        w,
                        &JsVal::Obj(vec![("retreatPort".to_string(), JsVal::Num(alt))]),
                    ); // 342
                }
                let state = self.un_warship_state(tr, w); // 928
                return as_opt_num(val_field(&state, "retreatPort")).is_some();
            }
        }
        if let Some(closer) = self.ws_find_better_port_tile(tr) {
            if closer != cur {
                self.un_update_warship_state_obj(
                    tr,
                    w,
                    &JsVal::Obj(vec![("retreatPort".to_string(), JsVal::Num(closer))]),
                ); // 342
                return true;
            }
        }
        true
    }

    /// `isPortFullOfHealing(port, excludeShip?)` — `level()` (540) is read
    /// BEFORE `dockedShipsAtPort`.
    fn ws_is_port_full_of_healing(
        &mut self,
        tr: &mut Vec<f64>,
        port: usize,
        exclude: Option<usize>,
    ) -> bool {
        let max_ships = self.un_level(tr, port); // 540
        let docked = self.ws_docked_ships_at_port(tr, port, exclude);
        docked.len() as f64 >= max_ships
    }

    /// `dockedShipsAtPort(port, excludeShip?)`.
    fn ws_docked_ships_at_port(
        &mut self,
        tr: &mut Vec<f64>,
        port: usize,
        exclude: Option<usize>,
    ) -> Vec<usize> {
        let w = self.ws_warship.unwrap();
        let docking_radius = self.mg_warship_docking_range(tr); // 935
        let owner = self.un_owner(tr, w); // 286
        let port_tile = self.un_tile(tr, port); // 344
        let ships =
            self.mg_nearby_units_d2(tr, port_tile, docking_radius, &[UNIT_WARSHIP.to_string()]); // 306
        let mut out = Vec::new();
        for (ship, _d2) in ships {
            if exclude == Some(ship) {
                continue;
            }
            if self.un_owner(tr, ship) != owner {
                // 286
                continue;
            }
            let state = self.un_warship_state(tr, ship); // 928
            if g5j_ws_state(&state) == "patrolling" {
                continue;
            }
            if self.un_target_tile(tr, ship).is_some() {
                // 282
                continue;
            }
            out.push(ship);
        }
        out
    }

    /// `applyActiveDockedHealing()` (only reachable while docked).
    fn ws_apply_active_docked_healing(&mut self, tr: &mut Vec<f64>) {
        let w = self.ws_warship.unwrap();
        let docked_port = match self.ws_current_retreat_port(tr) {
            Some(p) => p,
            None => return,
        };
        let docked_ships = self.ws_docked_ships_at_port(tr, docked_port, None);
        if !docked_ships.contains(&w) {
            return;
        }
        let level = self.un_level(tr, docked_port); // 540
        let bonus = self.mg_warship_port_healing_bonus(tr); // 936
        let healing_pool = level * bonus;
        if healing_pool <= 0.0 || docked_ships.is_empty() {
            return;
        }
        // Preserve the fractional split healing with a per-ship remainder.
        let active_healing = healing_pool / docked_ships.len() as f64;
        self.active_healing_remainder += active_healing;
        let integer_healing = self.active_healing_remainder.floor();
        if integer_healing <= 0.0 {
            return;
        }
        self.active_healing_remainder -= integer_healing;
        self.un_modify_health(tr, w, integer_healing, None); // 543
    }

    /// `currentRetreatPort()`.
    fn ws_current_retreat_port(&mut self, tr: &mut Vec<f64>) -> Option<usize> {
        let w = self.ws_warship.unwrap();
        let state = self.un_warship_state(tr, w); // 928
        let retreat_port = as_opt_num(val_field(&state, "retreatPort"))?;
        let owner = self.un_owner(tr, w)?; // 286
        let ports = self.pl_units(tr, owner, &[UNIT_PORT.to_string()]); // 241
        for p in ports {
            let pt = self.un_tile(tr, p); // 344
            if pt == retreat_port {
                return Some(p);
            }
        }
        None
    }

    /// `nearestAvailablePortTile(excludeShip?)` → `{tile, distSquared}`.
    fn ws_nearest_available_port_tile(
        &mut self,
        tr: &mut Vec<f64>,
        exclude: Option<usize>,
    ) -> Option<(f64, f64)> {
        let w = self.ws_warship.unwrap();
        let owner = self.un_owner(tr, w)?; // 286
        let ports = self.pl_units(tr, owner, &[UNIT_PORT.to_string()]); // 241
        let warship_tile = self.un_tile(tr, w); // 344
        let warship_comp = self.mg_get_water_component(tr, warship_tile); // 301
        let mut best_tile: Option<f64> = None;
        let mut best_dist = f64::INFINITY;
        for port in ports {
            if self.ws_is_port_full_of_healing(tr, port, exclude) {
                continue;
            }
            let port_tile = self.un_tile(tr, port); // 344
            if !self.mg_has_water_component(tr, port_tile, &warship_comp) {
                // 302
                continue;
            }
            let dist = self.mg_euclidean_dist_squared(tr, warship_tile, port_tile); // 710
            if dist < best_dist {
                best_dist = dist;
                best_tile = Some(port_tile);
            }
        }
        best_tile.map(|t| (t, best_dist))
    }

    /// `findNearestAvailablePort()`.
    fn ws_find_nearest_available_port(&mut self, tr: &mut Vec<f64>) -> Option<f64> {
        self.ws_nearest_available_port_tile(tr, None).map(|(t, _)| t)
    }

    /// `findNearestAvailablePortTile()` (excludes this warship from the
    /// port-capacity check).
    fn ws_find_nearest_available_port_tile(&mut self, tr: &mut Vec<f64>) -> Option<f64> {
        let w = self.ws_warship.unwrap();
        self.ws_nearest_available_port_tile(tr, Some(w)).map(|(t, _)| t)
    }

    /// `findBetterPortTile()`.
    fn ws_find_better_port_tile(&mut self, tr: &mut Vec<f64>) -> Option<f64> {
        let w = self.ws_warship.unwrap();
        let (tile, dist_sq) = self.ws_nearest_available_port_tile(tr, None)?;
        let mut current_distance = f64::INFINITY;
        let state = self.un_warship_state(tr, w); // 928
        if let Some(cur) = as_opt_num(val_field(&state, "retreatPort")) {
            let warship_tile = self.un_tile(tr, w); // 344
            current_distance = self.mg_euclidean_dist_squared(tr, warship_tile, cur); // 710
        }
        let threshold = self.mg_warship_port_switch_threshold(tr); // 937
        if dist_sq < current_distance * threshold {
            return Some(tile);
        }
        None
    }

    /// `findBestTarget(types, includeTradeShips)`.
    fn ws_find_best_target(
        &mut self,
        tr: &mut Vec<f64>,
        types: &[&str],
        include_trade_ships: bool,
    ) -> Option<usize> {
        let w = self.ws_warship.unwrap();
        let owner = self.un_owner(tr, w); // 286
        let warship_tile = self.un_tile(tr, w); // 344
        let range = self.mg_warship_targetting_range(tr); // 933
        let type_strings: Vec<String> = types.iter().map(|s| s.to_string()).collect();
        let ships = self.mg_nearby_units_d2(tr, warship_tile, range, &type_strings); // 306

        // Trade-ship-specific state, lazily computed.
        let mut lazy: Option<(bool, Option<f64>, f64)> = None; // (hasReachablePort, patrolTile, patrolRangeSquared)

        let mut best_unit: Option<usize> = None;
        let mut best_priority = 0.0;
        let mut best_dist_sq = 0.0;

        for (unit, d2) in ships {
            if unit == w {
                continue;
            }
            if self.un_owner(tr, unit) == owner {
                // 286 — `unit.owner() === owner`
                continue;
            }
            let u_owner = self.un_owner(tr, unit); // 286 — `unit.owner()` again
            let can_attack = match (owner, u_owner) {
                (Some(o), Some(uo)) => self.pl_can_attack_player(tr, o, uo), // 820
                _ => false,
            };
            if !can_attack {
                continue;
            }
            if self.already_sent_shell.contains(&unit) {
                continue;
            }
            let ty0 = self.un_type(tr, unit); // 283 — the docked-warship guard
            if ty0 == UNIT_WARSHIP {
                let st = self.un_warship_state(tr, unit); // 928
                if g5j_ws_state(&st) == "docked" {
                    continue;
                }
            }

            let ty = self.un_type(tr, unit); // 283 — `const type = unit.type()`

            if include_trade_ships && ty == UNIT_TRADE_SHIP {
                if lazy.is_none() {
                    let wt = self.un_tile(tr, w); // 344
                    let comp = self.mg_get_water_component(tr, wt); // 301
                    let reachable = !matches!(comp, JsVal::Null) && {
                        let ports = match owner {
                            Some(o) => self.pl_units(tr, o, &[UNIT_PORT.to_string()]), // 241
                            None => Vec::new(),
                        };
                        let mut found = false;
                        for p in ports {
                            if !self.un_is_active(tr, p) {
                                // 280
                                continue;
                            }
                            if self.un_is_marked_for_deletion(tr, p) {
                                // 913
                                continue;
                            }
                            if self.un_is_under_construction(tr, p) {
                                // 345
                                continue;
                            }
                            let pt = self.un_tile(tr, p); // 344
                            if self.mg_has_water_component(tr, pt, &comp) {
                                // 302
                                found = true;
                                break;
                            }
                        }
                        found
                    };
                    let state = self.un_warship_state(tr, w); // 928
                    let patrol_tile = as_opt_num(val_field(&state, "patrolTile"));
                    let pr = self.mg_warship_patrol_range(tr); // 934
                    lazy = Some((reachable, patrol_tile, pr * pr));
                }
                let (reachable, patrol_tile, patrol_range_sq) = lazy.unwrap();
                let skip = if !reachable || patrol_tile.is_none() {
                    true
                } else if self.un_is_safe_from_pirates(tr, unit) {
                    // 939
                    true
                } else {
                    // `unit.targetUnit()?.owner() === owner` (None === None too).
                    let lhs = match self.un_target_unit(tr, unit) {
                        // 735
                        Some(t) => self.un_owner(tr, t), // 286
                        None => None,
                    };
                    if lhs == owner {
                        true
                    } else {
                        let rhs = match self.un_target_unit(tr, unit) {
                            // 735
                            Some(t) => self.un_owner(tr, t), // 286
                            None => None,
                        };
                        match (rhs, owner) {
                            (Some(ro), Some(o)) => self.pl_is_friendly(tr, ro, Some(o)), // 242
                            _ => false,
                        }
                    }
                };
                if skip {
                    continue;
                }
                let ut = self.un_tile(tr, unit); // 344
                let d = self.mg_euclidean_dist_squared(tr, patrol_tile.unwrap(), ut); // 710
                if d > patrol_range_sq {
                    continue;
                }
            }

            let priority = if ty == UNIT_TRANSPORT_SHIP {
                0.0
            } else if ty == UNIT_WARSHIP {
                1.0
            } else {
                2.0
            };
            let better = match best_unit {
                None => true,
                Some(_) => {
                    priority < best_priority || (priority == best_priority && d2 < best_dist_sq)
                }
            };
            if better {
                best_unit = Some(unit);
                best_priority = priority;
                best_dist_sq = d2;
            }
        }
        best_unit
    }

    /// `shootTarget()`.
    fn ws_shoot_target(&mut self, tr: &mut Vec<f64>) {
        let w = self.ws_warship.unwrap();
        self.un_update_warship_state_obj(
            tr,
            w,
            &JsVal::Obj(vec![("isInCombat".to_string(), JsVal::Bool(true))]),
        ); // 342
        let rate = self.mg_warship_shell_attack_rate(tr); // 938
        let ticks = self.mg_ticks(tr); // 212
        if ticks - self.last_shell_attack > rate {
            let ty = match self.un_target_unit(tr, w) {
                // 735 — `targetUnit()?.type()`
                Some(t) => self.un_type(tr, t), // 283
                None => String::new(),
            };
            if ty != UNIT_TRANSPORT_SHIP {
                // Warships don't need to reload when attacking transport ships.
                self.last_shell_attack = self.mg_ticks(tr); // 212
            }
            let spawn = self.un_tile(tr, w); // 344
            let owner = self.un_owner(tr, w); // 286
            let target = self.un_target_unit(tr, w); // 735 — `targetUnit()!`
            match target {
                Some(t) => self.mg_add_execution_shell(tr, spawn, owner, w, t), // 307
                None => {
                    // `targetUnit()!` resolved to undefined (unreached in the
                    // capture) — encode the shell record with a Undef target.
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
                    push_val(tr, &JsVal::Num(w as f64));
                    push_val(tr, &JsVal::Undef);
                    push_val(tr, &JsVal::Undef);
                }
            }
            let target = self.un_target_unit(tr, w); // 735 — hasHealth check
            let has_health = match target {
                Some(t) => self.un_has_health(tr, t), // 352
                None => false,
            };
            if !has_health {
                // Don't send multiple shells to a target that can be oneshotted.
                if let Some(t) = self.un_target_unit(tr, w) {
                    // 735
                    if !self.already_sent_shell.contains(&t) {
                        self.already_sent_shell.push(t);
                    }
                }
                self.un_set_target_unit(tr, w, None); // 914
            }
        }
    }

    /// `huntDownTradeShip()`.
    fn ws_hunt_down_trade_ship(&mut self, tr: &mut Vec<f64>) {
        let w = self.ws_warship.unwrap();
        self.un_update_warship_state_obj(
            tr,
            w,
            &JsVal::Obj(vec![("isInCombat".to_string(), JsVal::Bool(true))]),
        ); // 342
        for _ in 0..2 {
            let target = match self.un_target_unit(tr, w) {
                // 735 — `targetUnit()!`
                Some(t) => t,
                None => return, // would throw in JS; unreached in the capture
            };
            let target_tile = self.un_tile(tr, target); // 344
            let warship_tile = self.un_tile(tr, w); // 344
            let dist = self.mg_manhattan_dist(tr, warship_tile, target_tile); // 509
            if dist <= 5.0 {
                let owner = self.un_owner(tr, w); // 286
                if let Some(o) = owner {
                    self.pl_capture_unit(tr, o, target); // 941
                }
                self.un_record_trade_capture(tr, w); // 929
                self.un_set_target_unit(tr, w, None); // 914
                self.un_touch(tr, w); // 736
                return;
            }
            // Close in with direct greedy movement when nearly there.
            if dist <= 20.0 {
                if let Some(next_tile) = self.ws_best_neighbor_toward(tr, target_tile) {
                    self.un_move(tr, w, &JsVal::Num(next_tile)); // 545
                    continue;
                }
            }
            let warship_tile = self.un_tile(tr, w); // 344
            // The capture's pathfinder mock drops the third arg, so the 2-arg
            // facade helper reproduces the recorded trace exactly.
            let result = self.mg_water_next(tr, warship_tile, target_tile); // 862
            let status = val_field(&result, "status").map(as_num).unwrap_or(f64::NAN);
            let node = val_field(&result, "node").cloned().unwrap_or(JsVal::Null);
            match status as i32 {
                2 => {
                    // PathStatus.COMPLETE — capture on arrival.
                    let owner = self.un_owner(tr, w); // 286
                    if let Some(o) = owner {
                        self.pl_capture_unit(tr, o, target); // 941
                    }
                    self.un_record_trade_capture(tr, w); // 929
                    self.un_set_target_unit(tr, w, None); // 914
                    self.un_touch(tr, w); // 736
                    return;
                }
                0 => {
                    // PathStatus.NEXT
                    self.un_move(tr, w, &node); // 545
                }
                _ => {} // NOT_FOUND — console.log only.
            }
        }
    }

    /// `bestNeighborToward(targetTile)`.
    fn ws_best_neighbor_toward(&mut self, tr: &mut Vec<f64>, target_tile: f64) -> Option<f64> {
        let w = self.ws_warship.unwrap();
        let warship_tile = self.un_tile(tr, w); // 344
        let mut best: Option<f64> = None;
        let mut best_dist = self.mg_manhattan_dist(tr, warship_tile, target_tile); // 509
        self.mg_for_each_neighbor(tr, warship_tile, |s, tr, n| {
            // 807
            if !s.mg_is_water(tr, n) {
                // 508
                return;
            }
            let d = s.mg_manhattan_dist(tr, n, target_tile); // 509
            if d < best_dist {
                best_dist = d;
                best = Some(n);
            }
        });
        best
    }

    /// `patrol()`.
    fn ws_patrol(&mut self, tr: &mut Vec<f64>) {
        let w = self.ws_warship.unwrap();
        if self.un_target_tile(tr, w).is_none() {
            // 282
            match self.ws_random_tile(tr, false) {
                Some(t) => self.un_set_target_tile_num(tr, w, t), // 343 — number
                None => self.un_set_target_tile(tr, w),           // 343 — undefined
            }
            if self.un_target_tile(tr, w).is_none() {
                // 282
                return;
            }
        }
        let warship_tile = self.un_tile(tr, w); // 344
        let target = self.un_target_tile(tr, w).unwrap_or(f64::NAN); // 282
        let result = self.mg_water_next(tr, warship_tile, target); // 862
        let status = val_field(&result, "status").map(as_num).unwrap_or(f64::NAN);
        let node = val_field(&result, "node").cloned().unwrap_or(JsVal::Null);
        match status as i32 {
            2 => {
                // PathStatus.COMPLETE — clear the target BEFORE moving.
                self.un_set_target_tile(tr, w); // 343 — undefined
                self.un_move(tr, w, &node); // 545
            }
            0 => {
                // PathStatus.NEXT
                self.un_move(tr, w, &node); // 545
            }
            3 => {
                // PathStatus.NOT_FOUND — console.log only.
                self.un_set_target_tile(tr, w); // 343 — undefined
            }
            _ => {}
        }
    }

    /// `randomTile(allowShoreline)`.
    fn ws_random_tile(&mut self, tr: &mut Vec<f64>, allow_shoreline: bool) -> Option<f64> {
        let w = self.ws_warship.unwrap();
        let mut patrol_range = self.mg_warship_patrol_range(tr); // 934
        let max_attempts = 500;
        let mut attempts = 0;
        let mut expand_count = 0;
        let warship_tile = self.un_tile(tr, w); // 344
        let warship_comp = self.mg_get_water_component(tr, warship_tile); // 301
        let state = self.un_warship_state(tr, w); // 928
        // A missing / invalid patrolTile would spin the TS loop forever; bail.
        let patrol_tile = as_opt_num(val_field(&state, "patrolTile"))?;
        if !self.mg_is_valid_ref(tr, patrol_tile) {
            // 300
            return None;
        }
        while expand_count < 3 {
            // `mg.x(patrolTile) + nextInt(...)` — x() is evaluated first.
            let px = self.mg_x(tr, patrol_tile); // 603
            let dx = self
                .ws_random
                .as_mut()
                .unwrap()
                .next_int(-patrol_range / 2.0, patrol_range / 2.0);
            let x = px + dx as f64;
            let py = self.mg_y(tr, patrol_tile); // 604
            let dy = self
                .ws_random
                .as_mut()
                .unwrap()
                .next_int(-patrol_range / 2.0, patrol_range / 2.0);
            let y = py + dy as f64;
            if !self.mg_is_valid_coord(tr, x, y) {
                // 616
                continue;
            }
            let tile = self.mg_ref(tr, x, y); // 609
            let bad = !self.mg_is_water(tr, tile) // 508
                || (!allow_shoreline && self.mg_is_shoreline(tr, tile)); // 901
            if bad {
                attempts += 1;
                if attempts == max_attempts {
                    expand_count += 1;
                    attempts = 0;
                    patrol_range += (patrol_range / 2.0).floor();
                }
                continue;
            }
            if !matches!(warship_comp, JsVal::Null)
                && !self.mg_has_water_component(tr, tile, &warship_comp)
            {
                // 302
                attempts += 1;
                if attempts == max_attempts {
                    expand_count += 1;
                    attempts = 0;
                    patrol_range += (patrol_range / 2.0).floor();
                }
                continue;
            }
            return Some(tile);
        }
        if !allow_shoreline {
            // Failed on the open ocean — retry allowing the shoreline.
            return self.ws_random_tile(tr, true);
        }
        None
    }

    // ---- G5j: `PlayerExecution` (class 42) ----------------------------------

    fn player_execution(
        &mut self,
        kind: u8,
        ticks: f64,
        tr: &mut Vec<f64>,
    ) -> Result<JsVal, String> {
        match kind {
            3 => return Ok(JsVal::Bool(self.pe_active)),
            // `activeDuringSpawnPhase()` -> false.
            4 => return Ok(JsVal::Bool(false)),
            1 => {
                // `lastCalc = ticks + (simpleHash(player.id()) % 20)`.
                let id = self.pl_id(tr, self.pe_player); // 231
                self.last_calc = ticks + (crate::util::simple_hash(&id) % 20.0);
                return Ok(JsVal::Undef);
            }
            2 => {}
            _ => return Ok(JsVal::Undef),
        }
        self.pe_tick(ticks, tr);
        Ok(JsVal::Undef)
    }

    /// `tick(ticks)`.
    fn pe_tick(&mut self, ticks: f64, tr: &mut Vec<f64>) {
        let p = self.pe_player;
        self.pl_decay_relations(tr, p); // 949
        for u in self.pl_units(tr, p, &[]) {
            // 241
            let ty = self.un_type(tr, u); // 283
            if !STRUCTURES_TYPES.contains(&ty.as_str()) {
                continue;
            }
            let tile = self.un_tile(tr, u); // 344
            let owner = self.mg_owner(tr, tile); // 222
            let not_player = match owner {
                Some(o) => !self.pl_is_player(tr, o), // 259
                None => true, // `!owner?.isPlayer()`
            };
            if not_player {
                self.un_delete(tr, u, false); // 285 — `u.delete()`
                continue;
            }
            if owner == Some(p) {
                continue;
            }
            let owner_id = self.pl_id(tr, owner.unwrap()); // 231
            let captor = self.mg_player(tr, &owner_id); // 211
            let ty = self.un_type(tr, u); // 283 — `u.type()` re-read
            if ty == UNIT_DEFENSE_POST {
                self.un_delete_captor(tr, u, true, captor); // 285 — delete(true, captor)
            } else if let Some(c) = captor {
                self.pl_capture_unit(tr, c, u); // 941
            }
        }

        if !self.pl_is_alive(tr, p) {
            // 233
            self.pe_remove_on_death(tr);
            self.pe_active = false;
            // Finishing place = non-bot players still standing, + 1.
            let mut still_standing = 0.0;
            for pl in self.mg_players(tr) {
                // 221
                if self.pl_type(tr, pl) != PLAYER_TYPE_BOT {
                    // 251
                    still_standing += 1.0;
                }
            }
            self.mg_stats_record_death_position(tr, p, still_standing + 1.0); // 943
            self.mg_stats_player_killed(tr, p, ticks); // 944
            return;
        }

        let troop_inc = self.mg_troop_increase_rate(tr, p); // 946
        self.pl_add_troops(tr, p, troop_inc); // 824
        let gold = self.mg_gold_addition_rate(tr, p); // 947
        self.pl_add_gold(tr, p, gold, f64::NAN); // 910 — the 1-arg call (tile NaN)
        self.mg_stats_gold_work(tr, p, gold); // 945

        for aid in self.pl_alliances(tr, p) {
            // 954
            let expires = self.al_expires_at(tr, aid); // 956
            if expires <= self.mg_ticks(tr) {
                // 212
                self.al_expire(tr, aid); // 957
            }
        }

        for (is_temporary, created_at, target) in self.pl_get_embargoes(tr, p) {
            // 955
            if is_temporary {
                let now = self.mg_ticks(tr); // 212
                let dur = self.mg_temporary_embargo_duration(tr); // 948
                if now - created_at > dur {
                    self.pl_stop_embargo(tr, p, Some(target)); // 254
                }
            }
        }

        if ticks - self.last_calc > 20.0 || self.pl_num_tiles_owned(tr, p) < 100.0 {
            // 532
            if self.pl_last_tile_change(tr, p) >= self.last_calc {
                // 952
                self.last_calc = ticks;
                let start = self.mg_perf_now(tr); // 942
                self.pe_remove_clusters(tr);
                let end = self.mg_perf_now(tr); // 942
                let _ = end - start; // `> 1000` only logs in TS.
            }
        }
    }

    /// `removeOnDeath()`.
    fn pe_remove_on_death(&mut self, tr: &mut Vec<f64>) {
        let p = self.pe_player;
        let gold = self.pl_gold(tr, p); // 417
        self.pl_remove_gold(tr, p, gold as f64); // 950
        for u in self.pl_units(tr, p, &[]) {
            // 241 — four separate `u.type()` reads, short-circuited by `!==`.
            if self.un_type(tr, u) != UNIT_ATOM_BOMB // 283
                && self.un_type(tr, u) != UNIT_HYDROGEN_BOMB
                && self.un_type(tr, u) != UNIT_MIRV_WARHEAD
                && self.un_type(tr, u) != UNIT_MIRV
            {
                self.un_delete(tr, u, false); // 285 — `u.delete()`
            }
        }
        self.pl_remove_all_alliances(tr, p); // 951
    }

    /// `removeClusters()`.
    fn pe_remove_clusters(&mut self, tr: &mut Vec<f64>) {
        let p = self.pe_player;
        let (clusters, boxes) = self.pe_calculate_clusters(tr);
        if clusters.is_empty() {
            self.pl_set_largest_cluster_bbox(tr, p, None); // 953 — null
            return;
        }
        if clusters.len() == 1 {
            self.pl_set_largest_cluster_bbox(
                tr,
                p,
                Some((
                    boxes[0] as f64,
                    boxes[1] as f64,
                    boxes[2] as f64,
                    boxes[3] as f64,
                )),
            ); // 953
            let surrounded = self.pe_surrounded_by_same_player(
                tr,
                &clusters[0],
                boxes[0],
                boxes[1],
                boxes[2],
                boxes[3],
            );
            if let Some(enemy) = surrounded {
                if !self.pl_is_friendly(tr, enemy, Some(p)) {
                    // 242
                    self.pe_remove_cluster(tr, &clusters[0]);
                }
            }
            return;
        }

        let mut largest = 0usize;
        for i in 1..clusters.len() {
            if clusters[i].len() > clusters[largest].len() {
                largest = i;
            }
        }
        let t_min_x = boxes[largest * 4];
        let t_min_y = boxes[largest * 4 + 1];
        let t_max_x = boxes[largest * 4 + 2];
        let t_max_y = boxes[largest * 4 + 3];

        // Doughnut-border fix: a cluster fully enclosed by another cluster of
        // the same contiguous territory is a hole, not the primary cluster.
        let mut territory_ids: Option<Vec<i32>> = None;
        let mut next_territory_id: i32 = 1;
        let mut is_largest_hole = false;
        for j in 0..clusters.len() {
            if j != largest {
                let jj = j * 4;
                if boxes[jj] <= t_min_x
                    && boxes[jj + 1] <= t_min_y
                    && boxes[jj + 2] >= t_max_x
                    && boxes[jj + 3] >= t_max_y
                {
                    if territory_ids.is_none() {
                        let ids = vec![0i32; clusters.len()];
                        self.pe_traversal_state(tr); // 610 / 611
                        for (c, cl) in clusters.iter().enumerate() {
                            for &t in cl {
                                self.el_scratch.cluster_index_map[t as usize] = c as i32 + 1;
                            }
                        }
                        territory_ids = Some(ids);
                    }
                    next_territory_id = self.pe_check_and_assign_territory(
                        tr,
                        &clusters,
                        largest,
                        j,
                        territory_ids.as_mut().unwrap(),
                        next_territory_id,
                    );
                    let ids = territory_ids.as_ref().unwrap();
                    if ids[largest] == ids[j] {
                        is_largest_hole = true;
                        break;
                    }
                }
            }
        }

        if is_largest_hole {
            let mut best_index: i64 = -1;
            let mut best_length: i64 = -1;
            for i in 0..clusters.len() {
                if i == largest || clusters[i].len() as i64 <= best_length {
                    continue;
                }
                let mut is_hole = false;
                let ii = i * 4;
                for j in 0..clusters.len() {
                    if j != i {
                        let jj = j * 4;
                        if boxes[jj] <= boxes[ii]
                            && boxes[jj + 1] <= boxes[ii + 1]
                            && boxes[jj + 2] >= boxes[ii + 2]
                            && boxes[jj + 3] >= boxes[ii + 3]
                        {
                            next_territory_id = self.pe_check_and_assign_territory(
                                tr,
                                &clusters,
                                i,
                                j,
                                territory_ids.as_mut().unwrap(),
                                next_territory_id,
                            );
                            let ids = territory_ids.as_ref().unwrap();
                            if ids[i] == ids[j] {
                                is_hole = true;
                                break;
                            }
                        }
                    }
                }
                if !is_hole {
                    best_length = clusters[i].len() as i64;
                    best_index = i as i64;
                }
            }
            if best_index != -1 {
                largest = best_index as usize;
            }
        }

        if territory_ids.is_some() {
            self.pe_traversal_state(tr); // 610 / 611
            for cl in &clusters {
                for &t in cl {
                    self.el_scratch.cluster_index_map[t as usize] = 0;
                }
            }
        }

        let l = largest * 4;
        self.pl_set_largest_cluster_bbox(
            tr,
            p,
            Some((
                boxes[l] as f64,
                boxes[l + 1] as f64,
                boxes[l + 2] as f64,
                boxes[l + 3] as f64,
            )),
        ); // 953
        let surrounded = self.pe_surrounded_by_same_player(
            tr,
            &clusters[largest],
            boxes[l],
            boxes[l + 1],
            boxes[l + 2],
            boxes[l + 3],
        );
        if let Some(enemy) = surrounded {
            if !self.pl_is_friendly(tr, enemy, Some(p)) {
                // 242
                let cluster = clusters[largest].clone();
                self.pe_remove_cluster(tr, &cluster);
            }
        }

        // Process the remaining clusters.
        for (i, cluster_i) in clusters.iter().enumerate() {
            if i == largest {
                continue;
            }
            let ii = i * 4;
            if self.pe_is_surrounded(
                tr,
                cluster_i,
                boxes[ii],
                boxes[ii + 1],
                boxes[ii + 2],
                boxes[ii + 3],
            ) {
                let cluster = cluster_i.clone();
                self.pe_remove_cluster(tr, &cluster);
            }
        }
    }

    /// `checkAndAssignTerritory(...)` — two `traversalState()` reads (the
    /// explicit one plus the one inside `this.bumpGeneration()`).
    fn pe_check_and_assign_territory(
        &mut self,
        tr: &mut Vec<f64>,
        clusters: &[Vec<f64>],
        a: usize,
        b: usize,
        territory_ids: &mut [i32],
        next_id: i32,
    ) -> i32 {
        if territory_ids[a] != 0 && territory_ids[a] == territory_ids[b] {
            return next_id;
        }
        if territory_ids[a] != 0 && territory_ids[b] != 0 {
            return next_id;
        }
        let start_idx = if territory_ids[b] == 0 { b } else { a };
        let territory_id = next_id;

        self.pe_traversal_state(tr); // 610 / 611 — `const state = ...`
        let gen = self.pe_bump_generation(tr); // 610 / 611 + bump
        let mut stack: Vec<f64> = Vec::new();
        let start = clusters[start_idx][0];
        self.el_scratch.visited[start as usize] = gen;
        stack.push(start);
        let map = self.el_map.clone().unwrap_or_default();
        let my_sid = self.pl_small_id(tr, self.pe_player); // 230
        territory_ids[start_idx] = territory_id;
        while let Some(tile) = stack.pop() {
            let n = map.neighbors8(tile, &mut self.nbuf8);
            let buf = self.nbuf8;
            for &nb in buf.iter().take(n) {
                if self.el_scratch.visited[nb as usize] == gen {
                    continue;
                }
                if map.owner_id(nb) == my_sid {
                    self.el_scratch.visited[nb as usize] = gen;
                    stack.push(nb);
                    let c_idx_plus_one = self.el_scratch.cluster_index_map[nb as usize];
                    if c_idx_plus_one > 0 {
                        territory_ids[(c_idx_plus_one - 1) as usize] = territory_id;
                    }
                }
            }
        }
        next_id + 1
    }

    /// `surroundedBySamePlayer(...)` — the surrounding enemy player token, or
    /// None (`false` in TS).
    fn pe_surrounded_by_same_player(
        &mut self,
        tr: &mut Vec<f64>,
        cluster: &[f64],
        box_min_x: i32,
        box_min_y: i32,
        box_max_x: i32,
        box_max_y: i32,
    ) -> Option<usize> {
        let map = self.el_map.clone().unwrap_or_default();
        let my_sid = self.pl_small_id(tr, self.pe_player); // 230
        let mut enemies: Vec<f64> = Vec::new(); // insertion-ordered set
        let mut min_x = 1e9_f64;
        let mut min_y = 1e9_f64;
        let mut max_x = -1e9_f64;
        let mut max_y = -1e9_f64;
        for &tile in cluster {
            if map.is_ocean_shore(tile) || map.is_on_edge_of_map(tile) {
                return None;
            }
            let n = map.neighbors4(tile, &mut self.nbuf);
            let buf = self.nbuf;
            for &nb in buf.iter().take(n) {
                let owner_id = map.owner_id(nb);
                if owner_id == 0.0 {
                    // Unowned neighbor: the cluster is not fully surrounded.
                    return None;
                }
                if owner_id != my_sid {
                    if !enemies.contains(&owner_id) {
                        enemies.push(owner_id);
                    }
                    let px = map.x(nb);
                    let py = map.y(nb);
                    if px < min_x {
                        min_x = px;
                    }
                    if py < min_y {
                        min_y = py;
                    }
                    if px > max_x {
                        max_x = px;
                    }
                    if py > max_y {
                        max_y = py;
                    }
                }
            }
            if enemies.len() != 1 {
                return None;
            }
        }
        if enemies.len() != 1 {
            return None;
        }
        let enemy = as_opt_tok(&self.mg_player_by_small_id(tr, enemies[0])); // 703
        if min_x <= box_min_x as f64
            && min_y <= box_min_y as f64
            && max_x >= box_max_x as f64
            && max_y >= box_max_y as f64
        {
            return enemy;
        }
        None
    }

    /// `isSurrounded(...)`.
    fn pe_is_surrounded(
        &mut self,
        tr: &mut Vec<f64>,
        cluster: &[f64],
        box_min_x: i32,
        box_min_y: i32,
        box_max_x: i32,
        box_max_y: i32,
    ) -> bool {
        let map = self.el_map.clone().unwrap_or_default();
        let my_sid = self.pl_small_id(tr, self.pe_player); // 230
        let mut has_enemy = false;
        let mut min_x = 1e9_f64;
        let mut min_y = 1e9_f64;
        let mut max_x = -1e9_f64;
        let mut max_y = -1e9_f64;
        for &tile in cluster {
            if map.is_shore(tile) || map.is_on_edge_of_map(tile) {
                return false;
            }
            let n = map.neighbors4(tile, &mut self.nbuf);
            let buf = self.nbuf;
            for &nb in buf.iter().take(n) {
                let owner_id = map.owner_id(nb);
                if owner_id != 0.0 && owner_id != my_sid {
                    has_enemy = true;
                    let x = map.x(nb);
                    let y = map.y(nb);
                    if x < min_x {
                        min_x = x;
                    }
                    if y < min_y {
                        min_y = y;
                    }
                    if x > max_x {
                        max_x = x;
                    }
                    if y > max_y {
                        max_y = y;
                    }
                }
            }
        }
        if !has_enemy {
            return false;
        }
        min_x <= box_min_x as f64
            && min_y <= box_min_y as f64
            && max_x >= box_max_x as f64
            && max_y >= box_max_y as f64
    }

    /// `removeCluster(cluster)`.
    fn pe_remove_cluster(&mut self, tr: &mut Vec<f64>, cluster: &[f64]) {
        let p = self.pe_player;
        for &t in cluster {
            // Other removeCluster operations could change tile owners — the
            // double check reads both facades per tile.
            let owner = self.mg_owner_id(tr, t); // 223
            let sid = self.pl_small_id(tr, p); // 230
            if owner != sid {
                return;
            }
        }
        let capturing = match self.pe_get_capturing_player(tr, cluster) {
            Some(c) => c,
            None => return,
        };
        let first_tile = match cluster.first() {
            Some(&t) => t,
            None => return,
        };
        if !self.pe_is_enclosed(tr, first_tile) {
            return;
        }
        let gen = self.pe_bump_generation(tr); // 610 / 611 + bump
        let tiles = self.pe_flood_fill(tr, gen, &[first_tile], false, FloodInclude::OwnedBy, None);
        if self.pl_num_tiles_owned(tr, p) == tiles.len() as f64 {
            // 532
            self.mg_conquer_player(tr, capturing, p); // 808
        }
        for tile in tiles {
            self.pl_conquer(tr, capturing, tile); // 637
        }
    }

    /// `isEnclosed(start)` — ONE `traversalState()` pair plus a direct
    /// `bumpTraversalGeneration` (NOT `this.bumpGeneration()`).
    fn pe_is_enclosed(&mut self, tr: &mut Vec<f64>, start: f64) -> bool {
        let map = self.el_map.clone().unwrap_or_default();
        let my_sid = self.pl_small_id(tr, self.pe_player); // 230
        self.pe_traversal_state(tr); // 610 / 611
        let gen = self.el_scratch.bump_generation();
        self.el_scratch.visited[start as usize] = gen;
        let mut stack: Vec<f64> = vec![start];
        while let Some(tile) = stack.pop() {
            if map.is_on_edge_of_map(tile) {
                return false;
            }
            let n = map.neighbors4(tile, &mut self.nbuf);
            let buf = self.nbuf;
            for &nb in buf.iter().take(n) {
                if self.el_scratch.visited[nb as usize] == gen {
                    continue;
                }
                let owner_id = map.owner_id(nb);
                if owner_id != 0.0 && owner_id != my_sid {
                    // Someone else's tile — part of the wall.
                    continue;
                }
                if owner_id == 0.0 && !map.is_land(nb) {
                    // Open water is a way out.
                    return false;
                }
                self.el_scratch.visited[nb as usize] = gen;
                stack.push(nb);
            }
        }
        true
    }

    /// `getCapturingPlayer(cluster)`.
    fn pe_get_capturing_player(&mut self, tr: &mut Vec<f64>, cluster: &[f64]) -> Option<usize> {
        let map = self.el_map.clone().unwrap_or_default();
        let p = self.pe_player;
        let my_sid = self.pl_small_id(tr, p); // 230
        let mut neighbors: Vec<(usize, f64)> = Vec::new(); // insertion-ordered Map
        for &t in cluster {
            let n = map.neighbors4(t, &mut self.nbuf);
            let buf = self.nbuf;
            for &nb in buf.iter().take(n) {
                let owner_id = map.owner_id(nb);
                if owner_id == 0.0 || owner_id == my_sid {
                    continue;
                }
                let owner = as_opt_tok(&self.mg_player_by_small_id(tr, owner_id)); // 703
                if let Some(o) = owner {
                    if !self.pl_is_friendly(tr, o, Some(p)) {
                        // 242
                        match neighbors.iter_mut().find(|(tok, _)| *tok == o) {
                            Some(e) => e.1 += 1.0,
                            None => neighbors.push((o, 1.0)),
                        }
                    }
                }
            }
        }
        if neighbors.is_empty() {
            return None;
        }

        // The largest attack on us from any neighbor wins.
        let mut largest_attack: Option<usize> = None;
        let mut largest_troops = 0.0;
        for (tok, _) in &neighbors {
            for a in self.pl_outgoing_attacks(tr, *tok) {
                // 721
                if self.at_target(tr, a) == Some(p) {
                    // 840
                    let troops = self.at_troops(tr, a); // 830
                    if troops > largest_troops {
                        largest_troops = troops;
                        largest_attack = Some(*tok);
                    }
                }
            }
        }
        if largest_attack.is_some() {
            return largest_attack;
        }

        // No ongoing attacks — the enemy with the largest border (getMode).
        let pairs: Vec<(f64, f64)> = neighbors.iter().map(|(t, c)| (*t as f64, *c)).collect();
        crate::util::get_mode(&pairs).map(|m| m as usize)
    }

    /// `calculateClusters()` — border-gen stamp, then one DFS flood per
    /// unvisited border tile (8-neighbor), packing each cluster's bbox into
    /// the doubling `boxes` array.
    fn pe_calculate_clusters(&mut self, tr: &mut Vec<f64>) -> (Vec<Vec<f64>>, Vec<i32>) {
        let p = self.pe_player;
        let border_tiles = self.pl_border_tiles(tr, p); // 635
        if border_tiles.is_empty() {
            return (Vec::new(), Vec::new());
        }
        if self.el_map.is_none() {
            // The grid-less scenarios never reach here (empty border tiles);
            // without a map the TS tile accessors are unavailable.
            return (Vec::new(), Vec::new());
        }
        self.pe_traversal_state(tr); // 610 / 611 — `const state = ...`
        let border_gen = self.pe_bump_generation(tr); // 610 / 611 + bump
        for &t in &border_tiles {
            self.el_scratch.visited[t as usize] = border_gen;
        }
        let current_gen = self.pe_bump_generation(tr); // 610 / 611 + bump

        let mut clusters: Vec<Vec<f64>> = Vec::new();
        let mut boxes: Vec<i32> = vec![0; 64];
        for start in border_tiles {
            if self.el_scratch.visited[start as usize] == current_gen {
                continue;
            }
            if clusters.len() * 4 >= boxes.len() {
                let mut nb = vec![0i32; boxes.len() * 2];
                nb[..boxes.len()].copy_from_slice(&boxes);
                boxes = nb;
            }
            let offset = clusters.len() * 4;
            let cluster = self.pe_flood_fill(
                tr,
                current_gen,
                &[start],
                true,
                FloodInclude::BorderGen(border_gen),
                Some((&mut boxes, offset)),
            );
            clusters.push(cluster);
        }
        (clusters, boxes)
    }

    /// `tileTraversalScratch(mg)` — the mock's `width() * height()` is traced
    /// (610 / 611) on every call.
    fn pe_traversal_state(&mut self, tr: &mut Vec<f64>) {
        let w = self.mg_width(tr); // 610
        let h = self.mg_height(tr); // 611
        self.el_scratch.ensure_capacity((w * h) as usize);
    }

    /// `bumpGeneration()` — `bumpTraversalGeneration(this.traversalState())`.
    fn pe_bump_generation(&mut self, tr: &mut Vec<f64>) -> u32 {
        self.pe_traversal_state(tr);
        self.el_scratch.bump_generation()
    }

    /// `floodFillWithGen(...)` — one `traversalState()` read for the reusable
    /// stack, then a plain DFS. `out_box` receives `[minX, minY, maxX, maxY]`.
    fn pe_flood_fill(
        &mut self,
        tr: &mut Vec<f64>,
        current_gen: u32,
        start_tiles: &[f64],
        diag: bool,
        include: FloodInclude,
        out_box: Option<(&mut Vec<i32>, usize)>,
    ) -> Vec<f64> {
        self.pe_traversal_state(tr); // 610 / 611 — `this.traversalState().stack`
        let map = self.el_map.clone().unwrap_or_default();
        let mut result: Vec<f64> = Vec::new();
        let mut stack: Vec<f64> = Vec::new();
        let mut min_x = 1e9_f64;
        let mut min_y = 1e9_f64;
        let mut max_x = -1e9_f64;
        let mut max_y = -1e9_f64;
        let track_box = out_box.is_some();

        for &start in start_tiles {
            if self.el_scratch.visited[start as usize] == current_gen {
                continue;
            }
            if !self.pe_include(tr, &include, start) {
                continue;
            }
            self.el_scratch.visited[start as usize] = current_gen;
            result.push(start);
            stack.push(start);
            if track_box {
                let x = map.x(start);
                let y = map.y(start);
                if x < min_x {
                    min_x = x;
                }
                if y < min_y {
                    min_y = y;
                }
                if x > max_x {
                    max_x = x;
                }
                if y > max_y {
                    max_y = y;
                }
            }
        }

        while let Some(tile) = stack.pop() {
            let n = if diag {
                map.neighbors8(tile, &mut self.nbuf8)
            } else {
                map.neighbors4(tile, &mut self.nbuf)
            };
            let buf = if diag { self.nbuf8 } else {
                let b = self.nbuf;
                let mut e = [0.0; 8];
                e[..4].copy_from_slice(&b);
                e
            };
            for &neighbor in buf.iter().take(n) {
                if self.el_scratch.visited[neighbor as usize] == current_gen {
                    continue;
                }
                if !self.pe_include(tr, &include, neighbor) {
                    continue;
                }
                self.el_scratch.visited[neighbor as usize] = current_gen;
                result.push(neighbor);
                stack.push(neighbor);
                if track_box {
                    let x = map.x(neighbor);
                    let y = map.y(neighbor);
                    if x < min_x {
                        min_x = x;
                    }
                    if y < min_y {
                        min_y = y;
                    }
                    if x > max_x {
                        max_x = x;
                    }
                    if y > max_y {
                        max_y = y;
                    }
                }
            }
        }

        if let Some((boxes, offset)) = out_box {
            boxes[offset] = min_x as i32;
            boxes[offset + 1] = min_y as i32;
            boxes[offset + 2] = max_x as i32;
            boxes[offset + 3] = max_y as i32;
        }
        result
    }

    /// The flood-include predicate (`visited[t] === borderGen` for the cluster
    /// fill, `mg.ownerID(t) === player.smallID()` for the removal fill).
    fn pe_include(&mut self, tr: &mut Vec<f64>, include: &FloodInclude, tile: f64) -> bool {
        match include {
            FloodInclude::BorderGen(g) => self.el_scratch.visited[tile as usize] == *g,
            FloodInclude::OwnedBy => {
                let owner = self.mg_owner_id(tr, tile); // 223
                let sid = self.pl_small_id(tr, self.pe_player); // 230
                owner == sid
            }
        }
    }

    // ---- G5k: `TribeExecution` (class 43) + `AiAttackBehavior` ---------------

    fn tribe_execution(
        &mut self,
        kind: u8,
        ticks: f64,
        tr: &mut Vec<f64>,
    ) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(false)),
            1 => Ok(JsVal::Undef),
            2 => {
                self.tribe_tick(ticks, tr)?;
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    fn tribe_tick(&mut self, ticks: f64, tr: &mut Vec<f64>) -> Result<(), String> {
        // `ticks % this.attackRate !== this.attackTick` gate.
        if ticks % self.te_attack_rate != self.te_attack_tick {
            return Ok(());
        }
        // `!this.tribe.isAlive()` gate.
        if !self.pl_is_alive(tr, self.te_tribe) {
            self.flag = false;
            return Ok(());
        }
        if !self.te_attack_behavior {
            // Lazy AiAttackBehavior ctor + first `sendAttack(terraNullius)`.
            self.te_attack_behavior = true;
            self.ai_send_attack(tr, TN_TOK, TargetVia::Tn, false)?;
            return Ok(());
        }
        self.tribe_accept_alliance_requests(tr);
        self.tribe_delete_next_structure(tr);
        self.tribe_maybe_attack(tr)?;
        Ok(())
    }

    /// `acceptAllAllianceRequests()`.
    fn tribe_accept_alliance_requests(&mut self, tr: &mut Vec<f64>) {
        let reqs = self.pl_incoming_alliance_requests(tr, self.te_tribe); // 720
        for r in reqs {
            self.rq_accept(tr, r); // 271
        }
        let alliances = self.pl_alliances(tr, self.te_tribe); // 954
        for al in alliances {
            if !self.al_only_one_agreed(tr, al) {
                // 260
                continue;
            }
            let human = self.al_other(tr, al, self.te_tribe); // 966
            let hid = self.pl_id(tr, human); // 231
            self.mg_add_alliance_extension_exec(tr, self.te_tribe, &hid);
        }
    }

    /// `deleteNextStructure()`.
    fn tribe_delete_next_structure(&mut self, tr: &mut Vec<f64>) {
        if !self.pl_can_delete_unit(tr, self.te_tribe) {
            // 324
            return;
        }
        let units = self.pl_units_all(tr, self.te_tribe); // 241 (no args)
        for u in units {
            let ty = self.un_type(tr, u); // 283
            if !STRUCTURES_TYPES.contains(&ty.as_str()) {
                continue;
            }
            if self.un_is_marked_for_deletion(tr, u) {
                // 913
                continue;
            }
            let uid = self.un_id(tr, u); // 340
            self.mg_add_delete_unit_exec(tr, self.te_tribe, uid);
            return;
        }
    }

    /// `maybeAttack()`.
    fn tribe_maybe_attack(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        let to_attack = self.ai_get_neighbor_traitor_to_attack(tr);
        if let Some(ta) = to_attack {
            let friendly = self.pl_is_friendly(tr, self.te_tribe, Some(ta)); // 242
            let odds = if friendly { 6.0 } else { 3.0 };
            let rng = self.random.as_mut().unwrap();
            if rng.chance(odds) {
                let alliance = self.pl_alliance_with(tr, self.te_tribe, Some(ta)); // 243
                if let Some(al) = alliance {
                    self.pl_break_alliance(tr, self.te_tribe, al); // 244
                }
                if self.ai_send_attack(tr, ta, TargetVia::Nb, false)? {
                    return Ok(());
                }
            }
        }
        if self.te_neighbors_tn {
            let neighbors = self.pl_nearby(tr, self.te_tribe); // 245
            let has_tn = neighbors.iter().any(|&n| !self.nb_is_player(tr, n)); // 257
            if has_tn {
                if self.ai_send_attack(tr, TN_TOK, TargetVia::Tn, false)? {
                    return Ok(());
                }
            } else {
                self.te_neighbors_tn = false;
            }
        }
        self.ai_attack_random_target(tr)?;
        Ok(())
    }

    // ---- NationExecution (class 44) ------------------------------------------

    fn nation_execution(
        &mut self,
        kind: u8,
        ticks: f64,
        tr: &mut Vec<f64>,
    ) -> Result<JsVal, String> {
        match kind {
            3 => Ok(JsVal::Bool(self.flag)),
            4 => Ok(JsVal::Bool(true)), // `activeDuringSpawnPhase(): true`
            1 => {
                self.ne_init(tr)?;
                Ok(JsVal::Undef)
            }
            2 => {
                self.ne_tick(ticks, tr)?;
                Ok(JsVal::Undef)
            }
            _ => unreachable!(),
        }
    }

    /// `init(mg)`.
    fn ne_init(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        let rate = self.ne_get_attack_rate(tr)?;
        self.ne_attack_rate = rate;
        let rng = self.random.as_mut().unwrap();
        self.ne_attack_tick = rng.next_int(0.0, rate) as f64;
        let id = self.ne_nation_id.clone();
        if !self.mg_has_player(tr, &id) {
            // 210
            let info = self.ne_player_info.clone();
            self.ne_player = self.mg_add_player(tr, &info); // 605
        } else {
            self.ne_player = self.mg_player(tr, &id); // 211
        }
        if let Some(p) = self.ne_player {
            self.te_tribe = p;
        }
        self.ne_initialized = true;
        Ok(())
    }

    /// `getAttackRate()`.
    fn ne_get_attack_rate(&mut self, tr: &mut Vec<f64>) -> Result<f64, String> {
        let diff = self.mg_difficulty(tr); // 402
        let rng = self.random.as_mut().unwrap();
        match diff.as_str() {
            DIFFICULTY_EASY => Ok(rng.next_int(65.0, 100.0) as f64),
            DIFFICULTY_MEDIUM => Ok(rng.next_int(55.0, 70.0) as f64),
            DIFFICULTY_HARD => Ok(rng.next_int(45.0, 60.0) as f64),
            DIFFICULTY_IMPOSSIBLE => Ok(rng.next_int(30.0, 50.0) as f64),
            other => Err(format!("Unexpected value: {other}")),
        }
    }

    /// `tick(ticks)`.
    fn ne_tick(&mut self, ticks: f64, tr: &mut Vec<f64>) -> Result<(), String> {
        // Ship tracking gate (short-circuit order mirrors the TS `&&` chain).
        if self.ne_behaviors_initialized {
            if let Some(pl) = self.ne_player {
                if self.pl_is_alive(tr, pl) // 233
                    && self.mg_difficulty(tr) != DIFFICULTY_EASY // 402
                    && {
                        let c = self.pl_units_constructed(tr, pl, UNIT_PORT); // 982
                        c != 0.0 && !c.is_nan()
                    }
                    && !self.mg_is_unit_disabled(tr, UNIT_WARSHIP)
                // 511
                {
                    self.ne_track_ships_and_retaliate(tr)?;
                }
            }
        }

        let pl = match self.ne_player {
            Some(p) => p,
            None => return Ok(()),
        };

        if self.mg_in_spawn_phase(tr) {
            // 216
            if self.pl_has_spawned(tr, pl) {
                // 530
                if ticks % self.ne_attack_rate != self.ne_attack_tick {
                    return Ok(());
                }
            } else if self.ne_spawn_exec_added {
                return Ok(());
            }
            if self.ne_spawn_cell.is_none() {
                let gid = self.el_game_id.clone();
                let info = self.ne_player_info.clone();
                self.mg_add_spawn_exec(tr, &gid, &info, None);
                self.ne_spawn_exec_added = true;
                return Ok(());
            }
            let team = self.pl_team(tr, pl); // 248
            if !matches!(team, JsVal::Null) {
                let key = as_str(&team);
                let area_v = self.mg_team_spawn_area(tr, &key); // 613
                if let Some((ax, ay, aw, ah)) = Self::area_from_val(&area_v) {
                    let (cx, cy) = self.ne_spawn_cell.unwrap();
                    let in_area =
                        cx >= ax && cx < ax + aw && cy >= ay && cy < ay + ah;
                    if !in_area {
                        let gid = self.el_game_id.clone();
                        let info = self.ne_player_info.clone();
                        self.mg_add_spawn_exec(tr, &gid, &info, None);
                        self.ne_spawn_exec_added = true;
                        return Ok(());
                    }
                }
            }
            let rl = self.ne_random_spawn_land(tr)?;
            if let Some(tile) = rl {
                let gid = self.el_game_id.clone();
                let info = self.ne_player_info.clone();
                self.mg_add_spawn_exec(tr, &gid, &info, Some(tile));
                self.ne_spawn_exec_added = true;
            }
            // `console.warn` is not traced.
            return Ok(());
        }

        if self.ne_spawn_exec_added && !self.pl_has_spawned(tr, pl) {
            // 530
            return Ok(());
        }

        if !self.pl_is_alive(tr, pl) {
            // 233
            self.flag = false;
            return Ok(());
        }

        if !self.ne_behaviors_initialized {
            self.ne_initialize_behaviors(tr)?;
            self.ai_force_send_attack(tr, TN_TOK)?;
            return Ok(());
        }

        if ticks % self.ne_attack_rate != self.ne_attack_tick {
            if self.pl_is_alive(tr, pl) {
                // 233
                let offset = ticks % self.ne_attack_rate;
                let one_third = (self.ne_attack_tick
                    + (self.ne_attack_rate / 3.0).floor())
                    % self.ne_attack_rate;
                let two_thirds = (self.ne_attack_tick
                    + ((self.ne_attack_rate * 2.0) / 3.0).floor())
                    % self.ne_attack_rate;
                if offset == one_third || offset == two_thirds {
                    self.ne_handle_structures(tr)?;
                }
            }
            return Ok(());
        }

        self.ne_maybe_send_casual_emoji(tr)?;
        self.ne_update_relations_from_embargos(tr)?;
        self.ne_handle_alliance_requests(tr)?;
        self.ne_handle_alliance_extension_requests(tr)?;
        self.ne_consider_mirv(tr)?;
        self.ne_handle_structures(tr)?;
        self.ne_maybe_spawn_warship(tr)?;
        self.ne_handle_embargoes_to_hostile_nations(tr)?;
        self.ai_maybe_attack(tr)?;
        self.ne_counter_warship_infestation(tr)?;
        self.ne_maybe_send_nuke(tr)?;
        Ok(())
    }

    /// `initializeBehaviors()`. Only the `NationNukeBehavior` field
    /// initializers trace: `cost(AtomBomb)` (512 -> 969), `cost(HydrogenBomb)`
    /// (512 -> 969) and `random.chance(3)`. The other six constructors read
    /// nothing.
    fn ne_initialize_behaviors(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        let pl = self.ne_player.ok_or("Player not initialized")?;
        let atom = self.ne_cost(tr, UNIT_ATOM_BOMB, pl);
        let hydro = self.ne_cost(tr, UNIT_HYDROGEN_BOMB, pl);
        self.ne_atom_perceived_cost = atom;
        self.ne_hydro_perceived_cost = hydro;
        self.ne_is_hydro_nation = self.random.as_mut().unwrap().chance(3.0);
        self.ne_behaviors_initialized = true;
        Ok(())
    }

    /// `game.unitInfo(type).cost(game, player)` — the paired 512 / 969 reads.
    fn ne_cost(&mut self, tr: &mut Vec<f64>, ty: &str, player: usize) -> i64 {
        let info = self.mg_unit_info(tr, ty); // 512
        let _ = info;
        self.mg_unit_info_cost(tr, ty, player) // 969
    }

    /// `randomSpawnLand()`.
    fn ne_random_spawn_land(&mut self, tr: &mut Vec<f64>) -> Result<Option<f64>, String> {
        let (cell_x, cell_y) = self.ne_spawn_cell.ok_or("not initialized")?;
        let delta = 25.0;
        let mut tries = 0;
        while tries < 50 {
            tries += 1;
            let x = self
                .random
                .as_mut()
                .unwrap()
                .next_int(cell_x - delta, cell_x + delta) as f64;
            let y = self
                .random
                .as_mut()
                .unwrap()
                .next_int(cell_y - delta, cell_y + delta) as f64;
            if !self.mg_is_valid_coord(tr, x, y) {
                // 616
                continue;
            }
            let tile = self.mg_ref(tr, x, y); // 609
            if self.mg_is_land(tr, tile) // 304
                && !self.mg_has_owner(tr, tile) // 607
                && !self.mg_is_impassable(tr, tile)
            // 617
            {
                if self.mg_terrain_type(tr, tile) == TERRAIN_MOUNTAIN // 805
                    && self.random.as_mut().unwrap().chance(2.0)
                {
                    continue;
                }
                return Ok(Some(tile));
            }
        }
        Ok(None)
    }

    /// `updateRelationsFromEmbargos()`.
    fn ne_update_relations_from_embargos(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        let pl = match self.ne_player {
            Some(p) => p,
            None => return Ok(()),
        };
        let others: Vec<usize> = self
            .mg_players(tr) // 221
            .into_iter()
            .filter(|&o| {
                let oid = self.pl_id(tr, o); // 231
                let pid = self.pl_id(tr, pl); // 231 (re-read per element)
                oid != pid
            })
            .collect();
        for &other in &others {
            let has1 = self.pl_has_embargo_against(tr, other, Some(pl)); // 238
            let not_applied = has1 && {
                let oid = self.pl_id(tr, other); // 231
                !self.ne_embargo_malus_applied.contains(&oid)
            };
            if has1 && not_applied {
                self.pl_update_relation(tr, pl, Some(other), -20.0); // 237
                let oid = self.pl_id(tr, other); // 231 (`add(other.id())`)
                if !self.ne_embargo_malus_applied.contains(&oid) {
                    self.ne_embargo_malus_applied.push(oid);
                }
            } else if !self.pl_has_embargo_against(tr, other, Some(pl)) // 238 (re-read)
                && {
                    let oid = self.pl_id(tr, other); // 231
                    self.ne_embargo_malus_applied.contains(&oid)
                }
            {
                self.pl_update_relation(tr, pl, Some(other), 20.0); // 237
                let oid = self.pl_id(tr, other); // 231 (`delete(other.id())`)
                self.ne_embargo_malus_applied.retain(|x| x != &oid);
            }
        }
        Ok(())
    }

    /// `handleEmbargoesToHostileNations()`.
    fn ne_handle_embargoes_to_hostile_nations(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        let pl = match self.ne_player {
            Some(p) => p,
            None => return Ok(()),
        };
        let others: Vec<usize> = self
            .mg_players(tr) // 221
            .into_iter()
            .filter(|&o| {
                let oid = self.pl_id(tr, o); // 231
                let pid = self.pl_id(tr, pl); // 231 (re-read per element)
                oid != pid
            })
            .collect();
        let difficulty = self.mg_difficulty(tr); // 402
        let is_higher = difficulty == DIFFICULTY_HARD || difficulty == DIFFICULTY_IMPOSSIBLE;
        let team_game = self.mg_game_mode(tr) == GAME_MODE_TEAM; // 520

        for &other in &others {
            if team_game
                && is_higher
                && self.pl_type(tr, other) != PLAYER_TYPE_BOT // 251
                && !self.pl_is_on_same_team(tr, pl, Some(other))
            // 250
            {
                if !self.pl_has_embargo_against(tr, pl, Some(other)) {
                    // 238
                    self.pl_add_embargo(tr, pl, Some(other), false); // 253
                }
                continue;
            }
            #[allow(clippy::if_same_then_else)]
            if self.pl_relation(tr, pl, Some(other)) <= RELATION_HOSTILE // 412
                && !self.pl_has_embargo_against(tr, pl, Some(other)) // 238
                && !self.pl_is_on_same_team(tr, pl, Some(other))
            // 250
            {
                self.pl_add_embargo(tr, pl, Some(other), false); // 253
            } else if self.pl_relation(tr, pl, Some(other)) >= RELATION_NEUTRAL // 412
                && self.pl_has_embargo_against(tr, pl, Some(other)) // 238
                && difficulty != DIFFICULTY_HARD
                && difficulty != DIFFICULTY_IMPOSSIBLE
            {
                self.pl_stop_embargo(tr, pl, Some(other)); // 254
            } else if self.pl_relation(tr, pl, Some(other)) >= RELATION_FRIENDLY // 412
                && self.pl_has_embargo_against(tr, pl, Some(other)) // 238
                && difficulty != DIFFICULTY_IMPOSSIBLE
            {
                self.pl_stop_embargo(tr, pl, Some(other)); // 254
            }
        }
        Ok(())
    }

    // ---- nation behavior entry points (implemented in the `ne_*` family) ----

    // ---- NationEmojiBehavior (ported inline) ---------------------------------

    /// `maybeSendCasualEmoji()`.
    fn ne_maybe_send_casual_emoji(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        if self.ne_game_over {
            return Ok(());
        }
        self.ne_check_overwhelmed_by_attacks(tr)?;
        self.ne_check_very_small_attack(tr)?;
        self.ne_congratulate_winner(tr)?;
        self.ne_brag(tr)?;
        self.ne_charm_allies(tr)?;
        self.ne_annoy_traitors(tr)?;
        self.ne_find_rat(tr)?;
        self.ne_greet_nearby_players(tr)?;
        Ok(())
    }

    fn ne_check_overwhelmed_by_attacks(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        let pl = self.ne_player.ok_or("Player not initialized")?;
        if !self.random.as_mut().unwrap().chance(16.0) {
            return Ok(());
        }
        let incoming = self.pl_incoming_attacks(tr, pl); // 822
        if incoming.is_empty() {
            return Ok(());
        }
        let mut incoming_troops = 0.0;
        for a in &incoming {
            incoming_troops += self.at_troops(tr, *a); // 830
        }
        let our_troops = self.pl_troops(tr, pl); // 418
        if incoming_troops >= our_troops * 3.0 {
            self.ne_send_emoji(tr, Rec::All, "EMOJI_OVERWHELMED")?;
        }
        Ok(())
    }

    fn ne_check_very_small_attack(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        let pl = self.ne_player.ok_or("Player not initialized")?;
        if !self.random.as_mut().unwrap().chance(8.0) {
            return Ok(());
        }
        let incoming = self.pl_incoming_attacks(tr, pl); // 822
        if incoming.is_empty() {
            return Ok(());
        }
        let our_troops = self.pl_troops(tr, pl); // 418
        if our_troops <= 0.0 {
            return Ok(());
        }
        for a in &incoming {
            let attacker = match self.at_attacker(tr, *a) {
                // 842
                Some(t) => t,
                None => continue,
            };
            if self.pl_type(tr, attacker) != PLAYER_TYPE_HUMAN {
                // 251
                continue;
            }
            if self.at_troops(tr, *a) < our_troops * 0.1 {
                // 830
                let list = if self.random.as_mut().unwrap().chance(2.0) {
                    "EMOJI_CONFUSED"
                } else {
                    "EMOJI_BORED"
                };
                self.ne_maybe_send_emoji(tr, Rec::P(attacker), list)?;
            }
        }
        Ok(())
    }

    fn ne_congratulate_winner(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        let pl = self.ne_player.ok_or("Player not initialized")?;
        let winner = self.mg_get_winner(tr); // 600
        if matches!(winner, JsVal::Null) || matches!(winner, JsVal::Absent) {
            return Ok(());
        }
        self.ne_game_over = true;
        let is_team_game = self.mg_game_mode(tr) == GAME_MODE_TEAM; // 520
        if is_team_game {
            let team = self.pl_team(tr, pl); // 248
            // `winner === this.player.team()`: only two equal team strings
            // can be `===` (a Player object never equals a string / null).
            if matches!((&winner, &team), (JsVal::Str(a), JsVal::Str(b)) if a == b) {
                return Ok(());
            }
            self.ne_send_emoji(tr, Rec::All, "EMOJI_CONGRATULATE")?;
        } else {
            if matches!(winner, JsVal::Str(_)) {
                return Ok(()); // a team string, not a player
            }
            let nations: Vec<usize> = self
                .mg_players(tr) // 221
                .into_iter()
                .filter(|&p| self.pl_type(tr, p) == PLAYER_TYPE_NATION) // 251
                .collect();
            let sorted = self.sort_players_by_tiles(tr, nations);
            match sorted.first() {
                Some(&first) if first == pl => {
                    let wtok = as_num(&winner) as usize;
                    self.ne_send_emoji(tr, Rec::P(wtok), "EMOJI_CONGRATULATE")?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn ne_brag(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        let pl = self.ne_player.ok_or("Player not initialized")?;
        if !self.random.as_mut().unwrap().chance(300.0) {
            return Ok(());
        }
        let players = self.mg_players(tr); // 221
        let sorted = self.sort_players_by_tiles(tr, players);
        if sorted.is_empty() || sorted[0] != pl {
            return Ok(());
        }
        self.ne_send_emoji(tr, Rec::All, "EMOJI_BRAG")
    }

    fn ne_charm_allies(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        let pl = self.ne_player.ok_or("Player not initialized")?;
        if !self.random.as_mut().unwrap().chance(250.0) {
            return Ok(());
        }
        let human_allies: Vec<usize> = self
            .pl_allies(tr, pl) // 964
            .into_iter()
            .filter(|&a| self.pl_type(tr, a) == PLAYER_TYPE_HUMAN) // 251
            .collect();
        if human_allies.is_empty() {
            return Ok(());
        }
        let idx = self
            .random
            .as_mut()
            .unwrap()
            .rand_element(&human_allies)
            .copied()
            .unwrap();
        let list = if self.random.as_mut().unwrap().chance(3.0) {
            "EMOJI_LOVE"
        } else {
            "EMOJI_CHARM_ALLIES"
        };
        self.ne_send_emoji(tr, Rec::P(idx), list)
    }

    fn ne_annoy_traitors(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        let pl = self.ne_player.ok_or("Player not initialized")?;
        if !self.random.as_mut().unwrap().chance(40.0) {
            return Ok(());
        }
        let traitors: Vec<usize> = self
            .mg_players(tr) // 221
            .into_iter()
            .filter(|&p| {
                self.pl_type(tr, p) == PLAYER_TYPE_HUMAN // 251
                    && !self.pl_is_friendly(tr, p, Some(pl)) // 242
                    && self.pl_is_traitor(tr, p) // 826
            })
            .collect();
        if traitors.is_empty() {
            return Ok(());
        }
        let idx = self
            .random
            .as_mut()
            .unwrap()
            .rand_element(&traitors)
            .copied()
            .unwrap();
        self.ne_send_emoji(tr, Rec::P(idx), "EMOJI_CLOWN")
    }

    fn ne_find_rat(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        self.ne_player.ok_or("Player not initialized")?;
        if self.mg_ticks(tr) < 6000.0 {
            // 212
            return Ok(());
        }
        if !self.random.as_mut().unwrap().chance(10000.0) {
            return Ok(());
        }
        let threshold = self.mg_num_land_tiles(tr) * 0.01; // 501
        let small: Vec<usize> = self
            .mg_players(tr) // 221
            .into_iter()
            .filter(|&p| {
                self.pl_type(tr, p) == PLAYER_TYPE_HUMAN // 251
                    && self.pl_num_tiles_owned(tr, p) < threshold // 532
                    && self.pl_num_tiles_owned(tr, p) > 0.0 // 532 (re-read)
            })
            .collect();
        if small.is_empty() {
            return Ok(());
        }
        let idx = self
            .random
            .as_mut()
            .unwrap()
            .rand_element(&small)
            .copied()
            .unwrap();
        self.ne_send_emoji(tr, Rec::P(idx), "EMOJI_RAT")
    }

    fn ne_greet_nearby_players(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        let pl = self.ne_player.ok_or("Player not initialized")?;
        if self.mg_ticks(tr) > 600.0 {
            // 212
            return Ok(());
        }
        if !self.random.as_mut().unwrap().chance(250.0) {
            return Ok(());
        }
        let nearby = self.pl_nearby(tr, pl); // 245
        let humans: Vec<usize> = nearby
            .into_iter()
            .filter(|&n| {
                self.nb_is_player(tr, n) // 257
                    && self.pl_type(tr, n) == PLAYER_TYPE_HUMAN // 251
            })
            .collect();
        if humans.is_empty() {
            return Ok(());
        }
        let idx = self
            .random
            .as_mut()
            .unwrap()
            .rand_element(&humans)
            .copied()
            .unwrap();
        self.ne_send_emoji(tr, Rec::P(idx), "EMOJI_GREET")
    }

    /// `maybeSendEmoji(other, emojisList)`.
    fn ne_maybe_send_emoji(
        &mut self,
        tr: &mut Vec<f64>,
        other: Rec,
        list: &str,
    ) -> Result<(), String> {
        if !self.ne_should_send_emoji(tr, other, true)? {
            return Ok(());
        }
        self.ne_send_emoji(tr, other, list)
    }

    /// `maybeSendAttackEmoji(otherPlayer)`.
    fn ne_maybe_send_attack_emoji(&mut self, tr: &mut Vec<f64>, other: usize) -> Result<(), String> {
        let pl = self.ne_player.ok_or("Player not initialized")?;
        if !self.ne_should_send_emoji(tr, Rec::P(other), true)? {
            return Ok(());
        }
        if self.pl_relation(tr, pl, Some(other)) >= RELATION_NEUTRAL {
            // 412
            if !self.random.as_mut().unwrap().chance(2.0) {
                return Ok(());
            }
            return self.ne_send_emoji(tr, Rec::P(other), "EMOJI_AGGRESSIVE_ATTACK");
        }
        if !self.random.as_mut().unwrap().chance(4.0) {
            return Ok(());
        }
        self.ne_send_emoji(tr, Rec::P(other), "EMOJI_ATTACK")
    }

    /// `sendEmoji(otherPlayer, emojisList)`.
    fn ne_send_emoji(&mut self, tr: &mut Vec<f64>, other: Rec, list: &str) -> Result<(), String> {
        let pl = self.ne_player.ok_or("Player not initialized")?;
        if !self.ne_should_send_emoji(tr, other, false)? {
            return Ok(());
        }
        if !self.pl_can_send_emoji(tr, pl, other) {
            // 410
            return Ok(());
        }
        let ids = emoji_ids(list);
        let emoji = self
            .random
            .as_mut()
            .unwrap()
            .rand_element(&ids)
            .copied()
            .unwrap_or(f64::NAN);
        let recipient_id = match other {
            Rec::All => ALL_PLAYERS.to_string(),
            Rec::P(t) => self.pl_id(tr, t), // 231
            _ => return Ok(()),
        };
        self.mg_add_execution_emoji(tr, pl, &recipient_id, emoji);
        Ok(())
    }

    /// `shouldSendEmoji(otherPlayer, limitEmojisByTime = true)`.
    fn ne_should_send_emoji(
        &mut self,
        tr: &mut Vec<f64>,
        other: Rec,
        limit: bool,
    ) -> Result<bool, String> {
        if other == Rec::All {
            return Ok(true);
        }
        let pl = self.ne_player.ok_or("Player not initialized")?;
        if self.pl_type(tr, pl) == PLAYER_TYPE_BOT {
            // 251
            return Ok(false);
        }
        match other {
            Rec::P(t) => {
                if self.pl_type(tr, t) != PLAYER_TYPE_HUMAN {
                    // 251
                    return Ok(false);
                }
                if limit {
                    let last_sent = self
                        .ne_last_emoji_sent
                        .iter()
                        .find(|(k, _)| *k == t)
                        .map(|(_, v)| *v)
                        .unwrap_or(-300.0);
                    if self.mg_ticks(tr) - last_sent <= 300.0 {
                        // 212
                        return Ok(false);
                    }
                    let now = self.mg_ticks(tr); // 212 (re-read for `set`)
                    if let Some(e) = self.ne_last_emoji_sent.iter_mut().find(|(k, _)| *k == t) {
                        e.1 = now;
                    } else {
                        self.ne_last_emoji_sent.push((t, now));
                    }
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// `respondToMIRV(game, random, mirvTarget)` (module-level helper).
    fn ne_respond_to_mirv(&mut self, tr: &mut Vec<f64>, mirv_target: usize) -> Result<(), String> {
        if !self.random.as_mut().unwrap().chance(8.0) {
            return Ok(());
        }
        if !self.pl_can_send_emoji(tr, mirv_target, Rec::All) {
            // 410
            return Ok(());
        }
        let ids = emoji_ids("EMOJI_OVERWHELMED");
        let emoji = self
            .random
            .as_mut()
            .unwrap()
            .rand_element(&ids)
            .copied()
            .unwrap_or(f64::NAN);
        self.mg_add_execution_emoji(tr, mirv_target, ALL_PLAYERS, emoji);
        Ok(())
    }

    // ---- NationAllianceBehavior (ported inline) ------------------------------

    /// `handleAllianceRequests()`.
    fn ne_handle_alliance_requests(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        let pl = self.ne_player.ok_or("Player not initialized")?;
        if self.mg_disable_alliances(tr) {
            // 958
            return Ok(());
        }
        let reqs = self.pl_incoming_alliance_requests(tr, pl); // 720
        for r in reqs {
            let created = self.rq_created_at(tr, r); // 274
            let spawn_turns = self.mg_num_spawn_phase_turns(tr); // 215
            if created <= spawn_turns + 1.0 {
                self.rq_reject(tr, r); // 272
                continue;
            }
            let requestor = self.rq_requestor(tr, r); // 744
            let ok = match requestor {
                Some(req_tok) => self.ne_get_alliance_decision(tr, req_tok, true)?,
                None => false,
            };
            if ok {
                self.rq_accept(tr, r); // 271
            } else {
                self.rq_reject(tr, r); // 272
            }
        }
        Ok(())
    }

    /// `handleAllianceExtensionRequests()`.
    fn ne_handle_alliance_extension_requests(
        &mut self,
        tr: &mut Vec<f64>,
    ) -> Result<(), String> {
        let pl = self.ne_player.ok_or("Player not initialized")?;
        if self.mg_disable_alliances(tr) {
            // 958
            return Ok(());
        }
        let alliances = self.pl_alliances(tr, pl); // 954
        for al in alliances {
            if !self.al_only_one_agreed(tr, al) {
                // 260
                continue;
            }
            let human = self.al_other(tr, al, pl); // 966
            if !self.ne_get_alliance_decision(tr, human, true)? {
                continue;
            }
            let hid = self.pl_id(tr, human); // 231
            self.mg_add_alliance_extension_exec(tr, pl, &hid);
        }
        Ok(())
    }

    /// `maybeSendAllianceRequests(borderingEnemies)`.
    fn ne_maybe_send_alliance_requests(
        &mut self,
        tr: &mut Vec<f64>,
        bordering_enemies: &[usize],
    ) -> Result<(), String> {
        let pl = self.ne_player.ok_or("Player not initialized")?;
        if self.mg_disable_alliances(tr) {
            // 958
            return Ok(());
        }
        for &enemy in bordering_enemies {
            if !self.random.as_mut().unwrap().chance(30.0) {
                continue;
            }
            if !self.ne_acceptable_player_type(tr, enemy)? {
                continue;
            }
            if !self.pl_can_send_alliance_request(tr, pl, Some(enemy)) {
                // 235
                continue;
            }
            if self.ne_get_alliance_decision(tr, enemy, false)? {
                let eid = self.pl_id(tr, enemy); // 231
                self.mg_add_alliance_request_exec(tr, pl, &eid);
            }
        }
        Ok(())
    }

    /// `(p.type() === Bot && difficulty === Easy) || p.type() !== Bot` — the
    /// `||` re-reads `type()` unless the left arm short-circuits to `true`.
    fn ne_acceptable_player_type(&mut self, tr: &mut Vec<f64>, p: usize) -> Result<bool, String> {
        if self.pl_type(tr, p) == PLAYER_TYPE_BOT {
            // 251
            if self.mg_difficulty(tr) == DIFFICULTY_EASY {
                // 402
                return Ok(true);
            }
        }
        Ok(self.pl_type(tr, p) != PLAYER_TYPE_BOT) // 251 (second read)
    }

    /// `getAllianceDecision(otherPlayer, isResponse)`.
    fn ne_get_alliance_decision(
        &mut self,
        tr: &mut Vec<f64>,
        other: usize,
        is_response: bool,
    ) -> Result<bool, String> {
        if self.ne_is_confused(tr)? {
            let rng = self.random.as_mut().unwrap();
            return Ok(rng.chance(2.0));
        }
        if self.pl_is_traitor(tr, other) {
            // 826
            let roll = self.random.as_mut().unwrap().next_int(0.0, 100.0);
            if roll >= 10 {
                if is_response && self.random.as_mut().unwrap().chance(3.0) {
                    self.ne_send_emoji(tr, Rec::P(other), "EMOJI_CONFUSED")?;
                }
                return Ok(false);
            }
        }
        if self.ne_has_too_many_alliances(tr, other)? {
            return Ok(false);
        }
        if self.ne_is_alliance_partner_threat(tr, other)? {
            if !is_response && self.random.as_mut().unwrap().chance(6.0) {
                self.ne_send_emoji(tr, Rec::P(other), "EMOJI_SCARED_OF_THREAT")?;
            }
            if is_response && self.random.as_mut().unwrap().chance(6.0) {
                self.ne_send_emoji(tr, Rec::P(other), "EMOJI_LOVE")?;
            }
            return Ok(true);
        }
        if self.ne_should_reject_in_team_game(tr)? {
            return Ok(false);
        }
        let pl = self.ne_player.ok_or("Player not initialized")?;
        if self.pl_relation(tr, pl, Some(other)) < RELATION_NEUTRAL {
            // 412
            if is_response && self.random.as_mut().unwrap().chance(3.0) {
                self.ne_send_emoji(tr, Rec::P(other), "EMOJI_CONFUSED")?;
            }
            return Ok(false);
        }
        if self.ne_is_alliance_partner_friendly(tr, other)? {
            if self.random.as_mut().unwrap().chance(3.0) {
                self.ne_send_emoji(tr, Rec::P(other), "EMOJI_HANDSHAKE")?;
            }
            return Ok(true);
        }
        if self.ne_check_already_enough_alliances(tr, other)? {
            return Ok(false);
        }
        if self.ne_is_earlygame(tr)? {
            return Ok(true);
        }
        self.ne_is_alliance_partner_similarly_strong(tr, other)
    }

    /// `hasTooManyAlliances(otherPlayer)`.
    fn ne_has_too_many_alliances(&mut self, tr: &mut Vec<f64>, other: usize) -> Result<bool, String> {
        let difficulty = self.mg_difficulty(tr); // 402
        if difficulty != DIFFICULTY_HARD && difficulty != DIFFICULTY_IMPOSSIBLE {
            return Ok(false);
        }
        let total_players = self
            .mg_players(tr) // 221
            .into_iter()
            .filter(|&p| self.pl_type(tr, p) != PLAYER_TYPE_BOT) // 251
            .count() as f64;
        let other_alliances = self.pl_alliances(tr, other).len() as f64; // 954
        if difficulty == DIFFICULTY_HARD {
            Ok(other_alliances >= total_players * 0.5)
        } else {
            Ok(other_alliances >= total_players * 0.25)
        }
    }

    /// `isConfused()`.
    fn ne_is_confused(&mut self, tr: &mut Vec<f64>) -> Result<bool, String> {
        let diff = self.mg_difficulty(tr); // 402
        let rng = self.random.as_mut().unwrap();
        match diff.as_str() {
            DIFFICULTY_EASY => Ok(rng.chance(10.0)),
            DIFFICULTY_MEDIUM => Ok(rng.chance(20.0)),
            DIFFICULTY_HARD => Ok(rng.chance(40.0)),
            DIFFICULTY_IMPOSSIBLE => Ok(false),
            other => Err(format!("Unexpected value: {other}")),
        }
    }

    /// `isEarlygame()`.
    fn ne_is_earlygame(&mut self, tr: &mut Vec<f64>) -> Result<bool, String> {
        let spawn_ticks = self.mg_num_spawn_phase_turns(tr); // 215
        let difficulty = self.mg_difficulty(tr); // 402
        let (limit, floor) = match difficulty.as_str() {
            DIFFICULTY_EASY => (3000.0, 10.0),
            DIFFICULTY_MEDIUM => (1800.0, 30.0),
            DIFFICULTY_HARD => (1800.0, 50.0),
            DIFFICULTY_IMPOSSIBLE => (600.0, 70.0),
            other => return Err(format!("Unexpected value: {other}")),
        };
        if self.mg_ticks(tr) >= limit + spawn_ticks {
            // 212
            return Ok(false);
        }
        let roll = self.random.as_mut().unwrap().next_int(0.0, 100.0);
        Ok(roll >= floor as i64)
    }

    /// `isAlliancePartnerThreat(otherPlayer)`.
    fn ne_is_alliance_partner_threat(&mut self, tr: &mut Vec<f64>, other: usize) -> Result<bool, String> {
        let pl = self.ne_player.ok_or("Player not initialized")?;
        let difficulty = self.mg_difficulty(tr); // 402
        match difficulty.as_str() {
            DIFFICULTY_EASY => Ok(false),
            DIFFICULTY_MEDIUM => {
                let ot = self.pl_troops(tr, other); // 418
                let pt = self.pl_troops(tr, pl); // 418
                Ok(ot > pt * 2.5)
            }
            DIFFICULTY_HARD => {
                let ot = self.pl_troops(tr, other); // 418
                let pt = self.pl_troops(tr, pl); // 418
                #[allow(clippy::neg_cmp_op_on_partial_ord)]
                if !(ot > pt) {
                    return Ok(false);
                }
                let om = self.mg_max_troops(tr, Rec::P(other)); // 401
                let pm = self.mg_max_troops(tr, Rec::P(pl)); // 401
                Ok(om > pm * 2.0)
            }
            DIFFICULTY_IMPOSSIBLE => {
                // The three `const` booleans are eagerly evaluated before the
                // `||` chain, so every `troops()` pair is read regardless of
                // the earlier results.
                let ot = self.pl_troops(tr, other); // 418
                let pt = self.pl_troops(tr, pl); // 418
                let more_troops = ot > pt * 1.5;
                let ot2 = self.pl_troops(tr, other); // 418
                let pt2 = self.pl_troops(tr, pl); // 418
                let more_max_troops = if ot2 > pt2 {
                    let om = self.mg_max_troops(tr, Rec::P(other)); // 401
                    let pm = self.mg_max_troops(tr, Rec::P(pl)); // 401
                    om > pm * 1.5
                } else {
                    false
                };
                let ot3 = self.pl_troops(tr, other); // 418
                let pt3 = self.pl_troops(tr, pl); // 418
                let more_tiles = if ot3 > pt3 {
                    let ont = self.pl_num_tiles_owned(tr, other); // 532
                    let pnt = self.pl_num_tiles_owned(tr, pl); // 532
                    ont > pnt * 1.5
                } else {
                    false
                };
                Ok(more_troops || more_max_troops || more_tiles)
            }
            other => Err(format!("Unexpected value: {other}")),
        }
    }

    /// `shouldRejectInTeamGame()`.
    fn ne_should_reject_in_team_game(&mut self, tr: &mut Vec<f64>) -> Result<bool, String> {
        if self.mg_game_mode(tr) != GAME_MODE_TEAM {
            // 520
            return Ok(false);
        }
        let difficulty = self.mg_difficulty(tr); // 402
        match difficulty.as_str() {
            DIFFICULTY_EASY => Ok(self.random.as_mut().unwrap().next_int(0.0, 100.0) < 25),
            DIFFICULTY_MEDIUM => Ok(self.random.as_mut().unwrap().next_int(0.0, 100.0) < 50),
            DIFFICULTY_HARD => Ok(self.random.as_mut().unwrap().next_int(0.0, 100.0) < 75),
            DIFFICULTY_IMPOSSIBLE => Ok(true),
            other => Err(format!("Unexpected value: {other}")),
        }
    }

    /// `checkAlreadyEnoughAlliances(otherPlayer)`.
    fn ne_check_already_enough_alliances(
        &mut self,
        tr: &mut Vec<f64>,
        other: usize,
    ) -> Result<bool, String> {
        let pl = self.ne_player.ok_or("Player not initialized")?;
        let difficulty = self.mg_difficulty(tr); // 402
        match difficulty.as_str() {
            DIFFICULTY_EASY => Ok(false),
            DIFFICULTY_MEDIUM => {
                let n = self.pl_alliances(tr, pl).len() as f64; // 954
                let roll = self.random.as_mut().unwrap().next_int(4.0, 6.0) as f64;
                Ok(n >= roll)
            }
            DIFFICULTY_HARD | DIFFICULTY_IMPOSSIBLE => {
                let nearby = self.pl_nearby(tr, pl); // 245
                let bordering: Vec<usize> = nearby
                    .into_iter()
                    .filter(|&n| {
                        self.nb_is_player(tr, n) // 257
                            && self.pl_type(tr, n) != PLAYER_TYPE_BOT // 251
                    })
                    .collect();
                let friends: Vec<usize> = bordering
                    .iter()
                    .copied()
                    .filter(|&o| self.pl_is_friendly(tr, pl, Some(o))) // 242
                    .collect();
                if bordering.len() >= 2 && bordering.contains(&other) {
                    return Ok((bordering.len() as f64) <= (friends.len() as f64) + 1.0);
                }
                let n = self.pl_alliances(tr, pl).len() as f64; // 954
                if difficulty == DIFFICULTY_HARD {
                    let roll = self.random.as_mut().unwrap().next_int(3.0, 5.0) as f64;
                    Ok(n >= roll)
                } else {
                    let roll = self.random.as_mut().unwrap().next_int(2.0, 4.0) as f64;
                    Ok(n >= roll)
                }
            }
            other => Err(format!("Unexpected value: {other}")),
        }
    }

    /// `isAlliancePartnerFriendly(otherPlayer)`.
    fn ne_is_alliance_partner_friendly(
        &mut self,
        tr: &mut Vec<f64>,
        other: usize,
    ) -> Result<bool, String> {
        let pl = self.ne_player.ok_or("Player not initialized")?;
        let difficulty = self.mg_difficulty(tr); // 402
        match difficulty.as_str() {
            DIFFICULTY_EASY | DIFFICULTY_MEDIUM => {
                Ok(self.pl_relation(tr, pl, Some(other)) == RELATION_FRIENDLY) // 412
            }
            DIFFICULTY_HARD => {
                if self.pl_relation(tr, pl, Some(other)) != RELATION_FRIENDLY {
                    // 412
                    return Ok(false);
                }
                Ok(self.random.as_mut().unwrap().next_int(0.0, 100.0) >= 17)
            }
            DIFFICULTY_IMPOSSIBLE => {
                if self.pl_relation(tr, pl, Some(other)) != RELATION_FRIENDLY {
                    // 412
                    return Ok(false);
                }
                Ok(self.random.as_mut().unwrap().next_int(0.0, 100.0) >= 33)
            }
            other => Err(format!("Unexpected value: {other}")),
        }
    }

    /// `isAlliancePartnerSimilarlyStrong(otherPlayer)`.
    fn ne_is_alliance_partner_similarly_strong(
        &mut self,
        tr: &mut Vec<f64>,
        other: usize,
    ) -> Result<bool, String> {
        let pl = self.ne_player.ok_or("Player not initialized")?;
        let difficulty = self.mg_difficulty(tr); // 402
        let (troop_lo, troop_hi, tile_lo, tile_hi) = match difficulty.as_str() {
            DIFFICULTY_EASY => (60.0, 70.0, 70.0, 80.0),
            DIFFICULTY_MEDIUM => (70.0, 80.0, 80.0, 90.0),
            DIFFICULTY_HARD => (75.0, 85.0, 85.0, 95.0),
            DIFFICULTY_IMPOSSIBLE => (80.0, 90.0, 90.0, 100.0),
            other => return Err(format!("Unexpected value: {other}")),
        };

        let mut player_outgoing = 0.0;
        for a in self.pl_outgoing_attacks(tr, pl) {
            // 721
            player_outgoing += self.at_troops(tr, a); // 830
        }
        let mut other_outgoing = 0.0;
        for a in self.pl_outgoing_attacks(tr, other) {
            // 721
            other_outgoing += self.at_troops(tr, a); // 830
        }
        let player_total = self.pl_troops(tr, pl) + player_outgoing; // 418
        let other_total = self.pl_troops(tr, other) + other_outgoing; // 418

        let troop_pct = self.random.as_mut().unwrap().next_int(troop_lo, troop_hi) as f64 / 100.0;
        let troop_threshold = player_total * troop_pct;
        let tile_pct = self.random.as_mut().unwrap().next_int(tile_lo, tile_hi) as f64 / 100.0;
        let tile_threshold = self.pl_num_tiles_owned(tr, pl) * tile_pct; // 532

        // Both `const` booleans are eagerly evaluated before the `||`.
        let has_comparable_troops = other_total > troop_threshold;
        let other_tiles = self.pl_num_tiles_owned(tr, other); // 532
        let has_comparable_tiles =
            other_tiles > tile_threshold && other_total > player_total * 0.5;
        Ok(has_comparable_troops || has_comparable_tiles)
    }

    /// `maybeBetray(otherPlayer, juiciestAlly, borderingFriends, borderingEnemies)`.
    fn ne_maybe_betray(
        &mut self,
        tr: &mut Vec<f64>,
        other: usize,
        juiciest_ally: Option<usize>,
        bordering_friends: &[usize],
        bordering_enemies: &[usize],
    ) -> Result<bool, String> {
        let pl = self.ne_player.ok_or("Player not initialized")?;
        if !self.pl_is_allied_with(tr, pl, other) {
            // 987
            return Ok(false);
        }
        let difficulty = self.mg_difficulty(tr); // 402

        if (difficulty == DIFFICULTY_HARD || difficulty == DIFFICULTY_IMPOSSIBLE)
            && juiciest_ally == Some(other)
            && self.ne_is_safe_to_betray(tr, other, bordering_friends, bordering_enemies)?
        {
            self.ne_betray(tr, other)?;
            return Ok(true);
        }

        if (difficulty == DIFFICULTY_EASY || difficulty == DIFFICULTY_MEDIUM)
            && !(difficulty == DIFFICULTY_EASY && self.pl_type(tr, other) == PLAYER_TYPE_HUMAN)
            && {
                let pt = self.pl_troops(tr, pl); // 418
                let ot = self.pl_troops(tr, other); // 418
                pt >= ot * 10.0
            }
        {
            self.ne_betray(tr, other)?;
            return Ok(true);
        }

        if difficulty != DIFFICULTY_EASY
            && self.pl_is_traitor(tr, other)
            && {
                // 826
                let ot = self.pl_troops(tr, other); // 418
                let pt = self.pl_troops(tr, pl); // 418
                ot < pt * 1.2
            }
        {
            self.ne_betray(tr, other)?;
            return Ok(true);
        }

        if difficulty != DIFFICULTY_EASY
            && bordering_friends.len() + bordering_enemies.len() == 1
            && {
                let ot = self.pl_troops(tr, other); // 418
                let pt = self.pl_troops(tr, pl); // 418
                ot * 3.0 < pt
            }
        {
            self.ne_betray(tr, other)?;
            return Ok(true);
        }

        Ok(false)
    }

    /// `findJuiciestAlly(borderingFriends)`.
    fn ne_find_juiciest_ally(
        &mut self,
        tr: &mut Vec<f64>,
        bordering_friends: &[usize],
    ) -> Result<Option<usize>, String> {
        let pl = self.ne_player.ok_or("Player not initialized")?;
        let candidates: Vec<usize> = bordering_friends
            .iter()
            .copied()
            .filter(|&f| self.pl_is_allied_with(tr, pl, f)) // 987
            .collect();
        Ok(self.ai_find_juiciest_target(tr, &candidates))
    }

    /// `isSafeToBetray(target, borderingFriends, borderingEnemies)`.
    fn ne_is_safe_to_betray(
        &mut self,
        tr: &mut Vec<f64>,
        target: usize,
        bordering_friends: &[usize],
        bordering_enemies: &[usize],
    ) -> Result<bool, String> {
        let pl = self.ne_player.ok_or("Player not initialized")?;
        let mut other_allies: Vec<usize> = Vec::new();
        if !self.pl_is_traitor(tr, target) {
            // 826
            for &f in bordering_friends {
                if f != target && self.pl_is_allied_with(tr, pl, f) {
                    // 987
                    other_allies.push(f);
                }
            }
        }
        let mut threats: Vec<usize> = vec![target];
        threats.extend_from_slice(bordering_enemies);
        threats.extend(other_allies.iter().copied());
        let mut nearby = 0.0;
        for &threat in &threats {
            let mut outgoing = 0.0;
            for a in self.pl_outgoing_attacks(tr, threat) {
                // 721
                outgoing += self.at_troops(tr, a); // 830
            }
            nearby += self.pl_troops(tr, threat) + outgoing; // 418
        }
        let pt = self.pl_troops(tr, pl); // 418
        Ok(nearby < pt * 0.33)
    }

    /// `betray(target)`.
    fn ne_betray(&mut self, tr: &mut Vec<f64>, target: usize) -> Result<(), String> {
        let pl = self.ne_player.ok_or("Player not initialized")?;
        if let Some(al) = self.pl_alliance_with(tr, pl, Some(target)) {
            // 243
            self.pl_break_alliance(tr, pl, al); // 244
        }
        Ok(())
    }

    /// `NationMIRVBehavior.considerMIRV()`. The boolean return value is
    /// discarded by `NationExecution.tick()`, so early exits just `return`.
    fn ne_consider_mirv(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        let pl = self.ne_player.ok_or("not initialized")?;
        if self.mg_is_unit_disabled(tr, UNIT_MIRV) {
            // 511
            return Ok(());
        }
        let silos = self.pl_units(tr, pl, &[UNIT_MISSILE_SILO.to_string()]); // 241
        if silos.is_empty() {
            return Ok(());
        }
        let cost = self.ne_cost(tr, UNIT_MIRV, pl); // 512 -> 969
        if self.pl_gold(tr, pl) < cost {
            // 417 (read after the cost pair, mirroring the TS `<` operand order)
            return Ok(());
        }
        let odds = self.ne_mirv_hesitation_odds(tr)?; // 402
        if self.random.as_mut().unwrap().chance(odds) {
            return Ok(());
        }
        let inbound = self.ne_select_counter_mirv_target(tr, pl)?;
        if let Some(t) = inbound {
            if !self.ne_was_recently_mirved(tr, t)? {
                self.ne_maybe_send_mirv(tr, t)?;
                return Ok(());
            }
        }
        let vd = self.ne_select_victory_denial_target(tr, pl)?;
        if let Some(t) = vd {
            if !self.ne_was_recently_mirved(tr, t)? {
                self.ne_maybe_send_mirv(tr, t)?;
                return Ok(());
            }
        }
        let sr = self.ne_select_steamroll_stop_target(tr, pl)?;
        if let Some(t) = sr {
            if !self.ne_was_recently_mirved(tr, t)? {
                self.ne_maybe_send_mirv(tr, t)?;
                return Ok(());
            }
        }
        Ok(())
    }

    /// `get hesitationOdds()`.
    fn ne_mirv_hesitation_odds(&mut self, tr: &mut Vec<f64>) -> Result<f64, String> {
        let difficulty = self.mg_difficulty(tr); // 402
        match difficulty.as_str() {
            DIFFICULTY_EASY => Ok(2.0),
            DIFFICULTY_MEDIUM => Ok(4.0),
            DIFFICULTY_HARD => Ok(8.0),
            DIFFICULTY_IMPOSSIBLE => Ok(16.0),
            other => Err(format!("Unexpected value: {other}")),
        }
    }

    /// `get victoryDenialThresholdPercent()`.
    fn ne_mirv_victory_denial_percent(&mut self, tr: &mut Vec<f64>) -> Result<f64, String> {
        let difficulty = self.mg_difficulty(tr); // 402
        match difficulty.as_str() {
            DIFFICULTY_EASY => Ok(75.0),
            DIFFICULTY_MEDIUM => Ok(65.0),
            DIFFICULTY_HARD => Ok(55.0),
            DIFFICULTY_IMPOSSIBLE => Ok(40.0),
            other => Err(format!("Unexpected value: {other}")),
        }
    }

    /// `get steamrollCityGapMultiplier()`.
    fn ne_mirv_steamroll_gap_multiplier(&mut self, tr: &mut Vec<f64>) -> Result<f64, String> {
        let difficulty = self.mg_difficulty(tr); // 402
        match difficulty.as_str() {
            DIFFICULTY_EASY => Ok(2.0),
            DIFFICULTY_MEDIUM => Ok(1.5),
            DIFFICULTY_HARD => Ok(1.25),
            DIFFICULTY_IMPOSSIBLE => Ok(1.15),
            other => Err(format!("Unexpected value: {other}")),
        }
    }

    /// `get steamrollMinLeaderCities()`.
    fn ne_mirv_steamroll_min_leader_cities(&mut self, tr: &mut Vec<f64>) -> Result<f64, String> {
        let difficulty = self.mg_difficulty(tr); // 402
        match difficulty.as_str() {
            DIFFICULTY_EASY => Ok(20.0),
            DIFFICULTY_MEDIUM => Ok(10.0),
            DIFFICULTY_HARD => Ok(10.0),
            DIFFICULTY_IMPOSSIBLE => Ok(8.0),
            other => Err(format!("Unexpected value: {other}")),
        }
    }

    /// `getValidMirvTargetPlayers()`. The `&&` chain short-circuits exactly as
    /// in TS: `p !== player` skips all facade reads for our own token.
    fn ne_get_valid_mirv_target_players(&mut self, tr: &mut Vec<f64>, pl: usize) -> Vec<usize> {
        let mut out = Vec::new();
        for p in self.mg_players(tr) {
            // 221
            if p == pl {
                continue;
            }
            if !self.pl_is_player(tr, p) {
                // 259
                continue;
            }
            if self.pl_type(tr, p) == PLAYER_TYPE_BOT {
                // 251
                continue;
            }
            if self.pl_is_on_same_team(tr, pl, Some(p)) {
                // 250
                continue;
            }
            out.push(p);
        }
        out
    }

    /// `selectCounterMirvTarget()`.
    fn ne_select_counter_mirv_target(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
    ) -> Result<Option<usize>, String> {
        let valid = self.ne_get_valid_mirv_target_players(tr, pl);
        let mut attackers = Vec::new();
        for p in valid {
            if self.ne_is_inbound_mirv_from(tr, pl, p)? {
                attackers.push(p);
            }
        }
        if attackers.is_empty() {
            return Ok(None);
        }
        let sorted = self.sort_players_by_tiles(tr, attackers);
        Ok(sorted.first().copied())
    }

    /// `isInboundMIRVFrom(attacker)`.
    fn ne_is_inbound_mirv_from(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
        attacker: usize,
    ) -> Result<bool, String> {
        let mirvs = self.pl_units(tr, attacker, &[UNIT_MIRV.to_string()]); // 241
        for m in mirvs {
            let dst = match self.un_target_tile(tr, m) {
                // 282
                Some(d) if js_truthy(d) => d,
                _ => continue,
            };
            if !self.mg_has_owner(tr, dst) {
                // 607
                continue;
            }
            let owner = self.mg_owner(tr, dst); // 222
            if owner == Some(pl) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// `selectVictoryDenialTarget()`.
    fn ne_select_victory_denial_target(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
    ) -> Result<Option<usize>, String> {
        let total_land = self.mg_num_land_tiles(tr); // 501
        if total_land == 0.0 {
            return Ok(None);
        }
        let percent = self.ne_mirv_victory_denial_percent(tr)?; // 402
        let scaled = total_land * percent;
        let valid = self.ne_get_valid_mirv_target_players(tr, pl);
        let mut best: Option<(usize, f64)> = None;
        for p in valid {
            let mut severity = 0.0;
            let team = self.pl_team(tr, p); // 248
            if !matches!(team, JsVal::Null) {
                let mut members = Vec::new();
                for x in self.mg_players(tr) {
                    // 221 (re-read per team candidate)
                    let xt = self.pl_team(tr, x); // 248
                    if !js_val_eq(&xt, &team) {
                        continue;
                    }
                    if !self.pl_is_player(tr, x) {
                        // 259
                        continue;
                    }
                    members.push(x);
                }
                let mut team_territory = 0.0;
                for m in &members {
                    team_territory += self.pl_num_tiles_owned(tr, *m); // 532
                }
                if team_territory * 100.0 >= scaled {
                    let mut largest: Option<usize> = None;
                    let mut largest_tiles = -1.0;
                    for m in &members {
                        let tiles = self.pl_num_tiles_owned(tr, *m); // 532 (re-read)
                        if tiles > largest_tiles {
                            largest_tiles = tiles;
                            largest = Some(*m);
                        }
                    }
                    if largest == Some(p) {
                        severity = team_territory;
                    }
                }
            } else {
                let tiles = self.pl_num_tiles_owned(tr, p); // 532
                if tiles * 100.0 >= scaled {
                    severity = tiles;
                }
            }
            if severity > 0.0 {
                let take = match best {
                    None => true,
                    Some((_, bs)) => severity > bs,
                };
                if take {
                    best = Some((p, severity));
                }
            }
        }
        Ok(best.map(|(p, _)| p))
    }

    /// `selectSteamrollStopTarget()`. The `{p, cityCount}` pairs are built
    /// before the sort, so the comparator itself traces nothing.
    fn ne_select_steamroll_stop_target(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
    ) -> Result<Option<usize>, String> {
        let valid = self.ne_get_valid_mirv_target_players(tr, pl);
        if valid.is_empty() {
            return Ok(None);
        }
        let mut scored: Vec<(usize, f64)> = Vec::new();
        for p in self.mg_players(tr) {
            // 221
            if !self.pl_is_player(tr, p) {
                // 259
                continue;
            }
            let cc = self.pl_unit_count(tr, p, UNIT_CITY); // 823
            scored.push((p, cc));
        }
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        if scored.len() < 2 {
            return Ok(None);
        }
        let top = scored[0];
        let min_leader = self.ne_mirv_steamroll_min_leader_cities(tr)?; // 402
        if top.1 <= min_leader {
            return Ok(None);
        }
        let second = scored[1].1;
        let mult = self.ne_mirv_steamroll_gap_multiplier(tr)?; // 402
        if top.1 >= second * mult && valid.contains(&top.0) {
            return Ok(Some(top.0));
        }
        Ok(None)
    }

    /// `wasRecentlyMirved(target)` (static `recentMirvTargets` map).
    fn ne_was_recently_mirved(&mut self, tr: &mut Vec<f64>, target: usize) -> Result<bool, String> {
        let id = self.pl_id(tr, target); // 231
        let last = match self.ne_recent_mirv.iter().find(|(k, _)| *k == id) {
            Some((_, v)) => *v,
            None => return Ok(false),
        };
        Ok(self.mg_ticks(tr) - last < MIRV_COOLDOWN_TICKS) // 212
    }

    /// `recordMirvHit(target)`.
    fn ne_record_mirv_hit(&mut self, tr: &mut Vec<f64>, target: usize) {
        let id = self.pl_id(tr, target); // 231
        let now = self.mg_ticks(tr); // 212
        if let Some(e) = self.ne_recent_mirv.iter_mut().find(|(k, _)| *k == id) {
            e.1 = now;
        } else {
            self.ne_recent_mirv.push((id, now));
        }
    }

    /// `maybeSendMIRV(enemy)`.
    fn ne_maybe_send_mirv(&mut self, tr: &mut Vec<f64>, enemy: usize) -> Result<(), String> {
        let pl = self.ne_player.ok_or("not initialized")?;
        self.ne_maybe_send_attack_emoji(tr, enemy)?;
        let center = self.ne_calculate_territory_center(tr, enemy)?;
        if let Some(c) = center {
            let spawn = self.pl_can_build(tr, pl, UNIT_MIRV, c); // 534
            if Self::js_truthy(&spawn) {
                self.mg_add_mirv(tr, pl, c);
                self.ne_record_mirv_hit(tr, enemy);
                self.ne_send_emoji(tr, Rec::All, "EMOJI_NUKE")?;
                self.ne_respond_to_mirv(tr, enemy)?;
            }
        }
        Ok(())
    }

    /// `Util.calculateTerritoryCenter(game, target)` (inlined).
    fn ne_calculate_territory_center(
        &mut self,
        tr: &mut Vec<f64>,
        target: usize,
    ) -> Result<Option<f64>, String> {
        let border = self.pl_border_tiles(tr, target); // 635
        if border.is_empty() {
            return Ok(None);
        }
        let (mut min_x, mut max_x) = (f64::INFINITY, f64::NEG_INFINITY);
        let (mut min_y, mut max_y) = (f64::INFINITY, f64::NEG_INFINITY);
        for &t in &border {
            let x = self.mg_x(tr, t); // 603
            let y = self.mg_y(tr, t); // 604
            if x < min_x {
                min_x = x;
            }
            if x > max_x {
                max_x = x;
            }
            if y < min_y {
                min_y = y;
            }
            if y > max_y {
                max_y = y;
            }
        }
        let center_x = ((min_x + max_x) / 2.0).floor();
        let center_y = ((min_y + max_y) / 2.0).floor();
        let center_tile = self.mg_ref(tr, center_x, center_y); // 609
        if self.mg_owner(tr, center_tile) == Some(target) {
            // 222
            return Ok(Some(center_tile));
        }
        let mut closest: Option<f64> = None;
        let mut closest_d2 = f64::INFINITY;
        for &t in &border {
            let dx = self.mg_x(tr, t) - center_x; // 603 (re-read)
            let dy = self.mg_y(tr, t) - center_y; // 604 (re-read)
            let d2 = dx * dx + dy * dy;
            if d2 < closest_d2 {
                closest_d2 = d2;
                closest = Some(t);
            }
        }
        Ok(closest)
    }

    // ---- NationStructureBehavior (ported inline) ----------------------------

    /// `spacingConstants()` — `nukeMagnitudes(AtomBomb)` (224) returns
    /// `(inner, outer)`; `borderSpacing = outer`, `structureSpacing = outer * 2`.
    fn ne_spacing_constants(&mut self, tr: &mut Vec<f64>) -> (f64, f64) {
        let (_, outer) = self.mg_nuke_magnitudes(tr, UNIT_ATOM_BOMB); // 224
        (outer, outer * 2.0)
    }

    /// Integer-keyed membership set for the `nearestTileDistCapped` ring walk
    /// (JS `Set<number>` over integer tile refs).
    fn ne_tile_set(v: &[f64]) -> std::collections::HashSet<i64> {
        v.iter().map(|&t| t as i64).collect()
    }

    /// `Util.nearestTileDist(game, tiles, tile)` — linear scan, 509 per element.
    fn ne_nearest_tile_dist(&mut self, tr: &mut Vec<f64>, tiles: &[f64], tile: f64) -> f64 {
        let mut best = f64::INFINITY;
        for &t in tiles {
            let d = self.mg_manhattan_dist(tr, t, tile); // 509
            if d < best {
                best = d;
            }
        }
        best
    }

    /// `Util.nearestTileDistCapped(game, borderTiles, tile, cap)` — the
    /// set-like branch: `size === 0 → Infinity`, `has(tile) → 0`, then
    /// `width()` (610), `height()` (611), `x(tile)` (603), `y(tile)` (604) and
    /// a Manhattan-ring walk over a local membership set.
    fn ne_nearest_tile_dist_capped(
        &mut self,
        tr: &mut Vec<f64>,
        border_set: &std::collections::HashSet<i64>,
        border_len: usize,
        tile: f64,
        cap: f64,
    ) -> f64 {
        if border_len == 0 {
            return f64::INFINITY;
        }
        if border_set.contains(&(tile as i64)) {
            return 0.0;
        }
        let w = self.mg_width(tr); // 610
        let h = self.mg_height(tr); // 611
        let cx = self.mg_x(tr, tile); // 603
        let cy = self.mg_y(tr, tile); // 604
        let (w_i, h_i) = (w as i64, h as i64);
        let (cx_i, cy_i) = (cx as i64, cy as i64);
        let cap_i = cap as i64;
        for d in 1..=cap_i {
            for dx in -d..=d {
                let x = cx_i + dx;
                if x < 0 || x >= w_i {
                    continue;
                }
                let dy = d - dx.abs();
                let y1 = cy_i - dy;
                if y1 >= 0 && border_set.contains(&(y1 * w_i + x)) {
                    return d as f64;
                }
                if dy != 0 {
                    let y2 = cy_i + dy;
                    if y2 < h_i && border_set.contains(&(y2 * w_i + x)) {
                        return d as f64;
                    }
                }
            }
        }
        f64::INFINITY
    }

    /// `new Set(units.map((u) => u.tile()))` — 344 per unit, first-insert order.
    fn ne_unit_tiles_set(&mut self, tr: &mut Vec<f64>, units: &[usize]) -> Vec<f64> {
        let mut out: Vec<f64> = Vec::new();
        for &u in units {
            let t = self.un_tile(tr, u); // 344
            if !out.contains(&t) {
                out.push(t);
            }
        }
        out
    }

    /// The same set with `otherTiles.delete(tile)` applied.
    fn ne_unit_tiles_set_minus(
        &mut self,
        tr: &mut Vec<f64>,
        units: &[usize],
        minus: f64,
    ) -> Vec<f64> {
        let mut out = self.ne_unit_tiles_set(tr, units);
        if let Some(pos) = out.iter().position(|&x| x == minus) {
            out.remove(pos);
        }
        out
    }

    /// `handleStructures()`. The boolean result is discarded by `tick()`.
    fn ne_handle_structures(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        let pl = self.ne_player.ok_or("not initialized")?;
        self.ne_handle_structures_inner(tr, pl)?;
        Ok(())
    }

    fn ne_handle_structures_inner(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
    ) -> Result<bool, String> {
        if self.ne_placements_count > 0.0 && !self.mg_is_unit_disabled(tr, UNIT_DEFENSE_POST)
        // 511
        {
            if self.ne_try_build_defense_post(tr, pl)? {
                return Ok(true);
            }
            if self.ne_defense_post_needed(tr, pl)? {
                return Ok(false);
            }
        }
        if self.ne_is_on_structure_cooldown(tr, pl)? {
            return Ok(false);
        }
        if self.ne_is_in_post_save_up_blocked_phase(tr, pl)? {
            return Ok(false);
        }
        let built = self.ne_do_handle_structures(tr, pl)?;
        if built {
            self.ne_last_structure_tick = Some(self.mg_ticks(tr)); // 212
            self.ne_placements_count += 1.0;
        }
        Ok(built)
    }

    /// `tryBuildDefensePost()`.
    fn ne_try_build_defense_post(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
    ) -> Result<bool, String> {
        let diff = self.mg_difficulty(tr); // 402
        if diff == DIFFICULTY_EASY {
            return Ok(false);
        }
        if diff == DIFFICULTY_MEDIUM && !self.random.as_mut().unwrap().chance(2.0) {
            return Ok(false);
        }
        let attacks = self.pl_incoming_attacks(tr, pl); // 822
        let mut land: Vec<usize> = Vec::new();
        for a in attacks {
            if self.at_source_tile(tr, a).is_none() {
                // 841
                land.push(a);
            }
        }
        if land.is_empty() {
            return Ok(false);
        }
        let our = self.pl_troops(tr, pl); // 418
        if our <= 0.0 {
            return Ok(false);
        }
        let mut incoming = 0.0;
        for a in &land {
            incoming += self.at_troops(tr, *a); // 830
        }
        let ratio = incoming / our;
        if ratio < 0.35 {
            return Ok(false);
        }
        let allowed = if diff == DIFFICULTY_MEDIUM {
            1.0
        } else {
            (ratio / 0.4).ceil()
        };
        let front = self.ne_get_attack_front_tiles(tr, pl, &land)?;
        let count = self.ne_count_defense_posts_near_front(tr, pl, &front, allowed)?;
        if count >= allowed {
            return Ok(false);
        }
        let cost = self.ne_cost(tr, UNIT_DEFENSE_POST, pl); // 512 -> 969
        if self.pl_gold(tr, pl) < cost {
            // 417
            return Ok(false);
        }
        let tiles = self.ne_sample_tiles_near_front(tr, pl, &front, 25.0)?;
        for t in tiles {
            let can = self.pl_can_build(tr, pl, UNIT_DEFENSE_POST, t); // 534
            if !Self::js_truthy(&can) {
                continue;
            }
            self.mg_add_construction_exec(tr, pl, UNIT_DEFENSE_POST, t);
            return Ok(true);
        }
        Ok(false)
    }

    /// `defensePostNeeded()`.
    fn ne_defense_post_needed(&mut self, tr: &mut Vec<f64>, pl: usize) -> Result<bool, String> {
        let diff = self.mg_difficulty(tr); // 402
        if diff == DIFFICULTY_EASY {
            return Ok(false);
        }
        let attacks = self.pl_incoming_attacks(tr, pl); // 822
        let mut land: Vec<usize> = Vec::new();
        for a in attacks {
            if self.at_source_tile(tr, a).is_none() {
                // 841
                land.push(a);
            }
        }
        if land.is_empty() {
            return Ok(false);
        }
        let our = self.pl_troops(tr, pl); // 418
        if our <= 0.0 {
            return Ok(false);
        }
        let mut incoming = 0.0;
        for a in &land {
            incoming += self.at_troops(tr, *a); // 830
        }
        Ok(incoming / our >= 0.35)
    }

    /// `getAttackFrontTiles(landAttacks)` — 842 per attack, then the border
    /// scan (635 -> 804 -> 222 per border tile).
    fn ne_get_attack_front_tiles(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
        land: &[usize],
    ) -> Result<Vec<f64>, String> {
        let mut attackers: Vec<usize> = Vec::new();
        for a in land {
            if let Some(p) = self.at_attacker(tr, *a) {
                // 842
                if !attackers.contains(&p) {
                    attackers.push(p);
                }
            }
        }
        if attackers.is_empty() {
            return Ok(Vec::new());
        }
        let border = self.pl_border_tiles(tr, pl); // 635
        let mut front: Vec<f64> = Vec::new();
        for &bt in &border {
            let nbrs = self.mg_neighbors4(tr, bt); // 804
            for &n in &nbrs {
                if let Some(o) = self.mg_owner(tr, n) {
                    // 222
                    if attackers.contains(&o) {
                        front.push(bt);
                        break;
                    }
                }
            }
        }
        Ok(front)
    }

    /// `countDefensePostsNearFront(frontTiles, cap)`.
    fn ne_count_defense_posts_near_front(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
        front: &[f64],
        cap: f64,
    ) -> Result<f64, String> {
        if front.is_empty() {
            return Ok(0.0);
        }
        let (bs, _) = self.ne_spacing_constants(tr); // 224
        let range_sq = (bs * 1.5) * (bs * 1.5);
        let dps = self.pl_units(tr, pl, &[UNIT_DEFENSE_POST.to_string()]); // 241
        let mut count = 0.0;
        for dp in dps {
            for &ft in front {
                let dt = self.un_tile(tr, dp); // 344 (re-read per pair, like TS)
                let d = self.mg_euclidean_dist_squared(tr, dt, ft); // 710
                if d <= range_sq {
                    count += 1.0;
                    if count >= cap {
                        return Ok(count);
                    }
                    break;
                }
            }
        }
        Ok(count)
    }

    /// `sampleTilesNearFront(frontTiles, count, DefensePost)`.
    fn ne_sample_tiles_near_front(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
        front: &[f64],
        count: f64,
    ) -> Result<Vec<f64>, String> {
        if front.is_empty() {
            return Ok(Vec::new());
        }
        let (bs, _) = self.ne_spacing_constants(tr); // 224
        let search_radius = (bs * 1.5).ceil();
        let min_border = (bs * 0.75).ceil();
        let max_border = (bs * 1.5).ceil();
        let border = self.pl_border_tiles(tr, pl); // 635
        let border_set = Self::ne_tile_set(&border);
        let spread_sq = (bs * 1.5) * (bs * 1.5);
        let dp_units = self.pl_units(tr, pl, &[UNIT_DEFENSE_POST.to_string()]); // 241
        let dp_tiles = self.ne_unit_tiles_set(tr, &dp_units); // 344 each

        let anchors: Vec<f64> = if !dp_tiles.is_empty() {
            let mut a: Vec<f64> = Vec::new();
            for &ft in front {
                let mut near = false;
                for &dp in &dp_tiles {
                    if self.mg_euclidean_dist_squared(tr, ft, dp) < spread_sq {
                        // 710
                        near = true;
                        break;
                    }
                }
                if !near {
                    a.push(ft);
                }
            }
            if a.is_empty() {
                front.to_vec()
            } else {
                a
            }
        } else {
            front.to_vec()
        };

        let count_i = count as usize;
        let mut result: Vec<f64> = Vec::new();
        let mut attempt = 0.0;
        while attempt < count * 6.0 && result.len() < count_i {
            attempt += 1.0;
            let anchor = match self.random.as_mut().unwrap().rand_element(&anchors) {
                Some(a) => *a,
                None => break,
            };
            let ax = self.mg_x(tr, anchor); // 603
            let ay = self.mg_y(tr, anchor); // 604
            let x = self
                .random
                .as_mut()
                .unwrap()
                .next_int(ax - search_radius, ax + search_radius + 1.0) as f64;
            let y = self
                .random
                .as_mut()
                .unwrap()
                .next_int(ay - search_radius, ay + search_radius + 1.0) as f64;
            if !self.mg_is_valid_coord(tr, x, y) {
                // 616
                continue;
            }
            let t = self.mg_ref(tr, x, y); // 609
            if self.mg_owner(tr, t) != Some(pl) {
                // 222
                continue;
            }
            let bd = self.ne_nearest_tile_dist_capped(tr, &border_set, border.len(), t, max_border);
            if bd < min_border || bd > max_border {
                continue;
            }
            let can = self.pl_can_build(tr, pl, UNIT_DEFENSE_POST, t); // 534
            if !Self::js_truthy(&can) {
                continue;
            }
            result.push(t);
        }
        if !result.is_empty() {
            return Ok(result);
        }

        let mut fallback: Vec<f64> = Vec::new();
        let mut attempt = 0.0;
        while attempt < count * 4.0 && fallback.len() < count_i {
            attempt += 1.0;
            let anchor = match self.random.as_mut().unwrap().rand_element(&anchors) {
                Some(a) => *a,
                None => break,
            };
            let ax = self.mg_x(tr, anchor); // 603
            let ay = self.mg_y(tr, anchor); // 604
            let x = self
                .random
                .as_mut()
                .unwrap()
                .next_int(ax - search_radius, ax + search_radius + 1.0) as f64;
            let y = self
                .random
                .as_mut()
                .unwrap()
                .next_int(ay - search_radius, ay + search_radius + 1.0) as f64;
            if !self.mg_is_valid_coord(tr, x, y) {
                // 616
                continue;
            }
            let t = self.mg_ref(tr, x, y); // 609
            if self.mg_owner(tr, t) != Some(pl) {
                // 222
                continue;
            }
            fallback.push(t);
        }
        Ok(fallback)
    }

    /// `isOnStructureCooldown()`.
    fn ne_is_on_structure_cooldown(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
    ) -> Result<bool, String> {
        let last = match self.ne_last_structure_tick {
            None => return Ok(false),
            Some(x) => x,
        };
        if !self.ne_has_high_starting_gold(tr, pl)? {
            return Ok(false);
        }
        let gaps = [0.0, 0.0, 250.0, 150.0, 100.0];
        let required = gaps
            .get(self.ne_placements_count as usize)
            .copied()
            .unwrap_or(0.0);
        if required == 0.0 {
            return Ok(false);
        }
        Ok(self.mg_ticks(tr) - last < required) // 212
    }

    /// `hasHighStartingGold()` — cached after the first `info()` (984) +
    /// `startingGold()` (970) pair.
    fn ne_has_high_starting_gold(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
    ) -> Result<bool, String> {
        if let Some(b) = self.ne_has_high_starting_gold {
            return Ok(b);
        }
        let info = self.pl_info(tr, pl); // 984
        let gold = self.mg_starting_gold(tr, &info); // 970
        let b = gold >= 3_000_000;
        self.ne_has_high_starting_gold = Some(b);
        Ok(b)
    }

    /// `isInPostSaveUpBlockedPhase()`.
    fn ne_is_in_post_save_up_blocked_phase(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
    ) -> Result<bool, String> {
        if self.mg_is_unit_disabled(tr, UNIT_MISSILE_SILO) {
            // 511
            return Ok(false);
        }
        let target = self.ne_get_save_up_target(tr, pl)?;
        let start = match self.ne_post_save_up_start_tick {
            None => {
                if self.pl_gold(tr, pl) < target {
                    // 417
                    return Ok(false);
                }
                let t = self.mg_ticks(tr); // 212
                self.ne_post_save_up_start_tick = Some(t);
                t
            }
            Some(x) => x,
        };
        let elapsed = self.mg_ticks(tr) - start; // 212
        Ok(elapsed % 300.0 >= 150.0)
    }

    /// `getSaveUpTarget()` — gold as the i64 side.
    fn ne_get_save_up_target(&mut self, tr: &mut Vec<f64>, pl: usize) -> Result<i64, String> {
        if self.mg_is_unit_disabled(tr, UNIT_MISSILE_SILO) {
            // 511
            return Ok(self.ne_cost(tr, UNIT_SAM_LAUNCHER, pl)); // 512 -> 969
        }
        if self.mg_game_mode(tr) == GAME_MODE_TEAM {
            // 520
            return Ok(self.ne_cost(tr, UNIT_HYDROGEN_BOMB, pl));
        }
        let mirv_enabled = !self.mg_is_unit_disabled(tr, UNIT_MIRV); // 511
        let hydro_enabled = !self.mg_is_unit_disabled(tr, UNIT_HYDROGEN_BOMB); // 511
        let atom_enabled = !self.mg_is_unit_disabled(tr, UNIT_ATOM_BOMB); // 511
        if mirv_enabled {
            let mirv = self.ne_cost(tr, UNIT_MIRV, pl);
            let hydro = self.ne_cost(tr, UNIT_HYDROGEN_BOMB, pl);
            return Ok(mirv + hydro);
        }
        if hydro_enabled {
            return Ok(self.ne_cost(tr, UNIT_HYDROGEN_BOMB, pl) * 5);
        }
        if atom_enabled {
            return Ok(self.ne_cost(tr, UNIT_ATOM_BOMB, pl) * 20);
        }
        Ok(self.ne_cost(tr, UNIT_SAM_LAUNCHER, pl))
    }

    /// `doHandleStructures()`.
    fn ne_do_handle_structures(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
    ) -> Result<bool, String> {
        self.ne_reachable_stations_cache = None;
        let cities_disabled = self.mg_is_unit_disabled(tr, UNIT_CITY); // 511
        let city_count = if cities_disabled {
            let n = self.pl_num_tiles_owned(tr, pl); // 532
            (n / 2000.0).floor().max(1.0)
        } else {
            self.pl_units_owned(tr, pl, UNIT_CITY) // 983
        };
        let shared = self.mg_shared_water_components(tr, pl); // 977
        self.ne_shared_water_components = shared.clone();
        let has_coastal = shared.is_some();
        let silo_enabled = !self.mg_is_unit_disabled(tr, UNIT_MISSILE_SILO); // 511
        let diff = self.mg_difficulty(tr); // 402

        if self.ne_placements_count == 0.0
            && (diff == DIFFICULTY_HARD || diff == DIFFICULTY_IMPOSSIBLE)
            && !self.mg_is_unit_disabled(tr, UNIT_ATOM_BOMB) // 511
            && silo_enabled
            && !self.mg_is_unit_disabled(tr, UNIT_SAM_LAUNCHER) // 511
            && self.ne_has_high_starting_gold(tr, pl)?
            && self.ne_maybe_spawn_structure(tr, pl, UNIT_SAM_LAUNCHER)?
        {
            return Ok(true);
        }

        if !cities_disabled && !self.ne_built_crowded && self.ne_is_high_nation_density(tr)? {
            let preferred = if has_coastal && !self.mg_is_unit_disabled(tr, UNIT_PORT) {
                // 511
                UNIT_PORT
            } else {
                UNIT_FACTORY
            };
            if self.mg_is_unit_disabled(tr, preferred) {
                // 511
                self.ne_built_crowded = true;
            } else if self.ne_maybe_spawn_structure(tr, pl, preferred)? {
                self.ne_built_crowded = true;
                return Ok(true);
            }
        }

        let build_order = [UNIT_PORT, UNIT_FACTORY, UNIT_SAM_LAUNCHER, UNIT_MISSILE_SILO];
        let nukes_enabled = !self.mg_is_unit_disabled(tr, UNIT_ATOM_BOMB)
            || !self.mg_is_unit_disabled(tr, UNIT_HYDROGEN_BOMB)
            || !self.mg_is_unit_disabled(tr, UNIT_MIRV);
        // 511 chain
        for st in build_order {
            if self.mg_is_unit_disabled(tr, st) {
                continue;
            }
            if st == UNIT_PORT && !has_coastal {
                continue;
            }
            if !nukes_enabled && (st == UNIT_MISSILE_SILO || st == UNIT_SAM_LAUNCHER) {
                continue;
            }
            if !silo_enabled && st == UNIT_SAM_LAUNCHER {
                continue;
            }
            if self.ne_should_build_structure(tr, pl, st, city_count, has_coastal)?
                && self.ne_maybe_spawn_structure(tr, pl, st)?
            {
                return Ok(true);
            }
        }

        if !cities_disabled && self.ne_maybe_spawn_structure(tr, pl, UNIT_CITY)? {
            return Ok(true);
        }
        Ok(false)
    }

    /// `isHighNationDensity()` — 501 then 971.
    fn ne_is_high_nation_density(&mut self, tr: &mut Vec<f64>) -> Result<bool, String> {
        let land = self.mg_num_land_tiles(tr); // 501
        if land <= 0.0 {
            return Ok(false);
        }
        let nations = self.mg_nations(tr); // 971
        Ok(nations.len() as f64 / land > 1.0 / 7500.0)
    }

    /// `shouldBuildStructure(type, cityCount, hasCoastalTiles)`.
    fn ne_should_build_structure(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
        ty: &str,
        city_count: f64,
        has_coastal: bool,
    ) -> Result<bool, String> {
        let diff = self.mg_difficulty(tr); // 402
        let mut ratio = match (ty, diff.as_str()) {
            (UNIT_PORT, _) => 0.75,
            (UNIT_FACTORY, _) => 0.75,
            (UNIT_SAM_LAUNCHER, DIFFICULTY_EASY) => 0.15,
            (UNIT_SAM_LAUNCHER, DIFFICULTY_MEDIUM) => 0.2,
            (UNIT_SAM_LAUNCHER, DIFFICULTY_HARD) => 0.25,
            (UNIT_SAM_LAUNCHER, DIFFICULTY_IMPOSSIBLE) => 0.3,
            (UNIT_MISSILE_SILO, _) => 0.2,
            // `getStructureRatios` has no entry for City / DefensePost.
            _ => return Ok(false),
        };
        if ty == UNIT_FACTORY
            && has_coastal
            && !self.mg_is_unit_disabled(tr, UNIT_PORT)
        // 511
        {
            ratio *= 0.33;
        }
        let owned = self.pl_units_owned(tr, pl, ty); // 983
        if ty == UNIT_MISSILE_SILO && owned >= 3.0 {
            return Ok(false);
        }
        if ty == UNIT_MISSILE_SILO && owned == 0.0 {
            ratio = 0.4;
        }
        Ok(owned < (city_count * ratio).floor())
    }

    /// `maybeSpawnStructure(type)`.
    fn ne_maybe_spawn_structure(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
        ty: &str,
    ) -> Result<bool, String> {
        let perceived = self.ne_get_perceived_cost(tr, pl, ty)?;
        if self.pl_gold(tr, pl) < perceived {
            // 417
            return Ok(false);
        }
        let structures = self.pl_units(tr, pl, &[ty.to_string()]); // 241
        if self.ne_get_total_structure_density(tr, pl)? > 1.0 / 1500.0 {
            let info = self.mg_config_unit_info(tr, ty); // 513
            let upgradable = val_field(&info, "upgradable").map(as_bool).unwrap_or(false);
            if upgradable {
                if self.ne_maybe_upgrade_structure(tr, pl, &structures)? {
                    return Ok(true);
                }
                if !structures.is_empty() {
                    return Ok(false);
                }
            }
        }
        let tile = match self.ne_structure_spawn_tile(tr, pl, ty)? {
            Some(t) => t,
            None => return Ok(false),
        };
        let can = self.pl_can_build(tr, pl, ty, tile); // 534
        if matches!(can, JsVal::Bool(false)) {
            return Ok(false);
        }
        self.mg_add_construction_exec(tr, pl, ty, tile);
        Ok(true)
    }

    /// `getPerceivedCost(type)`.
    fn ne_get_perceived_cost(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
        ty: &str,
    ) -> Result<i64, String> {
        let real_cost = self.ne_cost(tr, ty, pl); // 512 -> 969
        let save_up_target = self.ne_get_save_up_target(tr, pl)?;
        if save_up_target == 0 || self.pl_gold(tr, pl) >= save_up_target {
            // 417
            return Ok(real_cost);
        }
        let owned = self.pl_units_owned(tr, pl, ty); // 983
        let increase = if ty == UNIT_CITY {
            1.0
        } else {
            let diff = self.mg_difficulty(tr); // 402
            match (ty, diff.as_str()) {
                (UNIT_PORT, _) => 1.0,
                (UNIT_FACTORY, _) => 1.0,
                (UNIT_SAM_LAUNCHER, _) => 0.3,
                (UNIT_MISSILE_SILO, _) => 1.0,
                _ => 0.1,
            }
        };
        let multiplier = 1.0 + increase * owned;
        Ok(((real_cost as f64) * multiplier).ceil() as i64)
    }

    /// `getTotalStructureDensity()` — 532 then 241 (all six structure types).
    fn ne_get_total_structure_density(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
    ) -> Result<f64, String> {
        let tiles = self.pl_num_tiles_owned(tr, pl); // 532
        if tiles > 0.0 {
            let types: Vec<String> =
                STRUCTURES_TYPES.iter().map(|s| s.to_string()).collect();
            let all = self.pl_units(tr, pl, &types); // 241
            Ok(all.len() as f64 / tiles)
        } else {
            Ok(0.0)
        }
    }

    /// `maybeUpgradeStructure(structures)`.
    fn ne_maybe_upgrade_structure(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
        structures: &[usize],
    ) -> Result<bool, String> {
        if self.ne_get_total_structure_density(tr, pl)? <= 1.0 / 1500.0 {
            return Ok(false);
        }
        if structures.is_empty() {
            return Ok(false);
        }
        let best = self.ne_find_best_structure_to_upgrade(tr, pl, structures)?;
        if let Some(u) = best {
            let id = self.un_id(tr, u); // 340
            self.mg_add_upgrade_structure_exec(tr, pl, id);
            return Ok(true);
        }
        Ok(false)
    }

    /// `findBestStructureToUpgrade(structures)`.
    fn ne_find_best_structure_to_upgrade(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
        structures: &[usize],
    ) -> Result<Option<usize>, String> {
        if structures.is_empty() {
            return Ok(None);
        }
        let mut upgradable: Vec<usize> = Vec::new();
        for &s in structures {
            if self.pl_can_upgrade_unit(tr, pl, s) {
                // 326
                upgradable.push(s);
            }
        }
        if upgradable.is_empty() {
            return Ok(None);
        }
        let diff = self.mg_difficulty(tr); // 402
        let random_chance = match diff.as_str() {
            DIFFICULTY_EASY => 70.0,
            DIFFICULTY_MEDIUM => 40.0,
            DIFFICULTY_HARD => 25.0,
            DIFFICULTY_IMPOSSIBLE => 10.0,
            other => return Err(format!("Unexpected value: {other}")),
        };
        if (self.random.as_mut().unwrap().next_int(0.0, 100.0) as f64) < random_chance {
            return Ok(self.random.as_mut().unwrap().rand_element(&upgradable).copied());
        }
        let sams = self.pl_units(tr, pl, &[UNIT_SAM_LAUNCHER.to_string()]); // 241
        let mut scored: Vec<(usize, f64)> = Vec::new();
        for &s in &upgradable {
            let mut score = 0.0;
            for &sam in &sams {
                let lv = self.un_level(tr, sam); // 540
                let r = self.mg_sam_range(tr, lv); // 974
                let r_sq = r * r;
                let st = self.un_tile(tr, s); // 344
                let mt = self.un_tile(tr, sam); // 344
                let d = self.mg_euclidean_dist_squared(tr, st, mt); // 710
                if d <= r_sq {
                    score += 10.0;
                    // TS 808 + 809 are two separate sam.level() reads: the
                    // `> 1` guard and the bonus multiplier each consume one
                    // 540 event from the unit slot FIFO.
                    let lv2 = self.un_level(tr, sam); // 540 (line 808)
                    if lv2 > 1.0 {
                        let lv3 = self.un_level(tr, sam); // 540 (line 809)
                        score += (lv3 - 1.0) * 7.5;
                    }
                }
            }
            score += self.random.as_mut().unwrap().next_int(0.0, 5.0) as f64;
            scored.push((s, score));
        }
        if scored.is_empty() {
            return Ok(None);
        }
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        if scored.len() >= 2 && self.random.as_mut().unwrap().chance(2.0) {
            let pick = if scored.len() >= 3 {
                self.random.as_mut().unwrap().next_int(1.0, 3.0) as usize
            } else {
                1
            };
            return Ok(Some(scored[pick].0));
        }
        Ok(Some(scored[0].0))
    }

    /// `structureSpawnTile(type)`.
    fn ne_structure_spawn_tile(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
        ty: &str,
    ) -> Result<Option<f64>, String> {
        let tiles = if ty == UNIT_PORT {
            self.ne_rand_coastal_tile_array(tr, pl, 25.0)?
        } else {
            self.ne_rand_territory_tile_array(tr, pl, 25.0)?
        };
        if tiles.is_empty() {
            return Ok(None);
        }
        let ctx = match self.ne_structure_spawn_tile_value(tr, pl, ty)? {
            Some(c) => c,
            None => return Ok(None),
        };
        let mut best: Option<f64> = None;
        let mut best_value = 0.0f64;
        for &t in &tiles {
            let v = self.ne_value_at(tr, &ctx, t);
            if v <= best_value && best.is_some() {
                continue;
            }
            let can = self.pl_can_build(tr, pl, ty, t); // 534
            if !Self::js_truthy(&can) {
                continue;
            }
            best = Some(t);
            best_value = v;
        }
        Ok(best)
    }

    /// `structureSpawnTileValue(type)` — runs the closure's outer precomputation
    /// traces and returns the captured context (None for value-less types).
    fn ne_structure_spawn_tile_value(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
        ty: &str,
    ) -> Result<Option<NeSpawnValueCtx>, String> {
        match ty {
            UNIT_MISSILE_SILO => {
                let border = self.pl_border_tiles(tr, pl); // 635
                let other = self.pl_units(tr, pl, &[UNIT_MISSILE_SILO.to_string()]); // 241
                let (bs, ss) = self.ne_spacing_constants(tr); // 224
                Ok(Some(NeSpawnValueCtx {
                    kind: 1,
                    border_set: Self::ne_tile_set(&border),
                    border_tiles: border,
                    border_spacing: bs,
                    structure_spacing: ss,
                    other_units: other,
                    ..Default::default()
                }))
            }
            UNIT_PORT => {
                let other = self.pl_units(tr, pl, &[UNIT_PORT.to_string()]); // 241
                Ok(Some(NeSpawnValueCtx {
                    kind: 3,
                    other_units: other,
                    ..Default::default()
                }))
            }
            UNIT_FACTORY => {
                let border = self.pl_border_tiles(tr, pl); // 635
                let other = self.pl_units(tr, pl, &[UNIT_FACTORY.to_string()]); // 241
                let (bs, ss) = self.ne_spacing_constants(tr); // 224
                let station_range = self.mg_train_station_max_range(tr); // 309
                let d = self.mg_difficulty(tr); // 402
                let use_conn = self.ne_should_use_connectivity_score(tr, &d)?;
                let reachable = if use_conn {
                    self.ne_get_or_build_reachable_stations(tr, pl)?
                } else {
                    Vec::new()
                };
                let min_r = self.mg_train_station_min_range(tr); // 973
                let city_units = self.pl_units(tr, pl, &[UNIT_CITY.to_string()]); // 241
                let cross_tiles = self.ne_unit_tiles_set(tr, &city_units); // 344 each
                Ok(Some(NeSpawnValueCtx {
                    kind: 2,
                    border_set: Self::ne_tile_set(&border),
                    border_tiles: border,
                    border_spacing: bs,
                    structure_spacing: ss,
                    other_units: other,
                    station_range,
                    station_range_sq: station_range * station_range,
                    use_conn,
                    reachable,
                    min_range_sq: min_r * min_r,
                    cross_tiles,
                    ..Default::default()
                }))
            }
            UNIT_CITY => {
                let border = self.pl_border_tiles(tr, pl); // 635
                let other = self.pl_units(tr, pl, &[UNIT_CITY.to_string()]); // 241
                let (bs, ss) = self.ne_spacing_constants(tr); // 224
                let station_range = self.mg_train_station_max_range(tr); // 309
                let d = self.mg_difficulty(tr); // 402
                let use_conn = self.ne_should_use_connectivity_score(tr, &d)?;
                let reachable = if use_conn {
                    self.ne_get_or_build_reachable_stations(tr, pl)?
                } else {
                    Vec::new()
                };
                let min_r = self.mg_train_station_min_range(tr); // 973
                let factory_units = self.pl_units(tr, pl, &[UNIT_FACTORY.to_string()]); // 241
                let cross_tiles = self.ne_unit_tiles_set(tr, &factory_units); // 344 each
                Ok(Some(NeSpawnValueCtx {
                    kind: 0,
                    border_set: Self::ne_tile_set(&border),
                    border_tiles: border,
                    border_spacing: bs,
                    structure_spacing: ss,
                    other_units: other,
                    station_range,
                    station_range_sq: station_range * station_range,
                    use_conn,
                    reachable,
                    min_range_sq: min_r * min_r,
                    cross_tiles,
                    ..Default::default()
                }))
            }
            UNIT_SAM_LAUNCHER => {
                let border = self.pl_border_tiles(tr, pl); // 635
                let other = self.pl_units(tr, pl, &[UNIT_SAM_LAUNCHER.to_string()]); // 241
                let (bs, ss) = self.ne_spacing_constants(tr); // 224
                let d = self.mg_difficulty(tr); // 402
                let weight_by_level = d == DIFFICULTY_HARD || d == DIFFICULTY_IMPOSSIBLE;
                let all = self.pl_units_all(tr, pl); // 241 no-arg
                let mut protect: Vec<(f64, f64)> = Vec::new();
                for u in all {
                    let ut = self.un_type(tr, u); // 283
                    if ut == UNIT_CITY
                        || ut == UNIT_FACTORY
                        || ut == UNIT_MISSILE_SILO
                        || ut == UNIT_PORT
                    {
                        let t = self.un_tile(tr, u); // 344
                        let w = if weight_by_level {
                            self.un_level(tr, u) // 540
                        } else {
                            1.0
                        };
                        protect.push((t, w));
                    }
                }
                let range = self.mg_default_sam_range(tr); // 975
                let not_easy = d != DIFFICULTY_EASY;
                let use_coverage =
                    not_easy && (self.random.as_mut().unwrap().next_int(0.0, 100.0) as f64) < 25.0;
                let coverage: Vec<(f64, f64)> = if use_coverage {
                    let sams = self.pl_units(tr, pl, &[UNIT_SAM_LAUNCHER.to_string()]); // 241
                    let mut cov: Vec<(f64, f64)> = Vec::new();
                    for &(pt, _) in &protect {
                        let mut score = 0.0;
                        for &sam in &sams {
                            let lv = self.un_level(tr, sam); // 540
                            let r = self.mg_sam_range(tr, lv); // 974
                            let st = self.un_tile(tr, sam); // 344
                            let d2 = self.mg_euclidean_dist_squared(tr, pt, st); // 710
                            if d2 <= r * r {
                                score += self.un_level(tr, sam); // 540
                            }
                        }
                        cov.push((pt, score));
                    }
                    cov
                } else {
                    Vec::new()
                };
                Ok(Some(NeSpawnValueCtx {
                    kind: 4,
                    border_set: Self::ne_tile_set(&border),
                    border_tiles: border,
                    border_spacing: bs,
                    structure_spacing: ss,
                    other_units: other,
                    protect,
                    range_sq: range * range,
                    use_coverage,
                    coverage,
                    not_easy,
                    ..Default::default()
                }))
            }
            _ => Ok(None),
        }
    }

    /// The per-tile body of the selected `structureSpawnTileValue` closure.
    fn ne_value_at(&mut self, tr: &mut Vec<f64>, ctx: &NeSpawnValueCtx, tile: f64) -> f64 {
        let mut w = 0.0f64;
        match ctx.kind {
            3 => {
                // portValue
                let others = self.ne_unit_tiles_set_minus(tr, &ctx.other_units, tile);
                w += self.ne_nearest_tile_dist(tr, &others, tile);
            }
            1 => {
                // missileSiloValue
                w += self.mg_magnitude(tr, tile); // 980
                let bd = self.ne_nearest_tile_dist_capped(
                    tr,
                    &ctx.border_set,
                    ctx.border_tiles.len(),
                    tile,
                    ctx.border_spacing,
                );
                w += bd.min(ctx.border_spacing);
                let others = self.ne_unit_tiles_set_minus(tr, &ctx.other_units, tile);
                let d = self.ne_nearest_tile_dist(tr, &others, tile);
                if d != f64::INFINITY {
                    w += d.min(ctx.structure_spacing);
                }
            }
            2 | 0 => {
                // factoryValue / cityValue
                w += self.mg_magnitude(tr, tile); // 980
                let bd = self.ne_nearest_tile_dist_capped(
                    tr,
                    &ctx.border_set,
                    ctx.border_tiles.len(),
                    tile,
                    ctx.border_spacing,
                );
                w += bd.min(ctx.border_spacing);
                let others = self.ne_unit_tiles_set_minus(tr, &ctx.other_units, tile);
                let d = self.ne_nearest_tile_dist(tr, &others, tile);
                if d != f64::INFINITY {
                    let cap = if ctx.kind == 2 {
                        ctx.station_range
                    } else {
                        ctx.structure_spacing
                    };
                    w += d.min(cap);
                }
                let d2 = self.ne_nearest_tile_dist(tr, &ctx.cross_tiles, tile);
                if d2 != f64::INFINITY {
                    w += d2.min(ctx.structure_spacing);
                }
                if !ctx.use_conn {
                    return w;
                }
                w += self.ne_compute_connectivity_score(
                    tr,
                    tile,
                    &ctx.reachable,
                    ctx.min_range_sq,
                    ctx.station_range_sq,
                ) * ctx.structure_spacing;
            }
            _ => {
                // samLauncherValue
                w += self.mg_magnitude(tr, tile); // 980
                let bd = self.ne_nearest_tile_dist_capped(
                    tr,
                    &ctx.border_set,
                    ctx.border_tiles.len(),
                    tile,
                    ctx.border_spacing,
                );
                if !ctx.border_tiles.is_empty() {
                    w += bd.min(ctx.border_spacing);
                }
                let others = self.ne_unit_tiles_set_minus(tr, &ctx.other_units, tile);
                let d = self.ne_nearest_tile_dist(tr, &others, tile);
                if d != f64::INFINITY {
                    w += d.min(ctx.structure_spacing);
                }
                if ctx.not_easy {
                    for &(pt, pw) in &ctx.protect {
                        let dsq = self.mg_euclidean_dist_squared(tr, tile, pt); // 710
                        if dsq > ctx.range_sq {
                            continue;
                        }
                        if ctx.use_coverage {
                            let cov = ctx
                                .coverage
                                .iter()
                                .find(|(t, _)| *t == pt)
                                .map(|(_, c)| *c)
                                .unwrap_or(0.0);
                            w += ctx.structure_spacing * pw * (1.0 / (1.0 + cov));
                        } else {
                            w += ctx.structure_spacing * pw;
                        }
                    }
                }
            }
        }
        w
    }

    /// `shouldUseConnectivityScore(difficulty)` — always consumes one draw.
    fn ne_should_use_connectivity_score(
        &mut self,
        _tr: &mut Vec<f64>,
        diff: &str,
    ) -> Result<bool, String> {
        let random_chance = match diff {
            DIFFICULTY_EASY => 0.0,
            DIFFICULTY_MEDIUM => 60.0,
            DIFFICULTY_HARD => 75.0,
            DIFFICULTY_IMPOSSIBLE => 100.0,
            other => return Err(format!("Unexpected value: {other}")),
        };
        Ok((self.random.as_mut().unwrap().next_int(0.0, 100.0) as f64) < random_chance)
    }

    /// `getOrBuildReachableStations()` — the instance-level cache.
    fn ne_get_or_build_reachable_stations(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
    ) -> Result<Vec<(f64, Option<usize>, f64)>, String> {
        if self.ne_reachable_stations_cache.is_none() {
            let built = self.ne_build_reachable_stations(tr, pl)?;
            self.ne_reachable_stations_cache = Some(built);
        }
        Ok(self.ne_reachable_stations_cache.clone().unwrap())
    }

    /// `buildReachableStations()`.
    fn ne_build_reachable_stations(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
    ) -> Result<Vec<(f64, Option<usize>, f64)>, String> {
        let stations = self.mg_station_manager_get_all(tr); // 979
        let mut unit_to_cluster: Vec<(usize, Option<usize>)> = Vec::new();
        for s in &stations {
            let c = self.st_get_cluster(tr, *s); // 920
            unit_to_cluster.push((*s, c));
        }
        let max_trade_gold = self.mg_train_gold(tr, "ally", 0.0, pl).max(1.0); // 976
        let self_weight = self.mg_train_gold(tr, "self", 0.0, pl) / max_trade_gold; // 976
        let mut result: Vec<(f64, Option<usize>, f64)> = Vec::new();

        let own_types = [UNIT_CITY, UNIT_PORT, UNIT_FACTORY]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        let own = self.pl_units(tr, pl, &own_types); // 241 (three args)
        for u in own {
            if let Some((_, c)) = unit_to_cluster.iter().find(|(k, _)| *k == u) {
                let t = self.un_tile(tr, u); // 344
                result.push((t, *c, self_weight));
            }
        }

        let neighbors = self.pl_nearby(tr, pl); // 245
        for nb in neighbors {
            if !self.pl_is_player(tr, nb) {
                // 259
                continue;
            }
            if self.pl_type(tr, nb) == PLAYER_TYPE_BOT {
                // 251
                continue;
            }
            if !self.pl_can_trade(tr, pl, Some(nb)) {
                // 533
                continue;
            }
            let rel = if self.pl_is_on_same_team(tr, pl, Some(nb)) {
                // 250
                "team"
            } else if self.pl_is_allied_with(tr, pl, nb) {
                // 987
                "ally"
            } else {
                "other"
            };
            let weight = self.mg_train_gold(tr, rel, 0.0, pl) / max_trade_gold; // 976
            let units = self.pl_units(tr, nb, &own_types); // 241
            for u in units {
                if let Some((_, c)) = unit_to_cluster.iter().find(|(k, _)| *k == u) {
                    let t = self.un_tile(tr, u); // 344
                    result.push((t, *c, weight));
                }
            }
        }
        Ok(result)
    }

    /// `computeConnectivityScore(tile, reachableStations, min, max)`.
    fn ne_compute_connectivity_score(
        &mut self,
        tr: &mut Vec<f64>,
        tile: f64,
        reachable: &[(f64, Option<usize>, f64)],
        min_sq: f64,
        max_sq: f64,
    ) -> f64 {
        let mut clusters: Vec<(usize, f64)> = Vec::new();
        let mut isolated = 0.0f64;
        for &(st_tile, cluster, weight) in reachable {
            let d = self.mg_euclidean_dist_squared(tr, tile, st_tile); // 710
            if d < min_sq || d > max_sq {
                continue;
            }
            match cluster {
                Some(c) => match clusters.iter_mut().find(|(k, _)| *k == c) {
                    Some(e) => {
                        if weight > e.1 {
                            e.1 = weight;
                        }
                    }
                    None => clusters.push((c, weight)),
                },
                None => isolated += weight,
            }
        }
        let mut score = isolated;
        for (_, cw) in &clusters {
            score += cw;
        }
        score
    }

    /// `randCoastalTileArray(numTiles)` — shore filter over the border set
    /// (635 -> 967 -> 507 -> 508 -> 981 -> 301 -> 402 -> 978) then sampling.
    fn ne_rand_coastal_tile_array(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
        num_tiles: f64,
    ) -> Result<Vec<f64>, String> {
        let shared = self.ne_shared_water_components.clone();
        let border = self.pl_border_tiles(tr, pl); // 635
        let mut tiles: Vec<f64> = Vec::new();
        for &t in &border {
            if !self.mg_is_shore(tr, t) {
                // 967
                continue;
            }
            let shared = match &shared {
                None => continue,
                Some(s) => s,
            };
            let nbrs = self.mg_neighbors(tr, t); // 507
            let mut ok = false;
            for &n in &nbrs {
                if !self.mg_is_water(tr, n) {
                    // 508
                    continue;
                }
                if self.mg_is_ocean(tr, n) {
                    // 981
                    ok = true;
                    break;
                }
                let comp = self.mg_get_water_component(tr, n); // 301
                let comp_num = match &comp {
                    JsVal::Num(x) => *x,
                    _ => continue,
                };
                if !shared.contains(&comp_num) {
                    continue;
                }
                let diff = self.mg_difficulty(tr); // 402
                if diff != DIFFICULTY_EASY {
                    let size = self.mg_get_water_component_size(tr, n); // 978
                    if let Some(s) = size {
                        if s < 3000.0 {
                            continue;
                        }
                    }
                }
                ok = true;
                break;
            }
            if ok {
                tiles.push(t);
            }
        }
        Ok(self.ne_array_sampler(tr, tiles, num_tiles as usize))
    }

    /// `arraySampler(a, sampleSize)` — all elements in order when short, else
    /// `randFromSet` draws (one `nextInt(0, size)` each) over a deduped
    /// insertion-ordered pool with removals.
    fn ne_array_sampler(&mut self, _tr: &mut Vec<f64>, a: Vec<f64>, sample_size: usize) -> Vec<f64> {
        if a.len() <= sample_size {
            return a;
        }
        let mut remaining: Vec<f64> = Vec::new();
        for t in a {
            if !remaining.contains(&t) {
                remaining.push(t);
            }
        }
        let mut out: Vec<f64> = Vec::new();
        for _ in 0..sample_size {
            let idx = self
                .random
                .as_mut()
                .unwrap()
                .next_int(0.0, remaining.len() as f64) as usize;
            let t = remaining.remove(idx);
            out.push(t);
        }
        out
    }

    /// `randTerritoryTileArray(random, mg, player, numTiles)`.
    fn ne_rand_territory_tile_array(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
        num_tiles: f64,
    ) -> Result<Vec<f64>, String> {
        let border = self.pl_border_tiles(tr, pl); // 635
        let (mut min_x, mut max_x) = (f64::INFINITY, f64::NEG_INFINITY);
        let (mut min_y, mut max_y) = (f64::INFINITY, f64::NEG_INFINITY);
        for &t in &border {
            let x = self.mg_x(tr, t); // 603
            let y = self.mg_y(tr, t); // 604
            if x < min_x {
                min_x = x;
            }
            if y < min_y {
                min_y = y;
            }
            if x > max_x {
                max_x = x;
            }
            if y > max_y {
                max_y = y;
            }
        }
        let mut out: Vec<f64> = Vec::new();
        for _ in 0..num_tiles as i64 {
            if let Some(t) = self.ne_rand_territory_tile(tr, pl, min_x, min_y, max_x, max_y)? {
                out.push(t);
            }
        }
        Ok(out)
    }

    /// `randTerritoryTile(random, mg, p, boundingBox)`.
    fn ne_rand_territory_tile(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
        min_x: f64,
        min_y: f64,
        max_x: f64,
        max_y: f64,
    ) -> Result<Option<f64>, String> {
        for _ in 0..100 {
            let rx = self
                .random
                .as_mut()
                .unwrap()
                .next_int(min_x, max_x) as f64;
            let ry = self
                .random
                .as_mut()
                .unwrap()
                .next_int(min_y, max_y) as f64;
            if !self.mg_is_on_map(tr, rx, ry) {
                // 972
                continue;
            }
            let t = self.mg_ref(tr, rx, ry); // 609
            if self.mg_owner(tr, t) == Some(pl) {
                // 222
                return Ok(Some(t));
            }
        }
        let n1 = self.pl_num_tiles_owned(tr, pl); // 532
        if n1 > 0.0 {
            let n2 = self.pl_num_tiles_owned(tr, pl); // 532
            if n2 <= 100.0 {
                let tiles = self.pl_tiles(tr, pl); // 634
                if tiles.is_empty() {
                    return Ok(None);
                }
                let idx = self
                    .random
                    .as_mut()
                    .unwrap()
                    .next_int(0.0, tiles.len() as f64) as usize;
                return Ok(Some(tiles[idx]));
            }
        }
        Ok(None)
    }

    // ---- NationWarshipBehavior (ported inline) ------------------------------

    /// `maybeSpawnWarship()`. The boolean return value is discarded by
    /// `tick()`, so early exits just `return`.
    fn ne_maybe_spawn_warship(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        let pl = self.ne_player.ok_or("not initialized")?;
        if self.mg_is_unit_disabled(tr, UNIT_WARSHIP) {
            // 511
            return Ok(());
        }
        if !self.random.as_mut().unwrap().chance(50.0) {
            return Ok(());
        }
        let ports = self.pl_units(tr, pl, &[UNIT_PORT.to_string()]); // 241
        let ships = self.pl_units(tr, pl, &[UNIT_WARSHIP.to_string()]); // 241
        if ports.is_empty() || !ships.is_empty() {
            return Ok(());
        }
        let gold = self.pl_gold(tr, pl); // 417 (read before the cost pair)
        let cost = self.ne_cost(tr, UNIT_WARSHIP, pl); // 512 -> 969
        if gold <= cost {
            return Ok(());
        }
        let port = match self.random.as_mut().unwrap().rand_element(&ports) {
            Some(p) => *p,
            None => return Ok(()),
        };
        let port_tile = self.un_tile(tr, port); // 344
        let target_tile = self.ne_warship_spawn_tile(tr, port_tile, 250.0);
        let target = match target_tile {
            Some(t) => t,
            None => return Ok(()),
        };
        let can_build = self.pl_can_build(tr, pl, UNIT_WARSHIP, target); // 534
        if matches!(can_build, JsVal::Bool(false)) {
            return Ok(());
        }
        self.mg_add_construction_exec(tr, pl, UNIT_WARSHIP, target);
        Ok(())
    }

    /// `warshipSpawnTile(portTile, radius)`. Each attempt reads `game.x` /
    /// `game.y` twice (both `nextInt` bounds evaluate `this.game.x(portTile)`).
    fn ne_warship_spawn_tile(
        &mut self,
        tr: &mut Vec<f64>,
        port_tile: f64,
        radius: f64,
    ) -> Option<f64> {
        for _ in 0..50 {
            let x_lo = self.mg_x(tr, port_tile) - radius; // 603
            let x_hi = self.mg_x(tr, port_tile) + radius; // 603 (re-read)
            let rand_x = self.random.as_mut().unwrap().next_int(x_lo, x_hi) as f64;
            let y_lo = self.mg_y(tr, port_tile) - radius; // 604
            let y_hi = self.mg_y(tr, port_tile) + radius; // 604 (re-read)
            let rand_y = self.random.as_mut().unwrap().next_int(y_lo, y_hi) as f64;
            if !self.mg_is_valid_coord(tr, rand_x, rand_y) {
                // 616
                continue;
            }
            let tile = self.mg_ref(tr, rand_x, rand_y); // 609
            if !self.mg_is_water(tr, tile) {
                // 508
                continue;
            }
            return Some(tile);
        }
        None
    }

    /// `trackShipsAndRetaliate()`.
    fn ne_track_ships_and_retaliate(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        self.ne_track_transport_ships_and_retaliate(tr)?;
        self.ne_track_trade_ships_and_retaliate(tr)?;
        self.ne_track_incoming_transports_and_retaliate(tr)
    }

    /// `trackTransportShipsAndRetaliate()`. The tracked sets keep JS `Set`
    /// insertion order and dedupe on the unit token.
    fn ne_track_transport_ships_and_retaliate(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        let pl = self.ne_player.ok_or("not initialized")?;
        if self.mg_is_unit_disabled(tr, UNIT_TRANSPORT_SHIP) {
            // 511
            return Ok(());
        }
        for u in self.pl_units(tr, pl, &[UNIT_TRANSPORT_SHIP.to_string()]) {
            // 241
            if !self.ne_tracked_transport.contains(&u) {
                self.ne_tracked_transport.push(u);
            }
        }
        let mut i = 0usize;
        while i < self.ne_tracked_transport.len() {
            let ship = self.ne_tracked_transport[i];
            if self.un_is_active(tr, ship) {
                // 280
                i += 1;
                continue;
            }
            if self.un_was_destroyed_by_enemy(tr, ship) {
                // 985
                if self.un_destroyer(tr, ship).is_some() {
                    // 986
                    let tile = self.un_tile(tr, ship); // 344
                    let destroyer = self.un_destroyer(tr, ship); // 986 (re-read)
                    if let Some(d) = destroyer {
                        self.ne_maybe_retaliate_with_warship(tr, tile, d, false)?;
                    }
                }
            }
            self.ne_tracked_transport.remove(i);
        }
        Ok(())
    }

    /// `trackTradeShipsAndRetaliate()`.
    fn ne_track_trade_ships_and_retaliate(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        let pl = self.ne_player.ok_or("not initialized")?;
        for u in self.pl_units(tr, pl, &[UNIT_TRADE_SHIP.to_string()]) {
            // 241
            if !self.ne_tracked_trade.contains(&u) {
                self.ne_tracked_trade.push(u);
            }
        }
        let mut i = 0usize;
        while i < self.ne_tracked_trade.len() {
            let ship = self.ne_tracked_trade[i];
            if !self.un_is_active(tr, ship) {
                // 280
                self.ne_tracked_trade.remove(i);
                continue;
            }
            let owner = self.un_owner(tr, ship); // 286
            let oid = match owner {
                Some(o) => self.pl_id(tr, o), // 231
                None => {
                    return Err("Cannot read properties of undefined (reading 'id')".into())
                }
            };
            let pid = self.pl_id(tr, pl); // 231
            if oid != pid {
                let tile = self.un_tile(tr, ship); // 344
                let owner2 = self.un_owner(tr, ship); // 286 (re-read)
                if let Some(o) = owner2 {
                    self.ne_maybe_retaliate_with_warship(tr, tile, o, true)?;
                }
                self.ne_tracked_trade.remove(i);
                continue;
            }
            i += 1;
        }
        Ok(())
    }

    /// `trackIncomingTransportsAndRetaliate()`.
    fn ne_track_incoming_transports_and_retaliate(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        let pl = self.ne_player.ok_or("not initialized")?;
        for p in self.mg_units_typed(tr, &[UNIT_TRANSPORT_SHIP.to_string()]) {
            // 968
            let target = match self.un_target_tile(tr, p) {
                // 282
                Some(t) if js_truthy(t) => t,
                _ => continue,
            };
            if !self.un_is_active(tr, p) {
                // 280
                continue;
            }
            let st = self.un_transport_ship_state(tr, p); // 850
            if val_field(&st, "isRetreating").map(as_bool).unwrap_or(false) {
                continue;
            }
            let oid = self.mg_owner_id(tr, target); // 223
            let sid = self.pl_small_id(tr, pl); // 230
            if oid != sid {
                continue;
            }
            let owner = match self.un_owner(tr, p) {
                // 286
                Some(o) => o,
                None => {
                    return Err("Cannot read properties of undefined (reading 'smallID')".into())
                }
            };
            let osid = self.pl_small_id(tr, owner); // 230
            let psid = self.pl_small_id(tr, pl); // 230 (re-read)
            if osid == psid {
                continue;
            }
            if !self.ne_tracked_incoming.contains(&p) {
                self.ne_tracked_incoming.push(p);
            }
        }

        let mut i = 0usize;
        while i < self.ne_tracked_incoming.len() {
            let transport = self.ne_tracked_incoming[i];
            let target = self.un_target_tile(tr, transport); // 282
            let mut finished = !self.un_is_active(tr, transport); // 280
            if !finished {
                match target {
                    None => finished = true,
                    Some(t) => {
                        let st = self.un_transport_ship_state(tr, transport); // 850
                        let _ = t;
                        finished = val_field(&st, "isRetreating")
                            .map(as_bool)
                            .unwrap_or(false);
                    }
                }
            }
            if finished {
                self.ne_tracked_incoming.remove(i);
                self.ne_dealt_with.retain(|x| *x != transport);
                continue;
            }
            if self.ne_dealt_with.contains(&transport) {
                i += 1;
                continue;
            }
            let t = target.unwrap();
            let tt = self.un_tile(tr, transport); // 344
            let distance_to_target = self.mg_manhattan_dist(tr, tt, t); // 509
            if distance_to_target < 20.0 {
                self.ne_dealt_with.push(transport);
                i += 1;
                continue;
            }
            let owner = match self.un_owner(tr, transport) {
                // 286
                Some(o) => o,
                None => {
                    return Err("Cannot read properties of undefined (reading 'isAlliedWith')".into())
                }
            };
            if self.pl_is_allied_with(tr, owner, pl) {
                // 987
                i += 1;
                continue;
            }
            let pid = self.pl_id(tr, pl); // 231
            let has_nearby = self.mg_has_unit_nearby_pid(tr, t, 90.0, UNIT_WARSHIP, &pid); // 305
            let mut patrolling = false;
            if !has_nearby {
                let warships = self.pl_units(tr, pl, &[UNIT_WARSHIP.to_string()]); // 241
                for w in warships {
                    let ws = self.un_warship_state(tr, w); // 928
                    let patrol_tile = match val_field(&ws, "patrolTile") {
                        Some(JsVal::Num(p)) => *p,
                        _ => continue,
                    };
                    if self.mg_manhattan_dist(tr, t, patrol_tile) < 90.0 {
                        // 509
                        patrolling = true;
                        break;
                    }
                }
            }
            if has_nearby || patrolling {
                self.ne_dealt_with.push(transport);
                i += 1;
                continue;
            }
            let ocean = self.ne_warship_spawn_tile(tr, t, 30.0);
            match ocean {
                None => {
                    i += 1;
                    continue;
                }
                Some(o) => {
                    let owner2 = self.un_owner(tr, transport); // 286 (re-read)
                    if let Some(o2) = owner2 {
                        self.ne_maybe_retaliate_with_warship(tr, o, o2, false)?;
                    }
                    self.ne_dealt_with.push(transport);
                    // `break` — leave the iteration loop entirely.
                    break;
                }
            }
        }
        Ok(())
    }

    /// `maybeRetaliateWithWarship(tile, enemy, reason)`. `trade` selects the
    /// relation malus (`-7.5` for `"trade"`, `-15` for `"transport"`).
    fn ne_maybe_retaliate_with_warship(
        &mut self,
        tr: &mut Vec<f64>,
        tile: f64,
        enemy: usize,
        trade: bool,
    ) -> Result<(), String> {
        let pl = self.ne_player.ok_or("not initialized")?;
        if enemy == pl {
            return Ok(());
        }
        let ships = self.pl_units(tr, pl, &[UNIT_WARSHIP.to_string()]); // 241
        if ships.len() >= 10 {
            self.ne_maybe_move_warship(tr, tile)?;
            return Ok(());
        }
        // Only the matching difficulty arm evaluates its `nextInt`.
        let difficulty = self.mg_difficulty(tr); // 402
        let roll = match difficulty.as_str() {
            DIFFICULTY_MEDIUM => self.random.as_mut().unwrap().next_int(0.0, 100.0) < 15,
            DIFFICULTY_HARD => self.random.as_mut().unwrap().next_int(0.0, 100.0) < 50,
            DIFFICULTY_IMPOSSIBLE => self.random.as_mut().unwrap().next_int(0.0, 100.0) < 80,
            _ => false,
        };
        if roll {
            let can_build = self.pl_can_build(tr, pl, UNIT_WARSHIP, tile); // 534
            if matches!(can_build, JsVal::Bool(false)) {
                self.ne_maybe_move_warship(tr, tile)?;
                return Ok(());
            }
            self.mg_add_construction_exec(tr, pl, UNIT_WARSHIP, tile);
            self.ne_maybe_send_emoji(tr, Rec::P(enemy), "EMOJI_WARSHIP_RETALIATION")?;
            let malus = if trade { -7.5 } else { -15.0 };
            self.pl_update_relation(tr, pl, Some(enemy), malus); // 237
        }
        Ok(())
    }

    /// `maybeMoveWarship(tile)`.
    fn ne_maybe_move_warship(&mut self, tr: &mut Vec<f64>, tile: f64) -> Result<(), String> {
        let pl = self.ne_player.ok_or("not initialized")?;
        if !self.mg_is_water(tr, tile) {
            // 508
            return Ok(());
        }
        let warships = self.pl_units(tr, pl, &[UNIT_WARSHIP.to_string()]); // 241
        let mut filtered = Vec::new();
        for w in warships {
            let ws = self.un_warship_state(tr, w); // 928
            let patrol_tile = match val_field(&ws, "patrolTile") {
                Some(JsVal::Num(p)) => *p,
                _ => continue,
            };
            let wt = self.un_tile(tr, w); // 344
            if self.mg_manhattan_dist(tr, wt, patrol_tile) < 130.0 {
                // 509
                filtered.push(w);
            }
        }
        let sorted = self.ne_sort_warships_by_dist(tr, filtered, tile);
        if let Some(w) = sorted.first() {
            self.un_update_warship_state(tr, *w, tile); // 342
        }
        Ok(())
    }

    /// V8 binary insertion sort for the distance comparator
    /// `(a, b) => dist(a.tile(), tile) - dist(b.tile(), tile)`, invoked as
    /// `Compare(element, mid)` — the element side is read first.
    fn ne_sort_warships_by_dist(
        &mut self,
        tr: &mut Vec<f64>,
        mut arr: Vec<usize>,
        tile: f64,
    ) -> Vec<usize> {
        let n = arr.len();
        if n <= 1 {
            return arr;
        }
        for i in 1..n {
            let element = arr[i];
            let (mut lo, mut hi) = (0usize, i);
            while lo < hi {
                let mid = (lo + hi) / 2;
                let et = self.un_tile(tr, element); // 344
                let ed = self.mg_manhattan_dist(tr, et, tile); // 509
                let mt = self.un_tile(tr, arr[mid]); // 344
                let md = self.mg_manhattan_dist(tr, mt, tile); // 509
                if ed - md < 0.0 {
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

    /// `counterWarshipInfestation()`.
    fn ne_counter_warship_infestation(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        let pl = self.ne_player.ok_or("not initialized")?;
        if !self.ne_should_counter_warship_infestation(tr)? {
            return Ok(());
        }
        let team = self.pl_team(tr, pl); // 248
        let is_team_game = !matches!(team, JsVal::Null);
        if !self.ne_is_rich_player(tr, is_team_game)? {
            return Ok(());
        }
        let target = if is_team_game {
            self.ne_find_team_game_warship_target(tr)?
        } else {
            self.ne_find_ffa_warship_target(tr)?
        };
        if let Some((_player, warship)) = target {
            self.ne_build_counter_warship(tr, warship)?;
        }
        Ok(())
    }

    /// `shouldCounterWarshipInfestation()`.
    fn ne_should_counter_warship_infestation(&mut self, tr: &mut Vec<f64>) -> Result<bool, String> {
        let pl = self.ne_player.ok_or("not initialized")?;
        if self.mg_is_unit_disabled(tr, UNIT_WARSHIP) {
            // 511
            return Ok(false);
        }
        let difficulty = self.mg_difficulty(tr); // 402
        if difficulty != DIFFICULTY_HARD && difficulty != DIFFICULTY_IMPOSSIBLE {
            return Ok(false);
        }
        if self.mg_unit_count(tr, UNIT_WARSHIP) <= 10.0 {
            // 506
            return Ok(false);
        }
        let cost = self.ne_cost(tr, UNIT_WARSHIP, pl); // 512 -> 969
        if cost > self.pl_gold(tr, pl) {
            // 417 (cost is the left operand, so gold is read second)
            return Ok(false);
        }
        if self.pl_units(tr, pl, &[UNIT_PORT.to_string()]).is_empty() {
            // 241
            return Ok(false);
        }
        if self.pl_units(tr, pl, &[UNIT_WARSHIP.to_string()]).len() >= 10 {
            // 241
            return Ok(false);
        }
        Ok(true)
    }

    /// `isRichPlayer(isTeamGame)`.
    fn ne_is_rich_player(&mut self, tr: &mut Vec<f64>, is_team_game: bool) -> Result<bool, String> {
        let pl = self.ne_player.ok_or("not initialized")?;
        let mut players = Vec::new();
        for p in self.mg_players(tr) {
            // 221
            if self.pl_type(tr, p) == PLAYER_TYPE_HUMAN {
                // 251
                continue;
            }
            if is_team_game {
                let pt = self.pl_team(tr, p); // 248
                let mt = self.pl_team(tr, pl); // 248 (re-read per candidate)
                if !js_val_eq(&pt, &mt) {
                    continue;
                }
            }
            players.push(p);
        }
        let sorted = self.ne_sort_players_by_gold(tr, players);
        for p in sorted.iter().take(3) {
            let a = self.pl_id(tr, *p); // 231
            let b = self.pl_id(tr, pl); // 231 (re-read per candidate)
            if a == b {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// V8 binary insertion sort for `(a, b) => Number(b.gold() - a.gold())`
    /// (`BigInt` difference coerced by `Number`), invoked as
    /// `Compare(element, mid)` — the mid side is read first.
    fn ne_sort_players_by_gold(&mut self, tr: &mut Vec<f64>, mut arr: Vec<usize>) -> Vec<usize> {
        let n = arr.len();
        if n <= 1 {
            return arr;
        }
        for i in 1..n {
            let element = arr[i];
            let (mut lo, mut hi) = (0usize, i);
            while lo < hi {
                let mid = (lo + hi) / 2;
                let mg = self.pl_gold(tr, arr[mid]) as f64; // 417
                let eg = self.pl_gold(tr, element) as f64; // 417
                if mg - eg < 0.0 {
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

    /// `findTeamGameWarshipTarget()`. The `Map` keeps JS insertion order.
    fn ne_find_team_game_warship_target(
        &mut self,
        tr: &mut Vec<f64>,
    ) -> Result<Option<(usize, usize)>, String> {
        let pl = self.ne_player.ok_or("not initialized")?;
        let mut keys: Vec<String> = Vec::new();
        let mut counts: Vec<usize> = Vec::new();
        let mut members: Vec<Vec<usize>> = Vec::new();
        for p in self.mg_players(tr) {
            // 221
            let friendly = self.pl_is_friendly(tr, pl, Some(p)); // 242
            let mut skip = friendly;
            if !friendly {
                let a = self.pl_id(tr, p); // 231
                let b = self.pl_id(tr, pl); // 231
                skip = a == b;
            }
            if skip {
                continue;
            }
            let team = self.pl_team(tr, p); // 248
            if matches!(team, JsVal::Null) {
                continue;
            }
            let team_key = as_str(&team);
            let warship_count = self.pl_units(tr, p, &[UNIT_WARSHIP.to_string()]).len(); // 241
            let idx = match keys.iter().position(|k| *k == team_key) {
                Some(i) => i,
                None => {
                    keys.push(team_key);
                    counts.push(0);
                    members.push(Vec::new());
                    keys.len() - 1
                }
            };
            counts[idx] += warship_count;
            members[idx].push(p);
        }
        for idx in 0..counts.len() {
            if counts[idx] <= 15 {
                continue;
            }
            // `reduce` with a `null` seed: the element warship count is read
            // before the current max's.
            let mut best: Option<usize> = None;
            for p in members[idx].clone() {
                let count = self.pl_units(tr, p, &[UNIT_WARSHIP.to_string()]).len(); // 241
                let max_count = match best {
                    Some(m) => self.pl_units(tr, m, &[UNIT_WARSHIP.to_string()]).len(), // 241
                    None => 0,
                };
                if count > max_count {
                    best = Some(p);
                }
            }
            if let Some(m) = best {
                let warships = self.pl_units(tr, m, &[UNIT_WARSHIP.to_string()]); // 241 (re-read)
                if warships.len() > 3 {
                    if let Some(w) = self.random.as_mut().unwrap().rand_element(&warships) {
                        return Ok(Some((m, *w)));
                    }
                }
            }
        }
        Ok(None)
    }

    /// `findFreeForAllWarshipTarget()`.
    fn ne_find_ffa_warship_target(
        &mut self,
        tr: &mut Vec<f64>,
    ) -> Result<Option<(usize, usize)>, String> {
        let pl = self.ne_player.ok_or("not initialized")?;
        let mut enemies = Vec::new();
        for p in self.mg_players(tr) {
            // 221
            if self.pl_is_friendly(tr, pl, Some(p)) {
                // 242
                continue;
            }
            let a = self.pl_id(tr, p); // 231
            let b = self.pl_id(tr, pl); // 231
            if a == b {
                continue;
            }
            enemies.push(p);
        }
        for enemy in enemies {
            let enemy_warships = self.pl_units(tr, enemy, &[UNIT_WARSHIP.to_string()]); // 241
            if enemy_warships.len() > 10 {
                if let Some(w) = self.random.as_mut().unwrap().rand_element(&enemy_warships) {
                    return Ok(Some((enemy, *w)));
                }
            }
        }
        Ok(None)
    }

    /// `buildCounterWarship(target)` — only `target.warship` is read.
    fn ne_build_counter_warship(&mut self, tr: &mut Vec<f64>, warship: usize) -> Result<(), String> {
        let pl = self.ne_player.ok_or("not initialized")?;
        let wt = self.un_tile(tr, warship); // 344
        let can_build = self.pl_can_build(tr, pl, UNIT_WARSHIP, wt); // 534
        if matches!(can_build, JsVal::Bool(false)) {
            let wt2 = self.un_tile(tr, warship); // 344 (re-read)
            self.ne_maybe_move_warship(tr, wt2)?;
            return Ok(());
        }
        let wt3 = self.un_tile(tr, warship); // 344 (second read for the record)
        self.mg_add_construction_exec(tr, pl, UNIT_WARSHIP, wt3);
        self.ne_send_emoji(tr, Rec::All, "EMOJI_WARSHIP_RETALIATION")?;
        Ok(())
    }

    // ---- NationNukeBehavior (NationNukeBehavior.ts, G5m) -----------------------

    /// `boundingBoxTiles(gm, center, radius)` (Util.ts) — edge-only walk:
    /// top/bottom rows via `minY`/`maxY`, then the side columns strictly
    /// between them. Every coordinate read and `isValidCoord`/`ref` check is
    /// a traced facade read (603/604/616/609).
    fn nn_bounding_box_tiles(&mut self, tr: &mut Vec<f64>, center: f64, radius: f64) -> Vec<f64> {
        let center_x = self.mg_x(tr, center); // 603
        let center_y = self.mg_y(tr, center); // 604
        let (min_x, max_x) = (center_x - radius, center_x + radius);
        let (min_y, max_y) = (center_y - radius, center_y + radius);
        let mut tiles = Vec::new();
        let mut x = min_x;
        while x <= max_x {
            if self.mg_is_valid_coord(tr, x, min_y) {
                // 616 + 609
                tiles.push(self.mg_ref(tr, x, min_y));
            }
            if self.mg_is_valid_coord(tr, x, max_y) && min_y != max_y {
                // 616 + 609
                tiles.push(self.mg_ref(tr, x, max_y));
            }
            x += 1.0;
        }
        let mut y = min_y + 1.0;
        while y < max_y {
            if self.mg_is_valid_coord(tr, min_x, y) {
                // 616 + 609
                tiles.push(self.mg_ref(tr, min_x, y));
            }
            if self.mg_is_valid_coord(tr, max_x, y) && min_x != max_x {
                // 616 + 609
                tiles.push(self.mg_ref(tr, max_x, y));
            }
            y += 1.0;
        }
        tiles
    }

    /// `removeOldNukeEvents()` — `ticks()` (212) is read unconditionally;
    /// the `while` only pops when the FIFO head aged out.
    fn nn_remove_old_nuke_events(&mut self, tr: &mut Vec<f64>) {
        let max_age = 600.0;
        let tick = self.mg_ticks(tr); // 212
        while !self.ne_recently_sent_nukes.is_empty()
            && self.ne_recently_sent_nukes[0].0 + max_age < tick
        {
            self.ne_recently_sent_nukes.remove(0);
        }
    }

    /// `isUnderHeavyAttack()` — `incomingAttacks()` (822) troops sum vs.
    /// `troops()` (418).
    fn nn_is_under_heavy_attack(&mut self, tr: &mut Vec<f64>, pl: usize) -> bool {
        let incoming = self.pl_incoming_attacks(tr, pl); // 822
        let mut total_incoming = 0.0;
        for &a in &incoming {
            total_incoming += self.at_troops(tr, a); // 830
        }
        let my_troops = self.pl_troops(tr, pl); // 418
        total_incoming >= my_troops
    }

    /// `getPerceivedNukeCost(type)` — every `return this.cost(ty)` re-reads
    /// `unitInfo(ty).cost` (969); the three gates read players / isUnitDisabled
    /// / gameMode / gold / difficulty in source order.
    fn nn_get_perceived_nuke_cost(&mut self, tr: &mut Vec<f64>, pl: usize, ty: &str) -> i64 {
        if self.mg_players(tr).len() == 2 {
            // 221
            return self.ne_cost(tr, ty, pl);
        }
        if self.mg_is_unit_disabled(tr, UNIT_MIRV) {
            // 511
            return self.ne_cost(tr, ty, pl);
        }
        if self.mg_game_mode(tr) == GAME_MODE_TEAM
            // 520
            && self.pl_gold(tr, pl) > self.ne_cost(tr, UNIT_HYDROGEN_BOMB, pl)
        // 417 + 969
        {
            return self.ne_cost(tr, ty, pl);
        }
        if self.pl_gold(tr, pl) > self.ne_cost(tr, UNIT_MIRV, pl) + self.ne_cost(tr, UNIT_HYDROGEN_BOMB, pl)
        // 417 + 969 + 969
        {
            return self.ne_cost(tr, ty, pl);
        }
        let difficulty = self.mg_difficulty(tr); // 402
        if (difficulty == DIFFICULTY_HARD || difficulty == DIFFICULTY_IMPOSSIBLE)
            && self.nn_is_under_heavy_attack(tr, pl)
        {
            return self.ne_cost(tr, ty, pl);
        }
        if ty == UNIT_ATOM_BOMB {
            self.ne_atom_perceived_cost
        } else {
            self.ne_hydro_perceived_cost
        }
    }

    /// `isRichestNation()`.
    fn nn_is_richest_nation(&mut self, tr: &mut Vec<f64>, pl: usize) -> bool {
        let my_gold = self.pl_gold(tr, pl); // 417
        for other in self.mg_players(tr) {
            // 221
            if other == pl {
                continue;
            }
            if self.pl_type(tr, other) != PLAYER_TYPE_NATION {
                // 251
                continue;
            }
            if self.pl_gold(tr, other) > my_gold {
                // 417
                return false;
            }
        }
        true
    }

    /// `findHighDensityTarget()` — density is levelSum / numTilesOwned against
    /// the 1/75 threshold.
    fn nn_find_high_density_target(&mut self, tr: &mut Vec<f64>, pl: usize) -> Option<usize> {
        let mut best_target: Option<usize> = None;
        let mut best_density = NN_HIGH_DENSITY_NUKE_THRESHOLD;
        for other in self.mg_players(tr) {
            // 221
            if other == pl {
                continue;
            }
            if self.pl_type(tr, other) == PLAYER_TYPE_BOT {
                // 251
                continue;
            }
            if self.pl_is_friendly(tr, pl, Some(other)) {
                // 242
                continue;
            }
            let tiles_owned = self.pl_num_tiles_owned(tr, other); // 532
            if tiles_owned == 0.0 {
                continue;
            }
            let structures = self.pl_units(
                tr,
                other,
                &STRUCTURES_TYPES.iter().map(|s| s.to_string()).collect::<Vec<String>>(),
            ); // 241
            let mut level_sum = 0.0;
            for &s in &structures {
                level_sum += self.un_level(tr, s); // 540
            }
            if level_sum < NN_MIN_LEVEL_SUM_FOR_HIGH_DENSITY_NUKE {
                continue;
            }
            let density = level_sum / tiles_owned;
            if density > best_density {
                best_density = density;
                best_target = Some(other);
            }
        }
        best_target
    }

    /// `findFFACrownTarget()` — FFA-only crown logic with the
    /// `b.numTilesOwned() - a.numTilesOwned()` V8 sort (532 comparator).
    fn nn_find_ffa_crown_target(&mut self, tr: &mut Vec<f64>, pl: usize) -> Option<usize> {
        let difficulty = self.mg_difficulty(tr); // 402
        if self.mg_game_mode(tr) != GAME_MODE_FFA {
            // 520
            return None;
        }
        let players = self.mg_players(tr); // 221
        if players.len() <= 1 {
            return None;
        }
        let sorted = self.sort_players_by_tiles(tr, players);
        let first_place = sorted[0];
        if difficulty == DIFFICULTY_IMPOSSIBLE && first_place == pl && sorted.len() >= 2 {
            let second_place = sorted[1];
            if !self.pl_is_friendly(tr, pl, Some(second_place)) {
                // 242
                return Some(second_place);
            }
        }
        if first_place == pl || self.pl_is_friendly(tr, pl, Some(first_place)) {
            // 242
            return None;
        }
        let without_fallout = self.mg_num_land_tiles(tr) - self.mg_num_tiles_with_fallout(tr);
        // 501, 502
        if without_fallout <= 0.0 {
            return None;
        }
        let first_share = self.pl_num_tiles_owned(tr, first_place) / without_fallout; // 532
        let my_share = self.pl_num_tiles_owned(tr, pl) / without_fallout; // 532
        let threshold = match difficulty.as_str() {
            "Easy" => 0.4,
            "Medium" => 0.3,
            "Hard" => 0.2,
            "Impossible" => 0.1,
            _ => unreachable!("unknown difficulty"),
        };
        if first_share - my_share > threshold {
            return Some(first_place);
        }
        None
    }

    /// `findStrongestTeamTarget()` — Team-only; sorts team totals stably
    /// (comparator is facade-free so only the order matters).
    fn nn_find_strongest_team_target(&mut self, tr: &mut Vec<f64>, pl: usize) -> Option<usize> {
        if self.mg_game_mode(tr) != GAME_MODE_TEAM {
            // 520
            return None;
        }
        let players = self.mg_players(tr); // 221
        if players.len() <= 1 {
            return None;
        }
        // `teamTiles.set(team, (teamTiles.get(team) ?? 0) + numTilesOwned())`
        // then `teamPlayers.push` — numTilesOwned (532) read once per player.
        // The loop re-calls game.players() (TS 375) rather than reusing the
        // length-check list, so the facade read is traced twice.
        let mut team_tiles: Vec<(String, f64)> = Vec::new();
        let mut team_players: Vec<(String, Vec<usize>)> = Vec::new();
        for p in self.mg_players(tr) {
            let team = match self.pl_team(tr, p) {
                // 248
                JsVal::Str(s) => s,
                _ => continue,
            };
            let tiles = self.pl_num_tiles_owned(tr, p); // 532
            match team_tiles.iter_mut().find(|(t, _)| *t == team) {
                Some(e) => e.1 += tiles,
                None => team_tiles.push((team.clone(), tiles)),
            }
            match team_players.iter_mut().find(|(t, _)| *t == team) {
                Some(e) => e.1.push(p),
                None => team_players.push((team, vec![p])),
            }
        }
        if team_tiles.is_empty() {
            return None;
        }
        let mut sorted_teams = team_tiles;
        sorted_teams.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let mut strongest_team = sorted_teams[0].0.clone();
        // If the strongest team is ours, fall back to the second strongest.
        if let JsVal::Str(my_team) = self.pl_team(tr, pl) {
            // 248
            if strongest_team == my_team {
                if sorted_teams.len() > 1 {
                    strongest_team = sorted_teams[1].0.clone();
                } else {
                    return None;
                }
            }
        }
        let (_, members) = team_players.iter().find(|(t, _)| *t == strongest_team)?;
        // Filter out friendly players.
        let mut valid_targets: Vec<usize> = Vec::new();
        for &m in members {
            if !self.pl_is_friendly(tr, pl, Some(m)) {
                // 242
                valid_targets.push(m);
            }
        }
        if valid_targets.is_empty() {
            return None;
        }
        let rng = self.random.as_mut().unwrap();
        if rng.chance(2.0) {
            // Strongest player:
            // `reduce((prev, current) => maxTroops(prev) > maxTroops(current) ? prev : current)`
            // — prev read before current, n - 1 times each; ties keep `current`.
            let mut prev = valid_targets[0];
            for &current in &valid_targets[1..] {
                let pv = self.mg_max_troops(tr, Rec::P(prev)); // 401
                let cv = self.mg_max_troops(tr, Rec::P(current)); // 401
                // `maxTroops(prev) > maxTroops(current) ? prev : current` —
                // NaN must keep `current`, so the negated form stays.
                #[allow(clippy::neg_cmp_op_on_partial_ord)]
                let keep = pv > cv;
                prev = if keep { prev } else { current };
            }
            return Some(prev);
        }
        // Random player: `randElement` — PseudoRandom, untraced.
        let idx = rng.next_int(0.0, valid_targets.len() as f64) as usize;
        valid_targets.get(idx.min(valid_targets.len() - 1)).copied()
    }

    /// `findBestNukeTarget()` — 1v1 > retaliation > richest+density >
    /// Impossible crown > allies > most-hated > FFA crown > team ladder.
    fn nn_find_best_nuke_target(&mut self, tr: &mut Vec<f64>, pl: usize) -> Option<usize> {
        // `const { difficulty: diff } = this.game.config().gameConfig()` — 402
        // read before everything else.
        let diff = self.mg_difficulty(tr); // 402
        // On Hard & Impossible with only 2 players left, target the other one.
        if (diff == DIFFICULTY_HARD || diff == DIFFICULTY_IMPOSSIBLE)
            && self.mg_players(tr).len() == 2
        {
            // 221 — `players().find(...)` re-reads the list.
            let players = self.mg_players(tr); // 221
            if let Some(&other) = players.iter().find(|&&p| p != pl) {
                return Some(other);
            }
        }
        // Retaliate against incoming attacks (most important).
        if let Some(t) = self.ai_find_incoming_attack_player(tr, pl) {
            return Some(t);
        }
        // Impossible: the richest nation hunts high-density targets.
        if diff == DIFFICULTY_IMPOSSIBLE && self.nn_is_richest_nation(tr, pl) {
            let rng = self.random.as_mut().unwrap();
            if rng.chance(2.0) {
                return self.nn_find_high_density_target(tr, pl);
            }
        }
        // `const { difficulty, gameMode } = ...` — 402 re-read then 520.
        let difficulty = self.mg_difficulty(tr); // 402
        let game_mode = self.mg_game_mode(tr); // 520
        if difficulty == DIFFICULTY_IMPOSSIBLE && game_mode == GAME_MODE_FFA {
            // Nuke the crown owning > 50% of the map.
            let without_fallout =
                self.mg_num_land_tiles(tr) - self.mg_num_tiles_with_fallout(tr); // 501, 502
            if without_fallout > 0.0 {
                let players = self.mg_players(tr); // 221
                let sorted = self.sort_players_by_tiles(tr, players); // 532 comparator
                let crown = sorted[0];
                if crown != pl && !self.pl_is_friendly(tr, pl, Some(crown)) {
                    // 242
                    let crown_share = self.pl_num_tiles_owned(tr, crown) / without_fallout; // 532
                    if crown_share > 0.5 {
                        return Some(crown);
                    }
                }
            }
        }
        // Assist allies: nuke the first non-friendly target of any friendly ally.
        for ally in self.pl_allies(tr, pl) {
            // 964
            let ally_targets = self.pl_targets(tr, ally); // 963
            if ally_targets.is_empty() {
                continue;
            }
            if self.pl_relation(tr, pl, Some(ally)) < RELATION_FRIENDLY {
                // 412
                continue;
            }
            let ally_targets = self.pl_targets(tr, ally); // 963 (re-read)
            for &target in &ally_targets {
                if target == pl {
                    continue;
                }
                if self.pl_is_friendly(tr, pl, Some(target)) {
                    // 242
                    continue;
                }
                return Some(target);
            }
        }
        // Most hated hostile player, skipping much weaker ones.
        let my_max_troops = self.mg_max_troops(tr, Rec::P(pl)); // 401
        for (rel, other) in self.pl_all_relations_sorted(tr, pl) {
            // 965
            if rel != RELATION_HOSTILE {
                continue;
            }
            if self.pl_is_friendly(tr, pl, Some(other)) {
                // 242
                continue;
            }
            let other_max_troops = self.mg_max_troops(tr, Rec::P(other)); // 401
            if my_max_troops >= other_max_troops * 2.0 {
                continue;
            }
            return Some(other);
        }
        if let Some(t) = self.nn_find_ffa_crown_target(tr, pl) {
            return Some(t);
        }
        if let Some(t) = self.nn_find_strongest_team_target(tr, pl) {
            return Some(t);
        }
        None
    }

    /// `isTeammateAlreadyNukingThisSpot(tile, nukeType)`.
    fn nn_is_teammate_already_nuking_this_spot(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
        tile: f64,
        nuke_type: &str,
    ) -> bool {
        let (our_inner, _) = self.mg_nuke_magnitudes(tr, nuke_type); // 224
        let active_nukes = self.mg_units_typed(tr, &[UNIT_ATOM_BOMB.to_string(), UNIT_HYDROGEN_BOMB.to_string()]); // 968
        for nuke in active_nukes {
            let nuke_owner = self.un_owner(tr, nuke); // 286
            if nuke_owner == Some(pl) {
                continue;
            }
            if !self.pl_is_friendly(tr, pl, nuke_owner) {
                // 242
                continue;
            }
            let Some(target_tile) = self.un_target_tile(tr, nuke) else {
                // 282
                continue;
            };
            let teammate_type = self.un_type(tr, nuke); // 283
            let (_, teammate_inner) = self.mg_nuke_magnitudes(tr, &teammate_type); // 224
            let dist_squared = self.mg_euclidean_dist_squared(tr, tile, target_tile); // 710
            if dist_squared <= (our_inner + teammate_inner) * (our_inner + teammate_inner) {
                return true;
            }
        }
        false
    }

    /// `isTrajectoryInterceptableBySam(spawnTile, targetTile, excludedSamIds?)`
    /// — Parabola (751) sweep + per-SAM level/range checks (707/306/286/242/
    /// 340/540/974).
    fn nn_is_trajectory_interceptable_by_sam(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
        spawn_tile: f64,
        target_tile: f64,
        excluded_sam_ids: Option<&[f64]>,
    ) -> bool {
        let _speed = self.mg_nuke_speed(tr, UNIT_ATOM_BOMB); // 700
        let trajectory = self.mg_parabola_find_path(tr, spawn_tile, target_tile); // 751
        if trajectory.is_empty() {
            return false;
        }
        let target_range = self.mg_default_nuke_targetable_range(tr); // 702
        let target_range_sq = target_range * target_range; // `r ** 2` is one read
        let mut untargetable_start: i64 = -1;
        let mut untargetable_end: i64 = -1;
        for (i, &tile) in trajectory.iter().enumerate() {
            let i = i as i64;
            if untargetable_start == -1 {
                if self.mg_euclidean_dist_squared(tr, tile, spawn_tile) > target_range_sq {
                    // 710
                    if self.mg_euclidean_dist_squared(tr, tile, target_tile) < target_range_sq {
                        // 710
                        break;
                    } else {
                        untargetable_start = i;
                    }
                }
            } else if self.mg_euclidean_dist_squared(tr, tile, target_tile) < target_range_sq {
                // 710
                untargetable_end = i;
                break;
            }
        }
        let mut i: i64 = 0;
        while (i as usize) < trajectory.len() {
            if untargetable_start != -1
                && untargetable_end != -1
                && i == untargetable_start
            {
                i = untargetable_end - 1;
                i += 1;
                continue;
            }
            let tile = trajectory[i as usize];
            let max_sam = self.mg_max_sam_range(tr); // 707
            let nearby_sams =
                self.mg_nearby_units_d2(tr, tile, max_sam, &[UNIT_SAM_LAUNCHER.to_string()]); // 306
            for (sam, sam_d2) in nearby_sams {
                let owner = self.un_owner(tr, sam); // 286
                if owner == Some(pl) {
                    continue;
                }
                if self.pl_is_friendly(tr, pl, owner) {
                    // 242
                    continue;
                }
                if let Some(ex) = excluded_sam_ids {
                    // `excludedSamIds?.has(...)` — `id()` (340) only read when
                    // the set exists.
                    let id = self.un_id(tr, sam); // 340
                    if ex.contains(&id) {
                        continue;
                    }
                }
                let level = self.un_level(tr, sam); // 540
                let range = self.mg_sam_range(tr, level); // 974
                if sam_d2 <= range * range {
                    return true;
                }
            }
            i += 1;
        }
        false
    }

    /// `isValidNukeTile(tile, target)` — owner match or Hard/Impossible
    /// terra-nullius / hostile-team rules.
    fn nn_is_valid_nuke_tile(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
        t: f64,
        nuke_target: usize,
    ) -> bool {
        let difficulty = self.mg_difficulty(tr); // 402
        let owner = self.mg_owner(tr, t); // 222 — Option<usize>, None = TN
        if owner == Some(nuke_target) {
            return true;
        }
        if difficulty == DIFFICULTY_HARD || difficulty == DIFFICULTY_IMPOSSIBLE {
            // `!owner.isPlayer() || (gameMode === Team && owner.isPlayer() &&
            // !isFriendly(owner))` — `isPlayer()` is a traced 259 read per
            // call, and the third conjunct re-calls it (second read).
            if !self.pl_is_player_owner(tr, owner) {
                return true;
            }
            if self.mg_game_mode(tr) == GAME_MODE_TEAM {
                // 520
                if self.pl_is_player_owner(tr, owner) {
                    // 259 (re-read)
                    if !self.pl_is_friendly(tr, pl, owner) {
                        // 242
                        return true;
                    }
                }
            }
            return false;
        }
        false
    }

    /// `owner.isPlayer()` — the TN singleton is an untraced constant `false`;
    /// player tokens trace 259.
    fn pl_is_player_owner(&mut self, tr: &mut Vec<f64>, owner: Option<usize>) -> bool {
        match owner {
            Some(tok) => self.pl_is_player(tr, tok),
            None => false,
        }
    }

    /// `closestTwoTiles(game, a, b)` — one `width()` read (610); the sort and
    /// sweep are facade-free (coords are decoded from the tile id).
    fn nn_closest_two_tiles(
        &mut self,
        tr: &mut Vec<f64>,
        a: &[f64],
        b: &[f64],
    ) -> Option<(f64, f64)> {
        let w = self.mg_width(tr); // 610
        closest_two_tiles_w(w, a, b)
    }

    /// `nukeTileScore(tile, silos, targets, nukeType)`.
    fn nn_nuke_tile_score(
        &mut self,
        tr: &mut Vec<f64>,
        tile: f64,
        silos: &[usize],
        targets: &[usize],
        nuke_type: &str,
    ) -> f64 {
        let (_magnitude_inner, magnitude_outer) = self.mg_nuke_magnitudes(tr, nuke_type); // 224
        let mut tile_value = 0.0;
        // `.filter(euclDistFN(tile, outer))` — tile + dist read per unit, no
        // short-circuit; `.map(...)` then adds the per-type level weight.
        for &unit in targets {
            let ut = self.un_tile(tr, unit); // 978
            let d2 = self.mg_euclidean_dist_squared(tr, tile, ut); // 710
            // JS `!(d2 <= outer²)` — NaN must `continue`.
            #[allow(clippy::neg_cmp_op_on_partial_ord)]
            if !(d2 <= magnitude_outer * magnitude_outer) {
                continue;
            }
            let level = self.un_level(tr, unit); // 540
            let uty = self.un_type(tr, unit); // 283
            tile_value += match uty.as_str() {
                "City" => 25_000.0 * level,
                "Defense Post" => 5_000.0 * level,
                "Missile Silo" => 50_000.0 * level,
                "Port" => 15_000.0 * level,
                "Factory" => 15_000.0 * level,
                _ => 0.0,
            };
        }
        let difficulty = self.mg_difficulty(tr); // 402
        if difficulty == DIFFICULTY_MEDIUM {
            // `.some((unit) => unit.type() === SAMLauncher &&
            // euclDist50(unit.tile()))` — short-circuits on the first hit.
            for &unit in targets {
                let uty = self.un_type(tr, unit); // 283
                if uty == UNIT_SAM_LAUNCHER {
                    let ut = self.un_tile(tr, unit); // 978
                    let d2 = self.mg_euclidean_dist_squared(tr, tile, ut); // 710
                    if d2 <= 50.0 * 50.0 {
                        return -1.0;
                    }
                }
            }
        }
        if difficulty == DIFFICULTY_IMPOSSIBLE && nuke_type == UNIT_HYDROGEN_BOMB {
            let (_, hydro_outer) = self.mg_nuke_magnitudes(tr, UNIT_HYDROGEN_BOMB); // 224
            let nearby_sams = self.mg_nearby_units_d2(
                tr,
                tile,
                hydro_outer,
                &[UNIT_SAM_LAUNCHER.to_string()],
            ); // 306
            for (sam, _d2) in nearby_sams {
                let sam_level = self.un_level(tr, sam); // 540
                if sam_level >= NN_MAX_NATION_SILO_UPGRADE_LEVEL {
                    continue;
                }
                let sam_range = self.mg_sam_range(tr, sam_level); // 974
                let sam_tile = self.un_tile(tr, sam); // 978
                let d2 = self.mg_euclidean_dist_squared(tr, tile, sam_tile); // 710
                let dist = d2.sqrt();
                if dist > sam_range {
                    tile_value += 100_000.0 * sam_level;
                }
            }
        }
        // `closestTwoTiles(game, siloTiles, [tile])` — silo tiles first, then
        // the scored tile.
        let mut silo_tiles = Vec::with_capacity(silos.len());
        for &silo in silos {
            silo_tiles.push(self.un_tile(tr, silo)); // 978
        }
        let (closest_x, _closest_y) = match self.nn_closest_two_tiles(tr, &silo_tiles, &[tile]) {
            Some(p) => p,
            None => return -1.0,
        };
        // TS reads euclideanDistSquared(tile, closestSilo) (NationNukeBehavior
        // 729): the scored tile first, then the silo-side tile from
        // closestTwoTiles. Passing (closest_x, closest_y) instead recorded the
        // args swapped on tiles that differ (caught by G5o scenario C).
        let distance_squared = self.mg_euclidean_dist_squared(tr, tile, closest_x); // 710
        let distance_penalty = distance_squared.sqrt() * 30.0;
        let base = tile_value;
        tile_value = (base * 0.2).max(base - distance_penalty);
        // `recentlySentNukes.filter(...).map(...).reduce(...)` — per entry:
        // nukeMagnitudes (224) then dist (710); each hit subtracts 1M.
        let recent = self.ne_recently_sent_nukes.clone();
        for (_tick, recent_tile, recent_type) in recent {
            let (recent_inner, _) = self.mg_nuke_magnitudes(tr, &recent_type); // 224
            let d2 = self.mg_euclidean_dist_squared(tr, tile, recent_tile); // 710
            if d2 <= recent_inner * recent_inner {
                tile_value -= 1_000_000.0;
            }
        }
        tile_value
    }

    /// `sendNuke(tile, nukeType, targetPlayer)` — tick (212), the perceived-
    /// cost inflation (bigint *150/125 /100 as exact i64 math), the
    /// NukeExecution record (307 + TAG_NUKE) and the EMOJI_NUKE ladder.
    fn nn_send_nuke(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
        tile: f64,
        nuke_type: &str,
        target_player: usize,
        wait_ticks: f64,
    ) -> Result<(), String> {
        let tick = self.mg_ticks(tr); // 212
        self.ne_recently_sent_nukes
            .push((tick, tile, nuke_type.to_string()));
        if nuke_type == UNIT_ATOM_BOMB {
            self.ne_atom_bombs_launched += 1.0;
            self.ne_atom_perceived_cost = self.ne_atom_perceived_cost * 150 / 100;
        } else if nuke_type == UNIT_HYDROGEN_BOMB {
            self.ne_hydro_bombs_launched += 1.0;
            self.ne_hydro_perceived_cost = self.ne_hydro_perceived_cost * 125 / 100;
        }
        self.mg_add_nuke(tr, nuke_type, pl, tile, None, -1.0, wait_ticks, None);
        self.ne_maybe_send_emoji(tr, Rec::P(target_player), "EMOJI_NUKE")?;
        Ok(())
    }

    /// V8's binary insertion sort with the ascending `a.level() - b.level()`
    /// comparator (540). Invoked as `Compare(element, mid)`, so the ELEMENT's
    /// level is read before the mid's.
    fn nn_sort_units_by_level(&mut self, tr: &mut Vec<f64>, mut arr: Vec<usize>) -> Vec<usize> {
        let n = arr.len();
        if n <= 1 {
            return arr;
        }
        for i in 1..n {
            let element = arr[i];
            let (mut lo, mut hi) = (0usize, i);
            while lo < hi {
                let mid = (lo + hi) / 2;
                let ev = self.un_level(tr, element); // 540
                let mv = self.un_level(tr, arr[mid]); // 540
                if ev - mv < 0.0 {
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

    /// `maybeDestroyEnemySam(target)` — the Impossible-only AtomBomb salvo
    /// meant to overwhelm the target's SAM cover (NationNukeBehavior.ts
    /// lines 780-1005).
    fn nn_maybe_destroy_enemy_sam(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
        nuke_target: usize,
    ) -> Result<(), String> {
        if self.mg_is_unit_disabled(tr, UNIT_ATOM_BOMB) {
            // 511
            return Ok(());
        }
        // Don't launch another salvo if we already have atom bombs in flight.
        let our_atom_bombs = self.pl_units(tr, pl, &[UNIT_ATOM_BOMB.to_string()]); // 241
        if !our_atom_bombs.is_empty() {
            return Ok(());
        }
        let atom_cost = self.ne_cost(tr, UNIT_ATOM_BOMB, pl); // 969
        let enemy_sams = self.pl_units(tr, nuke_target, &[UNIT_SAM_LAUNCHER.to_string()]); // 241
        if enemy_sams.is_empty() {
            return Ok(());
        }
        // `player.units(MissileSilo).filter(silo => !silo.isUnderConstruction())`.
        let all_our_silos = self.pl_units(tr, pl, &[UNIT_MISSILE_SILO.to_string()]); // 241
        let mut our_silos: Vec<usize> = Vec::new();
        for s in all_our_silos {
            if !self.un_is_under_construction(tr, s) {
                // 345
                our_silos.push(s);
            }
        }
        if our_silos.is_empty() {
            return Ok(());
        }
        // `enemySams.slice().sort((a, b) => a.level() - b.level())` — V8 binary
        // insertion sort, element level read before the mid's.
        let sorted_sams = self.nn_sort_units_by_level(tr, enemy_sams); // 540 comparator
        let mut needs_more_silos = false;
        // First failed attempt, for `maybeUpgradeHelpfulSilo`:
        // (targetTile, coveringSamIds, totalBombs).
        let mut failed_target: Option<(f64, Vec<f64>, f64)> = None;
        for &target_sam in &sorted_sams {
            let target_tile = self.un_tile(tr, target_sam); // 978
            // Every enemy SAM whose range covers the target tile will try to
            // intercept, so the salvo must beat their combined capacity.
            let covering_sams = self.nn_find_enemy_sams_covering_tile(tr, pl, target_tile);
            let mut covering_sam_ids: Vec<f64> = Vec::with_capacity(covering_sams.len());
            for &s in &covering_sams {
                covering_sam_ids.push(self.un_id(tr, s)); // 340
            }
            let mut total_interceptions = 0.0;
            for &s in &covering_sams {
                total_interceptions += self.un_level(tr, s); // 540
            }
            let bombs_needed = total_interceptions + 1.0;
            // `nukeSpeed` only configures the untraced Parabola ctor; the 700
            // read itself is traced.
            let _nuke_speed = self.mg_nuke_speed(tr, UNIT_ATOM_BOMB); // 700
            // Per-silo launch planning; slots = level - missileTimerQueue length.
            let mut all_available_silos: Vec<(usize, f64, f64, bool)> = Vec::new();
            for &silo in &our_silos {
                let level = self.un_level(tr, silo); // 540
                let queue = self.un_missile_timer_queue(tr, silo); // 346
                let available_slots = level - as_arr_nums(&queue).len() as f64;
                if available_slots <= 0.0 {
                    continue;
                }
                let silo_tile = self.un_tile(tr, silo); // 978 (interceptability arg)
                let interceptable = self.nn_is_trajectory_interceptable_by_sam(
                    tr,
                    pl,
                    silo_tile,
                    target_tile,
                    Some(&covering_sam_ids),
                );
                let silo_tile2 = self.un_tile(tr, silo); // 978 (findPath arg, re-read)
                let trajectory = self.mg_parabola_find_path(tr, silo_tile2, target_tile); // 751
                if trajectory.is_empty() {
                    continue;
                }
                all_available_silos.push((
                    silo,
                    available_slots,
                    trajectory.len() as f64,
                    interceptable,
                ));
            }
            // `allAvailableSilos.sort((a, b) => manhattanDist(a.silo.tile(),
            // targetTile) - manhattanDist(b.silo.tile(), targetTile))` — V8
            // binary insertion sort, element's tile/dist read before the mid's.
            let n = all_available_silos.len();
            for i in 1..n {
                let element = all_available_silos[i];
                let (mut lo, mut hi) = (0usize, i);
                while lo < hi {
                    let mid = (lo + hi) / 2;
                    let ev = {
                        let et = self.un_tile(tr, element.0); // 978
                        self.mg_manhattan_dist(tr, et, target_tile) // 509
                    };
                    let mv = {
                        let mt = self.un_tile(tr, all_available_silos[mid].0); // 978
                        self.mg_manhattan_dist(tr, mt, target_tile) // 509
                    };
                    if ev - mv < 0.0 {
                        hi = mid;
                    } else {
                        lo = mid + 1;
                    }
                }
                if lo < i {
                    all_available_silos.remove(i);
                    all_available_silos.insert(lo, element);
                }
            }
            // Flatten into a per-bomb launch sequence: each silo contributes
            // `slots` consecutive bombs (NukeExecution's pick order).
            let mut launch_sequence: Vec<(f64, bool)> = Vec::new(); // (flightTicks, interceptable)
            for &(_, slots, flight_ticks, interceptable) in &all_available_silos {
                for _ in 0..slots as i64 {
                    launch_sequence.push((flight_ticks, interceptable));
                }
            }
            let sam_cooldown = self.mg_sam_cooldown(tr); // 708
            let max_total_arrival_spread = (sam_cooldown / 2.0).floor();
            // Extra bombs: 1 for every 5, for SAMs built while in flight.
            let extra_bombs = (bombs_needed / 5.0).floor();
            let total_bombs = bombs_needed + extra_bombs;
            // Bombs from silos whose trajectory is not blocked by non-covering SAMs.
            let mut unblocked_bombs: Vec<(usize, f64)> = Vec::new(); // (index, flightTicks)
            for (i, &(flight_ticks, interceptable)) in launch_sequence.iter().enumerate() {
                if !interceptable {
                    unblocked_bombs.push((i, flight_ticks));
                }
            }
            if (unblocked_bombs.len() as f64) < total_bombs {
                if failed_target.is_none() {
                    failed_target = Some((target_tile, covering_sam_ids.clone(), total_bombs));
                }
                needs_more_silos = true;
                continue;
            }
            // Facade-free comparator (`a.flightTicks - b.flightTicks`): the
            // stable result order is all that matters.
            let mut sorted_by_flight = unblocked_bombs.clone();
            sorted_by_flight
                .sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            // Sliding window of maxTotalArrivalSpread capturing the most bombs.
            let mut best_window_start = 0usize;
            let mut best_window_count = 0usize;
            for start in 0..sorted_by_flight.len() {
                let mut end = start;
                while end < sorted_by_flight.len()
                    && sorted_by_flight[end].1 - sorted_by_flight[start].1
                        <= max_total_arrival_spread
                {
                    end += 1;
                }
                if end - start > best_window_count {
                    best_window_count = end - start;
                    best_window_start = start;
                }
            }
            if (best_window_count as f64) < total_bombs {
                if failed_target.is_none() {
                    failed_target = Some((target_tile, covering_sam_ids.clone(), total_bombs));
                }
                needs_more_silos = true;
                continue;
            }
            // Pick totalBombs with the lowest launch-sequence indices.
            let window_bombs =
                sorted_by_flight[best_window_start..best_window_start + best_window_count].to_vec();
            let mut window_by_index = window_bombs;
            window_by_index.sort_by_key(|b| b.0); // facade-free, stable
            let selected = &window_by_index[..total_bombs as usize];
            let selected_set: std::collections::HashSet<usize> = selected.iter().map(|b| b.0).collect();
            let bombs_to_fire = (selected[selected.len() - 1].0 + 1) as f64;
            let selected_flight_min = selected
                .iter()
                .map(|b| b.1)
                .fold(f64::INFINITY, f64::min);
            let stagger_interval = (max_total_arrival_spread / total_bombs).floor().max(1.0);
            let mut selected_idx = 0.0f64;
            let mut wait_ticks_per_bomb: Vec<f64> = Vec::new();
            for (i, &(flight_ticks, _)) in
                launch_sequence.iter().enumerate().take(bombs_to_fire as usize)
            {
                if selected_set.contains(&i) {
                    let target_arrival = selected_flight_min + selected_idx * stagger_interval;
                    wait_ticks_per_bomb.push((target_arrival - flight_ticks).max(0.0));
                    selected_idx += 1.0;
                } else {
                    // Wasted bomb (interceptable or out-of-window).
                    wait_ticks_per_bomb.push(0.0);
                }
            }
            // `atomCost * BigInt(bombsToFire)` vs. `gold()` — exact small-int f64 math.
            let total_cost = atom_cost as f64 * bombs_to_fire;
            if (self.pl_gold(tr, pl) as f64) < total_cost {
                // 417
                continue;
            }
            // Fire the salvo; NukeExecution picks silos in the same Manhattan
            // distance order we planned.
            for &wait_ticks in &wait_ticks_per_bomb {
                self.nn_send_nuke(
                    tr,
                    pl,
                    target_tile,
                    UNIT_ATOM_BOMB,
                    nuke_target,
                    wait_ticks,
                )?;
            }
            return Ok(());
        }
        // Couldn't destroy any SAM — upgrade silos only if capacity was the
        // bottleneck (not when we merely lack gold).
        if needs_more_silos {
            if let Some((target_tile, covering_sam_ids, total_bombs)) = failed_target {
                self.nn_maybe_upgrade_helpful_silo(
                    tr,
                    pl,
                    target_tile,
                    &covering_sam_ids,
                    total_bombs,
                );
            }
        }
        Ok(())
    }

    /// `findEnemySamsCoveringTile(tile)` (NationNukeBehavior.ts lines
    /// 1010-1029) — every enemy SAM in maxSamRange whose own range covers
    /// `tile`.
    fn nn_find_enemy_sams_covering_tile(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
        tile: f64,
    ) -> Vec<usize> {
        let max_sam = self.mg_max_sam_range(tr); // 707
        let nearby = self.mg_nearby_units_d2(tr, tile, max_sam, &[UNIT_SAM_LAUNCHER.to_string()]); // 306
        let mut result = Vec::new();
        for (sam, sam_d2) in nearby {
            let owner = self.un_owner(tr, sam); // 286
            if owner == Some(pl) {
                continue;
            }
            if self.pl_is_friendly(tr, pl, owner) {
                // 242
                continue;
            }
            let level = self.un_level(tr, sam); // 540
            let range = self.mg_sam_range(tr, level); // 974
            if sam_d2 <= range * range {
                result.push(sam);
            }
        }
        result
    }

    /// `maybeUpgradeHelpfulSilo(failedTarget)` (NationNukeBehavior.ts lines
    /// 1037-1099) — upgrade a silo whose trajectory to the failed target is
    /// unblocked and that is best protected by our own SAMs.
    fn nn_maybe_upgrade_helpful_silo(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
        failed_target_tile: f64,
        covering_sam_ids: &[f64],
        total_bombs: f64,
    ) {
        let silos = self.pl_units(tr, pl, &[UNIT_MISSILE_SILO.to_string()]); // 241
        if silos.is_empty() {
            return;
        }
        // First pass: silos with an unblocked trajectory to the failed target.
        let mut unblocked_silos: Vec<usize> = Vec::new();
        for &silo in &silos {
            let silo_tile = self.un_tile(tr, silo); // 978
            if !self.nn_is_trajectory_interceptable_by_sam(
                tr,
                pl,
                silo_tile,
                failed_target_tile,
                Some(covering_sam_ids),
            ) {
                unblocked_silos.push(silo);
            }
        }
        if unblocked_silos.is_empty() {
            return;
        }
        // Bail out if unreachable even at max silo level.
        let max_achievable_slots =
            unblocked_silos.len() as f64 * NN_MAX_NATION_SILO_UPGRADE_LEVEL;
        if max_achievable_slots < total_bombs {
            return;
        }
        let our_sams = self.pl_units(tr, pl, &[UNIT_SAM_LAUNCHER.to_string()]); // 241
        let mut best_silo: Option<usize> = None;
        let mut best_protection = -1.0;
        for &silo in &unblocked_silos {
            if self.un_level(tr, silo) >= NN_MAX_NATION_SILO_UPGRADE_LEVEL {
                // 540
                continue;
            }
            if !self.pl_can_upgrade_unit(tr, pl, silo) {
                // 326
                continue;
            }
            let mut protection = 0.0;
            for &sam in &our_sams {
                let sam_level = self.un_level(tr, sam); // 540
                let range = self.mg_sam_range(tr, sam_level); // 974
                let silo_tile = self.un_tile(tr, silo); // 978
                let sam_tile = self.un_tile(tr, sam); // 978
                let d2 = self.mg_euclidean_dist_squared(tr, silo_tile, sam_tile); // 710
                if d2 <= range * range {
                    protection += self.un_level(tr, sam); // 540 (re-read)
                }
            }
            if protection > best_protection {
                best_protection = protection;
                best_silo = Some(silo);
            }
        }
        if let Some(silo) = best_silo {
            let silo_id = self.un_id(tr, silo); // 340
            self.mg_add_upgrade_structure_exec(tr, pl, silo_id);
        }
    }

    /// `NationNukeBehavior.maybeSendNuke()` — the real G5m port.
    fn ne_maybe_send_nuke(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        let pl = self.ne_player.ok_or("Player not initialized")?;
        // `player.units(MissileSilo)` (241) then the three isUnitDisabled
        // gates (511) — `||`/`&&` short-circuit in source order.
        let silos = self.pl_units(tr, pl, &[UNIT_MISSILE_SILO.to_string()]);
        if silos.is_empty()
            || self.mg_is_unit_disabled(tr, UNIT_MISSILE_SILO)
            || (self.mg_is_unit_disabled(tr, UNIT_ATOM_BOMB)
                && self.mg_is_unit_disabled(tr, UNIT_HYDROGEN_BOMB))
        {
            return Ok(());
        }
        let Some(nuke_target) = self.nn_find_best_nuke_target(tr, pl) else {
            return Ok(());
        };
        if self.pl_type(tr, nuke_target) == PLAYER_TYPE_BOT {
            // 251
            return Ok(());
        }
        if self.pl_is_on_same_team(tr, pl, Some(nuke_target)) {
            // 250
            return Ok(());
        }
        if !self.ai_should_attack(tr, pl, nuke_target, TargetVia::Pl)? {
            return Ok(());
        }
        let hydro_cost = self.nn_get_perceived_nuke_cost(tr, pl, UNIT_HYDROGEN_BOMB);
        let atom_cost = self.nn_get_perceived_nuke_cost(tr, pl, UNIT_ATOM_BOMB);
        let nuke_type: &str = if !self.mg_is_unit_disabled(tr, UNIT_HYDROGEN_BOMB)
            && self.pl_gold(tr, pl) >= hydro_cost
        {
            // 511 + 417
            UNIT_HYDROGEN_BOMB
        } else if !self.mg_is_unit_disabled(tr, UNIT_ATOM_BOMB)
            && (!self.ne_is_hydro_nation || self.nn_is_under_heavy_attack(tr, pl))
            && self.pl_gold(tr, pl) >= atom_cost
        {
            // 511 + (no facade) + 417
            UNIT_ATOM_BOMB
        } else {
            return Ok(());
        };
        let (_, outer) = self.mg_nuke_magnitudes(tr, nuke_type); // 224
        let structures = self.pl_units(
            tr,
            nuke_target,
            &STRUCTURES_TYPES.iter().map(|s| s.to_string()).collect::<Vec<String>>(),
        ); // 241
        let mut structure_tiles = Vec::with_capacity(structures.len());
        for &s in &structures {
            structure_tiles.push(self.un_tile(tr, s)); // 978
        }
        let difficulty = self.mg_difficulty(tr); // 402
        let num_random_tiles = if difficulty == DIFFICULTY_IMPOSSIBLE { 30.0 } else { 10.0 };
        let random_tiles = self.ne_rand_territory_tile_array(tr, nuke_target, num_random_tiles)?;
        let mut all_tiles = random_tiles;
        all_tiles.extend(structure_tiles);
        self.nn_remove_old_nuke_events(tr); // 212
        let mut best_tile: Option<f64> = None;
        let mut best_value = -1.0;
        // `new Set(allTiles)` — insertion-order dedup; `tile === null` skips.
        let mut dedup: Vec<f64> = Vec::new();
        for tile in all_tiles {
            if dedup.contains(&tile) {
                continue;
            }
            dedup.push(tile);
            let mut bounding = self.nn_bounding_box_tiles(tr, tile, outer);
            bounding.extend(self.nn_bounding_box_tiles(tr, tile, (outer / 2.0).floor()));
            if bounding
                .iter()
                .any(|&t| !self.nn_is_valid_nuke_tile(tr, pl, t, nuke_target))
            {
                continue;
            }
            let spawn_tile = self.pl_can_build(tr, pl, nuke_type, tile); // 534
            if matches!(spawn_tile, JsVal::Bool(false)) {
                continue;
            }
            let spawn_tile_num = as_num(&spawn_tile);
            if self.mg_game_mode(tr) == GAME_MODE_TEAM
                && difficulty != DIFFICULTY_EASY
                && self.nn_is_teammate_already_nuking_this_spot(tr, pl, tile, nuke_type)
            {
                continue;
            }
            if (difficulty == DIFFICULTY_HARD || difficulty == DIFFICULTY_IMPOSSIBLE)
                && self.nn_is_trajectory_interceptable_by_sam(
                    tr,
                    pl,
                    spawn_tile_num,
                    tile,
                    None,
                )
            {
                continue;
            }
            let value = self.nn_nuke_tile_score(tr, tile, &silos, &structures, nuke_type);
            if value > best_value {
                best_tile = Some(tile);
                best_value = value;
            }
        }
        match best_tile {
            Some(tile) if best_value > 0.0 || difficulty != DIFFICULTY_IMPOSSIBLE => {
                self.nn_send_nuke(tr, pl, tile, nuke_type, nuke_target, 0.0)?;
            }
            // JS: `bestTile !== null` gate failing still reaches the
            // Impossible SAM-destroy fallback.
            _ if difficulty == DIFFICULTY_IMPOSSIBLE => {
                self.nn_maybe_destroy_enemy_sam(tr, pl, nuke_target)?;
            }
            _ => {}
        }
        Ok(())
    }


    // ---- AiAttackBehavior methods (ported inline) -----------------------------

    /// `sendAttack(target, force = false)`. `via` selects which mock object
    /// class the target reached us through, so `target.isPlayer()` traces on
    /// the same code the TS harness records (259 `Player`, 257 `Neighbor`,
    /// untraced `false` for the `TerraNullius` singleton).
    fn ai_send_attack(
        &mut self,
        tr: &mut Vec<f64>,
        target: usize,
        via: TargetVia,
        force: bool,
    ) -> Result<bool, String> {
        if !force && !self.ai_should_attack(tr, self.te_tribe, target, via)? {
            return Ok(false);
        }
        if self.ai_target_is_player(tr, target, via) {
            if self.pl_shares_border_with(tr, self.te_tribe, target) {
                // 962
                return self.ai_send_land_attack(tr, target, via);
            }
            return self.ai_send_boat_attack(tr, target, via);
        }
        // TerraNullius: land-only adjacency check.
        if self.ai_has_land_border_with_tn(tr) {
            return self.ai_send_land_attack(tr, TN_TOK, TargetVia::Tn);
        }
        // (the `else` arm below is `sendBoatAttackToNearbyTerraNullius`)
        self.ai_send_boat_attack_to_nearby_tn(tr)
    }

    /// `target.isPlayer()` as traced on the target's concrete mock class.
    fn ai_target_is_player(&mut self, tr: &mut Vec<f64>, target: usize, via: TargetVia) -> bool {
        match via {
            TargetVia::Pl => self.pl_is_player(tr, target), // 259
            TargetVia::Nb => self.nb_is_player(tr, target), // 257
            TargetVia::Tn => false,                         // TerraNullius: untraced
        }
    }

    /// `forceSendAttack(target)`.
    #[allow(dead_code)]
    fn ai_force_send_attack(&mut self, tr: &mut Vec<f64>, target: usize) -> Result<(), String> {
        let troops = self.pl_troops(tr, self.te_tribe) / 2.0; // 418
        let tid = if self.pl_is_player(tr, target) {
            // 259
            Some(self.pl_id(tr, target)) // 231
        } else {
            self.tn_id(tr);
            None
        };
        self.mg_add_attack_exec(tr, troops, self.te_tribe, tid.as_deref(), None, true);
        Ok(())
    }

    /// `shouldAttack(other)`.
    fn ai_should_attack(
        &mut self,
        tr: &mut Vec<f64>,
        pl: usize,
        other: usize,
        via: TargetVia,
    ) -> Result<bool, String> {
        // `other.isPlayer() === false || other.type() !== Human ||
        // other.isTraitor() || player.type() === Bot || playerTeams === HvN`
        // — every `||` arm short-circuits, so the facade reads stop at the
        // first truthy arm.
        if !self.ai_target_is_player(tr, other, via) {
            return Ok(true);
        }
        if self.pl_type(tr, other) != PLAYER_TYPE_HUMAN {
            // 251
            return Ok(true);
        }
        if self.pl_is_traitor(tr, other) {
            // 826
            return Ok(true);
        }
        if self.pl_type(tr, pl) == PLAYER_TYPE_BOT {
            // 251
            return Ok(true);
        }
        let pt = self.mg_player_teams(tr); // 960
        if matches!(&pt, JsVal::Str(s) if s == HUMANS_VS_NATIONS) {
            return Ok(true);
        }
        let diff = self.mg_difficulty(tr); // 402
        let rng = self.random.as_mut().unwrap();
        if diff == DIFFICULTY_EASY && rng.next_int(0.0, 4.0) != 0 {
            return Ok(false);
        }
        if diff == DIFFICULTY_MEDIUM && rng.chance(4.0) {
            return Ok(false);
        }
        Ok(true)
    }

    /// `hasLandBorderWithTerraNullius()`.
    fn ai_has_land_border_with_tn(&mut self, tr: &mut Vec<f64>) -> bool {
        let border_tiles = self.pl_border_tiles(tr, self.te_tribe); // 635
        for tile in border_tiles {
            let neighbors = self.mg_neighbors4_map(tr, tile); // map().neighbors4
            for nb in neighbors {
                if self.mg_is_land(tr, nb)
                    && !self.mg_is_impassable(tr, nb)
                    && !self.mg_has_owner(tr, nb)
                {
                    return true;
                }
            }
        }
        false
    }

    /// `shoreTiles(player)`.
    fn ai_shore_tiles(&mut self, tr: &mut Vec<f64>, player: usize) -> Vec<f64> {
        let border_tiles = self.pl_border_tiles(tr, player); // 635
        let mut out = Vec::new();
        for t in border_tiles {
            if self.mg_is_shore(tr, t) {
                // 967 (`game.isShore`, forEach over the whole border set)
                out.push(t);
            }
        }
        out
    }

    /// `sendBoatAttackToNearbyTerraNullius()`.
    fn ai_send_boat_attack_to_nearby_tn(
        &mut self,
        tr: &mut Vec<f64>,
    ) -> Result<bool, String> {
        if self.mg_is_unit_disabled(tr, UNIT_TRANSPORT_SHIP) {
            // 511
            return Ok(false);
        }
        if self.pl_unit_count(tr, self.te_tribe, UNIT_TRANSPORT_SHIP)
            >= self.mg_boat_max_number(tr)
        {
            return Ok(false);
        }
        let directions: [(f64, f64); 4] = [(0.0, -1.0), (0.0, 1.0), (-1.0, 0.0), (1.0, 0.0)];
        let shores = self.ai_shore_tiles(tr, self.te_tribe);
        let mut i = 0;
        while i < shores.len() {
            let border = shores[i];
            let bx = self.mg_x(tr, border); // 603
            let by = self.mg_y(tr, border); // 604
            for &(dx, dy) in &directions {
                let x1 = bx + dx;
                let y1 = by + dy;
                if !self.mg_is_valid_coord(tr, x1, y1) {
                    // 616
                    continue;
                }
                let r1 = self.mg_ref(tr, x1, y1); // 609
                if !self.mg_is_water(tr, r1) {
                    // 508
                    continue;
                }
                let nx = bx + dx * 5.0;
                let ny = by + dy * 5.0;
                if !self.mg_is_valid_coord(tr, nx, ny) {
                    continue;
                }
                let tile = self.mg_ref(tr, nx, ny);
                if !self.mg_is_land(tr, tile) {
                    continue;
                }
                if self.mg_is_impassable(tr, tile) {
                    continue;
                }
                if self.mg_has_owner(tr, tile) {
                    continue;
                }
                if self.mg_has_fallout(tr, tile) {
                    continue;
                }
                if !self.ai_can_build_transport_ship(tr, tile)? {
                    continue;
                }
                let troops = js_min(
                    self.pl_troops(tr, self.te_tribe) / 5.0,
                    self.ai_troop_send_cap_for_expansion(tr)?,
                );
                if troops < 1.0 {
                    return Ok(false);
                }
                self.mg_add_transport_ship_exec(tr, self.te_tribe, tile, troops);
                return Ok(true);
            }
            i += 10;
        }
        Ok(false)
    }

    /// `sendLandAttack(target)`.
    fn ai_send_land_attack(
        &mut self,
        tr: &mut Vec<f64>,
        target: usize,
        via: TargetVia,
    ) -> Result<bool, String> {
        let troops = self.ai_calculate_attack_troops(tr, target, via, true)?;
        let troops = match troops {
            Some(t) => t,
            None => return Ok(false),
        };
        let tid = if self.ai_target_is_player(tr, target, via) {
            // 259 / 257 / untraced
            Some(self.pl_id(tr, target)) // 231
        } else {
            self.tn_id(tr);
            None
        };
        self.mg_add_attack_exec(tr, troops, self.te_tribe, tid.as_deref(), None, true);
        Ok(true)
    }

    /// `sendBoatAttack(target)`.
    fn ai_send_boat_attack(
        &mut self,
        tr: &mut Vec<f64>,
        target: usize,
        via: TargetVia,
    ) -> Result<bool, String> {
        if self.mg_is_unit_disabled(tr, UNIT_TRANSPORT_SHIP) {
            // 511
            return Ok(false);
        }
        let my_shore = self.ai_shore_tiles(tr, self.te_tribe);
        let their_shore = self.ai_shore_tiles(tr, target);
        let closest = self.ai_closest_two_tiles(tr, &my_shore, &their_shore);
        let (_, cy) = match closest {
            Some(pair) => pair,
            None => return Ok(false),
        };
        if !self.ai_can_build_transport_ship(tr, cy)? {
            return Ok(false);
        }
        let troops = self.ai_calculate_attack_troops(tr, target, via, false)?;
        let troops = match troops {
            Some(t) => t,
            None => return Ok(false),
        };
        self.mg_add_transport_ship_exec(tr, self.te_tribe, cy, troops);
        Ok(true)
    }

    /// `calculateAttackTroops(target, nonBotTroops)`. TS calls
    /// `target.isPlayer()` independently at six points, so each guard
    /// re-traces it (never cached).
    fn ai_calculate_attack_troops(
        &mut self,
        tr: &mut Vec<f64>,
        target: usize,
        via: TargetVia,
        land: bool,
    ) -> Result<Option<f64>, String> {
        let max_troops = self.mg_max_troops(tr, Rec::P(self.te_tribe)); // 401
        let bot_with_structures = self.ai_target_is_player(tr, target, via) // 259/257
            && self.pl_type(tr, target) == PLAYER_TYPE_BOT // 251
            && {
                let units = self.pl_units_all(tr, target); // 241
                units
                    .iter()
                    .any(|&u| STRUCTURES_TYPES.contains(&self.un_type(tr, u).as_str()))
            };
        let use_reserve =
            self.ai_target_is_player(tr, target, via) && !bot_with_structures; // 259/257
        let reserve_ratio = if use_reserve {
            self.te_reserve_ratio
        } else {
            self.te_expand_ratio
        };
        let target_troops = max_troops * reserve_ratio;

        let is_bot_attack = self.ai_target_is_player(tr, target, via) // 259/257
            && self.pl_type(tr, target) == PLAYER_TYPE_BOT // 251
            && self.pl_type(tr, self.te_tribe) != PLAYER_TYPE_BOT; // 251
        let mut troops = if is_bot_attack {
            let max = self.pl_troops(tr, self.te_tribe) - target_troops - self.te_bot_attack_troops_sent;
            self.ai_calculate_bot_attack_troops(tr, target, max)?
        } else if land {
            self.pl_troops(tr, self.te_tribe) - target_troops // 418
        } else {
            self.pl_troops(tr, self.te_tribe) / 5.0 // 418
        };

        // Hard & Impossible: cap by neighbor troop threshold.
        let cap = if self.ai_target_is_player(tr, target, via) {
            // 259/257
            self.ai_troop_send_cap(tr)?
        } else {
            self.ai_troop_send_cap_for_expansion(tr)?
        };
        troops = js_min(troops, cap);

        if troops < 1.0 {
            return Ok(None);
        }

        // Hard & Impossible: don't attack if we'd send less than 20%.
        if self.ai_target_is_player(tr, target, via) // 259/257
            && self.ai_is_attack_too_weak(tr, troops, target)?
        {
            return Ok(None);
        }

        // Nation emoji hook (skipped when emojiBehavior is undefined).
        if self.ai_target_is_player(tr, target, via) // 259/257
            && self.pl_type(tr, self.te_tribe) == PLAYER_TYPE_NATION
        {
            // 251 — `emojiBehavior` is undefined for tribes: the TS throws.
            if !self.te_is_nation {
                return Err("not initialized".into());
            }
            self.ne_maybe_send_attack_emoji(tr, target)?;
        }

        if is_bot_attack {
            self.te_bot_attack_troops_sent += troops;
        }

        Ok(Some(troops))
    }

    /// `calculateBotAttackTroops(target, maxTroops)`.
    fn ai_calculate_bot_attack_troops(
        &mut self,
        tr: &mut Vec<f64>,
        target: usize,
        max_troops: f64,
    ) -> Result<f64, String> {
        let diff = self.mg_difficulty(tr); // 402
        if diff == DIFFICULTY_EASY {
            return Ok(max_troops);
        }
        let mut troops = self.pl_troops(tr, target) * 4.0; // 418
        // `troops > maxTroops` — the inner branch re-reads `target.troops()`.
        if troops > max_troops {
            if max_troops < self.pl_troops(tr, target) * 2.0 {
                // 418
                troops = 0.0;
            } else {
                troops = max_troops;
            }
        }
        Ok(troops)
    }

    /// `isAttackTooWeak(troops, target)`.
    fn ai_is_attack_too_weak(
        &mut self,
        tr: &mut Vec<f64>,
        troops: f64,
        target: usize,
    ) -> Result<bool, String> {
        if self.pl_type(tr, self.te_tribe) == PLAYER_TYPE_BOT {
            // 251
            return Ok(false);
        }
        if self.mg_game_mode(tr) == GAME_MODE_TEAM {
            // 520
            return Ok(false);
        }
        if !self.pl_incoming_attacks(tr, self.te_tribe).is_empty() {
            // 822
            return Ok(false);
        }
        let diff = self.mg_difficulty(tr); // 402
        Ok((diff == DIFFICULTY_HARD || diff == DIFFICULTY_IMPOSSIBLE)
            && troops < self.pl_troops(tr, target) * 0.2) // 418
    }

    /// `troopSendCap()`.
    fn ai_troop_send_cap(&mut self, tr: &mut Vec<f64>) -> Result<f64, String> {
        if self.pl_type(tr, self.te_tribe) == PLAYER_TYPE_BOT {
            // 251
            return Ok(f64::INFINITY);
        }
        if self.mg_game_mode(tr) == GAME_MODE_TEAM {
            // 520
            return Ok(f64::INFINITY);
        }
        let diff = self.mg_difficulty(tr); // 402
        let retain_fraction = match diff.as_str() {
            "Hard" => 0.75,
            "Impossible" => 0.9,
            _ => return Ok(f64::INFINITY),
        };
        let mut max_neighbor_troops: f64 = 0.0;
        let neighbors = self.pl_nearby(tr, self.te_tribe); // 245
        for n in &neighbors {
            let n = *n;
            if !self.nb_is_player(tr, n) {
                // 257
                continue;
            }
            if self.pl_is_friendly(tr, self.te_tribe, Some(n)) {
                // 242
                continue;
            }
            if self.pl_type(tr, n) == PLAYER_TYPE_BOT {
                // 251
                continue;
            }
            // `n.troops() > maxNeighborTroops && (maxNeighborTroops = n.troops())`
            // — the condition and the assignment read `troops()` independently.
            if self.pl_troops(tr, n) > max_neighbor_troops {
                // 418
                max_neighbor_troops = self.pl_troops(tr, n); // 418
            }
        }
        let mut cap;
        if max_neighbor_troops == 0.0 {
            cap = f64::INFINITY;
        } else {
            let min_retained = (max_neighbor_troops * retain_fraction).ceil();
            cap = js_max(0.0, self.pl_troops(tr, self.te_tribe) - min_retained);
        }
        // Nations under attack may retaliate with at least the incoming troops.
        let incoming = self.pl_incoming_attacks(tr, self.te_tribe); // 822
        if !incoming.is_empty() {
            let total: f64 = incoming
                .iter()
                .map(|&a| self.at_troops(tr, a))
                .sum();
            cap = js_max(cap, total);
        }
        Ok(cap)
    }

    /// `troopSendCapForExpansion()`.
    fn ai_troop_send_cap_for_expansion(&mut self, tr: &mut Vec<f64>) -> Result<f64, String> {
        let cap = self.ai_troop_send_cap(tr)?;
        if cap > 0.0 {
            return Ok(cap);
        }
        Ok((self.pl_troops(tr, self.te_tribe) * 0.05).ceil()) // 418
    }

    /// `getNeighborTraitorToAttack()`.
    fn ai_get_neighbor_traitor_to_attack(&mut self, tr: &mut Vec<f64>) -> Option<usize> {
        if self.mg_disable_alliances(tr) {
            // 958
            return None;
        }
        let neighbors = self.pl_nearby(tr, self.te_tribe); // 245
        let traitors: Vec<usize> = neighbors
            .iter()
            .filter(|&&n| {
                self.nb_is_player(tr, n) // 257
                    && !self.pl_is_friendly(tr, self.te_tribe, Some(n)) // 242
                    && self.pl_is_traitor(tr, n) // 826
            })
            .copied()
            .collect();
        if traitors.is_empty() {
            return None;
        }
        let rng = self.random.as_mut().unwrap();
        rng.rand_element(&traitors).copied()
    }

    /// `attackRandomTarget()`.
    fn ai_attack_random_target(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        // Save up troops until trigger ratio.
        if !self.ai_has_trigger_ratio_troops(tr) {
            return Ok(());
        }
        // Retaliate against incoming attacks.
        if let Some(attacker) = self.ai_find_incoming_attack_player(tr, self.te_tribe) {
            if self.ai_send_attack(tr, attacker, TargetVia::Pl, true)? {
                return Ok(());
            }
        }
        // Select a traitor.
        if let Some(traitor) = self.ai_get_neighbor_traitor_to_attack(tr) {
            let rng = self.random.as_mut().unwrap();
            #[allow(clippy::collapsible_if)]
            if rng.chance(3.0) {
                if self.ai_send_attack(tr, traitor, TargetVia::Nb, false)? {
                    return Ok(());
                }
            }
        }
        // Choose a new enemy randomly.
        let neighbors = self.pl_nearby(tr, self.te_tribe); // 245
        let shuffled = {
            let rng = self.random.as_mut().unwrap();
            rng.shuffle_array(&neighbors)
        };
        for neighbor in shuffled {
            if !self.nb_is_player(tr, neighbor) {
                // 257
                continue;
            }
            if self.pl_is_friendly(tr, self.te_tribe, Some(neighbor)) {
                // 242
                continue;
            }
            // `neighbor.type() === Nation || neighbor.type() === Human` —
            // two independent `type()` reads with `||` short-circuit.
            let is_nation_or_human = self.pl_type(tr, neighbor) == PLAYER_TYPE_NATION // 251
                || self.pl_type(tr, neighbor) == PLAYER_TYPE_HUMAN; // 251
            if is_nation_or_human {
                let rng = self.random.as_mut().unwrap();
                if rng.chance(2.0) {
                    continue;
                }
            }
            if self.ai_send_attack(tr, neighbor, TargetVia::Nb, false)? {
                return Ok(());
            }
        }
        Ok(())
    }

    /// `hasTriggerRatioTroops()`.
    fn ai_has_trigger_ratio_troops(&mut self, tr: &mut Vec<f64>) -> bool {
        let max_troops = self.mg_max_troops(tr, Rec::P(self.te_tribe)); // 401
        let ratio = self.pl_troops(tr, self.te_tribe) / max_troops; // 418
        ratio >= self.te_trigger_ratio
    }

    /// `hasReserveRatioTroops()`.
    #[allow(dead_code)]
    fn ai_has_reserve_ratio_troops(&mut self, tr: &mut Vec<f64>) -> bool {
        let max_troops = self.mg_max_troops(tr, Rec::P(self.te_tribe)); // 401
        let ratio = self.pl_troops(tr, self.te_tribe) / max_troops; // 418
        ratio >= self.te_reserve_ratio
    }

    /// `findIncomingAttackPlayer()`.
    fn ai_find_incoming_attack_player(&mut self, tr: &mut Vec<f64>, pl: usize) -> Option<usize> {
        let mut incoming = self.pl_incoming_attacks(tr, pl); // 822
        incoming.retain(|&a| {
            let attacker = self.at_attacker(tr, a); // 832
            !self.pl_is_friendly(tr, pl, attacker) // 242
        });
        // Ignore bot attacks if we are not a bot.
        if self.pl_type(tr, pl) != PLAYER_TYPE_BOT {
            // 251
            incoming.retain(|&a| {
                let attacker = self.at_attacker(tr, a); // 832
                self.pl_type(tr, attacker.unwrap_or(usize::MAX)) != PLAYER_TYPE_BOT
                // 251
            });
        }
        let mut largest_attack: f64 = 0.0;
        let mut largest_attacker: Option<usize> = None;
        for &a in &incoming {
            // `if (attack.troops() <= largestAttack) continue;`
            // `largestAttack = attack.troops();` — two independent reads.
            let troops = self.at_troops(tr, a); // 830
            if troops <= largest_attack {
                continue;
            }
            largest_attack = self.at_troops(tr, a); // 830
            largest_attacker = self.at_attacker(tr, a); // 832
        }
        largest_attacker
    }

    /// `maybeAttack()` — the full AiAttackBehavior.maybeAttack pipeline.
    /// Note: `allianceBehavior` is undefined for tribes, so the
    /// `maybeSendAllianceRequests` call is skipped.
    #[allow(dead_code)]
    fn ai_maybe_attack(&mut self, tr: &mut Vec<f64>) -> Result<(), String> {
        // Build the bordering player set (insertion order).
        let mut bordering_set: Vec<usize> = Vec::new();
        let mut border_has_non_nuked_tn = false;
        let small_id = self.pl_small_id(tr, self.te_tribe); // 230
        let border_tiles = self.pl_border_tiles(tr, self.te_tribe); // 635
        for t in border_tiles {
            let visit = |slf: &mut Self, tr: &mut Vec<f64>, nb: f64| {
                if !slf.mg_is_land(tr, nb) || slf.mg_is_impassable(tr, nb) {
                    return;
                }
                // `ownerID(t) === smallID` and `playerBySmallID(ownerID(t))`
                // read `ownerID` independently.
                let oid = slf.mg_owner_id(tr, nb); // 223
                if oid == small_id {
                    return;
                }
                let owner = slf.mg_player_by_small_id(tr, oid); // 703
                if let Some(ot) = as_opt_tok(&owner) {
                    if slf.pl_is_player(tr, ot) && !bordering_set.contains(&ot) {
                        bordering_set.push(ot);
                    }
                }
                if !slf.mg_has_owner(tr, nb) && !slf.mg_has_fallout(tr, nb) {
                    border_has_non_nuked_tn = true;
                }
            };
            self.mg_for_each_neighbor(tr, t, visit); // 807 (callback form, trailing Undef)
        }
        let nearby = self.pl_nearby(tr, self.te_tribe); // 245
        for &n in &nearby {
            if self.nb_is_player(tr, n) {
                // 257
                let pt = self.nb_player_tok(tr, n);
                if let Some(t) = pt {
                    if !bordering_set.contains(&t) {
                        bordering_set.push(t);
                    }
                }
            }
        }
        // Sort by troops ascending (stable).
        let mut bordering_players = bordering_set;
        bordering_players.sort_by(|&a, &b| {
            let ta = self.pl_troops(tr, a); // 418
            let tb = self.pl_troops(tr, b); // 418
            ta.partial_cmp(&tb).unwrap_or(std::cmp::Ordering::Equal)
        });
        // TS runs two independent `filter` passes — `isFriendly` is traced
        // twice per player.
        let mut friends: Vec<usize> = Vec::new();
        for &o in &bordering_players {
            if self.pl_is_friendly(tr, self.te_tribe, Some(o)) {
                // 242
                friends.push(o);
            }
        }
        let mut enemies: Vec<usize> = Vec::new();
        for &o in &bordering_players {
            if !self.pl_is_friendly(tr, self.te_tribe, Some(o)) {
                // 242 (second pass)
                enemies.push(o);
            }
        }

        // Attack TerraNullius but not nuked territory.
        let has_non_nuked_tn = border_has_non_nuked_tn
            || nearby.iter().any(|&n| !self.nb_is_player(tr, n)); // 257
        #[allow(clippy::collapsible_if)]
        if has_non_nuked_tn {
            if self.ai_send_attack(tr, TN_TOK, TargetVia::Tn, false)? {
                return Ok(());
            }
        }

        if enemies.is_empty() {
            let rng = self.random.as_mut().unwrap();
            if rng.chance(5.0) {
                self.ai_attack_with_random_boat(tr, &[])?;
            }
        } else {
            let rng = self.random.as_mut().unwrap();
            if rng.chance(10.0) {
                self.ai_attack_with_random_boat(tr, &enemies)?;
                return Ok(());
            }
            // `allianceBehavior.maybeSendAllianceRequests(enemies)` —
            // undefined for tribes (skipped), nation-routed when present.
            if self.te_is_nation {
                self.ne_maybe_send_alliance_requests(tr, &enemies)?;
            }
        }

        self.ai_attack_best_target(tr, &friends, &enemies)?;
        Ok(())
    }

    /// `attackBestTarget(borderingFriends, borderingEnemies)`.
    fn ai_attack_best_target(
        &mut self,
        tr: &mut Vec<f64>,
        bordering_friends: &[usize],
        bordering_enemies: &[usize],
    ) -> Result<(), String> {
        // Attack bots with structures first.
        #[allow(clippy::collapsible_if)]
        if self.ai_has_neighboring_bot_with_structures(tr) {
            if self.ai_attack_bots(tr)? {
                return Ok(());
            }
        }
        // Save up troops until reserve ratio.
        if !self.ai_has_reserve_ratio_troops(tr) {
            return Ok(());
        }
        // Maybe save up until trigger ratio.
        if !self.ai_has_trigger_ratio_troops(tr) {
            let rng = self.random.as_mut().unwrap();
            if !rng.chance(10.0) {
                return Ok(());
            }
        }
        // Get attack strategies in priority order.
        let strategies = self.ai_get_attack_strategies(bordering_friends, bordering_enemies);
        for s in strategies {
            if self.ai_run_strategy(tr, &s, bordering_friends, bordering_enemies)? {
                return Ok(());
            }
        }
        Ok(())
    }

    /// `getAttackStrategies(borderingFriends, borderingEnemies)`.
    fn ai_get_attack_strategies(
        &mut self,
        _friends: &[usize],
        _enemies: &[usize],
    ) -> Vec<AiStrategy> {
        let mut scratch = Vec::new();
        let diff = self.mg_difficulty(&mut scratch); // 402
        match diff.as_str() {
            "Easy" => vec![
                AiStrategy::Nuked,
                AiStrategy::Bots,
                AiStrategy::Retaliate,
                AiStrategy::Assist,
                AiStrategy::Betray,
                AiStrategy::Hated,
                AiStrategy::Weakest,
            ],
            "Medium" => vec![
                AiStrategy::Bots,
                AiStrategy::Nuked,
                AiStrategy::Retaliate,
                AiStrategy::Assist,
                AiStrategy::Betray,
                AiStrategy::Hated,
                AiStrategy::Afk,
                AiStrategy::Traitor,
                AiStrategy::Weakest,
                AiStrategy::Island,
                AiStrategy::Donate,
            ],
            "Hard" => vec![
                AiStrategy::Bots,
                AiStrategy::Retaliate,
                AiStrategy::Assist,
                AiStrategy::Betray,
                AiStrategy::Nuked,
                AiStrategy::Traitor,
                AiStrategy::Afk,
                AiStrategy::Hated,
                AiStrategy::VeryWeak,
                AiStrategy::Juicy,
                AiStrategy::Victim,
                AiStrategy::Weakest,
                AiStrategy::Island,
                AiStrategy::Donate,
            ],
            "Impossible" => vec![
                AiStrategy::Retaliate,
                AiStrategy::Bots,
                AiStrategy::VeryWeak,
                AiStrategy::Betray,
                AiStrategy::Assist,
                AiStrategy::Victim,
                AiStrategy::Traitor,
                AiStrategy::Juicy,
                AiStrategy::Afk,
                AiStrategy::Nuked,
                AiStrategy::Hated,
                AiStrategy::Weakest,
                AiStrategy::Island,
                AiStrategy::Donate,
            ],
            _ => vec![],
        }
    }

    /// Run one attack strategy.
    fn ai_run_strategy(
        &mut self,
        tr: &mut Vec<f64>,
        s: &AiStrategy,
        bordering_friends: &[usize],
        bordering_enemies: &[usize],
    ) -> Result<bool, String> {
        match s {
            AiStrategy::Retaliate => {
                if let Some(attacker) = self.ai_find_incoming_attack_player(tr, self.te_tribe) {
                    return self.ai_send_attack(tr, attacker, TargetVia::Pl, true);
                }
                Ok(false)
            }
            AiStrategy::Bots => self.ai_attack_bots(tr),
            AiStrategy::Assist => self.ai_assist_allies(tr),
            AiStrategy::Traitor => {
                if let Some(t) = self.ai_find_traitor(tr, bordering_enemies) {
                    return self.ai_send_attack(tr, t, TargetVia::Pl, false);
                }
                Ok(false)
            }
            AiStrategy::Afk => {
                #[allow(clippy::bool_comparison)]
                let afk = bordering_enemies.iter().find(|&&e| {
                    self.pl_is_disconnected(tr, e) // 531
                        && (self.ai_is_ffa(tr) == false
                            || self.pl_troops(tr, e) < self.pl_troops(tr, self.te_tribe) * 3.0)
                });
                match afk {
                    Some(&a) => self.ai_send_attack(tr, a, TargetVia::Pl, false),
                    None => Ok(false),
                }
            }
            AiStrategy::Betray => {
                self.ai_maybe_betray_and_attack(tr, bordering_friends, bordering_enemies)
            }
            AiStrategy::Nuked => {
                if self.ai_is_bordering_nuked_territory(tr) {
                    return self.ai_send_attack(tr, TN_TOK, TargetVia::Tn, false);
                }
                Ok(false)
            }
            AiStrategy::Victim => {
                if let Some(v) = self.ai_find_victim(tr, bordering_enemies) {
                    return self.ai_send_attack(tr, v, TargetVia::Pl, false);
                }
                Ok(false)
            }
            AiStrategy::Juicy => {
                if let Some(t) = self.ai_find_juicy_target(tr, bordering_enemies) {
                    return self.ai_send_attack(tr, t, TargetVia::Pl, false);
                }
                Ok(false)
            }
            AiStrategy::Hated => {
                let relations = self.pl_all_relations_sorted(tr, self.te_tribe); // 965
                for (rel, other) in relations {
                    if rel != RELATION_HOSTILE {
                        continue;
                    }
                    if self.pl_is_friendly(tr, self.te_tribe, Some(other)) {
                        // 242
                        continue;
                    }
                    if self.ai_is_ffa(tr)
                        && self.pl_troops(tr, other) > self.pl_troops(tr, self.te_tribe) * 3.0
                    {
                        continue;
                    }
                    return self.ai_send_attack(tr, other, TargetVia::Pl, false);
                }
                Ok(false)
            }
            AiStrategy::VeryWeak => {
                if let Some(v) = self.ai_find_very_weak_enemy(tr, bordering_enemies) {
                    return self.ai_send_attack(tr, v, TargetVia::Pl, false);
                }
                Ok(false)
            }
            AiStrategy::Weakest => {
                if !bordering_enemies.is_empty() {
                    let weakest = bordering_enemies[0];
                    if !self.ai_is_ffa(tr)
                        || self.pl_troops(tr, weakest) < self.pl_troops(tr, self.te_tribe)
                    {
                        return self.ai_send_attack(tr, weakest, TargetVia::Pl, false);
                    }
                }
                Ok(false)
            }
            AiStrategy::Island => {
                if bordering_enemies.is_empty() {
                    if let Some(e) = self.ai_find_nearest_island_enemy(tr)? {
                        return self.ai_send_attack(tr, e, TargetVia::Pl, false);
                    }
                }
                Ok(false)
            }
            AiStrategy::Donate => self.ai_donate_troops(tr),
        }
    }

    /// `isFFA()`.
    fn ai_is_ffa(&mut self, tr: &mut Vec<f64>) -> bool {
        self.mg_game_mode(tr) == GAME_MODE_FFA // 520
    }

    /// `hasNeighboringBotWithStructures()`.
    fn ai_has_neighboring_bot_with_structures(&mut self, tr: &mut Vec<f64>) -> bool {
        let nearby = self.pl_nearby(tr, self.te_tribe); // 245
        nearby.iter().any(|&n| {
            if !self.nb_is_player(tr, n) {
                return false;
            }
            let pt = match self.nb_player_tok(tr, n) {
                Some(t) => t,
                None => return false,
            };
            self.pl_type(tr, pt) == PLAYER_TYPE_BOT // 251
                && !self.pl_is_friendly(tr, self.te_tribe, Some(pt)) // 242
                && {
                    let units = self.pl_units_all(tr, pt); // 241
                    units
                        .iter()
                        .any(|&u| STRUCTURES_TYPES.contains(&self.un_type(tr, u).as_str()))
                }
        })
    }

    /// `attackBots()`.
    fn ai_attack_bots(&mut self, tr: &mut Vec<f64>) -> Result<bool, String> {
        let nearby = self.pl_nearby(tr, self.te_tribe); // 245
        // TS filter order: `n.isPlayer() && isFriendly(n) === false &&
        // n.type() === Bot` (the `filter_map` cast is erased in JS).
        let mut bots: Vec<usize> = Vec::new();
        for &n in &nearby {
            if !self.nb_is_player(tr, n) {
                // 257
                continue;
            }
            if self.pl_is_friendly(tr, self.te_tribe, Some(n)) {
                // 242
                continue;
            }
            if self.pl_type(tr, n) == PLAYER_TYPE_BOT {
                // 251
                bots.push(n);
            }
        }
        if bots.is_empty() {
            return Ok(false);
        }
        self.te_bot_attack_troops_sent = 0.0;

        // Sort by density (troops / tiles), structure-owners first.
        let mut sorted = bots.clone();
        sorted.sort_by(|&a, &b| {
            let a_has = {
                let units = self.pl_units_all(tr, a);
                units
                    .iter()
                    .any(|&u| STRUCTURES_TYPES.contains(&self.un_type(tr, u).as_str()))
            };
            let b_has = {
                let units = self.pl_units_all(tr, b);
                units
                    .iter()
                    .any(|&u| STRUCTURES_TYPES.contains(&self.un_type(tr, u).as_str()))
            };
            if a_has != b_has {
                return if a_has {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                };
            }
            let da = self.pl_troops(tr, a) / self.pl_num_tiles_owned(tr, a); // 418 / 532
            let db = self.pl_troops(tr, b) / self.pl_num_tiles_owned(tr, b); // 418 / 532
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        });

        let max_parallel = self.ai_get_bot_attack_max_parallelism(tr)?;
        let reduced: Vec<usize> = sorted.into_iter().take(max_parallel as usize).collect();
        for bot in reduced {
            self.ai_send_attack(tr, bot, TargetVia::Nb, false)?;
        }
        Ok(self.te_bot_attack_troops_sent > 0.0)
    }

    /// `getBotAttackMaxParallelism()`.
    fn ai_get_bot_attack_max_parallelism(&mut self, tr: &mut Vec<f64>) -> Result<f64, String> {
        let diff = self.mg_difficulty(tr); // 402
        match diff.as_str() {
            "Easy" => Ok(1.0),
            "Medium" => {
                let rng = self.random.as_mut().unwrap();
                Ok(if rng.chance(2.0) { 1.0 } else { 2.0 })
            }
            "Hard" => Ok(3.0),
            "Impossible" => Ok(100.0),
            other => Err(format!("Unexpected value: {other}")),
        }
    }

    /// `assistAllies()` — `emojiBehavior` is undefined for tribes, so this
    /// throws if reached. (Not reachable from TribeExecution.)
    fn ai_assist_allies(&mut self, tr: &mut Vec<f64>) -> Result<bool, String> {
        if !self.te_is_nation {
            return Err("not initialized".into());
        }
        let pl = self.ne_player.ok_or("Player not initialized")?;
        if self.mg_disable_alliances(tr) {
            // 958
            return Ok(false);
        }
        let allies = self.pl_allies(tr, pl); // 964
        for ally in allies {
            let targets = self.pl_targets(tr, ally); // 963
            if targets.is_empty() {
                continue;
            }
            if self.pl_relation(tr, pl, Some(ally)) < RELATION_FRIENDLY {
                // 412
                self.ne_send_emoji(tr, Rec::P(ally), "EMOJI_ASSIST_RELATION_TOO_LOW")?;
                continue;
            }
            for target in targets {
                if target == pl {
                    self.ne_send_emoji(tr, Rec::P(ally), "EMOJI_ASSIST_TARGET_ME")?;
                    continue;
                }
                if self.pl_is_friendly(tr, pl, Some(target)) {
                    // 242
                    self.ne_send_emoji(tr, Rec::P(ally), "EMOJI_ASSIST_TARGET_ALLY")?;
                    continue;
                }
                if !self.ai_send_attack(tr, target, TargetVia::Pl, false)? {
                    continue;
                }
                self.pl_update_relation(tr, pl, Some(ally), -20.0); // 237
                self.ne_send_emoji(tr, Rec::P(ally), "EMOJI_ASSIST_ACCEPT")?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// `findTraitor(borderingEnemies)`.
    fn ai_find_traitor(
        &mut self,
        tr: &mut Vec<f64>,
        bordering_enemies: &[usize],
    ) -> Option<usize> {
        if self.mg_disable_alliances(tr) {
            // 958
            return None;
        }
        bordering_enemies.iter().find(|&&e| {
            self.pl_is_traitor(tr, e) // 826
                && (!self.ai_is_ffa(tr)
                    || self.pl_troops(tr, e) < self.pl_troops(tr, self.te_tribe) * 1.2)
        }).copied()
    }

    /// `maybeBetrayAndAttack(borderingFriends, borderingEnemies)` —
    /// `allianceBehavior` is undefined for tribes, so this throws if reached.
    fn ai_maybe_betray_and_attack(
        &mut self,
        tr: &mut Vec<f64>,
        friends: &[usize],
        enemies: &[usize],
    ) -> Result<bool, String> {
        if !self.te_is_nation {
            return Err("not initialized".into());
        }
        if self.mg_disable_alliances(tr) {
            // 958
            return Ok(false);
        }
        if !friends.is_empty() {
            // Computed once, not per friend below.
            let juiciest_ally = self.ne_find_juiciest_ally(tr, friends)?;
            for &friend in friends {
                if self.ne_maybe_betray(tr, friend, juiciest_ally, friends, enemies)? {
                    return self.ai_send_attack(tr, friend, TargetVia::Pl, true);
                }
            }
        }
        Ok(false)
    }

    /// `isBorderingNukedTerritory()`.
    fn ai_is_bordering_nuked_territory(&mut self, tr: &mut Vec<f64>) -> bool {
        if self.mg_is_unit_disabled(tr, UNIT_MISSILE_SILO) {
            // 511
            return false;
        }
        let border_tiles = self.pl_border_tiles(tr, self.te_tribe); // 635
        for tile in border_tiles {
            // `game.neighbors4(tile, nbuf)` — the Game facade (804), not
            // `map.neighbors4`.
            let neighbors = self.mg_neighbors4(tr, tile);
            for nb in neighbors {
                if self.mg_is_land(tr, nb)
                    && !self.mg_has_owner(tr, nb)
                    && self.mg_has_fallout(tr, nb)
                {
                    return true;
                }
            }
        }
        false
    }

    /// `findVictim(borderingEnemies)`.
    fn ai_find_victim(
        &mut self,
        tr: &mut Vec<f64>,
        bordering_enemies: &[usize],
    ) -> Option<usize> {
        bordering_enemies
            .iter()
            .find(|&&e| {
                if self.ai_is_ffa(tr)
                    && self.pl_troops(tr, e) > self.pl_troops(tr, self.te_tribe) * 1.2
                {
                    return false;
                }
                let incoming = self.pl_incoming_attacks(tr, e); // 822
                let total: f64 = incoming.iter().map(|&a| self.at_troops(tr, a)).sum();
                total > self.pl_troops(tr, e) * 0.5 // 418
            })
            .copied()
    }

    /// `findVeryWeakEnemy(borderingEnemies)` — TS `filter` visits **every**
    /// enemy (no short-circuit), then takes `[0]`.
    fn ai_find_very_weak_enemy(
        &mut self,
        tr: &mut Vec<f64>,
        bordering_enemies: &[usize],
    ) -> Option<usize> {
        let mut found: Option<usize> = None;
        for &e in bordering_enemies {
            let max_troops = self.mg_max_troops(tr, Rec::P(e)); // 401
            let ok = self.pl_troops(tr, e) < max_troops * 0.15 // 418
                && (!self.ai_is_ffa(tr)
                    || self.pl_troops(tr, e) < self.pl_troops(tr, self.te_tribe) * 1.2);
            if ok && found.is_none() {
                found = Some(e);
            }
        }
        found
    }

    /// `findJuicyTarget(borderingEnemies)` — the candidates are filtered to
    /// troops ≤ 75% of ours, then `findJuiciestTarget` ranks them.
    fn ai_find_juicy_target(
        &mut self,
        tr: &mut Vec<f64>,
        bordering_enemies: &[usize],
    ) -> Option<usize> {
        let candidates: Vec<usize> = bordering_enemies
            .iter()
            .filter(|&&e| self.pl_troops(tr, e) <= self.pl_troops(tr, self.te_tribe) * 0.75)
            .copied()
            .collect();
        self.ai_find_juiciest_target(tr, &candidates)
    }

    /// `findJuiciestTarget(game, candidates)` — inlined from
    /// `nation/NationUtils.ts` (G5e `LowestN` precedent: plain helper inside
    /// exec_layer.rs).
    fn ai_find_juiciest_target(
        &mut self,
        tr: &mut Vec<f64>,
        candidates: &[usize],
    ) -> Option<usize> {
        if candidates.is_empty() {
            return None;
        }
        // Compute per-candidate stats. TS reads `u.type()` up to three times
        // per unit (`Structures.has(u.type()) && u.type() !== DefensePost &&
        // u.type() !== MissileSilo`) with `&&` short-circuit, and `u.level()`
        // only when all three hold.
        let mut stats: Vec<(usize, f64, f64, f64)> = Vec::new(); // (tok, structureCount, troopGapRatio, tiles)
        for &p in candidates {
            let units = self.pl_units_all(tr, p); // 241
            let structure_count: f64 = units
                .iter()
                .map(|&u| {
                    if !STRUCTURES_TYPES.contains(&self.un_type(tr, u).as_str()) {
                        // 283
                        return 0.0;
                    }
                    if self.un_type(tr, u) == UNIT_DEFENSE_POST {
                        // 283
                        return 0.0;
                    }
                    if self.un_type(tr, u) == UNIT_MISSILE_SILO {
                        // 283
                        return 0.0;
                    }
                    self.un_level(tr, u) // 540
                })
                .sum();
            let max_troops = self.mg_max_troops(tr, Rec::P(p)); // 401
            let troop_gap_ratio = if max_troops > 0.0 {
                1.0 - self.pl_troops(tr, p) / max_troops // 418
            } else {
                0.0
            };
            let tiles = self.pl_num_tiles_owned(tr, p); // 532
            stats.push((p, structure_count, troop_gap_ratio, tiles));
        }
        // Normalize each stat and score.
        let scs: Vec<f64> = stats.iter().map(|s| s.1).collect();
        let tgrs: Vec<f64> = stats.iter().map(|s| s.2).collect();
        let tls: Vec<f64> = stats.iter().map(|s| s.3).collect();
        let normalize = |value: f64, values: &[f64]| -> f64 {
            let min = values.iter().fold(f64::INFINITY, |a, &b| js_min(a, b));
            let max = values.iter().fold(f64::NEG_INFINITY, |a, &b| js_max(a, b));
            if max > min {
                (value - min) / (max - min)
            } else {
                0.0
            }
        };
        let mut best: Option<usize> = None;
        let mut best_score = f64::NEG_INFINITY;
        for &(p, sc, tgr, tl) in &stats {
            let score = normalize(sc, &scs) + normalize(tgr, &tgrs) + normalize(tl, &tls);
            if score > best_score {
                best_score = score;
                best = Some(p);
            }
        }
        best
    }

    /// `findNearestIslandEnemy()`.
    fn ai_find_nearest_island_enemy(
        &mut self,
        tr: &mut Vec<f64>,
    ) -> Result<Option<usize>, String> {
        if self.mg_is_unit_disabled(tr, UNIT_TRANSPORT_SHIP) {
            // 511
            return Ok(None);
        }
        if self.pl_unit_count(tr, self.te_tribe, UNIT_TRANSPORT_SHIP)
            >= self.mg_boat_max_number(tr)
        {
            return Ok(None);
        }
        // `Array.from(borderTiles()).some(t => game.isShore(t))` — short
        // circuit on the first shore tile.
        let border_tiles = self.pl_border_tiles(tr, self.te_tribe); // 635
        let has_shore = border_tiles
            .into_iter()
            .any(|t| self.mg_is_shore(tr, t)); // 967
        if !has_shore {
            return Ok(None);
        }
        let all_players = self.mg_players(tr); // 221
        let filtered: Vec<usize> = all_players
            .iter()
            .filter(|&&p| {
                if p == self.te_tribe {
                    return false;
                }
                if self.pl_is_friendly(tr, self.te_tribe, Some(p)) {
                    // 242
                    return false;
                }
                !self.ai_is_ffa(tr) || self.pl_troops(tr, p) < self.pl_troops(tr, self.te_tribe)
            })
            .copied()
            .collect();
        if filtered.is_empty() {
            return Ok(None);
        }
        let my_center = self.ai_get_player_center(tr, self.te_tribe)?;
        let mut sorted: Vec<(usize, f64)> = filtered
            .iter()
            .map(|&p| {
                let pc = self.ai_get_player_center(tr, p)?;
                // `game.ref(playerCenter.x, playerCenter.y)` is re-called per
                // entry (609, 609) before the manhattanDist (509).
                let my_tile = self.mg_ref(tr, my_center.0, my_center.1); // 609
                let p_tile = self.mg_ref(tr, pc.0, pc.1); // 609
                let dist = self.mg_manhattan_dist(tr, my_tile, p_tile); // 509
                Ok((p, dist))
            })
            .collect::<Result<Vec<_>, String>>()?;
        sorted.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut reachable: Vec<usize> = Vec::new();
        for &(p, _) in &sorted {
            // `shoreTiles(this.player)` is recomputed every iteration in TS.
            let my_shore = self.ai_shore_tiles(tr, self.te_tribe);
            let their_shore = self.ai_shore_tiles(tr, p);
            let closest = self.ai_closest_two_tiles(tr, &my_shore, &their_shore);
            let (_, cy) = match closest {
                Some(pair) => pair,
                None => continue,
            };
            if self.ai_can_build_transport_ship(tr, cy)? {
                reachable.push(p);
                if reachable.len() >= 2 {
                    break;
                }
            }
        }
        if reachable.is_empty() {
            return Ok(None);
        }
        if reachable.len() >= 2 {
            let rng = self.random.as_mut().unwrap();
            if rng.chance(3.0) {
                return Ok(Some(reachable[1]));
            }
        }
        Ok(Some(reachable[0]))
    }

    /// `getPlayerCenter(player)`.
    fn ai_get_player_center(
        &mut self,
        tr: &mut Vec<f64>,
        player: usize,
    ) -> Result<(f64, f64), String> {
        // Check if largestClusterBoundingBox is set (G5j field).
        // We read it via the scripted mock — it's set by PlayerExecution.
        // For the tribe scenarios, we fall back to calculateBoundingBoxCenter.
        let border_tiles = self.pl_border_tiles(tr, player); // 635
        if border_tiles.is_empty() {
            return Ok((0.0, 0.0));
        }
        // calculateBoundingBoxCenter inline.
        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for &t in &border_tiles {
            let x = self.mg_x(tr, t); // 603
            let y = self.mg_y(tr, t); // 604
            if x < min_x {
                min_x = x;
            }
            if x > max_x {
                max_x = x;
            }
            if y < min_y {
                min_y = y;
            }
            if y > max_y {
                max_y = y;
            }
        }
        Ok((
            ((min_x + max_x) / 2.0).floor(),
            ((min_y + max_y) / 2.0).floor(),
        ))
    }

    /// `closestTwoTiles(x, y)` — inlined from `execution/Util.ts`. Uses the
    /// ElMap width when available; otherwise falls back to scripted width.
    fn ai_closest_two_tiles(
        &mut self,
        tr: &mut Vec<f64>,
        x: &[f64],
        y: &[f64],
    ) -> Option<(f64, f64)> {
        let w = if let Some(ref map) = self.el_map {
            map.w
        } else {
            self.mg_width(tr) // 610
        };
        crate::exec_util::closest_two_tiles_w(w, x, y)
    }

    /// `attackWithRandomBoat(borderingEnemies)`.
    fn ai_attack_with_random_boat(
        &mut self,
        tr: &mut Vec<f64>,
        bordering_enemies: &[usize],
    ) -> Result<(), String> {
        if self.mg_is_unit_disabled(tr, UNIT_TRANSPORT_SHIP) {
            // 511
            return Ok(());
        }
        if self.pl_unit_count(tr, self.te_tribe, UNIT_TRANSPORT_SHIP)
            >= self.mg_boat_max_number(tr)
        {
            return Ok(());
        }
        let shore = self.ai_shore_tiles(tr, self.te_tribe);
        if shore.is_empty() {
            return Ok(());
        }
        let src = {
            let rng = self.random.as_mut().unwrap();
            match rng.rand_element(&shore) {
                Some(&s) => s,
                None => return Ok(()),
            }
        };
        // High-interest targets first (unowned or bot-owned).
        let dst = match self.ai_find_random_boat_target(tr, src, bordering_enemies, true)? {
            Some(d) => Some(d),
            None => self.ai_find_random_boat_target(tr, src, bordering_enemies, false)?,
        };
        let dst = match dst {
            Some(d) => d,
            None => return Ok(()),
        };
        let owner = self.mg_owner(tr, dst); // 222
        // `owner.isPlayer()` — 259 on a Player, untraced `false` on the
        // TerraNullius singleton (`None`); TS calls it twice (cap ternary,
        // too-weak gate), so re-trace at each point.
        let owner_is_player = owner.map(|o| self.pl_is_player(tr, o)).unwrap_or(false); // 259
        let cap = if owner_is_player {
            self.ai_troop_send_cap(tr)?
        } else {
            self.ai_troop_send_cap_for_expansion(tr)?
        };
        let troops = js_min(self.pl_troops(tr, self.te_tribe) / 5.0, cap);
        if troops < 1.0 {
            return Ok(());
        }
        if owner.map(|o| self.pl_is_player(tr, o)).unwrap_or(false) // 259 (second call)
            && self.ai_is_attack_too_weak(tr, troops, owner.unwrap())?
        {
            return Ok(());
        }
        self.mg_add_transport_ship_exec(tr, self.te_tribe, dst, troops);
        Ok(())
    }

    /// `findRandomBoatTarget(tile, borderingEnemies, highInterestOnly)`.
    fn ai_find_random_boat_target(
        &mut self,
        tr: &mut Vec<f64>,
        tile: f64,
        bordering_enemies: &[usize],
        high_interest_only: bool,
    ) -> Result<Option<f64>, String> {
        let x = self.mg_x(tr, tile); // 603
        let y = self.mg_y(tr, tile); // 604
        let mut unreachable: Vec<String> = Vec::new(); // insertion-ordered Set<PlayerID>
        for _ in 0..500 {
            let (rand_x, rand_y) = {
                let rng = self.random.as_mut().unwrap();
                (
                    rng.next_int(x - 150.0, x + 150.0) as f64,
                    rng.next_int(y - 150.0, y + 150.0) as f64,
                )
            };
            if !self.mg_is_valid_coord(tr, rand_x, rand_y) {
                // 616
                continue;
            }
            let rand_tile = self.mg_ref(tr, rand_x, rand_y); // 609
            if !self.mg_is_land(tr, rand_tile) {
                // 304
                continue;
            }
            if self.mg_is_impassable(tr, rand_tile) {
                // 617
                continue;
            }
            let owner = self.mg_owner(tr, rand_tile); // 222
            if owner == Some(self.te_tribe) {
                continue;
            }
            // Skip unreachable players. TS: `owner.isPlayer() && has(owner.id())`
            // — 259 fires for every owned tile, 231 only when it is a player.
            if let Some(o) = owner {
                if self.pl_is_player(tr, o) {
                    // 259
                    let oid = self.pl_id(tr, o); // 231
                    if unreachable.contains(&oid) {
                        continue;
                    }
                }
            }
            // Don't send boats to players with which we share a border.
            if let Some(o) = owner {
                if self.pl_is_player(tr, o) && bordering_enemies.contains(&o) {
                    // 259
                    continue;
                }
            }
            // FFA: skip stronger players. `isFFA() && isPlayer() && troops cmp`
            // — 520, then 259, then 418 x2, each gated by the previous arm.
            if self.ai_is_ffa(tr) {
                // 520
                if let Some(o) = owner {
                    if self.pl_is_player(tr, o) {
                        // 259
                        if self.pl_troops(tr, o) > self.pl_troops(tr, self.te_tribe) {
                            continue;
                        }
                    }
                }
            }
            // `!owner.isPlayer() || <arm>` — 259 always, arm only for players.
            let matches = match owner {
                None => true,
                Some(o) => {
                    if !self.pl_is_player(tr, o) {
                        // 259
                        true
                    } else if high_interest_only {
                        self.pl_type(tr, o) == PLAYER_TYPE_BOT // 251
                    } else {
                        !self.pl_is_friendly(tr, o, Some(self.te_tribe)) // 242
                    }
                }
            };
            if !matches {
                continue;
            }
            if !self.ai_can_build_transport_ship(tr, rand_tile)? {
                if let Some(o) = owner {
                    if self.pl_is_player(tr, o) {
                        // 259
                        let oid = self.pl_id(tr, o); // 231
                        unreachable.push(oid);
                    }
                }
                continue;
            }
            return Ok(Some(rand_tile));
        }
        Ok(None)
    }

    /// `canBuildTransportShip(game, player, tile)` — inlined from
    /// `game/TransportShipUtils.ts`.
    fn ai_can_build_transport_ship(
        &mut self,
        tr: &mut Vec<f64>,
        tile: f64,
    ) -> Result<bool, String> {
        if self.pl_unit_count(tr, self.te_tribe, UNIT_TRANSPORT_SHIP) // 823
            >= self.mg_boat_max_number(tr) // 801
        {
            return Ok(false);
        }
        let dst = self.mg_target_transport_tile(tr, self.te_tribe, tile); // 880
        let dst = match dst {
            Some(d) => d,
            None => return Ok(false),
        };
        let other = self.mg_owner(tr, tile); // 222
        if other == Some(self.te_tribe) {
            return Ok(false);
        }
        if let Some(o) = other {
            if self.pl_is_player(tr, o) && !self.pl_can_attack_player(tr, self.te_tribe, o) {
                // 259, 820
                return Ok(false);
            }
        }
        let shore = self.mg_closest_shore_by_water(tr, self.te_tribe, dst); // 961
        Ok(shore.is_some())
    }

    /// `donateTroops()`.
    fn ai_donate_troops(&mut self, tr: &mut Vec<f64>) -> Result<bool, String> {
        if self.mg_game_mode(tr) != GAME_MODE_TEAM {
            // 520
            return Ok(false);
        }
        if self.mg_game_type(tr) == GAME_TYPE_PUBLIC {
            // 214
            return Ok(false);
        }
        if !self.mg_donate_troops_cfg(tr) {
            // 959
            return Ok(false);
        }
        let w = self.mg_get_winner(tr); // 600
        if !matches!(w, JsVal::Null) {
            // TS: `getWinner() !== null` — the harness only ever returns
            // `null` or a Player object.
            return Ok(false);
        }
        let diff = self.mg_difficulty(tr); // 402
        match diff.as_str() {
            "Easy" => return Ok(false),
            "Medium" => {
                let rng = self.random.as_mut().unwrap();
                if !rng.chance(4.0) {
                    return Ok(false);
                }
            }
            "Hard" => {
                let rng = self.random.as_mut().unwrap();
                if !rng.chance(2.0) {
                    return Ok(false);
                }
            }
            "Impossible" => {}
            other => return Err(format!("Unexpected value: {other}")),
        }

        // Find teammates in combat. TS runs two independent `filter` passes,
        // so 250 traces for every player first, then 822/721 only for the
        // surviving teammates.
        let all_players = self.mg_players(tr); // 221
        let same_team: Vec<usize> = all_players
            .iter()
            .copied()
            .filter(|&p| self.pl_is_on_same_team(tr, self.te_tribe, Some(p))) // 250
            .collect();
        let teammates: Vec<usize> = same_team
            .into_iter()
            .filter(|&p| {
                !self.pl_incoming_attacks(tr, p).is_empty() // 822
                    || !self.pl_outgoing_attacks(tr, p).is_empty() // 721
            })
            .collect();
        if teammates.is_empty() {
            return Ok(false);
        }
        // Sort by troop percentage ascending.
        let mut with_pct: Vec<(usize, f64)> = teammates
            .iter()
            .map(|&t| {
                let max_t = self.mg_max_troops(tr, Rec::P(t)); // 401
                let pct = self.pl_troops(tr, t) / js_max(max_t, 1.0); // 418
                (t, pct)
            })
            .collect();
        with_pct.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut selected: Option<usize> = None;
        for &(t, _) in &with_pct {
            if self.pl_can_donate_troops(tr, self.te_tribe, Rec::P(t)) {
                // 413
                selected = Some(t);
                break;
            }
        }
        let selected = match selected {
            Some(s) => s,
            None => return Ok(false),
        };

        let max_troops = self.mg_max_troops(tr, Rec::P(self.te_tribe)); // 401
        let troops_to_keep = max_troops * self.te_reserve_ratio;
        let available = self.pl_troops(tr, self.te_tribe) - troops_to_keep; // 418
        if available < 1.0 {
            return Ok(false);
        }
        let sid = self.pl_id(tr, selected); // 231
        self.mg_add_donate_troops_exec(tr, self.te_tribe, &sid, Some(available));
        Ok(true)
    }

    // ---- G5k helpers for the map/facade bridge -------------------------------

    /// `mg.map().neighbors4(tile, buf)` — delegates to ElMap when available,
    /// otherwise to the scripted `mg.neighbors4` facade.
    fn mg_neighbors4_map(&mut self, tr: &mut Vec<f64>, tile: f64) -> Vec<f64> {
        if let Some(ref map) = self.el_map {
            let n = map.neighbors4(tile, &mut self.te_nbuf);
            return self.te_nbuf[..n].to_vec();
        }
        self.mg_neighbors4(tr, tile) // 804
    }

    /// `player.units()` — no-args variant (returns all unit tokens).
    fn pl_units_all(&mut self, tr: &mut Vec<f64>, tok: usize) -> Vec<usize> {
        let v = self.pl(tok, PL_UNITS, tr, 241.0, &ev_strs(&[]));
        as_tok_list(&v)
    }

    /// `alliance.onlyOneAgreedToExtend()` — scripted facade (260).
    fn al_only_one_agreed(&mut self, tr: &mut Vec<f64>, tok: usize) -> bool {
        let v = mk_take(&mut self.alliances[tok], AL_ONLY_ONE_AGREED, tr, 260.0, &[tok as f64]);
        as_bool(&v)
    }

    /// Resolve a neighbor token to a player token. Neighbors wrap player tokens
    /// in the harness (the neighbor token IS the player token for players).
    fn nb_player_tok(&mut self, _tr: &mut Vec<f64>, tok: usize) -> Option<usize> {
        if tok == TN_TOK {
            return None;
        }
        Some(tok)
    }
}

/// G5k: how a `Player | TerraNullius` target reached `AiAttackBehavior`. The
/// harness traces `isPlayer()` on the concrete object class: `Player` objects
/// (259), `Neighbor` objects returned by `player.nearby()` (257), or the
/// untraced `TerraNullius` singleton.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TargetVia {
    Pl,
    Nb,
    Tn,
}

/// G5k: the `AiAttackBehavior.getAttackStrategies` strategy ids (the TS
/// closures are modelled as an enum dispatched by `ai_run_strategy`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AiStrategy {
    Nuked,
    Bots,
    Retaliate,
    Assist,
    Betray,
    Hated,
    Weakest,
    VeryWeak,
    Juicy,
    Victim,
    Afk,
    Traitor,
    Island,
    Donate,
}

/// G5j: the `floodFillWithGen` include predicate kind.
enum FloodInclude {
    /// `visited[t] === borderGen` (the `calculateClusters` fill).
    BorderGen(u32),
    /// `mg.ownerID(t) === player.smallID()` (the `removeCluster` fill).
    OwnedBy,
}

/// `warshipState().state` as an owned string (absent → "").
fn g5j_ws_state(v: &JsVal) -> String {
    val_field(v, "state").map(as_str).unwrap_or_default()
}

/// JS `===` on two scripted values (numbers by value; `null === null`).
fn g5j_js_eq(a: &JsVal, b: &JsVal) -> bool {
    match (a, b) {
        (JsVal::Num(x), JsVal::Num(y)) => x == y,
        (JsVal::Null, JsVal::Null) | (JsVal::Undef, JsVal::Undef) => true,
        (JsVal::Bool(x), JsVal::Bool(y)) => x == y,
        (JsVal::Str(x), JsVal::Str(y)) => x == y,
        _ => false,
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
