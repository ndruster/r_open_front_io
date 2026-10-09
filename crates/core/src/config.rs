//! Bit-exact port of `src/core/configuration/Config.ts`: the game-rule config
//! facade. The `GameConfig` wire object rides the `js_json` codec (an
//! insertion-ordered field list, precedent `config_patch`); the `Game` /
//! `Player` / `Unit` / `Stats` facades are scripted mocks whose every call is
//! pinned into a flat trace (precedent `nation_utils` / `stats_impl`), so the
//! facade call order, the `&&` / `??` short-circuits and the closure bodies of
//! `unitInfo`'s cost functions are part of the compared stream. bigints cross
//! as f64 tokens in the `|v| <= 2^53` capture domain (precedent `stats_impl`).
//!
//! Ported surface (TS line anchors):
//!
//! * `parseGameEnv` (75), `terrainAttackBase` (169, module-private, exercised
//!   through `attackLogic`), `largeTerritoryBonus` (157), all `Config` methods
//!   (265-1281), the module constants (126-263).
//! * `JwksSchema` (188) is a zod declaration (wire validation, not ported);
//!   `GameEnv` (69) is only its three numeric members, pinned by
//!   `parseGameEnv`'s results.
//!
//! Faithfulness notes:
//!
//! * `??` fires only on nullish (absent / `undefined` / `null`); a `false` /
//!   `0` / `""` config value passes through untouched — e.g.
//!   `doomsdayClockConfig().enabled` with `enabled: false` in the wire config
//!   resolves to `false`, NOT the default (which is also `false`, but
//!   `enabled: 0` resolves to `0`).
//! * `||` and truthiness differ from `??`: `disableAlliances()` is
//!   `customAllianceDuration === 0 || (disableAlliances ?? false)` — the `===`
//!   is true for `-0`; the `||` returns the RAW right operand (a `5` in
//!   `disableAlliances` returns `5`, not `true`). `waterNukes` /
//!   `disableNavMesh` likewise return raw values.
//! * `typeof m === "number" && m > 0` (`allianceDuration`) is true for `NaN`
//!   but the `> 0` gate is false, so `NaN` falls through to the 3000 default;
//!   `-0` likewise.
//! * `hc?.goldMultiplier && player.isLobbyCreator()` (`goldMultiplierFor`):
//!   `0` and `NaN` are FALSY, so a `goldMultiplier: 0` host cheat falls back
//!   to the base multiplier; `isLobbyCreator()` is only called when the left
//!   side is truthy (the trace pins the absence).
//! * `hasInfinite{Gold,Troops}For`: `infiniteGold()` truthy short-circuits
//!   BEFORE any `isLobbyCreator()` call; the `?? false` gate on
//!   `hostCheats.infiniteGold` likewise skips `isLobbyCreator()` when false.
//! * `attackLogic`: `terrainAttackBase`'s `default` interpolates the RAW
//!   numeric `terrain` (`terrain type ${terrain} not supported` — JS
//!   Number::toString through [`crate::js_fixed::js_to_string`]);
//!   `nukeMagnitudes` / `nukeSpeed` interpolate the STRING enum value
//!   directly (`Unknown nuke type: Atom Bomb`). `assertNever` throws
//!   `"Unexpected value: " + x` with ToPrimitive: a string rides raw, a
//!   number goes through Number::toString, `undefined` spells "undefined".
//! * `falloutRatio !== null` (879): `undefined` PASSES the gate (`5 -
//!   undefined * 2` is `NaN`); the capture domain only feeds numbers / null.
//! * `defender.troops / defender.numTiles` and `defender.troops /
//!   attackTroops` are plain f64 divisions (NaN / ±Inf semantics kept).
//! * `conquerGoldAmount`: `gold() / 2n` is BigInt division — truncation
//!   toward ZERO (`i64 / 2` in Rust matches for the |v| <= 2^53 domain).
//! * `BigInt(x)` throw matrix (V8 messages pinned): a NUMBER that is NaN /
//!   ±Infinity / non-integer throws "The number <Number::toString> cannot be
//!   converted to a BigInt because it is not an integer"; a non-numeric
//!   (string / null / undefined) throws "Cannot convert <String(x)> to a
//!   BigInt" (the ORIGINAL string, untrimmed). `toInt` (Util.ts:397)
//!   clamps ±Infinity to ±MAX_SAFE_INTEGER BEFORE the floor, so only NaN
//!   throws. `BigInt(Math.floor(x))` floors first: floor(Inf) is still Inf.
//! * `trainGold`'s `switch (rel)` has NO default: an unknown `rel` leaves
//!   `baseGold` `undefined`, `Math.max(5000, undefined - penalty)` is `NaN`,
//!   and `toInt(NaN)` throws — the throw point is pinned.
//! * `maxTroops`: `player.units(UnitType.City)` (string "City") -> `filter`
//!   calls `isUnderConstruction()` on EVERY unit, then `map` calls `level()`
//!   only on survivors, then `reduce` folds from `0` (JS array method order).
//! * `costWrapper`'s reduce calls `unitsOwned(type)` then
//!   `unitsConstructed(type)` per type, in `types` order (Port -> [Port,
//!   Factory], Factory -> [Factory, Port]).
//! * `pow2(numUnits)` (DetMath.ts:31): the JS `<<` coerces through ToInt32;
//!   the capture domain keeps `numUnits` a non-negative integer, and
//!   `Math.min(1_000_000, pow2(n) * 125_000)` caps the `n >= 1024` Infinity
//!   away, so `detmath::pow2(to_int32(n))` matches.
//! * `unitInfo` caches per `UnitType` in JS `Map` insertion order (precedent
//!   `event_bus`'s ListenerMap): the first call builds and inserts, later
//!   calls hit WITHOUT re-inserting (order survives; the `hit` flag and the
//!   key-order dump pin it). The cached `UnitInfo` object's key order is the
//!   switch-case declaration order; the `cost` function value crosses as the
//!   string `"function"` (the capture encodes `typeof v === "function"` that
//!   way — no real field value in the domain spells that).
//! * `dynamicSamRange`: `state === undefined || state.upgradeStartTick ===
//!   undefined` — an ABSENT field reads `undefined` and takes the `level()`
//!   branch; `state.duration ?? samUpgradeDuration()` keeps the `??` nullish
//!   gate (`duration: 0` would NOT fall back — division by zero then yields
//!   NaN/Infinity, the capture pins one such case).
//! * `DOOMSDAY_CLOCK_DEFAULTS` / `OVERTIME_DEFAULTS` key order is the TS
//!   declaration order, pinned by the codec dump.
//! * `Math.LN2` is the literal `0.6931471805599453`; `LN2 / 100` etc. are
//!   runtime f64 divisions matching JS bit for bit. All `exp` / `log` / `pow`
//!   / `pow2` ride `detmath` (never `std`); all `Math.max` / `Math.min` ride
//!   `js_max` / `js_min` (NaN / ±0 semantics); `within` / `sigmoid` reuse
//!   `crate::util`.

use crate::detmath::{exp, log, pow};
use crate::game_config_helpers::js_number;
use crate::js_fixed::js_to_string;
use crate::js_json::{push_map, push_str, push_val, read_map, read_str, read_val, val_field, JsVal};
use crate::jsnum::{js_max, js_min, to_int32};
use crate::util::{sigmoid, within};

/// `Math.LN2` (the literal from the TS call sites).
#[allow(clippy::approx_constant)]
const LN2: f64 = 0.6931471805599453;

/// The `railroadMaxSize` factor — the TS source spells the TRUNCATED literal
/// `1.4142` (Config.ts:510), not `SQRT_2`, so the approximation stays as-is.
#[allow(clippy::approx_constant)]
const RAILROAD_MAX_FACTOR: f64 = 1.4142;

// attackLogic tunables (Config.ts:126-151).
const LARGE_TERRITORY_MIDPOINT: f64 = 300_000.0;
const LARGE_TERRITORY_STEEPNESS: f64 = 2.5;
const LARGE_ATTACKER_DEPTH: f64 = 0.7;
const LARGE_DEFENDER_DEPTH: f64 = 0.3;
const BOT_DEFENDER_LOSS_MULT: f64 = 0.7;
const TERRA_NULLIUS_COST_SCALE: f64 = 2000.0;
const TERRA_NULLIUS_MIN_COST: f64 = 5.0;
const TERRA_NULLIUS_MAX_COST: f64 = 100.0;
const ATTACKER_LOSS_BASE: f64 = 0.463;
const ATTACKER_LOSS_PER_DENSITY: f64 = 0.0039;
const SPEED_COST_DIVISOR: f64 = 8.55;
const LARGE_ATTACKER_SPEED_DEPTH: f64 = 0.73;

