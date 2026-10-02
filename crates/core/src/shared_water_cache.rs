//! Port of `src/core/execution/nation/SharedWaterCache.ts`: the nation-AI
//! "which water bodies does each nation share with a valid trade partner"
//! cache (TTL-rebuilt `byPlayer` map + per-player `playerWater` rescan cache).
//! The `Game` / `Player` facades are scripted mocks (precedent:
//! `nation_utils`): the capture feeds the real TS code hand-scripted returns
//! and records every facade call (method tag + arguments + return) into a flat
//! trace, so the TTL rebuild condition, the `this.tick = tick` (not `++`)
//! same-tick cache hit, the strict-`===` (NaN never hits) waterFor double
//! version check, the border/shore/neighbor visit order, the `comp !== null`
//! lake add, the `Set` insertion order (the `OCEAN_SENTINEL` first when
//! `hasOcean`), the `lakePartners` array order, the `other !== player`
//! self-exclusion and the first-valid-partner `break` (pinning the `canTrade`
//! call count) are all pinned in the res stream; the port replays the same
//! logic over the same token stream.
//!
//! Facade modelling notes:
//!
//! * `game.map().waterVersion()` is a two-level facade; the intermediate
//!   `map()` object has no observable behaviour, so the pair collapses into
//!   the single trace event `[1, waterVersion]`.
//! * `game.forEachNeighbor(tile, visit)` is scripted as a neighbor list; the
//!   list itself rides in the trace event `[7, tile, n, (nbr)*]` and each
//!   `visit` call's inner facade events follow in script order.
//! * `getWaterComponent` returns `null` or a component id; the mock script
//!   only ever emits those two shapes, so the JS `comp !== null` quirk (an
//!   `undefined` would pass the check) stays outside the captured domain.
//! * Player identity is the capture `refid`; `other !== player` compares the
//!   refids with SameValueZero (`+0` / `-0` collapse, [`svz_key`] precedent
//!   from `unit_grid`).
//!
//! Faithfulness notes:
//!
//! * `tick - this.tick >= TTL_TICKS` starts from `tick = -Infinity`, so the
//!   first `get` always rebuilds (`100 - -Inf = +Inf >= 30`). A tick of
//!   `Infinity` would make the difference `NaN` (false, no rebuild) — outside
//!   the captured domain. After a rebuild `this.tick = tick` (not `++`), so a
//!   second `get` at the same tick finds a 0 difference and skips the rebuild
//!   (the `ticks()` call still happens and is traced).
//! * `waterFor` hit condition is JS `===` on both versions: Rust `f64 ==`
//!   matches it exactly (`NaN == NaN` false, `-0 == 0` true), so a NaN
//!   `tileChangeVersion` never hits and rescans on every rebuild.
//! * The `playerToWater` entry destructured from `waterFor` shares the very
//!   same `lakes` `Set` reference as the `playerWater` entry. Nothing mutates
//!   a stored `lakes` after construction (a rescan builds a fresh `Set`), and
//!   the capture only dumps (never mutates) the returned sets, so the port
//!   clones the insertion-ordered values instead of threading a shared
//!   reference — observationally identical.
//! * `lakes.add(comp)` keeps first-insertion position on a duplicate add
//!   ([`RefSet`] precedent from `unit_grid`); `playerWater.set` on an existing
//!   refid overwrites the entry in place (JS `Map` key order preserved).
//! * `lakePartners` is a `Map<number, Player[]>`: a new component key appends,
//!   an existing key pushes to the tail, and pass 2 reads the array in player
//!   join order (= `players()` iteration order).
//! * Pass 2 `break`s on the first valid partner per lake, so the number of
//!   `canTrade` calls per player is pinned by the trace.
//! * `shared.size > 0 ? shared : null` stores `null` under the player key (the
//!   key stays present), and `byPlayer.get(player) ?? null` maps a missing key
//!   to `null` as well — both dump as `[0]`.
//! * `build()` reads `game.map().waterVersion()` once before the pass-1 loop.

use std::collections::HashMap;

