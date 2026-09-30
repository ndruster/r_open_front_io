//! Port of `src/core/pathfinding/algorithms/AbstractGraph.ts` — both the
//! `AbstractGraph` container and the `AbstractGraphBuilder` that constructs it.
//!
//! The graph is a coarse navigation structure: cluster-boundary *gateway*
//! nodes (mid-points of contiguous water "entrance" spans along a cluster
//! edge) joined by intra-cluster edges whose cost is a bounded grid-BFS
//! distance. The builder is a transcription, not a reimplementation — every
//! ordering decision is observable through the constructed graph:
//!
//! * `processCluster` scans clusters in row-major order and, for each, the
//!   right (vertical) edge before the bottom (horizontal) edge; the span
//!   mid-point uses `Math.floor(spanLength / 2)`.
//! * `getOrCreateNode` dedupes gateways by tile (`tileToNode`), so a corner
//!   tile found by two edge scans becomes one node; the node's `componentId`
//!   is read from the water `ConnectedComponents` (whose `find` path-compresses
//!   on every query, so query order is observable).
//! * `buildClusterConnections` runs one BFS per source node over the targets
//!   that share its component, in the order the BFS *finds* them
//!   (`reachable` is an insertion-ordered `Map`); edge ids are assigned in
//!   that order, so the BFS visit order feeds directly into `nextEdgeId`.
//! * `addOrUpdateEdge` canonicalises `(lo, hi)` and keeps the cheaper cost,
//!   rewriting `clusterX/clusterY` only on a strict improvement.
//! * `computePartialRebuildInfo` / `buildClusterConnectionsFromCache` recreate
//!   clean-cluster edges from the old graph keyed by `(minTile, maxTile)`,
//!   preserving the ORIGINAL cluster attribution.
//!
//! `DebugSpan` calls are no-ops in the capture (the flag is off), so they are
//! omitted here — they never touch the graph. The `sharedWaterComponents` /
//! `sharedTileBFS` optimisation params have no observable effect on the built
//! graph given identical inputs (the shared CC is already initialised, the
//! shared BFS scratch is stateless between searches), so only the default
//! (fresh) path is modelled.

use std::collections::{HashMap, HashSet};

use crate::game_map::GameMap;
use crate::pathfinding::bfs_grid::{BfsGrid, Visit};
use crate::pathfinding::connected_components::ConnectedComponents;

/// `AbstractNode` — a gateway at a cluster boundary.
#[derive(Clone, Debug, PartialEq)]
pub struct AbstractNode {
    pub id: i64,
    pub x: i64,
    pub y: i64,
    pub tile: f64,
    pub component_id: i64,
}

/// `AbstractEdge` — a bidirectional edge stored once, canonical `nodeA < nodeB`.
#[derive(Clone, Debug, PartialEq)]
pub struct AbstractEdge {
    pub id: i64,
    pub node_a: i64,
    pub node_b: i64,
    pub cost: f64,
    pub cluster_x: i64,
    pub cluster_y: i64,
}

/// `Cluster` — a coarse spatial bucket with its member node ids.
#[derive(Clone, Debug, PartialEq)]
pub struct Cluster {
    pub x: i64,
    pub y: i64,
    pub node_ids: Vec<i64>,
}

/// The `AbstractGraph` container. Internal arrays mirror the TS sparse
/// arrays (`_nodes[id]`, `_edges[id]`, `_clusters[key]`) with `Option` slots so
/// an out-of-range read yields `None` exactly where JS yields `undefined`.
#[derive(Clone, Debug)]
pub struct AbstractGraph {
    cluster_size: i64,
    clusters_x: i64,
    clusters_y: i64,
    nodes: Vec<Option<AbstractNode>>,
    edges: Vec<Option<AbstractEdge>>,
    node_edge_ids: Vec<Vec<i64>>,
    clusters: Vec<Option<Cluster>>,
    path_cache: Vec<Option<Vec<f64>>>,
    water_components: Option<ConnectedComponents>,
}

impl AbstractGraph {
    pub fn new(cluster_size: i64, clusters_x: i64, clusters_y: i64) -> Self {
        Self {
            cluster_size,
            clusters_x,
            clusters_y,
            nodes: Vec::new(),
            edges: Vec::new(),
            node_edge_ids: Vec::new(),
            clusters: Vec::new(),
            path_cache: Vec::new(),
            water_components: None,
        }
    }

