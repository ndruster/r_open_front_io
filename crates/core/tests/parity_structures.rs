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
use openfront_core::pathfinding::abstract_graph::{
    AbstractEdge, AbstractGraph, AbstractGraphBuilder, AbstractNode,
};
use openfront_core::pathfinding::abstract_graph_astar::AbstractGraphAStar;
use openfront_core::pathfinding::a_star::{AStar, GridAdapter};
use openfront_core::pathfinding::bfs::BfsAdapter;
use openfront_core::pathfinding::bfs_grid::{BfsGrid, Visit};
use openfront_core::pathfinding::connected_components::ConnectedComponents;
use openfront_core::pathfinding::flat_heap::FlatBinaryHeap;
use openfront_core::pathfinding::priority_queue::{BucketQueue, MinHeap, PriorityQueue};
use openfront_core::pathfinding::rail::{RailAdapter, TerrainMap};
use openfront_core::pathfinding::water::AStarWater;
use openfront_core::pathfinding::water_bounded::AStarWaterBounded;
use openfront_core::pathfinding::water_hierarchical::AStarWaterHierarchical;
use openfront_core::terrain_search_map::TerrainSearchMap;
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

#[test]
fn replay_waterbounded_scenarios() {
    for s in vectors::WATERBOUNDED_SCENARIOS {
        let mut a = AStarWaterBounded::new(
            s.w,
            s.terrain.to_vec(),
            s.max_area,
            Some(s.weight),
            Some(s.max_iter),
        );
        let path = if s.mode == 1 {
            a.search_bounded(s.starts, s.goal, s.bounds[0], s.bounds[1], s.bounds[2], s.bounds[3])
        } else {
            a.find_path(s.starts, s.goal)
        };
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
            15 | 16 => {
                let units: Vec<u16> = a[1..].iter().map(|u| *u as u16).collect();
                let got = if s.kind == 15 {
                    util::sanitize_clan_tag(&units)
                } else {
                    util::sanitize_lobby_label(&units)
                };
                let mut v = vec![got.len() as f64];
                v.extend(got.iter().map(|u| f64::from(*u)));
                v
            }
            17 | 18 => {
                let gm = GameMap::new(a[0], a[1], vec![0x85; (a[0] * a[1]) as usize], a[0] * a[1]);
                util::dist_sort(&gm, a[2], &a[3..])
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

// ---------------------------------------------------------- TerrainSearchMap
// kind: 0=getWidth, 1=getHeight, 2=node(x,y) -> Val (the tile-type discriminant),
//       3=neighbors(x,y) -> flattened [x0,y0,x1,y1,...]. The buffer (4-byte
//       header + packed tiles) is replayed verbatim; node args and neighbor
//       coordinates may be NaN / infinite / fractional.
#[test]
fn replay_tsm_scenarios() {
    use openfront_core::terrain_search_map::SearchMapTileType;
    for s in vectors::TSM_SCENARIOS {
        let tsm = TerrainSearchMap::new(s.buffer.to_vec());
        for (i, op) in s.ops.iter().enumerate() {
            let ctx = || format!("{} op#{i} kind={}", s.name, op.kind);
            match op.kind {
                0 => assert_val(tsm.get_width(), &op.res, &ctx),
                1 => assert_val(tsm.get_height(), &op.res, &ctx),
                2 => {
                    let code = match tsm.node(op.a, op.b) {
                        SearchMapTileType::Land => 0.0,
                        SearchMapTileType::Shore => 1.0,
                        SearchMapTileType::Water => 2.0,
                    };
                    assert_val(code, &op.res, &ctx);
                }
                3 => {
                    let mut got = Vec::new();
                    for n in tsm.neighbors(op.a, op.b) {
                        got.push(n.x);
                        got.push(n.y);
                    }
                    assert_arr(&got, want_arr(&op.res), &ctx);
                }
                k => panic!("{} unexpected tsm op kind {k}", s.name),
            }
        }
    }
}

// -------------------------------------------------------------- AbstractGraph
// kind table (gen_vectors.mjs): 0=nodeCount, 1=edgeCount, 2=getNode(a),
// 3=getEdge(a), 4=getNodeEdges(a), 5=getEdgeBetween(a,b),
// 6=getOtherNode(edgeId=a,node=b), 7=getClusterKey(a,b), 8=getCluster(cx,cy),
// 9=getClusterNodes, 10=getNearbyClusterNodes, 11=getComponentId(a),
// 12=getComponentSize(a), 13=getCachedPath(edgeId,fromNode), 14=setCachedPath.
// Node results flatten to [id,x,y,tile,componentId]; edge results to
// [id,nodeA,nodeB,cost,clusterX,clusterY]; cluster to [x,y,count,ids...].
#[test]
fn replay_abstract_graph_scenarios() {
    use openfront_core::pathfinding::abstract_graph::{AbstractEdge, AbstractNode};

    let ag_node = |n: Option<AbstractNode>| -> Vec<f64> {
        match n {
            Some(n) => vec![
                n.id as f64,
                n.x as f64,
                n.y as f64,
                n.tile,
                n.component_id as f64,
            ],
            None => Vec::new(),
        }
    };
    let ag_edge = |e: Option<AbstractEdge>| -> Vec<f64> {
        match e {
            Some(e) => vec![
                e.id as f64,
                e.node_a as f64,
                e.node_b as f64,
                e.cost,
                e.cluster_x as f64,
                e.cluster_y as f64,
            ],
            None => Vec::new(),
        }
    };

    // Scenarios replay in order; each may seed a later partial rebuild.
    let mut built: Vec<AbstractGraph> = Vec::new();

    for (si, s) in vectors::AG_SCENARIOS.iter().enumerate() {
        let gm = GameMap::new(s.w, s.h, s.terrain.to_vec(), 0.0);
        let cs = s.cluster_size as i64;
        let old = if s.old_idx >= 0 {
            Some(built[s.old_idx as usize].clone())
        } else {
            None
        };
        let mut builder = if let Some(old) = old {
            AbstractGraphBuilder::with_rebuild(
                gm,
                cs,
                Some(old),
                (!s.dirty.is_empty()).then(|| s.dirty.to_vec()),
            )
        } else {
            AbstractGraphBuilder::new(gm, cs)
        };
        let mut graph = builder.build();

        for (i, op) in s.ops.iter().enumerate() {
            let ctx = || format!("{} op#{i} kind={}", s.name, op.kind);
            match op.kind {
                0 => assert_val(graph.node_count(), &op.res, &ctx),
                1 => assert_val(graph.edge_count(), &op.res, &ctx),
                2 => {
                    if matches!(op.res, GmRes::Undef) {
                        assert!(graph.get_node(op.a as i64).is_none(), "{} getNode none", ctx());
                    } else {
                        let got = ag_node(graph.get_node(op.a as i64));
                        assert_arr(&got, want_arr(&op.res), &ctx);
                    }
                }
                3 => {
                    if matches!(op.res, GmRes::Undef) {
                        assert!(graph.get_edge(op.a as i64).is_none(), "{} getEdge none", ctx());
                    } else {
                        let got = ag_edge(graph.get_edge(op.a as i64));
                        assert_arr(&got, want_arr(&op.res), &ctx);
                    }
                }
                4 => {
                    let mut got = Vec::new();
                    for e in graph.get_node_edges(op.a as i64) {
                        got.extend(ag_edge(Some(e)));
                    }
                    assert_arr(&got, want_arr(&op.res), &ctx);
                }
                5 => {
                    if matches!(op.res, GmRes::Undef) {
                        assert!(
                            graph.get_edge_between(op.a as i64, op.b as i64).is_none(),
                            "{} getEdgeBetween none",
                            ctx()
                        );
                    } else {
                        let got = ag_edge(graph.get_edge_between(op.a as i64, op.b as i64));
                        assert_arr(&got, want_arr(&op.res), &ctx);
                    }
                }
                6 => {
                    let edge = graph.get_edge(op.a as i64);
                    if matches!(op.res, GmRes::Threw) {
                        assert!(edge.is_none(), "{} getOtherNode expects missing edge", ctx());
                    } else {
                        let other = AbstractGraph::get_other_node(&edge.unwrap(), op.b as i64);
                        assert_val(other as f64, &op.res, &ctx);
                    }
                }
                7 => assert_val(
                    graph.get_cluster_key(op.a as i64, op.b as i64) as f64,
                    &op.res,
                    &ctx,
                ),
                8 => {
                    if matches!(op.res, GmRes::Undef) {
                        assert!(
                            graph.get_cluster(op.a as i64, op.b as i64).is_none(),
                            "{} getCluster none",
                            ctx()
                        );
                    } else {
                        let c = graph.get_cluster(op.a as i64, op.b as i64).unwrap();
                        let mut got = vec![c.x as f64, c.y as f64, c.node_ids.len() as f64];
                        got.extend(c.node_ids.iter().map(|&id| id as f64));
                        assert_arr(&got, want_arr(&op.res), &ctx);
                    }
                }
                9 => {
                    let mut got = Vec::new();
                    for n in graph.get_cluster_nodes(op.a as i64, op.b as i64) {
                        got.extend(ag_node(Some(n)));
                    }
                    assert_arr(&got, want_arr(&op.res), &ctx);
                }
                10 => {
                    let mut got = Vec::new();
                    for n in graph.get_nearby_cluster_nodes(op.a as i64, op.b as i64) {
                        got.extend(ag_node(Some(n)));
                    }
                    assert_arr(&got, want_arr(&op.res), &ctx);
                }
                11 => assert_val(graph.get_component_id(op.a) as f64, &op.res, &ctx),
                12 => assert_val(graph.get_component_size(op.a), &op.res, &ctx),
                13 => {
                    let p = graph.get_cached_path(op.a as i64, op.b as i64);
                    if matches!(op.res, GmRes::Undef) {
                        assert!(p.is_none(), "{} getCachedPath none", ctx());
                    } else {
                        assert_arr(&p.unwrap(), want_arr(&op.res), &ctx);
                    }
                }
                14 => {
                    // Mirror the TS capture: path = [edgeId, fromNode, dir].
                    let edge = graph.get_edge(op.a as i64);
                    if let Some(e) = edge {
                        let dir = if op.b as i64 == e.node_a { 0 } else { 1 };
                        graph.set_cached_path(
                            op.a as i64,
                            op.b as i64,
                            vec![op.a, op.b, (op.a * 2.0 + dir as f64)],
                        );
                    }
                    assert!(matches!(op.res, GmRes::Void), "{} setCachedPath void", ctx());
                }
                k => panic!("{} unexpected ag op kind {k}", s.name),
            }
        }

        // Final internal arrays.
        assert!(obj_is(graph.node_count(), s.node_count), "{} node_count", s.name);
        assert!(obj_is(graph.edge_count(), s.edge_count), "{} edge_count", s.name);
        assert!(
            obj_is(graph.debug_path_cache_len(), s.path_cache_len),
            "{} path_cache_len",
            s.name
        );
        assert_arr(&graph.debug_nodes(), s.nodes, &|| s.name.to_string());
        assert_arr(&graph.debug_edges(), s.edges, &|| s.name.to_string());
        assert_arr(&graph.debug_clusters(), s.clusters, &|| s.name.to_string());
        assert_arr(&graph.debug_node_edge_ids(), s.node_edge_ids, &|| s.name.to_string());

        built.push(graph);
        let _ = si;
    }
}

// ------------------------------------------------- AbstractGraphAStar (P14)
// Replays the query scripts the real TS `AbstractGraphAStar` ran over a
// hand-built graph, asserting the returned path AND the full engine state
// (stamp, the five node arrays, the live heap) after EVERY query — the
// multi-query scenarios pin stamp reuse and the queue-before-clear ordering.
#[test]
fn replay_abstract_graph_astar_scenarios() {
    for s in vectors::ABSTRACT_GRAPH_ASTAR_SCENARIOS {
        let mut graph = AbstractGraph::new(1, 1, 1);
        for n in s.nodes.chunks(3) {
            graph.add_node(AbstractNode {
                id: n[0] as i64,
                x: n[1] as i64,
                y: n[2] as i64,
                tile: 0.0,
                component_id: 0,
            });
        }
        for e in s.edges.chunks(4) {
            graph.add_edge(AbstractEdge {
                id: e[0] as i64,
                node_a: e[1] as i64,
                node_b: e[2] as i64,
                cost: e[3],
                cluster_x: 0,
                cluster_y: 0,
            });
        }
        assert_eq!(graph.node_count(), s.num_nodes, "{} nodeCount", s.name);
        assert_eq!(graph.edge_count(), s.edge_count, "{} edgeCount", s.name);

        let mut eng = AbstractGraphAStar::new(
            s.num_nodes,
            s.edge_count,
            Some(s.weight),
            Some(s.max_iter),
        );

        for (qi, q) in s.queries.iter().enumerate() {
            let ctx = || format!("{} q#{qi}", s.name);
            let path = if q.is_multi == 1 {
                eng.find_path_multi(&graph, q.starts, q.goal)
            } else {
                eng.find_path_single(&graph, q.starts[0], q.goal)
            };
            match (q.path, &path) {
                (None, None) => {}
                (Some(want), Some(got)) => {
                    assert_eq!(got.as_slice(), want, "{} path", ctx());
                }
                _ => panic!("{} path: got {:?} want {:?}", ctx(), path, q.path),
            }
            assert_eq!(eng.debug_stamp(), q.stamp_after, "{} stamp", ctx());
            assert_eq!(eng.debug_closed_stamp(), q.closed, "{} closedStamp", ctx());
            assert_eq!(eng.debug_g_score_stamp(), q.gs_stamp, "{} gScoreStamp", ctx());
            assert_eq!(eng.debug_g_score_bits(), q.g_score_bits, "{} gScore bits", ctx());
            assert_eq!(eng.debug_came_from(), q.came_from, "{} cameFrom", ctx());
            assert_eq!(eng.debug_start_node(), q.start_node, "{} startNode", ctx());
            let (heap, pri, size, cap) = eng.debug_queue();
            assert_eq!(heap, q.q_heap, "{} queue heap", ctx());
            assert_eq!(pri, q.q_pri_bits, "{} queue pri bits", ctx());
            assert_eq!(size, q.q_size, "{} queue size", ctx());
            assert_eq!(cap, q.q_cap, "{} queue capacity", ctx());
        }
    }
}

// --- AStarWaterHierarchical --------------------------------------------------
// Builds the real graph over a real GameMap (same construction the capture
// used), runs the orchestrator query script, and asserts the returned path,
// the five engine stamps (the dispatch witness) and — for cachePaths
// scenarios — the graph path cache after EVERY query.
#[test]
fn replay_water_hierarchical_scenarios() {
    for s in vectors::WATER_HIERARCHICAL_SCENARIOS {
        let gm = GameMap::new(s.w, s.h, s.terrain.to_vec(), s.w * s.h);
        let mut builder = AbstractGraphBuilder::new(gm, s.cluster_size as i64);
        let graph = builder.build();
        let gm2 = GameMap::new(s.w, s.h, s.terrain.to_vec(), s.w * s.h);
        let mut wh = AStarWaterHierarchical::new(gm2, graph, s.cache_paths == 1);

        for (qi, q) in s.queries.iter().enumerate() {
            let ctx = || format!("{} q#{qi}", s.name);
            if q.rebuild_before == 1 {
                let gm2 = GameMap::new(s.w, s.h, s.terrain.to_vec(), s.w * s.h);
                let mut b2 = AbstractGraphBuilder::new(gm2, s.cluster_size as i64);
                wh.set_graph(b2.build());
            }
            let path = if q.is_multi == 1 {
                wh.find_path_multi(q.starts, q.goal)
            } else {
                wh.find_path_single(q.starts[0], q.goal)
            };
            match (q.path, &path) {
                (None, None) => {}
                (Some(want), Some(got)) => {
                    assert_eq!(got.as_slice(), want, "{} path", ctx());
                }
                _ => panic!("{} path: got {:?} want {:?}", ctx(), path, q.path),
            }
            let (bfs, local, multi, short, aga) = wh.debug_stamps();
            assert_eq!(bfs, q.bfs, "{} bfs stamp", ctx());
            assert_eq!(local, q.local, "{} local stamp", ctx());
            assert_eq!(multi, q.multi, "{} multi stamp", ctx());
            assert_eq!(short, q.short, "{} short stamp", ctx());
            assert_eq!(aga, q.aga, "{} aga stamp", ctx());
            if s.cache_paths == 1 {
                assert_eq!(wh.debug_path_cache(), q.cache, "{} path cache", ctx());
            }
        }
    }
}

// --- Parabola (PathFinder.Parabola.ts) ----------------------------------------
// Replays the control-point reads, findPath script and single-instance
// next()/invalidate()/currentIndex walk against the Rust port. Out-of-bounds
// `ref` throws in TS and panics in Rust; those steps are asserted via
// catch_unwind, and the partial curve state at the throw (increment already
// ran) is pinned by the recorded index.
#[test]
fn replay_parabola_scenarios() {
    use openfront_core::pathfinding::parabola::{
        get_parabola_control_points, ParabolaOptions, ParabolaUniversalPathFinder, PathResult,
    };
    let tri = |v: u8| match v {
        0 => None,
        1 => Some(false),
        _ => Some(true),
    };
    for s in vectors::PARABOLA_SCENARIOS {
        let gm = GameMap::new(
            s.w,
            s.h,
            vec![0x03; (s.w * s.h) as usize],
            s.w * s.h,
        );
        let opt = ParabolaOptions {
            increment: if s.increment < 0.0 { None } else { Some(s.increment) },
            distance_based_height: tri(s.distance_based_height),
            direction_up: tri(s.direction_up),
            ignore_map_bounds: tri(s.ignore_map_bounds),
        };

        for (i, g) in s.cps.chunks_exact(10).enumerate() {
            let got = get_parabola_control_points(&gm, g[0], g[1], Some(&opt));
            let flat: Vec<f64> = got.iter().flat_map(|p| [p.x, p.y]).collect();
            assert_eq!(&flat[..], &g[2..], "{} cps#{} coords", s.name, i);
        }

        let pf = ParabolaUniversalPathFinder::new(&gm, Some(opt));
        let mut fi = 0;
        let mut k = 0;
        while k < s.finds.len() {
            let (from, to, len) = (s.finds[k], s.finds[k + 1], s.finds[k + 2] as i64);
            if len < 0 {
                let threw = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    pf.find_path(from, to)
                }))
                .is_err();
                assert!(threw, "{} find#{} expected panic", s.name, fi);
                k += 3;
            } else {
                let n = len as usize;
                let want = &s.finds[k + 3..k + 3 + n];
                let got = pf.find_path(from, to);
                assert_eq!(got.as_slice(), want, "{} find#{} path", s.name, fi);
                k += 3 + n;
            }
            fi += 1;
        }

        let mut pf = pf;
        for (j, g) in s.walk.chunks_exact(8).enumerate() {
            let (kind, from, to, has_speed, speed, status, node, index) = (
                g[0] as i64,
                g[1],
                g[2],
                g[3] as i64,
                g[4],
                g[5] as i64,
                g[6],
                g[7] as usize,
            );
            let sp = if has_speed == 1 { Some(speed) } else { None };
            match kind {
                0 => {
                    let r = pf.next(from, to, sp);
                    match (&r, status) {
                        (PathResult::Next { node: n, .. }, 0) => {
                            assert_eq!(*n, node, "{} walk#{} node", s.name, j)
                        }
                        (PathResult::Complete { node: n, .. }, 2) => {
                            assert_eq!(*n, node, "{} walk#{} node", s.name, j)
                        }
                        _ => panic!(
                            "{} walk#{} status: got {r:?} want {status}",
                            s.name, j
                        ),
                    }
                    assert_eq!(pf.current_index(), index, "{} walk#{} index", s.name, j);
                }
                1 => {
                    let threw = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        pf.next(from, to, sp)
                    }))
                    .is_err();
                    assert!(threw, "{} walk#{} expected panic", s.name, j);
                    assert_eq!(pf.current_index(), index, "{} walk#{} index", s.name, j);
                }
                2 => pf.invalidate(),
                3 => assert_eq!(pf.current_index(), index, "{} walk#{} index read", s.name, j),
                other => panic!("{} unknown walk kind {other}", s.name),
            }
        }
    }
}

