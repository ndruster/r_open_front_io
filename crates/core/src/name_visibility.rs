//! Port of `src/server/NameVisibility.ts`: `friendsLookup(active)` and
//! `class NameVisibility` over the scripted `NameVisibilityView` facade
//! (precedent: `train_station` / `consensus`). The facade thunks
//! `config()` / `clients()` / `teamIndex(client)` are traced events — every
//! call is recorded in the res stream (`res = [traceLen, (trace)*,
//! payload*]`), pinning the call counts and the short-circuit orders. The
//! `Client` class itself stays EXCLUDED: client stubs ride as plain codec
//! objects the capture feeds through the `js_json` codec.
//!
//! Faithfulness notes:
//!
//! * `viewerSeesAllNames`: `viewer === undefined` returns false WITHOUT any
//!   facade call; the `nameReveals?.includes(viewer) ?? false` hit returns
//!   BEFORE the `clients()` lookup (trace `[20]` vs `[20, 21, ...]`).
//! * `anonName`: the slot is the target's join-order index in
//!   `clients().keys()` (one `clients()` call); a target absent from the map
//!   leaves the loop running to completion, so slot = map size. Late
//!   joiners only append — existing slots never shift.
//! * `anonOffsetSeed`: viewer undefined -> 0 (no facade call); client
//!   missing -> NO `teamIndex` call, seed = `simpleHash(viewer)`; team
//!   present -> `simpleHash(`${gameID}:team:${team}`)` with the JS
//!   number->string interpolation (`team 0` -> `"g:team:0"`).
//! * `sameMatchmadeTeam`: `viewerTeam !== undefined && viewerTeam ===
//!   teamIndex(targetClient)` — when the viewer has no pinned team the
//!   SECOND `teamIndex` call never happens (trace count pinned).
//! * `seesRealBeyondTeam`: `!config().anonymizeNames || target === viewer ||
//!   viewerSeesAllNames(viewer)` — the self-view short-circuits before the
//!   reveal lookup (no second `config()` call).
//! * `startInfoFor`: the config is read ONCE at the top; `GameMode.FFA` is
//!   the string `"Free For All"`; the `!anonymizeNames` path returns the
//!   SAME object (`real` for admin+FFA else `wire` — observable because the
//!   capture feeds differing real/wire contents); the map path spreads
//!   `...p` first (key order = wire key order, overrides in place) and
//!   reads `real.players[i].clanTag` at the WIRE index (the index-alignment
//!   quirk, pinned with differently ordered real/wire player arrays).
//! * `lobbyClients`: `friendsLookup` is computed over `active` up front
//!   (no facade call), the config is read ONCE, `hideClanTags` is
//!   `disableClanTags ?? false`; the anon branch emits NO friends/verified
//!   keys (key order username, clanTag, clientID, spectator, teamIndex)
//!   while the real branch has all seven; `teammateOnly` re-reads the
//!   CACHED config (no extra `config()` trace) and only calls
//!   `seesRealBeyondTeam` when anonymizing. `spectator || undefined` maps
//!   `false` to undefined (NOT `??`); `verified` is `cosmetics?.verified`.

use crate::anon_names::anon_word_name;
use crate::game_ts::js_num_str;
use crate::js_json::{map_set, push_str, push_val, read_map, read_str, read_val, val_field, JsVal};
use crate::util::simple_hash;

/// JS truthiness restricted to the codec domain.
fn truthy(v: &JsVal) -> bool {
    match v {
        JsVal::Absent | JsVal::Undef | JsVal::Null => false,
        JsVal::Bool(b) => *b,
        JsVal::Num(n) => *n != 0.0 && !n.is_nan(),
        JsVal::Str(s) => !s.is_empty(),
        JsVal::Obj(_) | JsVal::Arr(_) => true,
    }
}

/// `obj[key]` where absent / present-`undefined` both model JS `undefined`.
fn field<'a>(obj: &'a JsVal, key: &str) -> Option<&'a JsVal> {
    match val_field(obj, key) {
        Some(JsVal::Absent) | Some(JsVal::Undef) | None => None,
        Some(v) => Some(v),
    }
}

/// `arr?.includes(str) ?? false` over a codec value: a non-array (absent /
/// undefined / null) yields `None` (the `?.` undefined), an array yields the
/// `===` scan result.
fn arr_includes(v: Option<&JsVal>, s: &str) -> Option<bool> {
    match v {
        Some(JsVal::Arr(items)) => Some(items.iter().any(|x| matches!(x, JsVal::Str(t) if t == s))),
        _ => None,
    }
}

