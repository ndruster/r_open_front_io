//! Port of `src/client/render/gl/Camera.ts` — the stateful 2D pan/zoom
//! camera producing the column-major mat3 (world → clip) for WebGL2.
//!
//! `renderDpr()` (`utils/Dpr.ts`) is host-bound (`window.devicePixelRatio`);
//! ts_load rewrites it to `Math.min(globalThis.__CAM_DPR || 2, 2)` and the
//! capture scripts `__CAM_DPR` immediately before every op that reads it,
//! so the Rust twin takes the RAW dpr as the op's first argument and
//! re-applies the same `|| 2` falsy gate (`0`, `-0`, `NaN` → 2; `-1`
//! survives and caps to `-1` through `Math.min`) and the cap of 2.
//!
//! Faithfulness notes (quirk list):
//!
//! * `resize` triggers `fitMap()` ONLY while `needsInitialFit` is still
//!   true; `setCameraState` clears the flag, so a later resize keeps the
//!   restored state. `fitMap` itself also clears it.
//! * `dirty` is observable through `getMatrix`: the first call recomputes
//!   and CLEARS the flag, later calls return the stored `Float32Array`
//!   unchanged. The dump leads with the pre-call flag so the parity run
//!   pins both the gate and the clear.
//! * `getMatrix` stores through a `Float32Array` — every entry is
//!   `to_float32`-rounded, and `m[6] = -offsetX * sx` keeps the `-0` sign
//!   (Object.is-compared on the wire).
//! * `Math.max` / `Math.min` are the NaN-propagating JS forms
//!   ([`crate::jsnum::js_max`] / [`crate::jsnum::js_min`]): `zoomBy(NaN)`
//!   poisons zoom, then `clampOffset` poisons BOTH offsets
//!   (`min(mapW + NaN, x)` → NaN), which the capture pins.
//! * `focusBBox`'s `padding = 1.4` default is pinned numerically (the
//!   capture calls the TS with four args once and records 1.4 as the
//!   effective fifth).
//! * Division by zero is JS division: `canvasW == 0` yields ±Infinity /
//!   NaN per operand, `zoom == 0` makes `clampOffset`'s half-viewport
//!   ±Infinity (clamps become no-ops) while `getMatrix`'s `sx` collapses
//!   to 0 and `tx` to `-0` for a positive offset.

use crate::jsnum::{js_max, js_min, js_round, to_float32};

/// The scripted-DPR form of `renderDpr()`: `Math.min(raw || 2, 2)`.
fn render_dpr(raw: f64) -> f64 {
    let d = if raw == 0.0 || raw.is_nan() { 2.0 } else { raw };
    js_min(d, 2.0)
}

/// The ported `Camera` object.
pub struct Cam {
    offset_x: f64,
    offset_y: f64,
    zoom: f64,
    map_w: f64,
    map_h: f64,
    canvas_w: f64,
    canvas_h: f64,
    mat: [f32; 9],
    dirty: bool,
    needs_initial_fit: bool,
}

impl Cam {
    fn new_cam(map_w: f64, map_h: f64) -> Cam {
        Cam {
            offset_x: map_w / 2.0,
            offset_y: map_h / 2.0,
            zoom: 1.0,
            map_w,
            map_h,
            canvas_w: 1.0,
            canvas_h: 1.0,
            mat: [0.0f32; 9],
            dirty: true,
            needs_initial_fit: true,
        }
    }

    fn resize(&mut self, dpr_raw: f64, css_w: f64, css_h: f64) {
        let dpr = render_dpr(dpr_raw);
        self.canvas_w = js_round(css_w * dpr);
        self.canvas_h = js_round(css_h * dpr);
        if self.needs_initial_fit {
            self.fit_map();
        }
        self.dirty = true;
    }

    fn fit_map(&mut self) {
        self.offset_x = self.map_w / 2.0;
        self.offset_y = self.map_h / 2.0;
        let sx = self.canvas_w / self.map_w;
        let sy = self.canvas_h / self.map_h;
        self.zoom = js_min(sx, sy) * 0.9;
        self.dirty = true;
        self.needs_initial_fit = false;
    }

    fn focus_bbox(&mut self, min_x: f64, min_y: f64, max_x: f64, max_y: f64, padding: f64) {
        self.offset_x = (min_x + max_x + 1.0) / 2.0;
        self.offset_y = (min_y + max_y + 1.0) / 2.0;
        let bbox_w = max_x - min_x + 1.0;
        let bbox_h = max_y - min_y + 1.0;
        let sx = self.canvas_w / bbox_w;
        let sy = self.canvas_h / bbox_h;
        self.zoom = js_max(0.7, js_min(3.0, js_min(sx, sy) / padding));
        self.clamp_offset();
        self.dirty = true;
    }

