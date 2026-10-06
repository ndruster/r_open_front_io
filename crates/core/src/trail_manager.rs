//! Port of `src/client/render/frame/TrailManager.ts` — the per-tile
//! "last owner" 16-bit stamp for trail rendering.
//!
//! Faithfulness notes (quirk list):
//!
//! * `trailState` / `trailCounts` are `Uint16Array`: every element write goes
//!   through [`to_uint16`] (wrap mod 2^16, NaN/±Infinity → 0); an out-of-range
//!   or non-integer index write is silently DROPPED (JS typed-array `[[Set]]`),
//!   and an out-of-range read is `undefined` → NaN numerically.
//! * `--trailCounts[ref] === 0` compares the PREFIX-DECREMENT result against
//!   the UNWRAPPED arithmetic value (`0 - 1` is `-1`, not `65535`), while the
//!   STORE wraps — the spec's UpdateExpression returns `newValue`, not a
//!   re-read. Only a count of exactly `1` triggers `stamp(ref, 0)`.
//! * `value = ownerID | (isNuke ? NUKE_TRAIL_BIT : 0)` is a ToInt32 `|`
//!   ([`to_int32`]), then stored through the Uint16Array wrap.
//! * `stamp`'s `row = (ref / mapW) | 0` is ToInt32 of the f64 quotient; the
//!   min/max gates use plain `<` / `>` against the seeded `Infinity` / `-1`.
//! * `update` runs `clearDeadUnits` FIRST; a trail's `lastPosStamped === -1`
//!   first-sighting stamps only `head` (nuke → lastPos, boat → pos);
//!   subsequent moves run `bresenham(lastPosStamped, head)` ONLY when the
//!   head actually changed (`!==`).
//! * `clearDeadUnits` deletes the current key while iterating the JS `Map`
//!   (legal); the tile release walks the trail's `Set<number>` in insertion
//!   order.
//! * bresenham is pure integer f64 arithmetic: `from % w` ([`js_mod`]),
//!   `y0 = (from - x0) / w`, `dy = -abs(...)`, the double-`if` (NOT
//!   else-if) on `e2 >= dy` / `e2 <= dx`.

use crate::desync_detector::{NumMap, NumSet};
use crate::js_json::read_str;
use crate::jsnum::{js_mod, to_int32, to_uint16};
use crate::unit_types::SMOOTHED_NUKE_TYPES;

/// `NUKE_TRAIL_BIT` — bit 12 of the trail texel flags a nuke trail.
pub const NUKE_TRAIL_BIT: i32 = 1 << 12;

/// `SMOOTHED_NUKE_TYPES` membership (ported in [`crate::unit_types`]).
fn smoothed_nuke(unit_type: &str) -> bool {
    SMOOTHED_NUKE_TYPES.contains(&unit_type)
}

/// `UnitTrail`.
#[derive(Debug, Clone)]
struct UnitTrail {
    value: f64,
    tiles: NumSet,
    last_pos_stamped: f64,
}

/// The `UnitState` fields the manager reads.
#[derive(Debug, Clone)]
struct UnitState {
    unit_type: String,
    owner_id: f64,
    pos: f64,
    last_pos: f64,
}

/// The ported `TrailManager`.
#[derive(Debug)]
pub struct TrailManager {
    trail_state: Vec<u16>,
    trail_counts: Vec<u16>,
    unit_trails: NumMap<UnitTrail>,
    map_w: f64,
    dirty_row_min: f64,
    dirty_row_max: f64,
}

impl Default for TrailManager {
    fn default() -> Self {
        Self {
            trail_state: Vec::new(),
            trail_counts: Vec::new(),
            unit_trails: NumMap::default(),
            map_w: 1.0,
            dirty_row_min: f64::INFINITY,
            dirty_row_max: -1.0,
        }
    }
}

/// Typed-array index resolution: a whole number in `[0, len)` (JS
/// CanonicalNumericIndex); everything else reads `undefined` / drops the
/// write.
fn ta_index(i: f64, len: usize) -> Option<usize> {
    if i.is_finite() && i >= 0.0 && i < len as f64 && i.fract() == 0.0 {
        Some(i as usize)
    } else {
        None
    }
}