const DEFAULT_SPAWN_IMMUNITY_TICKS: f64 = 5.0 * 10.0;
const SAM_CONSTRUCTION_TICKS: f64 = 30.0 * 10.0;
const PERCENT_TILES_OWNED_TO_WIN: f64 = 80.0;

/// `Number.MAX_SAFE_INTEGER` (the `toInt` ±Infinity clamp).
const MAX_SAFE: i64 = 9_007_199_254_740_991;

// ---- codec helpers -----------------------------------------------------------

fn truthy(v: &JsVal) -> bool {
    match v {
        JsVal::Absent | JsVal::Undef | JsVal::Null => false,
        JsVal::Num(n) => !n.is_nan() && *n != 0.0,
        JsVal::Bool(b) => *b,
        JsVal::Str(s) => !s.is_empty(),
        JsVal::Obj(_) | JsVal::Arr(_) => true,
    }
}

fn to_num(v: &JsVal) -> f64 {
    match v {
        JsVal::Num(n) => *n,
        JsVal::Bool(b) => if *b { 1.0 } else { 0.0 },
        JsVal::Null => 0.0,
        JsVal::Str(s) => js_number(s),
        JsVal::Absent | JsVal::Undef => f64::NAN,
        JsVal::Obj(_) | JsVal::Arr(_) => f64::NAN,
    }
}

/// `String(x)` for the throw-message interpolations (ToPrimitive over the
/// codec domain; `Obj` / `Arr` are outside the captured domain and spell the
/// Object.prototype.toString default).
fn to_str(v: &JsVal) -> String {
    match v {
        JsVal::Absent | JsVal::Undef => "undefined".to_string(),
        JsVal::Null => "null".to_string(),
        JsVal::Num(n) => js_to_string(*n),
        JsVal::Bool(b) => (if *b { "true" } else { "false" }).to_string(),
        JsVal::Str(s) => s.clone(),
        JsVal::Obj(_) => "[object Object]".to_string(),
        JsVal::Arr(items) => items
            .iter()
            .map(|x| match x {
                JsVal::Obj(_) => "[object Object]".to_string(),
                other => to_str(other),
            })
            .collect::<Vec<_>>()
            .join(","),
    }
}

/// `obj[key]` — an absent field reads `undefined`.
fn field<'a>(m: &'a [(String, JsVal)], key: &str) -> Option<&'a JsVal> {
    m.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn nullish(v: Option<&JsVal>) -> bool {
    matches!(v, None | Some(JsVal::Undef) | Some(JsVal::Null))
}

/// `v ?? d` over the field-read domain.
fn coalesce<'a>(v: Option<&'a JsVal>, d: &'a JsVal) -> &'a JsVal {
    if nullish(v) {
        d
    } else {
        v.unwrap()
    }
}

/// Success payload `[0, ...encVal(v)]` (the uniform kind-2 result codec).
fn push_ok(out: &mut Vec<f64>, v: &JsVal) {
    out.push(0.0);
    push_val(out, v);
}

/// Success payload for a number.
fn push_ok_num(out: &mut Vec<f64>, v: f64) {
    push_ok(out, &JsVal::Num(v));
}

/// Throw payload `[1, ...encS(msg)]`.
fn push_err(out: &mut Vec<f64>, msg: &str) {
    out.push(1.0);
    push_str(out, msg);
}

/// `BigInt(x)` for a number `x` — `Err(V8 message)` on the throw matrix.
fn bigint_from_num(x: f64) -> Result<i64, String> {
    if x.is_nan() || x.is_infinite() || x != x.floor() {
        return Err(format!(
            "The number {} cannot be converted to a BigInt because it is not an integer",
            js_to_string(x)
        ));
    }
    Ok(x as i64)
}

/// `BigInt(v)` for a raw codec value (the `startingGold` wire paths): a
/// numeric string parses (JS `BigInt("1000")` is legal), anything else
/// outside the integer-number domain spells its `String(v)` into the V8
/// message.
fn bigint_from_val(v: &JsVal) -> Result<i64, String> {
    match v {
        JsVal::Num(n) => bigint_from_num(*n),
        JsVal::Bool(b) => Ok(if *b { 1 } else { 0 }),
        JsVal::Str(s) => {
            let t = s.trim();
            if t.is_empty() {
                return Ok(0); // BigInt("") === 0n
            }
            let (sign, digits) = match t.strip_prefix('-') {
                Some(d) => (-1i64, d),
                None => (1i64, t.strip_prefix('+').unwrap_or(t)),
            };
            let is_int = !digits.is_empty()
                && digits.chars().all(|c| c.is_ascii_digit());
            match (is_int, digits.parse::<i64>()) {
                (true, Ok(d)) => Ok(sign * d),
                _ => Err(format!("Cannot convert {} to a BigInt", s)),
            }
        }
        other => Err(format!("Cannot convert {} to a BigInt", to_str(other))),
    }
}

/// DetMath `pow2(n)` with the exact JS coercion path: the guards are
/// relational (NaN fails both), and `(n + 1023) << 20` goes through
/// ToInt32 — `pow2(NaN)` writes the all-zero bit pattern and returns
/// `+0`, NOT `2^0`.
fn pow2_js(n: f64) -> f64 {
    if n > 1023.0 {
        return f64::INFINITY;
    }
    if n < -1022.0 {
        return 0.0;
    }
    let x = to_int32(n + 1023.0);
    let hi = (x.wrapping_shl(20) as u32) as u64;
    f64::from_bits(hi << 32)
}

/// `BigInt(Math.floor(x))` — floor first (floor(±Inf) stays ±Inf).
fn bigint_floor(x: f64) -> Result<i64, String> {
    bigint_from_num(x.floor())
}

/// `toInt(num)` (Util.ts:397) — ±Infinity clamp BEFORE the floor; only NaN
/// throws (the non-integer number message from [`bigint_from_num`]).
fn to_int_big(x: f64) -> Result<i64, String> {
    if x == f64::INFINITY {
        return Ok(MAX_SAFE);
    }
    if x == f64::NEG_INFINITY {
        return Ok(-MAX_SAFE);
    }
    bigint_floor(x)
}

// ---- facade trace event codes ------------------------------------------------
//
// 30 player.type [30,pid,(str)]            31 isLobbyCreator [31,pid,ret]
// 32 troops [32,pid,ret]                  33 numTilesOwned [33,pid,ret]
// 34 units [34,pid,(str),n,(uid)*]        35 isUnderConstruction [35,uid,ret]
// 36 unit.level [36,uid,ret]              37 unitsOwned [37,pid,(str),ret]
// 38 unitsConstructed [38,pid,(str),ret]  39 gold [39,pid,ret]
// 40 game.stats [40]                      41 numMirvsLaunched [41,ret]
// 42 samLauncherState [42,uid,...codec]    43 sam.level [43,uid,ret]

/// The scripted `Player` / `Unit` / `Game` facades for one op: per-method
/// return scripts consumed in call order, plus the cursor state.
struct Facades {
    pid: f64,
    ptype: String,
    ic: Vec<f64>,
    ic_i: usize,
    troops: Vec<f64>,
    tr_i: usize,
    nto: Vec<f64>,
    nto_i: usize,
    units: Vec<Vec<f64>>,
    units_i: usize,
    iuc: Vec<f64>,
    iuc_i: usize,
    ulevel: Vec<f64>,
    ulevel_i: usize,
    owned: Vec<f64>,
    owned_i: usize,
    constructed: Vec<f64>,
    con_i: usize,
    gold: Vec<f64>,
    gold_i: usize,
    mirv: Vec<f64>,
    mirv_i: usize,
    state: Vec<JsVal>,
    state_i: usize,
    sam_level: Vec<f64>,
    sam_level_i: usize,
}

