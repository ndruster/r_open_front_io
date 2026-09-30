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
use openfront_core::pathfinding::connected_components::ConnectedComponents;
use openfront_core::pathfinding::flat_heap::FlatBinaryHeap;
use openfront_core::pathfinding::priority_queue::{BucketQueue, MinHeap, PriorityQueue};
use openfront_core::pathfinding::rail::{RailAdapter, TerrainMap};
use openfront_core::pathfinding::water::AStarWater;
use openfront_core::tile_set::TileSet;
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

// ---------------------------------------------------------------- TileSet
// Replays the scripted op streams the real TS `TileSet` executed. Kinds 7/8
// run mutations *inside* a forEach callback (via the begin/next/end iteration
// surface), so the trace pins the denseLen-re-read, the tombstone skip, and
// the iterDepth-gated deferred compaction.
#[test]
fn replay_tileset_scenarios() {
    for s in vectors::TILESET_SCENARIOS {
        let mut ts = TileSet::new(if s.initial.is_empty() {
            None
        } else {
            Some(s.initial)
        });
        for (i, op) in s.ops.iter().enumerate() {
            let ctx = || format!("{} op#{i} kind={}", s.name, op.kind);
            let want_bool = |res: &GmRes| matches!(res, GmRes::Val(v) if *v == 1.0);
            match op.kind {
                0 => {
                    ts.add(op.a);
                    assert!(matches!(op.res, GmRes::Void), "{}", ctx());
                }
                1 => {
                    let got = ts.delete(op.a);
                    assert_eq!(got, want_bool(&op.res), "{} delete", ctx());
                }
                2 => {
                    let got = ts.has(op.a);
                    assert_eq!(got, want_bool(&op.res), "{} has", ctx());
                }
                3 => {
                    assert_eq!(ts.size(), want_val(&op.res), "{} size", ctx());
                }
                4 => {
                    let got = ts.collect_values();
                    assert_arr(&got, want_arr(&op.res), &ctx);
                }
                5 => {
                    ts.clear();
                    assert!(matches!(op.res, GmRes::Void), "{}", ctx());
                }
                6 => {
                    ts.iter_begin();
                    let mut got = Vec::new();
                    let mut cur = 0usize;
                    while let Some(v) = ts.iter_next(&mut cur) {
                        got.push(v);
                    }
                    ts.iter_end();
                    assert_arr(&got, want_arr(&op.res), &ctx);
                }
                7 => {
                    // add inside the callback after the first visit.
                    ts.iter_begin();
                    let mut got = Vec::new();
                    let mut cur = 0usize;
                    while let Some(v) = ts.iter_next(&mut cur) {
                        got.push(v);
                        if got.len() == 1 {
                            ts.add(op.a);
                        }
                    }
                    ts.iter_end();
                    assert_arr(&got, want_arr(&op.res), &ctx);
                }
                8 => {
                    // delete inside the callback after the first visit.
                    ts.iter_begin();
                    let mut got = Vec::new();
                    let mut cur = 0usize;
                    while let Some(v) = ts.iter_next(&mut cur) {
                        got.push(v);
                        if got.len() == 1 {
                            ts.delete(op.a);
                        }
                    }
                    ts.iter_end();
                    assert_arr(&got, want_arr(&op.res), &ctx);
                }
                k => panic!("{} unexpected op kind {k}", s.name),
            }
        }
        assert_eq!(ts.debug_dense(), s.dense, "{} final dense", s.name);
        assert_eq!(ts.debug_dense_len(), s.dense_len as usize, "{} denseLen", s.name);
        assert_eq!(ts.size(), s.size, "{} final size", s.name);
        assert_eq!(ts.debug_table(), s.table, "{} final table", s.name);
        assert_eq!(ts.debug_table_used(), s.table_used, "{} tableUsed", s.name);
        assert_eq!(ts.debug_iter_depth(), s.iter_depth, "{} iterDepth", s.name);
    }
}

