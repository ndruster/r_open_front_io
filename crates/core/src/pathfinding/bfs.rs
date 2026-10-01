//! Port of `src/core/pathfinding/algorithms/BFS.ts` (`BFS<T>`).
//!
//! Generic graph BFS with a visitor callback, `T = f64`. The whole class is
//! six lines of loop, so the JS-isms *are* the specification:
//!
//! * starts are seeded with `visited.add(s); queue.push(s)` — a **duplicate
//!   start is a Set no-op but still enqueued**, so the visitor sees that node
//!   twice (once per occurrence in the start array);
//! * the tri-state visitor return: a non-null, non-undefined value (including
//!   falsy `0`/`false`/`""`) short-circuits; `null` rejects the node without
//!   expanding it; `undefined` explores;
//! * the depth clip is `nextDist > maxDistance` — a JS `>` comparison, which
//!   is **false against `NaN`**, so a NaN bound never clips (unlike a
//!   `!(nextDist <= max)` rewrite);
//! * `visited` is a `Set` with SameValueZero semantics: `NaN` dedupes against
//!   `NaN`, `+0`/`-0` are one key — modelled by [`Bfs::set_has`];
//! * neighbours are marked visited at *enqueue* time, so repeated neighbours
//!   within one expansion (or a self-loop) collapse to a single queue entry.

use std::collections::VecDeque;

use super::{PathStart, Visit};

/// The `BFSAdapter<T>` interface with `T = f64`.
pub trait BfsAdapter {
    fn neighbors(&mut self, node: f64) -> Vec<f64>;
}

/// Generic BFS, matching `BFS<T>`.
pub struct Bfs<A: BfsAdapter> {
    adapter: A,
}

impl<A: BfsAdapter> Bfs<A> {
    pub fn new(adapter: A) -> Self {
        Self { adapter }
    }

    /// JS `Set.prototype.has` for `f64` keys: SameValueZero — `==` plus
    /// `NaN == NaN` (`+0`/`-0` already compare equal under `==`).
    fn set_has(set: &[f64], x: f64) -> bool {
        set.iter().any(|&y| y == x || (y.is_nan() && x.is_nan()))
    }

    fn set_add(set: &mut Vec<f64>, x: f64) {
        if !Self::set_has(set, x) {
            set.push(x);
        }
    }