/// `TTL_TICKS = 30`.
const TTL_TICKS: f64 = 30.0;

/// `OCEAN_SENTINEL = -1`.
const OCEAN_SENTINEL: f64 = -1.0;

/// `PlayerType.Bot` (Game.ts string enum, value `"BOT"`).
const PLAYER_TYPE_BOT: [u16; 3] = [66, 79, 84];

/// SameValueZero key for a JS number riding as a capture refid / component id
/// (`+0` / `-0` collapse, matching `Map` / `Set` keying).
fn svz_key(v: f64) -> u64 {
    if v == 0.0 {
        0.0f64.to_bits()
    } else {
        v.to_bits()
    }
}

/// A JS `Set<number>` with insertion-order iteration: `add` of a present
/// member is a no-op that does not move it.
#[derive(Debug, Default, Clone)]
pub(crate) struct RefSet {
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
    fn size(&self) -> usize {
        self.vals.len()
    }
    fn iter(&self) -> impl Iterator<Item = f64> + '_ {
        self.vals.iter().copied()
    }
}

/// A JS `Map` keyed by a number / refid with SameValueZero lookup, insertion
/// order and in-place overwrite on an existing key.
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
    fn get_mut_by(&mut self, k: f64) -> Option<&mut V> {
        let kk = svz_key(k);
        self.entries
            .iter_mut()
            .find(|(x, _)| svz_key(*x) == kk)
            .map(|(_, v)| v)
    }
    fn size(&self) -> usize {
        self.entries.len()
    }
}

/// The `PlayerWater` interface: the pass-1 result per player, reused while
/// that player's border and the map's water are unchanged.
#[derive(Debug, Default, Clone)]
struct PlayerWater {
    tile_version: f64,
    water_version: f64,
    has_ocean: bool,
    lakes: RefSet,
}

/// One scripted `Player` mock.
#[derive(Debug)]
struct PlayerSpec {
    pid: f64,
    typ: Vec<u16>,
    tile_version: f64,
    border: Vec<f64>,
    /// `canTrade(other)` script: `(other refid, 0|1)` pairs.
    trades: Vec<(f64, f64)>,
    in_players: bool,
}

/// The scripted `Game` mock: tick / waterVersion / tile-predicate streams and
/// the player list, each facade call appending its trace event.
pub(crate) struct Facade {
    ticks: Vec<f64>,
    ti: usize,
    water_version: f64,
    specs: Vec<PlayerSpec>,
    shore: Vec<(f64, f64)>,
    nbrs: Vec<(f64, Vec<f64>)>,
    water: Vec<(f64, f64)>,
    ocean: Vec<(f64, f64)>,
    comp: Vec<(f64, Option<f64>)>,
}

