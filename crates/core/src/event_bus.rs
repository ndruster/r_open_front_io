//! `src/core/EventBus.ts` — the typed pub/sub bus.
//!
//! `EventBus` keeps a `Map<EventConstructor, Array<callback>>`. JS `Map` and
//! array semantics carry the whole observable surface, so the port models the
//! two collaborator objects — the event *constructor* (the `Map` key) and each
//! *callback* (an array element) — as capture-assigned `refid`s, exactly as
//! [`crate::railroad`] keys stations/rails. The only side effect of `emit` is
//! the sequence of callback invocations, so `emit` is pinned as the ordered
//! `[(callback_refid, event_refid)]` call trace.
//!
//! Faithfulness notes:
//!
//! * `on`: `if (!this.listeners.has(eventType))` inserts a *new* empty array
//!   only when the constructor key is absent — re-`on`-ing a known constructor
//!   appends to the existing array without touching the `Map`'s key insertion
//!   order.
//! * `off`: `callbacks.indexOf(callback)` finds the *first* `===` match and
//!   `splice` removes exactly that one; a duplicate callback registered twice
//!   survives a single `off`. `off` on an unknown constructor is a no-op.
//! * `emit`: `const callbacks = this.listeners.get(eventConstructor)` — an
//!   absent key is `undefined` (`if (callbacks)` false, no calls); an *empty*
//!   array is truthy and iterates zero times. Callbacks fire in registration
//!   order, each with the emitted event object.
//! * `Map` keys use SameValueZero, so `-0` and `+0` refids collapse to one
//!   entry (`ref_key` normalises them).

use std::collections::HashMap;

/// Ordered callback list per constructor, with JS `Map` key-insertion order
/// preserved across `on` / `off`.
#[derive(Debug, Default)]
struct ListenerMap {
    /// `Vec<(ctor_refid, callbacks)>` in `Map` insertion order.
    entries: Vec<(f64, Vec<f64>)>,
    idx: HashMap<u64, usize>,
}

fn ref_key(v: f64) -> u64 {
    if v == 0.0 {
        0.0f64.to_bits()
    } else {
        v.to_bits()
    }
}

impl ListenerMap {
    fn has(&self, ctor: f64) -> bool {
        self.idx.contains_key(&ref_key(ctor))
    }

    /// `Map.set(ctor, [])` for a brand-new key only (JS `Map` keeps the first
    /// insertion position; re-setting an existing key preserves order).
    fn ensure(&mut self, ctor: f64) {
        if !self.has(ctor) {
            self.idx.insert(ref_key(ctor), self.entries.len());
            self.entries.push((ctor, Vec::new()));
        }
    }

    fn get(&self, ctor: f64) -> Option<&Vec<f64>> {
        self.idx.get(&ref_key(ctor)).map(|i| &self.entries[*i].1)
    }

    fn get_mut(&mut self, ctor: f64) -> Option<&mut Vec<f64>> {
        let i = *self.idx.get(&ref_key(ctor))?;
        Some(&mut self.entries[i].1)
    }

    /// `callbacks.push(callback)`.
    fn push(&mut self, ctor: f64, cb: f64) {
        self.ensure(ctor);
        let i = self.idx[&ref_key(ctor)];
        self.entries[i].1.push(cb);
    }

    /// `const idx = callbacks.indexOf(cb); if (idx > -1) callbacks.splice(idx, 1)`
    /// — first `===` match only; unknown constructor is a no-op.
    fn remove_first(&mut self, ctor: f64, cb: f64) {
        let Some(list) = self.get_mut(ctor) else { return };
        if let Some(pos) = list.iter().position(|&x| x == cb) {
            list.remove(pos);
        }
    }
}