    /// TS `search`. `start` carries the `T | T[]` union; only
    /// `Array.isArray` matters here, which `PathStart::as_slice` reproduces.
    pub fn search<R, F>(
        &mut self,
        start: PathStart<'_>,
        max_distance: f64,
        mut visitor: F,
    ) -> Option<R>
    where
        F: FnMut(f64, f64) -> Visit<R>,
    {
        let mut visited: Vec<f64> = Vec::new();
        let mut queue: VecDeque<(f64, f64)> = VecDeque::new();

        // Duplicate starts: Set no-op, queue push anyway.
        for &s in start.as_slice() {
            Self::set_add(&mut visited, s);
            queue.push_back((s, 0.0));
        }

        while let Some((node, dist)) = queue.pop_front() {
            match visitor(node, dist) {
                Visit::Found(r) => return Some(r),
                Visit::Reject => continue,
                Visit::Explore => {}
            }

            let next_dist = dist + 1.0;

            // JS `>`: false when either side is NaN -> a NaN bound never
            // clips. Keep the direct comparison; do not rewrite as `<=`.
            if next_dist > max_distance {
                continue;
            }

            for neighbor in self.adapter.neighbors(node) {
                if Self::set_has(&visited, neighbor) {
                    continue;
                }
                Self::set_add(&mut visited, neighbor);
                queue.push_back((neighbor, next_dist));
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Edge-table adapter mirroring the capture runner in gen_vectors.mjs.
    struct Table {
        edges: Vec<(f64, Vec<f64>)>,
    }

    impl Table {
        fn new(edges: Vec<(f64, Vec<f64>)>) -> Self {
            Self { edges }
        }
    }

    impl BfsAdapter for Table {
        fn neighbors(&mut self, node: f64) -> Vec<f64> {
            self.edges
                .iter()
                .find(|(k, _)| *k == node || (k.is_nan() && node.is_nan()))
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        }
    }

    /// 3x3 grid, N/S/W/E order, no row wrap (same probe order as BFSGrid).
    fn grid3x3() -> Vec<(f64, Vec<f64>)> {
        let mut e = Vec::new();
        for n in 0..9i64 {
            let mut nb = Vec::new();
            if n >= 3 {
                nb.push((n - 3) as f64);
            }
            if n < 6 {
                nb.push((n + 3) as f64);
            }
            if n % 3 != 0 {
                nb.push((n - 1) as f64);
            }
            if n % 3 != 2 {
                nb.push((n + 1) as f64);
            }
            e.push((n as f64, nb));
        }
        e
    }

    fn run_all(
        edges: Vec<(f64, Vec<f64>)>,
        start: PathStart<'_>,
        maxd: f64,
    ) -> (Vec<(f64, f64)>, Option<f64>) {
        let mut bfs = Bfs::new(Table::new(edges));
        let mut visits = Vec::new();
        let r = bfs.search(start, maxd, |n, d| -> Visit<f64> {
            visits.push((n, d));
            Visit::Explore
        });
        (visits, r)
    }

    #[test]
    fn spiral_order_is_n_s_w_e_breadth_first() {
        let (visits, r) = run_all(grid3x3(), PathStart::Single(4.0), f64::INFINITY);
        assert_eq!(r, None);
        assert_eq!(
            visits,
            vec![
                (4.0, 0.0),
                (1.0, 1.0),
                (7.0, 1.0),
                (3.0, 1.0),
                (5.0, 1.0),
                (0.0, 2.0),
                (2.0, 2.0),
                (6.0, 2.0),
                (8.0, 2.0)
            ]
        );
    }

    #[test]
    fn duplicate_start_is_enqueued_twice() {
        // Set.add is a no-op for the repeat, but queue.push still runs, so
        // the visitor sees node 0 at dist 0 twice.
        let (visits, r) = run_all(
            grid3x3(),
            PathStart::Multi(&[0.0, 0.0]),
            f64::INFINITY,
        );
        assert_eq!(r, None);
        assert_eq!(&visits[..2], &[(0.0, 0.0), (0.0, 0.0)]);
        // The second pass expands nothing new (all neighbours already
        // visited by the first pass), so the rest matches the single-start
        // order minus the duplicate.
        assert_eq!(visits.len(), 10);
    }

    #[test]
    fn found_zero_short_circuits() {
        // `0 !== null && 0 !== undefined` -> a falsy return still counts.
        let mut bfs = Bfs::new(Table::new(grid3x3()));
        let mut visits = Vec::new();
        let r = bfs.search(PathStart::Single(4.0), f64::INFINITY, |n, d| {
            visits.push((n, d));
            if n == 7.0 {
                Visit::Found(0.0)
            } else {
                Visit::Explore
            }
        });
        assert_eq!(r, Some(0.0));
        assert_eq!(visits, vec![(4.0, 0.0), (1.0, 1.0), (7.0, 1.0)]);
    }

    #[test]
    fn reject_skips_expansion_only() {
        let mut bfs = Bfs::new(Table::new(grid3x3()));
        let mut visits = Vec::new();
        let r = bfs.search(PathStart::Single(4.0), f64::INFINITY, |n, d| -> Visit<f64> {
            visits.push((n, d));
            if n == 1.0 {
                Visit::Reject
            } else {
                Visit::Explore
            }
        });
        assert_eq!(r, None);
        // 1 is visited but never expanded, so its children 0/2 arrive only
        // later via 3/5 at dist 2 (with 1 expanded they would be dist 1).
        let pos = |n: f64| visits.iter().position(|&(v, _)| v == n).unwrap();
        assert_eq!(visits[pos(0.0)].1, 2.0);
        assert_eq!(visits[pos(2.0)].1, 2.0);
        assert!(pos(0.0) > pos(3.0) && pos(2.0) > pos(5.0));
    }

    #[test]
    fn max_distance_clips_rings_after_visiting() {
        let (visits, r) = run_all(grid3x3(), PathStart::Single(4.0), 1.0);
        assert_eq!(r, None);
        // dist-1 nodes are still visited; their nextDist=2 > 1 clips.
        assert_eq!(
            visits,
            vec![(4.0, 0.0), (1.0, 1.0), (7.0, 1.0), (3.0, 1.0), (5.0, 1.0)]
        );
    }

    #[test]
    fn nan_max_distance_never_clips() {
        // `nextDist > NaN` is false in JS, so the bound is inert.
        let (visits, _) = run_all(grid3x3(), PathStart::Single(4.0), f64::NAN);
        assert_eq!(visits.len(), 9);
    }

    #[test]
    fn self_loop_and_duplicate_neighbours_collapse() {
        let (visits, r) = run_all(
            vec![(
                0.0,
                vec![0.0, 1.0, 1.0, 2.0],
            )],
            PathStart::Single(0.0),
            f64::INFINITY,
        );
        assert_eq!(r, None);
        assert_eq!(visits, vec![(0.0, 0.0), (1.0, 1.0), (2.0, 1.0)]);
    }

    #[test]
    fn empty_start_array_searches_nothing() {
        let (visits, r) = run_all(grid3x3(), PathStart::Multi(&[]), f64::INFINITY);
        assert_eq!(visits, vec![]);
        assert_eq!(r, None);
    }

    #[test]
    fn non_integer_and_negative_nodes() {
        let (visits, r) = run_all(
            vec![
                (-1.5, vec![0.25]),
                (0.25, vec![-1.5, 7.75]),
                (7.75, vec![]),
            ],
            PathStart::Single(-1.5),
            f64::INFINITY,
        );
        assert_eq!(r, None);
        assert_eq!(visits, vec![(-1.5, 0.0), (0.25, 1.0), (7.75, 2.0)]);
    }

    #[test]
    fn nan_nodes_dedupe_like_a_set() {
        // SameValueZero: two NaN starts are one Set key but two queue
        // entries; NaN neighbours of NaN are "already visited".
        let (visits, _) = run_all(
            vec![(f64::NAN, vec![1.0]), (1.0, vec![f64::NAN])],
            PathStart::Multi(&[f64::NAN, f64::NAN]),
            f64::INFINITY,
        );
        // Expected: NaN@0, NaN@0 (duplicate start enqueued twice), then
        // 1.0@1. The NaN->1.0 edge fires on the first NaN pop; the second
        // NaN pop finds 1.0 already visited. Compared element-wise because
        // assert_eq! treats NaN != NaN.
        assert_eq!(visits.len(), 3);
        assert!(visits[0].0.is_nan() && visits[0].1 == 0.0);
        assert!(visits[1].0.is_nan() && visits[1].1 == 0.0);
        assert_eq!(visits[2], (1.0, 1.0));
    }
}