// --- MiniMapTransformer (transformers/MiniMapTransformer.ts) ------------------
// Replays the query script against the Rust port with the scripted
// `ScriptedFinder` inner stub. Each group pins: the start/goal refs the
// transformer passed to `inner` (scalar vs array + the minimap downscale), the
// upscaled/endpoint-repaired result path, and the two throw classes (downscale
// and upscale `ref` out of bounds) asserted via catch_unwind.
#[test]
fn replay_minimap_transformer_scenarios() {
    use openfront_core::pathfinding::mini_map_transformer::{
        MiniMapTransformer, ScriptedFinder,
    };
    use openfront_core::pathfinding::PathStart;
    for s in vectors::MMT_SCENARIOS {
        let main = GameMap::new(
            s.mw,
            s.mh,
            vec![0x03; (s.mw * s.mh) as usize],
            s.mw * s.mh,
        );
        let mini = GameMap::new(
            s.mini_w,
            s.mini_h,
            vec![0x03; (s.mini_w * s.mini_h) as usize],
            s.mini_w * s.mini_h,
        );
        let g = s.groups;
        let mut k = 0;
        let mut qi = 0;
        while k < g.len() {
            // [from_is_array, from_len, from_tiles..., to]
            let from_is_array = g[k] as i64;
            let from_len = g[k + 1] as usize;
            let from_tiles = &g[k + 2..k + 2 + from_len];
            let to = g[k + 2 + from_len];
            k += 3 + from_len;
            // [inner_mode, inner_len, inner_tiles...]
            let inner_mode = g[k] as i64;
            let inner_len = g[k + 1] as usize;
            let inner_tiles: Vec<f64> = g[k + 2..k + 2 + inner_len].to_vec();
            k += 2 + inner_len;
            // [seen_flag, [is_multi, seen_len, seen_tiles..., seen_goal]]
            let seen_flag = g[k] as i64;
            let mut seen: Option<(i64, Vec<f64>, f64)> = None;
            if seen_flag == 1 {
                let m = g[k + 1] as i64;
                let l = g[k + 2] as usize;
                let tiles = g[k + 3..k + 3 + l].to_vec();
                let goal = g[k + 3 + l];
                seen = Some((m, tiles, goal));
                k += 3 + l + 1;
            } else {
                k += 1;
            }
            // [out_mode, [out_len, out_tiles...]]
            let out_mode = g[k] as i64;
            let out_tiles: Vec<f64> = if out_mode == 2 {
                let l = g[k + 1] as usize;
                let t = g[k + 2..k + 2 + l].to_vec();
                k += 2 + l;
                t
            } else {
                k += 1;
                Vec::new()
            };

            let starts = if from_is_array == 1 {
                PathStart::Multi(from_tiles)
            } else {
                PathStart::Single(from_tiles[0])
            };

            // Fresh stub + transformer per query (the transformer is stateless
            // across calls in TS, so this is faithful); `tr` borrows `stub`
            // mutably, so it is dropped before reading `last_seen`.
            let mut stub = ScriptedFinder::default();
            stub.push_path(match inner_mode {
                0 => None,
                1 => Some(vec![]),
                _ => Some(inner_tiles),
            });
            let got = {
                let mut tr = MiniMapTransformer::new(&mut stub, &main, &mini);
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    tr.find_path(starts, to)
                }))
            };
            match out_mode {
                0 => {
                    assert_eq!(got.expect("panic in null query"), None, "{} q{} null", s.name, qi);
                }
                1 => {
                    assert!(got.is_err(), "{} q{} expected panic", s.name, qi);
                }
                2 => {
                    let p = got.expect("panic in path query").expect("unexpected null");
                    assert_eq!(p, out_tiles, "{} q{} path", s.name, qi);
                }
                other => panic!("{} unknown out_mode {other}", s.name),
            }

            match (&seen, stub.last_seen.as_ref()) {
                (None, None) => {}
                (Some((m, tiles, goal)), Some((im, itiles, igoal))) => {
                    assert_eq!(*im as i64, *m, "{} q{} seen kind", s.name, qi);
                    assert_eq!(itiles, tiles, "{} q{} seen tiles", s.name, qi);
                    assert_eq!(igoal, goal, "{} q{} seen goal", s.name, qi);
                }
                (a, b) => panic!(
                    "{} q{} seen mismatch: recorded {:?}, rust {:?}",
                    s.name,
                    qi,
                    a.is_some(),
                    b.is_some()
                ),
            }
            qi += 1;
        }
    }
}