    // ---- node / edge accessors ----

    pub fn get_node(&self, id: i64) -> Option<AbstractNode> {
        if id < 0 || (id as usize) >= self.nodes.len() {
            return None;
        }
        self.nodes[id as usize].clone()
    }

    pub fn get_all_nodes(&self) -> Vec<AbstractNode> {
        self.nodes.iter().filter_map(|n| n.clone()).collect()
    }

    pub fn node_count(&self) -> f64 {
        self.nodes.len() as f64
    }

    pub fn get_edge(&self, id: i64) -> Option<AbstractEdge> {
        if id < 0 || (id as usize) >= self.edges.len() {
            return None;
        }
        self.edges[id as usize].clone()
    }

    /// `getNodeEdges` — resolve `nodeEdgeIds[nodeId]`, skipping missing edges
    /// (the TS `if (e)` guard).
    pub fn get_node_edges(&self, node_id: i64) -> Vec<AbstractEdge> {
        let mut out = Vec::new();
        if node_id < 0 || (node_id as usize) >= self.node_edge_ids.len() {
            return out;
        }
        for &eid in &self.node_edge_ids[node_id as usize] {
            if eid >= 0 && (eid as usize) < self.edges.len() {
                if let Some(e) = &self.edges[eid as usize] {
                    out.push(e.clone());
                }
            }
        }
        out
    }

    /// `getEdgeBetween` — the first edge in `nodeEdgeIds[nodeA]` touching
    /// `nodeB`. The TS loop reads `edge.nodeA`/`edge.nodeB` without a guard,
    /// so a stale edge id would throw; every stored id is valid, so a missing
    /// slot is simply skipped.
    pub fn get_edge_between(&self, node_a: i64, node_b: i64) -> Option<AbstractEdge> {
        if node_a < 0 || (node_a as usize) >= self.node_edge_ids.len() {
            return None;
        }
        for &eid in &self.node_edge_ids[node_a as usize] {
            if eid < 0 || (eid as usize) >= self.edges.len() {
                continue;
            }
            if let Some(edge) = &self.edges[eid as usize] {
                if edge.node_a == node_b || edge.node_b == node_b {
                    return Some(edge.clone());
                }
            }
        }
        None
    }

    /// `getOtherNode`.
    pub fn get_other_node(edge: &AbstractEdge, node_id: i64) -> i64 {
        if edge.node_a == node_id {
            edge.node_b
        } else {
            edge.node_a
        }
    }

    pub fn get_all_edges(&self) -> Vec<AbstractEdge> {
        self.edges.iter().filter_map(|e| e.clone()).collect()
    }

    pub fn edge_count(&self) -> f64 {
        self.edges.len() as f64
    }

    // ---- path cache ----

    /// `getCachedPath` — direction 0 when `fromNodeId` is `nodeA`, else 1;
    /// `?? null` collapses an out-of-range / null slot to `null` (`None`).
    pub fn get_cached_path(&self, edge_id: i64, from_node_id: i64) -> Option<Vec<f64>> {
        let edge = self.get_edge(edge_id)?;
        let direction = if from_node_id == edge.node_a { 0 } else { 1 };
        let cache_index = edge_id * 2 + direction;
        if cache_index < 0 || (cache_index as usize) >= self.path_cache.len() {
            return None;
        }
        self.path_cache[cache_index as usize].clone()
    }

    /// `setCachedPath`.
    pub fn set_cached_path(&mut self, edge_id: i64, from_node_id: i64, path: Vec<f64>) {
        let edge = match self.get_edge(edge_id) {
            Some(e) => e,
            None => return,
        };
        let direction = if from_node_id == edge.node_a { 0 } else { 1 };
        let cache_index = (edge_id * 2 + direction) as usize;
        while self.path_cache.len() <= cache_index {
            self.path_cache.push(None);
        }
        self.path_cache[cache_index] = Some(path);
    }

    /// `_initPathCache` — `new Array(edges.length * 2).fill(null)`.
    pub fn init_path_cache(&mut self) {
        self.path_cache = vec![None; self.edges.len() * 2];
    }

