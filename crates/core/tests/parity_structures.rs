//! Replays the operation traces captured from the real TypeScript queue,
//! heap and grid-BFS classes (`data/vectors.rs` -> `structures`) against the
//! Rust ports, asserting every return value *and* the final internal state.
//!
//! This is the strongest parity form available for stateful structures: rather
//! than pinning one output, it replays a whole 100-200-op script (pushes,
//! pops, clears, isEmpty) that the TS ran and recorded, then compares the
//! resulting arrays. A divergence in tie-breaking, f32 rounding, underflow
//! handling or bucket LIFO shows up as the first mismatching pop.

#[path = "data/vectors.rs"]
mod vectors;

use vectors::Op;
use vectors::Res;

use openfront_core::pathfinding::a_star::{AStar, GridAdapter};
use openfront_core::pathfinding::bfs_grid::{BfsGrid, Visit};
use openfront_core::pathfinding::flat_heap::FlatBinaryHeap;
use openfront_core::pathfinding::priority_queue::{BucketQueue, MinHeap, PriorityQueue};
use openfront_core::pathfinding::rail::{RailAdapter, TerrainMap};

fn res_eq(want: &Res, got: Option<f64>) -> bool {
    match (want, got) {
        (Res::Void, _) => true,
        (Res::Undef, None) => true,
        (Res::Nan, Some(v)) => v.is_nan(),
        (Res::Val(w), Some(v)) => v == *w || (v.is_nan() && w.is_nan()),
        _ => false,
    }
}

fn arg(op: &Op) -> (f64, f64) {
    (op.a, op.b)
}

#[test]
fn replay_minheap_scenarios() {
    for s in vectors::MINHEAP_SCENARIOS {
        let mut h = MinHeap::new(s.cap);
        for (i, op) in s.ops.iter().enumerate() {
            match op.kind {
                0 => {
                    let (a, b) = arg(op);
                    h.push(a, b)
                }
                1 => {
                    let got = h.pop();
                    assert!(res_eq(&op.res, got), "{} op#{i} pop: got {got:?}", s.name);
                }
                2 => h.clear(),
                3 => {
                    let got = h.is_empty();
                    let want = matches!(op.res, Res::Val(v) if v == 1.0);
                    assert_eq!(got, want, "{} op#{i} isEmpty", s.name);
                }
                k => panic!("{} unexpected op kind {k}", s.name),
            }
        }
        let (heap, pri, size) = h.debug_arrays();
        assert_eq!(heap, s.heap, "{} final heap", s.name);
        assert_eq!(pri, s.pri_bits, "{} final priority bits", s.name);
        assert_eq!(size, s.size, "{} final size", s.name);
        assert_eq!(h.debug_capacity(), s.capacity, "{} final capacity", s.name);
    }
}

#[test]
fn replay_bucket_scenarios() {
    for s in vectors::BUCKET_SCENARIOS {
        let mut q = BucketQueue::new(s.max_p);
        for (i, op) in s.ops.iter().enumerate() {
            match op.kind {
                0 => {
                    let (a, b) = arg(op);
                    q.push(a, b)
                }
                1 => {
                    let got = q.pop();
                    assert!(res_eq(&op.res, got), "{} op#{i} pop: got {got:?}", s.name);
                }
                2 => q.clear(),
                3 => {
                    let got = q.is_empty();
                    let want = matches!(op.res, Res::Val(v) if v == 1.0);
                    assert_eq!(got, want, "{} op#{i} isEmpty", s.name);
                }
                k => panic!("{} unexpected op kind {k}", s.name),
            }
        }
        let (sizes, stamps, keys) = q.debug_full();
        let (min_bucket, _max, size, stamp) = q.debug_state();
        assert_eq!(sizes, s.sizes, "{} final bucketSizes", s.name);
        assert_eq!(stamps, s.stamps, "{} final bucketStamp", s.name);
        assert_eq!(keys, s.keys, "{} final bucket keys", s.name);
        assert_eq!(min_bucket, s.min_bucket, "{} final minBucket", s.name);
        assert_eq!(size, s.size, "{} final size", s.name);
        assert_eq!(stamp, s.stamp, "{} final stamp", s.name);
    }
}

#[test]
fn replay_flat_heap_scenarios() {
    for s in vectors::FLATHEAP_SCENARIOS {
        let mut h = FlatBinaryHeap::new(s.cap);
        for (i, op) in s.ops.iter().enumerate() {
            match op.kind {
                0 => {
                    let (a, b) = arg(op);
                    h.enqueue(a, b)
                }
                1 => {
                    let got = h.dequeue();
                    assert!(res_eq(&op.res, got), "{} op#{i} dequeue: got {got:?}", s.name);
                }
                2 => h.clear(),
                5 => assert_eq!(h.size() as f64, val(&op.res), "{} op#{i} size", s.name),
                4 => {
                    // TS dequeue on an empty heap throws "heap empty".
                    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let _ = h.dequeue();
                    }))
                    .is_err();
                    assert!(panicked, "{} op#{i} expected dequeue to throw", s.name);
                }
                k => panic!("{} unexpected op kind {k}", s.name),
            }
        }
        let (bits, tiles, len) = h.debug_state();
        assert_eq!(bits, s.pri_bits, "{} final priority bits", s.name);
        // TS `Array.from(inst.tiles)` keeps holes as `undefined`; JS maps those
        // to "u"/None, so tile vectors must line up exactly.
        assert_eq!(tiles.len(), s.tiles.len(), "{} tiles length", s.name);
        for (j, (got, want)) in tiles.iter().zip(s.tiles.iter()).enumerate() {
            let same = match (got, want) {
                (None, None) => true,
                (Some(a), Some(b)) => a == b,
                _ => false,
            };
            assert!(same, "{} tile[{j}] got {got:?} want {want:?}", s.name);
        }
        assert_eq!(len, s.len, "{} final len", s.name);
    }
}

