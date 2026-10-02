//! Port of `src/core/game/TrainStation.ts`: the `TrainStation` graph node and
//! the `Cluster` station group. The stop-handler surface (`stopHandlers`,
//! `onTrainStop`, `TradeStationStopHandler`, `FactoryStopHandler`,
//! `createTrainStopHandlers`, `rel`) is **not** ported (existing exclusion
//! decision) and the ctor's `createTrainStopHandlers(new PseudoRandom(mg
//! .ticks()))` side effect is stripped by `tools/ts_load.mjs`, so
//! `TrainStation::new` must NOT replicate it.
//!
//! The `Game` (`mg`) is a mock whose only observable effect is `addUpdate`
//! (the `RailroadDestructionEvent` emitted by `removeNeighboringRails`); the
//! `Unit` facade (`type / owner / tile / isActive`) and the `Player` facade
//! (`canTrade`) are scripted mocks (precedent: `nation_utils` /
//! `shared_water_cache`): every facade call is recorded as a trace event and
//! rides in the res stream, pinning the call counts and short-circuits. The
//! harness is stateful (op stream, precedent: `unit_grid`):
//! `res = [traceLen, (trace)*, payload*]`.
//!
//! Faithfulness notes:
//!
//! * JS `Set` / `Map` iterate in insertion order with SameValueZero keys;
//!   `Set.add` of a present member does **not** move it, `delete` + re-`add`
//!   moves to the tail, `Map.set` overwrites in place. [`RefSet`] /
//!   [`NumMap`] replicate this. Object identity (`rail.from === this`,
//!   `otherPlayer === player`, `this.cluster !== cluster`) is JS `===` on
//!   refids: `NaN` never matches, `+0`/`-0` do — plain `f64` `==`.
//! * `removeNeighboringRails` spreads the rail set and takes the **first**
//!   rail whose `from` or `to` is the target station; only that one rail is
//!   removed (a second matching rail survives — `tsn_rnr_first_only`), and
//!   the `addUpdate` is emitted **before** the removal.
//! * `neighbors()` pushes `r.from !== this ? r.from : r.to` in rail-set
//!   order.
//! * `getRailroadTo` is `map.get(station) ?? null` — a stored `undefined`
//!   would also yield `null`; the captured domain stores rails only.
//! * `setCluster(c)`: disconnects from the old cluster **only** when the old
//!   cluster is non-null and different — re-setting the same cluster is a
//!   pure no-op (`clu_setcluster_same`), and `Cluster.removeStation` does
//!   **not** clear the station's cluster pointer (the station keeps pointing
//!   at the cluster it was removed from).
//! * `Cluster.addStation` re-reads `unit.type()` on every call (even for an
//!   already-member station, where both set adds are no-ops) — the trace
//!   pins the count (`clu_readd_type_count`).
//! * `merge(other)` iterates `other.stations` while `addStation` ->
//!   `setCluster` -> `other.removeStation` deletes the **current** element
//!   from the set being iterated: JS `Set` iteration visits every element
//!   when only the current one is deleted (verified against the real TS in
//!   the capture), but an element deleted *before* being reached would be
//!   skipped — the replay iterates a snapshot and skips members no longer in
//!   the set, exactly the JS iterator's linked-list semantics.
//! * `tradeAvailable`: `otherPlayer === player || player.canTrade(other)` —
//!   the `===` short-circuit means `canTrade` is **not** called for the
//!   owner itself (trace count pinned, `tsn_trade_self`).
//! * `randomTradeDestination` reservoir sampling: `eligibleSeen++` then keep
//!   when `random.nextInt(0, eligibleSeen) === 0` — one `nextInt` per
//!   **eligible** station only (ineligible ones consume no draw,
//!   `clu_rtd_skip_ineligible`), over the real ported [`PseudoRandom`].
//! * `isTradeStation` compares the unit type with `===` against the
//!   `UnitType.City` / `UnitType.Port` **strings**; Factory is not a trade
//!   station.
//! * `id` defaults to `-1` (the `StationManager` assigns it later; op 16
//!   models that external write).

use std::collections::HashMap;

