//! Bit-exact port of `src/core/game/PlayerImpl.ts` (G3c1 slice: the
//! identity / info / state / gold / troops / team / disconnect cluster).
//! The harness replicates every private field the slice touches;
//! construction and every method run as a `kind` of
//! [`PlayerHarness::run_op`] over the `js_json` codec (precedent
//! `unit_impl` / `attack_impl`). The `mg` (GameImpl) surface and the other
//! Players are scripted facades whose every call rides a flat trace, so the
//! facade call ORDER and the `===` reference-identity gates are compared,
//! not just the return values.
//!
//! Ported surface (TS line anchors): the ctor (191-201, class-field order:
//! `_troops = toInt(startTroops)` FIRST — a NaN startTroops throws before
//! facade 100 — then `_gold = mg.config().startingGold(playerInfo)`, then
//! `new PseudoRandom(simpleHash(playerInfo.id))` whose only observable
//! effect is the `str.length` TypeError on a null/undefined id); the
//! identity accessors (403-429, plain `playerInfo` field reads); `info` /
//! `isLobbyCreator` / `isAlive` / `hasSpawned` / `setSpawnTile` /
//! `spawnTile` (710-732); `isPlayer` (679); `numTilesOwned` / `tiles` /
//! `borderTiles` (569-579) over REAL ported `TileSet`s (the `ts_` vectors
//! prove the iteration order); the alliance gates `alliances` /
//! `isAlliedWith` / `allianceWith` (742-770); the team / gold / troops
//! cluster `team` / `isOnSameTeam` / `isFriendly` (1255-1280), `gold` /
//! `tradeGold` / `addTradeGold` / `trainGold` / `addTrainGold` /
//! `piracyGold` / `addPiracyGold` / `goldEarned` / `addGold` / `removeGold`
//! (1282-1339) and `troops` / `setTroops` / `addTroops` / `removeTroops`
//! (1341-1359); `tileChangeVersion` / `lastTileChange` (1788-1794) and the
//! disconnect cluster `isDisconnected` / `markDisconnected` /
//! `disconnectSnapshot` / `disconnectedAtTick` (1796-1826). G3c2 adds the
//! `expiredAlliances` (746) / `allies` (750) reads and the traitor / doomsday
//! cluster `isTraitor` / `getTraitorRemainingTicks` / `markTraitor` /
//! `betrayals` (851-911) and `inDoomsdayClock` / `doomsdayClockTicks` /
//! `enterDoomsdayClock` / `clearDoomsdayClock` / `markRotted` / `isDecaying`
//! (874-907) over the `markedTraitorTick` / `markedDoomsdayClockTick` /
//! `rottedAtTick` / `_betrayalCount` fields and the module-level
//! `DECAY_CUE_GRACE_TICKS = 30` (71). G3c3 adds the player-to-player
//! social surface: `incomingAllianceRequests` / `outgoingAllianceRequests`
//! (734-740, over the mg `allianceRequests` property array),
//! `allianceInfo` / `canSendAllianceRequest` (772-841), `breakAlliance` /
//! `removeAllAlliances` / `createAllianceRequest` (843-849, 913-918),
//! `relation` / `allRelationsSorted` / `updateRelation` / `decayRelations`
//! (920-970), `canTarget` / `target` / `targets` / `transitiveTargets`
//! (972-1006), `sendEmoji` / `outgoingEmojis` / `canSendEmoji` (1008-1050),
//! `canSendQuickChat` / `recordQuickChat` (1052-1065), the donations
//! cluster `canDonateGold` / `canDonateTroops` / `donateTroops` /
//! `donateGold` (1067-1165), `canDeleteUnit` / `recordDeleteUnit`
//! (1167-1176) and the embargo cluster `canEmbargoAll` /
//! `recordEmbargoAll` / `hasEmbargoAgainst` / `canTrade` / `getEmbargoes` /
//! `addEmbargo` / `stopEmbargo` / `endTemporaryEmbargo` /
//! `tradingPartners` (1178-1253) over the `relations` / `targets_` /
//! `outgoingEmojis_` / `outgoingQuickChats_` / `sentDonations` /
//! `embargoes` / `lastDeleteUnitTick` / `lastEmbargoAllTick` fields and
//! the scripted `pastOutgoingAllianceRequests` constant. G3c4 adds the
//! units-memo cluster `units` (4 overloads) / `unitsConstructed` /
//! `unitCount` / `unitsOwned` / `recordUnitConstructed` (430-528, over the
//! `myUnitsVersion`-keyed memos — the port always re-walks), the misc reads
//! `playerProfile` / `toString` / `hash` (1828-1853), `conquer` /
//! `relinquish` (685-709, the mg.owner self-token gate), `isImmune` /
//! `canAttackPlayer` (715-724, 1893-1902) and `outgoingAttacks` /
//! `incomingAttacks` (1876-1902), plus the `toUpdate` / `toFullUpdate`
//! wire (222-401) over `lastSentUpdate` and the direct attack-mutation ops
//! (116/117 setTroops / setRetreating on the attack mocks).
//!
//! G3c3 facade surface (event segment 109-130, all scripted):
//!
//! * 109-121 `mg.config().X()` — 109 allianceExtensionPromptOffset, 110
//!   disableAlliances (codec value, truthy gate), 111
//!   allianceRequestCooldown, 112 targetCooldown, 113 targetDuration, 114
//!   emojiMessageDuration, 115 emojiMessageCooldown, 116 quickChatCooldown,
//!   117 donateGold (codec value, `=== false` gate), 118 donateTroops
//!   (same), 119 donateCooldown, 120 deleteUnitCooldown, 121
//!   embargoAllCooldown. The f64 scripts trace `[code, ret]`, the codec
//!   scripts `[code, ...encVal(v)]`.
//! * 122 `mg.createAllianceRequest(this, recipient)` -> `[122, tok,
//!   ...encVal(ret)]` (the return is scripted: null or a request token).
//! * 123 `mg.breakAlliance(this, alliance)` -> `[123, alIdx]`.
//! * 124 `mg.removeAlliancesByPlayerSilently(this)` -> `[124]`.
//! * 125 `mg.target(this, other)` -> `[125, otherPid]`.
//! * 126 `mg.sendEmojiUpdate(msg)` -> `[126, ...encVal(msg)]` (key order
//!   message/senderID/recipientID/createdAt).
//! * 127 `other.isAlive()` -> `[127, pid, ret01]` (`!!v` normalised).
//! * 128 `other.addTroops(n)` -> `[128, pid, n]`.
//! * 129 `other.addGold(n)` -> `[129, pid, n]` (n is the removed bigint,
//!   crossing as a number).
//! * 130 `mg.players()` -> `[130, n, (tok)*]` (the scripted token list).
//! * G3c4 facade events: 131 `unit.hash()` -> `[131, hash]`; 132
//!   `mg.conquer(tile)` -> `[132, tile]`; 133 `mg.owner(tile)` ->
//!   `[133, tile, ...encVal(null | tok)]`; 134 `mg.relinquish(tile)` ->
//!   `[134, tile]`; 135 `mg.isSpawnImmunityActive()` / 136
//!   `mg.isNationSpawnImmunityActive()` -> `[135|136, rawV]` (the RAW f64
//!   crosses; the return is `v != 0 && !isNaN(v)`); 137 `other.isImmune()`
//!   -> `[137, pid, 01]`; 138 `mg.stats().getPlayerStats(this)` ->
//!   `[138, ...encVal(null | {killedBy?, deathPosition?})]`.
//! * `other.smallID()` / `other.id()` / `other.type()` /
//!   `other.hasEmbargoAgainst(this)` / `other.targets()` and the alliance
//!   mocks' `expiresAt()` / `agreedToExtend()` are untraced constant reads.
//!
//! Facade (mock) surface — the capture scripts these, every call traced
//! (G3c1 event segment 100-106):
//!
//! * 100 `mg.config().startingGold(playerInfo)` -> `[100, ...encVal(gold)]`
//!   (the scripted f64 crosses `BigInt(v)` FIRST — a non-integer script
//!   value throws before the trace push, pinned by construction order).
//! * 101 `mg.addUpdate(u)` -> `[101, ...encVal(u)]` (full codec dump of the
//!   BonusEvent update, key order type/player/tile/gold/troops).
//! * 102 `mg.ticks()` -> `[102, ret]`.
//! * 103 `mg.teamTilesOwned(team)` -> `[103, ...encVal(team), ret]`.
//! * 104 `mg.totalLandTiles()` -> `[104, ret]`.
//! * 105 `other.isDisconnected()` -> `[105, pid, ret01]` (the mock's
//!   `!!v` normalisation).
//! * 106 `other.team()` -> `[106, pid, ...encVal(v)]`.
//! * 107 `mg.stats().betray(this)` -> `[107, smallID]` (the capture pushes
//!   `this.smallID()`).
//! * 108 `mg.config().traitorDuration()` -> `[108, ret]`.
//! * Players cross as TOKENS (indices into the harness players table);
//!   `selfTok >= 0` puts the object under test into a table slot so the
//!   `other === this` identity gates fire naturally (the `ui_`/`ak_`
//!   identity model extended). `ColoredTeams.Bot === "Bot"` (Game.ts:110).
//! * `GameUpdateType.BonusEvent === 15` (zero-based declaration order).
//!
//! Faithfulness notes:
//!
//! * bigint domain: gold / troops are i64, crossing as f64 in the
//!   |v| <= 2^53 domain. `toInt` = ±Infinity clamp to MAX_SAFE then floor
//!   (only NaN throws); `BigInt(x)` over the raw `big()`-wrapped arg has NO
//!   clamp and NO floor (7.9 throws). The V8 throw matrix: non-integer
//!   number -> "The number X cannot be converted to a BigInt because it is
//!   not an integer"; `bigint OP non-bigint` -> "Cannot mix BigInt and
//!   other types, use explicit conversions"; cross-type RELATIONAL
//!   comparisons (`toRemove <= 0n`, `a < b` in minInt) convert the non-
//!   bigint side with ToNumber and NEVER throw — so `1n <= null` is true,
//!   `1n <= "x"` is false, and `minInt` can hand back the RAW non-bigint
//!   which then throws on the subtraction.
//! * `addGold`: `_gold +=` then `_goldEarned +=` (both before the tile
//!   gate); `if (tile)` is JS truthiness (0 / NaN / "" / null / undefined
//!   all skip the update); `gold: Number(toAdd)` converts the bigint back.
//! * `isOnSameTeam`: self -> false; `this.team() === null` short-circuits
//!   BEFORE the first `other.team()` facade; the Bot gate checks `this`
//!   side first (no facade); the final `===` fires the THIRD other-team
//!   facade — the call count is pinned by the trace (FIFO scripts repeat
//!   the last value when exhausted).
//! * `isFriendly(other, treatAFKFriendly = false)`: self -> true; the
//!   `other.isDisconnected() && !treatAFKFriendly` gate; then same-team OR
//!   allied (the alliance mocks' `requestor()`/`recipient()` are plain
//!   untraced identity reads).
//! * `markDisconnected`: the flag stores the RAW arg; on truthy + a null
//!   snapshot, `snapshot ?? {built}` — a non-nullish arg snapshot skips
//!   ALL facades; the built object evaluates left-to-right 102 -> team
//!   truthy gate -> 103 -> 104 -> `isAlive()` (untraced); on falsy the
//!   snapshot clears. `disconnectedAtTick` = `snapshot?.currentTick ?? null`.
//! * `_tiles` / `_borderTiles` are REAL ported `TileSet`s; mids 42/43/45/46
//!   call `add`/`delete` directly (bypassing `conquer`, so
//!   `tileChangeVersion` stays 0 — the capture pins that).
//! * A failed construct leaves the JS harness `pl === null`: every later
//!   method op throws "Cannot read properties of null (reading 'X')" with
//!   an empty trace (property access precedes any facade call).
//!
//! G3c4 adds the units / update / misc surface (mids 95-117, facade events
//! 131-138):
//!
//! * 95-99 `units()` overloads — the zero-arg and empty-array forms return
//!   ALL units, 1-3 string args and the array form filter by type. The
//!   `myUnitsMemo` / `myUnitCountMemo` / `myUnitsOwnedMemo` caches are keyed
//!   on `_myUnitsVersion`; every harness mutation bumps that version, so a
//!   cache hit and a recompute coincide and the port always recomputes.
//! * 100 `unitsConstructed(type)` over the `numUnitsConstructed` partial
//!   record (insertion-ordered upsert); 101 `unitCount` (Σ level); 102
//!   `unitsOwned` (Σ `uc ? 1 : level`); 103 `recordUnitConstructed` (the
//!   private method, called directly); 104 addUnit (a harness bookkeeping
//!   op: push the unit mock + `_myUnitsVersion++`); 105 bumpUnitsVersion.
//! * 106 `playerProfile` — `{relations: {smallID: relation}, alliances:
//!   [smallID]}` over the [`PlayerHarness::all_relation_pairs`] body (one
//!   127 facade per relations entry, ascending sort, `relationFromValue`).
//! * 107 `toString` (the trailing `]` quirk and the `numTileOwned`
//!   misspelling are pinned); 108 `hash` — `simpleHash(id) * (troops +
//!   numTilesOwned) + Σ unit.hash()` with a `[131, h]` facade per unit.
//! * 109 `conquer` -> `[132, tile]`; 110 `relinquish` — the `mg.owner(tile)
//!   !== this` gate (`[133, tile, null|tok]`, the self-token identity gate)
//!   throws before `[134, tile]`.
//! * 111 `isImmune` — HUMAN -> `[135]` spawn immunity, NATION -> `[136]`,
//!   else false; 112 `canAttackPlayer` — non-human attackers skip the
//!   `other.isImmune()` facade (`[137, pid, 0|1]`, the self slot runs the
//!   real body), then the isFriendly gates.
//! * 113/114 attack token tuples `[attTok, tgtTok, troops, id, retreating]`
//!   — 114 filters `attacker().isAlive()` (127 per attack, the self slot
//!   untraced); 116/117 are harness ops mutating the attack mocks' troops /
//!   retreating cells directly.
//! * 115 `toUpdate(statsOut?, attackTroopsOut?)` — the full snapshot is
//!   built in BOTH worlds: the typed `PlayerUpdate` the real
//!   `diff_player_update` / `pack_attack_troop_deltas` run over, and the
//!   wire `JsVal`s (the capture's normPU mirror: bigint gold -> Number, the
//!   embargo Set -> array, `undefined`-valued keys dropped). Array refids
//!   model JS identity for the pack gate: the EMPTY_* singletons share
//!   fixed ids (outgoing/incoming share the attack singleton), materialized
//!   arrays get fresh per-call ids. The result is
//!   `[0, ...enc(v), soLen, ...so, aoLen, ...ao]`; the diff `JsVal` is
//!   rebuilt from `diff_changed_indices` (the changed-field SET comes from
//!   the real comparator run; field values come from the typed full).
//! * Alliance mocks gain `id()` / `createdAt()` constants (legacy 5-field
//!   construct tuples default `id = i`, `createdAt = 100 + i`).
//!
//! G3c5 adds the build / upgrade / spawn family (mids 118-129, facade events
//! 139-157):
//!
//! * 118 `buildUnit(type, tile, params)` (1368-1397) — the 148-disabled
//!   short-circuit, then `149 unitInfo(type).cost(mg, this, 1)` crossing as
//!   bigint, troops removal, `139 mg.nextUnitID()` and the ctor param
//!   defaults; `b.toUpdate()` emits the full ctor-state shape (the warship
//!   spread appends `isInCombat` LAST via a 102 read, `transportShipState`
//!   only for Transport, the mock `shimUpdate` health crossing as
//!   `Number(Util.toInt(mh))`) before `101 addUpdate` + `140 addUnit`;
//!   `152 stats().unitBuild` + `157 bumpUnitsVersion` close it out.
//! * 119-121 the upgrade chain (1399-1477): `findUnitToUpgrade` /
//!   `canUpgradeUnit` / `upgradeUnit` over the unit mocks' ten fields
//!   (type/level/uc/hash/tile/targetTile/loaded/trainType/warshipState/
//!   owner), the `cost(mg, this, level)` bigint crossing.
//! * 122 `buildableUnits(tile, units)` (1479-1547) — per-type
//!   `{type, canBuild, canUpgrade, cost, upgradeCosts?}`; the upgradeCosts
//!   cumulate `cost(mg, this, n)` for n in 0..50 as bigints crossing as f64
//!   (`MAX_UPGRADE_AMOUNT = 50`); the 153/154 rail probes fire only on the
//!   build-new branch.
//! * 123 `canBuild(type, tile, validTiles)` (1549-1599) — `canSpawnUnitType`'s
//!   switch: the 144 `inSpawnPhase` gate, 143 `getWinner`, 148 disabled,
//!   then the per-type spawn helper.
//! * 124 `nukeSpawn` (1601-1651) — the untraced impassable / owner reads,
//!   143 + the untraced gameType, the Team-mode teammate-structure probe
//!   (147 nukeMagnitudes then 142 anyUnitNearby), the ready-silo filter and
//!   the manhattan-stable sort.
//! * 125-128 `portSpawn` / `warshipSpawn` / `landBasedUnitSpawn` /
//!   `landBasedStructureSpawn` (1653-1787) — `validStructureSpawnTiles`'s
//!   145 structureMinDist radius filter, the 146 radiusPortSpawn BFS.
//! * 129 `tradeShipSpawn` — the 141 nearbyUnits port scan.
//!
//! The G3c5 scenarios run against a MAP BLOCK: a real `GameMapImpl` +
//! `WaterManager` pair the mg map facade reads UNTRACED (map, width, height,
//! isValidRef, isWater, isLand, isShore, hasOwner, ownerID, bfs,
//! getWaterComponent, hasWaterComponent, manhattanDist,
//! euclideanDistSquared, isImpassable, isValidRef, waterGraphVersion,
//! miniMap / miniWaterHPA / miniWaterGraph). The ported
//! [`crate::spatial_query::SpatialQuery`] (bfsNearest, closestShore,
//! closestReachableShore with the (tileChangeVersion × waterVersion)-keyed
//! reachable-components memo, closestShoreByWater, refineStartTile) and
//! [`crate::transport_ship_utils`] (the boatMaxNumber 150 gate FIRST, then
//! targetTransportTile, the `other === player` / canAttackPlayer gates, then
//! closestShoreByWater; `bestTransportShipSpawn` at 1937-1939 rides
//! bestShoreDeploymentSource) therefore replay their REAL search order.
//!
//! * 139 `mg.nextUnitID()` -> `[139, ret]`; 140 `mg.addUnit(unit)` ->
//!   `[140, uid]`; 141 `mg.nearbyUnits(tile, radius, types)` -> scripted
//!   `[unitTok|~-1, tile, dist]` triples; 142 `mg.anyUnitNearby(...)` ->
//!   `[142, ret01]`; 143 `mg.getWinner()` -> `[143, ...encVal(v)]` (only
//!   `!== null` is observable); 144 `mg.inSpawnPhase()` -> `[144, ret01]`;
//!   145 `config().structureMinDist()`; 146 `config().radiusPortSpawn()`;
//!   147 `config().nukeMagnitudes()` -> `[147, mag, range]`; 148
//!   `config().isUnitDisabled(type)` -> `[148, ...encS(type), ret01]`; 149
//!   `unitInfo(type).cost(mg, this, n)` -> `[149, ...plEnc(n),
//!   ...plEnc(BigInt(v))]` (a non-integer script throws); 150
//!   `config().boatMaxNumber()`; 151 `unitInfo(type).maxHealth` (a scripted
//!   NaN crosses as `undefined`); 152 `mg.stats().unitBuild(owner, type)`;
//!   153 `railNetwork().overlappingRailroads(type, tile)` / 154
//!   `railNetwork().computeGhostRailPaths(type, tile)`; 155
//!   `config().samRange(1)` / 156 `config().samUpgradeDuration()` (the
//!   SAMLauncher ctor branch); 157 `mg.bumpUnitsVersion()` (side-effect only
//!   in this scope).

use crate::game_update_utils::{
    diff_changed_indices, diff_player_update, pack_attack_troop_deltas, Arr, AttackUpdate,
    AllianceView as TypedAllianceView, EmojiMessage as TypedEmoji, PlayerUpdate,
    ATTACK_DELTA_INCOMING, ATTACK_DELTA_OUTGOING,
};
use crate::game_map::GameMap;
use crate::js_json::{push_str, push_val, read_str, read_val, val_field, JsVal};
use crate::spatial_query::SpatialQuery;
use crate::tile_set::TileSet;

const MAX_SAFE: i64 = 9_007_199_254_740_991;
const MIX_ERR: &str = "Cannot mix BigInt and other types, use explicit conversions";
/// `DECAY_CUE_GRACE_TICKS` (PlayerImpl.ts:71).
const DECAY_CUE_GRACE_TICKS: f64 = 30.0;

/// FIFO script consumption with the capture's exhausted-repeat rule (the
/// `ui_` rule; event codes 100-106 are documented above).
fn take_f64(list: &[f64], i: &mut usize) -> f64 {
    let idx = (*i).min(list.len() - 1);
    *i += 1;
    list[idx]
}

/// Same rule for the team script; an exhausted-to-empty edge reads JS
/// `arr[-1]` -> `undefined` (never hit by the scenarios, kept total).
fn take_jsval(list: &[JsVal], i: &mut usize) -> JsVal {
    if list.is_empty() {
        *i += 1;
        return JsVal::Undef;
    }
    let idx = (*i).min(list.len() - 1);
    *i += 1;
    list[idx].clone()
}

/// JS `ToNumber` over the codec domain (the `unit_impl` helper).
fn to_num(v: &JsVal) -> f64 {
    match v {
        JsVal::Num(n) => *n,
        JsVal::Bool(b) => if *b { 1.0 } else { 0.0 },
        JsVal::Null => 0.0,
        JsVal::Str(s) => crate::game_config_helpers::js_number(s),
        JsVal::Absent | JsVal::Undef => f64::NAN,
        JsVal::Obj(_) | JsVal::Arr(_) => f64::NAN,
    }
}

/// JS truthiness over the codec domain (the `debug_gui` model).
fn js_truthy(v: &JsVal) -> bool {
    match v {
        JsVal::Absent | JsVal::Undef | JsVal::Null => false,
        JsVal::Bool(b) => *b,
        JsVal::Num(n) => !n.is_nan() && *n != 0.0,
        JsVal::Str(s) => !s.is_empty(),
        _ => true,
    }
}

/// `v == null` (the `??` nullish gate).
fn nullish(v: &JsVal) -> bool {
    matches!(v, JsVal::Absent | JsVal::Undef | JsVal::Null)
}

/// Arg cursor over the flat f64 token stream.
struct Cur<'a>(&'a [f64], usize);
impl<'a> Cur<'a> {
    fn f(&mut self) -> f64 {
        let v = self.0[self.1];
        self.1 += 1;
        v
    }
    fn u(&mut self) -> usize {
        self.f() as usize
    }
    fn has(&self) -> bool {
        self.1 < self.0.len()
    }
    fn list(&mut self) -> Vec<f64> {
        let n = self.u();
        (0..n).map(|_| self.f()).collect()
    }
    fn val(&mut self) -> JsVal {
        read_val(self.0, &mut self.1)
    }
    fn s(&mut self) -> String {
        read_str(self.0, &mut self.1)
    }
    /// `[n, (encVal)*n]` — the JsVal script blocks (da / dg / dt / car, emb).
    fn val_list(&mut self) -> Vec<JsVal> {
        let n = self.u();
        (0..n).map(|_| self.val()).collect()
    }
    /// The team script block `[n, (0 = null | 1 = string)*n]`.
    fn team_list(&mut self) -> Vec<JsVal> {
        let n = self.u();
        (0..n)
            .map(|_| {
                if self.f() == 0.0 {
                    JsVal::Null
                } else {
                    JsVal::Str(read_str(self.0, &mut self.1))
                }
            })
            .collect()
    }
}

/// `BigInt(x)` for a number `x` — `Err(V8 message)` on the throw matrix
/// (no clamp, no floor — the `big()`-wrapped gold args).
fn bigint_from_num(x: f64) -> Result<i64, String> {
    if x.is_nan() || x.is_infinite() || x != x.floor() {
        return Err(format!(
            "The number {} cannot be converted to a BigInt because it is not an integer",
            crate::js_fixed::js_to_string(x)
        ));
    }
    Ok(x as i64)
}

/// `toInt(num)` (Util.ts:397) — ±Infinity clamp BEFORE the floor; only NaN
/// throws.
fn to_int_big(x: f64) -> Result<i64, String> {
    if x == f64::INFINITY {
        return Ok(MAX_SAFE);
    }
    if x == f64::NEG_INFINITY {
        return Ok(-MAX_SAFE);
    }
    bigint_from_num(x.floor())
}

/// Success payload `[0, ...encVal(v)]`.
fn push_ok(out: &mut Vec<f64>, v: &JsVal) {
    out.push(0.0);
    push_val(out, v);
}

fn push_ok_num(out: &mut Vec<f64>, v: f64) {
    push_ok(out, &JsVal::Num(v));
}

fn push_ok_bool(out: &mut Vec<f64>, v: bool) {
    push_ok(out, &JsVal::Bool(v));
}

/// Throw payload `[1, ...encS(msg)]`.
fn push_err(out: &mut Vec<f64>, msg: &str) {
    out.push(1.0);
    push_str(out, msg);
}

/// The plEnc-encoded `validTiles` argument (`TileRef[] | null`): an array of
/// tile refs crosses as `Arr` of `Num`s; anything else is the `null` default.
fn decode_valid_tiles(v: &JsVal) -> Option<Vec<f64>> {
    match v {
        JsVal::Arr(items) => Some(
            items
                .iter()
                .filter_map(|it| match it {
                    JsVal::Num(t) => Some(*t),
                    _ => None,
                })
                .collect(),
        ),
        _ => None,
    }
}

