//! Port of `src/client/theme/ColorAllocator.ts` — `ColorAllocator.assignColor`
//! and `selectDistinctColorIndex`. Like the EffectPalette precedent the colord
//! package is NOT reimplemented: the capture runs the REAL colord 2.9.3 +
//! lab/lch plugins through `tools/vendor/colord/capture_shim.mjs`, which
//! memoizes every construction under a deterministic key string, assigns it an
//! auto-increment id, and traces every observation (toRgb / toLab / toLch /
//! toHsl / toRgbString / darken / alpha / delta) by id. The full trace travels
//! with every op as the [`ColordTables`] block and the Rust twin replays the
//! control flow over those ids — the genuine CIEDE2000 / rounding / parser
//! quirks stay pinned by V8.
//!
//! Faithfulness notes (quirk list, all V8-pinned):
//!
//! * `colord(hex)` memoizes by key string (`"S<len>:<u0,u1,..>"` for strings,
//!   `"O<k>=<String(v)>;..;"` for object inputs — NOT JSON.stringify, which
//!   collapses -0 and NaN). Repeated constructions of the same key return the
//!   SAME instance (one canonical id); observation rows are deduplicated per
//!   id (per (id1,id2) for delta), so table lookups are unambiguous.
//! * `new PseudoRandom(simpleHash(id)).nextInt(0, availableColors.length)`
//! * the distinct path runs while `0 < assigned.size <= 50`; the random path
//!   covers size 0 and size > 50 (strictly greater).
//! * `availableColors.splice(selectedIndex, 1)[0]` — `Vec::remove`.
//! * `assigned` is a Map: first-request cache hit by insertion order, and
//!   `Array.from(values())` feeds `selectDistinctColorIndex` in insertion
//!   order (the `>` comparison keeps the FIRST maximum index on ties).
//! * `selectDistinctColorIndex` throws `Error("No assigned colors")` on an
//!   empty assigned list; `minDeltaE` folds `Math.min` (NaN-propagating) from
//!   `Infinity`.
//! * delta() is CIEDE2000/100 rounded to 3 decimals and clamped to 0..1 by
//!   the lab plugin — the table carries the final value, no math here.

use crate::js_json::{read_str, push_str};
use crate::jsnum::js_min;
use crate::pseudo_random::PseudoRandom;
use crate::util::simple_hash;

/// The scripted colord facade trace: ten fixed-order blocks, each
/// `[n, (row)*n]` (the capture's `colordTables()` slices the shim trace):
/// construct, toRgb, toLab, toLch, toHsl, toRgbString, alpha, darken, delta,
/// sin. The sin table is the ThemeProvider `Math.sin` facade (x -> v).
#[derive(Default)]
pub struct ColordTables {
    /// key string -> canonical id.
    pub construct: Vec<(String, u32)>,
    /// id -> (r, g, b, a) of `toRgb()`.
    pub rgb: Vec<(u32, [f64; 4])>,
    /// id -> (l, a, b, alpha) of `toLab()`.
    pub lab: Vec<(u32, [f64; 4])>,
    /// id -> (l, c, h, a) of `toLch()`.
    pub lch: Vec<(u32, [f64; 4])>,
    /// id -> (h, s, l, a) of `toHsl()`.
    pub hsl: Vec<(u32, [f64; 4])>,
    /// id -> `toRgbString()` text.
    pub rgb_string: Vec<(u32, String)>,
    /// (id, amount) -> result id of `alpha(id, amount)`.
    pub alpha: Vec<(u32, f64, u32)>,
    /// (id, amount) -> result id of `darken(id, amount)`.
    pub darken: Vec<(u32, f64, u32)>,
    /// (id1, id2) -> `id1.delta(id2)`.
    pub delta: Vec<(u32, u32, f64)>,
    /// x -> V8 `Math.sin(x)`.
    pub sin: Vec<(f64, f64)>,
}

/// The shim's deterministic input key string for a hex / palette string.
pub fn key_hex(s: &str) -> String {
    let units: Vec<u16> = s.encode_utf16().collect();
    let joined = units
        .iter()
        .map(|u| u.to_string())
        .collect::<Vec<_>>()
        .join(",");
    format!("S{}:{joined}", units.len())
}