    // ---- water components ----

    pub fn set_water_components(&mut self, wc: ConnectedComponents) {
        self.water_components = Some(wc);
    }

    /// `getComponentId` — `this._waterComponents?.getComponentId(tile) ?? 0`.
    pub fn get_component_id(&mut self, tile: f64) -> i64 {
        match &mut self.water_components {
            Some(cc) => cc.get_component_id(tile),
            None => 0,
        }
    }

    /// `getComponentSize` — `this._waterComponents?.getComponentSize(id) ?? 0`.
    pub fn get_component_size(&mut self, component_id: f64) -> f64 {
        match &mut self.water_components {
            Some(cc) => cc.get_component_size(component_id),
            None => 0.0,
        }
    }

    // ---- clusters ----

    pub fn get_cluster_key(&self, cluster_x: i64, cluster_y: i64) -> i64 {
        cluster_y * self.clusters_x + cluster_x
    }

    pub fn get_cluster(&self, cluster_x: i64, cluster_y: i64) -> Option<Cluster> {
        let key = self.get_cluster_key(cluster_x, cluster_y);
        if key < 0 || (key as usize) >= self.clusters.len() {
            return None;
        }
        self.clusters[key as usize].clone()
    }

    /// `getClusterNodes` — resolve each member id through `_nodes` (a dangling
    /// id would be `undefined`; ids are always valid here).
    pub fn get_cluster_nodes(&self, cluster_x: i64, cluster_y: i64) -> Vec<AbstractNode> {
        let cluster = match self.get_cluster(cluster_x, cluster_y) {
            Some(c) => c,
            None => return Vec::new(),
        };
        cluster.node_ids.iter().filter_map(|&id| self.get_node(id)).collect()
    }

    /// `getNearbyClusterNodes` — the 3x3 ring, in `dy`-then-`dx` order; a
    /// corner node shared by several clusters appears once per cluster (JS
    /// pushes duplicates).
    pub fn get_nearby_cluster_nodes(&self, cluster_x: i64, cluster_y: i64) -> Vec<AbstractNode> {
        let mut nodes = Vec::new();
        for dy in -1..=1 {
            for dx in -1..=1 {
                if let Some(cluster) = self.get_cluster(cluster_x + dx, cluster_y + dy) {
                    for &node_id in &cluster.node_ids {
                        if let Some(n) = self.get_node(node_id) {
                            nodes.push(n);
                        }
                    }
                }
            }
        }
        nodes
    }

    // ---- mutation (underscore-prefixed in TS) ----

    pub fn add_node(&mut self, node: AbstractNode) {
        let id = node.id as usize;
        while self.nodes.len() <= id {
            self.nodes.push(None);
        }
        self.nodes[id] = Some(node);
        while self.node_edge_ids.len() <= id {
            self.node_edge_ids.push(Vec::new());
        }
        self.node_edge_ids[id] = Vec::new();
    }

    pub fn add_edge(&mut self, edge: AbstractEdge) {
        let id = edge.id as usize;
        while self.edges.len() <= id {
            self.edges.push(None);
        }
        self.edges[id] = Some(edge.clone());
        for &n in &[edge.node_a, edge.node_b] {
            let ni = n as usize;
            while self.node_edge_ids.len() <= ni {
                self.node_edge_ids.push(Vec::new());
            }
            self.node_edge_ids[ni].push(edge.id);
        }
    }

    pub fn set_cluster(&mut self, key: i64, cluster: Cluster) {
        let ki = key as usize;
        while self.clusters.len() <= ki {
            self.clusters.push(None);
        }
        self.clusters[ki] = Some(cluster);
    }

    /// `_addNodeToCluster` — no-op when the cluster is absent (matches the TS
    /// early return).
    pub fn add_node_to_cluster(&mut self, cluster_key: i64, node_id: i64) {
        if cluster_key < 0 || (cluster_key as usize) >= self.clusters.len() {
            return;
        }
        if let Some(cluster) = &mut self.clusters[cluster_key as usize] {
            cluster.node_ids.push(node_id);
        }
    }

    // ---- debug accessors (parity harness) ----

