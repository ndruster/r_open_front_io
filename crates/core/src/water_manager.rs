//! Port of `src/core/game/WaterManager.ts`.
//!
//! Water-nuke terrain fixup + throttled minimap water-graph rebuild. The TS
//! class holds two live `GameMap`s (full + minimap), a pending-tile `Set`,
//! stamp-based BFS scratch arrays, and the persistent minimap
//! `ConnectedComponents` shared with the abstract water graph.
//!
//! ## Graph/HPA bypass (bit-exactness argument)
//!
//! The only WaterManager-observable effects of `_miniWaterGraph` /
//! `_miniWaterHPA` / `_builderBFS` are:
//!
//! * `getWaterComponent` / `hasWaterComponent` / `getWaterComponentSize`,
//!   which delegate to `graph.getComponentId/Size` — and in TS the builder
//!   is *always* handed the persistent `this._miniWaterCC` (the
//!   `sharedWaterComponents` path), so the graph's components are literally
//!   the same object WaterManager owns and mutates via `addWaterTiles`.
//! * `graph !== null` ⟺ `!disableNavMesh` (the constructor builds it exactly
//!   when nav mesh is enabled; rebuilds only replace it in that mode).
//!
//! The node/edge/path-cache machinery never crosses the WaterManager API
//! surface, and a rebuild's only observable effect is
//! `_waterGraphVersion++` + `_dirtyMiniTiles.clear()`. So this port keeps the
//! persistent `ConnectedComponents` directly and models rebuilds as a version
//! bump. (The Rust `AbstractGraphBuilder` re-floods a fresh CC per build and
//! would renumber components differently from TS's incremental labeling —
//! using it here would *break* parity, not preserve it.)
//!
//! Faithfulness notes:
//!
//! * JS `Set`/`Map` iteration order is insertion order (SameValueZero keys);
//!   [`OrderedSet`] / [`OrderedMap`] replicate that, including `-0`/`0` and
//!   `NaN` keying.
//! * The magnitude BFS reads packed terrain bytes directly (`terrain[i] &
//!   0x80` etc.), mirroring `(map as any).terrain` in TS. Writes
//!   (`setMagnitude`) are deferred to the end of the update loop — each tile
//!   is read at most once inside the loop and nothing reads before the loop
//!   is affected, so the snapshot reads are identical to TS's live reads.
//! * `distArr[tile] + 1` / `stampArr[tile] = stamp` are `Uint16Array`
//!   element ops; the writes go through `as u16` (ToUint16 truncation).
//! * `Math.floor/ceil/min/max` on finite values agree with Rust `f64`
//!   methods; `Math.min(3, totalCount)` and the magnitude cap never see
//!   `NaN`/`±0` in reachable paths.
//! * `miniMap.ref(miniX, miniY)` throws on invalid coords in TS (panic here);
//!   capture scenarios keep every queued tile in range.

use std::collections::HashMap;

use crate::game_map::GameMap;
use crate::pathfinding::connected_components::ConnectedComponents;

/// `WATER_GRAPH_REBUILD_INTERVAL`.
const WATER_GRAPH_REBUILD_INTERVAL: f64 = 20.0;
/// `MAX_MAG_DIST`.
const MAX_MAG_DIST: f64 = 62.0;
/// `TERRAIN_LAND_MASK`.
const TERRAIN_LAND_MASK: u8 = 0x80;
/// `TERRAIN_MAG_MASK`.
const TERRAIN_MAG_MASK: u8 = 0x1f;
/// `IMPASSABLE_MAG`.
const IMPASSABLE_MAG: u8 = 31;

// ---------------------------------------------------------------------------
// Insertion-ordered JS `Set` / `Map` models (SameValueZero keys).
// ---------------------------------------------------------------------------

fn svz_key(v: f64) -> u64 {
    // SameValueZero: -0 === 0, NaN === NaN.
    if v == 0.0 {
        0.0f64.to_bits()
    } else {
        v.to_bits()
    }
}

/// A JS `Set<number>` with insertion-order iteration.
#[derive(Debug, Default)]
pub struct OrderedSet {
    vals: Vec<f64>,
    idx: HashMap<u64, usize>,
}

impl OrderedSet {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn add(&mut self, v: f64) -> bool {
        let k = svz_key(v);
        if self.idx.contains_key(&k) {
            return false;
        }
        self.idx.insert(k, self.vals.len());
        self.vals.push(v);
        true
    }
    pub fn has(&self, v: f64) -> bool {
        self.idx.contains_key(&svz_key(v))
    }
    pub fn len(&self) -> usize {
        self.vals.len()
    }
    pub fn is_empty(&self) -> bool {
        self.vals.is_empty()
    }
    pub fn clear(&mut self) {
        self.vals.clear();
        self.idx.clear();
    }
    pub fn iter(&self) -> impl Iterator<Item = f64> + '_ {
        self.vals.iter().copied()
    }
    pub fn as_slice(&self) -> &[f64] {
        &self.vals
    }
}

