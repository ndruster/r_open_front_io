//! Port of `src/core/game/UnitGrid.ts`: the 100-pixel-cell 2-D spatial index
//! over units, keyed inside each cell by the `UnitType` **string** with an
//! insertion-ordered `Set` of units per key. The `GameMap` is the real ported
//! [`GameMap`] (precedent: `railroad_spatial_grid` / `exec_util`), so
//! `gm.x / gm.y / gm.width / gm.height` are already bit-exact; the `Unit`
//! facade is a scripted mock (precedent: `nation_utils`'s `Player` mock):
//! every unit rides in as a capture-assigned `refid` and each facade call
//! (`tile()` / `type()` / `isActive()` / `isUnderConstruction()` /
//! `lastTile()` / `owner().id()`) is recorded as a trace event in the res
//! stream, pinning the call *counts* (e.g. `removeUnit` calls `tile()` once
//! and `removeUnitByTile` never again; `updateUnitCell`'s cross-cell move
//! calls `tile()` again inside `addUnit`). The `UnitPredicate` callbacks are
//! scripted return-value streams: the capture runs a real JS closure and
//! traces every invocation (event 16 for `nearbyUnits`, 17 for
//! `anyUnitNearby`), the replay feeds the same stream. The `owner()` facade
//! is two-stage in TS (`unit.owner().id()`); the mock always returns a player
//! whose `id()` is the scripted id, so the observable pair collapses to one
//! event 15 `[15, refid, id]` per probe (modelling note).
//!
//! Faithfulness notes:
//!
//! * JS `Map` / `Set` iterate in insertion order with SameValueZero keys:
//!   `Set.add` of a present member is a no-op that does **not** move it, while
//!   `delete` + re-`add` moves it to the tail; `Map.set` on a new key appends,
//!   on an existing key overwrites in place. [`RefSet`] / [`Cell`] replicate
//!   this (keys are UTF-16 code-unit vectors crossing as `[len,u0,..]`).
//! * `addUnit` reads `unit.type()` **twice** when the key is missing (`get`
//!   then `set` in the source) and once on the existing-key path; the trace
//!   pins both call counts (`ug_add_order`).
//! * `isValidCell` evaluates `gx >= 0 && gx < this.grid[0].length && ...` left
//!   to right: a failing `gx >= 0` (NaN included) short-circuits **before**
//!   `grid[0]` is read, but a passing `gx >= 0` on a 0-row grid (a 0-height
//!   `GameMap`: `Math.ceil(0/100) = 0`) reads `.length` off `undefined` and
//!   throws a `TypeError`. The capture pins that throw as op status 1 with the
//!   partial trace preserved; the Rust replay models it with [`Throw`].
//! * `updateUnitCell` compares tiles with `===` (SameValueZero: `+0 === -0`,
//!   `NaN !== NaN`) and, on a difference, computes the four `Math.floor`
//!   divisions without allocating the coordinate tuples; the cell compare is
//!   `!==`, then `removeUnitByTile(unit, oldTile)` (no extra `tile()`) and
//!   `addUnit(unit)` (one more `tile()` + one or two `type()` calls).
//! * `getCellsInRange` uses JS `%` (via `GameMap::x`, sign follows the
//!   dividend, `-0` kept) and the NaN-propagating / ±0-correct
//!   [`crate::game_map::js_min`] / [`crate::game_map::js_max`] left folds.
//!   `Math.ceil(-0.05)` is `-0`, so a negative range can still leave a
//!   single-cell window (`max(0, -0) = 0`, `min(cols-1, -0) = -0`, and
//!   `0 <= -0` is true). A NaN range makes every bound NaN and the
//!   `cy <= endGridY` test false, yielding an empty scan. `±Infinity` ranges
//!   clamp to the full grid and are outside the captured domain (they cannot
//!   loop forever: the bounds go through `max(0, …)` / `min(len-1, …)`).
//! * `nearbyUnits`' array branch iterates `cy -> cx -> types -> unitSet`
//!   (the declared `types` order inside each cell), while the scalar branch
//!   iterates `cy -> cx -> unitSet` — the same grid can yield different
//!   result orders, pinned by `ug_nearby_order`. The distance filter is a
//!   strict `distSquared > rangeSquared` (equality survives); `hasUnitNearby`
//!   / `anyUnitNearby` share `unitIsInRange`'s complementary `<=`.
//! * `unitIsInRange` short-circuits `!isActive()` -> `!includeUC &&
//!   isUnderConstruction()` -> `playerId !== undefined && unit.owner().id()
//!   !== playerId` (the owner facade is only touched when `playerId` is
//!   defined) -> `distSquared <= rangeSquared` (false for NaN).
//! * Fractional grid coordinates can pass `isValidCell` (it only compares
//!   numbers), after which `grid[gy][gx]` would index `undefined` and throw
//!   in TS; that path is outside the captured domain (every reachable
//!   `Math.floor` division yields an integer for finite inputs).

