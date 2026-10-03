//! Port of `src/server/Consensus.ts`: `class WinnerVote` and `class
//! LiveStatsVote` on top of the ported [`crate::vote_tally::VoteRound`].
//!
//! The candidate key is `JSON.stringify(msg.winner ?? null)` /
//! `JSON.stringify(stats)` — the Rust side reproduces it through
//! [`crate::js_json::json_stringify`] over the codec value, and the capture
//! additionally records the REAL TS `JSON.stringify` output as the expected
//! key token, so the two ends cross-check.
//!
//! Faithfulness notes:
//!
//! * `WinnerVote.cast` keys a cancelled match (`winner` absent or
//!   `undefined`) as the string `"null"` (`?? null` before stringify), never
//!   `undefined` (`cv_winner_cancelled`).
//! * `tally` / `tallyAmong` delegate to `result` / `resultAmong` and store
//!   `decided` on EVERY non-null result — the class itself does not latch
//!   ("Decided once; the game guards against votes arriving after that" is
//!   the caller's job), so a re-tally over a different electorate can move
//!   `decided` (pinned by `cv_redecide`).
//! * The candidate's stored value is the message / stats object of the vote
//!   that FIRST created the candidate; later same-key votes (possibly equal
//!   but distinct objects) do not move it — modelled by the payload token
//!   only being written on candidate creation (`VoteRound::add`).
//! * `LiveStatsVote.cast` ignores turns `<= settled.turn`, creates + prunes
//!   the round BEFORE the voter dedup check (a re-vote from the same
//!   clientID at a NEW turn still creates its round), dedups per
//!   (turn, clientID), keys by the full `JSON.stringify(stats)`, and on
//!   settle deletes every round key `t <= turn` (JS deletes visited keys
//!   during live `Map` iteration; collect-then-delete is observably
//!   identical here because only `<= turn` keys, all reached no later than
//!   the current one, are dropped).
//! * `prune` drops the oldest key (`keys().next().value`) while
//!   `rounds.size > MAX_PENDING_ROUNDS` (20); turns arrive ascending in the
//!   capture, so "oldest key" = first inserted.

use crate::js_json::{json_stringify, push_str, read_str, read_val, to_json, JsVal};
use crate::vote_tally::{StrSet, VoteRound};

/// `LiveStatsVote.MAX_PENDING_ROUNDS`.
const MAX_PENDING_ROUNDS: usize = 20;

/// The ported `WinnerVote`. The message payload rides as an opaque `f64`
/// token the capture assigns (the TS stores the message object; the token
/// round-trips through `winner()` / the tally result).
#[derive(Debug, Default)]
pub struct WinnerVote {
    round: VoteRound,
    decided: Option<f64>,
}

impl WinnerVote {
    /// `cast(msg, ip)` -> `(key, votes)`. `key = JSON.stringify(msg.winner
    /// ?? null)`; `value` is the capture's payload token for `msg`.
    pub(crate) fn cast(&mut self, msg: &JsVal, value: f64, ip: &str) -> (String, usize) {
        let winner = field(msg, "winner");
        // `msg.winner ?? null`: undefined / null / absent all key as "null".
        let keyed = match winner {
            Some(w) if !matches!(w, JsVal::Undef | JsVal::Null | JsVal::Absent) => w.clone(),
            _ => JsVal::Null,
        };
        let key = json_stringify(&to_json(&keyed)).expect("non-undefined stringify");
        let votes = self.round.add(&key, value, ip);
        (key, votes)
    }

    /// `tally(electorate)`: store `decided` on a non-null result.
    pub(crate) fn tally(&mut self, electorate: f64) -> Option<(f64, usize)> {
        let r = self.round.result(electorate);
        if let Some((v, _)) = r {
            self.decided = Some(v);
        }
        r
    }

    /// `tallyAmong(activeIPs)`: store `decided` on a non-null result.
    pub(crate) fn tally_among(&mut self, active: &StrSet) -> Option<(f64, usize)> {
        let r = self.round.result_among(active);
        if let Some((v, _)) = r {
            self.decided = Some(v);
        }
        r
    }
}

