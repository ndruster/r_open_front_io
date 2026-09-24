//! Port of `src/core/game/GameMap.ts` — `GameMapImpl` plus the distance
//! filter factories (`euclDistFN`, `manhattanDistFN`, `rectDistFN`,
//! `isometricDistFN`, `hexDistFN`).
//!
//! Faithfulness notes — every one of these is observable through the parity
//! traces in `gen_vectors.mjs`:
//!
//! * `terrain` is a `Uint8Array` and `state` a `Uint16Array`. Reads at
//!   negative / fractional / out-of-range refs yield `undefined`, whose bit
//!   tests see `0` — so invalid refs read as ownerless plain water. Writes at
//!   those indices are *dropped*, but the surrounding bookkeeping (counters,
//!   `waterVersion`, `updateTile`'s return value) still runs exactly as in JS.
//! * `x()` uses JS `%` (sign follows the dividend, `-0` preserved); `y()`
//!   uses `| 0` (ToInt32, which yields `-0` for negative fractions), not
//!   `Math.floor`.
//! * `bfs` uses `q.pop()` — a LIFO stack, so the traversal is depth-first
//!   with the N, S, W, E neighbour order.
//! * `Set` iteration order is insertion order; `circleSearch` and `bfs`
//!   return their tiles in that order (deduped with SameValueZero: `NaN`
//!   matches `NaN`, `-0` matches `0`).
//! * `ref()` and `setOwnerID()` `throw` in TS; the port panics with the same
//!   messages, and the throw paths are pinned by the replay tests.
//! * `cell()` is not ported yet — `Cell` lives in `Game.ts`.

use std::collections::HashSet;

use crate::jsnum::{to_int32, to_uint32};

/// `TerrainType` from `src/core/game/Game.ts` (declaration order = 0..4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainType {
    Plains,
    Highland,
    Mountain,
    Ocean,
    Impassable,
}

// Terrain bits (Uint8Array)
const IS_LAND_BIT: u8 = 1 << 7;
const SHORELINE_BIT: u8 = 1 << 6;
const OCEAN_BIT: u8 = 1 << 5;
const MAGNITUDE_MASK: u8 = 0x1f;
const IMPASSABLE_MAGNITUDE: u8 = 31;

// State bits (Uint16Array)
const PLAYER_ID_MASK: u16 = 0xfff;
const FALLOUT_BIT: u16 = 1 << 13;
const DEFENSE_BONUS_BIT: u16 = 1 << 14;

/// SameValueZero key for `Set` membership: `-0` and `0` collapse, `NaN`
/// matches itself.
fn sz_key(v: f64) -> u64 {
    if v == 0.0 {
        0
    } else {
        v.to_bits()
    }
}

/// `Math.max` with JS NaN propagation (Rust's `f64::max` ignores NaN) and the
/// zero rule: equal zeros yield `+0` unless *both* are `-0`.
pub(crate) fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a > b {
        a
    } else if a < b {
        b
    } else if a == 0.0 {
        // Equal and zero: Math.max -> -0 only when both sides are -0.
        if a.is_sign_negative() && b.is_sign_negative() {
            -0.0
        } else {
            0.0
        }
    } else {
        a
    }
}

/// `Math.min` with JS NaN propagation and the zero rule: equal zeros yield
/// `-0` unless *both* are `+0`.
pub(crate) fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a < b {
        a
    } else if a > b {
        b
    } else if a == 0.0 {
        // Equal and zero: Math.min -> +0 only when both sides are +0.
        if a.is_sign_negative() || b.is_sign_negative() {
            -0.0
        } else {
            0.0
        }
    } else {
        a
    }
}

pub struct GameMap {
    terrain: Vec<u8>,
    state: Vec<u16>,
    width_: f64,
    height_: f64,
    y_to_ref: Vec<i32>,
    num_land_tiles_: f64,
    water_version_: f64,
    num_tiles_with_fallout_: f64,
}