impl TrailManager {
    /// `new TrailManager(mapW, mapH)`.
    pub fn new(map_w: f64, map_h: f64) -> Self {
        let n = (map_w * map_h) as usize;
        Self {
            trail_state: vec![0u16; n],
            trail_counts: vec![0u16; n],
            unit_trails: NumMap::default(),
            map_w,
            dirty_row_min: f64::INFINITY,
            dirty_row_max: -1.0,
        }
    }

    /// `clearDirtyRows()`.
    pub fn clear_dirty_rows(&mut self) {
        self.dirty_row_min = f64::INFINITY;
        self.dirty_row_max = -1.0;
    }

    /// `reset()`.
    pub fn reset(&mut self) {
        self.unit_trails.clear();
        self.trail_state.iter_mut().for_each(|v| *v = 0);
        self.trail_counts.iter_mut().for_each(|v| *v = 0);
        self.dirty_row_min = f64::INFINITY;
        self.dirty_row_max = -1.0;
    }

    /// `update(units, trackedIds)`.
    fn update(&mut self, units: &NumMap<UnitState>, tracked_ids: &[f64]) {
        self.clear_dead_units(units);
        for id in tracked_ids {
            let Some(unit) = units.get(*id) else { continue };
            let is_nuke = smoothed_nuke(&unit.unit_type);
            if !self.unit_trails.has(*id) {
                let value = (to_int32(unit.owner_id)
                    | if is_nuke { NUKE_TRAIL_BIT } else { 0 })
                    as f64;
                self.unit_trails.set(
                    *id,
                    UnitTrail { value, tiles: NumSet::default(), last_pos_stamped: -1.0 },
                );
            }
            let head = if is_nuke { unit.last_pos } else { unit.pos };
            let stamped = self.unit_trails.get(*id).unwrap().last_pos_stamped;
            if stamped == -1.0 {
                self.claim(head, *id);
                self.unit_trails.get_mut(*id).unwrap().last_pos_stamped = head;
            } else if stamped != head {
                self.bresenham(stamped, head, *id);
                self.unit_trails.get_mut(*id).unwrap().last_pos_stamped = head;
            }
        }
    }

    /// `clearDeadUnits(units)`.
    fn clear_dead_units(&mut self, units: &NumMap<UnitState>) {
        // JS iterates the Map and deletes the CURRENT key (legal). The trail
        // handle is captured at visit time, so its tile set survives the
        // delete — collect the insertion-ordered tiles before removing.
        let dead: Vec<(f64, Vec<f64>)> = self
            .unit_trails
            .iter()
            .filter(|(id, _)| !units.has(*id))
            .map(|(id, t)| (id, t.tiles.iter().collect::<Vec<f64>>()))
            .collect();
        for (id, tiles) in dead {
            self.unit_trails.delete(id);
            for ref_tile in tiles {
                // `--trailCounts[ref] === 0`: the prefix-decrement expression
                // value is the ARITHMETIC `old - 1` (an out-of-range read is
                // `undefined` -> NaN), while the STORE wraps through
                // ToUint16. Only `old === 1` reaches `=== 0` and stamps.
                let expr = match ta_index(ref_tile, self.trail_counts.len()) {
                    Some(idx) => {
                        let v = f64::from(self.trail_counts[idx]) - 1.0;
                        self.trail_counts[idx] = to_uint16(v);
                        v
                    }
                    None => f64::NAN,
                };
                if expr == 0.0 {
                    self.stamp(ref_tile, 0.0);
                }
            }
        }
    }

    /// `claim(ref, trail)`.
    fn claim(&mut self, ref_tile: f64, trail_id: f64) {
        let value = self.unit_trails.get(trail_id).unwrap().value;
        {
            let trail = self.unit_trails.get_mut(trail_id).unwrap();
            if !trail.tiles.has(ref_tile) {
                trail.tiles.add(ref_tile);
                if let Some(idx) = ta_index(ref_tile, self.trail_counts.len()) {
                    // `trailCounts[ref]++`: read, +1, store wrapped.
                    self.trail_counts[idx] = to_uint16(f64::from(self.trail_counts[idx]) + 1.0);
                }
            }
        }
        self.stamp(ref_tile, value);
    }