    pub fn debug_cluster_size(&self) -> f64 {
        self.cluster_size as f64
    }
    pub fn debug_clusters_x(&self) -> f64 {
        self.clusters_x as f64
    }
    pub fn debug_clusters_y(&self) -> f64 {
        self.clusters_y as f64
    }
    pub fn debug_path_cache_len(&self) -> f64 {
        self.path_cache.len() as f64
    }

    /// Nodes flattened 5-per-entry `[id, x, y, tile, componentId]`.
    pub fn debug_nodes(&self) -> Vec<f64> {
        let mut out = Vec::new();
        for n in self.nodes.iter().flatten() {
            out.extend([
                n.id as f64,
                n.x as f64,
                n.y as f64,
                n.tile,
                n.component_id as f64,
            ]);
        }
        out
    }

    /// Edges flattened 7-per-entry `[id, nodeA, nodeB, cost, clusterX, clusterY]`.
    pub fn debug_edges(&self) -> Vec<f64> {
        let mut out = Vec::new();
        for e in self.edges.iter().flatten() {
            out.extend([
                e.id as f64,
                e.node_a as f64,
                e.node_b as f64,
                e.cost,
                e.cluster_x as f64,
                e.cluster_y as f64,
            ]);
        }
        out
    }

    /// Clusters flattened `[x, y, count, id...]` in key order (every key is
    /// pre-created, so no holes).
    pub fn debug_clusters(&self) -> Vec<f64> {
        let mut out = Vec::new();
        for c in self.clusters.iter().flatten() {
            out.push(c.x as f64);
            out.push(c.y as f64);
            out.push(c.node_ids.len() as f64);
            for &id in &c.node_ids {
                out.push(id as f64);
            }
        }
        out
    }

    /// Per-node edge-id lists flattened `[count, id...]` in node-id order.
    pub fn debug_node_edge_ids(&self) -> Vec<f64> {
        let mut out = Vec::new();
        for list in &self.node_edge_ids {
            out.push(list.len() as f64);
            for &id in list {
                out.push(id as f64);
            }
        }
        out
    }
}

/// An old-graph edge cost entry (`oldEdgeCosts` inner value).
#[derive(Clone, Copy, Debug)]
struct OldEdgeEntry {
    cost: f64,
    cluster_x: i64,
    cluster_y: i64,
}

/// `AbstractGraphBuilder`. Owns the map, the reusable BFS scratch, the water
/// components (transferred into the graph at the end of `build`), and the
/// partial-rebuild lookups.
pub struct AbstractGraphBuilder {
    map: GameMap,
    cluster_size: i64,
    clusters_x: i64,
    clusters_y: i64,
    width: i64,
    height: i64,
    tile_bfs: BfsGrid,
    water_components: Option<ConnectedComponents>,

    graph: AbstractGraph,
    tile_to_node: HashMap<i64, AbstractNode>,
    next_node_id: i64,
    next_edge_id: i64,
    edge_between: HashMap<(i64, i64), i64>,

    old_graph: Option<AbstractGraph>,
    dirty_mini_tiles: Option<Vec<f64>>,
    shared_water_components: bool,
    clean_clusters: Option<HashSet<i64>>,
    old_edge_costs: Option<HashMap<(i64, i64), OldEdgeEntry>>,
}

impl AbstractGraphBuilder {
    /// `CLUSTER_SIZE`.
    pub const CLUSTER_SIZE: i64 = 32;

    /// Fresh build (no partial-rebuild inputs).
    pub fn new(map: GameMap, cluster_size: i64) -> Self {
        Self::with_rebuild(map, cluster_size, None, None)
    }