/// JS `===` restricted to the capture's scalar codec domain.
fn js_eq(a: &JsVal, b: &JsVal) -> bool {
    match (a, b) {
        (JsVal::Str(x), JsVal::Str(y)) => x == y,
        (JsVal::Num(x), JsVal::Num(y)) => x == y,
        (JsVal::Bool(x), JsVal::Bool(y)) => x == y,
        (JsVal::Null, JsVal::Null) => true,
        _ => false,
    }
}

/// `arr?.includes(value)` with the `===` scan over any codec scalar (a
/// `null` publicId passes the `!== undefined` gate and must compare by
/// value, not be coerced to `""`).
fn arr_includes_val(v: Option<&JsVal>, want: &JsVal) -> bool {
    match v {
        Some(JsVal::Arr(items)) => items.iter().any(|x| js_eq(x, want)),
        _ => false,
    }
}

/// The scripted `NameVisibilityView`: the gameID, the live config map, the
/// insertion-ordered client map and the pinned-team table.
#[derive(Debug, Default)]
pub struct NameVisibility {
    game_id: String,
    config: Vec<(String, JsVal)>,
    clients: Vec<(String, JsVal)>,
    teams: Vec<(String, JsVal)>,
}

impl NameVisibility {
    fn cfg(&self, key: &str) -> Option<&JsVal> {
        self.config.iter().find(|(k, _)| k == key).map(|(_, v)| v).filter(|v| **v != JsVal::Absent && **v != JsVal::Undef)
    }

    fn client(&self, id: &str) -> Option<&JsVal> {
        self.clients.iter().find(|(k, _)| k == id).map(|(_, v)| v)
    }

    fn team_of(&self, id: &str) -> Option<f64> {
        match self.teams.iter().find(|(k, _)| k == id).map(|(_, v)| v) {
            Some(JsVal::Num(n)) => Some(*n),
            _ => None,
        }
    }

    /// `view.teamIndex(client)` — traced: `[22, (clientID-str), val]`.
    fn team_index(&self, client_id: &str, trace: &mut Vec<f64>) -> Option<f64> {
        let t = self.team_of(client_id);
        trace.push(22.0);
        push_str(trace, client_id);
        push_val(trace, &t.map_or(JsVal::Undef, JsVal::Num));
        t
    }

    /// `viewerSeesAllNames(viewer)` (private; observable through the ops).
    fn viewer_sees_all_names(&self, viewer: Option<&str>, trace: &mut Vec<f64>) -> bool {
        let Some(v) = viewer else { return false };
        trace.push(20.0); // view.config()
        if arr_includes(self.cfg("nameReveals"), v).unwrap_or(false) {
            return true;
        }
        trace.push(21.0); // view.clients()
        // `?.publicId` then `publicId !== undefined`: absent / present-
        // undefined both fail the gate; `null` PASSES it and is compared
        // by value inside includes.
        let public_id = self
            .client(v)
            .and_then(|c| val_field(c, "publicId"))
            .filter(|x| **x != JsVal::Absent && **x != JsVal::Undef);
        match public_id {
            None => false,
            Some(pid) => arr_includes_val(self.cfg("nameRevealPublicIds"), pid),
        }
    }

    /// `anonOffsetSeed(viewer)` — 0 / `simpleHash(viewer)` /
    /// `simpleHash(`${gameID}:team:${team}`)`.
    fn anon_offset_seed(&self, viewer: Option<&str>, trace: &mut Vec<f64>) -> f64 {
        let Some(v) = viewer else { return 0.0 };
        trace.push(21.0); // view.clients()
        let team = match self.client(v) {
            None => None,
            Some(_) => self.team_index(v, trace),
        };
        match team {
            None => simple_hash(v),
            Some(t) => {
                let s = format!("{}:team:{}", self.game_id, js_num_str(t));
                simple_hash(&s)
            }
        }
    }

    /// `anonName(viewer, target)` — join-order slot over `clients().keys()`.
    fn anon_name(&self, viewer: Option<&str>, target: &str, trace: &mut Vec<f64>) -> JsVal {
        trace.push(21.0); // view.clients()
        let mut slot = 0.0f64;
        for (id, _) in &self.clients {
            if id == target {
                break;
            }
            slot += 1.0;
        }
        let seed = self.anon_offset_seed(viewer, trace);
        anon_word_name(slot, Some(seed)).map_or(JsVal::Undef, JsVal::Str)
    }