/// A JS `Map<number, V>` with insertion-order iteration (integer keys).
#[derive(Debug)]
struct OrderedMap<V> {
    keys: Vec<i64>,
    idx: HashMap<i64, usize>,
    vals: Vec<V>,
}

impl<V> OrderedMap<V> {
    fn new() -> Self {
        Self {
            keys: Vec::new(),
            idx: HashMap::new(),
            vals: Vec::new(),
        }
    }
    fn get(&self, k: i64) -> Option<usize> {
        self.idx.get(&k).copied()
    }
    fn has(&self, k: i64) -> bool {
        self.idx.contains_key(&k)
    }
    fn set(&mut self, k: i64, v: V) {
        if let Some(&i) = self.idx.get(&k) {
            self.vals[i] = v;
        } else {
            self.idx.insert(k, self.vals.len());
            self.keys.push(k);
            self.vals.push(v);
        }
    }
}

/// `CraterBounds`.
#[derive(Clone, Copy, Debug)]
struct CraterBounds {
    min_x: f64,
    max_x: f64,
    min_y: f64,
    max_y: f64,
}

/// `WaterManager`. Owns the two maps (the TS class holds live references;
/// ownership here keeps the wasm probe simple — the final buffers are read
/// back through the `debug_*` accessors).
pub struct WaterManager {
    map: GameMap,
    mini_map: GameMap,
    disable_nav_mesh: bool,
    /// `_miniWaterCC` — `null` iff `disableNavMesh` (the graph it backs is
    /// the only consumer of its ids through the WaterManager API).
    mini_water_cc: Option<ConnectedComponents>,
    water_graph_version: f64,
    water_graph_dirty: bool,
    water_graph_last_rebuild_tick: f64,
    pending_water_tiles: OrderedSet,
    dirty_mini_tiles: OrderedSet,
    water_dist_arr: Vec<u16>,
    water_stamp_arr: Vec<u16>,
    water_stamp: i64,
    mini_dist_arr: Vec<u16>,
    mini_stamp_arr: Vec<u16>,
    mini_stamp: i64,
}

impl WaterManager {
    pub fn new(map: GameMap, mini_map: GameMap, disable_nav_mesh: bool) -> Self {
        let cc = if !disable_nav_mesh {
            // TS: `new ConnectedComponents(miniMap)` (accessTerrainDirectly
            // defaults true) + `initialize()`. The CC only reads terrain
            // during `initialize`, so a construction-time snapshot matches
            // the live-map reads.
            let w = mini_map.width() as i64;
            let h = mini_map.height() as i64;
            let mut cc = ConnectedComponents::new(w, h, mini_map.debug_terrain().to_vec(), true);
            cc.initialize();
            Some(cc)
        } else {
            None
        };
        Self {
            map,
            mini_map,
            disable_nav_mesh,
            mini_water_cc: cc,
            water_graph_version: 0.0,
            water_graph_dirty: false,
            water_graph_last_rebuild_tick: 0.0,
            pending_water_tiles: OrderedSet::new(),
            dirty_mini_tiles: OrderedSet::new(),
            water_dist_arr: Vec::new(),
            water_stamp_arr: Vec::new(),
            water_stamp: 0,
            mini_dist_arr: Vec::new(),
            mini_stamp_arr: Vec::new(),
            mini_stamp: 0,
        }
    }

    /// `queueTile(tile)`.
    pub fn queue_tile(&mut self, tile: f64) {
        self.pending_water_tiles.add(tile);
    }

    /// `tick(currentTick)` — returns the changed tiles.
    pub fn tick(&mut self, current_tick: f64) -> Vec<f64> {
        let mut changed_tiles: Vec<f64> = Vec::new();
        let mut converted_this_tick = false;

        if !self.pending_water_tiles.is_empty() {
            let mut converted: Vec<f64> = Vec::new();
            for tile in self.pending_water_tiles.iter().collect::<Vec<f64>>() {
                // Tile may have been conquered between queueing and flushing
                if self.map.is_land(tile)
                    && !self.map.has_owner(tile)
                    && !self.map.is_impassable(tile)
                {
                    if self.map.has_fallout(tile) {
                        self.map.set_fallout(tile, false);
                    }
                    self.map.set_water(tile);
                    converted.push(tile);
                }
            }
            self.pending_water_tiles.clear();
            if !converted.is_empty() {
                converted_this_tick = true;
                self.finalize_water_changes(&converted, &mut changed_tiles);
            }
        }

        // Throttled water graph rebuild: at most once every 20 ticks. The
        // graph/HPA/BFS swap is unobservable through this API (see module
        // doc); only the version bump and dirty-tile clear are kept.
        if self.water_graph_dirty
            && !self.disable_nav_mesh
            && !converted_this_tick
            && current_tick - self.water_graph_last_rebuild_tick >= WATER_GRAPH_REBUILD_INTERVAL
        {
            self.water_graph_dirty = false;
            self.water_graph_last_rebuild_tick = current_tick;
            self.dirty_mini_tiles.clear();
            self.water_graph_version += 1.0;
        }

        changed_tiles
    }

