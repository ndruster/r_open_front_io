//! Port of `src/core/pathfinding/algorithms/AStar.WaterHierarchical.ts`.
//!
//! `AStarWaterHierarchical` is an *orchestrator*: it composes the already-ported
//! [`BfsGrid`](super::bfs_grid) (nearest-node search),
//! [`AbstractGraphAStar`](super::abstract_graph_astar) (cluster-level routing)
//! and three [`AStarWaterBounded`](super::water_bounded) engines (single-cluster
//! / 3×3-cluster / short-path local routing) over a shared [`GameMap`] and
//! [`AbstractGraph`]. The observable behaviour lives entirely in the *dispatch*
//! and the endpoint stitching, so the port transcribes the control flow
//! literally:
//!
//! * `findPath` dispatches on `Array.isArray(from)` — an array start goes to
//!   the multi-source path, a scalar to `findPathSingle` (the dispatch is the
//!   caller's; the two Rust entry points mirror it).
//! * `findPathSingle` first tries a *3×3-cluster* local search when the
//!   Manhattan distance is within one cluster (`dist <= clusterSize`); if that
//!   fails it resolves both endpoints to the nearest gateway node, runs the
//!   abstract A* over node ids, then stitches a start segment, one segment per
//!   abstract edge, and an end segment — each via a *single-cluster* local
//!   search — concatenating with `slice(1)` to drop the duplicated joint.
//! * `findLocalPath` clamps the window to the map, picks the bounded engine by
//!   `multiCluster`, and **fixes the endpoints**: the bounded search clamps the
//!   goal to the window, so a gateway tile just outside the cluster gets
//!   `unshift(from)` / `push(to)` prepended/appended when the returned ends do
//!   not match — a real path mutation the port pins.
//! * `findNearestNode` runs a stamped grid BFS over water tiles inside the
//!   start's cluster rectangle, returning the first gateway node whose `(x, y)`
//!   matches a visited tile (visitor tri-state → [`Visit`]).
//! * `SourceResolver.getClusterNode` (private helper) picks the *closest*
//!   gateway node in the tile's cluster by Manhattan distance (`< bestDist`,
//!   so ties keep the first — the cluster `nodeIds` order decides).
//! * `findPathMultiSource` first tries a short-path bounded search (candidates
//!   within 120 tiles of the target, padded window), else resolves every source
//!   to its cluster node (keeping the *closest* source per node, insertion order
//!   preserved like a JS `Map`), runs multi-source abstract A*, then re-runs
//!   `findPathSingle` from the winning source tile.
//! * `options.cachePaths` gates a direction-aware edge path cache stored *on the
//!   graph* (`getCachedPath`/`setCachedPath`); a cache hit splices
//!   `cachedPath.slice(1)` and skips the local search.
//! * `setGraph` swaps in a rebuilt graph and recreates only the graph-derived
//!   helpers (abstract A* + resolver); the map-sized bounded engines and the
//!   BFS scratch buffer are reused.

use crate::game_map::{js_max, js_min, GameMap};
use crate::pathfinding::abstract_graph::{AbstractGraph, AbstractNode};
use crate::pathfinding::abstract_graph_astar::AbstractGraphAStar;
use crate::pathfinding::bfs_grid::{BfsGrid, Visit};
use crate::pathfinding::water_bounded::AStarWaterBounded;

const SHORT_PATH_THRESHOLD: f64 = 120.0;
const PADDING: f64 = 10.0;
const SHORT_PATH_SIZE: f64 = 260.0; // 2 * (120 + padding 10)

/// `AStarWaterHierarchical` (the `options.cachePaths` flag folded in).
pub struct AStarWaterHierarchical {
    map: GameMap,
    graph: AbstractGraph,
    cache_paths: bool,
    tile_bfs: BfsGrid,
    abstract_astar: AbstractGraphAStar,
    local_astar: AStarWaterBounded,
    local_astar_multi_cluster: AStarWaterBounded,
    local_astar_short_path: AStarWaterBounded,
}