impl GameMap {
    /// `new GameMapImpl(width, height, terrainData, numLandTiles_)`.
    ///
    /// Panics (mirrors the TS `throw`) when the terrain length does not match
    /// `width * height`.
    pub fn new(width: f64, height: f64, terrain_data: Vec<u8>, num_land_tiles: f64) -> Self {
        if terrain_data.len() as f64 != width * height {
            panic!(
                "Terrain data length {} doesn't match dimensions {}x{}",
                terrain_data.len(),
                width,
                height
            );
        }
        let mut y_to_ref = vec![0i32; height as usize];
        for (y, slot) in y_to_ref.iter_mut().enumerate() {
            // Int32Array write: ToInt32 wrap of `y * width`.
            *slot = to_int32(y as f64 * width);
        }
        Self {
            terrain: terrain_data,
            state: vec![0u16; (width * height) as usize],
            width_: width,
            height_: height,
            y_to_ref,
            num_land_tiles_: num_land_tiles,
            water_version_: 0.0,
            num_tiles_with_fallout_: 0.0,
        }
    }

    // ---- typed-array element access with JS semantics ----

    /// Canonical `Uint8Array` index for `terrain[tile]`, or `None` when JS
    /// would read `undefined` / drop the write.
    #[inline]
    fn tidx(&self, tile: f64) -> Option<usize> {
        if !tile.is_finite() || tile.fract() != 0.0 || tile < 0.0 {
            return None;
        }
        let i = tile as usize;
        if i < self.terrain.len() {
            Some(i)
        } else {
            None
        }
    }

    #[inline]
    fn sidx(&self, tile: f64) -> Option<usize> {
        if !tile.is_finite() || tile.fract() != 0.0 || tile < 0.0 {
            return None;
        }
        let i = tile as usize;
        if i < self.state.len() {
            Some(i)
        } else {
            None
        }
    }

    #[inline]
    fn tget(&self, tile: f64) -> Option<u8> {
        self.tidx(tile).map(|i| self.terrain[i])
    }

    #[inline]
    fn sget(&self, tile: f64) -> Option<u16> {
        self.sidx(tile).map(|i| self.state[i])
    }

    /// `terrain[tile] = v` — dropped for non-canonical indices.
    #[inline]
    fn tput(&mut self, tile: f64, v: u8) {
        if let Some(i) = self.tidx(tile) {
            self.terrain[i] = v;
        }
    }

    /// `state[tile] = v` — dropped for non-canonical indices. The `i32 ->
    /// u16` truncation matches the `Uint16Array` `ToUint16` write.
    #[inline]
    fn sput(&mut self, tile: f64, v: i32) {
        if let Some(i) = self.sidx(tile) {
            self.state[i] = v as u16;
        }
    }

    #[inline]
    fn y_to_ref_at(&self, j: f64) -> Option<i32> {
        if !j.is_finite() || j.fract() != 0.0 || j < 0.0 {
            return None;
        }
        self.y_to_ref.get(j as usize).copied()
    }

    // ---- geometry ----

    /// `ref(x, y)` — panics on invalid coordinates, like the TS `throw`.
    pub fn tile_ref(&self, x: f64, y: f64) -> f64 {
        if !self.is_valid_coord(x, y) {
            panic!("Invalid coordinates: {},{}", x, y);
        }
        self.y_to_ref_at(y).unwrap_or(0) as f64 + x
    }

    pub fn is_valid_ref(&self, tile: f64) -> bool {
        tile.is_finite() && tile.fract() == 0.0 && tile >= 0.0 && tile < self.width_ * self.height_
    }

    pub fn x(&self, tile: f64) -> f64 {
        tile % self.width_ // JS %: sign follows the dividend, -0 preserved
    }

    /// `(ref / width) | 0` — ToInt32 truncation toward zero (wraps past
    /// 2^31), never `-0`: bitwise operators always produce a plain integer.
    pub fn y(&self, tile: f64) -> f64 {
        to_int32(tile / self.width_) as f64
    }

    pub fn width(&self) -> f64 {
        self.width_
    }
    pub fn height(&self) -> f64 {
        self.height_
    }
    pub fn num_land_tiles(&self) -> f64 {
        self.num_land_tiles_
    }
    pub fn water_version(&self) -> f64 {
        self.water_version_
    }
    pub fn num_tiles_with_fallout(&self) -> f64 {
        self.num_tiles_with_fallout_
    }

