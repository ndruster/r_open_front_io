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

use openfront_core::game_map::GameMap;
use openfront_core::pathfinding::a_star::{AStar, GridAdapter};
use openfront_core::pathfinding::bfs_grid::{BfsGrid, Visit};
use openfront_core::pathfinding::flat_heap::FlatBinaryHeap;
use openfront_core::pathfinding::priority_queue::{BucketQueue, MinHeap, PriorityQueue};
use openfront_core::pathfinding::rail::{RailAdapter, TerrainMap};
use openfront_core::pathfinding::water::AStarWater;
use vectors::GmRes;

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

#[test]
fn replay_water_scenarios() {
    for s in vectors::WATER_SCENARIOS {
        let mut a = AStarWater::new(
            s.w,
            s.h,
            s.terrain.to_vec(),
            Some(s.weight),
            Some(s.max_iter),
        );
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

// ---------------------------------------------------------------- GameMap
// Object.is equality: distinguishes -0 from 0 and treats NaN as equal to NaN.
fn obj_is(a: f64, b: f64) -> bool {
    if a.is_nan() && b.is_nan() {
        return true;
    }
    if a == 0.0 && b == 0.0 {
        return a.is_sign_negative() == b.is_sign_negative();
    }
    a == b
}

fn want_val(want: &GmRes) -> f64 {
    match want {
        GmRes::Val(v) => *v,
        _ => panic!("expected GmRes::Val, got {want:?}"),
    }
}

fn want_arr(want: &GmRes) -> &'static [f64] {
    match want {
        GmRes::Arr(a) => a,
        _ => panic!("expected GmRes::Arr, got {want:?}"),
    }
}

#[test]
fn replay_gamemap_scenarios() {
    for s in vectors::GAMEMAP_SCENARIOS {
        let mut gm = GameMap::new(s.w, s.h, s.terrain.to_vec(), s.num_land);
        let mut buf4 = [0.0f64; 4];
        let mut buf8 = [0.0f64; 8];
        for (i, op) in s.ops.iter().enumerate() {
            let (a, b) = (op.a, op.b);
            let ctx = || format!("{} op#{i} kind={}", s.name, op.kind);
            match op.kind {
                0 => {
                    gm.set_water(a);
                    assert!(matches!(op.res, GmRes::Void), "{}", ctx());
                }
                1 => {
                    gm.set_shoreline_bit(a);
                    assert!(matches!(op.res, GmRes::Void), "{}", ctx());
                }
                2 => {
                    gm.clear_shoreline_bit(a);
                    assert!(matches!(op.res, GmRes::Void), "{}", ctx());
                }
                3 => {
                    gm.set_ocean(a);
                    assert!(matches!(op.res, GmRes::Void), "{}", ctx());
                }
                4 => {
                    gm.set_magnitude(a, b);
                    assert!(matches!(op.res, GmRes::Void), "{}", ctx());
                }
                5 => {
                    if matches!(op.res, GmRes::Threw) {
                        // The throw happens before any mutation, so the live
                        // map is untouched by the caught panic.
                        let threw = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            gm.set_owner_id(a, b)
                        }))
                        .is_err();
                        assert!(threw, "{} expected set_owner_id to throw", ctx());
                    } else {
                        gm.set_owner_id(a, b);
                        assert!(matches!(op.res, GmRes::Void), "{}", ctx());
                    }
                }
                6 => {
                    gm.set_fallout(a, b != 0.0);
                    assert!(matches!(op.res, GmRes::Void), "{}", ctx());
                }
                7 => {
                    gm.set_defense_bonus(a, b != 0.0);
                    assert!(matches!(op.res, GmRes::Void), "{}", ctx());
                }
                8 => {
                    let got = gm.update_tile(a, b);
                    assert_eq!(got, want_val(&op.res) != 0.0, "{} updateTile", ctx());
                }
                9 => {
                    let got = gm.neighbors(a);
                    assert_arr(&got, want_arr(&op.res), &ctx);
                }
                10 => {
                    let n = gm.neighbors4(a, &mut buf4);
                    let got = buf4[..n].to_vec();
                    assert_arr(&got, want_arr(&op.res), &ctx);
                }
                11 => {
                    let n = gm.neighbors8(a, &mut buf8);
                    let got = buf8[..n].to_vec();
                    assert_arr(&got, want_arr(&op.res), &ctx);
                }
                12 => {
                    let mut got = Vec::new();
                    gm.for_each_neighbor_with_diag(a, |t| got.push(t));
                    assert_arr(&got, want_arr(&op.res), &ctx);
                }
                13 => assert_bool(gm.is_land(a), &op.res, &ctx),
                14 => assert_bool(gm.is_impassable(a), &op.res, &ctx),
                15 => assert_bool(gm.is_ocean_shore(a), &op.res, &ctx),
                16 => assert_bool(gm.is_shore(a), &op.res, &ctx),
                17 => assert_bool(gm.is_water(a), &op.res, &ctx),
                18 => assert_val(gm.cost(a), &op.res, &ctx),
                19 => {
                    let got = gm.terrain_type(a) as u8 as f64;
                    assert_val(got, &op.res, &ctx);
                }
                20 => assert_val(gm.magnitude(a), &op.res, &ctx),
                21 => assert_opt(gm.terrain_byte(a), &op.res, &ctx),
                22 => assert_val(gm.owner_id(a), &op.res, &ctx),
                23 => assert_opt(gm.tile_state(a), &op.res, &ctx),
                24 => assert_bool(gm.has_fallout(a), &op.res, &ctx),
                25 => assert_bool(gm.has_defense_bonus(a), &op.res, &ctx),
                26 => assert_bool(gm.has_owner(a), &op.res, &ctx),
                27 => assert_bool(gm.is_border(a), &op.res, &ctx),
                28 => assert_bool(gm.is_on_edge_of_map(a), &op.res, &ctx),
                29 => assert_val(gm.x(a), &op.res, &ctx),
                30 => assert_val(gm.y(a), &op.res, &ctx),
                31 => {
                    if matches!(op.res, GmRes::Threw) {
                        let threw = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            gm.tile_ref(a, b)
                        }))
                        .is_err();
                        assert!(threw, "{} expected ref to throw", ctx());
                    } else {
                        assert_val(gm.tile_ref(a, b), &op.res, &ctx);
                    }
                }
                32 => assert_val(gm.manhattan_dist(a, b), &op.res, &ctx),
                33 => assert_val(gm.euclidean_dist_squared(a, b), &op.res, &ctx),
                34 => {
                    let got = match b {
                        1.0 => gm.bfs(a, &|m: &GameMap, t: f64| m.is_land(t)),
                        2.0 => gm.bfs(a, &|_: &GameMap, t: f64| t % 2.0 == 0.0),
                        _ => gm.bfs(a, &|_: &GameMap, _: f64| true),
                    };
                    assert_arr(&got, want_arr(&op.res), &ctx);
                }
                35 => {
                    let radius = if b.abs() == 0.0 { 1.0 } else { b.abs() };
                    let even = b != 0.0;
                    let got = gm.circle_search(a, radius, |_, d2| !even || d2 % 2.0 == 0.0);
                    assert_arr(&got, want_arr(&op.res), &ctx);
                }
                k => panic!("{} unexpected gm op kind {k}", s.name),
            }
        }
        assert_eq!(gm.debug_terrain(), s.terrain_after, "{} final terrain", s.name);
        assert_eq!(gm.debug_state(), s.state_after, "{} final state", s.name);
        assert_eq!(gm.num_land_tiles(), s.num_land_after, "{} final land count", s.name);
        assert_eq!(gm.water_version(), s.water_version_after, "{} final waterVersion", s.name);
        assert_eq!(
            gm.num_tiles_with_fallout(),
            s.fallout_after,
            "{} final fallout count",
            s.name
        );
    }
}

fn assert_arr(got: &[f64], want: &[f64], ctx: &dyn Fn() -> String) {
    assert_eq!(got.len(), want.len(), "{} array length: got {got:?} want {want:?}", ctx());
    for (j, (g, w)) in got.iter().zip(want.iter()).enumerate() {
        assert!(obj_is(*g, *w), "{} element[{j}]: got {g} want {w}", ctx());
    }
}

fn assert_bool(got: bool, want: &GmRes, ctx: &dyn Fn() -> String) {
    let w = want_val(want);
    assert_eq!(got, w != 0.0, "{} bool: got {got} want {w}", ctx());
}

fn assert_val(got: f64, want: &GmRes, ctx: &dyn Fn() -> String) {
    let w = want_val(want);
    assert!(obj_is(got, w), "{} value: got {got} want {w}", ctx());
}

fn assert_opt(got: Option<f64>, want: &GmRes, ctx: &dyn Fn() -> String) {
    match (got, want) {
        (None, GmRes::Undef) => {}
        (Some(g), GmRes::Val(w)) => assert!(obj_is(g, *w), "{} opt: got {g} want {w}", ctx()),
        _ => panic!("{} opt mismatch: got {got:?} want {want:?}", ctx()),
    }
}
