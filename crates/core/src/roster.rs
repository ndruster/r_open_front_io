//! Port of `src/server/Roster.ts` (the full class — pure bookkeeping).
//! `Client` is a narrow facade stub (precedent: `desync_detector`):
//! `{clientID, persistentID, ip, spectator, lastPing, publicId, ws}`. The
//! `ws` is a scripted object `{readyState, close(code?,reason?),
//! removeAllListeners()}` identified by an integer ws id in the stub world;
//! every call is a trace event in the res stream (`res = [traceLen,
//! (trace)*, payload...]`), pinning the call counts and arguments.
//!
//! Faithfulness notes (quirk list):
//!
//! * `add` operation order: `sockets.add` -> `reconnectable.set` ->
//!   `admitted.add` -> `connected.push` -> `everyone.set`. Only the final
//!   state is observable; the order is pinned by the capture's dump.
//! * `reconnect` FIRST does `sockets.add(ws)` (the NEW socket enters the Set
//!   even when it is the same object), THEN `client.ws !== ws` — an OBJECT
//!   REFERENCE identity test. In the stub world ws identity is the integer
//!   ws id (the capture never creates two distinct ws objects with the same
//!   id). On a different ws: `sockets.delete(client.ws)` +
//!   `client.ws.removeAllListeners()` + `client.ws.close()` (NO code/reason)
//!   — both calls traced (51 removeAllListeners, 52 close). Then
//!   `client.ws = ws`; `connected` is REBUILT by `filter` (order-preserving
//!   removal of the same-clientID entry) and `push` — the client MOVES TO
//!   THE END of the connected order.
//! * `markLeft` deletes the socket and the connected entry ONLY — the
//!   `everyone` record and the `reconnectable` mapping SURVIVE (the client
//!   can still be `get` and can still reconnect).
//! * `forgetReconnect` guards `reconnectable.get(pid) === client.clientID`:
//!   when the mapping points at ANOTHER client (a newer session took the
//!   seat) it deletes NOTHING.
//! * `kick` adds to `kicked` FIRST, then `some()` computes `wasConnected`
//!   BEFORE the `filter` removes it, and returns `wasConnected`.
//! * `pruneStale` uses `now - lastPing > maxSilenceMs` — STRICT `>`: a
//!   client whose silence is EXACTLY `maxSilenceMs` stays alive. Iteration
//!   is in `connected` order; stale/alive partition preserves it.
//! * `closeAll` iterates the `sockets` Set in INSERTION order and closes
//!   ONLY `readyState === WebSocket.OPEN` (=== 1) with
//!   `close(CloseCode.Normal /* 1000 */, reasonKey)` — traced as 50
//!   `[50, wsId, 1000, (reason-str)]`. A socket already removed by
//!   `markLeft`/`reconnect` is NOT in the Set anymore.
//! * `isConnected` is `connected.includes(client)` — reference identity;
//!   the stub world equates it with clientID identity (one Client object per
//!   clientID in the capture).
//! * `players()` filters `!spectator` (truthy negation on the boolean).
//! * `votingUniqueIPs()` = `new Set(players().map(ip)).size` — string Set,
//!   duplicates collapse.
//! * `isDisconnected` on an unknown clientID -> `?? true` (unknown counts
//!   as disconnected).
//! * `wasAdmitted` consults `kicked` FIRST (a kicked pid is never admitted,
//!   even when `admitted` holds it).
//! * `everyone` is a JS `Map<string, Client>`: string keys, SameValueZero
//!   (exact string equality), INSERTION ORDER for iteration — `all()`'s
//!   dump pins it; a `set` on an existing key overwrites IN PLACE (keeps
//!   position). `sockets` is a JS `Set` of ws objects -> integer ws ids in
//!   a Vec with insertion order and dedup (`add` of a present id is a
//!   no-op that does NOT move it).

use crate::js_json::{push_str, push_val, read_str, read_val, JsVal};
use crate::vote_tally::StrSet;