/// The property name in the `pl === null` TypeError for each mid (mirrors
/// the capture's `pl.X(...)` access; mids 42/43 read `_tiles`, 45/46 read
/// `_borderTiles`).
fn mid_property(mid: u8) -> &'static str {
    match mid {
        0 => "smallID",
        1 => "name",
        2 => "displayName",
        3 => "clanTag",
        4 => "nationFlag",
        5 => "clientID",
        6 => "id",
        7 => "type",
        8 => "info",
        9 => "isLobbyCreator",
        10 => "isAlive",
        11 => "hasSpawned",
        12 => "setSpawnTile",
        13 => "spawnTile",
        14 => "troops",
        15 => "setTroops",
        16 => "addTroops",
        17 => "removeTroops",
        18 => "gold",
        19 => "tradeGold",
        20 => "addTradeGold",
        21 => "trainGold",
        22 => "addTrainGold",
        23 => "piracyGold",
        24 => "addPiracyGold",
        25 => "goldEarned",
        26 => "addGold",
        27 => "removeGold",
        28 => "tileChangeVersion",
        29 => "lastTileChange",
        30 => "isDisconnected",
        31 => "markDisconnected",
        32 => "disconnectSnapshot",
        33 => "disconnectedAtTick",
        34 => "team",
        35 => "isOnSameTeam",
        36 => "isFriendly",
        37 => "isAlliedWith",
        38 => "allianceWith",
        39 => "alliances",
        40 => "numTilesOwned",
        41 => "tiles",
        42 | 43 => "_tiles",
        44 => "borderTiles",
        45 | 46 => "_borderTiles",
        47 => "isPlayer",
        48 => "expiredAlliances",
        49 => "allies",
        50 => "isTraitor",
        51 => "getTraitorRemainingTicks",
        52 => "markTraitor",
        53 => "inDoomsdayClock",
        54 => "doomsdayClockTicks",
        55 => "enterDoomsdayClock",
        56 => "clearDoomsdayClock",
        57 => "markRotted",
        58 => "isDecaying",
        59 => "betrayals",
        // G3c3: the player-to-player social surface.
        60 => "incomingAllianceRequests",
        61 => "outgoingAllianceRequests",
        62 => "allianceInfo",
        63 => "canSendAllianceRequest",
        64 => "breakAlliance",
        65 => "removeAllAlliances",
        66 => "createAllianceRequest",
        67 => "relation",
        68 => "allRelationsSorted",
        69 => "updateRelation",
        70 => "decayRelations",
        71 => "canTarget",
        72 => "target",
        73 => "targets",
        74 => "transitiveTargets",
        75 => "sendEmoji",
        76 => "outgoingEmojis",
        77 => "canSendEmoji",
        78 => "canSendQuickChat",
        79 => "recordQuickChat",
        80 => "canDonateGold",
        81 => "canDonateTroops",
        82 => "donateTroops",
        83 => "donateGold",
        84 => "canDeleteUnit",
        85 => "recordDeleteUnit",
        86 => "canEmbargoAll",
        87 => "recordEmbargoAll",
        88 => "hasEmbargoAgainst",
        89 => "canTrade",
        90 => "getEmbargoes",
        91 => "addEmbargo",
        92 => "stopEmbargo",
        93 => "endTemporaryEmbargo",
        94 => "tradingPartners",
        // G3c4: the units / hash / toUpdate surface. 104/105/116/117 are
        // harness bookkeeping ops (no TS property access); the names only
        // shape the null-throw message parity, which never fires for them.
        95..=99 => "units",
        100 => "unitsConstructed",
        101 => "unitCount",
        102 => "unitsOwned",
        103 => "recordUnitConstructed",
        104 => "_units",
        105 => "_myUnitsVersion",
        106 => "playerProfile",
        107 => "toString",
        108 => "hash",
        109 => "conquer",
        110 => "relinquish",
        111 => "isImmune",
        112 => "canAttackPlayer",
        113 => "outgoingAttacks",
        114 => "incomingAttacks",
        115 => "toUpdate",
        116 => "troops",
        117 => "retreating",
        _ => "unknown",
    }
}

/// One scripted player mock: pid, the `team()` script (codec values —
/// `Null` / `Str`) and the `isDisconnected()` script (f64, `!!`-normalised
/// into the trace). G3c3 adds the constant `type()` string, the scripted
/// `isAlive()` FIFO (event 127), the constant embargo id list
/// (`hasEmbargoAgainst`, untraced) and the constant `targets()` token list
/// (untraced). G3c4 adds the scripted `isImmune()` FIFO (event 137).
#[derive(Clone, Debug)]
struct PlPlayer {
    pid: f64,
    teams: Vec<JsVal>,
    team_i: usize,
    discs: Vec<f64>,
    disc_i: usize,
    typ: String,
    alives: Vec<f64>,
    alive_i: usize,
    emb: Vec<JsVal>,
    tgts: Vec<usize>,
    immune: Vec<f64>,
    immune_i: usize,
}

/// The bigint-or-string accumulator domain. The gold fields are typed
/// `bigint` in TS, but JS `+=` follows the `+` operator: `bigint + string`
/// is STRING concatenation (not a mix TypeError), so an accumulator that
/// receives a string operand becomes a `Str` and stays one. Only the three
/// `add*Gold` methods ever receive non-number operands in the capture (the
/// `big()` wrapper turns numbers into `BigInt` — throwing on non-integers —
/// and passes everything else through raw).
#[derive(Clone, Debug, PartialEq)]
enum Acc {
    Big(i64),
    Str(String),
}

impl Acc {
    /// The codec value the getter returns: a bigint crosses as a number
    /// (`plEnc` → `akEnc(Number(v))`), a string as a string.
    fn to_jsval(&self) -> JsVal {
        match self {
            Acc::Big(b) => JsVal::Num(*b as f64),
            Acc::Str(s) => JsVal::Str(s.clone()),
        }
    }
}

/// The operand after the capture's `big()` wrapper: numbers are already
/// `BigInt`-converted (`Op::Big`, throwing at conversion), everything else
/// crosses raw.
enum Op {
    Big(i64),
    Str(String),
    Null,
    Bool(bool),
    Undef,
    Obj,
}

/// `Number(op)` / the ToNumber used by the cross-type relational comparisons.
fn op_to_num(o: &Op) -> f64 {
    match o {
        Op::Big(b) => *b as f64,
        Op::Str(s) => crate::game_config_helpers::js_number(s),
        Op::Null => 0.0,
        Op::Bool(b) => if *b { 1.0 } else { 0.0 },
        Op::Undef | Op::Obj => f64::NAN,
    }
}

/// `String(op)` for the string-concatenation branch of `+`.
fn op_to_string(o: &Op) -> String {
    match o {
        Op::Big(b) => b.to_string(),
        Op::Str(s) => s.clone(),
        Op::Null => "null".to_string(),
        Op::Bool(true) => "true".to_string(),
        Op::Bool(false) => "false".to_string(),
        Op::Undef => "undefined".to_string(),
        Op::Obj => "[object Object]".to_string(),
    }
}

/// `acc + op` over the JS `+` semantics (the `+=` body). A `Big` accumulator
/// plus a non-`Big`/non-`Str` operand throws the mix TypeError; a `Str`
/// accumulator concatenates whatever the operand stringifies to.
fn acc_plus(f: &Acc, o: &Op) -> Result<Acc, String> {
    match f {
        Acc::Big(a) => match o {
            Op::Big(b) => Ok(Acc::Big(a + b)),
            Op::Str(s) => Ok(Acc::Str(format!("{}{}", a, s))),
            _ => Err(MIX_ERR.to_string()),
        },
        Acc::Str(s) => Ok(Acc::Str(format!("{}{}", s, op_to_string(o)))),
    }
}

/// The capture's `big()` wrapper as an operand: a number crosses
/// `BigInt(v)` (throwing on a non-integer BEFORE the method runs, so the
/// field is untouched), every other codec type passes through raw.
fn big_operand(v: &JsVal) -> Result<Op, String> {
    match v {
        JsVal::Num(n) => bigint_from_num(*n).map(Op::Big),
        JsVal::Str(s) => Ok(Op::Str(s.clone())),
        JsVal::Null => Ok(Op::Null),
        JsVal::Bool(b) => Ok(Op::Bool(*b)),
        JsVal::Absent | JsVal::Undef => Ok(Op::Undef),
        JsVal::Obj(_) | JsVal::Arr(_) => Ok(Op::Obj),
    }
}

// ---- G3c3 helpers ----

/// JS `===` over the codec domain (`SameValueZero` semantics for the
/// scalar arm: NaN !== NaN, -0 === +0, cross-type always false). Obj/Arr
/// compare by identity in JS; the social cluster never ===-compares them,
/// so they simply never match here.
fn js_strict_eq(a: &JsVal, b: &JsVal) -> bool {
    match (a, b) {
        (JsVal::Num(x), JsVal::Num(y)) => x == y,
        (JsVal::Str(x), JsVal::Str(y)) => x == y,
        (JsVal::Bool(x), JsVal::Bool(y)) => x == y,
        (JsVal::Null, JsVal::Null) => true,
        (JsVal::Undef, JsVal::Undef) | (JsVal::Absent, JsVal::Absent) => true,
        _ => false,
    }
}

/// JS `String(v)` (the template-literal interpolation): Arr ToPrimitives
/// through `join(",")` (null/undefined elements -> "").
fn js_to_string_val(v: &JsVal) -> String {
    match v {
        JsVal::Str(s) => s.clone(),
        JsVal::Num(n) => crate::js_fixed::js_to_string(*n),
        JsVal::Bool(b) => {
            if *b {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        JsVal::Null => "null".to_string(),
        JsVal::Absent | JsVal::Undef => "undefined".to_string(),
        JsVal::Obj(_) => "[object Object]".to_string(),
        JsVal::Arr(items) => items
            .iter()
            .map(|x| match x {
                JsVal::Null | JsVal::Absent | JsVal::Undef => String::new(),
                other => js_to_string_val(other),
            })
            .collect::<Vec<_>>()
            .join(","),
    }
}

/// JS "array index" property test: a canonical numeric string (digits only,
/// no leading zeros) in `[0, 2^32-1)`. Plain objects re-order these keys
/// numerically ascending before the ordinary string keys, so anything an
/// `Object.fromEntries` output feeds into `Object.entries` observes that
/// order regardless of the insertion order.
fn js_array_index_key(k: &str) -> Option<u64> {
    if k.is_empty() || k.len() > 10 || !k.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if k.len() > 1 && k.starts_with('0') {
        return None;
    }
    let n: u64 = k.parse().ok()?;
    (n < u32::MAX as u64).then_some(n)
}

/// Re-order `(key, val)` entries the way a JS plain object stores them:
/// array-index keys numerically ascending first, then the rest in their
/// original (insertion) order.
fn js_integer_key_order(mut entries: Vec<(String, JsVal)>) -> Vec<(String, JsVal)> {
    let mut keyed: Vec<(Option<u64>, usize, (String, JsVal))> = entries
        .drain(..)
        .enumerate()
        .map(|(i, e)| (js_array_index_key(&e.0), i, e))
        .collect();
    keyed.sort_by(|a, b| match (a.0, b.0) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.1.cmp(&b.1),
    });
    keyed.into_iter().map(|(_, _, e)| e).collect()
}

/// JS `Math.sign`: NaN -> NaN, ±0 preserved.
fn js_sign(v: f64) -> f64 {
    if v.is_nan() {
        f64::NAN
    } else if v > 0.0 {
        1.0
    } else if v < 0.0 {
        -1.0
    } else {
        v
    }
}

/// `relationFromValue` (PlayerImpl.ts:928): the Relation enum
/// (Game.ts:327) — Hostile 0 / Distrustful 1 / Neutral 2 / Friendly 3; a
/// NaN value fails all three `<` gates and lands on Friendly.
fn relation_from_value(v: f64) -> f64 {
    if v < -50.0 {
        0.0
    } else if v < 0.0 {
        1.0
    } else if v < 50.0 {
        2.0
    } else {
        3.0
    }
}

/// A scripted f64 facade read: FIFO with the exhausted-repeat rule, then
/// `[code, ret]` on the trace (the 102/109/111-116/119-121 pattern).
fn script_f64(list: &[f64], i: &mut usize, tr: &mut Vec<f64>, code: f64) -> f64 {
    let v = take_f64(list, i);
    tr.push(code);
    tr.push(v);
    v
}

/// Same for a codec-value script (`[code, ...encVal(v)]` — 110/117/118).
fn script_val(list: &[JsVal], i: &mut usize, tr: &mut Vec<f64>, code: f64) -> JsVal {
    let v = take_jsval(list, i);
    tr.push(code);
    push_val(tr, &v);
    v
}

/// V8 `Array.prototype.sort` with a subtraction comparator: stable, and a
/// NaN comparator result is treated as `+0` (Equal) — the
/// `outgoingEmojis` / past-request descending ordering rule.
fn js_sort_desc(pairs: &mut [(usize, f64)]) {
    pairs.sort_by(|a, b| {
        let d = b.1 - a.1;
        if d.is_nan() || d == 0.0 {
            std::cmp::Ordering::Equal
        } else if d < 0.0 {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        }
    });
}

/// Same rule ascending (`a.relation - b.relation`) — `allRelationsSorted`.
fn js_sort_asc(pairs: &mut [(usize, f64)]) {
    pairs.sort_by(|a, b| {
        let d = a.1 - b.1;
        if d.is_nan() || d == 0.0 {
            std::cmp::Ordering::Equal
        } else if d < 0.0 {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        }
    });
}

/// One alliance mock: the (req, rec) token pair plus the G3c3
/// `expiresAt()` / `agreedToExtend()` constants (`req_agree` is
/// `agreedToExtend(requestor())`; `agreedToExtend(p)` is
/// `p === requestor() ? reqAgree : recAgree`). G3c4 adds the `id()` /
/// `createdAt()` constants toFullUpdate reads (legacy 5-field construct
/// tuples default `id = i`, `createdAt = 100 + i`).
#[derive(Clone, Copy, Debug)]
struct AllianceMock {
    req: usize,
    rec: usize,
    expires_at: f64,
    req_agree: bool,
    rec_agree: bool,
    id: f64,
    created_at: f64,
}

/// One unit mock (`type()` / `level()` / `isUnderConstruction()` untraced
/// constants, `hash()` traced as event 131). G3c5 widens it to the full
/// spawn-family surface: `id()` / `owner()` identity / `isActive()` /
/// `isInCooldown()` (the mock's scripted stand-in for the missile-queue
/// `length === level` test) / `tile()` / `isMarkedForDeletion()`.
#[derive(Clone, Debug)]
struct UnitMock {
    typ: String,
    id: f64,
    owner_sid: f64,
    level: f64,
    uc: bool,
    active: bool,
    cooldown: bool,
    tile: f64,
    mfd: bool,
    hash: f64,
    /// The mock's `_missileTimerQueue` — starts empty, grows only through the
    /// `increaseLevel` shim's Silo/SAM push. Never serialized (always empty
    /// at construct time).
    queue: Vec<f64>,
}

/// One attack mock (`attacker()` / `target()` untraced identity reads); the
/// troops / retreating cells are mutated by the 116/117 harness ops.
#[derive(Clone, Debug)]
struct AttackMock {
    att: usize,
    tgt: usize,
    troops: f64,
    id: String,
    ret: bool,
}

/// One `embargoes` entry (PlayerImpl.ts:1225-1229 key order); the map is
/// keyed by `other.id()` (a JsVal — mock pids cross as `Num`) and keeps JS
/// insertion order.
#[derive(Clone, Debug)]
struct Embargo {
    created_at: f64,
    is_temporary: JsVal,
    target: usize,
}

/// The `mg.owner(tile)` identity model for the G3c5 map face. In the TS
/// capture the map is a real `GameMapImpl`, so `ownerID(tile)` returns a real
/// numeric id; `PlayerImpl` only ever compares it against `this.smallID()` /
/// `0` or forwards it as a player reference (`owner.isPlayer()` on
/// TerraNullius tiles short-circuits to `false`). We mirror those three
/// identities without fabricating full player objects.
#[derive(Clone, Copy, Debug, PartialEq)]
enum OwnerRef {
    /// The harness player itself (`ownerID(tile) === smallID()`).
    SelfP,
    /// TerraNullius (`ownerID(tile) === 0`); `smallID()` returns 0,
    /// `isPlayer()` returns false.
    TN,
    /// Another scripted player mock (`owner.isPlayer()` → true,
    /// `smallID()` → the pid).
    Mock(f64),
}

impl OwnerRef {
    fn small_id(&self) -> f64 {
        match self {
            OwnerRef::SelfP | OwnerRef::TN => 0.0,
            OwnerRef::Mock(pid) => *pid,
        }
    }
    /// `owner.isPlayer()` — true for the harness player and other mocks,
    /// false for TerraNullius.
    fn is_player(&self) -> bool {
        !matches!(self, OwnerRef::TN)
    }
}

/// The EMPTY_* singleton refids (JS reference identity for the diff/pack
/// gates): `allies` / `targets` share `EMPTY_NUMBER_ARRAY`, `outgoingAttacks`
/// / `incomingAttacks` share `EMPTY_ATTACK_UPDATES`, every other collection
/// has its own. Materialized arrays get fresh per-call ids (a fresh JS
/// object per toFullUpdate call).
const REF_EMPTY_NUM: f64 = -1.0;
const REF_EMPTY_STR: f64 = -2.0;
const REF_EMPTY_ATK: f64 = -3.0;
const REF_EMPTY_ALV: f64 = -4.0;
const REF_EMPTY_EMO: f64 = -5.0;
const REF_EMPTY_EMB: f64 = -6.0;

/// The `diffPlayerUpdate` field order (the game_update_utils `D_*` mirror):
/// the diff JsVal key per emission index.
const DIFF_FIELD_NAMES: [&str; 33] = [
    "clientID",
    "name",
    "displayName",
    "clanTag",
    "nationFlag",
    "team",
    "smallID",
    "playerType",
    "isAlive",
    "isDisconnected",
    "killedBy",
    "deathPosition",
    "tradeGold",
    "trainGold",
    "piracyGold",
    "isTraitor",
    "traitorRemainingTicks",
    "inDoomsdayClock",
    "markedDoomsdayClockTick",
    "isDecaying",
    "hasSpawned",
    "spawnTile",
    "betrayals",
    "lastDeleteUnitTick",
    "isLobbyCreator",
    "allies",
    "targets",
    "outgoingAllianceRequests",
    "embargoes",
    "outgoingEmojis",
    "outgoingAttacks",
    "incomingAttacks",
    "alliances",
];

/// The full `PlayerUpdate` snapshot in both worlds (mid 115): the typed
/// value the real `diff_player_update` / `pack_attack_troop_deltas` run
/// over, and the per-field wire encodings — index = the `diffPlayerUpdate`
/// emission order, `None` models a field whose JS value is `undefined`
/// (the capture's normPU drops the key). `full` is the first-call wire
/// form: the whole object in toFullUpdate's literal key order.
struct FullParts {
    typed: PlayerUpdate,
    fields: Vec<Option<JsVal>>,
    full: JsVal,
}

/// A `playerInfo` field on the wire: `undefined`/absent -> dropped by the
/// capture's normPU, everything else crosses as-is.
fn pi_wire(v: &JsVal) -> Option<JsVal> {
    match v {
        JsVal::Undef | JsVal::Absent => None,
        other => Some(other.clone()),
    }
}

/// The typed `Opt3<String>` for a playerInfo field.
fn pi_opt3(v: &JsVal) -> Option<Option<String>> {
    match v {
        JsVal::Str(s) => Some(Some(s.clone())),
        JsVal::Null => Some(None),
        _ => None,
    }
}

/// The typed `Opt3<bool>` for a playerInfo field (`isLobbyCreator`).
fn pi_opt3_bool(v: &JsVal) -> Option<Option<bool>> {
    match v {
        JsVal::Bool(b) => Some(Some(*b)),
        JsVal::Null => Some(None),
        _ => None,
    }
}

/// `Number(bigint-or-string)` for the typed gold fields (the diff never
/// reads them and the quint runs `Number(...)`).
fn acc_num(a: &Acc) -> f64 {
    match a {
        Acc::Big(b) => *b as f64,
        Acc::Str(s) => crate::game_config_helpers::js_number(s),
    }
}

/// The typed `EmojiMessage` for a stored outgoing-emoji object (the mid 75
/// construction shape; `recipientID: "AllPlayers"` -> `None`).
fn emoji_typed(e: &JsVal) -> TypedEmoji {
    let g = |k: &str| val_field(e, k).cloned().unwrap_or(JsVal::Undef);
    TypedEmoji {
        message: match g("message") {
            JsVal::Str(s) => s,
            _ => String::new(),
        },
        sender_id: to_num(&g("senderID")),
        recipient_id: match g("recipientID") {
            JsVal::Num(n) => Some(n),
            _ => None,
        },
        created_at: to_num(&g("createdAt")),
    }
}

/// The typed `outgoingAllianceRequests` / alliance-view `other` item: `id()`
/// is the pi id (string) for the self slot and the mock pid (number)
/// otherwise. The typed `Arr<String>` needs an injective mapping — a string
/// `"5"` must never equal the number 5, exactly like JS `!==` — so non-string
/// values get a suffix no scenario id uses. The typed items only feed
/// `stringArrayEqual` (the wire crosses the raw JsVal), so any injective
/// encoding keeps the comparator bit-exact.
fn oar_typed(v: &JsVal) -> String {
    match v {
        JsVal::Str(s) => s.clone(),
        other => format!("{}\u{0}", js_to_string_val(other)),
    }
}

/// The `f64` behind an `Option<Option<f64>>` typed field (the wire keeps the
/// three-state form; the quint pushes the value).
fn opt3_num(v: &Option<Option<f64>>) -> f64 {
    match v {
        Some(Some(n)) => *n,
        _ => f64::NAN,
    }
}

/// The `PlayerImpl` field block the G3c1 slice touches, replicated 1:1.
struct Player {
    small_id: f64,
    player_info: JsVal,
    team: JsVal,
    troops: i64,
    gold: Acc,
    trade_gold: Acc,
    train_gold: Acc,
    piracy_gold: Acc,
    gold_earned: Acc,
    tiles: TileSet,
    border_tiles: TileSet,
    alliances: Vec<AllianceMock>,
    expired_alliances: Vec<(usize, usize)>,
    marked_traitor_tick: f64,
    marked_doomsday_clock_tick: f64,
    rotted_at_tick: f64,
    betrayal_count: f64,
    spawn_tile: JsVal,
    is_disconnected: JsVal,
    disconnect_snapshot: JsVal,
    tile_change_version: f64,
    last_tile_change: f64,
    // G3c3: the player-to-player social surface. `relations` /
    // `quick_chats` / `embargoes` are JS Maps (insertion-ordered, set
    // updates in place); `targets_` is `{tick, targetTok}`;
    // `outgoing_emojis` stores the full EmojiMessage objects (the
    // message/senderID/recipientID/createdAt key order is observable).
    relations: Vec<(usize, f64)>,
    targets_: Vec<(f64, usize)>,
    outgoing_emojis: Vec<JsVal>,
    quick_chats: Vec<(f64, f64)>,
    sent_donations: Vec<(usize, f64)>,
    embargoes: Vec<(JsVal, Embargo)>,
    last_delete_unit_tick: f64,
    last_embargo_all_tick: f64,
    // G3c4: the units / update surface. `num_units_constructed` is the JS
    // partial record (insertion-ordered upsert); `last_sent` is
    // `lastSentUpdate` (the previous FULL snapshot, undefined on the first
    // toUpdate). The per-type memos are recomputed (see the module doc).
    units: Vec<UnitMock>,
    my_units_version: f64,
    num_units_constructed: Vec<(String, f64)>,
    out_attacks: Vec<AttackMock>,
    in_attacks: Vec<AttackMock>,
    last_sent: Option<PlayerUpdate>,
    /// The next fresh refid for materialized toFullUpdate arrays —
    /// monotonic across calls (each is a new JS object per call).
    next_refid: f64,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            small_id: 0.0,
            player_info: JsVal::Absent,
            team: JsVal::Null,
            troops: 0,
            gold: Acc::Big(0),
            trade_gold: Acc::Big(0),
            train_gold: Acc::Big(0),
            piracy_gold: Acc::Big(0),
            gold_earned: Acc::Big(0),
            tiles: TileSet::new(None),
            border_tiles: TileSet::new(None),
            alliances: Vec::new(),
            expired_alliances: Vec::new(),
            marked_traitor_tick: -1.0,
            marked_doomsday_clock_tick: -1.0,
            rotted_at_tick: -1.0,
            betrayal_count: 0.0,
            spawn_tile: JsVal::Undef,
            is_disconnected: JsVal::Bool(false),
            disconnect_snapshot: JsVal::Null,
            tile_change_version: 0.0,
            last_tile_change: 0.0,
            relations: Vec::new(),
            targets_: Vec::new(),
            outgoing_emojis: Vec::new(),
            quick_chats: Vec::new(),
            sent_donations: Vec::new(),
            embargoes: Vec::new(),
            last_delete_unit_tick: -1.0,
            last_embargo_all_tick: -1.0,
            units: Vec::new(),
            my_units_version: 0.0,
            num_units_constructed: Vec::new(),
            out_attacks: Vec::new(),
            in_attacks: Vec::new(),
            last_sent: None,
            next_refid: 1.0,
        }
    }
}

