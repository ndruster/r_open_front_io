//! Port of `src/server/VoteTally.ts`: the IP-weighted single-round vote
//! (`class VoteRound<T>`). The generic `T` rides as an opaque `f64` payload
//! token the capture assigns; the harness is stateful (op stream, precedent:
//! `station_manager`): kind 0 constructs a fresh round, the other kinds call
//! the three methods and dump observable state.
//!
//! Faithfulness notes:
//!
//! * `candidates` is a JS `Map<string, {value, ips:Set<string>}>`: string
//!   keys compare with exact equality, iteration is insertion order, and a
//!   `set` on an existing key overwrites in place. [`StrSet`] / the candidate
//!   vec replicate this.
//! * `add` of an already-known IP for the same candidate is idempotent and
//!   returns the unchanged size; the same IP voting for *different* candidates
//!   counts in both (one `Set` per candidate, `vt_cross_vote`).
//! * `result` returns the **first** candidate (in insertion order) with
//!   `ips.size * 2 > totalUniqueIPs` — strict majority: 1 of 2 is a tie and
//!   yields `null` (`vt_tie_1_of_2`).
//! * `resultAmong` counts only IPs still in `activeIPs` and compares against
//!   `activeIPs.size`; votes from departed IPs never count (`vt_among_*`).

use std::collections::HashSet;

/// A JS `Set<string>` with insertion-order iteration (`add` of a present
/// member is a no-op that does not move it).
#[derive(Debug, Default, Clone)]
pub(crate) struct StrSet {
    vals: Vec<String>,
    idx: HashSet<String>,
}

impl StrSet {
    pub(crate) fn add(&mut self, v: String) {
        if self.idx.insert(v.clone()) {
            self.vals.push(v);
        }
    }
    pub(crate) fn has(&self, v: &str) -> bool {
        self.idx.contains(v)
    }
    pub(crate) fn len(&self) -> usize {
        self.vals.len()
    }
    pub(crate) fn iter(&self) -> impl Iterator<Item = &str> {
        self.vals.iter().map(|s| s.as_str())
    }
}

/// One candidate slot: the opaque value token and its unique-IP set.
#[derive(Debug, Clone)]
struct Candidate {
    value: f64,
    ips: StrSet,
}

/// The ported `VoteRound<T>` (`T` = opaque `f64` payload token).
#[derive(Debug, Default, Clone)]
pub struct VoteRound {
    candidates: Vec<(String, Candidate)>,
}

impl VoteRound {
    /// `add(key, value, ip)`: create the candidate (append) or reuse it, add
    /// the IP, return the unique-IP count after the vote.
    pub(crate) fn add(&mut self, key: &str, value: f64, ip: &str) -> usize {
        if let Some(slot) = self.candidates.iter_mut().find(|(k, _)| k == key) {
            slot.1.ips.add(ip.to_string());
            return slot.1.ips.len();
        }
        let mut ips = StrSet::default();
        ips.add(ip.to_string());
        self.candidates.push((key.to_string(), Candidate { value, ips }));
        1
    }

    /// `result(totalUniqueIPs)`: first candidate (insertion order) with a
    /// strict majority of `total`, else `None` (JS `null`).
    pub(crate) fn result(&self, total: f64) -> Option<(f64, usize)> {
        for (_, c) in &self.candidates {
            if c.ips.len() as f64 * 2.0 > total {
                return Some((c.value, c.ips.len()));
            }
        }
        None
    }

    /// `resultAmong(activeIPs)`: first candidate whose *active* votes hold a
    /// strict majority of `activeIPs.size`, else `None`.
    pub(crate) fn result_among(&self, active: &StrSet) -> Option<(f64, usize)> {
        for (_, c) in &self.candidates {
            let votes = c.ips.iter().filter(|ip| active.has(ip)).count();
            if votes as f64 * 2.0 > active.len() as f64 {
                return Some((c.value, votes));
            }
        }
        None
    }

    /// Append the `[n,(key,value,m,(ip)*)*]` candidate dump (insertion
    /// order) to a token stream; shared by the `vt_` dump op and the
    /// `cv_` LiveStatsVote round dumps.
    pub(crate) fn dump_into(&self, out: &mut Vec<f64>) {
        out.push(self.candidates.len() as f64);
        for (k, c) in &self.candidates {
            crate::js_json::push_str(out, k);
            out.push(c.value);
            out.push(c.ips.len() as f64);
            for ip in c.ips.iter() {
                crate::js_json::push_str(out, ip);
            }
        }
    }
}

/// The capture harness: one `VoteRound` replaying an op stream.
#[derive(Debug)]
pub struct RigHarness {
    round: VoteRound,
}

