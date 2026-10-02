//! Port of `src/core/game/RailNetworkImpl.ts`: the `RailNetworkImpl` station
//! network orchestrator and the `createRailNetwork` factory.
//! The `RailPathFinderServiceImpl` is **not** ported (the capture injects a
//! scripted `pathService` mock: `findTilePath` consults a numeric-keyed
//! table, while `findStationsPath` is keyed by station *objects* against a
//! numeric table — `"[object Object]|[object Object]"` never hits, so it
//! always returns an empty path; the sp table entries ride in the construct
//! args only to be consumed); the type-only interfaces (`StationManager`,
//! `RailNetwork`, `RailPathFinderService`) carry no runtime.
//!
//! The `Game` (`mg`) is a scripted mock (`x / y / addUpdate / config /
//! hasUnitNearby / nearbyUnits`) and the `Unit` facade (`type / tile /
//! setTrainStation`) is scripted too (precedent: `nation_utils` /
//! `unit_grid` / `train_station`): every facade call is recorded as a trace
//! event and rides in the res stream, pinning the call counts and
//! short-circuits. The `StationManagerImpl`, `TrainStation` / `Cluster` and
//! `Railroad` semantics are re-implemented here against the same model the
//! `station_manager` / `train_station` / `railroad` ports pin (the harness
//! owns the whole network graph so the orchestration is exercised end-to-end).
//! The harness is stateful (op stream): `res = [traceLen, (trace)*, payload*]`.
//!
//! Faithfulness notes:
//!
//! * JS `Set` / `Map` iterate in insertion order with SameValueZero keys;
//!   `Set.add` of a present member does **not** move it, `delete` + re-`add`
//!   moves to the tail — [`RefSet`] / [`NumMap`] replicate this. Object
//!   identity (`rail.from === this`, `start === dest`, `neighbor.unit ===
//!   station.unit`) is JS `===` on refids: `NaN` never matches, `+0`/`-0` do —
//!   plain `f64` `==`.
//! * `connectStation` calls `mgr.addStation` **first** (the station gets its
//!   id before any graph work), then `connectToExistingRails`, and only on a
//!   `false` return `connectToNearbyStations`.
//! * `connectToExistingRails` consults **only** `from.getCluster()` for the
//!   snap adoption (`to`'s cluster is never consulted); the split rails keep
//!   the original orientation (`slice(0, closest)` from `from`, `slice(closest)`
//!   to `to` — the segments are NOT reoriented); `nextId` is consumed twice
//!   per snap (From first, then To) while the grid registers **To before
//!   From** and `station.addRailroad` runs From-before-To.
//! * `getClosestTileIndex` returns `-1` for an empty rail, which passes the
//!   `closestRailIndex === 0 || >= rail.tiles.length` guard (both slices come
//!   out empty) — unreachable through `query` in practice (an empty-tile rail
//!   never enters `railToCells`), but the slice math is faithful.
//! * `recomputeClusters` iterates `dirtyClusters`, copies each cluster's
//!   stations into a fresh `Set` and repeatedly reads `values().next().value`
//!   of the **shrinking** copy; the first BFS group keeps the original
//!   cluster object (no new cluster is made while `allOriginalStations` still
//!   holds it), later disconnected groups move to fresh clusters. `dirtyClusters.clear()`
//!   runs even for clusters whose stations emptied mid-loop.
//! * `removeStation`'s `disconnectFromNetwork` iterates the live
//!   `station.getRailroads()` set while `rail.delete(game)` removes the
//!   **current** element — JS `Set` iteration still visits every element
//!   (snapshot + `has`, same as `Cluster.merge`). The cluster pointer
//!   survives `mgr.removeStation` and `Cluster.removeStation` (which does not
//!   clear it); an emptied cluster is `deleteCluster`d (live-iterating
//!   `setCluster(null)` again visits all) and removed from `dirtyClusters`.
//! * `distanceFrom` dequeues (`shift`) **before** the `visited.has` check and
//!   checks `neighbor === dest` before the `visited` push guard;
//!   `computeCluster` is the same BFS without the destination test (visited
//!   `Set` insertion order = BFS visit order).
//! * `connectToNearbyStations` computes `distanceFrom` **before** the
//!   `neighborCluster === null` continue (so a null-cluster neighbour still
//!   pays the BFS), and `connectionAvailable = dist > 4 || dist === -1`
//!   short-circuits the `trainStationMinRange()` config read (traced 26) —
//!   the `** 2` is V8's integer-exponent path (`x * x`, so `NaN ** 2` is
//!   `NaN` and every `dist <= NaN` guard is false).
//! * `computeGhostRailPaths`: the `paths.length >= 5` break runs **before**
//!   the `distSquared <= minRangeSquared` continue; the `!neighborStation &&
//!   unit.type() === City` direction reversal only reads `type()` when the
//!   neighbour is not a station; `neighborStation` is pushed to
//!   `connectedStations` only when a path was accepted; `some()` short-circuits
//!   at the first reachable connected station.
//! * `overlappingRailroads` / `computeGhostRailPaths` guard on the
//!   `[City, Port, Factory]` includes list (string `===` — the **includes
//!   order** is City, Port, Factory while both `nearbyUnits` call sites pass
//!   `[City, Factory, Port]`).
//! * `Array.prototype.sort` is stable in V8 and a `NaN` comparator result is
//!   treated as `+0` (Equal) — [`js_sort_by_diff`] replicates both.
//! * `createRailNetwork(game)` builds `StationManagerImpl` → pathService →
//!   `RailNetworkImpl`; the factory op (kind 11) resets the manager / grid /
//!   nextId / dirty state (fresh network objects) while the scripted tables
//!   and station objects survive, exactly like the capture.
//! * `mg.x(t)` is `t % width` (`-0 % 16` stays `-0`, `NaN % 16` is `NaN`) and
//!   `mg.y(t)` is `(t / width) | 0` (`ToUint32`-style `ToInt32`: `NaN`/`±Inf`
//!   → `0`) — the capture's width is the scripted `w`.

use std::collections::{HashMap, VecDeque};

use crate::jsnum::to_int32;

/// SameValueZero key for a refid (`+0` / `-0` collapse, `NaN` keyed by bits).
fn svz_key(v: f64) -> u64 {
    if v == 0.0 {
        0.0f64.to_bits()
    } else {
        v.to_bits()
    }
}

/// A JS `Set` (refids) with insertion-order iteration: `add` of a present
/// member is a no-op that does not move it, `delete` keeps the survivors'
/// relative order, and a re-`add` after `delete` goes to the tail.
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
    fn has(&self, v: f64) -> bool {
        self.idx.contains_key(&svz_key(v))
    }
    fn clear(&mut self) {
        self.vals.clear();
        self.idx.clear();
    }
    fn is_empty(&self) -> bool {
        self.vals.is_empty()
    }
    fn len(&self) -> usize {
        self.vals.len()
    }
    fn iter(&self) -> impl Iterator<Item = f64> + '_ {
        self.vals.iter().copied()
    }
}

/// A JS `Map` keyed by refid, preserving key insertion order (`set` on an
/// existing key overwrites in place, a new key appends, `delete` removes).
#[derive(Debug, Default, Clone)]
struct NumMap<V> {
    entries: Vec<(f64, V)>,
}