    /// `waterGraphVersion()`.
    pub fn water_graph_version(&self) -> f64 {
        self.water_graph_version
    }

    /// `getWaterComponent(tile)` — `None` mirrors TS `null`.
    pub fn get_water_component(&mut self, tile: f64) -> Option<f64> {
        let cc = match &mut self.mini_water_cc {
            // Permissive fallback for tests with disableNavMesh
            None => return Some(0.0),
            Some(c) => c,
        };
        let mini_x = (self.map.x(tile) / 2.0).floor();
        let mini_y = (self.map.y(tile) / 2.0).floor();
        let mini_tile = self.mini_map.tile_ref(mini_x, mini_y);

        if self.mini_map.is_water(mini_tile) {
            return Some(cc.get_component_id(mini_tile) as f64);
        }

        // Shore tile: find water neighbor (expand search for minimap resolution loss)
        let nbs = self.mini_map.neighbors(mini_tile);
        for &n in &nbs {
            if self.mini_map.is_water(n) {
                return Some(cc.get_component_id(n) as f64);
            }
        }

        // Extended search: check 2-hop neighbors for narrow straits
        for &n in &nbs {
            let n2s = self.mini_map.neighbors(n);
            for n2 in n2s {
                if self.mini_map.is_water(n2) {
                    return Some(cc.get_component_id(n2) as f64);
                }
            }
        }
        None
    }

    /// `hasWaterComponent(tile, component)`.
    pub fn has_water_component(&mut self, tile: f64, component: f64) -> bool {
        let cc = match &mut self.mini_water_cc {
            // Permissive fallback for tests with disableNavMesh
            None => return true,
            Some(c) => c,
        };
        let mini_x = (self.map.x(tile) / 2.0).floor();
        let mini_y = (self.map.y(tile) / 2.0).floor();
        let mini_tile = self.mini_map.tile_ref(mini_x, mini_y);

        // Check miniTile itself (shore in full map may be water in minimap)
        if self.mini_map.is_water(mini_tile)
            && cc.get_component_id(mini_tile) as f64 == component
        {
            return true;
        }

        // Check neighbors
        let nbs = self.mini_map.neighbors(mini_tile);
        for &n in &nbs {
            if self.mini_map.is_water(n) && cc.get_component_id(n) as f64 == component {
                return true;
            }
        }

        // Extended search: check 2-hop neighbors for narrow straits
        for &n in &nbs {
            let n2s = self.mini_map.neighbors(n);
            for n2 in n2s {
                if self.mini_map.is_water(n2) && cc.get_component_id(n2) as f64 == component {
                    return true;
                }
            }
        }
        false
    }

    /// `getWaterComponentSize(tile)` — `None` mirrors TS `null`.
    pub fn get_water_component_size(&mut self, tile: f64) -> Option<f64> {
        let component_id = self.get_water_component(tile)?;
        let cc = match &mut self.mini_water_cc {
            // Permissive fallback for tests with disableNavMesh
            None => return Some(0.0),
            Some(c) => c,
        };
        Some(cc.get_component_size(component_id) * 4.0)
    }

    // ---- finalizeWaterChanges ----

