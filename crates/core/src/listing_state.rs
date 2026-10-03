//! Port of `src/server/ListingState.ts`: the private lobby's public-listing
//! presence (`class ListingState`). The TS `Date.now()` is the module's only
//! impure call; the capture scripts it through `globalThis.__LISTING_NOW`
//! (prepared rewrite in `tools/ts_load.mjs`) and the port takes the same
//! `now` as an explicit `set_listed` argument.
//!
//! Faithfulness notes:
//!
//! * `setListed` is a NO-OP when the value is unchanged (`this.listed ===
//!   listed`), so a duplicate `setListed(true)` never moves `listedAt` and a
//!   `setListed(false)` on an already-unlisted lobby does NOT clear a stale
//!   `listedAt` (`ls_dup_toggle_noop`, `ls_delist_noop_when_false`).
//! * `autoStartAt` returns `undefined` while unlisted even if a stale
//!   `listedAt` survived a no-op delist; relisting after a real delist
//!   stamps a fresh `Date.now()` (`ls_delist_relist`).
//! * `setFeatured` sets `featured = true` unconditionally, sanitises the
//!   label at the boundary (`opts.label ? sanitizeLobbyLabel(...) : ""` —
//!   an empty / absent / `undefined` label is falsy and skips sanitisation),
//!   stores `undefined` when the sanitised label is empty (the key is still
//!   assigned — observably identical through the getter), and assigns
//!   `this.accent = opts.accent` VERBATIM: a second `setFeatured({})` clears
//!   a previously set accent to `undefined` (`ls_set_featured_twice`,
//!   `ls_accent_undefined_passthrough`).
//! * `featured` survives a delist/relist and flips the auto-start constant
//!   between `FEATURED_LOBBY_AUTO_START_MS` (600_000) and
//!   `HOSTED_LOBBY_AUTO_START_MS` (300_000) (`ls_featured_survives_delist`).
//! * The label is kept as UTF-16 units through [`crate::util::sanitize_lobby_label`];
//!   the capture's label domain is valid scalar text (lone surrogates are not
//!   fed, matching the `js_json` codec's `String` model).

use crate::js_json::{push_val, read_map, val_field, JsVal};
use crate::schemas::{FEATURED_LOBBY_AUTO_START_MS, HOSTED_LOBBY_AUTO_START_MS};
use crate::util::sanitize_lobby_label;

/// The ported `ListingState`.
#[derive(Debug, Default, Clone)]
pub struct ListingState {
    listed: bool,
    listed_at: Option<f64>,
    label: Option<String>,
    accent: Option<String>,
    featured: bool,
}

impl ListingState {
    /// `isListed()`.
    pub fn is_listed(&self) -> bool {
        self.listed
    }

    /// `setListed(listed)` — `now` stands in for the scripted `Date.now()`.
    /// Duplicate toggles are a no-op (the deadline is not extended).
    pub fn set_listed(&mut self, listed: bool, now: f64) {
        if self.listed == listed {
            return;
        }
        self.listed = listed;
        self.listed_at = if listed { Some(now) } else { None };
    }

    /// `autoStartAt()`.
    pub fn auto_start_at(&self) -> Option<f64> {
        if !self.listed {
            return None;
        }
        let at = self.listed_at?;
        Some(at + if self.featured {
            FEATURED_LOBBY_AUTO_START_MS as f64
        } else {
            HOSTED_LOBBY_AUTO_START_MS as f64
        })
    }

    /// `isFeatured()`.
    pub fn is_featured(&self) -> bool {
        self.featured
    }

    /// `lobbyLabel()`.
    pub fn lobby_label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// `lobbyAccent()`.
    pub fn lobby_accent(&self) -> Option<&str> {
        self.accent.as_deref()
    }

    /// `setFeatured({label?, accent?})` — `opts` is the codec map (JS
    /// insertion order; absent vs present-`undefined` kept distinct, both
    /// falsy for the label gate).
    pub fn set_featured(&mut self, opts: &[(String, JsVal)]) {
        self.featured = true;
        let obj = JsVal::Obj(opts.to_vec());
        let sanitized: Vec<u16> = match val_field(&obj, "label") {
            Some(JsVal::Str(s)) if !s.is_empty() => {
                sanitize_lobby_label(&s.encode_utf16().collect::<Vec<u16>>())
            }
            _ => Vec::new(),
        };
        self.label = if sanitized.is_empty() {
            None
        } else {
            Some(String::from_utf16_lossy(&sanitized))
        };
        self.accent = match val_field(&obj, "accent") {
            Some(JsVal::Str(a)) => Some(a.clone()),
            _ => None,
        };
    }
}

/// The capture harness: one `ListingState` replaying an op stream.
#[derive(Debug, Default)]
pub struct RigHarness {
    state: ListingState,
}

