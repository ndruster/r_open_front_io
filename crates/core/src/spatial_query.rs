//! Port of `src/core/pathfinding/spatial/SpatialQuery.ts`.
//!
//! `SpatialQuery` bundles the shore/water lookup helpers the transport-ship and
//! deployment code paths share. It holds the *real* [`WaterManager`] (which
//! owns the main map and the half-resolution mini map), so terrain, ownership
//! and water-component reads run on the ported engine rather than scripted
//! stand-ins — the TS capture side does the same with real `GameMapImpl` /
//! `WaterManager` instances behind a mock `Game` facade.
//!
//! The mock `Game` in the capture provides no `miniWaterHPA()`/graph, so
//! `PathFinding.Water` always takes the `buildWaterChain` *simple* branch:
//! `AStarWater(miniMap)` wrapped by `ShoreCoercingTransformer` and
//! `MiniMapTransformer`, behind a `PathFinderStepper` configured with the
//! production `tileStepperConfig`. The shared per-game chain cache is a
//! performance memo with no observable state (each `findPath` re-stamps its
//! scratch arrays), so this port assembles the chain per call.
//!
//! JS-isms pinned here:
//!
//! * `bfsNearest` keeps the *first* candidate at the minimum distance via a
//!   strict `<` on the Manhattan distance — the winner depends only on the
//!   deterministic traversal order (LIFO stack, neighbors visited N/S/W/E).
//! * The per-game traversal scratch (`visited` Uint32Array + generation stamp)
//!   is shared across queries on one game — one `SpatialQuery` owns one slot.
//!   `bumpTraversalGeneration` wraps at `0xffffffff` with `fill(0)`.
//! * Fractional/NaN tile refs never hit the `visited` buffer (typed-array
//!   writes drop, reads are `undefined` → `!== gen` is true) — `bfsNearest`
//!   guards `from` with `isValidRef` but neighbor reads rely on the typed-array
//!   semantics, reproduced via [`canon_index`].
//! * `reachableComponents` is a `WeakMap<Player, cache>` in TS; the port keeps
//!   a single slot (one attacker per scenario) with the same
//!   tileVersion+waterVersion reuse test.
//! * `refineStartTile` binary-searches with `(lo + hi) >> 1` for at most 5
//!   iterations, `Math.min`/`Math.max` over f64 bounds, and falls back to
//!   `path[0]` whenever the bounded re-search finds nothing better.

use crate::game_map::GameMap;
use crate::jsnum::to_uint32;
use crate::pathfinding::mini_map_transformer::MiniMapTransformer;
use crate::pathfinding::shore_coercing_transformer::ShoreCoercingTransformer;
use crate::pathfinding::stepper::PathFinderStepper;
use crate::pathfinding::water::AStarWater;
use crate::pathfinding::water_bounded::AStarWaterBounded;
use crate::pathfinding::PathStart;
use crate::water_manager::WaterManager;

/// `REFINE_MAX_SEARCH_AREA = 100 * 100`.
const REFINE_MAX_SEARCH_AREA: f64 = 100.0 * 100.0;
/// `0xffffffff` — the generation value that triggers the `fill(0)` wrap.
const GEN_WRAP: f64 = 4_294_967_295.0;

/// Canonical numeric index for typed-array element access: fractional/NaN/negative
/// refs address plain object properties in JS (invisible to the buffer), so the
/// read is `undefined` and the write is dropped.
fn canon_index(v: f64) -> Option<usize> {
    if !v.is_finite() || v < 0.0 || v != v.trunc() || v > 9_007_199_254_740_991.0 {
        return None;
    }
    Some(v as usize)
}

/// The `bfsNearest` predicate. Only two shapes occur in the TS source:
/// `closestShore` (owned shore) and `closestReachableShore` (owned shore on a
/// water component reachable from the attacker's border).
enum ShorePred {
    Owned { owner_id: f64 },
    OwnedReachable { owner_id: f64 },
}

