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

// ---- AbstractGraphAStar ----
// Hand-built graph (the P14 scenarios add nodes/edges directly, not via the
// builder): node triples `id,x,y` and edge quadruples `id,nodeA,nodeB,cost`
// are queued one field at a time before `probe_aga_new`, which news the
// graph and the engine. Each query queues its starts, then `probe_aga_run`
// dispatches on `is_multi` and returns 1/0 for path/null; the path lands in
// `AGA_PATH`, the engine + heap state is read back field-wise.

use crate::pathfinding::abstract_graph_astar::AbstractGraphAStar;

thread_local! {
    static AGA: std::cell::RefCell<Option<Box<AbstractGraphAStar>>> =
        const { std::cell::RefCell::new(None) };
    static AGA_GRAPH: std::cell::RefCell<Option<Box<AbstractGraph>>> =
        const { std::cell::RefCell::new(None) };
    static AGA_NODES: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static AGA_EDGES: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static AGA_STARTS: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static AGA_PATH: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    /// Snapshot of `debug_queue()` refreshed after every run (heap, pri bits).
    static AGA_QHEAP: std::cell::RefCell<Vec<i32>> = const { std::cell::RefCell::new(Vec::new()) };
    static AGA_QPRI: std::cell::RefCell<Vec<u32>> = const { std::cell::RefCell::new(Vec::new()) };
    static AGA_GBITS: std::cell::RefCell<Vec<u32>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Queue one node field (`id`, `x`, or `y`) for the next graph.
#[no_mangle]
pub extern "C" fn probe_aga_node(v: f64) {
    AGA_NODES.with(|n| n.borrow_mut().push(v));
}

/// Queue one edge field (`id`, `nodeA`, `nodeB`, or `cost`).
#[no_mangle]
pub extern "C" fn probe_aga_edge(v: f64) {
    AGA_EDGES.with(|e| e.borrow_mut().push(v));
}

/// Build the graph from the queued nodes/edges, then the engine sized to
/// `num_nodes` / `edge_count` with the given weight and iteration cap.
#[no_mangle]
pub extern "C" fn probe_aga_new(num_nodes: f64, edge_count: f64, weight: f64, max_iter: f64) {
    let nodes = AGA_NODES.with(|n| std::mem::take(&mut *n.borrow_mut()));
    let edges = AGA_EDGES.with(|e| std::mem::take(&mut *e.borrow_mut()));
    let mut graph = AbstractGraph::new(1, 1, 1);
    for n in nodes.chunks(3) {
        graph.add_node(AbstractNode {
            id: n[0] as i64,
            x: n[1] as i64,
            y: n[2] as i64,
            tile: 0.0,
            component_id: 0,
        });
    }
    for e in edges.chunks(4) {
        graph.add_edge(AbstractEdge {
            id: e[0] as i64,
            node_a: e[1] as i64,
            node_b: e[2] as i64,
            cost: e[3],
            cluster_x: 0,
            cluster_y: 0,
        });
    }
    let eng = AbstractGraphAStar::new(num_nodes, edge_count, Some(weight), Some(max_iter));
    AGA_GRAPH.with(|g| *g.borrow_mut() = Some(Box::new(graph)));
    AGA.with(|a| *a.borrow_mut() = Some(Box::new(eng)));
}

/// Queue one start id for the next query.
#[no_mangle]
pub extern "C" fn probe_aga_start(v: f64) {
    AGA_STARTS.with(|s| s.borrow_mut().push(v));
}

/// Run one query. `is_multi` 1 = `findPath` with an array start (replayed as
/// `find_path_multi`), 0 = scalar start (`find_path_single`, first queued
/// start). Returns 1 when a path was found, 0 for the TS `null`. Refreshes
/// the heap/gScore snapshots.
#[no_mangle]
pub extern "C" fn probe_aga_run(goal: f64, is_multi: u32) -> u8 {
    let starts = AGA_STARTS.with(|s| std::mem::take(&mut *s.borrow_mut()));
    let path = AGA.with(|a| {
        let mut cell = a.borrow_mut();
        let eng = cell.as_mut().expect("probe_aga_new must be called first");
        AGA_GRAPH.with(|g| {
            let graph = g.borrow();
            let graph = graph.as_ref().unwrap();
            if is_multi == 1 {
                eng.find_path_multi(graph, &starts, goal)
            } else {
                eng.find_path_single(graph, starts[0], goal)
            }
        })
    });
    let found = path.is_some();
    AGA_PATH.with(|p| *p.borrow_mut() = path.unwrap_or_default());
    let (heap, pri, _size, _cap) = AGA.with(|a| a.borrow().as_ref().unwrap().debug_queue());
    AGA_QHEAP.with(|h| *h.borrow_mut() = heap);
    AGA_QPRI.with(|p| *p.borrow_mut() = pri);
    let bits = AGA.with(|a| a.borrow().as_ref().unwrap().debug_g_score_bits());
    AGA_GBITS.with(|b| *b.borrow_mut() = bits);
    if found {
        1
    } else {
        0
    }
}

#[no_mangle]
pub extern "C" fn probe_aga_path_len() -> usize {
    AGA_PATH.with(|p| p.borrow().len())
}

#[no_mangle]
pub extern "C" fn probe_aga_path_at(i: usize) -> f64 {
    AGA_PATH.with(|p| p.borrow()[i])
}

#[no_mangle]
pub extern "C" fn probe_aga_stamp() -> u64 {
    AGA.with(|a| a.borrow().as_ref().unwrap().debug_stamp())
}

/// Scalar queue field: 0 = size, 1 = capacity.
#[no_mangle]
pub extern "C" fn probe_aga_qfield(which: u32) -> f64 {
    AGA.with(|a| {
        let (_, _, size, cap) = a.borrow().as_ref().unwrap().debug_queue();
        match which {
            0 => size as f64,
            _ => cap as f64,
        }
    })
}

/// Array field: 0=closedStamp, 1=gScoreStamp, 2=gScoreBits, 3=cameFrom,
/// 4=startNode, 5=queueHeap, 6=queuePriBits.
#[no_mangle]
pub extern "C" fn probe_aga_arr_len(field: u32) -> usize {
    match field {
        0 | 1 | 3 | 4 => AGA.with(|a| {
            let eng = a.borrow();
            let eng = eng.as_ref().unwrap();
            match field {
                0 => eng.debug_closed_stamp().len(),
                1 => eng.debug_g_score_stamp().len(),
                3 => eng.debug_came_from().len(),
                _ => eng.debug_start_node().len(),
            }
        }),
        2 => AGA_GBITS.with(|b| b.borrow().len()),
        5 => AGA_QHEAP.with(|h| h.borrow().len()),
        _ => AGA_QPRI.with(|p| p.borrow().len()),
    }
}

#[no_mangle]
pub extern "C" fn probe_aga_arr_get(field: u32, i: usize) -> f64 {
    match field {
        0 => AGA.with(|a| a.borrow().as_ref().unwrap().debug_closed_stamp()[i] as f64),
        1 => AGA.with(|a| a.borrow().as_ref().unwrap().debug_g_score_stamp()[i] as f64),
        2 => AGA_GBITS.with(|b| b.borrow()[i] as f64),
        3 => AGA.with(|a| a.borrow().as_ref().unwrap().debug_came_from()[i] as f64),
        4 => AGA.with(|a| a.borrow().as_ref().unwrap().debug_start_node()[i] as f64),
        5 => AGA_QHEAP.with(|h| h.borrow()[i] as f64),
        _ => AGA_QPRI.with(|p| p.borrow()[i] as f64),
    }
}

// --- AStarWaterHierarchical probe --------------------------------------------
// Terrain bytes are queued one at a time; `probe_wh_new` builds the real
// GameMap + AbstractGraphBuilder graph and the orchestrator. Each query
// queues its starts, then `probe_wh_run(goal, is_multi, rebuild_before)`
// optionally replays `setGraph` (fresh rebuild from the stored terrain),
// dispatches on `is_multi` and returns 1/0 for path/null. After every run the
// five engine stamps and the graph path cache (capture snapshot format) are
// refreshed for read-back.

use crate::pathfinding::water_hierarchical::AStarWaterHierarchical;

thread_local! {
    static WH: std::cell::RefCell<Option<Box<AStarWaterHierarchical>>> =
        const { std::cell::RefCell::new(None) };
    static WH_TERRAIN: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
    /// Retained copy of the current scenario's terrain (for `rebuild_before`).
    static WH_KEEP: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
    static WH_STARTS: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static WH_PATH: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    /// (bfs, local, multi, short, aga) stamps refreshed after every run.
    static WH_STAMPS: std::cell::RefCell<[u64; 5]> = const { std::cell::RefCell::new([0; 5]) };
    static WH_CACHE: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static WH_GEOM: std::cell::RefCell<(f64, f64, i64)> = const { std::cell::RefCell::new((0.0, 0.0, 0)) };
}

/// Queue one packed terrain byte for the next map (before `probe_wh_new`).
#[no_mangle]
pub extern "C" fn probe_wh_terrain_byte(v: u32) {
    WH_TERRAIN.with(|t| t.borrow_mut().push(v as u8));
}

/// Build the map + graph + orchestrator. The terrain is retained (kept, not
/// taken) so `rebuild_before` queries can rebuild the graph identically.
#[no_mangle]
pub extern "C" fn probe_wh_new(w: f64, h: f64, cluster_size: f64, cache_paths: u32) {
    let terrain = WH_TERRAIN.with(|t| std::mem::take(&mut *t.borrow_mut()));
    WH_KEEP.with(|k| *k.borrow_mut() = terrain.clone());
    let cs = cluster_size as i64;
    let gm = GameMap::new(w, h, terrain.clone(), w * h);
    let graph = AbstractGraphBuilder::new(gm, cs).build();
    let gm2 = GameMap::new(w, h, terrain, w * h);
    let wh = AStarWaterHierarchical::new(gm2, graph, cache_paths == 1);
    WH_GEOM.with(|g| *g.borrow_mut() = (w, h, cs));
    WH.with(|x| *x.borrow_mut() = Some(Box::new(wh)));
}

/// Queue one start tile for the next query.
#[no_mangle]
pub extern "C" fn probe_wh_start(v: f64) {
    WH_STARTS.with(|s| s.borrow_mut().push(v));
}

/// Run one query. `is_multi` 1 = array start (`find_path_multi`), 0 = scalar
/// (`find_path_single`, first queued start). `rebuild_before` 1 = setGraph
/// with a fresh rebuild first. Returns 1 when a path was found, 0 for null;
/// refreshes the stamp and cache snapshots.
#[no_mangle]
pub extern "C" fn probe_wh_run(goal: f64, is_multi: u32, rebuild_before: u32) -> u8 {
    if rebuild_before == 1 {
        let (w, h, cs) = WH_GEOM.with(|g| *g.borrow());
        let terrain = WH_KEEP.with(|k| k.borrow().clone());
        let gm = GameMap::new(w, h, terrain, w * h);
        let graph = AbstractGraphBuilder::new(gm, cs).build();
        WH.with(|x| x.borrow_mut().as_mut().unwrap().set_graph(graph));
    }
    let starts = WH_STARTS.with(|s| std::mem::take(&mut *s.borrow_mut()));
    let path = WH.with(|x| {
        let mut cell = x.borrow_mut();
        let wh = cell.as_mut().expect("probe_wh_new must be called first");
        if is_multi == 1 {
            wh.find_path_multi(&starts, goal)
        } else {
            wh.find_path_single(starts[0], goal)
        }
    });
    let found = path.is_some();
    WH_PATH.with(|p| *p.borrow_mut() = path.unwrap_or_default());
    let stamps = WH.with(|x| x.borrow().as_ref().unwrap().debug_stamps());
    WH_STAMPS.with(|s| *s.borrow_mut() = [stamps.0, stamps.1, stamps.2, stamps.3, stamps.4]);
    let cache = WH.with(|x| x.borrow().as_ref().unwrap().debug_path_cache());
    WH_CACHE.with(|c| *c.borrow_mut() = cache);
    if found {
        1
    } else {
        0
    }
}

#[no_mangle]
pub extern "C" fn probe_wh_path_len() -> usize {
    WH_PATH.with(|p| p.borrow().len())
}

#[no_mangle]
pub extern "C" fn probe_wh_path_at(i: usize) -> f64 {
    WH_PATH.with(|p| p.borrow()[i])
}

/// Stamp selector: 0=bfs, 1=local, 2=multi, 3=short, 4=aga.
#[no_mangle]
pub extern "C" fn probe_wh_stamp(which: u32) -> u64 {
    WH_STAMPS.with(|s| s.borrow()[which as usize])
}

#[no_mangle]
pub extern "C" fn probe_wh_cache_len() -> usize {
    WH_CACHE.with(|c| c.borrow().len())
}

#[no_mangle]
pub extern "C" fn probe_wh_cache_at(i: usize) -> f64 {
    WH_CACHE.with(|c| c.borrow()[i])
}

// --- Parabola (PathFinder.Parabola.ts) probe ----------------------------------
// One all-land GameMap + one ParabolaUniversalPathFinder per scenario. The
// host drives the recorded op script: control-point reads, findPath (the
// out-of-bounds throw observed via the would-throw convention — the wasm
// instance aborts on panic), and the single-instance next/invalidate/
// currentIndex walk. All doubles are compared bit-exactly by the host.

use crate::pathfinding::parabola::{
    get_parabola_control_points, DebugNext, ParabolaOptions, ParabolaUniversalPathFinder,
};

thread_local! {
    static PB: std::cell::RefCell<Option<ParabolaUniversalPathFinder<'static>>> =
        const { std::cell::RefCell::new(None) };
    static PB_KEEP: std::cell::RefCell<Option<&'static GameMap>> =
        const { std::cell::RefCell::new(None) };
    static PB_OPT: std::cell::RefCell<Option<ParabolaOptions>> =
        const { std::cell::RefCell::new(None) };
    static PB_CP: std::cell::RefCell<[f64; 8]> = const { std::cell::RefCell::new([0.0; 8]) };
    static PB_PATH: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Leak a fresh all-land GameMap and build the finder with tri-state options
/// (0 = absent, 1 = false, 2 = true; increment < 0 = absent). The leak is
/// bounded by the scenario count and keeps the probe signatures `Copy`.
#[no_mangle]
pub extern "C" fn probe_pb_new(
    w: f64,
    h: f64,
    increment: f64,
    dbh: u32,
    up: u32,
    ignore: u32,
) {
    let tri = |v: u32| match v {
        0 => None,
        1 => Some(false),
        _ => Some(true),
    };
    let opt = ParabolaOptions {
        increment: if increment < 0.0 { None } else { Some(increment) },
        distance_based_height: tri(dbh),
        direction_up: tri(up),
        ignore_map_bounds: tri(ignore),
    };
    let gm: &'static GameMap = Box::leak(Box::new(GameMap::new(
        w,
        h,
        vec![0x03; (w * h) as usize],
        w * h,
    )));
    PB_KEEP.with(|k| *k.borrow_mut() = Some(gm));
    PB_OPT.with(|o| *o.borrow_mut() = Some(opt));
    PB.with(|p| *p.borrow_mut() = Some(ParabolaUniversalPathFinder::new(gm, Some(opt))));
}