impl RigHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Run one op. Kind table (see `tools/gen_vectors.mjs`):
    /// 0 construct/reset -> `[0]`;
    /// 1 setListed `[listed 0|1, now]` -> `[0]`;
    /// 2 isListed -> `[0|1]`;
    /// 3 autoStartAt -> `[0]` undefined | `[1,value]`;
    /// 4 isFeatured -> `[0|1]`;
    /// 5 lobbyLabel -> codec `val` (string or `undefined`);
    /// 6 lobbyAccent -> codec `val` (string or `undefined`);
    /// 7 setFeatured `[n,(key,value)*n]` (codec map `opts`) -> `[0]`;
    /// 8 dump state -> `[listed 0|1, val(listedAt), val(label), val(accent),
    ///   featured 0|1]` (values in the `js_json` codec form; `undefined`
    ///   fields ride as `[1]`).
    /// Strings cross as `[len, u0, ..]` UTF-16 units.
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut i = 0usize;
        match kind {
            0 => {
                self.reset();
                vec![0.0]
            }
            1 => {
                let listed = args[i] != 0.0;
                i += 1;
                let now = args[i];
                self.state.set_listed(listed, now);
                vec![0.0]
            }
            2 => vec![if self.state.is_listed() { 1.0 } else { 0.0 }],
            3 => match self.state.auto_start_at() {
                Some(v) => vec![1.0, v],
                None => vec![0.0],
            },
            4 => vec![if self.state.is_featured() { 1.0 } else { 0.0 }],
            5 => {
                let mut out = Vec::new();
                push_val(
                    &mut out,
                    &self
                        .state
                        .lobby_label()
                        .map_or(JsVal::Undef, |l| JsVal::Str(l.to_string())),
                );
                out
            }
            6 => {
                let mut out = Vec::new();
                push_val(
                    &mut out,
                    &self
                        .state
                        .lobby_accent()
                        .map_or(JsVal::Undef, |a| JsVal::Str(a.to_string())),
                );
                out
            }
            7 => {
                let opts = read_map(args, &mut i);
                self.state.set_featured(&opts);
                vec![0.0]
            }
            8 => {
                let mut out = vec![if self.state.is_listed() { 1.0 } else { 0.0 }];
                push_val(
                    &mut out,
                    &self
                        .state
                        .listed_at
                        .map_or(JsVal::Undef, JsVal::Num),
                );
                push_val(
                    &mut out,
                    &self
                        .state
                        .lobby_label()
                        .map_or(JsVal::Undef, |l| JsVal::Str(l.to_string())),
                );
                push_val(
                    &mut out,
                    &self
                        .state
                        .lobby_accent()
                        .map_or(JsVal::Undef, |a| JsVal::Str(a.to_string())),
                );
                out.push(if self.state.is_featured() { 1.0 } else { 0.0 });
                out
            }
            k => unreachable!("listing_state harness: unknown op kind {k}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::js_json::push_str;

    fn featured(opts: &[(&str, JsVal)]) -> Vec<f64> {
        let mut a = vec![opts.len() as f64];
        for (k, v) in opts {
            push_str(&mut a, k);
            push_val(&mut a, v);
        }
        a
    }

    #[test]
    fn duplicate_toggle_does_not_move_deadline() {
        let mut h = RigHarness::new();
        h.run_op(0, &[]);
        h.run_op(1, &[1.0, 1000.0]);
        h.run_op(1, &[1.0, 2000.0]); // no-op: listedAt stays 1000
        assert_eq!(h.run_op(3, &[]), vec![1.0, 1000.0 + 300_000.0]);
    }

    #[test]
    fn delist_noop_when_already_false_keeps_stale_listed_at() {
        let mut h = RigHarness::new();
        h.run_op(0, &[]);
        h.run_op(1, &[0.0, 9000.0]); // already false: no-op, listedAt untouched
        assert_eq!(h.run_op(8, &[]), vec![0.0, 1.0, 1.0, 1.0, 0.0]);
        h.run_op(1, &[1.0, 1000.0]);
        h.run_op(1, &[0.0, 2000.0]); // clears
        h.run_op(1, &[0.0, 3000.0]); // no-op
        assert_eq!(h.run_op(3, &[]), vec![0.0]); // unlisted -> undefined
    }

    #[test]
    fn featured_flips_the_constant_and_survives_relist() {
        let mut h = RigHarness::new();
        h.run_op(0, &[]);
        h.run_op(7, &featured(&[("accent", JsVal::Str("gold".into()))]));
        h.run_op(1, &[1.0, 1000.0]);
        assert_eq!(h.run_op(3, &[]), vec![1.0, 1000.0 + 600_000.0]);
        h.run_op(1, &[0.0, 2000.0]);
        h.run_op(1, &[1.0, 3000.0]);
        assert_eq!(h.run_op(3, &[]), vec![1.0, 3000.0 + 600_000.0]);
        assert_eq!(h.run_op(4, &[]), vec![1.0]);
    }

    #[test]
    fn set_featured_twice_label_and_accent_passthrough() {
        let mut h = RigHarness::new();
        h.run_op(0, &[]);
        h.run_op(7, &featured(&[("label", JsVal::Str("First".into())), ("accent", JsVal::Str("gold".into()))]));
        h.run_op(7, &featured(&[("label", JsVal::Str("Second".into()))]));
        assert_eq!(h.run_op(5, &[]), vec![5.0, 6.0, 83.0, 101.0, 99.0, 111.0, 110.0, 100.0]);
        assert_eq!(h.run_op(6, &[]), vec![1.0]); // accent cleared by the 2nd call
        h.run_op(7, &featured(&[("label", JsVal::Str("   ".into()))]));
        assert_eq!(h.run_op(5, &[]), vec![1.0]); // sanitised to empty -> undefined
        assert_eq!(h.run_op(4, &[]), vec![1.0]); // featured stays true
    }

    #[test]
    fn label_sanitize_edges() {
        let mut h = RigHarness::new();
        h.run_op(0, &[]);
        h.run_op(7, &featured(&[("label", JsVal::Str("\u{1}A\u{2}".into()))]));
        let out = h.run_op(5, &[]);
        assert_eq!(out, vec![5.0, 1.0, 65.0]);
        h.run_op(7, &featured(&[("label", JsVal::Str("".into()))])); // falsy -> ""
        assert_eq!(h.run_op(5, &[]), vec![1.0]);
    }
}
