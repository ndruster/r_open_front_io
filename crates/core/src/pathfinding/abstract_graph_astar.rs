//! Port of `src/core/pathfinding/algorithms/AStar.AbstractGraph.ts`.
//!
//! `AbstractGraphAStar` runs A* over the *abstract* graph from
//! [`abstract_graph`](super::abstract_graph) instead of raw tiles, so the
//! observable quirks live in the typed-array layer and the early-return
//! ordering:
//!
//! * `gScore` is a **`Float32Array`** (unlike `a_star`/`water`, whose g-scores
//!   are `Uint32Array`s): every store rounds through `to_float32`, and the
//!   relaxation comparison `tentativeG < gScore[n]` reads the *f32-rounded*
//!   value promoted back to f64. With fractional edge costs this changes which
//!   path wins — the `ag_f32_*` scenarios pin it.
//! * `closedStamp`/`gScoreStamp` are `Uint32Array`s and `cameFrom`/`startNode`
//!   `Int32Array`s; all element access goes through the JS-semantics helpers
//!   shared with [`a_star`](super::a_star) (OOB reads `undefined`, OOB writes
//!   drop).
//! * `findPathSingle` checks the goal node and the start node *before*
//!   `queue.clear()`, so a missing node leaves the previous search's queue
//!   untouched (observable through the queue debug arrays).
//! * `findPathMultiSource` with exactly one start delegates to
//!   `findPathSingle`, i.e. it consumes a stamp and does *not* touch
//!   `startNode`; with zero starts it returns `null` before the stamp bump.
//! * `startNode` (origin tracking) is only written by the multi-source path;
//!   the single-source path leaves the array stale across searches.
//! * `buildPathFromGoal` guards both a chain value outside `[0, len)` and a
//!   path longer than `cameFrom.length`, returning `null` — reachable only
//!   through a poisoned `cameFrom` (an OOB edge endpoint relaxing into an
//!   out-of-range node id), which the `ag_oob_edge` scenario pins.
//! * The heap is sized `numNodes + edgeCount * 2` (worst case: one push per
//!   directed relaxation), so the normal paths never hit the resize warning.
//! * Defaults: `heuristicWeight ?? 1`, `maxIterations ?? 100_000`.
//! * `stamp` starts at 1 and is *incremented* at the top of each search, so
//!   the first search runs with stamp 2; refill past `0xffffffff` zeroes both
//!   stamp arrays.

use crate::pathfinding::abstract_graph::{AbstractEdge, AbstractGraph};
use crate::pathfinding::a_star::{get_f32, get_i32, get_u32, set_f32, set_i32, set_u32};
use crate::pathfinding::priority_queue::{MinHeap, PriorityQueue};

/// `AbstractGraphAStarConfig` folded into plain parameters.
pub struct AbstractGraphAStar {
    stamp: u64,
    closed_stamp: Vec<u32>,
    g_score_stamp: Vec<u32>,
    /// JS `Float32Array` — elements stored as f32, compared as f64.
    g_score: Vec<f32>,
    came_from: Vec<i32>,
    start_node: Vec<i32>,
    queue: MinHeap,
    heuristic_weight: f64,
    max_iterations: f64,
}

impl AbstractGraphAStar {
    /// `new AbstractGraphAStar(graph, config?)`. `num_nodes`/`edge_count` come
    /// from `graph.nodeCount` / `graph.edgeCount` and size the five arrays and
    /// the heap (`numNodes + edgeCount * 2`). `None` config fields take the TS
    /// `??` defaults.
    pub fn new(
        num_nodes: f64,
        edge_count: f64,
        heuristic_weight: Option<f64>,
        max_iterations: Option<f64>,
    ) -> Self {
        // `new Uint32Array(numNodes)` — ToIndex truncation; callers pass real
        // node counts.
        let cap = num_nodes.max(0.0) as usize;
        Self {
            stamp: 1,
            closed_stamp: vec![0; cap],
            g_score_stamp: vec![0; cap],
            g_score: vec![0.0f32; cap],
            came_from: vec![0; cap],
            start_node: vec![0; cap],
            queue: MinHeap::new(num_nodes + edge_count * 2.0),
            heuristic_weight: heuristic_weight.unwrap_or(1.0),
            max_iterations: max_iterations.unwrap_or(100_000.0),
        }
    }