    fn pan_to(&mut self, world_x: f64, world_y: f64) {
        self.offset_x = world_x;
        self.offset_y = world_y;
        self.clamp_offset();
        self.dirty = true;
    }

    fn pan_by(&mut self, dx: f64, dy: f64) {
        self.offset_x += dx;
        self.offset_y += dy;
        self.clamp_offset();
        self.dirty = true;
    }

    fn set_camera_state(&mut self, x: f64, y: f64, z: f64) {
        self.offset_x = x;
        self.offset_y = y;
        self.zoom = z;
        self.needs_initial_fit = false;
        self.dirty = true;
    }

    fn zoom_by(&mut self, factor: f64) {
        self.zoom = js_max(0.2, js_min(20.0, self.zoom * factor));
        self.clamp_offset();
        self.dirty = true;
    }

    fn zoom_to(&mut self, level: f64) {
        self.zoom = js_max(0.2, js_min(20.0, level));
        self.clamp_offset();
        self.dirty = true;
    }

    fn zoom_at_screen(&mut self, dpr_raw: f64, factor: f64, screen_x: f64, screen_y: f64) {
        let world_before = self.screen_to_world(dpr_raw, screen_x, screen_y);
        self.zoom = js_max(0.2, js_min(20.0, self.zoom * factor));
        let world_after = self.screen_to_world(dpr_raw, screen_x, screen_y);
        self.offset_x += world_before.0 - world_after.0;
        self.offset_y += world_before.1 - world_after.1;
        self.clamp_offset();
        self.dirty = true;
    }

    /// `getMatrix()` — the 9 stored f32 entries widened to f64 (the
    /// capture's res[0] carries the pre-call `dirty` flag, snapshotted by
    /// the caller).
    fn get_matrix(&mut self) -> [f64; 9] {
        if self.dirty {
            let sx = (self.zoom * 2.0) / self.canvas_w;
            let sy = (self.zoom * -2.0) / self.canvas_h;
            let tx = -self.offset_x * sx;
            let ty = -self.offset_y * sy;
            let m = &mut self.mat;
            m[0] = to_float32(sx);
            m[1] = to_float32(0.0);
            m[2] = to_float32(0.0);
            m[3] = to_float32(0.0);
            m[4] = to_float32(sy);
            m[5] = to_float32(0.0);
            m[6] = to_float32(tx);
            m[7] = to_float32(ty);
            m[8] = to_float32(1.0);
            self.dirty = false;
        }
        let mut out = [0.0f64; 9];
        for (o, v) in out.iter_mut().zip(self.mat.iter()) {
            *o = *v as f64;
        }
        out
    }

    fn screen_to_world(&self, dpr_raw: f64, screen_x: f64, screen_y: f64) -> (f64, f64) {
        let dpr = render_dpr(dpr_raw);
        let ndc_x = ((screen_x * dpr) / self.canvas_w) * 2.0 - 1.0;
        let ndc_y = -(((screen_y * dpr) / self.canvas_h) * 2.0 - 1.0);
        let sx = (self.zoom * 2.0) / self.canvas_w;
        let sy = (self.zoom * -2.0) / self.canvas_h;
        (
            (ndc_x - -self.offset_x * sx) / sx,
            (ndc_y - -self.offset_y * sy) / sy,
        )
    }

    fn world_to_screen(&self, dpr_raw: f64, world_x: f64, world_y: f64) -> (f64, f64) {
        let dpr = render_dpr(dpr_raw);
        (
            (self.zoom * (world_x - self.offset_x)) / dpr + self.canvas_w / (2.0 * dpr),
            (self.zoom * (world_y - self.offset_y)) / dpr + self.canvas_h / (2.0 * dpr),
        )
    }

    fn clamp_offset(&mut self) {
        let half_vp_w = self.canvas_w / (2.0 * self.zoom);
        let half_vp_h = self.canvas_h / (2.0 * self.zoom);
        self.offset_x = js_max(-half_vp_w, js_min(self.map_w + half_vp_w, self.offset_x));
        self.offset_y = js_max(-half_vp_h, js_min(self.map_h + half_vp_h, self.offset_y));
    }

    fn dump(&self) -> Vec<f64> {
        vec![
            self.offset_x,
            self.offset_y,
            self.zoom,
            self.map_w,
            self.map_h,
            self.canvas_w,
            self.canvas_h,
            self.dirty as u8 as f64,
            self.needs_initial_fit as u8 as f64,
        ]
    }
}

