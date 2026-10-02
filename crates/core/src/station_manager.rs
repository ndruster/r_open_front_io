//! Port of `src/core/game/RailNetworkImpl.ts`: the `StationManagerImpl`
//! station registry (lines 24-58). `RailNetworkImpl`, `RailPathFinderService`
//! and `createRailNetwork` are **not** ported here (later phase); they are
//! never instantiated by the capture and their import surface is dropped by
//! `tools/ts_load.mjs`.
//!
//! Stations ride in as capture-assigned `refid`s (JS object identity becomes
//! `f64` equality); each station stub carries its `unit` refid (the
//! `station.unit === unit` lookup field) and its manager-assigned `id`. The
//! harness is stateful (op stream, precedent: `railroad_spatial_grid` /
//! `unit_grid`): kind 0 constructs a fresh manager, the other kinds call the
//! six methods and dump observable state.
//!
//! Faithfulness notes:
//!
//! * `count()` returns `this.nextId`, **not** `stations.size` — after three
//!   adds it is 4, and removing stations never decreases it (pinned by the
//!   `stm_count_*` scenarios).
//! * `addStation` assigns `station.id = nextId++` (the station's previous id
//!   — including the `-1` default or an id from another manager — is
//!   overwritten), then writes `stationsById[station.id]` and `stations.add`.
//!   Re-adding the same station object re-assigns a *new* id and appends a
//!   second `stationsById` slot while the `Set` add is a no-op that does not
//!   move the member (`stm_readd`).
//! * `stationsById` is a JS sparse array: `removeStation` writes `undefined`
//!   into the slot (the array length and the other holes survive), so
//!   `getById` on a removed id returns `undefined` — indistinguishable on the
//!   wire from an out-of-range or never-written id, and every read of a
//!   missing slot is `undefined` (encoded `[0]`).
//! * `findStation` iterates the `Set` in insertion order and compares
//!   `station.unit === unit` (JS `===`: `+0 === -0` true, `NaN === NaN`
//!   false); the **first** match wins (`stm_dup_units`), otherwise `null`
//!   (encoded `[0]`).
//! * `getAll` returns the live `Set` — the dump op rides the same insertion
//!   order (`delete` + re-`add` moves a station to the tail, `stm_reorder`).
//! * `getById` on a non-integer / NaN / negative id indexes the JS array as a
//!   string key that was never written -> `undefined`; the Rust model keeps a
//!   dense `Vec<Option>` of exactly the ids ever written, so any out-of-window
//!   read is `undefined` too.

use std::collections::HashMap;

/// SameValueZero key for a refid (`+0` / `-0` collapse, `NaN` keyed by bits)
/// — the `Set` / identity-table keying.
fn svz_key(v: f64) -> u64 {
    if v == 0.0 {
        0.0f64.to_bits()
    } else {
        v.to_bits()
    }
}

/// A JS `Set<TrainStation>` (refids) with insertion-order iteration: `add` of
/// a present member is a no-op that does not move it, `delete` keeps the
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

/// One station stub: its `unit` refid (identity field read by `findStation`)
/// and the manager-assigned `id` (initially the `-1` default).
#[derive(Clone, Copy, Debug)]
struct StationStub {
    unit: f64,
    id: f64,
}

/// The capture harness: one `StationManagerImpl` plus the station table the
/// capture hands to it.
#[derive(Debug)]
pub struct RigHarness {
    stations: RefSet,
    /// `stationsById`: the ids ever written, from index 0; `None` = a hole or
    /// an `undefined` slot (JS sparse array + the removeStation write).
    by_id: Vec<Option<f64>>,
    next_id: f64,
    table: HashMap<u64, StationStub>,
}

impl Default for RigHarness {
    /// `private nextId = 1` — the field initialiser, not the derived zero.
    fn default() -> Self {
        Self {
            stations: RefSet::default(),
            by_id: Vec::new(),
            next_id: 1.0,
            table: HashMap::new(),
        }
    }
}

