//! Port of the pure-geometry subset of `src/client/hud/NameBoxCalculator.ts`
//! (L96-203): `createGrid`, `findLargestInscribedRectangle`,
//! `largestRectangleInHistogram` and `calculateFontSize`. The placement
//! wrappers (`placeSpawnName` / `placeName`, which need the live `Game` /
//! `Player` objects and `calculateBoundingBox`) are out of scope.
//!
//! Faithfulness notes (quirk list):
//!
//! * `createGrid` floors the scaled box corners with `Math.floor` (toward
//!   -Infinity, NOT truncation — a negative `min.x / sf` rounds down), and
//!   the grid is COLUMN-major: `Array(width).fill(null).map(() =>
//!   Array(height).fill(false))`, indexed `grid[x - minX][y - minY]`.
//! * The per-cell fill gate is the TS `||` chain `isShore(tile) || (isOcean
//!   (tile) && magnitude(tile) < 10) || owner(tile) === player ||
//!   hasFallout(tile)`. The game facade is replaced by a closed-form terrain
//!   formula (identical in the capture and here) PLUS per-predicate call
//!   counters, so the short-circuit ORDER is observable through the dump even
//!   though the boolean result is order-independent.
//! * `findLargestInscribedRectangle` transposes on read: `rows = grid[0]
//!   .length` (the column's HEIGHT), `cols = grid.length` (the WIDTH), and
//!   walks `grid[col][row]`. The area update gate is STRICT `>` — a tie
//!   keeps the earlier rectangle; `y = row - rectForRow.height + 1`.
//! * `largestRectangleInHistogram` uses the sentinel column `i == length`
//!   with `h = 0`, a monotone stack with STRICT `<` (equal heights do NOT
//!   pop), and the `stack.length === 0` branches for `width` (`i`) and `x`
//!   (`0`).
//! * `calculateFontSize` divides by `name.length` — the UTF-16 CODE-UNIT
//!   count (an astral character counts 2) — and `Math.min` goes through
//!   [`js_min`] (NaN propagates like V8).

use crate::jsnum::js_min;

// `SPAWN_REGION_DIAMETER` is not ported (only the placement wrappers use
// it); the four pure functions need no constants.

/// The closed-form terrain facade shared with the capture. For cell
/// coordinates `(cx, cy)` inside the scripted map (`0 <= cx < map_w`,
/// `0 <= cy < map_h`), `ref = cx * 1000 + cy` and `cat = (ref * 31 + 7) %
/// 11` pick the category: `0` shore, `1` ocean magnitude 5, `2` ocean
/// magnitude 15, `3` owned by the player, `4` fallout, else plain land.
/// `cat >= 5` covers the remaining seven buckets as plain land.
fn terrain_cat(cx: f64, cy: f64) -> f64 {
    let r = cx * 1000.0 + cy;
    (r * 31.0 + 7.0) % 11.0
}

/// One createGrid dump: the column-major grid plus the facade call counters
/// that pin the `||` short-circuit order.
pub struct GridDump {
    pub width: usize,
    pub height: usize,
    /// `grid[col][row]` flattened column-major.
    pub cells: Vec<bool>,
    pub on_map_calls: f64,
    pub shore_calls: f64,
    pub ocean_calls: f64,
    pub magnitude_calls: f64,
    pub owner_calls: f64,
    pub fallout_calls: f64,
}