/// The shim's key string for an object input: `O<k>=<String(v)>;..;` in
/// insertion order (the literal's field order — `{l, c, h}`, `{...hsl, l}`,
/// `{r, g, b}`, `{l, a, b, alpha}`).
pub fn key_obj(fields: &[(&str, f64)]) -> String {
    let parts: Vec<String> = fields
        .iter()
        .map(|(k, v)| format!("{k}={}", crate::js_fixed::js_to_string(*v)))
        .collect();
    format!("O{};", parts.join(";"))
}

fn read_rows<T>(a: &[f64], i: &mut usize, f: fn(&[f64], &mut usize) -> T) -> Vec<T> {
    let n = a[*i] as usize;
    *i += 1;
    (0..n).map(|_| f(a, i)).collect()
}

fn read_construct(a: &[f64], i: &mut usize) -> (String, u32) {
    let key = read_str(a, i);
    let id = a[*i] as u32;
    *i += 1;
    (key, id)
}

fn read_quad(a: &[f64], i: &mut usize) -> (u32, [f64; 4]) {
    let id = a[*i] as u32;
    *i += 1;
    let q = [a[*i], a[*i + 1], a[*i + 2], a[*i + 3]];
    *i += 4;
    (id, q)
}

fn read_str_row(a: &[f64], i: &mut usize) -> (u32, String) {
    let id = a[*i] as u32;
    *i += 1;
    let s = read_str(a, i);
    (id, s)
}

fn read_derived(a: &[f64], i: &mut usize) -> (u32, f64, u32) {
    let id = a[*i] as u32;
    let amount = a[*i + 1];
    let rid = a[*i + 2] as u32;
    *i += 3;
    (id, amount, rid)
}

fn read_delta(a: &[f64], i: &mut usize) -> (u32, u32, f64) {
    let x = a[*i] as u32;
    let y = a[*i + 1] as u32;
    let d = a[*i + 2];
    *i += 3;
    (x, y, d)
}

fn read_sin(a: &[f64], i: &mut usize) -> (f64, f64) {
    let x = a[*i];
    let v = a[*i + 1];
    *i += 2;
    (x, v)
}

impl ColordTables {
    /// Read the ten-block tables section at `*i`.
    pub fn read(a: &[f64], i: &mut usize) -> Self {
        Self {
            construct: read_rows(a, i, read_construct),
            rgb: read_rows(a, i, read_quad),
            lab: read_rows(a, i, read_quad),
            lch: read_rows(a, i, read_quad),
            hsl: read_rows(a, i, read_quad),
            rgb_string: read_rows(a, i, read_str_row),
            alpha: read_rows(a, i, read_derived),
            darken: read_rows(a, i, read_derived),
            delta: read_rows(a, i, read_delta),
            sin: read_rows(a, i, read_sin),
        }
    }

    fn miss(what: &str) -> ! {
        panic!("color_allocator: colord table miss for {what} (capture bug)")
    }