use std::collections::HashMap;

use crate::game_map::{js_max, js_min, GameMap};

/// `private readonly cellSize = 100`.
const CELL_SIZE: f64 = 100.0;

/// The sentinel for a JS `TypeError` (only the 0-row `grid[0].length` read
/// reaches it in the captured domain); the harness converts it into op
/// status 1 with the partial trace preserved.
#[derive(Debug, Clone, Copy)]
pub struct Throw;

/// SameValueZero key for a `Set<Unit>` member riding in as a `refid`
/// (`+0` / `-0` collapse, matching `Set`'s keying).
fn svz_key(v: f64) -> u64 {
    if v == 0.0 {
        0.0f64.to_bits()
    } else {
        v.to_bits()
    }
}

/// A JS `Set<Unit>` (refids) with insertion-order iteration: `add` of a
/// present member is a no-op that does not move it, `delete` keeps the
/// survivors' relative order, and a re-`add` after `delete` goes to the tail.
#[derive(Debug, Default, Clone)]
struct RefSet {
    vals: Vec<f64>,
    idx: HashMap<u64, usize>,
}

impl RefSet {
    fn add(&mut self, v: f64) {
        let k = svz_key(v);
        if self.idx.contains_key(&k) {
            return;
        }
        self.idx.insert(k, self.vals.len());
        self.vals.push(v);
    }
    fn delete(&mut self, v: f64) {
        if let Some(pos) = self.idx.remove(&svz_key(v)) {
            self.vals.remove(pos);
            for (j, &x) in self.vals.iter().enumerate().skip(pos) {
                self.idx.insert(svz_key(x), j);
            }
        }
    }
    fn iter(&self) -> impl Iterator<Item = f64> + '_ {
        self.vals.iter().copied()
    }
}

/// One cell: a JS `Map<UnitType, Set<Unit>>` keyed by UTF-16 code units,
/// preserving key insertion order (`set` on an existing key overwrites in
/// place, a new key appends).
#[derive(Debug, Default, Clone)]
struct Cell {
    entries: Vec<(Vec<u16>, RefSet)>,
}

impl Cell {
    fn get(&self, key: &[u16]) -> Option<&RefSet> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, s)| s)
    }
    fn get_mut(&mut self, key: &[u16]) -> Option<&mut RefSet> {
        self.entries
            .iter_mut()
            .find(|(k, _)| k == key)
            .map(|(_, s)| s)
    }
    fn set_new(&mut self, key: Vec<u16>, member: f64) {
        let mut s = RefSet::default();
        s.add(member);
        self.entries.push((key, s));
    }
}

/// `UnitGrid` over the real [`GameMap`]: `grid[gy][gx]` (row-major,
/// `ceil(height/cs)` rows x `ceil(width/cs)` cols).
pub struct UnitGrid {
    gm: GameMap,
    grid: Vec<Vec<Cell>>,
}

impl UnitGrid {
    /// The constructor's `Array(Math.ceil(gm.height()/cs)).fill(null).map(...)
    /// .` — the sparse-then-fill dance is runtime-equivalent to plain nested
    /// vecs. `width()/height()` are real non-negative integers here, so
    /// `Math.ceil` never yields NaN (the `Array(NaN)` `RangeError` is outside
    /// the domain).
    pub fn new(gm: GameMap) -> Self {
        let rows = (gm.height() / CELL_SIZE).ceil();
        let cols = (gm.width() / CELL_SIZE).ceil();
        let grid = vec![vec![Cell::default(); cols as usize]; rows as usize];
        Self { gm, grid }
    }

    fn rows_f(&self) -> f64 {
        self.grid.len() as f64
    }
    /// `this.grid[0].length` — throws when `grid[0]` is `undefined`
    /// (a 0-row grid).
    fn cols_f(&self) -> Result<f64, Throw> {
        match self.grid.first() {
            Some(row) => Ok(row.len() as f64),
            None => Err(Throw),
        }
    }