/// getParabolaControlPoints; results readable via `probe_pb_cp_at`.
#[no_mangle]
pub extern "C" fn probe_pb_cp(from: f64, to: f64) {
    let gm = PB_KEEP.with(|k| k.borrow().unwrap());
    let opt = PB_OPT.with(|o| *o.borrow());
    let cps = get_parabola_control_points(gm, from, to, opt.as_ref());
    PB_CP.with(|c| {
        *c.borrow_mut() = [
            cps[0].x, cps[0].y, cps[1].x, cps[1].y, cps[2].x, cps[2].y, cps[3].x, cps[3].y,
        ]
    });
}

#[no_mangle]
pub extern "C" fn probe_pb_cp_at(i: usize) -> f64 {
    PB_CP.with(|c| c.borrow()[i])
}

/// findPath via the would-throw variant. Returns 1 = path, 0 = threw.
#[no_mangle]
pub extern "C" fn probe_pb_find(from: f64, to: f64) -> u8 {
    let path = PB.with(|p| p.borrow().as_ref().unwrap().debug_find_path(from, to));
    match path {
        Some(v) => {
            PB_PATH.with(|x| *x.borrow_mut() = v);
            1
        }
        None => {
            PB_PATH.with(|x| x.borrow_mut().clear());
            0
        }
    }
}

#[no_mangle]
pub extern "C" fn probe_pb_path_len() -> usize {
    PB_PATH.with(|p| p.borrow().len())
}

#[no_mangle]
pub extern "C" fn probe_pb_path_at(i: usize) -> f64 {
    PB_PATH.with(|p| p.borrow()[i])
}

/// next() via the would-throw variant. Returns 0 = NEXT (node readable via
/// `probe_pb_node`), 2 = COMPLETE (node = to), 1 = threw.
#[no_mangle]
pub extern "C" fn probe_pb_next(from: f64, to: f64, speed: f64) -> u8 {
    let r = PB.with(|p| {
        p.borrow_mut()
            .as_mut()
            .unwrap()
            .debug_next(from, to, if speed < 0.0 { None } else { Some(speed) })
    });
    match r {
        DebugNext::Next(n) => {
            PB_NODE.with(|x| *x.borrow_mut() = n);
            0
        }
        DebugNext::Complete(n) => {
            PB_NODE.with(|x| *x.borrow_mut() = n);
            2
        }
        DebugNext::Threw => 1,
    }
}

thread_local! {
    static PB_NODE: std::cell::RefCell<f64> = const { std::cell::RefCell::new(0.0) };
}

#[no_mangle]
pub extern "C" fn probe_pb_node() -> f64 {
    PB_NODE.with(|x| *x.borrow())
}

#[no_mangle]
pub extern "C" fn probe_pb_invalidate() {
    PB.with(|p| p.borrow_mut().as_mut().unwrap().invalidate());
}

#[no_mangle]
pub extern "C" fn probe_pb_index() -> u64 {
    PB.with(|p| p.borrow().as_ref().unwrap().current_index() as u64)
}

// --- MiniMapTransformer (transformers/MiniMapTransformer.ts) -----------------
// One all-land main + mini GameMap pair per scenario (leaked). The host drives
// the recorded query script: start/goal refs, the scripted inner result, and
// observes both what the transformer passed the inner finder (the scalar/array
// collapse + minimap downscale) and the upscaled, endpoint-repaired result.
// The two `ref` throw classes (downscale / upscale out of bounds) are observed
// via the would-throw convention; the panicking semantics are pinned by the
// native replay test.

use crate::pathfinding::mini_map_transformer::{MiniMapTransformer, ScriptedFinder};
use crate::pathfinding::PathStart;

thread_local! {
    static MMT_MAIN: std::cell::RefCell<Option<&'static GameMap>> =
        const { std::cell::RefCell::new(None) };
    static MMT_MINI: std::cell::RefCell<Option<&'static GameMap>> =
        const { std::cell::RefCell::new(None) };
    static MMT_FROM: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static MMT_INNER: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static MMT_OUT: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static MMT_SEEN: std::cell::RefCell<Option<(bool, Vec<f64>, f64)>> =
        const { std::cell::RefCell::new(None) };
}

/// Leak a fresh all-land main + mini GameMap pair.
#[no_mangle]
pub extern "C" fn probe_mmt_new(mw: f64, mh: f64, mini_w: f64, mini_h: f64) {
    let main: &'static GameMap = Box::leak(Box::new(GameMap::new(
        mw,
        mh,
        vec![0x03; (mw * mh) as usize],
        mw * mh,
    )));
    let mini: &'static GameMap = Box::leak(Box::new(GameMap::new(
        mini_w,
        mini_h,
        vec![0x03; (mini_w * mini_h) as usize],
        mini_w * mini_h,
    )));
    MMT_MAIN.with(|m| *m.borrow_mut() = Some(main));
    MMT_MINI.with(|m| *m.borrow_mut() = Some(mini));
}