impl Facade {
    fn ticks(&mut self, trace: &mut Vec<f64>) -> f64 {
        let r = self.ticks[self.ti];
        self.ti += 1;
        trace.push(0.0);
        trace.push(r);
        r
    }
    /// `game.map().waterVersion()` collapsed into one event.
    fn water_version(&mut self, trace: &mut Vec<f64>) -> f64 {
        trace.push(1.0);
        trace.push(self.water_version);
        self.water_version
    }
    fn players(&self, trace: &mut Vec<f64>) -> Vec<usize> {
        let order: Vec<usize> =
            (0..self.specs.len()).filter(|&i| self.specs[i].in_players).collect();
        trace.push(11.0);
        trace.push(order.len() as f64);
        for &i in &order {
            trace.push(self.specs[i].pid);
        }
        order
    }
    fn type_of(&self, i: usize, trace: &mut Vec<f64>) -> Vec<u16> {
        let s = &self.specs[i];
        trace.push(4.0);
        trace.push(s.pid);
        trace.push(s.typ.len() as f64);
        for &u in &s.typ {
            trace.push(f64::from(u));
        }
        s.typ.clone()
    }
    fn tile_change_version(&self, i: usize, trace: &mut Vec<f64>) -> f64 {
        let s = &self.specs[i];
        trace.push(3.0);
        trace.push(s.pid);
        trace.push(s.tile_version);
        s.tile_version
    }
    fn border_tiles(&self, i: usize, trace: &mut Vec<f64>) -> Vec<f64> {
        let s = &self.specs[i];
        trace.push(2.0);
        trace.push(s.pid);
        trace.push(s.border.len() as f64);
        trace.extend(s.border.iter().copied());
        s.border.clone()
    }
    fn can_trade(&self, i: usize, other: f64, trace: &mut Vec<f64>) -> bool {
        let s = &self.specs[i];
        let r = s
            .trades
            .iter()
            .find(|(o, _)| svz_key(*o) == svz_key(other))
            .map(|(_, v)| *v)
            .expect("swc: missing trade script");
        trace.push(5.0);
        trace.push(s.pid);
        trace.push(other);
        trace.push(r);
        r == 1.0
    }
    fn is_shore(&self, t: f64, trace: &mut Vec<f64>) -> bool {
        let r = lookup(&self.shore, t);
        trace.push(6.0);
        trace.push(t);
        trace.push(r);
        r == 1.0
    }
    fn for_each_neighbor(&self, t: f64, trace: &mut Vec<f64>) -> Vec<f64> {
        let ns = self
            .nbrs
            .iter()
            .find(|(x, _)| svz_key(*x) == svz_key(t))
            .map(|(_, v)| v.clone())
            .expect("swc: missing neighbors script");
        trace.push(7.0);
        trace.push(t);
        trace.push(ns.len() as f64);
        trace.extend(ns.iter().copied());
        ns
    }
    fn is_water(&self, t: f64, trace: &mut Vec<f64>) -> bool {
        let r = lookup(&self.water, t);
        trace.push(8.0);
        trace.push(t);
        trace.push(r);
        r == 1.0
    }
    fn is_ocean(&self, t: f64, trace: &mut Vec<f64>) -> bool {
        let r = lookup(&self.ocean, t);
        trace.push(9.0);
        trace.push(t);
        trace.push(r);
        r == 1.0
    }
    fn get_water_component(&self, t: f64, trace: &mut Vec<f64>) -> Option<f64> {
        let c = self
            .comp
            .iter()
            .find(|(x, _)| svz_key(*x) == svz_key(t))
            .map(|(_, v)| *v)
            .expect("swc: missing component script");
        trace.push(10.0);
        trace.push(t);
        match c {
            None => trace.push(0.0),
            Some(v) => {
                trace.push(1.0);
                trace.push(v);
            }
        }
        c
    }
}

fn lookup(v: &[(f64, f64)], k: f64) -> f64 {
    v.iter()
        .find(|(x, _)| svz_key(*x) == svz_key(k))
        .map(|(_, r)| *r)
        .expect("swc: missing tile script")
}

/// `SharedWaterCache`: the TTL-rebuilt `byPlayer` map over the per-player
/// `playerWater` rescan cache.
pub struct SharedWaterCache {
    tick: f64,
    by_player: Option<NumMap<Option<RefSet>>>,
    player_water: NumMap<PlayerWater>,
}

impl SharedWaterCache {
    pub fn new() -> Self {
        Self {
            tick: f64::NEG_INFINITY,
            by_player: None,
            player_water: NumMap::default(),
        }
    }

    /// `get(player)`: rebuild when `byPlayer === null` or the tick difference
    /// reaches the TTL, then hand back the stored set (or `null`).
    pub(crate) fn get(
        &mut self,
        f: &mut Facade,
        trace: &mut Vec<f64>,
        pidx: usize,
    ) -> Option<RefSet> {
        let tick = f.ticks(trace);
        let stale = self.by_player.is_none() || tick - self.tick >= TTL_TICKS;
        if stale {
            self.build(f, trace);
            self.tick = tick;
        }
        let pid = f.specs[pidx].pid;
        self.by_player
            .as_ref()
            .and_then(|m| m.get(pid))
            .and_then(|v| v.clone())
    }