// --- PathFinderStepper (PathFinderStepper.ts) ---------------------------------
// Replays the op script against the Rust stepper with the shared `SharedStub`
// inner finder. Each `next` pins the PathResult, the `pathAfterNext` slice,
// the internal `pathIndex`/`path !== null` cache state and the cumulative
// inner call count (so cache hits, pre-check short-circuits and the vacuous
// `every` on an empty start array are all observable without calling inner).
#[test]
fn replay_stepper_scenarios() {
    use openfront_core::pathfinding::parabola::PathResult;
    use openfront_core::pathfinding::stepper::{PathFinderStepper, SharedStub};
    use openfront_core::pathfinding::PathStart;
    for s in vectors::STEPPER_SCENARIOS {
        let gm = GameMap::new(10.0, 10.0, vec![0x03; 100], 100.0);
        let stub = SharedStub::default();
        let mut st = PathFinderStepper::new(stub.clone(), if s.prod { Some(&gm) } else { None });

        let o = s.ops;
        let mut i = 0;
        let mut oi = 0;
        while i < o.len() {
            let kind = o[i] as i64;
            match kind {
                2 => {
                    stub.reset();
                    i += 1;
                }
                1 => {
                    st.invalidate();
                    i += 1;
                }
                4 => {
                    let count = o[i + 1] as usize;
                    let mut j = i + 2;
                    for _ in 0..count {
                        if o[j] as i64 == 0 {
                            stub.push_null();
                            j += 1;
                        } else {
                            let l = o[j + 1] as usize;
                            stub.push_list(o[j + 2..j + 2 + l].to_vec());
                            j += 2 + l;
                        }
                    }
                    i = j;
                }
                0 => {
                    let (from, to, dist, status, node) = (o[i + 1], o[i + 2], o[i + 3], o[i + 4] as i64, o[i + 5]);
                    let mut j = i + 6;
                    let pan_len = o[j];
                    let pan: Option<Vec<f64>> = if pan_len < 0.0 {
                        j += 1;
                        None
                    } else {
                        let l = pan_len as usize;
                        let t = o[j + 1..j + 1 + l].to_vec();
                        j += 1 + l;
                        Some(t)
                    };
                    let idx = o[j] as usize;
                    let has_path = o[j + 1] as i64;
                    let calls = o[j + 2];
                    j += 3;

                    let got = st.next(from, to, if dist < 0.0 { None } else { Some(dist) });
                    match (&got, status) {
                        (PathResult::Next { node: n, .. }, 0) => {
                            assert_eq!(*n, node, "{} o{} next node", s.name, oi)
                        }
                        (PathResult::Complete { node: n, .. }, 2) => {
                            assert_eq!(*n, node, "{} o{} complete node", s.name, oi)
                        }
                        (PathResult::NotFound { .. }, 3) => {}
                        (r, want) => panic!("{} o{} status: got {r:?} want {want}", s.name, oi),
                    }
                    assert_eq!(
                        st.path_after_next(),
                        pan,
                        "{} o{} pathAfterNext",
                        s.name,
                        oi
                    );
                    assert_eq!(st.debug_path_index(), idx, "{} o{} pathIndex", s.name, oi);
                    assert_eq!(
                        st.debug_has_path() as i64,
                        has_path,
                        "{} o{} hasPath",
                        s.name,
                        oi
                    );
                    assert_eq!(stub.calls() as f64, calls, "{} o{} calls", s.name, oi);
                    i = j;
                }
                3 => {
                    let is_multi = o[i + 1] as i64;
                    let fl = o[i + 2] as usize;
                    let fs = &o[i + 3..i + 3 + fl];
                    let mut j = i + 3 + fl;
                    let to = o[j];
                    j += 1;
                    let out_mode = o[j] as i64;
                    j += 1;
                    let out_len = o[j] as usize;
                    let out: Option<Vec<f64>> = if out_mode == 2 {
                        Some(o[j + 1..j + 1 + out_len].to_vec())
                    } else {
                        None
                    };
                    j += 1 + out_len;
                    let seen_flag = o[j] as i64;
                    j += 1;
                    let mut seen: Option<(i64, Vec<f64>, f64)> = None;
                    if seen_flag == 1 {
                        let m = o[j] as i64;
                        let l = o[j + 1] as usize;
                        let tiles = o[j + 2..j + 2 + l].to_vec();
                        let goal = o[j + 2 + l];
                        seen = Some((m, tiles, goal));
                        j += 2 + l + 1;
                    }
                    let calls = o[j];
                    j += 1;

                    let starts = if is_multi == 1 {
                        PathStart::Multi(fs)
                    } else {
                        PathStart::Single(fs[0])
                    };
                    let got = st.find_path(starts, to);
                    assert_eq!(got, out, "{} o{} findPath out", s.name, oi);
                    match (&seen, stub.last_seen()) {
                        (None, None) => {}
                        (Some((m, tiles, goal)), Some((im, itiles, igoal))) => {
                            assert_eq!(im as i64, *m, "{} o{} seen kind", s.name, oi);
                            assert_eq!(itiles, *tiles, "{} o{} seen tiles", s.name, oi);
                            assert_eq!(igoal, *goal, "{} o{} seen goal", s.name, oi);
                        }
                        (a, b) => panic!(
                            "{} o{} seen mismatch: recorded {:?}, rust {:?}",
                            s.name,
                            oi,
                            a.is_some(),
                            b.is_some()
                        ),
                    }
                    assert_eq!(stub.calls() as f64, calls, "{} o{} calls", s.name, oi);
                    i = j;
                }
                other => panic!("{} unknown op kind {other}", s.name),
            }
            oi += 1;
        }
    }
}