    fn finalize_water_changes(&mut self, converted_tiles: &[f64], changed_tiles: &mut Vec<f64>) {
        let mut converted = OrderedSet::new();
        for &t in converted_tiles {
            converted.add(t);
        }
        if converted.is_empty() {
            return;
        }

        let w = self.map.width();
        let total_tiles = w * self.map.height();

        // Track changed tiles in a set for dedup, drain into output at end.
        let mut changed = OrderedSet::new();
        for tile in converted.iter() {
            changed.add(tile);
        }

        // ── 1. Propagate ocean bit ─────────────────────────────────
        let mut ocean_queue: Vec<f64> = Vec::new();
        let mut nb = [0f64; 8];
        for tile in converted.iter() {
            let end = push_neighbors(tile, &mut nb, 0, w, total_tiles);
            for &n in &nb[..end] {
                if !converted.has(n) && self.map.is_ocean(n) {
                    self.map.set_ocean(tile);
                    ocean_queue.push(tile);
                    break;
                }
            }
        }
        let mut o_head = 0usize;
        while o_head < ocean_queue.len() {
            let tile = ocean_queue[o_head];
            o_head += 1;
            let end = push_neighbors(tile, &mut nb, 0, w, total_tiles);
            for &n in &nb[..end] {
                if self.map.is_water(n) && !self.map.is_ocean(n) {
                    self.map.set_ocean(n);
                    changed.add(n);
                    ocean_queue.push(n);
                }
            }
        }

        // ── 2. Recompute magnitude via BFS from remaining land outward ─
        let n = total_tiles as usize;
        if self.water_dist_arr.len() != n {
            self.water_dist_arr = vec![0u16; n];
            self.water_stamp_arr = vec![0u16; n];
            self.water_stamp = 0;
        }
        let groups = compute_crater_groups(&converted, w, MAX_MAG_DIST);
        for g in &groups {
            let stamp = self.bump_full_map_stamp();
            recompute_magnitudes_in_box(
                &mut self.map,
                &mut self.water_stamp_arr,
                &mut self.water_dist_arr,
                g,
                stamp,
                true, // impassable terrain is void, not coastline
                Some(&mut changed),
            );
        }

        // ── 3. Fix shoreline bits ──────────────────────────────────
        let shore_stamp = self.bump_full_map_stamp();
        let mut tiles_to_check: Vec<f64> = Vec::new();
        for tile in converted.iter() {
            push_to_check(tile, shore_stamp, &mut self.water_stamp_arr, &mut tiles_to_check);
            let end = push_neighbors(tile, &mut nb, 0, w, total_tiles);
            for i in 0..end {
                push_to_check(nb[i], shore_stamp, &mut self.water_stamp_arr, &mut tiles_to_check);
                let end2 = push_neighbors(nb[i], &mut nb, end, w, total_tiles);
                for &t in &nb[end..end2] {
                    push_to_check(t, shore_stamp, &mut self.water_stamp_arr, &mut tiles_to_check);
                }
            }
        }
        for &tile in &tiles_to_check {
            // Impassable tiles never get shoreline.
            if self.map.is_impassable(tile) {
                if self.map.is_shoreline(tile) {
                    self.map.clear_shoreline_bit(tile);
                    changed.add(tile);
                }
                continue;
            }
            let tile_is_land = self.map.is_land(tile);
            let mut has_opposite = false;
            let end = push_neighbors(tile, &mut nb, 0, w, total_tiles);
            for &k in &nb[..end] {
                // Impassable neighbors don't create shorelines (void, not coast).
                if self.map.is_impassable(k) {
                    continue;
                }
                if self.map.is_land(k) != tile_is_land {
                    has_opposite = true;
                    break;
                }
            }
            let old_shoreline = self.map.is_shoreline(tile);
            if has_opposite {
                if !old_shoreline {
                    self.map.set_shoreline_bit(tile);
                    changed.add(tile);
                }
            } else if old_shoreline {
                self.map.clear_shoreline_bit(tile);
                changed.add(tile);
            }
        }

        // ── 4. Update minimap terrain ──────────────────────────────
        let mut mini_tiles_to_check = OrderedSet::new();
        let mut converted_mini_tiles = OrderedSet::new();
        for tile in converted.iter() {
            let mini_x = (self.map.x(tile) / 2.0).floor();
            let mini_y = (self.map.y(tile) / 2.0).floor();
            if self.mini_map.is_valid_coord(mini_x, mini_y) {
                let mt = self.mini_map.tile_ref(mini_x, mini_y);
                mini_tiles_to_check.add(mt);
            }
        }
        let mini_list = mini_tiles_to_check.as_slice().to_vec();
        for &mini_tile in &mini_list {
            if !self.mini_map.is_land(mini_tile) {
                continue;
            }
            let fx = self.mini_map.x(mini_tile) * 2.0;
            let fy = self.mini_map.y(mini_tile) * 2.0;
            let mut water_count = 0.0f64;
            let mut total_count = 0.0f64;
            for dy in 0..2 {
                for dx in 0..2 {
                    if self.map.is_valid_coord(fx + dx as f64, fy + dy as f64) {
                        total_count += 1.0;
                        if self.map.is_water(self.map.tile_ref(fx + dx as f64, fy + dy as f64)) {
                            water_count += 1.0;
                        }
                    }
                }
            }
            if water_count >= total_count.min(3.0) {
                self.mini_map.set_water(mini_tile);
                converted_mini_tiles.add(mini_tile);
            }
        }

        // ── 4b. Fix minimap ocean + magnitude for converted tiles ──
        if !converted_mini_tiles.is_empty() {
            let mini_w = self.mini_map.width();
            let mini_h = self.mini_map.height();

            // 4b-i. Propagate ocean bit to converted minimap tiles.
            let mut mini_ocean_queue: Vec<f64> = Vec::new();
            let mut mini_nb = [0f64; 4];
            for mt in converted_mini_tiles.iter() {
                let nc = push_mini_neighbors(mt, &mut mini_nb, mini_w, mini_h);
                let mut near_ocean = false;
                for &n in &mini_nb[..nc] {
                    if self.mini_map.is_ocean(n) {
                        near_ocean = true;
                        break;
                    }
                }
                if near_ocean {
                    self.mini_map.set_ocean(mt);
                    mini_ocean_queue.push(mt);
                }
            }
            let mut mo_head = 0usize;
            while mo_head < mini_ocean_queue.len() {
                let tile = mini_ocean_queue[mo_head];
                mo_head += 1;
                let nc = push_mini_neighbors(tile, &mut mini_nb, mini_w, mini_h);
                for &n in &mini_nb[..nc] {
                    if self.mini_map.is_water(n) && !self.mini_map.is_ocean(n) {
                        self.mini_map.set_ocean(n);
                        mini_ocean_queue.push(n);
                    }
                }
            }

            // 4b-ii. Recompute minimap magnitude via BFS from coastlines.
            let mini_total = (mini_w * mini_h) as usize;
            if self.mini_dist_arr.len() != mini_total {
                self.mini_dist_arr = vec![0u16; mini_total];
                self.mini_stamp_arr = vec![0u16; mini_total];
                self.mini_stamp = 0;
            }
            let mini_groups = compute_crater_groups(&converted_mini_tiles, mini_w, MAX_MAG_DIST);
            for g in &mini_groups {
                let stamp = self.bump_mini_stamp();
                recompute_magnitudes_in_box(
                    &mut self.mini_map,
                    &mut self.mini_stamp_arr,
                    &mut self.mini_dist_arr,
                    g,
                    stamp,
                    false,
                    None,
                );
            }
        }

        // ── 5. Mark water graph dirty (rebuilt lazily, throttled) ──
        if !converted_mini_tiles.is_empty() {
            // Fold the new water tiles into the persistent component labeling.
            if let Some(cc) = &mut self.mini_water_cc {
                for mt in converted_mini_tiles.iter().collect::<Vec<f64>>() {
                    cc.add_water_tile(mt);
                }
            }
            self.water_graph_dirty = true;
            for mt in converted_mini_tiles.iter() {
                self.dirty_mini_tiles.add(mt);
            }
        }

        // Drain changed set into output array.
        for tile in changed.iter() {
            changed_tiles.push(tile);
        }
    }