impl Facades {
    fn f_type(&mut self, tr: &mut Vec<f64>) -> String {
        tr.push(30.0);
        tr.push(self.pid);
        push_str(tr, &self.ptype);
        self.ptype.clone()
    }
    fn is_lobby_creator(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = self.ic[self.ic_i];
        self.ic_i += 1;
        tr.push(31.0);
        tr.push(self.pid);
        tr.push(v);
        v
    }
    fn troops(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = self.troops[self.tr_i];
        self.tr_i += 1;
        tr.push(32.0);
        tr.push(self.pid);
        tr.push(v);
        v
    }
    fn num_tiles_owned(&mut self, tr: &mut Vec<f64>) -> f64 {
        let v = self.nto[self.nto_i];
        self.nto_i += 1;
        tr.push(33.0);
        tr.push(self.pid);
        tr.push(v);
        v
    }
    fn units(&mut self, tr: &mut Vec<f64>, kind: &str) -> Vec<f64> {
        let list = self.units[self.units_i].clone();
        self.units_i += 1;
        tr.push(34.0);
        tr.push(self.pid);
        push_str(tr, kind);
        tr.push(list.len() as f64);
        tr.extend(list.iter().copied());
        list
    }
    fn is_under_construction(&mut self, tr: &mut Vec<f64>, uid: f64) -> f64 {
        let v = self.iuc[self.iuc_i];
        self.iuc_i += 1;
        tr.push(35.0);
        tr.push(uid);
        tr.push(v);
        v
    }
    fn unit_level(&mut self, tr: &mut Vec<f64>, uid: f64) -> f64 {
        let v = self.ulevel[self.ulevel_i];
        self.ulevel_i += 1;
        tr.push(36.0);
        tr.push(uid);
        tr.push(v);
        v
    }
    fn units_owned(&mut self, tr: &mut Vec<f64>, t: &str) -> f64 {
        let v = self.owned[self.owned_i];
        self.owned_i += 1;
        tr.push(37.0);
        tr.push(self.pid);
        push_str(tr, t);
        tr.push(v);
        v
    }
    fn units_constructed(&mut self, tr: &mut Vec<f64>, t: &str) -> f64 {
        let v = self.constructed[self.con_i];
        self.con_i += 1;
        tr.push(38.0);
        tr.push(self.pid);
        push_str(tr, t);
        tr.push(v);
        v
    }
    fn gold(&mut self, tr: &mut Vec<f64>) -> i64 {
        let v = self.gold[self.gold_i];
        self.gold_i += 1;
        tr.push(39.0);
        tr.push(self.pid);
        tr.push(v);
        v as i64
    }
    fn num_mirvs_launched(&mut self, tr: &mut Vec<f64>) -> i64 {
        tr.push(40.0);
        let v = self.mirv[self.mirv_i];
        self.mirv_i += 1;
        tr.push(41.0);
        tr.push(v);
        v as i64
    }
    fn sam_launcher_state(&mut self, tr: &mut Vec<f64>, uid: f64) -> JsVal {
        let v = self.state[self.state_i].clone();
        self.state_i += 1;
        tr.push(42.0);
        tr.push(uid);
        push_val(tr, &v);
        v
    }
    fn sam_level(&mut self, tr: &mut Vec<f64>, uid: f64) -> f64 {
        let v = self.sam_level[self.sam_level_i];
        self.sam_level_i += 1;
        tr.push(43.0);
        tr.push(uid);
        tr.push(v);
        v
    }
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
    fn s(&mut self) -> String {
        read_str(self.0, &mut self.1)
    }
    fn list(&mut self) -> Vec<f64> {
        let n = self.u();
        (0..n).map(|_| self.f()).collect()
    }
    fn val(&mut self) -> JsVal {
        read_val(self.0, &mut self.1)
    }
    fn player(&mut self) -> Facades {
        let pid = self.f();
        let ptype = self.s();
        let ic = self.list();
        let troops = self.list();
        let nto = self.list();
        let nu = self.u();
        let units = (0..nu).map(|_| self.list()).collect();
        let iuc = self.list();
        let ulevel = self.list();
        let owned = self.list();
        let constructed = self.list();
        let gold = self.list();
        let mirv = self.list();
        let ns = self.u();
        let state = (0..ns).map(|_| self.val()).collect();
        let sam_level = self.list();
        Facades {
            pid,
            ptype,
            ic,
            ic_i: 0,
            troops,
            tr_i: 0,
            nto,
            nto_i: 0,
            units,
            units_i: 0,
            iuc,
            iuc_i: 0,
            ulevel,
            ulevel_i: 0,
            owned,
            owned_i: 0,
            constructed,
            con_i: 0,
            gold,
            gold_i: 0,
            mirv,
            mirv_i: 0,
            state,
            state_i: 0,
            sam_level,
            sam_level_i: 0,
        }
    }
}

// ---- UnitInfo cache shape ------------------------------------------------------

/// The cached `UnitInfo` shape: which optional fields the switch-case built
/// (key order = declaration order) and the `constructionDuration` value
/// resolved at build time from `instantBuild()`.
#[derive(Clone, Debug)]
struct InfoShape {
    max_health: Option<f64>,
    damage: Option<f64>,
    construction_duration: Option<f64>,
    upgradable: Option<bool>,
}

impl InfoShape {
    /// The codec dump: `cost` first (a function value crosses as the string
    /// `"function"`), then the present optional fields in declaration order.
    fn dump(&self) -> Vec<(String, JsVal)> {
        let mut m = vec![("cost".to_string(), JsVal::Str("function".to_string()))];
        if let Some(h) = self.max_health {
            m.push(("maxHealth".to_string(), JsVal::Num(h)));
        }
        if let Some(d) = self.damage {
            m.push(("damage".to_string(), JsVal::Num(d)));
        }
        if let Some(c) = self.construction_duration {
            m.push(("constructionDuration".to_string(), JsVal::Num(c)));
        }
        if let Some(u) = self.upgradable {
            m.push(("upgradable".to_string(), JsVal::Bool(u)));
        }
        m
    }
}

/// The `unitInfo` switch-case shapes (UnitType string enum values). The
/// `constructionDuration` ternaries test the RAW `instantBuild` value for
/// truthiness (a non-empty string is truthy, its Number coercion is not).
fn build_shape(type_name: &str, instant_build: &JsVal) -> Option<InfoShape> {
    let ib = truthy(instant_build);
    let cd = if ib { 0.0 } else { 2.0 * 10.0 };
    let cd5 = if ib { 0.0 } else { 5.0 * 10.0 };
    let cd10 = if ib { 0.0 } else { 10.0 * 10.0 };
    let cd_sam = if ib { 0.0 } else { SAM_CONSTRUCTION_TICKS };
    let s = match type_name {
        "Transport" | "SAMMissile" | "MIRV Warhead" | "Trade Ship" | "Train" => InfoShape {
            max_health: None,
            damage: None,
            construction_duration: None,
            upgradable: None,
        },
        "Warship" => InfoShape {
            max_health: Some(1000.0),
            ..plain()
        },
        "Shell" => InfoShape {
            damage: Some(250.0),
            ..plain()
        },
        "Port" => InfoShape {
            construction_duration: Some(cd5),
            upgradable: Some(true),
            ..plain()
        },
        "Missile Silo" => InfoShape {
            construction_duration: Some(cd10),
            upgradable: Some(true),
            ..plain()
        },
        "Defense Post" => InfoShape {
            construction_duration: Some(cd5),
            ..plain()
        },
        "SAM Launcher" => InfoShape {
            construction_duration: Some(cd_sam),
            upgradable: Some(true),
            ..plain()
        },
        "City" => InfoShape {
            construction_duration: Some(cd),
            upgradable: Some(true),
            ..plain()
        },
        "Factory" => InfoShape {
            construction_duration: Some(cd),
            upgradable: Some(true),
            ..plain()
        },
        "Atom Bomb" | "Hydrogen Bomb" | "MIRV" => plain(),
        _ => return None,
    };
    Some(s)
}

fn plain() -> InfoShape {
    InfoShape {
        max_health: None,
        damage: None,
        construction_duration: None,
        upgradable: None,
    }
}