    /// Partial rebuild: `oldGraph` + `dirtyMiniTiles` drive the clean-cluster
    /// cache path. `sharedWaterComponents` mirrors the TS optimisation flag
    /// (when true, `build` skips `initialize()`).
    pub fn with_rebuild(
        map: GameMap,
        cluster_size: i64,
        old_graph: Option<AbstractGraph>,
        dirty_mini_tiles: Option<Vec<f64>>,
    ) -> Self {
        let width = map.width() as i64;
        let height = map.height() as i64;
        let clusters_x = ceil_div(width, cluster_size);
        let clusters_y = ceil_div(height, cluster_size);
        let terrain = map.debug_terrain().to_vec();
        let water_components = ConnectedComponents::new(width, height, terrain, true);
        let graph = AbstractGraph::new(cluster_size, clusters_x, clusters_y);
        Self {
            map,
            cluster_size,
            clusters_x,
            clusters_y,
            width,
            height,
            tile_bfs: BfsGrid::new((width * height) as f64),
            water_components: Some(water_components),
            graph,
            tile_to_node: HashMap::new(),
            next_node_id: 0,
            next_edge_id: 0,
            edge_between: HashMap::new(),
            old_graph,
            dirty_mini_tiles,
            shared_water_components: false,
            clean_clusters: None,
            old_edge_costs: None,
        }
    }

    /// `build()` — returns the constructed graph (water components moved in).
    pub fn build(&mut self) -> AbstractGraph {
        self.graph = AbstractGraph::new(self.cluster_size, self.clusters_x, self.clusters_y);

        // Initialize water components (shared ones are maintained by the caller).
        if !self.shared_water_components {
            if let Some(cc) = &mut self.water_components {
                cc.initialize();
            }
        }

        // Compute partial rebuild info (which clusters can skip BFS).
        let has_dirty = self
            .dirty_mini_tiles
            .as_ref()
            .is_some_and(|d| !d.is_empty());
        if self.old_graph.is_some() && has_dirty {
            self.compute_partial_rebuild_info();
        }

        // Pre-create all clusters.
        for cy in 0..self.clusters_y {
            for cx in 0..self.clusters_x {
                let key = self.graph.get_cluster_key(cx, cy);
                self.graph.set_cluster(key, Cluster { x: cx, y: cy, node_ids: Vec::new() });
            }
        }

        // Find nodes (gateways) at cluster boundaries.
        for cy in 0..self.clusters_y {
            for cx in 0..self.clusters_x {
                self.process_cluster(cx, cy);
            }
        }

        // Build edges between nodes in the same cluster.
        for cy in 0..self.clusters_y {
            for cx in 0..self.clusters_x {
                let empty = match self.graph.get_cluster(cx, cy) {
                    Some(c) => c.node_ids.is_empty(),
                    None => true,
                };
                if empty {
                    continue;
                }
                self.build_cluster_connections(cx, cy);
            }
        }

        // Initialize path cache after all edges are built.
        self.graph.init_path_cache();

        // Store water components for componentId lookups.
        let wc = self.water_components.take().expect("water components present");
        self.graph.set_water_components(wc);

        std::mem::replace(&mut self.graph, AbstractGraph::new(0, 0, 0))
    }

    // ---- node discovery ----

    fn get_or_create_node(&mut self, x: i64, y: i64) -> AbstractNode {
        let tile = self.map.tile_ref(x as f64, y as f64);
        let key = tile as i64;
        if let Some(existing) = self.tile_to_node.get(&key) {
            return existing.clone();
        }

        let component_id = match &mut self.water_components {
            Some(cc) => cc.get_component_id(tile),
            None => 0,
        };
        let node = AbstractNode {
            id: self.next_node_id,
            x,
            y,
            tile,
            component_id,
        };
        self.next_node_id += 1;
        self.graph.add_node(node.clone());
        self.tile_to_node.insert(key, node.clone());
        node
    }

    fn add_node_to_cluster(&mut self, cluster_x: i64, cluster_y: i64, node: &AbstractNode) {
        let cluster = match self.graph.get_cluster(cluster_x, cluster_y) {
            Some(c) => c,
            None => return,
        };
        if !cluster.node_ids.contains(&node.id) {
            self.graph.add_node_to_cluster(
                self.graph.get_cluster_key(cluster_x, cluster_y),
                node.id,
            );
        }
    }