impl RigHarness {
    pub fn new() -> Self {
        Self { round: VoteRound::default() }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Run one op. Kind table (see `tools/gen_vectors.mjs`):
    /// 0 construct/reset -> `[0]`; 1 add `[klen,(u)*,value,ilen,(u)*]` ->
    /// `[size]`; 2 result `[total]` -> `[0]` null | `[1,value,votes]`;
    /// 3 resultAmong `[n,(ilen,(u)*)*]` -> `[0]` | `[1,value,votes]`;
    /// 4 dump candidates -> `[n,(klen,(u)*,value,m,(ilen,(u)*)*)*]`.
    /// Strings cross as `[len, u0, ..]` UTF-16 units.
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut i = 0usize;
        match kind {
            0 => {
                self.reset();
                vec![0.0]
            }
            1 => {
                let key = crate::js_json::read_str(args, &mut i);
                let value = args[i];
                i += 1;
                let ip = crate::js_json::read_str(args, &mut i);
                let size = self.round.add(&key, value, &ip);
                vec![size as f64]
            }
            2 => {
                let total = args[0];
                match self.round.result(total) {
                    Some((v, votes)) => vec![1.0, v, votes as f64],
                    None => vec![0.0],
                }
            }
            3 => {
                let n = args[i] as usize;
                i += 1;
                let mut active = StrSet::default();
                for _ in 0..n {
                    active.add(crate::js_json::read_str(args, &mut i));
                }
                match self.round.result_among(&active) {
                    Some((v, votes)) => vec![1.0, v, votes as f64],
                    None => vec![0.0],
                }
            }
            4 => {
                let mut out = vec![self.round.candidates.len() as f64];
                for (k, c) in &self.round.candidates {
                    crate::js_json::push_str(&mut out, k);
                    out.push(c.value);
                    out.push(c.ips.len() as f64);
                    for ip in c.ips.iter() {
                        crate::js_json::push_str(&mut out, ip);
                    }
                }
                out
            }
            k => unreachable!("vote_tally harness: unknown op kind {k}"),
        }
    }
}

impl Default for RigHarness {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(x: &str) -> Vec<f64> {
        let mut v = vec![x.len() as f64];
        v.extend(x.encode_utf16().map(|u| u as f64));
        v
    }

    fn add(key: &str, value: f64, ip: &str) -> Vec<f64> {
        let mut a = s(key);
        a.push(value);
        a.extend(s(ip));
        a
    }

    #[test]
    fn strict_majority_and_tie() {
        let mut h = RigHarness::new();
        h.run_op(0, &[]);
        assert_eq!(h.run_op(1, &add("a", 1.0, "i1")), vec![1.0]);
        assert_eq!(h.run_op(2, &[2.0]), vec![0.0]); // 1*2 > 2 false: tie
        h.run_op(1, &add("b", 2.0, "i2"));
        assert_eq!(h.run_op(2, &[2.0]), vec![0.0]); // 1+1 of 2: no majority
        assert_eq!(h.run_op(1, &add("a", 1.0, "i3")), vec![2.0]);
        assert_eq!(h.run_op(2, &[3.0]), vec![1.0, 1.0, 2.0]); // 2*2>3, first
    }

    #[test]
    fn idempotent_same_ip_cross_vote() {
        let mut h = RigHarness::new();
        h.run_op(0, &[]);
        assert_eq!(h.run_op(1, &add("a", 1.0, "i1")), vec![1.0]);
        assert_eq!(h.run_op(1, &add("a", 1.0, "i1")), vec![1.0]); // idempotent
        assert_eq!(h.run_op(1, &add("b", 2.0, "i1")), vec![1.0]); // counts in b too
        assert_eq!(h.run_op(2, &[1.0]), vec![1.0, 1.0, 1.0]); // "a" first in insertion order
    }

    #[test]
    fn result_among_exits_departed() {
        let mut h = RigHarness::new();
        h.run_op(0, &[]);
        h.run_op(1, &add("a", 1.0, "i1"));
        h.run_op(1, &add("a", 1.0, "i2"));
        h.run_op(1, &add("b", 2.0, "i3"));
        // Active {i1,i3}: a has 1 of 2 (tie, null), b has 1 of 2 (null).
        let mut among = vec![2.0];
        among.extend(s("i1"));
        among.extend(s("i3"));
        assert_eq!(h.run_op(3, &among), vec![0.0]);
        // Active {i1}: a has 1 of 1 -> wins.
        let mut one = vec![1.0];
        one.extend(s("i1"));
        assert_eq!(h.run_op(3, &one), vec![1.0, 1.0, 1.0]);
    }

    #[test]
    fn empty_round() {
        let mut h = RigHarness::new();
        h.run_op(0, &[]);
        assert_eq!(h.run_op(2, &[0.0]), vec![0.0]);
        assert_eq!(h.run_op(3, &[0.0]), vec![0.0]); // 0 votes, 0 > 0 false
        assert_eq!(h.run_op(4, &[]), vec![0.0]);
    }
}