/// `obj[key]` for a codec object; absent field / non-object reads `None`
/// (JS `undefined`).
fn field<'a>(v: &'a JsVal, key: &str) -> Option<&'a JsVal> {
    match v {
        JsVal::Obj(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, x)| x),
        _ => None,
    }
}

/// One `LiveStatsVote` round entry.
#[derive(Debug, Default)]
struct LsEntry {
    round: VoteRound,
    voters: StrSet,
}

/// The ported `LiveStatsVote`. Stats payloads ride as opaque `f64` tokens;
/// the settled snapshot is `(value token, turn)`.
#[derive(Debug, Default)]
pub struct LiveStatsVote {
    rounds: Vec<(f64, LsEntry)>,
    settled: Option<(f64, f64)>,
}

impl LiveStatsVote {
    /// `cast(clientID, ip, stats, electorate)` -> whether this vote settled
    /// its turn. `turn` is `stats.turn` (read by the harness from the codec
    /// object), `value` the capture's payload token for `stats`.
    pub(crate) fn cast(
        &mut self,
        turn: f64,
        client_id: &str,
        ip: &str,
        key: &str,
        value: f64,
        electorate: f64,
    ) -> bool {
        if let Some((_, st)) = &self.settled {
            if turn <= *st {
                return false;
            }
        }
        let slot = self.round_slot(turn);
        if self.rounds[slot].1.voters.has(client_id) {
            return false;
        }
        self.rounds[slot].1.voters.add(client_id.to_string());
        self.rounds[slot].1.round.add(key, value, ip);
        let Some((rv, _)) = self.rounds[slot].1.round.result(electorate) else {
            return false;
        };
        self.settled = Some((rv, turn));
        // "This turn (and any older still-pending ones) are now settled."
        let doomed: Vec<usize> = (0..self.rounds.len())
            .filter(|&i| self.rounds[i].0 <= turn)
            .collect();
        for i in doomed.into_iter().rev() {
            self.rounds.remove(i);
        }
        true
    }

    /// Get (creating + pruning if absent) the round slot for `turn`. JS
    /// `Map<number>` keys with SameValueZero (`NaN` matches, `+0`/`-0`
    /// collapse).
    fn round_slot(&mut self, turn: f64) -> usize {
        if let Some(i) = self.rounds.iter().position(|(t, _)| *t == turn || (t.is_nan() && turn.is_nan())) {
            return i;
        }
        self.rounds.push((turn, LsEntry::default()));
        while self.rounds.len() > MAX_PENDING_ROUNDS {
            // `keys().next().value`: the first (oldest-inserted) key.
            self.rounds.remove(0);
        }
        self.rounds.len() - 1
    }
}

/// The capture harness: one `WinnerVote` and one `LiveStatsVote` sharing an
/// op stream.
#[derive(Debug, Default)]
pub struct RigHarness {
    winner: WinnerVote,
    stats: LiveStatsVote,
}