use crate::pseudo_random::PseudoRandom;

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
    fn iter(&self) -> impl Iterator<Item = f64> + '_ {
        self.vals.iter().copied()
    }
}

/// A JS `Map` keyed by refid, preserving key insertion order (`set` on an
/// existing key overwrites in place, a new key appends).
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

/// The scripted `Unit` mock: fixed answers, each read traced.
#[derive(Clone, Debug)]
struct UnitSpec {
    uref: f64,
    utype: Vec<u16>,
    owner: f64,
    tile: f64,
    active: bool,
}

/// The scripted `Player` mock: a `canTrade(other)` answer table.
#[derive(Debug, Default)]
struct PlayerSpec {
    trades: NumMap<bool>,
}

/// The `Railroad` record (only `from / to / id` are read by the ported
/// surface; identity is the rail refid).
#[derive(Clone, Copy, Debug)]
struct RailRec {
    from: f64,
    to: f64,
    id: f64,
}

/// `TrainStation` minus the excluded stop-handler surface. `cluster` uses
/// `0.0` as the JS `null` sentinel (cluster refs start at 1).
#[derive(Debug)]
struct Station {
    id: f64,
    unit: UnitSpec,
    railroads: RefSet,
    railroad_by_neighbor: NumMap<f64>,
    cluster: f64,
}

/// `Cluster`: the station set plus the trade-station subset.
#[derive(Debug, Default)]
struct Cluster {
    stations: RefSet,
    trade_stations: RefSet,
}

const CITY: &[u16] = &[67, 105, 116, 121]; // "City"
const PORT: &[u16] = &[80, 111, 114, 116]; // "Port"

/// The capture harness: station / cluster / player / rail tables, the
/// scenario `PseudoRandom` and the cluster refid counter.
#[derive(Debug, Default)]
pub struct RigHarness {
    stations: HashMap<u64, Station>,
    clusters: HashMap<u64, Cluster>,
    players: HashMap<u64, PlayerSpec>,
    rails: HashMap<u64, RailRec>,
    next_cluster: f64,
    rng: Option<PseudoRandom>,
}