/// Clear the per-query start / inner / seen buffers.
#[no_mangle]
pub extern "C" fn probe_mmt_reset() {
    MMT_FROM.with(|f| f.borrow_mut().clear());
    MMT_INNER.with(|i| i.borrow_mut().clear());
    MMT_SEEN.with(|s| *s.borrow_mut() = None);
}

/// Queue one main-map start ref.
#[no_mangle]
pub extern "C" fn probe_mmt_from(v: f64) {
    MMT_FROM.with(|f| f.borrow_mut().push(v));
}

/// Queue one mini-map inner tile ref.
#[no_mangle]
pub extern "C" fn probe_mmt_inner(v: f64) {
    MMT_INNER.with(|i| i.borrow_mut().push(v));
}

/// Run one query. `inner_mode` 0 = null, 1 = empty, 2 = queued tiles. Returns
/// 0 = null, 1 = threw (downscale or upscale), 2 = path (readable via
/// `probe_mmt_out_*`). The inner stub's observation is readable via
/// `probe_mmt_seen_*`.
#[no_mangle]
pub extern "C" fn probe_mmt_run(to: f64, from_is_array: u32, inner_mode: u32) -> u8 {
    let main = MMT_MAIN.with(|m| m.borrow().unwrap());
    let mini = MMT_MINI.with(|m| m.borrow().unwrap());
    let from = MMT_FROM.with(|f| f.borrow().clone());
    let inner = MMT_INNER.with(|i| i.borrow().clone());
    let mut stub = ScriptedFinder::default();
    stub.push_path(match inner_mode {
        0 => None,
        1 => Some(vec![]),
        _ => Some(inner),
    });
    let starts = if from_is_array == 1 {
        PathStart::Multi(&from)
    } else {
        PathStart::Single(from[0])
    };
    let r = {
        let mut tr = MiniMapTransformer::new(&mut stub, main, mini);
        tr.debug_find_path(starts, to)
    };
    MMT_SEEN.with(|s| *s.borrow_mut() = stub.last_seen);
    match r {
        Err(_) => 1,
        Ok(None) => {
            MMT_OUT.with(|o| o.borrow_mut().clear());
            0
        }
        Ok(Some(v)) => {
            MMT_OUT.with(|o| *o.borrow_mut() = v);
            2
        }
    }
}

#[no_mangle]
pub extern "C" fn probe_mmt_out_len() -> usize {
    MMT_OUT.with(|o| o.borrow().len())
}

#[no_mangle]
pub extern "C" fn probe_mmt_out_at(i: usize) -> f64 {
    MMT_OUT.with(|o| o.borrow()[i])
}

#[no_mangle]
pub extern "C" fn probe_mmt_seen_flag() -> u8 {
    MMT_SEEN.with(|s| s.borrow().is_some() as u8)
}

#[no_mangle]
pub extern "C" fn probe_mmt_seen_multi() -> u8 {
    MMT_SEEN.with(|s| s.borrow().as_ref().map_or(0, |(m, _, _)| *m as u8))
}

#[no_mangle]
pub extern "C" fn probe_mmt_seen_len() -> usize {
    MMT_SEEN.with(|s| s.borrow().as_ref().map_or(0, |(_, t, _)| t.len()))
}

#[no_mangle]
pub extern "C" fn probe_mmt_seen_at(i: usize) -> f64 {
    MMT_SEEN.with(|s| s.borrow().as_ref().unwrap().1[i])
}

#[no_mangle]
pub extern "C" fn probe_mmt_seen_goal() -> f64 {
    MMT_SEEN.with(|s| s.borrow().as_ref().unwrap().2)
}

// ============================ P18: PathFinderStepper ============================
// The stepper never throws here (the stub finder cannot), so no would-throw
// convention is needed; every observable of the TS trace (status, node,
// pathAfterNext, pathIndex, hasPath, calls, findPath out/seen) is exposed.

use crate::pathfinding::parabola::PathResult;
use crate::pathfinding::stepper::{PathFinderStepper, SharedStub};

thread_local! {
    static SP: std::cell::RefCell<Option<PathFinderStepper<'static, SharedStub>>> =
        const { std::cell::RefCell::new(None) };
    static SP_STUB: std::cell::RefCell<SharedStub> =
        std::cell::RefCell::new(SharedStub::default());
    static SP_PEND: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static SP_FROM: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static SP_PAN: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static SP_PAN_NULL: std::cell::RefCell<bool> = const { std::cell::RefCell::new(true) };
    static SP_OUT: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static SP_SEEN: std::cell::RefCell<Option<(bool, Vec<f64>, f64)>> =
        const { std::cell::RefCell::new(None) };
    static SP_NODE: std::cell::RefCell<f64> = const { std::cell::RefCell::new(-1.0) };
}

/// Build the stepper over a fresh shared stub; `prod != 0` selects the
/// production `tileStepperConfig` shape (leaks a 10x10 all-land map).
#[no_mangle]
pub extern "C" fn probe_sp_new(prod: u32) {
    let gm: &'static GameMap =
        Box::leak(Box::new(GameMap::new(10.0, 10.0, vec![0x03; 100], 100.0)));
    let stub = SharedStub::default();
    SP_STUB.with(|s| *s.borrow_mut() = stub.clone());
    let st = PathFinderStepper::new(stub, if prod == 0 { None } else { Some(gm) });
    SP.with(|c| *c.borrow_mut() = Some(st));
}

/// `stub.reset()` — clear queue + observation (calls persists).
#[no_mangle]
pub extern "C" fn probe_sp_reset() {
    SP_STUB.with(|s| s.borrow().reset());
}

/// `stepper.invalidate()`.
#[no_mangle]
pub extern "C" fn probe_sp_inv() {
    SP.with(|c| c.borrow_mut().as_mut().unwrap().invalidate());
}

/// Queue a `null` inner result.
#[no_mangle]
pub extern "C" fn probe_sp_push_null() {
    SP_STUB.with(|s| s.borrow().push_null());
}

/// Begin a queued tile-list result.
#[no_mangle]
pub extern "C" fn probe_sp_push_start() {
    SP_PEND.with(|p| p.borrow_mut().clear());
}

/// Append one tile to the pending list.
#[no_mangle]
pub extern "C" fn probe_sp_push_tile(v: f64) {
    SP_PEND.with(|p| p.borrow_mut().push(v));
}

/// Finish + queue the pending list.
#[no_mangle]
pub extern "C" fn probe_sp_push_end() {
    let tiles = SP_PEND.with(|p| p.borrow_mut().clone());
    SP_STUB.with(|s| s.borrow().push_list(tiles));
}

/// `next(from, to, dist)`; `dist < 0` means `undefined`. Returns the status
/// code (0 NEXT, 2 COMPLETE, 3 NOT_FOUND). Side observables: `probe_sp_node`,
/// `probe_sp_pan_*`, `probe_sp_idx`, `probe_sp_has_path`, `probe_sp_calls`.
#[no_mangle]
pub extern "C" fn probe_sp_next(from: f64, to: f64, dist: f64) -> u8 {
    let r = SP.with(|c| {
        let mut cell = c.borrow_mut();
        let st = cell.as_mut().unwrap();
        let r = st.next(from, to, if dist < 0.0 { None } else { Some(dist) });
        let pan = st.path_after_next();
        let idx = st.debug_path_index();
        let has = st.debug_has_path();
        (r, pan, idx, has)
    });
    let status = match r.0 {
        PathResult::Next { node, .. } => {
            SP_NODE.with(|n| *n.borrow_mut() = node);
            0
        }
        PathResult::Complete { node, .. } => {
            SP_NODE.with(|n| *n.borrow_mut() = node);
            2
        }
        PathResult::NotFound { .. } => {
            SP_NODE.with(|n| *n.borrow_mut() = -1.0);
            3
        }
    };
    match r.1 {
        None => {
            SP_PAN.with(|p| p.borrow_mut().clear());
            SP_PAN_NULL.with(|p| *p.borrow_mut() = true);
        }
        Some(v) => {
            SP_PAN.with(|p| *p.borrow_mut() = v);
            SP_PAN_NULL.with(|p| *p.borrow_mut() = false);
        }
    }
    SP_IDX.with(|v| *v.borrow_mut() = r.2);
    SP_HAS.with(|v| *v.borrow_mut() = r.3 as u8);
    status
}

thread_local! {
    static SP_IDX: std::cell::RefCell<usize> = const { std::cell::RefCell::new(0) };
    static SP_HAS: std::cell::RefCell<u8> = const { std::cell::RefCell::new(0) };
}

#[no_mangle]
pub extern "C" fn probe_sp_node() -> f64 {
    SP_NODE.with(|n| *n.borrow())
}

#[no_mangle]
pub extern "C" fn probe_sp_pan_null() -> u8 {
    SP_PAN_NULL.with(|p| *p.borrow() as u8)
}

#[no_mangle]
pub extern "C" fn probe_sp_pan_len() -> usize {
    SP_PAN.with(|p| p.borrow().len())
}

#[no_mangle]
pub extern "C" fn probe_sp_pan_at(i: usize) -> f64 {
    SP_PAN.with(|p| p.borrow()[i])
}

#[no_mangle]
pub extern "C" fn probe_sp_idx() -> usize {
    SP_IDX.with(|v| *v.borrow())
}

#[no_mangle]
pub extern "C" fn probe_sp_has_path() -> u8 {
    SP_HAS.with(|v| *v.borrow())
}

#[no_mangle]
pub extern "C" fn probe_sp_calls() -> f64 {
    SP_STUB.with(|s| s.borrow().calls() as f64)
}

/// Clear the findPath start buffer.
#[no_mangle]
pub extern "C" fn probe_sp_from_reset() {
    SP_FROM.with(|f| f.borrow_mut().clear());
}

/// Append one start tile.
#[no_mangle]
pub extern "C" fn probe_sp_from_push(v: f64) {
    SP_FROM.with(|f| f.borrow_mut().push(v));
}