// --- ComponentCheckTransformer (ComponentCheckTransformer.ts) -----------------
// Replays each query group against the Rust transformer over the shared
// `ScriptedFinder` inner stub + a `TableGetter` component table. Pins: the
// `PathStart` kind/tiles/goal `inner` received (filtering, order, and the
// single-survivor scalar collapse), the delegated result, and "inner never
// called" (seen_flag 0: component mismatch or vacuous empty start).
#[test]
fn replay_component_check_transformer_scenarios() {
    use openfront_core::pathfinding::component_check_transformer::{
        ComponentCheckTransformer, TableGetter,
    };
    use openfront_core::pathfinding::mini_map_transformer::ScriptedFinder;
    use openfront_core::pathfinding::PathStart;
    for s in vectors::COMPONENT_CHECK_SCENARIOS {
        let table: Vec<(f64, i64)> = s
            .table
            .chunks(2)
            .map(|c| (c[0], c[1] as i64))
            .collect();
        let g = s.groups;
        let mut k = 0;
        let mut qi = 0;
        while k < g.len() {
            // [is_multi, from_len, from_refs..., to]
            let is_multi = g[k] as i64;
            let from_len = g[k + 1] as usize;
            let from_refs = &g[k + 2..k + 2 + from_len];
            let to = g[k + 2 + from_len];
            k += 3 + from_len;
            // [inner_mode, inner_len, inner_refs...]
            let inner_mode = g[k] as i64;
            let inner_len = g[k + 1] as usize;
            let inner_refs: Vec<f64> = g[k + 2..k + 2 + inner_len].to_vec();
            k += 2 + inner_len;
            // [seen_flag, [seen_multi, seen_len, seen_refs..., seen_goal]]
            let seen_flag = g[k] as i64;
            let mut seen: Option<(i64, Vec<f64>, f64)> = None;
            if seen_flag == 1 {
                let m = g[k + 1] as i64;
                let l = g[k + 2] as usize;
                let tiles = g[k + 3..k + 3 + l].to_vec();
                let goal = g[k + 3 + l];
                seen = Some((m, tiles, goal));
                k += 3 + l + 1;
            } else {
                k += 1;
            }

            let mut getter = TableGetter::default();
            getter.set_table(table.clone(), s.default as i64);
            let mut stub = ScriptedFinder::default();
            stub.push_path(match inner_mode {
                0 => None,
                _ => Some(inner_refs.clone()),
            });
            let starts = if is_multi == 1 {
                PathStart::Multi(from_refs)
            } else {
                PathStart::Single(from_refs[0])
            };
            let got = {
                let mut tr = ComponentCheckTransformer::new(&mut stub, getter);
                tr.find_path(starts, to)
            };
            // Output is the pass-through of inner's result: null when inner
            // was never called or returned null, else the inner list.
            let want: Option<Vec<f64>> = if seen_flag == 0 || inner_mode == 0 {
                None
            } else {
                Some(inner_refs.clone())
            };
            assert_eq!(got, want, "{} q{} output", s.name, qi);
            match (&seen, stub.last_seen.as_ref()) {
                (None, None) => {}
                (Some((m, tiles, goal)), Some((im, itiles, igoal))) => {
                    assert_eq!(*im as i64, *m, "{} q{} seen kind", s.name, qi);
                    assert_eq!(itiles, tiles, "{} q{} seen tiles", s.name, qi);
                    assert_eq!(igoal, goal, "{} q{} seen goal", s.name, qi);
                }
                (a, b) => panic!(
                    "{} q{} seen mismatch: recorded {:?}, rust {:?}",
                    s.name,
                    qi,
                    a.is_some(),
                    b.is_some()
                ),
            }
            qi += 1;
        }
    }
}