    pub fn is_valid_coord(&self, x: f64, y: f64) -> bool {
        x.is_finite()
            && x.fract() == 0.0
            && y.is_finite()
            && y.fract() == 0.0
            && x >= 0.0
            && x < self.width_
            && y >= 0.0
            && y < self.height_
    }

    // ---- terrain getters (immutable) ----

    pub fn is_land(&self, tile: f64) -> bool {
        self.tget(tile).is_some_and(|b| b & IS_LAND_BIT != 0)
    }

    pub fn is_impassable(&self, tile: f64) -> bool {
        self.is_land(tile)
            && self.tget(tile).map_or(0, |b| b & MAGNITUDE_MASK) == IMPASSABLE_MAGNITUDE
    }

    pub fn is_ocean_shore(&self, tile: f64) -> bool {
        if !self.is_land(tile) {
            return false;
        }
        let w = self.width_;
        let x = tile % w;
        if x != 0.0 && self.is_ocean(tile - 1.0) {
            return true;
        }
        if x != w - 1.0 && self.is_ocean(tile + 1.0) {
            return true;
        }
        if tile >= w && self.is_ocean(tile - w) {
            return true;
        }
        if tile < (self.height_ - 1.0) * w && self.is_ocean(tile + w) {
            return true;
        }
        false
    }

    pub fn is_ocean(&self, tile: f64) -> bool {
        self.tget(tile).is_some_and(|b| b & OCEAN_BIT != 0)
    }

    pub fn is_shoreline(&self, tile: f64) -> bool {
        self.tget(tile).is_some_and(|b| b & SHORELINE_BIT != 0)
    }

    pub fn magnitude(&self, tile: f64) -> f64 {
        // `undefined & 31` is 0.
        f64::from(self.tget(tile).map_or(0, |b| b & MAGNITUDE_MASK))
    }

    /// `terrain[ref]` — `None` where JS yields `undefined`.
    pub fn terrain_byte(&self, tile: f64) -> Option<f64> {
        self.tget(tile).map(|b| b as f64)
    }

    // ---- terrain setters ----

    pub fn set_water(&mut self, tile: f64) {
        if !self.is_land(tile) || self.is_impassable(tile) {
            return;
        }
        self.water_version_ += 1.0;
        self.tput(tile, 0); // lake water: no bits set
        self.num_land_tiles_ -= 1.0;
    }

    pub fn set_shoreline_bit(&mut self, tile: f64) {
        let v = self.tget(tile).unwrap_or(0) | SHORELINE_BIT;
        self.tput(tile, v);
    }

    pub fn clear_shoreline_bit(&mut self, tile: f64) {
        let v = self.tget(tile).unwrap_or(0) & !SHORELINE_BIT;
        self.tput(tile, v);
    }

    pub fn set_ocean(&mut self, tile: f64) {
        let v = self.tget(tile).unwrap_or(0) | OCEAN_BIT;
        self.tput(tile, v);
    }

    pub fn set_magnitude(&mut self, tile: f64, value: f64) {
        let keep = self.tget(tile).unwrap_or(0) & !MAGNITUDE_MASK;
        let v = keep | (to_int32(value) as u8 & MAGNITUDE_MASK);
        self.tput(tile, v);
    }

    // ---- state getters and setters ----

    pub fn owner_id(&self, tile: f64) -> f64 {
        f64::from(self.sget(tile).map_or(0, |s| s & PLAYER_ID_MASK))
    }

    pub fn has_owner(&self, tile: f64) -> bool {
        self.owner_id(tile) != 0.0
    }

    /// Panics (mirrors the TS `throw`) when `playerId > 0xfff`.
    pub fn set_owner_id(&mut self, tile: f64, player_id: f64) {
        if player_id > PLAYER_ID_MASK as f64 {
            panic!("Player ID {} exceeds maximum value {}", player_id, PLAYER_ID_MASK);
        }
        let keep = i32::from(self.sget(tile).unwrap_or(0)) & !(PLAYER_ID_MASK as i32);
        self.sput(tile, keep | to_int32(player_id));
    }

    pub fn has_fallout(&self, tile: f64) -> bool {
        self.sget(tile).is_some_and(|s| s & FALLOUT_BIT != 0)
    }