/// The `costWrapper` type list per UnitType (the reduce order).
fn wrapper_types(type_name: &str) -> Option<Vec<&'static str>> {
    Some(match type_name {
        "Warship" => vec!["Warship"],
        "Port" => vec!["Port", "Factory"],
        "Atom Bomb" => vec!["Atom Bomb"],
        "Hydrogen Bomb" => vec!["Hydrogen Bomb"],
        "Missile Silo" => vec!["Missile Silo"],
        "Defense Post" => vec!["Defense Post"],
        "SAM Launcher" => vec!["SAM Launcher"],
        "City" => vec!["City"],
        "Factory" => vec!["Factory", "Port"],
        _ => return None,
    })
}

/// `costFn(numUnits + extraUnits)` per wrapper unit (before `BigInt`).
fn wrapper_cost_fn(type_name: &str, n: f64) -> f64 {
    match type_name {
        "Warship" => js_min(1_000_000.0, (n + 1.0) * 250_000.0),
        "Port" | "City" | "Factory" => js_min(1_000_000.0, pow2_js(n) * 125_000.0),
        "Atom Bomb" => 750_000.0,
        "Hydrogen Bomb" => 5_000_000.0,
        "Missile Silo" => 1_000_000.0,
        "Defense Post" => js_min(250_000.0, (n + 1.0) * 50_000.0),
        "SAM Launcher" => js_min(3_000_000.0, (n + 1.0) * 1_500_000.0),
        _ => f64::NAN,
    }
}

// ---- the harness ---------------------------------------------------------------

pub struct Config {
    game_config: Vec<(String, JsVal)>,
    user_settings: JsVal,
    is_replay: bool,
    listed: bool,
    spectator: bool,
    unit_cache: Vec<(String, InfoShape)>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            game_config: Vec::new(),
            user_settings: JsVal::Null,
            is_replay: false,
            listed: false,
            spectator: false,
            unit_cache: Vec::new(),
        }
    }
}

impl Config {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    // --- config reads ---

    fn cfg_bool(&self, key: &str) -> bool {
        truthy(field(&self.game_config, key).unwrap_or(&JsVal::Undef))
    }

    fn infinite_gold(&self) -> bool {
        self.cfg_bool("infiniteGold")
    }
    fn infinite_troops(&self) -> bool {
        self.cfg_bool("infiniteTroops")
    }
    fn gold_multiplier(&self) -> f64 {
        let v = field(&self.game_config, "goldMultiplier");
        to_num(coalesce(v, &JsVal::Num(1.0)))
    }
    fn spawn_immunity_duration(&self) -> f64 {
        let v = field(&self.game_config, "spawnImmunityDuration");
        to_num(coalesce(v, &JsVal::Num(DEFAULT_SPAWN_IMMUNITY_TICKS)))
    }
    fn is_random_spawn(&self) -> bool {
        self.cfg_bool("randomSpawn")
    }
    fn is_unit_disabled(&self, t: &str) -> bool {
        // `disabledUnits?.includes(unitType) ?? false` — Array#includes is
        // SameValueZero (only string items can match a UnitType); a string
        // value falls through to String#includes (substring).
        match field(&self.game_config, "disabledUnits") {
            Some(JsVal::Arr(items)) => items
                .iter()
                .any(|x| matches!(x, JsVal::Str(s) if s == t)),
            Some(JsVal::Str(s)) => s.contains(t),
            _ => false,
        }
    }
    fn difficulty(&self) -> JsVal {
        field(&self.game_config, "difficulty").cloned().unwrap_or(JsVal::Undef)
    }

