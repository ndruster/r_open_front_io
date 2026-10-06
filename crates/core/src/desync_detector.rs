//! Port of `src/server/DesyncDetector.ts`: `findOutOfSyncClients(active,
//! turnNumber)` and `class DesyncDetector` over the narrow `Client` facade
//! (precedent: `name_visibility`). The capture feeds plain client stubs
//! `{clientID, hashes: Map<turnNumber, hash>}` — the ONLY surface the TS
//! reads. `outOfSyncClients` crosses as the clientID list (the Rust side
//! identifies a Client by its id, same representation as the NVS stubs).
//!
//! Faithfulness notes:
//!
//! * `counts` is a JS `Map<number, number>`: first-seen hash order for
//!   `entries()` iteration, SameValueZero keys — [`NumMap`] replicates it.
//!   Clients that reported nothing for `turnNumber` are not counted at all.
//! * `mostCommonHash` uses `count > maxCount` (STRICT): on a tie the
//!   FIRST-inserted hash wins, the tied later one never replaces it.
//! * `outOfSyncClients` collects in `active` order, only among clients that
//!   DID report. The strict-majority replacement `outOfSync.length >
//!   Math.floor(active.length / 2)` swaps in `[...active]` — INCLUDING the
//!   clients that never reported a hash (they become out of sync too).
//!   `>` not `>=`: exactly half out of sync does NOT trigger the swap.
//! * `check` gates: `active.length <= 1` -> null (no facade call);
//!   `turnsCommitted % 10 !== 0 || turnsCommitted < 10` -> null (the `%`
//!   short-circuits first); else `turn = turnsCommitted - 10` and the tally
//!   spreads AFTER `turn` (key order turn, mostCommonHash, outOfSyncClients).
//! * `record` adds every id to `desynced` (even repeat offenders), but
//!   `notified` latches: a client is returned for notification at most once,
//!   however often it disagrees. `Set<ClientID>` insertion order is not
//!   observable here — only `size` and `has` are read — so [`StrSet`]
//!   (insertion-ordered) is used for fidelity.
//! * CHECK_INTERVAL = 10.

use crate::js_json::{push_str, push_val, read_str, JsVal};
use crate::vote_tally::StrSet;

/// SameValueZero key for a number (JS `Map` keying: `+0`/`-0` collapse,
/// `NaN` keyed by bits).
pub(crate) fn svz_key(v: f64) -> u64 {
    if v == 0.0 {
        0.0f64.to_bits()
    } else {
        v.to_bits()
    }
}

/// A JS `Map<number, V>` preserving key insertion order (`set` on an
/// existing key overwrites in place, a new key appends). Shared with
/// `match_telemetry` (the `tickCounts` map).
#[derive(Debug, Default, Clone)]
pub(crate) struct NumMap<V> {
    entries: Vec<(f64, V)>,
}

impl<V> NumMap<V> {
    pub(crate) fn has(&self, k: f64) -> bool {
        let kk = svz_key(k);
        self.entries.iter().any(|(x, _)| svz_key(*x) == kk)
    }
    pub(crate) fn get(&self, k: f64) -> Option<&V> {
        let kk = svz_key(k);
        self.entries.iter().find(|(x, _)| svz_key(*x) == kk).map(|(_, v)| v)
    }
    /// `get(k)`, inserting `default()` when absent (JS `if (!m.get(k))
    /// m.set(k, [])` bookkeeping; an EXISTING value is reused in place).
    pub(crate) fn get_mut_or_default(&mut self, k: f64) -> &mut V
    where
        V: Default,
    {
        let kk = svz_key(k);
        if !self.entries.iter().any(|(x, _)| svz_key(*x) == kk) {
            self.entries.push((k, V::default()));
        }
        &mut self.entries.iter_mut().find(|(x, _)| svz_key(*x) == kk).unwrap().1
    }
    pub(crate) fn set(&mut self, k: f64, v: V) {
        let kk = svz_key(k);
        if let Some(slot) = self.entries.iter_mut().find(|(x, _)| svz_key(*x) == kk) {
            slot.1 = v;
        } else {
            self.entries.push((k, v));
        }
    }
    pub(crate) fn delete(&mut self, k: f64) {
        let kk = svz_key(k);
        self.entries.retain(|(x, _)| svz_key(*x) != kk);
    }
    /// JS `Map#keys()` in insertion order.
    pub(crate) fn keys(&self) -> impl Iterator<Item = f64> + '_ {
        self.entries.iter().map(|(k, _)| *k)
    }
    /// JS `Map` iteration in insertion order (`(key, value)` pairs).
    pub(crate) fn iter(&self) -> impl Iterator<Item = (f64, &V)> + '_ {
        self.entries.iter().map(|(k, v)| (*k, v))
    }
    /// JS `Map#values()` in insertion order.
    pub(crate) fn values(&self) -> impl Iterator<Item = &V> {
        self.entries.iter().map(|(_, v)| v)
    }
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }
}

