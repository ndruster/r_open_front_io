//! Port of `src/client/render/frame/derive/TerrainRowSpans.ts` — the
//! texSubImage2D batching: changed tile refs -> row spans -> merged rects.
//!
//! Faithfulness notes (quirk list):
//!
//! * `rows` is a JS `Map<number, {min,max}>`: first-seen row order (not
//!   observable here — `ys` re-sorts ascending), and the min/max are mutated
//!   in place (`row.min = x`), not re-set.
//! * `x = ref % mapW` and `y = (ref - x) / mapW` are f64 JS math — a
//!   negative ref yields a negative x (JS `%` sign follows the dividend) and
//!   the y division keeps the fraction. The capture feeds canonical nonneg
//!   integer refs; the formula is transcribed verbatim.
//! * Merge gate: `previous && previous.y + previous.h === y` (adjacent rows)
//!   AND (`mergedArea <= sourceArea * 1.5` OR `extraTexels <= 4096`). On
//!   merge: `previous.x = minX`, `previous.w = maxX - minX + 1`, `h++`,
//!   `sourceArea += rowWidth` — the pending rect is mutated in place.
//! * `ys` sort is `(a, b) => a - b` — numeric ascending (stable for equal
//!   keys, but Map keys are unique).
//! * `bytes` fills in RECT order, then dy (top→bottom), then dx (left→
//!   right): `terrainByteAt((y + dy) * mapW + x + dx)`. The capture's
//!   `terrainByteAt` is the deterministic `(ref * 7 + 3) & 0xff` — modelled
//!   here as the same closed form (the TS callback is host-bound; the port
//!   exposes `terrain_byte_at` so the golden/wasm sides compute identically).
//! * `Uint8Array` write semantics: `bytes[offset] = v` stores `ToUint8(v)` —
//!   the closed form already lands in 0..=255.

/// `MAX_MERGE_OVERDRAW_RATIO`.
pub const MAX_MERGE_OVERDRAW_RATIO: f64 = 1.5;
/// `MAX_MERGE_EXTRA_TEXELS`.
pub const MAX_MERGE_EXTRA_TEXELS: f64 = 4096.0;

/// The capture's deterministic `terrainByteAt` — `(ref * 7 + 3) & 0xff`
/// through JS ToInt32/ToUint8 semantics (refs are small integers here, so
/// plain i64 math is exact).
pub fn terrain_byte_at(ref_: f64) -> f64 {
    let v = (ref_ * 7.0 + 3.0) as i64 & 0xff;
    v as f64
}

/// One output `TerrainRect`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Internal pending rect carrying the `sourceArea` bookkeeping.
struct PendingRect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    source_area: f64,
}

/// `buildTerrainRowSpans(refs, mapW, terrainByteAt)`.
pub fn build_terrain_row_spans(refs: &[f64], map_w: f64) -> (Vec<TerrainRect>, Vec<f64>) {
    // rows: Map<y, {min,max}> in insertion order.
    let mut rows: Vec<(f64, f64, f64)> = Vec::new();
    for r in refs {
        let x = r % map_w;
        let y = (r - x) / map_w;
        match rows.iter_mut().find(|(k, _, _)| *k == y) {
            Some((_, min, max)) => {
                if x < *min {
                    *min = x;
                }
                if x > *max {
                    *max = x;
                }
            }
            None => rows.push((y, x, x)),
        }
    }

    let mut ys: Vec<f64> = rows.iter().map(|(y, _, _)| *y).collect();
    ys.sort_by(|a, b| (a - b).partial_cmp(&0.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut pending: Vec<PendingRect> = Vec::new();
    for y in &ys {
        let row = rows.iter().find(|(k, _, _)| k == y).unwrap();
        let row_width = row.2 - row.1 + 1.0;
        let mut merged = false;
        if let Some(prev) = pending.last_mut() {
            if prev.y + prev.h == *y {
                let min_x = if prev.x < row.1 { prev.x } else { row.1 };
                let max_x = if prev.x + prev.w - 1.0 > row.2 {
                    prev.x + prev.w - 1.0
                } else {
                    row.2
                };
                let merged_area = (max_x - min_x + 1.0) * (prev.h + 1.0);
                let source_area = prev.source_area + row_width;
                let extra_texels = merged_area - source_area;
                if merged_area <= source_area * MAX_MERGE_OVERDRAW_RATIO
                    || extra_texels <= MAX_MERGE_EXTRA_TEXELS
                {
                    prev.x = min_x;
                    prev.w = max_x - min_x + 1.0;
                    prev.h += 1.0;
                    prev.source_area = source_area;
                    merged = true;
                }
            }
        }
        if !merged {
            pending.push(PendingRect {
                x: row.1,
                y: *y,
                w: row_width,
                h: 1.0,
                source_area: row_width,
            });
        }
    }

    let mut total = 0.0;
    for rect in &pending {
        total += rect.w * rect.h;
    }

    let mut bytes: Vec<f64> = Vec::with_capacity(total as usize);
    let mut rects: Vec<TerrainRect> = Vec::with_capacity(pending.len());
    for rect in &pending {
        for dy in 0..rect.h as i64 {
            let row_start = (rect.y + dy as f64) * map_w;
            for dx in 0..rect.w as i64 {
                bytes.push(terrain_byte_at(row_start + rect.x + dx as f64));
            }
        }
        rects.push(TerrainRect { x: rect.x, y: rect.y, w: rect.w, h: rect.h });
    }
    (rects, bytes)
}

/// `run_op(kind, args)` — capture harness entry (stateless). Kind table:
/// 0 -> buildTerrainRowSpans `[mapW, n, (ref)*n]`; res `[k, (x,y,w,h)*k,
///   m, (byte)*m]`;
/// 1 -> constants `[1.5, 4096]`;
/// 2 -> terrainByteAt probes `[n, (ref)*n]` -> `[n, (byte)*n]`.
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut i = 0usize;
    match kind {
        0 => {
            let map_w = args[i];
            i += 1;
            let n = args[i] as usize;
            i += 1;
            let refs = &args[i..i + n];
            let (rects, bytes) = build_terrain_row_spans(refs, map_w);
            let mut out = vec![rects.len() as f64];
            for r in &rects {
                out.push(r.x);
                out.push(r.y);
                out.push(r.w);
                out.push(r.h);
            }
            out.push(bytes.len() as f64);
            out.extend_from_slice(&bytes);
            out
        }
        1 => vec![MAX_MERGE_OVERDRAW_RATIO, MAX_MERGE_EXTRA_TEXELS],
        2 => {
            let n = args[i] as usize;
            i += 1;
            let mut out = vec![n as f64];
            for r in &args[i..i + n] {
                out.push(terrain_byte_at(*r));
            }
            out
        }
        k => unreachable!("terrain_row_spans: unknown op kind {k}"),
    }
}