// --- ShoreCoercingTransformer (ShoreCoercingTransformer.ts) -------------------
// Replays each query group against the Rust transformer over a hand-built
// land/water map + `ScriptedFinder`. Pins: the coerced water starts/goal
// passed to `inner` (scalar collapse, duplicate starts from the raw-water
// delete), the restored/extended output path (start unshift, goal append,
// the `!== originalTo` guard), and every `null` short-circuit (no water
// starts, goal with no water neighbor, inner null/empty).
#[test]
fn replay_shore_coercing_transformer_scenarios() {
    use openfront_core::pathfinding::mini_map_transformer::ScriptedFinder;
    use openfront_core::pathfinding::shore_coercing_transformer::ShoreCoercingTransformer;
    use openfront_core::pathfinding::PathStart;
    for s in vectors::SHORE_COERCING_SCENARIOS {
        let w = s.w as usize;
        let h = s.h as usize;
        let mut data = vec![0x83u8; w * h];
        for pair in s.water.chunks(2) {
            data[pair[1] as usize * w + pair[0] as usize] = 0x03;
        }
        let gm = GameMap::new(s.w, s.h, data, (w * h - s.water.len() / 2) as f64);
        let g = s.groups;
        let mut k = 0;
        let mut qi = 0;
        while k < g.len() {
            // [is_multi, from_len, from_refs..., to]
            let is_multi = g[k] as i64;
            let from_len = g[k + 1] as usize;
            let from_refs = &g[k + 2..k + 2 + from_len];
            let to = g[k + 2 + from_len];
            k += 3 + from_len;
            // [inner_mode, inner_len, inner_refs...]
            let inner_mode = g[k] as i64;
            let inner_len = g[k + 1] as usize;
            let inner_refs: Vec<f64> = g[k + 2..k + 2 + inner_len].to_vec();
            k += 2 + inner_len;
            // [seen_flag, [seen_multi, seen_len, seen_refs..., seen_goal]]
            let seen_flag = g[k] as i64;
            let mut seen: Option<(i64, Vec<f64>, f64)> = None;
            if seen_flag == 1 {
                let m = g[k + 1] as i64;
                let l = g[k + 2] as usize;
                let tiles = g[k + 3..k + 3 + l].to_vec();
                let goal = g[k + 3 + l];
                seen = Some((m, tiles, goal));
                k += 3 + l + 1;
            } else {
                k += 1;
            }
            // [out_mode, [out_len, out_refs...]]
            let out_mode = g[k] as i64;
            k += 1;
            let out_tiles: Vec<f64> = if out_mode == 2 {
                let l = g[k] as usize;
                let t = g[k + 1..k + 1 + l].to_vec();
                k += 1 + l;
                t
            } else {
                Vec::new()
            };

            let mut stub = ScriptedFinder::default();
            stub.push_path(match inner_mode {
                0 => None,
                1 => Some(vec![]),
                _ => Some(inner_refs),
            });
            let starts = if is_multi == 1 {
                PathStart::Multi(from_refs)
            } else {
                PathStart::Single(from_refs[0])
            };
            let got = {
                let mut tr = ShoreCoercingTransformer::new(&mut stub, &gm);
                tr.find_path(starts, to)
            };
            match out_mode {
                0 => assert_eq!(got, None, "{} q{} null", s.name, qi),
                2 => {
                    let p = got.expect("unexpected null");
                    assert_eq!(p, out_tiles, "{} q{} path", s.name, qi);
                }
                other => panic!("{} unknown out_mode {other}", s.name),
            }
            match (&seen, stub.last_seen.as_ref()) {
                (None, None) => {}
                (Some((m, tiles, goal)), Some((im, itiles, igoal))) => {
                    assert_eq!(*im as i64, *m, "{} q{} seen kind", s.name, qi);
                    assert_eq!(itiles, tiles, "{} q{} seen tiles", s.name, qi);
                    assert_eq!(igoal, goal, "{} q{} seen goal", s.name, qi);
                }
                (a, b) => panic!(
                    "{} q{} seen mismatch: recorded {:?}, rust {:?}",
                    s.name,
                    qi,
                    a.is_some(),
                    b.is_some()
                ),
            }
            qi += 1;
        }
    }
}