    /// `waterFor(player, waterVersion)`: strict-`===` double version check,
    /// otherwise the border / shore / neighbor rescan.
    fn water_for(
        &mut self,
        f: &mut Facade,
        trace: &mut Vec<f64>,
        pidx: usize,
        water_version: f64,
    ) -> (bool, Vec<f64>) {
        let pid = f.specs[pidx].pid;
        let tile_version = f.tile_change_version(pidx, trace);
        if let Some(e) = self.player_water.get(pid) {
            if e.tile_version == tile_version && e.water_version == water_version {
                return (e.has_ocean, e.lakes.iter().collect());
            }
        }
        let mut has_ocean = false;
        let mut lakes = RefSet::default();
        let border = f.border_tiles(pidx, trace);
        for tile in border {
            if !f.is_shore(tile, trace) {
                continue;
            }
            for n in f.for_each_neighbor(tile, trace) {
                if !f.is_water(n, trace) {
                    continue;
                }
                if f.is_ocean(n, trace) {
                    has_ocean = true;
                    continue;
                }
                if let Some(c) = f.get_water_component(n, trace) {
                    lakes.add(c);
                }
            }
        }
        let out = (has_ocean, lakes.iter().collect());
        self.player_water.set(
            pid,
            PlayerWater {
                tile_version,
                water_version,
                has_ocean,
                lakes,
            },
        );
        out
    }

    /// `build()`: pass 1 records each non-bot player's water and the lake
    /// partner arrays, pass 2 resolves the shared sets.
    fn build(&mut self, f: &mut Facade, trace: &mut Vec<f64>) {
        let water_version = f.water_version(trace);
        let order = f.players(trace);

        let mut player_to_water: Vec<(usize, f64, bool, Vec<f64>)> = Vec::new();
        let mut lake_partners: NumMap<Vec<f64>> = NumMap::default();
        for pidx in order {
            let typ = f.type_of(pidx, trace);
            if typ == PLAYER_TYPE_BOT {
                continue;
            }
            let pid = f.specs[pidx].pid;
            let (has_ocean, lakes) = self.water_for(f, trace, pidx, water_version);
            player_to_water.push((pidx, pid, has_ocean, lakes.clone()));
            for c in lakes {
                if lake_partners.get(c).is_none() {
                    lake_partners.set(c, Vec::new());
                }
                lake_partners
                    .get_mut_by(c)
                    .expect("swc: just-inserted")
                    .push(pid);
            }
        }

        let mut result: NumMap<Option<RefSet>> = NumMap::default();
        for (pidx, pid, has_ocean, lakes) in player_to_water {
            let mut shared = RefSet::default();
            if has_ocean {
                shared.add(OCEAN_SENTINEL);
            }
            for c in lakes {
                let Some(partners) = lake_partners.get(c).cloned() else {
                    continue;
                };
                for other in partners {
                    if svz_key(other) != svz_key(pid) && f.can_trade(pidx, other, trace) {
                        shared.add(c);
                        break;
                    }
                }
            }
            result.set(pid, if shared.size() > 0 { Some(shared) } else { None });
        }
        self.by_player = Some(result);
    }
}

impl Default for SharedWaterCache {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------- vectors op
//
// A flat `f64` token runner shared by the golden replay and the wasm probe.
// kind 0 replays one whole scripted op sequence (see the header comment and
// `tools/gen_vectors.mjs` for the exact framing): args
// `[0,nTicks,(tick)*,nPlayers,(pid,typeStr,tileVersion,nBorder,(tile)*,nTrade,
// (other,0|1)*,inPlayers)*,waterVersion,nShore,(tile,0|1)*,nNbr,(tile,n,
// (nbr)*)*,nWater,(t,0|1)*,nOcean,(t,0|1)*,nComp,(t,0|1,comp?)*,nOps,(op)*]`
// with op 0 `get [0,pid]`, op 1 `set tileChangeVersion [1,pid,v]`, op 2 `set
// waterVersion [2,v]`; res `[traceLen,(trace)*,nGets,(get)*,tick,byPlayer,
// playerWater]`.

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
    fn list(&mut self) -> Vec<f64> {
        let len = self.u();
        (0..len).map(|_| self.f()).collect()
    }
    fn str(&mut self) -> Vec<u16> {
        let len = self.u();
        (0..len).map(|_| self.f() as u16).collect()
    }
}