    /// `findPathSingle(startId, goalId)` — reached from `findPath` when the
    /// start is a scalar (the `Array.isArray` dispatch is the caller's).
    pub fn find_path_single(
        &mut self,
        graph: &AbstractGraph,
        start_id: f64,
        goal_id: f64,
    ) -> Option<Vec<f64>> {
        self.bump_stamp();
        let stamp = self.stamp as f64;

        let weight = self.heuristic_weight;

        // Get goal node for heuristic — checked BEFORE queue.clear().
        let goal_node = graph.get_node(to_node_id(goal_id));
        let goal_node = goal_node?;
        let goal_x = goal_node.x as f64;
        let goal_y = goal_node.y as f64;

        // Get start node for initial heuristic.
        let start_node = graph.get_node(to_node_id(start_id));
        let start_node = start_node?;

        // Initialize — the array index is the raw `startId` (JS canonical
        // numeric property), so a fractional id simply drops the writes.
        self.queue.clear();
        set_f32(&mut self.g_score, start_id, 0.0);
        set_u32(&mut self.g_score_stamp, start_id, stamp);
        set_i32(&mut self.came_from, start_id, -1.0);

        let start_h =
            weight * ((start_node.x as f64 - goal_x).abs() + (start_node.y as f64 - goal_y).abs());
        self.queue.push(start_id, start_h);

        let mut iterations = self.max_iterations;

        while !self.queue.is_empty() {
            iterations -= 1.0;
            if iterations <= 0.0 {
                return None;
            }

            let current = self.queue.pop().unwrap_or(f64::NAN);

            if get_u32(&self.closed_stamp, current) == Some(stamp) {
                continue;
            }
            set_u32(&mut self.closed_stamp, current, stamp);

            if current == goal_id {
                return self.build_path_from_goal(goal_id);
            }

            let current_g = get_f32(&self.g_score, current).unwrap_or(f64::NAN);
            let edges = graph.get_node_edges(to_node_id(current));

            // Inline neighbour iteration
            for edge in &edges {
                let neighbor = get_other_node(edge, current);

                if get_u32(&self.closed_stamp, neighbor) == Some(stamp) {
                    continue;
                }

                let tentative_g = current_g + edge.cost;

                if get_u32(&self.g_score_stamp, neighbor) != Some(stamp)
                    || tentative_g < get_f32(&self.g_score, neighbor).unwrap_or(f64::NAN)
                {
                    set_i32(&mut self.came_from, neighbor, current);
                    set_f32(&mut self.g_score, neighbor, tentative_g);
                    set_u32(&mut self.g_score_stamp, neighbor, stamp);

                    // Inline heuristic calculation
                    let neighbor_node = graph.get_node(to_node_id(neighbor));
                    if let Some(nn) = neighbor_node {
                        let h = weight
                            * ((nn.x as f64 - goal_x).abs() + (nn.y as f64 - goal_y).abs());
                        self.queue.push(neighbor, tentative_g + h);
                    }
                }
            }
        }

        None
    }

    /// `findPath(start, goal)` for an *array* start (`findPathMultiSource`).
    /// A single-element array delegates to `findPathSingle` (stamp consumed,
    /// `startNode` untouched); an empty array returns `null` before the stamp
    /// bump.
    pub fn find_path_multi(
        &mut self,
        graph: &AbstractGraph,
        start_ids: &[f64],
        goal_id: f64,
    ) -> Option<Vec<f64>> {
        if start_ids.is_empty() {
            return None;
        }
        if start_ids.len() == 1 {
            return self.find_path_single(graph, start_ids[0], goal_id);
        }

        self.bump_stamp();
        let stamp = self.stamp as f64;

        let weight = self.heuristic_weight;

        // Get goal node for heuristic — checked BEFORE queue.clear().
        let goal_node = graph.get_node(to_node_id(goal_id));
        let goal_node = goal_node?;
        let goal_x = goal_node.x as f64;
        let goal_y = goal_node.y as f64;

        // Initialize all start nodes
        self.queue.clear();
        for &sid in start_ids {
            let node = graph.get_node(to_node_id(sid));
            let Some(node) = node else { continue };

            set_f32(&mut self.g_score, sid, 0.0);
            set_u32(&mut self.g_score_stamp, sid, stamp);
            set_i32(&mut self.came_from, sid, -1.0);
            set_i32(&mut self.start_node, sid, sid); // each start is its own origin

            let h = weight * ((node.x as f64 - goal_x).abs() + (node.y as f64 - goal_y).abs());
            self.queue.push(sid, h);
        }

        let mut iterations = self.max_iterations;

        while !self.queue.is_empty() {
            iterations -= 1.0;
            if iterations <= 0.0 {
                return None;
            }

            let current = self.queue.pop().unwrap_or(f64::NAN);

            if get_u32(&self.closed_stamp, current) == Some(stamp) {
                continue;
            }
            set_u32(&mut self.closed_stamp, current, stamp);

            if current == goal_id {
                return self.build_path_from_goal(goal_id);
            }

            let current_g = get_f32(&self.g_score, current).unwrap_or(f64::NAN);
            let current_start = get_i32(&self.start_node, current).unwrap_or(f64::NAN);
            let edges = graph.get_node_edges(to_node_id(current));

            for edge in &edges {
                let neighbor = get_other_node(edge, current);

                if get_u32(&self.closed_stamp, neighbor) == Some(stamp) {
                    continue;
                }

                let tentative_g = current_g + edge.cost;

                if get_u32(&self.g_score_stamp, neighbor) != Some(stamp)
                    || tentative_g < get_f32(&self.g_score, neighbor).unwrap_or(f64::NAN)
                {
                    set_i32(&mut self.came_from, neighbor, current);
                    set_f32(&mut self.g_score, neighbor, tentative_g);
                    set_u32(&mut self.g_score_stamp, neighbor, stamp);
                    set_i32(&mut self.start_node, neighbor, current_start); // propagate origin

                    let neighbor_node = graph.get_node(to_node_id(neighbor));
                    if let Some(nn) = neighbor_node {
                        let h = weight
                            * ((nn.x as f64 - goal_x).abs() + (nn.y as f64 - goal_y).abs());
                        self.queue.push(neighbor, tentative_g + h);
                    }
                }
            }
        }

        None
    }

