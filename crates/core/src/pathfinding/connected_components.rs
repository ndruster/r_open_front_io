//! Port of `src/core/pathfinding/algorithms/ConnectedComponents.ts`.
//!
//! Connected-component labelling over a `GameMap`-shaped terrain surface,
//! using a scan-line flood fill plus a union-find alias table for incremental
//! water additions. The observable state is the packed `componentIds` buffer
//! (which upgrades from `Uint8Array` to `Uint16Array` past 253 components),
//! the `landMarker` sentinel that changes with it, the sparse
//! `_componentSizes` array, the `parents` union-find table (mutated by path
//! compression on every query), and `maxId`.
//!
//! Faithfulness notes — every one of these is pinned by the parity traces in
//! `gen_vectors.mjs`:
//!
//! * Terrain is read as `GameMapImpl`'s packed bytes: a tile is land when bit
//!   7 is set, water otherwise. Both the direct (`Uint32Array` 4-byte chunk
//!   write) and per-tile (`isWater`) pre-fill paths are transcribed; they are
//!   bit-identical but the direct one uses the `-((chunk >> k) & 1)` sign
//!   trick, replicated with `wrapping_sub`.
//! * `componentIds` is a real `Uint8Array` until the 253rd component forces a
//!   `Uint16Array` upgrade (because component id 254 would collide with the
//!   0xFF land marker); the land marker itself moves 0xFF -> 0xFFFF.
//! * `_componentSizes` is a sparse JS `number[]` — index 0 is a permanent
//!   hole. It is modelled as `Vec<Option<f64>>` so the hole survives the
//!   comparison.
//! * `find` mutates `parents` with path compression, so query order is
//!   observable; the final `parents` buffer is part of the trace.
//! * The flood-fill queue is a fixed `Int32Array(numTiles)`. Out-of-range
//!   writes are dropped (the slot keeps its stale value) and out-of-range
//!   reads yield `undefined`, which the `ids[seed] !== 0` guard treats as
//!   "already processed" — replicated by the `head < n` checks.

/// `LAND_MARKER` from the TS source (Uint8Array sentinel).
const LAND_MARKER: i64 = 0xff;
/// `LAND_MARKER_WIDE` (Uint16Array sentinel after promotion).
const LAND_MARKER_WIDE: i64 = 0xffff;

/// Canonical typed-array index for a JS `TileRef`, or `None` where the read
/// would yield `undefined` (and `?? 0` collapses it to 0).
fn canonical(tile: f64, len: usize) -> Option<usize> {
    if !tile.is_finite() || tile.fract() != 0.0 || tile < 0.0 {
        return None;
    }
    let i = tile as usize;
    if i < len {
        Some(i)
    } else {
        None
    }
}

/// The `componentIds` buffer, which changes element width on upgrade.
#[derive(Clone, Debug)]
pub enum Ids {
    U8(Vec<u8>),
    U16(Vec<u16>),
}

impl Ids {
    #[inline]
    fn len(&self) -> usize {
        match self {
            Ids::U8(v) => v.len(),
            Ids::U16(v) => v.len(),
        }
    }

    #[inline]
    fn is_u8(&self) -> bool {
        matches!(self, Ids::U8(_))
    }

    /// Read an in-range element as a JS number.
    #[inline]
    fn get(&self, i: usize) -> i64 {
        match self {
            Ids::U8(v) => i64::from(v[i]),
            Ids::U16(v) => i64::from(v[i]),
        }
    }

    /// Write an in-range element (the `Uint8Array`/`Uint16Array` truncation).
    #[inline]
    fn set(&mut self, i: usize, v: i64) {
        match self {
            Ids::U8(buf) => buf[i] = v as u8,
            Ids::U16(buf) => buf[i] = v as u16,
        }
    }

    /// `upgradeToUint16Array`: remap 0xFF land markers to 0xFFFF.
    fn upgrade(self) -> Ids {
        let Ids::U8(old) = self else { return self };
        let mut new = vec![0u16; old.len()];
        for (i, &b) in old.iter().enumerate() {
            new[i] = if i64::from(b) == LAND_MARKER {
                LAND_MARKER_WIDE as u16
            } else {
                u16::from(b)
            };
        }
        Ids::U16(new)
    }
}