impl RigHarness {
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop every table (one scenario's op stream ends; the next begins with
    /// a construct op).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    // ---- scripted facade probes (each pushes its trace event) ----

    fn unit_type(&self, sref: f64, trace: &mut Vec<f64>) -> Vec<u16> {
        let s = &self.stations[&svz_key(sref)];
        trace.push(10.0);
        trace.push(s.unit.uref);
        trace.push(s.unit.utype.len() as f64);
        trace.extend(s.unit.utype.iter().map(|&u| f64::from(u)));
        s.unit.utype.clone()
    }

    fn unit_owner(&self, sref: f64, trace: &mut Vec<f64>) -> f64 {
        let p = self.stations[&svz_key(sref)].unit.owner;
        trace.push(11.0);
        trace.push(self.stations[&svz_key(sref)].unit.uref);
        trace.push(p);
        p
    }

    fn unit_tile(&self, sref: f64, trace: &mut Vec<f64>) -> f64 {
        let t = self.stations[&svz_key(sref)].unit.tile;
        trace.push(12.0);
        trace.push(self.stations[&svz_key(sref)].unit.uref);
        trace.push(t);
        t
    }

    fn unit_active(&self, sref: f64, trace: &mut Vec<f64>) -> bool {
        let a = self.stations[&svz_key(sref)].unit.active;
        trace.push(13.0);
        trace.push(self.stations[&svz_key(sref)].unit.uref);
        trace.push(if a { 1.0 } else { 0.0 });
        a
    }

    fn can_trade(&self, p: f64, other: f64, trace: &mut Vec<f64>) -> bool {
        let r = *self
            .players
            .get(&svz_key(p))
            .and_then(|sp| sp.trades.get(other))
            .unwrap_or(&false);
        trace.push(14.0);
        trace.push(p);
        trace.push(other);
        trace.push(if r { 1.0 } else { 0.0 });
        r
    }

    // ---- TrainStation methods ----

    /// `tradeAvailable(otherPlayer)`: owner identity short-circuits
    /// `canTrade`.
    fn trade_available(&self, sref: f64, other: f64, trace: &mut Vec<f64>) -> bool {
        let player = self.unit_owner(sref, trace);
        if other == player {
            return true;
        }
        self.can_trade(player, other, trace)
    }

    /// `addRailroad(rail)`: set add + neighbor-map set (`from === this ? to
    /// : from`).
    fn add_railroad(&mut self, sref: f64, railref: f64) {
        let rail = self.rails[&svz_key(railref)];
        let s = self.stations.get_mut(&svz_key(sref)).expect("tsn: unknown station");
        s.railroads.add(railref);
        let neighbor = if rail.from == sref { rail.to } else { rail.from };
        s.railroad_by_neighbor.set(neighbor, railref);
    }

    /// `removeRailroad(rail)`.
    fn remove_railroad(&mut self, sref: f64, railref: f64) {
        let rail = self.rails[&svz_key(railref)];
        let s = self.stations.get_mut(&svz_key(sref)).expect("tsn: unknown station");
        s.railroads.delete(railref);
        let neighbor = if rail.from == sref { rail.to } else { rail.from };
        s.railroad_by_neighbor.delete(neighbor);
    }

    /// `setCluster(c)` (`0.0` = null): disconnect only when the old cluster
    /// is non-null and different.
    fn set_cluster(&mut self, sref: f64, cref: f64) {
        let old = self.stations[&svz_key(sref)].cluster;
        if old != 0.0 && old != cref {
            self.cluster_remove_station(old, sref);
        }
        self.stations.get_mut(&svz_key(sref)).unwrap().cluster = cref;
    }

    // ---- Cluster methods ----

    fn cluster_remove_station(&mut self, cref: f64, sref: f64) {
        let c = self.clusters.get_mut(&svz_key(cref)).expect("tsn: unknown cluster");
        c.stations.delete(sref);
        c.trade_stations.delete(sref);
    }

    /// `Cluster.addStation`: set add, trade add when `isTradeStation` (one
    /// `unit.type()` trace), then `station.setCluster(this)`.
    fn cluster_add_station(&mut self, cref: f64, sref: f64, trace: &mut Vec<f64>) {
        let is_trade = {
            let t = self.unit_type(sref, trace);
            t == CITY || t == PORT
        };
        let c = self.clusters.get_mut(&svz_key(cref)).expect("tsn: unknown cluster");
        c.stations.add(sref);
        if is_trade {
            c.trade_stations.add(sref);
        }
        self.set_cluster(sref, cref);
    }

    /// `Cluster.merge(other)`: JS live iteration over `other.stations` —
    /// snapshot order, skipping members deleted before being reached.
    fn cluster_merge(&mut self, cref: f64, other: f64, trace: &mut Vec<f64>) {
        let snapshot: Vec<f64> = self.clusters[&svz_key(other)].stations.iter().collect();
        for s in snapshot {
            let still = self.clusters[&svz_key(other)].stations.has(s);
            if still {
                self.cluster_add_station(cref, s, trace);
            }
        }
    }

    /// Run one op. Kind table (see `tools/gen_vectors.mjs`):
    /// 0 construct `[seed]` -> `[0]` (fresh harness + PseudoRandom);
    /// 1 player `[pref, n, (other, 0|1)*]` -> `[]`;
    /// 2 station `[sref, uref, (type len, u*), owner, tile, active]` -> `[]`
    ///   (ctor side effect stripped; `id` starts at -1);
    /// 3 rail `[railref, from, to, id]` -> `[]` (record the Railroad object);
    /// 4 addRailroad `[sref, railref]` -> `[]`; 5 removeRailroad -> `[]`;
    /// 6 clearRailroads `[sref]` -> `[]`;
    /// 7 removeNeighboringRails `[sref, target]` -> `[]` (addUpdate traced as
    ///   `[15, 16, railId]` before the removal; first match only);
    /// 8 neighbors `[sref]` -> `[n, refs*]`; 9 tile -> `[t]`;
    /// 10 isActive -> `[0|1]`; 11 getRailroads -> `[n, railrefs*]`;
    /// 12 getRailroadTo `[sref, other]` -> `[0]` null | `[1, railref]`;
    /// 13 setCluster `[sref, cref]` (0 = null) -> `[]`;
    /// 14 getCluster `[sref]` -> `[0]` | `[1, cref]`;
    /// 15 tradeAvailable `[sref, pref]` -> `[0|1]`;
    /// 16 getId `[sref]` -> `[id]`; 17 setId `[sref, id]` -> `[]`;
    /// 20 newCluster -> `[cref]`; 21 clusterHas `[cref, sref]` -> `[0|1]`;
    /// 22 clusterAddStation -> `[]`; 23 clusterRemoveStation -> `[]`;
    /// 24 clusterAddStations `[cref, n, (sref)*]` -> `[]`;
    /// 25 clusterMerge `[cref, other]` -> `[]`;
    /// 26 hasAnyTradeDestination `[cref, pref]` -> `[0|1]`;
    /// 27 randomTradeDestination `[cref, pref]` -> `[0]` | `[1, sref]`
    ///   (one nextInt trace per eligible station);
    /// 28 availableForTrade `[cref, pref]` -> `[n, refs*]`;
    /// 29 clusterSize `[cref]` -> `[n]`; 30 clusterClear `[cref]` -> `[]`;
    /// 31 dumpCluster `[cref]` -> `[nSt, (sref)*, nTrade, (sref)*]`;
    /// 32 dumpStation `[sref]` ->
    ///   `[id, 0|1+cref, nRails, (railref)*, nNbr, (nbr, railref)*]`.
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
                let seed = c.f();
                *self = Self { rng: Some(PseudoRandom::new(seed)), ..Self::default() };
                vec![0.0]
            }
            1 => {
                let pref = c.f();
                let n = c.u();
                let mut sp = PlayerSpec::default();
                for _ in 0..n {
                    let (other, v) = (c.f(), c.f() != 0.0);
                    sp.trades.set(other, v);
                }
                self.players.insert(svz_key(pref), sp);
                vec![]
            }
            2 => {
                let (sref, uref) = (c.f(), c.f());
                let utype = c.str();
                let (owner, tile, active) = (c.f(), c.f(), c.f() != 0.0);
                self.stations.insert(
                    svz_key(sref),
                    Station {
                        id: -1.0,
                        unit: UnitSpec { uref, utype, owner, tile, active },
                        railroads: RefSet::default(),
                        railroad_by_neighbor: NumMap::default(),
                        cluster: 0.0,
                    },
                );
                vec![]
            }
            3 => {
                let (railref, from, to, id) = (c.f(), c.f(), c.f(), c.f());
                self.rails.insert(svz_key(railref), RailRec { from, to, id });
                vec![]
            }
            4 => {
                let (sref, railref) = (c.f(), c.f());
                self.add_railroad(sref, railref);
                vec![]
            }
            5 => {
                let (sref, railref) = (c.f(), c.f());
                self.remove_railroad(sref, railref);
                vec![]
            }
            6 => {
                let s = self.stations.get_mut(&svz_key(c.f())).expect("tsn: unknown station");
                s.railroads.clear();
                s.railroad_by_neighbor.clear();
                vec![]
            }
            7 => {
                let (sref, target) = (c.f(), c.f());
                let found = self.stations[&svz_key(sref)]
                    .railroads
                    .iter()
                    .find(|&rr| {
                        let r = self.rails[&svz_key(rr)];
                        r.from == target || r.to == target
                    });
                if let Some(rr) = found {
                    let id = self.rails[&svz_key(rr)].id;
                    trace.push(15.0);
                    trace.push(crate::railroad::RAILROAD_DESTRUCTION_EVENT);
                    trace.push(id);
                    self.remove_railroad(sref, rr);
                }
                vec![]
            }
            8 => {
                let sref = c.f();
                let this = sref;
                let rails: Vec<f64> = self.stations[&svz_key(sref)].railroads.iter().collect();
                let refs: Vec<f64> = rails
                    .iter()
                    .map(|&rr| {
                        let r = self.rails[&svz_key(rr)];
                        if r.from != this { r.from } else { r.to }
                    })
                    .collect();
                let mut out = Vec::with_capacity(refs.len() + 1);
                out.push(refs.len() as f64);
                out.extend(refs);
                out
            }
            9 => vec![self.unit_tile(c.f(), trace)],
            10 => vec![if self.unit_active(c.f(), trace) { 1.0 } else { 0.0 }],
            11 => {
                let refs: Vec<f64> = self.stations[&svz_key(c.f())].railroads.iter().collect();
                let mut out = Vec::with_capacity(refs.len() + 1);
                out.push(refs.len() as f64);
                out.extend(refs);
                out
            }
            12 => {
                let (sref, other) = (c.f(), c.f());
                match self.stations[&svz_key(sref)].railroad_by_neighbor.get(other) {
                    Some(&rr) => vec![1.0, rr],
                    None => vec![0.0],
                }
            }
            13 => {
                let (sref, cref) = (c.f(), c.f());
                self.set_cluster(sref, cref);
                vec![]
            }
            14 => {
                let cl = self.stations[&svz_key(c.f())].cluster;
                if cl == 0.0 {
                    vec![0.0]
                } else {
                    vec![1.0, cl]
                }
            }
            15 => {
                let (sref, pref) = (c.f(), c.f());
                vec![if self.trade_available(sref, pref, trace) { 1.0 } else { 0.0 }]
            }
            16 => vec![self.stations[&svz_key(c.f())].id],
            17 => {
                let (sref, id) = (c.f(), c.f());
                self.stations.get_mut(&svz_key(sref)).unwrap().id = id;
                vec![]
            }
            20 => {
                self.next_cluster += 1.0;
                let cref = self.next_cluster;
                self.clusters.insert(svz_key(cref), Cluster::default());
                vec![cref]
            }
            21 => {
                let (cref, sref) = (c.f(), c.f());
                vec![if self.clusters[&svz_key(cref)].stations.has(sref) { 1.0 } else { 0.0 }]
            }
            22 => {
                let (cref, sref) = (c.f(), c.f());
                self.cluster_add_station(cref, sref, trace);
                vec![]
            }
            23 => {
                let (cref, sref) = (c.f(), c.f());
                self.cluster_remove_station(cref, sref);
                vec![]
            }
            24 => {
                let cref = c.f();
                let n = c.u();
                for _ in 0..n {
                    let sref = c.f();
                    self.cluster_add_station(cref, sref, trace);
                }
                vec![]
            }
            25 => {
                let (cref, other) = (c.f(), c.f());
                self.cluster_merge(cref, other, trace);
                vec![]
            }
            26 => {
                let (cref, pref) = (c.f(), c.f());
                let trade: Vec<f64> = self.clusters[&svz_key(cref)].trade_stations.iter().collect();
                let mut hit = false;
                for s in trade {
                    if self.trade_available(s, pref, trace) {
                        hit = true;
                        break;
                    }
                }
                vec![if hit { 1.0 } else { 0.0 }]
            }
            27 => {
                let (cref, pref) = (c.f(), c.f());
                let trade: Vec<f64> = self.clusters[&svz_key(cref)].trade_stations.iter().collect();
                let mut selected: Option<f64> = None;
                let mut eligible_seen = 0.0f64;
                for s in trade {
                    if !self.trade_available(s, pref, trace) {
                        continue;
                    }
                    eligible_seen += 1.0;
                    let rng = self.rng.as_mut().expect("tsn: rng not seeded");
                    let r = rng.next_int(0.0, eligible_seen);
                    trace.push(16.0);
                    trace.push(0.0);
                    trace.push(eligible_seen);
                    trace.push(r as f64);
                    if r == 0 {
                        selected = Some(s);
                    }
                }
                match selected {
                    Some(s) => vec![1.0, s],
                    None => vec![0.0],
                }
            }
            28 => {
                let (cref, pref) = (c.f(), c.f());
                let trade: Vec<f64> = self.clusters[&svz_key(cref)].trade_stations.iter().collect();
                let mut out: Vec<f64> = Vec::new();
                for s in trade {
                    if self.trade_available(s, pref, trace) {
                        out.push(s);
                    }
                }
                out.insert(0, out.len() as f64);
                out
            }
            29 => vec![self.clusters[&svz_key(c.f())].stations.vals.len() as f64],
            30 => {
                let c = self.clusters.get_mut(&svz_key(c.f())).expect("tsn: unknown cluster");
                c.stations.clear();
                c.trade_stations.clear();
                vec![]
            }
            31 => {
                let cref = c.f();
                let cl = &self.clusters[&svz_key(cref)];
                let st: Vec<f64> = cl.stations.iter().collect();
                let tr: Vec<f64> = cl.trade_stations.iter().collect();
                let mut out = Vec::with_capacity(st.len() + tr.len() + 2);
                out.push(st.len() as f64);
                out.extend(st);
                out.push(tr.len() as f64);
                out.extend(tr);
                out
            }
            32 => {
                let sref = c.f();
                let s = &self.stations[&svz_key(sref)];
                let rails: Vec<f64> = s.railroads.iter().collect();
                let nbrs: Vec<(f64, f64)> = s.railroad_by_neighbor.entries.iter().map(|(k, v)| (*k, *v)).collect();
                let mut out = Vec::with_capacity(rails.len() + 2 * nbrs.len() + 4);
                out.push(s.id);
                if s.cluster == 0.0 {
                    out.push(0.0);
                } else {
                    out.push(1.0);
                    out.push(s.cluster);
                }
                out.push(rails.len() as f64);
                out.extend(rails);
                out.push(nbrs.len() as f64);
                for (k, v) in nbrs {
                    out.push(k);
                    out.push(v);
                }
                out
            }
            k => unreachable!("tsn harness: unknown op kind {k}"),
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