/// `SpatialQuery` from the TS source, holding the real `WaterManager`.
pub struct SpatialQuery {
    water: WaterManager,
    /// Mini-map geometry snapshot for the simple water chain (`AStarWater` runs
    /// on the mini map's terrain, which is immutable).
    mini_w: f64,
    mini_h: f64,
    mini_terrain: Vec<u8>,
    // Per-game traversal scratch (TS WeakMap single slot): visited Uint32Array
    // + generation counter + reusable stack.
    visited: Vec<u32>,
    stack: Vec<f64>,
    gen: f64,
    /// `boundedAStar` — lazy (`??=`) refinement A* over the main map.
    bounded: Option<AStarWaterBounded>,
    /// `reachableComponents` memo: valid while the attacker's tiles (hence
    /// tileChangeVersion) and the map's water are unchanged.
    reach_tile_version: Option<f64>,
    reach_water_version: Option<f64>,
    reach_components: Vec<f64>,
}

impl SpatialQuery {
    /// `new SpatialQuery(game)` — the capture passes real `GameMapImpl`s and
    /// the nav-mesh disable flag straight through to `WaterManager`.
    pub fn new(map: GameMap, mini_map: GameMap, disable_nav_mesh: bool) -> Self {
        let mini_w = mini_map.width();
        let mini_h = mini_map.height();
        let mini_terrain = mini_map.debug_terrain().to_vec();
        Self {
            water: WaterManager::new(map, mini_map, disable_nav_mesh),
            mini_w,
            mini_h,
            mini_terrain,
            visited: Vec::new(),
            stack: Vec::new(),
            gen: 0.0,
            bounded: None,
            reach_tile_version: None,
            reach_water_version: None,
            reach_components: Vec::new(),
        }
    }

    fn map(&self) -> &GameMap {
        self.water.debug_map()
    }

    /// Borrowed main map for callers that need the full `GameMap` surface.
    pub fn debug_map(&self) -> &GameMap {
        self.water.debug_map()
    }

    /// `bfsNearest(from, maxDist, predicate)` — stack BFS, first candidate at
    /// the minimum Manhattan distance wins.
    fn bfs_nearest(&mut self, from: f64, max_dist: f64, pred: &ShorePred) -> Option<f64> {
        // A fractional ref would make `visited[t] = gen` a silent typed-array
        // no-op (dedup fails, stack grows unbounded); every caller is guarded.
        {
            let map = self.map();
            if !map.is_valid_ref(from) {
                return None;
            }
        }
        // bumpTraversalGeneration: gen++ with the 0xffffffff wrap.
        self.gen += 1.0;
        if self.gen == GEN_WRAP {
            self.visited.fill(0);
            self.gen = 1.0;
        }
        let gen = self.gen;
        // The scratch `visited` buffer is sized for the game once; keep it.
        let total = {
            let map = self.map();
            (map.width() * map.height()) as usize
        };
        if self.visited.len() < total {
            self.visited = vec![0u32; total];
        }
        self.stack.clear();

        let mut best: Option<f64> = None;
        let mut best_dist = f64::INFINITY;

        if max_dist >= 0.0 {
            self.mark(from, from, gen, pred, &mut best, &mut best_dist);
        }
        while let Some(curr) = self.stack.pop() {
            // Collect neighbors first (for_each_neighbor borrows the map
            // immutably; visit needs &mut self for the water-component read).
            let mut neigh = [0.0f64; 4];
            let mut n = 0usize;
            self.map().for_each_neighbor(curr, |t| {
                if n < 4 {
                    neigh[n] = t;
                    n += 1;
                }
            });
            for t in neigh[..n].iter().copied() {
                if self.visited_neq_gen(t, gen) {
                    let d = self.map().manhattan_dist(from, t);
                    if d <= max_dist {
                        self.mark(from, t, gen, pred, &mut best, &mut best_dist);
                    }
                }
            }
        }
        best
    }

    /// The `mark` closure: typed-array write + push + predicate test + the
    /// strict-`<` Manhattan-distance keep (first minimum wins).
    fn mark(
        &mut self,
        from: f64,
        t: f64,
        gen: f64,
        pred: &ShorePred,
        best: &mut Option<f64>,
        best_dist: &mut f64,
    ) {
        if let Some(k) = canon_index(t) {
            if k < self.visited.len() {
                self.visited[k] = to_uint32(gen);
            }
        }
        self.stack.push(t);
        if self.pred_ok(t, pred) {
            let dist = self.map().manhattan_dist(from, t);
            if dist < *best_dist {
                *best = Some(t);
                *best_dist = dist;
            }
        }
    }

