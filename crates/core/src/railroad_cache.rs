//! Port of `src/client/render/frame/RailroadCache.ts` — the always-on
//! railroad event accumulator (orientation computation, construction
//! animation, per-tile `Uint8Array` for GPU upload).
//!
//! Faithfulness notes (quirk list):
//!
//! * `RailType` is a regular TS enum — the six members are plain integers
//!   0..5 (`VERTICAL, HORIZONTAL, TOP_LEFT, TOP_RIGHT, BOTTOM_LEFT,
//!   BOTTOM_RIGHT`).
//! * `railExtremity`: `dx === 0 -> VERTICAL`, `dy === 0 -> HORIZONTAL`,
//!   fallback `VERTICAL` (diagonal extremities are VERTICAL).
//! * `railDirection`: straight runs pick HORIZONTAL when `dx1 !== 0` else
//!   VERTICAL; the eight corner rules only run when EXACTLY ONE of dx1/dx2
//!   is zero; anything else falls through to VERTICAL.
//! * `computeRailTiles`: empty -> `[]`; single -> one VERTICAL tile; else
//!   extremity / direction / extremity (the last extremity looks BACKWARD).
//! * `apply` event order: Construction -> Snap (remove original, add both
//!   halves complete) -> Destruction, then `tickAnimations`.
//! * `addRailroad` anim seeds `headIndex = complete ? len : 0`, `tailIndex =
//!   complete ? 0 : len`; refCount `get(ref) ?? 0` + 1; complete stamps the
//!   whole railroad and sets dirty.
//! * `removeRailroad` decrements with `get(ref) ?? 1` - 1; `count <= 0`
//!   deletes the entry and zeroes the tile, else stores the count. The
//!   `railroadDirty = true` write is UNCONDITIONAL (even for an empty tiles
//!   list) after the anim delete.
//! * `tickAnimations` clears `revealedRailTiles` first; per incomplete anim:
//!   `tailIndex - headIndex <= 2 * 3` finishes the whole span; else head-3 +
//!   tail-3 advance, `headIndex >= tailIndex` completes; either branch sets
//!   dirty.
//! * `railroadState[ref] = type + 1` writes through Uint8Array semantics
//!   ([`to_uint32`] wrap + index range drop).

use crate::desync_detector::NumMap;
use crate::jsnum::{js_mod, to_uint32};

/// `RailType` — the six regular-enum members.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RailType {
    Vertical = 0,
    Horizontal = 1,
    TopLeft = 2,
    TopRight = 3,
    BottomLeft = 4,
    BottomRight = 5,
}

impl RailType {
    fn value(self) -> f64 {
        self as i32 as f64
    }
}

/// `RailTile`.
#[derive(Debug, Clone, Copy)]
struct RailTile {
    ref_tile: f64,
    rtype: RailType,
}

/// `RailroadAnim`.
#[derive(Debug, Clone)]
struct RailroadAnim {
    tiles: Vec<RailTile>,
    head_index: f64,
    tail_index: f64,
    complete: bool,
}

/// One snap event: `(originalId, newId1, newId2, tiles1, tiles2)`.
type SnapEvent = (f64, f64, f64, Vec<f64>, Vec<f64>);

/// `RAIL_INCREMENT`.
const RAIL_INCREMENT: f64 = 3.0;

/// `railExtremity(tile, next, w)`.
fn rail_extremity(tile: f64, next: f64, w: f64) -> RailType {
    let dx = js_mod(next, w) - js_mod(tile, w);
    let dy = (next - js_mod(next, w)) / w - (tile - js_mod(tile, w)) / w;
    if dx == 0.0 {
        return RailType::Vertical;
    }
    if dy == 0.0 {
        return RailType::Horizontal;
    }
    RailType::Vertical
}