/// `findPath(from, to)`; `is_multi != 0` passes the array, else the scalar
/// first tile. Returns 0 = null, 2 = path (`probe_sp_out_*`). Inner
/// observation via `probe_sp_seen_*`, call count via `probe_sp_calls`.
#[no_mangle]
pub extern "C" fn probe_sp_find_path(to: f64, is_multi: u32) -> u8 {
    let from = SP_FROM.with(|f| f.borrow().clone());
    let starts = if is_multi == 1 {
        PathStart::Multi(&from)
    } else {
        PathStart::Single(from[0])
    };
    let out = SP.with(|c| c.borrow_mut().as_mut().unwrap().find_path(starts, to));
    SP_SEEN.with(|s| *s.borrow_mut() = SP_STUB.with(|st| st.borrow().last_seen()));
    match out {
        None => {
            SP_OUT.with(|o| o.borrow_mut().clear());
            0
        }
        Some(v) => {
            SP_OUT.with(|o| *o.borrow_mut() = v);
            2
        }
    }
}

#[no_mangle]
pub extern "C" fn probe_sp_out_len() -> usize {
    SP_OUT.with(|o| o.borrow().len())
}

#[no_mangle]
pub extern "C" fn probe_sp_out_at(i: usize) -> f64 {
    SP_OUT.with(|o| o.borrow()[i])
}

#[no_mangle]
pub extern "C" fn probe_sp_seen_flag() -> u8 {
    SP_SEEN.with(|s| s.borrow().is_some() as u8)
}

#[no_mangle]
pub extern "C" fn probe_sp_seen_multi() -> u8 {
    SP_SEEN.with(|s| s.borrow().as_ref().map_or(0, |(m, _, _)| *m as u8))
}

#[no_mangle]
pub extern "C" fn probe_sp_seen_len() -> usize {
    SP_SEEN.with(|s| s.borrow().as_ref().map_or(0, |(_, t, _)| t.len()))
}

#[no_mangle]
pub extern "C" fn probe_sp_seen_at(i: usize) -> f64 {
    SP_SEEN.with(|s| s.borrow().as_ref().unwrap().1[i])
}

#[no_mangle]
pub extern "C" fn probe_sp_seen_goal() -> f64 {
    SP_SEEN.with(|s| s.borrow().as_ref().unwrap().2)
}

// ======================= P19: ComponentCheckTransformer =======================
// The transformer never throws (no `ref` construction), so no would-throw
// convention is needed. Output is the pass-through of inner's result, pinned
// by (seen_flag, inner_mode); the observable of interest is what the
// transformer handed to `inner` (the filtered PathStart) and whether `inner`
// ran at all.

use crate::pathfinding::component_check_transformer::{
    ComponentCheckTransformer, TableGetter,
};

thread_local! {
    static CCT_TABLE: std::cell::RefCell<Vec<(f64, i64)>> =
        const { std::cell::RefCell::new(Vec::new()) };
    static CCT_DEFAULT: std::cell::RefCell<i64> = const { std::cell::RefCell::new(0) };
    static CCT_FROM: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static CCT_INNER: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static CCT_OUT: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static CCT_SEEN: std::cell::RefCell<Option<(bool, Vec<f64>, f64)>> =
        const { std::cell::RefCell::new(None) };
}

/// Clear the component table + set the default id.
#[no_mangle]
pub extern "C" fn probe_cct_new(default: f64) {
    CCT_TABLE.with(|t| t.borrow_mut().clear());
    CCT_DEFAULT.with(|d| *d.borrow_mut() = default as i64);
}

/// Append one `(ref, component_id)` pair to the table.
#[no_mangle]
pub extern "C" fn probe_cct_pair(v: f64, c: f64) {
    CCT_TABLE.with(|t| t.borrow_mut().push((v, c as i64)));
}

/// Clear the per-query start / inner / seen buffers.
#[no_mangle]
pub extern "C" fn probe_cct_reset() {
    CCT_FROM.with(|f| f.borrow_mut().clear());
    CCT_INNER.with(|i| i.borrow_mut().clear());
    CCT_SEEN.with(|s| *s.borrow_mut() = None);
}

/// Queue one start ref.
#[no_mangle]
pub extern "C" fn probe_cct_from(v: f64) {
    CCT_FROM.with(|f| f.borrow_mut().push(v));
}

/// Queue one inner result tile ref.
#[no_mangle]
pub extern "C" fn probe_cct_inner(v: f64) {
    CCT_INNER.with(|i| i.borrow_mut().push(v));
}

/// Run one query. `inner_mode` 0 = null, 2 = queued tiles. Returns 0 = null,
/// 2 = path (`probe_cct_out_*`). Inner observation via `probe_cct_seen_*`.
#[no_mangle]
pub extern "C" fn probe_cct_run(to: f64, from_is_array: u32, inner_mode: u32) -> u8 {
    let table = CCT_TABLE.with(|t| t.borrow().clone());
    let default = CCT_DEFAULT.with(|d| *d.borrow());
    let from = CCT_FROM.with(|f| f.borrow().clone());
    let inner = CCT_INNER.with(|i| i.borrow().clone());
    let mut getter = TableGetter::default();
    getter.set_table(table, default);
    let mut stub = ScriptedFinder::default();
    stub.push_path(match inner_mode {
        0 => None,
        _ => Some(inner),
    });
    let starts = if from_is_array == 1 {
        PathStart::Multi(&from)
    } else {
        PathStart::Single(from[0])
    };
    let r = {
        let mut tr = ComponentCheckTransformer::new(&mut stub, getter);
        tr.find_path(starts, to)
    };
    CCT_SEEN.with(|s| *s.borrow_mut() = stub.last_seen);
    match r {
        None => {
            CCT_OUT.with(|o| o.borrow_mut().clear());
            0
        }
        Some(v) => {
            CCT_OUT.with(|o| *o.borrow_mut() = v);
            2
        }
    }
}

#[no_mangle]
pub extern "C" fn probe_cct_out_len() -> usize {
    CCT_OUT.with(|o| o.borrow().len())
}

#[no_mangle]
pub extern "C" fn probe_cct_out_at(i: usize) -> f64 {
    CCT_OUT.with(|o| o.borrow()[i])
}

#[no_mangle]
pub extern "C" fn probe_cct_seen_flag() -> u8 {
    CCT_SEEN.with(|s| s.borrow().is_some() as u8)
}

#[no_mangle]
pub extern "C" fn probe_cct_seen_multi() -> u8 {
    CCT_SEEN.with(|s| s.borrow().as_ref().map_or(0, |(m, _, _)| *m as u8))
}

#[no_mangle]
pub extern "C" fn probe_cct_seen_len() -> usize {
    CCT_SEEN.with(|s| s.borrow().as_ref().map_or(0, |(_, t, _)| t.len()))
}

#[no_mangle]
pub extern "C" fn probe_cct_seen_at(i: usize) -> f64 {
    CCT_SEEN.with(|s| s.borrow().as_ref().unwrap().1[i])
}

#[no_mangle]
pub extern "C" fn probe_cct_seen_goal() -> f64 {
    CCT_SEEN.with(|s| s.borrow().as_ref().unwrap().2)
}

// ========================= P19: ShoreCoercingTransformer ======================
// No throws here either (the stub cannot); terrain reads are in-bounds for the
// scripted maps. Every observable is the coerced PathStart `inner` received
// (scalar collapse + duplicate starts) and the restored/extended output path.

use crate::pathfinding::shore_coercing_transformer::ShoreCoercingTransformer;

thread_local! {
    static SCT_MAP: std::cell::RefCell<Option<&'static GameMap>> =
        const { std::cell::RefCell::new(None) };
    static SCT_WATER: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static SCT_FROM: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static SCT_INNER: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static SCT_OUT: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static SCT_SEEN: std::cell::RefCell<Option<(bool, Vec<f64>, f64)>> =
        const { std::cell::RefCell::new(None) };
}

/// Clear the pending water-coord buffer.
#[no_mangle]
pub extern "C" fn probe_sct_water_reset() {
    SCT_WATER.with(|w| w.borrow_mut().clear());
}

/// Append one water `(x, y)` coord (consumed by `probe_sct_new`).
#[no_mangle]
pub extern "C" fn probe_sct_water(x: f64, y: f64) {
    SCT_WATER.with(|w| {
        w.borrow_mut().push(x);
        w.borrow_mut().push(y);
    });
}

/// Build the land/water map (default 0x83 land, queued coords -> 0x03 water)
/// and leak it.
#[no_mangle]
pub extern "C" fn probe_sct_new(w: f64, h: f64) {
    let water = SCT_WATER.with(|x| x.borrow().clone());
    let mut data = vec![0x83u8; (w * h) as usize];
    for pair in water.chunks(2) {
        data[pair[1] as usize * w as usize + pair[0] as usize] = 0x03;
    }
    let gm: &'static GameMap = Box::leak(Box::new(GameMap::new(
        w,
        h,
        data,
        (w * h - water.len() as f64 / 2.0).max(0.0),
    )));
    SCT_MAP.with(|m| *m.borrow_mut() = Some(gm));
}

/// Clear the per-query start / inner / seen buffers.
#[no_mangle]
pub extern "C" fn probe_sct_reset() {
    SCT_FROM.with(|f| f.borrow_mut().clear());
    SCT_INNER.with(|i| i.borrow_mut().clear());
    SCT_SEEN.with(|s| *s.borrow_mut() = None);
}

/// Queue one start ref.
#[no_mangle]
pub extern "C" fn probe_sct_from(v: f64) {
    SCT_FROM.with(|f| f.borrow_mut().push(v));
}

/// Queue one inner result tile ref.
#[no_mangle]
pub extern "C" fn probe_sct_inner(v: f64) {
    SCT_INNER.with(|i| i.borrow_mut().push(v));
}

/// Run one query. `inner_mode` 0 = null, 1 = empty, 2 = queued tiles. Returns
/// 0 = null, 2 = path (`probe_sct_out_*`). Inner observation via
/// `probe_sct_seen_*`.
#[no_mangle]
pub extern "C" fn probe_sct_run(to: f64, from_is_array: u32, inner_mode: u32) -> u8 {
    let gm = SCT_MAP.with(|m| m.borrow().unwrap());
    let from = SCT_FROM.with(|f| f.borrow().clone());
    let inner = SCT_INNER.with(|i| i.borrow().clone());
    let mut stub = ScriptedFinder::default();
    stub.push_path(match inner_mode {
        0 => None,
        1 => Some(vec![]),
        _ => Some(inner),
    });
    let starts = if from_is_array == 1 {
        PathStart::Multi(&from)
    } else {
        PathStart::Single(from[0])
    };
    let r = {
        let mut tr = ShoreCoercingTransformer::new(&mut stub, gm);
        tr.find_path(starts, to)
    };
    SCT_SEEN.with(|s| *s.borrow_mut() = stub.last_seen);
    match r {
        None => {
            SCT_OUT.with(|o| o.borrow_mut().clear());
            0
        }
        Some(v) => {
            SCT_OUT.with(|o| *o.borrow_mut() = v);
            2
        }
    }
}