fn val(r: &Res) -> f64 {
    match r {
        Res::Val(v) => *v,
        _ => panic!("expected Val"),
    }
}

#[test]
fn replay_bfs_grid_scenarios() {
    let scenarios: &[_] = vectors::BFS_SCENARIOS;
    for s in scenarios {
        let mut g = BfsGrid::new((s.w * s.h) as f64);
        let mut order = Vec::new();
        let mut dists = Vec::new();
        let valid = |n: i64| !(s.mode == 1 && n == s.blocker);
        let visitor = |n: i64, d: i64| -> Visit<i32> {
            order.push(n);
            dists.push(d);
            if s.mode == 2 && n == s.blocker {
                Visit::Reject
            } else if s.mode == 3 && n == s.blocker {
                Visit::Found(42)
            } else {
                Visit::Explore
            }
        };
        let starts = if s.s1 >= 0 {
            vec![s.s0, s.s1]
        } else {
            vec![s.s0]
        };
        let found = g.search(s.w, s.h, &starts, s.max_d, valid, visitor);
        let found_i = match found {
            Some(v) => v as i64,
            None => -1,
        };
        assert_eq!(order.len(), s.nvisits, "{} visit count", s.name);
        assert_eq!(found_i, s.found, "{} found", s.name);
        assert_eq!(g.debug_stamp(), s.stamp_after, "{} stamp after", s.name);
        if !s.order.is_empty() {
            assert_eq!(order, s.order.to_vec(), "{} visit order", s.name);
            assert_eq!(dists, s.dists.to_vec(), "{} visit dists", s.name);
        }
        // Sampled triples (covers the full stream for small scenarios and the
        // boundary/ends for the corridor).
        for &(idx, node, dist) in s.samples {
            assert_eq!(order.get(idx as usize).copied(), Some(node), "{} node@{idx}", s.name);
            assert_eq!(dists.get(idx as usize).copied(), Some(dist), "{} dist@{idx}", s.name);
        }
    }
}

#[test]
fn replay_astar_scenarios() {
    for s in vectors::ASTAR_SCENARIOS {
        let adapter = GridAdapter::new(s.w, s.h, s.blocked, s.cc, s.tp, s.hk, s.hs);
        let mut a = AStar::new(adapter, s.max_iter);
        let mut path = None;
        for _ in 0..s.runs {
            path = a.find_path(s.starts, s.goal);
        }
        match (s.path, &path) {
            (None, None) => {}
            (Some(want), Some(got)) => {
                assert_eq!(got.as_slice(), want, "{} path", s.name);
            }
            _ => panic!(
                "{} path: got {:?} want {:?}",
                s.name,
                path.as_deref(),
                s.path
            ),
        }
        assert_eq!(a.debug_stamp(), s.stamp_after, "{} stamp after", s.name);
        assert_eq!(a.debug_closed_stamp(), s.closed, "{} final closedStamp", s.name);
        assert_eq!(
            a.debug_g_score_stamp(),
            s.gs_stamp,
            "{} final gScoreStamp",
            s.name
        );
        assert_eq!(a.debug_g_score(), s.g_score, "{} final gScore", s.name);
        assert_eq!(a.debug_came_from(), s.came_from, "{} final cameFrom", s.name);
    }
}

#[test]
fn replay_rail_scenarios() {
    for s in vectors::RAIL_SCENARIOS {
        let map = TerrainMap::new(s.w, s.h, s.terrain.to_vec());
        let mut a = AStar::new(RailAdapter::new(map), None);
        let path = a.find_path(s.starts, s.goal);
        match (s.path, &path) {
            (None, None) => {}
            (Some(want), Some(got)) => {
                assert_eq!(got.as_slice(), want, "{} path", s.name);
            }
            _ => panic!(
                "{} path: got {:?} want {:?}",
                s.name,
                path.as_deref(),
                s.path
            ),
        }
        assert_eq!(a.debug_stamp(), s.stamp_after, "{} stamp after", s.name);
        assert_eq!(a.debug_closed_stamp(), s.closed, "{} final closedStamp", s.name);
        assert_eq!(
            a.debug_g_score_stamp(),
            s.gs_stamp,
            "{} final gScoreStamp",
            s.name
        );
        assert_eq!(a.debug_g_score(), s.g_score, "{} final gScore", s.name);
        assert_eq!(a.debug_came_from(), s.came_from, "{} final cameFrom", s.name);
    }
}