    /// `isValidCell(gridX, gridY)`: `gx >= 0 && gx < this.grid[0].length &&
    /// gy >= 0 && gy < this.grid.length` with JS `&&` evaluation order.
    #[allow(clippy::neg_cmp_op_on_partial_ord)] // `!(gx >= 0.0)`: NaN must be false
    fn is_valid_cell(&self, gx: f64, gy: f64) -> Result<bool, Throw> {
        if !(gx >= 0.0) {
            return Ok(false);
        }
        let cols = self.cols_f()?;
        if !(gx < cols) {
            return Ok(false);
        }
        if !(gy >= 0.0) {
            return Ok(false);
        }
        Ok(gy < self.rows_f())
    }

    /// `getCellsInRange(tile, range)`.
    fn get_cells_in_range(&self, tile: f64, range: f64) -> Result<[f64; 4], Throw> {
        let x = self.gm.x(tile);
        let y = self.gm.y(tile);
        let grid_x = (x / CELL_SIZE).floor();
        let grid_y = (y / CELL_SIZE).floor();
        let start_grid_x = js_max(
            0.0,
            grid_x - ((range - x % CELL_SIZE) / CELL_SIZE).ceil(),
        );
        let end_grid_x = js_min(
            self.cols_f()? - 1.0,
            grid_x + ((range - (CELL_SIZE - x % CELL_SIZE)) / CELL_SIZE).ceil(),
        );
        let start_grid_y = js_max(
            0.0,
            grid_y - ((range - y % CELL_SIZE) / CELL_SIZE).ceil(),
        );
        let end_grid_y = js_min(
            self.rows_f() - 1.0,
            grid_y + ((range - (CELL_SIZE - y % CELL_SIZE)) / CELL_SIZE).ceil(),
        );
        Ok([start_grid_x, end_grid_x, start_grid_y, end_grid_y])
    }
}

/// JS truthiness of a scripted predicate return (`0` / `NaN` / `undefined`
/// are falsy; the capture scripts plain `0` / `1`).
fn js_truthy(v: Option<f64>) -> bool {
    match v {
        None => false,
        Some(x) => !(x == 0.0 || x.is_nan()),
    }
}

/// The scripted `Unit` mock: the observable facade state the capture drives.
#[derive(Debug)]
struct UnitMock {
    tile: f64,
    last: f64,
    utype: Vec<u16>,
    active: f64,
    uc: f64,
    owner: f64,
}

/// Stateful parity harness: owns the grid (built by the construct op), the
/// unit table (refid -> mock) and replays the recorded op stream.
///
/// Every op's res is `[traceLen, (trace)*, status, payload*]`; `status` 1
/// means the JS call threw (the 0-row `grid[0].length` read) with the partial
/// trace preserved. Trace events:
/// 10 tile `[10,refid,tile]`, 11 type `[11,refid,len,(u16)*]`,
/// 12 isActive `[12,refid,0|1]`, 13 isUnderConstruction `[13,refid,0|1]`,
/// 14 lastTile `[14,refid,tile]`, 15 owner().id() `[15,refid,id]`,
/// 16 nearbyUnits predicate `[16,refid,distSquared,ret]`,
/// 17 anyUnitNearby predicate `[17,refid,ret]`.
///
/// Ops (args after the kind token):
/// 0 construct `[width,height]` -> `[rows,cols]`;
/// 1 define_unit `[refid,(str)type,tile,last,active,uc,owner]` -> `[]`;
/// 2 set_tile `[refid,tile,last]` -> `[]`;
/// 3 addUnit `[refid]` -> `[]`; 4 removeUnit `[refid]` -> `[]`;
/// 5 removeUnitByTile `[refid,tile]` -> `[]`; 6 updateUnitCell `[refid]` -> `[]`;
/// 7 isValidCell `[gx,gy]` -> `[0|1]`; 8 getCellsInRange `[tile,range]` ->
/// `[sx,ex,sy,ey]`; 9 squaredDistanceFromTile `[refid,tile]` -> `[d2]`;
/// 10 nearbyUnits `[tile,range,mode,(types),predMode,(script),incUC]` ->
/// `[len,(refid,d2)*]` (mode 1 = scalar type, 0 = array);
/// 11 hasUnitNearby `[tile,range,(str)type,pidMode,pid,incUC]` -> `[0|1]`;
/// 12 anyUnitNearby `[tile,range,n,(types),n,(script),pidMode,pid,incUC]` ->
/// `[0|1]`; 13 dump -> `[rows,cols,(nkeys,(str key,m,(refid)*m)*)*]`.
#[derive(Default)]
pub struct RigHarness {
    grid: Option<UnitGrid>,
    units: HashMap<u64, UnitMock>,
}