    pub fn id_of(&self, key: &str) -> u32 {
        self.construct
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, id)| *id)
            .unwrap_or_else(|| Self::miss(&format!("construct key {key:?}")))
    }

    pub fn rgb(&self, id: u32) -> [f64; 4] {
        self.rgb
            .iter()
            .find(|(x, _)| *x == id)
            .map(|(_, q)| *q)
            .unwrap_or_else(|| Self::miss(&format!("toRgb id {id}")))
    }

    pub fn lab(&self, id: u32) -> [f64; 4] {
        self.lab
            .iter()
            .find(|(x, _)| *x == id)
            .map(|(_, q)| *q)
            .unwrap_or_else(|| Self::miss(&format!("toLab id {id}")))
    }

    pub fn lch(&self, id: u32) -> [f64; 4] {
        self.lch
            .iter()
            .find(|(x, _)| *x == id)
            .map(|(_, q)| *q)
            .unwrap_or_else(|| Self::miss(&format!("toLch id {id}")))
    }

    pub fn hsl(&self, id: u32) -> [f64; 4] {
        self.hsl
            .iter()
            .find(|(x, _)| *x == id)
            .map(|(_, q)| *q)
            .unwrap_or_else(|| Self::miss(&format!("toHsl id {id}")))
    }

    pub fn rgb_string(&self, id: u32) -> &str {
        self.rgb_string
            .iter()
            .find(|(x, _)| *x == id)
            .map(|(_, s)| s.as_str())
            .unwrap_or_else(|| Self::miss(&format!("toRgbString id {id}")))
    }

    pub fn alpha(&self, id: u32, amount: f64) -> u32 {
        self.alpha
            .iter()
            .find(|(x, amt, _)| *x == id && *amt == amount)
            .map(|(_, _, rid)| *rid)
            .unwrap_or_else(|| Self::miss(&format!("alpha ({id}, {amount})")))
    }

    pub fn darken(&self, id: u32, amount: f64) -> u32 {
        self.darken
            .iter()
            .find(|(x, amt, _)| *x == id && *amt == amount)
            .map(|(_, _, rid)| *rid)
            .unwrap_or_else(|| Self::miss(&format!("darken ({id}, {amount})")))
    }

    pub fn delta(&self, id1: u32, id2: u32) -> f64 {
        self.delta
            .iter()
            .find(|(x, y, _)| *x == id1 && *y == id2)
            .map(|(_, _, d)| *d)
            .unwrap_or_else(|| Self::miss(&format!("delta ({id1}, {id2})")))
    }

    pub fn sin(&self, x: f64) -> f64 {
        self.sin
            .iter()
            .find(|(q, _)| *q == x)
            .map(|(_, v)| *v)
            .unwrap_or_else(|| Self::miss(&format!("sin {x}")))
    }
}

/// `selectDistinctColorIndex` over the scripted delta table: the available
/// index whose nearest assigned neighbor is farthest (strict `>`, so the
/// FIRST maximum index wins ties). `Err` models the throw.
pub fn select_distinct_color_index(
    available: &[u32],
    assigned: &[u32],
    t: &ColordTables,
) -> Result<usize, &'static str> {
    if assigned.is_empty() {
        return Err("No assigned colors");
    }
    let mut max_delta_e = 0.0f64;
    let mut max_index = 0usize;
    for (i, color) in available.iter().enumerate() {
        let delta_e = assigned
            .iter()
            .fold(f64::INFINITY, |min, a| js_min(min, t.delta(*color, *a)));
        if delta_e > max_delta_e {
            max_delta_e = delta_e;
            max_index = i;
        }
    }
    Ok(max_index)
}

/// `ColorAllocator` over the facade ids: available / fallback pools of Colord
/// ids plus the insertion-ordered assigned map.
#[derive(Default)]
pub struct ColorAllocator {
    available: Vec<u32>,
    fallback: Vec<u32>,
    assigned: Vec<(String, u32)>,
}

impl ColorAllocator {
    /// `constructor(colors, fallback)`: available is a copy of the pool, the
    /// fallback list is `[...colors, ...fallback]`.
    pub fn new(colors: &[u32], fallback: &[u32]) -> Self {
        let mut fallback_list = colors.to_vec();
        fallback_list.extend_from_slice(fallback);
        Self {
            available: colors.to_vec(),
            fallback: fallback_list,
            assigned: Vec::new(),
        }
    }

    /// `assignColor(id)`: cache hit, refill from the fallback list when the
    /// pool drains, random pick while `assigned.len()` is 0 or > 50, else the
    /// distinct-color scan. Returns the assigned Colord id.
    pub fn assign_color(&mut self, id: &str, t: &ColordTables) -> u32 {
        if let Some((_, c)) = self.assigned.iter().find(|(k, _)| k == id) {
            return *c;
        }
        if self.available.is_empty() {
            self.available = self.fallback.clone();
        }
        let selected_index: usize = if self.assigned.is_empty() || self.assigned.len() > 50 {
            let mut rand = PseudoRandom::new(simple_hash(id));
            rand.next_int(0.0, self.available.len() as f64) as usize
        } else {
            let assigned_colors: Vec<u32> = self.assigned.iter().map(|(_, c)| *c).collect();
            select_distinct_color_index(&self.available, &assigned_colors, t)
                .unwrap_or_else(|_| panic!("color_allocator: distinct path with empty assigned"))
        };
        let color = self.available.remove(selected_index);
        self.assigned.push((id.to_string(), color));
        color
    }
}