    fn rig(ops: &[(u8, &[f64])]) -> Vec<Vec<f64>> {
        let mut h = RigHarness::new();
        ops.iter().map(|(k, a)| h.run_op(*k, a)).collect()
    }

    fn play(ops: &[(u8, Vec<f64>)]) -> Vec<Vec<f64>> {
        rig(&ops.iter().map(|(k, a)| (*k, a.as_slice())).collect::<Vec<_>>())
    }

    /// Strip the `[traceLen, trace*]` prefix, keep the payload.
    fn payload(res: &[f64]) -> Vec<f64> {
        let tl = res[0] as usize;
        res[1 + tl..].to_vec()
    }

    fn setup(ops: &mut Vec<(u8, Vec<f64>)>) {
        ops.push((0, vec![42.0]));
        // player 100 trades with 101 only; player 101 trades with 100 only.
        ops.push((1, vec![100.0, 1.0, 101.0, 1.0]));
        ops.push((1, vec![101.0, 1.0, 100.0, 1.0]));
        // station 1: City unit (uref 11) on player 100; station 2: Port
        // (uref 12) on 101; station 3: Factory (uref 13) on 100.
        ops.push((2, vec![1.0, 11.0, 4.0, 67.0, 105.0, 116.0, 121.0, 100.0, 5.0, 1.0]));
        ops.push((2, vec![2.0, 12.0, 4.0, 80.0, 111.0, 114.0, 116.0, 101.0, 6.0, 1.0]));
        ops.push((2, vec![3.0, 13.0, 7.0, 70.0, 97.0, 99.0, 116.0, 111.0, 114.0, 121.0, 100.0, 7.0, 1.0]));
    }