pub struct ConnectedComponents {
    width: i64,
    num_tiles: i64,
    last_row_start: i64,
    /// `map.terrain` — the packed GameMapImpl bytes the constructor reads.
    terrain: Vec<u8>,
    access_directly: bool,
    component_ids: Option<Ids>,
    sizes: Vec<Option<f64>>,
    parents: Vec<i64>,
    max_id: i64,
    land_marker: i64,
}

impl ConnectedComponents {
    /// `new ConnectedComponents(map, accessTerrainDirectly)`.
    pub fn new(width: i64, height: i64, terrain: Vec<u8>, access_directly: bool) -> Self {
        let num_tiles = width * height;
        Self {
            width,
            num_tiles,
            last_row_start: (height - 1) * width,
            terrain,
            access_directly,
            component_ids: None,
            sizes: Vec::new(),
            parents: Vec::new(),
            max_id: 0,
            land_marker: LAND_MARKER,
        }
    }

    // ---- sparse sizes helpers (`_componentSizes[id] ?? 0`) ----

    fn sizes_get(&self, i: usize) -> f64 {
        self.sizes.get(i).copied().flatten().unwrap_or(0.0)
    }

    fn sizes_set(&mut self, i: usize, v: f64) {
        while self.sizes.len() <= i {
            self.sizes.push(None);
        }
        self.sizes[i] = Some(v);
    }

    // ---- initialize ----

    pub fn initialize(&mut self) {
        let n = self.num_tiles as usize;
        let mut queue = vec![0i32; n];
        let mut ids = self.create_prefilled_ids();
        self.sizes = Vec::new();
        let mut next_id: i64 = 0;

        for start in 0..n {
            let value = ids.get(start);
            if value == LAND_MARKER || value > 0 {
                continue;
            }
            next_id += 1;
            if next_id == 253 && ids.is_u8() {
                ids = ids.upgrade();
            }
            if next_id == 0xffff {
                break;
            }
            self.flood_fill(&mut ids, &mut queue, start, next_id);
        }

        self.component_ids = Some(ids);
        self.land_marker = match self.component_ids.as_ref().unwrap() {
            Ids::U8(_) => LAND_MARKER,
            Ids::U16(_) => LAND_MARKER_WIDE,
        };
        self.max_id = next_id;
        self.parents = (0..=next_id).collect();
    }

    /// `createPrefilledIds`: a Uint8Array of land markers, water tiles 0.
    fn create_prefilled_ids(&self) -> Ids {
        let n = self.num_tiles as usize;
        let mut ids = vec![0u8; n];
        if self.access_directly {
            self.premark_direct(&mut ids);
        } else {
            self.premark_iter(&mut ids);
        }
        Ids::U8(ids)
    }

    /// `premarkLandTiles` — `ids[i] = map.isWater(i) ? 0 : 0xff`.
    fn premark_iter(&self, ids: &mut [u8]) {
        for (i, slot) in ids.iter_mut().enumerate() {
            let is_water = self.terrain[i] & 0x80 == 0;
            *slot = if is_water { 0 } else { 0xff };
        }
    }

    /// `premarkLandTilesDirect` — the 4-bytes-at-a-time `Uint32Array` write,
    /// replicated bit-for-bit (little-endian chunk, sign-trick bytes, then the
    /// `numChunks*4..numTiles` tail with the `-(terrain[i] >> 7)` ToUint8).
    fn premark_direct(&self, ids: &mut [u8]) {
        let n = self.num_tiles as usize;
        let num_chunks = n / 4;
        for i in 0..num_chunks {
            let chunk =
                u32::from_le_bytes([self.terrain[4 * i], self.terrain[4 * i + 1], self.terrain[4 * i + 2], self.terrain[4 * i + 3]]);
            let b0 = 0i32.wrapping_sub(((chunk >> 7) & 1) as i32) & 0xff;
            let b1 = 0i32.wrapping_sub(((chunk >> 15) & 1) as i32) & 0xff;
            let b2 = 0i32.wrapping_sub(((chunk >> 23) & 1) as i32) & 0xff;
            let b3 = 0i32.wrapping_sub(((chunk >> 31) & 1) as i32);
            let word = (b0 | (b1 << 8) | (b2 << 16) | (b3 << 24)) as u32;
            ids[4 * i] = (word & 0xff) as u8;
            ids[4 * i + 1] = ((word >> 8) & 0xff) as u8;
            ids[4 * i + 2] = ((word >> 16) & 0xff) as u8;
            ids[4 * i + 3] = ((word >> 24) & 0xff) as u8;
        }
        for (j, slot) in ids[num_chunks * 4..n].iter_mut().enumerate() {
            let i = num_chunks * 4 + j;
            *slot = 0i32.wrapping_sub((self.terrain[i] >> 7) as i32) as u8;
        }
    }