/// `[n, (encS hex)*n]` raw palette list (the tables block follows it in the
/// arg stream, so the hexes are collected first and resolved afterwards).
pub fn hex_list(a: &[f64], i: &mut usize) -> Vec<String> {
    let n = a[*i] as usize;
    *i += 1;
    (0..n).map(|_| read_str(a, i)).collect()
}

/// Resolve a raw hex list through the construct table (the capture
/// pre-constructs every scripted palette entry, so every key exists).
pub fn hex_ids(hexes: &[String], t: &ColordTables) -> Vec<u32> {
    hexes.iter().map(|h| t.id_of(&key_hex(h))).collect()
}

// ---------------------------------------------------------------- vectors op
//
// kind 0: assignColor sequence — args `[n, (encS id)*n, (pool hex list),
//         (fallback hex list), (colord tables)]` -> `[n, (0, r, g, b, a)*n]`
//         (the assigned color's toRgb each step).
// kind 1: selectDistinctColorIndex — args `[(avail hex list), (assigned hex
//         list), (tables)]` -> `[0, idx] | [1, encS "No assigned colors"]`.

/// Flat-token runner shared by the golden replay and the wasm probe.
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut i = 0usize;
    match kind {
        0 => {
            let n = args[i] as usize;
            i += 1;
            let ids: Vec<String> = (0..n).map(|_| read_str(args, &mut i)).collect();
            let pool_hex = hex_list(args, &mut i);
            let fallback_hex = hex_list(args, &mut i);
            let t = ColordTables::read(args, &mut i);
            let pool = hex_ids(&pool_hex, &t);
            let fallback = hex_ids(&fallback_hex, &t);
            let mut alloc = ColorAllocator::new(&pool, &fallback);
            let mut out = vec![n as f64];
            for id in &ids {
                let c = alloc.assign_color(id, &t);
                let [r, g, b, a] = t.rgb(c);
                out.extend_from_slice(&[0.0, r, g, b, a]);
            }
            out
        }
        1 => {
            let avail_hex = hex_list(args, &mut i);
            let assigned_hex = hex_list(args, &mut i);
            let t = ColordTables::read(args, &mut i);
            let available = hex_ids(&avail_hex, &t);
            let assigned = hex_ids(&assigned_hex, &t);
            let mut out = Vec::new();
            match select_distinct_color_index(&available, &assigned, &t) {
                Ok(idx) => {
                    out.push(0.0);
                    out.push(idx as f64);
                }
                Err(msg) => {
                    out.push(1.0);
                    push_str(&mut out, msg);
                }
            }
            out
        }
        k => unreachable!("color_allocator: unknown op kind {k}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hand-built tables block: hexes -> ids 1.., every id has an rgb row
    /// `(id, id, id*2, 1)` (a synthetic fingerprint), delta rows are a fixed
    /// scripted matrix.
    fn fixture(hexes: &[&str], deltas: &[(u32, u32, f64)]) -> ColordTables {
        let mut t = ColordTables::default();
        for (k, h) in hexes.iter().enumerate() {
            let id = (k + 1) as u32;
            t.construct.push((key_hex(h), id));
            t.rgb.push((id, [f64::from(id), f64::from(id), f64::from(id) * 2.0, 1.0]));
        }
        t.delta.extend_from_slice(deltas);
        t
    }

    #[test]
    fn key_hex_matches_shim_form() {
        assert_eq!(key_hex("#ab"), "S3:35,97,98");
        assert_eq!(key_hex(""), "S0:");
    }

    #[test]
    fn distinct_first_max_and_throw() {
        let t = fixture(&["#a", "#b", "#c"], &[
            (1, 9, 0.5),
            (2, 9, 0.9),
            (3, 9, 0.9),
        ]);
        // available [#a #b #c], assigned [#x->id9]: max is index 1 (first on
        // the 0.9 tie with index 2).
        assert_eq!(
            select_distinct_color_index(&[1, 2, 3], &[9], &t),
            Ok(1)
        );
        assert_eq!(
            select_distinct_color_index(&[1], &[], &t),
            Err("No assigned colors")
        );
    }

    #[test]
    fn assign_cache_refill_and_distinct_gate() {
        let t = fixture(
            &["#a", "#b", "#f"],
            &[
                // self / cross deltas: id3 is the farthest from any assigned
                // pair, so the distinct scan always lands on it.
                (1, 1, 0.0),
                (2, 2, 0.0),
                (1, 2, 0.1),
                (2, 1, 0.1),
                (3, 1, 0.5),
                (3, 2, 0.5),
            ],
        );
        let mut alloc = ColorAllocator::new(&[1, 2], &[3]);
        // First assignment: random path (assigned empty).
        let c1 = alloc.assign_color("x", &t);
        assert!(c1 == 1 || c1 == 2);
        // Cache hit returns the same id without touching the pool.
        assert_eq!(alloc.assign_color("x", &t), c1);
        // Second assignment: distinct path (0 < size <= 50).
        let c2 = alloc.assign_color("y", &t);
        assert_ne!(c2, c1);
        // Pool drained; the third assignment refills from the fallback list
        // — the FULL [...colors, ...fallback] copy (splices never touched it).
        let c3 = alloc.assign_color("z", &t);
        assert_eq!(c3, 3);
    }

    fn tables_block(t: &ColordTables) -> Vec<f64> {
        let mut args = Vec::new();
        let block = |o: &mut Vec<f64>, n: usize| o.push(n as f64);
        block(&mut args, t.construct.len());
        for (k, id) in &t.construct {
            push_str(&mut args, k);
            args.push(f64::from(*id));
        }
        block(&mut args, t.rgb.len());
        for (id, q) in &t.rgb {
            args.push(f64::from(*id));
            args.extend_from_slice(q);
        }
        block(&mut args, t.lab.len());
        block(&mut args, t.lch.len());
        block(&mut args, t.hsl.len());
        block(&mut args, t.rgb_string.len());
        block(&mut args, t.alpha.len());
        block(&mut args, t.darken.len());
        block(&mut args, t.delta.len());
        for (x, y, d) in &t.delta {
            args.extend_from_slice(&[f64::from(*x), f64::from(*y), *d]);
        }
        block(&mut args, t.sin.len());
        args
    }

    #[test]
    fn run_op_kind0_sequence() {
        let t = fixture(&["#a", "#b", "#f"], &[(1, 2, 0.1), (2, 1, 0.1)]);
        let mut args: Vec<f64> = vec![2.0];
        for id in ["p1", "p2"] {
            push_str(&mut args, id);
        }
        args.push(2.0);
        for h in ["#a", "#b"] {
            push_str(&mut args, h);
        }
        args.push(1.0);
        push_str(&mut args, "#f");
        args.extend(tables_block(&t));
        let got = run_op(0, &args);
        assert_eq!(got.len(), 1 + 2 * 5);
        assert_eq!(got[0], 2.0);
        // Both assignments come from the pool; the rgb fingerprint decodes
        // back to the chosen ids.
        for step in got[1..].chunks_exact(5) {
            assert_eq!(step[0], 0.0);
            assert_eq!(step[3], step[1] * 2.0);
        }
        assert_ne!(got[2], got[7]);
    }

    #[test]
    fn run_op_kind1_throw_path() {
        let t = fixture(&["#a"], &[]);
        let mut args: Vec<f64> = vec![1.0];
        push_str(&mut args, "#a");
        args.push(0.0);
        args.extend(tables_block(&t));
        let got = run_op(1, &args);
        assert_eq!(got[0], 1.0);
        assert_eq!(read_str(&got, &mut 1usize), "No assigned colors");
    }
}