#[no_mangle]
pub extern "C" fn probe_sct_out_len() -> usize {
    SCT_OUT.with(|o| o.borrow().len())
}

#[no_mangle]
pub extern "C" fn probe_sct_out_at(i: usize) -> f64 {
    SCT_OUT.with(|o| o.borrow()[i])
}

#[no_mangle]
pub extern "C" fn probe_sct_seen_flag() -> u8 {
    SCT_SEEN.with(|s| s.borrow().is_some() as u8)
}

#[no_mangle]
pub extern "C" fn probe_sct_seen_multi() -> u8 {
    SCT_SEEN.with(|s| s.borrow().as_ref().map_or(0, |(m, _, _)| *m as u8))
}

#[no_mangle]
pub extern "C" fn probe_sct_seen_len() -> usize {
    SCT_SEEN.with(|s| s.borrow().as_ref().map_or(0, |(_, t, _)| t.len()))
}

#[no_mangle]
pub extern "C" fn probe_sct_seen_at(i: usize) -> f64 {
    SCT_SEEN.with(|s| s.borrow().as_ref().unwrap().1[i])
}

#[no_mangle]
pub extern "C" fn probe_sct_seen_goal() -> f64 {
    SCT_SEEN.with(|s| s.borrow().as_ref().unwrap().2)
}

// ========================= P20: SmoothingWaterTransformer =====================
// No throws (the stub cannot); terrain reads are in-bounds for the scripted
// maps. Observables: the PathStart `inner` received (the union forwarded
// untouched) and the smoothed output path.

use crate::pathfinding::smoothing_water_transformer::{
    SmoothingWaterTransformer, WaterTraversable,
};

thread_local! {
    static SWT_MAP: std::cell::RefCell<Option<&'static GameMap>> =
        const { std::cell::RefCell::new(None) };
    static SWT_CELLS: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static SWT_FROM: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static SWT_INNER: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static SWT_OUT: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static SWT_SEEN: std::cell::RefCell<Option<(bool, Vec<f64>, f64)>> =
        const { std::cell::RefCell::new(None) };
}

/// Clear the pending cell buffer.
#[no_mangle]
pub extern "C" fn probe_swt_cell_reset() {
    SWT_CELLS.with(|c| c.borrow_mut().clear());
}

/// Append one `[x, y, byte]` terrain override (consumed by `probe_swt_new`).
#[no_mangle]
pub extern "C" fn probe_swt_cell(x: f64, y: f64, b: f64) {
    SWT_CELLS.with(|c| {
        c.borrow_mut().push(x);
        c.borrow_mut().push(y);
        c.borrow_mut().push(b);
    });
}

/// Build the water map (default 0x83 land, queued cells -> explicit bytes)
/// and leak it.
#[no_mangle]
pub extern "C" fn probe_swt_new(w: f64, h: f64) {
    let cells = SWT_CELLS.with(|c| c.borrow().clone());
    let mut data = vec![0x83u8; (w * h) as usize];
    for t in cells.chunks(3) {
        data[t[1] as usize * w as usize + t[0] as usize] = t[2] as u8;
    }
    let land = data.iter().filter(|b| *b & 0x80 != 0).count() as f64;
    let gm: &'static GameMap = Box::leak(Box::new(GameMap::new(w, h, data, land)));
    SWT_MAP.with(|m| *m.borrow_mut() = Some(gm));
}

/// Clear the per-query start / inner / seen buffers.
#[no_mangle]
pub extern "C" fn probe_swt_reset() {
    SWT_FROM.with(|f| f.borrow_mut().clear());
    SWT_INNER.with(|i| i.borrow_mut().clear());
    SWT_SEEN.with(|s| *s.borrow_mut() = None);
}

/// Queue one start ref.
#[no_mangle]
pub extern "C" fn probe_swt_from(v: f64) {
    SWT_FROM.with(|f| f.borrow_mut().push(v));
}

/// Queue one inner result tile ref.
#[no_mangle]
pub extern "C" fn probe_swt_inner(v: f64) {
    SWT_INNER.with(|i| i.borrow_mut().push(v));
}

/// Run one query. `inner_mode` 0 = null, 1 = empty, 2 = queued tiles. Returns
/// 0 = null, 2 = path (`probe_swt_out_*`). Inner observation via
/// `probe_swt_seen_*`.
#[no_mangle]
pub extern "C" fn probe_swt_run(to: f64, from_is_array: u32, inner_mode: u32) -> u8 {
    let gm = SWT_MAP.with(|m| m.borrow().unwrap());
    let from = SWT_FROM.with(|f| f.borrow().clone());
    let inner = SWT_INNER.with(|i| i.borrow().clone());
    let mut stub = ScriptedFinder::default();
    stub.push_path(match inner_mode {
        0 => None,
        1 => Some(vec![]),
        _ => Some(inner),
    });
    let starts = if from_is_array == 1 {
        PathStart::Multi(&from)
    } else {
        PathStart::Single(from[0])
    };
    let r = {
        let mut tr = SmoothingWaterTransformer::new(&mut stub, gm, WaterTraversable(gm));
        tr.find_path(starts, to)
    };
    SWT_SEEN.with(|s| *s.borrow_mut() = stub.last_seen);
    match r {
        None => {
            SWT_OUT.with(|o| o.borrow_mut().clear());
            0
        }
        Some(v) => {
            SWT_OUT.with(|o| *o.borrow_mut() = v);
            2
        }
    }
}

#[no_mangle]
pub extern "C" fn probe_swt_out_len() -> usize {
    SWT_OUT.with(|o| o.borrow().len())
}

#[no_mangle]
pub extern "C" fn probe_swt_out_at(i: usize) -> f64 {
    SWT_OUT.with(|o| o.borrow()[i])
}

#[no_mangle]
pub extern "C" fn probe_swt_seen_flag() -> u8 {
    SWT_SEEN.with(|s| s.borrow().is_some() as u8)
}

#[no_mangle]
pub extern "C" fn probe_swt_seen_multi() -> u8 {
    SWT_SEEN.with(|s| s.borrow().as_ref().map_or(0, |(m, _, _)| *m as u8))
}

#[no_mangle]
pub extern "C" fn probe_swt_seen_len() -> usize {
    SWT_SEEN.with(|s| s.borrow().as_ref().map_or(0, |(_, t, _)| t.len()))
}

#[no_mangle]
pub extern "C" fn probe_swt_seen_at(i: usize) -> f64 {
    SWT_SEEN.with(|s| s.borrow().as_ref().unwrap().1[i])
}

#[no_mangle]
pub extern "C" fn probe_swt_seen_goal() -> f64 {
    SWT_SEEN.with(|s| s.borrow().as_ref().unwrap().2)
}

// ========================= P21: BFS (BFS.ts) ==================================

thread_local! {
    /// Flat edge table: `BFS_EDGE_KEYS[i]` has neighbours `BFS_EDGE_NBS[i]`.
    static BFS_EDGE_KEYS: std::cell::RefCell<Vec<f64>> =
        const { std::cell::RefCell::new(Vec::new()) };
    static BFS_EDGE_NBS: std::cell::RefCell<Vec<Vec<f64>>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// The neighbour list currently being built (consumed by `probe_bfs_edge_end`).
    static BFS_EDGE_CUR: std::cell::RefCell<Vec<f64>> =
        const { std::cell::RefCell::new(Vec::new()) };
    static BFS_STARTS: std::cell::RefCell<Vec<f64>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// Recorded visitor stream as flat [node, dist] pairs.
    static BFS_VISITS: std::cell::RefCell<Vec<f64>> =
        const { std::cell::RefCell::new(Vec::new()) };
    static BFS_RESULT: std::cell::RefCell<Option<f64>> =
        const { std::cell::RefCell::new(None) };
}

struct ProbeBfsAdapter {
    keys: Vec<f64>,
    nbrs: Vec<Vec<f64>>,
}

impl crate::pathfinding::bfs::BfsAdapter for ProbeBfsAdapter {
    fn neighbors(&mut self, node: f64) -> Vec<f64> {
        for (i, &k) in self.keys.iter().enumerate() {
            if k == node || (k.is_nan() && node.is_nan()) {
                return self.nbrs[i].clone();
            }
        }
        Vec::new()
    }
}

/// Clear the edge table.
#[no_mangle]
pub extern "C" fn probe_bfs_edge_reset() {
    BFS_EDGE_KEYS.with(|k| k.borrow_mut().clear());
    BFS_EDGE_NBS.with(|n| n.borrow_mut().clear());
    BFS_EDGE_CUR.with(|c| c.borrow_mut().clear());
}

/// Begin an edge entry keyed by `k` (neighbours follow via `probe_bfs_edge_nb`).
#[no_mangle]
pub extern "C"
fn probe_bfs_edge_key(k: f64) {
    BFS_EDGE_CUR.with(|c| c.borrow_mut().clear());
    BFS_EDGE_KEYS.with(|ks| ks.borrow_mut().push(k));
}

/// Append one neighbour to the current edge entry.
#[no_mangle]
pub extern "C"
fn probe_bfs_edge_nb(n: f64) {
    BFS_EDGE_CUR.with(|c| c.borrow_mut().push(n));
}

/// Finalise the current edge entry (call after its last `probe_bfs_edge_nb`).
#[no_mangle]
pub extern "C"
fn probe_bfs_edge_end() {
    let cur = BFS_EDGE_CUR.with(|c| c.borrow().clone());
    BFS_EDGE_NBS.with(|n| n.borrow_mut().push(cur));
}