    pub fn set_fallout(&mut self, tile: f64, value: bool) {
        let existing = self.has_fallout(tile);
        if value {
            if !existing {
                self.num_tiles_with_fallout_ += 1.0;
                let v = i32::from(self.sget(tile).unwrap_or(0)) | FALLOUT_BIT as i32;
                self.sput(tile, v);
            }
        } else if existing {
            self.num_tiles_with_fallout_ -= 1.0;
            let v = i32::from(self.sget(tile).unwrap_or(0)) & !(FALLOUT_BIT as i32);
            self.sput(tile, v);
        }
    }

    pub fn is_on_edge_of_map(&self, tile: f64) -> bool {
        let w = self.width_;
        let x = tile % w;
        if x == 0.0 || x == w - 1.0 || tile < w || tile >= (self.height_ - 1.0) * w {
            return true;
        }
        self.is_impassable(tile - 1.0)
            || self.is_impassable(tile + 1.0)
            || self.is_impassable(tile - w)
            || self.is_impassable(tile + w)
    }

    pub fn is_border(&self, tile: f64) -> bool {
        let w = self.width_;
        let x = tile % w;
        let owner = self.owner_id(tile);
        if x != 0.0 && self.owner_id(tile - 1.0) != owner {
            return true;
        }
        if x != w - 1.0 && self.owner_id(tile + 1.0) != owner {
            return true;
        }
        if tile >= w && self.owner_id(tile - w) != owner {
            return true;
        }
        if tile < (self.height_ - 1.0) * w && self.owner_id(tile + w) != owner {
            return true;
        }
        false
    }

    pub fn has_defense_bonus(&self, tile: f64) -> bool {
        self.sget(tile).is_some_and(|s| s & DEFENSE_BONUS_BIT != 0)
    }

    pub fn set_defense_bonus(&mut self, tile: f64, value: bool) {
        let cur = i32::from(self.sget(tile).unwrap_or(0));
        let v = if value {
            cur | DEFENSE_BONUS_BIT as i32
        } else {
            cur & !(DEFENSE_BONUS_BIT as i32)
        };
        self.sput(tile, v);
    }

    // ---- helpers ----

    pub fn is_water(&self, tile: f64) -> bool {
        !self.is_land(tile)
    }

    pub fn is_shore(&self, tile: f64) -> bool {
        self.is_land(tile) && self.is_shoreline(tile)
    }

    pub fn cost(&self, tile: f64) -> f64 {
        if self.magnitude(tile) < 10.0 {
            2.0
        } else {
            1.0
        }
    }

    pub fn terrain_type(&self, tile: f64) -> TerrainType {
        if self.is_land(tile) {
            let magnitude = self.magnitude(tile);
            if magnitude >= f64::from(IMPASSABLE_MAGNITUDE) {
                return TerrainType::Impassable;
            }
            if magnitude < 10.0 {
                return TerrainType::Plains;
            }
            if magnitude < 20.0 {
                return TerrainType::Highland;
            }
            return TerrainType::Mountain;
        }
        TerrainType::Ocean
    }

    // ---- neighbours ----

    pub fn neighbors(&self, tile: f64) -> Vec<f64> {
        let mut out = Vec::new();
        let w = self.width_;
        let x = tile % w;
        if tile >= w {
            out.push(tile - w);
        }
        if tile < (self.height_ - 1.0) * w {
            out.push(tile + w);
        }
        if x != 0.0 {
            out.push(tile - 1.0);
        }
        if x != w - 1.0 {
            out.push(tile + 1.0);
        }
        out
    }

    pub fn for_each_neighbor<F: FnMut(f64)>(&self, tile: f64, mut cb: F) {
        let w = self.width_;
        let x = tile % w;
        if tile >= w {
            cb(tile - w);
        }
        if tile < (self.height_ - 1.0) * w {
            cb(tile + w);
        }
        if x != 0.0 {
            cb(tile - 1.0);
        }
        if x != w - 1.0 {
            cb(tile + 1.0);
        }
    }