    /// `floodFillComponent` — scan-line fill over the shared queue.
    fn flood_fill(&mut self, ids: &mut Ids, queue: &mut [i32], start: usize, component_id: i64) {
        let n = self.num_tiles as usize;
        let width = self.width as usize;
        let last_row_start = self.last_row_start as usize;
        let mut head = 0usize;
        let mut tail = 0usize;
        if tail < n {
            queue[tail] = start as i32;
        }
        tail += 1;

        while head < tail {
            // Out-of-range read -> `undefined`; the `ids[seed] !== 0` guard
            // treats it as already-processed, so we skip.
            let seed = match head < n {
                true => queue[head] as usize,
                false => {
                    head += 1;
                    continue;
                }
            };
            head += 1;
            if ids.get(seed) != 0 {
                continue;
            }

            let row_start = seed - (seed % width);
            let mut left = seed;
            while left > row_start && ids.get(left - 1) == 0 {
                left -= 1;
            }
            let row_end = row_start + width - 1;
            let mut right = seed;
            while right < row_end && ids.get(right + 1) == 0 {
                right += 1;
            }

            let span_size = (right - left + 1) as f64;
            let cur = self.sizes_get(component_id as usize);
            self.sizes_set(component_id as usize, cur + span_size);

            let mut x = left;
            while x <= right {
                ids.set(x, component_id);
                if x >= width {
                    let above = x - width;
                    if ids.get(above) == 0 {
                        if tail < n {
                            queue[tail] = above as i32;
                        }
                        tail += 1;
                    }
                }
                if x < last_row_start {
                    let below = x + width;
                    if ids.get(below) == 0 {
                        if tail < n {
                            queue[tail] = below as i32;
                        }
                        tail += 1;
                    }
                }
                x += 1;
            }
        }
    }

    // ---- incremental water ----

    /// `addWaterTiles([tile])` for a single tile (the loop is per-tile).
    pub fn add_water_tile(&mut self, tile: f64) {
        if self.component_ids.is_none() {
            return;
        }
        let len = self.component_ids.as_ref().unwrap().len();
        let t = match canonical(tile, len) {
            Some(i) => i,
            // ids[tile] === undefined !== landMarker -> `continue`.
            None => return,
        };
        if self.component_ids.as_ref().unwrap().get(t) != self.land_marker {
            return; // already labeled water
        }

        let width = self.width as usize;
        let x = t % width;
        let mut r = [0i64; 4];
        if t >= width {
            r[0] = self.root_at(t - width);
        }
        if (t as i64) < self.last_row_start {
            r[1] = self.root_at(t + width);
        }
        if x > 0 {
            r[2] = self.root_at(t - 1);
        }
        if x + 1 < width {
            r[3] = self.root_at(t + 1);
        }

        let mut canon = 0i64;
        for &ri in &r {
            if ri != 0 && (canon == 0 || ri < canon) {
                canon = ri;
            }
        }

        if canon == 0 {
            let id = self.alloc_component_id();
            self.component_ids.as_mut().unwrap().set(t, id);
            self.sizes_set(id as usize, 1.0);
            return;
        }

        self.component_ids.as_mut().unwrap().set(t, canon);
        let cur = self.sizes_get(canon as usize);
        self.sizes_set(canon as usize, cur + 1.0);

        for &ri in &r {
            if ri != 0 && ri != canon && self.parents[ri as usize] != canon {
                self.parents[ri as usize] = canon;
                let a = self.sizes_get(canon as usize);
                let b = self.sizes_get(ri as usize);
                self.sizes_set(canon as usize, a + b);
                self.sizes_set(ri as usize, 0.0);
            }
        }
    }

    /// `rootAt`: component root of an in-range water tile, or 0 if land/unlabeled.
    fn root_at(&mut self, index: usize) -> i64 {
        let id = self.component_ids.as_ref().unwrap().get(index);
        if id == 0 || id == self.land_marker {
            return 0;
        }
        self.find(id)
    }

    /// `find` with path compression (mutates `parents`).
    fn find(&mut self, id: i64) -> i64 {
        let mut root = id;
        while self.parents[root as usize] != root {
            root = self.parents[root as usize];
        }
        let mut cur = id;
        while self.parents[cur as usize] != root {
            let next = self.parents[cur as usize];
            self.parents[cur as usize] = root;
            cur = next;
        }
        root
    }

