//! Port of `src/core/pathfinding/transformers/ComponentCheckTransformer.ts`.
//!
//! A `PathFinder` decorator that fails fast when every start is in a
//! different component than the goal (e.g. disconnected water bodies), and
//! otherwise delegates with *only* the same-component starts.
//!
//! JS-isms pinned here:
//! * `Array.isArray(from) ? from : [from]` then `filter` keeps the original
//!   start order.
//! * A single surviving source collapses to a scalar start for `inner`
//!   (`validSources.length === 1 ? validSources[0] : validSources`) — the
//!   collapse is observable through the recorded `PathStart` kind.
//! * An empty start array: `filter` over `[]` yields `[]`, so the transformer
//!   returns `null` without calling `inner` (vacuous no-valid-source case).
//! * `getComponent(to)` runs before any source check; the injected getter is
//!   pure in production (`graph.getComponentId`), so call order is not
//!   observable.

use super::{PathFinder, PathStart};

/// The injected `(t: T) => number` component lookup. `i64` mirrors the
/// production `ConnectedComponents.getComponentId` domain (integer ids).
pub trait ComponentGetter {
    fn get_component(&mut self, tile: f64) -> i64;
}

/// `ComponentCheckTransformer<TileRef>` from the TS source.
pub struct ComponentCheckTransformer<I: PathFinder, G: ComponentGetter> {
    inner: I,
    get_component: G,
}

impl<I: PathFinder, G: ComponentGetter> PathFinder for ComponentCheckTransformer<I, G> {
    fn find_path(&mut self, starts: PathStart<'_>, goal: f64) -> Option<Vec<f64>> {
        ComponentCheckTransformer::find_path(self, starts, goal)
    }
}

impl<I: PathFinder, G: ComponentGetter> ComponentCheckTransformer<I, G> {
    /// `constructor(inner, getComponent)`.
    pub fn new(inner: I, get_component: G) -> Self {
        Self {
            inner,
            get_component,
        }
    }

    /// `findPath(from, to)`.
    pub fn find_path(&mut self, from: PathStart<'_>, to: f64) -> Option<Vec<f64>> {
        let to_component = self.get_component.get_component(to);

        // Check all sources - at least one must match destination component.
        let mut valid_sources: Vec<f64> = Vec::new();
        for &f in from.as_slice() {
            if self.get_component.get_component(f) == to_component {
                valid_sources.push(f);
            }
        }

        if valid_sources.is_empty() {
            return None; // No source in same component as destination
        }

        // Delegate with only valid sources.
        let delegate_from = if valid_sources.len() == 1 {
            PathStart::Single(valid_sources[0])
        } else {
            PathStart::Multi(&valid_sources)
        };
        self.inner.find_path(delegate_from, to)
    }
}

/// Parity-harness [`ComponentGetter`]: a `(tile, component)` table with a
/// default for unknown tiles, plus a call counter so "getter untouched"
/// stays observable in principle. Mirrors the TS table closure used by
/// `tools/gen_vectors.mjs`.
#[derive(Default)]
pub struct TableGetter {
    table: Vec<(f64, i64)>,
    default: i64,
    calls: u64,
}

impl TableGetter {
    /// Replace the table (insertion order preserved; first match wins like
    /// `table[t]` lookups cannot diverge for unique keys).
    pub fn set_table(&mut self, table: Vec<(f64, i64)>, default: i64) {
        self.table = table;
        self.default = default;
    }
    /// Cumulative `getComponent` calls.
    pub fn calls(&self) -> u64 {
        self.calls
    }

    fn lookup(&self, tile: f64) -> i64 {
        self.table
            .iter()
            .find(|(k, _)| *k == tile)
            .map_or(self.default, |(_, v)| *v)
    }
}

impl ComponentGetter for TableGetter {
    fn get_component(&mut self, tile: f64) -> i64 {
        self.calls += 1;
        self.lookup(tile)
    }
}