impl AStarWaterHierarchical {
    /// `new AStarWaterHierarchical(map, graph, { cachePaths })`. The bounded
    /// engines are sized from `graph.clusterSize` (single = `cs²`, multi =
    /// `(cs*3)²`) and the fixed short-path window (`260²`); the BFS scratch is
    /// `width * height`.
    pub fn new(map: GameMap, graph: AbstractGraph, cache_paths: bool) -> Self {
        let width = map.width();
        let height = map.height();
        let terrain = map.debug_terrain().to_vec();
        let cluster_size = graph.cluster_size();

        let max_local_nodes = cluster_size * cluster_size;
        let multi_cluster_size = cluster_size * 3.0;
        let max_multi_cluster_nodes = multi_cluster_size * multi_cluster_size;
        let max_short_path_nodes = SHORT_PATH_SIZE * SHORT_PATH_SIZE;

        let abstract_astar =
            AbstractGraphAStar::new(graph.node_count(), graph.edge_count(), None, None);

        Self {
            tile_bfs: BfsGrid::new(width * height),
            abstract_astar,
            local_astar: AStarWaterBounded::new(width, terrain.clone(), max_local_nodes, None, None),
            local_astar_multi_cluster: AStarWaterBounded::new(
                width,
                terrain.clone(),
                max_multi_cluster_nodes,
                None,
                None,
            ),
            local_astar_short_path: AStarWaterBounded::new(
                width,
                terrain,
                max_short_path_nodes,
                None,
                None,
            ),
            map,
            graph,
            cache_paths,
        }
    }

    /// `setGraph(graph)` — swap the graph and recreate the graph-derived
    /// abstract A* (the resolver is stateless in this port).
    pub fn set_graph(&mut self, graph: AbstractGraph) {
        self.abstract_astar =
            AbstractGraphAStar::new(graph.node_count(), graph.edge_count(), None, None);
        self.graph = graph;
    }

    /// `findPathSingle(from, to)` — the full hierarchical route.
    pub fn find_path_single(&mut self, from: f64, to: f64) -> Option<Vec<f64>> {
        let cluster_size = self.graph.cluster_size();
        let dist = self.map.manhattan_dist(from, to);

        // Early exit for very short distances (3x3-cluster local search).
        if dist <= cluster_size {
            let start_x = self.map.x(from);
            let start_y = self.map.y(from);
            let cluster_x = (start_x / cluster_size).floor();
            let cluster_y = (start_y / cluster_size).floor();
            let local_path = self.find_local_path(from, to, cluster_x, cluster_y, true);
            if local_path.is_some() {
                return local_path;
            }
        }

        let start_node = self.find_nearest_node(from);
        let end_node = self.find_nearest_node(to);

        let start_node = start_node?;
        let end_node = end_node?;

        if start_node.id == end_node.id {
            let cluster_x = (start_node.x as f64 / cluster_size).floor();
            let cluster_y = (start_node.y as f64 / cluster_size).floor();
            return self.find_local_path(from, to, cluster_x, cluster_y, true);
        }

        let node_path = self
            .abstract_astar
            .find_path_single(&self.graph, start_node.id as f64, end_node.id as f64);
        let node_path = node_path?;

        let mut initial_path: Vec<f64> = Vec::new();

        // 1. Start tile -> first gateway node.
        let first_node_tile = self.graph.get_node(node_path[0] as i64)?.tile;
        let start_x = self.map.x(from);
        let start_y = self.map.y(from);
        let start_cluster_x = (start_x / cluster_size).floor();
        let start_cluster_y = (start_y / cluster_size).floor();
        let start_segment =
            self.find_local_path(from, first_node_tile, start_cluster_x, start_cluster_y, false);
        let start_segment = start_segment?;
        initial_path.extend(start_segment);

        // 2. One segment per abstract edge.
        for i in 0..node_path.len() - 1 {
            let from_node_id = node_path[i];
            let to_node_id = node_path[i + 1];

            let edge = self
                .graph
                .get_edge_between(from_node_id as i64, to_node_id as i64)?;

            let from_tile = self.graph.get_node(from_node_id as i64)?.tile;
            let to_tile = self.graph.get_node(to_node_id as i64)?.tile;

            if self.cache_paths {
                let cached = self.graph.get_cached_path(edge.id, from_node_id as i64);
                if let Some(cached) = cached {
                    if !cached.is_empty() {
                        initial_path.extend(cached.iter().skip(1).copied());
                        continue;
                    }
                }
            }

            let segment_path = self.find_local_path(
                from_tile,
                to_tile,
                edge.cluster_x as f64,
                edge.cluster_y as f64,
                false,
            );
            let segment_path = segment_path?;
            initial_path.extend(segment_path.iter().skip(1).copied());

            if self.cache_paths {
                self.graph
                    .set_cached_path(edge.id, from_node_id as i64, segment_path);
            }
        }

        // 3. Last gateway node -> end tile.
        let last_node_tile = self.graph.get_node(*node_path.last().unwrap() as i64)?.tile;
        let end_x = self.map.x(to);
        let end_y = self.map.y(to);
        let end_cluster_x = (end_x / cluster_size).floor();
        let end_cluster_y = (end_y / cluster_size).floor();
        let end_segment =
            self.find_local_path(last_node_tile, to, end_cluster_x, end_cluster_y, false);
        let end_segment = end_segment?;
        initial_path.extend(end_segment.iter().skip(1).copied());

        Some(initial_path)
    }