/// `railDirection(prev, cur, next, w)`.
fn rail_direction(prev: f64, cur: f64, next: f64, w: f64) -> RailType {
    let x1 = js_mod(prev, w);
    let y1 = (prev - x1) / w;
    let x2 = js_mod(cur, w);
    let y2 = (cur - x2) / w;
    let x3 = js_mod(next, w);
    let y3 = (next - x3) / w;
    let dx1 = x2 - x1;
    let dy1 = y2 - y1;
    let dx2 = x3 - x2;
    let dy2 = y3 - y2;
    if dx1 == dx2 && dy1 == dy2 {
        return if dx1 != 0.0 { RailType::Horizontal } else { RailType::Vertical };
    }
    if (dx1 == 0.0 && dx2 != 0.0) || (dx1 != 0.0 && dx2 == 0.0) {
        if dx1 == 0.0 && dx2 == 1.0 && dy1 == -1.0 {
            return RailType::BottomRight;
        }
        if dx1 == 0.0 && dx2 == -1.0 && dy1 == -1.0 {
            return RailType::BottomLeft;
        }
        if dx1 == 0.0 && dx2 == 1.0 && dy1 == 1.0 {
            return RailType::TopRight;
        }
        if dx1 == 0.0 && dx2 == -1.0 && dy1 == 1.0 {
            return RailType::TopLeft;
        }
        if dx1 == 1.0 && dx2 == 0.0 && dy2 == -1.0 {
            return RailType::TopLeft;
        }
        if dx1 == -1.0 && dx2 == 0.0 && dy2 == -1.0 {
            return RailType::TopRight;
        }
        if dx1 == 1.0 && dx2 == 0.0 && dy2 == 1.0 {
            return RailType::BottomLeft;
        }
        if dx1 == -1.0 && dx2 == 0.0 && dy2 == 1.0 {
            return RailType::BottomRight;
        }
    }
    RailType::Vertical
}

/// `computeRailTiles(tileRefs, w)`.
fn compute_rail_tiles(tile_refs: &[f64], w: f64) -> Vec<RailTile> {
    if tile_refs.is_empty() {
        return Vec::new();
    }
    if tile_refs.len() == 1 {
        return vec![RailTile { ref_tile: tile_refs[0], rtype: RailType::Vertical }];
    }
    let mut result = Vec::with_capacity(tile_refs.len());
    result.push(RailTile {
        ref_tile: tile_refs[0],
        rtype: rail_extremity(tile_refs[0], tile_refs[1], w),
    });
    for i in 1..tile_refs.len() - 1 {
        result.push(RailTile {
            ref_tile: tile_refs[i],
            rtype: rail_direction(tile_refs[i - 1], tile_refs[i], tile_refs[i + 1], w),
        });
    }
    let last = tile_refs.len() - 1;
    result.push(RailTile {
        ref_tile: tile_refs[last],
        rtype: rail_extremity(tile_refs[last], tile_refs[last - 1], w),
    });
    result
}

/// `Uint8Array` element write: ToUint8 wrap + index range check.
fn u8_write(arr: &mut [u8], i: f64, v: f64) {
    if i.is_finite() && i >= 0.0 && i < arr.len() as f64 && i.fract() == 0.0 {
        arr[i as usize] = to_uint32(v) as u8;
    }
}

/// The ported `RailroadCache`.
#[derive(Debug)]
pub struct RailroadCache {
    map_w: f64,
    anims: NumMap<RailroadAnim>,
    tile_ref_count: NumMap<f64>,
    railroad_state: Vec<u8>,
    railroad_dirty: bool,
    revealed_rail_tiles: Vec<f64>,
}

impl Default for RailroadCache {
    fn default() -> Self {
        Self {
            map_w: 1.0,
            anims: NumMap::default(),
            tile_ref_count: NumMap::default(),
            railroad_state: Vec::new(),
            railroad_dirty: false,
            revealed_rail_tiles: Vec::new(),
        }
    }
}

impl RailroadCache {
    /// `new RailroadCache(mapW, mapH)`.
    pub fn new(map_w: f64, map_h: f64) -> Self {
        Self {
            map_w,
            anims: NumMap::default(),
            tile_ref_count: NumMap::default(),
            railroad_state: vec![0u8; (map_w * map_h) as usize],
            railroad_dirty: false,
            revealed_rail_tiles: Vec::new(),
        }
    }

    /// `reset()`.
    pub fn reset(&mut self) {
        self.anims.clear();
        self.tile_ref_count.clear();
        self.railroad_state.iter_mut().for_each(|v| *v = 0);
        self.railroad_dirty = false;
    }

    /// `clearDirty()`.
    pub fn clear_dirty(&mut self) {
        self.railroad_dirty = false;
    }

    /// `apply(gu)` — one scripted tick's railroad events (Construction ->
    /// Snap -> Destruction) followed by `tickAnimations`.
    pub fn apply(
        &mut self,
        constructs: &[(f64, Vec<f64>)],
        snaps: &[SnapEvent],
        destructs: &[f64],
    ) {
        for (id, tiles) in constructs {
            self.add_railroad(*id, tiles, false);
        }
        for (original_id, new_id1, new_id2, tiles1, tiles2) in snaps {
            // TS: removeRailroad(originalId); addRailroad(newId1, tiles1,
            // true); addRailroad(newId2, tiles2, true) — per event.
            self.remove_railroad(*original_id);
            self.add_railroad(*new_id1, tiles1, true);
            self.add_railroad(*new_id2, tiles2, true);
        }
        for id in destructs {
            self.remove_railroad(*id);
        }
        self.tick_animations();
    }