    /// `bumpFullMapStamp()`.
    fn bump_full_map_stamp(&mut self) -> i64 {
        self.water_stamp += 1;
        if self.water_stamp >= 0xffff {
            self.water_stamp_arr.fill(0);
            self.water_stamp = 1;
        }
        self.water_stamp
    }

    /// `bumpMiniStamp()`.
    fn bump_mini_stamp(&mut self) -> i64 {
        self.mini_stamp += 1;
        if self.mini_stamp >= 0xffff {
            self.mini_stamp_arr.fill(0);
            self.mini_stamp = 1;
        }
        self.mini_stamp
    }

    // ---- parity harness ----

    /// Stateful op stream. `kind`:
    /// 0 `queueTile(a)` → `[]`;
    /// 1 `tick(a)` → `[len, changed…]`;
    /// 2 `waterGraphVersion()` → `[v]`;
    /// 3 `getWaterComponent(a)` → `[1]` (null) | `[0, id]`;
    /// 4 `hasWaterComponent(a, b)` → `[bool]`;
    /// 5 `getWaterComponentSize(a)` → `[1]` (null) | `[0, size]`;
    /// 6 `map.setOwnerID(a, b)` → `[]` (conquered-tile skip branch);
    /// 7 `map.setFallout(a, b)` → `[]` (fallout-clear branch).
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        match kind {
            0 => {
                self.queue_tile(args[0]);
                Vec::new()
            }
            1 => {
                let changed = self.tick(args[0]);
                let mut out = Vec::with_capacity(changed.len() + 1);
                out.push(changed.len() as f64);
                out.extend(changed);
                out
            }
            2 => vec![self.water_graph_version()],
            3 => match self.get_water_component(args[0]) {
                Some(id) => vec![0.0, id],
                None => vec![1.0],
            },
            4 => vec![self.has_water_component(args[0], args[1]) as u8 as f64],
            5 => match self.get_water_component_size(args[0]) {
                Some(s) => vec![0.0, s],
                None => vec![1.0],
            },
            6 => {
                self.map.set_owner_id(args[0], args[1]);
                Vec::new()
            }
            7 => {
                self.map.set_fallout(args[0], args[1] != 0.0);
                Vec::new()
            }
            _ => panic!("bad water_manager op kind {kind}"),
        }
    }

    // ---- debug accessors (parity harness) ----

    pub fn debug_map_terrain(&self) -> &[u8] {
        self.map.debug_terrain()
    }
    pub fn debug_map_state(&self) -> &[u16] {
        self.map.debug_state()
    }
    pub fn debug_mini_terrain(&self) -> &[u8] {
        self.mini_map.debug_terrain()
    }
}

/// Inline neighbor helper (no allocation, cardinal only) — TS `pushNeighbors`.
/// Order: up, down, left, right. Returns the new write cursor.
fn push_neighbors(tile: f64, out: &mut [f64; 8], start: usize, w: f64, total_tiles: f64) -> usize {
    let mut s = start;
    if tile >= w {
        out[s] = tile - w;
        s += 1;
    }
    if tile < total_tiles - w {
        out[s] = tile + w;
        s += 1;
    }
    let x = tile % w;
    if x > 0.0 {
        out[s] = tile - 1.0;
        s += 1;
    }
    if x < w - 1.0 {
        out[s] = tile + 1.0;
        s += 1;
    }
    s
}