    fn hc_field<'a>(&'a self, key: &str) -> Option<&'a JsVal> {
        // `hc?.x`: absent / undefined / null hostCheats read undefined.
        val_field(field(&self.game_config, "hostCheats").unwrap_or(&JsVal::Undef), key)
    }

    fn has_infinite_gold_for(&self, f: &mut Facades, tr: &mut Vec<f64>) -> bool {
        if self.infinite_gold() {
            return true;
        }
        let gate = truthy(coalesce(self.hc_field("infiniteGold"), &JsVal::Bool(false)));
        gate && truthy(&JsVal::Num(f.is_lobby_creator(tr)))
    }

    fn has_infinite_troops_for(&self, f: &mut Facades, tr: &mut Vec<f64>) -> bool {
        if self.infinite_troops() {
            return true;
        }
        let gate = truthy(coalesce(self.hc_field("infiniteTroops"), &JsVal::Bool(false)));
        gate && truthy(&JsVal::Num(f.is_lobby_creator(tr)))
    }

    fn gold_multiplier_for(&self, f: &mut Facades, tr: &mut Vec<f64>) -> f64 {
        let base = self.gold_multiplier();
        let hc_gm = self.hc_field("goldMultiplier");
        // `hc?.goldMultiplier && player.isLobbyCreator()` — 0 / NaN falsy.
        if hc_gm.map(truthy).unwrap_or(false) && truthy(&JsVal::Num(f.is_lobby_creator(tr))) {
            to_num(hc_gm.unwrap())
        } else {
            base
        }
    }

    fn sam_range(&self, level: f64) -> f64 {
        150.0 - 480.0 / (level + 5.0)
    }
    fn sam_upgrade_duration(&self) -> f64 {
        (90.0f64 / 2.0).floor()
    }

    fn overtime_enabled(&self) -> JsVal {
        let c = val_field(field(&self.game_config, "overtime").unwrap_or(&JsVal::Undef), "enabled");
        coalesce(c, &JsVal::Bool(false)).clone()
    }
    fn overtime_start_minutes(&self) -> JsVal {
        let c = val_field(
            field(&self.game_config, "overtime").unwrap_or(&JsVal::Undef),
            "startMinutes",
        );
        coalesce(c, &JsVal::Num(30.0)).clone()
    }

    // --- the op dispatcher ---

    /// Run one op. Kind table (see `tools/gen_vectors.mjs`):
    /// 0 construct `[n,(key,value)*n, userSettings, isReplay, listed,
    /// spectator]` -> `[0]` (fresh cache);
    /// 1 parseGameEnv `[codec value]` -> `[0,num] | [1,(msg-str)]`;
    /// 2 method call `[mid, ...]` -> `[traceLen,(trace)*,[0,...]|[1,(msg)]]`;
    /// 3 attackLogic `[flat input]` -> trace-less payload;
    /// 4 unitInfo `[type-str]` -> `[traceLen,0,[0,hit,n,(key,value)*n] |
    /// [1,(msg)]]` (mutates the cache);
    /// 5 callUnitCost `[type-str, extraUnits codec, player block]` ->
    /// `[traceLen,(trace)*,[0,bigint]|[1,(msg)]]`;
    /// 6 dumpUnitInfoCache -> `[0,n,(type-str)*n]` (insertion order).
    /// Facade trace events are documented at the event-code table.
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut c = Cur(args, 0);
        let mut tr: Vec<f64> = Vec::new();
        let mut out: Vec<f64> = Vec::new();
        match kind {
            0 => {
                self.reset();
                self.game_config = read_map(args, &mut c.1);
                self.user_settings = c.val();
                self.is_replay = c.f() != 0.0;
                self.listed = c.f() != 0.0;
                self.spectator = c.f() != 0.0;
                out.push(0.0);
                return out;
            }
            1 => {
                let v = c.val();
                let s = match &v {
                    JsVal::Str(s) => s.as_str(),
                    _ => "",
                };
                match s {
                    "dev" => push_ok_num(&mut out, 0.0),
                    "staging" => push_ok_num(&mut out, 1.0),
                    "prod" => push_ok_num(&mut out, 2.0),
                    _ => {
                        let msg = format!("unsupported game env: {}", to_str(&v));
                        push_err(&mut out, &msg);
                    }
                }
            }
            2 => {
                let mid = c.u() as u8;
                let mut res: Vec<f64> = Vec::new();
                self.run_method(mid, &mut c, &mut tr, &mut res);
                out.push(tr.len() as f64);
                out.extend(tr.iter().copied());
                out.extend(res.iter().copied());
                return out;
            }
            3 => {
                let res = attack_logic(&mut c);
                let mut full = Vec::with_capacity(1 + res.len());
                full.push(0.0); // empty trace length (attackLogic is facade-free)
                full.extend(res.iter().copied());
                return full;
            }
            4 => {
                let t = c.s();
                let mut res: Vec<f64> = Vec::new();
                if let Some(pos) = self.unit_cache.iter().position(|(k, _)| *k == t) {
                    let shape = self.unit_cache[pos].1.clone();
                    res.push(0.0);
                    res.push(1.0);
                    res.push(6.0);
                    push_map(&mut res, &shape.dump());
                } else if let Some(shape) =
                    build_shape(&t, field(&self.game_config, "instantBuild").unwrap_or(&JsVal::Undef))
                {
                    let dump = shape.dump();
                    self.unit_cache.push((t, shape));
                    res.push(0.0);
                    res.push(0.0);
                    res.push(6.0);
                    push_map(&mut res, &dump);
                } else {
                    let msg = format!("Unexpected value: {}", t);
                    res.push(1.0);
                    push_str(&mut res, &msg);
                }
                let mut full = Vec::with_capacity(1 + res.len());
                full.push(0.0); // empty trace length (unitInfo build is facade-free)
                full.extend(res.iter().copied());
                return full;
            }
            5 => {
                let t = c.s();
                let extra = c.val();
                let mut f = c.player();
                run_unit_cost(self, &t, &extra, &mut f, &mut tr, &mut out);
                let mut full = Vec::with_capacity(1 + tr.len() + out.len());
                full.push(tr.len() as f64);
                full.extend(tr.iter().copied());
                full.extend(out.iter().copied());
                return full;
            }
            6 => {
                out.push(0.0);
                out.push(self.unit_cache.len() as f64);
                for (k, _) in &self.unit_cache {
                    push_str(&mut out, k);
                }
                return out;
            }
            k => unreachable!("config harness: unknown op kind {k}"),
        }
        let mut full = Vec::with_capacity(1 + tr.len() + out.len());
        full.push(tr.len() as f64);
        full.extend(tr.iter().copied());
        full.extend(out.iter().copied());
        full
    }

    /// Method dispatch for kind 2. `mid` follows the TS declaration order
    /// (the table in `tools/gen_vectors.mjs` documents both sides).
    fn run_method(&self, mid: u8, c: &mut Cur, tr: &mut Vec<f64>, out: &mut Vec<f64>) {
        // Every kind-2 result rides the js_json codec uniformly: success is
        // `[0, ...encVal(v)]`, a throw is `[1, ...encS(msg)]` (the capture
        // `encVal`s the real TS return value, so bigints cross as numbers and
        // booleans as `[4,b]`).
        let ok = |out: &mut Vec<f64>, v: f64| {
            out.push(0.0);
            push_val(out, &JsVal::Num(v));
        };
        let okb = |out: &mut Vec<f64>, b: bool| {
            out.push(0.0);
            push_val(out, &JsVal::Bool(b));
        };
        let ok_val = |out: &mut Vec<f64>, v: &JsVal| {
            out.push(0.0);
            push_val(out, v);
        };
        let err = |out: &mut Vec<f64>, msg: &str| {
            out.push(1.0);
            push_str(out, msg);
        };
        match mid {
            0 => okb(out, self.is_replay),
            1 => okb(out, self.spectator),
            2 => okb(out, self.listed),
            3 => ok(out, 0.5),
            4 => ok(out, 0.8),
            5 => ok(out, 30.0 * 10.0),
            6 => ok(out, 7.0),
            7 => {
                let d = doomsday_clock_config(&self.game_config);
                out.push(0.0);
                out.push(6.0);
                push_map(out, &d);
            }
            8 => {
                let d = overtime_config(&self.game_config);
                out.push(0.0);
                out.push(6.0);
                push_map(out, &d);
            }
            9 => {
                let v = field(&self.game_config, "spawnImmunityDuration");
                ok_val(out, coalesce(v, &JsVal::Num(DEFAULT_SPAWN_IMMUNITY_TICKS)));
            }
            10 => ok(out, DEFAULT_SPAWN_IMMUNITY_TICKS),
            11 => okb(out, self.spawn_immunity_duration() > DEFAULT_SPAWN_IMMUNITY_TICKS),
            12 => {
                out.push(0.0);
                out.push(6.0);
                push_map(out, &self.game_config);
            }
            13 => {
                if self.user_settings == JsVal::Null {
                    err(out, "userSettings is null");
                } else {
                    ok_val(out, &self.user_settings);
                }
            }
            14 => ok(out, 250_000.0),
            15 => {
                let x = c.f();
                ok(out, 5.0 - x * 2.0);
            }
            16 => ok(out, 100.0),
            17 => ok(out, 90.0),
            18 => ok(out, 90.0),
            19 => ok(out, 30.0),
            20 => ok(out, 5.0),
            21 => ok(out, 3.0),
            22 => {
                let v = field(&self.game_config, "playerTeams");
                ok_val(out, coalesce(v, &JsVal::Num(0.0)));
            }
            23 => {
                let v = field(&self.game_config, "nations");
                let disabled = matches!(v, Some(JsVal::Str(s)) if s == "disabled");
                okb(out, !disabled);
            }
            24 => {
                let t = c.s();
                okb(out, self.is_unit_disabled(&t));
            }
            25 => ok_val(out, field(&self.game_config, "bots").unwrap_or(&JsVal::Undef)),
            26 => ok_val(out, field(&self.game_config, "instantBuild").unwrap_or(&JsVal::Undef)),
            27 => {
                let v = field(&self.game_config, "disableNavMesh");
                ok_val(out, coalesce(v, &JsVal::Bool(false)));
            }
            28 => {
                // `customAllianceDuration === 0 || (disableAlliances ?? false)`
                // — `=== 0` is true for -0; when it is false the `||` returns
                // the RAW right operand (a `5` in disableAlliances returns 5).
                let cad = field(&self.game_config, "customAllianceDuration");
                let zero = matches!(cad, Some(JsVal::Num(n)) if *n == 0.0);
                if zero {
                    okb(out, true);
                } else {
                    let da = field(&self.game_config, "disableAlliances");
                    ok_val(out, coalesce(da, &JsVal::Bool(false)));
                }
            }
            29 => {
                let v = field(&self.game_config, "waterNukes");
                ok_val(out, coalesce(v, &JsVal::Bool(false)));
            }
            30 => ok_val(out, field(&self.game_config, "randomSpawn").unwrap_or(&JsVal::Undef)),
            31 => ok_val(out, field(&self.game_config, "infiniteGold").unwrap_or(&JsVal::Undef)),
            32 => ok_val(out, field(&self.game_config, "donateGold").unwrap_or(&JsVal::Undef)),
            33 => ok_val(out, field(&self.game_config, "infiniteTroops").unwrap_or(&JsVal::Undef)),
            34 => ok_val(out, field(&self.game_config, "donateTroops").unwrap_or(&JsVal::Undef)),
            35 => {
                let v = field(&self.game_config, "goldMultiplier");
                ok_val(out, coalesce(v, &JsVal::Num(1.0)));
            }
            36 => {
                // startingGold(playerInfo): data fields, no facade.
                let ptype = c.s();
                let is_lc = c.f();
                if ptype == "BOT" {
                    ok(out, 0.0);
                    return;
                }
                match self.starting_gold_for(is_lc != 0.0) {
                    Ok(g) => ok(out, g as f64),
                    Err(m) => err(out, &m),
                }
            }
            37 => {
                let x = c.f();
                ok(out, train_saturation(x));
            }
            38 => {
                let nf = c.f();
                let x = c.f();
                let rate = (nf + 10.0) * 15.0;
                ok(out, js_max(1.0, (rate / train_saturation(x)).floor()));
            }
            39 => {
                let rel = c.s();
                let cv = c.f();
                let mut f = c.player();
                match self.train_gold(&rel, cv, &mut f, tr) {
                    Ok(g) => ok(out, g as f64),
                    Err(m) => err(out, &m),
                }
            }
            40 => ok(out, 15.0),
            41 => ok(out, 110.0),
            42 => ok(out, 110.0 * RAILROAD_MAX_FACTOR),
            43 => {
                let dist = c.f();
                let mut f = c.player();
                // TS order: `debuff` read (no facade), baseGold computed, then
                // `BigInt(Math.floor(baseGold * this.goldMultiplierFor(player)))`
                // — the multiplier (and its possible isLobbyCreator call) runs
                // AFTER baseGold.
                let mult = self.gold_multiplier_for(&mut f, tr);
                let debuff = 300.0;
                let base = 75_000.0 / (1.0 + exp(-0.03 * (dist - debuff))) + 50.0 * dist;
                match bigint_floor(base * mult) {
                    Ok(g) => ok(out, g as f64),
                    Err(m) => err(out, &m),
                }
            }
            44 => {
                let x = c.f();
                ok(out, trade_ship_saturation(x));
            }
            45 => {
                let rej = c.f();
                let x = c.f();
                let rm = 1.0 / (rej + 1.0);
                ok(out, js_max(1.0, ((100.0 * rm) / trade_ship_saturation(x)).floor()));
            }
            46 => {
                // conquerGoldAmount(captured): TS evaluates
                // `type() === Bot || type() === Nation` — two type() calls
                // (the || short-circuits after the first when it matches),
                // then gold().
                let mut f = c.player();
                let t1 = f.f_type(tr);
                let is_full = if t1 == "BOT" {
                    true
                } else {
                    let t2 = f.f_type(tr);
                    t2 == "NATION"
                };
                let g = f.gold(tr);
                ok(out, if is_full { g as f64 } else { (g / 2) as f64 });
            }
            47 => {
                // defaultDonationAmount(sender)
                let mut f = c.player();
                let t = f.troops(tr);
                ok(out, (t / 3.0).floor());
            }
            48 => ok(out, 10.0 * 10.0),
            49 => ok(out, 10.0 * 10.0),
            50 => ok(out, 30.0 * 10.0),
            51 => ok(out, 30.0 * 10.0),
            52 => ok(out, 5.0 * 10.0),
            53 => ok(out, 5.0 * 10.0),
            54 => ok(out, 3.0 * 10.0),
            55 => ok(out, 10.0 * 10.0),
            56 => ok(out, 15.0 * 10.0),
            57 => ok(out, 20.0 * 10.0),
            58 => ok(out, 30.0 * 10.0),
            59 => {
                // allianceDuration: `typeof m === "number" && m > 0`
                let m = field(&self.game_config, "customAllianceDuration");
                match m {
                    Some(JsVal::Num(n)) if *n > 0.0 => ok(out, *n * 60.0 * 10.0),
                    _ => ok(out, 300.0 * 10.0),
                }
            }
            60 => ok(out, 300.0 * 10.0),
            61 => ok(out, 30.0),
            62 => {
                let sec = c.f();
                let enabled = truthy(&self.overtime_enabled());
                if !enabled {
                    ok(out, PERCENT_TILES_OWNED_TO_WIN);
                    return;
                }
                let start = to_num(&self.overtime_start_minutes());
                let sps = sec.floor() - start * 60.0;
                if sps <= 0.0 {
                    ok(out, PERCENT_TILES_OWNED_TO_WIN);
                    return;
                }
                ok(out, js_max(0.0, PERCENT_TILES_OWNED_TO_WIN - ((sps * 2.0) / 60.0).floor()));
            }
            63 => ok(out, 0.8),
            64 => ok(out, if self.is_unit_disabled("Transport") { 0.0 } else { 3.0 }),
            65 => {
                let gt = field(&self.game_config, "gameType");
                if matches!(gt, Some(JsVal::Str(s)) if s == "Singleplayer") {
                    ok(out, 100.0);
                } else if self.is_random_spawn() {
                    ok(out, 150.0);
                } else {
                    ok(out, 200.0);
                }
            }
            66 => ok_val(out, field(&self.game_config, "bots").unwrap_or(&JsVal::Undef)),
            67 => {
                // boatAttackAmount(attacker)
                let mut f = c.player();
                let t = f.troops(tr);
                ok(out, (t / 5.0).floor());
            }
            68 => ok(out, 20.0),
            69 => ok(out, 20.0),
            70 => ok(out, 300.0),
            71 => {
                let tp = c.f();
                ok(out, within(tp / 3.0, 4.0, tp));
            }
            72 => {
                // attackAmount(attacker)
                let mut f = c.player();
                let t = f.f_type(tr);
                let tr_val = f.troops(tr);
                ok(out, if t == "BOT" { tr_val / 20.0 } else { tr_val / 5.0 });
            }
            73 => {
                // startManpower(playerInfo): data fields.
                let ptype = c.s();
                let is_lc = c.f();
                if ptype == "BOT" {
                    ok(out, 10_000.0);
                    return;
                }
                if ptype == "NATION" {
                    match self.difficulty() {
                        JsVal::Str(s) if s == "Easy" => ok(out, 12_500.0),
                        JsVal::Str(s) if s == "Medium" => ok(out, 18_750.0),
                        JsVal::Str(s) if s == "Hard" => ok(out, 25_000.0),
                        JsVal::Str(s) if s == "Impossible" => ok(out, 31_250.0),
                        other => err(out, &format!("Unexpected value: {}", to_str(&other))),
                    }
                    return;
                }
                let inf = self.infinite_troops()
                    || (truthy(coalesce(self.hc_field("infiniteTroops"), &JsVal::Bool(false))) && is_lc != 0.0);
                ok(out, if inf { 1_000_000.0 } else { 25_000.0 });
            }
            74 => {
                let mut f = c.player();
                match self.max_troops_checked(&mut f, tr) {
                    Ok(v) => ok(out, v),
                    Err(m) => err(out, &m),
                }
            }
            75 => {
                let mut f = c.player();
                match self.troop_increase_rate(&mut f, tr) {
                    Ok(v) => ok(out, v),
                    Err(m) => err(out, &m),
                }
            }
            76 => {
                let mut f = c.player();
                let mult = self.gold_multiplier_for(&mut f, tr);
                let t = f.f_type(tr);
                let base: f64 = if t == "BOT" { 50.0 } else { 100.0 };
                match bigint_floor(base * mult) {
                    Ok(g) => ok(out, g as f64),
                    Err(m) => err(out, &m),
                }
            }
            77 => {
                let t = c.s();
                let mag: (f64, f64) = match t.as_str() {
                    "MIRV Warhead" => (12.0, 18.0),
                    "Atom Bomb" => (12.0, 30.0),
                    "Hydrogen Bomb" => (80.0, 100.0),
                    _ => {
                        err(out, &format!("Unknown nuke type: {}", t));
                        return;
                    }
                };
                let obj = JsVal::Obj(vec![
                    ("inner".to_string(), JsVal::Num(mag.0)),
                    ("outer".to_string(), JsVal::Num(mag.1)),
                ]);
                ok_val(out, &obj);
            }
            78 => ok(out, 100.0),
            79 => {
                let t = c.s();
                match t.as_str() {
                    "Atom Bomb" | "Hydrogen Bomb" => ok(out, 10.0),
                    "MIRV" => ok(out, 15.0),
                    "MIRV Warhead" => ok(out, 22.0),
                    _ => err(out, &format!("Unknown nuke type: {}", t)),
                }
            }
            80 => ok(out, 14.0),
            81 => ok(out, 150.0),
            82 => ok(out, 70.0),
            83 => {
                let l = c.f();
                ok(out, self.sam_range(l));
            }
            84 => ok(out, 150.0),
            85 => ok(out, self.sam_upgrade_duration()),
            86 => {
                let tick = c.f();
                let mut f = c.player();
                let uid = f.pid;
                let state = f.sam_launcher_state(tr, uid);
                // `state === undefined` is strict: a scripted `null` state
                // falls through and `null.upgradeStartTick` throws a V8
                // TypeError (the message is part of the golden).
                if matches!(state, JsVal::Null) {
                    err(out, "Cannot read properties of null (reading 'upgradeStartTick')");
                    return;
                }
                let ust = val_field(&state, "upgradeStartTick");
                // TS gates on `state === undefined || state.upgradeStartTick
                // === undefined` — strict undefined (absent reads undefined).
                let undef_ust = matches!(ust, None | Some(JsVal::Undef) | Some(JsVal::Absent));
                if matches!(state, JsVal::Undef | JsVal::Absent) || undef_ust {
                    let l = f.sam_level(tr, uid);
                    ok(out, self.sam_range(l));
                    return;
                }
                let duration = to_num(coalesce(
                    val_field(&state, "duration"),
                    &JsVal::Num(self.sam_upgrade_duration()),
                ));
                let elapsed = tick - to_num(ust.unwrap());
                if elapsed >= duration {
                    let tl = to_num(val_field(&state, "targetLevel").unwrap_or(&JsVal::Undef));
                    ok(out, self.sam_range(tl));
                    return;
                }
                let target_range =
                    self.sam_range(to_num(val_field(&state, "targetLevel").unwrap_or(&JsVal::Undef)));
                let start_range =
                    to_num(val_field(&state, "startRange").unwrap_or(&JsVal::Undef));
                let diff = target_range - start_range;
                ok(out, start_range + (diff * elapsed) / duration);
            }
            87 => ok(out, 12.0),
            88 => {
                let t = c.s();
                let humans = c.f();
                let tiles = c.f();
                let max_troops = c.f();
                if t != "MIRV Warhead" {
                    ok(out, (5.0 * humans) / js_max(1.0, tiles));
                    return;
                }
                let target = 0.03 * max_troops;
                let excess = js_max(0.0, humans - target);
                let ne = excess / max_troops;
                ok(out, 500.0 * (1.0 - exp(-2.0 * ne)));
            }
            89 => ok(out, 15.0),
            90 => ok(out, 50.0),
            91 => ok(out, 100.0),
            92 => ok(out, 130.0),
            93 => ok(out, 20.0),
            94 => ok(out, 5.0),
            95 => ok(out, 5.0),
            96 => ok(out, 75.0),
            97 => ok(out, 1.0),
            98 => ok(out, 150.0),
            99 => ok(out, 0.75),
            100 => ok(out, 3.0),
            101 => ok(out, 20.0),
            102 => ok(out, 20.0),
            103 => ok(out, 10.0),
            104 => ok(out, 25.0),
            105 => ok(out, 100.0),
            106 => ok(out, 20.0),
            107 => ok(out, 75.0),
            108 => ok(out, 300.0),
            m => unreachable!("config harness: unknown method id {m}"),
        }
    }

    fn starting_gold_for(&self, is_lobby_creator: bool) -> Result<i64, String> {
        let sg = field(&self.game_config, "startingGold");
        let base = bigint_from_val(coalesce(sg, &JsVal::Num(0.0)))?;
        let hcs = self.hc_field("startingGold");
        if hcs.map(truthy).unwrap_or(false) && is_lobby_creator {
            return Ok(base + bigint_from_val(hcs.unwrap())?);
        }
        Ok(base)
    }

    fn train_gold(&self, rel: &str, cv: f64, f: &mut Facades, tr: &mut Vec<f64>) -> Result<i64, String> {
        let cv = js_max(0.0, cv - 9.0);
        let base: Option<f64> = match rel {
            "ally" => Some(35_000.0),
            "team" | "other" => Some(25_000.0),
            "self" => Some(10_000.0),
            _ => None,
        };
        let dist = cv * 5_000.0;
        let gold = js_max(5000.0, base.unwrap_or(f64::NAN) - dist);
        let mult = self.gold_multiplier_for(f, tr);
        to_int_big(gold * mult)
    }

    fn max_troops_checked(&self, f: &mut Facades, tr: &mut Vec<f64>) -> Result<f64, String> {
        // TS calls `player.type()` at every comparison site: the ternary
        // guard, the outer Bot check and the outer Human check (three trace
        // events, each consuming one scripted return).
        let t = f.f_type(tr);
        let max_troops = if t == "HUMAN" && self.has_infinite_troops_for(f, tr) {
            1_000_000_000.0
        } else {
            let nto = f.num_tiles_owned(tr);
            let cities = f.units(tr, "City");
            // JS runs `.filter(!isUnderConstruction)` over EVERY unit first,
            // then `.map(level)` over the survivors — two array-method
            // phases, not interleaved (the trace order pins this).
            let survivors: Vec<f64> = cities
                .iter()
                .copied()
                .filter(|uid| !truthy(&JsVal::Num(f.is_under_construction(tr, *uid))))
                .collect();
            let mut sum = 0.0f64;
            for uid in survivors {
                sum += f.unit_level(tr, uid);
            }
            2.0 * (pow(nto, 0.6) * 1000.0 + 50_000.0) + sum * self.city_troop_increase()
        };
        let t2 = f.f_type(tr);
        if t2 == "BOT" {
            return Ok(max_troops / 3.0);
        }
        let t3 = f.f_type(tr);
        if t3 == "HUMAN" {
            return Ok(max_troops);
        }
        match self.difficulty() {
            JsVal::Str(s) if s == "Easy" => Ok(max_troops * 0.5),
            JsVal::Str(s) if s == "Medium" => Ok(max_troops * 0.75),
            JsVal::Str(s) if s == "Hard" => Ok(max_troops * 1.0),
            JsVal::Str(s) if s == "Impossible" => Ok(max_troops * 1.25),
            other => Err(format!("Unexpected value: {}", to_str(&other))),
        }
    }

    fn city_troop_increase(&self) -> f64 {
        250_000.0
    }

    fn troop_increase_rate(&self, f: &mut Facades, tr: &mut Vec<f64>) -> Result<f64, String> {
        let max = self.max_troops_checked(f, tr)?;
        let t1 = f.troops(tr);
        let mut to_add = 10.0 + pow(t1, 0.73) / 4.0;
        let t2 = f.troops(tr);
        let ratio = 1.0 - t2 / max;
        to_add *= ratio;
        // TS calls `player.type()` at both check sites (Bot, then Nation).
        let t = f.f_type(tr);
        if t == "BOT" {
            to_add *= 0.5;
        }
        let t = f.f_type(tr);
        if t == "NATION" {
            match self.difficulty() {
                JsVal::Str(s) if s == "Easy" => to_add *= 0.9,
                JsVal::Str(s) if s == "Medium" => to_add *= 0.95,
                JsVal::Str(s) if s == "Hard" => to_add *= 1.0,
                JsVal::Str(s) if s == "Impossible" => to_add *= 1.05,
                other => return Err(format!("Unexpected value: {}", to_str(&other))),
            }
        }
        let t3 = f.troops(tr);
        let t4 = f.troops(tr);
        Ok(js_min(t3 + to_add, max) - t4)
    }
}

