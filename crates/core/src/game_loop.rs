//! G6-0 (M1-0) — integration harness (`game_loop`): the real-assembly layer
//! the M1 milestone grows into the tick-loop parity rig.
//!
//! Unlike the G4a/G4b `game_impl` harness (scripted facades + op-by-op
//! traces), this module assembles REAL ported components — [`GameMap`] and
//! [`WaterManager`], with the map living inside the water manager exactly as
//! the G4a `debug_map` convention — and compares aggregate OUTPUT streams.
//! The recorder (`tools/gen_m1_vectors.mjs`) drives the identical real TS
//! classes (`GameMapImpl` + `WaterManager`) over the same synthetic terrain
//! bytes and pins the whole per-op result stream. Both the native test
//! (`tests/parity_m1.rs`) and the wasm scan (`tools/m1_scan.mjs`) replay it
//! bit-for-bit.
//!
//! M1a will extend the construct with a real [`crate::config::Config`] and
//! real players; M1b adds the real `executeNextTick` body (the G4b mid-300
//! script-facade version stays frozen). Nothing here traces facade calls —
//! the comparison payload is state digests and packed update drains.
//!
//! ## Op table (`run_op(kind, args)`)
//!
//! | kind | op | args | result payload |
//! |------|----|------|----------------|
//! | 0 | construct | `w, h, numLand, miniW, miniH, miniNumLand, terrainLen, ..terrain, miniLen, ..mini` | `[]` |
//! | 1 | `setFallout` (TS 230-240) | `tile, value` | `[]` |
//! | 2 | `setWater` (TS 242-254) | `tile` | `[]` |
//! | 3 | `queueWaterConversion` (TS 256-266) | `tile` | `[]` |
//! | 4 | water tick | `ticks` | `n, ..changedTiles` |
//! | 5 | drain packed tile updates | — | `len, ..pairs` |
//! | 6 | map digest | — | `territoryVersion, ticks, numLand, waterVersion, falloutCount, waterGraphVersion, ..perTilePacked` |
//!
//! Result convention: `[0, ..payload]` for a return / `undefined`, or
//! `[1, ..encS(message)]` for a throw — same tag pair as `game_impl`. Error
//! strings are the verbatim TS messages. Payloads are RAW numbers (no JsVal
//! wrapping): the M1-0 comparison domain is flat numeric streams.
//!
//! ## M1-0 pins
//!
//! * `Config` does not exist in the assembly yet, so
//!   `queueWaterConversion` runs with `config.waterNukes()` PINNED `true`
//!   (the `!waterNukes()` fallback arm is exactly op 1 with `value=1`).
//! * No players exist yet, so the `hasOwner` throw arms of ops 1-3 are
//!   unreachable — their `hasOwner` reads still run against the real map
//!   (always false) and the arms arrive with the M1a players.

use crate::game_map::GameMap;
use crate::js_json::push_str;
use crate::water_manager::WaterManager;

pub struct GameLoopHarness {
    water: Option<WaterManager>,
    territory_version: f64,
    ticks: f64,
    tile_updates: Vec<f64>,
}

impl Default for GameLoopHarness {
    fn default() -> Self {
        Self {
            water: None,
            territory_version: 0.0,
            ticks: 0.0,
            tile_updates: Vec::new(),
        }
    }
}

/// Flat arg cursor over one op's `args` (mirrors the `C` helper of the other
/// harnesses).
struct C<'a> {
    a: &'a [f64],
    i: usize,
}

impl<'a> C<'a> {
    fn f(&mut self) -> f64 {
        let v = self.a[self.i];
        self.i += 1;
        v
    }
    fn done(&self) -> bool {
        self.i >= self.a.len()
    }
}

impl GameLoopHarness {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// The shared real map (owned by the water manager, G4a convention).
    fn gm(&self) -> &GameMap {
        self.water.as_ref().expect("map built").debug_map()
    }
    fn gm_mut(&mut self) -> &mut GameMap {
        self.water.as_mut().expect("map built").debug_map_mut()
    }

    /// `GameImpl.recordTileUpdate` (TS 548-555): low 16 bits tile state,
    /// bits 16-23 terrain byte.
    fn record_tile_update(&mut self, tile: f64) {
        let state = self.gm().tile_state(tile).expect("valid ref") as u16;
        let terrain = self.gm().terrain_byte(tile).expect("valid ref") as u16;
        self.tile_updates.push(tile);
        self.tile_updates
            .push(((state as u32) | ((terrain as u32) << 16)) as f64);
    }

    /// `GameImpl.setFallout` (TS 230-240).
    fn op_set_fallout(&mut self, tile: f64, value: bool, o: &mut Vec<f64>) {
        if value && self.gm().has_owner(tile) {
            push_err(o, &format!("cannot set fallout, tile {tile} has owner"));
            return;
        }
        if self.gm().has_fallout(tile) {
            o.push(0.0);
            return;
        }
        self.territory_version += 1.0;
        self.gm_mut().set_fallout(tile, value);
        self.record_tile_update(tile);
        o.push(0.0);
    }