fn push_set(out: &mut Vec<f64>, s: &RefSet) {
    out.push(s.size() as f64);
    out.extend(s.iter());
}

fn run_replay(c: &mut Cur, trace: &mut Vec<f64>, tail: &mut Vec<f64>) {
    let ticks = c.list();
    let n_players = c.u();
    let mut specs = Vec::with_capacity(n_players);
    for _ in 0..n_players {
        let pid = c.f();
        let typ = c.str();
        let tile_version = c.f();
        let border = c.list();
        let n_trade = c.u();
        let mut trades = Vec::with_capacity(n_trade);
        for _ in 0..n_trade {
            let o = c.f();
            let r = c.f();
            trades.push((o, r));
        }
        let in_players = c.f();
        specs.push(PlayerSpec {
            pid,
            typ,
            tile_version,
            border,
            trades,
            in_players: in_players == 1.0,
        });
    }
    let water_version = c.f();
    let n_shore = c.u();
    let shore: Vec<(f64, f64)> = (0..n_shore).map(|_| (c.f(), c.f())).collect();
    let n_nbr = c.u();
    let nbrs: Vec<(f64, Vec<f64>)> = (0..n_nbr).map(|_| (c.f(), c.list())).collect();
    let n_water = c.u();
    let water: Vec<(f64, f64)> = (0..n_water).map(|_| (c.f(), c.f())).collect();
    let n_ocean = c.u();
    let ocean: Vec<(f64, f64)> = (0..n_ocean).map(|_| (c.f(), c.f())).collect();
    let n_comp = c.u();
    let comp: Vec<(f64, Option<f64>)> = (0..n_comp)
        .map(|_| {
            let t = c.f();
            let k = c.f();
            (t, if k == 0.0 { None } else { Some(c.f()) })
        })
        .collect();
    let n_ops = c.u();
    let mut ops = Vec::with_capacity(n_ops);
    for _ in 0..n_ops {
        let k = c.f() as u8;
        match k {
            0 => ops.push((0u8, c.f(), 0.0)),
            1 => ops.push((1, c.f(), c.f())),
            _ => ops.push((2, c.f(), 0.0)),
        }
    }

    let mut f = Facade {
        ticks,
        ti: 0,
        water_version,
        specs,
        shore,
        nbrs,
        water,
        ocean,
        comp,
    };
    let mut cache = SharedWaterCache::new();
    let mut gets: Vec<Option<RefSet>> = Vec::new();
    for (k, a, b) in ops {
        match k {
            0 => {
                let pidx = find_spec(&f, a);
                gets.push(cache.get(&mut f, trace, pidx));
            }
            1 => {
                let pidx = find_spec(&f, a);
                f.specs[pidx].tile_version = b;
            }
            _ => f.water_version = a,
        }
    }

    tail.push(gets.len() as f64);
    for g in &gets {
        match g {
            None => tail.push(0.0),
            Some(s) => {
                tail.push(1.0);
                push_set(tail, s);
            }
        }
    }
    tail.push(cache.tick);
    match &cache.by_player {
        None => tail.push(0.0),
        Some(m) => {
            tail.push(1.0);
            tail.push(m.size() as f64);
            for (pid, v) in &m.entries {
                tail.push(*pid);
                match v {
                    None => tail.push(0.0),
                    Some(s) => {
                        tail.push(1.0);
                        push_set(tail, s);
                    }
                }
            }
        }
    }
    tail.push(cache.player_water.size() as f64);
    for (pid, e) in &cache.player_water.entries {
        tail.push(*pid);
        tail.push(e.tile_version);
        tail.push(e.water_version);
        tail.push(if e.has_ocean { 1.0 } else { 0.0 });
        push_set(tail, &e.lakes);
    }
}

fn find_spec(f: &Facade, pid: f64) -> usize {
    let kk = svz_key(pid);
    f.specs
        .iter()
        .position(|s| svz_key(s.pid) == kk)
        .expect("swc: unknown player pid")
}

pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut c = Cur(args, 0);
    let k = c.u() as u8;
    let mut trace: Vec<f64> = Vec::new();
    let mut tail: Vec<f64> = Vec::new();
    if k == kind && kind == 0 {
        run_replay(&mut c, &mut trace, &mut tail);
    }
    let mut out = Vec::with_capacity(1 + trace.len() + tail.len());
    out.push(trace.len() as f64);
    out.extend(trace.iter().copied());
    out.extend(tail.iter().copied());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal scenario builder mirroring the capture framing.
    #[allow(clippy::type_complexity, clippy::too_many_arguments)]
    fn args(
        ticks: &[f64],
        players: &[(f64, &str, f64, &[f64], &[(f64, f64)])],
        wv: f64,
        shore: &[(f64, f64)],
        nbrs: &[(f64, &[f64])],
        water: &[(f64, f64)],
        ocean: &[(f64, f64)],
        comp: &[(f64, Option<f64>)],
        ops: &[Vec<f64>],
    ) -> Vec<f64> {
        let mut v = vec![0.0, ticks.len() as f64];
        v.extend(ticks.iter().copied());
        v.push(players.len() as f64);
        for (pid, ty, tv, border, trades) in players {
            v.push(*pid);
            let u: Vec<u16> = ty.encode_utf16().collect();
            v.push(u.len() as f64);
            v.extend(u.iter().map(|&x| f64::from(x)));
            v.push(*tv);
            v.push(border.len() as f64);
            v.extend(border.iter().copied());
            v.push(trades.len() as f64);
            for (o, r) in *trades {
                v.push(*o);
                v.push(*r);
            }
            v.push(1.0);
        }
        v.push(wv);
        v.push(shore.len() as f64);
        for (t, r) in shore {
            v.push(*t);
            v.push(*r);
        }
        v.push(nbrs.len() as f64);
        for (t, ns) in nbrs {
            v.push(*t);
            v.push(ns.len() as f64);
            v.extend(ns.iter().copied());
        }
        v.push(water.len() as f64);
        for (t, r) in water {
            v.push(*t);
            v.push(*r);
        }
        v.push(ocean.len() as f64);
        for (t, r) in ocean {
            v.push(*t);
            v.push(*r);
        }
        v.push(comp.len() as f64);
        for (t, c) in comp {
            v.push(*t);
            match c {
                None => v.push(0.0),
                Some(x) => {
                    v.push(1.0);
                    v.push(*x);
                }
            }
        }
        v.push(ops.len() as f64);
        for o in ops {
            v.extend(o.iter().copied());
        }
        v
    }

    fn payload(res: &[f64]) -> &[f64] {
        &res[1 + res[0] as usize..]
    }

    /// Walk the trace stream by event tag and count occurrences of `want`.
    fn count_event(res: &[f64], want: u8) -> usize {
        let trace = &res[1..1 + res[0] as usize];
        let mut i = 0;
        let mut n = 0;
        while i < trace.len() {
            let tag = trace[i] as u8;
            let len = match tag {
                0 | 1 => 2,
                2 | 7 => 3 + trace[i + 2] as usize,
                3 | 6 | 8 | 9 => 3,
                5 => 4,
                4 => 3 + trace[i + 2] as usize,
                10 => {
                    if trace[i + 2] == 0.0 {
                        3
                    } else {
                        4
                    }
                }
                11 => 2 + trace[i + 1] as usize,
                _ => panic!("swc test: bad trace tag {tag}"),
            };
            if tag == want {
                n += 1;
            }
            i += len;
        }
        n
    }

    /// The scenario-1 shape: first get rebuilds, {-1} for the ocean toucher,
    /// tick pinned, playerWater entry present.
    #[test]
    fn first_get_rebuild() {
        let a = args(
            &[100.0],
            &[(1.0, "HUMAN", 5.0, &[10.0, 11.0], &[])],
            7.0,
            &[(10.0, 1.0), (11.0, 0.0)],
            &[(10.0, &[20.0, 21.0, 22.0])],
            &[(20.0, 1.0), (21.0, 1.0), (22.0, 0.0)],
            &[(20.0, 1.0), (21.0, 0.0)],
            &[(21.0, Some(3.0))],
            &[vec![0.0, 1.0]],
        );
        let res = run_op(0, &a);
        assert_eq!(
            payload(&res),
            &[1.0, 1.0, 1.0, -1.0, 100.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 5.0, 7.0, 1.0, 1.0, 3.0]
        );
    }

    /// TTL boundary: 29 no rebuild (ticks traced once per get), 30 rebuilds
    /// with a waterFor cache hit (no border walk in the second build).
    #[test]
    fn ttl_boundary() {
        let mk = |t: f64| {
            args(
                &[100.0, t],
                &[(1.0, "HUMAN", 5.0, &[10.0], &[])],
                7.0,
                &[(10.0, 1.0)],
                &[(10.0, &[20.0])],
                &[(20.0, 1.0)],
                &[(20.0, 1.0)],
                &[],
                &[vec![0.0, 1.0], vec![0.0, 1.0]],
            )
        };
        let r29 = run_op(0, &mk(129.0));
        let r30 = run_op(0, &mk(130.0));
        // The 30-diff rebuild adds exactly [1,wv] (2) + [11,n,pid] (3) +
        // [4,pid,len,u*5] (8) + [3,pid,tv] (3) = 16 tokens, no border walk.
        assert_eq!(r30[0], r29[0] + 16.0);
        assert!(payload(&r29).starts_with(&[2.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 100.0]));
        assert!(payload(&r30).starts_with(&[2.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 130.0]));
        // ticks() fires once per get in both (no extra call on the no-rebuild).
        assert_eq!(count_event(&r29, 0), 2);
        assert_eq!(count_event(&r30, 0), 2);
        // The rebuild fires waterVersion once per build: 1 (first) vs 2.
        assert_eq!(count_event(&r29, 1), 1);
        assert_eq!(count_event(&r30, 1), 2);
    }

    /// canTrade count is pinned: shared lake breaks on the first valid
    /// partner, an embargoed pair scans every partner.
    #[test]
    fn can_trade_counts() {
        let count = |trade: f64| {
            let a = args(
                &[100.0, 100.0],
                &[
                    (1.0, "HUMAN", 5.0, &[10.0], &[(2.0, trade)]),
                    (2.0, "HUMAN", 5.0, &[11.0], &[(1.0, trade)]),
                ],
                7.0,
                &[(10.0, 1.0), (11.0, 1.0)],
                &[(10.0, &[20.0]), (11.0, &[21.0])],
                &[(20.0, 1.0), (21.0, 1.0)],
                &[(20.0, 0.0), (21.0, 0.0)],
                &[(20.0, Some(3.0)), (21.0, Some(3.0))],
                &[vec![0.0, 1.0], vec![0.0, 2.0]],
            );
            let res = run_op(0, &a);
            let ev5 = count_event(&res, 5);
            (ev5, payload(&res).to_vec())
        };
        let (t, _) = count(1.0);
        assert_eq!(t, 2); // one break-short-circuit per player
        let (t, p) = count(0.0);
        assert_eq!(t, 2); // self skipped by identity, the other scanned once
        assert!(p.starts_with(&[2.0, 0.0, 0.0])); // both gets null, null stored
    }

    /// Bot skip: the type event is traced, no waterFor, get(bot) -> null.
    #[test]
    fn bot_skipped() {
        let a = args(
            &[100.0, 100.0],
            &[
                (1.0, "HUMAN", 5.0, &[10.0], &[]),
                (2.0, "BOT", 1.0, &[11.0], &[]),
            ],
            7.0,
            &[(10.0, 1.0)],
            &[(10.0, &[20.0])],
            &[(20.0, 1.0)],
            &[(20.0, 1.0)],
            &[],
            &[vec![0.0, 1.0], vec![0.0, 2.0]],
        );
        let res = run_op(0, &a);
        let p = payload(&res);
        assert_eq!(&p[..4], &[2.0, 1.0, 1.0, -1.0]); // p1 {-1}, bot null
    }
}