    fn visited_neq_gen(&self, t: f64, gen: f64) -> bool {
        match canon_index(t) {
            Some(k) if k < self.visited.len() => (self.visited[k] as f64) != gen,
            _ => true, // undefined !== gen
        }
    }

    fn pred_ok(&mut self, t: f64, pred: &ShorePred) -> bool {
        let map = self.water.debug_map();
        if !map.is_shore(t) || !map.is_land(t) {
            return false;
        }
        match pred {
            ShorePred::Owned { owner_id } => map.owner_id(t) == *owner_id,
            ShorePred::OwnedReachable { owner_id } => {
                if map.owner_id(t) != *owner_id {
                    return false;
                }
                match self.water.get_water_component(t) {
                    None => false,
                    Some(c) => self.reach_components.contains(&c),
                }
            }
        }
    }

    /// `closestShore(owner, tile, maxDist = 50)`.
    pub fn closest_shore(&mut self, owner_small_id: f64, tile: f64, max_dist: f64) -> Option<f64> {
        self.bfs_nearest(tile, max_dist, &ShorePred::Owned { owner_id: owner_small_id })
    }

    /// `closestReachableShore(targetOwner, attacker, tile, maxDist = 50)`.
    pub fn closest_reachable_shore(
        &mut self,
        target_small_id: f64,
        attacker_border: &[f64],
        attacker_tile_version: f64,
        tile: f64,
        max_dist: f64,
    ) -> Option<f64> {
        self.reachable_components(attacker_border, attacker_tile_version);
        if self.reach_components.is_empty() {
            return None;
        }
        // The start tile sits at distance 0: when it qualifies no BFS can beat
        // it, so return it directly.
        if max_dist >= 0.0 && self.map().is_valid_ref(tile) {
            let map = self.water.debug_map();
            let qualifies = map.is_shore(tile)
                && map.is_land(tile)
                && map.owner_id(tile) == target_small_id
                && match self.water.get_water_component(tile) {
                    None => false,
                    Some(c) => self.reach_components.contains(&c),
                };
            if qualifies {
                return Some(tile);
            }
        }
        self.bfs_nearest(
            tile,
            max_dist,
            &ShorePred::OwnedReachable { owner_id: target_small_id },
        )
    }

    /// `reachableComponents(attacker)` — memo keyed on the attacker's
    /// tileChangeVersion and the map's waterVersion. Refreshes the single
    /// slot in place.
    fn reachable_components(&mut self, border: &[f64], tile_version: f64) {
        let water_version = self.map().water_version();
        if self.reach_tile_version == Some(tile_version)
            && self.reach_water_version == Some(water_version)
        {
            return;
        }
        self.reach_components.clear();
        for &t in border {
            let map = self.water.debug_map();
            if !map.is_shore(t) || !map.is_land(t) {
                continue;
            }
            if let Some(c) = self.water.get_water_component(t) {
                if !self.reach_components.contains(&c) {
                    self.reach_components.push(c);
                }
            }
        }
        self.reach_tile_version = Some(tile_version);
        self.reach_water_version = Some(water_version);
    }

    /// `closestShoreByWater(owner, target)` — `owner.isPlayer()` is checked by
    /// the caller (the TS body returns `null` for terra nullius, which has no
    /// border tiles; the capture only drives this with players).
    pub fn closest_shore_by_water(&mut self, attacker_border: &[f64], target: f64) -> Option<f64> {
        let map = self.water.debug_map();
        // Target must be water or shore (land adjacent to water).
        if !map.is_water(target) && !map.is_shore(target) {
            return None;
        }
        let target_component = self.water.get_water_component(target)?;
        // Single pass over the border set.
        let mut shores: Vec<f64> = Vec::new();
        for &t in attacker_border {
            let map = self.water.debug_map();
            let same_component = map.is_shore(t)
                && map.is_land(t)
                && match self.water.get_water_component(t) {
                    Some(c) => c == target_component,
                    None => false,
                };
            if same_component {
                shores.push(t);
            }
        }
        if shores.is_empty() {
            return None;
        }
        let path = self.water_find_path(&shores, target)?;
        if path.is_empty() {
            return None;
        }
        Some(self.refine_start_tile(&path, &shores))
    }