    /// Cardinal neighbours into `out` (length >= 4); returns the count.
    pub fn neighbors4(&self, tile: f64, out: &mut [f64]) -> usize {
        let w = self.width_;
        let x = tile % w;
        let mut n = 0;
        if tile >= w {
            out[n] = tile - w;
            n += 1;
        }
        if tile < (self.height_ - 1.0) * w {
            out[n] = tile + w;
            n += 1;
        }
        if x != 0.0 {
            out[n] = tile - 1.0;
            n += 1;
        }
        if x != w - 1.0 {
            out[n] = tile + 1.0;
            n += 1;
        }
        n
    }

    /// 8-neighbours into `out` (length >= 8), W-diagonal block, N, S, then
    /// the E-diagonal block; returns the count.
    pub fn neighbors8(&self, tile: f64, out: &mut [f64]) -> usize {
        let w = self.width_;
        let x = tile % w;
        let has_n = tile >= w;
        let has_s = tile < (self.height_ - 1.0) * w;
        let mut n = 0;
        if x != 0.0 {
            if has_n {
                out[n] = tile - 1.0 - w;
                n += 1;
            }
            out[n] = tile - 1.0;
            n += 1;
            if has_s {
                out[n] = tile - 1.0 + w;
                n += 1;
            }
        }
        if has_n {
            out[n] = tile - w;
            n += 1;
        }
        if has_s {
            out[n] = tile + w;
            n += 1;
        }
        if x != w - 1.0 {
            if has_n {
                out[n] = tile + 1.0 - w;
                n += 1;
            }
            out[n] = tile + 1.0;
            n += 1;
            if has_s {
                out[n] = tile + 1.0 + w;
                n += 1;
            }
        }
        n
    }

    pub fn for_each_neighbor_with_diag<F: FnMut(f64)>(&self, tile: f64, mut cb: F) {
        let w = self.width_;
        let x = tile % w;
        let has_n = tile >= w;
        let has_s = tile < (self.height_ - 1.0) * w;
        if x != 0.0 {
            if has_n {
                cb(tile - 1.0 - w);
            }
            cb(tile - 1.0);
            if has_s {
                cb(tile - 1.0 + w);
            }
        }
        if has_n {
            cb(tile - w);
        }
        if has_s {
            cb(tile + w);
        }
        if x != w - 1.0 {
            if has_n {
                cb(tile + 1.0 - w);
            }
            cb(tile + 1.0);
            if has_s {
                cb(tile + 1.0 + w);
            }
        }
    }

    pub fn for_each_tile<F: FnMut(f64)>(&self, mut f: F) {
        let n = self.width_ * self.height_;
        let mut tile = 0.0f64;
        while tile < n {
            f(tile);
            tile += 1.0;
        }
    }

    // ---- distances and searches ----

    pub fn manhattan_dist(&self, c1: f64, c2: f64) -> f64 {
        (self.x(c1) - self.x(c2)).abs() + (self.y(c1) - self.y(c2)).abs()
    }

    pub fn euclidean_dist_squared(&self, c1: f64, c2: f64) -> f64 {
        let x = self.x(c1) - self.x(c2);
        let y = self.y(c1) - self.y(c2);
        x * x + y * y
    }

    /// Insertion-ordered `Set` result (TS's optional filter is `undefined`,
    /// which behaves like `|_, _| true` — no observable difference).
    pub fn circle_search<F: Fn(f64, f64) -> bool>(&self, tile: f64, radius: f64, filter: F) -> Vec<f64> {
        let center_x = self.x(tile);
        let center_y = self.y(tile);
        let mut tiles: Vec<f64> = Vec::new();
        let mut seen: HashSet<u64> = HashSet::new();
        let min_x = js_max(0.0, center_x - radius);
        let max_x = js_min(self.width_ - 1.0, center_x + radius);
        let min_y = js_max(0.0, center_y - radius);
        let max_y = js_min(self.height_ - 1.0, center_y + radius);
        let mut i = min_x;
        while i <= max_x {
            let mut j = min_y;
            while j <= max_y {
                // `yToRef[j] + i` — an out-of-range `j` reads `undefined`, so
                // the sum is NaN, exactly like JS.
                let t = match self.y_to_ref_at(j) {
                    Some(v) => v as f64 + i,
                    None => f64::NAN,
                };
                let d2 = self.euclidean_dist_squared(tile, t);
                // `if (d2 > radius * radius) continue;` — NaN d2 never
                // exceeds the bound, so it falls through to the filter,
                // exactly like JS.
                if d2 > radius * radius {
                    j += 1.0;
                    continue;
                }
                if filter(t, d2) && seen.insert(sz_key(t)) {
                    tiles.push(t);
                }
                j += 1.0;
            }
            i += 1.0;
        }
        tiles
    }