#[derive(Default)]
pub struct PlayerHarness {
    constructed: bool,
    p: Player,
    players: Vec<PlPlayer>,
    self_tok: Option<usize>,
    sg: Vec<f64>,
    sg_i: usize,
    ticks: Vec<f64>,
    ticks_i: usize,
    team_tiles: Vec<f64>,
    team_tiles_i: usize,
    total_land: Vec<f64>,
    total_land_i: usize,
    traitor_duration: Vec<f64>,
    traitor_duration_i: usize,
    // G3c3 mg surface: `allianceRequests` (property array of (req, rec)
    // token pairs), `players()` (scripted token list), the config FIFO
    // scripts and the `createAllianceRequest` return FIFO.
    alliance_requests: Vec<(usize, usize)>,
    past_outgoing: Vec<(usize, f64)>,
    players_list: Vec<usize>,
    off: Vec<f64>,
    off_i: usize,
    da: Vec<JsVal>,
    da_i: usize,
    arc: Vec<f64>,
    arc_i: usize,
    tc: Vec<f64>,
    tc_i: usize,
    tdur: Vec<f64>,
    tdur_i: usize,
    emd: Vec<f64>,
    emd_i: usize,
    emc: Vec<f64>,
    emc_i: usize,
    qc: Vec<f64>,
    qc_i: usize,
    dg: Vec<JsVal>,
    dg_i: usize,
    dt: Vec<JsVal>,
    dt_i: usize,
    dc: Vec<f64>,
    dc_i: usize,
    duc: Vec<f64>,
    duc_i: usize,
    eac: Vec<f64>,
    eac_i: usize,
    car: Vec<JsVal>,
    car_i: usize,
    // G3c4: getPlayerStats (138), mg.owner (133 — `Null` | `Num(tok)`), the
    // spawn-immunity flags (135/136).
    gs: Vec<JsVal>,
    gs_i: usize,
    owner: Vec<JsVal>,
    owner_i: usize,
    sia: Vec<f64>,
    sia_i: usize,
    nsia: Vec<f64>,
    nsia_i: usize,
    // G3c5: the real-map block. `sq` owns the WaterManager (map + mini map)
    // and the per-game traversal scratch; `None` when the scenario injects no
    // map (the mg map facade then panics — the capture would too, its mock
    // `map()` being undefined).
    sq: Option<SpatialQuery>,
    // G3c5 config/mg scripts (FIFO). `gwin` is getWinner (Null | Num(token));
    // `nu` the nearbyUnits per-call scripts; `orail` / `gravel` the rail
    // network returns (JsVals crossing verbatim); `uimh` uses NaN as the
    // `undefined` marker (maxHealth is optional in UnitInfo).
    upg: Vec<(String, bool)>,
    gt: String,
    gmd: String,
    gwin: Vec<JsVal>,
    gwin_i: usize,
    isp: Vec<f64>,
    isp_i: usize,
    smd: Vec<f64>,
    smd_i: usize,
    rps: Vec<f64>,
    rps_i: usize,
    nmag: Vec<(f64, f64)>,
    nmag_i: usize,
    bmn: Vec<f64>,
    bmn_i: usize,
    nuid: Vec<f64>,
    nuid_i: usize,
    nu: Vec<NuCall>,
    nu_i: usize,
    anyn: Vec<f64>,
    anyn_i: usize,
    idis: Vec<f64>,
    idis_i: usize,
    uicost: Vec<f64>,
    uicost_i: usize,
    uimh: Vec<f64>,
    uimh_i: usize,
    sam_r: Vec<f64>,
    sam_r_i: usize,
    sam_d: Vec<f64>,
    sam_d_i: usize,
    orail: Vec<JsVal>,
    orail_i: usize,
    gravel: Vec<JsVal>,
    gravel_i: usize,
}

/// One scripted `mg.nearbyUnits(tile, radius, types, ...)` call: the call
/// identity (tile, radius, the type set) is re-pinned by the 141 event, the
/// entries reference unit mocks by `id()`.
#[derive(Clone, Debug)]
struct NuCall {
    tile: f64,
    radius: f64,
    types: Vec<String>,
    entries: Vec<(f64, f64, f64)>,
}