    #[test]
    fn id_defaults_to_minus_one() {
        let ops = vec![
            (0u8, vec![1.0f64]),
            (2, vec![1.0, 11.0, 4.0, 67.0, 105.0, 116.0, 121.0, 100.0, 5.0, 1.0]),
            (16, vec![1.0]),
        ];
        let out = play(&ops);
        assert_eq!(payload(&out[2]), vec![-1.0]);
    }

    #[test]
    fn trade_available_self_short_circuits() {
        let mut ops: Vec<(u8, Vec<f64>)> = Vec::new();
        setup(&mut ops);
        ops.push((15, vec![1.0, 100.0])); // owner asking -> no canTrade
        ops.push((15, vec![1.0, 101.0])); // other -> canTrade(100,101)=true
        ops.push((15, vec![1.0, 102.0])); // no table entry -> false
        let out = play(&ops);
        assert_eq!(payload(&out[6]), vec![1.0]);
        assert_eq!(payload(&out[7]), vec![1.0]);
        assert_eq!(payload(&out[8]), vec![0.0]);
        // trace lengths: owner event [11,uref,pref] = 3 vs owner + canTrade
        // [14,p,other,0|1] = 7. The short-circuit is pinned by the count.
        assert_eq!(out[6][0], 3.0);
        assert_eq!(out[7][0], 7.0);
        assert_eq!(out[8][0], 7.0);
    }