    fn process_cluster(&mut self, cx: i64, cy: i64) {
        let base_x = cx * self.cluster_size;
        let base_y = cy * self.cluster_size;

        // Right edge (vertical boundary to the next cluster).
        if cx < self.clusters_x - 1 {
            let edge_x = (base_x + self.cluster_size - 1).min(self.width - 1);
            let nodes = self.find_nodes_on_vertical_edge(edge_x, base_y);
            for node in &nodes {
                self.add_node_to_cluster(cx, cy, node);
                self.add_node_to_cluster(cx + 1, cy, node);
            }
        }

        // Bottom edge (horizontal boundary to the next cluster).
        if cy < self.clusters_y - 1 {
            let edge_y = (base_y + self.cluster_size - 1).min(self.height - 1);
            let nodes = self.find_nodes_on_horizontal_edge(edge_y, base_x);
            for node in &nodes {
                self.add_node_to_cluster(cx, cy, node);
                self.add_node_to_cluster(cx, cy + 1, node);
            }
        }
    }

    fn find_nodes_on_vertical_edge(&mut self, x: i64, base_y: i64) -> Vec<AbstractNode> {
        let mut nodes = Vec::new();
        let max_y = (base_y + self.cluster_size).min(self.height);
        let mut span_start = -1i64;

        let mut y = base_y;
        while y < max_y {
            let tile = self.map.tile_ref(x as f64, y as f64);
            let next_tile = if x + 1 < self.width {
                self.map.tile_ref((x + 1) as f64, y as f64)
            } else {
                -1.0
            };
            let is_entrance =
                self.map.is_water(tile) && next_tile != -1.0 && self.map.is_water(next_tile);

            if is_entrance {
                if span_start == -1 {
                    span_start = y;
                }
            } else if span_start != -1 {
                let span_length = y - span_start;
                let mid_y = span_start + span_length.div_euclid(2);
                span_start = -1;
                let node = self.get_or_create_node(x, mid_y);
                nodes.push(node);
            }
            y += 1;
        }

        if span_start != -1 {
            let span_length = max_y - span_start;
            let mid_y = span_start + span_length.div_euclid(2);
            let node = self.get_or_create_node(x, mid_y);
            nodes.push(node);
        }
        nodes
    }

    fn find_nodes_on_horizontal_edge(&mut self, y: i64, base_x: i64) -> Vec<AbstractNode> {
        let mut nodes = Vec::new();
        let max_x = (base_x + self.cluster_size).min(self.width);
        let mut span_start = -1i64;

        let mut x = base_x;
        while x < max_x {
            let tile = self.map.tile_ref(x as f64, y as f64);
            let next_tile = if y + 1 < self.height {
                self.map.tile_ref(x as f64, (y + 1) as f64)
            } else {
                -1.0
            };
            let is_entrance =
                self.map.is_water(tile) && next_tile != -1.0 && self.map.is_water(next_tile);

            if is_entrance {
                if span_start == -1 {
                    span_start = x;
                }
            } else if span_start != -1 {
                let span_length = x - span_start;
                let mid_x = span_start + span_length.div_euclid(2);
                span_start = -1;
                let node = self.get_or_create_node(mid_x, y);
                nodes.push(node);
            }
            x += 1;
        }

        if span_start != -1 {
            let span_length = max_x - span_start;
            let mid_x = span_start + span_length.div_euclid(2);
            let node = self.get_or_create_node(mid_x, y);
            nodes.push(node);
        }
        nodes
    }

    // ---- edge construction ----

    fn build_cluster_connections(&mut self, cx: i64, cy: i64) {
        let cluster_key = cy * self.clusters_x + cx;

        // Clean clusters copy edge costs from the old graph instead of BFS.
        if self.clean_clusters.as_ref().is_some_and(|s| s.contains(&cluster_key)) {
            self.build_cluster_connections_from_cache(cx, cy);
            return;
        }

        let cluster = match self.graph.get_cluster(cx, cy) {
            Some(c) => c,
            None => return,
        };
        let nodes: Vec<AbstractNode> = cluster
            .node_ids
            .iter()
            .filter_map(|&id| self.graph.get_node(id))
            .collect();

        let cluster_min_x = cx * self.cluster_size;
        let cluster_min_y = cy * self.cluster_size;
        let cluster_max_x = (self.width - 1).min(cluster_min_x + self.cluster_size - 1);
        let cluster_max_y = (self.height - 1).min(cluster_min_y + self.cluster_size - 1);

        for i in 0..nodes.len() {
            let from_node = nodes[i].clone();

            let target_nodes: Vec<AbstractNode> = (i + 1..nodes.len())
                .filter(|&j| nodes[i].component_id == nodes[j].component_id)
                .map(|j| nodes[j].clone())
                .collect();
            if target_nodes.is_empty() {
                continue;
            }

            let reachable = find_all_reachable_nodes_in_bounds(
                &mut self.tile_bfs,
                &self.map,
                from_node.tile,
                &target_nodes,
                cluster_min_x,
                cluster_max_x,
                cluster_min_y,
                cluster_max_y,
            );

            for (target_id, cost) in reachable {
                self.add_or_update_edge(from_node.id, target_id, cost, cx, cy);
            }
        }
    }