/// The narrow `Client` facade: only `clientID` and `hashes` are read.
#[derive(Debug, Default, Clone)]
struct ClientStub {
    id: String,
    hashes: NumMap<f64>,
}

/// `CHECK_INTERVAL`.
const CHECK_INTERVAL: f64 = 10.0;

/// The ported `DesyncDetector` (the module-level `findOutOfSyncClients`
/// lives on the harness as an inherent fn so it can resolve stubs).
#[derive(Debug, Default)]
pub struct DesyncDetector {
    clients: Vec<ClientStub>,
    desynced: StrSet,
    notified: StrSet,
}

/// `(mostCommonHash, outOfSyncClientIDs)` — the `HashTally`.
type Tally = (JsVal, Vec<String>);

impl DesyncDetector {
    fn stub(&self, id: &str) -> &ClientStub {
        self.clients
            .iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| unreachable!("dd harness: unregistered client {id}"))
    }

    /// `findOutOfSyncClients(active, turnNumber)`.
    fn find_out_of_sync(&self, active: &[String], turn: f64) -> Tally {
        // Count occurrences of each hash (JS Map insertion order).
        let mut counts: NumMap<f64> = NumMap::default();
        for id in active {
            let c = self.stub(id);
            if c.hashes.has(turn) {
                let h = *c.hashes.get(turn).expect("has -> get");
                counts.set(h, counts.get(h).copied().unwrap_or(0.0) + 1.0);
            }
        }
        // Most common hash: STRICT `>` keeps the first on a tie; `null`
        // (JS) when nobody reported (the loop never runs).
        let mut most_common: JsVal = JsVal::Null;
        let mut max_count = 0.0f64;
        for (hash, count) in counts.entries.iter().copied() {
            if count > max_count {
                most_common = JsVal::Num(hash);
                max_count = count;
            }
        }
        // Clients whose hash differs, in active order (reporters only).
        let mut out: Vec<String> = Vec::new();
        for id in active {
            let c = self.stub(id);
            if c.hashes.has(turn) {
                let h = *c.hashes.get(turn).expect("has -> get");
                if h != most_common_num(&most_common) {
                    out.push(id.clone());
                }
            }
        }
        // Strict majority out of sync -> nobody can be trusted.
        if out.len() > active.len() / 2 {
            out = active.to_vec();
        }
        (most_common, out)
    }

    /// `check(turnsCommitted, active)` -> `DesyncCheck | null`.
    fn check(&self, turns_committed: f64, active: &[String]) -> Option<(f64, Tally)> {
        if active.len() <= 1 {
            return None;
        }
        if turns_committed % CHECK_INTERVAL != 0.0 || turns_committed < CHECK_INTERVAL {
            return None;
        }
        let turn = turns_committed - CHECK_INTERVAL;
        Some((turn, self.find_out_of_sync(active, turn)))
    }

    /// `record(outOfSync)` -> the not-yet-notified clients (ids).
    fn record(&mut self, out_of_sync: &[String]) -> Vec<String> {
        let mut to_notify: Vec<String> = Vec::new();
        for id in out_of_sync {
            self.desynced.add(id.clone());
            if self.notified.has(id) {
                continue;
            }
            self.notified.add(id.clone());
            to_notify.push(id.clone());
        }
        to_notify
    }
}