    /// `sameMatchmadeTeam(viewer, target)`.
    fn same_matchmade_team(&self, viewer: Option<&str>, target: &str, trace: &mut Vec<f64>) -> bool {
        let Some(v) = viewer else { return false };
        trace.push(21.0); // view.clients()
        if self.client(v).is_none() || self.client(target).is_none() {
            return false;
        }
        let Some(vt) = self.team_index(v, trace) else {
            return false; // second teamIndex NOT called
        };
        let tt = self.team_index(target, trace);
        tt == Some(vt)
    }

    /// `seesRealBeyondTeam(viewer, target)`.
    fn sees_real_beyond_team(&self, viewer: Option<&str>, target: &str, trace: &mut Vec<f64>) -> bool {
        trace.push(20.0); // view.config()
        let anon = self.cfg("anonymizeNames").is_some_and(truthy);
        if !anon {
            return true;
        }
        if viewer == Some(target) {
            return true;
        }
        self.viewer_sees_all_names(viewer, trace)
    }

    /// `seesReal(viewer, target)`.
    fn sees_real(&self, viewer: Option<&str>, target: &str, trace: &mut Vec<f64>) -> bool {
        self.sees_real_beyond_team(viewer, target, trace)
            || self.same_matchmade_team(viewer, target, trace)
    }