    #[test]
    fn rail_first_only_removal_and_neighbor_map_quirk() {
        let mut ops: Vec<(u8, Vec<f64>)> = Vec::new();
        setup(&mut ops);
        // two parallel rails 1 -> 2.
        ops.push((3, vec![500.0, 1.0, 2.0, 77.0]));
        ops.push((3, vec![501.0, 1.0, 2.0, 88.0]));
        ops.push((4, vec![1.0, 500.0])); // byNeighbor[2] = 500
        ops.push((4, vec![1.0, 501.0])); // byNeighbor[2] = 501 (overwrite)
        ops.push((4, vec![2.0, 500.0]));
        ops.push((7, vec![1.0, 2.0])); // removeNeighboringRails(2): first rail 500 only
        ops.push((11, vec![1.0])); // railroads left: [501]
        ops.push((12, vec![1.0, 2.0])); // quirk: removeRailroad deletes the
        // neighbor KEY (2), so the map entry to the surviving 501 is gone.
        let out = play(&ops);
        let tl = out[11][0] as usize;
        assert_eq!(&out[11][1..1 + tl], &[15.0, 16.0, 77.0]); // addUpdate before removal
        assert_eq!(payload(&out[12]), vec![1.0, 501.0]);
        assert_eq!(payload(&out[13]), vec![0.0]);
    }