/// The narrow `Client` facade stub.
#[derive(Debug, Clone)]
struct ClientStub {
    client_id: String,
    persistent_id: String,
    ip: String,
    spectator: bool,
    last_ping: f64,
    public_id: JsVal,
    ws: f64,
}

impl Default for ClientStub {
    fn default() -> Self {
        Self {
            client_id: String::new(),
            persistent_id: String::new(),
            ip: String::new(),
            spectator: false,
            last_ping: 0.0,
            public_id: JsVal::Absent,
            ws: 0.0,
        }
    }
}

impl ClientStub {
    /// The capture's stub object literal `{clientID, persistentID, ip,
    /// spectator, lastPing, publicId, ws}` — key order pinned.
    fn to_val(&self) -> JsVal {
        JsVal::Obj(vec![
            ("clientID".to_string(), JsVal::Str(self.client_id.clone())),
            ("persistentID".to_string(), JsVal::Str(self.persistent_id.clone())),
            ("ip".to_string(), JsVal::Str(self.ip.clone())),
            ("spectator".to_string(), JsVal::Bool(self.spectator)),
            ("lastPing".to_string(), JsVal::Num(self.last_ping)),
            ("publicId".to_string(), self.public_id.clone()),
            ("ws".to_string(), JsVal::Num(self.ws)),
        ])
    }
}

/// A JS `Map<string, V>` with insertion-order iteration and in-place
/// overwrite on an existing key (the `everyone` / `reconnectable` /
/// `disconnected` maps; string keys, SameValueZero = exact equality).
#[derive(Debug, Default, Clone)]
struct StrMap<V> {
    entries: Vec<(String, V)>,
}

impl<V> StrMap<V> {
    fn get(&self, k: &str) -> Option<&V> {
        self.entries.iter().find(|(x, _)| x == k).map(|(_, v)| v)
    }
    fn set(&mut self, k: String, v: V) {
        if let Some(slot) = self.entries.iter_mut().find(|(x, _)| *x == k) {
            slot.1 = v;
        } else {
            self.entries.push((k, v));
        }
    }
}

/// A JS `Set<WebSocket>` keyed by the stub's integer ws id: insertion order,
/// `add` of a present id is a no-op that does not move it.
#[derive(Debug, Default, Clone)]
struct WsSet {
    ids: Vec<f64>,
}

impl WsSet {
    fn add(&mut self, id: f64) {
        if !self.ids.contains(&id) {
            self.ids.push(id);
        }
    }
    fn delete(&mut self, id: f64) {
        self.ids.retain(|x| *x != id);
    }
}

/// The ported `Roster`.
#[derive(Debug, Default)]
pub struct Roster {
    connected: Vec<ClientStub>,
    everyone: StrMap<ClientStub>,
    sockets: WsSet,
    reconnectable: StrMap<String>,
    admitted: StrSet,
    kicked: StrSet,
    disconnected: StrMap<bool>,
    /// ws id -> readyState (the scripted ws objects).
    ws_states: Vec<(f64, f64)>,
}

impl Roster {
    fn ws_state(&self, id: f64) -> f64 {
        self.ws_states
            .iter()
            .find(|(x, _)| *x == id)
            .map(|(_, r)| *r)
            .unwrap_or_else(|| panic!("roster harness: unregistered ws {id}"))
    }

    fn set_ws_state(&mut self, id: f64, ready: f64) {
        if let Some(slot) = self.ws_states.iter_mut().find(|(x, _)| *x == id) {
            slot.1 = ready;
        } else {
            self.ws_states.push((id, ready));
        }
    }

    fn find_mut(&mut self, client_id: &str) -> &mut ClientStub {
        self.connected
            .iter_mut()
            .find(|c| c.client_id == client_id)
            .or_else(|| {
                self.everyone
                    .entries
                    .iter_mut()
                    .find(|(k, _)| k == client_id)
                    .map(|(_, c)| c)
            })
            .unwrap_or_else(|| panic!("roster harness: unknown client {client_id:?}"))
    }