    /// `startInfoFor(viewer, isAdmin, real, wire)`.
    fn start_info_for(
        &self,
        viewer: &str,
        is_admin: bool,
        real: &JsVal,
        wire: &JsVal,
        trace: &mut Vec<f64>,
    ) -> JsVal {
        trace.push(20.0); // config read ONCE at the top
        let reveal_clan_tags =
            is_admin && matches!(self.cfg("gameMode"), Some(JsVal::Str(m)) if m == "Free For All");
        let anon = self.cfg("anonymizeNames").is_some_and(truthy);
        if !anon {
            return if reveal_clan_tags { real.clone() } else { wire.clone() };
        }
        let real_players: Vec<JsVal> = match field(real, "players") {
            Some(JsVal::Arr(items)) => items.clone(),
            _ => Vec::new(),
        };
        let wire_players: Vec<JsVal> = match field(wire, "players") {
            Some(JsVal::Arr(items)) => items.clone(),
            _ => Vec::new(),
        };
        let mut out: Vec<(String, JsVal)> = match wire {
            JsVal::Obj(fields) => fields.clone(),
            _ => Vec::new(),
        };
        let players: Vec<JsVal> = wire_players
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let cid = match field(p, "clientID") {
                    Some(JsVal::Str(s)) => s.clone(),
                    _ => String::new(),
                };
                let sees = self.sees_real(Some(viewer), &cid, trace);
                let username = if sees {
                    field(p, "username").cloned().unwrap_or(JsVal::Undef)
                } else {
                    self.anon_name(Some(viewer), &cid, trace)
                };
                let clan_tag = if reveal_clan_tags {
                    real_players
                        .get(i)
                        .and_then(|rp| field(rp, "clanTag"))
                        .cloned()
                        .unwrap_or(JsVal::Undef)
                } else {
                    JsVal::Null
                };
                let mut np: Vec<(String, JsVal)> = match p {
                    JsVal::Obj(fields) => fields.clone(),
                    _ => Vec::new(),
                };
                map_set(&mut np, "username", username);
                map_set(&mut np, "clanTag", clan_tag);
                map_set(&mut np, "friends", JsVal::Undef);
                let cosmetics = if sees {
                    field(p, "cosmetics").cloned().unwrap_or(JsVal::Undef)
                } else {
                    JsVal::Undef
                };
                map_set(&mut np, "cosmetics", cosmetics);
                JsVal::Obj(np)
            })
            .collect();
        map_set(&mut out, "players", JsVal::Arr(players));
        JsVal::Obj(out)
    }

    /// `friendsLookup(active)` — the publicId -> clientID table (only
    /// truthy-publicId, non-spectator clients register).
    fn friends_table(active: &[JsVal]) -> Vec<(String, String)> {
        let mut m: Vec<(String, String)> = Vec::new();
        for c in active {
            let pid = field(c, "publicId");
            let spec = field(c, "spectator").is_some_and(truthy);
            if let Some(JsVal::Str(p)) = pid {
                if !p.is_empty() && !spec {
                    let cid = match field(c, "clientID") {
                        Some(JsVal::Str(s)) => s.clone(),
                        _ => String::new(),
                    };
                    map_set_str(&mut m, p, cid);
                }
            }
        }
        m
    }

    /// The `friendsLookup` closure applied to one client.
    fn friends_for(table: &[(String, String)], client: &JsVal) -> JsVal {
        let friends: Vec<JsVal> = match field(client, "friends") {
            Some(JsVal::Arr(items)) => items.clone(),
            _ => Vec::new(),
        };
        let ids: Vec<JsVal> = friends
            .iter()
            .filter_map(|f| match f {
                JsVal::Str(p) => table
                    .iter()
                    .find(|(k, _)| k == p)
                    .map(|(_, v)| JsVal::Str(v.clone())),
                _ => None,
            })
            .collect();
        if ids.is_empty() {
            JsVal::Undef
        } else {
            JsVal::Arr(ids)
        }
    }

    /// `lobbyClients(viewer, active)`.
    fn lobby_clients(&self, viewer: Option<&str>, active: &[JsVal], trace: &mut Vec<f64>) -> JsVal {
        let table = Self::friends_table(active);
        trace.push(20.0); // config read ONCE
        let hide_clan_tags = match self.cfg("disableClanTags") {
            Some(v) => truthy(v),
            None => false,
        };
        let anon = self.cfg("anonymizeNames").is_some_and(truthy);
        let rows: Vec<JsVal> = active
            .iter()
            .map(|c| {
                let cid = match field(c, "clientID") {
                    Some(JsVal::Str(s)) => s.clone(),
                    _ => String::new(),
                };
                if !self.sees_real(viewer, &cid, trace) {
                    let username = self.anon_name(viewer, &cid, trace);
                    let spectator = field(c, "spectator");
                    let team = self.team_index(&cid, trace);
                    let mut o: Vec<(String, JsVal)> =
                        vec![("username".to_string(), username)];
                    o.push(("clanTag".to_string(), JsVal::Null));
                    o.push(("clientID".to_string(), JsVal::Str(cid)));
                    o.push((
                        "spectator".to_string(),
                        match spectator {
                            Some(v) if truthy(v) => v.clone(),
                            _ => JsVal::Undef,
                        },
                    ));
                    o.push(("teamIndex".to_string(), team.map_or(JsVal::Undef, JsVal::Num)));
                    return JsVal::Obj(o);
                }
                // Teammate-only reveal: cached config.anonymizeNames (no
                // extra config() trace), short-circuits when not anonymizing.
                let teammate_only = anon && !self.sees_real_beyond_team(viewer, &cid, trace);
                let clan_tag = if teammate_only || hide_clan_tags {
                    JsVal::Null
                } else {
                    match field(c, "clanTag") {
                        Some(v) => v.clone(),
                        None => JsVal::Null,
                    }
                };
                let friends = if teammate_only {
                    JsVal::Undef
                } else {
                    Self::friends_for(&table, c)
                };
                let verified = match field(c, "cosmetics") {
                    Some(cos) => field(cos, "verified").cloned().unwrap_or(JsVal::Undef),
                    None => JsVal::Undef,
                };
                let spectator = field(c, "spectator");
                let team = self.team_index(&cid, trace);
                let mut o: Vec<(String, JsVal)> = vec![(
                    "username".to_string(),
                    field(c, "username").cloned().unwrap_or(JsVal::Undef),
                )];
                o.push(("clanTag".to_string(), clan_tag));
                o.push(("clientID".to_string(), JsVal::Str(cid)));
                o.push(("friends".to_string(), friends));
                o.push(("verified".to_string(), verified));
                o.push((
                    "spectator".to_string(),
                    match spectator {
                        Some(v) if truthy(v) => v.clone(),
                        _ => JsVal::Undef,
                    },
                ));
                o.push(("teamIndex".to_string(), team.map_or(JsVal::Undef, JsVal::Num)));
                JsVal::Obj(o)
            })
            .collect();
        JsVal::Arr(rows)
    }
}

/// `Map<string, string>` set: existing key overwritten in place, new key
/// appended.
fn map_set_str(m: &mut Vec<(String, String)>, key: &str, v: String) {
    if let Some(slot) = m.iter_mut().find(|(k, _)| k == key) {
        slot.1 = v;
    } else {
        m.push((key.to_string(), v));
    }
}

