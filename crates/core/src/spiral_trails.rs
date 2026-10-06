//! Port of `src/client/render/frame/SpiralTrails.ts` — per-nuke centerline
//! polylines for the spiral nukeTrail cosmetic.
//!
//! Faithfulness notes (quirk list):
//!
//! * `params` / `ribbonsById` are JS `Map<number, _>` — [`NumMap`] keeps the
//!   insertion order; `update` iterates `ribbonsById.keys()` while deleting
//!   the current key (JS-legal; a snapshot walk is equivalent since only the
//!   visited key is removed).
//! * `ribbonList` is rebuilt ONLY when `changed` (a ribbon dropped or a new
//!   one was created); an existing ribbon's advance never rebuilds it.
//! * `setParams` clamps strands `Math.min(Math.max(Math.round(s),1),8)` —
//!   [`js_round`] half-up, NaN propagates through both clamps.
//! * `advance` runs entirely in f64: `x0 = lastPos % w` ([`js_mod`]),
//!   `y0 = (lastPos - x0) / w`; `segLen = Math.hypot(dx, dy)` goes through
//!   [`js_hypot`] (V8's scaled `m * sqrt(1 + t*t)` form — the naive sqrt
//!   differs in the last bit across the tile domain). `segLen === 0` returns
//!   BEFORE any state write (lastPos is already assigned above it).
//! * `dirAt` blends the previous segment direction into the new one; a
//!   blended length `< 1e-6` (exact 180° turn) falls back to the new
//!   direction. The first sample of a fresh ribbon is pushed at `f = 0` with
//!   `d = 0` BEFORE the loop; the loop runs `i = 1..=steps` with
//!   `steps = Math.ceil(segLen * 2)`.
//! * `pushSample` doubles the `Float32Array` when `off + 5 > length` and
//!   writes through f32 storage ([`to_float32`]); reads widen back to f64.
//! * MIRV warheads never grow ribbons; owners without params are skipped;
//!   `unit.lastPos !== ribbon.lastPos` gates the advance.

use crate::desync_detector::NumMap;
use crate::js_json::read_str;
use crate::jsnum::{js_hypot, js_max, js_min, js_mod, js_round, to_float32};
use crate::unit_types::SMOOTHED_NUKE_TYPES;

/// `MAX_TRAIL_STRANDS`.
pub const MAX_TRAIL_STRANDS: f64 = 8.0;
/// `SAMPLE_FLOATS` — floats per sample (cx, cy, px, py, d).
pub const SAMPLE_FLOATS: usize = 5;
/// `TAU = 2 * Math.PI`.
const TAU: f64 = 2.0 * std::f64::consts::PI;
/// `PITCH_PER_RADIUS`.
const PITCH_PER_RADIUS: f64 = 4.0;
/// `MIN_PITCH`.
const MIN_PITCH: f64 = 8.0;
/// `SAMPLES_PER_TILE`.
const SAMPLES_PER_TILE: f64 = 2.0;

/// `SMOOTHED_NUKE_TYPES` membership (the render types barrel set, ported in
/// [`crate::unit_types`]).
fn smoothed_nuke(unit_type: &str) -> bool {
    SMOOTHED_NUKE_TYPES.contains(&unit_type)
}

/// `UT_MIRV_WARHEAD`.
const UT_MIRV_WARHEAD: &str = "MIRV Warhead";

/// `SpiralParams` (post-clamp strands live here).
#[derive(Debug, Clone)]
struct SpiralParams {
    radius: f64,
    strands: f64,
    rotation_speed: f64,
    colors: Vec<[f64; 3]>,
}

/// `RibbonState` — the live ribbon (samples stored as the f32 buffer).
#[derive(Debug, Clone)]
struct RibbonState {
    id: f64,
    radius: f64,
    strands: f64,
    twist: f64,
    rotation_speed: f64,
    colors: Vec<[f64; 3]>,
    head_dist: f64,
    sample_count: usize,
    samples: Vec<f32>,
    last_pos: f64,
    dir_x: f64,
    dir_y: f64,
    has_dir: bool,
}

/// The `UnitState` fields the manager reads.
#[derive(Debug, Clone)]
struct UnitState {
    unit_type: String,
    owner_id: f64,
    last_pos: f64,
}

/// The ported `SpiralTrails`.
#[derive(Debug, Default)]
pub struct SpiralTrails {
    params: NumMap<SpiralParams>,
    ribbons_by_id: NumMap<RibbonState>,
    /// `ribbonList` — stable array instance; rebuilt only when `changed`.
    ribbon_list: Vec<f64>,
    map_w: f64,
}

impl SpiralTrails {
    /// `setParams(ownerID, params)` — stores the CLAMPED strands.
    fn set_params(&mut self, owner_id: f64, params: SpiralParams) {
        let strands = js_min(js_max(js_round(params.strands), 1.0), MAX_TRAIL_STRANDS);
        self.params.set(
            owner_id,
            SpiralParams { radius: params.radius, strands, rotation_speed: params.rotation_speed, colors: params.colors },
        );
    }