    /// `findPath(start, goal)` for an *array* start (`findPathMultiSource`).
    pub fn find_path_multi(&mut self, sources: &[f64], target: f64) -> Option<Vec<f64>> {
        // Early exit: bounded A* for sources close to the target.
        if let Some(short) = self.try_short_path_multi_source(sources, target) {
            return Some(short);
        }

        let target_node = cluster_node(&self.map, &self.graph, target)?;

        // Map sources -> abstract nodes (closest source per node).
        let node_to_source = resolve_sources_to_nodes(&self.map, &self.graph, sources);
        if node_to_source.is_empty() {
            return None;
        }

        let node_ids: Vec<f64> = node_to_source.iter().map(|(id, _)| *id as f64).collect();
        let target_id = target_node.id as f64;
        let node_path = self.abstract_astar.find_path_multi(&self.graph, &node_ids, target_id);
        let node_path = node_path?;

        // `nodePath[0]` is the winning start node.
        let winning_source = node_to_source
            .iter()
            .find(|(id, _)| *id as f64 == node_path[0])
            .map(|(_, src)| *src)?;

        self.find_path_single(winning_source, target)
    }

    /// `tryShortPathMultiSource` — filter candidates within 120 tiles, build a
    /// padded bounding window, and run the short-path bounded engine.
    fn try_short_path_multi_source(&mut self, sources: &[f64], target: f64) -> Option<Vec<f64>> {
        let candidates: Vec<f64> = sources
            .iter()
            .copied()
            .filter(|&s| self.map.manhattan_dist(s, target) <= SHORT_PATH_THRESHOLD)
            .collect();
        if candidates.is_empty() {
            return None;
        }

        let to_x = self.map.x(target);
        let to_y = self.map.y(target);
        let mut min_x = to_x;
        let mut max_x = to_x;
        let mut min_y = to_y;
        let mut max_y = to_y;
        for &s in &candidates {
            let sx = self.map.x(s);
            let sy = self.map.y(s);
            min_x = js_min(min_x, sx);
            max_x = js_max(max_x, sx);
            min_y = js_min(min_y, sy);
            max_y = js_max(max_y, sy);
        }

        let b_min_x = js_max(0.0, min_x - PADDING);
        let b_max_x = js_min(self.map.width() - 1.0, max_x + PADDING);
        let b_min_y = js_max(0.0, min_y - PADDING);
        let b_max_y = js_min(self.map.height() - 1.0, max_y + PADDING);

        self.local_astar_short_path
            .search_bounded(&candidates, target, b_min_x, b_max_x, b_min_y, b_max_y)
    }

    /// `findNearestNode(tile)` — stamped grid BFS over water tiles in the
    /// tile's cluster rectangle, returning the first matching gateway node.
    fn find_nearest_node(&mut self, tile: f64) -> Option<AbstractNode> {
        let cluster_size = self.graph.cluster_size();
        let x = self.map.x(tile);
        let y = self.map.y(tile);
        let cluster_x = (x / cluster_size).floor();
        let cluster_y = (y / cluster_size).floor();

        let min_x = cluster_x * cluster_size;
        let min_y = cluster_y * cluster_size;
        let max_x = js_min(self.map.width() - 1.0, min_x + cluster_size - 1.0);
        let max_y = js_min(self.map.height() - 1.0, min_y + cluster_size - 1.0);

        let cluster = self.graph.get_cluster(cluster_x as i64, cluster_y as i64);
        let cluster = match cluster {
            Some(c) if !c.node_ids.is_empty() => c,
            _ => return None,
        };
        let candidate_nodes: Vec<AbstractNode> = cluster
            .node_ids
            .iter()
            .filter_map(|&id| self.graph.get_node(id))
            .collect();
        let max_distance = cluster_size * cluster_size;

        let width = self.map.width();
        let height = self.map.height();

        // The visitor needs `map.x/map.y` and the candidate list; the BFS
        // receiver mutates only `tile_bfs`, so the disjoint captures hold.
        self.tile_bfs.search(
            width as i64,
            height as i64,
            &[tile as i64],
            max_distance,
            |t| self.map.is_water(t as f64),
            |t, _dist| {
                let tile_x = self.map.x(t as f64);
                let tile_y = self.map.y(t as f64);
                for node in &candidate_nodes {
                    if node.x as f64 == tile_x && node.y as f64 == tile_y {
                        return Visit::Found(node.clone());
                    }
                }
                if tile_x < min_x || tile_x > max_x || tile_y < min_y || tile_y > max_y {
                    Visit::Reject
                } else {
                    Visit::Explore
                }
            },
        )
    }