    fn add_or_update_edge(
        &mut self,
        node_id_a: i64,
        node_id_b: i64,
        cost: f64,
        cluster_x: i64,
        cluster_y: i64,
    ) {
        let (lo, hi) = if node_id_a < node_id_b {
            (node_id_a, node_id_b)
        } else {
            (node_id_b, node_id_a)
        };

        if let Some(&existing_id) = self.edge_between.get(&(lo, hi)) {
            if let Some(existing) = self.graph.edges_mut().get_mut(existing_id as usize) {
                if let Some(existing) = existing.as_mut() {
                    if cost < existing.cost {
                        existing.cost = cost;
                        existing.cluster_x = cluster_x;
                        existing.cluster_y = cluster_y;
                    }
                }
            }
            return;
        }

        let edge = AbstractEdge {
            id: self.next_edge_id,
            node_a: lo,
            node_b: hi,
            cost,
            cluster_x,
            cluster_y,
        };
        self.next_edge_id += 1;
        self.edge_between.insert((lo, hi), edge.id);
        self.graph.add_edge(edge);
    }

    fn build_cluster_connections_from_cache(&mut self, cx: i64, cy: i64) {
        let cluster = match self.graph.get_cluster(cx, cy) {
            Some(c) => c,
            None => return,
        };
        let nodes: Vec<AbstractNode> = cluster
            .node_ids
            .iter()
            .filter_map(|&id| self.graph.get_node(id))
            .collect();
        let old_edge_costs = self.old_edge_costs.as_ref().expect("old edge costs present");

        // Collect first (the lookup immutably borrows self), then update.
        let mut entries = Vec::new();
        for i in 0..nodes.len() {
            for j in i + 1..nodes.len() {
                if nodes[i].component_id != nodes[j].component_id {
                    continue;
                }
                let (tile_min, tile_max) = if nodes[i].tile < nodes[j].tile {
                    (nodes[i].tile, nodes[j].tile)
                } else {
                    (nodes[j].tile, nodes[i].tile)
                };
                if let Some(entry) = old_edge_costs.get(&(tile_min as i64, tile_max as i64)) {
                    entries.push((nodes[i].id, nodes[j].id, *entry));
                }
            }
        }
        for (a, b, entry) in entries {
            // Preserve the ORIGINAL (clusterX, clusterY) from the old graph.
            self.add_or_update_edge(a, b, entry.cost, entry.cluster_x, entry.cluster_y);
        }
    }