    /// `clearParams(ownerID)`.
    pub fn clear_params(&mut self, owner_id: f64) {
        self.params.delete(owner_id);
    }

    /// `reset()` — ribbons and list gone, params survive.
    pub fn reset(&mut self) {
        self.ribbons_by_id.clear();
        self.ribbon_list.clear();
    }

    /// `update(units, trackedIds)`.
    fn update(&mut self, units: &NumMap<UnitState>, tracked_ids: &[f64]) {
        let mut changed = false;
        for id in self.ribbons_by_id.keys().collect::<Vec<f64>>() {
            if !units.has(id) {
                self.ribbons_by_id.delete(id);
                changed = true;
            }
        }
        for id in tracked_ids {
            let Some(unit) = units.get(*id) else { continue };
            if !smoothed_nuke(&unit.unit_type) {
                continue;
            }
            if unit.unit_type == UT_MIRV_WARHEAD {
                continue;
            }
            if !self.ribbons_by_id.has(*id) {
                let Some(params) = self.params.get(unit.owner_id) else {
                    continue;
                };
                let params = params.clone();
                let ribbon = Self::new_ribbon(*id, &params, unit.last_pos);
                self.ribbons_by_id.set(*id, ribbon);
                changed = true;
            }
            // JS `!==` on f64: Rust `!=` matches it exactly (NaN !== NaN,
            // +0 === -0) — a NaN lastPos re-advances a fresh ribbon, whose
            // advance then runs the NaN path verbatim.
            let ribbon_last = self.ribbons_by_id.get(*id).unwrap().last_pos;
            if unit.last_pos != ribbon_last {
                let head = unit.last_pos;
                Self::advance(self.ribbons_by_id.get_mut(*id).unwrap(), head, self.map_w);
            }
        }
        if changed {
            self.ribbon_list.clear();
            for (id, _) in self.ribbons_by_id.iter() {
                self.ribbon_list.push(id);
            }
        }
    }

    /// `newRibbon(id, params, startPos)`.
    fn new_ribbon(id: f64, params: &SpiralParams, start_pos: f64) -> RibbonState {
        let pitch = js_max(params.radius * PITCH_PER_RADIUS, MIN_PITCH);
        RibbonState {
            id,
            radius: params.radius,
            strands: params.strands,
            twist: TAU / pitch,
            rotation_speed: params.rotation_speed,
            colors: params.colors.clone(),
            head_dist: 0.0,
            sample_count: 0,
            samples: vec![0.0f32; 256 * SAMPLE_FLOATS],
            last_pos: start_pos,
            dir_x: 0.0,
            dir_y: 0.0,
            has_dir: false,
        }
    }

    /// `advance(r, head)`.
    fn advance(r: &mut RibbonState, head: f64, w: f64) {
        let x0 = js_mod(r.last_pos, w);
        let y0 = (r.last_pos - x0) / w;
        let x1 = js_mod(head, w);
        let y1 = (head - x1) / w;
        r.last_pos = head;
        let dx = x1 - x0;
        let dy = y1 - y0;
        let seg_len = js_hypot(dx, dy);
        if seg_len == 0.0 {
            return;
        }
        let ndx = dx / seg_len;
        let ndy = dy / seg_len;
        let from_dir_x = if r.has_dir { r.dir_x } else { ndx };
        let from_dir_y = if r.has_dir { r.dir_y } else { ndy };
        let dir_at = |f: f64| -> (f64, f64) {
            let bx = from_dir_x + (ndx - from_dir_x) * f;
            let by = from_dir_y + (ndy - from_dir_y) * f;
            let len = js_hypot(bx, by);
            if len < 1e-6 {
                return (ndx, ndy); // 180° turn — no meaningful blend
            }
            (bx / len, by / len)
        };
        if r.sample_count == 0 {
            let (bx, by) = dir_at(0.0);
            Self::push_sample(r, x0, y0, -by, bx, 0.0);
        }
        let steps = (seg_len * SAMPLES_PER_TILE).ceil();
        let mut i = 1.0f64;
        while i <= steps {
            let f = i / steps;
            let (bx, by) = dir_at(f);
            let cx = x0 + dx * f;
            let cy = y0 + dy * f;
            let d = r.head_dist + seg_len * f;
            Self::push_sample(r, cx, cy, -by, bx, d);
            i += 1.0;
        }
        r.dir_x = ndx;
        r.dir_y = ndy;
        r.has_dir = true;
        r.head_dist += seg_len;
    }