    /// `add(client)`.
    fn add(&mut self, client: ClientStub) {
        self.sockets.add(client.ws);
        self.reconnectable
            .set(client.persistent_id.clone(), client.client_id.clone());
        self.admitted.add(client.persistent_id.clone());
        self.connected.push(client.clone());
        self.everyone.set(client.client_id.clone(), client);
    }

    /// `reconnect(client, ws)` — traced (51 removeAllListeners, 52 close).
    fn reconnect(&mut self, client_id: &str, ws: f64, trace: &mut Vec<f64>) {
        self.sockets.add(ws);
        let old = self.find_mut(client_id).ws;
        if old != ws {
            self.sockets.delete(old);
            trace.push(51.0);
            trace.push(old);
            trace.push(52.0);
            trace.push(old);
            self.find_mut(client_id).ws = ws;
            // The everyone record and the connected entry share identity by
            // clientID; update both (the capture keeps ONE Client object per
            // clientID, so both views must move together).
            if let Some((_, c)) = self
                .everyone
                .entries
                .iter_mut()
                .find(|(k, _)| k == client_id)
            {
                c.ws = ws;
            }
        }
        self.connected.retain(|c| c.client_id != client_id);
        let moved = self
            .everyone
            .get(client_id)
            .expect("everyone never shrinks")
            .clone();
        self.connected.push(moved);
    }

    /// `markLeft(client)`.
    fn mark_left(&mut self, client_id: &str) {
        let ws = self.find_mut(client_id).ws;
        self.sockets.delete(ws);
        self.connected.retain(|c| c.client_id != client_id);
    }

    /// `forgetReconnect(client)` — guarded by the mapping identity.
    fn forget_reconnect(&mut self, client_id: &str) {
        let (pid, cid) = {
            let c = self.find_mut(client_id);
            (c.persistent_id.clone(), c.client_id.clone())
        };
        if self.reconnectable.get(&pid) == Some(&cid) {
            self.reconnectable.entries.retain(|(k, _)| *k != pid);
        }
    }

    /// `kick(client)` -> wasConnected.
    fn kick(&mut self, client_id: &str) -> bool {
        let pid = self.find_mut(client_id).persistent_id.clone();
        self.kicked.add(pid);
        let was_connected = self.connected.iter().any(|c| c.client_id == client_id);
        self.connected.retain(|c| c.client_id != client_id);
        was_connected
    }

    /// `pruneStale(now, maxSilenceMs)` -> the stale clients (connected
    /// order); `connected` becomes the alive list.
    fn prune_stale(&mut self, now: f64, max_silence_ms: f64) -> Vec<ClientStub> {
        let mut stale: Vec<ClientStub> = Vec::new();
        let mut alive: Vec<ClientStub> = Vec::new();
        for client in self.connected.drain(..) {
            if now - client.last_ping > max_silence_ms {
                stale.push(client.clone());
            } else {
                alive.push(client);
            }
        }
        self.connected = alive;
        stale
    }

    /// `closeAll(reasonKey)` — traced (50 close(code,reason) per OPEN
    /// socket, in sockets Set insertion order).
    fn close_all(&mut self, reason: &str, trace: &mut Vec<f64>) {
        let ids: Vec<f64> = self.sockets.ids.clone();
        for id in ids {
            // `ws.readyState === WebSocket.OPEN` (OPEN === 1).
            if self.ws_state(id) == 1.0 {
                trace.push(50.0);
                trace.push(id);
                trace.push(1000.0); // CloseCode.Normal
                push_str(trace, reason);
            }
        }
    }

    /// `isConnected(client)` — reference identity in JS, clientID identity
    /// in the stub world.
    fn is_connected(&self, client_id: &str) -> bool {
        self.connected.iter().any(|c| c.client_id == client_id)
    }

    /// `players()`.
    fn players(&self) -> Vec<&ClientStub> {
        self.connected.iter().filter(|c| !c.spectator).collect()
    }

    /// `byPersistentId(pid)`.
    fn by_persistent_id(&self, pid: &str) -> Option<&ClientStub> {
        let cid = self.reconnectable.get(pid)?;
        self.everyone.get(cid)
    }