/// The numeric view of the tally hash for the `!==` compare (the hash is
/// always a number when a client reported; `Null` only when nobody did).
fn most_common_num(v: &JsVal) -> f64 {
    match v {
        JsVal::Num(n) => *n,
        _ => f64::NAN,
    }
}

/// The capture harness: one `DesyncDetector` over registered client stubs,
/// replaying an op stream.
#[derive(Debug, Default)]
pub struct RigHarness {
    dd: DesyncDetector,
}

impl RigHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Run one op. Kind table (see `tools/gen_vectors.mjs`):
    /// 0 construct/reset -> `[0]`;
    /// 1 addClient `[(clientID-str), n, (turn,hash)*n]` -> `[0]`;
    /// 2 findOutOfSync `[turn, n, (id-str)*n]` -> `[val(mostCommonHash), k,
    ///   (id-str)*k]`;
    /// 3 check `[turnsCommitted, n, (id-str)*n]` -> `[0]` null | `[1, turn,
    ///   val(mostCommonHash), k, (id-str)*k]`;
    /// 4 record `[n, (id-str)*n]` -> `[k, (id-str)*k]` (toNotify);
    /// 5 count -> `[n]`;
    /// 6 isDesynced `[(id-str)]` -> `[0|1]`.
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut i = 0usize;
        match kind {
            0 => {
                self.reset();
                vec![0.0]
            }
            1 => {
                let id = read_str(args, &mut i);
                let n = args[i] as usize;
                i += 1;
                let mut hashes = NumMap::default();
                for _ in 0..n {
                    let turn = args[i];
                    let hash = args[i + 1];
                    i += 2;
                    hashes.set(turn, hash);
                }
                self.dd.clients.push(ClientStub { id, hashes });
                vec![0.0]
            }
            2 => {
                let turn = args[i];
                i += 1;
                let active = read_ids(args, &mut i);
                let (mch, out) = self.dd.find_out_of_sync(&active, turn);
                let mut res = Vec::new();
                push_val(&mut res, &mch);
                res.push(out.len() as f64);
                for id in &out {
                    push_str(&mut res, id);
                }
                res
            }
            3 => {
                let turns = args[i];
                i += 1;
                let active = read_ids(args, &mut i);
                match self.dd.check(turns, &active) {
                    None => vec![0.0],
                    Some((turn, (mch, out))) => {
                        let mut res = vec![1.0, turn];
                        push_val(&mut res, &mch);
                        res.push(out.len() as f64);
                        for id in &out {
                            push_str(&mut res, id);
                        }
                        res
                    }
                }
            }
            4 => {
                let ids = read_ids(args, &mut i);
                let out = self.dd.record(&ids);
                let mut res = vec![out.len() as f64];
                for id in &out {
                    push_str(&mut res, id);
                }
                res
            }
            5 => vec![self.dd.desynced.len() as f64],
            6 => {
                let id = read_str(args, &mut i);
                vec![if self.dd.desynced.has(&id) { 1.0 } else { 0.0 }]
            }
            k => unreachable!("desync_detector harness: unknown op kind {k}"),
        }
    }
}