/// Harness over a single `Camera` instance (one per scenario; the parity
/// runner resets between scenarios).
#[derive(Default)]
pub struct RigHarness {
    cam: Option<Cam>,
}

impl RigHarness {
    pub fn new() -> Self {
        RigHarness { cam: None }
    }

    pub fn reset(&mut self) {
        self.cam = None;
    }

    /// kind table (mirrors the capture):
    ///   0 construct        [mapW, mapH]                  -> dump
    ///   1 resize           [dprRaw, cssW, cssH]          -> dump
    ///   2 fitMap           []                            -> dump
    ///   3 focusBBox        [minX, minY, maxX, maxY, padding] -> dump
    ///   4 panTo            [x, y]                        -> dump
    ///   5 panBy            [dx, dy]                      -> dump
    ///   6 setCameraState   [x, y, z]                     -> dump
    ///   7 zoomBy           [factor]                      -> dump
    ///   8 zoomTo           [level]                       -> dump
    ///   9 zoomAtScreen     [dprRaw, factor, sx, sy]      -> dump
    ///  10 getMatrix        [] -> [dirtyAfterClear, m0..m8]  (see below)
    ///  11 screenToWorld    [dprRaw, sx, sy]              -> [x, y]
    ///  12 worldToScreen    [dprRaw, wx, wy]              -> [x, y]
    ///  13 dump             []                            -> dump
    ///
    /// dump = [offsetX, offsetY, zoom, mapW, mapH, canvasW, canvasH,
    /// dirty, needsInitialFit].
    ///
    /// kind 10: the capture records `[dirty_before, m0..m8]` — the flag as
    /// observed BEFORE the call (pinning the recompute gate), then the
    /// stored f32 entries.
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut i = 0usize;
        let mut next = || {
            let v = args.get(i).copied().unwrap_or(f64::NAN);
            i += 1;
            v
        };
        match kind {
            0 => {
                let (w, h) = (next(), next());
                self.cam = Some(Cam::new_cam(w, h));
                self.cam.as_ref().unwrap().dump()
            }
            1 => {
                let (d, w, h) = (next(), next(), next());
                self.cam.as_mut().unwrap().resize(d, w, h);
                self.cam.as_ref().unwrap().dump()
            }
            2 => {
                self.cam.as_mut().unwrap().fit_map();
                self.cam.as_ref().unwrap().dump()
            }
            3 => {
                let (a, b, c, e, p) = (next(), next(), next(), next(), next());
                self.cam.as_mut().unwrap().focus_bbox(a, b, c, e, p);
                self.cam.as_ref().unwrap().dump()
            }
            4 => {
                let (x, y) = (next(), next());
                self.cam.as_mut().unwrap().pan_to(x, y);
                self.cam.as_ref().unwrap().dump()
            }
            5 => {
                let (dx, dy) = (next(), next());
                self.cam.as_mut().unwrap().pan_by(dx, dy);
                self.cam.as_ref().unwrap().dump()
            }
            6 => {
                let (x, y, z) = (next(), next(), next());
                self.cam.as_mut().unwrap().set_camera_state(x, y, z);
                self.cam.as_ref().unwrap().dump()
            }
            7 => {
                let f = next();
                self.cam.as_mut().unwrap().zoom_by(f);
                self.cam.as_ref().unwrap().dump()
            }
            8 => {
                let l = next();
                self.cam.as_mut().unwrap().zoom_to(l);
                self.cam.as_ref().unwrap().dump()
            }
            9 => {
                let (d, f, sx, sy) = (next(), next(), next(), next());
                self.cam.as_mut().unwrap().zoom_at_screen(d, f, sx, sy);
                self.cam.as_ref().unwrap().dump()
            }
            10 => {
                let cam = self.cam.as_mut().unwrap();
                let before = cam.dirty as u8 as f64;
                let m = cam.get_matrix();
                let mut out = Vec::with_capacity(10);
                out.push(before);
                out.extend_from_slice(&m);
                out
            }
            11 => {
                let (d, sx, sy) = (next(), next(), next());
                let (x, y) = self.cam.as_ref().unwrap().screen_to_world(d, sx, sy);
                vec![x, y]
            }
            12 => {
                let (d, wx, wy) = (next(), next(), next());
                let (x, y) = self.cam.as_ref().unwrap().world_to_screen(d, wx, wy);
                vec![x, y]
            }
            13 => self.cam.as_ref().unwrap().dump(),
            k => unreachable!("camera: unknown op kind {k}"),
        }
    }
}