/// `createGrid(game, player, boundingBox, scalingFactor)` over the scripted
/// terrain facade.
pub fn create_grid(
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
    scaling_factor: f64,
    map_w: f64,
    map_h: f64,
) -> GridDump {
    let smin_x = (min_x / scaling_factor).floor();
    let smin_y = (min_y / scaling_factor).floor();
    let smax_x = (max_x / scaling_factor).floor();
    let smax_y = (max_y / scaling_factor).floor();

    let width = (smax_x - smin_x + 1.0) as usize;
    let height = (smax_y - smin_y + 1.0) as usize;
    // Array(width).fill(null).map(() => Array(height).fill(false)) —
    // column-major, all false.
    let mut grid = vec![false; width * height];

    let mut d = GridDump {
        width,
        height,
        cells: Vec::new(),
        on_map_calls: 0.0,
        shore_calls: 0.0,
        ocean_calls: 0.0,
        magnitude_calls: 0.0,
        owner_calls: 0.0,
        fallout_calls: 0.0,
    };

    let mut x = smin_x;
    while x <= smax_x {
        let mut y = smin_y;
        while y <= smax_y {
            let cx = x * scaling_factor;
            let cy = y * scaling_factor;
            d.on_map_calls += 1.0;
            if cx >= 0.0 && cy >= 0.0 && cx < map_w && cy < map_h {
                // game.ref(cell.x, cell.y) — the tile ref feeds the formula.
                let cat = terrain_cat(cx, cy);
                // The TS `||` chain, evaluated left to right with the same
                // short-circuit call counts.
                let is_shore = cat == 0.0;
                d.shore_calls += 1.0;
                let mut value = is_shore;
                if !value {
                    let is_ocean = cat == 1.0 || cat == 2.0;
                    d.ocean_calls += 1.0;
                    if is_ocean {
                        let magnitude = if cat == 1.0 { 5.0 } else { 15.0 };
                        d.magnitude_calls += 1.0;
                        value = magnitude < 10.0;
                    }
                    if !value {
                        let owned_by_player = cat == 3.0;
                        d.owner_calls += 1.0;
                        value = owned_by_player;
                        if !value {
                            let fallout = cat == 4.0;
                            d.fallout_calls += 1.0;
                            value = fallout;
                        }
                    }
                }
                grid[((x - smin_x) as usize) * height + (y - smin_y) as usize] = value;
            }
            y += 1.0;
        }
        x += 1.0;
    }

    d.cells = grid;
    d
}

/// The `{x, y, width, height}` rectangle.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// `findLargestInscribedRectangle(grid)` — `grid` is column-major
/// (`cols = grid.length` = width, `rows = grid[0].length` = height).
pub fn find_largest_inscribed_rectangle(
    width: usize,
    height: usize,
    cells: &[bool],
) -> Rect {
    let rows = height;
    let cols = width;
    let mut heights = vec![0.0f64; cols];
    let mut largest = Rect::default();

    for row in 0..rows {
        for col in 0..cols {
            if cells[col * height + row] {
                heights[col] += 1.0;
            } else {
                heights[col] = 0.0;
            }
        }

        let rect_for_row = largest_rectangle_in_histogram(&heights);

        if rect_for_row.width * rect_for_row.height > largest.width * largest.height {
            largest = Rect {
                x: rect_for_row.x,
                y: row as f64 - rect_for_row.height + 1.0,
                width: rect_for_row.width,
                height: rect_for_row.height,
            };
        }
    }

    largest
}

/// `largestRectangleInHistogram(widths)` — monotone stack, sentinel column
/// at `i == length` with `h = 0`, STRICT `<` pop gate (equal heights do not
/// pop), STRICT `>` area gate.
pub fn largest_rectangle_in_histogram(widths: &[f64]) -> Rect {
    let mut stack: Vec<usize> = Vec::new();
    let mut max_area = 0.0f64;
    let mut largest = Rect::default();

    for i in 0..=widths.len() {
        let h = if i == widths.len() { 0.0 } else { widths[i] };

        while !stack.is_empty() && h < widths[*stack.last().unwrap()] {
            let height = widths[stack.pop().unwrap()];
            let width = if stack.is_empty() {
                i as f64
            } else {
                i as f64 - *stack.last().unwrap() as f64 - 1.0
            };

            if height * width > max_area {
                max_area = height * width;
                largest = Rect {
                    x: if stack.is_empty() {
                        0.0
                    } else {
                        *stack.last().unwrap() as f64 + 1.0
                    },
                    y: 0.0,
                    width,
                    height,
                };
            }
        }

        stack.push(i);
    }

    largest
}

