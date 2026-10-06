//! Port of `src/client/render/frame/derive/AllianceClusters.ts` — the
//! union-find alliance clustering behind the SAM-radius pass.
//!
//! Faithfulness notes (quirk list):
//!
//! * `find` walks `while (parent.get(x) !== x)` with path HALVING: each pass
//!   mutates `parent[x] = parent[parent[x]]` then advances `x = p` (the OLD
//!   parent, not the halved one). The step-by-step mutation is observable
//!   through the dump of the parent map after a find run.
//! * `union(a, b)` sets `parent[rb] = ra` only when the roots differ.
//! * Seeding gate: `ps.smallID > 0` (a 0/negative smallID never enters the
//!   parent map).
//! * Union gate: `!ps.allies || ps.smallID <= 0` skips; per ally,
//!   `parent.has(allyID)` must hold before unioning (an ally outside the
//!   player set is ignored — never seeded, never unioned).
//! * The result map iterates `parent.keys()` in INSERTION order (seed order),
//!   and `find` during the result pass mutates the parent map in place.
//! * JS `Map` number keys are SameValueZero (`-0` collapses onto `0`, `NaN`
//!   keys by bits) — `NumMap` models that.

use crate::desync_detector::NumMap;
use crate::renderer_consts::{read_player, PlayerState};

/// `computeAllianceClusters(players)` — the ported union-find, with the
/// parent map exposed for the capture dump.
#[derive(Debug, Default)]
pub struct ClusterFinder {
    parent: NumMap<f64>,
}

impl ClusterFinder {
    /// The internal `find(x)` with path halving — the SAME step-by-step
    /// mutation as TS: `parent.set(x, parent.get(parent.get(x))!)`.
    pub fn find(&mut self, x0: f64) -> f64 {
        let mut x = x0;
        while self.parent.get(x) != Some(&x) {
            let p = *self.parent.get(x).unwrap();
            let gp = *self.parent.get(p).unwrap();
            self.parent.set(x, gp);
            x = p;
        }
        x
    }

    /// The internal `union(a, b)`.
    pub fn union(&mut self, a: f64, b: f64) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra != rb {
            self.parent.set(rb, ra);
        }
    }

    /// `computeAllianceClusters`: seed, union, then resolve every parent key
    /// in insertion order.
    pub fn compute(&mut self, players: &[PlayerState]) -> Vec<(f64, f64)> {
        self.parent = NumMap::default();
        for ps in players {
            if ps.small_id > 0.0 {
                self.parent.set(ps.small_id, ps.small_id);
            }
        }
        for ps in players {
            if ps.allies.is_empty() || ps.small_id <= 0.0 {
                continue;
            }
            for ally_id in &ps.allies {
                if self.parent.has(*ally_id) {
                    self.union(ps.small_id, *ally_id);
                }
            }
        }
        // result: parent.keys() insertion order; find mutates parent.
        let keys: Vec<f64> = self.parent.keys().collect();
        let mut result = Vec::with_capacity(keys.len());
        for id in keys {
            let root = self.find(id);
            result.push((id, root));
        }
        result
    }

    /// The parent map dump (post-compute, after the result-pass mutations):
    /// `[n, (key, value)*n]` in insertion order.
    pub fn dump_parent(&self) -> Vec<f64> {
        let mut out = vec![self.parent.len() as f64];
        for (k, v) in self.parent.iter() {
            out.push(k);
            out.push(*v);
        }
        out
    }
}

/// `run_op(kind, args)` — capture harness entry. Kind table:
/// 0 -> compute over the scripted players `[n, (PlayerState)*n]`; res is
///   `[result n, (key, root)*n]` (the result map in parent-key insertion
///   order);
/// 1 -> scripted finder session over a FRESH finder: args `[sN, (seed-sid)*sN,
///   fN, (0=find | 1=union, a, b)*fN]` (the seeds pre-fill `parent.set(sid,
///   sid)` exactly like the compute seeding loop); res `[fN, (kind, ret)*fN,
///   parentN, (key, value)*n]` — the post-session parent dump pins the
///   path-halving STEP mutations (a bare compute cannot show them: its result
///   pass re-find masks the intermediate state).
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut i = 0usize;
    match kind {
        0 => {
            let n = args[i] as usize;
            i += 1;
            let players: Vec<PlayerState> =
                (0..n).map(|_| read_player(args, &mut i)).collect();
            let mut finder = ClusterFinder::default();
            let result = finder.compute(&players);
            let mut out = vec![result.len() as f64];
            for (k, r) in &result {
                out.push(*k);
                out.push(*r);
            }
            out
        }
        1 => {
            let mut finder = ClusterFinder::default();
            let s = args[i] as usize;
            i += 1;
            for _ in 0..s {
                let sid = args[i];
                i += 1;
                finder.parent.set(sid, sid);
            }
            let f = args[i] as usize;
            i += 1;
            let mut out = Vec::new();
            let mut rets = Vec::with_capacity(f);
            for _ in 0..f {
                let k = args[i] as u8;
                i += 1;
                let a = args[i];
                i += 1;
                let b = args[i];
                i += 1;
                match k {
                    0 => {
                        let r = finder.find(a);
                        rets.push((0.0, r));
                    }
                    1 => {
                        finder.union(a, b);
                        rets.push((1.0, 0.0));
                    }
                    o => unreachable!("alliance_clusters: bad session kind {o}"),
                }
            }
            out.push(rets.len() as f64);
            for (k, r) in &rets {
                out.push(*k);
                out.push(*r);
            }
            out.extend(finder.dump_parent());
            out
        }
        k => unreachable!("alliance_clusters: unknown op kind {k}"),
    }
}