    /// `GameImpl.setWater` (TS 242-254).
    fn op_set_water(&mut self, tile: f64, o: &mut Vec<f64>) {
        if !self.gm().is_land(tile) {
            o.push(0.0);
            return;
        }
        if self.gm().has_owner(tile) {
            push_err(o, &format!("cannot set water, tile {tile} has owner"));
            return;
        }
        if self.gm().has_fallout(tile) {
            self.gm_mut().set_fallout(tile, false);
        }
        self.territory_version += 1.0;
        self.gm_mut().set_water(tile);
        self.record_tile_update(tile);
        o.push(0.0);
    }

    /// `GameImpl.queueWaterConversion` (TS 256-266) with `waterNukes()`
    /// pinned `true` (M1-0 pin — the fallback arm is op 1 with `value=1`).
    fn op_queue_water(&mut self, tile: f64, o: &mut Vec<f64>) {
        if !self.gm().is_land(tile) {
            o.push(0.0);
            return;
        }
        if self.gm().has_owner(tile) {
            push_err(
                o,
                &format!("cannot queue water conversion, tile {tile} has owner"),
            );
            return;
        }
        self.water
            .as_mut()
            .expect("map built")
            .queue_tile(tile);
        o.push(0.0);
    }

    /// Water-manager tick flush; every changed tile is recorded like
    /// `executeNextTick`'s flush loop (TS 540-543).
    fn op_water_tick(&mut self, ticks: f64, o: &mut Vec<f64>) {
        self.ticks = ticks;
        let changed = self.water.as_mut().expect("map built").tick(ticks);
        for t in &changed {
            self.record_tile_update(*t);
        }
        o.push(0.0);
        o.push(changed.len() as f64);
        for t in changed {
            o.push(t);
        }
    }

    fn op_drain(&mut self, o: &mut Vec<f64>) {
        let pairs = std::mem::take(&mut self.tile_updates);
        o.push(0.0);
        o.push(pairs.len() as f64);
        for p in pairs {
            o.push(p);
        }
    }

    /// Whole-map digest: counters plus the per-tile `recordTileUpdate`
    /// packing over every ref in order. `waterGraphVersion` rides along so
    /// the throttled graph rebuild (at most once every 20 ticks) is
    /// observable without reaching into the water manager's private state.
    fn op_digest(&self, o: &mut Vec<f64>) {
        let m = self.gm();
        let n = (m.width() * m.height()) as usize;
        o.push(0.0);
        o.push(self.territory_version);
        o.push(self.ticks);
        o.push(m.num_land_tiles());
        o.push(m.water_version());
        o.push(m.num_tiles_with_fallout());
        o.push(
            self.water
                .as_ref()
                .expect("map built")
                .water_graph_version(),
        );
        for i in 0..n {
            let t = i as f64;
            let state = m.tile_state(t).expect("valid ref") as u16;
            let terrain = m.terrain_byte(t).expect("valid ref") as u16;
            o.push(((state as u32) | ((terrain as u32) << 16)) as f64);
        }
    }

    fn construct(&mut self, a: &[f64], o: &mut Vec<f64>) {
        let mut c = C { a, i: 0 };
        let w = c.f();
        let h = c.f();
        let nl = c.f();
        let mw = c.f();
        let mh = c.f();
        let mnl = c.f();
        let tn = c.f() as usize;
        let terrain: Vec<u8> = (0..tn).map(|_| c.f() as u8).collect();
        let mtn = c.f() as usize;
        let mterrain: Vec<u8> = (0..mtn).map(|_| c.f() as u8).collect();
        if !c.done() {
            panic!("gl construct: trailing args");
        }
        let map = GameMap::new(w, h, terrain, nl);
        let mini = GameMap::new(mw, mh, mterrain, mnl);
        self.water = Some(WaterManager::new(map, mini, false));
        self.territory_version = 0.0;
        self.ticks = 0.0;
        self.tile_updates.clear();
        o.push(0.0);
    }

    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut o: Vec<f64> = Vec::new();
        match kind {
            0 => self.construct(args, &mut o),
            1 => {
                let mut c = C { a: args, i: 0 };
                let tile = c.f();
                let value = c.f() != 0.0;
                self.op_set_fallout(tile, value, &mut o);
            }
            2 => {
                let mut c = C { a: args, i: 0 };
                let tile = c.f();
                self.op_set_water(tile, &mut o);
            }
            3 => {
                let mut c = C { a: args, i: 0 };
                let tile = c.f();
                self.op_queue_water(tile, &mut o);
            }
            4 => {
                let mut c = C { a: args, i: 0 };
                let ticks = c.f();
                self.op_water_tick(ticks, &mut o);
            }
            5 => self.op_drain(&mut o),
            6 => self.op_digest(&mut o),
            _ => panic!("gl: unknown kind {kind}"),
        }
        o
    }
}

fn push_err(o: &mut Vec<f64>, m: &str) {
    o.push(1.0);
    push_str(o, m);
}