/// The capture harness: one `NameVisibility` over the scripted facade,
/// replaying an op stream. Every facade call is a trace event in the res
/// stream: `20` = `config()`, `21` = `clients()`, `22` = `teamIndex`
/// (`[22, (clientID-str), val]`).
#[derive(Debug, Default)]
pub struct RigHarness {
    nv: NameVisibility,
}

impl RigHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Run one op. Kind table (see `tools/gen_vectors.mjs`):
    /// 0 construct/reset `[(gameID-str)]` -> `[0]`;
    /// 1 setConfig `[n,(key,value)*n]` -> `[0]`;
    /// 2 addClient `[(clientID-str), stub]` -> `[0]` (JS `Map.set` order);
    /// 3 setTeam `[(clientID-str), val]` -> `[0]`;
    /// 10 viewerSeesAllNames `[viewer]` -> `[0|1]`;
    /// 11 anonName `[viewer, target]` -> `val(string|undefined)`;
    /// 12 anonOffsetSeed `[viewer]` -> `[value]`;
    /// 13 sameMatchmadeTeam `[viewer, target]` -> `[0|1]`;
    /// 14 seesRealBeyondTeam `[viewer, target]` -> `[0|1]`;
    /// 15 seesReal `[viewer, target]` -> `[0|1]`;
    /// 16 startInfoFor `[viewer, isAdmin 0|1, real, wire]` -> `val`;
    /// 17 lobbyClients `[viewer, active-arr]` -> `val`;
    /// 18 friendsLookup `[active-arr]` -> `[n,(val)*n]`.
    /// Optional `viewer` crosses as codec `undefined`. All method results
    /// ride as `[traceLen, (trace)*, payload*]`.
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut i = 0usize;
        let mut trace: Vec<f64> = Vec::new();
        let payload: Vec<f64> = match kind {
            0 => {
                self.reset();
                self.nv.game_id = read_str(args, &mut i);
                vec![0.0]
            }
            1 => {
                self.nv.config = read_map(args, &mut i);
                vec![0.0]
            }
            2 => {
                let id = read_str(args, &mut i);
                let stub = read_val(args, &mut i);
                map_set(&mut self.nv.clients, &id, stub);
                vec![0.0]
            }
            3 => {
                let id = read_str(args, &mut i);
                let v = read_val(args, &mut i);
                map_set(&mut self.nv.teams, &id, v);
                vec![0.0]
            }
            10 => {
                let viewer = read_val(args, &mut i);
                let v = as_opt_str(&viewer);
                vec![if self.nv.viewer_sees_all_names(v, &mut trace) { 1.0 } else { 0.0 }]
            }
            11 => {
                let viewer = read_val(args, &mut i);
                let target = read_val(args, &mut i);
                let t = as_str(&target);
                let mut out = Vec::new();
                push_val(&mut out, &self.nv.anon_name(as_opt_str(&viewer), t, &mut trace));
                out
            }
            12 => {
                let viewer = read_val(args, &mut i);
                vec![self.nv.anon_offset_seed(as_opt_str(&viewer), &mut trace)]
            }
            13 => {
                let viewer = read_val(args, &mut i);
                let target = read_val(args, &mut i);
                let t = as_str(&target);
                vec![if self.nv.same_matchmade_team(as_opt_str(&viewer), t, &mut trace) { 1.0 } else { 0.0 }]
            }
            14 => {
                let viewer = read_val(args, &mut i);
                let target = read_val(args, &mut i);
                let t = as_str(&target);
                vec![if self.nv.sees_real_beyond_team(as_opt_str(&viewer), t, &mut trace) { 1.0 } else { 0.0 }]
            }
            15 => {
                let viewer = read_val(args, &mut i);
                let target = read_val(args, &mut i);
                let t = as_str(&target);
                vec![if self.nv.sees_real(as_opt_str(&viewer), t, &mut trace) { 1.0 } else { 0.0 }]
            }
            16 => {
                let viewer = read_val(args, &mut i);
                let is_admin = args[i] != 0.0;
                i += 1;
                let real = read_val(args, &mut i);
                let wire = read_val(args, &mut i);
                let v = as_str(&viewer).to_string();
                let r = self.nv.start_info_for(&v, is_admin, &real, &wire, &mut trace);
                let mut out = Vec::new();
                push_val(&mut out, &r);
                out
            }
            17 => {
                let viewer = read_val(args, &mut i);
                let active = read_val(args, &mut i);
                let items: Vec<JsVal> = match &active {
                    JsVal::Arr(xs) => xs.clone(),
                    _ => Vec::new(),
                };
                let r = self.nv.lobby_clients(as_opt_str(&viewer), &items, &mut trace);
                let mut out = Vec::new();
                push_val(&mut out, &r);
                out
            }
            18 => {
                let active = read_val(args, &mut i);
                let items: Vec<JsVal> = match &active {
                    JsVal::Arr(xs) => xs.clone(),
                    _ => Vec::new(),
                };
                let table = NameVisibility::friends_table(&items);
                let mut out = vec![items.len() as f64];
                for c in &items {
                    push_val(&mut out, &NameVisibility::friends_for(&table, c));
                }
                out
            }
            k => unreachable!("name_visibility harness: unknown op kind {k}"),
        };
        let mut out = Vec::with_capacity(1 + trace.len() + payload.len());
        out.push(trace.len() as f64);
        out.extend(trace.iter().copied());
        out.extend(payload);
        out
    }
}