    /// LIFO-stack BFS (`q.pop()`), insertion-ordered `Set` result.
    pub fn bfs<F: Fn(&GameMap, f64) -> bool>(&self, tile: f64, filter: &F) -> Vec<f64> {
        let mut seen: Vec<f64> = Vec::new();
        let mut keys: HashSet<u64> = HashSet::new();
        let mut q: Vec<f64> = Vec::new();
        if filter(self, tile) {
            keys.insert(sz_key(tile));
            seen.push(tile);
            q.push(tile);
        }
        let w = self.width_;
        let south_limit = (self.height_ - 1.0) * w;
        let mut visit = |n: f64, seen: &mut Vec<f64>, q: &mut Vec<f64>| {
            if !keys.contains(&sz_key(n)) && filter(self, n) {
                keys.insert(sz_key(n));
                seen.push(n);
                q.push(n);
            }
        };
        while let Some(curr) = q.pop() {
            let x = curr % w;
            if curr >= w {
                visit(curr - w, &mut seen, &mut q);
            }
            if curr < south_limit {
                visit(curr + w, &mut seen, &mut q);
            }
            if x != 0.0 {
                visit(curr - 1.0, &mut seen, &mut q);
            }
            if x != w - 1.0 {
                visit(curr + 1.0, &mut seen, &mut q);
            }
        }
        seen
    }

    // ---- packed tile state ----

    /// `state[tile]` — `None` where JS yields `undefined`.
    pub fn tile_state(&self, tile: f64) -> Option<f64> {
        self.sget(tile).map(|s| s as f64)
    }

    pub fn tile_state_buffer(&self) -> &[u16] {
        &self.state
    }

    /// Apply a packed uint32 (bits 0-15 state, bits 16-23 terrain byte).
    /// Returns `true` when the terrain byte differed (JS `!==` against the
    /// possibly-`undefined` old byte, so an invalid ref with a land bit in
    /// the packed value still flips the counters).
    pub fn update_tile(&mut self, tile: f64, packed: f64) -> bool {
        let state = to_int32(packed) & 0xffff;
        let terrain_byte = ((to_uint32(packed) >> 16) & 0xff) as u8;

        let existing_fallout = self.has_fallout(tile);
        self.sput(tile, state);
        let new_fallout = self.has_fallout(tile);
        if existing_fallout && !new_fallout {
            self.num_tiles_with_fallout_ -= 1.0;
        }
        if !existing_fallout && new_fallout {
            self.num_tiles_with_fallout_ += 1.0;
        }

        let terrain_changed = self.tget(tile) != Some(terrain_byte);
        if terrain_changed {
            let was_land = self.is_land(tile);
            self.tput(tile, terrain_byte);
            let is_now_land = terrain_byte & IS_LAND_BIT != 0;
            if was_land != is_now_land {
                self.water_version_ += 1.0;
            }
            if was_land && !is_now_land {
                self.num_land_tiles_ -= 1.0;
            } else if !was_land && is_now_land {
                self.num_land_tiles_ += 1.0;
            }
        }
        terrain_changed
    }

    // ---- debug accessors (parity harness) ----

    pub fn debug_terrain(&self) -> &[u8] {
        &self.terrain
    }
    pub fn debug_state(&self) -> &[u16] {
        &self.state
    }
}

// ============================================================ distance filters
// The `*DistFN` factories, ported as direct predicates: `n` is the candidate
// tile. `center` shifts the root by (-0.5, -0.5).

pub fn eucl_dist_fn(gm: &GameMap, root: f64, dist: f64, center: bool, n: f64) -> bool {
    let dist2 = dist * dist;
    if !center {
        gm.euclidean_dist_squared(root, n) <= dist2
    } else {
        let root_x = gm.x(root) - 0.5;
        let root_y = gm.y(root) - 0.5;
        let dx = gm.x(n) - root_x;
        let dy = gm.y(n) - root_y;
        dx * dx + dy * dy <= dist2
    }
}