/// Allocation-free cardinal-neighbor helper for minimap BFSes — TS
/// `pushMiniNeighbors`. Order: up, down, left, right.
fn push_mini_neighbors(tile: f64, out: &mut [f64; 4], mini_w: f64, mini_h: f64) -> usize {
    let x = tile % mini_w;
    let y = (tile - x) / mini_w;
    let mut count = 0usize;
    if y > 0.0 {
        out[count] = tile - mini_w;
        count += 1;
    }
    if y < mini_h - 1.0 {
        out[count] = tile + mini_w;
        count += 1;
    }
    if x > 0.0 {
        out[count] = tile - 1.0;
        count += 1;
    }
    if x < mini_w - 1.0 {
        out[count] = tile + 1.0;
        count += 1;
    }
    count
}

/// TS `pushToCheck` closure: stamp-dedup push into `tiles_to_check`.
fn push_to_check(tile: f64, stamp: i64, stamp_arr: &mut [u16], out: &mut Vec<f64>) {
    let i = tile as usize;
    if stamp_arr[i] != stamp as u16 {
        stamp_arr[i] = stamp as u16;
        out.push(tile);
    }
}

/// `computeCraterGroups(converted, w, maxMagDist)`.
fn compute_crater_groups(converted: &OrderedSet, w: f64, max_mag_dist: f64) -> Vec<CraterBounds> {
    let cell_size = max_mag_dist;
    let cells_x = (w / cell_size).ceil();

    // Bucket tiles into coarse cells, tracking per-cell crater bounds.
    let mut cells: OrderedMap<CraterBounds> = OrderedMap::new();
    for tile in converted.iter() {
        let tx = tile % w;
        let ty = (tile - tx) / w;
        let key = (ty / cell_size).floor() * cells_x + (tx / cell_size).floor();
        match cells.get(key as i64) {
            Some(i) => {
                let b = &mut cells.vals[i];
                if tx < b.min_x {
                    b.min_x = tx;
                }
                if tx > b.max_x {
                    b.max_x = tx;
                }
                if ty < b.min_y {
                    b.min_y = ty;
                }
                if ty > b.max_y {
                    b.max_y = ty;
                }
            }
            None => cells.set(
                key as i64,
                CraterBounds {
                    min_x: tx,
                    max_x: tx,
                    min_y: ty,
                    max_y: ty,
                },
            ),
        }
    }

    // Union-find over occupied cells; adjacent cells (8-neighborhood) belong
    // to the same crater cluster.
    let mut parent: HashMap<i64, i64> = HashMap::new();
    for &key in &cells.keys {
        parent.insert(key, key);
    }
    for i in 0..cells.keys.len() {
        let key = cells.keys[i];
        let cx = key % cells_x as i64;
        let cy = (key - cx) / cells_x as i64;
        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = cx + dx;
                let ny = cy + dy;
                if nx < 0 || nx >= cells_x as i64 || ny < 0 {
                    continue;
                }
                let nk = ny * cells_x as i64 + nx;
                if !cells.has(nk) {
                    continue;
                }
                let ra = uf_find(&mut parent, key);
                let rb = uf_find(&mut parent, nk);
                if ra != rb {
                    parent.insert(ra.max(rb), ra.min(rb));
                }
            }
        }
    }

    // Merge cell bounds per root (Map iteration order is deterministic).
    let mut by_root: OrderedMap<CraterBounds> = OrderedMap::new();
    for i in 0..cells.keys.len() {
        let key = cells.keys[i];
        let b = cells.vals[i];
        let root = uf_find(&mut parent, key);
        match by_root.get(root) {
            Some(g) => {
                let entry = &mut by_root.vals[g];
                if b.min_x < entry.min_x {
                    entry.min_x = b.min_x;
                }
                if b.max_x > entry.max_x {
                    entry.max_x = b.max_x;
                }
                if b.min_y < entry.min_y {
                    entry.min_y = b.min_y;
                }
                if b.max_y > entry.max_y {
                    entry.max_y = b.max_y;
                }
            }
            None => by_root.set(root, b),
        }
    }
    let mut groups: Vec<CraterBounds> = by_root.vals;

    // Greedily merge groups when the merged seed box is cheaper to process
    // than the two separate (possibly overlapping) seed boxes.
    let pad = 4.0 * max_mag_dist; // seed box adds 2*maxMagDist on each side
    let box_area = |g: &CraterBounds| (g.max_x - g.min_x + pad) * (g.max_y - g.min_y + pad);
    let mut merged = true;
    while merged {
        merged = false;
        let mut i = 0usize;
        while i < groups.len() && !merged {
            let mut j = i + 1;
            while j < groups.len() {
                let a = groups[i];
                let b = groups[j];
                let union = CraterBounds {
                    min_x: a.min_x.min(b.min_x),
                    max_x: a.max_x.max(b.max_x),
                    min_y: a.min_y.min(b.min_y),
                    max_y: a.max_y.max(b.max_y),
                };
                if box_area(&union) < box_area(&a) + box_area(&b) {
                    groups[i] = union;
                    groups.remove(j);
                    merged = true;
                    break;
                }
                j += 1;
            }
            i += 1;
        }
    }
    groups
}

