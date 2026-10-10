//! OpenFront deterministic simulation core, ported from `src/core`.
//!
//! Scope of this crate today: the determinism layer and the data structures
//! that decide ordering everywhere else:
//!
//! * [`pseudo_random`] — `src/core/PseudoRandom.ts` (sfc32 seeded by splitmix32)
//! * [`detmath`] — `src/core/DetMath.ts` (cross-platform `exp/log/pow/atan2`)
//! * [`game_map`] — `src/core/game/GameMap.ts` (`GameMapImpl`: packed
//!   terrain/state typed arrays, neighbour orders, stack-BFS, `updateTile`)
//! * [`jsnum`] — JS numeric coercions (`ToInt32`/`ToUint32`/`ToUint16`) that
//!   typed-array element writes in the ported structures rely on
//! * [`pathfinding::priority_queue`] — `MinHeap` / `BucketQueue`, whose
//!   tie-breaking order feeds directly into A*
//! * [`pathfinding::a_star`] — the generic `AStar` engine (with a reference
//!   `GridAdapter`), whose expansion order is observable through every
//!   army/ship path the simulation takes
//! * [`pathfinding::rail`] — the `AStar.Rail` adapter over a `GameMap`-shaped
//!   terrain surface (`TerrainMap` mirrors `GameMapImpl`'s packed bytes)
//! * [`pathfinding::water`] — the self-contained `AStarWater` engine (inlined
//!   expansion, `MinHeap` f32 priorities, cross-product tie-breaker)
//! * [`tile_set`] — `src/core/game/TileSet.ts` (insertion-ordered tile
//!   container: open-addressed table, tombstones, `iterDepth`-gated deferred
//!   compaction, `Uint32Array` storage quirks)
//! * [`util`] — the deterministic core of `src/core/Util.ts` (wrapped
//!   Manhattan distance, `simpleHash`, first-minimum searches, `getMode`,
//!   bigint clamps, `sigmoid`, bounding-box helpers over a `GameMap`)
//! * [`team_assignment`] — `src/core/game/TeamAssignment.ts` (lobby team
//!   balancing: pins, strict clans, soft friend preference, nation shuffle,
//!   and `resolveTeamsList`)
//! * [`close_codes`] — `src/core/CloseCodes.ts` (`CloseCode` / `CloseReason`
//!   tables, `isTerminalClose` range test and `isCloseReason` set membership)
//! * [`anon_names`] — `src/core/AnonNames.ts` (`ANON_WORDS` bank +
//!   `anonWordName`: trunc/abs/floor JS-isms, property-access word lookup
//!   that spells out-of-range indices as `"undefined"`, and `String(round)`
//!   for the suffix)
//! * [`server_list`] — `src/core/ServerList.ts` (commit/site predicates,
//!   `commitsMatch`/`versionMatches` prefix rules, build-aware letter
//!   filtering and `pickServerForBuild` clamping, `ownLetterIn` host match,
//!   and the `/v/<commit>/` URL rewrites with hand-transcribed regexes and a
//!   JS-accurate `decodeURIComponent`; the zod schemas are not ported)
//! * [`pattern_decoder`] — `src/core/PatternDecoder.ts` (packed player-pattern
//!   header decode and `isPrimary` bit lookup; the injected `base64urlDecode`
//!   is not ported — the module takes the decoded bytes directly)
//! * [`doomsday_clock`] — `src/core/game/DoomsdayClock.ts` (wave-schedule
//!   threshold math: required share / HUD wave state / troop floor / convex
//!   drain curve / rot lattice + hashed noises / rot quota; integer-only,
//!   `Math.imul`→`wrapping_mul`, `>>>0`→`to_uint32`)
//! * [`exec_util`] — the pure-`GameMap` subset of `src/core/execution/Util.ts`
//!   (`computeNukeBlastCounts` insertion-ordered owner weights, `getSpawnTiles`
//!   over the centred radius-4 Euclidean stack-BFS, `closestTile` /
//!   `nearestTileDist` first-minimum strict `<`, `nearestTileDistCapped` with
//!   the `isTileSetLike` duck-test branch (Manhattan ring walk vs. clamped
//!   linear scan) and `closestTwoTiles` stable-by-column sort + two-pointer
//!   sweep; the three `Game`-facade functions are out of scope)
//! * [`water_manager`] — `src/core/game/WaterManager.ts` (water-nuke terrain
//!   fixup: pending-tile flush, ocean BFS propagation, crater-grouped
//!   magnitude BFS over direct terrain bytes, shoreline 2-ring recompute,
//!   minimap 2×2 downsample + mini ocean/magnitude BFSes, persistent shared
//!   `ConnectedComponents` incremental labeling, 20-tick throttled graph
//!   rebuild as a version bump — the graph/HPA machinery never crosses the
//!   WaterManager API surface)
//! * [`game_update_utils`] — `src/core/game/GameUpdateUtils.ts`
//!   (`diffPlayerUpdate` field-by-field diff with the `a === b` reference
//!   fast path modelled as a capture `refid`, `applyStateUpdate` in-place
//!   merge with the `Number()` / `Math.max(0,·)` / `.slice()` quirks, and
//!   `packAttackTroopDeltas` membership-gated troop quads)
//! * [`railroad`] — `src/core/game/Railroad.ts` (`getClosestTileIndex`
//!   squared-distance first-minimum over `game.x/y`-decoded tiles,
//!   `getOrientedRailroad` neighbor lookup + tile reversal, and `delete`'s
//!   observable destruction-update / `removeRailroad` call sequence; station
//!   identity rides in as capture refids — the `TrainStation` internals are a
//!   later port)
//! * [`railroad_spatial_grid`] — `src/core/game/RailroadSpatialGrid.ts`
//!   (`RailSpatialGrid`: `cellSize x cellSize` spatial buckets keyed by the
//!   `` `${cx}:${cy}` `` template string; register / unregister / query over
//!   `game.x/y`-decoded rail tiles, rails keyed by capture refid, insertion-
//!   ordered `Map`/`Set` semantics preserved)
//! * [`rail_network`] — `src/core/game/RailNetworkImpl.ts` (`RailNetworkImpl`
//!   station-network orchestrator + `createRailNetwork` factory: connect /
//!   snap-split / nearby-BFS clustering over the re-implemented
//!   `StationManagerImpl` / `TrainStation` / `Cluster` / `Railroad` /
//!   `RailSpatialGrid` semantics; the `Game` and `Unit` facades and the
//!   `pathService` are scripted mocks whose full call trace rides in the res
//!   stream, same pattern as `nation_utils` / `unit_grid`)
//! * [`unit_grid`] — `src/core/game/UnitGrid.ts` (the 100-pixel-cell 2-D
//!   spatial index: add / remove / updateUnitCell with the JS `Map`/`Set`
//!   insertion-order semantics (dup `add` keeps position, `delete` + re-`add`
//!   moves to the tail), `getCellsInRange`'s ceil/`js_min`/`js_max` window
//!   math, the array-vs-scalar `nearbyUnits` loop orders, the
//!   `isActive`/`isUnderConstruction`/`playerId` short-circuit chain and the
//!   0-row `grid[0].length` `TypeError`; the `GameMap` is the real ported
//!   `game_map` and the `Unit` facade is a scripted mock whose call trace
//!   rides in the res stream, same pattern as `nation_utils`)
//! * [`tile_traversal_scratch`] — `src/core/game/TileTraversalScratch.ts`
//!   (per-game generation-stamped `Uint32Array` visited buffer + reusable
//!   stack + `Int32Array` cluster map, `WeakMap`-cached by game refid; JS
//!   `ToIndex` allocation coercion, shrink-keeps / grow-reallocates reuse,
//!   `0xffffffff` bump wrap with `fill(0)`)
//! * [`event_bus`] — `src/core/EventBus.ts` (`on` / `off` / `emit` over a
//!   `Map<EventConstructor, Array<callback>>`; ctors / callbacks / events ride
//!   in as capture refids, `emit` is pinned as the ordered callback call trace
//!   and the `Map`-insertion-order dump)
//! * [`asset_urls`] — `src/core/AssetUrls.ts` (`normalizeAssetPath` /
//!   `encodeAssetPath` / `buildAssetUrl`: percent-decode + `.`/`..` segment
//!   guards, the any-scheme `isAbsoluteUrl` regex and the manifest / cdn-base
//!   join; the `window`/`globalThis` readers and the HTML rewrite are out of
//!   scope, and a JS-accurate `decodeURIComponent` / `encodeURIComponent`
//!   pair backs the escapes)
//! * [`maps_gen`] — `src/core/game/Maps.gen.ts` (the pure-data map table:
//!   the 127-member `GameMapType` string enum, the 16-entry
//!   `mapCategoryOrder`, and the 127 `MapInfo` records with their optional
//!   fields; the data region is generated by `tools/gen_maps_rs.mjs` and the
//!   `run_op` dump serialiser pins every field through the golden vectors)
//! * [`line`] — `src/core/utilities/Line.ts` (`DistanceBasedBezierCurve`:
//!   fixed-point De Casteljau subdivision for MIRV trajectories)
//! * [`veterancy`] — `src/core/game/Veterancy.ts` (`maxHealthWithVeterancy`:
//!   shared warship-veterancy math; integer-percent health bonus floored)
//! * [`motion_plans`] — `src/core/game/MotionPlans.ts` (`packMotionPlans` /
//!   `unpackMotionPlans`: two-pass `Uint32Array` wire format with strict
//!   `wordCount` validation; every field clamped via `to_uint32`)
//! * [`pathfinding::connected_components`] — `src/core/pathfinding/algorithms/ConnectedComponents.ts`
//!   (scan-line flood-fill component labelling over packed terrain bytes,
//!   `Uint8Array`→`Uint16Array` upgrade at 253 components, union-find alias
//!   table for incremental `addWaterTiles` merges, path-compressed `find`)
//! * [`terrain_search_map`] — `src/core/game/TerrainSearchMap.ts` (read-only
//!   search view over packed terrain bytes: header-decoded width/height,
//!   `Land`/`Shore`/`Water` classification, 8-way `neighbors` with JS
//!   out-of-range and fractional-index semantics)
//! * [`pathfinding::abstract_graph`] — `src/core/pathfinding/algorithms/AbstractGraph.ts`
//!   (`AbstractGraph` container + `AbstractGraphBuilder`: cluster-boundary
//!   gateway nodes from contiguous water "entrance" spans, intra-cluster edges
//!   from bounded grid-BFS distances (edge ids follow BFS find order),
//!   canonical `(lo,hi)` edge dedup keeping the cheaper cost, and the
//!   clean-cluster partial-rebuild cache keyed by `(minTile,maxTile)`)
//! * [`pathfinding::water_bounded`] — `src/core/pathfinding/algorithms/AStar.WaterBounded.ts`
//!   (`AStarWaterBounded`: the bounded-window sibling of `AStarWater` — all
//!   four stamp arrays indexed by a clamped window-local id, the
//!   `numLocalNodes` guard, the 3× shore penalty curve, and defaults
//!   `heuristicWeight ?? 3` / `maxIterations ?? 100_000`)
//! * [`pathfinding::abstract_graph_astar`] — `src/core/pathfinding/algorithms/AStar.AbstractGraph.ts`
//!   (`AbstractGraphAStar`: A* over the abstract graph — `Float32Array`
//!   g-scores (f32-rounded comparisons), multi-source origin tracking via
//!   `startNode`, goal/start checks *before* `queue.clear()`, single-start
//!   multi delegation, and the `buildPathFromGoal` out-of-range/over-long
//!   null guards; defaults `heuristicWeight ?? 1` / `maxIterations ?? 100_000`)
//! * [`game_ts`] — the runtime-value subset of `src/core/game/Game.ts` (the
//!   twelve enums as name/value tables, the five `unitTypeGroup` tables with
//!   their spread order, `isEnumValue` guards, `MESSAGE_TYPE_CATEGORIES` /
//!   `getMessageCategory`, `ColoredTeams`, `Cell`'s template-literal repr,
//!   `PlayerInfo`'s display name + ctor defaults, and the bigint bulk-cost
//!   math; the interfaces have no runtime and are not ported)
//! * [`nation_creation`] — `src/core/game/NationCreation.ts` (the 194-entry
//!   `NAME_TEMPLATES` with symbol markers as enum variants, the 236-word
//!   `NOUNS` bank with its intentional duplicates, `O_TO_OES` /
//!   `SPECIAL_PLURALS`, `pluralize` with JS UTF-16 indexing and the
//!   `undefined`-vowel quirk, `generateNationName` /
//!   `generateUniqueNationName`, `getCompactMapNationCount`, and
//!   `createRandomNations`' shuffle/nextID draw order; `createNationsForGame`
//!   is out of scope — its config branching rides on already-ported
//!   `Game.ts` constants)
//! * [`game_updates`] — `src/core/game/GameUpdates.ts` (`GameUpdateType`:
//!   the 24-member numeric wire-tag enum with explicit discriminants,
//!   name/value conversions; the update interfaces have no runtime and are
//!   not ported). Also covers `GameImpl.ts`'s module-tail
//!   `createGameUpdatesMap` — the traverse / `!isNaN(Number(key))` filter /
//!   write loop over the numeric enum's reverse-mapped runtime values; the
//!   `GameImpl` class body and `createGame` factory ride on the Config /
//!   PlayerImpl facades and are not ported)
//! * [`nation_emoji`] — `src/core/Util.ts` (`emojiTable` / 
//!   `flattenedEmojiTable`) + `src/core/execution/nation/NationEmojiBehavior.ts`
//!   (the 23 `EMOJI_*` id arrays as literals and `emojiId`'s `indexOf`
//!   semantics; the behavior class and `respondTo*` ride on `Game`/`Player`
//!   and are a later port)
//! * [`water_path_memo`] — `src/core/pathfinding/PathFinder.ts`
//!   (`WaterPathMemo`: LRU byte-budget memo in front of a scripted inner
//!   pathfinder — waterVersion entry check, insertion-order LRU with
//!   hit-reinsert, null=16-byte accounting, `Uint32Array` coercion on hits,
//!   f64 key arithmetic collisions; the rest of the file rides on `Game`)
//! * [`cosmetic_schemas`] — `src/core/CosmeticSchemas.ts` (the runtime-value
//!   subset: `EFFECT_TYPES` / `TRAIL_EFFECT_TYPES` / `NUKE_EXPLOSION_TYPES`,
//!   `isTrailEffect` / `isNukeExplosionEffect` / `effectTypeForSlot` /
//!   `effectMatchesSlot`, and `DefaultPattern`; the zod schema declarations
//!   are inert, and `findEffect` / `findEffectForSlot` ride on the nested
//!   catalog object and are not ported)
//! * [`schemas`] — the runtime-value subset of `src/core/Schemas.ts` (the
//!   enum option arrays `PublicGameType` / `ScheduledPublicGameType` /
//!   `LobbyAccent` / `ClientPlatform` / `ReportReason`, the lobby numeric and
//!   string constants, the `LogSeverity` table, the 58-key JSON-derived
//!   `QuickChatKeySchema` list, and the `isValidGameID` / renderable-name
//!   regex predicates with their probed `u`-flag semantics; the zod/zbin
//!   schema declarations are inert and the two event classes are pure
//!   field-storage — neither is ported)
//! * [`stats_schemas`] — `src/core/StatsSchemas.ts` (the runtime-value
//!   subset: `bombUnits` / `boatUnits` / `otherUnits`, the
//!   `unitTypeToBombUnit` / `unitTypeToOtherUnit` lookup tables, the numeric
//!   stats index constants, and the module-private `toBigInt` coercion; the
//!   zod/zbin schema declarations are inert, and `unitTypeToBoatUnit` is
//!   commented out upstream)
//! * [`stats_impl`] — `src/core/game/StatsImpl.ts` (the pure bigint stats
//!   accumulator: `_bigint`'s `Math.floor` + `BigInt(NaN/±Inf)` throw matrix,
//!   the `??=` / `while`-grow array primitives, the boats/bombs/units
//!   string-keyed objects with JS insertion order, the `conquest_by_type`
//!   lookup, the `recordTickSample` cross-multiplied drawdown update, the
//!   first-write-wins `killedBy` / `deathPosition` / `kills`, and the MIRV
//!   launch counter; the `Player` facade is a scripted mock whose call trace
//!   rides in the res stream, same pattern as `nation_utils`)
//! * [`api_schemas`] — the runtime-value subset of `src/core/ApiSchemas.ts`
//!   (the data constants `ADMIN_ROLES` / `PlayerStatsGameModes` / the two
//!   player game filter arrays, the eight `z.enum` option arrays, and the
//!   predicates `isAdminRole` / `isTemporaryUsername` / `isVerifiedUsername`
//!   / `isGrantedSubscription` with their probed no-`u`-flag regex and
//!   three-state provider semantics; the zod schema declarations are inert
//!   and the `TokenPayloadSchema` refine/transform callbacks are never
//!   invoked — neither is ported)
//! * [`terrain_map_loader`] — `src/core/game/TerrainMapLoader.ts` (the
//!   `loadTerrainMap` orchestration with `loadImages` fixed `false`: the
//!   module `loadedMaps` cache keyed by `` `${map}:${mapSize}` `` (hits skip
//!   `getMapData`, throw paths never cache), the Normal/Compact game-map and
//!   mini-map selection (the dead `mapSize === Normal` inner ternary), the
//!   Compact in-place nation coordinate scaling shared by reference with the
//!   cached results, the spawn-area `Math.max(1, floor(/2))` scaling, the
//!   layer placement / alpha validation with JS `Number`→string message
//!   interpolation, and `genTerrainFromBin`'s buffer-size check; the
//!   `loadLayerImages` / `createImageBitmap` branch is host-bound and not
//!   ported)
//! * [`nation_utils`] — `src/core/execution/nation/NationUtils.ts` (the
//!   `randTerritoryTileArray` / `randTerritoryTile` bounding-box sampling
//!   loop with the `numTilesOwned` 1..100 `randElement` fallback and the
//!   `findJuiciestTarget` normalize / strict-`>` best scan; the `Game` /
//!   `Player` / `PseudoRandom` facades are scripted mocks whose call traces
//!   ride in the res stream, same pattern as `water_path_memo`)
//! * [`execution_manager`] — `src/core/execution/ExecutionManager.ts` (the
//!   `Executor` intent dispatcher: the `createExecs` / `createExec`
//!   `playerByClientID` + `!player` warn/NoOp branch + the 24-case switch
//!   arg-extraction orders, the `spawnTribes` nations/map/filter pipeline
//!   into the `TribeSpawner` stub, the `PlayerSpawner` stub and the
//!   `nationExecutions` loop; the ctor pins the real `simpleHash(gameID) + 1`
//!   seed feeding the (never-used) `PseudoRandom`. The `XxxExecution` classes
//!   and the spawners are not ported (existing exclusion) and are stubbed as
//!   construction recorders whose events ride the res stream, same pattern as
//!   `nation_utils`)
//! * [`shared_water_cache`] — `src/core/execution/nation/SharedWaterCache.ts`
//!   (the nation-AI shared-water cache: the `TTL_TICKS` rebuild from
//!   `tick = -Infinity` with `this.tick = tick`, the strict-`===`
//!   tileVersion / waterVersion rescan check, the shore / neighbor border
//!   walk with the `comp !== null` lake add, the bot-skip pass-1 / partner
//!   `break` pass-2 over insertion-ordered `Map` / `Set` structures and the
//!   `OCEAN_SENTINEL`-first shared sets; the `Game` / `Player` facades are
//!   scripted mocks whose call traces ride in the res stream, same pattern
//!   as `nation_utils`)
//! * [`terra_nullius`] — `src/core/game/TerraNulliusImpl.ts` (the stateless
//!   neutral-player facade: four constant returns — `smallID` 0, `clientID`
//!   the literal string, `id` JS `null`, `isPlayer` the literal `false`)
//! * [`js_json`] — the shared JS `JSON.stringify` fidelity helper (the
//!   `JsonValue` serialisation domain: insertion-order objects, `undefined`
//!   key omission, array `null` fill, `NaN`/`±Infinity`→`null`, `-0`→`0`,
//!   control-character and lone-surrogate escaping) plus the `cp_` / `ia_` /
//!   `cv_` harness value codec carrying the absent / `undefined` / `null` /
//!   value tri-state
//! * [`vote_tally`] — `src/server/VoteTally.ts` (the IP-weighted
//!   `VoteRound<T>`: insertion-ordered string-keyed candidates over
//!   per-candidate `Set<string>` IP sets, idempotent same-IP adds, the
//!   strict-majority `result` / `resultAmong` first-winner scans)
//! * [`config`] — `src/core/configuration/Config.ts` (the game-rule config
//!   facade: `parseGameEnv`, the `Config` method table — every numeric /
//!   boolean getter, the `doomsdayClock` / `overtime` `??`-resolved dumps,
//!   `trainSaturation` / `tradeShipSaturation` / spawn-rate curves, the
//!   bigint gold paths with the V8 `BigInt()` throw matrix, `attackLogic`
//!   with `terrainAttackBase`, `maxTroops` / `troopIncreaseRate` /
//!   `startManpower` over scripted `Player` facades whose call order is
//!   traced, the `unitInfo` switch with its JS-`Map`-ordered cache and the
//!   `costWrapper` closure bodies exercised as standalone ops, and
//!   `dynamicSamRange` over a scripted `Unit.samLauncherState()`)
//! * [`unit_impl`] — `src/core/game/UnitImpl.ts` (the game unit entity: the
//!   full private field block, the ctor's `"x" in params` presence gates and
//!   type-driven state construction, `toUpdate`'s declaration-order key
//!   dump, `move` / `setOwner` / `modifyHealth` / `delete` with the
//!   `displayMessage` default-parameter and stats fan-out, the warship /
//!   transport / nuke state getters and `update*State` Partial merges with
//!   their rebuild key orders, the SAM missile queue, the trajectory
//!   accessors with the V8 `!.` TypeError messages, the veterancy cluster
//!   (the `addVeterancyProgress` while-loop re-reading the cap per level)
//!   and the level cluster over scripted `mg` / owner facades whose every
//!   call is pinned into a flat trace; the owner `_units` /
//!   `_myUnitsVersion` mutations ride as real harness state)
//! * [`alliance_impl`] — `src/core/game/AllianceImpl.ts` (the alliance
//!   entity: the ctor's `expiresAt_ = createdAt_ + allianceDuration()` facade
//!   read, `other` / `addExtensionRequest` / `agreedToExtend` over player
//!   reference-identity TOKENS, the unconditional `AllianceExtension` update
//!   dump, the `bothAgreedToExtend` / `onlyOneAgreedToExtend` flag predicates
//!   and `extend`'s ticks-before-config evaluation order over scripted `mg` /
//!   player facades (trace events 90-94, 92 pins `expireAlliance(this)`)
//! * [`alliance_request_impl`] — `src/core/game/AllianceRequestImpl.ts` (the
//!   alliance request state bag: the `"pending"` / `"accepted"` /
//!   `"rejected"` status string, `accept` / `reject` setting the status
//!   BEFORE the `game.acceptAllianceRequest(this)` /
//!   `rejectAllianceRequest(this)` facade (95/96 with the identity token),
//!   and `toUpdate`'s declaration-order key dump with the requestor-then-
//!   recipient `smallID()` facade order (94))
//! * [`attack_impl`] — `src/core/game/AttackImpl.ts` (the attack entity: the
//!   ctor's seven fields with the `_borderSize` NOT derived from the border
//!   `Set`, `setTroops`' `Math.max(0, ·)` over the raw JsVal, `delete`'s
//!   `isPlayer` gate (97) and the two identity `filter` passes over the
//!   players' REAL `_incomingAttacks` / `_outgoingAttacks` token arrays
//!   (token 0 = the attack under test — the `ui_` dumpOwner convention), the
//!   retreat flags, the insertion-ordered border `Set` add/remove gates and
//!   `clusteredPositions` / `clusterBorderTiles`' BFS clustering over the
//!   REAL ported `GameMap` (`mg.map()` facade 98; `forEachNeighborWithDiag`
//!   replays the ported neighbor order) with the centroid strict-`<`
//!   tie-break, the subtraction-comparator stable sort and the switch /
//!   filter / slice boundaries)
//! * [`config_patch`] — `src/server/ConfigPatch.ts` (`applyGameConfigPatch`'s
//!   `COPIED_KEYS` copy-if-`!== undefined` loop, the `NULLABLE_KEYS`
//!   `value ?? undefined` clear-to-undefined loop and the unconditional
//!   `target.hostCheats = patch.hostCheats` write, plus the
//!   `hostCheatsEnabled` four-field truth table; `GameConfig` rides as a
//!   plain insertion-ordered field map)
//! * [`intent_authorization`] — `src/server/IntentAuthorization.ts` (the
//!   pure `authorizeIntent` guard table: the admin-bot/public pre-switch
//!   guard, the per-intent guard orderings and exact `status` / `error`
//!   strings, the `GameType.Public` string-enum comparison, the
//!   `hostCheatsEnabled` and `allowedPublicIds?.length ?? 0` reads)
//! * [`consensus`] — `src/server/Consensus.ts` (`WinnerVote`'s
//!   `JSON.stringify(msg.winner ?? null)` candidate keys and the
//!   `decided`-storing `tally` / `tallyAmong`, plus `LiveStatsVote`'s
//!   turn-keyed pending rounds (`MAX_PENDING_ROUNDS` 20 oldest-prune, the
//!   `turn <= settled.turn` ignore, per-(turn, clientID) voter dedup and the
//!   on-settle delete-all-`t <= turn`) over the ported [`vote_tally`]
//! * [`listing_state`] — `src/server/ListingState.ts` (the public-listing
//!   presence: `setListed`'s duplicate-toggle no-op keeping the deadline,
//!   the scripted `Date.now()` (capture `globalThis.__LISTING_NOW` / port
//!   explicit `now` arg), `autoStartAt`'s featured/hosted constant flip and
//!   `setFeatured`'s boundary label sanitisation with the verbatim
//!   `opts.accent` pass-through)
//! * [`map_playlist`] — the deterministic layer of
//!   `src/server/MapPlaylist.ts` (the module tables, the
//!   `buildMapsList` / `playlistKey` / `addNextMapNonConsecutive` /
//!   `generateNewPlaylist` / `getNextMap` playlist chain with the scripted
//!   `Date.now()` seed (capture `globalThis.__MP_SEED`) and the real-TS log
//!   message pinning the attempt count, plus the pure team-count helpers;
//!   the `Math.random` orchestration methods and the async
//!   `getMapLandTiles` facades are a later port)
//! * [`name_visibility`] — `src/server/NameVisibility.ts` (`friendsLookup`
//!   and the per-viewer identity rules over a scripted `NameVisibilityView`
//!   facade whose `config()` / `clients()` / `teamIndex()` calls are traced
//!   in the res stream: the join-order anon slot, the team-seeded offset
//!   with JS number interpolation, the reveal / publicId grants, the
//!   `viewerTeam !== undefined` short-circuit before the second
//!   `teamIndex`, `startInfoFor`'s same-object non-anon return and
//!   index-aligned `real.players[i].clanTag` read, and `lobbyClients`'
//!   anon-vs-real key sets with the `|| undefined` spectator quirk)
//! * [`game_info_ranking`] — `src/client/components/baseComponents/ranking/`
//!   `GameInfoRanking.ts` (the `Ranking` over an `AnalyticsRecord`: the
//!   `hasPlayed` gate, the insertion-ordered `Record` summariser with the
//!   winner-block pass and the `BigInt(v ?? 0)` element coercion (a
//!   non-integral number throws — modelled as the harness error status), the
//!   twelve-branch `getScore` switch with no `default` and the
//!   subtraction-comparator `sortedBy` (NaN comparator → `+0`, stable))
//! * [`tutorial`] — the pure logic of `src/client/hud/Tutorial.ts` (the
//!   22-step `TUTORIAL_STEPS` predicate table as `fn(&TutorialContext)`
//!   closures and the `TutorialProgress` cursor: the first-post-spawn
//!   `countCtx` latch, the `doneTicks++` before the `< 15` linger return,
//!   the manual `acknowledge` gate and the applicable-step `position` /
//!   `total` counts; `TutorialHighlightEvent` is out of scope)
//! * [`preview_map`] — `src/client/render/preview/PreviewMap.ts` (the
//!   preview-map wrapper over the 1000x750 Australia terrain: the
//!   `!== mapW * mapH` throw with the exact template message, the
//!   undefined-only default parameters, the `& 0x80` land bit, the closed
//!   180-tile rail rectangle through the reused
//!   [`railroad_cache::compute_rail_tiles`] and the module-singleton
//!   `getPreviewRailLoop` latch (sparse `railroadState` dump))
//! * [`static_asset_cache`] — `src/server/StaticAssetCache.ts` (the
//!   `split("?", 1)[0]` query strip, the falsy `!urlPath` gate, the
//!   `/assets/` / `/_assets/` prefix test and the `setHeader` truthy gate
//!   over a facade trace)
//! * [`frame_upload`] — `src/client/render/frame/Upload.ts` (the
//!   `uploadFrameData` dispatch loop over a `(methodId, params…)` trace: the
//!   `changedTiles` truthy-vs-`length > 0` split (the empty array is truthy),
//!   the `trailDirtyRowMax >= 0` numeric gate inside the branch, the
//!   railroad / structures / relations truthy gates and the three
//!   independent event-length gates; the unconditional spiral / units /
//!   rings / telegraphs / names / clusters calls)
//! * [`lobby_card`] — the pure predicate subset of
//!   `src/client/components/LobbyCard.ts` (`viewerIsTrusted` with the strict
//!   `!== false` gate and the `.player.trustTier` TypeError domain,
//!   `canJoinTrustedLobby` with the `gameConfig?.trusted !== true` optional
//!   chain; `viewerIsSignedIn` delegates to [`account_identity`]; the lit /
//!   fetch-bound dialog and aspect-ratio cache are out of scope)
//! * [`sounds`] — the pure subset of `src/client/sound/Sounds.ts` (the
//!   31-entry `CUE_CATEGORY` declaration-order table, the four-key
//!   `ambienceUrls` set and the `categoryOf` ambience-first lookup; the
//!   `assetUrl`-bound `soundEffectUrls` and the `GameEvent` classes are out
//!   of scope)
//! * [`misc_pure`] — `isFfa` from `GameTypeLabels.ts` (the `GameMode.FFA`
//!   string gate and the mode-undefined + nullish-`playerTeams` fallback;
//!   `formatGameType` is intl-bound and out of scope) and `cardClass` from
//!   `InputCardStyles.ts` (the undefined-only default parameter and the
//!   verbatim template spacing)
//! * [`debug_gui`] — the `src/client/render/gl/debug/` GUI cluster: the
//!   `folder` factory (undefined-only default parameter, nullish-only
//!   `closed ?? true` fallback), the `toggle` / `slider` / `select` / `color`
//!   prop-factory lifecycles (strict-`!==` `isModified`, the captured-default
//!   `resetToDefault` write-back, the `Math.round(v*255).toString(16)
//!   .padStart(2,"0")` hex quirk surface) and `buildTree`'s pure literal
//!   debug tree; plus `LINES_PER_PLAYER` from `name-pass/Types.ts`
//! * [`base64_uuid`] — `src/core/Base64.ts` (`uuidToBase64url` /
//!   `base64urlToUuid`: the dash-stripping 16-slot `parseInt(_, 16)` pass with
//!   JS slice clamping and the `ToUint8` typed-array store, and the WHATWG
//!   forgiving-base64 decode the jose `base64url` codec delegates to — the
//!   throw domain is the `[1]` status token; the jose package itself is not in
//!   the capture graph)
//! * [`match_telemetry`] (S15 `mtl_` ops) — appends the `zeroCounters()`
//!   twelve-key declaration-order literal and the stateless
//!   `noopMatchTelemetryEmitter` (`emit` → `"dropped"`, `counters` → a fresh
//!   zeroed object, `stop` → `undefined`) from
//!   `src/server/telemetry/MatchTelemetry.ts`
//! * [`hotbar_icons`] — `src/client/hud/HotbarIcons.ts` (the nineteen
//!   `assetUrl("images/....svg")` load-time constants in declaration order,
//!   re-evaluated per scenario through the [`asset_urls`] build facade with a
//!   scripted manifest / CDN base)
//! * [`client_platform`] — `src/client/ClientPlatform.ts` (`clientPlatform()`'s
//!   three-gate short-circuit order: `isDesktopShell()` → `"steam"`, the
//!   `typeof window !== "undefined"` gate conjoined with
//!   `crazyGamesSDK.isOnCrazyGames()` → `"crazygames"`, else `"web"`; the
//!   host-bound SDK / shell predicates are facades whose call trace is dumped)
//! * [`effect_palette`] — `src/client/render/gl/utils/EffectPalette.ts`
//!   (`parseEffectColors` / `packEffectEntry` / `EFFECT_ENTRY_FLOATS`: the
//!   colord validity + `toRgb` surface is a scripted facade table observed
//!   from the real colord 2.9.3 package in V8, the `?? 0` intercept and the
//!   Float32Array f32 store pinned per quirk)
//! * [`news_markdown`] — `src/client/NewsMarkdown.ts` (`normalizeNewsMarkdown`'s
//!   four-`.replace` chain: the bold-header `gm` regex, the PR / compare URL
//!   `g` regexes with the `(?<!\()` lookbehind and trailing `\b` backtrack,
//!   and the `gim` @mention regex — all through a hand-written engine subset,
//!   not the `regex` crate)
//! * [`color_allocator`] — `src/client/theme/ColorAllocator.ts`
//!   (`ColorAllocator.assignColor` / `selectDistinctColorIndex`: the pool /
//!   fallback splice model, the 0 / >50 random-pick gate through [`util`]'s
//!   `simple_hash` + [`pseudo_random`], and the CIEDE2000 nearest-neighbor
//!   scan — every colord primitive replays the capture-facade id tables)
//! * [`theme_provider`] — `src/client/theme/ThemeProvider.ts`
//!   (`generateTeamColors` / `buildTeamPalettes` / `SettingsTheme` / the
//!   `themeProvider` singleton: the golden-angle LCH spread with the scripted
//!   `Math.sin` facade, the team / player color dispatch, the structure
//!   contrast loop with the runaway `console.warn` text, and the palette
//!   override gate — all over the same colord id tables as [`color_allocator`])
//!
//! Two rules govern every future port into this crate:
//!
//! 1. **Never use a platform transcendental in simulation logic.** Rust's
//!    `f64::exp` is free to differ from V8's in the last bit. Anything the
//!    simulation needs beyond [`detmath`] gets ported the same way.
//! 2. **Transcribe, do not simplify.** Association order, division vs.
//!    multiplication by a reciprocal, and rounding mode of each step all affect
//!    the result. A "cleaner" expression that changes a last bit is a desync
//!    waiting to happen.
//!
//! Parity is verified against vectors generated from the TS sources by
//! `rust/tools/gen_vectors.mjs`; rerun that script whenever a ported source
//! file changes.