pub fn manhattan_dist_fn(gm: &GameMap, root: f64, dist: f64, center: bool, n: f64) -> bool {
    if !center {
        gm.manhattan_dist(root, n) <= dist
    } else {
        let root_x = gm.x(root) - 0.5;
        let root_y = gm.y(root) - 0.5;
        let dx = (gm.x(n) - root_x).abs();
        let dy = (gm.y(n) - root_y).abs();
        dx + dy <= dist
    }
}

pub fn rect_dist_fn(gm: &GameMap, root: f64, dist: f64, center: bool, n: f64) -> bool {
    if !center {
        let dx = (gm.x(n) - gm.x(root)).abs();
        let dy = (gm.y(n) - gm.y(root)).abs();
        dx <= dist && dy <= dist
    } else {
        let root_x = gm.x(root) - 0.5;
        let root_y = gm.y(root) - 0.5;
        let dx = (gm.x(n) - root_x).abs();
        let dy = (gm.y(n) - root_y).abs();
        dx <= dist && dy <= dist
    }
}

fn is_in_isometric_tile(
    center_x: f64,
    center_y: f64,
    tile_x: f64,
    tile_y: f64,
    y_offset: f64,
    distance: f64,
) -> bool {
    let dx = (tile_x - center_x).abs();
    let dy = (tile_y - (center_y + y_offset)).abs();
    dx + dy * 2.0 <= distance + 1.0
}

pub fn isometric_dist_fn(gm: &GameMap, root: f64, dist: f64, center: bool, n: f64) -> bool {
    if !center {
        gm.manhattan_dist(root, n) <= dist
    } else {
        let root_x = gm.x(root) - 0.5;
        let root_y = gm.y(root) - 0.5;
        is_in_isometric_tile(root_x, root_y, gm.x(n), gm.y(n), 0.0, dist)
    }
}