    /// `pushSample(r, cx, cy, px, py, d)` — f32 writes, double-on-full.
    fn push_sample(r: &mut RibbonState, cx: f64, cy: f64, px: f64, py: f64, d: f64) {
        let off = r.sample_count * SAMPLE_FLOATS;
        if off + SAMPLE_FLOATS > r.samples.len() {
            let mut grown = vec![0.0f32; r.samples.len() * 2];
            grown[..r.samples.len()].copy_from_slice(&r.samples);
            r.samples = grown;
        }
        r.samples[off] = to_float32(cx);
        r.samples[off + 1] = to_float32(cy);
        r.samples[off + 2] = to_float32(px);
        r.samples[off + 3] = to_float32(py);
        r.samples[off + 4] = to_float32(d);
        r.sample_count += 1;
    }
}

/// The capture harness: an op stream over one `SpiralTrails` instance.
#[derive(Debug, Default)]
pub struct RigHarness {
    trails: SpiralTrails,
}

impl RigHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Run one op. Kind table (see `tools/gen_vectors.mjs`):
    /// 0 construct/reset `[mapW]` -> `[0]`;
    /// 1 setParams `[ownerID, radius, strands, rotationSpeed, nc, (r,g,b)*nc]`
    ///   -> `[0]`;
    /// 2 clearParams `[ownerID]` -> `[0]`;
    /// 3 update `[n, (id, tlen, (type)*tlen, ownerID, pos, lastPos)*n, m,
    ///   (trackedId)*m]` -> `[0]`;
    /// 4 dumpRibbons (the live `ribbonList`) -> `[n, (id, radius, strands,
    ///   twist, rotationSpeed, nc, (r,g,b)*nc, headDist, sampleCount,
    ///   samplesLen, (f64 from f32)*sampleCount*5, lastPos, dirX, dirY,
    ///   hasDir)*n]`;
    /// 5 dumpParams -> `[n, (ownerID, radius, strands, rotationSpeed, nc,
    ///   (r,g,b)*nc)*n]`;
    /// 6 constants -> `[MAX_TRAIL_STRANDS, SAMPLE_FLOATS]`.
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut i = 0usize;
        match kind {
            0 => {
                *self = Self::default();
                self.trails.map_w = args[i];
                vec![0.0]
            }
            1 => {
                let owner_id = args[i];
                i += 1;
                let radius = args[i];
                let strands = args[i + 1];
                let rotation_speed = args[i + 2];
                i += 3;
                let nc = args[i] as usize;
                i += 1;
                let mut colors = Vec::with_capacity(nc);
                for _ in 0..nc {
                    colors.push([args[i], args[i + 1], args[i + 2]]);
                    i += 3;
                }
                self.trails.set_params(
                    owner_id,
                    SpiralParams { radius, strands, rotation_speed, colors },
                );
                vec![0.0]
            }
            2 => {
                self.trails.clear_params(args[i]);
                vec![0.0]
            }
            3 => {
                let n = args[i] as usize;
                i += 1;
                let mut units: NumMap<UnitState> = NumMap::default();
                for _ in 0..n {
                    let id = args[i];
                    i += 1;
                    let unit_type = read_str(args, &mut i);
                    let owner_id = args[i];
                    let _pos = args[i + 1];
                    let last_pos = args[i + 2];
                    i += 3;
                    units.set(id, UnitState { unit_type, owner_id, last_pos });
                }
                let m = args[i] as usize;
                i += 1;
                let tracked: Vec<f64> = args[i..i + m].to_vec();
                self.trails.update(&units, &tracked);
                vec![0.0]
            }
            4 => {
                let mut out = Vec::new();
                out.push(self.trails.ribbon_list.len() as f64);
                for id in &self.trails.ribbon_list {
                    let r = self.trails.ribbons_by_id.get(*id).unwrap();
                    out.push(r.id);
                    out.push(r.radius);
                    out.push(r.strands);
                    out.push(r.twist);
                    out.push(r.rotation_speed);
                    out.push(r.colors.len() as f64);
                    for c in &r.colors {
                        out.extend_from_slice(c);
                    }
                    out.push(r.head_dist);
                    out.push(r.sample_count as f64);
                    out.push(r.samples.len() as f64);
                    for s in &r.samples[..r.sample_count * SAMPLE_FLOATS] {
                        out.push(f64::from(*s));
                    }
                    out.push(r.last_pos);
                    out.push(r.dir_x);
                    out.push(r.dir_y);
                    out.push(if r.has_dir { 1.0 } else { 0.0 });
                }
                out
            }
            5 => {
                let mut out = Vec::new();
                out.push(self.trails.params.len() as f64);
                for (owner_id, p) in self.trails.params.iter() {
                    out.push(owner_id);
                    out.push(p.radius);
                    out.push(p.strands);
                    out.push(p.rotation_speed);
                    out.push(p.colors.len() as f64);
                    for c in &p.colors {
                        out.extend_from_slice(c);
                    }
                }
                out
            }
            6 => vec![MAX_TRAIL_STRANDS, SAMPLE_FLOATS as f64],
            k => unreachable!("spiral_trails harness: unknown op kind {k}"),
        }
    }
}