    /// `allocComponentId`: bump `maxId`, upgrading the buffer past 253.
    fn alloc_component_id(&mut self) -> i64 {
        if self.max_id >= 0xfffe {
            return self.max_id;
        }
        self.max_id += 1;
        let id = self.max_id;
        if id >= 253 && self.component_ids.as_ref().unwrap().is_u8() {
            let old = self.component_ids.take().unwrap();
            self.component_ids = Some(old.upgrade());
            self.land_marker = LAND_MARKER_WIDE;
        }
        while self.parents.len() <= id as usize {
            self.parents.push(0);
        }
        self.parents[id as usize] = id;
        id
    }

    // ---- queries ----

    /// `getComponentId(tile)`.
    pub fn get_component_id(&mut self, tile: f64) -> i64 {
        let ids = match &self.component_ids {
            Some(x) => x,
            None => return 0,
        };
        let len = ids.len();
        let id = match canonical(tile, len) {
            Some(i) => ids.get(i),
            None => 0,
        };
        if id == 0 || id == self.land_marker {
            return id;
        }
        self.find(id)
    }

    /// `getComponentSize(componentId)`.
    pub fn get_component_size(&mut self, component_id: f64) -> f64 {
        if component_id <= 0.0 || component_id > self.max_id as f64 {
            return 0.0;
        }
        let root = self.find(component_id as i64) as usize;
        self.sizes_get(root)
    }

    // ---- debug accessors (parity harness) ----

    pub fn debug_ids(&self) -> Vec<f64> {
        match &self.component_ids {
            Some(ids) => (0..ids.len()).map(|i| ids.get(i) as f64).collect(),
            None => Vec::new(),
        }
    }

    /// 0 while `componentIds` is still `null` (pre-initialize), else 8/16.
    pub fn debug_bits(&self) -> u8 {
        match &self.component_ids {
            None => 0,
            Some(Ids::U16(_)) => 16,
            Some(Ids::U8(_)) => 8,
        }
    }

    pub fn debug_land_marker(&self) -> f64 {
        self.land_marker as f64
    }

    pub fn debug_max_id(&self) -> f64 {
        self.max_id as f64
    }

    /// Final `_componentSizes`, holes rendered as NaN (a legal size is never
    /// NaN, so the sentinel cannot alias a real value).
    pub fn debug_sizes(&self) -> Vec<f64> {
        self.sizes.iter().map(|o| o.unwrap_or(f64::NAN)).collect()
    }

    pub fn debug_parents(&self) -> Vec<f64> {
        self.parents.iter().map(|&p| p as f64).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: u8 = 0x00; // water
    const L: u8 = 0x85; // land

    #[test]
    fn single_blob_and_land_marker() {
        let cc_terrain = vec![W, W, L, W, W, L];
        let mut cc = ConnectedComponents::new(3, 2, cc_terrain, true);
        cc.initialize();
        assert_eq!(cc.get_component_id(0.0), 1);
        assert_eq!(cc.get_component_id(2.0), 255); // land marker
        assert_eq!(cc.get_component_size(1.0), 4.0);
        assert_eq!(cc.debug_max_id(), 1.0); // one blob: tiles 0,1,3,4
    }

    #[test]
    fn bridge_unions_two_components() {
        // 3x2: col0 water, col1 land, col2 water -> two components.
        let terrain = vec![W, L, W, W, L, W];
        let mut cc = ConnectedComponents::new(3, 2, terrain, false);
        cc.initialize();
        assert_eq!(cc.debug_max_id(), 2.0);
        // Add tile 1 (row0 col1): joins comp1 (left) and comp2 (right).
        cc.add_water_tile(1.0);
        assert_eq!(cc.get_component_id(1.0), 1);
        assert_eq!(cc.get_component_id(2.0), 1); // comp2 aliased to 1
        assert_eq!(cc.get_component_size(1.0), 5.0); // 2 + 2 + the added tile
        assert_eq!(cc.get_component_size(2.0), 5.0); // resolves to root 1
    }

    #[test]
    fn add_before_init_is_noop() {
        let mut cc = ConnectedComponents::new(2, 2, vec![L; 4], true);
        cc.add_water_tile(0.0);
        assert_eq!(cc.get_component_id(0.0), 0);
        assert!(cc.component_ids.is_none());
    }
}