// ---- module-level pure helpers -------------------------------------------------

fn large_territory_bonus(num_tiles: f64, depth: f64) -> f64 {
    1.0 - depth * sigmoid(log(num_tiles), LARGE_TERRITORY_STEEPNESS, log(LARGE_TERRITORY_MIDPOINT))
}

fn train_saturation(x: f64) -> f64 {
    let boost = 1.0 + 0.5 * exp(-x / 30.0);
    let damping = 1.0 - sigmoid(x, LN2 / 100.0, 560.0);
    let plateau = 0.25 * (1.0 - sigmoid(x, LN2 / 150.0, 900.0));
    boost * js_max(damping, plateau)
}

fn trade_ship_saturation(x: f64) -> f64 {
    let boost = 1.0 + 0.45 * exp(-x / 120.0);
    let damping = 1.0 - sigmoid(x, LN2 / 50.0, 330.0);
    let plateau = 0.25 * (1.0 - sigmoid(x, LN2 / 100.0, 800.0));
    boost * js_max(damping, plateau)
}

/// `doomsdayClockConfig()` — the 15-key declaration order.
fn doomsday_clock_config(gc: &[(String, JsVal)]) -> Vec<(String, JsVal)> {
    let c = field(gc, "doomsdayClock").cloned().unwrap_or(JsVal::Undef);
    let get = |k: &str, d: JsVal| -> (String, JsVal) {
        (k.to_string(), coalesce(val_field(&c, k), &d).clone())
    };
    vec![
        get("enabled", JsVal::Bool(false)),
        get("speed", JsVal::Str("normal".to_string())),
        ("warnSeconds".to_string(), JsVal::Num(30.0)),
        ("drainStartPercent".to_string(), JsVal::Num(2.0)),
        ("drainMaxPercent".to_string(), JsVal::Num(5.0)),
        ("drainRampSeconds".to_string(), JsVal::Num(90.0)),
        ("drainFloorPercent".to_string(), JsVal::Num(5.0)),
        ("floorStartPercent".to_string(), JsVal::Num(40.0)),
        ("floorDecaySeconds".to_string(), JsVal::Num(90.0)),
        ("rotDeathSeconds".to_string(), JsVal::Num(150.0)),
        ("rotGrainSeconds".to_string(), JsVal::Num(10.0)),
        ("rotSpecklePercent".to_string(), JsVal::Num(15.0)),
        ("warshipDrainStartPercent".to_string(), JsVal::Num(1.0)),
        ("warshipDrainMaxPercent".to_string(), JsVal::Num(50.0)),
        ("warshipDrainCurveExponent".to_string(), JsVal::Num(8.0)),
    ]
}