impl RigHarness {
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop the grid and the unit table (one scenario ends; the next starts
    /// with a construct op).
    pub fn reset(&mut self) {
        self.grid = None;
        self.units = HashMap::new();
    }

    fn g(&self) -> &UnitGrid {
        self.grid.as_ref().expect("unit grid harness: grid not constructed")
    }

    // ---- scripted facade probes (each pushes its trace event) ----

    fn tile(&self, refid: f64, trace: &mut Vec<f64>) -> f64 {
        let t = self.units[&svz_key(refid)].tile;
        trace.push(10.0);
        trace.push(refid);
        trace.push(t);
        t
    }

    fn utype(&self, refid: f64, trace: &mut Vec<f64>) -> Vec<u16> {
        let k = &self.units[&svz_key(refid)].utype;
        trace.push(11.0);
        trace.push(refid);
        trace.push(k.len() as f64);
        trace.extend(k.iter().map(|&c| f64::from(c)));
        k.clone()
    }

    fn is_active(&self, refid: f64, trace: &mut Vec<f64>) -> bool {
        let a = self.units[&svz_key(refid)].active;
        trace.push(12.0);
        trace.push(refid);
        trace.push(a);
        a == 1.0
    }

    fn is_uc(&self, refid: f64, trace: &mut Vec<f64>) -> bool {
        let u = self.units[&svz_key(refid)].uc;
        trace.push(13.0);
        trace.push(refid);
        trace.push(u);
        u == 1.0
    }

    fn last_tile(&self, refid: f64, trace: &mut Vec<f64>) -> f64 {
        let t = self.units[&svz_key(refid)].last;
        trace.push(14.0);
        trace.push(refid);
        trace.push(t);
        t
    }

    fn owner_id(&self, refid: f64, trace: &mut Vec<f64>) -> f64 {
        let o = self.units[&svz_key(refid)].owner;
        trace.push(15.0);
        trace.push(refid);
        trace.push(o);
        o
    }

    // ---- UnitGrid methods (trace + Throw-propagating) ----

    fn add_unit(&mut self, refid: f64, trace: &mut Vec<f64>) -> Result<(), Throw> {
        let tile = self.tile(refid, trace);
        let gx = (self.g().gm.x(tile) / CELL_SIZE).floor();
        let gy = (self.g().gm.y(tile) / CELL_SIZE).floor();
        if !self.g().is_valid_cell(gx, gy)? {
            return Ok(());
        }
        let key = self.utype(refid, trace);
        let (gx, gy) = (gx as usize, gy as usize);
        if self.g().grid[gy][gx].get(&key).is_some() {
            let cell = &mut self.grid.as_mut().unwrap().grid[gy][gx];
            if let Some(set) = cell.get_mut(&key) {
                set.add(refid);
            }
        } else {
            // `set(unit.type(), new Set([unit]))` — the JS source calls
            // `type()` a *second* time on the new-key branch; the trace pins
            // it.
            let key = self.utype(refid, trace);
            let cell = &mut self.grid.as_mut().unwrap().grid[gy][gx];
            cell.set_new(key, refid);
        }
        Ok(())
    }

    fn remove_unit(&mut self, refid: f64, tile: f64, trace: &mut Vec<f64>) -> Result<(), Throw> {
        let gx = (self.g().gm.x(tile) / CELL_SIZE).floor();
        let gy = (self.g().gm.y(tile) / CELL_SIZE).floor();
        if !self.g().is_valid_cell(gx, gy)? {
            return Ok(());
        }
        let key = self.utype(refid, trace);
        if let Some(set) = self.grid.as_mut().unwrap().grid[gy as usize][gx as usize].get_mut(&key) {
            set.delete(refid);
        }
        Ok(())
    }

    fn update_unit_cell(&mut self, refid: f64, trace: &mut Vec<f64>) -> Result<(), Throw> {
        let new_tile = self.tile(refid, trace);
        let old_tile = self.last_tile(refid, trace);
        // `newTile === oldTile` — SameValueZero on numbers.
        if new_tile == old_tile {
            return Ok(());
        }
        let gx = (self.g().gm.x(old_tile) / CELL_SIZE).floor();
        let gy = (self.g().gm.y(old_tile) / CELL_SIZE).floor();
        let new_gx = (self.g().gm.x(new_tile) / CELL_SIZE).floor();
        let new_gy = (self.g().gm.y(new_tile) / CELL_SIZE).floor();
        if gx != new_gx || gy != new_gy {
            self.remove_unit(refid, old_tile, trace)?;
            self.add_unit(refid, trace)?;
        }
        Ok(())
    }

    /// `squaredDistanceFromTile(unit, tile)` — `gm.x/y(tile)` first, then
    /// `unit.tile()` **twice** (`gm.x(unit.tile())` and `gm.y(unit.tile())`
    /// each call it — the trace pins the double call).
    fn squared_distance_from_tile(&self, refid: f64, tile: f64, trace: &mut Vec<f64>) -> f64 {
        let x = self.g().gm.x(tile);
        let y = self.g().gm.y(tile);
        let tile_x = self.g().gm.x(self.tile(refid, trace));
        let tile_y = self.g().gm.y(self.tile(refid, trace));
        let dx = tile_x - x;
        let dy = tile_y - y;
        dx * dx + dy * dy
    }

    /// `unitIsInRange` — the shared short-circuit chain.
    fn unit_is_in_range(
        &self,
        refid: f64,
        tile: f64,
        range_squared: f64,
        player_id: Option<f64>,
        include_uc: bool,
        trace: &mut Vec<f64>,
    ) -> Result<bool, Throw> {
        if !self.is_active(refid, trace) {
            return Ok(false);
        }
        if !include_uc && self.is_uc(refid, trace) {
            return Ok(false);
        }
        if let Some(pid) = player_id {
            if self.owner_id(refid, trace) != pid {
                return Ok(false);
            }
        }
        let d2 = self.squared_distance_from_tile(refid, tile, trace);
        Ok(d2 <= range_squared)
    }

    #[allow(clippy::too_many_arguments)] // faithful to the TS parameter list
    fn nearby_units(
        &self,
        tile: f64,
        range: f64,
        types: &[Vec<u16>],
        array_branch: bool,
        pred: &mut Script,
        include_uc: bool,
        trace: &mut Vec<f64>,
    ) -> Result<Vec<(f64, f64)>, Throw> {
        let mut nearby: Vec<(f64, f64)> = Vec::new();
        let x = self.g().gm.x(tile);
        let y = self.g().gm.y(tile);
        let [sx, ex, sy, ey] = self.g().get_cells_in_range(tile, range)?;
        let range_squared = range * range;
        let mut cy = sy;
        while cy <= ey {
            let mut cx = sx;
            while cx <= ex {
                let cell = &self.g().grid[cy as usize][cx as usize];
                if array_branch {
                    for t in types {
                        let Some(set) = cell.get(t) else { continue };
                        for refid in set.iter() {
                            if !self.is_active(refid, trace) {
                                continue;
                            }
                            if !include_uc && self.is_uc(refid, trace) {
                                continue;
                            }
                            let ut = self.tile(refid, trace);
                            let dx = self.g().gm.x(ut) - x;
                            let dy = self.g().gm.y(ut) - y;
                            let d2 = dx * dx + dy * dy;
                            if d2 > range_squared {
                                continue;
                            }
                            let ret = pred.next();
                            if pred.enabled {
                                trace.push(16.0);
                                trace.push(refid);
                                trace.push(d2);
                                trace.push(ret.unwrap_or(f64::NAN));
                            }
                            if pred.enabled && !js_truthy(ret) {
                                continue;
                            }
                            nearby.push((refid, d2));
                        }
                    }
                } else {
                    // The scalar branch's `continue` advances cx.
                    let Some(set) = cell.get(&types[0]) else {
                        cx += 1.0;
                        continue;
                    };
                    for refid in set.iter() {
                        if !self.is_active(refid, trace) {
                            continue;
                        }
                        if !include_uc && self.is_uc(refid, trace) {
                            continue;
                        }
                        let ut = self.tile(refid, trace);
                        let dx = self.g().gm.x(ut) - x;
                        let dy = self.g().gm.y(ut) - y;
                        let d2 = dx * dx + dy * dy;
                        if d2 > range_squared {
                            continue;
                        }
                        let ret = pred.next();
                        if pred.enabled {
                            trace.push(16.0);
                            trace.push(refid);
                            trace.push(d2);
                            trace.push(ret.unwrap_or(f64::NAN));
                        }
                        if pred.enabled && !js_truthy(ret) {
                            continue;
                        }
                        nearby.push((refid, d2));
                    }
                }
                cx += 1.0;
            }
            cy += 1.0;
        }
        Ok(nearby)
    }

    fn has_unit_nearby(
        &self,
        tile: f64,
        range: f64,
        utype: &[u16],
        player_id: Option<f64>,
        include_uc: bool,
        trace: &mut Vec<f64>,
    ) -> Result<bool, Throw> {
        let [sx, ex, sy, ey] = self.g().get_cells_in_range(tile, range)?;
        let range_squared = range * range;
        let mut cy = sy;
        while cy <= ey {
            let mut cx = sx;
            while cx <= ex {
                if let Some(set) = self.g().grid[cy as usize][cx as usize].get(utype) {
                    for refid in set.iter() {
                        if self.unit_is_in_range(refid, tile, range_squared, player_id, include_uc, trace)? {
                            return Ok(true);
                        }
                    }
                }
                cx += 1.0;
            }
            cy += 1.0;
        }
        Ok(false)
    }

    #[allow(clippy::too_many_arguments)] // faithful to the TS parameter list
    fn any_unit_nearby(
        &self,
        tile: f64,
        range: f64,
        types: &[Vec<u16>],
        pred: &mut Script,
        player_id: Option<f64>,
        include_uc: bool,
        trace: &mut Vec<f64>,
    ) -> Result<bool, Throw> {
        let [sx, ex, sy, ey] = self.g().get_cells_in_range(tile, range)?;
        let range_squared = range * range;
        let mut cy = sy;
        while cy <= ey {
            let mut cx = sx;
            while cx <= ex {
                for t in types {
                    let Some(set) = self.g().grid[cy as usize][cx as usize].get(t) else {
                        continue;
                    };
                    for refid in set.iter() {
                        if !self
                            .unit_is_in_range(refid, tile, range_squared, player_id, include_uc, trace)?
                        {
                            continue;
                        }
                        let ret = pred.next();
                        trace.push(17.0);
                        trace.push(refid);
                        trace.push(ret.unwrap_or(f64::NAN));
                        if js_truthy(ret) {
                            return Ok(true);
                        }
                    }
                }
                cx += 1.0;
            }
            cy += 1.0;
        }
        Ok(false)
    }

    // ---- op replay ----

    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut c = Cur(args, 0);
        let mut trace: Vec<f64> = Vec::new();
        let mut payload: Vec<f64> = Vec::new();
        let r = self.exec_op(kind, &mut c, &mut trace, &mut payload);
        let status = if r.is_err() {
            payload.clear();
            1.0
        } else {
            0.0
        };
        let mut out = Vec::with_capacity(2 + trace.len() + payload.len());
        out.push(trace.len() as f64);
        out.extend(trace.iter().copied());
        out.push(status);
        out.extend(payload.iter().copied());
        out
    }