impl RigHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Run one op. Kind table (see `tools/gen_vectors.mjs`):
    /// 0 construct -> `[0]`;
    /// 1 WinnerVote.cast `[value, msg, (len,u0..)* ip]` ->
    ///   `[1,(key-str),votes]` (key is the Rust-computed `JSON.stringify`;
    ///   the golden `res` carries the real TS one);
    /// 2 WinnerVote.tally `[electorate]` -> `[0]` | `[1,value,votes]`;
    /// 3 WinnerVote.tallyAmong `[n,(ip)*]` -> `[0]` | `[1,value,votes]`;
    /// 4 WinnerVote.winner -> `[0]` null | `[1,value]`;
    /// 5 LiveStatsVote.cast `[turn, value, (id-str), (ip-str), electorate,
    ///   stats]` -> `[0|1]`;
    /// 6 LiveStatsVote.latest -> `[0]` | `[1,turn,value]`;
    /// 7 dump WinnerVote round -> candidate dump `[n,(key,value,m,(ip)*)*]`;
    /// 8 dump LiveStatsVote rounds -> `[n,(turn,votersN,(id)*,roundDump)*]`.
    /// Strings cross as `[len, u0, ..]` UTF-16 units; `msg` / `stats` are
    /// codec values. The `value` token is the opaque payload id the capture
    /// assigned to the message / stats object.
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut i = 0usize;
        match kind {
            0 => {
                self.reset();
                vec![0.0]
            }
            1 => {
                let value = args[i];
                i += 1;
                let msg = read_val(args, &mut i);
                let ip = read_str(args, &mut i);
                let (key, votes) = self.winner.cast(&msg, value, &ip);
                let mut out = vec![1.0];
                push_str(&mut out, &key);
                out.push(votes as f64);
                out
            }
            2 => {
                let electorate = args[0];
                match self.winner.tally(electorate) {
                    Some((v, votes)) => vec![1.0, v, votes as f64],
                    None => vec![0.0],
                }
            }
            3 => {
                let n = args[i] as usize;
                i += 1;
                let mut active = StrSet::default();
                for _ in 0..n {
                    active.add(read_str(args, &mut i));
                }
                match self.winner.tally_among(&active) {
                    Some((v, votes)) => vec![1.0, v, votes as f64],
                    None => vec![0.0],
                }
            }
            4 => match self.winner.decided {
                Some(v) => vec![1.0, v],
                None => vec![0.0],
            },
            5 => {
                let turn = args[i];
                i += 1;
                let value = args[i];
                i += 1;
                let client_id = read_str(args, &mut i);
                let ip = read_str(args, &mut i);
                let electorate = args[i];
                i += 1;
                let stats = read_val(args, &mut i);
                let key = json_stringify(&to_json(&stats)).expect("stats stringify");
                let settled = self.stats.cast(turn, &client_id, &ip, &key, value, electorate);
                vec![if settled { 1.0 } else { 0.0 }]
            }
            6 => match self.stats.settled {
                Some((v, t)) => vec![1.0, t, v],
                None => vec![0.0],
            },
            7 => {
                let mut out = Vec::new();
                self.winner.round.dump_into(&mut out);
                out
            }
            8 => {
                let mut out = vec![self.stats.rounds.len() as f64];
                for (t, e) in &self.stats.rounds {
                    out.push(*t);
                    out.push(e.voters.len() as f64);
                    for id in e.voters.iter() {
                        push_str(&mut out, id);
                    }
                    e.round.dump_into(&mut out);
                }
                out
            }
            k => unreachable!("consensus harness: unknown op kind {k}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::js_json::push_val;

    fn cast_args(msg: JsVal, value: f64, ip: &str) -> Vec<f64> {
        let mut a = vec![value];
        push_val(&mut a, &msg);
        push_str(&mut a, ip);
        a
    }

    #[test]
    fn winner_stringify_keys() {
        let mut h = RigHarness::new();
        h.run_op(0, &[]);
        let msg = JsVal::Obj(vec![("winner".to_string(), JsVal::Str("p1".into()))]);
        let out = h.run_op(1, &cast_args(msg, 7.0, "ip1"));
        // out = [1, (key-str), votes]; key = JSON.stringify("p1") = "\"p1\"".
        let mut j = 1usize;
        let key = read_str(&out, &mut j);
        assert_eq!(key, "\"p1\"");
        assert_eq!(out[j], 1.0); // votes
    }

    #[test]
    fn cancelled_winner_keys_null() {
        let mut h = RigHarness::new();
        h.run_op(0, &[]);
        let msg = JsVal::Obj(vec![("type".to_string(), JsVal::Str("client_send_winner".into()))]);
        let out = h.run_op(1, &cast_args(msg, 3.0, "ip1"));
        let mut j = 1usize;
        let key = read_str(&out, &mut j);
        assert_eq!(key, "null");
        assert_eq!(out[j], 1.0); // votes
    }

    #[test]
    fn object_winner_key_order_is_insertion_order() {
        let mut h = RigHarness::new();
        h.run_op(0, &[]);
        let winner = JsVal::Obj(vec![
            ("b".to_string(), JsVal::Num(2.0)),
            ("a".to_string(), JsVal::Bool(true)),
        ]);
        let msg = JsVal::Obj(vec![("winner".to_string(), winner)]);
        let out = h.run_op(1, &cast_args(msg, 9.0, "ip1"));
        let mut j = 1usize;
        let key = read_str(&out, &mut j);
        assert_eq!(key, "{\"b\":2,\"a\":true}");
    }

    #[test]
    fn decided_overwrites_on_recount() {
        let mut h = RigHarness::new();
        h.run_op(0, &[]);
        let a = JsVal::Obj(vec![("winner".to_string(), JsVal::Str("a".into()))]);
        let b = JsVal::Obj(vec![("winner".to_string(), JsVal::Str("b".into()))]);
        h.run_op(1, &cast_args(a.clone(), 1.0, "i1"));
        h.run_op(1, &cast_args(a, 1.0, "i2"));
        h.run_op(1, &cast_args(b, 2.0, "i3"));
        // Electorate 3: a (2 votes) wins, decided = a.
        assert_eq!(h.run_op(2, &[3.0]), vec![1.0, 1.0, 2.0]);
        assert_eq!(h.run_op(4, &[]), vec![1.0, 1.0]);
        // Re-tally among {i3}: b holds 1 of 1 -> decided MOVES to b (the
        // class does not latch; the caller guards).
        let mut among = vec![1.0];
        push_str(&mut among, "i3");
        assert_eq!(h.run_op(3, &among), vec![1.0, 2.0, 1.0]);
        assert_eq!(h.run_op(4, &[]), vec![1.0, 2.0]);
    }

    #[test]
    fn live_stats_ignore_stale_and_dedup() {
        let mut h = RigHarness::new();
        h.run_op(0, &[]);
        let cast = |h: &mut RigHarness, turn: f64, cid: &str, ip: &str, electorate: f64| {
            let stats = JsVal::Obj(vec![("turn".to_string(), JsVal::Num(turn))]);
            let mut a = vec![turn, turn * 10.0];
            push_str(&mut a, cid);
            push_str(&mut a, ip);
            a.push(electorate);
            push_val(&mut a, &stats);
            h.run_op(5, &a)
        };
        // Electorate 1: the first vote settles turn 1.
        assert_eq!(cast(&mut h, 1.0, "c1", "i1", 1.0), vec![1.0]);
        // Stale turns are ignored.
        assert_eq!(cast(&mut h, 1.0, "c2", "i2", 1.0), vec![0.0]);
        assert_eq!(cast(&mut h, 0.0, "c2", "i2", 1.0), vec![0.0]);
        // New turn 2, electorate 5: voter dedup, settle at 3 of 5.
        assert_eq!(cast(&mut h, 2.0, "c1", "i1", 5.0), vec![0.0]); // 1 vote
        assert_eq!(cast(&mut h, 2.0, "c1", "i9", 5.0), vec![0.0]); // same clientID
        assert_eq!(cast(&mut h, 2.0, "c2", "i2", 5.0), vec![0.0]); // 2 votes: 4>5 false
        assert_eq!(cast(&mut h, 2.0, "c3", "i3", 5.0), vec![1.0]); // 3 votes: 6>5
        // latest() reflects the turn-2 settle.
        assert_eq!(h.run_op(6, &[]), vec![1.0, 2.0, 20.0]);
    }

    #[test]
    fn prune_keeps_twenty() {
        let mut h = RigHarness::new();
        h.run_op(0, &[]);
        for t in 1..=25 {
            let mut a = vec![t as f64, t as f64];
            push_str(&mut a, "c1");
            push_str(&mut a, "i1");
            a.push(100.0); // never settles
            let stats = JsVal::Obj(vec![("turn".to_string(), JsVal::Num(t as f64))]);
            push_val(&mut a, &stats);
            h.run_op(5, &a);
        }
        let dump = h.run_op(8, &[]);
        // 20 rounds survive: turns 6..25 (the oldest 1..5 pruned).
        assert_eq!(dump[0], 20.0);
        assert_eq!(dump[1], 6.0);
    }
}
