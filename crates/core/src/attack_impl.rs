//! Bit-exact port of `src/core/game/AttackImpl.ts`: an attack between an
//! attacker player and a target (`Player | TerraNullius`). The `Attack`
//! struct replicates every private field; construction and every method run
//! as a `kind` of [`AttackHarness::run_op`] over the `js_json` codec
//! (precedent `unit_impl` / `alliance_impl`). The `mg` (GameImpl) surface is
//! a scripted facade whose calls are pinned into a flat trace; the players'
//! `_incomingAttacks` / `_outgoingAttacks` are REAL harness arrays with
//! reference-identity token semantics (the `ui_` dumpOwner convention
//! extended: `0` = the attack under test, `k > 0` = other attacks — G3b3
//! reuses this identity model).
//!
//! Ported surface (TS line anchors): the field block (7-10), the ctor
//! (12-20), `sourceTile` (22), `target` / `attacker` (26-31), `troops` /
//! `setTroops` (32-37), `isActive` (39), `id` (43), `delete` (47-59),
//! `orderRetreat` / `executeRetreat` / `retreating` / `retreated` (61-75),
//! `borderSize` / `clearBorder` / `addBorderTile` / `removeBorderTile`
//! (77-98), `clusteredPositions` (102-108) and the private
//! `clusterBorderTiles` (119-184) — exposed as a direct op (JS `private` is
//! compile-time only; the capture calls it to pin the switch / filter /
//! slice boundaries).
//!
//! Facade (mock) surface — the capture scripts these, every call traced:
//!
//! * `player.isPlayer()` (97, keyed by pid — the `delete` gate),
//!   `mg.map()` (98 — returns the REAL ported `GameMap`; the scenario's
//!   `w,h` construct it on both sides, the `x` / `y` reads are real, NOT
//!   traced, same precedent as `unit_grid`).
//! * `_mg.forEachNeighborWithDiag(t, cb)` is NOT a trace event: GameImpl
//!   delegates verbatim to `GameMapImpl.forEachNeighborWithDiag`, which is
//!   already bit-ported in `game_map.rs` (`for_each_neighbor_with_diag`);
//!   the Rust side replays the callback over the real map, so the neighbor
//!   order is pinned by construction (the callback body only touches harness
//!   state — border / visited / queue — nothing observable to trace).
//! * `target` / `attacker` cross as player TOKENS (indices into the harness
//!   `players` table); `target()` / `attacker()` return the token.
//!
//! Faithfulness notes:
//!
//! * The ctor does NOT derive `_borderSize` from the border `Set` — a fresh
//!   attack over a non-empty set has `borderSize() === 0` and
//!   `clusteredPositions()` returns `[sourceTile]` (or `[]` when the tile is
//!   `null` — the gate is `!== null`, so `undefined` yields `[undefined]`).
//! * `setTroops`: `_troops = Math.max(0, troops)` over the ToNumber coercion
//!   of the raw arg (`js_max` NaN / -0 semantics); the ctor stores the raw
//!   JsVal (typed `number` but JS keeps whatever crossed).
//! * `delete`: the `_target.isPlayer()` facade (97) fires FIRST; the two
//!   `filter(a => a !== this)` passes are identity filters modelled as
//!   `retain(token != 0)` — the target's incoming array is filtered ONLY on
//!   the true branch, the attacker's outgoing array ALWAYS, then
//!   `_isActive = false`.
//! * The border `Set` keeps JS insertion order + SameValueZero (`+0`/`-0`
//!   same key, `NaN` equals `NaN`) — `clusterBorderTiles` iterates it in
//!   that order, and the BFS start order is observable through the cluster
//!   representative tie-breaks.
//! * `clusterBorderTiles`: centroid `sumX / count` is f64 division; the
//!   best-tile scan uses strict `<` from an `Infinity` start (first minimum
//!   wins ties); the sort is the subtraction comparator `b.size - a.size`
//!   over a stable TimSort (V8 `Array#sort` ≡ Rust `sort_by` — the
//!   `boot_interrupts` model); `switch` 0 / 1 / default + `filter(size >=
//!   minSize)` (NaN minSize drops everything → largest-cluster fallback) +
//!   `slice(0, maxClusters)`.

use crate::game_map::GameMap;
use crate::js_json::{push_val, read_str, read_val, JsVal};
use crate::jsnum::js_max;
use std::cmp::Ordering;