    /// `addRailroad(id, tileRefs, complete)`.
    fn add_railroad(&mut self, id: f64, tile_refs: &[f64], complete: bool) {
        let tiles = compute_rail_tiles(tile_refs, self.map_w);
        let len = tiles.len() as f64;
        self.anims.set(
            id,
            RailroadAnim {
                tiles: tiles.clone(),
                head_index: if complete { len } else { 0.0 },
                tail_index: if complete { 0.0 } else { len },
                complete,
            },
        );
        for rt in &tiles {
            let c = self.tile_ref_count.get(rt.ref_tile).copied().unwrap_or(0.0) + 1.0;
            self.tile_ref_count.set(rt.ref_tile, c);
        }
        if complete {
            for rt in &tiles {
                u8_write(&mut self.railroad_state, rt.ref_tile, rt.rtype.value() + 1.0);
            }
            self.railroad_dirty = true;
        }
    }

    /// `removeRailroad(id)`.
    fn remove_railroad(&mut self, id: f64) {
        let Some(anim) = self.anims.get(id) else { return };
        let tiles = anim.tiles.clone();
        for rt in &tiles {
            let count = self.tile_ref_count.get(rt.ref_tile).copied().unwrap_or(1.0) - 1.0;
            if count <= 0.0 {
                self.tile_ref_count.delete(rt.ref_tile);
                u8_write(&mut self.railroad_state, rt.ref_tile, 0.0);
            } else {
                self.tile_ref_count.set(rt.ref_tile, count);
            }
        }
        self.anims.delete(id);
        self.railroad_dirty = true;
    }

    /// `tickAnimations()`.
    fn tick_animations(&mut self) {
        self.revealed_rail_tiles.clear();
        let ids: Vec<f64> = self.anims.keys().collect();
        for id in ids {
            let complete = self.anims.get(id).unwrap().complete;
            if complete {
                continue;
            }
            let (head_index, tail_index, tiles) = {
                let a = self.anims.get(id).unwrap();
                (a.head_index, a.tail_index, a.tiles.clone())
            };
            if tail_index - head_index <= 2.0 * RAIL_INCREMENT {
                let mut i = head_index;
                while i < tail_index {
                    let t = tiles[i as usize];
                    u8_write(&mut self.railroad_state, t.ref_tile, t.rtype.value() + 1.0);
                    self.revealed_rail_tiles.push(t.ref_tile);
                    i += 1.0;
                }
                let a = self.anims.get_mut(id).unwrap();
                a.head_index = tail_index;
                a.complete = true;
                self.railroad_dirty = true;
            } else {
                let mut i = head_index;
                while i < head_index + RAIL_INCREMENT {
                    let t = tiles[i as usize];
                    u8_write(&mut self.railroad_state, t.ref_tile, t.rtype.value() + 1.0);
                    self.revealed_rail_tiles.push(t.ref_tile);
                    i += 1.0;
                }
                let mut i = tail_index - RAIL_INCREMENT;
                while i < tail_index {
                    let t = tiles[i as usize];
                    u8_write(&mut self.railroad_state, t.ref_tile, t.rtype.value() + 1.0);
                    self.revealed_rail_tiles.push(t.ref_tile);
                    i += 1.0;
                }
                let a = self.anims.get_mut(id).unwrap();
                a.head_index += RAIL_INCREMENT;
                a.tail_index -= RAIL_INCREMENT;
                if a.head_index >= a.tail_index {
                    a.complete = true;
                }
                self.railroad_dirty = true;
            }
        }
    }

    /// `getRailroadTileRefs(ids)`.
    pub fn get_railroad_tile_refs(&self, ids: &[f64]) -> Vec<f64> {
        let mut tiles = Vec::new();
        for id in ids {
            if let Some(anim) = self.anims.get(*id) {
                for t in &anim.tiles {
                    tiles.push(t.ref_tile);
                }
            }
        }
        tiles
    }

    /// `getRailroads()` — id -> raw tile refs, Map insertion order.
    pub fn get_railroads(&self) -> Vec<(f64, Vec<f64>)> {
        self.anims.iter().map(|(id, a)| (id, a.tiles.iter().map(|t| t.ref_tile).collect())).collect()
    }
}

/// The capture harness.
#[derive(Debug, Default)]
pub struct RigHarness {
    cache: RailroadCache,
}