    fn compute_partial_rebuild_info(&mut self) {
        let dirty_mini_tiles = self.dirty_mini_tiles.clone().unwrap_or_default();
        let old_graph = match &self.old_graph {
            Some(g) => g,
            None => return,
        };

        // Map dirty minimap tiles to their cluster indices.
        let mut primary_dirty: HashSet<i64> = HashSet::new();
        for tile in &dirty_mini_tiles {
            let x = self.map.x(*tile);
            let y = self.map.y(*tile);
            let cx = x.div_euclid(self.cluster_size as f64) as i64;
            let cy = y.div_euclid(self.cluster_size as f64) as i64;
            primary_dirty.insert(cy * self.clusters_x + cx);
        }

        // Expand by 1-ring neighbours.
        let mut expanded_dirty: HashSet<i64> = HashSet::new();
        for &key in &primary_dirty {
            let cy = key.div_euclid(self.clusters_x);
            let cx = key - cy * self.clusters_x;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let nx = cx + dx;
                    let ny = cy + dy;
                    if nx >= 0 && nx < self.clusters_x && ny >= 0 && ny < self.clusters_y {
                        expanded_dirty.insert(ny * self.clusters_x + nx);
                    }
                }
            }
        }

        // Everything not expanded-dirty is clean.
        let mut clean = HashSet::new();
        let total = self.clusters_x * self.clusters_y;
        for k in 0..total {
            if !expanded_dirty.contains(&k) {
                clean.insert(k);
            }
        }
        self.clean_clusters = Some(clean);

        // Build the old edge cost lookup: (minTile, maxTile) -> entry.
        let mut costs: HashMap<(i64, i64), OldEdgeEntry> = HashMap::new();
        for edge in old_graph.get_all_edges() {
            let node_a = match old_graph.get_node(edge.node_a) {
                Some(n) => n,
                None => continue,
            };
            let node_b = match old_graph.get_node(edge.node_b) {
                Some(n) => n,
                None => continue,
            };
            let (tile_min, tile_max) = if node_a.tile < node_b.tile {
                (node_a.tile, node_b.tile)
            } else {
                (node_b.tile, node_a.tile)
            };
            let key = (tile_min as i64, tile_max as i64);
            let dominated = costs.get(&key).is_none_or(|e| edge.cost < e.cost);
            if dominated {
                costs.insert(
                    key,
                    OldEdgeEntry {
                        cost: edge.cost,
                        cluster_x: edge.cluster_x,
                        cluster_y: edge.cluster_y,
                    },
                );
            }
        }
        self.old_edge_costs = Some(costs);
    }
}

/// `findAllReachableNodesInBounds` — one bounded grid BFS returning the
/// `(nodeId, dist)` pairs in the order the BFS *finds* them (JS `Map`
/// insertion order). A free function so `tile_bfs` (mut) and `map` (immut)
/// are disjoint borrows.
#[allow(clippy::too_many_arguments)]
fn find_all_reachable_nodes_in_bounds(
    tile_bfs: &mut BfsGrid,
    map: &GameMap,
    from: f64,
    target_nodes: &[AbstractNode],
    min_x: i64,
    max_x: i64,
    min_y: i64,
    max_y: i64,
) -> Vec<(i64, f64)> {
    let from_x = map.x(from) as i64;
    let from_y = map.y(from) as i64;

    let mut tile_to_node_id: HashMap<i64, i64> = HashMap::new();
    let mut max_manhattan_dist = 0i64;
    for node in target_nodes {
        tile_to_node_id.insert(node.tile as i64, node.id);
        let dx = (node.x - from_x).abs();
        let dy = (node.y - from_y).abs();
        max_manhattan_dist = max_manhattan_dist.max(dx + dy);
    }

    let max_distance = (max_manhattan_dist * 4) as f64;
    let mut reachable: Vec<(i64, f64)> = Vec::new();
    let mut found_count = 0usize;
    let target_len = target_nodes.len();
    let from_i = from as i64;

    tile_bfs.search::<f64, _, _>(
        map.width() as i64,
        map.height() as i64,
        &[from_i],
        max_distance,
        |tile| map.is_water(tile as f64),
        |tile, dist| {
            let x = map.x(tile as f64) as i64;
            let y = map.y(tile as f64) as i64;

            let is_start_or_target = tile == from_i || tile_to_node_id.contains_key(&tile);
            if !is_start_or_target && (x < min_x || x > max_x || y < min_y || y > max_y) {
                return Visit::Reject;
            }

            if let Some(&node_id) = tile_to_node_id.get(&tile) {
                match reachable.iter_mut().find(|(k, _)| *k == node_id) {
                    Some(slot) => slot.1 = dist as f64,
                    None => reachable.push((node_id, dist as f64)),
                }
                found_count += 1;
                if found_count == target_len {
                    return Visit::Found(dist as f64);
                }
            }
            Visit::Explore
        },
    );

    reachable
}

/// `Math.ceil(a / b)` for non-negative integers.
fn ceil_div(a: i64, b: i64) -> i64 {
    if b <= 0 {
        return 0;
    }
    (a + b - 1) / b
}

impl AbstractGraph {
    /// Mutable view of the edge table (used by the builder's cost update).
    fn edges_mut(&mut self) -> &mut Vec<Option<AbstractEdge>> {
        &mut self.edges
    }
}