/// FIFO script consumption with the capture's exhausted-repeat rule (the
/// `ui_` rule; event codes 90-98 are documented in `alliance_impl`, the
/// attack harness uses 97 isPlayer and 98 map).
fn take_f64(list: &[f64], i: &mut usize) -> f64 {
    let idx = (*i).min(list.len() - 1);
    *i += 1;
    list[idx]
}

/// JS `ToNumber` over the codec domain (the `unit_impl` helper).
fn to_num(v: &JsVal) -> f64 {
    match v {
        JsVal::Num(n) => *n,
        JsVal::Bool(b) => if *b { 1.0 } else { 0.0 },
        JsVal::Null => 0.0,
        JsVal::Str(s) => crate::game_config_helpers::js_number(s),
        JsVal::Absent | JsVal::Undef => f64::NAN,
        JsVal::Obj(_) | JsVal::Arr(_) => f64::NAN,
    }
}

/// Arg cursor over the flat f64 token stream.
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
    fn s(&mut self) -> String {
        read_str(self.0, &mut self.1)
    }
    fn list(&mut self) -> Vec<f64> {
        let n = self.u();
        (0..n).map(|_| self.f()).collect()
    }
    fn val(&mut self) -> JsVal {
        read_val(self.0, &mut self.1)
    }
}

// ---- insertion-ordered JS `Set<number>` (SameValueZero) ---------------------

fn svz(a: f64, b: f64) -> bool {
    a == b || (a.is_nan() && b.is_nan())
}
fn set_has(s: &[f64], x: f64) -> bool {
    s.iter().any(|&v| svz(v, x))
}
fn set_del(s: &mut Vec<f64>, x: f64) {
    if let Some(i) = s.iter().position(|&v| svz(v, x)) {
        s.remove(i);
    }
}

/// One scripted player mock: pid, the `isPlayer` script, and the REAL
/// `_incomingAttacks` / `_outgoingAttacks` token arrays (`0` = the attack
/// under test, `k > 0` = opaque other attacks).
#[derive(Clone, Debug)]
struct AkPlayer {
    pid: f64,
    is_player: Vec<f64>,
    ip_i: usize,
    incoming: Vec<f64>,
    outgoing: Vec<f64>,
}

/// The `AttackImpl` private field block, replicated 1:1.
#[derive(Clone, Debug, Default)]
struct Attack {
    active: bool,
    border_size: f64,
    retreating: bool,
    retreated: bool,
    id: String,
    target_tok: usize,
    attacker_tok: usize,
    troops: JsVal,
    source_tile: JsVal,
    border: Vec<f64>,
}

/// Success payload `[0, ...encVal(v)]`.
fn push_ok(out: &mut Vec<f64>, v: &JsVal) {
    out.push(0.0);
    push_val(out, v);
}

fn push_ok_num(out: &mut Vec<f64>, v: f64) {
    push_ok(out, &JsVal::Num(v));
}

fn push_ok_bool(out: &mut Vec<f64>, v: bool) {
    push_ok(out, &JsVal::Bool(v));
}

#[derive(Default)]
pub struct AttackHarness {
    a: Attack,
    players: Vec<AkPlayer>,
    map: Option<GameMap>,
}