// SmoothingWaterTransformer: replays each recorded query against the Rust
// port over the same hand-built water map. The groups pin the inner stub's
// observation (the start/goal union forwarded untouched), the LOS collapse
// + Bresenham trace splice, the endpoint refinement through the local
// bounded A* (hit and miss), and the pass-3 magnitude gate.
#[test]
fn replay_smoothing_water_transformer_scenarios() {
    use openfront_core::pathfinding::mini_map_transformer::ScriptedFinder;
    use openfront_core::pathfinding::smoothing_water_transformer::{
        SmoothingWaterTransformer, WaterTraversable,
    };
    use openfront_core::pathfinding::PathStart;
    for s in vectors::SMOOTHING_WATER_SCENARIOS {
        let w = s.w as usize;
        let h = s.h as usize;
        let mut data = vec![0x83u8; w * h];
        for t in s.cells.chunks(3) {
            data[t[1] as usize * w + t[0] as usize] = t[2] as u8;
        }
        let land = data.iter().filter(|b| *b & 0x80 != 0).count() as f64;
        let gm = GameMap::new(s.w, s.h, data, land);
        let g = s.groups;
        let mut k = 0;
        let mut qi = 0;
        while k < g.len() {
            // [is_multi, from_len, from_refs..., to]
            let is_multi = g[k] as i64;
            let from_len = g[k + 1] as usize;
            let from_refs = &g[k + 2..k + 2 + from_len];
            let to = g[k + 2 + from_len];
            k += 3 + from_len;
            // [inner_mode, inner_len, inner_refs...]
            let inner_mode = g[k] as i64;
            let inner_len = g[k + 1] as usize;
            let inner_refs: Vec<f64> = g[k + 2..k + 2 + inner_len].to_vec();
            k += 2 + inner_len;
            // [seen_flag, [seen_multi, seen_len, seen_refs..., seen_goal]]
            let seen_flag = g[k] as i64;
            let mut seen: Option<(i64, Vec<f64>, f64)> = None;
            if seen_flag == 1 {
                let m = g[k + 1] as i64;
                let l = g[k + 2] as usize;
                let tiles = g[k + 3..k + 3 + l].to_vec();
                let goal = g[k + 3 + l];
                seen = Some((m, tiles, goal));
                k += 3 + l + 1;
            } else {
                k += 1;
            }
            // [out_mode, [out_len, out_refs...]]
            let out_mode = g[k] as i64;
            k += 1;
            let out_tiles: Vec<f64> = if out_mode == 2 {
                let l = g[k] as usize;
                let t = g[k + 1..k + 1 + l].to_vec();
                k += 1 + l;
                t
            } else {
                Vec::new()
            };

            let mut stub = ScriptedFinder::default();
            stub.push_path(match inner_mode {
                0 => None,
                1 => Some(vec![]),
                _ => Some(inner_refs),
            });
            let starts = if is_multi == 1 {
                PathStart::Multi(from_refs)
            } else {
                PathStart::Single(from_refs[0])
            };
            let got = {
                let mut tr =
                    SmoothingWaterTransformer::new(&mut stub, &gm, WaterTraversable(&gm));
                tr.find_path(starts, to)
            };
            match out_mode {
                0 => assert_eq!(got, None, "{} q{} null", s.name, qi),
                2 => {
                    let p = got.expect("unexpected null");
                    assert_eq!(p, out_tiles, "{} q{} path", s.name, qi);
                }
                other => panic!("{} unknown out_mode {other}", s.name),
            }
            match (&seen, stub.last_seen.as_ref()) {
                (None, None) => {}
                (Some((m, tiles, goal)), Some((im, itiles, igoal))) => {
                    assert_eq!(*im as i64, *m, "{} q{} seen kind", s.name, qi);
                    assert_eq!(itiles, tiles, "{} q{} seen tiles", s.name, qi);
                    assert_eq!(igoal, goal, "{} q{} seen goal", s.name, qi);
                }
                (a, b) => panic!(
                    "{} q{} seen mismatch: recorded {:?}, rust {:?}",
                    s.name,
                    qi,
                    a.is_some(),
                    b.is_some()
                ),
            }
            qi += 1;
        }
    }
}

// Generic BFS (BFS.ts): replays the recorded (node, dist) visitor stream and
// the search return. The edge table is keyed with SameValueZero (NaN keys are
// reachable), mirroring the capture runner's `Map`.
struct TableAdapter {
    keys: Vec<f64>,
    nbrs: Vec<Vec<f64>>,
}

impl BfsAdapter for TableAdapter {
    fn neighbors(&mut self, node: f64) -> Vec<f64> {
        for (i, &k) in self.keys.iter().enumerate() {
            if k == node || (k.is_nan() && node.is_nan()) {
                return self.nbrs[i].clone();
            }
        }
        Vec::new()
    }
}

fn same_f64(a: f64, b: f64) -> bool {
    a == b || (a.is_nan() && b.is_nan())
}

#[test]
fn replay_bfs_ts_scenarios() {
    use openfront_core::pathfinding::bfs::Bfs as BfsTs;
    use openfront_core::pathfinding::PathStart;
    for s in vectors::BFS_TS_SCENARIOS {
        // Rebuild the edge table from the flat encoding.
        let mut keys = Vec::new();
        let mut nbrs = Vec::new();
        let mut p = 0usize;
        for &deg in s.edge_degrees {
            keys.push(s.edges[p]);
            p += 1;
            nbrs.push(s.edges[p..p + deg].to_vec());
            p += deg;
        }
        assert_eq!(p, s.edges.len(), "{} edge table length", s.name);

        let mut bfs = BfsTs::new(TableAdapter { keys, nbrs });
        let mut visits = Vec::new();
        let got = bfs.search(
            PathStart::Multi(s.starts),
            s.max_d,
            |n, d| -> Visit<f64> {
                visits.push((n, d));
                if s.mode == 1 && same_f64(n, s.blocker) {
                    Visit::Reject
                } else if s.mode == 2 && same_f64(n, s.blocker) {
                    Visit::Found(s.foundval)
                } else {
                    Visit::Explore
                }
            },
        );

        assert_eq!(visits.len(), s.visits.len() / 2, "{} visit count", s.name);
        for (i, &(n, d)) in visits.iter().enumerate() {
            assert!(
                same_f64(n, s.visits[2 * i]),
                "{} visit[{i}] node got {n} want {}",
                s.name,
                s.visits[2 * i]
            );
            assert!(
                same_f64(d, s.visits[2 * i + 1]),
                "{} visit[{i}] dist got {d} want {}",
                s.name,
                s.visits[2 * i + 1]
            );
        }
        match (s.has_result, got) {
            (false, None) => {}
            (true, Some(v)) => assert!(same_f64(v, s.result), "{} result", s.name),
            (want, got) => panic!("{} result mismatch: want {want}, got {got:?}", s.name),
        }
    }
}

// AirPathFinder (PathFinder.Air.ts): replays the walk and compares the (x, y)
// coordinate stream bit-exactly. Throw scenarios (multi-start, out-of-range
// game.ref) are pinned via catch_unwind on the panicking path.
#[test]
fn replay_air_scenarios() {
    use openfront_core::pathfinding::air::AirPathFinder;
    use openfront_core::pathfinding::PathStart;
    for s in vectors::AIR_SCENARIOS {
        let gm = GameMap::new(s.w, s.h, vec![0x83u8; (s.w * s.h) as usize], s.w * s.h);
        let pf = AirPathFinder::new(&gm, s.ticks);
        if s.threw {
            let start = if s.multi {
                PathStart::Multi(&[s.from])
            } else {
                PathStart::Single(s.from)
            };
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                pf.find_path(start, s.to)
            }));
            assert!(r.is_err(), "{} expected throw", s.name);
            continue;
        }
        let refs = pf.find_path(PathStart::Single(s.from), s.to);
        let coords: Vec<f64> = refs.iter().flat_map(|&t| [gm.x(t), gm.y(t)]).collect();
        assert_eq!(coords.len(), s.path.len(), "{} path length", s.name);
        for (i, (&got, &want)) in coords.iter().zip(s.path.iter()).enumerate() {
            assert_eq!(got, want, "{} path[{i}]", s.name);
        }
    }
}

// anonWordName (AnonNames.ts): replay every scenario and compare the returned
// handle exactly (the bank order, trunc/abs/floor JS-isms, the "undefined"
// property-miss spelling and the round === 0 undefined return are all
// observable).
#[test]
fn replay_anon_scenarios() {
    use openfront_core::anon_names::anon_word_name;
    for s in vectors::ANON_SCENARIOS {
        let offset = if s.has == 1 { Some(s.offset) } else { None };
        let got = anon_word_name(s.slot, offset);
        assert_eq!(got.as_deref(), s.res, "{} handle", s.name);
    }
}

// CloseCodes.ts: replay both predicates over every declared value plus the
// boundary / non-finite / case-mismatch edges and compare the verdicts.
#[test]
fn replay_close_scenarios() {
    use openfront_core::close_codes::{is_close_reason, is_terminal_close};
    for s in vectors::CLOSE_SCENARIOS {
        let got = match s.kind {
            0 => is_terminal_close(s.code),
            _ => is_close_reason(s.val),
        };
        assert_eq!(got, s.res, "{} verdict", s.name);
    }
}

// ServerList.ts: replay every pure function through the shared `run_op`
// runner (the zod schemas are not ported) and compare the flat token streams.
#[test]
fn replay_serverlist_scenarios() {
    use openfront_core::server_list::run_op;
    for s in vectors::SL_SCENARIOS {
        let got = run_op(s.kind, s.args);
        assert_eq!(got.len(), s.res.len(), "{} res len: got {got:?} want {:?}", s.name, s.res);
        for (j, (g, w)) in got.iter().zip(s.res.iter()).enumerate() {
            assert!(obj_is(*g, *w), "{} res[{j}]: got {g} want {w}", s.name);
        }
    }
}

