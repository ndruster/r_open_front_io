//! Port of `src/core/pathfinding/PathFinder.ts` — the `WaterPathMemo` class
//! only. Everything else in that file (`UniversalPathFinding`,
//! `sharedWaterChain` / `buildWaterChain`, `PathFinding`, `WaterPathFinder`,
//! `tileStepperConfig`) rides on the `Game` facade and is a later port.
//!
//! `WaterPathMemo` is an LRU byte-budget memo in front of a `PathFinder`. The
//! inner finder is modelled here (and in the capture) by a scripted mock: a
//! queue of canned answers, each consumed by one `find_path` call, with the
//! call arguments recorded so cache hits (inner untouched) and array-`from`
//! passthrough are observable.
//!
//! Faithfulness notes:
//!
//! * The water-version check runs at *every* `findPath` entry, before the
//!   array-`from` branch — a version bump clears the whole cache even when the
//!   triggering query is a passthrough.
//! * Array `from` values go straight to the inner finder: no key, no cache
//!   entry, no byte accounting.
//! * The key is the f64 arithmetic `from * num_tiles + to`; JS `Map` keys use
//!   SameValueZero, so two distinct pairs colliding on one key (e.g. (1,2) and
//!   (0,12) with numTiles 10) share the entry — reproduced with a NaN-equal
//!   comparison over the f64 key.
//! * A `null` answer is cached too and costs a flat 16 bytes; a real path
//!   costs `len * 4` (the `Uint32Array.byteLength` of the stored copy).
//! * Stored paths are `Uint32Array.from(path)` — every element passes through
//!   `ToUint32`, so a cache *hit* can return different numbers than the miss
//!   that produced it (-1 → 4294967295, 2^32+1 → 1, 2.9 → 2). The miss always
//!   returns the inner finder's raw path.
//! * LRU order is JS `Map` insertion order: a hit deletes and re-inserts the
//!   entry; eviction takes the front entry until `liveBytes <= maxBytes`, and
//!   a single over-budget insert evicts itself.
//! * `liveBytes` is f64 throughout — every charge is an integer, so the
//!   running total is exact and matches JS bit-for-bit.

use crate::jsnum::to_uint32;

/// The `from` argument of a pathfinding query: a tile ref, or an array of
/// tile refs (multi-source start, forwarded to the inner finder untouched).
#[derive(Clone, Debug)]
pub enum PathFrom {
    Num(f64),
    Arr(Vec<f64>),
}

/// Scripted inner `PathFinder` mock: each call consumes the next canned
/// answer and records its arguments (number `from` as `[0, from, to]`, array
/// `from` as `[1, len, to, elems..]`) so the caller can observe whether the
/// memo actually reached the chain.
#[derive(Debug)]
pub struct ScriptedInner {
    script: Vec<Option<Vec<f64>>>,
    idx: usize,
    pub calls: Vec<Vec<f64>>,
}

impl ScriptedInner {
    pub fn new(script: Vec<Option<Vec<f64>>>) -> Self {
        Self { script, idx: 0, calls: Vec::new() }
    }

    pub fn find_path(&mut self, from: PathFrom, to: f64) -> Option<Vec<f64>> {
        match &from {
            PathFrom::Num(f) => self.calls.push(vec![0.0, *f, to]),
            PathFrom::Arr(a) => {
                let mut v = vec![1.0, a.len() as f64, to];
                v.extend(a.iter().copied());
                self.calls.push(v);
            }
        }
        let ret = self.script[self.idx].clone();
        self.idx += 1;
        ret
    }
}

/// JS `Map` key equality (SameValueZero): NaN matches NaN, -0 matches 0.
fn key_eq(a: f64, b: f64) -> bool {
    a == b || (a.is_nan() && b.is_nan())
}

/// Byte charge of a stored entry: `null` costs 16, a path costs its
/// `Uint32Array.byteLength` (len * 4).
fn stored_bytes(stored: &Option<Vec<f64>>) -> f64 {
    match stored {
        None => 16.0,
        Some(s) => (s.len() * 4) as f64,
    }
}