impl<V> NumMap<V> {
    fn get(&self, k: f64) -> Option<&V> {
        let kk = svz_key(k);
        self.entries.iter().find(|(x, _)| svz_key(*x) == kk).map(|(_, v)| v)
    }
    fn set(&mut self, k: f64, v: V) {
        let kk = svz_key(k);
        if let Some(slot) = self.entries.iter_mut().find(|(x, _)| svz_key(*x) == kk) {
            slot.1 = v;
        } else {
            self.entries.push((k, v));
        }
    }
    fn delete(&mut self, k: f64) {
        let kk = svz_key(k);
        if let Some(pos) = self.entries.iter().position(|(x, _)| svz_key(*x) == kk) {
            self.entries.remove(pos);
        }
    }
    fn clear(&mut self) {
        self.entries.clear();
    }
}

/// `String(Number)` for the grid-key domain: integer-valued `f64` or
/// `NaN` / `±Infinity` / `-0` (collapses to `"0"`).
fn js_int_str(v: f64) -> String {
    if v.is_nan() {
        "NaN".to_string()
    } else if v == f64::INFINITY {
        "Infinity".to_string()
    } else if v == f64::NEG_INFINITY {
        "-Infinity".to_string()
    } else if v == 0.0 {
        "0".to_string()
    } else if v.abs() < 1e21 {
        format!("{v}")
    } else {
        format!("{v:e}")
    }
}

/// A JS `Set<string>`.
#[derive(Debug, Default, Clone)]
struct StrSet {
    vals: Vec<String>,
}

impl StrSet {
    fn has(&self, k: &str) -> bool {
        self.vals.iter().any(|s| s == k)
    }
    fn add(&mut self, k: String) {
        if !self.has(&k) {
            self.vals.push(k);
        }
    }
    fn is_empty(&self) -> bool {
        self.vals.is_empty()
    }
    fn iter(&self) -> impl Iterator<Item = &String> {
        self.vals.iter()
    }
}

/// A JS `Map<string, V>` with insertion-order keys.
#[derive(Debug, Default, Clone)]
struct StrMap<V> {
    entries: Vec<(String, V)>,
}

impl<V> StrMap<V> {
    fn get(&self, k: &str) -> Option<&V> {
        self.entries.iter().find(|(x, _)| x == k).map(|(_, v)| v)
    }
    fn get_mut(&mut self, k: &str) -> Option<&mut V> {
        self.entries
            .iter_mut()
            .find(|(x, _)| x == k)
            .map(|(_, v)| v)
    }
    fn has(&self, k: &str) -> bool {
        self.entries.iter().any(|(x, _)| x == k)
    }
    fn set(&mut self, k: String, v: V) {
        if let Some(slot) = self.entries.iter_mut().find(|(x, _)| x == &k) {
            slot.1 = v;
        } else {
            self.entries.push((k, v));
        }
    }
    fn delete(&mut self, k: &str) {
        if let Some(pos) = self.entries.iter().position(|(x, _)| x == k) {
            self.entries.remove(pos);
        }
    }
}

/// V8 `Array.prototype.sort` with the `(a, b) => a - b` comparator: stable,
/// and a `NaN` comparator result is treated as `+0` (Equal).
fn js_sort_by_diff(v: &mut [f64]) {
    v.sort_by(|a, b| {
        let d = *a - *b;
        if d.is_nan() || d == 0.0 {
            std::cmp::Ordering::Equal
        } else if d < 0.0 {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        }
    });
}