impl AttackHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    // --- facade plumbing ---

    fn player_is_player(&mut self, tr: &mut Vec<f64>, tok: usize) -> bool {
        let p = &mut self.players[tok];
        let v = take_f64(&p.is_player, &mut p.ip_i);
        tr.push(97.0);
        tr.push(p.pid);
        tr.push(v);
        // JS `v !== 0`: NaN passes the strict gate (NaN !== 0 is true).
        v != 0.0
    }

    fn mg_map(&mut self, tr: &mut Vec<f64>) {
        tr.push(98.0);
    }

    // --- clusterBorderTiles (AttackImpl.ts:119-184) ---

    /// The private BFS clustering, replayed over the REAL ported `GameMap`.
    /// Returns the representative tile list (the TS `TileRef[]` domain —
    /// plain numbers).
    fn cluster_border_tiles(&self, min_size: f64, max_clusters: f64) -> Vec<f64> {
        let map = self.map.as_ref().expect("attack harness: map not constructed");
        let mut visited: Vec<f64> = Vec::new();
        let mut clusters: Vec<(f64, f64)> = Vec::new(); // (tile, size)

        for &start_tile in &self.a.border {
            if set_has(&visited, start_tile) {
                continue;
            }
            let mut queue: Vec<f64> = vec![start_tile];
            visited.push(start_tile);
            let mut qi = 0usize;
            let mut sum_x = 0.0;
            let mut sum_y = 0.0;
            let mut count = 0.0;
            let border = &self.a.border;
            while qi < queue.len() {
                let t = queue[qi];
                qi += 1;
                sum_x += map.x(t);
                sum_y += map.y(t);
                count += 1.0;
                map.for_each_neighbor_with_diag(t, |neighbor| {
                    if set_has(border, neighbor) && !set_has(&visited, neighbor) {
                        visited.push(neighbor);
                        queue.push(neighbor);
                    }
                });
            }
            let cx = sum_x / count;
            let cy = sum_y / count;
            let mut best = queue[0];
            let mut best_dist = f64::INFINITY;
            for &t in &queue {
                let dx = map.x(t) - cx;
                let dy = map.y(t) - cy;
                let dist = dx * dx + dy * dy;
                if dist < best_dist {
                    best_dist = dist;
                    best = t;
                }
            }
            clusters.push((best, count));
        }

        // `clusters.sort((a, b) => b.size - a.size)` — subtraction comparator
        // over a stable sort (the `boot_interrupts` model; sizes are finite
        // integers here, so NaN never enters the comparator).
        clusters.sort_by(|a, b| {
            let d = b.1 - a.1;
            if d < 0.0 {
                Ordering::Less
            } else if d > 0.0 {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        });

        match clusters.len() {
            0 => Vec::new(),
            1 => vec![clusters[0].0],
            _ => {
                let significant: Vec<(f64, f64)> = clusters
                    .iter()
                    .cloned()
                    .filter(|c| c.1 >= min_size)
                    .collect();
                if significant.is_empty() {
                    vec![clusters[0].0]
                } else {
                    // `slice(0, maxClusters)` — JS relative-end semantics:
                    // NaN / -Infinity -> 0, negative -> len + end clamped to
                    // 0, fractional -> trunc, +Infinity / past-the-end -> len.
                    let len = significant.len() as f64;
                    let end = if max_clusters.is_nan() || max_clusters == f64::NEG_INFINITY {
                        0.0
                    } else if max_clusters < 0.0 {
                        (len + max_clusters.trunc()).max(0.0)
                    } else {
                        max_clusters.trunc().min(len)
                    };
                    significant.iter().take(end as usize).map(|c| c.0).collect()
                }
            }
        }
    }

    // --- the op dispatcher ---

    /// Run one op. Kind table (mirrored by `tools/gen_vectors.mjs`):
    /// 0 construct `[encS id, targetTok, attackerTok, encVal troops, encVal
    /// sourceTile, nBorder, (tiles)*, w, h, playersBlock]` -> `[0]` (the ctor
    /// has no facade calls; the real `GameMap(w,h)` is built here); kind 1
    /// method `[mid,...]` -> `[traceLen,(trace)*,[0,...encVal]]`. mid follows
    /// the TS declaration order: 0 sourceTile 1 target 2 attacker 3 troops
    /// 4 setTroops 5 isActive 6 id 7 delete 8 orderRetreat 9 executeRetreat
    /// 10 retreating 11 retreated 12 borderSize 13 clearBorder
    /// 14 addBorderTile 15 removeBorderTile 16 clusteredPositions
    /// 17 clusterBorderTiles [minSize,maxClusters] 18 dumpPlayers [idx].
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut c = Cur(args, 0);
        let mut tr: Vec<f64> = Vec::new();
        let mut out: Vec<f64> = Vec::new();
        match kind {
            0 => {
                self.reset();
                let id = c.s();
                let target_tok = c.u();
                let attacker_tok = c.u();
                let troops = c.val();
                let source_tile = c.val();
                let border = c.list();
                let w = c.f();
                let h = c.f();
                let n = c.u();
                let players = (0..n)
                    .map(|_| {
                        let pid = c.f();
                        let is_player = c.list();
                        let incoming = c.list();
                        let outgoing = c.list();
                        AkPlayer {
                            pid,
                            is_player,
                            ip_i: 0,
                            incoming,
                            outgoing,
                        }
                    })
                    .collect();
                self.players = players;
                // The scenario map: all-land terrain (the x/y + neighbor math
                // is index-only; the unit_grid precedent fills 0x85).
                self.map = Some(GameMap::new(
                    w,
                    h,
                    vec![0x85u8; (w * h) as usize],
                    w * h,
                ));
                self.a = Attack {
                    active: true,
                    border_size: 0.0,
                    retreating: false,
                    retreated: false,
                    id,
                    target_tok,
                    attacker_tok,
                    troops,
                    source_tile,
                    border,
                };
                out.push(tr.len() as f64);
                out.extend(tr.iter().copied());
                out.push(0.0);
                out
            }
            1 => {
                let mid = c.u() as u8;
                match mid {
                    0 => push_ok(&mut out, &self.a.source_tile.clone()),
                    1 => push_ok_num(&mut out, self.a.target_tok as f64),
                    2 => push_ok_num(&mut out, self.a.attacker_tok as f64),
                    3 => push_ok(&mut out, &self.a.troops.clone()),
                    4 => {
                        // setTroops: Math.max(0, ToNumber(troops))
                        let t = c.val();
                        self.a.troops = JsVal::Num(js_max(0.0, to_num(&t)));
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    5 => push_ok_bool(&mut out, self.a.active),
                    6 => push_ok(&mut out, &JsVal::Str(self.a.id.clone())),
                    7 => {
                        // delete(): isPlayer facade FIRST, then the two
                        // identity filters, then _isActive = false.
                        let ip = self.player_is_player(&mut tr, self.a.target_tok);
                        if ip {
                            let tok = self.a.target_tok;
                            self.players[tok].incoming.retain(|&t| t != 0.0);
                        }
                        let atok = self.a.attacker_tok;
                        self.players[atok].outgoing.retain(|&t| t != 0.0);
                        self.a.active = false;
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    8 => {
                        self.a.retreating = true;
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    9 => {
                        self.a.retreated = true;
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    10 => push_ok_bool(&mut out, self.a.retreating),
                    11 => push_ok_bool(&mut out, self.a.retreated),
                    12 => push_ok_num(&mut out, self.a.border_size),
                    13 => {
                        self.a.border_size = 0.0;
                        self.a.border.clear();
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    14 => {
                        // addBorderTile: !has gate -> size += 1, add
                        let tile = c.f();
                        if !set_has(&self.a.border, tile) {
                            self.a.border_size += 1.0;
                            self.a.border.push(tile);
                        }
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    15 => {
                        // removeBorderTile: has gate -> size -= 1, delete
                        let tile = c.f();
                        if set_has(&self.a.border, tile) {
                            self.a.border_size -= 1.0;
                            set_del(&mut self.a.border, tile);
                        }
                        push_ok(&mut out, &JsVal::Undef);
                    }
                    16 => {
                        // clusteredPositions
                        if self.a.border_size == 0.0 {
                            let t = self.a.source_tile.clone();
                            let arr: Vec<JsVal> = if t == JsVal::Null {
                                Vec::new()
                            } else {
                                vec![t]
                            };
                            push_ok(&mut out, &JsVal::Arr(arr));
                        } else {
                            self.mg_map(&mut tr);
                            let tiles = self.cluster_border_tiles(30.0, 2.0);
                            push_ok(
                                &mut out,
                                &JsVal::Arr(tiles.into_iter().map(JsVal::Num).collect()),
                            );
                        }
                    }
                    17 => {
                        // clusterBorderTiles (private; direct op)
                        let min_size = c.f();
                        let max_clusters = c.f();
                        self.mg_map(&mut tr);
                        let tiles = self.cluster_border_tiles(min_size, max_clusters);
                        push_ok(
                            &mut out,
                            &JsVal::Arr(tiles.into_iter().map(JsVal::Num).collect()),
                        );
                    }
                    18 => {
                        // dumpPlayers [idx] -> [0,pid,nIn,(toks)*,nOut,(toks)*]
                        let idx = c.u();
                        let p = &self.players[idx];
                        out.push(0.0);
                        out.push(p.pid);
                        out.push(p.incoming.len() as f64);
                        out.extend(p.incoming.iter().copied());
                        out.push(p.outgoing.len() as f64);
                        out.extend(p.outgoing.iter().copied());
                    }
                    k => unreachable!("attack harness: unknown method id {k}"),
                }
                let mut full = Vec::with_capacity(1 + tr.len() + out.len());
                full.push(tr.len() as f64);
                full.extend(tr.iter().copied());
                full.extend(out.iter().copied());
                full
            }
            k => unreachable!("attack harness: unknown op kind {k}"),
        }
    }
}