    #[test]
    fn set_cluster_same_is_noop_and_remove_keeps_pointer() {
        let mut ops: Vec<(u8, Vec<f64>)> = Vec::new();
        setup(&mut ops);
        ops.push((20, vec![])); // cluster 1
        ops.push((22, vec![1.0, 1.0])); // c1.addStation(s1)
        ops.push((13, vec![1.0, 1.0])); // setCluster(s1, c1) again: no-op
        ops.push((23, vec![1.0, 1.0])); // c1.removeStation(s1): pointer NOT cleared
        ops.push((14, vec![1.0])); // getCluster -> [1, 1]
        ops.push((21, vec![1.0, 1.0])); // has -> 0
        let out = play(&ops);
        assert_eq!(payload(&out[6]), vec![1.0]); // cref 1
        assert_eq!(payload(&out[10]), vec![1.0, 1.0]);
        assert_eq!(payload(&out[11]), vec![0.0]);
    }

    #[test]
    fn cluster_switch_disconnects_old() {
        let mut ops: Vec<(u8, Vec<f64>)> = Vec::new();
        setup(&mut ops);
        ops.push((20, vec![])); // c1
        ops.push((20, vec![])); // c2
        ops.push((22, vec![1.0, 1.0])); // c1.addStation(s1)
        ops.push((22, vec![2.0, 1.0])); // c2.addStation(s1) -> setCluster
        // disconnects c1 (removeStation does not touch s1's pointer)
        ops.push((31, vec![1.0]));
        ops.push((31, vec![2.0]));
        let out = play(&ops);
        assert_eq!(payload(&out[10]), vec![0.0, 0.0]); // c1 emptied
        assert_eq!(payload(&out[11]), vec![1.0, 1.0, 1.0, 1.0]); // c2: [s1], trade [s1]
    }

    #[test]
    fn merge_moves_every_station() {
        let mut ops: Vec<(u8, Vec<f64>)> = Vec::new();
        setup(&mut ops);
        ops.push((20, vec![])); // c1 (this)
        ops.push((20, vec![])); // c2 (other)
        ops.push((22, vec![2.0, 1.0]));
        ops.push((22, vec![2.0, 2.0]));
        ops.push((22, vec![2.0, 3.0])); // Factory: stations only, no trade
        ops.push((25, vec![1.0, 2.0])); // c1.merge(c2)
        ops.push((31, vec![1.0]));
        ops.push((31, vec![2.0]));
        let out = play(&ops);
        assert_eq!(payload(&out[12]), vec![3.0, 1.0, 2.0, 3.0, 2.0, 1.0, 2.0]);
        assert_eq!(payload(&out[13]), vec![0.0, 0.0]); // current-delete visits all
    }