/// Clear the per-run start / visit / result buffers.
#[no_mangle]
pub extern "C" fn probe_bfs_reset() {
    BFS_STARTS.with(|s| s.borrow_mut().clear());
    BFS_VISITS.with(|v| v.borrow_mut().clear());
    BFS_RESULT.with(|r| *r.borrow_mut() = None);
}

/// Queue one start node.
#[no_mangle]
pub extern "C" fn probe_bfs_start(v: f64) {
    BFS_STARTS.with(|s| s.borrow_mut().push(v));
}

/// Run one search. `mode` 0 = explore-all, 1 = reject `blocker`, 2 = found
/// `blocker` returning `foundval`. Returns 0 = null, 1 = found (read the value
/// via `probe_bfs_result`).
#[no_mangle]
pub extern "C" fn probe_bfs_run(max_d: f64, mode: u32, blocker: f64, foundval: f64) -> u8 {
    let keys = BFS_EDGE_KEYS.with(|k| k.borrow().clone());
    let nbrs = BFS_EDGE_NBS.with(|n| n.borrow().clone());
    let starts = BFS_STARTS.with(|s| s.borrow().clone());
    let mut bfs = crate::pathfinding::bfs::Bfs::new(ProbeBfsAdapter { keys, nbrs });
    let r = bfs.search(
        crate::pathfinding::PathStart::Multi(&starts),
        max_d,
        |n: f64, d: f64| -> crate::pathfinding::Visit<f64> {
            BFS_VISITS.with(|v| {
                v.borrow_mut().push(n);
                v.borrow_mut().push(d);
            });
            let is_blocker = n == blocker || (n.is_nan() && blocker.is_nan());
            if is_blocker && mode == 1 {
                crate::pathfinding::Visit::Reject
            } else if is_blocker && mode == 2 {
                crate::pathfinding::Visit::Found(foundval)
            } else {
                crate::pathfinding::Visit::Explore
            }
        },
    );
    BFS_RESULT.with(|res| *res.borrow_mut() = r);
    r.is_some() as u8
}

#[no_mangle]
pub extern "C" fn probe_bfs_visits_len() -> usize {
    BFS_VISITS.with(|v| v.borrow().len())
}

#[no_mangle]
pub extern "C" fn probe_bfs_visit_node(i: usize) -> f64 {
    BFS_VISITS.with(|v| v.borrow()[2 * i])
}

#[no_mangle]
pub extern "C" fn probe_bfs_visit_dist(i: usize) -> f64 {
    BFS_VISITS.with(|v| v.borrow()[2 * i + 1])
}

#[no_mangle]
pub extern "C" fn probe_bfs_result_flag() -> u8 {
    BFS_RESULT.with(|r| r.borrow().is_some() as u8)
}

#[no_mangle]
pub extern "C" fn probe_bfs_result() -> f64 {
    BFS_RESULT.with(|r| r.borrow().unwrap())
}

// ========================= P22: AirPathFinder (PathFinder.Air.ts) =============