    /// `buildPathFromGoal(goalId)`: walk `cameFrom` back to the `-1` sentinel.
    /// A chain value outside `[0, maxLen)` or a path longer than `maxLen`
    /// yields `null` (the TS guards).
    fn build_path_from_goal(&self, goal_id: f64) -> Option<Vec<f64>> {
        let mut path: Vec<f64> = Vec::new();
        let mut current = goal_id;
        let max_len = self.came_from.len() as f64;

        while current != -1.0 {
            if current < 0.0 || current >= max_len {
                return None;
            }
            path.push(current);
            if (path.len() as f64) > max_len {
                return None;
            }
            current = get_i32(&self.came_from, current).unwrap_or(f64::NAN);
        }

        path.reverse();
        Some(path)
    }

    fn bump_stamp(&mut self) {
        self.stamp += 1;
        if self.stamp > 0xffff_ffff {
            self.closed_stamp.fill(0);
            self.g_score_stamp.fill(0);
            self.stamp = 1;
        }
    }

    // ---- debug views for parity traces ----

    pub fn debug_stamp(&self) -> u64 {
        self.stamp
    }
    pub fn debug_closed_stamp(&self) -> &[u32] {
        &self.closed_stamp
    }
    pub fn debug_g_score_stamp(&self) -> &[u32] {
        &self.g_score_stamp
    }
    /// `gScore` elements as raw f32 bits (JS `Float32Array` storage).
    pub fn debug_g_score_bits(&self) -> Vec<u32> {
        self.g_score.iter().map(|v| v.to_bits()).collect()
    }
    pub fn debug_came_from(&self) -> &[i32] {
        &self.came_from
    }
    pub fn debug_start_node(&self) -> &[i32] {
        &self.start_node
    }
    pub fn debug_queue(&self) -> (Vec<i32>, Vec<u32>, i64, usize) {
        let (heap, pri, size) = self.queue.debug_arrays();
        (heap, pri, size, self.queue.debug_capacity())
    }
}

/// Node ids are `number`s in TS; array indexing uses the canonical numeric
/// property, so a fractional/NaN start or goal simply misses (`getNode`
/// returns `undefined`). Integer ids are the only ones the graph ever holds.
#[inline]
fn to_node_id(id: f64) -> i64 {
    if id.fract() != 0.0 || !id.is_finite() {
        // Any non-integer index is a miss; -1 keeps `get_node` in its
        // negative-range early return (JS `arr[NaN]` is also undefined).
        -1
    } else {
        id as i64
    }
}

/// `graph.getOtherNode(edge, nodeId)` — `edge.nodeA === nodeId ? nodeB :
/// nodeA`, with JS `===` on numbers (NaN is never equal, so a NaN current
/// yields `nodeA`).
#[inline]
fn get_other_node(edge: &AbstractEdge, node_id: f64) -> f64 {
    if edge.node_a as f64 == node_id {
        edge.node_b as f64
    } else {
        edge.node_a as f64
    }
}