/// `WaterPathMemo`: LRU byte-budget memo in front of a [`ScriptedInner`].
pub struct WaterPathMemo {
    pub inner: ScriptedInner,
    num_tiles: f64,
    max_bytes: f64,
    /// Insertion-ordered cache: JS `Map<number, Uint32Array | null>`.
    paths: Vec<(f64, Option<Vec<f64>>)>,
    live_bytes: f64,
    water_version: f64,
}

impl WaterPathMemo {
    /// TS ctor: `waterVersion` initialises from the closure at construction.
    pub fn new(inner: ScriptedInner, num_tiles: f64, current_water_version: f64, max_bytes: f64) -> Self {
        Self {
            inner,
            num_tiles,
            max_bytes,
            paths: Vec::new(),
            live_bytes: 0.0,
            water_version: current_water_version,
        }
    }

    /// `get entryCount()`
    pub fn entry_count(&self) -> f64 {
        self.paths.len() as f64
    }

    /// `get byteCount()`
    pub fn byte_count(&self) -> f64 {
        self.live_bytes
    }

    /// `findPath(from, to)`. `current_wv` is what the map's
    /// `waterVersion()` closure returns right now.
    pub fn find_path(&mut self, from: PathFrom, to: f64, current_wv: f64) -> Option<Vec<f64>> {
        if current_wv != self.water_version {
            self.water_version = current_wv;
            self.paths.clear();
            self.live_bytes = 0.0;
        }
        let from = match from {
            // Array `from` bypasses the memo entirely.
            PathFrom::Arr(_) => return self.inner.find_path(from, to),
            PathFrom::Num(f) => f,
        };
        let key = from * self.num_tiles + to;
        if let Some(pos) = self.paths.iter().position(|(k, _)| key_eq(*k, key)) {
            // LRU: re-insert so the hot pairs outlive the cold ones.
            let (_, hit) = self.paths.remove(pos);
            let ret = hit.clone();
            self.paths.push((key, hit));
            return ret;
        }
        let path = self.inner.find_path(PathFrom::Num(from), to);
        let stored = path.as_ref().map(|p| p.iter().map(|&v| to_uint32(v) as f64).collect::<Vec<f64>>());
        self.live_bytes += stored_bytes(&stored);
        self.paths.push((key, stored));
        while self.live_bytes > self.max_bytes {
            let (_, oldest) = self.paths.remove(0);
            self.live_bytes -= stored_bytes(&oldest);
        }
        path
    }
}

// ---------------------------------------------------------------- vectors op
//
// A flat `f64` token runner shared by the golden replay and the wasm probe.
// No strings cross.
//
// kind table (mirrors the capture):
//   0 replay one scenario:
//     args [numTiles, maxBytes, wv0, scriptLen,
//             (retKind, len, (elem)*len)*scriptLen,      retKind 0=null, 1=array
//             opsLen, (op)*)*opsLen]
//       op 0 findPath(number from, to) -> [0, from, to]
//       op 1 findPath(array from, to)  -> [1, fromLen, to, (elem)*fromLen]
//       op 2 setWaterVersion(v)        -> [2, v]
//       op 3 read entryCount           -> [3]
//       op 4 read byteCount            -> [4]
//     res  per findPath op: [retKind, len, (elem)*len, entryCount, byteCount,
//                             nInner, (innerCall)*)
//          per setWaterVersion: nothing
//          per entryCount / byteCount read: [value]

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
    fn vec(&mut self) -> Vec<f64> {
        let len = self.u();
        (0..len).map(|_| self.f()).collect()
    }
}

fn push_find(out: &mut Vec<f64>, memo: &WaterPathMemo, path: Option<&Vec<f64>>) {
    match path {
        None => out.extend([0.0, 0.0]),
        Some(p) => {
            out.push(1.0);
            out.push(p.len() as f64);
            out.extend(p.iter().copied());
        }
    }
    out.push(memo.entry_count());
    out.push(memo.byte_count());
}

pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    let mut c = Cur(args, 0);
    if kind == 0 {
        let num_tiles = c.f();
        let max_bytes = c.f();
        let mut wv = c.f();
        let script_len = c.u();
        let script: Vec<Option<Vec<f64>>> = (0..script_len)
            .map(|_| {
                let ret_kind = c.f();
                let elems = c.vec();
                if ret_kind == 0.0 { None } else { Some(elems) }
            })
            .collect();
        let ops_len = c.u();
        let mut memo = WaterPathMemo::new(ScriptedInner::new(script), num_tiles, wv, max_bytes);
        for _ in 0..ops_len {
            match c.f() as i32 {
                0 => {
                    let from = c.f();
                    let to = c.f();
                    let path = memo.find_path(PathFrom::Num(from), to, wv);
                    push_find(&mut out, &memo, path.as_ref());
                    let calls = std::mem::take(&mut memo.inner.calls);
                    out.push(calls.len() as f64);
                    for call in calls {
                        out.extend(call);
                    }
                }
                1 => {
                    // op framing: [1, fromLen, to, (elem)*fromLen]
                    let from_len = c.u();
                    let to = c.f();
                    let from: Vec<f64> = (0..from_len).map(|_| c.f()).collect();
                    let path = memo.find_path(PathFrom::Arr(from), to, wv);
                    push_find(&mut out, &memo, path.as_ref());
                    let calls = std::mem::take(&mut memo.inner.calls);
                    out.push(calls.len() as f64);
                    for call in calls {
                        out.extend(call);
                    }
                }
                2 => wv = c.f(),
                3 => out.push(memo.entry_count()),
                4 => out.push(memo.byte_count()),
                _ => {}
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memo(script: Vec<Option<Vec<f64>>>, max_bytes: f64) -> WaterPathMemo {
        WaterPathMemo::new(ScriptedInner::new(script), 100.0, 1.0, max_bytes)
    }

    #[test]
    fn miss_then_hit_skips_inner() {
        let mut m = memo(vec![Some(vec![1.0, 2.0, 3.0])], 1e9);
        assert_eq!(m.find_path(PathFrom::Num(1.0), 2.0, 1.0), Some(vec![1.0, 2.0, 3.0]));
        assert_eq!(m.inner.calls.len(), 1);
        assert_eq!(m.entry_count(), 1.0);
        assert_eq!(m.byte_count(), 12.0);
        assert_eq!(m.find_path(PathFrom::Num(1.0), 2.0, 1.0), Some(vec![1.0, 2.0, 3.0]));
        assert_eq!(m.inner.calls.len(), 1);
    }

    #[test]
    fn null_costs_sixteen_bytes() {
        let mut m = memo(vec![None], 1e9);
        assert_eq!(m.find_path(PathFrom::Num(1.0), 2.0, 1.0), None);
        assert_eq!(m.byte_count(), 16.0);
        assert_eq!(m.find_path(PathFrom::Num(1.0), 2.0, 1.0), None);
        assert_eq!(m.inner.calls.len(), 1);
        assert_eq!(m.entry_count(), 1.0);
        assert_eq!(m.byte_count(), 16.0);
    }

    #[test]
    fn lru_reinsert_changes_eviction_order() {
        let ten = |v: f64| Some(vec![v; 10]);
        let mut m = memo(
            vec![ten(1.0), ten(2.0), Some(vec![3.0; 5]), ten(4.0), ten(5.0)],
            100.0,
        );
        m.find_path(PathFrom::Num(1.0), 2.0, 1.0); // A 40B
        m.find_path(PathFrom::Num(3.0), 4.0, 1.0); // B 40B
        m.find_path(PathFrom::Num(5.0), 6.0, 1.0); // C 20B -> full
        assert_eq!(m.byte_count(), 100.0);
        m.find_path(PathFrom::Num(1.0), 2.0, 1.0); // hit A -> A newest
        m.find_path(PathFrom::Num(7.0), 8.0, 1.0); // D 40B -> evicts B
        assert_eq!(m.entry_count(), 3.0);
        assert_eq!(m.byte_count(), 100.0);
        assert_eq!(m.find_path(PathFrom::Num(1.0), 2.0, 1.0), ten(1.0)); // A survives
        // B missed again: inner answers with the *next* script entry (5s),
        // and its own insert evicts C then D down to [A, B] = 80B.
        assert_eq!(m.find_path(PathFrom::Num(3.0), 4.0, 1.0), ten(5.0));
        assert_eq!(m.inner.calls.len(), 5);
        assert_eq!(m.entry_count(), 2.0);
        assert_eq!(m.byte_count(), 80.0);
    }

    #[test]
    fn version_change_clears_at_entry() {
        let mut m = memo(vec![Some(vec![1.0, 2.0, 3.0]), Some(vec![1.0, 2.0, 3.0]), Some(vec![9.0])], 1e9);
        m.find_path(PathFrom::Num(1.0), 2.0, 1.0);
        assert_eq!(m.entry_count(), 1.0);
        // The passthrough query still runs the entry check first.
        assert_eq!(
            m.find_path(PathFrom::Arr(vec![1.0, 2.0, 3.0]), 1.0, 2.0),
            Some(vec![1.0, 2.0, 3.0])
        );
        assert_eq!(m.entry_count(), 0.0);
        assert_eq!(m.byte_count(), 0.0);
        m.find_path(PathFrom::Num(1.0), 2.0, 2.0); // miss again under the new version
        assert_eq!(m.inner.calls.len(), 3);
    }

    #[test]
    fn array_from_passthrough_not_cached() {
        let mut m = memo(vec![Some(vec![4.0, 5.0]), None], 1e9);
        assert_eq!(
            m.find_path(PathFrom::Arr(vec![10.0, 20.0]), 7.0, 1.0),
            Some(vec![4.0, 5.0])
        );
        assert_eq!(m.entry_count(), 0.0);
        assert_eq!(m.byte_count(), 0.0);
        assert_eq!(m.inner.calls[0], vec![1.0, 2.0, 7.0, 10.0, 20.0]);
    }

    #[test]
    fn hit_returns_uint32_coerced_copy() {
        let mut m = memo(vec![Some(vec![-1.0, 4294967297.0, 2.9])], 1e9);
        // Miss returns the inner finder's raw path.
        assert_eq!(
            m.find_path(PathFrom::Num(1.0), 2.0, 1.0),
            Some(vec![-1.0, 4294967297.0, 2.9])
        );
        // Hit returns the Uint32Array copy.
        assert_eq!(
            m.find_path(PathFrom::Num(1.0), 2.0, 1.0),
            Some(vec![4294967295.0, 1.0, 2.0])
        );
        assert_eq!(m.byte_count(), 12.0);
    }

    #[test]
    fn over_budget_entry_evicts_itself_and_keys_collide() {
        let mut m = memo(vec![Some(vec![1.0; 10]), Some(vec![1.0; 10])], 10.0);
        m.find_path(PathFrom::Num(1.0), 2.0, 1.0);
        assert_eq!(m.entry_count(), 0.0);
        assert_eq!(m.byte_count(), 0.0);
        m.find_path(PathFrom::Num(1.0), 2.0, 1.0); // still a miss
        assert_eq!(m.inner.calls.len(), 2);
        // numTiles 10: (1,2) and (0,12) share key 12.
        let mut m2 = WaterPathMemo::new(
            ScriptedInner::new(vec![Some(vec![7.0])]),
            10.0,
            1.0,
            1e9,
        );
        assert_eq!(m2.find_path(PathFrom::Num(1.0), 2.0, 1.0), Some(vec![7.0]));
        assert_eq!(m2.find_path(PathFrom::Num(0.0), 12.0, 1.0), Some(vec![7.0]));
        assert_eq!(m2.inner.calls.len(), 1);
    }

    #[test]
    fn run_op_replays_scenario() {
        // wpm_hit shape: one script entry, two identical queries.
        let args = [100.0, 100000.0, 1.0, 1.0, 1.0, 3.0, 1.0, 2.0, 3.0, 2.0, 0.0, 1.0, 2.0, 0.0, 1.0, 2.0];
        let res = run_op(0, &args);
        assert_eq!(
            res,
            vec![
                1.0, 3.0, 1.0, 2.0, 3.0, 1.0, 12.0, 1.0, 0.0, 1.0, 2.0, // miss
                1.0, 3.0, 1.0, 2.0, 3.0, 1.0, 12.0, 0.0, // hit, inner untouched
            ]
        );
    }
}