    /// `PathFinding.Water(gm).findPath(shores, target)` — the *simple* chain
    /// (the mock game has no mini-HPA graph): `AStarWater(miniMap)` →
    /// `ShoreCoercing` → `MiniMap`, behind the production tile stepper.
    fn water_find_path(&mut self, shores: &[f64], target: f64) -> Option<Vec<f64>> {
        let simple = AStarWater::new(
            self.mini_w,
            self.mini_h,
            self.mini_terrain.clone(),
            None,
            None,
        );
        let map_ptr: &GameMap = self.water.debug_map();
        let mini_ptr: &GameMap = self.water.debug_mini_map();
        let shore = ShoreCoercingTransformer::new(simple, mini_ptr);
        let chain = MiniMapTransformer::new(shore, map_ptr, mini_ptr);
        let mut stepper = PathFinderStepper::new(chain, Some(map_ptr));
        stepper.find_path(PathStart::Multi(shores), target)
    }

    /// `refineStartTile(path, shores, gm)` — binary-search for the furthest
    /// waypoint whose bounding box stays inside the refinement area, then
    /// re-run a bounded A* and prefer its first tile.
    fn refine_start_tile(&mut self, path: &[f64], shores: &[f64]) -> f64 {
        const CANDIDATE_RADIUS: f64 = 20.0;
        const MIN_WAYPOINT_DIST: f64 = 50.0;
        const MAX_WAYPOINT_DIST: f64 = 200.0;
        const PADDING: f64 = 10.0;

        if (path.len() as f64) <= MIN_WAYPOINT_DIST {
            return path[0];
        }

        let best_tile = path[0];
        let map = self.water.debug_map();
        let w = map.width();
        let h = map.height();

        let candidates: Vec<f64> = shores
            .iter()
            .copied()
            .filter(|&s| map.manhattan_dist(s, best_tile) <= CANDIDATE_RADIUS)
            .collect();

        if candidates.len() <= 1 {
            return best_tile;
        }

        // Precompute candidate bounds.
        let mut cand_min_x = map.x(candidates[0]);
        let mut cand_max_x = cand_min_x;
        let mut cand_min_y = map.y(candidates[0]);
        let mut cand_max_y = cand_min_y;
        for &c in candidates.iter().skip(1) {
            let sx = map.x(c);
            let sy = map.y(c);
            cand_min_x = cand_min_x.min(sx);
            cand_max_x = cand_max_x.max(sx);
            cand_min_y = cand_min_y.min(sy);
            cand_max_y = cand_max_y.max(sy);
        }

        // Binary search for the furthest waypoint keeping the area in bounds.
        let mut lo = MIN_WAYPOINT_DIST;
        let mut hi = MAX_WAYPOINT_DIST.min((path.len() as f64) - 1.0);
        let mut best_waypoint_idx = lo;

        // `for (let i = 0; i < 5 && lo <= hi; i++)`.
        let mut i = 0;
        while i < 5 && lo <= hi {
            let mid = to_int32_shr1(lo + hi);
            let wp = path[mid as usize];
            let wpx = map.x(wp);
            let wpy = map.y(wp);

            let min_x = cand_min_x.min(wpx) - PADDING;
            let max_x = cand_max_x.max(wpx) + PADDING;
            let min_y = cand_min_y.min(wpy) - PADDING;
            let max_y = cand_max_y.max(wpy) + PADDING;

            let area = (max_x - min_x + 1.0) * (max_y - min_y + 1.0);
            if area <= REFINE_MAX_SEARCH_AREA {
                best_waypoint_idx = mid;
                lo = mid + 1.0;
            } else {
                hi = mid - 1.0;
            }
            i += 1;
        }

        let waypoint = path[best_waypoint_idx as usize];
        let wpx = map.x(waypoint);
        let wpy = map.y(waypoint);

        let bounds_min_x = 0.0f64.max(cand_min_x.min(wpx) - PADDING);
        let bounds_max_x = (w - 1.0).min(cand_max_x.max(wpx) + PADDING);
        let bounds_min_y = 0.0f64.max(cand_min_y.min(wpy) - PADDING);
        let bounds_max_y = (h - 1.0).min(cand_max_y.max(wpy) + PADDING);

        let bounds_area =
            (bounds_max_x - bounds_min_x + 1.0) * (bounds_max_y - bounds_min_y + 1.0);
        if bounds_area > REFINE_MAX_SEARCH_AREA {
            return best_tile;
        }

        let bounded = self.get_bounded_a_star();
        match bounded.search_bounded(
            &candidates,
            waypoint,
            bounds_min_x,
            bounds_max_x,
            bounds_min_y,
            bounds_max_y,
        ) {
            Some(refined) if !refined.is_empty() => refined[0],
            // `refinedPath?.[0] ?? bestTile` — null or empty both fall back.
            _ => best_tile,
        }
    }