impl RigHarness {
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop the manager state and the station table (one scenario's op stream
    /// ends; the next begins with a construct op).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// `addStation(station)`: `station.id = nextId++`,
    /// `stationsById[station.id] = station`, `stations.add(station)`.
    fn add_station(&mut self, refid: f64) -> f64 {
        let id = self.next_id;
        self.next_id += 1.0;
        let stub = self.table.get_mut(&svz_key(refid)).expect("stm: unknown station");
        stub.id = id;
        let need = id as usize + 1;
        if self.by_id.len() < need {
            self.by_id.resize(need, None);
        }
        self.by_id[id as usize] = Some(refid);
        self.stations.add(refid);
        id
    }

    /// `removeStation(station)`: `stationsById[station.id] = undefined` then
    /// `stations.delete(station)`. The station's `id` field is left as-is.
    fn remove_station(&mut self, refid: f64) {
        let id = self.table[&svz_key(refid)].id;
        let idx = id as usize;
        if idx < self.by_id.len() {
            self.by_id[idx] = None;
        }
        self.stations.delete(refid);
    }

    /// `findStation(unit)`: first set member whose `unit` field `===` the
    /// probe (JS equality), else `None` (JS `null`).
    fn find_station(&self, unit: f64) -> Option<f64> {
        self.stations
            .iter()
            .find(|&r| self.table[&svz_key(r)].unit == unit)
    }

    /// `getById(id)`: the slot, or `undefined` for a hole / removed slot /
    /// out-of-range id.
    fn get_by_id(&self, id: f64) -> Option<f64> {
        if id.is_nan() || id < 0.0 || id.fract() != 0.0 || id > i32::MAX as f64 {
            return None;
        }
        let idx = id as usize;
        if idx < self.by_id.len() {
            self.by_id[idx]
        } else {
            None
        }
    }

    /// Run one op. Kind table (see `tools/gen_vectors.mjs`):
    /// 0 construct `[0]` ok; 1 addStation `[refid, unit]` -> `[id]`;
    /// 2 removeStation `[refid]` -> `[]`; 3 findStation `[unit]` ->
    /// `[0]` null | `[1, refid]`; 4 getById `[id]` -> `[0]` undefined |
    /// `[1, refid]`; 5 count -> `[nextId]`; 6 dump getAll set ->
    /// `[n, (refid)*, (id)*]`; 7 dump stationsById -> `[len, (0|1, refid?)*]`.
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut c = Cur(args, 0);
        match kind {
            0 => {
                *self = Self::default();
                vec![0.0]
            }
            1 => {
                let (refid, unit) = (c.f(), c.f());
                self.table.insert(svz_key(refid), StationStub { unit, id: -1.0 });
                let id = self.add_station(refid);
                vec![id]
            }
            2 => {
                let refid = c.f();
                self.remove_station(refid);
                vec![]
            }
            3 => match self.find_station(c.f()) {
                Some(r) => vec![1.0, r],
                None => vec![0.0],
            },
            4 => match self.get_by_id(c.f()) {
                Some(r) => vec![1.0, r],
                None => vec![0.0],
            },
            5 => vec![self.next_id],
            6 => {
                let refs: Vec<f64> = self.stations.iter().collect();
                let mut out = Vec::with_capacity(2 * refs.len() + 1);
                out.push(refs.len() as f64);
                out.extend(refs.iter().copied());
                for r in refs {
                    out.push(self.table[&svz_key(r)].id);
                }
                out
            }
            7 => {
                let mut out = Vec::with_capacity(self.by_id.len() + 1);
                out.push(self.by_id.len() as f64);
                for slot in &self.by_id {
                    match slot {
                        Some(r) => {
                            out.push(1.0);
                            out.push(*r);
                        }
                        None => out.push(0.0),
                    }
                }
                out
            }
            k => unreachable!("stm harness: unknown op kind {k}"),
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rig(ops: &[(u8, &[f64])]) -> Vec<Vec<f64>> {
        let mut h = RigHarness::new();
        ops.iter().map(|(k, a)| h.run_op(*k, a)).collect()
    }

    #[test]
    fn count_is_next_id_not_size() {
        let out = rig(&[
            (0, &[]),
            (1, &[1.0, 10.0]),
            (1, &[2.0, 20.0]),
            (1, &[3.0, 30.0]),
            (5, &[]),
        ]);
        assert_eq!(out[4], vec![4.0]); // 3 adds -> nextId 4, not size 3
        let out = rig(&[
            (0, &[]),
            (1, &[1.0, 10.0]),
            (1, &[2.0, 20.0]),
            (1, &[3.0, 30.0]),
            (2, &[1.0]),
            (2, &[2.0]),
            (5, &[]),
            (6, &[]),
        ]);
        assert_eq!(out[6], vec![4.0]); // removals never lower nextId
        assert_eq!(out[7], vec![1.0, 3.0, 3.0]); // only station 3 left, id 3
    }

    #[test]
    fn ids_assigned_from_one() {
        let out = rig(&[(0, &[]), (1, &[7.0, 70.0]), (1, &[8.0, 80.0])]);
        assert_eq!(out[1], vec![1.0]);
        assert_eq!(out[2], vec![2.0]);
    }

    #[test]
    fn readd_new_id_set_noop() {
        let out = rig(&[
            (0, &[]),
            (1, &[1.0, 10.0]),
            (1, &[2.0, 20.0]),
            (1, &[1.0, 10.0]), // re-add the same station object
            (6, &[]),
            (7, &[]),
            (5, &[]),
        ]);
        assert_eq!(out[3], vec![3.0]); // the re-add takes a fresh id
        assert_eq!(out[4], vec![2.0, 1.0, 2.0, 3.0, 2.0]); // set order unchanged, ids 3 & 2
        assert_eq!(out[5], vec![4.0, 0.0, 1.0, 1.0, 1.0, 2.0, 1.0, 1.0]);
        // slots: 0 hole, 1 -> station refid 1 (id 1, later overwritten),
        // 2 -> refid 2, 3 -> refid 1 again (the re-add's new slot)
        assert_eq!(out[6], vec![4.0]);
    }

    #[test]
    fn remove_writes_undefined_not_hole() {
        let out = rig(&[
            (0, &[]),
            (1, &[1.0, 10.0]),
            (1, &[2.0, 20.0]),
            (2, &[1.0]),
            (4, &[1.0]),
            (4, &[2.0]),
            (4, &[9.0]),
            (7, &[]),
        ]);
        assert_eq!(out[4], vec![0.0]); // removed slot reads undefined
        assert_eq!(out[5], vec![1.0, 2.0]);
        assert_eq!(out[6], vec![0.0]); // out-of-range reads undefined
        assert_eq!(out[7], vec![3.0, 0.0, 0.0, 1.0, 2.0]); // length survives, slot 1 undefined
    }

    #[test]
    fn find_station_identity_and_first_match() {
        let out = rig(&[
            (0, &[]),
            (1, &[1.0, 10.0]),
            (1, &[2.0, 10.0]), // same unit field
            (3, &[10.0]),
            (3, &[99.0]),
            (2, &[1.0]),
            (3, &[10.0]),
        ]);
        assert_eq!(out[3], vec![1.0, 1.0]); // first match in set order
        assert_eq!(out[4], vec![0.0]); // miss -> null
        assert_eq!(out[6], vec![1.0, 2.0]); // after delete, the survivor
    }

    #[test]
    fn delete_readd_moves_to_tail() {
        let out = rig(&[
            (0, &[]),
            (1, &[1.0, 10.0]),
            (1, &[2.0, 20.0]),
            (1, &[3.0, 30.0]),
            (2, &[2.0]),
            (1, &[2.0, 20.0]),
            (6, &[]),
        ]);
        assert_eq!(out[6], vec![3.0, 1.0, 3.0, 2.0, 1.0, 3.0, 4.0]);
    }

    #[test]
    fn get_by_id_non_integral_undefined() {
        let out = rig(&[(0, &[]), (1, &[1.0, 10.0]), (4, &[-1.0]), (4, &[1.5])]);
        assert_eq!(out[2], vec![0.0]);
        assert_eq!(out[3], vec![0.0]);
    }

    #[test]
    fn find_station_nan_never_matches() {
        // JS `NaN === NaN` is false: a station whose unit is NaN and a NaN
        // probe must not match (unlike the Set's SameValueZero keying).
        let out = rig(&[(0, &[]), (1, &[1.0, f64::NAN]), (3, &[f64::NAN])]);
        assert_eq!(out[2], vec![0.0]);
    }
}
