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

// ---- AStarWaterBounded ----
// Same scalar-only pattern as the water probe. Terrain bytes are queued
// before probe_wb_new (the constructor needs width/maxSearchArea/config, not
// height); starts are queued, then one run chooses findPath (mode 0) or
// searchBounded (mode 1, bounds cross as four f64s — always integers in the
// recorded scenarios).

use crate::pathfinding::water_bounded::AStarWaterBounded;

thread_local! {
    static WB: std::cell::RefCell<Option<Box<AStarWaterBounded>>> =
        const { std::cell::RefCell::new(None) };
    static WB_TERRAIN: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
    static WB_STARTS: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static WB_PATH: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Queue one packed terrain byte (GameMapImpl layout) for the next map.
#[no_mangle]
pub extern "C" fn probe_wb_terrain_byte(v: u32) {
    WB_TERRAIN.with(|t| t.borrow_mut().push(v as u8));
}

#[no_mangle]
pub extern "C" fn probe_wb_new(w: f64, max_area: f64, weight: f64, max_iter: f64) {
    let terrain = WB_TERRAIN.with(|t| std::mem::take(&mut *t.borrow_mut()));
    let wb = AStarWaterBounded::new(w, terrain, max_area, Some(weight), Some(max_iter));
    WB.with(|a| *a.borrow_mut() = Some(Box::new(wb)));
}

#[no_mangle]
pub extern "C" fn probe_wb_start(v: f64) {
    WB_STARTS.with(|s| s.borrow_mut().push(v));
}

/// One search; mode 0 = findPath (bounds ignored), 1 = searchBounded with
/// explicit [minX,maxX,minY,maxY]. 1 = path found, 0 = TS null.
#[no_mangle]
pub extern "C" fn probe_wb_run(goal: f64, mode: u32, b0: f64, b1: f64, b2: f64, b3: f64) -> u8 {
    let starts = WB_STARTS.with(|s| std::mem::take(&mut *s.borrow_mut()));
    let path = WB.with(|a| {
        let mut wb = a.borrow_mut();
        let wb = wb.as_mut().expect("probe_wb_new must be called first");
        if mode == 1 {
            wb.search_bounded(&starts, goal, b0, b1, b2, b3)
        } else {
            wb.find_path(&starts, goal)
        }
    });
    match path {
        Some(p) => {
            WB_PATH.with(|buf| *buf.borrow_mut() = p);
            1
        }
        None => {
            WB_PATH.with(|buf| buf.borrow_mut().clear());
            0
        }
    }
}

#[no_mangle]
pub extern "C" fn probe_wb_path_len() -> usize {
    WB_PATH.with(|p| p.borrow().len())
}

#[no_mangle]
pub extern "C" fn probe_wb_path_at(i: usize) -> f64 {
    WB_PATH.with(|p| p.borrow()[i])
}

#[no_mangle]
pub extern "C" fn probe_wb_stamp() -> u64 {
    WB.with(|a| a.borrow().as_ref().unwrap().debug_stamp())
}

/// field: 0 = closedStamp, 1 = gScoreStamp, 2 = gScore, 3 = cameFrom.
#[no_mangle]
pub extern "C" fn probe_wb_arr_len(field: u32) -> usize {
    WB.with(|a| {
        let wb = a.borrow();
        let wb = wb.as_ref().unwrap();
        match field {
            0 => wb.debug_closed_stamp().len(),
            1 => wb.debug_g_score_stamp().len(),
            2 => wb.debug_g_score().len(),
            _ => wb.debug_came_from().len(),
        }
    })
}

#[no_mangle]
pub extern "C" fn probe_wb_arr_get(field: u32, i: usize) -> f64 {
    WB.with(|a| {
        let wb = a.borrow();
        let wb = wb.as_ref().unwrap();
        match field {
            0 => wb.debug_closed_stamp()[i] as f64,
            1 => wb.debug_g_score_stamp()[i] as f64,
            2 => wb.debug_g_score()[i] as f64,
            _ => wb.debug_came_from()[i] as f64,
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

// ---- TileSet ----
// Op-stream replay over the real `TileSet` port. The kind table matches
// `runTs` in gen_vectors.mjs: 0=add, 1=delete->bool, 2=has->bool, 3=size,
// 4=values()->out buffer, 5=clear, 6=forEach collect->out buffer,
// 7=add-during-forEach, 8=delete-during-forEach (both collect via the
// begin/next/end iteration surface so the callback mutation happens at the
// same point in the walk as the TS `forEach`).

use crate::tile_set::TileSet;

thread_local! {
    static TS: std::cell::RefCell<Option<TileSet>> = const { std::cell::RefCell::new(None) };
    static TS_INIT: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static TS_OUT: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Queue one initial value for the next set (before `probe_ts_new`).
#[no_mangle]
pub extern "C" fn probe_ts_initial_value(v: f64) {
    TS_INIT.with(|t| t.borrow_mut().push(v));
}

#[no_mangle]
pub extern "C" fn probe_ts_new() {
    let init = TS_INIT.with(|t| std::mem::take(&mut *t.borrow_mut()));
    TS.with(|s| {
        *s.borrow_mut() = Some(TileSet::new(if init.is_empty() {
            None
        } else {
            Some(&init)
        }))
    });
}

fn with_ts<T>(f: impl FnOnce(&TileSet) -> T) -> T {
    TS.with(|s| f(s.borrow().as_ref().expect("probe_ts_new must be called first")))
}

fn with_ts_mut<T>(f: impl FnOnce(&mut TileSet) -> T) -> T {
    TS.with(|s| f(s.borrow_mut().as_mut().expect("probe_ts_new must be called first")))
}

/// Replay one op; returns the scalar result (NaN for void ops), or fills the
/// out buffer for array ops (4/6/7/8).
#[no_mangle]
pub extern "C" fn probe_ts_op(kind: u32, a: f64) -> f64 {
    match kind {
        0 => { with_ts_mut(|s| s.add(a)); f64::NAN }
        1 => with_ts_mut(|s| s.delete(a)) as u8 as f64,
        2 => with_ts(|s| s.has(a)) as u8 as f64,
        3 => with_ts(|s| s.size()),
        4 => {
            let v = with_ts(|s| s.collect_values());
            TS_OUT.with(|o| *o.borrow_mut() = v);
            f64::NAN
        }
        5 => { with_ts_mut(|s| s.clear()); f64::NAN }
        6 => {
            with_ts_mut(|s| s.iter_begin());
            let mut v = Vec::new();
            let mut cur = 0usize;
            while let Some(x) = with_ts(|s| s.iter_next(&mut cur)) {
                v.push(x);
            }
            with_ts_mut(|s| s.iter_end());
            TS_OUT.with(|o| *o.borrow_mut() = v);
            f64::NAN
        }
        7 => {
            // add inside the callback after the first visit.
            with_ts_mut(|s| s.iter_begin());
            let mut v = Vec::new();
            let mut cur = 0usize;
            while let Some(x) = with_ts(|s| s.iter_next(&mut cur)) {
                v.push(x);
                if v.len() == 1 {
                    with_ts_mut(|s| s.add(a));
                }
            }
            with_ts_mut(|s| s.iter_end());
            TS_OUT.with(|o| *o.borrow_mut() = v);
            f64::NAN
        }
        8 => {
            // delete inside the callback after the first visit.
            with_ts_mut(|s| s.iter_begin());
            let mut v = Vec::new();
            let mut cur = 0usize;
            while let Some(x) = with_ts(|s| s.iter_next(&mut cur)) {
                v.push(x);
                if v.len() == 1 {
                    with_ts_mut(|s| s.delete(a));
                }
            }
            with_ts_mut(|s| s.iter_end());
            TS_OUT.with(|o| *o.borrow_mut() = v);
            f64::NAN
        }
        k => panic!("unexpected ts op kind {k}"),
    }
}

#[no_mangle]
pub extern "C" fn probe_ts_out_len() -> usize {
    TS_OUT.with(|o| o.borrow().len())
}

#[no_mangle]
pub extern "C" fn probe_ts_out_at(i: usize) -> f64 {
    TS_OUT.with(|o| o.borrow()[i])
}

/// Scalar debug field: 0 = denseLen, 1 = tableUsed, 2 = iterDepth.
#[no_mangle]
pub extern "C" fn probe_ts_field(which: u32) -> f64 {
    with_ts(|s| match which {
        0 => s.debug_dense_len() as f64,
        1 => s.debug_table_used(),
        _ => s.debug_iter_depth(),
    })
}

/// Array debug field: 0 = dense (u32), 1 = table (i32).
#[no_mangle]
pub extern "C" fn probe_ts_arr_len(field: u32) -> usize {
    with_ts(|s| match field {
        0 => s.debug_dense().len(),
        _ => s.debug_table().len(),
    })
}

#[no_mangle]
pub extern "C" fn probe_ts_arr_get(field: u32, i: usize) -> f64 {
    with_ts(|s| match field {
        0 => s.debug_dense()[i] as f64,
        _ => s.debug_table()[i] as f64,
    })
}

// ---- Util ----
// Single-call replay over the `util` port. The host queues the scalar args
// (`probe_util_arg`) and, for simpleHash, the UTF-16 code units
// (`probe_util_str_unit`) before each `probe_util_op(kind)`; kinds 12/13/14
// read the map previously built by `probe_gm_new`. Array results (kinds 3/4/5
// winners, 13 tiles) land in the out buffer.

use crate::util;

thread_local! {
    static UTIL_ARGS: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static UTIL_UNITS: std::cell::RefCell<Vec<u16>> = const { std::cell::RefCell::new(Vec::new()) };
    static UTIL_OUT: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[no_mangle]
pub extern "C" fn probe_util_arg(v: f64) {
    UTIL_ARGS.with(|t| t.borrow_mut().push(v));
}

#[no_mangle]
pub extern "C" fn probe_util_str_unit(u: f64) {
    UTIL_UNITS.with(|t| t.borrow_mut().push(u as u16));
}

/// Replay one util call; returns the first result value (array results return
/// their first element and also fill the out buffer — read it via
/// `probe_util_out_len` / `probe_util_out_at`).
#[allow(clippy::too_many_lines)]
#[no_mangle]
pub extern "C" fn probe_util_op(kind: u32) -> f64 {
    let a = UTIL_ARGS.with(|t| std::mem::take(&mut *t.borrow_mut()));
    let out: Vec<f64> = match kind {
        0 => vec![util::manhattan_dist_wrapped(
            &util::Cell { x: a[0], y: a[1] },
            &util::Cell { x: a[2], y: a[3] },
            a[4],
        )],
        1 => vec![util::within(a[0], a[1], a[2])],
        2 => {
            let units = UTIL_UNITS.with(|t| std::mem::take(&mut *t.borrow_mut()));
            vec![util::simple_hash_units(&units)]
        }
        3 => match util::find_minimum_by(&a[2..], a[0] as u8, a[1] as u8) {
            Some(v) => vec![v],
            None => vec![],
        },
        4 => {
            let pairs: Vec<(f64, f64)> = a.chunks(2).map(|c| (c[0], c[1])).collect();
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
            let tiles = a[3..].to_vec();
            let bb = with_gm(|gm| util::calculate_bounding_box(gm, tiles));
            vec![bb.min.x, bb.min.y, bb.max.x, bb.max.y]
        }
        13 => with_gm(|gm| util::bounding_box_tiles(gm, a[2], a[3])),
        14 => {
            let tiles = a[2..].to_vec();
            let c = with_gm(|gm| util::calculate_bounding_box_center(gm, tiles));
            vec![c.x, c.y]
        }
        k => panic!("unexpected util op kind {k}"),
    };
    let first = out.first().copied().unwrap_or(f64::NAN);
    UTIL_OUT.with(|o| *o.borrow_mut() = out);
    first
}

#[no_mangle]
pub extern "C" fn probe_util_out_len() -> usize {
    UTIL_OUT.with(|o| o.borrow().len())
}

#[no_mangle]
pub extern "C" fn probe_util_out_at(i: usize) -> f64 {
    UTIL_OUT.with(|o| o.borrow()[i])
}

// ---------------------------------------------------------------- TeamAssignment
//
// The scenario inputs are structured (nested players with string fields), so
// they cross the boundary as a flat `f64` token stream: a string is
// `[len, u0, .. u(len-1)]` (UTF-16 code units), an optional string is
// `[present, (string if present)]`, and a `teamIndex` is `[flag]` where
// flag 0 = null, 1 = a finite number follows, 2 = NaN. The host (see
// `run_wasm_parity.mjs`) encodes vectors.json the same way.

use crate::team_assignment as ta;

thread_local! {
    static TEAM_ARGS: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static TEAM_OUT: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[no_mangle]
pub extern "C" fn probe_team_arg(v: f64) {
    TEAM_ARGS.with(|t| t.borrow_mut().push(v));
}

/// Cursor over the flat token stream.
struct Cur<'a>(&'a [f64], usize);
impl<'a> Cur<'a> {
    fn f(&mut self) -> f64 {
        let v = self.0[self.1];
        self.1 += 1;
        v
    }
    fn u(&mut self) -> usize {
        self.f() as usize
    }
    /// Decode a UTF-16-code-unit string correctly across surrogate pairs.
    fn utf16_string(&mut self) -> String {
        let len = self.u();
        let units: Vec<u16> = (0..len).map(|_| self.f() as u16).collect();
        String::from_utf16_lossy(&units)
    }
    fn opt_string(&mut self) -> Option<String> {
        if self.f() != 0.0 {
            Some(self.utf16_string())
        } else {
            None
        }
    }
    fn team_index(&mut self) -> Option<f64> {
        match self.f() {
            flag if flag == 0.0 => None,
            flag if flag == 1.0 => Some(self.f()),
            _ => Some(f64::NAN),
        }
    }
    fn config(&mut self) -> ta::TeamCountConfig {
        match self.u() {
            0 => ta::TeamCountConfig::Num(self.f()),
            1 => ta::TeamCountConfig::Duos,
            2 => ta::TeamCountConfig::Trios,
            3 => ta::TeamCountConfig::Quads,
            4 => ta::TeamCountConfig::HumansVsNations,
            _ => ta::TeamCountConfig::Other(self.utf16_string()),
        }
    }
}

fn push_str(out: &mut Vec<f64>, s: &str) {
    let units: Vec<u16> = s.encode_utf16().collect();
    out.push(units.len() as f64);
    out.extend(units.iter().map(|&u| u as f64));
}

#[no_mangle]
pub extern "C" fn probe_team_op(kind: u32) -> f64 {
    let a = TEAM_ARGS.with(|t| std::mem::take(&mut *t.borrow_mut()));
    let mut c = Cur(&a, 0);
    let mut out: Vec<f64> = Vec::new();
    match kind {
        0 | 1 => {
            let n = c.u();
            let mut players = Vec::with_capacity(n);
            for _ in 0..n {
                let pt = match c.u() {
                    0 => ta::PlayerType::Bot,
                    1 => ta::PlayerType::Human,
                    _ => ta::PlayerType::Nation,
                };
                let team_index = c.team_index();
                let id = c.utf16_string();
                let client_id = c.opt_string();
                let clan_tag = c.opt_string();
                let nf = c.u();
                let friends = (0..nf).map(|_| c.utf16_string()).collect();
                players.push(ta::PlayerInfo {
                    id,
                    player_type: pt,
                    client_id,
                    clan_tag,
                    friends,
                    team_index,
                });
            }
            let nt = c.u();
            let teams: Vec<String> = (0..nt).map(|_| c.utf16_string()).collect();
            let is_duo = c.f() != 0.0;
            let got = if kind == 0 {
                let has_max = c.f() != 0.0;
                let max = c.f();
                if has_max {
                    ta::assign_teams_with_max(&players, &teams, is_duo, max)
                } else {
                    ta::assign_teams(&players, &teams, is_duo)
                }
            } else {
                let nation_count = c.u();
                let config = c.config();
                ta::assign_teams_lobby_preview(&players, &teams, &config, nation_count)
            };
            for (pi, assign) in &got {
                let team_idx = match assign {
                    ta::Assignment::Kicked => -1.0,
                    ta::Assignment::Team(t) => {
                        teams.iter().position(|x| x == t).unwrap() as f64
                    }
                };
                out.push(*pi as f64);
                out.push(team_idx);
            }
        }
        2 => {
            let num_players = c.f();
            let num_teams = c.f();
            out.push(ta::get_max_team_size(num_players, num_teams));
        }
        3 => {
            let config = c.config();
            let total = c.f();
            match ta::resolve_teams_list(&config, total) {
                Ok(list) => {
                    out.push(0.0);
                    out.push(list.len() as f64);
                    for t in &list {
                        push_str(&mut out, t);
                    }
                }
                Err(e) => {
                    out.push(match e {
                        ta::ResolveTeamsError::UnknownConfig => 1.0,
                        ta::ResolveTeamsError::TooFewTeams => 2.0,
                        ta::ResolveTeamsError::InvalidLength => 3.0,
                    });
                }
            }
        }
        k => panic!("unexpected team op kind {k}"),
    }
    let first = out.first().copied().unwrap_or(f64::NAN);
    TEAM_OUT.with(|o| *o.borrow_mut() = out);
    first
}

#[no_mangle]
pub extern "C" fn probe_team_out_len() -> usize {
    TEAM_OUT.with(|o| o.borrow().len())
}

#[no_mangle]
pub extern "C" fn probe_team_out_at(i: usize) -> f64 {
    TEAM_OUT.with(|o| o.borrow()[i])
}

// ---------------------------------------------------------------- Bezier
//
// Kind 0 (getLength): 8 control-point scalars. Kind 1 (walk): 8 scalars +
// spacing + the increment script. The out buffer for a walk is
// [np, points(flat x/y), nw, walk(flat index/x/y triples, -1 = null),
// final_index]; getLength returns [len] directly.

use crate::line::{DistanceBasedBezierCurve, Point};

thread_local! {
    static BEZIER_ARGS: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static BEZIER_OUT: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[no_mangle]
pub extern "C" fn probe_bezier_arg(v: f64) {
    BEZIER_ARGS.with(|t| t.borrow_mut().push(v));
}

#[no_mangle]
pub extern "C" fn probe_bezier_op(kind: u32) -> f64 {
    let a = BEZIER_ARGS.with(|t| std::mem::take(&mut *t.borrow_mut()));
    let pt = |i: usize| Point { x: a[i * 2], y: a[i * 2 + 1] };
    let out: Vec<f64> = match kind {
        0 => vec![DistanceBasedBezierCurve::get_length(&pt(0), &pt(1), &pt(2), &pt(3))],
        1 => {
            let spacing = a[8];
            let mut curve = DistanceBasedBezierCurve::new(&pt(0), &pt(1), &pt(2), &pt(3), spacing);
            let mut out = Vec::new();
            let points: Vec<Point> = curve.all_points().to_vec();
            out.push(points.len() as f64);
            for p in &points {
                out.push(p.x);
                out.push(p.y);
            }
            let incs = &a[9..];
            out.push(incs.len() as f64);
            for &d in incs {
                match curve.increment(d) {
                    None => out.extend([f64::from(-1), 0.0, 0.0]),
                    Some(p) => {
                        out.push(curve.current_index() as f64);
                        out.push(p.x);
                        out.push(p.y);
                    }
                }
            }
            out.push(curve.current_index() as f64);
            out
        }
        k => panic!("unexpected bezier op kind {k}"),
    };
    let first = out.first().copied().unwrap_or(f64::NAN);
    BEZIER_OUT.with(|o| *o.borrow_mut() = out);
    first
}

#[no_mangle]
pub extern "C" fn probe_bezier_out_len() -> usize {
    BEZIER_OUT.with(|o| o.borrow().len())
}

#[no_mangle]
pub extern "C" fn probe_bezier_out_at(i: usize) -> f64 {
    BEZIER_OUT.with(|o| o.borrow()[i])
}

// ---------------------------------------------------------------- Veterancy
//
// Pure scalar function: three f64 args in, one f64 out. No buffer needed.

use crate::veterancy::max_health_with_veterancy;

thread_local! {
    static VET_ARGS: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[no_mangle]
pub extern "C" fn probe_veterancy_arg(v: f64) {
    VET_ARGS.with(|t| t.borrow_mut().push(v));
}

#[no_mangle]
pub extern "C" fn probe_veterancy_op() -> f64 {
    let a = VET_ARGS.with(|t| std::mem::take(&mut *t.borrow_mut()));
    max_health_with_veterancy(a[0], a[1], a[2])
}

// ---------------------------------------------------------------- MotionPlans
//
// Records cross the boundary as the same flat `f64` token stream the host
// reads from vectors.json: `[count, per record
// 1,unitId,planId,startTick,ticksPerStep,pathLen,path... |
// 2,engineId,planId,startTick,speed,spacing,carCount,pathLen,cars...,path...]`.
// op kind 0 = pack: input is the record stream, out buffer is
// `[wlen, ...packedWords]`. op kind 1 = unpack: input is `[wlen, ...words]`,
// out buffer is the re-encoded record stream. The host drives both legs of a
// roundtrip scenario (op 0 then feed its words back to op 1).

use crate::motion_plans::{
    pack_motion_plans, unpack_motion_plans, MotionPlanInput, MotionPlanRecord,
};

thread_local! {
    static MP_ARGS: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static MP_OUT: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[no_mangle]
pub extern "C" fn probe_mp_arg(v: f64) {
    MP_ARGS.with(|t| t.borrow_mut().push(v));
}

fn mp_decode_inputs(t: &[f64]) -> Vec<MotionPlanInput<'_>> {
    let mut out = Vec::new();
    let count = t[0] as usize;
    let mut i = 1usize;
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

fn mp_encode_records(records: &[MotionPlanRecord]) -> Vec<f64> {
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

#[no_mangle]
pub extern "C" fn probe_mp_op(kind: u32) -> f64 {
    let a = MP_ARGS.with(|t| std::mem::take(&mut *t.borrow_mut()));
    let out: Vec<f64> = match kind {
        0 => {
            let inputs = mp_decode_inputs(&a);
            let packed = pack_motion_plans(&inputs);
            let mut o = vec![packed.len() as f64];
            o.extend(packed.iter().map(|w| *w as f64));
            o
        }
        1 => {
            let wlen = a[0] as usize;
            let words: Vec<u32> = a[1..1 + wlen].iter().map(|v| *v as u32).collect();
            mp_encode_records(&unpack_motion_plans(&words))
        }
        k => panic!("unexpected mp op kind {k}"),
    };
    let first = out.first().copied().unwrap_or(f64::NAN);
    MP_OUT.with(|o| *o.borrow_mut() = out);
    first
}

#[no_mangle]
pub extern "C" fn probe_mp_out_len() -> usize {
    MP_OUT.with(|o| o.borrow().len())
}

#[no_mangle]
pub extern "C" fn probe_mp_out_at(i: usize) -> f64 {
    MP_OUT.with(|o| o.borrow()[i])
}

// ------------------------------------------------------ ConnectedComponents
// terrain bytes are queued one at a time (like the GM/Rail probes); the
// instance is built by `probe_cc_new`, ops replayed by `probe_cc_op` (void
// ops return NaN, queries return the scalar), and the internal buffers are
// read back element-wise via `probe_cc_arr_len` / `..._get`.

use crate::pathfinding::connected_components::ConnectedComponents;

thread_local! {
    static CC: std::cell::RefCell<Option<Box<ConnectedComponents>>> =
        const { std::cell::RefCell::new(None) };
    static CC_TERRAIN: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Queue one packed terrain byte for the next map (before `probe_cc_new`).
#[no_mangle]
pub extern "C" fn probe_cc_terrain_byte(v: u32) {
    CC_TERRAIN.with(|t| t.borrow_mut().push(v as u8));
}

#[no_mangle]
pub extern "C" fn probe_cc_new(w: f64, h: f64, direct: u32) {
    let terrain = CC_TERRAIN.with(|t| std::mem::take(&mut *t.borrow_mut()));
    CC.with(|c| {
        *c.borrow_mut() = Some(Box::new(ConnectedComponents::new(
            w as i64,
            h as i64,
            terrain,
            direct != 0,
        )))
    });
}

fn with_cc<T>(f: impl FnOnce(&ConnectedComponents) -> T) -> T {
    CC.with(|c| {
        f(c.borrow().as_ref().expect("probe_cc_new must be called first"))
    })
}

fn with_cc_mut<T>(f: impl FnOnce(&mut ConnectedComponents) -> T) -> T {
    CC.with(|c| {
        f(c.borrow_mut()
            .as_mut()
            .expect("probe_cc_new must be called first"))
    })
}

/// Replay one op: 0=initialize, 1=addWaterTiles(a), 2=getComponentId(a),
/// 3=getComponentSize(a). Void ops (0, 1) return NaN.
#[no_mangle]
pub extern "C" fn probe_cc_op(kind: u32, a: f64) -> f64 {
    match kind {
        0 => { with_cc_mut(|cc| cc.initialize()); f64::NAN }
        1 => { with_cc_mut(|cc| cc.add_water_tile(a)); f64::NAN }
        2 => with_cc_mut(|cc| cc.get_component_id(a)) as f64,
        3 => with_cc_mut(|cc| cc.get_component_size(a)),
        k => panic!("unexpected cc op kind {k}"),
    }
}

/// Scalar field: 0=bits, 1=landMarker, 2=maxId.
#[no_mangle]
pub extern "C" fn probe_cc_field(which: u32) -> f64 {
    with_cc(|cc| match which {
        0 => cc.debug_bits() as f64,
        1 => cc.debug_land_marker(),
        2 => cc.debug_max_id(),
        k => panic!("unexpected cc field {k}"),
    })
}

/// Array field: 0=ids, 1=sizes, 2=parents.
#[no_mangle]
pub extern "C" fn probe_cc_arr_len(field: u32) -> usize {
    with_cc(|cc| match field {
        0 => cc.debug_ids().len(),
        1 => cc.debug_sizes().len(),
        2 => cc.debug_parents().len(),
        k => panic!("unexpected cc array {k}"),
    })
}

#[no_mangle]
pub extern "C" fn probe_cc_arr_get(field: u32, i: usize) -> f64 {
    with_cc(|cc| match field {
        0 => cc.debug_ids()[i],
        1 => cc.debug_sizes()[i],
        2 => cc.debug_parents()[i],
        k => panic!("unexpected cc array {k}"),
    })
}

// ------------------------------------------------------ TerrainSearchMap
// Buffer bytes are queued one at a time (like the GM/Rail/CC probes); the
// instance is built by `probe_tsm_new`, ops replayed by `probe_tsm_op`
// (getWidth/getHeight/node return the scalar; neighbors lands in the
// probe-side out buffer read back via `probe_tsm_out_len` / `..._at`).

use crate::terrain_search_map::{SearchMapTileType, TerrainSearchMap};

thread_local! {
    static TSM: std::cell::RefCell<Option<Box<TerrainSearchMap>>> =
        const { std::cell::RefCell::new(None) };
    static TSM_BUFFER: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
    static TSM_OUT: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Queue one buffer byte for the next map (before `probe_tsm_new`).
#[no_mangle]
pub extern "C" fn probe_tsm_buffer_byte(v: u32) {
    TSM_BUFFER.with(|b| b.borrow_mut().push(v as u8));
}

#[no_mangle]
pub extern "C" fn probe_tsm_new() {
    let buffer = TSM_BUFFER.with(|b| std::mem::take(&mut *b.borrow_mut()));
    TSM.with(|t| *t.borrow_mut() = Some(Box::new(TerrainSearchMap::new(buffer))));
}

fn with_tsm<T>(f: impl FnOnce(&TerrainSearchMap) -> T) -> T {
    TSM.with(|t| {
        f(t.borrow()
            .as_ref()
            .expect("probe_tsm_new must be called first"))
    })
}

/// Replay one op: 0=getWidth, 1=getHeight, 2=node(x,y), 3=neighbors(x,y)
/// (result lands in the out buffer, this returns NaN).
#[no_mangle]
pub extern "C" fn probe_tsm_op(kind: u32, a: f64, b: f64) -> f64 {
    match kind {
        0 => with_tsm(|tsm| tsm.get_width()),
        1 => with_tsm(|tsm| tsm.get_height()),
        2 => with_tsm(|tsm| match tsm.node(a, b) {
            SearchMapTileType::Land => 0.0,
            SearchMapTileType::Shore => 1.0,
            SearchMapTileType::Water => 2.0,
        }),
        3 => {
            let ns = with_tsm(|tsm| tsm.neighbors(a, b));
            let flat: Vec<f64> = ns.into_iter().flat_map(|n| [n.x, n.y]).collect();
            TSM_OUT.with(|o| *o.borrow_mut() = flat);
            f64::NAN
        }
        k => panic!("unexpected tsm op kind {k}"),
    }
}

#[no_mangle]
pub extern "C" fn probe_tsm_out_len() -> usize {
    TSM_OUT.with(|o| o.borrow().len())
}

#[no_mangle]
pub extern "C" fn probe_tsm_out_at(i: usize) -> f64 {
    TSM_OUT.with(|o| o.borrow()[i])
}

// --------------------------------------------------------- AbstractGraph
// Terrain bytes and dirty-tile tokens are queued one at a time; `probe_ag_new`
// builds the graph (cloning a prior scenario's graph as the old graph when
// `old_idx >= 0`, mirroring the TS capture's `agGraphs` map), `probe_ag_op`
// replays the access stream — scalar kinds return the value directly, array
// kinds land in the out buffer where `probe_ag_out_len` == -1 encodes
// `undefined`/`null` — and the final internal arrays are read back
// element-wise via `probe_ag_arr_len` / `..._get`.

use crate::pathfinding::abstract_graph::{
    AbstractEdge, AbstractGraph, AbstractGraphBuilder, AbstractNode,
};

thread_local! {
    static AG: std::cell::RefCell<Option<Box<AbstractGraph>>> =
        const { std::cell::RefCell::new(None) };
    static AG_BUILT: std::cell::RefCell<Vec<AbstractGraph>> =
        const { std::cell::RefCell::new(Vec::new()) };
    static AG_TERRAIN: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
    static AG_DIRTY: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static AG_OUT: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static AG_OUT_LEN: std::cell::RefCell<i32> = const { std::cell::RefCell::new(-1) };
}

/// Queue one packed terrain byte for the next map (before `probe_ag_new`).
#[no_mangle]
pub extern "C" fn probe_ag_terrain_byte(v: u32) {
    AG_TERRAIN.with(|t| t.borrow_mut().push(v as u8));
}

/// Queue one dirty minimap tile for a partial rebuild (before `probe_ag_new`).
#[no_mangle]
pub extern "C" fn probe_ag_dirty_byte(v: f64) {
    AG_DIRTY.with(|d| d.borrow_mut().push(v));
}

#[no_mangle]
pub extern "C" fn probe_ag_new(w: f64, h: f64, cluster_size: f64, old_idx: i32) {
    let terrain = AG_TERRAIN.with(|t| std::mem::take(&mut *t.borrow_mut()));
    let dirty = AG_DIRTY.with(|d| std::mem::take(&mut *d.borrow_mut()));
    let gm = GameMap::new(w, h, terrain, 0.0);
    let cs = cluster_size as i64;
    let graph = if old_idx >= 0 {
        let old = AG_BUILT.with(|b| b.borrow()[old_idx as usize].clone());
        AbstractGraphBuilder::with_rebuild(gm, cs, Some(old), (!dirty.is_empty()).then_some(dirty))
            .build()
    } else {
        AbstractGraphBuilder::new(gm, cs).build()
    };
    AG_BUILT.with(|b| b.borrow_mut().push(graph.clone()));
    AG.with(|g| *g.borrow_mut() = Some(Box::new(graph)));
}

fn ag_node_flat(n: &AbstractNode) -> Vec<f64> {
    vec![
        n.id as f64,
        n.x as f64,
        n.y as f64,
        n.tile,
        n.component_id as f64,
    ]
}

fn ag_edge_flat(e: &AbstractEdge) -> Vec<f64> {
    vec![
        e.id as f64,
        e.node_a as f64,
        e.node_b as f64,
        e.cost,
        e.cluster_x as f64,
        e.cluster_y as f64,
    ]
}

fn set_ag_out(out: Option<Vec<f64>>) -> f64 {
    AG_OUT.with(|o| {
        let mut buf = o.borrow_mut();
        match out {
            Some(v) => {
                *buf = v;
                AG_OUT_LEN.with(|l| *l.borrow_mut() = buf.len() as i32);
            }
            None => {
                buf.clear();
                AG_OUT_LEN.with(|l| *l.borrow_mut() = -1);
            }
        }
    });
    f64::NAN
}

/// Replay one op (kind table in gen_vectors.mjs): 0=nodeCount, 1=edgeCount,
/// 2=getNode(a), 3=getEdge(a), 4=getNodeEdges(a), 5=getEdgeBetween(a,b),
/// 6=getOtherNode(edgeId=a,node=b) — NaN when the edge is missing (the TS
/// throw guard), 7=getClusterKey(a,b), 8=getCluster(cx,cy), 9=getClusterNodes,
/// 10=getNearbyClusterNodes, 11=getComponentId(a), 12=getComponentSize(a),
/// 13=getCachedPath(edgeId,fromNode), 14=setCachedPath(a,b) (void).
/// Array kinds (2,3,4,5,8,9,10,13) return NaN; read `probe_ag_out_len`/`_at`.
#[no_mangle]
pub extern "C" fn probe_ag_op(kind: u32, a: f64, b: f64) -> f64 {
    AG.with(|g| {
        let mut cell = g.borrow_mut();
        let graph = cell.as_mut().expect("probe_ag_new must be called first");
        match kind {
            0 => graph.node_count(),
            1 => graph.edge_count(),
            2 => set_ag_out(graph.get_node(a as i64).map(|n| ag_node_flat(&n))),
            3 => set_ag_out(graph.get_edge(a as i64).map(|e| ag_edge_flat(&e))),
            4 => {
                let mut v = Vec::new();
                for e in graph.get_node_edges(a as i64) {
                    v.extend(ag_edge_flat(&e));
                }
                set_ag_out(Some(v))
            }
            5 => set_ag_out(
                graph
                    .get_edge_between(a as i64, b as i64)
                    .map(|e| ag_edge_flat(&e)),
            ),
            6 => match graph.get_edge(a as i64) {
                Some(e) => AbstractGraph::get_other_node(&e, b as i64) as f64,
                None => f64::NAN,
            },
            7 => graph.get_cluster_key(a as i64, b as i64) as f64,
            8 => set_ag_out(graph.get_cluster(a as i64, b as i64).map(|c| {
                let mut v = vec![c.x as f64, c.y as f64, c.node_ids.len() as f64];
                v.extend(c.node_ids.iter().map(|&id| id as f64));
                v
            })),
            9 => {
                let mut v = Vec::new();
                for n in graph.get_cluster_nodes(a as i64, b as i64) {
                    v.extend(ag_node_flat(&n));
                }
                set_ag_out(Some(v))
            }
            10 => {
                let mut v = Vec::new();
                for n in graph.get_nearby_cluster_nodes(a as i64, b as i64) {
                    v.extend(ag_node_flat(&n));
                }
                set_ag_out(Some(v))
            }
            11 => graph.get_component_id(a) as f64,
            12 => graph.get_component_size(a),
            13 => set_ag_out(graph.get_cached_path(a as i64, b as i64)),
            14 => {
                if let Some(e) = graph.get_edge(a as i64) {
                    let dir = if b as i64 == e.node_a { 0 } else { 1 };
                    graph.set_cached_path(a as i64, b as i64, vec![a, b, a * 2.0 + dir as f64]);
                }
                f64::NAN
            }
            k => panic!("unexpected ag op kind {k}"),
        }
    })
}

#[no_mangle]
pub extern "C" fn probe_ag_out_len() -> i32 {
    AG_OUT_LEN.with(|l| *l.borrow())
}

#[no_mangle]
pub extern "C" fn probe_ag_out_at(i: usize) -> f64 {
    AG_OUT.with(|o| o.borrow()[i])
}

/// Scalar field: 0=nodeCount, 1=edgeCount, 2=pathCacheLen.
#[no_mangle]
pub extern "C" fn probe_ag_field(which: u32) -> f64 {
    AG.with(|g| {
        let cell = g.borrow();
        let graph = cell.as_ref().expect("probe_ag_new must be called first");
        match which {
            0 => graph.node_count(),
            1 => graph.edge_count(),
            2 => graph.debug_path_cache_len(),
            k => panic!("unexpected ag field {k}"),
        }
    })
}

/// Array field: 0=nodes, 1=edges, 2=clusters, 3=nodeEdgeIds (flattened like
/// the capture: nodes 5-per-entry, edges 6-per-entry, clusters
/// [x,y,count,ids...], nodeEdgeIds [count,ids...]).
#[no_mangle]
pub extern "C" fn probe_ag_arr_len(field: u32) -> usize {
    AG.with(|g| {
        let cell = g.borrow();
        let graph = cell.as_ref().expect("probe_ag_new must be called first");
        match field {
            0 => graph.debug_nodes().len(),
            1 => graph.debug_edges().len(),
            2 => graph.debug_clusters().len(),
            3 => graph.debug_node_edge_ids().len(),
            k => panic!("unexpected ag array {k}"),
        }
    })
}

#[no_mangle]
pub extern "C" fn probe_ag_arr_get(field: u32, i: usize) -> f64 {
    AG.with(|g| {
        let cell = g.borrow();
        let graph = cell.as_ref().expect("probe_ag_new must be called first");
        match field {
            0 => graph.debug_nodes()[i],
            1 => graph.debug_edges()[i],
            2 => graph.debug_clusters()[i],
            3 => graph.debug_node_edge_ids()[i],
            k => panic!("unexpected ag array {k}"),
        }
    })
}