thread_local! {
    static AIR_MAP: std::cell::RefCell<Option<&'static GameMap>> =
        const { std::cell::RefCell::new(None) };
    /// Walked path as a flat (x, y) coordinate stream.
    static AIR_PATH: std::cell::RefCell<Vec<f64>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Build a `w x h` all-land map and leak it (the finder borrows it).
#[no_mangle]
pub extern "C" fn probe_air_new(w: f64, h: f64) {
    let data = vec![0x83u8; (w * h) as usize];
    let gm: &'static GameMap = Box::leak(Box::new(GameMap::new(w, h, data, w * h)));
    AIR_MAP.with(|m| *m.borrow_mut() = Some(gm));
}

/// Run one `findPath`. `multi` 1 = array start (TS throws). Returns 0 = threw,
/// 1 = path (`probe_air_path_*`, flat x/y pairs).
#[no_mangle]
pub extern "C" fn probe_air_run(ticks: f64, from: f64, to: f64, multi: u32) -> u8 {
    let gm = AIR_MAP.with(|m| m.borrow().unwrap());
    let pf = crate::pathfinding::air::AirPathFinder::new(gm, ticks);
    // The would-throw convention: a multi start throws at the Array.isArray
    // guard, and an out-of-range game.ref throws mid-walk; both report 0.
    if multi == 1 {
        AIR_PATH.with(|p| p.borrow_mut().clear());
        return 0;
    }
    match pf.debug_find_path(from, to) {
        None => {
            AIR_PATH.with(|p| p.borrow_mut().clear());
            0
        }
        Some(refs) => {
            let coords: Vec<f64> = refs.iter().flat_map(|&t| [gm.x(t), gm.y(t)]).collect();
            AIR_PATH.with(|p| *p.borrow_mut() = coords);
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn probe_air_path_len() -> usize {
    AIR_PATH.with(|p| p.borrow().len())
}

#[no_mangle]
pub extern "C" fn probe_air_path_at(i: usize) -> f64 {
    AIR_PATH.with(|p| p.borrow()[i])
}

// ========================= P23: AnonNames (AnonNames.ts) ======================

thread_local! {
    /// Last handle as UTF-16 code units (JS `charCodeAt` stream).
    static ANON_OUT: std::cell::RefCell<Vec<u16>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Run `anonWordName(slot, offset)`. `has` 0 = offset defaulted (TS default
/// param). Returns -1 for the JS `undefined` a missed lookup yields when
/// round === 0, else the handle's UTF-16 length (`probe_anon_out_at`).
#[no_mangle]
pub extern "C" fn probe_anon_run(slot: f64, offset: f64, has: u32) -> i32 {
    let s = crate::anon_names::anon_word_name(slot, if has == 1 { Some(offset) } else { None });
    match s {
        None => -1,
        Some(handle) => {
            let units: Vec<u16> = handle.encode_utf16().collect();
            let len = units.len() as i32;
            ANON_OUT.with(|o| *o.borrow_mut() = units);
            len
        }
    }
}

#[no_mangle]
pub extern "C" fn probe_anon_out_at(i: usize) -> f64 {
    ANON_OUT.with(|o| o.borrow()[i] as f64)
}

// ========================= P24: CloseCodes (CloseCodes.ts) ====================

thread_local! {
    static CLOSE_ARGS: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Push one argument: kind 0 pushes the code f64; kind 1 pushes
/// `[len, u0, .. u(len-1)]` (UTF-16 code units), same layout as the team
/// probe's strings.
#[no_mangle]
pub extern "C" fn probe_close_arg(v: f64) {
    CLOSE_ARGS.with(|t| t.borrow_mut().push(v));
}

/// Run one predicate over the pushed args. Returns the verdict (0/1).
#[no_mangle]
pub extern "C" fn probe_close_op(kind: u32) -> u8 {
    use crate::close_codes as cc;
    let a = CLOSE_ARGS.with(|t| std::mem::take(&mut *t.borrow_mut()));
    match kind {
        0 => u8::from(cc::is_terminal_close(a[0])),
        _ => {
            let len = a[0] as usize;
            let units: Vec<u16> = (0..len).map(|i| a[1 + i] as u16).collect();
            let s = String::from_utf16_lossy(&units);
            u8::from(cc::is_close_reason(&s))
        }
    }
}

// ========================= P25: ServerList (ServerList.ts) ====================

thread_local! {
    static SL_ARGS: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static SL_OUT: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Push one flat token (string length, code unit, numeric arg, list entry…).
#[no_mangle]
pub extern "C" fn probe_sl_arg(v: f64) {
    SL_ARGS.with(|t| t.borrow_mut().push(v));
}

/// Run `server_list::run_op(kind, args)`; returns the result-stream length.
#[no_mangle]
pub extern "C" fn probe_sl_op(kind: u32) -> usize {
    let a = SL_ARGS.with(|t| std::mem::take(&mut *t.borrow_mut()));
    let out = crate::server_list::run_op(kind as u8, &a);
    let len = out.len();
    SL_OUT.with(|o| *o.borrow_mut() = out);
    len
}

#[no_mangle]
pub extern "C" fn probe_sl_out_at(i: usize) -> f64 {
    SL_OUT.with(|o| o.borrow()[i])
}

// ========================= P35: AssetUrls (AssetUrls.ts) ======================

thread_local! {
    static AU_ARGS: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static AU_OUT: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Push one flat token (string length, code unit, manifest entry count…).
#[no_mangle]
pub extern "C" fn probe_au_arg(v: f64) {
    AU_ARGS.with(|t| t.borrow_mut().push(v));
}

/// Run `asset_urls::run_op(kind, args)`; returns the result-stream length.
#[no_mangle]
pub extern "C" fn probe_au_op(kind: u32) -> usize {
    let a = AU_ARGS.with(|t| std::mem::take(&mut *t.borrow_mut()));
    let out = crate::asset_urls::run_op(kind as u8, &a);
    let len = out.len();
    AU_OUT.with(|o| *o.borrow_mut() = out);
    len
}

#[no_mangle]
pub extern "C" fn probe_au_out_at(i: usize) -> f64 {
    AU_OUT.with(|o| o.borrow()[i])
}

// ======================== P36: Maps.gen (Maps.gen.ts) =========================

thread_local! {
    static MG_ARGS: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static MG_OUT: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Push one flat token (string length, code unit…).
#[no_mangle]
pub extern "C" fn probe_mg_arg(v: f64) {
    MG_ARGS.with(|t| t.borrow_mut().push(v));
}

/// Run `maps_gen::run_op(kind, args)`; returns the result-stream length.
#[no_mangle]
pub extern "C" fn probe_mg_op(kind: u32) -> usize {
    let a = MG_ARGS.with(|t| std::mem::take(&mut *t.borrow_mut()));
    let out = crate::maps_gen::run_op(kind as u8, &a);
    let len = out.len();
    MG_OUT.with(|o| *o.borrow_mut() = out);
    len
}

#[no_mangle]
pub extern "C" fn probe_mg_out_at(i: usize) -> f64 {
    MG_OUT.with(|o| o.borrow()[i])
}

// ====================== P37: TribeNames (TribeNames.ts) =======================

thread_local! {
    static TN_ARGS: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static TN_OUT: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Push one flat token (string length, code unit, name-list count…).
#[no_mangle]
pub extern "C" fn probe_tn_arg(v: f64) {
    TN_ARGS.with(|t| t.borrow_mut().push(v));
}

/// Run `tribe_names::run_op(kind, args)`; returns the result-stream length.
#[no_mangle]
pub extern "C" fn probe_tn_op(kind: u32) -> usize {
    let a = TN_ARGS.with(|t| std::mem::take(&mut *t.borrow_mut()));
    let out = crate::tribe_names::run_op(kind as u8, &a);
    let len = out.len();
    TN_OUT.with(|o| *o.borrow_mut() = out);
    len
}

#[no_mangle]
pub extern "C" fn probe_tn_out_at(i: usize) -> f64 {
    TN_OUT.with(|o| o.borrow()[i])
}

// ======================= P38: game/Game.ts (game_ts) =========================

thread_local! {
    static GAME_ARGS: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static GAME_OUT: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Push one flat token (enum index, string length, code unit…).
#[no_mangle]
pub extern "C" fn probe_game_arg(v: f64) {
    GAME_ARGS.with(|t| t.borrow_mut().push(v));
}

/// Run `game_ts::run_op(kind, args)`; returns the result-stream length.
#[no_mangle]
pub extern "C" fn probe_game_op(kind: u32) -> usize {
    let a = GAME_ARGS.with(|t| std::mem::take(&mut *t.borrow_mut()));
    let out = crate::game_ts::run_op(kind as u8, &a);
    let len = out.len();
    GAME_OUT.with(|o| *o.borrow_mut() = out);
    len
}

#[no_mangle]
pub extern "C" fn probe_game_out_at(i: usize) -> f64 {
    GAME_OUT.with(|o| o.borrow()[i])
}

// ============== P39: game/NationCreation.ts (nation_creation) ===============

thread_local! {
    static NC_ARGS: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static NC_OUT: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Push one flat token (seed, string length, code unit…).
#[no_mangle]
pub extern "C" fn probe_nc_arg(v: f64) {
    NC_ARGS.with(|t| t.borrow_mut().push(v));
}

/// Run `nation_creation::run_op(kind, args)`; returns the result-stream length.
#[no_mangle]
pub extern "C" fn probe_nc_op(kind: u32) -> usize {
    let a = NC_ARGS.with(|t| std::mem::take(&mut *t.borrow_mut()));
    let out = crate::nation_creation::run_op(kind as u8, &a);
    let len = out.len();
    NC_OUT.with(|o| *o.borrow_mut() = out);
    len
}

#[no_mangle]
pub extern "C" fn probe_nc_out_at(i: usize) -> f64 {
    NC_OUT.with(|o| o.borrow()[i])
}

// ============== P40a: game/GameUpdates.ts (game_updates) ================

thread_local! {
    static GUPD_ARGS: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static GUPD_OUT: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Push one flat token (string length, code unit…).
#[no_mangle]
pub extern "C" fn probe_gupd_arg(v: f64) {
    GUPD_ARGS.with(|t| t.borrow_mut().push(v));
}

/// Run `game_updates::run_op(kind, args)`; returns the result-stream length.
#[no_mangle]
pub extern "C" fn probe_gupd_op(kind: u32) -> usize {
    let a = GUPD_ARGS.with(|t| std::mem::take(&mut *t.borrow_mut()));
    let out = crate::game_updates::run_op(kind as u8, &a);
    let len = out.len();
    GUPD_OUT.with(|o| *o.borrow_mut() = out);
    len
}

#[no_mangle]
pub extern "C" fn probe_gupd_out_at(i: usize) -> f64 {
    GUPD_OUT.with(|o| o.borrow()[i])
}

// ====== P40b: Util.ts emojiTable + NationEmojiBehavior.ts (nation_emoji) ======

thread_local! {
    static NE_ARGS: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static NE_OUT: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Push one flat token (string length, code unit…).
#[no_mangle]
pub extern "C" fn probe_ne_arg(v: f64) {
    NE_ARGS.with(|t| t.borrow_mut().push(v));
}

/// Run `nation_emoji::run_op(kind, args)`; returns the result-stream length.
#[no_mangle]
pub extern "C" fn probe_ne_op(kind: u32) -> usize {
    let a = NE_ARGS.with(|t| std::mem::take(&mut *t.borrow_mut()));
    let out = crate::nation_emoji::run_op(kind as u8, &a);
    let len = out.len();
    NE_OUT.with(|o| *o.borrow_mut() = out);
    len
}

#[no_mangle]
pub extern "C" fn probe_ne_out_at(i: usize) -> f64 {
    NE_OUT.with(|o| o.borrow()[i])
}

// ====== P41: pathfinding/PathFinder.ts WaterPathMemo (water_path_memo) ======

thread_local! {
    static WPM_ARGS: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static WPM_OUT: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Push one flat token (numTiles, maxBytes, script / op framing, tile refs…).
#[no_mangle]
pub extern "C" fn probe_wpm_arg(v: f64) {
    WPM_ARGS.with(|t| t.borrow_mut().push(v));
}

/// Run `water_path_memo::run_op(kind, args)`; returns the result-stream length.
#[no_mangle]
pub extern "C" fn probe_wpm_op(kind: u32) -> usize {
    let a = WPM_ARGS.with(|t| std::mem::take(&mut *t.borrow_mut()));
    let out = crate::water_path_memo::run_op(kind as u8, &a);
    let len = out.len();
    WPM_OUT.with(|o| *o.borrow_mut() = out);
    len
}

#[no_mangle]
pub extern "C" fn probe_wpm_out_at(i: usize) -> f64 {
    WPM_OUT.with(|o| o.borrow()[i])
}

// ====================== P26: PatternDecoder (PatternDecoder.ts) ===============

thread_local! {
    static PD_ARGS: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static PD_OUT: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Push one flat token (byte-buffer length, byte value, x, y…).
#[no_mangle]
pub extern "C" fn probe_pd_arg(v: f64) {
    PD_ARGS.with(|t| t.borrow_mut().push(v));
}

/// Run `pattern_decoder::run_op(kind, args)`; returns the result-stream length.
#[no_mangle]
pub extern "C" fn probe_pd_op(kind: u32) -> usize {
    let a = PD_ARGS.with(|t| std::mem::take(&mut *t.borrow_mut()));
    let out = crate::pattern_decoder::run_op(kind as u8, &a);
    let len = out.len();
    PD_OUT.with(|o| *o.borrow_mut() = out);
    len
}

#[no_mangle]
pub extern "C" fn probe_pd_out_at(i: usize) -> f64 {
    PD_OUT.with(|o| o.borrow()[i])
}

// ====================== P27: DoomsdayClock (DoomsdayClock.ts) =================

thread_local! {
    static DC_ARGS: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static DC_OUT: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Push one flat token (speed/team codes, land/elapsed, noise coords…).
#[no_mangle]
pub extern "C" fn probe_dc_arg(v: f64) {
    DC_ARGS.with(|t| t.borrow_mut().push(v));
}

/// Run `doomsday_clock::run_op(kind, args)`; returns the result-stream length.
#[no_mangle]
pub extern "C" fn probe_dc_op(kind: u32) -> usize {
    let a = DC_ARGS.with(|t| std::mem::take(&mut *t.borrow_mut()));
    let out = crate::doomsday_clock::run_op(kind as u8, &a);
    let len = out.len();
    DC_OUT.with(|o| *o.borrow_mut() = out);
    len
}

#[no_mangle]
pub extern "C" fn probe_dc_out_at(i: usize) -> f64 {
    DC_OUT.with(|o| o.borrow()[i])
}

// ================= P28: execution/Util.ts (exec_util) =========================
// One packed-terrain GameMap + owner writes per scenario, then one
// `exec_util::run_op(gm, kind, args)`. Terrain bytes are queued before
// `probe_eu_new` (like the GM probe); owners are queued via `probe_eu_owner`
// and applied inside `probe_eu_new` with the same `set_owner_id` the TS
// runner used (scenario owners are all <= 4095, so no throw path).

thread_local! {
    static EU_ARGS: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static EU_OUT: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static EU_TERRAIN: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
    static EU_OWNERS: std::cell::RefCell<Vec<(f64, f64)>> = const { std::cell::RefCell::new(Vec::new()) };
    static EU_GM: std::cell::RefCell<Option<Box<GameMap>>> =
        const { std::cell::RefCell::new(None) };
}

/// Queue one packed terrain byte for the next map (before `probe_eu_new`).
#[no_mangle]
pub extern "C" fn probe_eu_terrain_byte(v: u32) {
    EU_TERRAIN.with(|t| t.borrow_mut().push(v as u8));
}

/// Queue one `(tile, playerId)` owner write for the next map.
#[no_mangle]
pub extern "C" fn probe_eu_owner(tile: f64, player_id: f64) {
    EU_OWNERS.with(|o| o.borrow_mut().push((tile, player_id)));
}

/// Build the map from the queued terrain + owners.
#[no_mangle]
pub extern "C" fn probe_eu_new(w: f64, h: f64) {
    let terrain = EU_TERRAIN.with(|t| std::mem::take(&mut *t.borrow_mut()));
    let owners = EU_OWNERS.with(|o| std::mem::take(&mut *o.borrow_mut()));
    let mut gm = GameMap::new(w, h, terrain, w * h);
    for (t, id) in owners {
        gm.set_owner_id(t, id);
    }
    EU_GM.with(|g| *g.borrow_mut() = Some(Box::new(gm)));
}

/// Push one flat token (tile, cap, mode, list elements…).
#[no_mangle]
pub extern "C" fn probe_eu_arg(v: f64) {
    EU_ARGS.with(|t| t.borrow_mut().push(v));
}

/// Run `exec_util::run_op(gm, kind, args)`; returns the result-stream length.
#[no_mangle]
pub extern "C" fn probe_eu_op(kind: u32) -> usize {
    let a = EU_ARGS.with(|t| std::mem::take(&mut *t.borrow_mut()));
    let out = EU_GM.with(|g| {
        crate::exec_util::run_op(
            g.borrow().as_ref().expect("probe_eu_new must be called first").as_ref(),
            kind as u8,
            &a,
        )
    });
    let len = out.len();
    EU_OUT.with(|o| *o.borrow_mut() = out);
    len
}

#[no_mangle]
pub extern "C" fn probe_eu_out_at(i: usize) -> f64 {
    EU_OUT.with(|o| o.borrow()[i])
}

// ================= P29: game/WaterManager.ts (water_manager) ================
// Two packed GameMaps (full + minimap) queued before `probe_wm_new`, then an
// op stream replayed through `WaterManager::run_op` (kind table in
// water_manager.rs). The initial `map_state` is always all-zero (the TS
// GameMapImpl ctor allocates a zeroed Uint16Array), so only terrain bytes are
// queued; the final buffers are read back per index.

thread_local! {
    static WM_MAP_T: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
    static WM_MINI_T: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
    static WM_ARGS: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static WM_OUT: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static WM_GM: std::cell::RefCell<Option<crate::water_manager::WaterManager>> =
        const { std::cell::RefCell::new(None) };
}

/// Queue one full-map terrain byte (before `probe_wm_new`).
#[no_mangle]
pub extern "C" fn probe_wm_map_terrain_byte(v: u32) {
    WM_MAP_T.with(|t| t.borrow_mut().push(v as u8));
}

/// Queue one minimap terrain byte (before `probe_wm_new`).
#[no_mangle]
pub extern "C" fn probe_wm_mini_terrain_byte(v: u32) {
    WM_MINI_T.with(|t| t.borrow_mut().push(v as u8));
}

/// Build both maps and the manager from the queued terrain.
#[no_mangle]
pub extern "C" fn probe_wm_new(mw: f64, mh: f64, nw: f64, nh: f64, disable: u32) {
    let mt = WM_MAP_T.with(|t| std::mem::take(&mut *t.borrow_mut()));
    let nt = WM_MINI_T.with(|t| std::mem::take(&mut *t.borrow_mut()));
    let map = GameMap::new(mw, mh, mt, mw * mh);
    let mini = GameMap::new(nw, nh, nt, nw * nh);
    let wm = crate::water_manager::WaterManager::new(map, mini, disable != 0);
    WM_GM.with(|g| *g.borrow_mut() = Some(wm));
}

/// Push one flat op arg (a, then b).
#[no_mangle]
pub extern "C" fn probe_wm_arg(v: f64) {
    WM_ARGS.with(|t| t.borrow_mut().push(v));
}

/// Run `WaterManager::run_op(kind, args)`; returns the result-stream length.
#[no_mangle]
pub extern "C" fn probe_wm_op(kind: u32) -> usize {
    let a = WM_ARGS.with(|t| std::mem::take(&mut *t.borrow_mut()));
    let out = WM_GM.with(|g| {
        g.borrow_mut()
            .as_mut()
            .expect("probe_wm_new must be called first")
            .run_op(kind as u8, &a)
    });
    let len = out.len();
    WM_OUT.with(|o| *o.borrow_mut() = out);
    len
}

#[no_mangle]
pub extern "C" fn probe_wm_out_at(i: usize) -> f64 {
    WM_OUT.with(|o| o.borrow()[i])
}

#[no_mangle]
pub extern "C" fn probe_wm_version() -> f64 {
    WM_GM.with(|g| {
        g.borrow()
            .as_ref()
            .expect("probe_wm_new must be called first")
            .water_graph_version()
    })
}

#[no_mangle]
pub extern "C" fn probe_wm_map_terrain_at(i: usize) -> u32 {
    WM_GM.with(|g| g.borrow().as_ref().unwrap().debug_map_terrain()[i] as u32)
}

#[no_mangle]
pub extern "C" fn probe_wm_map_state_at(i: usize) -> u32 {
    WM_GM.with(|g| g.borrow().as_ref().unwrap().debug_map_state()[i] as u32)
}

#[no_mangle]
pub extern "C" fn probe_wm_mini_terrain_at(i: usize) -> u32 {
    WM_GM.with(|g| g.borrow().as_ref().unwrap().debug_mini_terrain()[i] as u32)
}

// ================= P30: game/GameUpdateUtils.ts (game_update_utils) ==========

thread_local! {
    static GU_ARGS: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static GU_OUT: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Push one flat token of the PlayerUpdate / PlayerState token stream.
#[no_mangle]
pub extern "C" fn probe_gu_arg(v: f64) {
    GU_ARGS.with(|t| t.borrow_mut().push(v));
}

/// Run `game_update_utils::run_op(kind, args)`; returns the result-stream
/// length.
#[no_mangle]
pub extern "C" fn probe_gu_op(kind: u32) -> usize {
    let a = GU_ARGS.with(|t| std::mem::take(&mut *t.borrow_mut()));
    let out = crate::game_update_utils::run_op(kind as u8, &a);
    let len = out.len();
    GU_OUT.with(|o| *o.borrow_mut() = out);
    len
}

#[no_mangle]
pub extern "C" fn probe_gu_out_at(i: usize) -> f64 {
    GU_OUT.with(|o| o.borrow()[i])
}

// ================= P31: game/Railroad.ts (railroad) ===========================

thread_local! {
    static RR_ARGS: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static RR_OUT: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Push one flat token of the Railroad token stream (stations/rails by refid).
#[no_mangle]
pub extern "C" fn probe_rr_arg(v: f64) {
    RR_ARGS.with(|t| t.borrow_mut().push(v));
}

/// Run `railroad::run_op(kind, args)`; returns the result-stream length.
#[no_mangle]
pub extern "C" fn probe_rr_op(kind: u32) -> usize {
    let a = RR_ARGS.with(|t| std::mem::take(&mut *t.borrow_mut()));
    let out = crate::railroad::run_op(kind as u8, &a);
    let len = out.len();
    RR_OUT.with(|o| *o.borrow_mut() = out);
    len
}

#[no_mangle]
pub extern "C" fn probe_rr_out_at(i: usize) -> f64 {
    RR_OUT.with(|o| o.borrow()[i])
}

// ================= P32: game/RailroadSpatialGrid.ts (railroad_spatial_grid) ==

thread_local! {
    static RSG_HARNESS: std::cell::RefCell<crate::railroad_spatial_grid::RigHarness> =
        std::cell::RefCell::new(crate::railroad_spatial_grid::RigHarness::new());
    static RSG_ARGS: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static RSG_OUT: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Drop the current grid (one scenario's op stream ends; the next begins with
/// a construct op).
#[no_mangle]
pub extern "C" fn probe_rsg_reset() {
    RSG_HARNESS.with(|h| h.borrow_mut().reset());
}

/// Push one flat token of the op's arg stream.
#[no_mangle]
pub extern "C" fn probe_rsg_arg(v: f64) {
    RSG_ARGS.with(|t| t.borrow_mut().push(v));
}

/// Run `RigHarness::run_op(kind, args)`; returns the result-stream length.
#[no_mangle]
pub extern "C" fn probe_rsg_op(kind: u32) -> usize {
    let a = RSG_ARGS.with(|t| std::mem::take(&mut *t.borrow_mut()));
    let out = RSG_HARNESS.with(|h| h.borrow_mut().run_op(kind as u8, &a));
    let len = out.len();
    RSG_OUT.with(|o| *o.borrow_mut() = out);
    len
}

#[no_mangle]
pub extern "C" fn probe_rsg_out_at(i: usize) -> f64 {
    RSG_OUT.with(|o| o.borrow()[i])
}

// ============= P33: game/TileTraversalScratch.ts (tile_traversal_scratch) ====

thread_local! {
    static TTS_HARNESS: std::cell::RefCell<crate::tile_traversal_scratch::RigHarness> =
        std::cell::RefCell::new(crate::tile_traversal_scratch::RigHarness::new());
    static TTS_ARGS: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static TTS_OUT: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Drop all scratches (one scenario's op stream ends; the next begins with an
/// allocate op). Mirrors a fresh WeakMap per capture scenario.
#[no_mangle]
pub extern "C" fn probe_tts_reset() {
    TTS_HARNESS.with(|h| h.borrow_mut().reset());
}

/// Push one flat token of the op's arg stream.
#[no_mangle]
pub extern "C" fn probe_tts_arg(v: f64) {
    TTS_ARGS.with(|t| t.borrow_mut().push(v));
}

/// Run `RigHarness::run_op(kind, args)`; returns the result-stream length.
#[no_mangle]
pub extern "C" fn probe_tts_op(kind: u32) -> usize {
    let a = TTS_ARGS.with(|t| std::mem::take(&mut *t.borrow_mut()));
    let out = TTS_HARNESS.with(|h| h.borrow_mut().run_op(kind as u8, &a));
    let len = out.len();
    TTS_OUT.with(|o| *o.borrow_mut() = out);
    len
}

#[no_mangle]
pub extern "C" fn probe_tts_out_at(i: usize) -> f64 {
    TTS_OUT.with(|o| o.borrow()[i])
}

// ========================== P34: EventBus.ts (event_bus) ======================

thread_local! {
    static EB_HARNESS: std::cell::RefCell<crate::event_bus::RigHarness> =
        std::cell::RefCell::new(crate::event_bus::RigHarness::new());
    static EB_ARGS: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
    static EB_OUT: std::cell::RefCell<Vec<f64>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Drop all listeners (one scenario's op stream ends; the next begins fresh).
#[no_mangle]
pub extern "C" fn probe_eb_reset() {
    EB_HARNESS.with(|h| h.borrow_mut().reset());
}

/// Push one flat token of the op's arg stream.
#[no_mangle]
pub extern "C" fn probe_eb_arg(v: f64) {
    EB_ARGS.with(|t| t.borrow_mut().push(v));
}

/// Run `RigHarness::run_op(kind, args)`; returns the result-stream length.
#[no_mangle]
pub extern "C" fn probe_eb_op(kind: u32) -> usize {
    let a = EB_ARGS.with(|t| std::mem::take(&mut *t.borrow_mut()));
    let out = EB_HARNESS.with(|h| h.borrow_mut().run_op(kind as u8, &a));
    let len = out.len();
    EB_OUT.with(|o| *o.borrow_mut() = out);
    len
}

#[no_mangle]
pub extern "C" fn probe_eb_out_at(i: usize) -> f64 {
    EB_OUT.with(|o| o.borrow()[i])
}