pub mod account_identity;
pub mod affiliation_palette;
pub mod alliance_clusters;
pub mod alliance_impl;
pub mod alliance_request_impl;
pub mod anon_names;
pub mod api_schemas;
pub mod asset_urls;
pub mod atlas_data;
pub mod attack_impl;
pub mod attack_rings;
pub mod base64_uuid;
pub mod boot_interrupts;
pub mod camera;
pub mod censor;
pub mod client_platform;
pub mod close_codes;
pub mod color_allocator;
pub mod color_utils;
pub mod cluster_checkin;
pub mod config;
pub mod config_patch;
pub mod consensus;
pub mod cosmetic_schemas;
pub mod cosmetic_visibility;
pub mod creator_code;
pub mod debug_gui;
pub mod desync_detector;
pub mod detmath;
pub mod doomsday_clock;
pub mod effect_editor_state;
pub mod effect_palette;
pub mod event_bus;
pub mod exec_util;
pub mod execution_manager;
pub mod frame_upload;
pub mod fx_settings;
pub mod game_api_cors;
pub mod game_config_helpers;
pub mod game_info_ranking;
pub mod game_map;
pub mod game_mode_gate;
pub mod game_ts;
pub mod game_update_utils;
pub mod game_updates;
pub mod game_version;
pub mod gold_rate_tracker;
pub mod hotbar_icons;
pub mod intent_authorization;
pub mod join_verify;
pub mod js_fixed;
pub mod js_json;
pub mod jsnum;
pub mod line;
pub mod listing_state;
pub mod lobby_card;
pub mod map_layer_settings;
pub mod map_playlist;
pub mod maps_gen;
pub mod match_telemetry;
pub mod misc_pure;
pub mod motion_plans;
pub mod name_box_calculator;
pub mod name_visibility;
pub mod nation_creation;
pub mod nation_emoji;
pub mod nation_utils;
pub mod news_markdown;
pub mod nuke_telegraphs;
pub mod nuke_trajectory;
pub mod pathfinding;
pub mod pattern_decoder;
pub mod page_pin;
pub mod player_name;
pub mod player_profile_url;
pub mod player_status;
pub mod presence_group;
pub mod preview_map;
pub mod privilege;
pub mod pseudo_random;
pub mod rail_network;
pub mod ranked_checkin_gate;
pub mod railroad;
pub mod railroad_cache;
pub mod railroad_spatial_grid;
pub mod relation_matrix;
pub mod render_overrides;
pub mod render_settings;
pub mod renderer_consts;
pub mod replay_speed;
pub mod roster;
pub mod schemas;
pub mod server_list;
pub mod settings_utils;
pub mod shared_water_cache;
pub mod spiral_trails;
pub mod sounds;
pub mod stable_stringify;
pub mod static_asset_cache;
pub mod stats_constants;
pub mod station_manager;
pub mod stats_impl;
pub mod stats_schemas;
pub mod subscription_policy;
pub mod team_assignment;
pub mod terra_nullius;
pub mod terrain_map_loader;
pub mod terrain_row_spans;
pub mod terrain_search_map;
pub mod text_layout;
pub mod theme_provider;
pub mod tile_codec;
pub mod tile_set;
pub mod tile_traversal_scratch;
pub mod trail_manager;
pub mod train_station;
pub mod tribe_names;
pub mod tutorial;
pub mod unit_grid;
pub mod unit_impl;
pub mod unit_types;
pub mod util;
pub mod utils_format;
pub mod utils_nav;
pub mod veterancy;
pub mod versioned_replay;
pub mod vote_tally;
pub mod water_manager;
pub mod water_path_memo;

#[cfg(feature = "wasm-probe")]
mod wasm_probe;

pub use detmath::{atan2, exp, log, pow, pow2};
pub use pseudo_random::PseudoRandom;