/// `overtimeConfig()` — the 3-key declaration order.
fn overtime_config(gc: &[(String, JsVal)]) -> Vec<(String, JsVal)> {
    let c = field(gc, "overtime").cloned().unwrap_or(JsVal::Undef);
    vec![
        (
            "enabled".to_string(),
            coalesce(val_field(&c, "enabled"), &JsVal::Bool(false)).clone(),
        ),
        (
            "startMinutes".to_string(),
            coalesce(val_field(&c, "startMinutes"), &JsVal::Num(30.0)).clone(),
        ),
        ("dropPercentPerMinute".to_string(), JsVal::Num(2.0)),
    ]
}

/// `terrainAttackBase(terrain)` — numeric enum values 0..4; the default
/// branch interpolates the raw number.
fn terrain_attack_base(terrain: f64) -> Result<(f64, f64), String> {
    if terrain == 0.0 {
        Ok((80.0, 16.5))
    } else if terrain == 1.0 {
        Ok((100.0, 20.0))
    } else if terrain == 2.0 {
        Ok((120.0, 25.0))
    } else if terrain == 4.0 {
        Err("impassable terrain cannot be attacked".to_string())
    } else {
        Err(format!("terrain type {} not supported", js_to_string(terrain)))
    }
}

/// kind 3: `attackLogic(input)` — the flat input block:
/// `[terrain, attackTroops, (att-type-str), attNumTiles, defPresent,
/// defType?, defNumTiles?, defTroops?, defTraitor?, defDisconn?,
/// defensePost, fallout codec, borderSize]`. The result object rides the
/// codec as `[6,3,(key,value)*3]` (declaration order attackerTroopLoss,
/// defenderTroopLoss, tickFraction).
fn attack_logic(c: &mut Cur) -> Vec<f64> {
    let terrain = c.f();
    let attack_troops = c.f();
    let att_type = c.s();
    let att_tiles = c.f();
    let def_present = c.f() != 0.0;
    let def_type = if def_present { c.s() } else { String::new() };
    let def_tiles = if def_present { c.f() } else { 0.0 };
    let def_troops = if def_present { c.f() } else { 0.0 };
    let def_traitor = if def_present { c.f() } else { 0.0 };
    let def_disconn = if def_present { c.f() } else { 0.0 };
    let has_dp = c.f() != 0.0;
    let fallout = c.val();
    let border_size = c.f();

    let mut res = Vec::new();
    let (mut mag, mut tile_cost) = match terrain_attack_base(terrain) {
        Ok(v) => v,
        Err(m) => {
            push_err(&mut res, &m);
            return res;
        }
    };

    if def_present && has_dp {
        mag *= 5.0;
        tile_cost *= 3.0;
    }
    if fallout != JsVal::Null {
        let modifier = 5.0 - to_num(&fallout) * 2.0;
        mag *= modifier;
        tile_cost *= modifier;
    }

    let push_obj3 = |res: &mut Vec<f64>, a: f64, d: f64, t: f64| {
        res.push(0.0);
        res.push(6.0);
        res.push(3.0);
        push_str(res, "attackerTroopLoss");
        push_val(res, &JsVal::Num(a));
        push_str(res, "defenderTroopLoss");
        push_val(res, &JsVal::Num(d));
        push_str(res, "tickFraction");
        push_val(res, &JsVal::Num(t));
    };

    if !def_present {
        let tick_budget = border_size * 2.0;
        let loss = mag / if att_type == "BOT" { 10.0 } else { 5.0 };
        let tf = within(
            (TERRA_NULLIUS_COST_SCALE * tile_cost) / attack_troops,
            TERRA_NULLIUS_MIN_COST,
            TERRA_NULLIUS_MAX_COST,
        ) / tick_budget;
        push_obj3(&mut res, loss, 0.0, tf);
        return res;
    }

    if def_disconn != 0.0 {
        mag = 0.0;
    }
    if (att_type == "HUMAN" || att_type == "NATION") && def_type == "BOT" {
        mag *= BOT_DEFENDER_LOSS_MULT;
    }

    let large_attacker_bonus = large_territory_bonus(att_tiles, LARGE_ATTACKER_DEPTH);
    let large_defender_bonus = large_territory_bonus(def_tiles, LARGE_DEFENDER_DEPTH);

    let traitor_loss = if def_traitor != 0.0 { 0.5 } else { 1.0 };
    let traitor_cost = if def_traitor != 0.0 { 0.8 } else { 1.0 };

    let defender_troop_loss = def_troops / def_tiles;
    let troop_ratio = def_troops / attack_troops;
    let attacker_troop_loss = mag
        * traitor_loss
        * within(troop_ratio, 0.6, 2.0)
        * (ATTACKER_LOSS_BASE * large_attacker_bonus * large_defender_bonus
            + ATTACKER_LOSS_PER_DENSITY * defender_troop_loss);

    let speed_cost = (within(troop_ratio, 0.82, 7.5) * within(troop_ratio / 20.0, 1.0, 50.0))
        / SPEED_COST_DIVISOR;
    let large_attacker_speed_bonus =
        large_territory_bonus(att_tiles, LARGE_ATTACKER_SPEED_DEPTH);
    let tick_fraction = (speed_cost
        * tile_cost
        * large_attacker_speed_bonus
        * large_defender_bonus
        * traitor_cost)
        / border_size;

    push_obj3(&mut res, attacker_troop_loss, defender_troop_loss, tick_fraction);
    res
}