impl RigHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Run one op. Kind table (see `tools/gen_vectors.mjs`):
    /// 0 construct `[mapW, mapH]` -> `[0]`;
    /// 1 apply `[nc, (id, t, (ref)*t)*nc, ns, (originalId, newId1, t1,
    ///   (ref)*t1, t2, (ref)*t2)*ns, nd, (id)*nd]` -> `[0]`;
    /// 2 clearDirty -> `[0]`;
    /// 3 reset -> `[0]`;
    /// 4 dumpState -> `[k, (ref, value)*k]` nonzero railroadState + dirty +
    ///   `[r, (ref)*r]` revealedRailTiles;
    /// 5 dumpRailroads (getRailroads) -> `[n, (id, t, (ref)*t)*n]`;
    /// 6 getRailroadTileRefs `[m, (id)*m]` -> `[t, (ref)*t]`;
    /// 7 computeRailTiles `[w, n, (ref)*n]` -> `[n, (ref, type)*n]`;
    /// 8 dumpRefCount -> `[k, (ref, count)*k]` (Map insertion order).
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut i = 0usize;
        match kind {
            0 => {
                self.cache = RailroadCache::new(args[i], args[i + 1]);
                vec![0.0]
            }
            1 => {
                let nc = args[i] as usize;
                i += 1;
                let mut constructs = Vec::with_capacity(nc);
                for _ in 0..nc {
                    let id = args[i];
                    i += 1;
                    let t = args[i] as usize;
                    i += 1;
                    let tiles = args[i..i + t].to_vec();
                    i += t;
                    constructs.push((id, tiles));
                }
                let ns = args[i] as usize;
                i += 1;
                let mut snaps = Vec::with_capacity(ns);
                for _ in 0..ns {
                    let original_id = args[i];
                    let new_id1 = args[i + 1];
                    let new_id2 = args[i + 2];
                    i += 3;
                    let t1 = args[i] as usize;
                    i += 1;
                    let tiles1 = args[i..i + t1].to_vec();
                    i += t1;
                    let t2 = args[i] as usize;
                    i += 1;
                    let tiles2 = args[i..i + t2].to_vec();
                    i += t2;
                    snaps.push((original_id, new_id1, new_id2, tiles1, tiles2));
                }
                let nd = args[i] as usize;
                i += 1;
                let destructs: Vec<f64> = args[i..i + nd].to_vec();
                self.cache.apply(&constructs, &snaps, &destructs);
                vec![0.0]
            }
            2 => {
                self.cache.clear_dirty();
                vec![0.0]
            }
            3 => {
                self.cache.reset();
                vec![0.0]
            }
            4 => {
                let mut out = Vec::new();
                let mut pairs = Vec::new();
                for (idx, v) in self.cache.railroad_state.iter().enumerate() {
                    if *v != 0 {
                        pairs.push((idx as f64, f64::from(*v)));
                    }
                }
                out.push(pairs.len() as f64);
                for (r, v) in pairs {
                    out.push(r);
                    out.push(v);
                }
                out.push(if self.cache.railroad_dirty { 1.0 } else { 0.0 });
                out.push(self.cache.revealed_rail_tiles.len() as f64);
                out.extend_from_slice(&self.cache.revealed_rail_tiles);
                out
            }
            5 => {
                let rr = self.cache.get_railroads();
                let mut out = Vec::new();
                out.push(rr.len() as f64);
                for (id, tiles) in rr {
                    out.push(id);
                    out.push(tiles.len() as f64);
                    out.extend_from_slice(&tiles);
                }
                out
            }
            6 => {
                let m = args[i] as usize;
                i += 1;
                let ids: Vec<f64> = args[i..i + m].to_vec();
                let refs = self.cache.get_railroad_tile_refs(&ids);
                let mut out = Vec::new();
                out.push(refs.len() as f64);
                out.extend_from_slice(&refs);
                out
            }
            7 => {
                let w = args[i];
                i += 1;
                let n = args[i] as usize;
                i += 1;
                let tile_refs: Vec<f64> = args[i..i + n].to_vec();
                let tiles = compute_rail_tiles(&tile_refs, w);
                let mut out = Vec::new();
                out.push(tiles.len() as f64);
                for t in tiles {
                    out.push(t.ref_tile);
                    out.push(t.rtype.value());
                }
                out
            }
            8 => {
                let mut out = Vec::new();
                out.push(self.cache.tile_ref_count.len() as f64);
                for (r, c) in self.cache.tile_ref_count.iter() {
                    out.push(r);
                    out.push(*c);
                }
                out
            }
            k => unreachable!("railroad_cache harness: unknown op kind {k}"),
        }
    }
}