    #[test]
    fn factory_not_trade_station_and_readd_counts_type() {
        let mut ops: Vec<(u8, Vec<f64>)> = Vec::new();
        setup(&mut ops);
        ops.push((20, vec![])); // c1
        ops.push((22, vec![1.0, 3.0])); // Factory
        ops.push((22, vec![1.0, 3.0])); // re-add: type() traced again, sets no-op
        ops.push((31, vec![1.0]));
        let out = play(&ops);
        // trace: [10, uref=13, len=7, 7 units] = 10 tokens per add
        assert_eq!(out[7][0], 10.0);
        assert_eq!(out[8][0], 10.0);
        assert_eq!(payload(&out[9]), vec![1.0, 3.0, 0.0]); // stations [3], trade []
    }

    #[test]
    fn random_trade_reservoir_and_skip_ineligible() {
        let mut ops: Vec<(u8, Vec<f64>)> = Vec::new();
        setup(&mut ops);
        ops.push((20, vec![])); // c1
        ops.push((22, vec![1.0, 1.0])); // s1 City (owner 100)
        ops.push((22, vec![1.0, 2.0])); // s2 Port (owner 101)
        // player 100 can trade with 102 (s1 eligible); player 101 cannot
        // (s2 ineligible -> no nextInt draw consumed).
        ops.push((1, vec![100.0, 2.0, 101.0, 1.0, 102.0, 1.0]));
        ops.push((27, vec![1.0, 102.0]));
        let out = play(&ops);
        let tl = out[10][0] as usize;
        let next_ints: Vec<f64> = out[10][1..1 + tl]
            .windows(4)
            .filter(|w| w[0] == 16.0)
            .map(|w| w[3])
            .collect();
        assert_eq!(next_ints, vec![0.0]); // exactly one draw, nextInt(0,1)=0
        assert_eq!(payload(&out[10]), vec![1.0, 1.0]); // s1 selected
    }

    #[test]
    fn neighbors_push_rule_and_clear() {
        let mut ops: Vec<(u8, Vec<f64>)> = Vec::new();
        setup(&mut ops);
        ops.push((3, vec![500.0, 1.0, 2.0, 1.0]));
        ops.push((3, vec![501.0, 3.0, 1.0, 2.0])); // rail from 3 to 1
        ops.push((4, vec![1.0, 500.0])); // neighbor = to = 2
        ops.push((4, vec![1.0, 501.0])); // from != this -> push from = 3
        ops.push((8, vec![1.0])); // neighbors in rail order: [2, 3]
        ops.push((6, vec![1.0])); // clearRailroads
        ops.push((8, vec![1.0]));
        ops.push((32, vec![1.0])); // dump: id -1, cluster null, rails/map empty
        let out = play(&ops);
        assert_eq!(payload(&out[10]), vec![2.0, 2.0, 3.0]);
        assert_eq!(payload(&out[12]), vec![0.0]);
        assert_eq!(payload(&out[13]), vec![-1.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn has_any_trade_destination_short_circuits() {
        let mut ops: Vec<(u8, Vec<f64>)> = Vec::new();
        setup(&mut ops);
        ops.push((20, vec![])); // c1
        ops.push((22, vec![1.0, 1.0])); // s1 City (owner 100)
        ops.push((22, vec![1.0, 2.0])); // s2 Port (owner 101)
        ops.push((26, vec![1.0, 101.0])); // s1: canTrade(100,101)=true -> stop
        ops.push((26, vec![1.0, 102.0])); // both false -> scan all
        let out = play(&ops);
        assert_eq!(payload(&out[9]), vec![1.0]);
        assert_eq!(payload(&out[10]), vec![0.0]);
        // short-circuit: first op traces owner+canTrade for s1 only (7 tokens),
        // second scans s1 and s2 (14 tokens).
        assert_eq!(out[9][0], 7.0);
        assert_eq!(out[10][0], 14.0);
    }
}