    /// `stamp(ref, value)`.
    fn stamp(&mut self, ref_tile: f64, value: f64) {
        if let Some(idx) = ta_index(ref_tile, self.trail_state.len()) {
            self.trail_state[idx] = to_uint16(value);
        }
        let row = to_int32(ref_tile / self.map_w) as f64;
        if row < self.dirty_row_min {
            self.dirty_row_min = row;
        }
        if row > self.dirty_row_max {
            self.dirty_row_max = row;
        }
    }

    /// `bresenham(from, to, trail)`.
    fn bresenham(&mut self, from: f64, to: f64, trail_id: f64) {
        let w = self.map_w;
        let mut x0 = js_mod(from, w);
        let mut y0 = (from - x0) / w;
        let x1 = js_mod(to, w);
        let y1 = (to - x1) / w;
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1.0 } else { -1.0 };
        let sy = if y0 < y1 { 1.0 } else { -1.0 };
        let mut err = dx + dy;
        loop {
            let ref_tile = y0 * w + x0;
            self.claim(ref_tile, trail_id);
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2.0 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }
}

/// The capture harness.
#[derive(Debug, Default)]
pub struct RigHarness {
    mgr: TrailManager,
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
    /// 1 update `[n, (id, tlen, (type)*tlen, ownerID, pos, lastPos)*n, m,
    ///   (trackedId)*m]` -> `[0]`;
    /// 2 clearDirtyRows -> `[0]`;
    /// 3 reset -> `[0]`;
    /// 4 dumpState -> `[k, (ref, value)*k]` nonzero trailState + `[c, (ref,
    ///   value)*c]` nonzero trailCounts;
    /// 5 dumpTrails -> `[n, (id, value, t, (ref)*t, lastPosStamped)*n]`;
    /// 6 dumpDirty -> `[dirtyRowMin, dirtyRowMax]`;
    /// 7 constants -> `[NUKE_TRAIL_BIT]`.
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut i = 0usize;
        match kind {
            0 => {
                self.mgr = TrailManager::new(args[i], args[i + 1]);
                vec![0.0]
            }
            1 => {
                let n = args[i] as usize;
                i += 1;
                let mut units: NumMap<UnitState> = NumMap::default();
                for _ in 0..n {
                    let id = args[i];
                    i += 1;
                    let unit_type = read_str(args, &mut i);
                    let owner_id = args[i];
                    let pos = args[i + 1];
                    let last_pos = args[i + 2];
                    i += 3;
                    units.set(id, UnitState { unit_type, owner_id, pos, last_pos });
                }
                let m = args[i] as usize;
                i += 1;
                let tracked: Vec<f64> = args[i..i + m].to_vec();
                self.mgr.update(&units, &tracked);
                vec![0.0]
            }
            2 => {
                self.mgr.clear_dirty_rows();
                vec![0.0]
            }
            3 => {
                self.mgr.reset();
                vec![0.0]
            }
            4 => {
                let mut out = Vec::new();
                let mut pairs = Vec::new();
                for (idx, v) in self.mgr.trail_state.iter().enumerate() {
                    if *v != 0 {
                        pairs.push((idx as f64, f64::from(*v)));
                    }
                }
                out.push(pairs.len() as f64);
                for (r, v) in pairs {
                    out.push(r);
                    out.push(v);
                }
                let mut pairs = Vec::new();
                for (idx, v) in self.mgr.trail_counts.iter().enumerate() {
                    if *v != 0 {
                        pairs.push((idx as f64, f64::from(*v)));
                    }
                }
                out.push(pairs.len() as f64);
                for (r, v) in pairs {
                    out.push(r);
                    out.push(v);
                }
                out
            }
            5 => {
                let mut out = Vec::new();
                out.push(self.mgr.unit_trails.len() as f64);
                for (id, t) in self.mgr.unit_trails.iter() {
                    out.push(id);
                    out.push(t.value);
                    out.push(t.tiles.len() as f64);
                    for r in t.tiles.iter() {
                        out.push(r);
                    }
                    out.push(t.last_pos_stamped);
                }
                out
            }
            6 => vec![self.mgr.dirty_row_min, self.mgr.dirty_row_max],
            7 => vec![f64::from(NUKE_TRAIL_BIT)],
            k => unreachable!("trail_manager harness: unknown op kind {k}"),
        }
    }
}