fn as_str(v: &JsVal) -> &str {
    match v {
        JsVal::Str(s) => s,
        _ => "",
    }
}

fn as_opt_str(v: &JsVal) -> Option<&str> {
    match v {
        JsVal::Str(s) => Some(s),
        _ => None,
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
    fn sv(x: &str) -> JsVal {
        JsVal::Str(x.to_string())
    }
    fn kv(k: &str, v: JsVal) -> (String, JsVal) {
        (k.to_string(), v)
    }
    fn map_tokens(m: &[(String, JsVal)]) -> Vec<f64> {
        let mut a = vec![m.len() as f64];
        for (k, v) in m {
            push_str(&mut a, k);
            push_val(&mut a, v);
        }
        a
    }

    #[test]
    fn viewer_undef_no_trace() {
        let mut h = RigHarness::new();
        h.run_op(0, &s("g"));
        let mut a = vec![1.0]; // undefined viewer
        a.push(5.0);
        a.extend(s("t"));
        let r = h.run_op(11, &a); // anonName(undefined, "t") on empty map
        assert_eq!(r[0], 1.0); // one clients() trace
        assert_eq!(r[1], 21.0);
    }

    #[test]
    fn reveals_hit_skips_clients_lookup() {
        let mut h = RigHarness::new();
        h.run_op(0, &s("g"));
        h.run_op(
            1,
            &map_tokens(&[kv("nameReveals", JsVal::Arr(vec![sv("v")]))]),
        );
        let mut a = vec![5.0];
        a.extend(s("v"));
        let r = h.run_op(10, &a);
        assert_eq!(r, vec![1.0, 20.0, 1.0]); // config() only, true
    }

    #[test]
    fn seed_team_interpolation() {
        let mut h = RigHarness::new();
        h.run_op(0, &s("g"));
        let mut a = s("v");
        let mut stub = Vec::new();
        push_val(&mut stub, &JsVal::Obj(vec![kv("clientID", sv("v"))]));
        a.extend(stub);
        h.run_op(2, &a); // addClient v
        let mut t = s("v");
        push_val(&mut t, &JsVal::Num(0.0));
        h.run_op(3, &t);
        let mut b = vec![5.0];
        b.extend(s("v"));
        let r = h.run_op(12, &b);
        // seed = simpleHash("g:team:0")
        assert_eq!(r[r.len() - 1], simple_hash("g:team:0"));
    }

    #[test]
    fn same_team_second_index_short_circuit() {
        let mut h = RigHarness::new();
        h.run_op(0, &s("g"));
        let add = |h: &mut RigHarness, id: &str| {
            let mut a = s(id);
            let mut stub = Vec::new();
            push_val(&mut stub, &JsVal::Obj(vec![kv("clientID", sv(id))]));
            a.extend(stub);
            h.run_op(2, &a);
        };
        add(&mut h, "v");
        add(&mut h, "t");
        // v has NO team: only one teamIndex call.
        let mut a = vec![5.0];
        a.extend(s("v"));
        a.push(5.0);
        a.extend(s("t"));
        let r = h.run_op(13, &a);
        let n_team = r[1..1 + r[0] as usize].iter().filter(|&&x| x == 22.0).count();
        assert_eq!(n_team, 1);
        assert_eq!(*r.last().unwrap(), 0.0);
    }
}