    /// `getBoundedAStar()` — lazy bounded A* over the main map's terrain.
    fn get_bounded_a_star(&mut self) -> &mut AStarWaterBounded {
        if self.bounded.is_none() {
            let map = self.water.debug_map();
            let w = map.width();
            let terrain = map.debug_terrain().to_vec();
            self.bounded = Some(AStarWaterBounded::new(
                w,
                terrain,
                REFINE_MAX_SEARCH_AREA,
                None,
                None,
            ));
        }
        self.bounded.as_mut().unwrap()
    }

    /// `gm.getWaterComponent(tile)` — the persistent `WaterManager` labeling.
    pub fn get_water_component(&mut self, tile: f64) -> Option<f64> {
        self.water.get_water_component(tile)
    }

    /// `gm.hasWaterComponent(tile, component)`.
    pub fn has_water_component(&mut self, tile: f64, component: f64) -> bool {
        self.water.has_water_component(tile, component)
    }

    /// The flood-fill core of `PlayerImpl.validStructureSpawnTiles`
    /// (`PlayerImpl.ts:1729-1761`): a stack walk over the player's own tiles
    /// inside the Euclidean radius, in the shared `GameMap.bfs` N/S/W/E
    /// push order, on the same per-game traversal scratch
    /// (`tileTraversalScratch` / `bumpTraversalGeneration`) the shore queries
    /// use. The `inside` test is `dx*dx + dy*dy < searchRadiusSquared &&
    /// map.ownerID(t) === smallID`.
    pub fn flood_owned_tiles(&mut self, tile: f64, small_id: f64, radius: f64) -> Vec<f64> {
        let radius_squared = radius * radius;
        // Destructure for disjoint borrows: the map (inside `water`) is read
        // immutably while visited / stack / gen mutate.
        let Self {
            water,
            visited,
            stack,
            gen,
            ..
        } = self;
        let map = water.debug_map();
        let w = map.width();
        let cx = tile % w;
        let cy = map.y(tile);
        let inside = |t: f64| -> bool {
            let dx = (t % w) - cx;
            let dy = map.y(t) - cy;
            dx * dx + dy * dy < radius_squared && map.owner_id(t) == small_id
        };
        // bumpTraversalGeneration: gen++ with the 0xffffffff wrap.
        *gen += 1.0;
        if *gen == GEN_WRAP {
            visited.fill(0);
            *gen = 1.0;
        }
        let g = *gen;
        let total = (map.width() * map.height()) as usize;
        if visited.len() < total {
            *visited = vec![0u32; total];
        }
        stack.clear();
        let mut nearby_tiles: Vec<f64> = Vec::new();
        // `mark` as a macro-free inline block is awkward with the disjoint
        // borrows; a plain loop body keeps `visited` reads and writes split.
        macro_rules! mark {
            ($t:expr) => {{
                let t = $t;
                if t.is_finite() && t.fract() == 0.0 && t >= 0.0 {
                    let i = t as usize;
                    if i < visited.len() {
                        visited[i] = g as u32;
                    }
                }
                nearby_tiles.push(t);
            }};
        }
        if inside(tile) {
            mark!(tile);
            stack.push(tile);
        }
        while let Some(curr) = stack.pop() {
            let mut neigh = [0.0f64; 4];
            let mut n = 0usize;
            map.for_each_neighbor(curr, |t| {
                if n < 4 {
                    neigh[n] = t;
                    n += 1;
                }
            });
            for t in neigh[..n].iter().copied() {
                // `visited[n] !== gen` on the Uint32Array: an invalid ref
                // reads `undefined`, which never equals the stamp.
                let canonical = t.is_finite() && t.fract() == 0.0 && t >= 0.0;
                let unseen = !canonical
                    || (t as usize) >= visited.len()
                    || visited[t as usize] != g as u32;
                if unseen && inside(t) {
                    mark!(t);
                    stack.push(t);
                }
            }
        }
        nearby_tiles
    }
}