    /// `findLocalPath(from, to, clusterX, clusterY, multiCluster)`.
    fn find_local_path(
        &mut self,
        from: f64,
        to: f64,
        cluster_x: f64,
        cluster_y: f64,
        multi_cluster: bool,
    ) -> Option<Vec<f64>> {
        let cluster_size = self.graph.cluster_size();
        let width = self.map.width();
        let height = self.map.height();

        let (min_x, min_y, max_x, max_y) = if multi_cluster {
            (
                js_max(0.0, (cluster_x - 1.0) * cluster_size),
                js_max(0.0, (cluster_y - 1.0) * cluster_size),
                js_min(width - 1.0, (cluster_x + 2.0) * cluster_size - 1.0),
                js_min(height - 1.0, (cluster_y + 2.0) * cluster_size - 1.0),
            )
        } else {
            let min_x = cluster_x * cluster_size;
            let min_y = cluster_y * cluster_size;
            (
                min_x,
                min_y,
                js_min(width - 1.0, min_x + cluster_size - 1.0),
                js_min(height - 1.0, min_y + cluster_size - 1.0),
            )
        };

        let astar = if multi_cluster {
            &mut self.local_astar_multi_cluster
        } else {
            &mut self.local_astar
        };
        let mut path = astar.search_bounded(&[from], to, min_x, max_x, min_y, max_y)?;
        if path.is_empty() {
            return None;
        }

        // Fix endpoints: the bounded search clamps tiles to the window, but a
        // gateway node may sit just outside it — restore the exact requested
        // tiles.
        if path[0] != from {
            path.insert(0, from);
        }
        if *path.last().unwrap() != to {
            path.push(to);
        }

        Some(path)
    }

    // ---- debug accessors (parity harness) ----

    /// The five engine stamps — the dispatch witness compared by the replay.
    pub fn debug_stamps(&self) -> (u64, u64, u64, u64, u64) {
        (
            self.tile_bfs.debug_stamp(),
            self.local_astar.debug_stamp(),
            self.local_astar_multi_cluster.debug_stamp(),
            self.local_astar_short_path.debug_stamp(),
            self.abstract_astar.debug_stamp(),
        )
    }

    /// The graph's path cache in the capture snapshot format.
    pub fn debug_path_cache(&self) -> Vec<f64> {
        self.graph.debug_path_cache()
    }
}

/// `SourceResolver.getClusterNode(tile)` — the closest gateway node in the
/// tile's cluster (Manhattan distance, `<` so ties keep the first `nodeId`).
fn cluster_node(map: &GameMap, graph: &AbstractGraph, tile: f64) -> Option<AbstractNode> {
    let cluster_size = graph.cluster_size();
    let x = map.x(tile);
    let y = map.y(tile);
    let cluster_x = (x / cluster_size).floor();
    let cluster_y = (y / cluster_size).floor();

    let cluster = graph.get_cluster(cluster_x as i64, cluster_y as i64)?;
    if cluster.node_ids.is_empty() {
        return None;
    }

    let mut best: Option<AbstractNode> = None;
    let mut best_dist = f64::INFINITY;
    for &node_id in &cluster.node_ids {
        let node = match graph.get_node(node_id) {
            Some(n) => n,
            None => continue,
        };
        let dist = (node.x as f64 - x).abs() + (node.y as f64 - y).abs();
        if dist < best_dist {
            best_dist = dist;
            best = Some(node);
        }
    }
    best
}

/// `SourceResolver.resolveSourcesToNodes` — `Map<nodeId, sourceTile>` keeping
/// the *closest* source per node. Insertion order is preserved (a JS `Map`
/// retains first-insert order on update), so the vector is scanned by node id.
fn resolve_sources_to_nodes(
    map: &GameMap,
    graph: &AbstractGraph,
    sources: &[f64],
) -> Vec<(i64, f64)> {
    let mut entries: Vec<(i64, f64, f64)> = Vec::new(); // (nodeId, source, dist)
    for &source in sources {
        let node = match cluster_node(map, graph, source) {
            Some(n) => n,
            None => continue,
        };
        let x = map.x(source);
        let y = map.y(source);
        let dist = (node.x as f64 - x).abs() + (node.y as f64 - y).abs();
        if let Some(slot) = entries.iter_mut().find(|(id, _, _)| *id == node.id) {
            if dist < slot.2 {
                slot.1 = source;
                slot.2 = dist;
            }
        } else {
            entries.push((node.id, source, dist));
        }
    }
    entries.into_iter().map(|(id, src, _)| (id, src)).collect()
}