    /// `wasAdmitted(pid)` — kicked excluded first.
    fn was_admitted(&self, pid: &str) -> bool {
        if self.kicked.has(pid) {
            return false;
        }
        self.admitted.has(pid)
    }

    /// `isDisconnected(clientID)` — unknown -> `?? true`.
    fn is_disconnected(&self, client_id: &str) -> bool {
        *self.disconnected.get(client_id).unwrap_or(&true)
    }

    /// `votingUniqueIPs()`.
    fn voting_unique_ips(&self) -> usize {
        let mut set = StrSet::default();
        for c in self.players() {
            set.add(c.ip.clone());
        }
        set.len()
    }
}

/// The capture harness: one `Roster` over registered ws stubs, replaying an
/// op stream. Traced ops (2 reconnect, 7 closeAll) prefix their res with
/// `[traceLen,(trace)*]`.
#[derive(Debug, Default)]
pub struct RigHarness {
    roster: Roster,
}

impl RigHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Run one op. Kind table (see `tools/gen_vectors.mjs`):
    /// 0 reset -> `[0]`;
    /// 1 add `[(clientID-str),(persistentID-str),(ip-str),spectator 0|1,
    ///   lastPing,(publicId val),wsId,readyState]` -> `[0]` (also registers
    ///   the ws stub state);
    /// 2 reconnect `[(clientID-str),wsId,readyState]` -> `[traceLen,(trace)*]`;
    /// 3 markLeft `[(clientID-str)]` -> `[0]`;
    /// 4 forgetReconnect `[(clientID-str)]` -> `[0]`;
    /// 5 kick `[(clientID-str)]` -> `[0|1]` (wasConnected);
    /// 6 pruneStale `[now,maxSilenceMs]` -> `[n,(clientID-str)*n]` stale;
    /// 7 closeAll `[(reasonKey-str)]` -> `[traceLen,(trace)*]`;
    /// 8 active -> `[n,(clientID-str)*n]`;
    /// 9 isConnected `[(clientID-str)]` -> `[0|1]`;
    /// 10 players -> `[n,(clientID-str)*n]`;
    /// 11 all -> `[n,(val client)*n]` (everyone Map insertion order);
    /// 12 get `[(clientID-str)]` -> val client | Undef;
    /// 13 byPersistentId `[(pid-str)]` -> val client | Undef;
    /// 14 isKicked `[(pid-str)]` -> `[0|1]`;
    /// 15 wasAdmitted `[(pid-str)]` -> `[0|1]`;
    /// 16 isDisconnected `[(clientID-str)]` -> `[0|1]`;
    /// 17 setDisconnected `[(clientID-str),0|1]` -> `[0]`;
    /// 18 votingUniqueIPs -> `[n]`;
    /// 19 setWsReadyState `[wsId,readyState]` -> `[0]`.
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut i = 0usize;
        let mut trace: Vec<f64> = Vec::new();
        let payload: Vec<f64> = match kind {
            0 => {
                self.reset();
                vec![0.0]
            }
            1 => {
                let client_id = read_str(args, &mut i);
                let persistent_id = read_str(args, &mut i);
                let ip = read_str(args, &mut i);
                let spectator = args[i] != 0.0;
                i += 1;
                let last_ping = args[i];
                i += 1;
                let public_id = read_val(args, &mut i);
                let ws = args[i];
                i += 1;
                let ready = args[i];
                self.roster.set_ws_state(ws, ready);
                self.roster.add(ClientStub {
                    client_id,
                    persistent_id,
                    ip,
                    spectator,
                    last_ping,
                    public_id,
                    ws,
                });
                vec![0.0]
            }
            2 => {
                let client_id = read_str(args, &mut i);
                let ws = args[i];
                i += 1;
                let ready = args[i];
                self.roster.set_ws_state(ws, ready);
                self.roster.reconnect(&client_id, ws, &mut trace);
                vec![0.0]
            }
            3 => {
                let client_id = read_str(args, &mut i);
                self.roster.mark_left(&client_id);
                vec![0.0]
            }
            4 => {
                let client_id = read_str(args, &mut i);
                self.roster.forget_reconnect(&client_id);
                vec![0.0]
            }
            5 => {
                let client_id = read_str(args, &mut i);
                vec![if self.roster.kick(&client_id) { 1.0 } else { 0.0 }]
            }
            6 => {
                let now = args[i];
                i += 1;
                let max_silence = args[i];
                let stale = self.roster.prune_stale(now, max_silence);
                let mut out = vec![stale.len() as f64];
                for c in &stale {
                    push_str(&mut out, &c.client_id);
                }
                out
            }
            7 => {
                let reason = read_str(args, &mut i);
                self.roster.close_all(&reason, &mut trace);
                vec![0.0]
            }
            8 => {
                let mut out = vec![self.roster.connected.len() as f64];
                for c in &self.roster.connected {
                    push_str(&mut out, &c.client_id);
                }
                out
            }
            9 => {
                let client_id = read_str(args, &mut i);
                vec![if self.roster.is_connected(&client_id) { 1.0 } else { 0.0 }]
            }
            10 => {
                let p = self.roster.players();
                let mut out = vec![p.len() as f64];
                for c in p {
                    push_str(&mut out, &c.client_id);
                }
                out
            }
            11 => {
                let mut out = vec![self.roster.everyone.entries.len() as f64];
                for (_, c) in self.roster.everyone.entries.iter() {
                    push_val(&mut out, &c.to_val());
                }
                out
            }
            12 => {
                let client_id = read_str(args, &mut i);
                let mut out = Vec::new();
                match self.roster.everyone.get(&client_id) {
                    Some(c) => push_val(&mut out, &c.to_val()),
                    None => push_val(&mut out, &JsVal::Undef), // JS undefined
                }
                out
            }
            13 => {
                let pid = read_str(args, &mut i);
                let mut out = Vec::new();
                match self.roster.by_persistent_id(&pid) {
                    Some(c) => push_val(&mut out, &c.to_val()),
                    None => push_val(&mut out, &JsVal::Undef), // JS undefined
                }
                out
            }
            14 => {
                let pid = read_str(args, &mut i);
                vec![if self.roster.kicked.has(&pid) { 1.0 } else { 0.0 }]
            }
            15 => {
                let pid = read_str(args, &mut i);
                vec![if self.roster.was_admitted(&pid) { 1.0 } else { 0.0 }]
            }
            16 => {
                let client_id = read_str(args, &mut i);
                vec![if self.roster.is_disconnected(&client_id) { 1.0 } else { 0.0 }]
            }
            17 => {
                let client_id = read_str(args, &mut i);
                let v = args[i] != 0.0;
                self.roster.disconnected.set(client_id, v);
                vec![0.0]
            }
            18 => vec![self.roster.voting_unique_ips() as f64],
            19 => {
                let ws = args[i];
                i += 1;
                let ready = args[i];
                self.roster.set_ws_state(ws, ready);
                vec![0.0]
            }
            k => unreachable!("roster harness: unknown op kind {k}"),
        };
        if kind == 2 || kind == 7 {
            let mut out = Vec::with_capacity(1 + trace.len() + payload.len());
            out.push(trace.len() as f64);
            out.extend(trace.iter().copied());
            out.extend(payload);
            out
        } else {
            payload
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc(x: &str) -> Vec<f64> {
        let mut v = vec![x.encode_utf16().count() as f64];
        v.extend(x.encode_utf16().map(|u| u as f64));
        v
    }

    fn add_args(id: &str, pid: &str, ip: &str, spectator: bool, last_ping: f64, ws: f64, ready: f64) -> Vec<f64> {
        let mut a = Vec::new();
        a.extend(enc(id));
        a.extend(enc(pid));
        a.extend(enc(ip));
        a.push(if spectator { 1.0 } else { 0.0 });
        a.push(last_ping);
        push_val(&mut a, &JsVal::Str(format!("pub-{id}")));
        a.push(ws);
        a.push(ready);
        a
    }

    #[test]
    fn add_then_all_dump_order() {
        let mut h = RigHarness::new();
        h.run_op(0, &[]);
        h.run_op(1, &add_args("a", "pa", "1.1.1.1", false, 0.0, 1.0, 1.0));
        h.run_op(1, &add_args("b", "pb", "2.2.2.2", true, 0.0, 2.0, 1.0));
        let r = h.run_op(11, &[]);
        assert_eq!(r[0], 2.0); // two everyone records, insertion order a,b
        let mut j = 1usize;
        let a = read_val(&r, &mut j);
        if let JsVal::Obj(f) = &a {
            assert_eq!(f[0].0, "clientID");
            assert_eq!(f[0].1, JsVal::Str("a".into()));
            assert_eq!(f[6].0, "ws");
            assert_eq!(f[6].1, JsVal::Num(1.0));
        } else {
            panic!()
        }
        let b = read_val(&r, &mut j);
        assert!(matches!(&b, JsVal::Obj(f) if f[0].1 == JsVal::Str("b".into())));
        // players() excludes the spectator
        let p = h.run_op(10, &[]);
        assert_eq!(p[0], 1.0);
        // votingUniqueIPs counts both? players = [a] -> 1
        assert_eq!(h.run_op(18, &[]), vec![1.0]);
    }

    #[test]
    fn reconnect_same_ws_no_close_trace() {
        let mut h = RigHarness::new();
        h.run_op(1, &add_args("a", "pa", "1.1.1.1", false, 0.0, 1.0, 1.0));
        // Same ws: sockets.add is a no-op, NO removeAllListeners/close.
        let r = h.run_op(2, &{
            let mut a = enc("a");
            a.push(1.0);
            a.push(1.0);
            a
        });
        assert_eq!(r[0], 0.0); // empty trace
        // Different ws: 51 + 52 on the OLD ws id.
        let r = h.run_op(2, &{
            let mut a = enc("a");
            a.push(2.0);
            a.push(1.0);
            a
        });
        assert_eq!(r[0], 4.0);
        assert_eq!(&r[1..5], &[51.0, 1.0, 52.0, 1.0]);
    }

    #[test]
    fn reconnect_moves_to_end() {
        let mut h = RigHarness::new();
        h.run_op(1, &add_args("a", "pa", "1", false, 0.0, 1.0, 1.0));
        h.run_op(1, &add_args("b", "pb", "2", false, 0.0, 2.0, 1.0));
        h.run_op(2, &{
            let mut a = enc("a");
            a.push(3.0);
            a.push(1.0);
            a
        });
        let r = h.run_op(8, &[]); // active order: b, a
        let mut j = 1usize;
        assert_eq!(read_str(&r, &mut j), "b");
        assert_eq!(read_str(&r, &mut j), "a");
    }

    #[test]
    fn mark_left_keeps_record() {
        let mut h = RigHarness::new();
        h.run_op(1, &add_args("a", "pa", "1", false, 0.0, 1.0, 1.0));
        h.run_op(3, &enc("a"));
        assert_eq!(h.run_op(9, &enc("a")), vec![0.0]); // not connected
        let r = h.run_op(12, &enc("a")); // record survives
        assert!(matches!(read_val(&r, &mut 0usize), JsVal::Obj(_)));
        // byPersistentId still resolves
        let r = h.run_op(13, &enc("pa"));
        assert!(matches!(read_val(&r, &mut 0usize), JsVal::Obj(_)));
        // forgetReconnect + byPersistentId miss -> Undef
        h.run_op(4, &enc("a"));
        let r = h.run_op(13, &enc("pa"));
        assert!(matches!(read_val(&r, &mut 0usize), JsVal::Undef));
    }

    #[test]
    fn forget_reconnect_guard() {
        let mut h = RigHarness::new();
        h.run_op(1, &add_args("a", "pa", "1", false, 0.0, 1.0, 1.0));
        // b takes over pa's seat
        h.run_op(1, &add_args("b", "pa", "2", false, 0.0, 2.0, 1.0));
        // a's mapping now points at b -> forgetReconnect(a) deletes NOTHING
        h.run_op(4, &enc("a"));
        let r = h.run_op(13, &enc("pa"));
        assert!(matches!(read_val(&r, &mut 0usize), JsVal::Obj(f) if f[0].1 == JsVal::Str("b".into())));
        // b's mapping points at itself -> deletes
        h.run_op(4, &enc("b"));
        let r = h.run_op(13, &enc("pa"));
        assert!(matches!(read_val(&r, &mut 0usize), JsVal::Undef));
    }

    #[test]
    fn kick_returns_and_excludes_admission() {
        let mut h = RigHarness::new();
        h.run_op(1, &add_args("a", "pa", "1", false, 0.0, 1.0, 1.0));
        assert_eq!(h.run_op(5, &enc("a")), vec![1.0]); // wasConnected
        assert_eq!(h.run_op(5, &enc("a")), vec![0.0]); // already gone
        assert_eq!(h.run_op(14, &enc("pa")), vec![1.0]); // isKicked
        assert_eq!(h.run_op(15, &enc("pa")), vec![0.0]); // wasAdmitted false
    }

    #[test]
    fn prune_stale_strict_gt() {
        let mut h = RigHarness::new();
        h.run_op(1, &add_args("a", "pa", "1", false, 100.0, 1.0, 1.0));
        h.run_op(1, &add_args("b", "pb", "2", false, 50.0, 2.0, 1.0));
        // now=200: a silence 100 == max -> NOT stale (strict >); b 150 > 100 stale.
        let r = h.run_op(6, &[200.0, 100.0]);
        assert_eq!(r[0], 1.0);
        let mut j = 1usize;
        assert_eq!(read_str(&r, &mut j), "b");
        assert_eq!(h.run_op(9, &enc("a")), vec![1.0]);
        assert_eq!(h.run_op(9, &enc("b")), vec![0.0]);
    }

    #[test]
    fn close_all_open_gate_only() {
        let mut h = RigHarness::new();
        h.run_op(1, &add_args("a", "pa", "1", false, 0.0, 1.0, 1.0)); // OPEN
        h.run_op(1, &add_args("b", "pb", "2", false, 0.0, 2.0, 3.0)); // CLOSED
        h.run_op(1, &add_args("c", "pc", "3", false, 0.0, 3.0, 0.0)); // CONNECTING
        let r = h.run_op(7, &enc("close_reason.game_ended"));
        // only ws 1 closes: [50, 1, 1000, (reason)]
        let tl = r[0] as usize;
        assert_eq!(&r[1..4], &[50.0, 1.0, 1000.0]);
        let mut j = 4usize;
        assert_eq!(read_str(&r, &mut j), "close_reason.game_ended");
        assert_eq!(tl, j - 1);
    }

    #[test]
    fn disconnected_unknown_true_and_flip() {
        let mut h = RigHarness::new();
        assert_eq!(h.run_op(16, &enc("zz")), vec![1.0]); // unknown -> true
        h.run_op(1, &add_args("a", "pa", "1", false, 0.0, 1.0, 1.0));
        assert_eq!(h.run_op(16, &enc("a")), vec![1.0]); // still unknown flag
        h.run_op(17, &{
            let mut a = enc("a");
            a.push(0.0);
            a
        });
        assert_eq!(h.run_op(16, &enc("a")), vec![0.0]);
    }

    #[test]
    fn voting_unique_ips_dedup() {
        let mut h = RigHarness::new();
        h.run_op(1, &add_args("a", "pa", "1", false, 0.0, 1.0, 1.0));
        h.run_op(1, &add_args("b", "pb", "1", false, 0.0, 2.0, 1.0)); // same ip
        h.run_op(1, &add_args("c", "pc", "2", true, 0.0, 3.0, 1.0)); // spectator
        assert_eq!(h.run_op(18, &[]), vec![1.0]); // {1} only
    }
}