/// `calculateFontSize(rectangle, name)` — `name.length` is the UTF-16
/// code-unit count; `Math.min` goes through [`js_min`].
pub fn calculate_font_size(rect_width: f64, rect_height: f64, name_units: usize) -> f64 {
    let width_constrained = (rect_width / name_units as f64) * 2.0;
    let height_constrained = rect_height / 3.0;
    js_min(width_constrained, height_constrained)
}

/// `run_op(kind, args)` — capture harness entry (stateless). Kind table (see
/// `tools/gen_vectors.mjs`):
/// 0 createGrid `[minX, minY, maxX, maxY, sf, mapW, mapH]` ->
///   `[w, h, (bool)*w*h, onMap, shore, ocean, magnitude, owner, fallout]`;
/// 1 findLargestInscribedRectangle `[w, h, (bool)*w*h]` -> `[x, y, w, h]`;
/// 2 largestRectangleInHistogram `[n, (widths)*n]` -> `[x, y, w, h]`;
/// 3 calculateFontSize `[rectW, rectH, n, (units)*n]` -> `[f64]`.
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut i = 0usize;
    match kind {
        0 => {
            let (min_x, min_y, max_x, max_y) = (args[i], args[i + 1], args[i + 2], args[i + 3]);
            i += 4;
            let sf = args[i];
            i += 1;
            let (map_w, map_h) = (args[i], args[i + 1]);
            let d = create_grid(min_x, min_y, max_x, max_y, sf, map_w, map_h);
            let mut out = Vec::with_capacity(2 + d.cells.len() + 6);
            out.push(d.width as f64);
            out.push(d.height as f64);
            for c in &d.cells {
                out.push(if *c { 1.0 } else { 0.0 });
            }
            out.extend_from_slice(&[
                d.on_map_calls,
                d.shore_calls,
                d.ocean_calls,
                d.magnitude_calls,
                d.owner_calls,
                d.fallout_calls,
            ]);
            out
        }
        1 => {
            let width = args[i] as usize;
            i += 1;
            let height = args[i] as usize;
            i += 1;
            let cells: Vec<bool> = args[i..i + width * height].iter().map(|v| *v != 0.0).collect();
            let r = find_largest_inscribed_rectangle(width, height, &cells);
            vec![r.x, r.y, r.width, r.height]
        }
        2 => {
            let n = args[i] as usize;
            i += 1;
            let r = largest_rectangle_in_histogram(&args[i..i + n]);
            vec![r.x, r.y, r.width, r.height]
        }
        3 => {
            let rect_w = args[i];
            i += 1;
            let rect_h = args[i];
            i += 1;
            let n = args[i] as usize;
            // The UTF-16 units themselves are not needed — only the length —
            // but the cursor must still consume them for stream alignment.
            vec![calculate_font_size(rect_w, rect_h, n)]
        }
        k => unreachable!("name_box_calculator: unknown op kind {k}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram_ties_and_sentinel() {
        // Equal heights do not pop (STRICT `<`): [2,2] resolves at the
        // sentinel with the LAST bar's extent.
        let r = largest_rectangle_in_histogram(&[2.0, 2.0]);
        assert_eq!(r, Rect { x: 0.0, y: 0.0, width: 2.0, height: 2.0 });
        // Empty histogram -> zero rect.
        let e = largest_rectangle_in_histogram(&[]);
        assert_eq!(e, Rect::default());
    }

    #[test]
    fn font_size_uses_utf16_length() {
        // "😀a" is 3 UTF-16 units: (30/3)*2 = 20, 9/3 = 3, min = 3.
        assert_eq!(calculate_font_size(30.0, 9.0, 3), 3.0);
        assert!(calculate_font_size(30.0, f64::NAN, 3).is_nan());
    }
}