    fn exec_op(
        &mut self,
        kind: u8,
        c: &mut Cur,
        trace: &mut Vec<f64>,
        payload: &mut Vec<f64>,
    ) -> Result<(), Throw> {
        match kind {
            0 => {
                let w = c.f();
                let h = c.f();
                let map = GameMap::new(w, h, vec![0u8; (w * h) as usize], w * h);
                let grid = UnitGrid::new(map);
                payload.push(grid.rows_f());
                payload.push(grid.cols_f().unwrap_or(0.0));
                self.grid = Some(grid);
                Ok(())
            }
            1 => {
                let refid = c.f();
                let utype = c.units();
                let tile = c.f();
                let last = c.f();
                let active = c.f();
                let uc = c.f();
                let owner = c.f();
                self.units.insert(
                    svz_key(refid),
                    UnitMock {
                        tile,
                        last,
                        utype,
                        active,
                        uc,
                        owner,
                    },
                );
                Ok(())
            }
            2 => {
                let k = svz_key(c.f());
                let tile = c.f();
                let last = c.f();
                let u = self.units.get_mut(&k).expect("unit grid harness: unknown refid");
                u.tile = tile;
                u.last = last;
                Ok(())
            }
            3 => {
                let refid = c.f();
                self.add_unit(refid, trace)
            }
            4 => {
                let refid = c.f();
                let tile = self.tile(refid, trace);
                self.remove_unit(refid, tile, trace)
            }
            5 => {
                let refid = c.f();
                let tile = c.f();
                self.remove_unit(refid, tile, trace)
            }
            6 => {
                let refid = c.f();
                self.update_unit_cell(refid, trace)
            }
            7 => {
                let gx = c.f();
                let gy = c.f();
                payload.push(if self.g().is_valid_cell(gx, gy)? { 1.0 } else { 0.0 });
                Ok(())
            }
            8 => {
                let tile = c.f();
                let range = c.f();
                payload.extend(self.g().get_cells_in_range(tile, range)?);
                Ok(())
            }
            9 => {
                let refid = c.f();
                let tile = c.f();
                payload.push(self.squared_distance_from_tile(refid, tile, trace));
                Ok(())
            }
            10 => {
                let tile = c.f();
                let range = c.f();
                let array_branch = c.f() == 0.0;
                let types = if array_branch {
                    let n = c.u();
                    (0..n).map(|_| c.units()).collect::<Vec<_>>()
                } else {
                    vec![c.units()]
                };
                let mut pred = Script {
                    enabled: c.f() != 0.0,
                    script: c.list(),
                    i: 0,
                };
                let include_uc = c.f() != 0.0;
                let out = self.nearby_units(
                    tile,
                    range,
                    &types,
                    array_branch,
                    &mut pred,
                    include_uc,
                    trace,
                )?;
                payload.push(out.len() as f64);
                for (r, d2) in out {
                    payload.push(r);
                    payload.push(d2);
                }
                Ok(())
            }
            11 => {
                let tile = c.f();
                let range = c.f();
                let utype = c.units();
                let pid = if c.f() != 0.0 { Some(c.f()) } else { c.f(); None };
                let include_uc = c.f() != 0.0;
                payload.push(if self.has_unit_nearby(tile, range, &utype, pid, include_uc, trace)? {
                    1.0
                } else {
                    0.0
                });
                Ok(())
            }
            12 => {
                let tile = c.f();
                let range = c.f();
                let n = c.u();
                let types: Vec<Vec<u16>> = (0..n).map(|_| c.units()).collect();
                let mut pred = Script {
                    enabled: true,
                    script: c.list(),
                    i: 0,
                };
                let pid = if c.f() != 0.0 { Some(c.f()) } else { c.f(); None };
                let include_uc = c.f() != 0.0;
                payload.push(if self.any_unit_nearby(tile, range, &types, &mut pred, pid, include_uc, trace)? {
                    1.0
                } else {
                    0.0
                });
                Ok(())
            }
            13 => {
                let g = self.g();
                payload.push(g.rows_f());
                payload.push(g.cols_f().unwrap_or(0.0));
                for row in &g.grid {
                    for cell in row {
                        payload.push(cell.entries.len() as f64);
                        for (k, set) in &cell.entries {
                            payload.push(k.len() as f64);
                            payload.extend(k.iter().map(|&x| f64::from(x)));
                            payload.push(set.vals.len() as f64);
                            payload.extend(set.vals.iter().copied());
                        }
                    }
                }
                Ok(())
            }
            _ => panic!("bad unit grid op kind {kind}"),
        }
    }
}