pub fn hex_dist_fn(gm: &GameMap, root: f64, dist: f64, center: bool, n: f64) -> bool {
    if !center {
        let dx = (gm.x(n) - gm.x(root)).abs();
        let dy = (gm.y(n) - gm.y(root)).abs();
        dx <= dist && dy <= dist && dx + dy <= dist * 1.5
    } else {
        let root_x = gm.x(root) - 0.5;
        let root_y = gm.y(root) - 0.5;
        let dx = (gm.x(n) - root_x).abs();
        let dy = (gm.y(n) - root_y).abs();
        dx <= dist && dy <= dist && dx + dy <= dist * 1.5
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const L: u8 = 0x85; // land, magnitude 5
    const O: u8 = 0x20; // plain ocean
    const M: u8 = 0x9f; // impassable land

    fn map3() -> GameMap {
        GameMap::new(3.0, 3.0, vec![L, L, O, L, M, O, L, L, O], 6.0)
    }

    #[test]
    fn ref_xy_roundtrip_and_throw() {
        let gm = map3();
        assert_eq!(gm.tile_ref(2.0, 1.0), 5.0);
        assert_eq!(gm.x(5.0), 2.0);
        assert_eq!(gm.y(5.0), 1.0);
        // y() truncates toward zero (ToInt32), unlike Math.floor:
        // (-1/3)|0 === 0, while Math.floor would give -1.
        assert_eq!(gm.y(-1.0), 0.0);
        assert!(!gm.y(-1.0).is_sign_negative());
        // x() keeps the JS % sign: -1 % 3 === -1.
        assert_eq!(gm.x(-1.0), -1.0);
        assert!(std::panic::catch_unwind(|| gm.tile_ref(3.0, 0.0)).is_err());
    }

    #[test]
    fn typed_array_semantics() {
        let gm = map3();
        // Out-of-range / fractional / negative refs read as undefined -> 0.
        assert!(!gm.is_land(9.0));
        assert!(!gm.is_land(1.5));
        assert!(!gm.is_land(-1.0));
        assert_eq!(gm.magnitude(9.0), 0.0);
        assert_eq!(gm.terrain_byte(9.0), None);
        assert_eq!(gm.terrain_byte(4.0), Some(159.0));
        assert_eq!(gm.tile_state(9.0), None);
        assert_eq!(gm.tile_state(0.0), Some(0.0));
    }

    #[test]
    fn set_water_and_counters() {
        let mut gm = map3();
        gm.set_water(1.0); // land -> lake water
        assert_eq!(gm.num_land_tiles(), 5.0);
        assert_eq!(gm.water_version(), 1.0);
        gm.set_water(4.0); // impassable: guard returns early
        assert_eq!(gm.water_version(), 1.0);
        gm.set_water(2.0); // already water
        assert_eq!(gm.num_land_tiles(), 5.0);
        // Invalid ref: isLand false -> early return, no counter change.
        gm.set_water(99.0);
        assert_eq!(gm.water_version(), 1.0);
    }

    #[test]
    fn update_tile_packing() {
        let mut gm = map3();
        // state=0x4001 (owner 1 + defense bit 14), identical terrain byte.
        let packed = (0x4001u32 | ((L as u32) << 16)) as f64;
        assert!(!gm.update_tile(0.0, packed)); // terrain unchanged -> false
        assert_eq!(gm.owner_id(0.0), 1.0);
        assert!(gm.has_defense_bonus(0.0));
        assert_eq!(gm.water_version(), 0.0);
        // Land flip through updateTile bumps waterVersion and land count.
        assert!(gm.update_tile(0.0, 0x4001u32 as f64)); // terrain -> 0x00 water
        assert_eq!(gm.water_version(), 1.0);
        assert_eq!(gm.num_land_tiles(), 5.0);
        // Fallout counter diff across a packed update.
        let mut gm3 = map3();
        gm3.set_fallout(1.0, true);
        assert_eq!(gm3.num_tiles_with_fallout(), 1.0);
        gm3.update_tile(1.0, 0.0); // clears fallout, flips land->water
        assert_eq!(gm3.num_tiles_with_fallout(), 0.0);
        assert!(!gm3.update_tile(1.0, 0.0));
    }

    #[test]
    fn invalid_ref_update_tile_still_books_counters() {
        let mut gm = map3();
        // terrain[99] is undefined !== 0x85 -> "changed": wasLand false,
        // isNowLand true -> waterVersion++ and land count++, write dropped.
        let packed = ((L as u32) << 16) as f64;
        assert!(gm.update_tile(99.0, packed));
        assert_eq!(gm.water_version(), 1.0);
        assert_eq!(gm.num_land_tiles(), 7.0);
        assert_eq!(gm.terrain_byte(99.0), None);
    }

    #[test]
    fn bfs_is_stack_dfs_order() {
        // 3x3 all land from ref 0: q.pop() makes this depth-first.
        let gm = GameMap::new(3.0, 3.0, vec![L; 9], 9.0);
        let order = gm.bfs(0.0, &|_, _| true);
        assert_eq!(order, vec![0.0, 3.0, 1.0, 4.0, 2.0, 5.0, 8.0, 7.0, 6.0]);
    }

    #[test]
    fn circle_search_insertion_order() {
        // i (x) outer, j (y) inner: column-major insertion.
        let gm = GameMap::new(3.0, 3.0, vec![L; 9], 9.0);
        let hits = gm.circle_search(4.0, 1.0, |_, _| true);
        assert_eq!(hits, vec![3.0, 1.0, 4.0, 7.0, 5.0]);
    }

    #[test]
    fn set_owner_id_throw_and_wrap() {
        assert!(std::panic::catch_unwind(|| {
            let mut g = map3();
            g.set_owner_id(0.0, 4096.0)
        })
        .is_err());
        let mut gm = map3();
        gm.set_owner_id(0.0, 4094.9); // not > 4095; ToInt32 -> 4094
        assert_eq!(gm.owner_id(0.0), 4094.0);
        gm.set_owner_id(1.0, -1.0); // low 12 bits of -1 -> 4095
        assert_eq!(gm.owner_id(1.0), 4095.0);
        // Invalid ref: the write is dropped but no throw happens.
        gm.set_owner_id(99.0, 7.0);
        assert_eq!(gm.tile_state(99.0), None);
    }
}