/// `(lo + hi) >> 1` — JS ToInt32 then arithmetic right shift; the operands are
/// small non-negative integers so the shift is the plain floor-half.
fn to_int32_shr1(v: f64) -> f64 {
    (crate::jsnum::to_int32(v) >> 1) as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_map::GameMap;

    /// 8x8 map: land rows 0-2 (tiles 0-23), water rows 3-6 (24-55), land row 7
    /// (56-63). Terrain byte layout matches `GameMapImpl`: bit 7 land,
    /// bits 0-4 magnitude. `num_land_tiles` = 24.
    fn small_maps() -> (GameMap, GameMap) {
        let w = 8.0;
        let h = 8.0;
        let mut terrain = vec![0x00u8; 64]; // water, magnitude 0
        terrain[..24].fill(0x83); // land
        terrain[56..64].fill(0x83);
        let mini = GameMap::new(w / 2.0, h / 2.0, vec![0x83; 16], 8.0);
        (GameMap::new(w, h, terrain, 24.0), mini)
    }

    #[test]
    fn to_int32_shr1_matches_js() {
        assert_eq!(to_int32_shr1(99.0), 49.0);
        assert_eq!(to_int32_shr1(100.0), 50.0);
        assert_eq!(to_int32_shr1(101.0), 50.0);
    }

    #[test]
    fn bfs_nearest_invalid_ref_is_none() {
        let (map, mini) = small_maps();
        let mut sq = SpatialQuery::new(map, mini, true);
        assert_eq!(sq.closest_shore(1.0, 1.5, 50.0), None);
        assert_eq!(sq.closest_shore(1.0, -1.0, 50.0), None);
        assert_eq!(sq.closest_shore(1.0, 64.0, 50.0), None);
    }

    #[test]
    fn closest_shore_finds_first_min() {
        // Owner 1 owns tile 20 (x4,y2 — shore: row 2 land above water row 3).
        let (mut map, mini) = small_maps();
        map.set_shoreline_bit(20.0);
        map.set_owner_id(20.0, 1.0);
        let mut sq = SpatialQuery::new(map, mini, true);
        // From tile 28 (x4,y4, water): manhattan 0+2 = 2.
        assert_eq!(sq.closest_shore(1.0, 28.0, 50.0), Some(20.0));
    }

    #[test]
    fn closest_shore_respects_max_dist() {
        // Owner 1 owns only tile 60 (x4,y7 — shore row 7, 4 below tile 28).
        let (mut map, mini) = small_maps();
        map.set_shoreline_bit(60.0);
        map.set_owner_id(60.0, 1.0);
        let mut sq = SpatialQuery::new(map, mini, true);
        assert_eq!(sq.closest_shore(1.0, 28.0, 3.0), None);
        assert_eq!(sq.closest_shore(1.0, 28.0, 4.0), Some(60.0));
    }

    #[test]
    fn closest_shore_owner_filter() {
        // Tile 20 shore owned by player 2 — owner 1 sees nothing.
        let (mut map, mini) = small_maps();
        map.set_shoreline_bit(20.0);
        map.set_owner_id(20.0, 2.0);
        let mut sq = SpatialQuery::new(map, mini, true);
        assert_eq!(sq.closest_shore(1.0, 28.0, 50.0), None);
        assert_eq!(sq.closest_shore(2.0, 28.0, 50.0), Some(20.0));
    }
}