/// A scripted predicate callback: `enabled` distinguishes `undefined`
/// (predicate absent) from a real closure; `next()` yields the scripted
/// return (JS `undefined` past the end of the stream is modeled as `None`,
/// which is falsy — never exercised by the sized capture scripts).
struct Script {
    enabled: bool,
    script: Vec<f64>,
    i: usize,
}

impl Script {
    fn next(&mut self) -> Option<f64> {
        if !self.enabled {
            return None;
        }
        let v = self.script.get(self.i).copied();
        self.i += 1;
        v
    }
}

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
    fn units(&mut self) -> Vec<u16> {
        let len = self.u();
        (0..len).map(|_| self.f() as u16).collect()
    }
    fn list(&mut self) -> Vec<f64> {
        let len = self.u();
        (0..len).map(|_| self.f()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc_str(s: &str) -> Vec<f64> {
        let u: Vec<u16> = s.encode_utf16().collect();
        let mut v = vec![u.len() as f64];
        v.extend(u.iter().map(|&x| f64::from(x)));
        v
    }

    fn harness_300x100() -> RigHarness {
        let mut h = RigHarness::new();
        h.run_op(0, &[300.0, 100.0]);
        h
    }

    #[allow(clippy::too_many_arguments)] // mirrors the op-1 arg layout
    fn define(h: &mut RigHarness, refid: f64, ty: &str, tile: f64, last: f64, active: f64, uc: f64, owner: f64) {
        let mut a = vec![refid];
        a.extend(enc_str(ty));
        a.extend([tile, last, active, uc, owner]);
        h.run_op(1, &a);
    }

    #[test]
    fn ctor_ceil_grid() {
        let mut h = harness_300x100();
        // [traceLen, status, rows=1, cols=3, three empty cells].
        assert_eq!(h.run_op(13, &[]), vec![0.0, 0.0, 1.0, 3.0, 0.0, 0.0, 0.0]);
        let mut h2 = RigHarness::new();
        h2.run_op(0, &[250.0, 150.0]);
        let d = h2.run_op(13, &[]);
        assert_eq!(&d[2..4], &[2.0, 3.0]); // rows, cols
    }

    #[test]
    fn add_dup_keeps_order() {
        let mut h = harness_300x100();
        define(&mut h, 1.0, "City", 50.0, 50.0, 1.0, 0.0, 7.0);
        define(&mut h, 2.0, "City", 60.0, 60.0, 1.0, 0.0, 7.0);
        h.run_op(3, &[1.0]);
        h.run_op(3, &[2.0]);
        h.run_op(3, &[1.0]); // dup add: no-op, order stays [1, 2]
        let dump = h.run_op(13, &[]);
        // [traceLen, status, rows=1, cols=3, cell0: 1 key "City" {1,2}, 2 empty]
        assert_eq!(
            dump,
            vec![0.0, 0.0, 1.0, 3.0, 1.0, 4.0, 67.0, 105.0, 116.0, 121.0, 2.0, 1.0, 2.0, 0.0, 0.0]
        );
    }

    #[test]
    fn delete_readd_moves_to_tail() {
        let mut h = harness_300x100();
        define(&mut h, 1.0, "City", 50.0, 50.0, 1.0, 0.0, 7.0);
        define(&mut h, 2.0, "City", 60.0, 60.0, 1.0, 0.0, 7.0);
        h.run_op(3, &[1.0]);
        h.run_op(3, &[2.0]);
        h.run_op(5, &[1.0, 50.0]);
        h.run_op(3, &[1.0]);
        let dump = h.run_op(13, &[]);
        // Set order after delete+re-add: [2, 1].
        assert_eq!(
            dump,
            vec![0.0, 0.0, 1.0, 3.0, 1.0, 4.0, 67.0, 105.0, 116.0, 121.0, 2.0, 2.0, 1.0, 0.0, 0.0]
        );
    }

    #[test]
    fn zero_height_isvalidcell_throws() {
        let mut h = RigHarness::new();
        h.run_op(0, &[100.0, 0.0]);
        // (0,0) reaches grid[0].length on an empty grid -> throw.
        assert_eq!(h.run_op(7, &[0.0, 0.0]), vec![0.0, 1.0]);
        // (-1,0) short-circuits before grid[0].
        assert_eq!(h.run_op(7, &[-1.0, 0.0]), vec![0.0, 0.0, 0.0]);
        // (NaN,0) likewise.
        assert_eq!(h.run_op(7, &[f64::NAN, 0.0]), vec![0.0, 0.0, 0.0]);
    }

    #[test]
    fn update_same_tile_short_circuits() {
        let mut h = harness_300x100();
        define(&mut h, 1.0, "City", 50.0, 50.0, 1.0, 0.0, 7.0);
        // tile() + lastTile() only.
        assert_eq!(h.run_op(6, &[1.0]), vec![6.0, 10.0, 1.0, 50.0, 14.0, 1.0, 50.0, 0.0]);
    }

    #[test]
    fn nearby_boundary_and_predicate() {
        let mut h = harness_300x100();
        define(&mut h, 1.0, "City", 50.0, 50.0, 1.0, 0.0, 7.0);
        define(&mut h, 2.0, "City", 60.0, 60.0, 1.0, 0.0, 7.0);
        define(&mut h, 3.0, "City", 70.0, 70.0, 1.0, 0.0, 7.0);
        h.run_op(3, &[1.0]);
        h.run_op(3, &[2.0]);
        h.run_op(3, &[3.0]);
        // range 10: d2 0 and 100 (== rangeSquared) survive, 400 drops.
        let mut a = vec![50.0, 10.0, 1.0];
        a.extend(enc_str("City"));
        a.extend([0.0, 0.0, 0.0]);
        let r = h.run_op(10, &a);
        let n = 1 + r[0] as usize;
        assert_eq!(&r[n..], &[0.0, 2.0, 1.0, 0.0, 2.0, 100.0]);
    }
}