/// Stable sort of `(uref, distSquared)` neighbour pairs by `distSquared` with
/// the same NaN-compares-Equal semantics.
fn js_sort_neighbors(v: &mut [(f64, f64)]) {
    v.sort_by(|a, b| {
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

/// `Array.prototype.slice(0, end)` for the integer-ish end the snap uses:
/// `end < 0` clamps to `len + end` (never negative in the reachable domain
/// except the empty-rail `-1`, which yields `[]`).
fn js_slice_head(a: &[f64], end: f64) -> Vec<f64> {
    let len = a.len() as f64;
    let e = if end < 0.0 { (len + end).max(0.0) } else { end.min(len) };
    a[..e as usize].to_vec()
}

/// `Array.prototype.slice(start)`.
fn js_slice_from(a: &[f64], start: f64) -> Vec<f64> {
    let len = a.len() as f64;
    let s = if start < 0.0 { (len + start).max(0.0) } else { start.min(len) };
    a[s as usize..].to_vec()
}

const CITY: &[u16] = &[67, 105, 116, 121]; // "City"
const PORT: &[u16] = &[80, 111, 114, 116]; // "Port"
const FACTORY: &[u16] = &[70, 97, 99, 116, 111, 114, 121]; // "Factory"

fn type_str(t: &[u16]) -> String {
    t.iter().map(|&c| char::from_u32(c as u32).unwrap_or('\u{fffd}')).collect()
}

/// The scripted `Unit` mock: fixed `type` / `tile` answers, each read traced.
#[derive(Clone, Debug)]
struct UnitSpec {
    utype: Vec<u16>,
    tile: f64,
}

/// The `Railroad` record (full field set: the network reads `tiles` / `id`).
#[derive(Clone, Debug)]
struct RailRec {
    from: f64,
    to: f64,
    tiles: Vec<f64>,
    id: f64,
}

/// `TrainStation` minus the excluded stop-handler surface. `cluster` uses
/// `0.0` as the JS `null` sentinel (internal cluster refs start at 1). The
/// `utype` / `tile` snapshot mirrors the unit object bound at construction.
#[derive(Debug)]
struct Station {
    id: f64,
    unit: f64,
    utype: Vec<u16>,
    tile: f64,
    railroads: RefSet,
    railroad_by_neighbor: NumMap<f64>,
    cluster: f64,
}

/// `Cluster`: the station set plus the trade-station subset (the subset is
/// maintained for fidelity; the network never reads it).
#[derive(Debug, Default)]
struct ClusterData {
    stations: RefSet,
    trade_stations: RefSet,
}

/// The capture harness: the scripted facade tables plus the whole network
/// graph (manager, stations, rails, clusters, spatial grid).
#[derive(Debug, Default)]
pub struct RigHarness {
    // scripted facade tables
    tp: HashMap<String, Vec<f64>>,
    sp: HashMap<String, Vec<f64>>,
    nu: HashMap<String, Vec<(f64, f64)>>,
    hn: HashMap<String, bool>,
    max_range: f64,
    min_range: f64,
    max_size: f64,
    width: f64,
    // object tables
    units: HashMap<u64, UnitSpec>,
    stations: HashMap<u64, Station>,
    rails: HashMap<u64, RailRec>,
    clusters: HashMap<u64, ClusterData>,
    rail_seq: f64,
    cluster_seq: f64,
    // StationManagerImpl
    mgr_stations: RefSet,
    mgr_by_id: Vec<Option<f64>>,
    mgr_next_id: f64,
    // RailSpatialGrid
    cells: StrMap<RefSet>,
    rail_to_cells: NumMap<StrSet>,
    // RailNetworkImpl
    next_id: f64,
    dirty: RefSet,
}

impl RigHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    // ---- scripted facade probes (each pushes its trace event) ----

    fn game_x(&self, t: f64, trace: &mut Vec<f64>) -> f64 {
        let v = t % self.width;
        trace.push(20.0);
        trace.push(t);
        trace.push(v);
        v
    }

    fn game_y(&self, t: f64, trace: &mut Vec<f64>) -> f64 {
        let v = to_int32(t / self.width) as f64;
        trace.push(21.0);
        trace.push(t);
        trace.push(v);
        v
    }

    fn add_construction(&self, id: f64, tiles: &[f64], trace: &mut Vec<f64>) {
        trace.push(22.0);
        trace.push(17.0);
        trace.push(id);
        trace.push(tiles.len() as f64);
        trace.extend(tiles.iter().copied());
    }

    fn add_destruction(&self, id: f64, trace: &mut Vec<f64>) {
        trace.push(23.0);
        trace.push(16.0);
        trace.push(id);
    }

    fn add_snap(
        &self,
        original_id: f64,
        new_id1: f64,
        new_id2: f64,
        tiles1: &[f64],
        tiles2: &[f64],
        trace: &mut Vec<f64>,
    ) {
        trace.push(24.0);
        trace.push(18.0);
        trace.push(original_id);
        trace.push(new_id1);
        trace.push(new_id2);
        trace.push(tiles1.len() as f64);
        trace.extend(tiles1.iter().copied());
        trace.push(tiles2.len() as f64);
        trace.extend(tiles2.iter().copied());
    }

    fn cfg_max_range(&self, trace: &mut Vec<f64>) -> f64 {
        trace.push(25.0);
        trace.push(self.max_range);
        self.max_range
    }

    fn cfg_min_range(&self, trace: &mut Vec<f64>) -> f64 {
        trace.push(26.0);
        trace.push(self.min_range);
        self.min_range
    }

    fn cfg_max_size(&self, trace: &mut Vec<f64>) -> f64 {
        trace.push(27.0);
        trace.push(self.max_size);
        self.max_size
    }

    fn has_unit_nearby(&self, t: f64, r: f64, ty: &[u16], trace: &mut Vec<f64>) -> bool {
        let key = format!("{}|{}|{}", js_int_str(t), js_int_str(r), type_str(ty));
        let v = self.hn.get(&key).copied().unwrap_or(false);
        trace.push(28.0);
        trace.push(t);
        trace.push(r);
        trace.push(ty.len() as f64);
        trace.extend(ty.iter().map(|&c| f64::from(c)));
        trace.push(if v { 1.0 } else { 0.0 });
        v
    }

    /// `nearbyUnits(tile, range, [City, Factory, Port])` — the type list is
    /// always the call-site order (City, Factory, Port).
    fn nearby_units(&self, t: f64, r: f64, trace: &mut Vec<f64>) -> Vec<(f64, f64)> {
        let key = format!("{}|{}", js_int_str(t), js_int_str(r));
        let list = self.nu.get(&key).cloned().unwrap_or_default();
        trace.push(29.0);
        trace.push(t);
        trace.push(r);
        trace.push(3.0);
        for ty in [CITY, FACTORY, PORT] {
            trace.push(ty.len() as f64);
            trace.extend(ty.iter().map(|&c| f64::from(c)));
        }
        trace.push(list.len() as f64);
        for (u, d) in list.iter() {
            trace.push(*u);
            trace.push(*d);
        }
        list
    }

    fn find_tile_path(&self, a: f64, b: f64, trace: &mut Vec<f64>) -> Vec<f64> {
        let key = format!("{}|{}", js_int_str(a), js_int_str(b));
        let p = self.tp.get(&key).cloned().unwrap_or_default();
        trace.push(30.0);
        trace.push(a);
        trace.push(b);
        trace.push(p.len() as f64);
        trace.extend(p.iter().copied());
        p
    }

    fn find_stations_path(&self, a: f64, b: f64, trace: &mut Vec<f64>) -> Vec<f64> {
        // Faithful to the capture mock: `spTable` is keyed by
        // `` `${a}|${b}` `` with *numeric* table entries, but the calls pass
        // station objects, so the lookup stringifies to
        // `"[object Object]|[object Object]"` and never hits — the service
        // always returns an empty path (the call itself is still traced).
        trace.push(31.0);
        trace.push(a);
        trace.push(b);
        trace.push(0.0);
        Vec::new()
    }

    fn map_unit_type(&self, uref: f64, trace: &mut Vec<f64>) -> Vec<u16> {
        let u = self.units.get(&svz_key(uref)).expect("rn: unknown unit");
        trace.push(33.0);
        trace.push(uref);
        trace.push(u.utype.len() as f64);
        trace.extend(u.utype.iter().map(|&c| f64::from(c)));
        u.utype.clone()
    }

    fn map_unit_tile(&self, uref: f64, trace: &mut Vec<f64>) -> f64 {
        let t = self.units.get(&svz_key(uref)).expect("rn: unknown unit").tile;
        trace.push(34.0);
        trace.push(uref);
        trace.push(t);
        t
    }

    fn set_train_station(&self, uref: f64, v: bool, trace: &mut Vec<f64>) {
        trace.push(32.0);
        trace.push(uref);
        trace.push(if v { 1.0 } else { 0.0 });
    }

    // ---- TrainStation surface ----

    /// `station.tile()` -> `unit.tile()` on the creation-bound unit object.
    fn station_tile(&self, sref: f64, trace: &mut Vec<f64>) -> f64 {
        let s = &self.stations[&svz_key(sref)];
        trace.push(34.0);
        trace.push(s.unit);
        trace.push(s.tile);
        s.tile
    }

    fn add_railroad(&mut self, sref: f64, railref: f64) {
        let rail = self.rails[&svz_key(railref)].clone();
        let s = self.stations.get_mut(&svz_key(sref)).expect("rn: unknown station");
        s.railroads.add(railref);
        let neighbor = if rail.from == sref { rail.to } else { rail.from };
        s.railroad_by_neighbor.set(neighbor, railref);
    }

    fn remove_railroad(&mut self, sref: f64, railref: f64) {
        let rail = self.rails[&svz_key(railref)].clone();
        let s = self.stations.get_mut(&svz_key(sref)).expect("rn: unknown station");
        s.railroads.delete(railref);
        let neighbor = if rail.from == sref { rail.to } else { rail.from };
        s.railroad_by_neighbor.delete(neighbor);
    }

    fn neighbors(&self, sref: f64) -> Vec<f64> {
        let s = &self.stations[&svz_key(sref)];
        s.railroads
            .iter()
            .map(|rr| {
                let r = &self.rails[&svz_key(rr)];
                if r.from != sref { r.from } else { r.to }
            })
            .collect()
    }

    fn set_cluster(&mut self, sref: f64, cref: f64) {
        let old = self.stations[&svz_key(sref)].cluster;
        if old != 0.0 && old != cref {
            self.cluster_remove_station(old, sref);
        }
        self.stations.get_mut(&svz_key(sref)).unwrap().cluster = cref;
    }

    // ---- Cluster surface ----

    fn new_cluster(&mut self) -> f64 {
        self.cluster_seq += 1.0;
        let cref = self.cluster_seq;
        self.clusters.insert(svz_key(cref), ClusterData::default());
        cref
    }

    fn cluster_remove_station(&mut self, cref: f64, sref: f64) {
        let c = self.clusters.get_mut(&svz_key(cref)).expect("rn: unknown cluster");
        c.stations.delete(sref);
        c.trade_stations.delete(sref);
    }

    /// `Cluster.addStation`: `unit.type()` trace (on the bound unit object),
    /// trade add for City/Port, then `station.setCluster(this)`.
    fn cluster_add_station(&mut self, cref: f64, sref: f64, trace: &mut Vec<f64>) {
        let is_trade = {
            let s = &self.stations[&svz_key(sref)];
            trace.push(33.0);
            trace.push(s.unit);
            trace.push(s.utype.len() as f64);
            trace.extend(s.utype.iter().map(|&c| f64::from(c)));
            s.utype == CITY || s.utype == PORT
        };
        let c = self.clusters.get_mut(&svz_key(cref)).expect("rn: unknown cluster");
        c.stations.add(sref);
        if is_trade {
            c.trade_stations.add(sref);
        }
        self.set_cluster(sref, cref);
    }

    // ---- StationManagerImpl surface ----

    fn mgr_add_station(&mut self, sref: f64) {
        let id = self.mgr_next_id;
        self.mgr_next_id += 1.0;
        self.stations.get_mut(&svz_key(sref)).expect("rn: unknown station").id = id;
        let need = id as usize + 1;
        if self.mgr_by_id.len() < need {
            self.mgr_by_id.resize(need, None);
        }
        self.mgr_by_id[id as usize] = Some(sref);
        self.mgr_stations.add(sref);
    }

    fn mgr_remove_station(&mut self, sref: f64) {
        let id = self.stations[&svz_key(sref)].id;
        let idx = id as usize;
        if idx < self.mgr_by_id.len() {
            self.mgr_by_id[idx] = None;
        }
        self.mgr_stations.delete(sref);
    }

    fn mgr_find_station(&self, uref: f64) -> Option<f64> {
        self.mgr_stations
            .iter()
            .find(|&r| self.stations[&svz_key(r)].unit == uref)
    }

    fn mgr_get_by_id(&self, id: f64) -> Option<f64> {
        if id.is_nan() || id < 0.0 || id.fract() != 0.0 || id > i32::MAX as f64 {
            return None;
        }
        let idx = id as usize;
        if idx < self.mgr_by_id.len() {
            self.mgr_by_id[idx]
        } else {
            None
        }
    }

    // ---- RailSpatialGrid surface ----

    fn cell_of(&self, x: f64, y: f64) -> (f64, f64) {
        ((x / 4.0).floor(), (y / 4.0).floor())
    }

    fn key(&self, cx: f64, cy: f64) -> String {
        format!("{}:{}", js_int_str(cx), js_int_str(cy))
    }

    fn grid_register(&mut self, railref: f64, trace: &mut Vec<f64>) {
        self.grid_unregister(railref);
        let tiles = self.rails[&svz_key(railref)].tiles.clone();
        let mut rail_cells = StrSet::default();
        for &tile in &tiles {
            let (cx, cy) = self.cell_of(self.game_x(tile, trace), self.game_y(tile, trace));
            let k = self.key(cx, cy);
            if rail_cells.has(&k) {
                continue;
            }
            rail_cells.add(k.clone());
            if !self.cells.has(&k) {
                self.cells.set(k.clone(), RefSet::default());
            }
            self.cells.get_mut(&k).unwrap().add(railref);
        }
        if !rail_cells.is_empty() {
            self.rail_to_cells.set(railref, rail_cells);
        }
    }

    fn grid_unregister(&mut self, railref: f64) {
        let keys: Vec<String> = match self.rail_to_cells.get(railref) {
            Some(s) => s.iter().cloned().collect(),
            None => return,
        };
        for k in keys {
            if let Some(set) = self.cells.get_mut(&k) {
                set.delete(railref);
                if set.is_empty() {
                    self.cells.delete(&k);
                }
            }
        }
        self.rail_to_cells.delete(railref);
    }

    fn grid_query(&self, tile: f64, radius: f64, trace: &mut Vec<f64>) -> Vec<f64> {
        let x = self.game_x(tile, trace);
        let y = self.game_y(tile, trace);
        let (c0x, c0y) = self.cell_of(x - radius, y - radius);
        let (c1x, c1y) = self.cell_of(x + radius, y + radius);
        let mut result = RefSet::default();
        let mut cx = c0x;
        while cx <= c1x {
            let mut cy = c0y;
            while cy <= c1y {
                if let Some(set) = self.cells.get(&self.key(cx, cy)) {
                    for r in set.iter() {
                        result.add(r);
                    }
                }
                cy += 1.0;
            }
            cx += 1.0;
        }
        result.vals
    }

    // ---- Railroad surface ----

    fn rail_delete(&mut self, railref: f64, trace: &mut Vec<f64>) {
        let r = self.rails[&svz_key(railref)].clone();
        self.add_destruction(r.id, trace);
        self.remove_railroad(r.from, railref);
        self.remove_railroad(r.to, railref);
    }

    fn get_closest_tile_index(&self, railref: f64, to: f64, trace: &mut Vec<f64>) -> f64 {
        let tiles = &self.rails[&svz_key(railref)].tiles;
        if tiles.is_empty() {
            return -1.0;
        }
        let to_x = self.game_x(to, trace);
        let to_y = self.game_y(to, trace);
        let mut best = 0usize;
        let mut min = f64::INFINITY;
        for (i, &tile) in tiles.iter().enumerate() {
            let dx = self.game_x(tile, trace) - to_x;
            let dy = self.game_y(tile, trace) - to_y;
            let d = dx * dx + dy * dy;
            if d < min {
                min = d;
                best = i;
            }
        }
        best as f64
    }

    // ---- RailNetworkImpl methods ----

    fn connect_station(&mut self, sref: f64, trace: &mut Vec<f64>) {
        self.mgr_add_station(sref);
        if !self.connect_to_existing_rails(sref, trace) {
            self.connect_to_nearby_stations(sref, trace);
        }
    }

    fn connect_to_existing_rails(&mut self, sref: f64, trace: &mut Vec<f64>) -> bool {
        let tile = self.station_tile(sref, trace);
        let rails = self.grid_query(tile, 3.0, trace);
        let mut edited = RefSet::default();
        for railref in rails {
            let rail = self.rails[&svz_key(railref)].clone();
            let (from, to, original_id) = (rail.from, rail.to, rail.id);
            let st_tile = self.station_tile(sref, trace);
            let closest = self.get_closest_tile_index(railref, st_tile, trace);
            if closest == 0.0 || closest >= rail.tiles.len() as f64 {
                continue;
            }
            self.remove_railroad(from, railref);
            self.remove_railroad(to, railref);
            self.grid_unregister(railref);

            let seg1 = js_slice_head(&rail.tiles, closest);
            let seg2 = js_slice_from(&rail.tiles, closest);
            let id1 = self.next_id;
            self.next_id += 1.0;
            let id2 = self.next_id;
            self.next_id += 1.0;
            self.rail_seq += 1.0;
            let rf = self.rail_seq;
            self.rails.insert(svz_key(rf), RailRec { from, to: sref, tiles: seg1.clone(), id: id1 });
            self.rail_seq += 1.0;
            let rt = self.rail_seq;
            self.rails.insert(svz_key(rt), RailRec { from: sref, to, tiles: seg2.clone(), id: id2 });

            self.add_railroad(sref, rf);
            self.add_railroad(sref, rt);
            self.add_railroad(from, rf);
            self.add_railroad(to, rt);
            self.grid_register(rt, trace);
            self.grid_register(rf, trace);

            let cluster = self.stations[&svz_key(from)].cluster;
            if cluster != 0.0 {
                self.cluster_add_station(cluster, sref, trace);
                edited.add(cluster);
            }
            self.add_snap(original_id, id1, id2, &seg1, &seg2, trace);
        }
        if edited.len() > 1 {
            let list: Vec<f64> = edited.iter().collect();
            self.merge_clusters(&list, trace);
        }
        !edited.is_empty()
    }

    fn connect_to_nearby_stations(&mut self, sref: f64, trace: &mut Vec<f64>) {
        let tile = self.station_tile(sref, trace);
        let max_range = self.cfg_max_range(trace);
        let mut neighbors = self.nearby_units(tile, max_range, trace);
        let mut edited = RefSet::default();
        // pair with the station's own unit refid for the `===` continue
        let own_unit = self.stations[&svz_key(sref)].unit;
        js_sort_neighbors(&mut neighbors);
        for (uref, dist) in neighbors {
            if uref == own_unit {
                continue;
            }
            let ns = match self.mgr_find_station(uref) {
                Some(x) => x,
                None => continue,
            };
            let distance_to_station = self.distance_from(ns, sref, 4.0);
            let neighbor_cluster = self.stations[&svz_key(ns)].cluster;
            if neighbor_cluster == 0.0 {
                continue;
            }
            let connection_available =
                distance_to_station > 4.0 || distance_to_station == -1.0;
            if connection_available {
                let min_range = self.cfg_min_range(trace);
                if dist > min_range * min_range && self.connect(sref, ns, trace) {
                    self.cluster_add_station(neighbor_cluster, sref, trace);
                    edited.add(neighbor_cluster);
                }
            }
        }
        if edited.len() > 1 {
            let list: Vec<f64> = edited.iter().collect();
            self.merge_clusters(&list, trace);
        } else if edited.is_empty() {
            let cref = self.new_cluster();
            self.cluster_add_station(cref, sref, trace);
        }
    }

    fn recompute_clusters(&mut self, trace: &mut Vec<f64>) {
        if self.dirty.is_empty() {
            return;
        }
        let dirty_snapshot: Vec<f64> = self.dirty.iter().collect();
        for cref in dirty_snapshot {
            let mut all_original = self.clusters[&svz_key(cref)].stations.clone();
            while !all_original.is_empty() {
                let next_station = all_original.vals[0];
                let connected = self.compute_cluster(next_station);
                let connected_list: Vec<f64> = connected.iter().collect();
                for s in &connected_list {
                    all_original.delete(*s);
                }
                if !all_original.is_empty() {
                    let new_cluster = self.new_cluster();
                    for s in connected_list {
                        self.cluster_add_station(new_cluster, s, trace);
                    }
                }
            }
        }
        self.dirty.clear();
    }

    fn remove_station(&mut self, uref: f64, trace: &mut Vec<f64>) {
        let sref = match self.mgr_find_station(uref) {
            Some(s) => s,
            None => return,
        };
        self.disconnect_from_network(sref, trace);
        self.mgr_remove_station(sref);
        self.set_train_station(uref, false, trace);
        let cluster = self.stations[&svz_key(sref)].cluster;
        if cluster == 0.0 {
            return;
        }
        self.cluster_remove_station(cluster, sref);
        if self.clusters[&svz_key(cluster)].stations.is_empty() {
            self.delete_cluster(cluster);
            self.dirty.delete(cluster);
            return;
        }
        self.dirty.add(cluster);
    }

    fn disconnect_from_network(&mut self, sref: f64, trace: &mut Vec<f64>) {
        let snapshot: Vec<f64> = self.stations[&svz_key(sref)].railroads.iter().collect();
        for rr in snapshot {
            if !self.stations[&svz_key(sref)].railroads.has(rr) {
                continue;
            }
            self.rail_delete(rr, trace);
            self.grid_unregister(rr);
        }
        let s = self.stations.get_mut(&svz_key(sref)).expect("rn: unknown station");
        s.railroads.clear();
        s.railroad_by_neighbor.clear();
    }

    fn delete_cluster(&mut self, cref: f64) {
        let snapshot: Vec<f64> = self.clusters[&svz_key(cref)].stations.iter().collect();
        for s in snapshot {
            if !self.clusters[&svz_key(cref)].stations.has(s) {
                continue;
            }
            self.set_cluster(s, 0.0);
        }
        let c = self.clusters.get_mut(&svz_key(cref)).expect("rn: unknown cluster");
        c.stations.clear();
        c.trade_stations.clear();
    }

    fn connect(&mut self, from: f64, to: f64, trace: &mut Vec<f64>) -> bool {
        let ft = self.station_tile(from, trace);
        let tt = self.station_tile(to, trace);
        let path = self.find_tile_path(ft, tt, trace);
        if !path.is_empty() {
            let max_size = self.cfg_max_size(trace);
            if (path.len() as f64) < max_size {
                let id = self.next_id;
                self.next_id += 1.0;
                self.rail_seq += 1.0;
                let railref = self.rail_seq;
                self.rails
                    .insert(svz_key(railref), RailRec { from, to, tiles: path.clone(), id });
                self.add_construction(id, &path, trace);
                self.add_railroad(from, railref);
                self.add_railroad(to, railref);
                self.grid_register(railref, trace);
                return true;
            }
        }
        false
    }

    fn distance_from(&self, start: f64, dest: f64, max_distance: f64) -> f64 {
        if start == dest {
            return 0.0;
        }
        let mut visited = RefSet::default();
        let mut queue: VecDeque<(f64, f64)> = VecDeque::new();
        queue.push_back((start, 0.0));
        while let Some((station, distance)) = queue.pop_front() {
            if visited.has(station) {
                continue;
            }
            visited.add(station);
            if distance >= max_distance {
                continue;
            }
            for neighbor in self.neighbors(station) {
                if neighbor == dest {
                    return distance + 1.0;
                }
                if !visited.has(neighbor) {
                    queue.push_back((neighbor, distance + 1.0));
                }
            }
        }
        -1.0
    }

    fn compute_cluster(&self, start: f64) -> RefSet {
        let mut visited = RefSet::default();
        let mut queue: VecDeque<f64> = VecDeque::new();
        queue.push_back(start);
        while let Some(current) = queue.pop_front() {
            if visited.has(current) {
                continue;
            }
            visited.add(current);
            for neighbor in self.neighbors(current) {
                if !visited.has(neighbor) {
                    queue.push_back(neighbor);
                }
            }
        }
        visited
    }

    fn merge_clusters(&mut self, list: &[f64], trace: &mut Vec<f64>) {
        let merged = self.new_cluster();
        for &cref in list {
            let snapshot: Vec<f64> = self.clusters[&svz_key(cref)].stations.iter().collect();
            for s in snapshot {
                if !self.clusters[&svz_key(cref)].stations.has(s) {
                    continue;
                }
                self.cluster_add_station(merged, s, trace);
            }
        }
    }

    fn type_guard(t: &[u16]) -> bool {
        t == CITY || t == PORT || t == FACTORY
    }

    fn overlapping_railroads(&self, utype: &[u16], tile: f64, trace: &mut Vec<f64>) -> Vec<f64> {
        if !Self::type_guard(utype) {
            return vec![];
        }
        let rails = self.grid_query(tile, 3.0, trace);
        let mut tiles = RefSet::default();
        for rr in rails {
            for &t in &self.rails[&svz_key(rr)].tiles {
                tiles.add(t);
            }
        }
        let mut out: Vec<f64> = tiles.iter().collect();
        js_sort_by_diff(&mut out);
        out
    }

    fn can_snap_to_existing_railway(&self, tile: f64, trace: &mut Vec<f64>) -> bool {
        !self.grid_query(tile, 3.0, trace).is_empty()
    }

    fn compute_ghost_rail_paths(
        &self,
        utype: &[u16],
        tile: f64,
        trace: &mut Vec<f64>,
    ) -> Vec<Vec<f64>> {
        if !Self::type_guard(utype) {
            return vec![];
        }
        if self.can_snap_to_existing_railway(tile, trace) {
            return vec![];
        }
        let max_range = self.cfg_max_range(trace);
        let min_range = self.cfg_min_range(trace);
        let min_range_squared = min_range * min_range;
        let max_path_size = self.cfg_max_size(trace);
        let building_factory = utype == FACTORY;
        if !building_factory && !self.has_unit_nearby(tile, max_range, FACTORY, trace) {
            return vec![];
        }
        let mut neighbors = self.nearby_units(tile, max_range, trace);
        js_sort_neighbors(&mut neighbors);

        let mut paths: Vec<Vec<f64>> = Vec::new();
        let mut connected_stations: Vec<f64> = Vec::new();
        for (uref, dist) in neighbors {
            if paths.len() >= 5 {
                break;
            }
            if dist <= min_range_squared {
                continue;
            }
            let neighbor_station = self.mgr_find_station(uref);
            let target_tile = match neighbor_station {
                Some(ns) => {
                    let already_reachable = connected_stations
                        .iter()
                        .any(|&s| self.distance_from(ns, s, 3.0) != -1.0);
                    if already_reachable {
                        continue;
                    }
                    self.station_tile(ns, trace)
                }
                None => {
                    if building_factory {
                        self.map_unit_tile(uref, trace)
                    } else {
                        continue;
                    }
                }
            };
            let reversed = neighbor_station.is_none() && self.map_unit_type(uref, trace) == CITY;
            let path = if reversed {
                self.find_tile_path(target_tile, tile, trace)
            } else {
                self.find_tile_path(tile, target_tile, trace)
            };
            if !path.is_empty() && (path.len() as f64) < max_path_size {
                paths.push(path);
                if let Some(ns) = neighbor_station {
                    connected_stations.push(ns);
                }
            }
        }
        paths
    }

    // ---- dump ----

    fn dump_network(&mut self) -> Vec<f64> {
        let mut ext: HashMap<u64, f64> = HashMap::new();
        let mut cref_seq = 0.0f64;
        let cref_of = |h: &mut HashMap<u64, f64>, seq: &mut f64, cref: f64| -> f64 {
            let k = svz_key(cref);
            if let Some(&e) = h.get(&k) {
                return e;
            }
            *seq += 1.0;
            h.insert(k, *seq);
            *seq
        };
        let st_refs: Vec<f64> = self.mgr_stations.iter().collect();
        let mut station_rows: Vec<Vec<f64>> = Vec::new();
        let mut seen_rails: Vec<f64> = Vec::new();
        let mut seen_clusters: Vec<f64> = Vec::new();
        for sref in &st_refs {
            let s = &self.stations[&svz_key(*sref)];
            let cl = s.cluster;
            let cref = if cl == 0.0 { 0.0 } else { cref_of(&mut ext, &mut cref_seq, cl) };
            let rails: Vec<f64> = s.railroads.iter().collect();
            let mut row = vec![*sref, s.id, cref, rails.len() as f64];
            for &rr in &rails {
                row.push(self.rails[&svz_key(rr)].id);
                if !seen_rails.iter().any(|&x| svz_key(x) == svz_key(rr)) {
                    seen_rails.push(rr);
                }
            }
            station_rows.push(row);
            if cl != 0.0 && !seen_clusters.contains(&cl) {
                seen_clusters.push(cl);
            }
        }
        let rail_rows: Vec<Vec<f64>> = seen_rails
            .iter()
            .map(|&rr| {
                let r = &self.rails[&svz_key(rr)];
                let mut row = vec![r.id, r.from, r.to, r.tiles.len() as f64];
                row.extend(r.tiles.iter().copied());
                row
            })
            .collect();
        let cluster_rows: Vec<Vec<f64>> = seen_clusters
            .iter()
            .map(|&c| {
                let cref = cref_of(&mut ext, &mut cref_seq, c);
                let ss: Vec<f64> = self.clusters[&svz_key(c)].stations.iter().collect();
                let mut row = vec![cref, ss.len() as f64];
                row.extend(ss.iter().copied());
                row
            })
            .collect();
        let dirty: Vec<f64> = self.dirty.iter().collect();
        let cell_rows: Vec<(Vec<f64>, Vec<f64>)> = self
            .cells
            .entries
            .iter()
            .map(|(key, set)| {
                let bytes: Vec<f64> = key.as_bytes().iter().map(|&b| f64::from(b)).collect();
                let rails: Vec<f64> = set.iter().map(|rr| self.rails[&svz_key(rr)].id).collect();
                (bytes, rails)
            })
            .collect();
        let rc_rows: Vec<(f64, Vec<Vec<f64>>)> = self
            .rail_to_cells
            .entries
            .iter()
            .map(|(rr, keys)| {
                let id = self.rails[&svz_key(*rr)].id;
                let ks: Vec<Vec<f64>> = keys
                    .iter()
                    .map(|k| k.as_bytes().iter().map(|&b| f64::from(b)).collect())
                    .collect();
                (id, ks)
            })
            .collect();
        let mut out: Vec<f64> = Vec::new();
        out.push(self.next_id);
        out.push(self.mgr_next_id);
        out.push(st_refs.len() as f64);
        for row in &station_rows {
            out.extend(row.iter().copied());
        }
        out.push(rail_rows.len() as f64);
        for row in &rail_rows {
            out.extend(row.iter().copied());
        }
        out.push(cluster_rows.len() as f64);
        for row in &cluster_rows {
            out.extend(row.iter().copied());
        }
        out.push(dirty.len() as f64);
        for c in &dirty {
            out.push(cref_of(&mut ext, &mut cref_seq, *c));
        }
        out.push(cell_rows.len() as f64);
        for (bytes, rails) in &cell_rows {
            // capture wraps the key as `[keyLen+1, keyLen, bytes…]`
            out.push(bytes.len() as f64 + 1.0);
            out.push(bytes.len() as f64);
            out.extend(bytes.iter().copied());
            out.push(rails.len() as f64);
            out.extend(rails.iter().copied());
        }
        out.push(rc_rows.len() as f64);
        for (id, ks) in &rc_rows {
            out.push(*id);
            out.push(ks.len() as f64);
            for kb in ks {
                out.push(kb.len() as f64 + 1.0);
                out.push(kb.len() as f64);
                out.extend(kb.iter().copied());
            }
        }
        out
    }

    // ---- op dispatch ----

    /// Run one op. Kind table (see `tools/gen_vectors.mjs`):
    /// 0 construct `[maxRange, minRange, maxSize, w, h, nTP, (a,b,m,tiles)*,
    ///   nSP, (a,b,m,srefs)*, nNU, (tile,range,m,(uref,dist)*m)*, nHN,
    ///   (tile,range,(str),0|1)*]` -> `[0]` (fresh harness + tables);
    /// 1 station `[sref, uref, (str), tile]` -> `[]` (unit mock + TrainStation);
    /// 2 connectStation `[sref]` -> `[0]`; 3 recomputeClusters -> `[0]`;
    /// 4 removeStation `[uref]` -> `[0]`;
    /// 5 overlappingRailroads `[(str), tile]` -> `[m, tiles*]`;
    /// 6 computeGhostRailPaths `[(str), tile]` -> `[p, (m, tiles*)*]`;
    /// 7 findStationsPath `[a, b]` -> `[m, srefs*]`;
    /// 8 mgrGetById `[id]` -> `[0]` | `[1, sref]`;
    /// 9 dumpNetwork -> `[nextId, mgrNextId, nSt, (sref,id,cref|0,nRail,
    ///   (railId)*)*, nRail, (railId,from,to,m,(tiles)*)*, nCl, (cref,m,
    ///   (sref)*)*, nDirty, (cref)*, nCells, (1+klen,klen,(byte)*,m,(railId)*)*,
    ///   nRC, (railId,n,((1+klen,klen,(byte)*))*)*]` (cell / railToCells keys
    ///   carry an outer length prefix wrapping the byte list, exactly like the
    ///   capture; cluster refids assigned lazily in
    ///   station-then-dirty encounter order, like the capture's `crefOf`);
    /// 10 unit `[uref, (str), tile]` -> `[]`;
    /// 11 factoryConstruct -> `[0]` (replace the network with
    ///   `createRailNetwork(mg)`: fresh manager / grid / nextId / dirty).
    /// All results ride as `[traceLen, (trace)*, payload*]`.
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut c = Cur(args, 0);
        let mut trace: Vec<f64> = Vec::new();
        let payload = self.exec_op(kind, &mut c, &mut trace);
        let mut out = Vec::with_capacity(1 + trace.len() + payload.len());
        out.push(trace.len() as f64);
        out.extend(trace.iter().copied());
        out.extend(payload);
        out
    }

    fn exec_op(&mut self, kind: u8, c: &mut Cur, trace: &mut Vec<f64>) -> Vec<f64> {
        match kind {
            0 => {
                let (max_range, min_range, max_size, width, _height) =
                    (c.f(), c.f(), c.f(), c.f(), c.f());
                let mut h = Self {
                    max_range,
                    min_range,
                    max_size,
                    width,
                    mgr_next_id: 1.0,
                    ..Self::default()
                };
                let n = c.u();
                for _ in 0..n {
                    let (a, b) = (c.f(), c.f());
                    let m = c.u();
                    let p: Vec<f64> = (0..m).map(|_| c.f()).collect();
                    h.tp.insert(format!("{}|{}", js_int_str(a), js_int_str(b)), p);
                }
                let n = c.u();
                for _ in 0..n {
                    let (a, b) = (c.f(), c.f());
                    let m = c.u();
                    let p: Vec<f64> = (0..m).map(|_| c.f()).collect();
                    h.sp.insert(format!("{}|{}", js_int_str(a), js_int_str(b)), p);
                }
                let n = c.u();
                for _ in 0..n {
                    let (t, r) = (c.f(), c.f());
                    let m = c.u();
                    let l: Vec<(f64, f64)> = (0..m).map(|_| (c.f(), c.f())).collect();
                    h.nu.insert(format!("{}|{}", js_int_str(t), js_int_str(r)), l);
                }
                let n = c.u();
                for _ in 0..n {
                    let (t, r) = (c.f(), c.f());
                    let ty = c.str();
                    let v = c.f() != 0.0;
                    h.hn
                        .insert(format!("{}|{}|{}", js_int_str(t), js_int_str(r), type_str(&ty)), v);
                }
                *self = h;
                vec![0.0]
            }
            1 => {
                let (sref, uref) = (c.f(), c.f());
                let utype = c.str();
                let tile = c.f();
                self.units.insert(svz_key(uref), UnitSpec { utype: utype.clone(), tile });
                self.stations.insert(
                    svz_key(sref),
                    Station {
                        id: -1.0,
                        unit: uref,
                        utype,
                        tile,
                        railroads: RefSet::default(),
                        railroad_by_neighbor: NumMap::default(),
                        cluster: 0.0,
                    },
                );
                vec![]
            }
            2 => {
                let sref = c.f();
                self.connect_station(sref, trace);
                vec![0.0]
            }
            3 => {
                self.recompute_clusters(trace);
                vec![0.0]
            }
            4 => {
                let uref = c.f();
                self.remove_station(uref, trace);
                vec![0.0]
            }
            5 => {
                let utype = c.str();
                let tile = c.f();
                let mut out = self.overlapping_railroads(&utype, tile, trace);
                let mut res = Vec::with_capacity(out.len() + 1);
                res.push(out.len() as f64);
                res.append(&mut out);
                res
            }
            6 => {
                let utype = c.str();
                let tile = c.f();
                let paths = self.compute_ghost_rail_paths(&utype, tile, trace);
                let mut res = Vec::new();
                res.push(paths.len() as f64);
                for p in paths {
                    res.push(p.len() as f64);
                    res.extend(p);
                }
                res
            }
            7 => {
                let (a, b) = (c.f(), c.f());
                let p = self.find_stations_path(a, b, trace);
                let mut res = Vec::with_capacity(p.len() + 1);
                res.push(p.len() as f64);
                res.extend(p);
                res
            }
            8 => {
                let id = c.f();
                match self.mgr_get_by_id(id) {
                    Some(sref) => vec![1.0, sref],
                    None => vec![0.0],
                }
            }
            9 => self.dump_network(),
            10 => {
                let uref = c.f();
                let utype = c.str();
                let tile = c.f();
                self.units.insert(svz_key(uref), UnitSpec { utype, tile });
                vec![]
            }
            11 => {
                // createRailNetwork(mg): fresh StationManagerImpl + grid +
                // nextId / dirty state; the scripted tables and the station /
                // unit objects survive.
                self.mgr_stations = RefSet::default();
                self.mgr_by_id.clear();
                self.mgr_next_id = 1.0;
                self.cells = StrMap::default();
                self.rail_to_cells = NumMap::default();
                self.next_id = 0.0;
                self.dirty = RefSet::default();
                vec![0.0]
            }
            k => unreachable!("rn harness: unknown op kind {k}"),
        }
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
    fn str(&mut self) -> Vec<u16> {
        let len = self.u();
        (0..len).map(|_| self.f() as u16).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(res: &[f64]) -> Vec<f64> {
        let tl = res[0] as usize;
        res[1 + tl..].to_vec()
    }

    type Ops = Vec<(u8, Vec<f64>)>;
    type NuSpec<'a> = (f64, f64, &'a [(f64, f64)]);

    fn construct(ops: &mut Ops, nu: &[NuSpec<'_>]) {
        let mut args = vec![10.0, 2.0, 12.0, 16.0, 16.0, 0.0, 0.0, nu.len() as f64];
        for (t, r, list) in nu {
            args.push(*t);
            args.push(*r);
            args.push(list.len() as f64);
            for (u, d) in list.iter() {
                args.push(*u);
                args.push(*d);
            }
        }
        args.push(0.0);
        ops.push((0, args));
    }

    fn station(ops: &mut Vec<(u8, Vec<f64>)>, sref: f64, uref: f64, utype: &[u16], tile: f64) {
        let mut a = vec![sref, uref, utype.len() as f64];
        a.extend(utype.iter().map(|&c| f64::from(c)));
        a.push(tile);
        ops.push((1, a));
    }

    fn play(ops: &[(u8, Vec<f64>)]) -> Vec<Vec<f64>> {
        let mut h = RigHarness::new();
        ops.iter().map(|(k, a)| h.run_op(*k, a)).collect()
    }

    #[test]
    fn fresh_network_dump_is_all_zero_counters() {
        let mut ops: Vec<(u8, Vec<f64>)> = Vec::new();
        construct(&mut ops, &[]);
        ops.push((9, vec![]));
        let out = play(&ops);
        assert_eq!(out[1][0], 0.0); // no facade calls
        // nextId 0, mgrNextId 1 (the StationManagerImpl field initialiser),
        // then the six empty counters.
        assert_eq!(
            payload(&out[1]),
            vec![0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]
        );
    }

    #[test]
    fn isolated_station_gets_fresh_cluster_and_trace_shape() {
        let mut ops: Vec<(u8, Vec<f64>)> = Vec::new();
        construct(&mut ops, &[(34.0, 10.0, &[])]);
        station(&mut ops, 1.0, 11.0, CITY, 34.0);
        ops.push((2, vec![1.0]));
        ops.push((9, vec![]));
        let out = play(&ops);
        // station_tile(3) + x/y(6) + station_tile(3) + maxRange(2) +
        // nearbyUnits(23) + cluster type(7) = 44
        assert_eq!(out[2][0], 44.0);
        assert_eq!(payload(&out[2]), vec![0.0]);
        assert_eq!(
            payload(&out[3]),
            vec![0.0, 2.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0]
        );
    }

    #[test]
    fn recompute_with_empty_dirty_is_a_pure_noop() {
        let mut ops: Vec<(u8, Vec<f64>)> = Vec::new();
        construct(&mut ops, &[]);
        ops.push((3, vec![]));
        let out = play(&ops);
        assert_eq!(out[1][0], 0.0);
        assert_eq!(payload(&out[1]), vec![0.0]);
    }

    #[test]
    fn ghost_paths_caps_at_five_before_the_minrange_check() {
        // Seven factory units (no stations), all beyond minRange^2: exactly
        // five findTilePath calls; the 6th/7th units are never touched (the
        // cap break beats the minRange continue and the tile()/type() reads).
        let mut ops: Vec<(u8, Vec<f64>)> = Vec::new();
        let mut args = vec![10.0, 2.0, 12.0, 16.0, 16.0, 7.0];
        for i in 0..7 {
            args.extend([50.0, 51.0 + i as f64, 2.0, 50.0, 51.0 + i as f64]);
        }
        args.extend([0.0, 1.0, 50.0, 10.0, 7.0]);
        for (i, uref) in (13i32..=19).enumerate() {
            let uref = uref as f64;
            args.extend([uref, 9.0 + i as f64]);
        }
        args.push(0.0);
        ops.push((0, args));
        for (uref, tile) in (13i32..=19).zip([51.0, 52.0, 53.0, 54.0, 55.0, 56.0, 57.0]) {
            let uref = uref as f64;
            let mut a = vec![uref, FACTORY.len() as f64];
            a.extend(FACTORY.iter().map(|&c| f64::from(c)));
            a.push(tile);
            ops.push((10, a));
        }
        let mut g = vec![FACTORY.len() as f64];
        g.extend(FACTORY.iter().map(|&c| f64::from(c)));
        g.push(50.0);
        ops.push((6, g));
        let out = play(&ops);
        let res = &out[8];
        // Walk the trace structurally and count event ids.
        let tl = res[0] as usize;
        let mut i = 1;
        let mut counts = [0usize; 35];
        while i < 1 + tl {
            let ev = res[i] as usize;
            counts[ev] += 1;
            i += match ev {
                20..=21 | 23 | 32 | 34 => 3,
                25..=27 => 2,
                30 => 4 + res[i + 3] as usize,
                33 => 2 + 1 + res[i + 2] as usize,
                28 => 4 + 1 + res[i + 3] as usize,
                29 => {
                    let mut n = 4;
                    for _ in 0..3 {
                        n += 1 + res[i + n] as usize;
                    }
                    n + 1 + res[i + n] as usize * 2
                }
                _ => panic!("unexpected trace event {ev}"),
            };
        }
        assert_eq!(i, 1 + tl);
        assert_eq!(counts[30], 5); // five findTilePath calls
        assert_eq!(counts[34], 5); // five unit.tile() reads (non-station targets)
        assert_eq!(counts[33], 5); // five unit.type() direction reads
        assert_eq!(counts[29], 1);
        assert_eq!(
            payload(res),
            vec![5.0, 2.0, 50.0, 51.0, 2.0, 50.0, 52.0, 2.0, 50.0, 53.0, 2.0, 50.0, 54.0, 2.0, 50.0, 55.0]
        );
    }

    #[test]
    fn snap_splits_consumes_two_ids_and_adopts_from_cluster() {
        let mut ops: Vec<(u8, Vec<f64>)> = Vec::new();
        let mut args = vec![
            10.0, 2.0, 12.0, 16.0, 16.0, //
            1.0, 38.0, 34.0, 5.0, 38.0, 37.0, 36.0, 35.0, 34.0, // tp
            0.0, // sp
            3.0, 34.0, 10.0, 0.0, 38.0, 10.0, 1.0, 11.0, 16.0, 36.0, 10.0, 0.0, // nu
            0.0, // hn
        ];
        ops.push((0, std::mem::take(&mut args)));
        station(&mut ops, 1.0, 11.0, CITY, 34.0);
        station(&mut ops, 2.0, 12.0, PORT, 38.0);
        station(&mut ops, 3.0, 13.0, FACTORY, 36.0);
        ops.push((2, vec![1.0]));
        ops.push((2, vec![2.0]));
        ops.push((2, vec![3.0]));
        ops.push((9, vec![]));
        let out = play(&ops);
        // station 2 nearby-connects to station 1 (rail id 0, nextId -> 1),
        // station 3 snaps the rail at index 2 (ids 1, 2, nextId -> 3);
        // mgrNextId 4, three stations in the manager.
        let d = payload(&out[7]);
        assert_eq!(d[0], 3.0);
        assert_eq!(d[1], 4.0);
        assert_eq!(d[2], 3.0); // three stations
    }
}