// ---------------------------------------------------------------- Util
// Replays the single-call scenarios the real TS `Util.ts` functions produced
// (kind table in gen_vectors.mjs). Results are compared by IEEE-754 bit
// pattern, so -0 and NaN are pinned exactly like every other vector.
#[test]
fn replay_util_scenarios() {
    use openfront_core::util;

    for s in vectors::UTIL_SCENARIOS {
        let a = s.args;
        let got: Vec<f64> = match s.kind {
            0 => vec![util::manhattan_dist_wrapped(
                &util::Cell { x: a[0], y: a[1] },
                &util::Cell { x: a[2], y: a[3] },
                a[4],
            )],
            1 => vec![util::within(a[0], a[1], a[2])],
            2 => vec![util::simple_hash(s.strs[0])],
            3 => match util::find_minimum_by(&a[2..], a[0] as u8, a[1] as u8) {
                Some(v) => vec![v],
                None => vec![],
            },
            4 => {
                let pairs: Vec<(f64, f64)> =
                    a.chunks(2).map(|c| (c[0], c[1])).collect();
                match util::get_mode(&pairs) {
                    Some(v) => vec![v],
                    None => vec![],
                }
            }
            5 => match util::to_int(a[0]) {
                Some(v) => vec![v as f64],
                None => vec![],
            },
            6 => vec![util::max_int(a[0] as i64, a[1] as i64) as f64],
            7 => vec![util::min_int(a[0] as i64, a[1] as i64) as f64],
            8 => vec![util::within_int(a[0] as i64, a[1] as i64, a[2] as i64) as f64],
            9 => vec![util::sigmoid(a[0], a[1], a[2])],
            10 => {
                let box_ = util::BoundingBox {
                    min: util::Cell { x: a[0], y: a[1] },
                    max: util::Cell { x: a[2], y: a[3] },
                };
                let c = util::bounding_box_center(&box_);
                vec![c.x, c.y]
            }
            11 => {
                let outer = util::BoundingBox {
                    min: util::Cell { x: a[0], y: a[1] },
                    max: util::Cell { x: a[2], y: a[3] },
                };
                let inner = util::BoundingBox {
                    min: util::Cell { x: a[4], y: a[5] },
                    max: util::Cell { x: a[6], y: a[7] },
                };
                vec![util::inscribed(&outer, &inner) as u8 as f64]
            }
            12 => {
                let gm = GameMap::new(a[0], a[1], vec![0x85; (a[0] * a[1]) as usize], a[0] * a[1]);
                let tiles = &a[3..];
                let bb = match a[2] as u8 {
                    0 => util::calculate_bounding_box(&gm, tiles.to_vec()),
                    1 => util::calculate_bounding_box(&gm, tiles.to_vec()),
                    _ => {
                        let mut ts = TileSet::new(Some(tiles));
                        ts.iter_begin();
                        let mut v = Vec::new();
                        let mut cur = 0usize;
                        while let Some(x) = ts.iter_next(&mut cur) {
                            v.push(x);
                        }
                        ts.iter_end();
                        util::calculate_bounding_box(&gm, v)
                    }
                };
                vec![bb.min.x, bb.min.y, bb.max.x, bb.max.y]
            }
            13 => {
                let gm = GameMap::new(a[0], a[1], vec![0x85; (a[0] * a[1]) as usize], a[0] * a[1]);
                util::bounding_box_tiles(&gm, a[2], a[3])
            }
            14 => {
                let gm = GameMap::new(a[0], a[1], vec![0x85; (a[0] * a[1]) as usize], a[0] * a[1]);
                let c = util::calculate_bounding_box_center(&gm, a[2..].to_vec());
                vec![c.x, c.y]
            }
            k => panic!("{} unexpected util kind {k}", s.name),
        };
        assert_eq!(got.len(), s.res.len(), "{} result length", s.name);
        for (i, (g, w)) in got.iter().zip(s.res.iter()).enumerate() {
            assert_eq!(
                g.to_bits(),
                w.to_bits(),
                "{} res[{}]: got {} want {}",
                s.name,
                i,
                g,
                w
            );
        }
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

// ---------------------------------------------------------------- TeamAssignment
#[test]
fn replay_team_scenarios() {
    use openfront_core::team_assignment as ta;

    for s in vectors::TEAM_SCENARIOS {
        let players: Vec<ta::PlayerInfo> = s
            .players
            .iter()
            .map(|p| ta::PlayerInfo {
                id: p.id.to_string(),
                player_type: match p.player_type {
                    0 => ta::PlayerType::Bot,
                    1 => ta::PlayerType::Human,
                    _ => ta::PlayerType::Nation,
                },
                client_id: if p.client_id.is_empty() { None } else { Some(p.client_id.to_string()) },
                clan_tag: if p.clan_tag.is_empty() { None } else { Some(p.clan_tag.to_string()) },
                friends: p.friends.iter().map(|f| f.to_string()).collect(),
                team_index: p.team_index,
            })
            .collect();
        let teams: Vec<String> = s.teams.iter().map(|t| t.to_string()).collect();
        let config = match s.config.kind {
            0 => ta::TeamCountConfig::Num(s.config.num),
            1 => ta::TeamCountConfig::Duos,
            2 => ta::TeamCountConfig::Trios,
            3 => ta::TeamCountConfig::Quads,
            4 => ta::TeamCountConfig::HumansVsNations,
            _ => ta::TeamCountConfig::Other(s.config.s.to_string()),
        };
        match s.kind {
            0 | 1 => {
                let got = if s.kind == 0 {
                    if s.has_max == 1 {
                        ta::assign_teams_with_max(&players, &teams, s.is_duo == 1, s.max_team_size)
                    } else {
                        ta::assign_teams(&players, &teams, s.is_duo == 1)
                    }
                } else {
                    ta::assign_teams_lobby_preview(
                        &players,
                        &teams,
                        &config,
                        s.nation_count as usize,
                    )
                };
                assert_eq!(got.len(), s.res.len(), "{} result length", s.name);
                for (i, ((pi, assign), want)) in got.iter().zip(s.res.iter()).enumerate() {
                    let team_idx = match assign {
                        ta::Assignment::Kicked => -1i64,
                        ta::Assignment::Team(t) => {
                            teams.iter().position(|x| x == t).unwrap() as i64
                        }
                    };
                    assert_eq!(
                        (*pi as i64, team_idx),
                        *want,
                        "{} entry#{i}: got ({pi}, {team_idx}) want {want:?}",
                        s.name
                    );
                }
            }
            2 => {
                let got = ta::get_max_team_size(s.config.num, s.total_players);
                // obj_is: NaN payloads differ between V8 and Rust's
                // canonical NaN, so compare by Object.is semantics.
                assert!(obj_is(got, s.res_nums[0]), "{} max: got {got} want {}", s.name, s.res_nums[0]);
            }
            3 => {
                let got = ta::resolve_teams_list(&config, s.total_players);
                match (s.status, got) {
                    (0, Ok(list)) => {
                        assert_eq!(list.len(), s.res_teams.len(), "{} teams len", s.name);
                        for (i, (g, w)) in list.iter().zip(s.res_teams.iter()).enumerate() {
                            assert_eq!(g, *w, "{} team#{i}", s.name);
                        }
                    }
                    (1, Err(ta::ResolveTeamsError::UnknownConfig)) => {}
                    (2, Err(ta::ResolveTeamsError::TooFewTeams)) => {}
                    (3, Err(ta::ResolveTeamsError::InvalidLength)) => {}
                    (st, r) => panic!("{} status {st} got {r:?}", s.name),
                }
            }
            k => panic!("{} unexpected team kind {k}", s.name),
        }
    }
}

// ---------------------------------------------------------------- Bezier
#[test]
fn replay_bezier_scenarios() {
    use openfront_core::line::{DistanceBasedBezierCurve, Point};

    for s in vectors::BEZIER_SCENARIOS {
        let cp = s.cp;
        let pts = |i: usize| Point { x: cp[i * 2], y: cp[i * 2 + 1] };
        if s.kind == 0 {
            let got = DistanceBasedBezierCurve::get_length(&pts(0), &pts(1), &pts(2), &pts(3));
            assert!(obj_is(got, s.len), "{} len: got {got} want {}", s.name, s.len);
            continue;
        }
        let mut curve =
            DistanceBasedBezierCurve::new(&pts(0), &pts(1), &pts(2), &pts(3), s.spacing);
        let points = curve.all_points();
        assert_eq!(points.len() * 2, s.points.len(), "{} points len", s.name);
        for (i, p) in points.iter().enumerate() {
            assert!(
                obj_is(p.x, s.points[i * 2]) && obj_is(p.y, s.points[i * 2 + 1]),
                "{} point#{i}: got ({}, {}) want ({}, {})",
                s.name,
                p.x,
                p.y,
                s.points[i * 2],
                s.points[i * 2 + 1]
            );
        }
        assert_eq!(s.walk.len() % 3, 0, "{} walk shape", s.name);
        let n = s.walk.len() / 3;
        for i in 0..n {
            let got = curve.increment(s.incs[i]);
            let (wi, wx, wy) = (s.walk[i * 3], s.walk[i * 3 + 1], s.walk[i * 3 + 2]);
            match got {
                None => assert!(wi == -1.0, "{} walk#{i}: got None want ({wi}, {wx}, {wy})", s.name),
                Some(p) => {
                    assert!(wi >= 0.0, "{} walk#{i}: got ({}, {}) want null", s.name, p.x, p.y);
                    assert_eq!(curve.current_index() as f64, wi, "{} walk#{i} index", s.name);
                    assert!(
                        obj_is(p.x, wx) && obj_is(p.y, wy),
                        "{} walk#{i}: got ({}, {}) want ({wx}, {wy})",
                        s.name,
                        p.x,
                        p.y
                    );
                }
            }
        }
        assert_eq!(curve.current_index() as u64, s.final_index, "{} final index", s.name);
    }
}

// ---------------------------------------------------------------- Veterancy
#[test]
fn replay_veterancy_scenarios() {
    use openfront_core::veterancy::max_health_with_veterancy;
    for s in vectors::VETERANCY_SCENARIOS {
        let got = max_health_with_veterancy(s.base, s.vet, s.pct);
        assert!(obj_is(got, s.res), "{}: got {got} want {}", s.name, s.res);
    }
}

// ---------------------------------------------------------------- MotionPlans
#[test]
fn replay_motionplans_scenarios() {
    use openfront_core::motion_plans::{
        pack_motion_plans, unpack_motion_plans, MotionPlanInput, MotionPlanRecord,
    };

    // Decode the [count, ...records] token stream into pack inputs. Scalars
    // and path elements stay f64 so the `>>> 0` coercion inside pack is
    // exercised exactly as the TS saw it.
    fn decode_inputs(t: &[f64]) -> Vec<MotionPlanInput<'_>> {
        let mut out = Vec::new();
        let mut i = 1usize; // skip the leading count
        let count = t[0] as usize;
        for _ in 0..count {
            let kind = t[i] as u32;
            i += 1;
            if kind == 1 {
                let path_len = t[i + 4] as usize;
                let path = &t[i + 5..i + 5 + path_len];
                out.push(MotionPlanInput::Grid {
                    unit_id: t[i],
                    plan_id: t[i + 1],
                    start_tick: t[i + 2],
                    ticks_per_step: t[i + 3],
                    path,
                });
                i += 5 + path_len;
            } else {
                let car_count = t[i + 5] as usize;
                let path_len = t[i + 6] as usize;
                let cars = &t[i + 7..i + 7 + car_count];
                let path = &t[i + 7 + car_count..i + 7 + car_count + path_len];
                out.push(MotionPlanInput::Train {
                    engine_unit_id: t[i],
                    plan_id: t[i + 1],
                    start_tick: t[i + 2],
                    speed: t[i + 3],
                    spacing: t[i + 4],
                    car_unit_ids: cars,
                    path,
                });
                i += 7 + car_count + path_len;
            }
        }
        out
    }

    // Encode unpacked records back into the same token-stream shape, with the
    // u32 fields widened to f64 (they fit exactly).
    fn encode_records(records: &[MotionPlanRecord]) -> Vec<f64> {
        let mut out = vec![records.len() as f64];
        for r in records {
            match r {
                MotionPlanRecord::Grid {
                    unit_id,
                    plan_id,
                    start_tick,
                    ticks_per_step,
                    path,
                } => {
                    out.extend([
                        1.0,
                        *unit_id as f64,
                        *plan_id as f64,
                        *start_tick as f64,
                        *ticks_per_step as f64,
                        path.len() as f64,
                    ]);
                    out.extend(path.iter().map(|v| *v as f64));
                }
                MotionPlanRecord::Train {
                    engine_unit_id,
                    car_unit_ids,
                    plan_id,
                    start_tick,
                    speed,
                    spacing,
                    path,
                } => {
                    out.extend([
                        2.0,
                        *engine_unit_id as f64,
                        *plan_id as f64,
                        *start_tick as f64,
                        *speed as f64,
                        *spacing as f64,
                        car_unit_ids.len() as f64,
                        path.len() as f64,
                    ]);
                    out.extend(car_unit_ids.iter().map(|v| *v as f64));
                    out.extend(path.iter().map(|v| *v as f64));
                }
            }
        }
        out
    }

    fn cmp_out(name: &str, got: &[f64], want: &[f64]) {
        assert_eq!(got.len(), want.len(), "{name} out len: got {} want {}", got.len(), want.len());
        for (i, v) in got.iter().enumerate() {
            assert!(obj_is(*v, want[i]), "{name} out#{i}: got {v} want {}", want[i]);
        }
    }

    for s in vectors::MP_SCENARIOS {
        // words = [len, ...buffer]
        let buf = &s.words[1..];
        let buf_u32: Vec<u32> = buf.iter().map(|v| *v as u32).collect();
        if s.kind == 0 {
            // pack(input) must equal words, then unpack(words) must equal out.
            let inputs = decode_inputs(s.input);
            let packed = pack_motion_plans(&inputs);
            assert_eq!(packed.len(), buf.len(), "{} pack len", s.name);
            for (i, w) in packed.iter().enumerate() {
                assert_eq!(*w, buf_u32[i], "{} pack word#{i}: got {w} want {}", s.name, buf_u32[i]);
            }
        }
        let unpacked = unpack_motion_plans(&buf_u32);
        cmp_out(s.name, &encode_records(&unpacked), s.out);
    }
}

// ---------------------------------------------------------- ConnectedComponents
// kind: 0=initialize (void), 1=addWaterTiles([a]) (void), 2=getComponentId(a),
//       3=getComponentSize(a). The trace pins the final componentIds buffer,
//       the sparse _componentSizes (holes -> NaN), the union-find parents,
//       maxId and landMarker.
#[test]
fn replay_cc_scenarios() {
    for s in vectors::CC_SCENARIOS {
        let mut cc = ConnectedComponents::new(
            s.w as i64,
            s.h as i64,
            s.terrain.to_vec(),
            s.direct != 0,
        );
        for (i, op) in s.ops.iter().enumerate() {
            let ctx = || format!("{} op#{i} kind={}", s.name, op.kind);
            match op.kind {
                0 => cc.initialize(),
                1 => cc.add_water_tile(op.a),
                2 => assert_val(cc.get_component_id(op.a) as f64, &op.res, &ctx),
                3 => assert_val(cc.get_component_size(op.a), &op.res, &ctx),
                k => panic!("{} unexpected cc op kind {k}", s.name),
            }
        }
        assert_eq!(cc.debug_bits(), s.bits, "{} final bits", s.name);
        assert_arr(&cc.debug_ids(), s.ids, &|| format!("{} ids", s.name));
        assert_arr(&cc.debug_sizes(), s.sizes, &|| format!("{} sizes", s.name));
        assert_arr(&cc.debug_parents(), s.parents, &|| format!("{} parents", s.name));
        assert!(obj_is(cc.debug_max_id(), s.max_id), "{} maxId", s.name);
        assert!(
            obj_is(cc.debug_land_marker(), s.land_marker),
            "{} landMarker",
            s.name
        );
    }
}