impl PlayerHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    // --- facade plumbing ---

    fn mg_starting_gold(&mut self, tr: &mut Vec<f64>) -> Result<i64, String> {
        let v = take_f64(&self.sg, &mut self.sg_i);
        // JS: `BigInt(v)` throws BEFORE the trace push.
        let b = bigint_from_num(v)?;
        tr.push(100.0);
        push_val(tr, &JsVal::Num(b as f64));
        Ok(b)
    }

    fn mg_ticks(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = take_f64(&self.ticks, &mut self.ticks_i);
        tr.push(102.0);
        tr.push(v);
        v
    }

    fn mg_team_tiles_owned(&mut self, tr: &mut Vec<f64>, team: &JsVal) -> f64 {
        let v = take_f64(&self.team_tiles, &mut self.team_tiles_i);
        tr.push(103.0);
        push_val(tr, team);
        tr.push(v);
        v
    }

    fn mg_total_land(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = take_f64(&self.total_land, &mut self.total_land_i);
        tr.push(104.0);
        tr.push(v);
        v
    }

    fn mg_add_update(&mut self, tr: &mut Vec<f64>, u: &JsVal) {
        tr.push(101.0);
        push_val(tr, u);
    }

    fn mg_traitor_duration(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = take_f64(&self.traitor_duration, &mut self.traitor_duration_i);
        tr.push(108.0);
        tr.push(v);
        v
    }

    /// `mg.stats().betray(this)` — the capture pushes `this.smallID()` into
    /// the trace (event 107).
    fn mg_betray(&mut self, tr: &mut Vec<f64>) {
        tr.push(107.0);
        tr.push(self.p.small_id);
    }

    /// `other.team()` — the mock facade (106); the self slot reads the REAL
    /// field (untraced), mirroring `mocks[selfTok] = pl`.
    fn other_team(&mut self, tr: &mut Vec<f64>, tok: usize) -> Result<JsVal, String> {
        if self.self_tok == Some(tok) {
            return Ok(self.p.team.clone());
        }
        let p = self
            .players
            .get_mut(tok)
            .ok_or_else(|| "Cannot read properties of undefined (reading 'team')".to_string())?;
        let v = take_jsval(&p.teams, &mut p.team_i);
        tr.push(106.0);
        tr.push(p.pid);
        push_val(tr, &v);
        Ok(v)
    }

    /// `other.isDisconnected()` — the mock facade (105) with the `!!v`
    /// normalisation the capture pushes.
    fn other_is_disconnected(&mut self, tr: &mut Vec<f64>, tok: usize) -> Result<bool, String> {
        if self.self_tok == Some(tok) {
            return Ok(js_truthy(&self.p.is_disconnected));
        }
        let p = self.players.get_mut(tok).ok_or_else(|| {
            "Cannot read properties of undefined (reading 'isDisconnected')".to_string()
        })?;
        let v = take_f64(&p.discs, &mut p.disc_i);
        let b = v != 0.0 && !v.is_nan();
        tr.push(105.0);
        tr.push(p.pid);
        tr.push(if b { 1.0 } else { 0.0 });
        Ok(b)
    }

    // --- G3c3 facade plumbing ---

    /// `other.isAlive()` — the mock facade (127) with the `!!v`
    /// normalisation; the self slot reads the REAL tile count (untraced),
    /// mirroring `mocks[selfTok] = pl`.
    fn other_is_alive(&mut self, tr: &mut Vec<f64>, tok: usize) -> Result<bool, String> {
        if self.self_tok == Some(tok) {
            return Ok(self.is_alive());
        }
        let p = self.players.get_mut(tok).ok_or_else(|| {
            "Cannot read properties of undefined (reading 'isAlive')".to_string()
        })?;
        let v = take_f64(&p.alives, &mut p.alive_i);
        let b = v != 0.0 && !v.is_nan();
        tr.push(127.0);
        tr.push(p.pid);
        tr.push(if b { 1.0 } else { 0.0 });
        Ok(b)
    }

    /// `other.smallID()` — the mock's constant pid (untraced); the self
    /// slot reads the real field.
    fn other_small_id(&self, tok: usize) -> Result<f64, String> {
        if self.self_tok == Some(tok) {
            return Ok(self.p.small_id);
        }
        self.players
            .get(tok)
            .map(|p| p.pid)
            .ok_or_else(|| "Cannot read properties of undefined (reading 'smallID')".to_string())
    }

    /// `other.id()` — the mock's constant pid as `Num` (untraced); the self
    /// slot reads `playerInfo.id`.
    fn other_id(&self, tok: usize) -> JsVal {
        if self.self_tok == Some(tok) {
            return self.pi_field("id");
        }
        match self.players.get(tok) {
            Some(p) => JsVal::Num(p.pid),
            None => JsVal::Undef,
        }
    }

    /// `other.type()` — the mock's constant type string (untraced); the self
    /// slot reads `playerInfo.playerType`.
    fn other_type(&self, tok: usize) -> JsVal {
        if self.self_tok == Some(tok) {
            return self.pi_field("playerType");
        }
        match self.players.get(tok) {
            Some(p) => JsVal::Str(p.typ.clone()),
            None => JsVal::Undef,
        }
    }

    /// `other.hasEmbargoAgainst(this)` — the mock's constant embargo id
    /// list (untraced); the self slot runs the REAL `hasEmbargoAgainst`.
    fn other_has_embargo_against(&self, tok: usize) -> bool {
        let my_id = self.pi_field("id");
        if self.self_tok == Some(tok) {
            return self.has_embargo_id(&my_id);
        }
        match self.players.get(tok) {
            Some(p) => p.emb.iter().any(|e| js_strict_eq(e, &my_id)),
            None => false,
        }
    }

    /// `other.targets()` — the mock's constant token list (untraced); the
    /// self slot is handled by the caller (the real `targets()` body needs
    /// the facade trace).
    fn other_targets(&self, tok: usize) -> Vec<usize> {
        match self.players.get(tok) {
            Some(p) => p.tgts.clone(),
            None => Vec::new(),
        }
    }

    /// `other.addTroops(n)` / `other.addGold(n)` — the traced mock writes
    /// (128 / 129); the self slot runs the REAL method (unreachable from
    /// the `donate*` self-gates, kept total).
    fn other_add_troops(&mut self, tr: &mut Vec<f64>, tok: usize, n: f64) -> Result<(), String> {
        if self.self_tok == Some(tok) {
            if n < 0.0 {
                self.remove_troops(-n)?;
            } else {
                self.p.troops += to_int_big(n)?;
            }
            return Ok(());
        }
        let pid = self
            .players
            .get(tok)
            .map(|p| p.pid)
            .ok_or_else(|| "Cannot read properties of undefined (reading 'addTroops')".to_string())?;
        tr.push(128.0);
        tr.push(pid);
        push_val(tr, &JsVal::Num(n));
        Ok(())
    }

    fn other_add_gold(&mut self, tr: &mut Vec<f64>, tok: usize, n: i64) -> Result<(), String> {
        if self.self_tok == Some(tok) {
            let o = Op::Big(n);
            if let Ok(v) = acc_plus(&self.p.gold, &o) {
                self.p.gold = v;
            }
            return Ok(());
        }
        let pid = self
            .players
            .get(tok)
            .map(|p| p.pid)
            .ok_or_else(|| "Cannot read properties of undefined (reading 'addGold')".to_string())?;
        tr.push(129.0);
        tr.push(pid);
        push_val(tr, &JsVal::Num(n as f64));
        Ok(())
    }

    /// `mg.players()` — event 130 dumps the scripted token list.
    fn mg_players(&mut self, tr: &mut Vec<f64>) -> Vec<usize> {
        tr.push(130.0);
        tr.push(self.players_list.len() as f64);
        for t in &self.players_list {
            tr.push(*t as f64);
        }
        self.players_list.clone()
    }

    // --- G3c4 facade plumbing ---

    /// `mg.stats().getPlayerStats(this)` — event 138 (always fired by
    /// toFullUpdate); the script is `null` or `{killedBy, deathPosition}`.
    fn mg_get_player_stats(&mut self, tr: &mut Vec<f64>) -> JsVal {
        let v = take_jsval(&self.gs, &mut self.gs_i);
        tr.push(138.0);
        push_val(tr, &v);
        v
    }

    /// `mg.conquer(this, tile)` — event 132.
    fn mg_conquer(&mut self, tr: &mut Vec<f64>, tile: f64) {
        tr.push(132.0);
        tr.push(tile);
    }

    /// `mg.owner(tile) !== this` — event 133 pushes the scripted owner
    /// (`null` or the token number); the gate is the self-token identity
    /// (`mocks[selfTok] = pl`).
    fn mg_owner_is_self(&mut self, tr: &mut Vec<f64>, tile: f64) -> bool {
        let v = take_jsval(&self.owner, &mut self.owner_i);
        tr.push(133.0);
        tr.push(tile);
        push_val(tr, &v);
        matches!(v, JsVal::Num(t) if self.self_tok == Some(t as usize))
    }

    /// `mg.relinquish(tile)` — event 134.
    fn mg_relinquish(&mut self, tr: &mut Vec<f64>, tile: f64) {
        tr.push(134.0);
        tr.push(tile);
    }

    /// `mg.isSpawnImmunityActive()` / `isNationSpawnImmunityActive()` —
    /// events 135 / 136.
    fn mg_spawn_immunity(&mut self, tr: &mut Vec<f64>) -> bool {
        let v = take_f64(&self.sia, &mut self.sia_i);
        tr.push(135.0);
        tr.push(v);
        v != 0.0 && !v.is_nan()
    }

    fn mg_nation_spawn_immunity(&mut self, tr: &mut Vec<f64>) -> bool {
        let v = take_f64(&self.nsia, &mut self.nsia_i);
        tr.push(136.0);
        tr.push(v);
        v != 0.0 && !v.is_nan()
    }

    // --- G3c5: the spawn-family mg scripts and the real-map facade ---

    /// `mg.nextUnitID()` — event 139.
    fn mg_next_unit_id(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = take_f64(&self.nuid, &mut self.nuid_i);
        tr.push(139.0);
        tr.push(v);
        v
    }

    /// `mg.addUnit(unit)` — event 140 (the unit's `id()`).
    fn mg_add_unit(&mut self, tr: &mut Vec<f64>, uid: f64) {
        tr.push(140.0);
        tr.push(uid);
    }

    /// `mg.nearbyUnits(tile, radius, types, ...)` — event 141; the scripted
    /// entries resolve against the unit mocks by `id()` (the TS harness looks
    /// the same ids up in `pl._units`; an unknown id yields a `unit:
    /// undefined` entry whose later reads throw).
    fn mg_nearby_units(&mut self, tr: &mut Vec<f64>, tile: f64, radius: f64, types: &[String]) -> Result<Vec<(Option<usize>, f64, f64)>, String> {
        let call = self.nu.get(self.nu_i).ok_or_else(|| "pl harness: nearbyUnits script exhausted".to_string())?;
        self.nu_i += 1;
        tr.push(141.0);
        tr.push(call.tile);
        tr.push(call.radius);
        tr.push(call.types.len() as f64);
        for t in &call.types {
            push_str(tr, t);
        }
        tr.push(call.entries.len() as f64);
        for (uid, etile, dist) in &call.entries {
            tr.push(*uid);
            tr.push(*etile);
            tr.push(*dist);
        }
        let _ = (tile, radius, types);
        Ok(call
            .entries
            .iter()
            .map(|(uid, etile, dist)| {
                let idx = self
                    .p
                    .units
                    .iter()
                    .position(|u| u.id == *uid);
                (idx, *etile, *dist)
            })
            .collect())
    }

    /// `mg.anyUnitNearby(tile, radius, types, pred)` — event 142 (the
    /// predicate never runs: the scripted return pins its result).
    fn mg_any_unit_nearby(&mut self, tr: &mut Vec<f64>, tile: f64, radius: f64, types: &[String]) -> Result<bool, String> {
        let v = take_f64(&self.anyn, &mut self.anyn_i);
        tr.push(142.0);
        tr.push(tile);
        tr.push(radius);
        tr.push(types.len() as f64);
        for t in types {
            push_str(tr, t);
        }
        tr.push(v);
        Ok(v != 0.0 && !v.is_nan())
    }

    /// `mg.getWinner()` — event 143 (`Null` = no winner, `Num(tok)` = a mock
    /// player; only `!== null` is observable).
    fn mg_get_winner(&mut self, tr: &mut Vec<f64>) -> Result<bool, String> {
        let v = take_jsval(&self.gwin, &mut self.gwin_i);
        tr.push(143.0);
        push_val(tr, &v);
        Ok(!matches!(v, JsVal::Null | JsVal::Undef | JsVal::Absent))
    }

    /// `mg.inSpawnPhase()` — event 144.
    fn mg_in_spawn_phase(&mut self, tr: &mut Vec<f64>) -> bool {
        let v = take_f64(&self.isp, &mut self.isp_i);
        tr.push(144.0);
        tr.push(v);
        v != 0.0 && !v.is_nan()
    }

    /// `config().structureMinDist()` — event 145.
    fn cfg_structure_min_dist(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = take_f64(&self.smd, &mut self.smd_i);
        tr.push(145.0);
        tr.push(v);
        v
    }

    /// `config().radiusPortSpawn()` — event 146.
    fn cfg_radius_port_spawn(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = take_f64(&self.rps, &mut self.rps_i);
        tr.push(146.0);
        tr.push(v);
        v
    }

    /// `config().nukeMagnitudes(type)` — event 147 (`[inner, outer]` FIFO).
    fn cfg_nuke_magnitudes(&mut self, tr: &mut Vec<f64>) -> Result<(f64, f64), String> {
        let (inner, outer) = self
            .nmag
            .get(self.nmag_i)
            .copied()
            .ok_or_else(|| "pl harness: nukeMagnitudes script exhausted".to_string())?;
        self.nmag_i += 1;
        tr.push(147.0);
        tr.push(inner);
        tr.push(outer);
        Ok((inner, outer))
    }

    /// `config().isUnitDisabled(type)` — event 148.
    fn cfg_is_unit_disabled(&mut self, tr: &mut Vec<f64>, typ: &str) -> bool {
        let v = take_f64(&self.idis, &mut self.idis_i);
        tr.push(148.0);
        push_str(tr, typ);
        tr.push(v);
        v != 0.0 && !v.is_nan()
    }

    /// `unitInfo(type).cost(mg, this, n)` — event 149; `n_raw` is the third
    /// argument (`Undef` when absent) and the scripted cost returns as a
    /// bigint (`BigInt(v)`; NaN would throw exactly like the capture).
    fn ui_cost(&mut self, tr: &mut Vec<f64>, n_raw: JsVal) -> Result<i64, String> {
        let v = take_f64(&self.uicost, &mut self.uicost_i);
        tr.push(149.0);
        push_val(tr, &n_raw);
        let b = bigint_from_num(v)?;
        push_val(tr, &JsVal::Num(b as f64));
        Ok(b)
    }

    /// `unitInfo(type).maxHealth` — event 151; NaN in the script is the
    /// `undefined` marker (`maxHealth ?? 1` / `hasHealth` read it).
    fn ui_max_health(&mut self, tr: &mut Vec<f64>) -> Result<Option<f64>, String> {
        let v = take_f64(&self.uimh, &mut self.uimh_i);
        tr.push(151.0);
        if v.is_nan() {
            push_val(tr, &JsVal::Undef);
            Ok(None)
        } else {
            push_val(tr, &JsVal::Num(v));
            Ok(Some(v))
        }
    }

    /// `config().boatMaxNumber()` — event 150.
    fn cfg_boat_max_number(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = take_f64(&self.bmn, &mut self.bmn_i);
        tr.push(150.0);
        tr.push(v);
        v
    }

    /// `config().samRange(1)` — event 155 (only the SAMLauncher ctor branch).
    fn cfg_sam_range(&mut self, tr: &mut Vec<f64>, level: f64) -> f64 {
        let v = take_f64(&self.sam_r, &mut self.sam_r_i);
        tr.push(155.0);
        tr.push(level);
        tr.push(v);
        v
    }

    /// `config().samUpgradeDuration()` — event 156.
    fn cfg_sam_upgrade_duration(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = take_f64(&self.sam_d, &mut self.sam_d_i);
        tr.push(156.0);
        tr.push(v);
        v
    }

    /// `mg.stats().unitBuild(owner, type)` — event 152 (side-effect only).
    fn mg_stats_unit_build(&mut self, tr: &mut Vec<f64>, typ: &str) {
        tr.push(152.0);
        tr.push(self.p.small_id);
        push_str(tr, typ);
    }

    /// `mg.bumpUnitsVersion()` — event 157 (side-effect only in this scope).
    fn mg_bump_units_version(&mut self, tr: &mut Vec<f64>) {
        tr.push(157.0);
    }

    /// `railNetwork().overlappingRailroads(type, tile)` — event 153; the
    /// scripted return crosses verbatim.
    fn rail_overlapping(&mut self, tr: &mut Vec<f64>, tile: f64) -> Result<JsVal, String> {
        let v = take_jsval(&self.orail, &mut self.orail_i);
        tr.push(153.0);
        tr.push(tile);
        push_val(tr, &v);
        Ok(v)
    }

    /// `railNetwork().computeGhostRailPaths(type, tile)` — event 154.
    fn rail_ghost_paths(&mut self, tr: &mut Vec<f64>, tile: f64) -> Result<JsVal, String> {
        let v = take_jsval(&self.gravel, &mut self.gravel_i);
        tr.push(154.0);
        tr.push(tile);
        push_val(tr, &v);
        Ok(v)
    }

    // --- G3c5: the real-map facade (all untraced — pure reads over the same
    // injected GameMap / WaterManager both worlds hold) ---

    /// `mg.map()` — panics without a map block, mirroring the capture (its
    /// mock has no `map` member; TS would throw on the call itself).
    fn map(&self) -> &GameMap {
        self.sq
            .as_ref()
            .expect("pl harness: map scenario without a map block")
            .debug_map()
    }

    fn spatial(&mut self) -> &mut SpatialQuery {
        self.sq
            .as_mut()
            .expect("pl harness: map scenario without a map block")
    }

    /// `mg.owner(tile)` for map scenarios — the real ownerID mapped to the
    /// owner objects: `0` is terra nullius, `smallID` is `this`, anything
    /// else is the mock with that pid.
    fn owner_ref(&self, tile: f64) -> OwnerRef {
        match self.map().owner_id(tile) {
            sid if sid == self.p.small_id => OwnerRef::SelfP,
            0.0 => OwnerRef::TN,
            pid => OwnerRef::Mock(pid),
        }
    }

    /// `mg.hasOwner(tile)` / `mg.isImpassable(tile)` / `mg.isWater(tile)` /
    /// `mg.isLand(tile)` / `mg.isShore(tile)` — direct map reads.
    fn mg_has_owner(&self, tile: f64) -> bool {
        self.map().has_owner(tile)
    }
    fn mg_is_impassable(&self, tile: f64) -> bool {
        self.map().is_impassable(tile)
    }
    fn mg_is_water(&self, tile: f64) -> bool {
        self.map().is_water(tile)
    }
    fn mg_is_land(&self, tile: f64) -> bool {
        self.map().is_land(tile)
    }
    fn mg_is_shore(&self, tile: f64) -> bool {
        self.map().is_shore(tile)
    }

    /// `mg.getWaterComponent(tile)` / `mg.hasWaterComponent(tile, comp)` —
    /// the WaterManager labeling through the persistent SpatialQuery.
    fn mg_get_water_component(&mut self, tile: f64) -> Option<f64> {
        self.spatial().get_water_component(tile)
    }
    fn mg_has_water_component(&mut self, tile: f64, comp: f64) -> bool {
        self.spatial().has_water_component(tile, comp)
    }

    /// The upgradable table lookup (`config().unitInfo(u).upgradable` —
    /// untraced constant).
    fn ui_upgradable(&self, typ: &str) -> bool {
        self.upg
            .iter()
            .find(|(t, _)| t == typ)
            .map(|(_, b)| *b)
            .unwrap_or(false)
    }

    // --- G3c5: the spawn-family bodies ---

    /// `Structures.types` — the shared unit-type group (Game.ts:233-240).
    const STRUCTURE_TYPES: [&'static str; 6] = [
        "City",
        "Defense Post",
        "SAM Launcher",
        "Missile Silo",
        "Port",
        "Factory",
    ];

    /// The mock-player token for a scripted map-owner pid; a miss means the
    /// TS `mocks.find(...)` returned `undefined` and the next member read
    /// throws.
    fn owner_tok(&self, pid: f64) -> Option<usize> {
        self.players.iter().position(|p| p.pid == pid)
    }

    /// The border tiles in TileSet insertion order (the `borderTiles()`
    /// iteration the water queries walk).
    fn border_vec(&mut self) -> Vec<f64> {
        self.p.border_tiles.iter_begin();
        let mut cur = 0usize;
        let mut vals = Vec::new();
        while let Some(v) = self.p.border_tiles.iter_next(&mut cur) {
            vals.push(v);
        }
        self.p.border_tiles.iter_end();
        vals
    }

    /// The mock unit's `toUpdate()` shim — the real `UnitUpdate` key shape
    /// over the 10-field mock. Event 101 keeps `undefined`-valued keys; the
    /// `health` read fires 151 once per update (the real `hasHealth()` →
    /// `info().maxHealth` read).
    fn mock_unit_update(&mut self, tr: &mut Vec<f64>, idx: usize) -> Result<JsVal, String> {
        let u = self.p.units[idx].clone();
        let health = match self.ui_max_health(tr)? {
            Some(v) => JsVal::Num(to_int_big(v)? as f64),
            None => JsVal::Undef,
        };
        Ok(JsVal::Obj(vec![
            ("type".to_string(), JsVal::Num(1.0)), // GameUpdateType.Unit
            ("unitType".to_string(), JsVal::Str(u.typ)),
            ("id".to_string(), JsVal::Num(u.id)),
            ("troops".to_string(), JsVal::Num(0.0)),
            ("ownerID".to_string(), JsVal::Num(u.owner_sid)),
            ("lastOwnerID".to_string(), JsVal::Undef),
            ("isActive".to_string(), JsVal::Bool(u.active)),
            ("reachedTarget".to_string(), JsVal::Bool(false)),
            ("warshipState".to_string(), JsVal::Undef),
            ("transportShipState".to_string(), JsVal::Undef),
            ("nukeState".to_string(), JsVal::Undef),
            ("samUpgrade".to_string(), JsVal::Undef),
            ("pos".to_string(), JsVal::Num(u.tile)),
            ("markedForDeletion".to_string(), JsVal::Bool(u.mfd)),
            ("targetable".to_string(), JsVal::Bool(true)),
            ("lastPos".to_string(), JsVal::Num(u.tile)),
            ("health".to_string(), health),
            ("underConstruction".to_string(), JsVal::Bool(u.uc)),
            ("targetUnitId".to_string(), JsVal::Undef),
            ("targetTile".to_string(), JsVal::Undef),
            (
                "missileTimerQueue".to_string(),
                JsVal::Arr(u.queue.iter().map(|&x| JsVal::Num(x)).collect()),
            ),
            ("level".to_string(), JsVal::Num(u.level)),
            ("hasTrainStation".to_string(), JsVal::Bool(false)),
            ("trainType".to_string(), JsVal::Undef),
            ("loaded".to_string(), JsVal::Undef),
        ]))
    }

    /// The mock unit's `increaseLevel()` shim — the real facade order
    /// (UnitImpl.ts:709-728) minus the unobservable state writes: [SAM: 102]
    /// -> `_level++` -> 157 -> `_myUnitsVersion++` -> [Silo/SAM: 102 + queue
    /// push] -> 101 (+151 from the shim update's health read).
    fn mock_increase_level(&mut self, tr: &mut Vec<f64>, idx: usize) -> Result<(), String> {
        let typ = self.p.units[idx].typ.clone();
        if typ == "SAM Launcher" {
            self.mg_ticks(tr);
        }
        self.p.units[idx].level += 1.0;
        self.mg_bump_units_version(tr);
        self.p.my_units_version += 1.0;
        if typ == "Missile Silo" || typ == "SAM Launcher" {
            let tick = self.mg_ticks(tr);
            self.p.units[idx].queue.push(tick);
        }
        let upd = self.mock_unit_update(tr, idx)?;
        self.mg_add_update(tr, &upd);
        Ok(())
    }

    /// `canBuildUnitType` (1425-1440) — 148 first, then the cost (149)
    /// unless the caller already knows it, the cross-type `_gold < cost`
    /// gate, and the MIRV-warhead isAlive exemption.
    fn can_build_unit_type(
        &mut self,
        tr: &mut Vec<f64>,
        typ: &str,
        known_cost: Option<i64>,
    ) -> Result<bool, String> {
        if self.cfg_is_unit_disabled(tr, typ) {
            return Ok(false);
        }
        let cost = match known_cost {
            Some(c) => c,
            None => self.ui_cost(tr, JsVal::Undef)?,
        };
        let gold_lt = match &self.p.gold {
            Acc::Big(g) => *g < cost,
            Acc::Str(s) => crate::game_config_helpers::js_number(s) < cost as f64,
        };
        if gold_lt {
            return Ok(false);
        }
        if typ != "MIRV Warhead" && !self.is_alive() {
            return Ok(false);
        }
        Ok(true)
    }

    /// `isUnitValidToUpgrade` (1446-1457) — untraced field reads; the owner
    /// gate compares the mock's `ownerSID` against this player's.
    fn is_unit_valid_to_upgrade(&self, idx: usize) -> bool {
        let u = &self.p.units[idx];
        !(u.uc || u.mfd || u.owner_sid != self.p.small_id)
    }

    /// `findExistingUnitToUpgrade` (1407-1423): 145 (argument evaluation)
    /// then 141; `findClosestBy` keeps the first minimum `distSquared`, and
    /// `closest?.unit ?? false` maps an unknown id to `false` without
    /// touching the (throwing) `unit.tile()` read.
    fn find_existing_unit_to_upgrade(
        &mut self,
        tr: &mut Vec<f64>,
        typ: &str,
        tile: f64,
    ) -> Result<Option<usize>, String> {
        let smd = self.cfg_structure_min_dist(tr);
        let types = [typ.to_string()];
        let entries = self.mg_nearby_units(tr, tile, smd, &types)?;
        let mut best: Option<(Option<usize>, f64)> = None;
        for (uid, _etile, dist) in entries {
            if dist < best.map(|(_, d)| d).unwrap_or(f64::INFINITY) {
                best = Some((uid, dist));
            }
        }
        Ok(best.and_then(|(uid, _)| uid))
    }

    /// `canUpgradeUnit` (1459-1470) — the upgradable table, the cost probe
    /// (148 + 149) and the validity read.
    fn can_upgrade_unit(&mut self, tr: &mut Vec<f64>, idx: usize) -> Result<bool, String> {
        let typ = self.p.units[idx].typ.clone();
        if !self.ui_upgradable(&typ) {
            return Ok(false);
        }
        if !self.can_build_unit_type(tr, &typ, None)? {
            return Ok(false);
        }
        Ok(self.is_unit_valid_to_upgrade(idx))
    }

    /// `findUnitToUpgrade` (1399-1405) — the unit crosses as its id.
    fn find_unit_to_upgrade(
        &mut self,
        tr: &mut Vec<f64>,
        typ: &str,
        tile: f64,
    ) -> Result<Option<f64>, String> {
        match self.find_existing_unit_to_upgrade(tr, typ, tile)? {
            None => Ok(None),
            Some(idx) => {
                if self.can_upgrade_unit(tr, idx)? {
                    Ok(Some(self.p.units[idx].id))
                } else {
                    Ok(None)
                }
            }
        }
    }

    /// `upgradeUnit` (1472-1477) over the mock: 149, `removeGold`, the mock
    /// `increaseLevel` shim, `recordUnitConstructed`.
    fn upgrade_unit(&mut self, tr: &mut Vec<f64>, idx: usize) -> Result<(), String> {
        let typ = self.p.units[idx].typ.clone();
        let cost = self.ui_cost(tr, JsVal::Undef)?;
        self.remove_gold_body(&Op::Big(cost))?;
        self.mock_increase_level(tr, idx)?;
        self.record_unit_constructed(typ);
        Ok(())
    }

    /// `buildUnit` (1368-1397) — the capture side constructs a REAL
    /// `UnitImpl`, so the ctor's observable reads fire here: 148 (the
    /// disabled throw gate), 149 (cost), 139 (`nextUnitID`), 151
    /// (`maxHealth`), [SAM: 155 + 156], [structure: 152]; then
    /// `recordUnitConstructed`, the gold / troop debits, 101 (the full real
    /// `toUpdate` — its `hasHealth` read fires 151 again, a Warship
    /// `warshipState` getter fires 102 via `isInCombat`) and 140
    /// (`addUnit`). `params` may carry `troops` / `targetTile` /
    /// `patrolTile` / `trajectory` / `waitTicks` / `trainType` / `loaded`;
    /// `targetUnit` is out of scope (the scenarios never pass it).
    fn build_unit(
        &mut self,
        tr: &mut Vec<f64>,
        typ: String,
        spawn_tile: f64,
        params: JsVal,
    ) -> Result<f64, String> {
        if self.cfg_is_unit_disabled(tr, &typ) {
            let name = js_to_string_val(&self.pi_field("name"));
            return Err(format!(
                "Attempted to build disabled unit {typ} at tile {spawn_tile} by player {name}"
            ));
        }
        let cost = self.ui_cost(tr, JsVal::Undef)?;
        let uid = self.mg_next_unit_id(tr);
        let mh = self.ui_max_health(tr)?;
        let _health = to_int_big(mh.unwrap_or(1.0))?;
        let pobj = match &params {
            JsVal::Obj(kv) => Some(kv.clone()),
            _ => None,
        };
        let pk = |k: &str| -> Option<JsVal> {
            pobj
                .as_ref()
                .and_then(|kv| kv.iter().find(|(key, _)| key == k))
                .map(|(_, v)| v.clone())
        };
        let has_k = |k: &str| -> bool {
            pobj
                .as_ref()
                .map(|kv| kv.iter().any(|(key, _)| key == k))
                .unwrap_or(false)
        };
        if typ == "SAM Launcher" {
            let sr = self.cfg_sam_range(tr, 1.0);
            let _ = sr;
            let sd = self.cfg_sam_upgrade_duration(tr);
            let _ = sd;
        }
        let troops_val: JsVal = if has_k("troops") {
            match pk("troops") {
                None | Some(JsVal::Undef) | Some(JsVal::Null) | Some(JsVal::Absent) => {
                    JsVal::Num(0.0)
                }
                Some(v) => v,
            }
        } else {
            JsVal::Num(0.0)
        };
        let mut warship: Option<Vec<(String, JsVal)>> = None;
        if has_k("patrolTile") {
            warship = Some(vec![
                ("state".to_string(), JsVal::Str("patrolling".to_string())),
                (
                    "patrolTile".to_string(),
                    pk("patrolTile").unwrap_or(JsVal::Undef),
                ),
                ("lastCombatTick".to_string(), JsVal::Num(-100.0)),
                ("veterancy".to_string(), JsVal::Num(0.0)),
                ("veterancyProgress".to_string(), JsVal::Num(0.0)),
            ]);
        }
        let mut nuke: Option<Vec<(String, JsVal)>> = None;
        if has_k("trajectory") || has_k("waitTicks") {
            let traj = match pk("trajectory") {
                None | Some(JsVal::Undef) | Some(JsVal::Null) => JsVal::Arr(vec![]),
                Some(v) => v,
            };
            nuke = Some(vec![
                ("trajectory".to_string(), traj),
                ("waitTicks".to_string(), JsVal::Num(0.0)),
                ("trajectoryIndex".to_string(), JsVal::Num(0.0)),
                ("targetedBySam".to_string(), JsVal::Bool(false)),
            ]);
        }
        if Self::STRUCTURE_TYPES.contains(&typ.as_str()) || typ == "Warship" {
            self.mg_stats_unit_build(tr, &typ);
        }
        let sid = self.p.small_id;
        self.p.units.push(UnitMock {
            typ: typ.clone(),
            id: uid,
            owner_sid: sid,
            level: 1.0,
            uc: false,
            active: true,
            cooldown: false,
            tile: spawn_tile,
            mfd: false,
            hash: 0.0,
            queue: Vec::new(),
        });
        self.p.my_units_version += 1.0;
        self.record_unit_constructed(typ.clone());
        self.remove_gold_body(&Op::Big(cost))?;
        let troops = to_num(&troops_val);
        self.remove_troops(troops)?;
        // b.toUpdate() — the full real shape over the ctor state.
        let mut m: Vec<(String, JsVal)> = vec![
            ("type".to_string(), JsVal::Num(1.0)),
            ("unitType".to_string(), JsVal::Str(typ.clone())),
            ("id".to_string(), JsVal::Num(uid)),
            ("troops".to_string(), troops_val.clone()),
            ("ownerID".to_string(), JsVal::Num(sid)),
            ("lastOwnerID".to_string(), JsVal::Undef),
            ("isActive".to_string(), JsVal::Bool(true)),
            ("reachedTarget".to_string(), JsVal::Bool(false)),
        ];
        let ws = match &warship {
            Some(st) => {
                // The getter mutates `isInCombat` via `isInCombat()` →
                // `mg.ticks() - lastCombatTick <= 3` (event 102), and the
                // spread appends the new key last.
                let tick = self.mg_ticks(tr);
                let mut st2 = st.clone();
                st2.push(("isInCombat".to_string(), JsVal::Bool(tick - -100.0 <= 3.0)));
                JsVal::Obj(st2)
            }
            None => JsVal::Undef,
        };
        m.push(("warshipState".to_string(), ws));
        let ts = if typ == "Transport" {
            // transportShipState(): `{ isRetreating, troops: this._troops }`.
            JsVal::Obj(vec![
                ("isRetreating".to_string(), JsVal::Bool(false)),
                ("troops".to_string(), troops_val.clone()),
            ])
        } else {
            JsVal::Undef
        };
        m.push(("transportShipState".to_string(), ts));
        m.push((
            "nukeState".to_string(),
            nuke.map(JsVal::Obj).unwrap_or(JsVal::Undef),
        ));
        m.push(("samUpgrade".to_string(), JsVal::Undef));
        m.push(("pos".to_string(), JsVal::Num(spawn_tile)));
        m.push(("markedForDeletion".to_string(), JsVal::Bool(false)));
        m.push(("targetable".to_string(), JsVal::Bool(true)));
        m.push(("lastPos".to_string(), JsVal::Num(spawn_tile)));
        let health = match self.ui_max_health(tr)? {
            Some(v) => JsVal::Num(to_int_big(v)? as f64),
            None => JsVal::Undef,
        };
        m.push(("health".to_string(), health));
        m.push(("underConstruction".to_string(), JsVal::Bool(false)));
        m.push(("targetUnitId".to_string(), JsVal::Undef));
        let tt = match pk("targetTile") {
            None | Some(JsVal::Undef) | Some(JsVal::Null) => JsVal::Undef,
            Some(v) => v,
        };
        m.push(("targetTile".to_string(), tt));
        m.push(("missileTimerQueue".to_string(), JsVal::Arr(vec![])));
        m.push(("level".to_string(), JsVal::Num(1.0)));
        m.push(("hasTrainStation".to_string(), JsVal::Bool(false)));
        m.push((
            "trainType".to_string(),
            pk("trainType").unwrap_or(JsVal::Undef),
        ));
        let ld = match pk("loaded") {
            None | Some(JsVal::Undef) | Some(JsVal::Null) => JsVal::Undef,
            Some(v) => v,
        };
        m.push(("loaded".to_string(), ld));
        self.mg_add_update(tr, &JsVal::Obj(m));
        self.mg_add_unit(tr, uid);
        Ok(uid)
    }

    /// `nukeSpawn` (1601-1651): 135, the untraced impassable / owner map
    /// reads, 143 (`getWinner`) + the untraced gameType, the Team-mode
    /// teammate-structure probe (147 then 142), then the ready-silo filter
    /// and the manhattan-stable sort over the mock tiles. An undefined
    /// script owner throws like the capture's `owner.isPlayer()` read.
    fn nuke_spawn(
        &mut self,
        tr: &mut Vec<f64>,
        tile: f64,
        nuke_type: &str,
    ) -> Result<Option<f64>, String> {
        if self.mg_spawn_immunity(tr) {
            return Ok(None);
        }
        if self.mg_is_impassable(tile) {
            return Ok(None);
        }
        let owner = self.owner_ref(tile);
        if let OwnerRef::Mock(pid) = owner {
            if self.owner_tok(pid).is_none() {
                return Err(
                    "Cannot read properties of undefined (reading 'isPlayer')".to_string()
                );
            }
        }
        let winner = self.mg_get_winner(tr)?;
        let game_over = winner && self.gt != "Singleplayer";
        if owner.is_player() {
            let same = match owner {
                // `other === this` returns false before any facade.
                OwnerRef::SelfP => false,
                OwnerRef::Mock(pid) => {
                    let tok = self.owner_tok(pid).expect("checked above");
                    self.same_team(tr, tok)?
                }
                OwnerRef::TN => false,
            };
            if same && !game_over {
                return Ok(None);
            }
        }
        if self.gmd == "Team" && nuke_type != "MIRV" && !game_over {
            let (inner, outer) = self.cfg_nuke_magnitudes(tr)?;
            let _ = inner;
            let structures: Vec<String> =
                Self::STRUCTURE_TYPES.iter().map(|s| s.to_string()).collect();
            if self.mg_any_unit_nearby(tr, tile, outer, &structures)? {
                return Ok(None);
            }
        }
        let idxs = self.units_filtered(Some(&["Missile Silo".to_string()]));
        let mut ready: Vec<(usize, f64)> = Vec::new();
        for i in idxs {
            let (active, cooldown, uc, utile) = {
                let u = &self.p.units[i];
                (u.active, u.cooldown, u.uc, u.tile)
            };
            if active && !cooldown && !uc {
                let d = self.map().manhattan_dist(utile, tile);
                ready.push((i, d));
            }
        }
        ready.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(ready
            .first()
            .map(|&(i, _)| self.p.units[i].tile))
    }

    /// `portSpawn` (1653-1674): 146 (radius), the untraced bfs / owner /
    /// shore filter and manhattan sort, then the valid-tile set — the
    /// pass-through list or a fresh `validStructureSpawnTiles` (141 + 145).
    fn port_spawn(
        &mut self,
        tr: &mut Vec<f64>,
        tile: f64,
        valid_tiles: Option<Vec<f64>>,
    ) -> Result<Option<f64>, String> {
        let radius = self.cfg_radius_port_spawn(tr);
        let spawns: Vec<f64> = {
            let map = self.map();
            map.bfs(tile, &|gm, t| {
                crate::game_map::manhattan_dist_fn(gm, tile, radius, false, t)
            })
        };
        let mut owned_shores: Vec<(f64, f64)> = Vec::new();
        for t in spawns {
            if self.owner_ref(t) == OwnerRef::SelfP && self.mg_is_shore(t) {
                let d = self.map().manhattan_dist(t, tile);
                owned_shores.push((t, d));
            }
        }
        owned_shores.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        let valid_set: Vec<f64> = match valid_tiles {
            Some(v) => v,
            None => self.valid_structure_spawn_tiles(tr, tile)?,
        };
        for (t, _) in owned_shores {
            if valid_set.contains(&t) {
                return Ok(Some(t));
            }
        }
        Ok(None)
    }

    /// `warshipSpawn` (1676-1693): the water gate, one `getWaterComponent`,
    /// then `findClosestBy` over the Port mocks — the filter (active /
    /// construction / component membership) runs before the score.
    fn warship_spawn(&mut self, tr: &mut Vec<f64>, tile: f64) -> Result<Option<f64>, String> {
        let _ = tr;
        if !self.mg_is_water(tile) {
            return Ok(None);
        }
        let tile_component = self.mg_get_water_component(tile);
        let idxs = self.units_filtered(Some(&["Port".to_string()]));
        let mut best: Option<(usize, f64)> = None;
        for i in idxs {
            let (active, uc, ptile) = {
                let u = &self.p.units[i];
                (u.active, u.uc, u.tile)
            };
            if !active || uc {
                continue;
            }
            let c = match tile_component {
                Some(c) => c,
                None => continue,
            };
            if !self.mg_has_water_component(ptile, c) {
                continue;
            }
            let d = self.map().manhattan_dist(ptile, tile);
            if d < best.map(|(_, bd)| bd).unwrap_or(f64::INFINITY) {
                best = Some((i, d));
            }
        }
        Ok(best.map(|(i, _)| self.p.units[i].tile))
    }

    /// `landBasedUnitSpawn` (1695-1697).
    fn land_based_unit_spawn(&mut self, tr: &mut Vec<f64>, tile: f64) -> Result<Option<f64>, String> {
        let _ = tr;
        Ok(if self.mg_is_land(tile) && !self.mg_is_impassable(tile) {
            Some(tile)
        } else {
            None
        })
    }

    /// `landBasedStructureSpawn` (1699-1708).
    fn land_based_structure_spawn(
        &mut self,
        tr: &mut Vec<f64>,
        tile: f64,
        valid_tiles: Option<Vec<f64>>,
    ) -> Result<Option<f64>, String> {
        let tiles = match valid_tiles {
            Some(v) => v,
            None => self.valid_structure_spawn_tiles(tr, tile)?,
        };
        Ok(tiles.first().copied())
    }

    /// `tradeShipSpawn` (1783-1787).
    fn trade_ship_spawn(&mut self, tr: &mut Vec<f64>, target: f64) -> Result<Option<f64>, String> {
        let _ = tr;
        let idxs = self.units_filtered(Some(&["Port".to_string()]));
        let found = idxs.into_iter().any(|i| self.p.units[i].tile == target);
        Ok(if found { Some(target) } else { None })
    }

    /// `validStructureSpawnTiles` (1710-1781): the owner gate (untraced),
    /// 141 (`nearbyUnits` at radius 30 over `Structures.types`), the shared-
    /// scratch flood (untraced), 145 (`structureMinDist` — read AFTER the
    /// flood), then the euclidean block filter and the stable distance
    /// sort. An unknown nearby id throws on the first `unit.tile()` read.
    fn valid_structure_spawn_tiles(
        &mut self,
        tr: &mut Vec<f64>,
        tile: f64,
    ) -> Result<Vec<f64>, String> {
        if self.owner_ref(tile) != OwnerRef::SelfP {
            return Ok(Vec::new());
        }
        let structures: Vec<String> =
            Self::STRUCTURE_TYPES.iter().map(|s| s.to_string()).collect();
        let nearby = self.mg_nearby_units(tr, tile, 30.0, &structures)?;
        let small = self.p.small_id;
        let nearby_tiles = self.spatial().flood_owned_tiles(tile, small, 15.0);
        let min_sq = {
            let smd = self.cfg_structure_min_dist(tr);
            smd * smd
        };
        let mut valid: Vec<f64> = Vec::new();
        for &t in &nearby_tiles {
            let mut blocked = false;
            for (uid, _etile, _dist) in &nearby {
                let idx = match uid {
                    Some(i) => *i,
                    None => {
                        return Err(
                            "Cannot read properties of undefined (reading 'tile')".to_string()
                        )
                    }
                };
                let utile = self.p.units[idx].tile;
                if self.map().euclidean_dist_squared(utile, t) < min_sq {
                    blocked = true;
                    break;
                }
            }
            if !blocked {
                valid.push(t);
            }
        }
        let mut pairs: Vec<(f64, f64)> = valid
            .into_iter()
            .map(|t| (t, self.map().euclidean_dist_squared(t, tile)))
            .collect();
        pairs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(pairs.into_iter().map(|(t, _)| t).collect())
    }

    /// `canBuildTransportShip` (TransportShipUtils.ts:5-31) inlined for the
    /// event order: `unitCount` (untraced), 150 (`boatMaxNumber`),
    /// `targetTransportTile` (the reachable-shore query — untraced map
    /// reads over the persistent SpatialQuery), the `owner === player`
    /// gate, `canAttackPlayer` (137 / 106 facades), then
    /// `closestShoreByWater`. The capture news a `SpatialQuery` per call;
    /// the persistent instance is equivalent (the bounded A* re-stamps and
    /// the memos were shared per-game in TS already).
    fn transport_ship_spawn(&mut self, tr: &mut Vec<f64>, tile: f64) -> Result<Option<f64>, String> {
        let count = self.unit_count("Transport");
        let boat_max = self.cfg_boat_max_number(tr);
        if count >= boat_max {
            return Ok(None);
        }
        let border = self.border_vec();
        let tile_version = self.p.tile_change_version;
        let target = self.owner_ref(tile);
        let dst = self
            .spatial()
            .closest_reachable_shore(target.small_id(), &border, tile_version, tile, 50.0);
        let dst = match dst {
            Some(d) => d,
            None => return Ok(None),
        };
        if let OwnerRef::Mock(pid) = self.owner_ref(tile) {
            let tok = self
                .owner_tok(pid)
                .ok_or_else(|| "Cannot read properties of undefined (reading 'isPlayer')".to_string())?;
            if !self.can_attack_player(tr, tok, false)? {
                return Ok(None);
            }
        } else if matches!(self.owner_ref(tile), OwnerRef::SelfP) {
            return Ok(None);
        }
        Ok(self.spatial().closest_shore_by_water(&border, dst))
    }

    /// `canSpawnUnitType` (1561-1599) — the switch. `validTiles` is the
    /// canBuild / buildableUnits pass-through; the default arm is
    /// `assertNever` ("Unexpected value: ...").
    #[allow(clippy::too_many_lines)]
    fn can_spawn_unit_type(
        &mut self,
        tr: &mut Vec<f64>,
        typ: &str,
        tile: f64,
        valid_tiles: Option<Vec<f64>>,
    ) -> Result<Option<f64>, String> {
        match typ {
            "MIRV" => {
                if !self.mg_has_owner(tile) {
                    return Ok(None);
                }
                self.nuke_spawn(tr, tile, typ)
            }
            "Atom Bomb" | "Hydrogen Bomb" => self.nuke_spawn(tr, tile, typ),
            "MIRV Warhead" | "Shell" | "SAMMissile" => Ok(Some(tile)),
            "Port" => self.port_spawn(tr, tile, valid_tiles),
            "Warship" => self.warship_spawn(tr, tile),
            "Transport" => self.transport_ship_spawn(tr, tile),
            "Trade Ship" => self.trade_ship_spawn(tr, tile),
            "Train" => self.land_based_unit_spawn(tr, tile),
            "Missile Silo" | "Defense Post" | "SAM Launcher" | "City" | "Factory" => {
                self.land_based_structure_spawn(tr, tile, valid_tiles)
            }
            other => Err(format!("Unexpected value: {other}")),
        }
    }

    /// `canBuild` (1549-1559) — the cost probe (148 + 149, no known cost)
    /// then the spawn switch.
    fn can_build(
        &mut self,
        tr: &mut Vec<f64>,
        typ: &str,
        tile: f64,
        valid: Option<Vec<f64>>,
    ) -> Result<Option<f64>, String> {
        if !self.can_build_unit_type(tr, typ, None)? {
            return Ok(None);
        }
        self.can_spawn_unit_type(tr, typ, tile, valid)
    }

    /// `buildableUnits` (1479-1547): 144 once, the spawn-tile set (141 + 145
    /// when a structure type is present, else the empty `[]` fallback),
    /// then per unit: 149, 148 (via `canBuildUnitType` with the known cost),
    /// [145 + 141, the spawn switch], the 50-step cumulative upgrade costs
    /// (149 with n = 0..49) and 153 / 154 when `buildNew`. The result
    /// objects drop `upgradeCosts` when no upgrade is possible (the normPU
    /// undefined-key rule).
    fn buildable_units(
        &mut self,
        tr: &mut Vec<f64>,
        tile: Option<f64>,
        units: &[String],
    ) -> Result<JsVal, String> {
        let in_spawn = self.mg_in_spawn_phase(tr);
        let structures: Vec<String> =
            Self::STRUCTURE_TYPES.iter().map(|s| s.to_string()).collect();
        let valid_tiles: Option<Vec<f64>> = match tile {
            Some(t) if units.iter().any(|u| structures.contains(u)) => {
                Some(self.valid_structure_spawn_tiles(tr, t)?)
            }
            _ => Some(Vec::new()),
        };
        let mut result: Vec<JsVal> = Vec::new();
        for u in units {
            let cost = self.ui_cost(tr, JsVal::Undef)?;
            let mut can_upgrade: Option<f64> = None;
            let mut can_build: Option<f64> = None;
            let eligible = match tile {
                Some(_) => self.can_build_unit_type(tr, u, Some(cost))? && !in_spawn,
                None => false,
            };
            if eligible {
                let t = tile.expect("checked above");
                if self.ui_upgradable(u) {
                    if let Some(idx) = self.find_existing_unit_to_upgrade(tr, u, t)? {
                        if self.is_unit_valid_to_upgrade(idx) {
                            can_upgrade = Some(self.p.units[idx].id);
                        }
                    }
                }
                can_build = self.can_spawn_unit_type(tr, u, t, valid_tiles.clone())?;
            }
            let build_new = can_build.is_some() && can_upgrade.is_none();
            let mut upgrade_costs: Option<Vec<JsVal>> = None;
            if can_upgrade.is_some() {
                // MAX_UPGRADE_AMOUNT = 50 (Game.ts:173): cost(mg, this, n)
                // for n = 0..49, cumulated as bigints; they cross as f64.
                let mut totals: Vec<JsVal> = Vec::with_capacity(50);
                let mut total: i64 = 0;
                for n in 0..50 {
                    total += self.ui_cost(tr, JsVal::Num(n as f64))?;
                    totals.push(JsVal::Num(total as f64));
                }
                upgrade_costs = Some(totals);
            }
            let mut obj: Vec<(String, JsVal)> = vec![
                ("type".to_string(), JsVal::Str(u.clone())),
                (
                    "canBuild".to_string(),
                    can_build.map(JsVal::Num).unwrap_or(JsVal::Bool(false)),
                ),
                (
                    "canUpgrade".to_string(),
                    can_upgrade.map(JsVal::Num).unwrap_or(JsVal::Bool(false)),
                ),
                ("cost".to_string(), JsVal::Num(cost as f64)),
            ];
            if let Some(uc) = upgrade_costs {
                obj.push(("upgradeCosts".to_string(), JsVal::Arr(uc)));
            }
            if build_new {
                let ct = can_build.expect("build_new");
                let or = self.rail_overlapping(tr, ct)?;
                let gp = self.rail_ghost_paths(tr, ct)?;
                obj.push(("overlappingRailroads".to_string(), or));
                obj.push(("ghostRailPaths".to_string(), gp));
            } else {
                obj.push(("overlappingRailroads".to_string(), JsVal::Arr(vec![])));
                obj.push(("ghostRailPaths".to_string(), JsVal::Arr(vec![])));
            }
            result.push(JsVal::Obj(obj));
        }
        Ok(JsVal::Arr(result))
    }


    /// `isImmune()` (1883-1891) — the type gates read `playerInfo.playerType`
    /// (untraced); the self-slot 137 caller runs this real body.
    fn is_immune_body(&mut self, tr: &mut Vec<f64>) -> bool {
        let t = self.pi_field("playerType");
        if js_strict_eq(&t, &JsVal::Str("HUMAN".to_string())) {
            self.mg_spawn_immunity(tr)
        } else if js_strict_eq(&t, &JsVal::Str("NATION".to_string())) {
            self.mg_nation_spawn_immunity(tr)
        } else {
            false
        }
    }

    /// `other.isImmune()` — the mock facade (137) with the `!!v`
    /// normalisation; the self slot runs the real body (untraced, mirroring
    /// `mocks[selfTok] = pl`).
    fn other_is_immune(&mut self, tr: &mut Vec<f64>, tok: usize) -> Result<bool, String> {
        if self.self_tok == Some(tok) {
            return Ok(self.is_immune_body(tr));
        }
        let p = self.players.get_mut(tok).ok_or_else(|| {
            "Cannot read properties of undefined (reading 'isImmune')".to_string()
        })?;
        let v = take_f64(&p.immune, &mut p.immune_i);
        let b = v != 0.0 && !v.is_nan();
        tr.push(137.0);
        tr.push(p.pid);
        tr.push(if b { 1.0 } else { 0.0 });
        Ok(b)
    }

    /// `unit.hash()` — event 131.
    fn unit_hash(&mut self, tr: &mut Vec<f64>, h: f64) -> f64 {
        tr.push(131.0);
        tr.push(h);
        h
    }

    // --- identity helpers ---

    fn pi_field(&self, key: &str) -> JsVal {
        val_field(&self.p.player_info, key)
            .cloned()
            .unwrap_or(JsVal::Undef)
    }

    fn is_alive(&self) -> bool {
        self.p.tiles.size() > 0.0
    }

    /// `hasEmbargoAgainst` (1200): the `embargoes` Map lookup by
    /// `other.id()` — strict `===` on the key (`SameValueZero` agrees for
    /// the string/number domain here).
    fn has_embargo_id(&self, id: &JsVal) -> bool {
        self.p.embargoes.iter().any(|(k, _)| js_strict_eq(k, id))
    }

    fn alliance_with(&self, tok: usize) -> Option<AllianceMock> {
        if self.self_tok == Some(tok) {
            return None;
        }
        self.p
            .alliances
            .iter()
            .find(|a| a.rec == tok || a.req == tok)
            .copied()
    }

    /// `agreedToExtend(p)` on an alliance mock:
    /// `p === requestor() ? reqAgree : recAgree` — the this-arg matches
    /// `requestor()` only when the self token sits at the req slot.
    fn agreed_to_extend(&self, a: &AllianceMock, arg_self: bool, tok: usize) -> bool {
        let at_req = if arg_self {
            self.self_tok == Some(a.req)
        } else {
            tok == a.req
        };
        if at_req { a.req_agree } else { a.rec_agree }
    }

    /// `isOnSameTeam` (PlayerImpl.ts:1259) — the facade call counts are the
    /// observable part: 0 (self / this-null), 1 (other-null), 2 (this-Bot),
    /// 3 (the final `===`).
    fn same_team(&mut self, tr: &mut Vec<f64>, tok: usize) -> Result<bool, String> {
        if self.self_tok == Some(tok) {
            return Ok(false);
        }
        if self.p.team == JsVal::Null {
            return Ok(false);
        }
        let t1 = self.other_team(tr, tok)?;
        if t1 == JsVal::Null {
            return Ok(false);
        }
        if self.p.team == JsVal::Str("Bot".to_string()) {
            return Ok(false);
        }
        let t2 = self.other_team(tr, tok)?;
        if t2 == JsVal::Str("Bot".to_string()) {
            return Ok(false);
        }
        let t3 = self.other_team(tr, tok)?;
        Ok(self.p.team == t3)
    }

    fn is_friendly(&mut self, tr: &mut Vec<f64>, tok: usize, treat: bool) -> Result<bool, String> {
        if self.self_tok == Some(tok) {
            return Ok(true);
        }
        let d = self.other_is_disconnected(tr, tok)?;
        if d && !treat {
            return Ok(false);
        }
        if self.same_team(tr, tok)? {
            return Ok(true);
        }
        Ok(self.alliance_with(tok).is_some())
    }

    /// `removeTroops` (1352) as a reusable body — `addTroops` delegates to
    /// it on the negative branch. Returns the removed amount.
    fn remove_troops(&mut self, troops: f64) -> Result<f64, String> {
        if troops <= 0.0 {
            return Ok(0.0);
        }
        let to_remove = to_int_big(troops)?;
        let actual = if self.p.troops < to_remove {
            self.p.troops
        } else {
            to_remove
        };
        self.p.troops -= actual;
        Ok(actual as f64)
    }

    /// `getTraitorRemainingTicks` (PlayerImpl.ts:855) — the `ticks()` facade
    /// (102) runs BEFORE the `traitorDuration()` facade (108), JS
    /// left-to-right; a never-marked player fires neither.
    fn traitor_remaining(&mut self, tr: &mut Vec<f64>) -> f64 {
        if self.p.marked_traitor_tick < 0.0 {
            return 0.0;
        }
        let elapsed = self.mg_ticks(tr) - self.p.marked_traitor_tick;
        let duration = self.mg_traitor_duration(tr);
        let remaining = duration - elapsed;
        if remaining > 0.0 {
            remaining
        } else {
            0.0
        }
    }

    /// `inDoomsdayClock` (PlayerImpl.ts:874) — `isAlive()` is untraced.
    fn in_doomsday(&self) -> bool {
        self.is_alive() && self.p.marked_doomsday_clock_tick >= 0.0
    }

    // --- G3c3 method bodies ---

    /// `toString()` (1833-1839) — only observable through the self-throw
    /// messages (`relation` / `updateRelation` / `sendEmoji`). `_troops` is
    /// a bigint (`${10n}` -> "10"); `numTilesOwned` interpolates as a JS
    /// number.
    fn player_to_string(&self) -> String {
        let name = js_to_string_val(&self.pi_field("name"));
        let client = js_to_string_val(&self.pi_field("clientID"));
        format!(
            "Player:{{name:{},clientID:{},isAlive:{},troops:{},numTileOwned:{}}}]",
            name,
            client,
            self.is_alive(),
            self.p.troops,
            crate::js_fixed::js_to_string(self.p.tiles.size())
        )
    }

    /// `removeGold` (1332-1339) as a reusable body — mid 27 and
    /// `donateGold` (1147) share it. The `toRemove <= 0n` gate (cross-type
    /// relational, ToNumber, never throws), then `minInt(_gold, toRemove)`
    /// (same rule) and `_gold -= actual` (the mix TypeError whenever both
    /// sides are not bigints). Returns the removed amount.
    fn remove_gold_body(&mut self, o: &Op) -> Result<i64, String> {
        let le0 = match o {
            Op::Big(b) => *b <= 0,
            other => op_to_num(other) <= 0.0,
        };
        if le0 {
            return Ok(0);
        }
        let gold_lt = match (&self.p.gold, o) {
            (Acc::Big(a), Op::Big(b)) => *a < *b,
            (Acc::Big(a), other) => (*a as f64) < op_to_num(other),
            (Acc::Str(s), Op::Big(b)) => {
                crate::game_config_helpers::js_number(s) < *b as f64
            }
            (Acc::Str(_), _) => false,
        };
        if gold_lt {
            // actual = _gold (only ever a Big — the Str rows above are
            // false); _gold -= actual -> 0n.
            if let Acc::Big(a) = self.p.gold {
                self.p.gold = Acc::Big(0);
                return Ok(a);
            }
            unreachable!()
        }
        match (&self.p.gold, o) {
            (Acc::Big(a), Op::Big(b)) => {
                let r = *b;
                self.p.gold = Acc::Big(a - b);
                Ok(r)
            }
            _ => Err(MIX_ERR.to_string()),
        }
    }

    /// `relations.get(other) ?? 0` — token-keyed insertion-ordered map.
    fn relation_get(&self, tok: usize) -> f64 {
        self.p
            .relations
            .iter()
            .find(|(k, _)| *k == tok)
            .map(|(_, v)| *v)
            .unwrap_or(0.0)
    }

    /// `relations.set(other, v)` — an existing key updates in place (the
    /// JS Map keeps its insertion position).
    fn relation_set(&mut self, tok: usize, v: f64) {
        if let Some(e) = self.p.relations.iter_mut().find(|(k, _)| *k == tok) {
            e.1 = v;
        } else {
            self.p.relations.push((tok, v));
        }
    }

    /// `targets()` (992-998) as a reusable body — mid 73 and
    /// `transitiveTargets` (1000) share it. The `ticks()` (102) and
    /// `targetDuration()` (113) facades fire PER element, in that order.
    fn targets_body(&mut self, tr: &mut Vec<f64>) -> Vec<usize> {
        let mut kept = Vec::new();
        for (tick, tok) in self.p.targets_.clone() {
            let t = script_f64(&self.ticks, &mut self.ticks_i, tr, 102.0) - tick;
            if t < script_f64(&self.tdur, &mut self.tdur_i, tr, 113.0) {
                kept.push(tok);
            }
        }
        kept
    }

    /// `canTrade` (1204-1208): the `other.hasEmbargoAgainst(this) ||
    /// this.hasEmbargoAgainst(other)` short-circuit, then the
    /// `other.id() !== this.id()` strict gate.
    fn can_trade(&self, tok: usize) -> bool {
        let embargo = self.other_has_embargo_against(tok) || {
            let oid = self.other_id(tok);
            self.has_embargo_id(&oid)
        };
        !embargo && !js_strict_eq(&self.other_id(tok), &self.pi_field("id"))
    }

    /// `allianceInfo` (772-794): null without an alliance; otherwise the
    /// 102 ticks + 109 offset facades, then the `canExtend` `&&` chain —
    /// `!this.isDisconnected()` (untraced field read), `!other.isDisconnected()`
    /// (105), `this.isAlive()` (untraced), `other.isAlive()` (127),
    /// `inExtensionWindow`, `!agreedToExtend(this)` — each short-circuit
    /// pinning the facade count. The returned object evaluates
    /// `expiresAt()` / `agreedToExtend(...)` again (untraced constants),
    /// key order expiresAt/inExtensionWindow/myPlayerAgreedToExtend/
    /// otherAgreedToExtend/canExtend.
    fn alliance_info(&mut self, tr: &mut Vec<f64>, tok: usize) -> Result<JsVal, String> {
        let a = match self.alliance_with(tok) {
            None => return Ok(JsVal::Null),
            Some(a) => a,
        };
        let t = self.mg_ticks(tr);
        let off = script_f64(&self.off, &mut self.off_i, tr, 109.0);
        let in_window = a.expires_at <= t + off;
        let can_extend = !js_truthy(&self.p.is_disconnected)
            && !self.other_is_disconnected(tr, tok)?
            && self.is_alive()
            && self.other_is_alive(tr, tok)?
            && in_window
            && !self.agreed_to_extend(&a, true, tok);
        Ok(JsVal::Obj(vec![
            ("expiresAt".to_string(), JsVal::Num(a.expires_at)),
            ("inExtensionWindow".to_string(), JsVal::Bool(in_window)),
            (
                "myPlayerAgreedToExtend".to_string(),
                JsVal::Bool(self.agreed_to_extend(&a, true, tok)),
            ),
            (
                "otherAgreedToExtend".to_string(),
                JsVal::Bool(self.agreed_to_extend(&a, false, tok)),
            ),
            ("canExtend".to_string(), JsVal::Bool(can_extend)),
        ]))
    }

    /// `canSendAllianceRequest` (796-841). The `recent` list is the
    /// harness-scripted `pastOutgoingAllianceRequests` filtered to the
    /// recipient and sorted by createdAt descending (stable, NaN → Equal);
    /// the cooldown gate reads 102 then 111 only when `recent` is non-empty.
    fn can_send_alliance_request(
        &mut self,
        tr: &mut Vec<f64>,
        tok: usize,
    ) -> Result<bool, String> {
        let da = script_val(&self.da, &mut self.da_i, tr, 110.0);
        if js_truthy(&da) {
            return Ok(false);
        }
        if self.self_tok == Some(tok) {
            return Ok(false);
        }
        // `this.isDisconnected() || other.isDisconnected()` — this-side is
        // the untraced field read; the 105 facade fires only on a
        // non-disconnected this.
        if js_truthy(&self.p.is_disconnected) || self.other_is_disconnected(tr, tok)? {
            return Ok(false);
        }
        if self.is_friendly(tr, tok, false)? || !self.is_alive() {
            return Ok(false);
        }
        let st = self.self_tok;
        let has_pending = self
            .alliance_requests
            .iter()
            .any(|(req, rec)| Some(*req) == st && *rec == tok);
        if has_pending {
            return Ok(false);
        }
        let has_incoming = self
            .alliance_requests
            .iter()
            .any(|(req, rec)| Some(*rec) == st && *req == tok);
        if has_incoming {
            return Ok(true);
        }
        let mut recent: Vec<(usize, f64)> = self
            .past_outgoing
            .iter()
            .copied()
            .filter(|(rec, _)| *rec == tok)
            .collect();
        js_sort_desc(&mut recent);
        if recent.is_empty() {
            return Ok(true);
        }
        let delta = self.mg_ticks(tr) - recent[0].1;
        let cd = script_f64(&self.arc, &mut self.arc_i, tr, 111.0);
        Ok(delta >= cd)
    }

    /// `allRelationsSorted` (941-949): one 127 facade per relations entry
    /// (insertion order, BEFORE the sort), then the stable ascending sort
    /// and the `relationFromValue` map. Encoded as `[[tok, relation], ...]`.
    fn all_relations_sorted(&mut self, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        Ok(JsVal::Arr(
            self.all_relation_pairs(tr)?
                .into_iter()
                .map(|(tok, v)| JsVal::Arr(vec![JsVal::Num(tok as f64), JsVal::Num(v)]))
                .collect(),
        ))
    }

    /// The `allRelationsSorted` pair list (mid 68 and `playerProfile` share
    /// it): `(token, relationFromValue(v))`, alive-filtered, ascending.
    fn all_relation_pairs(&mut self, tr: &mut Vec<f64>) -> Result<Vec<(usize, f64)>, String> {
        let entries = self.p.relations.clone();
        let mut kept: Vec<(usize, f64)> = Vec::new();
        for (tok, v) in entries {
            if self.other_is_alive(tr, tok)? {
                kept.push((tok, v));
            }
        }
        js_sort_asc(&mut kept);
        Ok(kept
            .into_iter()
            .map(|(tok, v)| (tok, relation_from_value(v)))
            .collect())
    }

    /// `updateRelation` (951-958): `relation + delta` is the JS `+` — a
    /// string-primitive delta (Str / Obj / Arr) concatenates, everything
    /// else adds numerically. `within(v, -100, 100)` = Math.min(Math.max(v,
    /// -100), 100) with NaN propagation.
    fn update_relation(&mut self, tok: usize, delta: &JsVal) {
        let relation = self.relation_get(tok);
        let sum = match delta {
            JsVal::Str(_) | JsVal::Obj(_) | JsVal::Arr(_) => JsVal::Str(format!(
                "{}{}",
                crate::js_fixed::js_to_string(relation),
                js_to_string_val(delta)
            )),
            other => JsVal::Num(relation + to_num(other)),
        };
        let v = crate::jsnum::js_min(
            crate::jsnum::js_max(to_num(&sum), -100.0),
            100.0,
        );
        self.relation_set(tok, v);
    }

    /// `decayRelations` (960-970): `-1 * Math.sign(r)` (NaN → NaN, ±0
    /// preserved), `r += sign * 0.05`, then the `|r| < 0.1` zero snap; the
    /// Map updates in place during the forEach (insertion order).
    fn decay_relations(&mut self) {
        let keys: Vec<usize> = self.p.relations.iter().map(|(k, _)| *k).collect();
        for k in keys {
            let r0 = self.relation_get(k);
            let delta = 0.05;
            let mut r = r0 + -js_sign(r0) * delta;
            if r.abs() < delta * 2.0 {
                r = 0.0;
            }
            self.relation_set(k, r);
        }
    }

    /// `canTarget` (972-985): self gate, `isFriendly` gate, then the
    /// targets_ scan — 102 ticks + 112 targetCooldown per element until the
    /// first cooldown hit.
    fn can_target(&mut self, tr: &mut Vec<f64>, tok: usize) -> Result<bool, String> {
        if self.self_tok == Some(tok) {
            return Ok(false);
        }
        if self.is_friendly(tr, tok, false)? {
            return Ok(false);
        }
        for (tick, _) in self.p.targets_.clone() {
            let t = self.mg_ticks(tr) - tick;
            if t < script_f64(&self.tc, &mut self.tc_i, tr, 112.0) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// `transitiveTargets` (1000-1006): allies via `a.other(this)` (the mid
    /// 49 token flip), their `targets()` are the mocks' constant lists
    /// (untraced), then the real `targets()` body; `[...new Set(ts)]`
    /// dedups by first occurrence.
    fn transitive_targets(&mut self, tr: &mut Vec<f64>) -> Vec<usize> {
        let self_tok = self.self_tok;
        let mut ts: Vec<usize> = Vec::new();
        for a in self.p.alliances.clone() {
            let ally = if self_tok == Some(a.req) { a.rec } else { a.req };
            ts.extend(self.other_targets(ally));
        }
        ts.extend(self.targets_body(tr));
        let mut seen: Vec<usize> = Vec::new();
        for t in ts {
            if !seen.contains(&t) {
                seen.push(t);
            }
        }
        seen
    }

    /// `outgoingEmojis` (1022-1030): 102 ticks + 114 emojiMessageDuration
    /// per stored message, then the stable createdAt-descending sort.
    fn outgoing_emojis_body(&mut self, tr: &mut Vec<f64>) -> JsVal {
        let mut kept: Vec<JsVal> = Vec::new();
        for e in self.p.outgoing_emojis.clone() {
            let created = to_num(&val_field(&e, "createdAt").cloned().unwrap_or(JsVal::Undef));
            let t = self.mg_ticks(tr) - created;
            if t < script_f64(&self.emd, &mut self.emd_i, tr, 114.0) {
                kept.push(e);
            }
        }
        let mut pairs: Vec<(usize, f64)> = kept
            .iter()
            .enumerate()
            .map(|(i, e)| {
                (
                    i,
                    to_num(&val_field(e, "createdAt").cloned().unwrap_or(JsVal::Undef)),
                )
            })
            .collect();
        js_sort_desc(&mut pairs);
        JsVal::Arr(pairs.into_iter().map(|(i, _)| kept[i].clone()).collect())
    }

    /// `canSendEmoji` (1032-1050): the `recipient === this` gate (a
    /// `"AllPlayers"` string never ===), the recipientID compute (untraced),
    /// then per matching message 102 ticks + 115 emojiMessageCooldown until
    /// the first hit.
    fn can_send_emoji(&mut self, tr: &mut Vec<f64>, recip: &JsVal) -> Result<bool, String> {
        if let JsVal::Num(t) = recip {
            if self.self_tok == Some(*t as usize) {
                return Ok(false);
            }
        }
        let rid = self.emoji_recipient_id(recip)?;
        for m in self.p.outgoing_emojis.clone() {
            let mrid = val_field(&m, "recipientID").cloned().unwrap_or(JsVal::Undef);
            if js_strict_eq(&mrid, &rid) {
                let created = to_num(&val_field(&m, "createdAt").cloned().unwrap_or(JsVal::Undef));
                let t = self.mg_ticks(tr) - created;
                if t < script_f64(&self.emc, &mut self.emc_i, tr, 115.0) {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    /// `recipient === AllPlayers ? recipient : recipient.smallID()` — the
    /// AllPlayers string crosses as `Str("AllPlayers")` (Game.ts:61).
    fn emoji_recipient_id(&self, recip: &JsVal) -> Result<JsVal, String> {
        match recip {
            JsVal::Str(s) if s == "AllPlayers" => Ok(JsVal::Str(s.clone())),
            JsVal::Num(t) => Ok(JsVal::Num(self.other_small_id(*t as usize)?)),
            _ => Ok(JsVal::Undef),
        }
    }

    /// `canDonateGold` / `canDonateTroops` (1067-1125): self gate, then
    /// `!this.isAlive() || !recipient.isAlive() || !this.isFriendly(...)`
    /// (127 once, then the isFriendly facades), then the
    /// `type() === Human && config === false` gate (117 gold / 118 troops,
    /// read only for humans), then the sentDonations scan (102 + 119 per
    /// matching donation until the first hit).
    fn can_donate(&mut self, tr: &mut Vec<f64>, tok: usize, gold: bool) -> Result<bool, String> {
        if self.self_tok == Some(tok) {
            return Ok(false);
        }
        if !self.is_alive() || !self.other_is_alive(tr, tok)? || !self.is_friendly(tr, tok, false)?
        {
            return Ok(false);
        }
        if js_strict_eq(&self.other_type(tok), &JsVal::Str("HUMAN".to_string())) {
            let v = if gold {
                take_jsval(&self.dg, &mut self.dg_i)
            } else {
                take_jsval(&self.dt, &mut self.dt_i)
            };
            tr.push(if gold { 117.0 } else { 118.0 });
            push_val(tr, &v);
            if js_strict_eq(&v, &JsVal::Bool(false)) {
                return Ok(false);
            }
        }
        for (rec, tick) in self.p.sent_donations.clone() {
            if rec == tok {
                let t = self.mg_ticks(tr) - tick;
                if t < script_f64(&self.dc, &mut self.dc_i, tr, 119.0) {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    /// The DonateEvent update (GameUpdateType.DonateEvent === 23, zero-based
    /// declaration order), key order type/donationType/senderId/recipientId/
    /// amount. `amount` is a bigint (`BigInt(removed)` / `removed`): the
    /// capture codec has no bigint branch, so it falls into the object branch
    /// and `Object.keys(100n) === []` crosses as an empty object [6,0].
    fn donate_update(&self, kind: &str, tok: usize) -> JsVal {
        JsVal::Obj(vec![
            ("type".to_string(), JsVal::Num(23.0)),
            ("donationType".to_string(), JsVal::Str(kind.to_string())),
            ("senderId".to_string(), self.pi_field("id")),
            ("recipientId".to_string(), self.other_id(tok)),
            ("amount".to_string(), JsVal::Obj(vec![])),
        ])
    }

    /// `canEmbargoAll` (1178-1194): 102 + 121 cooldown gate, then the
    /// `mg.players()` scan (130 once) — `p.id() === this.id()` and
    /// `p.type() === PlayerType.Bot` are untraced constant reads, the
    /// `isOnSameTeam(p)` continue carries the 106 facade counts.
    fn can_embargo_all(&mut self, tr: &mut Vec<f64>) -> Result<bool, String> {
        let t = self.mg_ticks(tr) - self.p.last_embargo_all_tick;
        if t < script_f64(&self.eac, &mut self.eac_i, tr, 121.0) {
            return Ok(false);
        }
        let players = self.mg_players(tr);
        let my_id = self.pi_field("id");
        for tok in players {
            if js_strict_eq(&self.other_id(tok), &my_id) {
                continue;
            }
            if js_strict_eq(&self.other_type(tok), &JsVal::Str("BOT".to_string())) {
                continue;
            }
            if self.same_team(tr, tok)? {
                continue;
            }
            return Ok(true);
        }
        Ok(false)
    }

    /// `addEmbargo` (1214-1230): a non-temporary existing entry returns
    /// BEFORE the facade; otherwise the EmbargoEvent "start" update (101)
    /// fires FIRST, then `createdAt: mg.ticks()` (102) inside the stored
    /// object. GameUpdateType.EmbargoEvent === 20.
    fn add_embargo(&mut self, tr: &mut Vec<f64>, tok: usize, temp: JsVal) -> Result<(), String> {
        let oid = self.other_id(tok);
        if let Some((_, e)) = self.p.embargoes.iter().find(|(k, _)| js_strict_eq(k, &oid)) {
            if !js_truthy(&e.is_temporary) {
                return Ok(());
            }
        }
        let u = JsVal::Obj(vec![
            ("type".to_string(), JsVal::Num(20.0)),
            ("event".to_string(), JsVal::Str("start".to_string())),
            ("playerID".to_string(), JsVal::Num(self.p.small_id)),
            (
                "embargoedID".to_string(),
                JsVal::Num(self.other_small_id(tok)?),
            ),
        ]);
        self.mg_add_update(tr, &u);
        let created = self.mg_ticks(tr);
        let e = Embargo {
            created_at: created,
            is_temporary: temp,
            target: tok,
        };
        if let Some(slot) = self.p.embargoes.iter_mut().find(|(k, _)| js_strict_eq(k, &oid)) {
            slot.1 = e;
        } else {
            self.p.embargoes.push((oid, e));
        }
        Ok(())
    }

    /// `stopEmbargo` (1232-1240): the Map delete THEN the unconditional
    /// EmbargoEvent "stop" update (101).
    fn stop_embargo(&mut self, tr: &mut Vec<f64>, tok: usize) -> Result<(), String> {
        let oid = self.other_id(tok);
        if let Some(pos) = self.p.embargoes.iter().position(|(k, _)| js_strict_eq(k, &oid)) {
            self.p.embargoes.remove(pos);
        }
        let u = JsVal::Obj(vec![
            ("type".to_string(), JsVal::Num(20.0)),
            ("event".to_string(), JsVal::Str("stop".to_string())),
            ("playerID".to_string(), JsVal::Num(self.p.small_id)),
            (
                "embargoedID".to_string(),
                JsVal::Num(self.other_small_id(tok)?),
            ),
        ]);
        self.mg_add_update(tr, &u);
        Ok(())
    }

    // --- G3c4: units, hash, profile and the toUpdate surface ---

    /// `isDecaying` — the mid 58 body (the toFullUpdate literal fires the
    /// same 102 facade when armed).
    fn is_decaying_body(&mut self, tr: &mut Vec<f64>) -> bool {
        if !self.in_doomsday() || self.p.rotted_at_tick < 0.0 {
            false
        } else {
            (self.mg_ticks(tr) - self.p.rotted_at_tick) <= DECAY_CUE_GRACE_TICKS
        }
    }

    /// `units()` (430-490) — the match set. The three memos are version-
    /// gated in TS: a hit returns exactly what a re-walk produces, so the
    /// port always re-walks (see the module doc). `types` is `None` for the
    /// no-arg / empty-array overloads (`this._units` verbatim).
    fn units_filtered(&self, types: Option<&[String]>) -> Vec<usize> {
        match types {
            None => (0..self.p.units.len()).collect(),
            Some(ts) => self
                .p
                .units
                .iter()
                .enumerate()
                .filter(|(_, u)| ts.contains(&u.typ))
                .map(|(i, _)| i)
                .collect(),
        }
    }

    /// The units wire form: each mock crosses as `[type, level, uc]`.
    fn units_wire(&self, idxs: Vec<usize>) -> JsVal {
        JsVal::Arr(
            idxs.into_iter()
                .map(|i| {
                    let u = &self.p.units[i];
                    JsVal::Arr(vec![
                        JsVal::Str(u.typ.clone()),
                        JsVal::Num(u.level),
                        JsVal::Bool(u.uc),
                    ])
                })
                .collect(),
        )
    }

    /// `recordUnitConstructed` (493-499): the insertion-ordered upsert.
    fn record_unit_constructed(&mut self, typ: String) {
        match self
            .p
            .num_units_constructed
            .iter_mut()
            .find(|(t, _)| *t == typ)
        {
            Some((_, n)) => *n += 1.0,
            None => self.p.num_units_constructed.push((typ, 1.0)),
        }
    }

    /// `unitsConstructed` (505-507): `numUnitsConstructed[type] ?? 0`.
    fn units_constructed(&self, typ: &str) -> f64 {
        self.p
            .num_units_constructed
            .iter()
            .find(|(t, _)| t == typ)
            .map(|(_, n)| *n)
            .unwrap_or(0.0)
    }

    /// `unitCount` (510-528): Σ `unit.level()` per matching type.
    fn unit_count(&self, typ: &str) -> f64 {
        self.p
            .units
            .iter()
            .filter(|u| u.typ == typ)
            .map(|u| u.level)
            .sum()
    }

    /// `unitsOwned` (531-551): under-construction units count once,
    /// otherwise `level()`.
    fn units_owned(&self, typ: &str) -> f64 {
        self.p
            .units
            .iter()
            .filter(|u| u.typ == typ)
            .map(|u| if u.uc { 1.0 } else { u.level })
            .sum()
    }

    /// `hash()` (1828-1833): `simpleHash(this.id()) * (this.troops() +
    /// this.numTilesOwned())` plus the units reduce (left fold from 0, each
    /// `unit.hash()` traced as 131). The id read throws on null / undefined
    /// exactly like the ctor's seed read.
    fn hash_body(&mut self, tr: &mut Vec<f64>) -> Result<f64, String> {
        let id = self.pi_field("id");
        let base = match &id {
            JsVal::Null => {
                return Err("Cannot read properties of null (reading 'length')".to_string())
            }
            JsVal::Undef | JsVal::Absent => {
                return Err("Cannot read properties of undefined (reading 'length')".to_string())
            }
            _ => crate::util::simple_hash(&js_to_string_val(&id)),
        };
        let mut acc = base * (self.p.troops as f64 + self.p.tiles.size());
        for u in self.p.units.clone() {
            acc += self.unit_hash(tr, u.hash);
        }
        Ok(acc)
    }

    /// `playerProfile()` (1842-1853): `Object.fromEntries` over
    /// allRelationsSorted — key `String(player.smallID())`, value the mapped
    /// relation — plus the alliance others' smallIDs (in alliance order).
    fn player_profile(&mut self, tr: &mut Vec<f64>) -> Result<JsVal, String> {
        let pairs = self.all_relation_pairs(tr)?;
        let mut relations: Vec<(String, JsVal)> = Vec::new();
        for (tok, v) in pairs {
            let sid = self.other_small_id(tok)?;
            relations.push((crate::js_fixed::js_to_string(sid), JsVal::Num(v)));
        }
        // `Object.fromEntries` builds a plain object: smallID keys are
        // canonical array indices, so JS re-numbers them ascending ahead of
        // any ordinary string keys — the allRelationsSorted order only
        // survives for the string-key tail.
        let relations = js_integer_key_order(relations);
        let self_tok = self.self_tok;
        let toks: Vec<usize> = self
            .p
            .alliances
            .iter()
            .map(|a| if self_tok == Some(a.req) { a.rec } else { a.req })
            .collect();
        let mut alliances: Vec<JsVal> = Vec::new();
        for t in toks {
            alliances.push(JsVal::Num(self.other_small_id(t)?));
        }
        Ok(JsVal::Obj(vec![
            ("relations".to_string(), JsVal::Obj(relations)),
            ("alliances".to_string(), JsVal::Arr(alliances)),
        ]))
    }

    /// `canAttackPlayer` (1893-1902): non-human attackers skip the immunity
    /// probe entirely; humans probe `player.isImmune()` FIRST (the
    /// short-circuit pins the facade count) then isFriendly.
    fn can_attack_player(
        &mut self,
        tr: &mut Vec<f64>,
        tok: usize,
        treat: bool,
    ) -> Result<bool, String> {
        let human = js_strict_eq(
            &self.pi_field("playerType"),
            &JsVal::Str("HUMAN".to_string()),
        );
        if !human {
            return Ok(!self.is_friendly(tr, tok, treat)?);
        }
        Ok(!self.other_is_immune(tr, tok)? && !self.is_friendly(tr, tok, treat)?)
    }

    /// The attack wire/typed pair for one mock: `attacker().smallID()` /
    /// `target().smallID()` (untraced identity reads), `troops()`, `id()`,
    /// `retreating()`.
    fn attack_parts(
        &self,
        a: &AttackMock,
    ) -> Result<(JsVal, AttackUpdate), String> {
        let att = self.other_small_id(a.att)?;
        let tgt = self.other_small_id(a.tgt)?;
        Ok((
            JsVal::Obj(vec![
                ("attackerID".to_string(), JsVal::Num(att)),
                ("targetID".to_string(), JsVal::Num(tgt)),
                ("troops".to_string(), JsVal::Num(a.troops)),
                ("id".to_string(), JsVal::Str(a.id.clone())),
                ("retreating".to_string(), JsVal::Bool(a.ret)),
            ]),
            AttackUpdate {
                attacker_id: att,
                target_id: tgt,
                troops: a.troops,
                id: a.id.clone(),
                retreating: a.ret,
            },
        ))
    }

    /// `toFullUpdate` (PlayerImpl.ts:267-401) in all three worlds at once:
    /// the typed snapshot (`diff_player_update` / `pack_attack_troop_deltas`
    /// run over it verbatim), the per-field wire encodings indexed by the
    /// diffPlayerUpdate emission order, and the full literal (the first-call
    /// wire form). Facade order: the alliance cutoff (102 + 109, only when
    /// alliances exist), `targets()` (102/113 per entry),
    /// `outgoingEmojis()` (102/114 per entry), the incoming-attack alive
    /// filter (127 per entry), `getPlayerStats` (138, always), then the
    /// literal — `isTraitor` / `traitorRemainingTicks` each fire the traitor
    /// facades when marked, `isDecaying` fires 102 when armed.
    #[allow(clippy::wrong_self_convention)] // TS name (toFullUpdate); &mut self drives the facades
    fn to_full_update_parts(&mut self, tr: &mut Vec<f64>) -> Result<FullParts, String> {
        // Each materialized array is a fresh JS object per call, so ids must
        // never repeat within a scenario (a repeated id would let a
        // different-object pair hit a ref fast path).
        let mut fresh = self.p.next_refid;
        let self_tok = self.self_tok;

        // outgoingAllianceRequests (273-281): `ar.requestor() === this`.
        let mut oar: Option<Vec<JsVal>> = None;
        for (req, rec) in self.alliance_requests.clone() {
            if Some(req) == self_tok {
                oar.get_or_insert_with(Vec::new).push(self.other_id(rec));
            }
        }
        let oar_refid = match oar {
            None => REF_EMPTY_STR,
            Some(_) => {
                let r = fresh;
                fresh += 1.0;
                r
            }
        };

        // allies + alliance views (283-300): the cutoff facades only run
        // when alliances exist.
        let alliances = self.p.alliances.clone();
        let others: Vec<usize> = alliances
            .iter()
            .map(|a| if self_tok == Some(a.req) { a.rec } else { a.req })
            .collect();
        let mut allies_items: Vec<f64> = Vec::new();
        for t in &others {
            allies_items.push(self.other_small_id(*t)?);
        }
        let mut alv_wire: Vec<JsVal> = Vec::new();
        let mut views: Vec<TypedAllianceView> = Vec::new();
        let (allies_refid, alv_refid) = if alliances.is_empty() {
            (REF_EMPTY_NUM, REF_EMPTY_ALV)
        } else {
            let t = self.mg_ticks(tr);
            let off = script_f64(&self.off, &mut self.off_i, tr, 109.0);
            for (a, tok) in alliances.iter().zip(others.iter()) {
                let oid = self.other_id(*tok);
                views.push(TypedAllianceView {
                    id: a.id,
                    other: oar_typed(&oid),
                    created_at: a.created_at,
                    expires_at: a.expires_at,
                    has_extension_request: a.expires_at <= t + off,
                });
                alv_wire.push(JsVal::Obj(vec![
                    ("id".to_string(), JsVal::Num(a.id)),
                    ("other".to_string(), oid),
                    ("createdAt".to_string(), JsVal::Num(a.created_at)),
                    ("expiresAt".to_string(), JsVal::Num(a.expires_at)),
                    (
                        "hasExtensionRequest".to_string(),
                        JsVal::Bool(a.expires_at <= t + off),
                    ),
                ]));
            }
            let ar = fresh;
            fresh += 1.0;
            let vr = fresh;
            fresh += 1.0;
            (ar, vr)
        };

        // embargoes (302-308): the keys `toString()`ed into a Set.
        let emb_items: Vec<String> = self
            .p
            .embargoes
            .iter()
            .map(|(k, _)| js_to_string_val(k))
            .collect();
        let emb_refid = if emb_items.is_empty() {
            REF_EMPTY_EMB
        } else {
            let r = fresh;
            fresh += 1.0;
            r
        };

        // targets (310-316): the gate reads `targets_.length`, then the
        // filtered view maps to smallIDs.
        let mut targets_items: Vec<f64> = Vec::new();
        if !self.p.targets_.is_empty() {
            for t in self.targets_body(tr) {
                targets_items.push(self.other_small_id(t)?);
            }
        }
        let targets_refid = if targets_items.is_empty() {
            REF_EMPTY_NUM
        } else {
            let r = fresh;
            fresh += 1.0;
            r
        };

        // outgoingEmojis (318-324): the stored objects pass through as-is.
        let mut emojis_wire: Vec<JsVal> = Vec::new();
        let mut emojis_typed: Vec<TypedEmoji> = Vec::new();
        if !self.p.outgoing_emojis.is_empty() {
            let e = self.outgoing_emojis_body(tr);
            if let JsVal::Arr(items) = e {
                if !items.is_empty() {
                    for it in items {
                        emojis_typed.push(emoji_typed(&it));
                        emojis_wire.push(it);
                    }
                }
            }
        }
        let emojis_refid = if emojis_wire.is_empty() {
            REF_EMPTY_EMO
        } else {
            let r = fresh;
            fresh += 1.0;
            r
        };

        // outgoingAttacks (326-337): EMPTY vs the mapped array.
        let mut oa_wire: Vec<JsVal> = Vec::new();
        let mut oa_typed: Vec<AttackUpdate> = Vec::new();
        for a in &self.p.out_attacks {
            let (w, t) = self.attack_parts(a)?;
            oa_wire.push(w);
            oa_typed.push(t);
        }
        let oa_refid = if oa_wire.is_empty() {
            REF_EMPTY_ATK
        } else {
            let r = fresh;
            fresh += 1.0;
            r
        };

        // incomingAttacks (339-353): the alive filter (127 per entry) runs
        // over every stored attack, the map only over the kept ones.
        let mut ia_wire: Vec<JsVal> = Vec::new();
        let mut ia_typed: Vec<AttackUpdate> = Vec::new();
        for a in self.p.in_attacks.clone() {
            if self.other_is_alive(tr, a.att)? {
                let (w, t) = self.attack_parts(&a)?;
                ia_wire.push(w);
                ia_typed.push(t);
            }
        }
        let ia_refid = if ia_wire.is_empty() {
            REF_EMPTY_ATK
        } else {
            let r = fresh;
            fresh += 1.0;
            r
        };
        self.p.next_refid = fresh;

        // deathStats (358): `deathStats?.killedBy ?? null` /
        // `deathStats?.deathPosition ?? null`.
        let gs = self.mg_get_player_stats(tr);
        let norm_stat = |v: Option<JsVal>| match v {
            Some(JsVal::Undef) | Some(JsVal::Absent) | None => JsVal::Null,
            Some(other) => other,
        };
        let (killed_by, death_position) = match &gs {
            JsVal::Null | JsVal::Undef | JsVal::Absent => (JsVal::Null, JsVal::Null),
            _ => (
                norm_stat(val_field(&gs, "killedBy").cloned()),
                norm_stat(val_field(&gs, "deathPosition").cloned()),
            ),
        };

        // The literal's facade calls (364-399): isTraitor + traitor ticks
        // probe the marked tick twice, isDecaying probes the rot.
        let traitor = self.traitor_remaining(tr) > 0.0;
        let trt = self.traitor_remaining(tr);
        let doomsday = self.in_doomsday();
        let decaying = self.is_decaying_body(tr);
        let spawned = self.p.spawn_tile != JsVal::Undef;

        let team_wire = match &self.p.team {
            JsVal::Null | JsVal::Undef | JsVal::Absent => None,
            v => Some(v.clone()),
        };
        let allies_wire = JsVal::Arr(allies_items.iter().map(|n| JsVal::Num(*n)).collect());
        let emb_wire = JsVal::Arr(emb_items.iter().map(|s| JsVal::Str(s.clone())).collect());
        let targets_wire = JsVal::Arr(targets_items.iter().map(|n| JsVal::Num(*n)).collect());
        let oar_wire = JsVal::Arr(oar.unwrap_or_default());

        let typed = PlayerUpdate {
            id: match self.pi_field("id") {
                JsVal::Str(s) => s,
                _ => String::new(),
            },
            client_id: pi_opt3(&self.pi_field("clientID")),
            name: pi_opt3(&self.pi_field("name")),
            display_name: pi_opt3(&self.pi_field("displayName")),
            clan_tag: pi_opt3(&self.pi_field("clanTag")),
            nation_flag: pi_opt3(&self.pi_field("nationFlag")),
            // `this.team() ?? undefined` — null folds to undefined.
            team: match team_wire.clone() {
                Some(JsVal::Str(s)) => Some(Some(s)),
                _ => None,
            },
            small_id: Some(Some(self.p.small_id)),
            player_type: pi_opt3(&self.pi_field("playerType")),
            is_alive: Some(Some(self.is_alive())),
            // markDisconnected stores the raw arg; the typed side folds
            // truthiness (scenarios only pass booleans).
            is_disconnected: Some(Some(js_truthy(&self.p.is_disconnected))),
            killed_by: match &killed_by {
                JsVal::Str(s) => Some(Some(s.clone())),
                JsVal::Null => Some(None),
                // `?? null` only folds null/undefined; the remaining shapes
                // never reach a typed String in the scenario domain.
                _ => Some(None),
            },
            death_position: match &death_position {
                JsVal::Num(n) => Some(Some(*n)),
                JsVal::Null => Some(None),
                _ => Some(None),
            },
            tiles_owned: Some(Some(self.p.tiles.size())),
            gold: Some(Some(acc_num(&self.p.gold))),
            trade_gold: Some(Some(acc_num(&self.p.trade_gold))),
            train_gold: Some(Some(acc_num(&self.p.train_gold))),
            piracy_gold: Some(Some(acc_num(&self.p.piracy_gold))),
            gold_earned: Some(Some(acc_num(&self.p.gold_earned))),
            troops: Some(Some(self.p.troops as f64)),
            allies: Some(Arr {
                refid: allies_refid,
                items: allies_items,
            }),
            embargoes: Some(Arr {
                refid: emb_refid,
                items: emb_items,
            }),
            is_traitor: Some(Some(traitor)),
            traitor_remaining_ticks: Some(Some(trt)),
            in_doomsday_clock: Some(Some(doomsday)),
            is_decaying: Some(Some(decaying)),
            marked_doomsday_clock_tick: Some(Some(self.p.marked_doomsday_clock_tick)),
            targets: Some(Arr {
                refid: targets_refid,
                items: targets_items,
            }),
            outgoing_emojis: Some(Arr {
                refid: emojis_refid,
                items: emojis_typed,
            }),
            outgoing_attacks: Some(Arr {
                refid: oa_refid,
                items: oa_typed,
            }),
            incoming_attacks: Some(Arr {
                refid: ia_refid,
                items: ia_typed,
            }),
            outgoing_alliance_requests: match &oar_wire {
                JsVal::Arr(items) => Some(Arr {
                    refid: oar_refid,
                    items: items.iter().map(oar_typed).collect(),
                }),
                _ => unreachable!(),
            },
            alliances: Some(Arr {
                refid: alv_refid,
                items: views,
            }),
            has_spawned: Some(Some(spawned)),
            spawn_tile: match &self.p.spawn_tile {
                JsVal::Num(n) => Some(Some(*n)),
                JsVal::Null => Some(None),
                _ => None,
            },
            betrayals: Some(Some(self.p.betrayal_count)),
            last_delete_unit_tick: Some(Some(self.p.last_delete_unit_tick)),
            is_lobby_creator: pi_opt3_bool(&self.pi_field("isLobbyCreator")),
        };

        // The literal (360-401) in key order; normPU drops undefined values.
        let mut full: Vec<(String, JsVal)> = vec![("type".to_string(), JsVal::Num(2.0))];
        let put = |keys: &mut Vec<(String, JsVal)>, k: &str, v: Option<JsVal>| {
            if let Some(v) = v {
                keys.push((k.to_string(), v));
            }
        };
        put(&mut full, "clientID", pi_wire(&self.pi_field("clientID")));
        put(&mut full, "name", pi_wire(&self.pi_field("name")));
        put(
            &mut full,
            "displayName",
            pi_wire(&self.pi_field("displayName")),
        );
        put(&mut full, "clanTag", pi_wire(&self.pi_field("clanTag")));
        put(
            &mut full,
            "nationFlag",
            pi_wire(&self.pi_field("nationFlag")),
        );
        put(&mut full, "id", pi_wire(&self.pi_field("id")));
        put(&mut full, "team", team_wire.clone());
        full.push(("smallID".to_string(), JsVal::Num(self.p.small_id)));
        put(
            &mut full,
            "playerType",
            pi_wire(&self.pi_field("playerType")),
        );
        full.push(("isAlive".to_string(), JsVal::Bool(self.is_alive())));
        put(
            &mut full,
            "isDisconnected",
            pi_wire(&self.p.is_disconnected),
        );
        full.push(("killedBy".to_string(), killed_by.clone()));
        full.push(("deathPosition".to_string(), death_position.clone()));
        full.push(("tilesOwned".to_string(), JsVal::Num(self.p.tiles.size())));
        full.push(("gold".to_string(), self.p.gold.to_jsval()));
        full.push(("tradeGold".to_string(), self.p.trade_gold.to_jsval()));
        full.push(("trainGold".to_string(), self.p.train_gold.to_jsval()));
        full.push(("piracyGold".to_string(), self.p.piracy_gold.to_jsval()));
        full.push(("goldEarned".to_string(), self.p.gold_earned.to_jsval()));
        full.push(("troops".to_string(), JsVal::Num(self.p.troops as f64)));
        full.push(("allies".to_string(), allies_wire.clone()));
        full.push(("embargoes".to_string(), emb_wire.clone()));
        full.push(("isTraitor".to_string(), JsVal::Bool(traitor)));
        full.push(("traitorRemainingTicks".to_string(), JsVal::Num(trt)));
        full.push(("inDoomsdayClock".to_string(), JsVal::Bool(doomsday)));
        full.push(("isDecaying".to_string(), JsVal::Bool(decaying)));
        full.push((
            "markedDoomsdayClockTick".to_string(),
            JsVal::Num(self.p.marked_doomsday_clock_tick),
        ));
        full.push(("targets".to_string(), targets_wire.clone()));
        full.push((
            "outgoingEmojis".to_string(),
            JsVal::Arr(emojis_wire.clone()),
        ));
        full.push(("outgoingAttacks".to_string(), JsVal::Arr(oa_wire.clone())));
        full.push((
            "incomingAttacks".to_string(),
            JsVal::Arr(ia_wire.clone()),
        ));
        full.push(("outgoingAllianceRequests".to_string(), oar_wire.clone()));
        full.push(("alliances".to_string(), JsVal::Arr(alv_wire.clone())));
        full.push(("hasSpawned".to_string(), JsVal::Bool(spawned)));
        put(&mut full, "spawnTile", pi_wire(&self.p.spawn_tile));
        full.push(("betrayals".to_string(), JsVal::Num(self.p.betrayal_count)));
        full.push((
            "lastDeleteUnitTick".to_string(),
            JsVal::Num(self.p.last_delete_unit_tick),
        ));
        put(
            &mut full,
            "isLobbyCreator",
            pi_wire(&self.pi_field("isLobbyCreator")),
        );

        // The per-field wire encodings, indexed by the diffPlayerUpdate
        // emission order (the diff never reads tilesOwned/gold/troops/
        // goldEarned, so they are absent here).
        let fields: Vec<Option<JsVal>> = vec![
            pi_wire(&self.pi_field("clientID")),
            pi_wire(&self.pi_field("name")),
            pi_wire(&self.pi_field("displayName")),
            pi_wire(&self.pi_field("clanTag")),
            pi_wire(&self.pi_field("nationFlag")),
            team_wire,
            Some(JsVal::Num(self.p.small_id)),
            pi_wire(&self.pi_field("playerType")),
            Some(JsVal::Bool(self.is_alive())),
            pi_wire(&self.p.is_disconnected),
            Some(killed_by),
            Some(death_position),
            Some(self.p.trade_gold.to_jsval()),
            Some(self.p.train_gold.to_jsval()),
            Some(self.p.piracy_gold.to_jsval()),
            Some(JsVal::Bool(traitor)),
            Some(JsVal::Num(trt)),
            Some(JsVal::Bool(doomsday)),
            Some(JsVal::Num(self.p.marked_doomsday_clock_tick)),
            Some(JsVal::Bool(decaying)),
            Some(JsVal::Bool(spawned)),
            pi_wire(&self.p.spawn_tile),
            Some(JsVal::Num(self.p.betrayal_count)),
            Some(JsVal::Num(self.p.last_delete_unit_tick)),
            pi_wire(&self.pi_field("isLobbyCreator")),
            Some(allies_wire),
            Some(targets_wire),
            Some(oar_wire),
            Some(emb_wire),
            Some(JsVal::Arr(emojis_wire)),
            Some(JsVal::Arr(oa_wire)),
            Some(JsVal::Arr(ia_wire)),
            Some(JsVal::Arr(alv_wire)),
        ];

        Ok(FullParts {
            typed,
            fields,
            full: JsVal::Obj(full),
        })
    }

    /// `toUpdate` (222-265): the full snapshot always lands in
    /// `lastSentUpdate`; the first call returns it verbatim (no quint, no
    /// pack), later calls the real diff — recovered as a wire JsVal from the
    /// changed indices so the changed-field set always comes from the real
    /// comparator set — plus the stats quint and the attack-troop quads.
    #[allow(clippy::wrong_self_convention)] // TS name (toUpdate); &mut self drives the facades
    fn to_update(
        &mut self,
        tr: &mut Vec<f64>,
        stats_en: bool,
        atk_en: bool,
    ) -> Result<(JsVal, Vec<f64>, Vec<f64>), String> {
        let parts = self.to_full_update_parts(tr)?;
        let prev = self.p.last_sent.replace(parts.typed.clone());
        let Some(prev) = prev else {
            return Ok((parts.full, Vec::new(), Vec::new()));
        };
        let mut so: Vec<f64> = Vec::new();
        let mut ao: Vec<f64> = Vec::new();
        // The quint gate compares the raw values with `!==`; the typed
        // gold/goldEarned are the `Number(...)` folds, exact for the bigint
        // domain the scenarios keep gold in.
        if stats_en
            && (prev.tiles_owned != parts.typed.tiles_owned
                || prev.gold != parts.typed.gold
                || prev.troops != parts.typed.troops
                || prev.gold_earned != parts.typed.gold_earned)
        {
            so.extend([
                self.p.small_id,
                opt3_num(&parts.typed.tiles_owned),
                opt3_num(&parts.typed.gold),
                opt3_num(&parts.typed.troops),
                opt3_num(&parts.typed.gold_earned),
            ]);
        }
        if atk_en {
            pack_attack_troop_deltas(
                &prev.outgoing_attacks,
                &parts.typed.outgoing_attacks,
                self.p.small_id,
                ATTACK_DELTA_OUTGOING,
                &mut ao,
            );
            pack_attack_troop_deltas(
                &prev.incoming_attacks,
                &parts.typed.incoming_attacks,
                self.p.small_id,
                ATTACK_DELTA_INCOMING,
                &mut ao,
            );
        }
        let stream = diff_player_update(&prev, &parts.typed);
        let v = if stream.first().is_some_and(|&m| m != 0.0) {
            let mut keys: Vec<(String, JsVal)> = vec![("type".to_string(), JsVal::Num(2.0))];
            if let Some(idv) = pi_wire(&self.pi_field("id")) {
                keys.push(("id".to_string(), idv));
            }
            for i in diff_changed_indices(&stream) {
                if let Some(w) = &parts.fields[i as usize] {
                    keys.push((DIFF_FIELD_NAMES[i as usize].to_string(), w.clone()));
                }
            }
            JsVal::Obj(keys)
        } else {
            JsVal::Null
        };
        Ok((v, so, ao))
    }

    // --- the op dispatcher ---

    /// Run one op. Kind table (mirrored by `tools/gen_vectors.mjs`):
    /// 0 construct `[encVal playerInfo, smallID, encVal startTroops, encVal
    /// team, nAlliances, (req,rec,expiresAt,reqAgree,recAgree)*, nTiles,(t)*,
    /// nBorder,(t)*, sgScript, ticksScript, teamTilesScript, totalLandScript,
    /// playersBlock(pid, teamList, discList, encS type, aliveList, embVals,
    /// tgts)*, selfTok, nExpired, (req,rec)*, traitorDurationScript,
    /// nAllianceReqs, (req,rec)*, nPastOutgoing, (rec,createdAt)*,
    /// playersList, offScript, daScript, arcScript, tcScript, tdurScript,
    /// emdScript, emcScript, qcScript, dgScript, dtScript, dcScript,
    /// ducScript, eacScript, carScript]`
    /// -> `[traceLen,(trace)*,0]` or `[traceLen,(trace)*,1,encS]`; kind 1
    /// method `[mid,...]` -> `[traceLen,(trace)*,[0,...encVal] | [1,encS]]`.
    /// mid follows the TS declaration order: 0 smallID 1 name 2 displayName
    /// 3 clanTag 4 nationFlag 5 clientID 6 id 7 type 8 info 9
    /// isLobbyCreator 10 isAlive 11 hasSpawned 12 setSpawnTile 13 spawnTile
    /// 14 troops 15 setTroops 16 addTroops 17 removeTroops 18 gold 19
    /// tradeGold 20 addTradeGold 21 trainGold 22 addTrainGold 23 piracyGold
    /// 24 addPiracyGold 25 goldEarned 26 addGold 27 removeGold 28
    /// tileChangeVersion 29 lastTileChange 30 isDisconnected 31
    /// markDisconnected 32 disconnectSnapshot 33 disconnectedAtTick 34 team
    /// 35 isOnSameTeam 36 isFriendly 37 isAlliedWith 38 allianceWith 39
    /// alliances 40 numTilesOwned 41 dumpTiles 42 addTile 43 removeTile 44
    /// dumpBorderTiles 45 addBorderTile 46 removeBorderTile 47 isPlayer
    /// (G3c2:) 48 expiredAlliances 49 allies 50 isTraitor 51
    /// getTraitorRemainingTicks 52 markTraitor 53 inDoomsdayClock 54
    /// doomsdayClockTicks 55 enterDoomsdayClock 56 clearDoomsdayClock 57
    /// markRotted 58 isDecaying 59 betrayals. (G3c3:) 60
    /// incomingAllianceRequests 61 outgoingAllianceRequests 62 allianceInfo
    /// 63 canSendAllianceRequest 64 breakAlliance 65 removeAllAlliances 66
    /// createAllianceRequest 67 relation 68 allRelationsSorted 69
    /// updateRelation 70 decayRelations 71 canTarget 72 target 73 targets 74
    /// transitiveTargets 75 sendEmoji 76 outgoingEmojis 77 canSendEmoji 78
    /// canSendQuickChat 79 recordQuickChat 80 canDonateGold 81
    /// canDonateTroops 82 donateTroops 83 donateGold 84 canDeleteUnit 85
    /// recordDeleteUnit 86 canEmbargoAll 87 recordEmbargoAll 88
    /// hasEmbargoAgainst 89 canTrade 90 getEmbargoes 91 addEmbargo 92
    /// stopEmbargo 93 endTemporaryEmbargo 94 tradingPartners.
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut c = Cur(args, 0);
        let mut tr: Vec<f64> = Vec::new();
        let mut out: Vec<f64> = Vec::new();
        match kind {
            0 => {
                self.reset();
                let pi = c.val();
                let small_id = c.f();
                let start_troops = c.val();
                let team = c.val();
                let n_all = c.u();
                let alliances: Vec<AllianceMock> = (0..n_all)
                    .map(|_| AllianceMock {
                        req: c.u(),
                        rec: c.u(),
                        expires_at: c.f(),
                        req_agree: c.f() != 0.0,
                        rec_agree: c.f() != 0.0,
                        // G3c4: the alliance mock's id()/createdAt().
                        id: c.f(),
                        created_at: c.f(),
                    })
                    .collect();
                let tiles = c.list();
                let border = c.list();
                let sg = c.list();
                let ticks = c.list();
                let team_tiles = c.list();
                let total_land = c.list();
                let n = c.u();
                let players = (0..n)
                    .map(|_| {
                        let pid = c.f();
                        let teams = c.team_list();
                        let discs = c.list();
                        let typ = c.s();
                        let alives = c.list();
                        let emb = c.val_list();
                        let tgts = (0..c.u()).map(|_| c.u()).collect();
                        // G3c4: the mock's isImmune() FIFO (event 137).
                        let immune = c.list();
                        PlPlayer {
                            pid,
                            teams,
                            team_i: 0,
                            discs,
                            disc_i: 0,
                            typ,
                            alives,
                            alive_i: 0,
                            emb,
                            tgts,
                            immune,
                            immune_i: 0,
                        }
                    })
                    .collect();
                let self_tok = c.f();
                let n_exp = c.u();
                let expired_alliances: Vec<(usize, usize)> =
                    (0..n_exp).map(|_| (c.u(), c.u())).collect();
                let traitor_duration = c.list();
                // G3c3 tail: the mg social surface and the config scripts.
                let alliance_requests = (0..c.u()).map(|_| (c.u(), c.u())).collect();
                let past_outgoing = (0..c.u()).map(|_| (c.u(), c.f())).collect();
                let players_list = (0..c.u()).map(|_| c.u()).collect();
                let off = c.list();
                let da = c.val_list();
                let arc = c.list();
                let tc = c.list();
                let tdur = c.list();
                let emd = c.list();
                let emc = c.list();
                let qc = c.list();
                let dg = c.val_list();
                let dt = c.val_list();
                let dc = c.list();
                let duc = c.list();
                let eac = c.list();
                let car = c.val_list();
                // G3c4 tail: units, attacks, and the mg surfaces
                // toFullUpdate reads (getPlayerStats / owner / the two
                // spawn-immunity flags).
                fn atk_block(c: &mut Cur) -> Vec<AttackMock> {
                    (0..c.u())
                        .map(|_| {
                            let att = c.u();
                            let tgt = c.u();
                            let troops = c.f();
                            let id = c.s();
                            let ret = c.f() != 0.0;
                            AttackMock {
                                att,
                                tgt,
                                troops,
                                id,
                                ret,
                            }
                        })
                        .collect()
                }
                let units: Vec<UnitMock> = (0..c.u())
                    .map(|_| UnitMock {
                        typ: c.s(),
                        id: c.f(),
                        owner_sid: c.f(),
                        level: c.f(),
                        uc: c.f() != 0.0,
                        active: c.f() != 0.0,
                        cooldown: c.f() != 0.0,
                        tile: c.f(),
                        mfd: c.f() != 0.0,
                        hash: c.f(),
                        queue: Vec::new(),
                    })
                    .collect();
                let out_attacks = atk_block(&mut c);
                let in_attacks = atk_block(&mut c);
                let gs = c.val_list();
                let owner = c.val_list();
                let sia = c.list();
                let nsia = c.list();
                // G3c5 tail: the real-map block and the spawn-family scripts.
                // map: [0] | [1, w, h, numLand, nTerr, (bytes)*, mW, mH,
                // mNumLand, nMini, (bytes)*, nOwners, (tile, ownerID)*].
                // Guarded with `c.has()` so pre-G3c5 vectors still replay.
                let (sq, upg, gt, gmd, gwin, isp, smd, rps, nmag, bmn, nuid, nu,
                     anyn, idis, uicost, uimh, sam_r, sam_d, orail, gravel) = if c.has() {
                    let sq = if c.f() != 0.0 {
                        let w = c.f();
                        let h = c.f();
                        let num_land = c.f();
                        let terr: Vec<u8> = c.list().into_iter().map(|b| b as u8).collect();
                        let mw = c.f();
                        let mh = c.f();
                        let m_num_land = c.f();
                        let m_terr: Vec<u8> = c.list().into_iter().map(|b| b as u8).collect();
                        // Initial ownership applied before the WaterManager
                        // sees the map (owner writes don't touch water state).
                        let owners: Vec<(f64, f64)> =
                            (0..c.u()).map(|_| (c.f(), c.f())).collect();
                        let mut main = GameMap::new(w, h, terr, num_land);
                        for (t, o) in owners {
                            main.set_owner_id(t, o);
                        }
                        Some(SpatialQuery::new(
                            main,
                            GameMap::new(mw, mh, m_terr, m_num_land),
                            false,
                        ))
                    } else {
                        None
                    };
                    let upg: Vec<(String, bool)> =
                        (0..c.u()).map(|_| (c.s(), c.f() != 0.0)).collect();
                    let gt = c.s();
                    let gmd = c.s();
                    let gwin = c.val_list();
                    let isp = c.list();
                    let smd = c.list();
                    let rps = c.list();
                    let nmag: Vec<(f64, f64)> =
                        (0..c.u()).map(|_| (c.f(), c.f())).collect();
                    let bmn = c.list();
                    let nuid = c.list();
                    let nu: Vec<NuCall> = (0..c.u())
                        .map(|_| NuCall {
                            tile: c.f(),
                            radius: c.f(),
                            types: (0..c.u()).map(|_| c.s()).collect(),
                            entries: (0..c.u())
                                .map(|_| (c.f(), c.f(), c.f()))
                                .collect(),
                        })
                        .collect();
                    let anyn = c.list();
                    let idis = c.list();
                    let uicost = c.list();
                    let uimh = c.list();
                    let sam_r = c.list();
                    let sam_d = c.list();
                    let orail = c.val_list();
                    let gravel = c.val_list();
                    (sq, upg, gt, gmd, gwin, isp, smd, rps, nmag, bmn, nuid, nu,
                     anyn, idis, uicost, uimh, sam_r, sam_d, orail, gravel)
                } else {
                    let def: Vec<f64> = Vec::new();
                    let defv: Vec<JsVal> = Vec::new();
                    (None, Vec::new(), String::new(), String::new(), defv.clone(),
                     def.clone(), def.clone(), def.clone(), Vec::new(), def.clone(),
                     def.clone(), Vec::new(), def.clone(), def.clone(), def.clone(),
                     def.clone(), def.clone(), def.clone(), defv.clone(), defv)
                };
                self.sg = sg;
                self.ticks = ticks;
                self.team_tiles = team_tiles;
                self.total_land = total_land;
                self.traitor_duration = traitor_duration;
                self.players = players;
                self.alliance_requests = alliance_requests;
                self.past_outgoing = past_outgoing;
                self.players_list = players_list;
                self.off = off;
                self.da = da;
                self.arc = arc;
                self.tc = tc;
                self.tdur = tdur;
                self.emd = emd;
                self.emc = emc;
                self.qc = qc;
                self.dg = dg;
                self.dt = dt;
                self.dc = dc;
                self.duc = duc;
                self.eac = eac;
                self.car = car;
                self.gs = gs;
                self.owner = owner;
                self.sia = sia;
                self.nsia = nsia;
                self.sq = sq;
                self.upg = upg;
                self.gt = gt;
                self.gmd = gmd;
                self.gwin = gwin;
                self.isp = isp;
                self.smd = smd;
                self.rps = rps;
                self.nmag = nmag;
                self.bmn = bmn;
                self.nuid = nuid;
                self.nu = nu;
                self.anyn = anyn;
                self.idis = idis;
                self.uicost = uicost;
                self.uimh = uimh;
                self.sam_r = sam_r;
                self.sam_d = sam_d;
                self.orail = orail;
                self.gravel = gravel;
                self.self_tok = if self_tok >= 0.0 {
                    Some(self_tok as usize)
                } else {
                    None
                };

                // ctor order (PlayerImpl.ts:198-200): _troops FIRST (a NaN
                // startTroops throws before facade 100), then _gold, then
                // the PseudoRandom seed read.
                let troops = to_int_big(to_num(&start_troops));
                let gold = troops.and_then(|b| self.mg_starting_gold(&mut tr).map(|g| (b, g)));
                let seeded = gold.and_then(|(b, g)| {
                    // simpleHash(this.playerInfo.id): `str.length` throws
                    // only on null / undefined; the value is unobservable.
                    // `playerInfo` is a parameter property — assigned before
                    // the ctor body runs, so the id read uses the local `pi`.
                    match val_field(&pi, "id") {
                        Some(JsVal::Null) => {
                            Err("Cannot read properties of null (reading 'length')".to_string())
                        }
                        None | Some(JsVal::Absent) | Some(JsVal::Undef) => Err(
                            "Cannot read properties of undefined (reading 'length')".to_string(),
                        ),
                        _ => Ok((b, g)),
                    }
                });
                match seeded {
                    Ok((b, g)) => {
                        self.p = Player {
                            small_id,
                            player_info: pi,
                            team,
                            troops: b,
                            gold: Acc::Big(g),
                            tiles: TileSet::new(Some(&tiles)),
                            border_tiles: TileSet::new(Some(&border)),
                            alliances,
                            expired_alliances,
                            units,
                            out_attacks,
                            in_attacks,
                            ..Default::default()
                        };
                        self.constructed = true;
                        out.push(tr.len() as f64);
                        out.extend(tr.iter().copied());
                        out.push(0.0);
                    }
                    Err(msg) => {
                        out.push(tr.len() as f64);
                        out.extend(tr.iter().copied());
                        push_err(&mut out, &msg);
                    }
                }
                out
            }
            1 => {
                let mid = c.u() as u8;
                if !self.constructed {
                    // JS `pl === null`: the property access throws before
                    // anything else; the trace stays empty.
                    out.push(0.0);
                    out.push(1.0);
                    push_str(
                        &mut out,
                        &format!(
                            "Cannot read properties of null (reading '{}')",
                            mid_property(mid)
                        ),
                    );
                    return out;
                }
                match mid {
                    0 => push_ok_num(&mut out, self.p.small_id),
                    1 | 2 | 3 | 4 | 5 | 6 | 7 | 9 => {
                        let key = match mid {
                            1 => "name",
                            2 => "displayName",
                            3 => "clanTag",
                            4 => "nationFlag",
                            5 => "clientID",
                            6 => "id",
                            7 => "playerType",
                            _ => "isLobbyCreator",
                        };
                        let v = self.pi_field(key);
                        push_ok(&mut out, &v);
                    }
                    8 => push_ok(&mut out, &self.p.player_info.clone()),
                    10 => push_ok_bool(&mut out, self.is_alive()),
                    11 => push_ok_bool(&mut out, self.p.spawn_tile != JsVal::Undef),
                    12 => {
                        self.p.spawn_tile = c.val();
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    13 => push_ok(&mut out, &self.p.spawn_tile.clone()),
                    14 => push_ok_num(&mut out, self.p.troops as f64),
                    15 => match to_int_big(to_num(&c.val())) {
                        Ok(b) => {
                            self.p.troops = b;
                            push_ok(&mut out, &JsVal::Undef);
                        }
                        Err(m) => push_err(&mut out, &m),
                    },
                    16 => {
                        let t = to_num(&c.val());
                        if t < 0.0 {
                            // JS `-1 * troops` ≡ `-t` for every f64 (the
                            // -0 / NaN cases agree: -1*-0 = +0, -(-0) = +0).
                            match self.remove_troops(-t) {
                                Ok(_) => push_ok(&mut out, &JsVal::Undef),
                                Err(m) => push_err(&mut out, &m),
                            }
                        } else {
                            match to_int_big(t) {
                                Ok(b) => {
                                    self.p.troops += b;
                                    push_ok(&mut out, &JsVal::Undef);
                                }
                                Err(m) => push_err(&mut out, &m),
                            }
                        }
                    }
                    17 => match self.remove_troops(to_num(&c.val())) {
                        Ok(r) => push_ok_num(&mut out, r),
                        Err(m) => push_err(&mut out, &m),
                    },
                    18 => push_ok(&mut out, &self.p.gold.to_jsval()),
                    19 => push_ok(&mut out, &self.p.trade_gold.to_jsval()),
                    20 => match big_operand(&c.val()) {
                        Err(m) => push_err(&mut out, &m),
                        Ok(o) => match acc_plus(&self.p.trade_gold, &o) {
                            Err(m) => push_err(&mut out, &m),
                            Ok(v) => {
                                self.p.trade_gold = v;
                                push_ok(&mut out, &JsVal::Undef);
                            }
                        },
                    },
                    21 => push_ok(&mut out, &self.p.train_gold.to_jsval()),
                    22 => match big_operand(&c.val()) {
                        Err(m) => push_err(&mut out, &m),
                        Ok(o) => match acc_plus(&self.p.train_gold, &o) {
                            Err(m) => push_err(&mut out, &m),
                            Ok(v) => {
                                self.p.train_gold = v;
                                push_ok(&mut out, &JsVal::Undef);
                            }
                        },
                    },
                    23 => push_ok(&mut out, &self.p.piracy_gold.to_jsval()),
                    24 => match big_operand(&c.val()) {
                        Err(m) => push_err(&mut out, &m),
                        Ok(o) => match acc_plus(&self.p.piracy_gold, &o) {
                            Err(m) => push_err(&mut out, &m),
                            Ok(v) => {
                                self.p.piracy_gold = v;
                                push_ok(&mut out, &JsVal::Undef);
                            }
                        },
                    },
                    25 => push_ok(&mut out, &self.p.gold_earned.to_jsval()),
                    26 => {
                        let to_add = c.val();
                        let tile = if c.has() { c.val() } else { JsVal::Undef };
                        match big_operand(&to_add) {
                            Err(m) => push_err(&mut out, &m),
                            Ok(o) => match acc_plus(&self.p.gold, &o) {
                                Err(m) => push_err(&mut out, &m),
                                Ok(ng) => {
                                    self.p.gold = ng;
                                    match acc_plus(&self.p.gold_earned, &o) {
                                        Err(m) => push_err(&mut out, &m),
                                        Ok(ne) => {
                                            self.p.gold_earned = ne;
                                            if js_truthy(&tile) {
                                                let player_id = self.pi_field("id");
                                                let u = JsVal::Obj(vec![
                                                    ("type".to_string(), JsVal::Num(15.0)),
                                                    ("player".to_string(), player_id),
                                                    ("tile".to_string(), tile),
                                                    ("gold".to_string(), JsVal::Num(op_to_num(&o))),
                                                    ("troops".to_string(), JsVal::Num(0.0)),
                                                ]);
                                                self.mg_add_update(&mut tr, &u);
                                            }
                                            push_ok(&mut out, &JsVal::Undef);
                                        }
                                    }
                                }
                            },
                        }
                    }
                    27 => {
                        // `big()` operand conversion throws BEFORE the call
                        // (a non-integer number); the body is shared with
                        // `donateGold` — see `remove_gold_body`.
                        match big_operand(&c.val()) {
                            Err(m) => push_err(&mut out, &m),
                            Ok(o) => match self.remove_gold_body(&o) {
                                Ok(r) => push_ok_num(&mut out, r as f64),
                                Err(m) => push_err(&mut out, &m),
                            },
                        }
                    }
                    28 => push_ok_num(&mut out, self.p.tile_change_version),
                    29 => push_ok_num(&mut out, self.p.last_tile_change),
                    30 => push_ok(&mut out, &self.p.is_disconnected.clone()),
                    31 => {
                        let v = c.val();
                        let snap = if c.has() { c.val() } else { JsVal::Absent };
                        self.p.is_disconnected = v;
                        if js_truthy(&self.p.is_disconnected) {
                            if self.p.disconnect_snapshot == JsVal::Null {
                                if nullish(&snap) {
                                    // built object, left-to-right: 102 ->
                                    // team truthy gate -> 103 -> 104.
                                    let current_tick = self.mg_ticks(&mut tr);
                                    let team = self.p.team.clone();
                                    let team_tiles = if js_truthy(&team) {
                                        self.mg_team_tiles_owned(&mut tr, &team)
                                    } else {
                                        0.0
                                    };
                                    let total_land = self.mg_total_land(&mut tr);
                                    let was_alive = self.is_alive();
                                    self.p.disconnect_snapshot = JsVal::Obj(vec![
                                        ("currentTick".to_string(), JsVal::Num(current_tick)),
                                        ("teamTiles".to_string(), JsVal::Num(team_tiles)),
                                        ("totalLand".to_string(), JsVal::Num(total_land)),
                                        ("wasAlive".to_string(), JsVal::Bool(was_alive)),
                                    ]);
                                } else {
                                    self.p.disconnect_snapshot = snap;
                                }
                            }
                        } else {
                            self.p.disconnect_snapshot = JsVal::Null;
                        }
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    32 => push_ok(&mut out, &self.p.disconnect_snapshot.clone()),
                    33 => {
                        // `snapshot?.currentTick ?? null` — a non-nullish
                        // currentTick crosses RAW.
                        let tick = match &self.p.disconnect_snapshot {
                            JsVal::Obj(fields) => {
                                let f = fields
                                    .iter()
                                    .find(|(k, _)| k == "currentTick")
                                    .map(|(_, v)| v.clone())
                                    .unwrap_or(JsVal::Undef);
                                if nullish(&f) {
                                    None
                                } else {
                                    Some(f)
                                }
                            }
                            _ => None,
                        };
                        match tick {
                            Some(f) => push_ok(&mut out, &f),
                            None => push_ok(&mut out, &JsVal::Null),
                        }
                    }
                    34 => push_ok(&mut out, &self.p.team.clone()),
                    35 => {
                        let tok = to_num(&c.val()) as usize;
                        match self.same_team(&mut tr, tok) {
                            Ok(b) => push_ok_bool(&mut out, b),
                            Err(m) => push_err(&mut out, &m),
                        }
                    }
                    36 => {
                        let tok = to_num(&c.val()) as usize;
                        let treat = if c.has() { js_truthy(&c.val()) } else { false };
                        match self.is_friendly(&mut tr, tok, treat) {
                            Ok(b) => push_ok_bool(&mut out, b),
                            Err(m) => push_err(&mut out, &m),
                        }
                    }
                    37 => {
                        let tok = to_num(&c.val()) as usize;
                        push_ok_bool(&mut out, self.alliance_with(tok).is_some());
                    }
                    38 => {
                        let tok = to_num(&c.val()) as usize;
                        match self.alliance_with(tok) {
                            Some(a) => push_ok(
                                &mut out,
                                &JsVal::Arr(vec![
                                    JsVal::Num(a.req as f64),
                                    JsVal::Num(a.rec as f64),
                                ]),
                            ),
                            None => push_ok(&mut out, &JsVal::Null),
                        }
                    }
                    39 => push_ok(
                        &mut out,
                        &JsVal::Arr(
                            self.p
                                .alliances
                                .iter()
                                .map(|a| {
                                    JsVal::Arr(vec![
                                        JsVal::Num(a.req as f64),
                                        JsVal::Num(a.rec as f64),
                                    ])
                                })
                                .collect(),
                        ),
                    ),
                    40 => push_ok_num(&mut out, self.p.tiles.size()),
                    41 => {
                        self.p.tiles.iter_begin();
                        let mut cur = 0usize;
                        let mut vals = Vec::new();
                        while let Some(v) = self.p.tiles.iter_next(&mut cur) {
                            vals.push(v);
                        }
                        self.p.tiles.iter_end();
                        push_ok(&mut out, &JsVal::Arr(vals.into_iter().map(JsVal::Num).collect()));
                    }
                    42 => {
                        self.p.tiles.add(to_num(&c.val()));
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    43 => {
                        self.p.tiles.delete(to_num(&c.val()));
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    44 => {
                        self.p.border_tiles.iter_begin();
                        let mut cur = 0usize;
                        let mut vals = Vec::new();
                        while let Some(v) = self.p.border_tiles.iter_next(&mut cur) {
                            vals.push(v);
                        }
                        self.p.border_tiles.iter_end();
                        push_ok(&mut out, &JsVal::Arr(vals.into_iter().map(JsVal::Num).collect()));
                    }
                    45 => {
                        self.p.border_tiles.add(to_num(&c.val()));
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    46 => {
                        self.p.border_tiles.delete(to_num(&c.val()));
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    47 => push_ok_bool(&mut out, true),
                    48 => push_ok(
                        &mut out,
                        &JsVal::Arr(
                            self.p
                                .expired_alliances
                                .iter()
                                .map(|(req, rec)| {
                                    JsVal::Arr(vec![JsVal::Num(*req as f64), JsVal::Num(*rec as f64)])
                                })
                                .collect(),
                        ),
                    ),
                    49 => {
                        // `alliances().map((a) => a.other(this))` — the mock's
                        // `other(p)` is `p === mocks[req] ? mocks[rec] :
                        // mocks[req]`, i.e. the self slot at the REQ position
                        // flips to REC, everything else (self at REC, or a
                        // player under test not in the alliance) stays REQ.
                        let self_tok = self.self_tok;
                        push_ok(
                            &mut out,
                            &JsVal::Arr(
                                self.p
                                    .alliances
                                    .iter()
                                    .map(|a| {
                                        let tok = if self_tok == Some(a.req) {
                                            a.rec
                                        } else {
                                            a.req
                                        };
                                        JsVal::Num(tok as f64)
                                    })
                                    .collect(),
                            ),
                        )
                    }
                    50 => push_ok_bool(&mut out, self.traitor_remaining(&mut tr) > 0.0),
                    51 => {
                        let r = self.traitor_remaining(&mut tr);
                        push_ok_num(&mut out, r)
                    }
                    52 => {
                        self.p.marked_traitor_tick = self.mg_ticks(&mut tr);
                        self.p.betrayal_count += 1.0;
                        self.mg_betray(&mut tr);
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    53 => push_ok_bool(&mut out, self.in_doomsday()),
                    54 => {
                        if self.in_doomsday() {
                            let t = self.mg_ticks(&mut tr) - self.p.marked_doomsday_clock_tick;
                            push_ok_num(&mut out, t)
                        } else {
                            push_ok_num(&mut out, 0.0)
                        }
                    }
                    55 => {
                        if self.p.marked_doomsday_clock_tick < 0.0 {
                            self.p.marked_doomsday_clock_tick = self.mg_ticks(&mut tr);
                        }
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    56 => {
                        self.p.marked_doomsday_clock_tick = -1.0;
                        self.p.rotted_at_tick = -1.0;
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    57 => {
                        self.p.rotted_at_tick = self.mg_ticks(&mut tr);
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    58 => push_ok_bool(&mut out, self.is_decaying_body(&mut tr)),
                    59 => push_ok_num(&mut out, self.p.betrayal_count),
                    // ---- G3c3: the player-to-player social surface ----
                    60 | 61 => {
                        // incoming: ar.recipient() === this; outgoing:
                        // ar.requestor() === this. `this` is the mocks table
                        // slot selfTok, so the filter is a token compare.
                        let st = self.self_tok;
                        push_ok(
                            &mut out,
                            &JsVal::Arr(
                                self.alliance_requests
                                    .iter()
                                    .filter(|(req, rec)| {
                                        Some(if mid == 60 { *rec } else { *req }) == st
                                    })
                                    .map(|(req, rec)| {
                                        JsVal::Arr(vec![
                                            JsVal::Num(*req as f64),
                                            JsVal::Num(*rec as f64),
                                        ])
                                    })
                                    .collect(),
                            ),
                        )
                    }
                    62 => {
                        let tok = to_num(&c.val()) as usize;
                        match self.alliance_info(&mut tr, tok) {
                            Ok(v) => push_ok(&mut out, &v),
                            Err(m) => push_err(&mut out, &m),
                        }
                    }
                    63 => {
                        let tok = to_num(&c.val()) as usize;
                        match self.can_send_alliance_request(&mut tr, tok) {
                            Ok(b) => push_ok_bool(&mut out, b),
                            Err(m) => push_err(&mut out, &m),
                        }
                    }
                    64 => {
                        // mg.breakAlliance(this, alliance) — the alliance
                        // crosses as its index into the _alliances array.
                        let al = to_num(&c.val());
                        tr.push(123.0);
                        tr.push(al);
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    65 => {
                        // mg.removeAlliancesByPlayerSilently(this)
                        tr.push(124.0);
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    66 => {
                        let tok = to_num(&c.val()) as usize;
                        if self.alliance_with(tok).is_some() {
                            push_err(
                                &mut out,
                                "cannot create alliance request, already allies",
                            );
                        } else {
                            let v = take_jsval(&self.car, &mut self.car_i);
                            tr.push(122.0);
                            tr.push(tok as f64);
                            push_val(&mut tr, &v);
                            push_ok(&mut out, &v);
                        }
                    }
                    67 => {
                        let tok = to_num(&c.val()) as usize;
                        if self.self_tok == Some(tok) {
                            let m = format!(
                                "cannot get relation with self: {}",
                                self.player_to_string()
                            );
                            push_err(&mut out, &m);
                        } else {
                            let r = relation_from_value(self.relation_get(tok));
                            push_ok_num(&mut out, r);
                        }
                    }
                    68 => match self.all_relations_sorted(&mut tr) {
                        Ok(v) => push_ok(&mut out, &v),
                        Err(m) => push_err(&mut out, &m),
                    },
                    69 => {
                        let tok = to_num(&c.val()) as usize;
                        let delta = c.val();
                        if self.self_tok == Some(tok) {
                            let m = format!(
                                "cannot update relation with self: {}",
                                self.player_to_string()
                            );
                            push_err(&mut out, &m);
                        } else {
                            self.update_relation(tok, &delta);
                            push_ok(&mut out, &JsVal::Undef);
                        }
                    }
                    70 => {
                        self.decay_relations();
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    71 => {
                        let tok = to_num(&c.val()) as usize;
                        match self.can_target(&mut tr, tok) {
                            Ok(b) => push_ok_bool(&mut out, b),
                            Err(m) => push_err(&mut out, &m),
                        }
                    }
                    72 => {
                        // targets_.push({tick: mg.ticks(), target: other})
                        // THEN mg.target(this, other).
                        let tok = to_num(&c.val()) as usize;
                        let t = self.mg_ticks(&mut tr);
                        self.p.targets_.push((t, tok));
                        match self.other_small_id(tok) {
                            Ok(pid) => {
                                tr.push(125.0);
                                tr.push(pid);
                                push_ok(&mut out, &JsVal::Undef);
                            }
                            Err(m) => push_err(&mut out, &m),
                        }
                    }
                    73 => {
                        let ts = self.targets_body(&mut tr);
                        push_ok(
                            &mut out,
                            &JsVal::Arr(ts.into_iter().map(|t| JsVal::Num(t as f64)).collect()),
                        );
                    }
                    74 => {
                        let ts = self.transitive_targets(&mut tr);
                        push_ok(
                            &mut out,
                            &JsVal::Arr(ts.into_iter().map(|t| JsVal::Num(t as f64)).collect()),
                        );
                    }
                    75 => {
                        let recip = c.val();
                        let emoji = c.val();
                        let is_self =
                            matches!(&recip, JsVal::Num(t) if self.self_tok == Some(*t as usize));
                        if is_self {
                            let m = format!(
                                "Cannot send emoji to oneself: {}",
                                self.player_to_string()
                            );
                            push_err(&mut out, &m);
                        } else {
                            match self.emoji_recipient_id(&recip) {
                                Err(m) => push_err(&mut out, &m),
                                Ok(rid) => {
                                    let created = self.mg_ticks(&mut tr);
                                    let msg = JsVal::Obj(vec![
                                        ("message".to_string(), emoji),
                                        (
                                            "senderID".to_string(),
                                            JsVal::Num(self.p.small_id),
                                        ),
                                        ("recipientID".to_string(), rid),
                                        ("createdAt".to_string(), JsVal::Num(created)),
                                    ]);
                                    self.p.outgoing_emojis.push(msg.clone());
                                    // mg.sendEmojiUpdate(msg)
                                    tr.push(126.0);
                                    push_val(&mut tr, &msg);
                                    push_ok(&mut out, &JsVal::Undef);
                                }
                            }
                        }
                    }
                    76 => {
                        let v = self.outgoing_emojis_body(&mut tr);
                        push_ok(&mut out, &v);
                    }
                    77 => {
                        let recip = c.val();
                        match self.can_send_emoji(&mut tr, &recip) {
                            Ok(b) => push_ok_bool(&mut out, b),
                            Err(m) => push_err(&mut out, &m),
                        }
                    }
                    78 => {
                        // lastSentAt === undefined short-circuits BOTH
                        // facades (102 ticks + 116 quickChatCooldown).
                        let tok = to_num(&c.val()) as usize;
                        if self.self_tok == Some(tok) {
                            push_ok_bool(&mut out, false);
                        } else {
                            match self.other_small_id(tok) {
                                Err(m) => push_err(&mut out, &m),
                                Ok(sid) => {
                                    let last = self
                                        .p
                                        .quick_chats
                                        .iter()
                                        .find(|(k, _)| *k == sid)
                                        .map(|(_, v)| *v);
                                    match last {
                                        None => push_ok_bool(&mut out, true),
                                        Some(l) => {
                                            let t = self.mg_ticks(&mut tr) - l;
                                            let cd = script_f64(
                                                &self.qc,
                                                &mut self.qc_i,
                                                &mut tr,
                                                116.0,
                                            );
                                            push_ok_bool(&mut out, t >= cd);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    79 => {
                        // Map.set(k, v): the key (untraced smallID) evaluates
                        // before the 102 ticks.
                        let tok = to_num(&c.val()) as usize;
                        match self.other_small_id(tok) {
                            Err(m) => push_err(&mut out, &m),
                            Ok(sid) => {
                                let t = self.mg_ticks(&mut tr);
                                if let Some(e) =
                                    self.p.quick_chats.iter_mut().find(|(k, _)| *k == sid)
                                {
                                    e.1 = t;
                                } else {
                                    self.p.quick_chats.push((sid, t));
                                }
                                push_ok(&mut out, &JsVal::Undef);
                            }
                        }
                    }
                    80 | 81 => {
                        let tok = to_num(&c.val()) as usize;
                        match self.can_donate(&mut tr, tok, mid == 80) {
                            Ok(b) => push_ok_bool(&mut out, b),
                            Err(m) => push_err(&mut out, &m),
                        }
                    }
                    82 => {
                        let tok = to_num(&c.val()) as usize;
                        let troops = to_num(&c.val());
                        if self.self_tok == Some(tok) || troops <= 0.0 {
                            push_ok_bool(&mut out, false);
                        } else {
                            match self.remove_troops(troops) {
                                Err(m) => push_err(&mut out, &m),
                                Ok(0.0) => {
                                    push_ok_bool(&mut out, false)
                                }
                                Ok(removed) => match self.other_add_troops(&mut tr, tok, removed)
                                {
                                    Err(m) => push_err(&mut out, &m),
                                    Ok(()) => {
                                        let t = self.mg_ticks(&mut tr);
                                        self.p.sent_donations.push((tok, t));
                                        let u = self.donate_update("troops", tok);
                                        self.mg_add_update(&mut tr, &u);
                                        push_ok_bool(&mut out, true);
                                    }
                                },
                            }
                        }
                    }
                    83 => {
                        // The capture wraps the Gold arg in big() (the
                        // removeGold precedent); the `gold <= 0n` gate and
                        // removeGold's own `toRemove <= 0n` are observably
                        // identical, both folding into remove_gold_body.
                        let tok = to_num(&c.val()) as usize;
                        let gold_v = c.val();
                        if self.self_tok == Some(tok) {
                            push_ok_bool(&mut out, false);
                        } else {
                            match big_operand(&gold_v) {
                                Err(m) => push_err(&mut out, &m),
                                Ok(o) => match self.remove_gold_body(&o) {
                                    Err(m) => push_err(&mut out, &m),
                                    Ok(0) => push_ok_bool(&mut out, false),
                                    Ok(removed) => {
                                        match self.other_add_gold(&mut tr, tok, removed) {
                                            Err(m) => push_err(&mut out, &m),
                                            Ok(()) => {
                                                let t = self.mg_ticks(&mut tr);
                                                self.p.sent_donations.push((tok, t));
                                                let u =
                                                    self.donate_update("gold", tok);
                                                self.mg_add_update(&mut tr, &u);
                                                push_ok_bool(&mut out, true);
                                            }
                                        }
                                    }
                                },
                            }
                        }
                    }
                    84 => {
                        let t = self.mg_ticks(&mut tr) - self.p.last_delete_unit_tick;
                        let cd = script_f64(&self.duc, &mut self.duc_i, &mut tr, 120.0);
                        push_ok_bool(&mut out, t >= cd);
                    }
                    85 => {
                        self.p.last_delete_unit_tick = self.mg_ticks(&mut tr);
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    86 => match self.can_embargo_all(&mut tr) {
                        Ok(b) => push_ok_bool(&mut out, b),
                        Err(m) => push_err(&mut out, &m),
                    },
                    87 => {
                        self.p.last_embargo_all_tick = self.mg_ticks(&mut tr);
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    88 => {
                        let tok = to_num(&c.val()) as usize;
                        let oid = self.other_id(tok);
                        push_ok_bool(&mut out, self.has_embargo_id(&oid));
                    }
                    89 => {
                        let tok = to_num(&c.val()) as usize;
                        push_ok_bool(&mut out, self.can_trade(tok));
                    }
                    90 => push_ok(
                        &mut out,
                        &JsVal::Arr(
                            self.p
                                .embargoes
                                .iter()
                                .map(|(_, e)| {
                                    JsVal::Obj(vec![
                                        ("createdAt".to_string(), JsVal::Num(e.created_at)),
                                        ("isTemporary".to_string(), e.is_temporary.clone()),
                                        ("target".to_string(), JsVal::Num(e.target as f64)),
                                    ])
                                })
                                .collect(),
                        ),
                    ),
                    91 => {
                        let tok = to_num(&c.val()) as usize;
                        let temp = c.val();
                        match self.add_embargo(&mut tr, tok, temp) {
                            Ok(()) => push_ok(&mut out, &JsVal::Undef),
                            Err(m) => push_err(&mut out, &m),
                        }
                    }
                    92 => match self.stop_embargo(&mut tr, to_num(&c.val()) as usize) {
                        Ok(()) => push_ok(&mut out, &JsVal::Undef),
                        Err(m) => push_err(&mut out, &m),
                    },
                    93 => {
                        // endTemporaryEmbargo: a missing entry still runs
                        // stopEmbargo (the addUpdate is unconditional).
                        let tok = to_num(&c.val()) as usize;
                        let oid = self.other_id(tok);
                        let blocked = self
                            .p
                            .embargoes
                            .iter()
                            .find(|(k, _)| js_strict_eq(k, &oid))
                            .is_some_and(|(_, e)| !js_truthy(&e.is_temporary));
                        if blocked {
                            push_ok(&mut out, &JsVal::Undef);
                        } else {
                            match self.stop_embargo(&mut tr, tok) {
                                Ok(()) => push_ok(&mut out, &JsVal::Undef),
                                Err(m) => push_err(&mut out, &m),
                            }
                        }
                    }
                    94 => {
                        let players = self.mg_players(&mut tr);
                        let st = self.self_tok;
                        push_ok(
                            &mut out,
                            &JsVal::Arr(
                                players
                                    .into_iter()
                                    .filter(|tok| Some(*tok) != st && self.can_trade(*tok))
                                    .map(|t| JsVal::Num(t as f64))
                                    .collect(),
                            ),
                        );
                    }
                    // ---- G3c4: units, hash, profile and toUpdate ----
                    95 => push_ok(&mut out, &self.units_wire(self.units_filtered(None))),
                    96 => {
                        let t = c.s();
                        push_ok(
                            &mut out,
                            &self.units_wire(self.units_filtered(Some(&[t]))),
                        );
                    }
                    97 => {
                        let t1 = c.s();
                        let t2 = c.s();
                        push_ok(
                            &mut out,
                            &self.units_wire(self.units_filtered(Some(&[t1, t2]))),
                        );
                    }
                    98 => {
                        let t1 = c.s();
                        let t2 = c.s();
                        let t3 = c.s();
                        push_ok(
                            &mut out,
                            &self.units_wire(self.units_filtered(Some(&[t1, t2, t3]))),
                        );
                    }
                    99 => {
                        // units([...types]): an empty array returns _units
                        // verbatim; a non-array falls into the single-type
                        // path (only a Str first ever matches; the set holds
                        // raw values so non-Str members match nothing).
                        let arg = c.val();
                        let f = match &arg {
                            JsVal::Arr(items) if items.is_empty() => None,
                            JsVal::Arr(items) => Some(
                                items
                                    .iter()
                                    .filter_map(|v| match v {
                                        JsVal::Str(s) => Some(s.clone()),
                                        _ => None,
                                    })
                                    .collect::<Vec<String>>(),
                            ),
                            JsVal::Str(s) => Some(vec![s.clone()]),
                            JsVal::Undef | JsVal::Absent => None,
                            _ => Some(Vec::new()),
                        };
                        push_ok(
                            &mut out,
                            &self.units_wire(self.units_filtered(f.as_deref())),
                        );
                    }
                    100 => push_ok_num(&mut out, self.units_constructed(&c.s())),
                    101 => push_ok_num(&mut out, self.unit_count(&c.s())),
                    102 => push_ok_num(&mut out, self.units_owned(&c.s())),
                    103 => {
                        let t = c.s();
                        self.record_unit_constructed(t);
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    104 => {
                        // addUnit(type, level, uc) — buildUnit bookkeeping
                        // (the added mock's hash is the 0 default). The six
                        // fields the 4-tuple does not carry cross as the TS
                        // normaliser's defaults: id 0, ownerSid smallID,
                        // active true, cooldown false, tile -1, mfd false.
                        let typ = c.s();
                        let level = c.f();
                        let uc = c.f() != 0.0;
                        let sid = self.p.small_id;
                        self.p.units.push(UnitMock {
                            typ,
                            id: 0.0,
                            owner_sid: sid,
                            level,
                            uc,
                            active: true,
                            cooldown: false,
                            tile: -1.0,
                            mfd: false,
                            hash: 0.0,
                            queue: Vec::new(),
                        });
                        self.p.my_units_version += 1.0;
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    105 => {
                        self.p.my_units_version += 1.0;
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    106 => match self.player_profile(&mut tr) {
                        Ok(v) => push_ok(&mut out, &v),
                        Err(m) => push_err(&mut out, &m),
                    },
                    107 => push_ok(&mut out, &JsVal::Str(self.player_to_string())),
                    108 => match self.hash_body(&mut tr) {
                        Ok(h) => push_ok_num(&mut out, h),
                        Err(m) => push_err(&mut out, &m),
                    },
                    109 => {
                        self.mg_conquer(&mut tr, to_num(&c.val()));
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    110 => {
                        let tile = to_num(&c.val());
                        if self.mg_owner_is_self(&mut tr, tile) {
                            self.mg_relinquish(&mut tr, tile);
                            push_ok(&mut out, &JsVal::Undef);
                        } else {
                            push_err(&mut out, "Cannot relinquish tile not owned by this player");
                        }
                    }
                    111 => push_ok_bool(&mut out, self.is_immune_body(&mut tr)),
                    112 => {
                        let tok = to_num(&c.val()) as usize;
                        let treat = if c.has() { js_truthy(&c.val()) } else { false };
                        match self.can_attack_player(&mut tr, tok, treat) {
                            Ok(b) => push_ok_bool(&mut out, b),
                            Err(m) => push_err(&mut out, &m),
                        }
                    }
                    113 => push_ok(
                        &mut out,
                        &JsVal::Arr(
                            (0..self.p.out_attacks.len())
                                .map(|i| JsVal::Num(i as f64))
                                .collect(),
                        ),
                    ),
                    114 => {
                        // incomingAttacks(): the alive filter (127 per entry)
                        // over the stored attacks; tokens are the kept
                        // entries' indices.
                        let mut toks: Vec<JsVal> = Vec::new();
                        let mut err = None;
                        for (i, a) in self.p.in_attacks.clone().into_iter().enumerate() {
                            match self.other_is_alive(&mut tr, a.att) {
                                Ok(true) => toks.push(JsVal::Num(i as f64)),
                                Ok(false) => {}
                                Err(m) => {
                                    err = Some(m);
                                    break;
                                }
                            }
                        }
                        match err {
                            Some(m) => push_err(&mut out, &m),
                            None => push_ok(&mut out, &JsVal::Arr(toks)),
                        }
                    }
                    115 => {
                        let so_en = c.has() && c.f() != 0.0;
                        let ao_en = c.has() && c.f() != 0.0;
                        match self.to_update(&mut tr, so_en, ao_en) {
                            Ok((v, so, ao)) => {
                                out.push(0.0);
                                push_val(&mut out, &v);
                                out.push(so.len() as f64);
                                out.extend(so.iter().copied());
                                out.push(ao.len() as f64);
                                out.extend(ao.iter().copied());
                            }
                            Err(m) => push_err(&mut out, &m),
                        }
                    }
                    116 => {
                        // setAttackTroops(which, idx, troops) — the direct
                        // mock mutation the pack quads observe.
                        let which = c.f();
                        let idx = c.f() as usize;
                        let v = c.f();
                        if which == 0.0 {
                            if let Some(a) = self.p.out_attacks.get_mut(idx) {
                                a.troops = v;
                            }
                        } else if let Some(a) = self.p.in_attacks.get_mut(idx) {
                            a.troops = v;
                        }
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    117 => {
                        let which = c.f();
                        let idx = c.f() as usize;
                        let v = c.f() != 0.0;
                        if which == 0.0 {
                            if let Some(a) = self.p.out_attacks.get_mut(idx) {
                                a.ret = v;
                            }
                        } else if let Some(a) = self.p.in_attacks.get_mut(idx) {
                            a.ret = v;
                        }
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    // ---- G3c5: build / upgrade / spawn family ----
                    118 => {
                        // buildUnit(type, tile, params) — the returned unit
                        // crosses as `[0, 1, uid]`.
                        let typ = c.s();
                        let tile = c.f();
                        let params = c.val();
                        match self.build_unit(&mut tr, typ, tile, params) {
                            Ok(uid) => {
                                out.push(0.0);
                                out.push(1.0);
                                out.push(uid);
                            }
                            Err(m) => push_err(&mut out, &m),
                        }
                    }
                    119 => {
                        // findUnitToUpgrade(type, tile) — Unit | false crosses
                        // as `[0, 1, uid]` | `[0, 0]`.
                        let typ = c.s();
                        let tile = c.f();
                        match self.find_unit_to_upgrade(&mut tr, &typ, tile) {
                            Ok(Some(uid)) => {
                                out.push(0.0);
                                out.push(1.0);
                                out.push(uid);
                            }
                            Ok(None) => {
                                out.push(0.0);
                                out.push(0.0);
                            }
                            Err(m) => push_err(&mut out, &m),
                        }
                    }
                    120 => {
                        let idx = c.u();
                        match self.can_upgrade_unit(&mut tr, idx) {
                            Ok(b) => push_ok_bool(&mut out, b),
                            Err(m) => push_err(&mut out, &m),
                        }
                    }
                    121 => {
                        let idx = c.u();
                        match self.upgrade_unit(&mut tr, idx) {
                            Ok(()) => push_ok(&mut out, &JsVal::Undef),
                            Err(m) => push_err(&mut out, &m),
                        }
                    }
                    122 => {
                        // buildableUnits(tile, units) — `[plEnc(tile|null),
                        // n, (encS)*n]`.
                        let tile = match c.val() {
                            JsVal::Num(t) => Some(t),
                            _ => None,
                        };
                        let n = c.u();
                        let units: Vec<String> = (0..n).map(|_| c.s()).collect();
                        match self.buildable_units(&mut tr, tile, &units) {
                            Ok(v) => push_ok(&mut out, &v),
                            Err(m) => push_err(&mut out, &m),
                        }
                    }
                    123 => {
                        let typ = c.s();
                        let tile = c.f();
                        let valid = decode_valid_tiles(&c.val());
                        match self.can_build(&mut tr, &typ, tile, valid) {
                            Ok(Some(t)) => push_ok_num(&mut out, t),
                            Ok(None) => push_ok_bool(&mut out, false),
                            Err(m) => push_err(&mut out, &m),
                        }
                    }
                    124 => {
                        let tile = c.f();
                        let typ = c.s();
                        match self.nuke_spawn(&mut tr, tile, &typ) {
                            Ok(Some(t)) => push_ok_num(&mut out, t),
                            Ok(None) => push_ok_bool(&mut out, false),
                            Err(m) => push_err(&mut out, &m),
                        }
                    }
                    125 => {
                        let tile = c.f();
                        let valid = decode_valid_tiles(&c.val());
                        match self.port_spawn(&mut tr, tile, valid) {
                            Ok(Some(t)) => push_ok_num(&mut out, t),
                            Ok(None) => push_ok_bool(&mut out, false),
                            Err(m) => push_err(&mut out, &m),
                        }
                    }
                    126 => {
                        let tile = c.f();
                        match self.warship_spawn(&mut tr, tile) {
                            Ok(Some(t)) => push_ok_num(&mut out, t),
                            Ok(None) => push_ok_bool(&mut out, false),
                            Err(m) => push_err(&mut out, &m),
                        }
                    }
                    127 => {
                        let tile = c.f();
                        match self.land_based_unit_spawn(&mut tr, tile) {
                            Ok(Some(t)) => push_ok_num(&mut out, t),
                            Ok(None) => push_ok_bool(&mut out, false),
                            Err(m) => push_err(&mut out, &m),
                        }
                    }
                    128 => {
                        let tile = c.f();
                        let valid = decode_valid_tiles(&c.val());
                        match self.land_based_structure_spawn(&mut tr, tile, valid) {
                            Ok(Some(t)) => push_ok_num(&mut out, t),
                            Ok(None) => push_ok_bool(&mut out, false),
                            Err(m) => push_err(&mut out, &m),
                        }
                    }
                    129 => {
                        let tile = c.f();
                        match self.trade_ship_spawn(&mut tr, tile) {
                            Ok(Some(t)) => push_ok_num(&mut out, t),
                            Ok(None) => push_ok_bool(&mut out, false),
                            Err(m) => push_err(&mut out, &m),
                        }
                    }
                    k => unreachable!("player harness: unknown method id {k}"),
                }
                let mut full = Vec::with_capacity(1 + tr.len() + out.len());
                full.push(tr.len() as f64);
                full.extend(tr.iter().copied());
                full.extend(out.iter().copied());
                full
            }
            k => unreachable!("player harness: unknown op kind {k}"),
        }
    }
}