// AssetUrls.ts: replay normalizeAssetPath / encodeAssetPath / buildAssetUrl
// through the shared `run_op` runner and compare the flat token streams
// (strings as [len, u0, ..], throws as [1]).
#[test]
fn replay_asseturls_scenarios() {
    use openfront_core::asset_urls::run_op;
    for s in vectors::AU_SCENARIOS {
        let got = run_op(s.kind, s.args);
        assert_eq!(got.len(), s.res.len(), "{} res len: got {got:?} want {:?}", s.name, s.res);
        for (j, (g, w)) in got.iter().zip(s.res.iter()).enumerate() {
            assert!(obj_is(*g, *w), "{} res[{j}]: got {g} want {w}", s.name);
        }
    }
}

// Maps.gen.ts: replay the data-table dumps / id lookup through the shared
// `run_op` runner and compare the flat token streams (the Rust table must
// serialise to the exact golden stream captured from the TS source).
#[test]
fn replay_maps_scenarios() {
    use openfront_core::maps_gen::run_op;
    for s in vectors::MG_SCENARIOS {
        let got = run_op(s.kind, s.args);
        assert_eq!(got.len(), s.res.len(), "{} res len", s.name);
        for (j, (g, w)) in got.iter().zip(s.res.iter()).enumerate() {
            assert!(obj_is(*g, *w), "{} res[{j}]: got {g} want {w}", s.name);
        }
    }
}

// TribeNames.ts: replay resolveTribeNameData through the shared `run_op`
// runner (the capture's theme-record mutations ride in as removed/blanked
// name lists) and compare the flat token streams.
#[test]
fn replay_tribenames_scenarios() {
    use openfront_core::tribe_names::run_op;
    for s in vectors::TN_SCENARIOS {
        let got = run_op(s.kind, s.args);
        assert_eq!(got.len(), s.res.len(), "{} res len", s.name);
        for (j, (g, w)) in got.iter().zip(s.res.iter()).enumerate() {
            assert!(obj_is(*g, *w), "{} res[{j}]: got {g} want {w}", s.name);
        }
    }
}

// game/Game.ts: replay the runtime-value subset (enum dumps, group has()
// matrices, guards, Cell/PlayerInfo, bulk-cost math, consts) through the
// shared `run_op` runner and compare the flat token streams.
#[test]
fn replay_game_scenarios() {
    use openfront_core::game_ts::run_op;
    for s in vectors::GAME_SCENARIOS {
        let got = run_op(s.kind, s.args);
        assert_eq!(got.len(), s.res.len(), "{} res len: got {got:?} want {:?}", s.name, s.res);
        for (j, (g, w)) in got.iter().zip(s.res.iter()).enumerate() {
            assert!(obj_is(*g, *w), "{} res[{j}]: got {g} want {w}", s.name);
        }
    }
}

// game/NationCreation.ts: replay the tables dump / pluralize / name
// generation / createRandomNations through the shared `run_op` runner and
// compare the flat token streams.
#[test]
fn replay_nationcreation_scenarios() {
    use openfront_core::nation_creation::run_op;
    for s in vectors::NC_SCENARIOS {
        let got = run_op(s.kind, s.args);
        assert_eq!(got.len(), s.res.len(), "{} res len: got {got:?} want {:?}", s.name, s.res);
        for (j, (g, w)) in got.iter().zip(s.res.iter()).enumerate() {
            assert!(obj_is(*g, *w), "{} res[{j}]: got {g} want {w}", s.name);
        }
    }
}

// game/GameUpdates.ts: replay the GameUpdateType dump / name lookup through
// the shared `run_op` runner and compare the flat token streams.
#[test]
fn replay_gameupdates_scenarios() {
    use openfront_core::game_updates::run_op;
    for s in vectors::GUPD_SCENARIOS {
        let got = run_op(s.kind, s.args);
        assert_eq!(got.len(), s.res.len(), "{} res len: got {got:?} want {:?}", s.name, s.res);
        for (j, (g, w)) in got.iter().zip(s.res.iter()).enumerate() {
            assert!(obj_is(*g, *w), "{} res[{j}]: got {g} want {w}", s.name);
        }
    }
}

// Util.ts emojiTable + NationEmojiBehavior.ts EMOJI_*: replay the table
// dumps / id-array dump / emoji_id batches through the shared `run_op`
// runner and compare the flat token streams (UTF-16 code-unit exact).
#[test]
fn replay_nationemoji_scenarios() {
    use openfront_core::nation_emoji::run_op;
    for s in vectors::NE_SCENARIOS {
        let got = run_op(s.kind, s.args);
        assert_eq!(got.len(), s.res.len(), "{} res len: got {got:?} want {:?}", s.name, s.res);
        for (j, (g, w)) in got.iter().zip(s.res.iter()).enumerate() {
            assert!(obj_is(*g, *w), "{} res[{j}]: got {g} want {w}", s.name);
        }
    }
}

// CosmeticSchemas.ts: replay the effect-type array dumps / DefaultPattern /
// the four pure effect-slot function batches through the shared `run_op`
// runner and compare the flat token streams (UTF-16 code-unit exact).
#[test]
fn replay_cosmeticschemas_scenarios() {
    use openfront_core::cosmetic_schemas::run_op;
    for s in vectors::CS_SCENARIOS {
        let got = run_op(s.kind, s.args);
        assert_eq!(got.len(), s.res.len(), "{} res len: got {got:?} want {:?}", s.name, s.res);
        for (j, (g, w)) in got.iter().zip(s.res.iter()).enumerate() {
            assert!(obj_is(*g, *w), "{} res[{j}]: got {g} want {w}", s.name);
        }
    }
}

// StatsSchemas.ts: replay the unit-name array dumps / lookup-table dumps /
// the 34 index-constant dump / the toBigInt coercion batches through the
// shared `run_op` runner and compare the flat token streams (UTF-16
// code-unit exact).
#[test]
fn replay_statschemas_scenarios() {
    use openfront_core::stats_schemas::run_op;
    for s in vectors::SS_SCENARIOS {
        let got = run_op(s.kind, s.args);
        assert_eq!(got.len(), s.res.len(), "{} res len: got {got:?} want {:?}", s.name, s.res);
        for (j, (g, w)) in got.iter().zip(s.res.iter()).enumerate() {
            assert!(obj_is(*g, *w), "{} res[{j}]: got {g} want {w}", s.name);
        }
    }
}

// Schemas.ts: replay the enum option-array dumps / the lobby constants / the
// LogSeverity table / the QuickChat key dump / the isValidGameID and
// renderable-name regex batches through the shared `run_op` runner and
// compare the flat token streams (UTF-16 code-unit exact).
#[test]
fn replay_schemas_scenarios() {
    use openfront_core::schemas::run_op;
    for s in vectors::SC_SCENARIOS {
        let got = run_op(s.kind, s.args);
        assert_eq!(got.len(), s.res.len(), "{} res len: got {got:?} want {:?}", s.name, s.res);
        for (j, (g, w)) in got.iter().zip(s.res.iter()).enumerate() {
            assert!(obj_is(*g, *w), "{} res[{j}]: got {g} want {w}", s.name);
        }
    }
}

// pathfinding/PathFinder.ts WaterPathMemo: replay the scripted-inner memo
// scenarios (miss / hit / null accounting / LRU re-insert / waterVersion
// clears / array-from passthrough / over-budget eviction / key collision /
// Uint32 coercion) through the shared `run_op` runner and compare the flat
// token streams.
#[test]
fn replay_waterpathmemo_scenarios() {
    use openfront_core::water_path_memo::run_op;
    for s in vectors::WPM_SCENARIOS {
        let got = run_op(s.kind, s.args);
        assert_eq!(got.len(), s.res.len(), "{} res len: got {got:?} want {:?}", s.name, s.res);
        for (j, (g, w)) in got.iter().zip(s.res.iter()).enumerate() {
            assert!(obj_is(*g, *w), "{} res[{j}]: got {g} want {w}", s.name);
        }
    }
}

