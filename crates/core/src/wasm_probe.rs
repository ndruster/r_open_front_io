//! Minimal `extern "C"` probe surface, compiled only with `--features wasm-probe`.
//!
//! Purpose: on a machine where the MSVC linker is unavailable (no Windows SDK
//! import libs), the parity vectors can still be *executed* — build this crate
//! to `wasm32-unknown-unknown`, then call it from Node (see
//! `rust/tools/run_wasm_parity.mjs`) and diff against `data/vectors.rs`.
//!
//! Only scalar f64/u32/i64/u8 values cross the boundary, so the host needs no
//! shared-memory layout knowledge and the probe cannot disagree with the Rust
//! API the real tests use.
//!
//! State is seeded explicitly via `probe_prng_seed`; the stepping functions
//! advance a running generator. `probe_prng_shuffle_reset` + `..._at_last`
//! replay a fresh shuffle from the post-seed snapshot, mirroring how the
//! vectors group each shuffle under its own freshly seeded generator.

use crate::detmath;
use crate::pseudo_random::PseudoRandom;

thread_local! {
    static RNG: std::cell::Cell<Option<PseudoRandom>> = const { std::cell::Cell::new(None) };
    /// Snapshot taken right after the last seed, used for shuffle/correlation
    /// probes that must each start from a freshly seeded generator.
    static SHUFFLE_BASE: std::cell::Cell<Option<PseudoRandom>> = const { std::cell::Cell::new(None) };
    static SHUFFLE_BUF: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn with_rng<T>(f: impl FnOnce(&mut PseudoRandom) -> T) -> T {
    RNG.with(|slot| {
        let mut cur = slot.take().expect("probe_prng_seed must be called first");
        let out = f(&mut cur);
        slot.set(Some(cur));
        out
    })
}

/// Re-seeds the probe generator and snapshots the fresh state.
#[no_mangle]
pub extern "C" fn probe_prng_seed(seed: f64) {
    let rng = PseudoRandom::new(seed);
    SHUFFLE_BASE.with(|slot| slot.set(Some(rng.clone())));
    RNG.with(|slot| slot.set(Some(rng)));
}

#[no_mangle]
pub extern "C" fn probe_prng_next_u32() -> u32 {
    with_rng(|r| r.next_u32())
}

#[no_mangle]
pub extern "C" fn probe_prng_next() -> f64 {
    with_rng(|r| r.next())
}

#[no_mangle]
pub extern "C" fn probe_prng_next_int(min: f64, max: f64) -> i64 {
    with_rng(|r| r.next_int(min, max))
}

/// The numeric form of `nextID()`; the host renders it in base 36.
#[no_mangle]
pub extern "C" fn probe_prng_next_id_value() -> f64 {
    with_rng(|r| r.next_id_value())
}

#[no_mangle]
pub extern "C" fn probe_prng_chance(odds: f64) -> u8 {
    with_rng(|r| r.chance(odds) as u8)
}

/// `shuffle_len` values of a fresh shuffle from the snapshotted state, written
/// one at a time by the host through successive calls.
#[no_mangle]
pub extern "C" fn probe_prng_shuffle_reset(len: usize) {
    let mut base = SHUFFLE_BASE
        .with(|slot| slot.take())
        .expect("probe_prng_seed must be called first");
    let input: Vec<u8> = (0..len as u8).collect();
    let perm = base.shuffle_array(&input);
    SHUFFLE_BUF.with(|buf| *buf.borrow_mut() = perm);
}

#[no_mangle]
pub extern "C" fn probe_prng_shuffle_at_last(index: usize) -> u8 {
    SHUFFLE_BUF.with(|buf| buf.borrow()[index])
}

#[no_mangle]
pub extern "C" fn probe_exp(x: f64) -> f64 {
    detmath::exp(x)
}

#[no_mangle]
pub extern "C" fn probe_log(x: f64) -> f64 {
    detmath::log(x)
}

#[no_mangle]
pub extern "C" fn probe_pow(x: f64, y: f64) -> f64 {
    detmath::pow(x, y)
}

#[no_mangle]
pub extern "C" fn probe_atan2(y: f64, x: f64) -> f64 {
    detmath::atan2(y, x)
}

#[no_mangle]
pub extern "C" fn probe_pow2(n: i32) -> f64 {
    detmath::pow2(n)
}

// ---------------------------------------------------------------- structures
// Trace replay surface for the queue/heap/grid ports. One live object at a
// time (`probe_st_new` selects the kind); `undefined` results cross the
// boundary as NaN, which no legitimate node id can equal.

use crate::pathfinding::bfs_grid::{self, BfsGrid};
use crate::pathfinding::flat_heap::FlatBinaryHeap;
use crate::pathfinding::priority_queue::{BucketQueue, MinHeap, PriorityQueue};

enum St {
    Mh(Box<MinHeap>),
    Bq(Box<BucketQueue>),
    Fbh(Box<FlatBinaryHeap>),
}

type GridSlot = (BfsGrid, Vec<(i64, i64)>);

thread_local! {
    static ST: std::cell::RefCell<Option<St>> = const { std::cell::RefCell::new(None) };
    static GRID: std::cell::RefCell<Option<GridSlot>> =
        const { std::cell::RefCell::new(None) };
}

/// kind: 0 = MinHeap, 1 = BucketQueue, 2 = FlatBinaryHeap.
#[no_mangle]
pub extern "C" fn probe_st_new(kind: u32, cap: f64) {
    let obj = match kind {
        0 => St::Mh(Box::new(MinHeap::new(cap))),
        1 => St::Bq(Box::new(BucketQueue::new(cap))),
        _ => St::Fbh(Box::new(FlatBinaryHeap::new(cap.max(0.0) as usize))),
    };
    ST.with(|s| *s.borrow_mut() = Some(obj));
}

#[no_mangle]
pub extern "C" fn probe_st_push(a: f64, b: f64) {
    ST.with(|s| match &mut *s.borrow_mut() {
        Some(St::Mh(h)) => h.push(a, b),
        Some(St::Bq(q)) => q.push(a, b),
        Some(St::Fbh(h)) => h.enqueue(a, b),
        None => panic!("probe_st_new must be called first"),
    });
}

/// Pop/dequeue. Returns NaN where JS yields `undefined`. FBH on an empty
/// heap would panic here, but the trace only issues kind-1 pops at non-empty
/// states (the generator recorded that; the host checks `probe_st_can_throw`).
#[no_mangle]
pub extern "C" fn probe_st_pop() -> f64 {
    ST.with(|s| match &mut *s.borrow_mut() {
        Some(St::Mh(h)) => h.pop().unwrap_or(f64::NAN),
        Some(St::Bq(q)) => q.pop().unwrap_or(f64::NAN),
        Some(St::Fbh(h)) => h.dequeue().unwrap_or(f64::NAN),
        None => panic!("probe_st_new must be called first"),
    })
}

#[no_mangle]
pub extern "C" fn probe_st_clear() {
    ST.with(|s| match &mut *s.borrow_mut() {
        Some(St::Mh(h)) => h.clear(),
        Some(St::Bq(q)) => q.clear(),
        Some(St::Fbh(h)) => h.clear(),
        None => panic!("probe_st_new must be called first"),
    });
}

#[no_mangle]
pub extern "C" fn probe_st_is_empty() -> u8 {
    ST.with(|s| match &*s.borrow() {
        Some(St::Mh(h)) => h.is_empty() as u8,
        Some(St::Bq(q)) => q.is_empty() as u8,
        Some(St::Fbh(h)) => (h.size() == 0) as u8,
        None => panic!("probe_st_new must be called first"),
    })
}

/// 1 when an FBH dequeue would throw (empty), else 0. The throw itself is a
/// wasm trap that would kill the instance, so the host observes the condition
/// instead; the *panicking* semantics are pinned by the native test.
#[no_mangle]
pub extern "C" fn probe_st_can_throw() -> u8 {
    ST.with(|s| match &*s.borrow() {
        Some(St::Fbh(h)) => h.size() == 0,
        _ => false,
    }) as u8
}

/// Scalar state field. Per kind: MH (0=size,1=capacity); BQ (0=minBucket,
/// 1=size,2=stamp); FBH (0=len).
#[no_mangle]
pub extern "C" fn probe_st_field(which: u32) -> f64 {
    with_st(
        |st| match st {
            St::Mh(h) => match which {
                0 => h.debug_size() as f64,
                _ => h.debug_capacity() as f64,
            },
            St::Bq(q) => {
                let (min_b, _, size, stamp) = q.debug_state();
                match which {
                    0 => min_b,
                    1 => size,
                    _ => stamp as f64,
                }
            }
            St::Fbh(h) => h.size() as f64,
        },
        f64::NAN,
    )
}

fn with_st<T>(f: impl FnOnce(&St) -> T, dflt: T) -> T {
    ST.with(|s| match &*s.borrow() {
        Some(st) => f(st),
        None => dflt,
    })
}

#[no_mangle]
pub extern "C" fn probe_st_a_len() -> usize {
    with_st(|st| match st {
        St::Mh(h) => h.debug_heap_len(),
        St::Bq(q) => q.debug_full().0.len(),
        St::Fbh(h) => h.debug_state().0.len(),
    }, 0)
}

#[no_mangle]
pub extern "C" fn probe_st_a_get(i: usize) -> f64 {
    with_st(|st| match st {
        St::Mh(h) => h.debug_heap_at(i) as f64,
        St::Bq(q) => q.debug_full().0[i] as f64,
        St::Fbh(h) => f64::from(h.debug_state().0[i]),
    }, f64::NAN)
}

#[no_mangle]
pub extern "C" fn probe_st_b_len() -> usize {
    with_st(|st| match st {
        St::Mh(h) => h.debug_pri_len(),
        St::Bq(q) => q.debug_full().1.len(),
        St::Fbh(h) => h.debug_state().1.len(),
    }, 0)
}

/// NaN encodes `undefined` (FBH tile holes); priority bits / stamps are exact.
#[no_mangle]
pub extern "C" fn probe_st_b_get(i: usize) -> f64 {
    with_st(|st| match st {
        St::Mh(h) => f64::from(h.debug_pri_bits_at(i)),
        St::Bq(q) => q.debug_full().1[i] as f64,
        St::Fbh(h) => h.debug_state().1[i].unwrap_or(f64::NAN),
    }, f64::NAN)
}

/// BucketQueue's sparse keys (empty for the other kinds).
#[no_mangle]
pub extern "C" fn probe_st_c_len() -> usize {
    with_st(|st| match st {
        St::Bq(q) => q.debug_full().2.len(),
        _ => 0,
    }, 0)
}

#[no_mangle]
pub extern "C" fn probe_st_c_get(i: usize) -> f64 {
    with_st(
        |st| match st {
            St::Bq(q) => q.debug_full().2[i] as f64,
            _ => f64::NAN,
        },
        f64::NAN,
    )
}

// ---- grid BFS ----

#[no_mangle]
pub extern "C" fn probe_grid_new(nodes: f64) {
    GRID.with(|g| *g.borrow_mut() = Some((BfsGrid::new(nodes), Vec::new())));
}

/// mode: 0 none, 1 blocker invalid, 2 blocker rejected, 3 blocker found(42).
#[no_mangle]
pub extern "C" fn probe_grid_search(
    w: i64,
    h: i64,
    s0: i64,
    s1: i64,
    max_d: f64,
    mode: u8,
    blocker: i64,
) -> f64 {
    GRID.with(|cell| {
        let mut slot = cell.borrow_mut();
        let (grid, visits) = slot.as_mut().expect("probe_grid_new must be called first");
        visits.clear();
        let valid = |n: i64| !(mode == 1 && n == blocker);
        let visitor = |n: i64, d: i64| -> bfs_grid::Visit<i32> {
            visits.push((n, d));
            if mode == 2 && n == blocker {
                bfs_grid::Visit::Reject
            } else if mode == 3 && n == blocker {
                bfs_grid::Visit::Found(42)
            } else {
                bfs_grid::Visit::Explore
            }
        };
        let starts = if s1 >= 0 { vec![s0, s1] } else { vec![s0] };
        match grid.search(w, h, &starts, max_d, valid, visitor) {
            Some(v) => v as f64,
            None => -1.0,
        }
    })
}

#[no_mangle]
pub extern "C" fn probe_grid_visit_count() -> usize {
    GRID.with(|g| {
        let b = g.borrow();
        b.as_ref().map(|(_, v)| v.len()).unwrap_or(0)
    })
}

#[no_mangle]
pub extern "C" fn probe_grid_visit_node(i: usize) -> f64 {
    GRID.with(|g| g.borrow().as_ref().unwrap().1[i].0 as f64)
}

#[no_mangle]
pub extern "C" fn probe_grid_visit_dist(i: usize) -> f64 {
    GRID.with(|g| g.borrow().as_ref().unwrap().1[i].1 as f64)
}

#[no_mangle]
pub extern "C" fn probe_grid_stamp() -> u64 {
    GRID.with(|g| {
        let b = g.borrow();
        b.as_ref().unwrap().0.debug_stamp()
    })
}

// ---- A* ----
// A single live AStar<GridAdapter>. Blocked tiles are queued first (the
// adapter is built in probe_astar_new), then starts, then the search runs.
// `max_iter` crosses as NaN to mean "default". Scalar-only boundary, matching
// the rest of the probe.

use crate::pathfinding::a_star::{AStar, GridAdapter};

thread_local! {
    static ASTAR: std::cell::RefCell<Option<Box<AStar<GridAdapter>>>> =
        const { std::cell::RefCell::new(None) };
    static ASTAR_BLOCKED: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static ASTAR_STARTS: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static ASTAR_PATH: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Queue a blocked tile for the next adapter (call before probe_astar_new).
#[no_mangle]
pub extern "C" fn probe_astar_block(v: f64) {
    ASTAR_BLOCKED.with(|b| b.borrow_mut().push(v));
}

/// Build the AStar. `max_iter` = NaN means the TS default (500_000).
#[no_mangle]
pub extern "C" fn probe_astar_new(
    w: f64,
    h: f64,
    cc: f64,
    tp: f64,
    hk: u32,
    hs: f64,
    max_iter: f64,
) {
    let blocked = ASTAR_BLOCKED.with(|b| std::mem::take(&mut *b.borrow_mut()));
    let adapter = GridAdapter::new(w, h, &blocked, cc, tp, hk as u8, hs);
    let mi = if max_iter.is_nan() {
        None
    } else {
        Some(max_iter)
    };
    ASTAR.with(|a| *a.borrow_mut() = Some(Box::new(AStar::new(adapter, mi))));
}

/// Queue a start node for the next run (call before probe_astar_run).
#[no_mangle]
pub extern "C" fn probe_astar_start(v: f64) {
    ASTAR_STARTS.with(|s| s.borrow_mut().push(v));
}

/// Run `runs` findPath calls; returns 1 when the last call produced a path.
#[no_mangle]
pub extern "C" fn probe_astar_run(goal: f64, runs: u32) -> u8 {
    let starts = ASTAR_STARTS.with(|s| std::mem::take(&mut *s.borrow_mut()));
    let path = ASTAR.with(|a| {
        let mut astar = a.borrow_mut();
        let astar = astar.as_mut().expect("probe_astar_new must be called first");
        let mut last = None;
        for _ in 0..runs {
            last = astar.find_path(&starts, goal);
        }
        last
    });
    match path {
        Some(p) => {
            ASTAR_PATH.with(|buf| *buf.borrow_mut() = p);
            1
        }
        None => {
            ASTAR_PATH.with(|buf| buf.borrow_mut().clear());
            0
        }
    }
}

#[no_mangle]
pub extern "C" fn probe_astar_path_len() -> usize {
    ASTAR_PATH.with(|p| p.borrow().len())
}

#[no_mangle]
pub extern "C" fn probe_astar_path_at(i: usize) -> f64 {
    ASTAR_PATH.with(|p| p.borrow()[i])
}

#[no_mangle]
pub extern "C" fn probe_astar_stamp() -> u64 {
    ASTAR.with(|a| a.borrow().as_ref().unwrap().debug_stamp())
}

/// field: 0 = closedStamp, 1 = gScoreStamp, 2 = gScore, 3 = cameFrom.
#[no_mangle]
pub extern "C" fn probe_astar_arr_len(field: u32) -> usize {
    ASTAR.with(|a| {
        let astar = a.borrow();
        let astar = astar.as_ref().unwrap();
        match field {
            0 => astar.debug_closed_stamp().len(),
            1 => astar.debug_g_score_stamp().len(),
            2 => astar.debug_g_score().len(),
            _ => astar.debug_came_from().len(),
        }
    })
}

#[no_mangle]
pub extern "C" fn probe_astar_arr_get(field: u32, i: usize) -> f64 {
    ASTAR.with(|a| {
        let astar = a.borrow();
        let astar = astar.as_ref().unwrap();
        match field {
            0 => astar.debug_closed_stamp()[i] as f64,
            1 => astar.debug_g_score_stamp()[i] as f64,
            2 => astar.debug_g_score()[i] as f64,
            _ => astar.debug_came_from()[i] as f64,
        }
    })
}

// ---- A* Rail ----
// Same scalar-only pattern as the AStar probe, over RailAdapter<TerrainMap>.
// Terrain bytes are queued one at a time before probe_rail_new.

use crate::pathfinding::rail::{RailAdapter, TerrainMap};

thread_local! {
    static RAIL: std::cell::RefCell<Option<Box<AStar<RailAdapter<TerrainMap>>>>> =
        const { std::cell::RefCell::new(None) };
    static RAIL_TERRAIN: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
    static RAIL_STARTS: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static RAIL_PATH: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Queue one packed terrain byte (GameMapImpl layout) for the next map.
#[no_mangle]
pub extern "C" fn probe_rail_terrain_byte(v: u32) {
    RAIL_TERRAIN.with(|t| t.borrow_mut().push(v as u8));
}

#[no_mangle]
pub extern "C" fn probe_rail_new(w: f64, h: f64) {
    let terrain = RAIL_TERRAIN.with(|t| std::mem::take(&mut *t.borrow_mut()));
    let adapter = RailAdapter::new(TerrainMap::new(w, h, terrain));
    RAIL.with(|a| *a.borrow_mut() = Some(Box::new(AStar::new(adapter, None))));
}

#[no_mangle]
pub extern "C" fn probe_rail_start(v: f64) {
    RAIL_STARTS.with(|s| s.borrow_mut().push(v));
}

/// One multi-start findPath; 1 = path found, 0 = TS null.
#[no_mangle]
pub extern "C" fn probe_rail_run(goal: f64) -> u8 {
    let starts = RAIL_STARTS.with(|s| std::mem::take(&mut *s.borrow_mut()));
    let path = RAIL.with(|a| {
        let mut rail = a.borrow_mut();
        let rail = rail.as_mut().expect("probe_rail_new must be called first");
        rail.find_path(&starts, goal)
    });
    match path {
        Some(p) => {
            RAIL_PATH.with(|buf| *buf.borrow_mut() = p);
            1
        }
        None => {
            RAIL_PATH.with(|buf| buf.borrow_mut().clear());
            0
        }
    }
}

#[no_mangle]
pub extern "C" fn probe_rail_path_len() -> usize {
    RAIL_PATH.with(|p| p.borrow().len())
}

#[no_mangle]
pub extern "C" fn probe_rail_path_at(i: usize) -> f64 {
    RAIL_PATH.with(|p| p.borrow()[i])
}

#[no_mangle]
pub extern "C" fn probe_rail_stamp() -> u64 {
    RAIL.with(|a| a.borrow().as_ref().unwrap().debug_stamp())
}

/// field: 0 = closedStamp, 1 = gScoreStamp, 2 = gScore, 3 = cameFrom.
#[no_mangle]
pub extern "C" fn probe_rail_arr_len(field: u32) -> usize {
    RAIL.with(|a| {
        let rail = a.borrow();
        let rail = rail.as_ref().unwrap();
        match field {
            0 => rail.debug_closed_stamp().len(),
            1 => rail.debug_g_score_stamp().len(),
            2 => rail.debug_g_score().len(),
            _ => rail.debug_came_from().len(),
        }
    })
}

#[no_mangle]
pub extern "C" fn probe_rail_arr_get(field: u32, i: usize) -> f64 {
    RAIL.with(|a| {
        let rail = a.borrow();
        let rail = rail.as_ref().unwrap();
        match field {
            0 => rail.debug_closed_stamp()[i] as f64,
            1 => rail.debug_g_score_stamp()[i] as f64,
            2 => rail.debug_g_score()[i] as f64,
            _ => rail.debug_came_from()[i] as f64,
        }
    })
}

// ---- A* Water ----
// Same scalar-only pattern as the rail probe, over the self-contained
// AStarWater. Terrain bytes are queued before probe_water_new; weight/maxIter
// cross the boundary as f64 (the vectors always carry concrete numbers).

use crate::pathfinding::water::AStarWater;

thread_local! {
    static WATER: std::cell::RefCell<Option<Box<AStarWater>>> =
        const { std::cell::RefCell::new(None) };
    static WATER_TERRAIN: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
    static WATER_STARTS: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static WATER_PATH: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Queue one packed terrain byte (GameMapImpl layout) for the next map.
#[no_mangle]
pub extern "C" fn probe_water_terrain_byte(v: u32) {
    WATER_TERRAIN.with(|t| t.borrow_mut().push(v as u8));
}

#[no_mangle]
pub extern "C" fn probe_water_new(w: f64, h: f64, weight: f64, max_iter: f64) {
    let terrain = WATER_TERRAIN.with(|t| std::mem::take(&mut *t.borrow_mut()));
    let water = AStarWater::new(w, h, terrain, Some(weight), Some(max_iter));
    WATER.with(|a| *a.borrow_mut() = Some(Box::new(water)));
}

#[no_mangle]
pub extern "C" fn probe_water_start(v: f64) {
    WATER_STARTS.with(|s| s.borrow_mut().push(v));
}

/// One multi-start findPath; 1 = path found, 0 = TS null.
#[no_mangle]
pub extern "C" fn probe_water_run(goal: f64) -> u8 {
    let starts = WATER_STARTS.with(|s| std::mem::take(&mut *s.borrow_mut()));
    let path = WATER.with(|a| {
        let mut water = a.borrow_mut();
        let water = water.as_mut().expect("probe_water_new must be called first");
        water.find_path(&starts, goal)
    });
    match path {
        Some(p) => {
            WATER_PATH.with(|buf| *buf.borrow_mut() = p);
            1
        }
        None => {
            WATER_PATH.with(|buf| buf.borrow_mut().clear());
            0
        }
    }
}

#[no_mangle]
pub extern "C" fn probe_water_path_len() -> usize {
    WATER_PATH.with(|p| p.borrow().len())
}

#[no_mangle]
pub extern "C" fn probe_water_path_at(i: usize) -> f64 {
    WATER_PATH.with(|p| p.borrow()[i])
}

#[no_mangle]
pub extern "C" fn probe_water_stamp() -> u64 {
    WATER.with(|a| a.borrow().as_ref().unwrap().debug_stamp())
}

/// field: 0 = closedStamp, 1 = gScoreStamp, 2 = gScore, 3 = cameFrom.
#[no_mangle]
pub extern "C" fn probe_water_arr_len(field: u32) -> usize {
    WATER.with(|a| {
        let water = a.borrow();
        let water = water.as_ref().unwrap();
        match field {
            0 => water.debug_closed_stamp().len(),
            1 => water.debug_g_score_stamp().len(),
            2 => water.debug_g_score().len(),
            _ => water.debug_came_from().len(),
        }
    })
}

#[no_mangle]
pub extern "C" fn probe_water_arr_get(field: u32, i: usize) -> f64 {
    WATER.with(|a| {
        let water = a.borrow();
        let water = water.as_ref().unwrap();
        match field {
            0 => water.debug_closed_stamp()[i] as f64,
            1 => water.debug_g_score_stamp()[i] as f64,
            2 => water.debug_g_score()[i] as f64,
            _ => water.debug_came_from()[i] as f64,
        }
    })
}

// ---- GameMap ----
// Op-stream replay over the real `GameMap` port. The kind table matches
// `runGm` in gen_vectors.mjs. Scalar-only boundary: array results land in a
// probe-side buffer (`probe_gm_out_len` / `..._at`); `undefined` scalars
// cross as NaN; the two `throw` paths (setOwnerID overflow, invalid ref)
// are observed via would-throw checks instead of trapping the instance —
// the panicking semantics themselves are pinned by the native replay test.

use crate::game_map::GameMap;

thread_local! {
    static GM: std::cell::RefCell<Option<Box<GameMap>>> =
        const { std::cell::RefCell::new(None) };
    static GM_TERRAIN: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
    static GM_OUT: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Queue one packed terrain byte for the next map (before `probe_gm_new`).
#[no_mangle]
pub extern "C" fn probe_gm_terrain_byte(v: u32) {
    GM_TERRAIN.with(|t| t.borrow_mut().push(v as u8));
}

#[no_mangle]
pub extern "C" fn probe_gm_new(w: f64, h: f64, num_land: f64) {
    let terrain = GM_TERRAIN.with(|t| std::mem::take(&mut *t.borrow_mut()));
    GM.with(|g| *g.borrow_mut() = Some(Box::new(GameMap::new(w, h, terrain, num_land))));
}

/// 1 when `set_owner_id(_, player_id)` would throw.
#[no_mangle]
pub extern "C" fn probe_gm_set_owner_throws(player_id: f64) -> u8 {
    (player_id > 4095.0) as u8
}

/// 1 when `tile_ref(x, y)` would throw.
#[no_mangle]
pub extern "C" fn probe_gm_ref_throws(x: f64, y: f64) -> u8 {
    with_gm(|gm| !gm.is_valid_coord(x, y)) as u8
}

fn with_gm<T>(f: impl FnOnce(&GameMap) -> T) -> T {
    GM.with(|g| f(g.borrow().as_ref().expect("probe_gm_new must be called first")))
}

fn with_gm_mut<T>(f: impl FnOnce(&mut GameMap) -> T) -> T {
    GM.with(|g| f(g.borrow_mut().as_mut().expect("probe_gm_new must be called first")))
}

/// Replay one op; returns the scalar result (NaN for void ops and for
/// `undefined`), or fills the out buffer for array ops.
#[allow(clippy::too_many_lines)]
#[no_mangle]
pub extern "C" fn probe_gm_op(kind: u32, a: f64, b: f64) -> f64 {
    match kind {
        0 => { with_gm_mut(|gm| gm.set_water(a)); f64::NAN }
        1 => { with_gm_mut(|gm| gm.set_shoreline_bit(a)); f64::NAN }
        2 => { with_gm_mut(|gm| gm.clear_shoreline_bit(a)); f64::NAN }
        3 => { with_gm_mut(|gm| gm.set_ocean(a)); f64::NAN }
        4 => { with_gm_mut(|gm| gm.set_magnitude(a, b)); f64::NAN }
        5 => { with_gm_mut(|gm| gm.set_owner_id(a, b)); f64::NAN }
        6 => { with_gm_mut(|gm| gm.set_fallout(a, b != 0.0)); f64::NAN }
        7 => { with_gm_mut(|gm| gm.set_defense_bonus(a, b != 0.0)); f64::NAN }
        8 => with_gm_mut(|gm| gm.update_tile(a, b) as u8) as f64,
        9 => {
            let v = with_gm(|gm| gm.neighbors(a));
            GM_OUT.with(|o| *o.borrow_mut() = v);
            f64::NAN
        }
        10 => {
            let v = with_gm(|gm| {
                let mut buf = [0.0f64; 4];
                let n = gm.neighbors4(a, &mut buf);
                buf[..n].to_vec()
            });
            GM_OUT.with(|o| *o.borrow_mut() = v);
            f64::NAN
        }
        11 => {
            let v = with_gm(|gm| {
                let mut buf = [0.0f64; 8];
                let n = gm.neighbors8(a, &mut buf);
                buf[..n].to_vec()
            });
            GM_OUT.with(|o| *o.borrow_mut() = v);
            f64::NAN
        }
        12 => {
            let mut v = Vec::new();
            with_gm(|gm| gm.for_each_neighbor_with_diag(a, |t| v.push(t)));
            GM_OUT.with(|o| *o.borrow_mut() = v);
            f64::NAN
        }
        13 => with_gm(|gm| gm.is_land(a)) as u8 as f64,
        14 => with_gm(|gm| gm.is_impassable(a)) as u8 as f64,
        15 => with_gm(|gm| gm.is_ocean_shore(a)) as u8 as f64,
        16 => with_gm(|gm| gm.is_shore(a)) as u8 as f64,
        17 => with_gm(|gm| gm.is_water(a)) as u8 as f64,
        18 => with_gm(|gm| gm.cost(a)),
        19 => with_gm(|gm| gm.terrain_type(a) as u8) as f64,
        20 => with_gm(|gm| gm.magnitude(a)),
        21 => with_gm(|gm| gm.terrain_byte(a)).unwrap_or(f64::NAN),
        22 => with_gm(|gm| gm.owner_id(a)),
        23 => with_gm(|gm| gm.tile_state(a)).unwrap_or(f64::NAN),
        24 => with_gm(|gm| gm.has_fallout(a)) as u8 as f64,
        25 => with_gm(|gm| gm.has_defense_bonus(a)) as u8 as f64,
        26 => with_gm(|gm| gm.has_owner(a)) as u8 as f64,
        27 => with_gm(|gm| gm.is_border(a)) as u8 as f64,
        28 => with_gm(|gm| gm.is_on_edge_of_map(a)) as u8 as f64,
        29 => with_gm(|gm| gm.x(a)),
        30 => with_gm(|gm| gm.y(a)),
        31 => with_gm(|gm| gm.tile_ref(a, b)),
        32 => with_gm(|gm| gm.manhattan_dist(a, b)),
        33 => with_gm(|gm| gm.euclidean_dist_squared(a, b)),
        34 => {
            let v = with_gm(|gm| match b {
                1.0 => gm.bfs(a, &|m: &GameMap, t: f64| m.is_land(t)),
                2.0 => gm.bfs(a, &|_: &GameMap, t: f64| t % 2.0 == 0.0),
                _ => gm.bfs(a, &|_: &GameMap, _: f64| true),
            });
            GM_OUT.with(|o| *o.borrow_mut() = v);
            f64::NAN
        }
        35 => {
            let radius = if b.abs() == 0.0 { 1.0 } else { b.abs() };
            let even = b != 0.0;
            let v = with_gm(|gm| gm.circle_search(a, radius, move |_, d2| !even || d2 % 2.0 == 0.0));
            GM_OUT.with(|o| *o.borrow_mut() = v);
            f64::NAN
        }
        k => panic!("unexpected gm op kind {k}"),
    }
}

#[no_mangle]
pub extern "C" fn probe_gm_out_len() -> usize {
    GM_OUT.with(|o| o.borrow().len())
}

#[no_mangle]
pub extern "C" fn probe_gm_out_at(i: usize) -> f64 {
    GM_OUT.with(|o| o.borrow()[i])
}

/// Counter field: 0 = numLandTiles, 1 = waterVersion, 2 = numTilesWithFallout.
#[no_mangle]
pub extern "C" fn probe_gm_field(which: u32) -> f64 {
    with_gm(|gm| match which {
        0 => gm.num_land_tiles(),
        1 => gm.water_version(),
        _ => gm.num_tiles_with_fallout(),
    })
}

/// field: 0 = terrain bytes, 1 = state words.
#[no_mangle]
pub extern "C" fn probe_gm_arr_len(field: u32) -> usize {
    with_gm(|gm| match field {
        0 => gm.debug_terrain().len(),
        _ => gm.debug_state().len(),
    })
}

#[no_mangle]
pub extern "C" fn probe_gm_arr_get(field: u32, i: usize) -> f64 {
    with_gm(|gm| match field {
        0 => gm.debug_terrain()[i] as f64,
        _ => gm.debug_state()[i] as f64,
    })
}