/// kind 5: the cached `unitInfo(type).cost(game, player, extraUnits?)` body.
/// The bigint result crosses as `Number(v)` via the capture's `encVal`
/// (the |v| <= 2^53 domain), so the payload is `[0,3,v]` / `[1,msg]`.
fn run_unit_cost(cfg: &Config, t: &str, extra: &JsVal, f: &mut Facades, tr: &mut Vec<f64>, out: &mut Vec<f64>) {
    match t {
        "Transport" | "Shell" | "SAMMissile" | "MIRV Warhead" | "Trade Ship" | "Train" => {
            push_ok_num(out, 0.0);
        }
        "MIRV" => {
            let ty = f.f_type(tr);
            if ty == "HUMAN" && cfg.has_infinite_gold_for(f, tr) {
                push_ok_num(out, 0.0);
            } else {
                let launches = f.num_mirvs_launched(tr);
                push_ok_num(out, (25_000_000i64 + launches * 15_000_000i64) as f64);
            }
        }
        "Warship" | "Port" | "Atom Bomb" | "Hydrogen Bomb" | "Missile Silo" | "Defense Post"
        | "SAM Launcher" | "City" | "Factory" => {
            let types = wrapper_types(t).unwrap();
            let ty = f.f_type(tr);
            if ty == "HUMAN" && cfg.has_infinite_gold_for(f, tr) {
                push_ok_num(out, 0.0);
            } else {
                let mut num_units = 0.0f64;
                for &wt in &types {
                    let o = f.units_owned(tr, wt);
                    let cn = f.units_constructed(tr, wt);
                    num_units += js_min(o, cn);
                }
                // `extraUnits: number = 0` — the default fires on undefined
                // (and the absent argument), not on null / NaN.
                let extra_num = match extra {
                    JsVal::Undef | JsVal::Absent => 0.0,
                    other => to_num(other),
                };
                let extra_units = num_units + extra_num;
                match bigint_from_num(wrapper_cost_fn(t, extra_units)) {
                    Ok(g) => push_ok_num(out, g as f64),
                    Err(m) => push_err(out, &m),
                }
            }
        }
        _ => {
            // cost() only exists on a cached unitInfo; the capture always
            // calls unitInfo first, so unknown types never reach here.
            push_err(out, "no such unit type");
        }
    }
}