// GameUpdateUtils.ts: replay diff / apply / pack through the shared `run_op`
// runner and compare the flat token streams (reference identity rides in as
// the capture's refid, NaN/-0 through obj_is).
#[test]
fn replay_gameupdateutils_scenarios() {
    use openfront_core::game_update_utils::run_op;
    for s in vectors::GU_SCENARIOS {
        let got = run_op(s.kind, s.args);
        assert_eq!(got.len(), s.res.len(), "{} res len: got {got:?} want {:?}", s.name, s.res);
        for (j, (g, w)) in got.iter().zip(s.res.iter()).enumerate() {
            assert!(obj_is(*g, *w), "{} res[{j}]: got {g} want {w}", s.name);
        }
    }
}

// Railroad.ts: replay closest-tile-index / oriented-railroad / delete through
// the shared `run_op` runner and compare the flat token streams (station and
// railroad identity ride in as the capture's refid, NaN/±Inf through obj_is).
#[test]
fn replay_railroad_scenarios() {
    use openfront_core::railroad::run_op;
    for s in vectors::RR_SCENARIOS {
        let got = run_op(s.kind, s.args);
        assert_eq!(got.len(), s.res.len(), "{} res len: got {got:?} want {:?}", s.name, s.res);
        for (j, (g, w)) in got.iter().zip(s.res.iter()).enumerate() {
            assert!(obj_is(*g, *w), "{} res[{j}]: got {g} want {w}", s.name);
        }
    }
}

// RailroadSpatialGrid.ts: replay the stateful op stream (construct / register /
// unregister / query / dumps) through `RigHarness::run_op` and compare every
// op's flat token stream.
#[test]
fn replay_railgrid_scenarios() {
    use openfront_core::railroad_spatial_grid::RigHarness;
    for s in vectors::RSG_SCENARIOS {
        let mut rig = RigHarness::new();
        for op in s.ops {
            let got = rig.run_op(op.kind, op.args);
            assert_eq!(
                got.len(),
                op.res.len(),
                "{} op[{:?}] res len: got {got:?} want {:?}",
                s.name,
                op.kind,
                op.res
            );
            for (j, (g, w)) in got.iter().zip(op.res.iter()).enumerate() {
                assert!(
                    obj_is(*g, *w),
                    "{} op[{:?}] res[{j}]: got {g} want {w}",
                    s.name,
                    op.kind
                );
            }
        }
    }
}

// TileTraversalScratch.ts: replay the stateful op stream (allocate / bump /
// typed-array writes / stack push) through `RigHarness::run_op` and compare
// every op's flat token stream.
#[test]
fn replay_tiletravscratch_scenarios() {
    use openfront_core::tile_traversal_scratch::RigHarness;
    for s in vectors::TTS_SCENARIOS {
        let mut rig = RigHarness::new();
        for op in s.ops {
            let got = rig.run_op(op.kind, op.args);
            assert_eq!(
                got.len(),
                op.res.len(),
                "{} op[{:?}] res len: got {got:?} want {:?}",
                s.name,
                op.kind,
                op.res
            );
            for (j, (g, w)) in got.iter().zip(op.res.iter()).enumerate() {
                assert!(
                    obj_is(*g, *w),
                    "{} op[{:?}] res[{j}]: got {g} want {w}",
                    s.name,
                    op.kind
                );
            }
        }
    }
}

// EventBus.ts: replay the stateful op stream (on / off / emit call trace /
// Map-order dump) through `RigHarness::run_op` and compare every op's flat
// token stream (constructors, callbacks and events cross as capture refids).
#[test]
fn replay_eventbus_scenarios() {
    use openfront_core::event_bus::RigHarness;
    for s in vectors::EB_SCENARIOS {
        let mut rig = RigHarness::new();
        for op in s.ops {
            let got = rig.run_op(op.kind, op.args);
            assert_eq!(
                got.len(),
                op.res.len(),
                "{} op[{:?}] res len: got {got:?} want {:?}",
                s.name,
                op.kind,
                op.res
            );
            for (j, (g, w)) in got.iter().zip(op.res.iter()).enumerate() {
                assert!(
                    obj_is(*g, *w),
                    "{} op[{:?}] res[{j}]: got {g} want {w}",
                    s.name,
                    op.kind
                );
            }
        }
    }
}

// PatternDecoder.ts: replay decode + isPrimary through the shared `run_op`
// runner (throws recorded as numeric codes) and compare the token streams.
#[test]
fn replay_patterndecoder_scenarios() {
    use openfront_core::pattern_decoder::run_op;
    for s in vectors::PD_SCENARIOS {
        let got = run_op(s.kind, s.args);
        assert_eq!(got.len(), s.res.len(), "{} res len: got {got:?} want {:?}", s.name, s.res);
        for (j, (g, w)) in got.iter().zip(s.res.iter()).enumerate() {
            assert!(obj_is(*g, *w), "{} res[{j}]: got {g} want {w}", s.name);
        }
    }
}

// DoomsdayClock.ts: replay the wave math through the shared `run_op` runner
// and compare the flat token streams.
#[test]
fn replay_doomsdayclock_scenarios() {
    use openfront_core::doomsday_clock::run_op;
    for s in vectors::DC_SCENARIOS {
        let got = run_op(s.kind, s.args);
        assert_eq!(got.len(), s.res.len(), "{} res len: got {got:?} want {:?}", s.name, s.res);
        for (j, (g, w)) in got.iter().zip(s.res.iter()).enumerate() {
            assert!(obj_is(*g, *w), "{} res[{j}]: got {g} want {w}", s.name);
        }
    }
}

// execution/Util.ts: rebuild the packed-terrain GameMap + owner writes, run
// one `exec_util::run_op` per scenario and compare the flat token streams.
#[test]
fn replay_executil_scenarios() {
    use openfront_core::exec_util::run_op;
    for s in vectors::EU_SCENARIOS {
        let mut gm = GameMap::new(s.w, s.h, s.terrain.to_vec(), s.w * s.h);
        for &(t, id) in s.owners {
            gm.set_owner_id(t, id);
        }
        let got = run_op(&gm, s.kind, s.args);
        assert_eq!(got.len(), s.res.len(), "{} res len: got {got:?} want {:?}", s.name, s.res);
        for (j, (g, w)) in got.iter().zip(s.res.iter()).enumerate() {
            assert!(obj_is(*g, *w), "{} res[{j}]: got {g} want {w}", s.name);
        }
    }
}

// game/WaterManager.ts: rebuild the two packed GameMaps, replay the op stream
// through `WaterManager::run_op`, compare every per-op result stream, then the
// final terrain/state buffers of both maps and the graph version.
#[test]
fn replay_watermanager_scenarios() {
    use openfront_core::water_manager::WaterManager;
    for s in vectors::WM_SCENARIOS {
        let map = GameMap::new(s.mw, s.mh, s.map_terrain.to_vec(), s.mw * s.mh);
        let mini = GameMap::new(s.nw, s.nh, s.mini_terrain.to_vec(), s.nw * s.nh);
        {
            let mut wm = WaterManager::new(map, mini, s.disable);
            for op in s.ops {
                let args = [op.a, op.b];
                let got = wm.run_op(op.kind, &args);
                assert_eq!(
                    got.len(),
                    op.res.len(),
                    "{} op[{:?}] res len: got {got:?} want {:?}",
                    s.name,
                    op.kind,
                    op.res
                );
                for (j, (g, w)) in got.iter().zip(op.res.iter()).enumerate() {
                    assert!(
                        obj_is(*g, *w),
                        "{} op[{:?}] res[{j}]: got {g} want {w}",
                        s.name,
                        op.kind
                    );
                }
            }
            assert!(
                obj_is(wm.water_graph_version(), s.version_after),
                "{} version: got {} want {}",
                s.name,
                wm.water_graph_version(),
                s.version_after
            );
            assert_eq!(
                wm.debug_map_terrain(),
                s.map_terrain_after,
                "{} map terrain after",
                s.name
            );
            assert_eq!(
                wm.debug_map_state(),
                s.map_state_after,
                "{} map state after",
                s.name
            );
            assert_eq!(
                wm.debug_mini_terrain(),
                s.mini_terrain_after,
                "{} mini terrain after",
                s.name
            );
        }
    }
}