/// TS union-find `find` with path compression over a `Map`.
fn uf_find(parent: &mut HashMap<i64, i64>, k: i64) -> i64 {
    let mut root = k;
    while *parent.get(&root).unwrap() != root {
        root = *parent.get(&root).unwrap();
    }
    let mut cur = k;
    while *parent.get(&cur).unwrap() != root {
        let next = *parent.get(&cur).unwrap();
        parent.insert(cur, root);
        cur = next;
    }
    root
}

/// `recomputeMagnitudesInBox(map, bounds, stampArr, distArr, stamp,
/// passableCoastOnly, changed)`.
///
/// Terrain reads come from a snapshot slice; `setMagnitude` writes are
/// deferred past the update loop (each tile is read at most once inside the
/// loop, so the result is identical to TS's interleaved reads/writes).
fn recompute_magnitudes_in_box(
    map: &mut GameMap,
    stamp_arr: &mut [u16],
    dist_arr: &mut [u16],
    bounds: &CraterBounds,
    stamp: i64,
    passable_coast_only: bool,
    mut changed: Option<&mut OrderedSet>,
) {
    let w = map.width();
    let h = map.height();
    let terrain = map.debug_terrain();

    // Dirty box: tiles whose magnitude may need updating.
    let d_min_x = (bounds.min_x - MAX_MAG_DIST).max(0.0);
    let d_max_x = (bounds.max_x + MAX_MAG_DIST).min(w - 1.0);
    let d_min_y = (bounds.min_y - MAX_MAG_DIST).max(0.0);
    let d_max_y = (bounds.max_y + MAX_MAG_DIST).min(h - 1.0);
    // Seed box: coastlines here are seeded; BFS is clipped here.
    let s_min_x = (bounds.min_x - MAX_MAG_DIST * 2.0).max(0.0);
    let s_max_x = (bounds.max_x + MAX_MAG_DIST * 2.0).min(w - 1.0);
    let s_min_y = (bounds.min_y - MAX_MAG_DIST * 2.0).max(0.0);
    let s_max_y = (bounds.max_y + MAX_MAG_DIST * 2.0).min(h - 1.0);

    let is_coast_byte = |b: u8| -> bool {
        (b & TERRAIN_LAND_MASK) != 0
            && (!passable_coast_only || (b & TERRAIN_MAG_MASK) != IMPASSABLE_MAG)
    };
    let byte = |i: f64| -> u8 { terrain[i as usize] };

    // Seed from coastline water tiles inside the seed box.
    let mut queue: Vec<f64> = Vec::new();
    let mut y = s_min_y;
    while y <= s_max_y {
        let row_start = y * w;
        let mut x = s_min_x;
        while x <= s_max_x {
            let tile = row_start + x;
            if (byte(tile) & TERRAIN_LAND_MASK) == 0 {
                let ti = tile as usize;
                if stamp_arr[ti] != stamp as u16 {
                    let is_coast = (y > 0.0 && is_coast_byte(byte(tile - w)))
                        || (y < h - 1.0 && is_coast_byte(byte(tile + w)))
                        || (x > 0.0 && is_coast_byte(byte(tile - 1.0)))
                        || (x < w - 1.0 && is_coast_byte(byte(tile + 1.0)));
                    if is_coast {
                        stamp_arr[ti] = stamp as u16;
                        dist_arr[ti] = 0;
                        queue.push(tile);
                    }
                }
            }
            x += 1.0;
        }
        y += 1.0;
    }

    // BFS outward through water, clipped to the seed box.
    let mut head = 0usize;
    while head < queue.len() {
        let tile = queue[head];
        head += 1;
        let next_dist = dist_arr[tile as usize] as u32 + 1;
        let x = tile % w;
        let y = (tile - x) / w;
        if y > s_min_y {
            let n = tile - w;
            if (byte(n) & TERRAIN_LAND_MASK) == 0 && stamp_arr[n as usize] != stamp as u16 {
                stamp_arr[n as usize] = stamp as u16;
                dist_arr[n as usize] = next_dist as u16;
                queue.push(n);
            }
        }
        if y < s_max_y {
            let n = tile + w;
            if (byte(n) & TERRAIN_LAND_MASK) == 0 && stamp_arr[n as usize] != stamp as u16 {
                stamp_arr[n as usize] = stamp as u16;
                dist_arr[n as usize] = next_dist as u16;
                queue.push(n);
            }
        }
        if x > s_min_x {
            let n = tile - 1.0;
            if (byte(n) & TERRAIN_LAND_MASK) == 0 && stamp_arr[n as usize] != stamp as u16 {
                stamp_arr[n as usize] = stamp as u16;
                dist_arr[n as usize] = next_dist as u16;
                queue.push(n);
            }
        }
        if x < s_max_x {
            let n = tile + 1.0;
            if (byte(n) & TERRAIN_LAND_MASK) == 0 && stamp_arr[n as usize] != stamp as u16 {
                stamp_arr[n as usize] = stamp as u16;
                dist_arr[n as usize] = next_dist as u16;
                queue.push(n);
            }
        }
    }

    // Update magnitudes only for dirty-box tiles.
    let mut writes: Vec<(f64, f64)> = Vec::new();
    let mut y = d_min_y;
    while y <= d_max_y {
        let row_start = y * w;
        let mut x = d_min_x;
        while x <= d_max_x {
            let tile = row_start + x;
            let b = byte(tile);
            if (b & TERRAIN_LAND_MASK) == 0 {
                // Reached by BFS → magnitude from distance; unreached →
                // nearest coast is >MAX_MAG_DIST away → deep water (31).
                let new_mag = if stamp_arr[tile as usize] == stamp as u16 {
                    ((dist_arr[tile as usize] as f64) / 2.0).ceil().min(31.0)
                } else {
                    31.0
                };
                if (b & TERRAIN_MAG_MASK) as f64 != new_mag {
                    writes.push((tile, new_mag));
                    if let Some(c) = &mut changed {
                        c.add(tile);
                    }
                }
            }
            x += 1.0;
        }
        y += 1.0;
    }
    for (tile, mag) in writes {
        map.set_magnitude(tile, mag);
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const LAND: u8 = 0x85; // land, magnitude 5
    const OCEAN: u8 = 0x20; // water with ocean bit

    fn gm(w: f64, h: f64, cells: Vec<u8>) -> GameMap {
        GameMap::new(w, h, cells, w * h)
    }

    fn all_land(w: f64, h: f64) -> GameMap {
        gm(w, h, vec![LAND; (w * h) as usize])
    }

    #[test]
    fn tick_converts_pending_and_bumps_version_after_throttle() {
        let map = all_land(4.0, 4.0);
        let mini = all_land(2.0, 2.0);
        let mut wm = WaterManager::new(map, mini, false);
        // Three of the four full-map tiles under mini tile 0 convert, so the
        // minimap folds them in (>= min(3, 4)) and marks the graph dirty.
        for t in [0.0, 1.0, 4.0] {
            wm.queue_tile(t);
        }
        let changed = wm.run_op(1, &[0.0]);
        assert!(changed.contains(&0.0));
        assert_eq!(wm.water_graph_version(), 0.0);
        // Rebuild is throttled to 20 ticks and never shares a tick with
        // conversion.
        wm.run_op(1, &[19.0]);
        assert_eq!(wm.water_graph_version(), 0.0);
        wm.run_op(1, &[20.0]);
        assert_eq!(wm.water_graph_version(), 1.0);
        // Mini tile 0 became water and got a fresh component id.
        assert_eq!(wm.run_op(3, &[0.0]), vec![0.0, 1.0]);
    }

    #[test]
    fn disable_nav_mesh_permissive_fallbacks() {
        let map = all_land(4.0, 4.0);
        let mini = all_land(2.0, 2.0);
        let mut wm = WaterManager::new(map, mini, true);
        wm.queue_tile(5.0);
        wm.run_op(1, &[0.0]);
        wm.run_op(1, &[100.0]);
        assert_eq!(wm.water_graph_version(), 0.0);
        assert_eq!(wm.run_op(3, &[5.0]), vec![0.0, 0.0]); // graph-null -> 0
        assert_eq!(wm.run_op(4, &[5.0, 99.0]), vec![1.0]); // graph-null -> true
        assert_eq!(wm.run_op(5, &[5.0]), vec![0.0, 0.0]); // graph-null -> 0
    }

    #[test]
    fn component_query_and_size_on_converted_crater() {
        let map = all_land(8.0, 8.0);
        let mini = gm(4.0, 4.0, vec![OCEAN; 16]);
        let mut wm = WaterManager::new(map, mini, false);
        wm.queue_tile(18.0); // (x2,y2) -> mini (1,1)
        wm.run_op(1, &[0.0]);
        let r = wm.run_op(3, &[18.0]);
        assert_eq!(r[0], 0.0);
        assert!(r[1] > 0.0); // a real component id
        let s = wm.run_op(5, &[18.0]);
        assert_eq!(s[0], 0.0);
        assert!(s[1] >= 4.0);
        assert!(wm.has_water_component(18.0, r[1]));
    }

    #[test]
    fn magnitude_shore_gradient_after_conversion() {
        // All-land full map over an ocean minimap: converting one tile
        // creates a crater whose shoreline neighbors get magnitude updates.
        let map = all_land(6.0, 6.0);
        let mini = gm(3.0, 3.0, vec![OCEAN; 9]);
        let mut wm = WaterManager::new(map, mini, false);
        wm.queue_tile(15.0); // (x3,y2)
        let changed = wm.run_op(1, &[0.0]);
        // The converted tile itself is in `changed`.
        assert!(changed.contains(&15.0));
        // Neighbors became shoreline and/or magnitude-changed.
        assert!(changed.contains(&9.0) && changed.contains(&21.0));
    }
}