/// Stateful parity harness replaying the recorded op stream. `kind`:
/// 0 `on(ctor, cb)` → `[]`;
/// 1 `off(ctor, cb)` → `[]`;
/// 2 `emit(ctor, event)` → `[n, (cb, event)*n]` callback call trace;
/// 3 `dump()` → `[nentries, (ctor, m, cbs…)*]` in `Map` insertion order.
#[derive(Default)]
pub struct RigHarness {
    listeners: ListenerMap,
}

impl RigHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.listeners = ListenerMap::default();
    }

    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        match kind {
            0 => {
                self.listeners.push(args[0], args[1]);
                vec![]
            }
            1 => {
                self.listeners.remove_first(args[0], args[1]);
                vec![]
            }
            2 => {
                let (ctor, event) = (args[0], args[1]);
                let mut out = Vec::new();
                if let Some(cbs) = self.listeners.get(ctor) {
                    for &cb in cbs {
                        out.push(cb);
                        out.push(event);
                    }
                }
                let n = out.len() / 2;
                let mut res = Vec::with_capacity(out.len() + 1);
                res.push(n as f64);
                res.extend(out);
                res
            }
            3 => {
                let mut res = vec![self.listeners.entries.len() as f64];
                for (ctor, cbs) in &self.listeners.entries {
                    res.push(*ctor);
                    res.push(cbs.len() as f64);
                    res.extend(cbs.iter());
                }
                res
            }
            _ => panic!("bad event_bus op kind {kind}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emit_fires_in_registration_order() {
        let mut h = RigHarness::new();
        h.run_op(0, &[1.0, 10.0]);
        h.run_op(0, &[1.0, 20.0]);
        assert_eq!(h.run_op(2, &[1.0, 77.0]), vec![2.0, 10.0, 77.0, 20.0, 77.0]);
    }

    #[test]
    fn off_removes_first_match_only() {
        let mut h = RigHarness::new();
        h.run_op(0, &[1.0, 10.0]);
        h.run_op(0, &[1.0, 10.0]); // duplicate callback
        h.run_op(1, &[1.0, 10.0]); // splice removes the first
        assert_eq!(h.run_op(2, &[1.0, 55.0]), vec![1.0, 10.0, 55.0]);
    }

    #[test]
    fn off_unknown_ctor_is_noop() {
        let mut h = RigHarness::new();
        h.run_op(0, &[1.0, 10.0]);
        h.run_op(1, &[2.0, 10.0]); // ctor 2 absent
        assert_eq!(h.run_op(2, &[1.0, 1.0]), vec![1.0, 10.0, 1.0]);
    }

    #[test]
    fn emit_unknown_ctor_no_calls() {
        let h_empty = RigHarness::new().run_op(2, &[9.0, 1.0]);
        assert_eq!(h_empty, vec![0.0]);
    }

    #[test]
    fn empty_callback_array_is_truthy() {
        let mut h = RigHarness::new();
        h.run_op(0, &[1.0, 10.0]);
        h.run_op(1, &[1.0, 10.0]); // entry exists but empty
        assert_eq!(h.run_op(2, &[1.0, 1.0]), vec![0.0]);
    }

    #[test]
    fn map_insertion_order_preserved() {
        let mut h = RigHarness::new();
        h.run_op(0, &[2.0, 20.0]); // ctor 2 first
        h.run_op(0, &[1.0, 10.0]); // ctor 1 second
        h.run_op(0, &[2.0, 21.0]); // re-on ctor 2 appends, no reorder
        assert_eq!(
            h.run_op(3, &[]),
            vec![2.0, 2.0, 2.0, 20.0, 21.0, 1.0, 1.0, 10.0]
        );
    }

    #[test]
    fn neg_zero_ctor_collapses_to_zero() {
        let mut h = RigHarness::new();
        h.run_op(0, &[-0.0, 10.0]);
        h.run_op(0, &[0.0, 20.0]); // SameValueZero: same key
        assert_eq!(h.run_op(3, &[]), vec![1.0, -0.0, 2.0, 10.0, 20.0]);
    }
}
