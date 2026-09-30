//! Port of `src/core/game/TerrainSearchMap.ts` — a read-only search view
//! over packed terrain bytes (`bit 7 = land`, `bits 0-4 = magnitude`).
//!
//! The class is a pure function of its buffer, so the port keeps the raw
//! bytes and re-derives every answer at query time. The JS-isms pinned here:
//!
//! * **Header decode.** `width = (d[1] << 8) | d[0]`, `height = (d[3] << 8) |
//!   d[2]`. A short buffer makes the missing reads `undefined`, whose `<<` /
//!   `|` coercion is `0` — so a 2-byte buffer yields the *decoded* width and
//!   `height = 0`, not a throw.
//! * **Out-of-range / non-integer `node()` reads.** `mapData[idx]` returns
//!   `undefined` for any index that is not an in-range integer (negative,
//!   fractional, huge, or NaN). `undefined & 0x80` and `undefined & 0x1f` are
//!   both `0` (ToInt32 of NaN), so such reads classify as **Shore**, and an
//!   index that lands inside the 4-byte header reads the header byte like any
//!   other tile — there is no bounds check.
//! * **`neighbors()` passes fractional coordinates through.** The bounds test
//!   is a plain relational comparison, so `neighbors(2.5, 2.5)` on a 3×3 map
//!   pushes `(1.5, 1.5)` etc.; `NaN` / `±Infinity` coordinates fail every
//!   comparison and yield the empty list. Relational semantics match Rust
//!   (`NaN >= 0` is `false` on both sides).

/// `SearchMapTileType` — the three-way classification of a packed tile byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SearchMapTileType {
    Land = 0,
    Shore = 1,
    Water = 2,
}

/// One neighbour coordinate pair; TS pushes `{x, y}` objects, which may be
/// fractional because the bounds test does not require integers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Neighbor {
    pub x: f64,
    pub y: f64,
}

/// `TerrainSearchMap` — width/height from the buffer header, packed bytes
/// after it, and the `node` / `neighbors` / `getWidth` / `getHeight` surface.
pub struct TerrainSearchMap {
    width: f64,
    height: f64,
    map_data: Vec<u8>,
}

impl TerrainSearchMap {
    /// `constructor(buffer)` over the buffer's bytes (JS `new Uint8Array`).
    pub fn new(buffer: Vec<u8>) -> Self {
        // A missing byte reads as `undefined`, which coerces to 0 in the
        // `<<` / `|` header decode.
        let byte = |i: usize| buffer.get(i).copied().unwrap_or(0) as i64;
        let width = ((byte(1) << 8) | byte(0)) as f64;
        let height = ((byte(3) << 8) | byte(2)) as f64;
        Self {
            width,
            height,
            map_data: buffer,
        }
    }

    /// `node(x, y)` — classify the packed byte at `4 + y*width + x`.
    pub fn node(&self, x: f64, y: f64) -> SearchMapTileType {
        let idx = 4.0 + y * self.width + x;
        // Only an in-range integer index yields a byte; every other read is
        // `undefined`, whose masked bits are all 0 -> not land, magnitude 0.
        let packed = if idx.fract() == 0.0 && idx >= 0.0 && (idx as usize) < self.map_data.len() {
            self.map_data[idx as usize]
        } else {
            0
        };
        if packed & 0b1000_0000 != 0 {
            SearchMapTileType::Land
        } else if (packed & 0b0001_1111) < 10 {
            SearchMapTileType::Shore
        } else {
            SearchMapTileType::Water
        }
    }

    /// `neighbors(x, y)` — the 8 adjacent tiles in the TS `dirs` order,
    /// kept when they pass the (non-integer-tolerant) bounds test.
    pub fn neighbors(&self, x: f64, y: f64) -> Vec<Neighbor> {
        const DIRS: [(f64, f64); 8] = [
            (-1.0, -1.0),
            (0.0, -1.0),
            (1.0, -1.0),
            (-1.0, 0.0),
            (1.0, 0.0),
            (-1.0, 1.0),
            (0.0, 1.0),
            (1.0, 1.0),
        ];
        let mut result = Vec::new();
        for &(dx, dy) in &DIRS {
            let new_x = x + dx;
            let new_y = y + dy;
            if new_x >= 0.0 && new_x < self.width && new_y >= 0.0 && new_y < self.height {
                result.push(Neighbor { x: new_x, y: new_y });
            }
        }
        result
    }

    /// `getWidth()`
    pub fn get_width(&self) -> f64 {
        self.width
    }

    /// `getHeight()`
    pub fn get_height(&self) -> f64 {
        self.height
    }
}