fn read_ids(args: &[f64], i: &mut usize) -> Vec<String> {
    let n = args[*i] as usize;
    *i += 1;
    (0..n).map(|_| read_str(args, i)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(x: &str) -> Vec<f64> {
        let mut v = vec![x.len() as f64];
        v.extend(x.encode_utf16().map(|u| u as f64));
        v
    }

    fn add(id: &str, pairs: &[(f64, f64)]) -> Vec<f64> {
        let mut a = s(id);
        a.push(pairs.len() as f64);
        for (t, h) in pairs {
            a.push(*t);
            a.push(*h);
        }
        a
    }

    fn active(ids: &[&str]) -> Vec<f64> {
        let mut a = vec![ids.len() as f64];
        for id in ids {
            a.extend(s(id));
        }
        a
    }

    #[test]
    fn majority_and_tie_keeps_first() {
        let mut h = RigHarness::new();
        h.run_op(0, &[]);
        h.run_op(1, &add("a", &[(5.0, 100.0)]));
        h.run_op(1, &add("b", &[(5.0, 200.0)]));
        h.run_op(1, &add("c", &[(5.0, 200.0)]));
        h.run_op(1, &add("d", &[])); // no report for turn 5
        // 200 has count 2 > 1 -> mostCommon 200, a out of sync.
        let mut args = vec![5.0];
        args.extend(active(&["a", "b", "c", "d"]));
        let r = h.run_op(2, &args);
        assert_eq!(r[0], 3.0); // Num code
        assert_eq!(r[1], 200.0);
        assert_eq!(r[2], 1.0); // one out-of-sync client
        // Tie 1-1: first-inserted (100) stays mostCommon (strict >).
        h.run_op(0, &[]);
        h.run_op(1, &add("a", &[(5.0, 100.0)]));
        h.run_op(1, &add("b", &[(5.0, 200.0)]));
        let mut args2 = vec![5.0];
        args2.extend(active(&["a", "b"]));
        let r2 = h.run_op(2, &args2);
        assert_eq!(r2[1], 100.0);
        assert_eq!(r2[2], 1.0); // b out; 1 > floor(2/2)=1 is FALSE -> no swap
        let mut j = 3usize;
        assert_eq!(read_str(&r2, &mut j), "b");
    }

    #[test]
    fn strict_majority_swap_includes_reporters_only() {
        let mut h = RigHarness::new();
        h.run_op(0, &[]);
        h.run_op(1, &add("a", &[(0.0, 1.0)]));
        h.run_op(1, &add("b", &[(0.0, 2.0)]));
        h.run_op(1, &add("c", &[(0.0, 3.0)]));
        h.run_op(1, &add("d", &[])); // never reported
        let mut args = vec![0.0];
        args.extend(active(&["a", "b", "c", "d"]));
        let r = h.run_op(2, &args);
        // counts: 1,2,3 each once -> mostCommon 1 (first), out = [b, c],
        // 2 > floor(4/2)=2 false -> NO swap.
        assert_eq!(r[2], 2.0);
        // Now three disagree out of four: 2 > floor(3/2)=1 -> swap to ALL.
        let mut args3 = vec![0.0];
        args3.extend(active(&["a", "b", "c"]));
        let r3 = h.run_op(2, &args3);
        assert_eq!(r3[2], 3.0);
        let mut j = 3usize;
        assert_eq!(read_str(&r3, &mut j), "a");
        assert_eq!(read_str(&r3, &mut j), "b");
        assert_eq!(read_str(&r3, &mut j), "c");
    }

    #[test]
    fn check_gates() {
        let mut h = RigHarness::new();
        h.run_op(0, &[]);
        h.run_op(1, &add("a", &[(10.0, 7.0)]));
        h.run_op(1, &add("b", &[(10.0, 8.0)]));
        let act = active(&["a", "b"]);
        let mut one = active(&["a"]);
        let mut g1 = vec![20.0];
        g1.append(&mut one);
        assert_eq!(h.run_op(3, &g1), vec![0.0]); // <=1 client
        let mut g2 = vec![15.0];
        g2.extend(act.clone());
        assert_eq!(h.run_op(3, &g2), vec![0.0]); // not a multiple of 10
        let mut g3 = vec![0.0];
        g3.extend(act.clone());
        assert_eq!(h.run_op(3, &g3), vec![0.0]); // 0 % 10 === 0 but < 10
        let mut g4 = vec![20.0];
        g4.extend(act);
        let r = h.run_op(3, &g4);
        assert_eq!(r[0], 1.0);
        assert_eq!(r[1], 10.0); // turn = 20 - 10
    }

    #[test]
    fn record_notifies_once() {
        let mut h = RigHarness::new();
        h.run_op(0, &[]);
        let r1 = h.run_op(4, &active(&["a", "b"]));
        assert_eq!(r1[0], 2.0);
        let r2 = h.run_op(4, &active(&["a", "c"]));
        assert_eq!(r2[0], 1.0); // only c is new
        assert_eq!(h.run_op(5, &[]), vec![3.0]); // a, b, c
        assert_eq!(h.run_op(6, &s("a")), vec![1.0]);
        assert_eq!(h.run_op(6, &s("z")), vec![0.0]);
    }
}
